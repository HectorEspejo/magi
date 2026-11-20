#!/bin/sh
# terminal_enganche.sh: el enganche mínimo con PTY de una pestaña de tmux.
# Va en la línea de órdenes del exec (con PTY no hay entrada estándar que lo
# lleve), así que es corto a propósito: lo grande es terminal_tmux.sh, por la
# entrada estándar. Uso (MAGI valida cada dato también en Rust):
#   sh -c <este script> magi-enganche <servidor> <nombre> <soltar> <equipo> <guardar>
# <servidor> magi|habitual, <soltar> y <guardar> 0|1. Con <guardar>, solo se
# engancha si la sesión está libre o es de <equipo>; si no, sale con 101 sin
# tocar nada. Sale con 0 si la sesión terminó, 100 si te soltaron (sigue viva),
# 101 si la tiene otro equipo, 75 si no existía, 76 sin tmux, 74 con tmux
# anterior a 3.0 (solo en magi) y, si tmux no pudo engancharse, con su código.
# Sin rangos de caracteres (dependen del idioma): listas explícitas (R159).

set -u
unset TMUX
TAB=$(printf '\t')

uso() {
    printf '%s\n' 'uso: terminal_enganche.sh <servidor> <nombre> <soltar> <equipo> <guardar>' >&2
    exit 64
}

nombre_valido() {
    case "$1" in
        ''|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-]*) return 1 ;;
    esac
    [ "${#1}" -le 64 ]
}

equipo_valido() {
    case "$1" in
        ''|*[!abcdefghijklmnopqrstuvwxyz0123456789-]*) return 1 ;;
    esac
    [ "${#1}" -le 20 ]
}

# tmux contra el servidor elegido; siempre con -u (UTF-8 aunque el entorno no lo
# diga).
tmux_srv() {
    if [ "$SERVIDOR" = magi ]; then
        tmux -u -L magi -f /dev/null "$@"
    else
        tmux -u "$@"
    fi
}

# Existe la sesión con ese nombre exacto (nunca has-session, que casa prefijos).
sesion_existe() {
    tmux_srv list-sessions -F '#{session_name}' 2>/dev/null | grep -Fxq -e "$1"
}

# Identificador interno de la sesión: sobrevive a un renombrado.
id_de_sesion() {
    tmux_srv list-sessions -F "#{session_id}${TAB}#{session_name}" 2>/dev/null |
        LC_ALL=C awk -F "$TAB" -v nombre="$1" '$2 == nombre { print $1; exit }'
}

# Sigue viva: por el identificador si lo hay, por el nombre si no.
sesion_viva() {
    if [ -n "$2" ]; then
        tmux_srv list-sessions -F '#{session_id}' 2>/dev/null | grep -Fxq -e "$2"
    else
        sesion_existe "$1"
    fi
}

[ "$#" -eq 5 ] || uso
case "$1" in
    magi|habitual) ;;
    *) uso ;;
esac
nombre_valido "$2" || uso
case "$3$5" in
    [01][01]) ;;
    *) uso ;;
esac
equipo_valido "$4" || uso
SERVIDOR=$1
NOMBRE=$2
SOLTAR=$3
EQUIPO=$4
GUARDAR=$5

if ! version=$(tmux -V 2>/dev/null) || [ -z "$version" ]; then
    printf '%s\n' '[MAGI] instala tmux en el host' >&2
    exit 76
fi
if [ "$SERVIDOR" = magi ]; then
    case "$version" in
        'tmux '[012].*|'tmux '[012])
            printf '%s\n' '[MAGI] tmux demasiado antiguo (3.0 o posterior)' >&2
            exit 74
            ;;
    esac
fi
if ! sesion_existe "$NOMBRE"; then
    printf '[MAGI] la sesión «%s» ya no existe\n' "$NOMBRE" >&2
    exit 75
fi
ID=$(id_de_sesion "$NOMBRE")
if [ "$SERVIDOR" = magi ]; then
    if [ "$GUARDAR" = 1 ]; then
        # La guarda y el enganche, en el mismo paso: con clientes de otro equipo
        # (o de uno sin marca), la sesión no se toca.
        ajena=$(tmux_srv list-sessions -F "#{session_name}${TAB}#{session_attached}${TAB}#{@magi_equipo}" 2>/dev/null |
            LC_ALL=C awk -F "$TAB" -v nombre="$NOMBRE" -v equipo="$EQUIPO" '
                $1 == nombre { if ($2 + 0 > 0 && $3 != equipo) print "si"; exit }')
        if [ "$ajena" = si ]; then
            printf '%s\n' '[MAGI] la sesión la tiene ahora otro equipo' >&2
            exit 101
        fi
    fi
    tmux_srv set-option -q -t "=$NOMBRE:" @magi_equipo "$EQUIPO" >/dev/null 2>&1
fi
if [ "$SOLTAR" = 1 ]; then
    tmux_srv attach-session -d -t "=$NOMBRE"
else
    tmux_srv attach-session -t "=$NOMBRE"
fi
fin=$?
if sesion_viva "$NOMBRE" "$ID"; then
    # Sigue viva: o te soltaron (tmux sale con 0) o el cliente no pudo
    # engancharse (sin terminal, por ejemplo).
    if [ "$fin" -ne 0 ]; then
        exit "$fin"
    fi
    exit 100
fi
exit 0
