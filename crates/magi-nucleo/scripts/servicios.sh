#!/bin/sh
# servicios.sh — MAGI lo ejecuta con:
#   sh -c "$SCRIPT" magi-servicios <suborden> <privilegio> <argumentos…>
# 1.ª línea de stdin: contraseña de sudo (vacía si no hay). En «logs», el fin de
# stdin dice que MAGI ha cerrado el canal.
#
# En construcción (Fase 13): la implementación completa la trae la pista del
# núcleo.
set -u
LC_ALL=C; export LC_ALL
orden=${1:-} priv=${2:-}
IFS= read -r CLAVE || CLAVE=
case $priv in
  root|sudo_sin_clave|sudo_con_clave|sin_sudo) ;;
  *) echo "ERROR privilegio" >&2; exit 64 ;;
esac
case $orden in
  privilegio) printf 'MAGI_UID=%s\nMAGI_PRIV=%s\nMAGI_FIN\n' "$(id -u)" "$priv" ;;
  *) echo "ERROR no_implementado" >&2; exit 70 ;;
esac
