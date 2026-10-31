#!/usr/bin/env bash
# generar.sh — rellena la plantilla del PKGBUILD de magi-ssh-bin (Fase 9).
#
# Uso:
#   generar.sh --version 1.0.0 \
#     --sha256-x86_64-fichero magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256 \
#     --sha256-aarch64-fichero magi-ssh-aarch64-unknown-linux-gnu.tar.xz.sha256
#
#   generar.sh --version 1.0.0 --sha256-x86_64 HASH --sha256-aarch64 HASH \
#     --salida /tmp/magi-ssh-bin/PKGBUILD
#
# Opciones:
#   --version VERSION              versión estable X.Y.Z de la release
#   --sha256-x86_64 HASH           suma SHA-256 del tarball x86_64
#   --sha256-aarch64 HASH          suma SHA-256 del tarball aarch64
#   --sha256-x86_64-fichero RUTA   fichero .sha256 publicado para x86_64
#   --sha256-aarch64-fichero RUTA  fichero .sha256 publicado para aarch64
#   --plantilla RUTA               plantilla (por defecto la de este script)
#   --salida RUTA                  PKGBUILD resultante (por defecto ./PKGBUILD)
#   -h, --help                     muestra esta ayuda
#
# Solo versiones estables: makepkg no admite guiones en pkgver y las
# prereleases (vX.Y.Z-rc.N) no se publican en AUR. No modifica la plantilla
# del repositorio: escribe el PKGBUILD relleno en --salida.

set -euo pipefail

aqui="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
plantilla="$aqui/PKGBUILD"
salida="./PKGBUILD"
version=""
sha_x86_64=""
sha_aarch64=""
fichero_x86_64=""
fichero_aarch64=""

error() {
  printf 'generar.sh: %s\n' "$*" >&2
  exit 1
}

uso() {
  sed -n '2,24p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

mientras_haya_argumentos() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --version) version="${2:-}"; shift 2 ;;
      --sha256-x86_64) sha_x86_64="${2:-}"; shift 2 ;;
      --sha256-aarch64) sha_aarch64="${2:-}"; shift 2 ;;
      --sha256-x86_64-fichero) fichero_x86_64="${2:-}"; shift 2 ;;
      --sha256-aarch64-fichero) fichero_aarch64="${2:-}"; shift 2 ;;
      --plantilla) plantilla="${2:-}"; shift 2 ;;
      --salida) salida="${2:-}"; shift 2 ;;
      -h|--help) uso; exit 0 ;;
      *) error "opción desconocida: $1 (usa --help)" ;;
    esac
  done
}

leer_suma_de_fichero() {
  local ruta="$1"
  [ -f "$ruta" ] || error "no existe el fichero de suma: $ruta"
  awk 'NR == 1 { print $1 }' "$ruta"
}

normalizar_suma() {
  local suma="$1"
  suma="$(printf '%s' "$suma" | tr '[:upper:]' '[:lower:]')"
  [[ "$suma" =~ ^[0-9a-f]{64}$ ]] || error "suma SHA-256 inválida: $suma"
  printf '%s' "$suma"
}

mientras_haya_argumentos "$@"

[ -n "$version" ] || error "falta --version"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
  || error "versión no válida: «$version» (solo estable X.Y.Z; las prereleases no se publican en AUR)"
[ -f "$plantilla" ] || error "no existe la plantilla: $plantilla"

if [ -n "$fichero_x86_64" ]; then
  sha_x86_64="$(leer_suma_de_fichero "$fichero_x86_64")"
fi
if [ -n "$fichero_aarch64" ]; then
  sha_aarch64="$(leer_suma_de_fichero "$fichero_aarch64")"
fi
[ -n "$sha_x86_64" ] || error "falta --sha256-x86_64 o --sha256-x86_64-fichero"
[ -n "$sha_aarch64" ] || error "falta --sha256-aarch64 o --sha256-aarch64-fichero"
sha_x86_64="$(normalizar_suma "$sha_x86_64")"
sha_aarch64="$(normalizar_suma "$sha_aarch64")"

for marcador in '@VERSION@' '@SHA256_X86_64@' '@SHA256_AARCH64@'; do
  grep -q -- "$marcador" "$plantilla" \
    || error "la plantilla no contiene el marcador $marcador: $plantilla"
done

contenido="$(<"$plantilla")"
contenido="${contenido//@VERSION@/$version}"
contenido="${contenido//@SHA256_X86_64@/$sha_x86_64}"
contenido="${contenido//@SHA256_AARCH64@/$sha_aarch64}"

if printf '%s' "$contenido" | grep -qE '@[A-Z0-9_]+@'; then
  error "quedan marcadores sin rellenar en el PKGBUILD resultante"
fi

mkdir -p "$(dirname -- "$salida")"
temporal="${salida}.tmp.$$"
printf '%s\n' "$contenido" > "$temporal"
chmod 644 "$temporal"
mv -f "$temporal" "$salida"

printf 'PKGBUILD generado: %s\n' "$salida"
printf '  pkgver:           %s\n' "$version"
printf '  sha256 x86_64:    %s\n' "$sha_x86_64"
printf '  sha256 aarch64:   %s\n' "$sha_aarch64"
