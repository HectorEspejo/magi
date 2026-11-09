#!/bin/sh
# terminal_tmux.sh — sesión persistente con tmux de MAGI (Fase 15).
#
# MAGI lo ejecuta con "sh -c <script> magi-tmux <suborden> [<nombre>]" por un
# canal del pool (salvo "abrir", que va en el PTY de la pestaña). El nombre se
# valida aquí y en Rust y va siempre como argumento: nunca se interpola en una
# orden de tmux. En el host solo se usa sh POSIX y tmux 1.8 o posterior, y
# siempre el binario "tmux" del PATH (nunca una ruta absoluta).
#
#   comprobar <nombre>  MAGI_TMUX=<versión>|no y MAGI_SESION=existe|nueva
#   abrir <nombre>      exec tmux new-session -A -s <nombre>, o el shell de acceso
#   listar              MAGI_TMUX=<versión>|no y una línea por sesión
#   terminar <nombre>   MAGI_RESULTADO=ok|no_existe tras kill-session

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

# Versión de tmux ya comprobada por detectar_tmux; vacía mientras no se sepa.
VERSION_TMUX=''

# ¿Hay tmux utilizable (1.8 o posterior)? Deja la versión en VERSION_TMUX.
detectar_tmux() {
    [ -n "$VERSION_TMUX" ] && return 0
    command -v tmux >/dev/null 2>&1 || return 1
    salida=$(tmux -V 2>/dev/null) || return 1
    [ -n "$salida" ] || return 1
    version=${salida#tmux }
    version=${version#next-}
    mayor=${version%%.*}
    case "$mayor" in
        ''|*[!0-9]*) return 1 ;;
    esac
    if [ "$mayor" -lt 1 ]; then
        return 1
    fi
    if [ "$mayor" -eq 1 ]; then
        resto=${version#*.}
        menor=${resto%%.*}
        case "$menor" in
            ''|*[!0-9]*) return 1 ;;
        esac
        if [ "$menor" -lt 8 ]; then
            return 1
        fi
    fi
    VERSION_TMUX=$salida
    return 0
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

case "$SUBORDEN" in
    comprobar)
        if ! detectar_tmux; then
            printf '%s\n' 'MAGI_TMUX=no'
            printf '%s\n' 'MAGI_SESION=nueva'
            exit 0
        fi
        printf 'MAGI_TMUX=%s\n' "$VERSION_TMUX"
        # Comparación exacta: `has-session -t` casa por prefijo en tmux y
        # magi-x-1 también casaría con magi-x-10.
        if tmux list-sessions -F '#{session_name}' 2>/dev/null | grep -Fxq "$NOMBRE"; then
            printf '%s\n' 'MAGI_SESION=existe'
        else
            printf '%s\n' 'MAGI_SESION=nueva'
        fi
        ;;
    abrir)
        if ! detectar_tmux; then
            # Sin tmux (o demasiado viejo): el shell de acceso de siempre.
            exec "${SHELL:-/bin/sh}" -l
        fi
        exec tmux new-session -A -s "$NOMBRE"
        ;;
    listar)
        if ! detectar_tmux; then
            printf '%s\n' 'MAGI_TMUX=no'
            exit 0
        fi
        printf 'MAGI_TMUX=%s\n' "$VERSION_TMUX"
        # Separador improbable (US, 0x1f): un nombre de sesión ajeno con «|»
        # no desplaza los campos.
        separador=$(printf '\037')
        formato="#{session_name}${separador}#{session_attached}${separador}#{session_windows}${separador}#{session_created_string}"
        tmux list-sessions -F "$formato" 2>/dev/null | while IFS="$separador" read -r nombre adheridos ventanas creada; do
            case "$adheridos" in
                ''|*[!0-9]*) adheridos=0 ;;
            esac
            if [ "$adheridos" -gt 0 ]; then
                enganchada=1
            else
                enganchada=0
            fi
            printf 'MAGI_SESION\t%s\t%s\t%s\t%s\n' "$nombre" "$enganchada" "$ventanas" "$creada"
        done
        ;;
    terminar)
        if ! detectar_tmux; then
            printf '%s\n' 'MAGI_RESULTADO=no_existe'
            exit 0
        fi
        if tmux kill-session -t "$NOMBRE" 2>/dev/null; then
            printf '%s\n' 'MAGI_RESULTADO=ok'
        else
            printf '%s\n' 'MAGI_RESULTADO=no_existe'
        fi
        ;;
esac
