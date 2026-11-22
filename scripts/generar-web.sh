#!/usr/bin/env bash
# Construye la web de MAGI en local, en target/web/ (salida: target/web/sitio).
#
# Hace lo mismo que .github/workflows/web.yml (informe de la Fase 9c, §5.3), sin
# desplegar: compila el generador (magi-web), lee las releases de la API de
# GitHub y sus .sha256, prepara el manual, construye el sitio y lo comprueba.
# Si algo falla, termina con un mensaje que dice qué y dónde, y no deja un sitio
# a medias donde estaba el anterior (magi-web sustituye la carpeta de salida
# entera solo al construir).
#
# mdBook va fijado por versión y SHA-256 (los mismos valores que web.yml y el job
# web de ci.yml; una prueba de crates/magi-web comprueba que coinciden): se usa
# el de MAGI_WEB_MDBOOK, si está definida; si no, el `mdbook` del PATH si es
# exactamente esa versión; y si no, se descarga a
# ${XDG_CACHE_HOME:-$HOME/.cache}/magi/mdbook-<versión>/ y se comprueba su suma.
#
# Uso:
#   scripts/generar-web.sh [--sin-red] [--con-manual-de-la-rama]
#
# Variables de entorno:
#   MAGI_WEB_MDBOOK     ruta de un mdbook que usar en vez del fijado
#   PUBLICAR_HOMEBREW   «true» enseña el bloque de Homebrew en descargas
#   PUBLICAR_AUR        «true» enseña el bloque de AUR en descargas
#   GH_TOKEN            token de la API de GitHub (en local vale la sesión de gh)
#
# Códigos de salida: 0 bien, 1 fallo (falta una herramienta, dato raro, la
# construcción o la comprobación), 2 uso incorrecto.
set -euo pipefail

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# mdBook fijado. SHA-256 del tarball musl (estático) de esa versión: mdBook no
# publica un .sha256, y la suma coincide con el digest de la API de GitHub.
MDBOOK_VERSION=0.5.4
MDBOOK_SHA256=5222beabd3e37dc5be0d18ff99b79058469354db5c220153a1b92db5ba12be89

trabajo="$raiz/target/web"
sitio="$trabajo/sitio"
releases_de_ejemplo="$raiz/crates/magi-web/tests/fijos/releases/sin-red.json"

sin_red=0
manual_de_la_rama=0
manual_dir=""
mdbook=""
temporal=""

uso() {
	cat <<'FIN'
Construye la web de MAGI en local, en target/web/ (salida: target/web/sitio).

Hace lo mismo que .github/workflows/web.yml, sin desplegar: compila magi-web,
lee las releases de la API de GitHub (y sus .sha256), prepara el manual,
construye el sitio y lo comprueba con `magi-web comprobar`.

Uso:
  scripts/generar-web.sh [--sin-red] [--con-manual-de-la-rama]

Opciones:
  --sin-red                 no llama a GitHub: usa las releases de ejemplo de
                            crates/magi-web/tests/fijos/releases/sin-red.json
  --con-manual-de-la-rama   el manual sale de docs/manual/ del árbol de trabajo
                            y no de la etiqueta de la última versión publicada
  -h, --ayuda               muestra esta ayuda

mdBook va fijado por versión y SHA-256: se usa el de MAGI_WEB_MDBOOK, el del PATH
si es exactamente la versión fijada, o se descarga a
${XDG_CACHE_HOME:-$HOME/.cache}/magi/mdbook-<versión>/ comprobando su suma.

PUBLICAR_HOMEBREW=true y PUBLICAR_AUR=true encienden los bloques de Homebrew y
AUR en descargas; cualquier otro valor los apaga.

Códigos de salida: 0 bien, 1 fallo, 2 uso incorrecto.
FIN
}

fallar() {
	echo "error: $*" >&2
	exit 1
}

# exigir <orden> <qué hacer si falta>
exigir() {
	command -v "$1" >/dev/null 2>&1 || fallar "falta «$1» en el PATH: $2"
}

# Al salir: borra el temporal de la descarga y reduce el código a 0, 1 o 2.
al_salir() {
	local codigo=$?
	if [[ -n "$temporal" && -d "$temporal" ]]; then
		rm -rf -- "$temporal"
	fi
	if [[ "$codigo" -ne 0 && "$codigo" -ne 2 ]]; then
		exit 1
	fi
	exit "$codigo"
}

# version_de_mdbook <ruta>: escribe la versión que dice el ejecutable (solo si su
# salida es exactamente «mdbook vX.Y.Z»; con cualquier otra cosa, falla).
version_de_mdbook() {
	local salida
	local patron='^mdbook v?([0-9]+\.[0-9]+\.[0-9]+)$'
	salida="$("$1" --version 2>/dev/null)" || return 1
	if [[ "$salida" =~ $patron ]]; then
		printf '%s\n' "${BASH_REMATCH[1]}"
	else
		return 1
	fi
}

# Deja en $mdbook la ruta de un mdbook utilizable (se llama sin sustitución de
# comandos: `fallar` sale del script, no de un subshell).
obtener_mdbook() {
	local encontrada ruta cache archivo url

	if [[ -n "${MAGI_WEB_MDBOOK:-}" ]]; then
		[[ -f "$MAGI_WEB_MDBOOK" && -x "$MAGI_WEB_MDBOOK" ]] \
			|| fallar "MAGI_WEB_MDBOOK ($MAGI_WEB_MDBOOK) no es un ejecutable"
		mdbook="$MAGI_WEB_MDBOOK"
		encontrada="$(version_de_mdbook "$mdbook" || true)"
		if [[ "$encontrada" != "$MDBOOK_VERSION" ]]; then
			echo "aviso: MAGI_WEB_MDBOOK es la versión «${encontrada:-desconocida}», no la fijada ($MDBOOK_VERSION)" >&2
		fi
		return 0
	fi

	if ruta="$(command -v mdbook 2>/dev/null)"; then
		encontrada="$(version_de_mdbook "$ruta" || true)"
		if [[ "$encontrada" == "$MDBOOK_VERSION" ]]; then
			mdbook="$ruta"
			return 0
		fi
		echo "aviso: el mdbook del PATH es la versión «${encontrada:-desconocida}», no la fijada ($MDBOOK_VERSION); se usa el de la caché" >&2
	fi

	cache="${XDG_CACHE_HOME:-${HOME:?HOME sin definir: no hay dónde guardar mdBook}/.cache}/magi/mdbook-$MDBOOK_VERSION"
	if [[ -f "$cache/mdbook" && -x "$cache/mdbook" ]]; then
		encontrada="$(version_de_mdbook "$cache/mdbook" || true)"
		if [[ "$encontrada" == "$MDBOOK_VERSION" ]]; then
			mdbook="$cache/mdbook"
			return 0
		fi
		echo "aviso: el mdbook de la caché ($cache/mdbook) no es la versión fijada; se descarga de nuevo" >&2
	fi

	# El SHA-256 fijado es el del tarball de Linux x86_64.
	if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
		fallar "no hay mdBook $MDBOOK_VERSION fijado para $(uname -s) $(uname -m): instálalo (cargo install mdbook --version =$MDBOOK_VERSION --locked) y ponlo en el PATH, o apunta MAGI_WEB_MDBOOK a su ruta"
	fi
	exigir curl "hace falta para descargar mdBook"
	exigir sha256sum "hace falta para comprobar la descarga de mdBook"
	exigir tar "hace falta para extraer mdBook"

	archivo="mdbook-v${MDBOOK_VERSION}-x86_64-unknown-linux-musl.tar.gz"
	url="https://github.com/rust-lang/mdBook/releases/download/v${MDBOOK_VERSION}/${archivo}"
	echo "==> Descargando mdBook $MDBOOK_VERSION a $cache"
	mkdir -p -- "$cache"
	temporal="$(mktemp -d "$cache/.descarga.XXXXXX")"
	# Con plazo: sin red o con GitHub colgado, curl no espera para siempre.
	curl --proto '=https' --tlsv1.2 --connect-timeout 20 --max-time 300 -fsSL -o "$temporal/$archivo" "$url" \
		|| fallar "no se pudo descargar $url (sin red o sin respuesta en 5 min)"
	# Se comprueba el fichero entero antes de abrirlo.
	(cd "$temporal" && echo "$MDBOOK_SHA256  $archivo" | sha256sum -c - >/dev/null 2>&1) \
		|| fallar "la suma SHA-256 de $archivo no coincide con la fijada ($MDBOOK_SHA256): no se instala"
	mkdir -- "$temporal/extraido"
	tar xzf "$temporal/$archivo" -C "$temporal/extraido" \
		|| fallar "no se pudo extraer $archivo"
	[[ -f "$temporal/extraido/mdbook" && ! -L "$temporal/extraido/mdbook" ]] \
		|| fallar "$archivo no trae el ejecutable mdbook"
	encontrada="$(version_de_mdbook "$temporal/extraido/mdbook" || true)"
	[[ "$encontrada" == "$MDBOOK_VERSION" ]] \
		|| fallar "el mdbook descargado dice ser la versión «${encontrada:-desconocida}», no la fijada ($MDBOOK_VERSION)"
	install -m 755 -- "$temporal/extraido/mdbook" "$cache/mdbook.nuevo"
	mv -f -- "$cache/mdbook.nuevo" "$cache/mdbook"
	mdbook="$cache/mdbook"
}

# Prepara el manual para construirlo con mdBook y deja en $manual_dir la carpeta
# ya generada (vacía mientras no esté hecho).
preparar_manual() {
	obtener_mdbook
	echo "==> mdBook $MDBOOK_VERSION: $mdbook"
	# CONTRATO-S3-MANUAL: el Sprint 3 completa esta función con `magi-web
	# preparar-manual --etiqueta <última versión publicada> --salida <dir>` (o,
	# con --con-manual-de-la-rama, desde docs/manual/ del árbol de trabajo),
	# `mdbook build` y, al final, `manual_dir="$trabajo/manual"`. También decide
	# de dónde sale el manual con --sin-red.
	local origen="la última etiqueta publicada"
	if [[ "$manual_de_la_rama" -eq 1 ]]; then
		origen="docs/manual/ del árbol de trabajo"
	fi
	echo "manual: pendiente de la 9c S3 (origen previsto: $origen)"
}

principal() {
	local generador releases
	local -a argumentos

	cd "$raiz"
	exigir cargo "instala Rust (https://rustup.rs)"
	[[ -d web && -d crates/magi-web ]] \
		|| fallar "no encuentro web/ ni crates/magi-web/ en $raiz"
	mkdir -p -- "$trabajo"

	echo "==> Compilando magi-web"
	cargo build --release --locked -p magi-web \
		|| fallar "la compilación de magi-web falló"
	generador="${CARGO_TARGET_DIR:-$raiz/target}/release/magi-web"
	[[ -x "$generador" ]] || fallar "no encuentro el binario compilado: $generador"

	releases="$trabajo/releases.json"
	if [[ "$sin_red" -eq 1 ]]; then
		echo "==> Releases de ejemplo (--sin-red)"
		[[ -f "$releases_de_ejemplo" ]] \
			|| fallar "falta el fichero de ejemplo $releases_de_ejemplo"
		cp -- "$releases_de_ejemplo" "$releases"
	else
		echo "==> Leyendo las releases de GitHub"
		exigir gh "hace falta para leer la API de GitHub (o usa --sin-red)"
		"$generador" releases --salida "$releases" \
			|| fallar "no se pudieron leer o validar las releases (la web anterior no cambia)"
	fi

	preparar_manual

	echo "==> Construyendo el sitio en $sitio"
	argumentos=(construir --web web --releases "$releases" --animacion assets/demo/magi.gif --salida "$sitio")
	if [[ -n "$manual_dir" ]]; then
		argumentos+=(--manual "$manual_dir")
	fi
	# Solo el valor exacto «true» enciende un bloque; cualquier otro lo apaga.
	if [[ "${PUBLICAR_HOMEBREW:-}" == "true" ]]; then
		argumentos+=(--homebrew)
	fi
	if [[ "${PUBLICAR_AUR:-}" == "true" ]]; then
		argumentos+=(--aur)
	fi
	"$generador" "${argumentos[@]}" \
		|| fallar "la construcción del sitio falló"

	echo "==> Comprobando el sitio"
	"$generador" comprobar "$sitio" \
		|| fallar "la comprobación del sitio falló"

	echo "web lista en $sitio"
}

analizar_argumentos() {
	local argumento
	for argumento in "$@"; do
		case "$argumento" in
			--sin-red) sin_red=1 ;;
			--con-manual-de-la-rama) manual_de_la_rama=1 ;;
			-h | --ayuda)
				uso
				exit 0
				;;
			*)
				echo "opción desconocida: $argumento (usa --ayuda)" >&2
				exit 2
				;;
		esac
	done
}

# Solo se ejecuta si se llama al script, no si se carga con `source`.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
	trap al_salir EXIT
	analizar_argumentos "$@"
	principal
fi
