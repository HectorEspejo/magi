#!/bin/sh
# terminal_tmux.sh — sesión persistente con tmux de MAGI (Fase 15).
#
# MAGI lo ejecuta con `sh -c "$SCRIPT" magi-tmux <suborden> [<nombre>]` por un
# canal del pool (salvo `abrir`, que va en el PTY de la pestaña). El nombre se
# valida aquí y en Rust y va siempre como argumento: nunca se interpola en una
# orden de tmux. En el host solo se usa sh POSIX y tmux 1.8 o posterior.

set -u

uso() {
    printf '%s\n' 'uso: terminal_tmux.sh comprobar|abrir|listar|terminar [nombre]' >&2
    exit 64
}

nombre_valido() {
    case "$1" in
        ''|*[!A-Za-z0-9_-]*) return 1 ;;
    esac
    [ "${#1}" -le 64 ]
}

SUBORDEN=${1:-}
NOMBRE=${2:-}

case "$SUBORDEN" in
    comprobar|abrir|terminar)
        if [ $# -ne 2 ] || ! nombre_valido "$NOMBRE"; then
            uso
        fi
        ;;
    listar)
        if [ $# -ne 1 ]; then
            uso
        fi
        ;;
    *)
        uso
        ;;
esac

# Todavía no implementado: la pista del núcleo escribe el script de verdad.
case "$SUBORDEN" in
    comprobar)
        printf '%s\n' 'MAGI_TMUX=no'
        printf '%s\n' 'MAGI_SESION=nueva'
        ;;
    abrir)
        exec "${SHELL:-/bin/sh}" -l
        ;;
    listar)
        printf '%s\n' 'MAGI_TMUX=no'
        ;;
    terminar)
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
        ;;
esac
