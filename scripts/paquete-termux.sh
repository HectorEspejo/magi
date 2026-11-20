#!/usr/bin/env bash
# paquete-termux.sh — arma el .deb de MAGI para Termux (Fase 17).
#
# Uso:
#   scripts/paquete-termux.sh <versión> [binario]
#   scripts/paquete-termux.sh --comprobar-binario [binario]
#   scripts/paquete-termux.sh -h | --help
#
# Con --comprobar-binario solo hace el paso 1 (no necesita dpkg-deb): es lo que
# usan termux-release.yml y ci.yml para fallar pronto.
#
# <versión> es X.Y.Z o X.Y.Z-rc.N, sin la «v» del tag. Una candidata produce
# «Version: X.Y.Z~rc.N» en el control (la tilde ordena antes que X.Y.Z, así que
# la final actualiza a la candidata; con guion ordenaría después), pero el
# NOMBRE DEL FICHERO lleva el guion (magi-ssh_X.Y.Z-rc.N_aarch64.deb): GitHub
# puede cambiar la tilde del nombre de un adjunto por un punto y entonces el
# .sha256, que nombra el fichero, ya no cuadra. Una versión estable no cambia.
# Por eso el nombre no sigue la convención de Debian (nombre_versión_arquitectura)
# en las candidatas; dpkg y apt no miran el nombre del fichero.
#
# El binario es el de `cargo ndk -t arm64-v8a --platform 24 build --release -p
# magi-ssh --locked` (por defecto, target/aarch64-linux-android/release/magi, o
# el que diga MAGI_BINARIO_TERMUX, o el segundo argumento). Nunca se ejecuta:
# se mira con readelf.
#
# Qué hace, en este orden:
#   1. Comprueba que el binario es un ELF aarch64 y que solo pide bibliotecas
#      del NDK estable: libc, libm, libdl, liblog y libz (desviación 1 de la
#      Fase 17). Con libc++_shared.so el paquete tendría que depender de libc++:
#      falla y lo dice.
#   2. Monta el árbol del paquete bajo /data/data/com.termux/files/usr/ con
#      exactamente lo del informe (§5.1): bin/magi (700), la página man (gzip),
#      el autocompletado de bash, zsh y fish, y las licencias y el README.
#   3. Arma magi-ssh_<versión>_aarch64.deb (con la versión tal cual, sin la tilde)
#      con `dpkg-deb -Zxz --root-owner-group` (xz, que el dpkg de Termux lee
#      siempre).
#   4. Comprueba lo armado (dpkg-deb --contents exacto y el control: arquitectura,
#      sugerencias, ningún Depends) y escribe el .sha256 junto al .deb.
#
# Salida: MAGI_DEB_SALIDA (por defecto, target/termux). Reproducible: las marcas
# de tiempo salen de SOURCE_DATE_EPOCH o, si no está, de la fecha del último
# commit. Necesita dpkg-deb, readelf, gzip, sha256sum, install, find, touch y
# awk (Linux; en Arch y Omarchy, `sudo pacman -S dpkg`).
#
# Códigos de salida: 0 bien · 1 fallo · 2 uso incorrecto.

set -euo pipefail

# Todo lo que se ordena o se compara va en el locale C: mismo resultado en el
# portátil y en CI.
export LC_ALL=C
umask 022

aqui="${BASH_SOURCE[0]%/*}"
[ "$aqui" != "${BASH_SOURCE[0]}" ] || aqui=.
raiz="$(cd -- "$aqui/.." && pwd)"

# Ruta del paquete, sin la barra inicial (así es como la lista dpkg-deb).
PREFIJO_TERMUX='data/data/com.termux/files/usr'
# Lo único que puede pedir el binario (desviación 1 de la Fase 17): bibliotecas
# públicas del NDK, presentes en toda versión de Android. Con otra, el paquete
# tendría que declarar una dependencia.
BIBLIOTECAS_PERMITIDAS=(libc.so libm.so libdl.so liblog.so libz.so)
HOMEPAGE='https://github.com/HectorEspejo/magi'

error() {
  printf 'paquete-termux.sh: %s\n' "$*" >&2
  exit 1
}

uso_incorrecto() {
  printf 'paquete-termux.sh: %s\n' "$*" >&2
  printf 'Uso: scripts/paquete-termux.sh <versión> [binario]  (usa --help)\n' >&2
  exit 2
}

ayuda() {
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
}

# Comprueba que existen las herramientas pedidas y, si faltan, las nombra todas.
exigir_herramientas() {
  local faltan=() herramienta
  for herramienta in "$@"; do
    command -v "$herramienta" > /dev/null 2>&1 || faltan+=("$herramienta")
  done
  if [ "${#faltan[@]}" -gt 0 ]; then
    error "faltan herramientas en el PATH: ${faltan[*]}"
  fi
}

# --- El binario ---------------------------------------------------------------

# Comprueba que $1 es un ELF aarch64 de Android que solo pide bibliotecas del
# NDK estable. No ejecuta nada del binario.
comprobar_binario() {
  local binario="$1" cabecera dinamica biblioteca candidata permitida explicacion
  local pedidas=() fuera=() hay_libc=0

  if [ ! -e "$binario" ]; then
    error "no existe el binario «$binario»; compílalo con: cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked"
  fi
  [ -f "$binario" ] || error "«$binario» no es un fichero normal"
  [ -r "$binario" ] || error "no se puede leer «$binario»"
  [ -s "$binario" ] || error "«$binario» está vacío"

  cabecera="$(readelf -h "$binario" 2>&1)" \
    || error "readelf no puede leer «$binario»: no es un ELF"
  printf '%s\n' "$cabecera" | grep -Eq '^ *Class: +ELF64$' \
    || error "«$binario» no es un ELF de 64 bits"
  printf '%s\n' "$cabecera" | grep -Eq '^ *Machine: +AArch64$' \
    || error "«$binario» no es aarch64 (el paquete es solo para aarch64-linux-android)"
  printf '%s\n' "$cabecera" | grep -Eq '^ *Type: +DYN' \
    || error "«$binario» no es un ejecutable PIE (Android lo exige)"

  dinamica="$(readelf -d "$binario" 2>&1)" \
    || error "readelf -d no puede leer la sección dinámica de «$binario»"
  while IFS= read -r biblioteca; do
    [ -n "$biblioteca" ] || continue
    pedidas+=("$biblioteca")
  done < <(printf '%s\n' "$dinamica" | sed -n 's/.*(NEEDED).*Shared library: \[\(.*\)\]$/\1/p')

  for biblioteca in "${pedidas[@]+"${pedidas[@]}"}"; do
    permitida=0
    for candidata in "${BIBLIOTECAS_PERMITIDAS[@]}"; do
      if [ "$biblioteca" = "$candidata" ]; then
        permitida=1
      fi
    done
    if [ "$permitida" -eq 0 ]; then
      fuera+=("$biblioteca")
    fi
    if [ "$biblioteca" = "libc.so" ]; then
      hay_libc=1
    fi
  done

  if [ "${#fuera[@]}" -gt 0 ]; then
    explicacion=''
    for biblioteca in "${fuera[@]}"; do
      if [ "$biblioteca" = "libc++_shared.so" ]; then
        explicacion=" Con libc++_shared.so el paquete tendría que depender de libc++ (Depends: libc++) y dejar su línea en el informe de la fase."
      fi
    done
    error "«$binario» pide bibliotecas fuera de las del NDK estable (${BIBLIOTECAS_PERMITIDAS[*]}): ${fuera[*]}.$explicacion"
  fi
  if [ "$hay_libc" -eq 0 ]; then
    error "«$binario» no pide libc.so: no parece un binario de Android"
  fi

  printf 'Binario correcto: ELF aarch64, pide solo %s\n' "${pedidas[*]}"
}

# --- Argumentos ---------------------------------------------------------------

solo_comprobar=0
case "${1:-}" in
  -h | --help)
    ayuda
    exit 0
    ;;
  --comprobar-binario)
    solo_comprobar=1
    shift
    ;;
esac

binario_por_defecto="${CARGO_TARGET_DIR:-$raiz/target}/aarch64-linux-android/release/magi"

if [ "$solo_comprobar" -eq 1 ]; then
  [ "$#" -le 1 ] || uso_incorrecto "--comprobar-binario admite como mucho un argumento (el binario)"
  exigir_herramientas readelf grep sed
  comprobar_binario "${1:-${MAGI_BINARIO_TERMUX:-$binario_por_defecto}}"
  exit 0
fi

[ "$#" -ge 1 ] || uso_incorrecto "falta la versión"
[ "$#" -le 2 ] || uso_incorrecto "demasiados argumentos"
version="$1"
binario="${2:-${MAGI_BINARIO_TERMUX:-$binario_por_defecto}}"

# X.Y.Z o X.Y.Z-rc.N, sin ceros a la izquierda y sin «v».
version_valida='^(0|[1-9][0-9]{0,5})\.(0|[1-9][0-9]{0,5})\.(0|[1-9][0-9]{0,5})(-rc\.[1-9][0-9]{0,2})?$'
if [[ ! "$version" =~ $version_valida ]]; then
  uso_incorrecto "«$version» no es una versión válida; se esperaba X.Y.Z o X.Y.Z-rc.N, sin la «v» del tag"
fi
# Versión de Debian (la del control): la tilde hace que la candidata ordene
# antes que la final.
tilde='~'
version_deb="${version/-rc./${tilde}rc.}"
# Nombre del fichero: la versión tal cual, con el guion de «-rc.N». No lleva la
# tilde a propósito: GitHub puede cambiarla por un punto en el nombre del
# adjunto y el .sha256 (que nombra el fichero) dejaría de cuadrar.
nombre_deb="magi-ssh_${version}_aarch64.deb"

if ! command -v dpkg-deb > /dev/null 2>&1; then
  error "no se encuentra «dpkg-deb». Instálalo y vuelve a ejecutar el script:
  Arch y Omarchy:  sudo pacman -S dpkg
  Debian y Ubuntu: sudo apt install dpkg
  macOS:           brew install dpkg"
fi
exigir_herramientas readelf gzip sha256sum install find touch awk sort sed grep mktemp chmod mv rm mkdir diff wc cut

salida="${MAGI_DEB_SALIDA:-${CARGO_TARGET_DIR:-$raiz/target}/termux}"

# Las marcas de tiempo salen de SOURCE_DATE_EPOCH o del último commit: dos
# ejecuciones con las mismas entradas dan el mismo .deb.
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
  epoca="$SOURCE_DATE_EPOCH"
  [[ "$epoca" =~ ^[0-9]{1,12}$ ]] || error "SOURCE_DATE_EPOCH no es un número de segundos: «$epoca»"
else
  command -v git > /dev/null 2>&1 \
    || error "no hay SOURCE_DATE_EPOCH ni git para sacar la fecha del último commit; define SOURCE_DATE_EPOCH"
  epoca="$(git -C "$raiz" log -1 --format=%ct 2> /dev/null)" \
    || error "no se pudo leer la fecha del último commit; define SOURCE_DATE_EPOCH"
  [[ "$epoca" =~ ^[0-9]{1,12}$ ]] || error "la fecha del último commit no es válida: «$epoca»; define SOURCE_DATE_EPOCH"
fi
export SOURCE_DATE_EPOCH="$epoca"

# Lo que va en el paquete y de dónde sale (todo versionado en el repositorio).
origen_man="$raiz/assets/man/magi.1"
origen_bash="$raiz/assets/completions/magi.bash"
origen_zsh="$raiz/assets/completions/_magi"
origen_fish="$raiz/assets/completions/magi.fish"
origen_mit="$raiz/LICENSE-MIT"
origen_apache="$raiz/LICENSE-APACHE"
origen_readme="$raiz/README.md"
for origen in "$origen_man" "$origen_bash" "$origen_zsh" "$origen_fish" "$origen_mit" "$origen_apache" "$origen_readme"; do
  [ -f "$origen" ] || error "falta «${origen#"$raiz"/}»; si es de assets/, ejecuta scripts/generar-recursos.sh"
done

comprobar_binario "$binario"

# --- El árbol del paquete -----------------------------------------------------

trabajo="$(mktemp -d "${TMPDIR:-/tmp}/paquete-termux.XXXXXX")"
trap 'rm -rf "$trabajo"' EXIT
pkg="$trabajo/pkg"
mkdir -p "$pkg/DEBIAN" "$pkg/$PREFIJO_TERMUX"

# Cada fichero instalado se anota con el modo y la ruta con que debe salir en
# `dpkg-deb --contents`: es la lista que se compara después con la real.
esperadas=()
registrar() {
  local modo_ls
  case "$1" in
    700) modo_ls='-rwx------' ;;
    644) modo_ls='-rw-r--r--' ;;
    *) error "modo interno no previsto: $1" ;;
  esac
  esperadas+=("$modo_ls ./$PREFIJO_TERMUX/$2")
}
instalar_copia() { # modo origen destino
  install -D -m "$1" "$2" "$pkg/$PREFIJO_TERMUX/$3"
  registrar "$1" "$3"
}

instalar_copia 700 "$binario" bin/magi
# Página man comprimida sin nombre ni fecha (gzip -n): el .deb es reproducible.
mkdir -p "$pkg/$PREFIJO_TERMUX/share/man/man1"
gzip -9 -n -c "$origen_man" > "$pkg/$PREFIJO_TERMUX/share/man/man1/magi.1.gz"
chmod 644 "$pkg/$PREFIJO_TERMUX/share/man/man1/magi.1.gz"
registrar 644 share/man/man1/magi.1.gz
instalar_copia 644 "$origen_bash" share/bash-completion/completions/magi
instalar_copia 644 "$origen_zsh" share/zsh/site-functions/_magi
instalar_copia 644 "$origen_fish" share/fish/vendor_completions.d/magi.fish
instalar_copia 644 "$origen_mit" share/doc/magi-ssh/LICENSE-MIT
instalar_copia 644 "$origen_apache" share/doc/magi-ssh/LICENSE-APACHE
instalar_copia 644 "$origen_readme" share/doc/magi-ssh/README.md

# Los directorios que deben aparecer: la raíz y todos los padres de lo anterior.
directorios=('drwxr-xr-x ./')
for entrada in "${esperadas[@]}"; do
  ruta="${entrada#* ./}"
  while [[ "$ruta" == */* ]]; do
    ruta="${ruta%/*}"
    directorios+=("drwxr-xr-x ./$ruta/")
  done
done

# Instalado: suma de los tamaños de los ficheros, en KiB hacia arriba. Se
# calcula de los ficheros, no de `du`, para que no dependa del sistema de ficheros.
total_bytes=0
while IFS= read -r fichero; do
  tamano="$(wc -c < "$fichero")"
  total_bytes=$((total_bytes + tamano))
done < <(find "$pkg" -path "$pkg/DEBIAN" -prune -o -type f -print | sort)
tamano_kib=$(((total_bytes + 1023) / 1024))

{
  printf 'Package: magi-ssh\n'
  printf 'Version: %s\n' "$version_deb"
  printf 'Architecture: aarch64\n'
  printf 'Maintainer: 4d3\n'
  printf 'Installed-Size: %s\n' "$tamano_kib"
  printf 'Suggests: termux-api, openssh\n'
  printf 'Homepage: %s\n' "$HOMEPAGE"
  printf 'Description: Gestor SSH de terminal con estética de cabina técnica\n'
  printf ' MAGI es un gestor SSH de terminal (TUI) operado por teclado: inventario\n'
  printf ' de hosts, sesiones que sobreviven a la ventana, túneles, archivos por\n'
  printf ' SFTP, servicios y sincronización del inventario entre equipos. El\n'
  printf ' comando es «magi».\n'
  printf ' .\n'
  printf ' Termux:API (el paquete termux-api) añade el portapapeles de Android y\n'
  printf ' los avisos; openssh aporta el agente SSH. Software libre bajo\n'
  printf ' MIT OR Apache-2.0.\n'
} > "$pkg/DEBIAN/control"
chmod 644 "$pkg/DEBIAN/control"

# Directorios en 755 y todas las marcas de tiempo fijas (dpkg-deb exige que
# DEBIAN no pase de 775 y 755 es lo normal).
find "$pkg" -type d -exec chmod 755 {} +
find "$pkg" -exec touch -h -d "@$epoca" {} +

# --- Armar y comprobar --------------------------------------------------------

deb="$trabajo/$nombre_deb"
if ! armado="$(dpkg-deb -Zxz --root-owner-group --build "$pkg" "$deb" 2>&1)"; then
  printf '%s\n' "$armado" >&2
  error "dpkg-deb no pudo armar el paquete"
fi

comprobar_deb() {
  local fichero="$1" contenido real esperado campos campo
  contenido="$(dpkg-deb --contents "$fichero")" \
    || error "dpkg-deb --contents no puede leer el paquete armado"

  # Rutas y modos: exactamente los de §5.1, ni uno más ni uno menos.
  real="$(printf '%s\n' "$contenido" | awk '{ print $1, $6 }' | sort)"
  esperado="$(printf '%s\n' "${esperadas[@]}" "${directorios[@]}" | sort -u)"
  if [ "$real" != "$esperado" ]; then
    diff <(printf '%s\n' "$esperado") <(printf '%s\n' "$real") >&2 || true
    error "el contenido del paquete no es el esperado (arriba, esperado → real)"
  fi
  # Todo de root:root (--root-owner-group).
  if printf '%s\n' "$contenido" | awk '$2 != "root/root" { malo = 1 } END { exit !malo }'; then
    error "el paquete lleva ficheros que no son de root:root"
  fi

  # Control.
  campos="$(dpkg-deb --field "$fichero")" || error "dpkg-deb --field no puede leer el control"
  for campo in \
    'Package: magi-ssh' \
    "Version: $version_deb" \
    'Architecture: aarch64' \
    'Maintainer: 4d3' \
    "Installed-Size: $tamano_kib" \
    'Suggests: termux-api, openssh' \
    "Homepage: $HOMEPAGE"; do
    if ! printf '%s\n' "$campos" | grep -qxF "$campo"; then
      error "el control del paquete no lleva «$campo»"
    fi
  done
  if printf '%s\n' "$campos" | grep -Eq '^(Pre-)?Depends:'; then
    error "el control del paquete lleva Depends: el binario solo debe pedir bibliotecas del NDK estable"
  fi
}
comprobar_deb "$deb"

# --- Salida -------------------------------------------------------------------

mkdir -p "$salida"
mv -f "$deb" "$salida/$nombre_deb"
(
  cd "$salida"
  sha256sum "$nombre_deb" > "$nombre_deb.sha256"
  sha256sum -c "$nombre_deb.sha256" > /dev/null
)
chmod 644 "$salida/$nombre_deb" "$salida/$nombre_deb.sha256"

printf 'Paquete:  %s/%s\n' "$salida" "$nombre_deb"
printf 'SHA-256:  %s\n' "$(cut -d' ' -f1 "$salida/$nombre_deb.sha256")"
printf 'Versión:  %s (Installed-Size: %s KiB)\n' "$version_deb" "$tamano_kib"
