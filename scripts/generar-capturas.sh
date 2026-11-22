#!/usr/bin/env bash
# Regenera las capturas de la web (web/capturas/): las seis vistas de la TUI (Flota,
# sesión con paneles, archivos, túneles, deliberación MAGI y servicios) pintadas con
# App::de_prueba sobre un TestBackend de 100x30, con el tema fijo y el inventario de
# la demo (assets/demo/inventario), y exportadas a HTML con clases (un fichero por
# vista y capturas.css).
#
# Lanza la prueba ignorada `generar_capturas` de tests/capturas_web.rs y, después, la
# normal (`las_capturas_estan_al_dia`) para comprobar lo que ha escrito. No usa hosts
# reales, ni la semilla de las pruebas, ni tu servidor de MAGI: todo va en un
# directorio temporal y solo se escribe en web/capturas/ y en la carpeta fija
# /tmp/magi-capturas-web, que el panel local de Archivos lista y que la prueba
# prepara sin borrar nada. No hace falta ninguna herramienta aparte de cargo.
#
# Cuando cambias una vista que sale en la web, la prueba normal falla con «captura
# desfasada»: ejecuta este script, revisa el diff y súbelo en el mismo PR.
#
# Uso: scripts/generar-capturas.sh [-h|--ayuda]
set -euo pipefail

uso() {
    cat <<'AYUDA'
Uso: scripts/generar-capturas.sh [-h|--ayuda]

Regenera web/capturas/ (seis capturas HTML y capturas.css) desde el código de
pintado de la TUI y comprueba que quedan al día.

  -h, --ayuda   muestra esta ayuda y sale
AYUDA
}

case "${1:-}" in
    -h | --ayuda)
        uso
        exit 0
        ;;
    "") ;;
    *)
        echo "error: argumento desconocido: $1" >&2
        uso >&2
        exit 2
        ;;
esac

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: falta cargo" >&2
    exit 1
fi

cd "$raiz"

echo "==> Pintando las vistas y escribiendo web/capturas/"
TZ=UTC cargo test --locked -p magi-ssh --test capturas_web -- --ignored --exact generar_capturas --nocapture

echo "==> Comprobando que lo escrito está al día"
TZ=UTC cargo test --locked -p magi-ssh --test capturas_web

echo "==> Cambios en web/capturas/"
if command -v git >/dev/null 2>&1 && git -C "$raiz" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$raiz" status --short web/capturas
    echo "Revisa el diff antes de subirlo: git diff web/capturas"
else
    echo "(no es un repositorio de git: nada que mostrar)"
fi
