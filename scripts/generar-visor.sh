#!/usr/bin/env bash
# Genera visor/dist/ con esbuild, o comprueba que está al día.
#
# MAGI compila sin Node: visor/dist/ va versionado y el binario lo incrusta con
# include_bytes!/include_str!. Este script solo hace falta para el visor.
#
# Uso:
#   scripts/generar-visor.sh              regenera visor/dist/
#   scripts/generar-visor.sh --comprobar  reconstruye en un temporal y falla si
#                                         visor/dist/ no coincide byte a byte
set -euo pipefail

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
visor="$raiz/visor"
dist="$visor/dist"
esbuild="$visor/node_modules/.bin/esbuild"

uso() {
	cat <<'FIN'
Genera visor/dist/ con esbuild, o comprueba que está al día.

MAGI compila sin Node: visor/dist/ va versionado y el binario lo incrusta.
Este script solo hace falta para el visor.

Uso:
  scripts/generar-visor.sh              regenera visor/dist/
  scripts/generar-visor.sh --comprobar  reconstruye en un temporal y falla si
                                        visor/dist/ no coincide byte a byte
FIN
}

comprobar=0
for argumento in "$@"; do
	case "$argumento" in
		--comprobar) comprobar=1 ;;
		-h | --ayuda)
			uso
			exit 0
			;;
		*)
			echo "opción desconocida: $argumento" >&2
			exit 2
			;;
	esac
done

if [[ ! -x "$esbuild" ]]; then
	echo "error: falta esbuild en visor/node_modules; ejecuta: npm --prefix visor ci" >&2
	exit 1
fi

# esbuild va fijado por versión exacta en visor/package.json y package-lock.json.
version_fijada="$(sed -n 's/.*"esbuild": "\([^"]*\)".*/\1/p' "$visor/package.json" | head -n 1)"
version_instalada="$("$esbuild" --version)"
if [[ -z "$version_fijada" || "$version_instalada" != "$version_fijada" ]]; then
	echo "error: esbuild instalado ($version_instalada) no es la versión fijada ($version_fijada);" >&2
	echo "       ejecuta: npm --prefix visor ci" >&2
	exit 1
fi

aviso_js='/*! MAGI · visor de escritorio remoto (Fase 12). Adaptado del cliente web de voxl 0.3.3 (MIT): ver visor/LICENSE-voxl y visor/README.md. */'
aviso_css='/*! MAGI · visor de escritorio remoto (Fase 12). Estética de cabina; adaptado del cliente web de voxl 0.3.3 (MIT): ver visor/LICENSE-voxl y visor/README.md. */'

salida="$(mktemp -d "${TMPDIR:-/tmp}/magi-visor.XXXXXX")"
trap 'rm -rf "$salida"' EXIT

construir() {
	local destino="$1"
	"$esbuild" "$visor/src/cliente/arranque.ts" \
		--bundle \
		--format=iife \
		--target=es2022 \
		--minify \
		--charset=utf8 \
		--log-level=warning \
		--banner:js="$aviso_js" \
		--outfile="$destino/visor.js"
	"$esbuild" "$visor/src/trabajador.ts" \
		--bundle \
		--format=iife \
		--target=es2022 \
		--minify \
		--charset=utf8 \
		--log-level=warning \
		--banner:js="$aviso_js" \
		--outfile="$destino/trabajador.js"
	"$esbuild" "$visor/src/visor.css" \
		--bundle \
		--minify \
		--charset=utf8 \
		--log-level=warning \
		--banner:css="$aviso_css" \
		--outfile="$destino/visor.css"
}

construir "$salida"

if [[ "$comprobar" -eq 0 ]]; then
	mkdir -p "$dist"
	cp "$salida/visor.js" "$dist/visor.js"
	cp "$salida/trabajador.js" "$dist/trabajador.js"
	cp "$salida/visor.css" "$dist/visor.css"
	echo "generado visor/dist/:"
	wc -c "$dist/visor.js" "$dist/trabajador.js" "$dist/visor.css"
	exit 0
fi

# --comprobar: mismos ficheros, mismos bytes, sin sobras.
fallo=0
for fichero in visor.js trabajador.js visor.css; do
	if [[ ! -f "$dist/$fichero" ]]; then
		echo "falta visor/dist/$fichero: ejecuta scripts/generar-visor.sh" >&2
		fallo=1
	elif ! cmp -s "$salida/$fichero" "$dist/$fichero"; then
		echo "visor/dist/$fichero no está al día: ejecuta scripts/generar-visor.sh" >&2
		fallo=1
	fi
done
if [[ -d "$dist" ]]; then
	for fichero in "$dist"/*; do
		[[ -e "$fichero" ]] || continue
		case "$(basename "$fichero")" in
			visor.js | trabajador.js | visor.css) ;;
			*)
				echo "visor/dist/$(basename "$fichero") no lo genera este script" >&2
				fallo=1
				;;
		esac
	done
fi
if [[ "$fallo" -ne 0 ]]; then
	echo "visor/dist no coincide con las fuentes (esbuild $version_instalada)" >&2
	exit 1
fi
echo "visor/dist al día (esbuild $version_instalada)"
