#!/bin/sh
# servicios.sh — MAGI lo ejecuta con:
#   sh -c "$SCRIPT" magi-servicios <suborden> <privilegio> <argumentos…>
# La 1.ª línea de la entrada estándar es siempre la contraseña de sudo (vacía si
# no hay). En «logs», el fin de la entrada estándar dice que MAGI ha cerrado el
# canal.
#
# Salida (la lee crates/magi-nucleo/src/servicios/lectura.rs):
#   privilegio → MAGI_UID=<uid>, MAGI_PRIV=<privilegio>, MAGI_FIN
#   sudo       → MAGI_SUDO=ok|rechazada|sin_sudo, MAGI_FIN
#   listar     → MAGI_PRIV, MAGI_SYSTEMD=<versión>|no, MAGI_DIARIO, MAGI_RELOJ,
#                MAGI_BLOQUE servicio|timer … MAGI_FIN_BLOQUE, MAGI_MOTOR,
#                MAGI_CONTENEDOR, MAGI_PROYECTO, MAGI_AVISO, MAGI_FIN
#   consumo    → MAGI_CONSUMO, MAGI_FIN
#   detalle    → MAGI_PROP, MAGI_DEP, MAGI_TEXTO + texto, MAGI_FIN
#   accion     → MAGI_FASE, MAGI_COMPROBACION, MAGI_MENSAJE, MAGI_LINEA,
#                MAGI_RESULTADO, MAGI_FIN
#   logs       → journalctl -o json tal cual, o MAGI_LOG out|err <hora> <texto>
#
# Códigos de salida: 0 bien; 64 argumento inválido; 66 objetivo no encontrado;
# 69 sin permiso; 77 sin privilegios. En «logs»: 0 fin normal (o canal cerrado),
# 66 no encontrado y 69 sin permiso. La contraseña de sudo nunca aparece en la
# línea de órdenes, en la salida ni en ningún registro.
set -u
LC_ALL=C; export LC_ALL

if [ "$#" -lt 2 ]; then
  echo "ERROR argumentos" >&2
  exit 64
fi
orden=$1
priv=$2
shift 2

case $priv in
  root|sudo_sin_clave|sudo_con_clave|sin_sudo) ;;
  *)
    echo "ERROR privilegio" >&2
    exit 64
    ;;
esac

IFS= read -r CLAVE || CLAVE=

# --------------------------------------------------------------- validación
es_unidad() {
  case $1 in
    ''|*[!A-Za-z0-9@._-]*) return 1 ;;
  esac
  [ "${#1}" -le 256 ] || return 1
  case $1 in
    *.service|*.timer) return 0 ;;
    *) return 1 ;;
  esac
}

es_contenedor() {
  [ -n "$1" ] || return 1
  [ "${#1}" -le 128 ] || return 1
  case $1 in
    *[!0-9A-Fa-f]*) ;;
    *)
      if [ "${#1}" -ge 12 ] && [ "${#1}" -le 64 ]; then
        return 0
      fi
      ;;
  esac
  case $1 in
    [!A-Za-z0-9]*) return 1 ;;
    *[!A-Za-z0-9_.-]*) return 1 ;;
  esac
  return 0
}

es_proyecto() {
  case $1 in
    ''|*[!a-z0-9_-]*) return 1 ;;
    [!a-z0-9]*) return 1 ;;
  esac
  [ "${#1}" -le 64 ]
}

es_motor() {
  case $1 in
    docker|podman) return 0 ;;
    *) return 1 ;;
  esac
}

es_ruta_compose() {
  case $1 in
    /*) ;;
    *) return 1 ;;
  esac
  case $1 in
    *//*|*[!A-Za-z0-9._/-]*) return 1 ;;
  esac
  [ "${#1}" -le 4096 ]
}

error_y_sal() {
  echo "ERROR $1" >&2
  exit "$2"
}

# --------------------------------------------------------------- privilegios
# sudo orden a orden, nunca «sudo sh»; el plazo va por fuera de sudo.
con_priv() {
  plazo=$1
  shift
  case $priv in
    root) timeout "$plazo" "$@" ;;
    sudo_sin_clave) timeout "$plazo" sudo -n -- "$@" ;;
    sudo_con_clave)
      if [ -n "$CLAVE" ]; then
        printf '%s\n' "$CLAVE" | timeout "$plazo" sudo -S -p '' -- "$@"
      else
        timeout "$plazo" sudo -n -- "$@"
      fi
      ;;
    *) return 77 ;;
  esac
}

sudo_sin_clave_disponible() {
  timeout 5 sudo -n -v >/dev/null 2>&1
}

exigir_sudo_si_hace_falta() {
  case $priv in
    root|sudo_sin_clave) return 0 ;;
    sudo_con_clave)
      [ -n "$CLAVE" ] && return 0
      sudo_sin_clave_disponible && return 0
      return 77
      ;;
    *) return 77 ;;
  esac
}

# --------------------------------------------------------------- motor
MOTOR=
MODO=directo

# Prepara el motor y decide si hace falta sudo para tocarlo. Podman nunca usa
# sudo: sus contenedores sin root son los del usuario SSH.
preparar_motor() {
  MOTOR=$1
  MODO=directo
  case $MOTOR in
    podman)
      command -v podman >/dev/null 2>&1 || return 66
      timeout 10 podman ps -a -q >/dev/null 2>&1 || return 69
      return 0
      ;;
    docker)
      command -v docker >/dev/null 2>&1 || return 66
      timeout 10 docker ps -a -q >/dev/null 2>&1 && return 0
      case $priv in
        root) return 69 ;;
        sudo_sin_clave)
          if timeout 10 sudo -n -- docker ps -a -q >/dev/null 2>&1; then
            MODO=privilegiado
            return 0
          fi
          return 69
          ;;
        sudo_con_clave)
          if [ -n "$CLAVE" ]; then
            if printf '%s\n' "$CLAVE" | timeout 10 sudo -S -p '' -- docker ps -a -q >/dev/null 2>&1; then
              MODO=privilegiado
              return 0
            fi
            return 69
          fi
          if timeout 10 sudo -n -- docker ps -a -q >/dev/null 2>&1; then
            MODO=privilegiado
            return 0
          fi
          return 69
          ;;
        *) return 69 ;;
      esac
      ;;
    *) return 69 ;;
  esac
}

motor_run() {
  plazo=$1
  shift
  if [ "$MODO" = privilegiado ]; then
    con_priv "$plazo" docker "$@"
  else
    timeout "$plazo" "$MOTOR" "$@"
  fi
}

version_motor() {
  salida=$(timeout 10 "$1" --version 2>/dev/null) || salida=
  version=$(printf '%s\n' "$salida" | sed -n 's/.*[Vv]ersion \([0-9][0-9.]*\).*/\1/p' | head -n 1)
  [ -n "$version" ] || version=desconocida
  printf '%s' "$version"
}

# Busca en qué motor está el contenedor; deja MOTOR y MODO listos.
# 0: encontrado; 66: no existe; 69: existe pero sin permiso.
detectar_motor_para() {
  nombre=$1
  existe=0
  sin_permiso=0
  for motor in docker podman; do
    command -v "$motor" >/dev/null 2>&1 || continue
    salida=$(timeout 10 "$motor" inspect --format '{{.Id}}' "$nombre" 2>&1)
    codigo=$?
    if [ "$codigo" = 0 ]; then
      MOTOR=$motor
      MODO=directo
      return 0
    fi
    case $salida in
      *"No such"*|*"no such"*) existe=1 ;;
      *) sin_permiso=1 ;;
    esac
    if [ "$motor" = docker ]; then
      case $priv in
        root) ;;
        sudo_sin_clave)
          if timeout 10 sudo -n -- docker inspect --format '{{.Id}}' "$nombre" >/dev/null 2>&1; then
            MOTOR=docker
            MODO=privilegiado
            return 0
          fi
          ;;
        sudo_con_clave)
          if [ -n "$CLAVE" ]; then
            if printf '%s\n' "$CLAVE" | timeout 10 sudo -S -p '' -- docker inspect --format '{{.Id}}' "$nombre" >/dev/null 2>&1; then
              MOTOR=docker
              MODO=privilegiado
              return 0
            fi
          elif timeout 10 sudo -n -- docker inspect --format '{{.Id}}' "$nombre" >/dev/null 2>&1; then
            MOTOR=docker
            MODO=privilegiado
            return 0
          fi
          ;;
      esac
    fi
  done
  if [ "$existe" = 1 ] && [ "$sin_permiso" = 0 ]; then
    return 66
  fi
  return 69
}

# Deja FICHEROS con los ficheros Compose del proyecto y MOTOR/MODO listos.
preparar_proyecto() {
  proyecto=$1
  FICHEROS=
  sin_permiso=0
  for motor in docker podman; do
    command -v "$motor" >/dev/null 2>&1 || continue
    if preparar_motor "$motor"; then
      etiquetas=$(motor_run 10 ps -a --filter "label=com.docker.compose.project=$proyecto" \
        --format '{{.Label "com.docker.compose.project.config_files"}}' 2>/dev/null) || etiquetas=
      ficheros=$(printf '%s\n' "$etiquetas" | tr ',' '\n' | while IFS= read -r fichero; do
        if es_ruta_compose "$fichero"; then
          printf '%s\n' "$fichero"
        fi
      done)
      FICHEROS=$(printf '%s\n' "$ficheros" | awk 'NF && !visto[$0]++')
      return 0
    fi
    sin_permiso=1
  done
  if [ "$sin_permiso" = 1 ]; then
    return 69
  fi
  return 66
}

# --------------------------------------------------------------- salida
emitir_lineas() {
  # Recorta por líneas completas: nunca por bytes (no se parte un carácter
  # UTF-8) y no se pierde la última línea aunque no acabe en salto.
  printf '%s\n' "$1" | awk '
    BEGIN { max = 16384 }
    {
      tamano = length($0) + 1
      if (bytes + tamano > max) { exit }
      bytes += tamano
      print
    }
  ' | while IFS= read -r linea; do
    if [ -n "$linea" ]; then
      printf 'MAGI_LINEA %s\n' "$linea"
    fi
  done
}

emitir_mensaje() {
  printf '%s\n' "$1" | head -n 4 | while IFS= read -r linea; do
    if [ -n "$linea" ]; then
      printf 'MAGI_MENSAJE %s\n' "$linea"
    fi
  done
}

# --------------------------------------------------------------- privilegio
sub_privilegio() {
  uid=$(id -u)
  if [ "$uid" = 0 ]; then
    printf 'MAGI_UID=%s\nMAGI_PRIV=root\nMAGI_FIN\n' "$uid"
    return
  fi
  efectivo=sin_sudo
  if command -v sudo >/dev/null 2>&1; then
    err=$(timeout 5 sudo -n -v 2>&1 >/dev/null)
    codigo=$?
    if [ "$codigo" = 0 ]; then
      efectivo=sudo_sin_clave
    else
      case $err in
        *"password is required"*) efectivo=sudo_con_clave ;;
        *) efectivo=sin_sudo ;;
      esac
    fi
  fi
  printf 'MAGI_UID=%s\nMAGI_PRIV=%s\nMAGI_FIN\n' "$uid" "$efectivo"
}

sub_sudo() {
  if [ "$(id -u)" = 0 ]; then
    printf 'MAGI_SUDO=ok\nMAGI_FIN\n'
    return
  fi
  if ! command -v sudo >/dev/null 2>&1; then
    printf 'MAGI_SUDO=sin_sudo\nMAGI_FIN\n'
    return
  fi
  if [ -z "$CLAVE" ]; then
    printf 'MAGI_SUDO=rechazada\nMAGI_FIN\n'
    return
  fi
  err=$(printf '%s\n' "$CLAVE" | timeout 15 sudo -S -p '' -v 2>&1 >/dev/null)
  codigo=$?
  if [ "$codigo" = 0 ]; then
    estado=ok
  else
    case $err in
      *"not in the sudoers"*|*"not allowed to"*|*"may not run sudo"*|*"is not allowed"*)
        estado=sin_sudo
        ;;
      *) estado=rechazada ;;
    esac
  fi
  printf 'MAGI_SUDO=%s\nMAGI_FIN\n' "$estado"
}

# --------------------------------------------------------------- listar
listar_systemd() {
  tipo=$1
  cargadas=$(systemctl list-units --type="$tipo" --all --plain --no-legend --no-pager 2>/dev/null | awk '{print $1}')
  instaladas=$(systemctl list-unit-files --type="$tipo" --plain --no-legend --no-pager 2>/dev/null | awk '{print $1}')
  nombres=$(printf '%s\n%s\n' "$cargadas" "$instaladas" | awk 'NF && !visto[$0]++ && $0 !~ /^-/')
  [ -n "$nombres" ] || return 0
  set -f
  # shellcheck disable=SC2086
  set -- $nombres
  set +f
  systemctl show --no-pager "$@" 2>/dev/null | awk -v tipo="$tipo" '
    { sub(/\r$/, "") }
    $0 == "" {
      if (dentro) { print "MAGI_FIN_BLOQUE"; dentro = 0 }
      next
    }
    !dentro { print "MAGI_BLOQUE " tipo; dentro = 1 }
    { print }
    END { if (dentro) print "MAGI_FIN_BLOQUE" }
  '
}

listar_contenedores() {
  motores=$1
  elegido=
  primer_permiso=
  for motor in $(printf '%s' "$motores" | tr ',' ' '); do
    [ "$motor" = "-" ] && continue
    command -v "$motor" >/dev/null 2>&1 || continue
    if preparar_motor "$motor"; then
      elegido=$motor
      break
    fi
    [ -n "$primer_permiso" ] || primer_permiso=$motor
  done
  if [ -z "$elegido" ]; then
    if [ -n "$primer_permiso" ]; then
      printf 'MAGI_MOTOR=%s sin_permiso\n' "$primer_permiso"
    else
      printf 'MAGI_MOTOR=no\n'
    fi
    return 0
  fi
  version=$(version_motor "$elegido")
  printf 'MAGI_MOTOR=%s %s\n' "$elegido" "$version"
  contenedores=$(motor_run 15 ps -a --format '{{json .}}' 2>/dev/null) || contenedores=
  printf '%s\n' "$contenedores" | while IFS= read -r linea; do
    if [ -n "$linea" ]; then
      printf 'MAGI_CONTENEDOR %s\n' "$linea"
    fi
  done
  etiquetas=$(motor_run 15 ps -a --format '{{.Label "com.docker.compose.project"}}|{{.Label "com.docker.compose.project.config_files"}}' 2>/dev/null) || etiquetas=
  printf '%s\n' "$etiquetas" | awk -F'|' '
    NF >= 2 && $1 != "" && $1 ~ /^[a-z0-9][a-z0-9_-]*$/ {
      proyecto = $1
      if (!(proyecto in visto)) { visto[proyecto] = 1; orden[++n] = proyecto }
      m = split($2, ficheros, ",")
      for (i = 1; i <= m; i++) {
        fichero = ficheros[i]
        if (fichero ~ /^\/[A-Za-z0-9._\/-]+$/ && !((proyecto SUBSEP fichero) in yavisto)) {
          yavisto[proyecto, fichero] = 1
          nf[proyecto]++
          ff[proyecto, nf[proyecto]] = fichero
        }
      }
    }
    END {
      for (j = 1; j <= n; j++) {
        p = orden[j]
        lista = ""
        for (i = 1; i <= nf[p]; i++) {
          if (i > 1) lista = lista ","
          lista = lista "\"" ff[p, i] "\""
        }
        printf "MAGI_PROYECTO {\"nombre\":\"%s\",\"ficheros\":[%s]}\n", p, lista
      }
    }
  '
}

sub_listar() {
  motores=$1
  case $motores in
    -|docker|podman|docker,podman|podman,docker) ;;
    *) error_y_sal motores 64 ;;
  esac
  printf 'MAGI_PRIV=%s\n' "$priv"
  version=
  if command -v systemctl >/dev/null 2>&1; then
    version=$(systemctl --version 2>/dev/null | sed -n '1s/^systemd \([0-9][0-9]*\).*/\1/p')
  fi
  if [ -z "$version" ]; then
    printf 'MAGI_SYSTEMD=no\n'
  else
    printf 'MAGI_SYSTEMD=%s\n' "$version"
    if [ "$version" -ge 230 ] 2>/dev/null; then
      listar_systemd service
      listar_systemd timer
    fi
  fi
  diario=propio
  if [ "$(id -u)" = 0 ]; then
    diario=todo
  elif command -v journalctl >/dev/null 2>&1; then
    if timeout 5 journalctl -q -n 0 --no-pager >/dev/null 2>&1; then
      diario=todo
    elif con_priv 5 journalctl -q -n 0 --no-pager >/dev/null 2>&1; then
      diario=todo
    fi
  fi
  printf 'MAGI_DIARIO=%s\n' "$diario"
  epoch=$(date +%s 2>/dev/null) || epoch=0
  uptime=$(cut -d' ' -f1 /proc/uptime 2>/dev/null | cut -d. -f1)
  [ -n "$uptime" ] || uptime=0
  printf 'MAGI_RELOJ=%s %s\n' "$epoch" "$uptime"
  listar_contenedores "$motores"
  printf 'MAGI_FIN\n'
}

# --------------------------------------------------------------- consumo
sub_consumo() {
  motor=$1
  if ! preparar_motor "$motor"; then
    error_y_sal sin_permiso 69
  fi
  if [ "$MOTOR" = podman ]; then
    salida=$(motor_run 20 stats --no-stream --format json 2>/dev/null) || salida=
    salida=$(printf '%s' "$salida" | tr '\n' ' ')
    if [ -n "$salida" ]; then
      printf 'MAGI_CONSUMO %s\n' "$salida"
    fi
  else
    salida=$(motor_run 20 stats --no-stream --format '{{json .}}' 2>/dev/null) || salida=
    printf '%s\n' "$salida" | while IFS= read -r linea; do
      if [ -n "$linea" ]; then
        printf 'MAGI_CONSUMO %s\n' "$linea"
      fi
    done
  fi
  printf 'MAGI_FIN\n'
}

# --------------------------------------------------------------- detalle
dependencias() {
  unidad=$1
  modo=$2
  if [ "$modo" = reverse ]; then
    salida=$(systemctl list-dependencies --reverse --no-pager "$unidad" 2>/dev/null) || salida=
  else
    salida=$(systemctl list-dependencies --no-pager "$unidad" 2>/dev/null) || salida=
  fi
  tipo=necesita
  [ "$modo" = reverse ] && tipo=necesitada
  # Solo el primer nivel. Con LC_ALL=C, systemctl pinta el árbol en ASCII
  # («* |-nombre» o «* `-nombre»); si la salida llegara en UTF-8 («├─»,
  # «└─»), también se reconoce. Se compara byte a byte para no depender del
  # juego de caracteres.
  printf '%s\n' "$salida" | sed -n '2,$p' | sed -e 's/^● //' | awk '
    { sub(/^[*] /, "") }
    substr($0, 2, 1) == "-" { print substr($0, 3); next }
    substr($0, 1, 1) == sprintf("%c", 226) &&
    substr($0, 2, 1) == sprintf("%c", 148) &&
    (substr($0, 3, 1) == sprintf("%c", 156) || substr($0, 3, 1) == sprintf("%c", 148)) &&
    substr($0, 4, 1) == sprintf("%c", 226) &&
    substr($0, 5, 1) == sprintf("%c", 148) &&
    substr($0, 6, 1) == sprintf("%c", 128) { print substr($0, 7) }
  ' | while IFS= read -r nombre; do
    if [ -n "$nombre" ]; then
      printf 'MAGI_DEP %s %s\n' "$tipo" "$nombre"
    fi
  done
}



detalle_unidad() {
  objetivo=$1
  systemctl show --no-pager "$objetivo" 2>/dev/null | while IFS= read -r linea; do
    if [ -n "$linea" ]; then
      printf 'MAGI_PROP %s\n' "$linea"
    fi
  done
  dependencias "$objetivo" directa
  dependencias "$objetivo" reverse
  printf 'MAGI_TEXTO\n'
  timeout 10 systemctl cat --no-pager "$objetivo" 2>/dev/null | head -c 262144
  printf '\nMAGI_FIN\n'
}

detalle_contenedor_sub() {
  objetivo=$1
  detectar_motor_para "$objetivo"
  codigo=$?
  if [ "$codigo" != 0 ]; then
    error_y_sal objetivo "$codigo"
  fi
  salida=$(motor_run 20 inspect "$objetivo" 2>/dev/null) || salida=
  printf 'MAGI_TEXTO\n'
  printf '%s\n' "$salida" | head -c 262144
  printf '\nMAGI_FIN\n'
}

sub_detalle() {
  tipo=$1
  caso_args=$2
  case $tipo in
    unidad|timer)
      [ "$caso_args" = 2 ] || error_y_sal argumentos 64
      nombre=$3
      es_unidad "$nombre" || error_y_sal objetivo 64
      detalle_unidad "$nombre"
      ;;
    contenedor)
      if [ "$caso_args" = 2 ]; then
        nombre=$3
      elif [ "$caso_args" = 3 ]; then
        es_motor "$3" || error_y_sal motor 64
        nombre=$4
      else
        error_y_sal argumentos 64
      fi
      es_contenedor "$nombre" || error_y_sal objetivo 64
      detalle_contenedor_sub "$nombre"
      ;;
    *) error_y_sal tipo 64 ;;
  esac
}

# --------------------------------------------------------------- comprobación
comprobar_conocida() {
  objetivo=$1
  SALIDA_COMPROBACION=
  case $objetivo in
    nginx.service)
      SALIDA_COMPROBACION=$(con_priv 20 nginx -t 2>&1)
      return $?
      ;;
    caddy.service)
      exec_start=$(systemctl show --no-pager -p ExecStart --value caddy.service 2>/dev/null)
      config=$(printf '%s\n' "$exec_start" | sed -n 's/.*--config \([^ ;]*\).*/\1/p' | head -n 1)
      adaptador=$(printf '%s\n' "$exec_start" | sed -n 's/.*--adapter \([^ ;]*\).*/\1/p' | head -n 1)
      if [ -z "$config" ] || ! es_ruta_compose "$config"; then
        config=/etc/caddy/Caddyfile
      fi
      case $adaptador in
        ''|*[!A-Za-z0-9._-]*) adaptador=caddyfile ;;
      esac
      SALIDA_COMPROBACION=$(con_priv 20 caddy validate --config "$config" --adapter "$adaptador" 2>&1)
      return $?
      ;;
    apache2.service|httpd.service)
      if command -v apachectl >/dev/null 2>&1; then
        SALIDA_COMPROBACION=$(con_priv 20 apachectl configtest 2>&1)
        return $?
      elif command -v apache2ctl >/dev/null 2>&1; then
        SALIDA_COMPROBACION=$(con_priv 20 apache2ctl configtest 2>&1)
        return $?
      fi
      SALIDA_COMPROBACION="apachectl no encontrado"
      return 127
      ;;
    haproxy.service)
      exec_start=$(systemctl show --no-pager -p ExecStart --value haproxy.service 2>/dev/null)
      fichero=$(printf '%s\n' "$exec_start" | sed -n 's/.*-f \([^ ;]*\).*/\1/p' | head -n 1)
      if [ -z "$fichero" ] || ! es_ruta_compose "$fichero"; then
        fichero=/etc/haproxy/haproxy.cfg
      fi
      SALIDA_COMPROBACION=$(con_priv 20 haproxy -c -f "$fichero" 2>&1)
      return $?
      ;;
    *) return 0 ;;
  esac
}

comprobar_proyecto() {
  proyecto=$1
  if ! preparar_proyecto "$proyecto"; then
    SALIDA_COMPROBACION="no se conocen los ficheros de compose del proyecto"
    return 127
  fi
  if [ -z "$FICHEROS" ]; then
    SALIDA_COMPROBACION="no se conocen los ficheros de compose del proyecto"
    return 127
  fi
  if ! motor_run 10 compose version >/dev/null 2>&1; then
    SALIDA_COMPROBACION="compose no encontrado"
    return 127
  fi
  set -- compose -p "$proyecto"
  for fichero in $FICHEROS; do
    set -- "$@" -f "$fichero"
  done
  set -- "$@" config -q
  SALIDA_COMPROBACION=$(motor_run 20 "$@" 2>&1)
  return $?
}

# La comprobación va dentro de la acción, en el mismo canal y justo antes:
# si falla, no se ejecuta nada.
comprobar_antes() {
  comprobacion_de=$1
  if [ "$comprobacion" = ninguna ]; then
    return 0
  fi
  printf 'MAGI_FASE=comprobando\n'
  SALIDA_COMPROBACION=
  codigo=0
  case $comprobacion_de in
    unidad)
      if [ "$comprobacion" = propia ]; then
        SALIDA_COMPROBACION=$(con_priv 20 sh -c "$orden_propia" 2>&1)
        codigo=$?
      else
        comprobar_conocida "$objetivo"
        codigo=$?
      fi
      ;;
    proyecto)
      if [ "$comprobacion" = propia ]; then
        # La comprobación propia de un proyecto se ejecuta con los
        # privilegios de la acción: los del motor (MODO), no con sudo fijo.
        # Con Podman nunca hay sudo y con Docker directo tampoco.
        if [ "$MODO" = privilegiado ]; then
          SALIDA_COMPROBACION=$(con_priv 20 sh -c "$orden_propia" 2>&1)
        else
          SALIDA_COMPROBACION=$(timeout 20 sh -c "$orden_propia" 2>&1)
        fi
        codigo=$?
      else
        comprobar_proyecto "$objetivo"
        codigo=$?
      fi
      ;;
  esac
  if [ "$codigo" = 0 ]; then
    printf 'MAGI_COMPROBACION=ok\n'
    return 0
  fi
  printf 'MAGI_COMPROBACION=fallo %s\n' "$codigo"
  emitir_lineas "$SALIDA_COMPROBACION"
  segundos=$(($(date +%s) - inicio))
  printf 'MAGI_RESULTADO=rechazada_configuracion - %s\n' "$segundos"
  printf 'MAGI_FIN\n'
  exit 0
}

# --------------------------------------------------------------- acción: unidades
ACTIVO=
TIPO_ESTADO=
RESULTADO_ESTADO=
ARRANQUE=

estado_unidad() {
  props=$(systemctl show --no-pager -p ActiveState -p SubState -p Type -p Result \
    -p UnitFileState "$1" 2>/dev/null) || props=
  ACTIVO=$(printf '%s\n' "$props" | sed -n 's/^ActiveState=//p' | head -n 1)
  TIPO_ESTADO=$(printf '%s\n' "$props" | sed -n 's/^Type=//p' | head -n 1)
  RESULTADO_ESTADO=$(printf '%s\n' "$props" | sed -n 's/^Result=//p' | head -n 1)
  ARRANQUE=$(printf '%s\n' "$props" | sed -n 's/^UnitFileState=//p' | head -n 1)
}

exito_unidad() {
  case $accion in
    arrancar|reiniciar|lanzar)
      if [ "$ACTIVO" = active ]; then
        return 0
      fi
      if [ "$TIPO_ESTADO" = oneshot ] && [ "$ACTIVO" = inactive ] && [ "$RESULTADO_ESTADO" = success ]; then
        return 0
      fi
      return 1
      ;;
    parar) [ "$ACTIVO" = inactive ] ;;
    recargar) [ "$ACTIVO" = active ] ;;
    activar) [ "$ARRANQUE" = enabled ] || [ "$ARRANQUE" = enabled-runtime ] ;;
    desactivar) [ "$ARRANQUE" = disabled ] ;;
    *) return 1 ;;
  esac
}

esperar_unidad() {
  limite=$(( $(date +%s) + espera ))
  while :; do
    estado_unidad "$unidad"
    case $accion in
      arrancar|reiniciar|lanzar)
        [ "$ACTIVO" = active ] && break
        [ "$ACTIVO" = failed ] && break
        if [ "$TIPO_ESTADO" = oneshot ] && [ "$ACTIVO" = inactive ]; then
          break
        fi
        ;;
      parar)
        [ "$ACTIVO" = inactive ] && break
        [ "$ACTIVO" = failed ] && break
        ;;
      recargar)
        [ "$ACTIVO" = active ] && break
        ;;
      *) break ;;
    esac
    [ "$(date +%s)" -ge "$limite" ] && break
    sleep 1
  done
}

ultimas_lineas_unidad() {
  unidad=$1
  salida=$(timeout 10 journalctl -u "$unidad" -n 20 --no-pager -o short-iso 2>/dev/null) || salida=
  if [ -z "$salida" ]; then
    salida=$(con_priv 10 journalctl -u "$unidad" -n 20 --no-pager -o short-iso 2>/dev/null) || salida=
  fi
  emitir_lineas "$salida"
}

accion_unidad() {
  unidad=$objetivo
  if [ "$TIPO_OBJETIVO" = timer ]; then
    unidad=${objetivo%.timer}.service
  fi
  case $accion in
    recargar|reiniciar)
      comprobar_antes unidad
      ;;
  esac
  printf 'MAGI_FASE=ejecutando\n'
  case $accion in
    arrancar|lanzar) salida=$(con_priv "$espera" systemctl start "$unidad" 2>&1) ;;
    parar) salida=$(con_priv "$espera" systemctl stop "$unidad" 2>&1) ;;
    reiniciar) salida=$(con_priv "$espera" systemctl restart "$unidad" 2>&1) ;;
    recargar) salida=$(con_priv "$espera" systemctl reload "$unidad" 2>&1) ;;
    activar) salida=$(con_priv "$espera" systemctl enable "$unidad" 2>&1) ;;
    desactivar) salida=$(con_priv "$espera" systemctl disable "$unidad" 2>&1) ;;
    *) error_y_sal accion 64 ;;
  esac
  codigo=$?
  segundos=$(($(date +%s) - inicio))
  if [ "$codigo" = 77 ]; then
    error_y_sal sin_privilegio 77
  fi
  if [ "$codigo" = 124 ]; then
    printf 'MAGI_FASE=esperando\n'
    printf 'MAGI_RESULTADO=sin_confirmar - %s\n' "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  if [ "$codigo" != 0 ]; then
    emitir_mensaje "$salida"
    ultimas_lineas_unidad "$unidad"
    estado_unidad "$unidad"
    printf 'MAGI_RESULTADO=fallido %s %s\n' "${ACTIVO:--}" "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  printf 'MAGI_FASE=esperando\n'
  sleep 2
  esperar_unidad
  estado_unidad "$unidad"
  segundos=$(($(date +%s) - inicio))
  if exito_unidad; then
    printf 'MAGI_RESULTADO=hecho %s %s\n' "${ACTIVO:--}" "$segundos"
  elif [ "$ACTIVO" = failed ] || [ "$ACTIVO" = inactive ] || [ "$ACTIVO" = unknown ]; then
    ultimas_lineas_unidad "$unidad"
    printf 'MAGI_RESULTADO=fallido %s %s\n' "${ACTIVO:--}" "$segundos"
  else
    printf 'MAGI_MENSAJE no se llegó al estado final en %s s\n' "$espera"
    printf 'MAGI_RESULTADO=sin_confirmar %s %s\n' "${ACTIVO:--}" "$segundos"
  fi
  printf 'MAGI_FIN\n'
}

# --------------------------------------------------------------- acción: contenedores
ESTADO_CONT=
SALUD_CONT=

estado_contenedor() {
  salida=$(motor_run 10 inspect --format '{{.State.Status}}|{{if .State.Health}}{{.State.Health.Status}}{{else}}-{{end}}' "$1" 2>/dev/null) || salida=
  ESTADO_CONT=${salida%%|*}
  SALUD_CONT=${salida#*|}
}

exito_contenedor() {
  case $accion in
    parar) [ "$ESTADO_CONT" = exited ] ;;
    *) [ "$ESTADO_CONT" = running ] && [ "$SALUD_CONT" != unhealthy ] ;;
  esac
}

ultimas_lineas_contenedor() {
  salida=$(motor_run 10 logs --tail 20 "$1" 2>&1) || salida=
  emitir_lineas "$salida"
}

esperar_contenedor() {
  limite=$(( $(date +%s) + espera ))
  while :; do
    estado_contenedor "$objetivo"
    case $accion in
      parar)
        [ "$ESTADO_CONT" = exited ] && break
        [ "$ESTADO_CONT" = dead ] && break
        ;;
      *)
        if [ "$ESTADO_CONT" = running ] && [ "$SALUD_CONT" != starting ]; then
          break
        fi
        ;;
    esac
    [ "$(date +%s)" -ge "$limite" ] && break
    sleep 1
  done
}

accion_contenedor() {
  detectar_motor_para "$objetivo"
  codigo=$?
  if [ "$codigo" != 0 ]; then
    error_y_sal objetivo "$codigo"
  fi
  printf 'MAGI_FASE=ejecutando\n'
  case $accion in
    arrancar) salida=$(motor_run "$espera" start "$objetivo" 2>&1) ;;
    parar) salida=$(motor_run "$espera" stop "$objetivo" 2>&1) ;;
    reiniciar) salida=$(motor_run "$espera" restart "$objetivo" 2>&1) ;;
    *) error_y_sal accion 64 ;;
  esac
  codigo=$?
  segundos=$(($(date +%s) - inicio))
  if [ "$codigo" = 124 ]; then
    printf 'MAGI_FASE=esperando\n'
    printf 'MAGI_RESULTADO=sin_confirmar - %s\n' "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  if [ "$codigo" != 0 ]; then
    emitir_mensaje "$salida"
    ultimas_lineas_contenedor "$objetivo"
    estado_contenedor "$objetivo"
    printf 'MAGI_RESULTADO=fallido %s %s\n' "${ESTADO_CONT:--}" "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  printf 'MAGI_FASE=esperando\n'
  sleep 2
  esperar_contenedor
  estado_contenedor "$objetivo"
  segundos=$(($(date +%s) - inicio))
  if exito_contenedor; then
    printf 'MAGI_RESULTADO=hecho %s %s\n' "${ESTADO_CONT:--}" "$segundos"
  else
    ultimas_lineas_contenedor "$objetivo"
    printf 'MAGI_RESULTADO=fallido %s %s\n' "${ESTADO_CONT:--}" "$segundos"
  fi
  printf 'MAGI_FIN\n'
}

# --------------------------------------------------------------- acción: proyectos
EN_MARCHA=0
TOTAL_CONTENEDORES=0

estados_proyecto() {
  salida=$(motor_run 15 ps -a --filter "label=com.docker.compose.project=$objetivo" \
    --format '{{.State}}' 2>/dev/null) || salida=
  EN_MARCHA=0
  TOTAL_CONTENEDORES=0
  for estado in $salida; do
    TOTAL_CONTENEDORES=$((TOTAL_CONTENEDORES + 1))
    if [ "$estado" = running ]; then
      EN_MARCHA=$((EN_MARCHA + 1))
    fi
  done
}

exito_proyecto() {
  if [ "$TOTAL_CONTENEDORES" = 0 ]; then
    return 1
  fi
  case $accion in
    parar) [ "$EN_MARCHA" = 0 ] ;;
    *) [ "$EN_MARCHA" = "$TOTAL_CONTENEDORES" ] ;;
  esac
}

ultimas_lineas_proyecto() {
  salida=$(motor_run 15 ps -a --filter "label=com.docker.compose.project=$objetivo" \
    --format '{{.ID}}' 2>/dev/null) || salida=
  for id in $salida; do
    lineas=$(motor_run 10 logs --tail 10 "$id" 2>&1) || lineas=
    emitir_lineas "$lineas"
  done
}

esperar_proyecto() {
  limite=$(( $(date +%s) + espera ))
  while :; do
    estados_proyecto
    case $accion in
      parar) [ "$TOTAL_CONTENEDORES" -gt 0 ] && [ "$EN_MARCHA" = 0 ] && break ;;
      *) [ "$TOTAL_CONTENEDORES" -gt 0 ] && [ "$EN_MARCHA" = "$TOTAL_CONTENEDORES" ] && break ;;
    esac
    [ "$(date +%s)" -ge "$limite" ] && break
    sleep 1
  done
}

accion_proyecto() {
  if ! preparar_proyecto "$objetivo"; then
    error_y_sal sin_ficheros 66
  fi
  if [ -z "$FICHEROS" ]; then
    error_y_sal sin_ficheros 66
  fi
  case $accion in
    arrancar) verbo=start ;;
    parar) verbo=stop ;;
    reiniciar)
      comprobar_antes proyecto
      verbo=restart
      ;;
    *) error_y_sal accion 64 ;;
  esac
  printf 'MAGI_FASE=ejecutando\n'
  set -- compose -p "$objetivo"
  set -f
  # shellcheck disable=SC2086
  for fichero in $FICHEROS; do
    set -- "$@" -f "$fichero"
  done
  set +f
  set -- "$@" "$verbo"
  salida=$(motor_run "$espera" "$@" 2>&1)
  codigo=$?
  segundos=$(($(date +%s) - inicio))
  if [ "$codigo" = 124 ]; then
    printf 'MAGI_FASE=esperando\n'
    printf 'MAGI_RESULTADO=sin_confirmar - %s\n' "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  if [ "$codigo" != 0 ]; then
    emitir_mensaje "$salida"
    ultimas_lineas_proyecto
    printf 'MAGI_RESULTADO=fallido - %s\n' "$segundos"
    printf 'MAGI_FIN\n'
    exit 0
  fi
  printf 'MAGI_FASE=esperando\n'
  sleep 2
  esperar_proyecto
  segundos=$(($(date +%s) - inicio))
  if exito_proyecto; then
    printf 'MAGI_RESULTADO=hecho en marcha %s\n' "$segundos"
  else
    ultimas_lineas_proyecto
    printf 'MAGI_RESULTADO=fallido - %s\n' "$segundos"
  fi
  printf 'MAGI_FIN\n'
}

# --------------------------------------------------------------- acción
TIPO_OBJETIVO=
objetivo=
accion=
espera=
comprobacion=ninguna
orden_propia=
inicio=0

sub_accion() {
  inicio=$(date +%s)
  case $TIPO_OBJETIVO in
    unidad|timer)
      exigir_sudo_si_hace_falta || error_y_sal sin_privilegio 77
      accion_unidad
      ;;
    contenedor)
      accion_contenedor
      ;;
    proyecto)
      accion_proyecto
      ;;
    *) error_y_sal tipo 64 ;;
  esac
}

# --------------------------------------------------------------- logs
lineas_iniciales=0

logs_unidad() {
  unidad=$objetivo
  if [ "$TIPO_OBJETIVO" = timer ]; then
    unidad=${objetivo%.timer}.service
  fi
  if ! command -v journalctl >/dev/null 2>&1; then
    error_y_sal sin_permiso 69
  fi
  privilegiado=no
  if [ "$(id -u)" != 0 ]; then
    if ! timeout 5 journalctl -q -n 0 --no-pager -u "$unidad" >/dev/null 2>&1; then
      if con_priv 5 journalctl -q -n 0 --no-pager -u "$unidad" >/dev/null 2>&1; then
        privilegiado=si
      else
        error_y_sal sin_permiso 69
      fi
    fi
  fi
  # El seguidor se lanza sin envolverlo en una función: así $! es el proceso
  # real (journalctl o sudo), no la subshell de una función, y se le puede
  # terminar de verdad.
  if [ "$privilegiado" = si ]; then
    case $priv in
      root)
        journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
        ;;
      sudo_sin_clave)
        sudo -n -- journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
        ;;
      sudo_con_clave)
        if [ -n "$CLAVE" ]; then
          printf '%s\n' "$CLAVE" | sudo -S -p '' -- journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
        else
          sudo -n -- journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
        fi
        ;;
      *)
        journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
        ;;
    esac
  else
    journalctl -u "$unidad" -f -n "$lineas_iniciales" -o json --no-pager &
  fi
  seguidor=$!
  exec 3<&0
  (
    cat <&3 >/dev/null
    kill -TERM "$seguidor" 2>/dev/null
    # Si ignorase TERM (un journalctl atascado), se remata con KILL.
    sleep 2
    kill -KILL "$seguidor" 2>/dev/null
  ) &
  espia=$!
  wait "$seguidor" 2>/dev/null
  kill -TERM "$espia" 2>/dev/null
  wait "$espia" 2>/dev/null
  exit 0
}

logs_contenedor() {
  detectar_motor_para "$objetivo"
  codigo=$?
  if [ "$codigo" != 0 ]; then
    error_y_sal objetivo "$codigo"
  fi
  # Los dos flujos van por tuberías con nombre y un solo proceso de «logs»
  # escribe en las dos: así se termina el proceso real y no queda ninguno
  # vivo, aunque esté callado y solo se matase la subshell exterior.
  if ! command -v mktemp >/dev/null 2>&1 || ! command -v mkfifo >/dev/null 2>&1; then
    error_y_sal sin_temporales 70
  fi
  directorio=$(mktemp -d 2>/dev/null) || error_y_sal sin_temporales 70
  tuberia_salida=$directorio/salida
  tuberia_error=$directorio/error
  if ! mkfifo "$tuberia_salida" "$tuberia_error" 2>/dev/null; then
    rm -rf "$directorio"
    error_y_sal sin_temporales 70
  fi
  # Los lectores son bucles del shell (printf no se almacena en búfer): un
  # sed leyendo de una tubería sí acumularía las líneas y llegarían tarde.
  (
    while IFS= read -r linea; do
      case $linea in
        *' '*)
          hora=${linea%% *}
          texto=${linea#* }
          printf 'MAGI_LOG out %s %s\n' "$hora" "$texto"
          ;;
        *)
          printf 'MAGI_LOG out - %s\n' "$linea"
          ;;
      esac
    done < "$tuberia_salida"
  ) &
  lector_salida=$!
  (
    while IFS= read -r linea; do
      case $linea in
        *' '*)
          hora=${linea%% *}
          texto=${linea#* }
          printf 'MAGI_LOG err %s %s\n' "$hora" "$texto"
          ;;
        *)
          printf 'MAGI_LOG err - %s\n' "$linea"
          ;;
      esac
    done < "$tuberia_error"
  ) &
  lector_error=$!
  if [ "$MODO" = privilegiado ]; then
    case $priv in
      sudo_con_clave)
        if [ -n "$CLAVE" ]; then
          # Las tuberías son del usuario SSH: el redirect no pide privilegios.
# shellcheck disable=SC2024
          printf '%s\n' "$CLAVE" | sudo -S -p '' -- docker logs -f --tail "$lineas_iniciales" --timestamps "$objetivo" >"$tuberia_salida" 2>"$tuberia_error" &
        else
          # Las tuberías son del usuario SSH: el redirect no pide privilegios.
# shellcheck disable=SC2024
          sudo -n -- docker logs -f --tail "$lineas_iniciales" --timestamps "$objetivo" >"$tuberia_salida" 2>"$tuberia_error" &
        fi
        ;;
      *)
        # Las tuberías son del usuario SSH: el redirect no pide privilegios.
# shellcheck disable=SC2024
        sudo -n -- docker logs -f --tail "$lineas_iniciales" --timestamps "$objetivo" >"$tuberia_salida" 2>"$tuberia_error" &
        ;;
    esac
  else
    "$MOTOR" logs -f --tail "$lineas_iniciales" --timestamps "$objetivo" >"$tuberia_salida" 2>"$tuberia_error" &
  fi
  productor=$!
  exec 3<&0
  (
    cat <&3 >/dev/null
    kill -TERM "$productor" 2>/dev/null
    # Si ignorase TERM, se remata con KILL.
    sleep 2
    kill -KILL "$productor" 2>/dev/null
    rm -rf "$directorio"
  ) &
  espia=$!
  wait "$productor" 2>/dev/null
  wait "$lector_salida" 2>/dev/null
  wait "$lector_error" 2>/dev/null
  kill -TERM "$espia" 2>/dev/null
  wait "$espia" 2>/dev/null
  rm -rf "$directorio"
  exit 0
}

sub_logs() {
  case $1 in
    unidad|timer|contenedor) ;;
    *) error_y_sal tipo 64 ;;
  esac
  case $2 in
    ''|*[!0-9]*) error_y_sal lineas 64 ;;
  esac
  [ "$2" -le 5000 ] || error_y_sal lineas 64
  lineas_iniciales=$2
  TIPO_OBJETIVO=$1
  objetivo=$3
  case $TIPO_OBJETIVO in
    unidad|timer)
      es_unidad "$objetivo" || error_y_sal objetivo 64
      logs_unidad
      ;;
    contenedor)
      es_contenedor "$objetivo" || error_y_sal objetivo 64
      logs_contenedor
      ;;
  esac
}

# --------------------------------------------------------------- despacho
case $orden in
  privilegio)
    [ "$#" -eq 0 ] || error_y_sal argumentos 64
    sub_privilegio
    ;;
  sudo)
    [ "$#" -eq 0 ] || error_y_sal argumentos 64
    sub_sudo
    ;;
  listar)
    [ "$#" -eq 1 ] || error_y_sal argumentos 64
    sub_listar "$1"
    ;;
  consumo)
    [ "$#" -eq 1 ] || error_y_sal argumentos 64
    es_motor "$1" || error_y_sal motor 64
    sub_consumo "$1"
    ;;
  detalle)
    if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
      error_y_sal argumentos 64
    fi
    sub_detalle "$1" "$#" "$2" "${3:-}" "${4:-}"
    ;;
  accion)
    if [ "$#" -lt 5 ] || [ "$#" -gt 6 ]; then
      error_y_sal argumentos 64
    fi
    TIPO_OBJETIVO=$1
    objetivo=$2
    accion=$3
    espera=$4
    comprobacion=$5
    case $TIPO_OBJETIVO in
      unidad)
        es_unidad "$objetivo" || error_y_sal objetivo 64
        case $accion in
          arrancar|parar|reiniciar|recargar|activar|desactivar) ;;
          *) error_y_sal accion 64 ;;
        esac
        ;;
      timer)
        es_unidad "$objetivo" || error_y_sal objetivo 64
        case $objetivo in
          *.timer) ;;
          *) error_y_sal objetivo 64 ;;
        esac
        [ "$accion" = lanzar ] || error_y_sal accion 64
        ;;
      contenedor)
        es_contenedor "$objetivo" || error_y_sal objetivo 64
        case $accion in
          arrancar|parar|reiniciar) ;;
          *) error_y_sal accion 64 ;;
        esac
        ;;
      proyecto)
        es_proyecto "$objetivo" || error_y_sal objetivo 64
        case $accion in
          arrancar|parar|reiniciar) ;;
          *) error_y_sal accion 64 ;;
        esac
        ;;
      *) error_y_sal tipo 64 ;;
    esac
    case $espera in
      ''|*[!0-9]*) error_y_sal espera 64 ;;
    esac
    [ "$espera" -ge 1 ] && [ "$espera" -le 600 ] || error_y_sal espera 64
    case $comprobacion in
      ninguna|conocida|propia) ;;
      *) error_y_sal comprobacion 64 ;;
    esac
    if [ "$comprobacion" = propia ]; then
      [ "$#" -eq 6 ] || error_y_sal argumentos 64
      orden_propia=$6
      [ -n "$orden_propia" ] || error_y_sal comprobacion 64
      [ "${#orden_propia}" -le 1024 ] || error_y_sal comprobacion 64
      salto=$(printf '\nx')
      salto=${salto%x}
      case $orden_propia in
        *"$salto"*) error_y_sal comprobacion 64 ;;
      esac
    else
      [ "$#" -eq 5 ] || error_y_sal argumentos 64
    fi
    sub_accion
    ;;
  logs)
    if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
      error_y_sal argumentos 64
    fi
    if [ "$#" -eq 4 ]; then
      es_motor "$4" || error_y_sal motor 64
    fi
    sub_logs "$1" "$3" "$2"
    ;;
  *)
    error_y_sal orden 64
    ;;
esac
