# shellcheck shell=bash
# shellcheck disable=SC2034
#
# Receta de TUR (Termux User Repository) para MAGI — Fase 17.
#
# Qué es esto: la receta que el desarrollador propone al repositorio de TUR
# (https://github.com/termux-user-repository/tur) con una PR. Ahí vive en
# tur/magi-ssh/build.sh; aquí se mantiene como packaging/termux/build.sh. El
# agente no abre esa PR: docs/publicar-termux.md explica cómo proponerla y qué
# cambiar en cada versión.
#
# Es un fichero que Termux lee con `source` desde su sistema de compilación
# (termux-packages), no un script que se ejecute. Por eso no lleva shebang y
# tiene un `disable=SC2034` de ShellCheck para todo el fichero: las variables
# TERMUX_PKG_* se declaran para que las consuma ese sistema, no este fichero, y
# el analizador no puede saberlo. Este fichero se pasa por ShellCheck sin avisos
# (el comando está en CLAUDE.md y en docs/publicar-termux.md).
#
# Compila desde el tarball del tag, con termux_setup_rust, solo el paquete
# magi-ssh del workspace, e instala lo mismo que el .deb de
# scripts/paquete-termux.sh: el binario, la página man, el autocompletado de
# bash, zsh y fish, y el README (las licencias las instala Termux por
# TERMUX_PKG_LICENSE_FILE, en share/doc/magi-ssh/).

TERMUX_PKG_HOMEPAGE=https://github.com/HectorEspejo/magi
TERMUX_PKG_DESCRIPTION="Gestor SSH de terminal con estética de cabina técnica"
TERMUX_PKG_LICENSE="Apache-2.0, MIT"
TERMUX_PKG_LICENSE_FILE="LICENSE-APACHE, LICENSE-MIT"
TERMUX_PKG_MAINTAINER="@HectorEspejo"

# CAMBIAR EN CADA VERSIÓN (docs/publicar-termux.md, «Receta de TUR»): la versión
# y el SHA-256 del tarball de ese tag. No se activa TERMUX_PKG_AUTO_UPDATE: cada
# versión se revisa a mano antes de proponerla.
TERMUX_PKG_VERSION="1.6.0"
TERMUX_PKG_SRCURL="https://github.com/HectorEspejo/magi/archive/refs/tags/v${TERMUX_PKG_VERSION}.tar.gz"
# MARCADOR: sin sustituir (64 ceros), la compilación falla por la suma. Se
# calcula con: curl -fsSL "$SRCURL" | sha256sum
TERMUX_PKG_SHA256=0000000000000000000000000000000000000000000000000000000000000000

# Se compila en el árbol de fuentes (Cargo deja ahí su directorio target).
TERMUX_PKG_BUILD_IN_SRC=true

# MAGI en Termux es solo aarch64 (decisión D373 de la Fase 17): arm e i686 no se
# compilan. x86_64 (el emulador) sí compila con el NDK, pero no se ha probado en
# TUR: si su compilación diera problemas, se excluye con la línea de abajo, que
# queda comentada a propósito.
TERMUX_PKG_EXCLUDED_ARCHES="arm, i686"
# TERMUX_PKG_EXCLUDED_ARCHES="arm, i686, x86_64"

# El .deb de GitHub no depende de nada: el binario solo pide bibliotecas del NDK
# estable (libc, libm, libdl, liblog y libz). Aquí, en cambio, el enlazador de
# Termux puede resolver libz contra el paquete zlib y dejar la biblioteca con
# otro nombre (libz.so.1): zlib siempre está instalado (lo necesitan dpkg y apt),
# así que declararlo no cuesta nada. Al proponer la receta, comprobar con
# `readelf -d` el .deb que genera TUR y ajustar esta línea a lo que pida.
TERMUX_PKG_DEPENDS="zlib"
# El portapapeles y los avisos de Android (Termux:API) y el agente SSH (openssh)
# son opcionales.
TERMUX_PKG_SUGGESTS="termux-api, openssh"

termux_step_pre_configure() {
	termux_setup_rust

	# Descarga las dependencias con el Cargo.lock del tag, para que la
	# compilación no cambie ninguna versión.
	cargo fetch --locked --target "${CARGO_TARGET_NAME}"
}

termux_step_make() {
	termux_setup_rust

	# Solo el paquete magi-ssh (el workspace trae también magi-nucleo, que es una
	# dependencia suya, y magi-movil, que no se compila aquí).
	cargo build \
		--jobs "${TERMUX_PKG_MAKE_PROCESSES}" \
		--target "${CARGO_TARGET_NAME}" \
		--release \
		--locked \
		-p magi-ssh
}

termux_step_make_install() {
	# El comando se llama magi (el paquete, magi-ssh). 700 como los paquetes de
	# Termux.
	install -Dm700 "target/${CARGO_TARGET_NAME}/release/magi" \
		"${TERMUX_PREFIX}/bin/magi"

	# Página man y autocompletado: los de assets/, que versiona el repositorio.
	install -Dm644 assets/man/magi.1 \
		"${TERMUX_PREFIX}/share/man/man1/magi.1"
	install -Dm644 assets/completions/magi.bash \
		"${TERMUX_PREFIX}/share/bash-completion/completions/magi"
	install -Dm644 assets/completions/_magi \
		"${TERMUX_PREFIX}/share/zsh/site-functions/_magi"
	install -Dm644 assets/completions/magi.fish \
		"${TERMUX_PREFIX}/share/fish/vendor_completions.d/magi.fish"

	install -Dm644 README.md \
		"${TERMUX_PREFIX}/share/doc/magi-ssh/README.md"
}
