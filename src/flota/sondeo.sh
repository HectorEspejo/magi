#!/bin/sh
# Ejecutado por MAGI vía exec; argumentos: unidades systemd a comprobar.
printf 'MAGI_NUCLEOS=%s\n' "$(nproc 2>/dev/null || echo 1)"
[ -r /proc/loadavg ] && printf 'MAGI_LOADAVG=%s\n' "$(cat /proc/loadavg)"
[ -r /proc/meminfo ] && grep -E '^(MemTotal|MemAvailable):' /proc/meminfo | sed 's/^/MAGI_MEM_/'
df -kP / 2>/dev/null | awk 'NR==2 {print "MAGI_DISCO=" $2 " " $3}'
[ -r /proc/net/dev ] && awk -F'[: ]+' 'NR>2 && $2!="lo" {rx+=$3; tx+=$11} END {print "MAGI_RED=" rx " " tx}' /proc/net/dev
[ -r /proc/uptime ] && printf 'MAGI_UPTIME=%s\n' "$(cut -d' ' -f1 /proc/uptime)"
for u in "$@"; do
  estado=$(systemctl is-active "$u" 2>/dev/null)
  carga=$(systemctl show --property=LoadState --value "$u" 2>/dev/null)
  if [ -z "$estado" ] || [ "$carga" = "not-found" ]; then
    estado=unknown
  fi
  printf 'MAGI_SVC=%s=%s\n' "$u" "$estado"
done
