#!/bin/sh
# Sondeo de MAGI ejecutado por el host remoto vía exec. Solo depende de sh y
# coreutils. Argumentos:
#   $1  <motores>: motores de contenedores separados por comas (docker,podman)
#       o "-" para no mirar contenedores (R125).
#   $2… unidades systemd vigiladas del inventario.
# Sin systemctl, los servicios quedan como unknown y no hay fallos de unidades;
# sin permiso para el motor, el sondeo lo dice y no falla.
#
# Los argumentos llegan validados desde Rust, pero el script no se fía: solo
# imprime unidades con el patrón de la F2 y solo ejecuta docker o podman.
motores=${1:-}
shift 2>/dev/null || true

# Una unidad válida: [A-Za-z0-9@._-]+ y 256 caracteres como mucho (F2).
es_unidad() {
  case $1 in
    ''|*[!A-Za-z0-9@._-]*) return 1 ;;
  esac
  [ "${#1}" -le 256 ]
}

printf 'MAGI_NUCLEOS=%s\n' "$(nproc 2>/dev/null || echo 1)"
[ -r /proc/loadavg ] && printf 'MAGI_LOADAVG=%s\n' "$(cat /proc/loadavg)"
[ -r /proc/meminfo ] && grep -E '^(MemTotal|MemAvailable):' /proc/meminfo | sed 's/^/MAGI_MEM_/'
df -kP / 2>/dev/null | awk 'NR==2 {print "MAGI_DISCO=" $2 " " $3}'
[ -r /proc/net/dev ] && awk -F'[: ]+' 'NR>2 && $2!="lo" {rx+=$3; tx+=$11} END {print "MAGI_RED=" rx " " tx}' /proc/net/dev
[ -r /proc/uptime ] && printf 'MAGI_UPTIME=%s\n' "$(cut -d' ' -f1 /proc/uptime)"
for u in "$@"; do
  es_unidad "$u" || continue
  estado=$(systemctl is-active "$u" 2>/dev/null)
  carga=$(systemctl show --property=LoadState --value "$u" 2>/dev/null)
  if [ -z "$estado" ] || [ "$carga" = "not-found" ]; then
    estado=unknown
  fi
  printf 'MAGI_SVC=%s=%s\n' "$u" "$estado"
done
# Fallos de la Fase 13 (§7.8): unidades fallidas y contenedores caídos.
# Sin sudo y sin diálogos: lo que no se puede mirar se queda sin datos.
if command -v systemctl >/dev/null 2>&1; then
  fallos_unidades=$(systemctl list-units --type=service --state=failed \
    --plain --no-legend --no-pager 2>/dev/null)
else
  fallos_unidades=
fi
printf '%s' "$fallos_unidades" | awk '
  NF { n++; if (n <= 20) print "MAGI_FALLO_UNIDAD=" $1 }
  END { print "MAGI_FALLOS_UNIDADES=" n+0 }'
# Motores configurados en [servicios] contenedores (R125); "-" o vacío = ninguno.
case $motores in
  -|"") motores= ;;
esac
motor=
sin_permiso=
# Sin expansión de comodines: la lista de motores viene de fuera y aquí solo
# puede haber docker o podman.
set -f
IFS=,
for m in $motores; do
  [ -n "$m" ] || continue
  case $m in
    docker|podman) ;;
    *) continue ;;
  esac
  command -v "$m" >/dev/null 2>&1 || continue
  if salida=$("$m" ps -a --format '{{.Names}}\t{{.State}}\t{{.Status}}' 2>/dev/null); then
    motor=$m
    printf 'MAGI_MOTOR=%s\n' "$m"
    printf '%s' "$salida" | awk -F'\t' '
      $2 == "restarting" || $2 == "dead" ||
      ($2 == "exited" && $3 !~ /^Exited \(0\)/) || $3 ~ /unhealthy/ {
        n++; if (n <= 20) print "MAGI_FALLO_CONTENEDOR=" $1
      }
      END { print "MAGI_FALLOS_CONTENEDORES=" n+0 }'
    break
  fi
  if [ -z "$sin_permiso" ]; then
    sin_permiso=$m
  fi
done
unset IFS
set +f
[ -n "$motor" ] || [ -z "$sin_permiso" ] || printf 'MAGI_MOTOR=%s sin_permiso\n' "$sin_permiso"
