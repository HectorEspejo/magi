#!/bin/sh
# escritorio_voxl.sh — MAGI lo ejecuta con:
#   sh -c "$SCRIPT" magi-voxl <pantalla> <puerto> <programa> <portapapeles>
# La contraseña llega en la primera línea de stdin; el resto de stdin solo sirve para
# saber cuándo cierra MAGI el canal.
set -u
pantalla=$1 puerto=$2 programa=$3 portapapeles=$4
case $pantalla in :[0-9]|:[0-9][0-9]|:[0-9].[0-9]|:[0-9][0-9].[0-9]) ;; *) echo "ERROR pantalla" >&2; exit 64 ;; esac
case $puerto in ''|*[!0-9]*) echo "ERROR puerto" >&2; exit 64 ;; esac
[ "$puerto" -ge 1024 ] && [ "$puerto" -le 65535 ] || { echo "ERROR puerto" >&2; exit 64; }
case $programa in voxl|/*) ;; *) echo "ERROR programa" >&2; exit 64 ;; esac
case $programa in *[!A-Za-z0-9._/-]*) echo "ERROR programa" >&2; exit 64 ;; esac
case $portapapeles in true|false) ;; *) echo "ERROR portapapeles" >&2; exit 64 ;; esac
command -v "$programa" >/dev/null 2>&1 || { echo "ERROR sin_voxl" >&2; exit 69; }
echo "VOXL $("$programa" --version 2>/dev/null | head -n 1)"
IFS= read -r VOXL_PASSWORD || { echo "ERROR sin_clave" >&2; exit 65; }
export VOXL_PASSWORD
DISPLAY=$pantalla "$programa" --addr=127.0.0.1 --port="$puerto" --enable-resize=true \
  --file-transfers=none --audio-enabled=false --microphone-enabled=false \
  --enable-clipboard="$portapapeles" </dev/null &
pid=$!
unset VOXL_PASSWORD
echo "LISTO $pid"
cat >/dev/null                      # espera a que MAGI cierre el canal (o a que caiga SSH)
kill -TERM "$pid" 2>/dev/null
i=0
while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 5 ]; do sleep 1; i=$((i + 1)); done
kill -KILL "$pid" 2>/dev/null
