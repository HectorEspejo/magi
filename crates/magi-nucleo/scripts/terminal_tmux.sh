#!/bin/sh
# terminal_tmux.sh — sesiones persistentes con el tmux de MAGI (Fase 16).
#
# MAGI lo manda por la ENTRADA ESTÁNDAR de un canal exec del pool, leído entero
# antes de ejecutar nada, con un arranque mínimo en la línea de órdenes
# («sh -c '<arranque>' magi-tmux <suborden> <argumentos…>»): en la línea solo
# van ese arranque y los argumentos validados, nunca el script (Dropbear limita
# su longitud, D357). El enganche con PTY, que no puede recibir el script por la
# entrada estándar, es el script aparte terminal_enganche.sh. Todo argumento se
# valida aquí y en Rust y va siempre como argumento: nunca se interpola en una
# orden de tmux. En el host solo se usa sh POSIX, awk y tmux (el binario "tmux"
# del PATH, nunca una ruta absoluta).
#
# <servidor> es "magi" (el tmux propio de MAGI: tmux -L magi -f /dev/null, que
# nunca carga ~/.tmux.conf) o "habitual" (el tmux de siempre del usuario, solo
# para verlo, engancharse a él y terminar sus sesiones).
#
#   comprobar <servidor> <nombre>
#       MAGI_TMUX=<versión>|no, MAGI_SESION=existe|nueva y MAGI_TERM=…
#   crear <nombre> <carpeta> <comando> <líneas> <columnas> <filas>
#       MAGI_TMUX=… y MAGI_RESULTADO=creada|existe|sin_carpeta|antiguo|sin_tmux|error
#       (códigos de salida 0, 73, 72, 74, 76 y 1)
#   preparar <servidor> <nombre>
#       la primera mitad de engancharse: comprueba que la sesión existe, aplica la
#       configuración de MAGI (solo en el servidor propio) y escribe
#       MAGI_ID=<identificador interno de tmux> y MAGI_CREADA=<segundos Unix de
#       su creación>. Sale con 0, 75 si la sesión no existía, 76 sin tmux y 74
#       con un tmux demasiado antiguo. La segunda mitad (la marca @magi_equipo y
#       el attach, con PTY) es terminal_enganche.sh
#   estado
#       MAGI_TMUX=…, MAGI_AHORA=<segundos> y una línea MAGI_SES por sesión de
#       los dos servidores, con los campos separados por 0x1F
#   vista <servidor> <nombre> <líneas>
#       las últimas líneas visibles, en texto plano
#   historial <servidor> <nombre> <líneas>
#       el historial con colores
#   renombrar <nombre> <nuevo>
#       MAGI_RESULTADO=ok|no_existe|ocupado|error
#   terminar <servidor> <nombre>
#       MAGI_RESULTADO=ok|no_existe
#
# Toda orden con -t apunta a «=<nombre>» (coincidencia exacta; sin el «=» tmux
# casa por prefijo y magi-a-1 mataría magi-a-10) y la existencia de una sesión
# se comprueba con la lista exacta de list-sessions, nunca con has-session -t.
# Para las órdenes cuyo objetivo es un panel (capture-pane) o que aceptan un
# panel (set-option), el objetivo es «=<nombre>:».
#
# Sin rangos de caracteres (de la «a» a la «z», del 0 al 9…): dependen del idioma
# del host, y un canal exec no lo fija. Las listas van escritas carácter a
# carácter (R159).
# La última línea es la marca que el arranque exige para dar el script por
# entero.

set -u

# Teclas extendidas del tmux de MAGI (D338, decisión provisional): "on" las
# entrega a quien las pide (modifyOtherKeys o kitty); "always" las entrega
# siempre. Si con Claude Code real Mayús+Intro no llega, cambiar a "always".
EXTENDED_KEYS=on

NL='
'
CR=$(printf '\r')
TAB=$(printf '\t')
US=$(printf '\037')
IFS_ORIGINAL=$IFS

# La variable TMUX apunta al servidor en el que ya se está dentro: con ella
# puesta, tmux ignora el socket habitual y se niega a engancharse («sessions
# should be nested with care»). MAGI elige siempre el servidor por su cuenta.
unset TMUX

# Códigos de salida.
SALIDA_USO=64
SALIDA_SIN_CARPETA=72
SALIDA_EXISTE=73
SALIDA_ANTIGUO=74
SALIDA_NO_EXISTE=75
SALIDA_SIN_TMUX=76
SALIDA_FALLO_TMUX=77

# Comando de arranque (§7.2): corre con el shell de acceso del usuario, en modo
# interactivo para que tenga su PATH; al acabar escribe el código y deja un
# shell en la carpeta. El comando va como $1 (dato), nunca dentro de este texto.
# shellcheck disable=SC2016  # el texto es para el shell de la sesión: nada se expande aquí
ENVOLTORIO='"${SHELL:-/bin/sh}" -l -i -c "$1"; c=$?; printf "\n[MAGI] %s terminó con código %s\n" "$1" "$c"; exec "${SHELL:-/bin/sh}" -l'

# Límites de los argumentos (los mismos que en Rust, terminal/tmux.rs).
MAXIMO_CARPETA=1024
MAXIMO_COMANDO=4096
MAXIMO_LINEAS_HISTORIAL=1000000

# Servidor sobre el que actúa tmux_srv: "magi" o "habitual".
SERVIDOR=habitual

rechazar() {
    printf 'uso: terminal_tmux.sh %s\n' "$1" >&2
    exit "$SALIDA_USO"
}

uso() {
    rechazar 'comprobar|crear|preparar|estado|vista|historial|renombrar|terminar [argumentos]'
}

nombre_valido() {
    case "$1" in
        ''|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-]*) return 1 ;;
    esac
    [ "${#1}" -le 64 ]
}

servidor_valido() {
    case "$1" in
        magi|habitual) return 0 ;;
    esac
    return 1
}

# Etiqueta del equipo: minúsculas, dígitos y guion, de 1 a 20 caracteres.
equipo_valido() {
    case "$1" in
        ''|*[!abcdefghijklmnopqrstuvwxyz0123456789-]*) return 1 ;;
    esac
    [ "${#1}" -le 20 ]
}

# numero_valido <texto> <mínimo> <máximo>: solo dígitos, sin ceros a la
# izquierda y dentro del rango.
numero_valido() {
    case "$1" in
        ''|*[!0123456789]*|0?*) return 1 ;;
    esac
    [ "${#1}" -le 9 ] || return 1
    [ "$1" -ge "$2" ] && [ "$1" -le "$3" ]
}

# Bytes de un texto (con wc, que cuenta bytes sea cual sea el idioma).
bytes_de() {
    longitud=$(printf '%s' "$1" | wc -c)
    printf '%s' $((longitud + 0))
}

# Carpeta del host: vacía, absoluta o «~/…»; sin saltos de línea.
carpeta_valida() {
    [ -z "$1" ] && return 0
    # shellcheck disable=SC2088  # la tilde literal es lo que se busca
    case "$1" in
        *"$NL"*|*"$CR"*) return 1 ;;
        /*|"~/"*) ;;
        *) return 1 ;;
    esac
    [ "$(bytes_de "$1")" -le "$MAXIMO_CARPETA" ]
}

# --------------------------------------------------------------- tmux

# Versión de tmux ya comprobada por detectar_tmux; vacía mientras no se sepa.
VERSION_TMUX=''
VERSION_MAYOR=0
VERSION_MENOR=0

# ¿Hay tmux utilizable (1.8 o posterior)? Deja el texto de «tmux -V» en
# VERSION_TMUX y la versión en VERSION_MAYOR y VERSION_MENOR. Entiende
# «tmux 3.3a», «tmux next-3.6» y «tmux master»: la letra no cuenta.
detectar_tmux() {
    [ -n "$VERSION_TMUX" ] && return 0
    command -v tmux >/dev/null 2>&1 || return 1
    salida=$(tmux -V 2>/dev/null) || return 1
    [ -n "$salida" ] || return 1
    version=${salida#tmux }
    version=${version#next-}
    case "$version" in
        master*|openbsd-*)
            # Un tmux de desarrollo: se trata como el más nuevo.
            mayor=99
            menor=0
            ;;
        *)
            mayor=${version%%[!0123456789]*}
            case "$mayor" in
                ''|*[!0123456789]*) return 1 ;;
            esac
            [ "${#mayor}" -le 4 ] || return 1
            resto=${version#"$mayor"}
            menor=0
            case "$resto" in
                .[0123456789]*)
                    resto=${resto#.}
                    menor=${resto%%[!0123456789]*}
                    [ "${#menor}" -le 4 ] || return 1
                    ;;
            esac
            ;;
    esac
    if [ "$mayor" -lt 1 ]; then
        return 1
    fi
    if [ "$mayor" -eq 1 ] && [ "$menor" -lt 8 ]; then
        return 1
    fi
    VERSION_MAYOR=$mayor
    VERSION_MENOR=$menor
    VERSION_TMUX=$salida
    return 0
}

# ¿La versión de tmux es la dada o posterior? version_al_menos <mayor> <menor>
version_al_menos() {
    [ "$VERSION_MAYOR" -gt "$1" ] && return 0
    [ "$VERSION_MAYOR" -eq "$1" ] && [ "$VERSION_MENOR" -ge "$2" ]
}

# Ejecuta tmux contra el servidor elegido en SERVIDOR. Siempre con «-u»: sin él,
# un host cuyo entorno no dice UTF-8 (LANG sin poner, como en muchos canales
# exec de ssh) hace que tmux cambie por «_» todo lo que no es ASCII imprimible
# en lo que escribe, tabuladores y 0x1F incluidos, y que «estado» no vea ni una
# sesión; y, enganchado, que pinte «_» en vez de las tildes.
tmux_srv() {
    if [ "$SERVIDOR" = magi ]; then
        tmux -u -L magi -f /dev/null "$@"
    else
        tmux -u "$@"
    fi
}

# ¿Existe la sesión con ese nombre exacto? (la lista exacta, nunca has-session)
sesion_existe() {
    tmux_srv list-sessions -F '#{session_name}' 2>/dev/null | grep -Fxq -e "$1"
}

# Identificador interno de tmux ($0, $1…) de la sesión con ese nombre exacto;
# vacío si no se sabe. Sobrevive a un renombrado y no se repite mientras viva
# el servidor, así que sirve para saber si «la misma» sesión sigue viva.
id_de_sesion() {
    tmux_srv list-sessions -F "#{session_id}${TAB}#{session_name}" 2>/dev/null |
        LC_ALL=C awk -F "$TAB" -v nombre="$1" '$2 == nombre { print $1; exit }'
}

# ¿Sigue viva la sesión? sesion_viva <nombre> <identificador, o vacío>. Con
# identificador, por él (aunque la hayan renombrado); sin él, por el nombre.
sesion_viva() {
    if [ -n "$2" ]; then
        tmux_srv list-sessions -F '#{session_id}' 2>/dev/null | grep -Fxq -e "$2"
    else
        sesion_existe "$1"
    fi
}

# ¿Tiene el host la terminfo de tmux-256color? Sin ella, screen-256color.
tiene_terminfo() {
    if command -v infocmp >/dev/null 2>&1; then
        infocmp "$1" >/dev/null 2>&1
        return
    fi
    for directorio in "${HOME:-/nonexistent}/.terminfo" /etc/terminfo /lib/terminfo /usr/share/terminfo /usr/lib/terminfo /usr/share/lib/terminfo; do
        # La inicial del terminal es la carpeta (en macOS, su código hexadecimal).
        if [ -f "$directorio/t/$1" ] || [ -f "$directorio/74/$1" ]; then
            return 0
        fi
    done
    return 1
}

terminfo_interno() {
    if tiene_terminfo tmux-256color; then
        printf '%s' tmux-256color
    else
        printf '%s' screen-256color
    fi
}

# --------------------------------------------------- configuración de MAGI

# Las órdenes «set» de la configuración, como palabras separadas por saltos de
# línea (ninguna lleva espacios ni saltos). Se aplica con una sola invocación
# de tmux, separadas por «;», porque con la opción «always» tmux fija el modo
# de teclado de cada pantalla al crearla (D338).
CONF=''

palabra_conf() {
    CONF="${CONF:+$CONF$NL}$1"
}

# opcion <alcance> <nombre> <valor>: «set -<alcance>q <nombre> <valor>». El
# «-q» hace que un tmux que no conozca la opción no falle (las opciones nuevas
# van además protegidas por la versión).
opcion() {
    if [ -n "$CONF" ]; then
        palabra_conf ';'
    fi
    palabra_conf set
    palabra_conf "-${1}q"
    palabra_conf "$2"
    palabra_conf "$3"
}

# preparar_configuracion <líneas de historial, vacío para no tocarlas>
# <crear|enganchar>
preparar_configuracion() {
    CONF=''
    opcion g status off
    opcion g prefix None
    opcion g prefix2 None
    opcion g mouse on
    if version_al_menos 3 2; then
        opcion sg extended-keys "$EXTENDED_KEYS"
    fi
    if version_al_menos 3 5; then
        # En 3.5 el formato por defecto es xterm; MAGI codifica con csi-u.
        opcion sg extended-keys-format csi-u
    fi
    opcion sg set-clipboard on
    opcion sg focus-events on
    if version_al_menos 3 3; then
        opcion g allow-passthrough on
    fi
    opcion g set-titles on
    opcion g set-titles-string '#T'
    if [ -n "$1" ]; then
        opcion g history-limit "$1"
    fi
    opcion sg default-terminal "$(terminfo_interno)"
    if version_al_menos 3 2; then
        # Sin extkeys, tmux no le pide nada a MAGI: no reconoce su XTVERSION.
        opcion sg terminal-features 'xterm*:RGB:clipboard:extkeys:focus:title'
    elif [ "$2" = crear ]; then
        # 3.0 y 3.1: color completo con terminal-overrides. Solo al crear, en un
        # servidor recién nacido: añadir en cada enganche lo duplicaría.
        opcion sga terminal-overrides ',xterm*:Tc'
    fi
    opcion sg escape-time 10
    if version_al_menos 3 1; then
        opcion g window-size latest
    fi
    opcion g monitor-bell on
    opcion g bell-action any
}

# lanzar_propio [<orden de tmux>…]: ejecuta tmux -L magi con la configuración
# ya preparada en CONF. Con una orden, «start-server ; set … ; <orden>» en una
# sola invocación (un servidor sin sesiones se acaba enseguida, y la
# configuración tiene que estar antes de crear la sesión); sin ella, solo las
# órdenes «set», para un servidor que ya corre.
lanzar_propio() {
    cuantas=$#
    IFS=$NL
    set -f
    if [ "$cuantas" -gt 0 ]; then
        # shellcheck disable=SC2086
        set -- "$@" start-server ';' $CONF ';'
    else
        # shellcheck disable=SC2086
        set -- $CONF
    fi
    IFS=$IFS_ORIGINAL
    set +f
    # La orden va al final: se mueven al fondo las palabras que había.
    movidas=0
    while [ "$movidas" -lt "$cuantas" ]; do
        set -- "$@" "$1"
        shift
        movidas=$((movidas + 1))
    done
    tmux -u -L magi -f /dev/null "$@"
}

# ------------------------------------------------------------ subórdenes

# Hay que tener tmux y, en el servidor propio, 3.0 o posterior. Sale con el
# código que toque si no (el mensaje va a stderr, que MAGI enseña en la pestaña).
exigir_tmux() {
    if ! detectar_tmux; then
        printf '%s\n' '[MAGI] instala tmux en el host' >&2
        exit "$SALIDA_SIN_TMUX"
    fi
    if [ "$SERVIDOR" = magi ] && ! version_al_menos 3 0; then
        printf '%s\n' '[MAGI] tmux demasiado antiguo (3.0 o posterior)' >&2
        exit "$SALIDA_ANTIGUO"
    fi
}

hacer_comprobar() {
    SERVIDOR=$1
    NOMBRE=$2
    if ! detectar_tmux; then
        printf '%s\n' 'MAGI_TMUX=no'
        printf '%s\n' 'MAGI_SESION=nueva'
        exit 0
    fi
    printf 'MAGI_TMUX=%s\n' "$VERSION_TMUX"
    if sesion_existe "$NOMBRE"; then
        printf '%s\n' 'MAGI_SESION=existe'
    else
        printf '%s\n' 'MAGI_SESION=nueva'
    fi
    printf 'MAGI_TERM=%s\n' "$(terminfo_interno)"
}

resultado_crear() {
    printf 'MAGI_RESULTADO=%s\n' "$1"
    exit "$2"
}

hacer_crear() {
    SERVIDOR=magi
    NOMBRE=$1
    CARPETA=$2
    COMANDO=$3
    LINEAS=$4
    COLUMNAS=$5
    FILAS=$6
    if ! detectar_tmux; then
        printf '%s\n' 'MAGI_TMUX=no'
        resultado_crear sin_tmux "$SALIDA_SIN_TMUX"
    fi
    printf 'MAGI_TMUX=%s\n' "$VERSION_TMUX"
    if ! version_al_menos 3 0; then
        resultado_crear antiguo "$SALIDA_ANTIGUO"
    fi
    if sesion_existe "$NOMBRE"; then
        resultado_crear existe "$SALIDA_EXISTE"
    fi
    # «~/» pasa a $HOME/ sin eval.
    DIRECTORIO=''
    # shellcheck disable=SC2088  # la tilde literal es lo que se busca
    case "$CARPETA" in
        '') ;;
        "~/"*)
            if [ -z "${HOME:-}" ]; then
                resultado_crear sin_carpeta "$SALIDA_SIN_CARPETA"
            fi
            DIRECTORIO="${HOME}/${CARPETA#"~/"}"
            ;;
        *) DIRECTORIO=$CARPETA ;;
    esac
    if [ -n "$DIRECTORIO" ] && [ ! -d "$DIRECTORIO" ]; then
        resultado_crear sin_carpeta "$SALIDA_SIN_CARPETA"
    fi
    # tmux toma un argumento que acaba en «;» como el fin de la orden y se lo
    # quita (con «\;» al final, deja solo el «;»): «find … -exec … \;» perdería
    # la barra y una carpeta «datos;» se cambiaría por «datos». Un espacio al
    # final del comando (para el shell da igual) y una barra al final de la
    # carpeta (sigue siendo la misma) lo evitan.
    case "$COMANDO" in
        *\;) COMANDO="$COMANDO " ;;
    esac
    case "$DIRECTORIO" in
        *\;) DIRECTORIO="$DIRECTORIO/" ;;
    esac
    # tmux expande formatos en «-c»: un «#» tiene que ir como «##». Sin esto,
    # «~/notas/#Docs» arrancaría en el home (por «#D») y un «#(orden)» en el
    # nombre de la carpeta ejecutaría la orden.
    case "$DIRECTORIO" in
        *'#'*) DIRECTORIO=$(printf '%s' "$DIRECTORIO" | LC_ALL=C sed 's/#/##/g') ;;
    esac
    preparar_configuracion "$LINEAS" crear
    # Los datos del usuario (carpeta, comando) van como argumentos sueltos.
    set -- new-session -d -s "$NOMBRE" -x "$COLUMNAS" -y "$FILAS"
    if [ -n "$DIRECTORIO" ]; then
        set -- "$@" -c "$DIRECTORIO"
    fi
    if [ -n "$COMANDO" ]; then
        set -- "$@" -- sh -c "$ENVOLTORIO" magi-sesion "$COMANDO"
    fi
    if lanzar_propio "$@"; then
        resultado_crear creada 0
    fi
    # Otro equipo pudo crearla entre la comprobación y ahora.
    if sesion_existe "$NOMBRE"; then
        resultado_crear existe "$SALIDA_EXISTE"
    fi
    resultado_crear error 1
}

# La primera mitad de engancharse (la segunda, con PTY, es
# terminal_enganche.sh): la sesión tiene que existir, y en el servidor propio se
# aplica la configuración de MAGI (idempotente: se repite en cada enganche, y
# sin history-limit porque el de la sesión ya está fijado desde que se creó). La
# marca @magi_equipo NO se pone aquí: la pone el enganche, después de mirar que
# la sesión sigue libre cuando es una reconexión automática (R157). Escribe el
# identificador interno de la sesión y su hora de creación, con los que MAGI la
# sigue aunque otro equipo la renombre.
hacer_preparar() {
    SERVIDOR=$1
    NOMBRE=$2
    exigir_tmux
    if ! sesion_existe "$NOMBRE"; then
        printf '[MAGI] la sesión «%s» ya no existe\n' "$NOMBRE" >&2
        exit "$SALIDA_NO_EXISTE"
    fi
    # Identificador y creación de la sesión: con los dos se sabe que sigue siendo
    # la misma aunque la renombren y aunque tmux se haya reiniciado y reutilice
    # identificadores.
    identidad=$(tmux_srv list-sessions -F "#{session_name}${TAB}#{session_id}${TAB}#{session_created}" 2>/dev/null |
        LC_ALL=C awk -F "$TAB" -v nombre="$NOMBRE" '$1 == nombre { print $2 " " $3; exit }')
    printf 'MAGI_ID=%s\nMAGI_CREADA=%s\n' "${identidad%% *}" "${identidad#* }"
    if [ "$SERVIDOR" = magi ]; then
        # Solo el tmux propio: el habitual no se toca (ni su configuración ni
        # sus opciones).
        preparar_configuracion '' enganchar
        lanzar_propio >/dev/null 2>&1
    fi
    exit 0
}

# Texto de «date +%s» (con awk de respaldo) para MAGI_AHORA.
ahora_del_host() {
    ahora=$(date +%s 2>/dev/null)
    case "$ahora" in
        ''|*[!0123456789]*) ahora=$(awk 'BEGIN { srand(); print srand() }' 2>/dev/null) ;;
    esac
    case "$ahora" in
        ''|*[!0123456789]*) return 1 ;;
    esac
    printf '%s' "$ahora"
}

# Ejecuta una orden de lista de tmux y deja su salida en $SALIDA_LISTA. Un
# servidor que no existe o no contesta («no server running», «error connecting
# to…») es una lista vacía; cualquier otro fallo (un tmux nuevo frente a un
# servidor viejo tras un `apt upgrade`, por ejemplo) devuelve 1: con las
# sesiones vivas, un fallo no puede pasar por «no hay sesiones», o MAGI las
# daría por terminadas.
leer_lista() {
    SALIDA_LISTA=$(tmux_srv "$@" 2>&1)
    codigo_lista=$?
    if [ "$codigo_lista" -eq 0 ]; then
        return 0
    fi
    case "$SALIDA_LISTA" in
        *'no server running'*|*'error connecting to'*|*'No such file or directory'*|*'failed to connect to server'*)
            SALIDA_LISTA=''
            return 0
            ;;
    esac
    SALIDA_LISTA=''
    return 1
}

# Una línea MAGI_SES por sesión del servidor $1, con los campos separados por
# 0x1F: servidor, nombre, identificador, clientes, equipo, actividad, creada,
# campana, ventanas y título. Solo salen las sesiones con un nombre que MAGI
# pueda manejar; el título va sin caracteres de control y a 400 bytes como
# mucho, y la lista, a 500 sesiones por servidor. La actividad y la campana son
# las de las ventanas.
#
# Entre tmux y awk los campos van separados por tabuladores, no por 0x1F: tmux
# 3.4 y 3.5 escriben los caracteres de control de la salida de list-sessions
# como «\037» (3.2 y 3.7 los dejan tal cual), y el tabulador sobrevive en todas.
listar_servidor() {
    SERVIDOR=$1
    if ! leer_lista list-sessions -F "S${TAB}#{session_name}${TAB}#{session_attached}${TAB}#{@magi_equipo}${TAB}#{session_created}${TAB}#{session_windows}${TAB}#{session_activity}${TAB}#{session_id}${TAB}#{pane_title}"; then
        return 1
    fi
    LISTA_SESIONES=$SALIDA_LISTA
    if ! leer_lista list-windows -a -F "W${TAB}#{session_name}${TAB}#{window_activity}${TAB}#{window_bell_flag}"; then
        return 1
    fi
    LISTA_VENTANAS=$SALIDA_LISTA
    {
        printf '%s\n' "$LISTA_SESIONES"
        printf '%s\n' "$LISTA_VENTANAS"
    } | LC_ALL=C awk -F "$TAB" -v OFS="$US" -v servidor="$1" '
        function limpio(texto) {
            gsub(/[[:cntrl:]]/, "", texto)
            return texto
        }
        function numero(texto) {
            return (texto ~ /^[0123456789]+$/) ? texto + 0 : -1
        }
        # Un entero siempre en decimal: con «print», el awk de Debian 10 y
        # Ubuntu 18.04 (mawk 1.3.3) escribe lo que pasa de 2^31, como una hora
        # Unix, en notación científica («1.79485e+09») y Rust no la lee.
        function entero(valor) {
            return sprintf("%.0f", valor)
        }
        $1 == "S" && NF >= 9 && n < 500 {
            nombre = $2
            if (nombre !~ /^[abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-]+$/ || length(nombre) > 64) next
            clientes = numero($3); creada = numero($5); ventanas = numero($6); actividad_sesion = numero($7)
            if (clientes < 0 || creada < 0 || ventanas < 0 || actividad_sesion < 0) next
            titulo = $9
            for (i = 10; i <= NF; i++) titulo = titulo $i
            n++
            orden[n] = nombre
            cl[nombre] = clientes; eq[nombre] = substr(limpio($4), 1, 64); cr[nombre] = creada
            vt[nombre] = ventanas; ac[nombre] = actividad_sesion; ti[nombre] = substr(limpio(titulo), 1, 400)
            # El identificador interno de tmux: «$» y dígitos; si no lo es, vacío.
            ident[nombre] = ($8 ~ /^[$][0123456789]+$/ && length($8) <= 12) ? $8 : ""
            next
        }
        $1 == "W" && NF >= 4 {
            if (numero($3) > ventana_ac[$2]) ventana_ac[$2] = numero($3)
            if ($4 == "1") campana[$2] = 1
            tiene_ventana[$2] = 1
            next
        }
        END {
            for (i = 1; i <= n; i++) {
                nombre = orden[i]
                actividad = ventana_ac[nombre] + 0
                if (!tiene_ventana[nombre] || actividad <= 0) actividad = ac[nombre]
                print "MAGI_SES", servidor, nombre, ident[nombre], entero(cl[nombre]), eq[nombre], entero(actividad), entero(cr[nombre]), (campana[nombre] ? 1 : 0), entero(vt[nombre]), ti[nombre]
            }
        }'
}

hacer_estado() {
    if ! detectar_tmux; then
        printf '%s\n' 'MAGI_TMUX=no'
        exit 0
    fi
    printf 'MAGI_TMUX=%s\n' "$VERSION_TMUX"
    if ahora=$(ahora_del_host); then
        printf 'MAGI_AHORA=%s\n' "$ahora"
    fi
    if ! listar_servidor magi || ! listar_servidor habitual; then
        printf '%s\n' '[MAGI] tmux no pudo listar las sesiones del host' >&2
        exit "$SALIDA_FALLO_TMUX"
    fi
}

# El texto de las últimas $3 líneas visibles de la sesión, sin espacios
# finales ni líneas vacías al final.
hacer_vista() {
    SERVIDOR=$1
    NOMBRE=$2
    LINEAS=$3
    exigir_tmux
    if ! sesion_existe "$NOMBRE"; then
        printf '[MAGI] la sesión «%s» no existe\n' "$NOMBRE" >&2
        exit "$SALIDA_NO_EXISTE"
    fi
    # La captura va a una variable: en una tubería, el código de salida sería
    # el de awk y una sesión que desaparece a mitad saldría como una vista vacía.
    if ! captura=$(tmux_srv capture-pane -p -J -t "=$NOMBRE:" 2>/dev/null); then
        printf '[MAGI] la sesión «%s» no existe\n' "$NOMBRE" >&2
        exit "$SALIDA_NO_EXISTE"
    fi
    printf '%s\n' "$captura" | LC_ALL=C awk -v ultimas="$LINEAS" '
        { sub(/[ \t]+$/, ""); linea[NR] = $0 }
        END {
            n = NR
            while (n > 0 && linea[n] == "") n--
            desde = n - ultimas + 1
            if (desde < 1) desde = 1
            for (i = desde; i <= n; i++) print linea[i]
        }'
}

# El historial con colores: $3 líneas por encima de la pantalla visible.
hacer_historial() {
    SERVIDOR=$1
    NOMBRE=$2
    LINEAS=$3
    exigir_tmux
    if ! sesion_existe "$NOMBRE"; then
        printf '[MAGI] la sesión «%s» no existe\n' "$NOMBRE" >&2
        exit "$SALIDA_NO_EXISTE"
    fi
    if ! captura=$(tmux_srv capture-pane -p -e -J -S "-$LINEAS" -t "=$NOMBRE:" 2>/dev/null); then
        printf '[MAGI] la sesión «%s» no existe\n' "$NOMBRE" >&2
        exit "$SALIDA_NO_EXISTE"
    fi
    printf '%s\n' "$captura" | sed 's/ *$//'
}

hacer_renombrar() {
    SERVIDOR=magi
    NOMBRE=$1
    NUEVO=$2
    if ! detectar_tmux; then
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
        exit 0
    fi
    if ! sesion_existe "$NOMBRE"; then
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
        exit 0
    fi
    if sesion_existe "$NUEVO"; then
        printf '%s\n' 'MAGI_RESULTADO=ocupado'
        exit 0
    fi
    # «--»: un nombre nuevo que empiece por «-» no es una opción.
    if tmux_srv rename-session -t "=$NOMBRE" -- "$NUEVO" >/dev/null 2>&1; then
        printf '%s\n' 'MAGI_RESULTADO=ok'
    elif sesion_existe "$NUEVO"; then
        printf '%s\n' 'MAGI_RESULTADO=ocupado'
    elif ! sesion_existe "$NOMBRE"; then
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
    else
        printf '%s\n' 'MAGI_RESULTADO=error'
    fi
}

hacer_terminar() {
    SERVIDOR=$1
    NOMBRE=$2
    if ! detectar_tmux || ! sesion_existe "$NOMBRE"; then
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
        exit 0
    fi
    if tmux_srv kill-session -t "=$NOMBRE" >/dev/null 2>&1; then
        printf '%s\n' 'MAGI_RESULTADO=ok'
    else
        printf '%s\n' 'MAGI_RESULTADO=no_existe'
    fi
}

# --------------------------------------------------------------- despacho

SUBORDEN=${1:-}
ARGUMENTOS=$(($# - 1))
if [ "$ARGUMENTOS" -lt 0 ]; then
    ARGUMENTOS=0
fi

case "$SUBORDEN:$ARGUMENTOS" in
    comprobar:2)
        servidor_valido "${2:-}" || uso
        nombre_valido "${3:-}" || uso
        hacer_comprobar "$2" "$3"
        ;;
    crear:6)
        nombre_valido "${2:-}" || uso
        carpeta_valida "${3:-}" || rechazar 'carpeta absoluta o «~/…», de hasta 1024 bytes y sin saltos de línea'
        [ "$(bytes_de "${4:-}")" -le "$MAXIMO_COMANDO" ] || rechazar 'comando de hasta 4096 bytes'
        numero_valido "${5:-}" 0 "$MAXIMO_LINEAS_HISTORIAL" || rechazar 'líneas de historial no válidas'
        numero_valido "${6:-}" 2 1000 || rechazar 'columnas no válidas (2 a 1000)'
        numero_valido "${7:-}" 1 2000 || rechazar 'filas no válidas (1 a 2000)'
        hacer_crear "$2" "$3" "$4" "$5" "$6" "$7"
        ;;
    preparar:2)
        servidor_valido "${2:-}" || uso
        nombre_valido "${3:-}" || uso
        hacer_preparar "$2" "$3"
        ;;
    estado:0)
        hacer_estado
        ;;
    vista:3|historial:3)
        servidor_valido "${2:-}" || uso
        nombre_valido "${3:-}" || uso
        numero_valido "${4:-}" 1 100000 || rechazar 'líneas no válidas (1 a 100000)'
        if [ "$SUBORDEN" = vista ]; then
            hacer_vista "$2" "$3" "$4"
        else
            hacer_historial "$2" "$3" "$4"
        fi
        ;;
    renombrar:2)
        nombre_valido "${2:-}" || uso
        nombre_valido "${3:-}" || uso
        hacer_renombrar "$2" "$3"
        ;;
    terminar:2)
        servidor_valido "${2:-}" || uso
        nombre_valido "${3:-}" || uso
        hacer_terminar "$2" "$3"
        ;;
    *)
        uso
        ;;
esac

#MAGI-FIN
