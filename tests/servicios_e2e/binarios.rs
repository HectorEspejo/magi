//! Binarios falsos copiados de tests/servicios_servidor.rs (misma semántica):
//! systemctl, journalctl, docker/podman, sudo y nginx controlados por ficheros
//! de estado en un directorio temporal. El host de prueba los ve en el PATH.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub struct BinariosFalsos {
    dir: PathBuf,
    _temporal: tempfile::TempDir,
}

impl BinariosFalsos {
    pub fn nuevos() -> Self {
        let temporal = tempfile::tempdir().expect("directorio de binarios falsos");
        let dir = temporal.path().to_path_buf();
        Self {
            dir,
            _temporal: temporal,
        }
    }

    pub fn ruta(&self) -> &Path {
        &self.dir
    }

    /// Escribe un binario falso ejecutable.
    pub fn escribir(&self, nombre: &str, guion: &str) {
        let ruta = self.dir.join(nombre);
        let guion = format!(
            "#!/bin/sh\nMAGI_FALSOS='{}'\n{}\n",
            self.dir.display(),
            guion
        );
        std::fs::write(&ruta, guion).expect("escribiendo el binario falso");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755))
                .expect("permisos del binario falso");
        }
    }

    /// Las llamadas que dejaron los binarios falsos, una por línea.
    pub fn llamadas(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("llamadas.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    pub fn borrar_llamadas(&self) {
        let _ = std::fs::remove_file(self.dir.join("llamadas.log"));
    }
}

// ------------------------------------------------------- binarios falsos

impl BinariosFalsos {
    /// Escribe el juego completo de binarios falsos que ve el script.
    pub fn sistema(&self) {
        let dir = self.dir.display().to_string();
        let guiones: [(&str, &str, &str); 6] = [
            ("systemctl", GUION_SYSTEMCTL, "systemctl"),
            ("journalctl", GUION_JOURNALCTL, "journalctl"),
            ("docker", GUION_DOCKER, "docker"),
            ("podman", GUION_DOCKER, "podman"),
            ("sudo", GUION_SUDO, "sudo"),
            ("nginx", GUION_NGINX, "nginx"),
        ];
        for (nombre, guion, valor) in guiones {
            let guion = guion.replace("@DIR@", &dir).replace("@MOTOR@", valor);
            self.escribir(nombre, &guion);
        }
        std::fs::create_dir_all(self.dir.join("unidades")).expect("unidades");
        std::fs::create_dir_all(self.dir.join("contenedores")).expect("contenedores");
    }

    /// Una unidad de systemd con sus propiedades (las que da systemctl show).
    pub fn unidad(&self, nombre: &str, props: &[(&str, &str)]) {
        let mut contenido = String::new();
        let mut vistas: Vec<String> = Vec::new();
        let base: [(String, String); 13] = [
            ("Id".to_string(), nombre.to_string()),
            ("LoadState".to_string(), "loaded".to_string()),
            ("ActiveState".to_string(), "active".to_string()),
            ("SubState".to_string(), "running".to_string()),
            ("UnitFileState".to_string(), "enabled".to_string()),
            ("Description".to_string(), format!("prueba {nombre}")),
            ("Type".to_string(), "simple".to_string()),
            ("Result".to_string(), "success".to_string()),
            ("CanReload".to_string(), "yes".to_string()),
            ("ExecMainPID".to_string(), "4242".to_string()),
            ("MemoryCurrent".to_string(), "1048576".to_string()),
            (
                "ActiveEnterTimestampMonotonic".to_string(),
                "1000000".to_string(),
            ),
            (
                "FragmentPath".to_string(),
                format!("/etc/systemd/system/{nombre}"),
            ),
        ];
        for (clave, valor) in &base {
            let valor = props
                .iter()
                .find(|(buscada, _)| buscada == clave)
                .map(|(_, valor)| valor.to_string())
                .unwrap_or_else(|| valor.clone());
            contenido.push_str(&format!("{clave}={valor}\n"));
            vistas.push(clave.clone());
        }
        for (clave, valor) in props {
            if !vistas.iter().any(|vista| vista == clave) {
                contenido.push_str(&format!("{clave}={valor}\n"));
            }
        }
        let ruta = self.dir.join("unidades").join(nombre);
        std::fs::write(ruta, contenido).expect("unidad falsa");
    }

    /// El texto de systemctl cat de una unidad.
    pub fn texto_unidad(&self, nombre: &str, texto: &str) {
        let ruta = self.dir.join("unidades").join(format!("{nombre}.cat"));
        std::fs::write(ruta, texto).expect("texto de la unidad");
    }

    /// Un contenedor tal como lo enseña ps -a --format json.
    pub fn contenedor(&self, id: &str, nombre: &str, estado: &str, etiquetas: &str) {
        let fichero = self.dir.join("contenedores").join(format!("{id}.json"));
        let json = format!(
            "{{\"ID\":\"{id}\",\"Names\":\"{nombre}\",\"Image\":\"nginx:1.27\",\"State\":\"{estado}\",\"Status\":\"Up 2 hours\",\"Ports\":\"0.0.0.0:8080->80/tcp\",\"Labels\":\"{etiquetas}\"}}"
        );
        std::fs::write(fichero, json).expect("contenedor falso");
    }

    /// Las estadísticas de un contenedor (stats --no-stream).
    pub fn consumo(&self, id: &str, linea: &str) {
        let fichero = self.dir.join("contenedores").join(format!("{id}.stats"));
        std::fs::write(fichero, linea).expect("consumo falso");
    }

    /// Deja un marcador que cambia lo que hace un binario falso.
    pub fn marcar(&self, nombre: &str, contenido: &str) {
        std::fs::write(self.dir.join(nombre), contenido).expect("marcador");
    }
}

/// systemctl falso: lee las unidades de `unidades/` y guarda su estado.
const GUION_SYSTEMCTL: &str = r#"#!/bin/sh
set -u
MAGI_FALSOS="@DIR@"
printf 'systemctl %s\n' "$*" >> "$MAGI_FALSOS/llamadas.log"
case ${1:-} in
  --version) printf 'systemd 252 (252.31)\n'; exit 0 ;;
esac
caso=${1:-}; [ $# -gt 0 ] && shift
cambiar() {
  f="$MAGI_FALSOS/unidades/$1"
  [ -f "$f" ] || return 0
  tmp="$f.$$"
  while IFS= read -r linea; do
    case $linea in
      "$2="*) printf '%s=%s\n' "$2" "$3" ;;
      *) printf '%s\n' "$linea" ;;
    esac
  done < "$f" > "$tmp"
  mv "$tmp" "$f"
}
unidad_de() {
  resultado=
  for argumento in "$@"; do
    case $argumento in -*) continue ;; esac
    resultado=$argumento
  done
  printf '%s' "$resultado"
}
case $caso in
  show)
    for argumento in "$@"; do
      case $argumento in -*) continue ;; esac
      f="$MAGI_FALSOS/unidades/$argumento"
      [ -f "$f" ] || continue
      cat "$f"
      printf '\n'
    done
    ;;
  list-units)
    [ -f "$MAGI_FALSOS/lento_listar" ] && sleep 15
    tipo=service
    for argumento in "$@"; do
      case $argumento in
        --type=timer) tipo=timer ;;
        --type=service) tipo=service ;;
      esac
    done
    sufijo=.service; [ "$tipo" = timer ] && sufijo=.timer
    for f in "$MAGI_FALSOS"/unidades/*"$sufijo"; do
      [ -e "$f" ] || continue
      nombre=$(basename "$f")
      activa=$(sed -n 's/^ActiveState=//p' "$f" | head -n 1)
      sub=$(sed -n 's/^SubState=//p' "$f" | head -n 1)
      printf '%s loaded %s %s descripcion\n' "$nombre" "${activa:-inactive}" "${sub:-dead}"
    done
    ;;
  list-unit-files)
    tipo=service
    for argumento in "$@"; do
      case $argumento in
        --type=timer) tipo=timer ;;
        --type=service) tipo=service ;;
      esac
    done
    sufijo=.service; [ "$tipo" = timer ] && sufijo=.timer
    for f in "$MAGI_FALSOS"/unidades/*"$sufijo"; do
      [ -e "$f" ] || continue
      nombre=$(basename "$f")
      estado=$(sed -n 's/^UnitFileState=//p' "$f" | head -n 1)
      printf '%s %s\n' "$nombre" "${estado:-disabled}"
    done
    ;;
  cat)
    for argumento in "$@"; do
      case $argumento in -*) continue ;; esac
      f="$MAGI_FALSOS/unidades/$argumento.cat"
      if [ -f "$f" ]; then cat "$f"; else printf '[Unit]\nDescription=%s\n' "$argumento"; fi
      exit 0
    done
    ;;
  list-dependencies)
    reverso=no
    for argumento in "$@"; do [ "$argumento" = --reverse ] && reverso=si; done
    if [ "$reverso" = si ]; then
      printf 'app.service\n└─otro.service\n'
    else
      printf 'app.service\n├─network.target\n└─basic.target\n'
    fi
    ;;
  is-active)
    u=$(unidad_de "$@")
    f="$MAGI_FALSOS/unidades/$u"
    activa=$(sed -n 's/^ActiveState=//p' "$f" 2>/dev/null | head -n 1)
    printf '%s\n' "${activa:-inactive}"
    [ "$activa" = active ]
    ;;
  is-enabled)
    u=$(unidad_de "$@")
    f="$MAGI_FALSOS/unidades/$u"
    estado=$(sed -n 's/^UnitFileState=//p' "$f" 2>/dev/null | head -n 1)
    printf '%s\n' "${estado:-disabled}"
    ;;
  start|stop|restart|reload|enable|disable)
    u=$(unidad_de "$@")
    [ -n "$u" ] || exit 0
    if [ -f "$MAGI_FALSOS/falla.$u" ]; then
      printf 'Job for %s failed because the control process exited with error code.\n' "$u" >&2
      exit 1
    fi
    if [ -f "$MAGI_FALSOS/lento.$u" ]; then sleep 2; fi
    case $caso in
      start|restart|reload)
        cambiar "$u" ActiveState active
        cambiar "$u" SubState running
        cambiar "$u" Result success
        ;;
      stop)
        cambiar "$u" ActiveState inactive
        cambiar "$u" SubState dead
        ;;
      enable) cambiar "$u" UnitFileState enabled ;;
      disable) cambiar "$u" UnitFileState disabled ;;
    esac
    if [ -f "$MAGI_FALSOS/muere.$u" ]; then
      cambiar "$u" ActiveState failed
      cambiar "$u" SubState failed
      cambiar "$u" Result exit-code
    fi
    ;;
esac
exit 0
"#;

/// journalctl falso: líneas JSON o de texto y seguimiento hasta que lo matan.
const GUION_JOURNALCTL: &str = r#"#!/bin/sh
set -u
MAGI_FALSOS="@DIR@"
printf 'journalctl %s\n' "$*" >> "$MAGI_FALSOS/llamadas.log"
unidad=; n=10; json=no; seguir=no
while [ $# -gt 0 ]; do
  case $1 in
    -u|--unit) unidad=${2:-}; shift 2 ;;
    -n) n=${2:-0}; shift 2 ;;
    -o|--output) [ ${2:-} = json ] && json=si; shift 2 ;;
    -f|--follow) seguir=si; shift ;;
    *) shift ;;
  esac
done
case $n in ''|*[!0-9]*) n=0 ;; esac
i=0
relleno=
[ -f "$MAGI_FALSOS/relleno" ] && relleno=$(cat "$MAGI_FALSOS/relleno")
linea() {
  i=$((i+1))
  if [ "$json" = si ]; then
    printf '{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","MESSAGE":"%s %s linea %s %s"}\n' "$unidad" "${MAGI_USUARIO:-usuario}" "$i" "$relleno"
  else
    printf '2026-11-08T12:00:00+00:00 %s linea %s\n' "$unidad" "$i"
  fi
}
while [ "$i" -lt "$n" ]; do linea; done
if [ "$seguir" = si ]; then
  trap 'printf TERM >> "$MAGI_FALSOS/journalctl.paradas"; exit 0' TERM INT
  while :; do
    [ -f "$MAGI_FALSOS/parar" ] && exit 0
    k=0
    while [ "$k" -lt 5 ]; do linea; k=$((k+1)); done
    sleep 0.01
  done
fi
exit 0
"#;

/// docker y podman falsos: contenedores, consumo, logs, acciones y compose.
const GUION_DOCKER: &str = r#"#!/bin/sh
set -u
MAGI_FALSOS="@DIR@"
MOTOR="@MOTOR@"
printf '%s %s\n' "$MOTOR" "$*" >> "$MAGI_FALSOS/llamadas.log"
case ${1:-} in
  --version) printf '%s version 27.0.3, build abc\n' "$MOTOR"; exit 0 ;;
esac
caso=${1:-}; [ $# -gt 0 ] && shift
ultimo() {
  resultado=
  for argumento in "$@"; do
    case $argumento in -*) continue ;; esac
    resultado=$argumento
  done
  printf '%s' "$resultado"
}
case $caso in
  ps)
    for f in "$MAGI_FALSOS"/contenedores/*.json; do
      [ -e "$f" ] || continue
      cat "$f"
      printf '\n'
    done
    ;;
  stats)
    for f in "$MAGI_FALSOS"/contenedores/*.stats; do
      [ -e "$f" ] || continue
      cat "$f"
      printf '\n'
    done
    ;;
  inspect)
    u=$(ultimo "$@")
    f="$MAGI_FALSOS/contenedores/$u.inspect"
    if [ -f "$f" ]; then
      cat "$f"
    else
      printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running"}}]\n' "$u" "$u"
    fi
    ;;
  logs)
    seguir=no
    for argumento in "$@"; do
      case $argumento in -f|--follow) seguir=si ;; esac
    done
    i=0
    while [ "$i" -lt 30 ]; do
      i=$((i+1))
      printf '2026-11-08T12:00:00Z linea out %s\n' "$i"
      printf '2026-11-08T12:00:00Z linea err %s\n' "$i" >&2
    done
    if [ "$seguir" = si ]; then
      trap 'printf TERM >> "$MAGI_FALSOS/docker.paradas"; exit 0' TERM INT
      while :; do
        i=$((i+1))
        printf '2026-11-08T12:00:00Z linea out %s\n' "$i"
        printf '2026-11-08T12:00:00Z linea err %s\n' "$i" >&2
        sleep 0.05
      done
    fi
    ;;
  start|stop|restart)
    printf 'contenedor %s\n' "$(ultimo "$@")"
    ;;
  compose)
    sub=
    while [ $# -gt 0 ]; do
      case $1 in
        -p|--project-name|-f|--file) shift 2 ;;
        -*) shift ;;
        *) sub=$1; break ;;
      esac
    done
    case $sub in
      config)
        if [ -f "$MAGI_FALSOS/compose_roto" ]; then
          printf 'compose: configuracion no valida\n' >&2
          exit 1
        fi
        ;;
    esac
    ;;
  exec)
    printf 'shell del contenedor\n'
    ;;
esac
exit 0
"#;

/// sudo falso: comprueba la contraseña que llega por la entrada estándar.
const GUION_SUDO: &str = r#"#!/bin/sh
set -u
MAGI_FALSOS="@DIR@"
printf 'sudo %s\n' "$*" >> "$MAGI_FALSOS/llamadas.log"
modo_n=no; modo_s=no; modo_v=no
while [ $# -gt 0 ]; do
  case $1 in
    -n) modo_n=si; shift ;;
    -S) modo_s=si; shift ;;
    -v) modo_v=si; shift ;;
    -p) shift 2 ;;
    --) shift; break ;;
    -*) shift ;;
    *) break ;;
  esac
done
CLAVE=
if [ "$modo_s" = si ]; then
  IFS= read -r CLAVE || CLAVE=
  printf '%s\n' "$CLAVE" >> "$MAGI_FALSOS/sudo.entrada"
fi
if [ "$modo_n" = si ] && [ -f "$MAGI_FALSOS/sudo_pide_clave" ]; then
  printf 'sudo: a password is required\n' >&2
  exit 1
fi
if [ "$modo_v" = si ]; then
  if [ -f "$MAGI_FALSOS/clave" ]; then
    esperada=$(cat "$MAGI_FALSOS/clave")
    if [ "$CLAVE" != "$esperada" ]; then
      printf 'sudo: 1 incorrect password attempt\n' >&2
      exit 1
    fi
  fi
  exit 0
fi
if [ -f "$MAGI_FALSOS/sudo_sin_permiso" ]; then
  printf 'usuario no está en el archivo sudoers\n' >&2
  exit 1
fi
exec "$@"
"#;

/// nginx falso: la configuración se rompe con el marcador de la prueba.
const GUION_NGINX: &str = r#"#!/bin/sh
set -u
MAGI_FALSOS="@DIR@"
printf 'nginx %s\n' "$*" >> "$MAGI_FALSOS/llamadas.log"
if [ -f "$MAGI_FALSOS/nginx_roto" ]; then
  printf 'nginx: [emerg] invalid parameter roto in /etc/nginx/nginx.conf:3\n' >&2
  printf 'nginx: configuration file /etc/nginx/nginx.conf test failed\n' >&2
  exit 1
fi
printf 'nginx: configuration file /etc/nginx/nginx.conf test is successful\n'
"#;
