//! Pruebas del servidor de servicios (Fase 13): protocolo v8 (rango de las
//! peticiones, hosts que no son ssh, nombres no válidos), pestañas con orden,
//! acciones custodiadas con su deliberación, sudo, plazos y logs en vivo con
//! contrapresión y parada.
//!
//! Sin red externa: el servidor SSH de prueba (`comun`) lanza los `exec` en
//! local y, cuando la prueba lo pide, con un `PATH` en el que van
//! `systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos.

mod comun;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;

use magi::almacen::{deliberaciones, hosts};
use magi::config::{Config, Rutas};
use magi::deliberacion::{ComprobacionesHost, NuevaDeliberacion, ResultadoDeliberacion, Veredicto};
use magi::escritorio::TipoHost;
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{
    Accion, MensajeCliente, MensajeServidor, MotivoFinLog, Objetivo, OrdenPestana, ResultadoAccion,
    Secreto, VERSION_PROTOCOLO,
};
use magi::servidor;

use comun::*;

const PETICION: u64 = magi::protocolo::RANGO_SERVICIOS + 1;

/// Arranca el servidor de sesiones y saluda una ventana (sin host SSH: para
/// las pruebas que no llegan a hablar con el host).
async fn arrancar_sin_ssh(
    rutas: &Rutas,
    config: &Config,
) -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    tokio::net::unix::OwnedWriteHalf,
    tokio_util::codec::FramedRead<tokio::net::unix::OwnedReadHalf, magi::protocolo::LinesCodec>,
    u32,
) {
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), config.clone()));
    esperar_socket(&servidor::ruta_socket(rutas)).await;
    let mut stream = tokio::net::UnixStream::connect(servidor::ruta_socket(rutas))
        .await
        .expect("conectando al servidor");
    let saludo = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: 42,
    };
    enviar(&mut stream, &saludo).await;
    let (lectura, escritura) = stream.into_split();
    let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
    let MensajeServidor::Bienvenida {
        cliente_id,
        acciones_servicio,
        ..
    } = bienvenida
    else {
        panic!("se esperaba Bienvenida y llegó {bienvenida:?}");
    };
    assert!(
        acciones_servicio.is_empty(),
        "sin acciones en curso la lista va vacía"
    );
    (tarea, escritura, lector, cliente_id)
}

/// Lee mensajes hasta que uno cumpla el predicado (con plazo).
async fn esperar<F>(
    lector: &mut tokio_util::codec::FramedRead<
        tokio::net::unix::OwnedReadHalf,
        magi::protocolo::LinesCodec,
    >,
    mut vale: F,
) -> MensajeServidor
where
    F: FnMut(&MensajeServidor) -> bool,
{
    let fin = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let restante = fin.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(restante, siguiente_framed(lector)).await {
            Ok(mensaje) if vale(&mensaje) => return mensaje,
            Ok(_) => {}
            Err(_) => panic!("el servidor no mandó lo esperado en 20 s"),
        }
    }
}

/// Crea un host en la base de datos del entorno.
fn crear_host(rutas: &Rutas, nombre: &str, tipo: TipoHost) -> i64 {
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
    let id = hosts::crear(
        almacen.conexion(),
        &DatosHost {
            nombre: nombre.to_string(),
            direccion: "127.0.0.1".to_string(),
            puerto: 22,
            tipo,
            ..DatosHost::default()
        },
        Origen::Manual,
    )
    .expect("creando el host");
    almacen.cerrar().expect("cerrando magi.db");
    id
}

/// Directorio temporal con binarios falsos que el host de prueba ve en el
/// `PATH` (systemctl, journalctl, docker, podman, sudo, nginx…). Cada guion
/// deja sus llamadas en `llamadas.log` para que la prueba las compruebe.
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
    if [ -f "$MAGI_FALSOS/despacio" ]; then
      sleep 2
    else
      sleep 0.01
    fi
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

// ---------------------------------------------------------------- protocolo

/// Una petición de servicios fuera de su rango se rechaza con su
/// `peticion_id` (el rango empieza en 2^45).
#[tokio::test]
async fn una_peticion_fuera_del_rango_se_rechaza_con_el_mismo_peticion_id() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let (tarea, mut escritura, mut lector, _) = arrancar_sin_ssh(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::ListarServicios {
            peticion_id: 3,
            host_id: 1,
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::Error { .. })
    })
    .await;
    match respuesta {
        MensajeServidor::Error {
            mensaje,
            peticion_id,
        } => {
            assert_eq!(peticion_id, Some(3));
            assert!(mensaje.contains("rango"), "{mensaje}");
        }
        otro => panic!("{otro:?}"),
    }

    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

/// Un host que no es de tipo `ssh` no tiene servicios (§2, T100).
#[tokio::test]
async fn un_host_que_no_es_ssh_no_tiene_servicios() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, "escritorio", TipoHost::Vnc);
    let (tarea, mut escritura, mut lector, _) = arrancar_sin_ssh(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::ListarServicios {
            peticion_id: PETICION,
            host_id: host,
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } if *id == PETICION
        )
    })
    .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("no es un host ssh"), "{mensaje}");
        }
        otro => panic!("{otro:?}"),
    }

    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

/// Un nombre de objetivo con metacaracteres se rechaza en Rust antes de tocar
/// el host (AC del checklist: `nginx.service; rm -rf /`).
#[tokio::test]
async fn un_objetivo_con_metacaracteres_se_rechaza_en_rust() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, "prueba-1", TipoHost::Ssh);
    let (tarea, mut escritura, mut lector, _) = arrancar_sin_ssh(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::AccionServicio {
            peticion_id: PETICION,
            host_id: host,
            objetivo: magi::protocolo::Objetivo::unidad("nginx.service; rm -rf /"),
            accion: magi::protocolo::Accion::Reiniciar,
            deliberacion: None,
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } if *id == PETICION
        )
    })
    .await;
    assert!(matches!(respuesta, MensajeServidor::Error { .. }));

    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

/// Un proyecto Compose no tiene logs propios: se rechaza sin abrir canal.
#[tokio::test]
async fn un_proyecto_no_tiene_logs_propios() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, "prueba-1", TipoHost::Ssh);
    let (tarea, mut escritura, mut lector, _) = arrancar_sin_ssh(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::SeguirLog {
            peticion_id: PETICION,
            host_id: host,
            objetivo: magi::protocolo::Objetivo::proyecto("web"),
            lineas_iniciales: 200,
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } if *id == PETICION
        )
    })
    .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("proyecto"), "{mensaje}")
        }
        otro => panic!("{otro:?}"),
    }

    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

/// Lo que deja algo parado exige deliberación: el servidor lo recomprueba y
/// rechaza la acción antes de tocar el host (T62).
#[tokio::test]
async fn una_accion_que_deja_algo_parado_exige_deliberacion() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, "prueba-1", TipoHost::Ssh);
    let (tarea, mut escritura, mut lector, _) = arrancar_sin_ssh(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::AccionServicio {
            peticion_id: PETICION,
            host_id: host,
            objetivo: magi::protocolo::Objetivo::unidad("app.service"),
            accion: magi::protocolo::Accion::Reiniciar,
            deliberacion: None,
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } if *id == PETICION
        )
    })
    .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("deliberar"), "{mensaje}")
        }
        otro => panic!("{otro:?}"),
    }

    // Una deliberación que no existe tampoco vale.
    enviar_a(
        &mut escritura,
        &MensajeCliente::AccionServicio {
            peticion_id: PETICION + 1,
            host_id: host,
            objetivo: magi::protocolo::Objetivo::unidad("app.service"),
            accion: magi::protocolo::Accion::Parar,
            deliberacion: Some(magi::protocolo::DeliberacionLanzada {
                id: 424242,
                forzada: false,
                motivo: None,
            }),
            sudo: None,
        },
    )
    .await;
    let respuesta = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } if *id == PETICION + 1
        )
    })
    .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("no existe"), "{mensaje}")
        }
        otro => panic!("{otro:?}"),
    }

    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

/// Un cliente de la versión anterior recibe `VersionIncompatible` con lo que
/// custodia el servidor, incluidas las acciones de servicio (T67): así
/// pregunta antes de sustituirlo.
#[tokio::test]
async fn el_rechazo_a_un_cliente_anterior_cuenta_las_acciones_de_servicio() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;
    let mut lector = cliente(&servidor::ruta_socket(&rutas), VERSION_PROTOCOLO - 1).await;
    let rechazo: MensajeServidor = siguiente(&mut lector).await;
    match rechazo {
        MensajeServidor::VersionIncompatible {
            version, trabajo, ..
        } => {
            assert_eq!(version, VERSION_PROTOCOLO);
            assert_eq!(trabajo.expect("trabajo").acciones_servicio, 0);
        }
        otro => panic!("se esperaba VersionIncompatible, llegó {otro:?}"),
    }
    let _ = lector.get_mut().shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

// -------------------------------------------- escenario con binarios falsos

/// Escenario completo con el juego de binarios falsos que ve el host.
async fn escenario_servicios() -> (Escenario, BinariosFalsos) {
    let binarios = BinariosFalsos::nuevos();
    binarios.sistema();
    let escenario = escenario(OpcionesEscenario {
        binarios: Some(binarios.ruta().to_path_buf()),
        ..Default::default()
    })
    .await;
    (escenario, binarios)
}

/// Deja la pista de privilegio cacheada en el servidor con un listado.
async fn calentar_privilegio(escenario: &mut Escenario, host: i64) {
    let peticion = PETICION + 900;
    escenario
        .enviar(&MensajeCliente::ListarServicios {
            peticion_id: peticion,
            host_id: host,
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Servicios {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    assert!(
        matches!(respuesta, MensajeServidor::Servicios { .. }),
        "{respuesta:?}"
    );
}

/// Inserta una deliberación de servicio como la dejaría el cliente.
fn deliberacion_de_servicio(
    escenario: &Escenario,
    host_id: i64,
    accion: &str,
    objetivo: &str,
) -> i64 {
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).expect("magi.db");
    let id = deliberaciones::crear(
        almacen.conexion(),
        &NuevaDeliberacion {
            snippet_id: None,
            accion: format!("{accion} {objetivo} → prueba-1"),
            hosts: vec![(host_id, "prueba-1".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id,
                host: "prueba-1".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 10,
                },
                backup: Veredicto::NoActiva,
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Aprobada,
            bloqueada: false,
            motivo: None,
            usuario: "usuario".to_string(),
        },
    )
    .expect("deliberación de prueba");
    almacen.cerrar().expect("cerrando magi.db");
    id
}

async fn esperar_fichero(ruta: &Path, plazo: Duration) -> bool {
    let fin = tokio::time::Instant::now() + plazo;
    while tokio::time::Instant::now() < fin {
        if ruta.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

// ------------------------------------------------ listado, detalle y motor

/// El listado trae systemd, privilegio, diario, unidades, timers y motor.
#[tokio::test]
async fn el_listado_trae_unidades_timers_privilegio_y_motor() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    binarios.unidad(
        "fallido.service",
        &[("ActiveState", "failed"), ("SubState", "failed")],
    );
    binarios.unidad(
        "backup.timer",
        &[
            ("Unit", "backup.service"),
            ("NextElapseUSecRealtime", "1700000600000000"),
            ("LastTriggerUSecMonotonic", "1000000"),
        ],
    );
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::ListarServicios {
            peticion_id: peticion,
            host_id: host,
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Servicios {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::Servicios { listado, .. } => {
            match &listado.systemd {
                magi_nucleo::servicios::Systemd::Disponible(version) => {
                    assert_eq!(version, "252");
                }
                otro => panic!("se esperaba systemd 252, llegó {otro:?}"),
            }
            assert_eq!(
                listado.privilegio,
                magi::protocolo::Privilegio::SudoSinClave
            );
            assert_eq!(listado.diario, magi_nucleo::servicios::PermisoDiario::Todo);
            assert!(listado
                .unidades
                .iter()
                .any(|unidad| unidad.nombre == "app.service"));
            assert!(listado
                .unidades
                .iter()
                .any(|unidad| unidad.nombre == "fallido.service" && unidad.activo() == "failed"));
            assert!(listado
                .timers
                .iter()
                .any(|timer| timer.nombre == "backup.timer"));
            let motor = listado.motor.disponible().expect("motor docker");
            assert_eq!(motor.nombre, magi_nucleo::servicios::MotorNombre::Docker);
            assert_eq!(motor.version, "27.0.3");
        }
        otro => panic!("{otro:?}"),
    }
}

/// El detalle trae propiedades, dependencias y el texto de la unidad.
#[tokio::test]
async fn el_detalle_trae_propiedades_dependencias_y_texto() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    binarios.texto_unidad(
        "app.service",
        "[Unit]\nDescription=App\n[Service]\nExecStart=/usr/bin/app\n",
    );
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::DetalleServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Detalle {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::Detalle { detalle, .. } => {
            assert_eq!(
                detalle.props.get("Id").map(String::as_str),
                Some("app.service")
            );
            assert!(
                detalle.necesita.contains(&"network.target".to_string()),
                "necesita: {:?} · texto: {:?}",
                detalle.necesita,
                detalle.texto
            );
            assert!(
                detalle.necesitada.contains(&"otro.service".to_string()),
                "necesitada: {:?}",
                detalle.necesitada
            );
            assert!(
                detalle.texto.contains("Description=App"),
                "{}",
                detalle.texto
            );
        }
        otro => panic!("{otro:?}"),
    }
}

// ------------------------------------------------------------- acciones

/// Una acción con deliberación válida termina hecha, anota servicio_accion
/// tras la barrera y consume la deliberación (T47, T62).
#[tokio::test]
async fn una_accion_con_deliberacion_termina_hecha_y_anota_el_registro() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad(
        "app.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    calentar_privilegio(&mut escenario, host).await;
    let deliberacion = deliberacion_de_servicio(&escenario, host, "reiniciar", "app.service");
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            accion: Accion::Reiniciar,
            deliberacion: Some(magi::protocolo::DeliberacionLanzada {
                id: deliberacion,
                forzada: false,
                motivo: None,
            }),
            sudo: None,
        })
        .await;
    // Se difunde en curso antes del efecto (T40).
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionesServicio { lista } if lista.iter().any(|accion| accion.id == peticion)
            )
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::AccionServicioHecha { resultado, .. } => {
            assert_eq!(resultado.resultado, ResultadoAccion::Hecho, "{resultado:?}");
            assert_eq!(resultado.estado, "active");
        }
        otro => panic!("{otro:?}"),
    }
    // REGISTRO servicio_accion, con reintentos (lo escribe el hilo escritor).
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut filas = Vec::new();
    while tokio::time::Instant::now() < fin {
        filas = escenario.registro();
        if filas.iter().any(|(tipo, _)| tipo == "servicio_accion") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let detalle = filas
        .iter()
        .find(|(tipo, _)| tipo == "servicio_accion")
        .map(|(_, detalle)| detalle.clone())
        .expect("anotación servicio_accion");
    assert!(detalle.contains("reiniciar"), "{detalle}");
    assert!(detalle.contains("hecho"), "{detalle}");
    assert!(detalle.contains("deliberación #"), "{detalle}");
    // La deliberación queda consumida en DELIBERACIONES.
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).expect("magi.db");
    let registro = deliberaciones::obtener(almacen.conexion(), deliberacion).expect("fila");
    almacen.cerrar().expect("cerrando magi.db");
    assert_eq!(
        registro.ejecucion_resultado,
        Some(magi::deliberacion::EjecucionResultado::Ok)
    );
    // Y la orden real se ejecutó.
    assert!(binarios
        .llamadas()
        .iter()
        .any(|llamada| llamada == "systemctl restart app.service"));
}

/// Una segunda acción sobre el mismo objetivo se rechaza con «hay otra acción
/// en curso» mientras la primera sigue (T27).
#[tokio::test]
async fn una_segunda_accion_sobre_el_mismo_objetivo_se_rechaza() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad(
        "lento.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    binarios.marcar("lento.lento.service", "");
    calentar_privilegio(&mut escenario, host).await;
    let primera = PETICION;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: primera,
            host_id: host,
            objetivo: Objetivo::unidad("lento.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionesServicio { lista } if lista.iter().any(|accion| accion.id == primera)
            )
        })
        .await;
    let segunda = PETICION + 1;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: segunda,
            host_id: host,
            objetivo: Objetivo::unidad("lento.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error {
                    peticion_id: Some(id),
                    ..
                } if *id == segunda
            )
        })
        .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("otra acción en curso"), "{mensaje}")
        }
        otro => panic!("{otro:?}"),
    }
    // La primera termina bien.
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == primera
            )
        })
        .await;
    match respuesta {
        MensajeServidor::AccionServicioHecha { resultado, .. } => {
            assert_eq!(resultado.resultado, ResultadoAccion::Hecho, "{resultado:?}")
        }
        otro => panic!("{otro:?}"),
    }
}

/// Una comprobación de configuración fallida (nginx -t) rechaza la acción y no
/// ejecuta la orden (D243).
#[tokio::test]
async fn una_comprobacion_de_configuracion_fallida_no_ejecuta_la_accion() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("nginx.service", &[]);
    binarios.marcar("nginx_roto", "");
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("nginx.service"),
            accion: Accion::Recargar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::AccionServicioHecha { resultado, .. } => {
            assert_eq!(
                resultado.resultado,
                ResultadoAccion::RechazadaConfiguracion,
                "{resultado:?}"
            );
            assert!(
                resultado
                    .lineas
                    .iter()
                    .any(|linea| linea.contains("test failed")),
                "{resultado:?}"
            );
        }
        otro => panic!("{otro:?}"),
    }
    let llamadas = binarios.llamadas();
    assert!(
        llamadas
            .iter()
            .any(|llamada| llamada.starts_with("nginx -t")),
        "{llamadas:?}"
    );
    assert!(
        !llamadas
            .iter()
            .any(|llamada| llamada == "systemctl reload nginx.service"),
        "no se ejecuta la acción: {llamadas:?}"
    );
}

// ---------------------------------------------------------------- sudo

/// La contraseña de sudo se comprueba, viaja por la entrada estándar y no
/// aparece en ninguna orden (T25).
#[tokio::test]
async fn la_contrasena_de_sudo_se_comprueba_y_viaja_por_la_entrada_estandar() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.marcar("sudo_pide_clave", "");
    binarios.marcar("clave", "secreta");
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::ComprobarSudo {
            peticion_id: peticion,
            host_id: host,
            contrasena: Secreto::nuevo("secreta"),
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SudoComprobado {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::SudoComprobado { resultado, .. } => {
            assert_eq!(resultado, magi::protocolo::ComprobacionSudo::Ok);
        }
        otro => panic!("{otro:?}"),
    }
    // La contraseña llegó al host por la entrada estándar.
    let entrada = std::fs::read_to_string(binarios.ruta().join("sudo.entrada"))
        .expect("la contraseña pasó por la entrada estándar");
    assert!(entrada.contains("secreta"), "{entrada}");
    // Y no aparece en ninguna orden.
    let comandos = escenario
        .observado
        .comandos
        .lock()
        .expect("comandos")
        .clone();
    assert!(
        !comandos.iter().any(|comando| comando.contains("secreta")),
        "la contraseña no va en la línea de órdenes: {comandos:?}"
    );
    // Una contraseña incorrecta se rechaza.
    let peticion = PETICION + 1;
    escenario
        .enviar(&MensajeCliente::ComprobarSudo {
            peticion_id: peticion,
            host_id: host,
            contrasena: Secreto::nuevo("otra"),
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SudoComprobado {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::SudoComprobado { resultado, .. } => {
            assert_eq!(resultado, magi::protocolo::ComprobacionSudo::Rechazada);
        }
        otro => panic!("{otro:?}"),
    }
}

/// Una acción en curso cuenta en TrabajoServidor.acciones_servicio, que es lo
/// que ve un cliente de la versión anterior al preguntar (T67).
#[tokio::test]
async fn una_accion_en_curso_cuenta_como_trabajo_para_un_cliente_anterior() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad(
        "lento.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    binarios.marcar("lento.lento.service", "");
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("lento.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionesServicio { lista } if lista.iter().any(|accion| accion.id == peticion)
            )
        })
        .await;
    // Un cliente con la versión de protocolo anterior pregunta: el rechazo
    // cuenta la acción en curso.
    let mut lector = cliente(
        &magi::servidor::ruta_socket(escenario.rutas()),
        VERSION_PROTOCOLO - 1,
    )
    .await;
    let rechazo: MensajeServidor = siguiente(&mut lector).await;
    match rechazo {
        MensajeServidor::VersionIncompatible {
            trabajo: Some(trabajo),
            ..
        } => assert_eq!(trabajo.acciones_servicio, 1),
        otro => panic!("{otro:?}"),
    }
    // Y la acción termina bien igualmente.
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    assert!(matches!(
        respuesta,
        MensajeServidor::AccionServicioHecha { .. }
    ));
}

/// Una acción sigue (y se anota) si se cierra la ventana que la pidió: es
/// trabajo del servidor, no de la ventana (§3.3, T67).
#[tokio::test]
async fn una_accion_sigue_si_se_cierra_la_ventana_que_la_pidio() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad(
        "lento.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    binarios.marcar("lento.lento.service", "");
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION + 40;
    let mut ventana = escenario.otra_ventana().await;
    ventana
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("lento.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionesServicio { lista } if lista.iter().any(|accion| accion.id == peticion)
            )
        })
        .await;
    // Se cae la ventana que la pidió: la acción no.
    drop(ventana);
    let fin = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut anotada = false;
    while tokio::time::Instant::now() < fin {
        anotada = escenario.registro().iter().any(|(tipo, detalle)| {
            tipo == "servicio_accion" && detalle.contains("arrancar") && detalle.contains("hecho")
        });
        if anotada {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        anotada,
        "la acción no terminó ni se anotó tras cerrarse la ventana"
    );
}

// ---------------------------------------------------------------- logs

/// Los logs llegan en lotes y PararLog cierra el canal (fin de la entrada
/// estándar del script), que corta el seguimiento en el host.
#[tokio::test]
async fn los_logs_llegan_y_parar_log_cierra_el_canal() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 50,
            sudo: None,
        })
        .await;
    let mut vistos = Vec::new();
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    let lote = loop {
        if tokio::time::Instant::now() >= fin {
            panic!("no llegó ningún LineasLog; llegaron: {vistos:?}");
        }
        let mensaje = tokio::time::timeout_at(fin, escenario.siguiente());
        let Ok(mensaje) = mensaje.await else {
            panic!("no llegó ningún LineasLog; llegaron: {vistos:?}");
        };
        vistos.push(mensaje.clone());
        if let MensajeServidor::LineasLog {
            peticion_id: id,
            lineas,
            ..
        } = &mensaje
        {
            if *id == peticion && !lineas.is_empty() {
                break mensaje;
            }
        }
    };
    match lote {
        MensajeServidor::LineasLog { lineas, .. } => {
            assert!(
                lineas
                    .iter()
                    .any(|linea| linea.texto.contains("app.service")),
                "{lineas:?}"
            );
        }
        otro => panic!("{otro:?}"),
    }
    escenario
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::FinLog { motivo, .. } => {
            assert_eq!(motivo, MotivoFinLog::Cerrado);
        }
        otro => panic!("{otro:?}"),
    }
    // El script ve el fin de su entrada estándar y corta el journalctl del
    // host (el falso deja el marcador al recibir SIGTERM).
    let marcador = binarios.ruta().join("journalctl.paradas");
    assert!(
        esperar_fichero(&marcador, Duration::from_secs(10)).await,
        "el seguimiento del host no se cortó; llamadas: {:?}",
        binarios.llamadas()
    );
}

/// Si el cliente no da abasto, la cola de la suscripción descarta las líneas
/// más viejas y las cuenta (§7.6).
#[tokio::test]
async fn los_logs_descartan_las_viejas_con_recuento_si_el_cliente_no_da_abasto() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    binarios.marcar("relleno", &"x".repeat(4000));
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 200,
            sudo: None,
        })
        .await;
    // El cliente no lee: el socket se llena, el servidor deja de mandar lotes
    // y el host sigue llenando la cola de 4 MiB, que descarta las más viejas.
    tokio::time::sleep(Duration::from_secs(4)).await;
    let fin = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut omitidas = 0u32;
    while tokio::time::Instant::now() < fin {
        let mensaje = escenario.siguiente().await;
        if let MensajeServidor::LineasLog {
            peticion_id: id,
            omitidas: en_lote,
            ..
        } = mensaje
        {
            if id == peticion {
                omitidas = omitidas.saturating_add(en_lote);
                if omitidas > 0 {
                    break;
                }
            }
        }
    }
    assert!(omitidas > 0, "la contrapresión no descartó líneas");
    escenario
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    assert!(matches!(respuesta, MensajeServidor::FinLog { .. }));
}

// ------------------------------------------------- pestañas con orden

/// Una pestaña con orden la arma el servidor desde el tipo cerrado y la
/// escribe como comando inicial (D248); el cliente nunca manda texto.
#[tokio::test]
async fn una_pestana_con_orden_la_arma_el_servidor() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    calentar_privilegio(&mut escenario, host).await;
    let conocidas: HashSet<u32> = escenario.sesiones.iter().map(|sesion| sesion.id).collect();
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: host,
            cols: 100,
            filas: 30,
            comandos_iniciales: Vec::new(),
            orden: Some(OrdenPestana::SeguirDiario {
                unidad: "app.service".to_string(),
            }),
        })
        .await;
    let _sesion = escenario.esperar_sesion_nueva(&conocidas).await;
    let fin = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut escrito = String::new();
    while tokio::time::Instant::now() < fin {
        escrito = escenario.observado.shell();
        if escrito.contains("journalctl -fu") && escrito.contains("app.service") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        escrito.contains("journalctl -fu"),
        "la orden no se escribió en la pestaña: {escrito:?}"
    );
    assert!(
        escrito.contains("app.service") && escrito.contains("sudo -n"),
        "la orden armada por el servidor no es la esperada: {escrito:?}"
    );
}

// ------------------------------------------------------------- plazos

/// Un listado que no contesta en 10 s se corta con error (T32).
#[tokio::test]
async fn un_listado_que_pasa_del_plazo_contesta_error() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    binarios.marcar("lento_listar", "");
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::ListarServicios {
            peticion_id: peticion,
            host_id: host,
            sudo: None,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error {
                    peticion_id: Some(id),
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(
                mensaje.contains("no respondió") || mensaje.contains("10 s"),
                "{mensaje}"
            );
        }
        otro => panic!("{otro:?}"),
    }
}

// --------------------------------------- clientes distintos, mismo peticion_id

/// Dos ventanas pueden repetir el peticion_id: las acciones se custodian por
/// (cliente, peticion_id), así que cada una termina la suya, se anotan las dos
/// en REGISTRO y ninguna respuesta se cruza.
#[tokio::test]
async fn dos_ventanas_con_el_mismo_peticion_id_custodian_sus_acciones() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad(
        "app.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    binarios.unidad(
        "otra.service",
        &[("ActiveState", "inactive"), ("SubState", "dead")],
    );
    calentar_privilegio(&mut escenario, host).await;
    // Las dos ventanas usan el MISMO peticion_id sobre objetivos distintos.
    let peticion = PETICION;
    let mut ventana = escenario.otra_ventana().await;
    escenario
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    ventana
        .enviar(&MensajeCliente::AccionServicio {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("otra.service"),
            accion: Accion::Arrancar,
            deliberacion: None,
            sudo: None,
        })
        .await;
    // Cada ventana recibe su propio AccionServicioHecha.
    let suya = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    assert!(matches!(suya, MensajeServidor::AccionServicioHecha { .. }));
    let suya = ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::AccionServicioHecha {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match suya {
        MensajeServidor::AccionServicioHecha { resultado, .. } => {
            assert_eq!(resultado.resultado, ResultadoAccion::Hecho, "{resultado:?}")
        }
        otro => panic!("{otro:?}"),
    }
    // No llega una segunda respuesta con el mismo id a la ventana principal.
    let cola = escenario.escuchar(Duration::from_millis(400)).await;
    assert!(
        !cola.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::AccionServicioHecha { peticion_id: id, .. } if *id == peticion
        )),
        "la acción de la otra ventana se contestó también a la principal"
    );
    // REGISTRO tiene las dos filas, cada una con su objetivo.
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut filas = Vec::new();
    while tokio::time::Instant::now() < fin {
        filas = escenario
            .registro()
            .into_iter()
            .filter(|(tipo, _)| tipo == "servicio_accion")
            .map(|(_, detalle)| detalle)
            .collect();
        if filas.iter().any(|detalle| detalle.contains("app.service"))
            && filas.iter().any(|detalle| detalle.contains("otra.service"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        filas.iter().any(|detalle| detalle.contains("app.service")),
        "{filas:?}"
    );
    assert!(
        filas.iter().any(|detalle| detalle.contains("otra.service")),
        "{filas:?}"
    );
}

/// Dos ventanas con el mismo peticion_id en SeguirLog: cada una recibe sus
/// líneas y su fin, sin cruzarse ni pisarse.
#[tokio::test]
async fn dos_ventanas_con_el_mismo_peticion_id_tienen_sus_logs() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    binarios.unidad("otra.service", &[]);
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    let mut ventana = escenario.otra_ventana().await;
    escenario
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 10,
            sudo: None,
        })
        .await;
    ventana
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("otra.service"),
            lineas_iniciales: 10,
            sudo: None,
        })
        .await;
    // La ventana principal solo ve app.service y la otra solo otra.service.
    let suyo = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::LineasLog {
                    peticion_id: id,
                    lineas,
                    ..
                } if *id == peticion && lineas.iter().any(|linea| linea.texto.contains("app.service"))
            )
        })
        .await;
    assert!(matches!(suyo, MensajeServidor::LineasLog { .. }));
    let suyo = ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::LineasLog {
                    peticion_id: id,
                    lineas,
                    ..
                } if *id == peticion && lineas.iter().any(|linea| linea.texto.contains("otra.service"))
            )
        })
        .await;
    assert!(matches!(suyo, MensajeServidor::LineasLog { .. }));
    // Ninguno recibe líneas del objetivo del otro.
    let cola = escenario.escuchar(Duration::from_millis(300)).await;
    assert!(
        !cola.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::LineasLog { lineas, .. }
                if lineas.iter().any(|linea| linea.texto.contains("otra.service"))
        )),
        "la ventana principal recibió líneas de la otra suscripción"
    );
    // PararLog de cada una con el mismo id: cada una recibe su FinLog.
    escenario
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    ventana
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    let suyo = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match suyo {
        MensajeServidor::FinLog { motivo, .. } => assert_eq!(motivo, MotivoFinLog::Cerrado),
        otro => panic!("{otro:?}"),
    }
    let suyo = ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match suyo {
        MensajeServidor::FinLog { motivo, .. } => assert_eq!(motivo, MotivoFinLog::Cerrado),
        otro => panic!("{otro:?}"),
    }
}

// ------------------------------------------- orden por cliente (M1 revisión)

/// `SeguirLog` y `PararLog` inmediatos del mismo cliente se procesan en
/// orden: no queda ni suscripción huérfana ni journalctl corriendo en el host.
#[tokio::test]
async fn un_parar_log_inmediato_no_deja_el_seguimiento_huerfano() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    // Los dos mensajes salen seguidos, sin esperar al primero.
    escenario
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 5,
            sudo: None,
        })
        .await;
    escenario
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    // Serializados: el PararLog ve la suscripción ya insertada y contesta.
    let respuesta = tokio::time::timeout(
        Duration::from_secs(8),
        escenario.esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        }),
    )
    .await;
    let respuesta = match respuesta {
        Ok(respuesta) => respuesta,
        Err(_) => panic!("el PararLog no encontró la suscripción: quedó huérfana"),
    };
    match respuesta {
        MensajeServidor::FinLog { motivo, .. } => assert_eq!(motivo, MotivoFinLog::Cerrado),
        otro => panic!("{otro:?}"),
    }
    // Si el journalctl llegó a arrancar, el fin de la entrada estándar lo
    // cortó; si no llegó a arrancar, no hay nada corriendo. Nunca huérfano.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let arrancado = binarios.llamadas().iter().any(|llamada| {
        llamada.starts_with("journalctl")
            && llamada.contains("-f")
            && llamada.contains("app.service")
    });
    if arrancado {
        assert!(
            esperar_fichero(
                &binarios.ruta().join("journalctl.paradas"),
                Duration::from_secs(5)
            )
            .await,
            "el journalctl del seguimiento quedó corriendo en el host"
        );
    }
    // Y no siguen llegando líneas después de pararlo.
    let cola = escenario.escuchar(Duration::from_millis(300)).await;
    assert!(
        !cola.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::LineasLog { peticion_id: id, .. } if *id == peticion
        )),
        "llegaron líneas después de PararLog"
    );
}

/// Un cliente que cae justo después de pedir los logs no deja el seguimiento
/// vivo en el host: o no llegó a arrancar, o el servidor lo cortó al caer.
#[tokio::test]
async fn un_cliente_que_cae_con_seguir_log_en_vuelo_no_deja_nada_corriendo() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    let mut ventana = escenario.otra_ventana().await;
    ventana
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 5,
            sudo: None,
        })
        .await;
    // La ventana se va sin esperar respuesta.
    drop(ventana);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let arrancado = binarios.llamadas().iter().any(|llamada| {
        llamada.starts_with("journalctl")
            && llamada.contains("-f")
            && llamada.contains("app.service")
    });
    if arrancado {
        assert!(
            esperar_fichero(
                &binarios.ruta().join("journalctl.paradas"),
                Duration::from_secs(5)
            )
            .await,
            "el seguimiento de la ventana caída quedó corriendo en el host"
        );
    }
}

/// Un seguimiento sigue vivo pasada la gracia de 30 s del pool: la conexión
/// que lo sostiene cuenta como canal hasta que se cierra la suscripción.
#[tokio::test]
async fn un_seguimiento_sigue_vivo_pasada_la_gracia_del_pool() {
    let (mut escenario, binarios) = escenario_servicios().await;
    let host = escenario.hosts[0];
    binarios.unidad("app.service", &[]);
    // El journalctl falso escribe una línea cada 2 s: basta para ver que sigue
    // vivo sin llenar el socket.
    binarios.marcar("despacio", "");
    calentar_privilegio(&mut escenario, host).await;
    let peticion = PETICION;
    escenario
        .enviar(&MensajeCliente::SeguirLog {
            peticion_id: peticion,
            host_id: host,
            objetivo: Objetivo::unidad("app.service"),
            lineas_iniciales: 2,
            sudo: None,
        })
        .await;
    // Primer lote con las líneas iniciales.
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::LineasLog {
                    peticion_id: id,
                    lineas,
                    ..
                } if *id == peticion && !lineas.is_empty()
            )
        })
        .await;
    // La gracia del pool son 30 s sin canales (la revisora mira cada 5 s): si
    // la conexión se hubiera soltado al abrir el canal, aquí llegaría
    // FinLog(conexion_caida). Se espera a 40 s para cubrir los dos márgenes.
    let llegados = escenario.escuchar(Duration::from_secs(40)).await;
    assert!(
        !llegados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::FinLog { peticion_id: id, motivo }
                if *id == peticion && *motivo == MotivoFinLog::ConexionCaida
        )),
        "el seguimiento se cortó al vencer la gracia del pool"
    );
    assert!(
        llegados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::LineasLog { peticion_id: id, .. } if *id == peticion
        )),
        "el seguimiento dejó de recibir líneas pasada la gracia del pool"
    );
    // Y PararLog lo cierra como siempre.
    escenario
        .enviar(&MensajeCliente::PararLog {
            peticion_id: peticion,
        })
        .await;
    let respuesta = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::FinLog {
                    peticion_id: id,
                    ..
                } if *id == peticion
            )
        })
        .await;
    match respuesta {
        MensajeServidor::FinLog { motivo, .. } => assert_eq!(motivo, MotivoFinLog::Cerrado),
        otro => panic!("{otro:?}"),
    }
}
