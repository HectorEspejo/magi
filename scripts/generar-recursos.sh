#!/usr/bin/env bash
# Regenera los recursos de instalación de MAGI a partir del propio binario:
#   assets/completions/magi.bash   autocompletado de bash
#   assets/completions/_magi       autocompletado de zsh
#   assets/completions/magi.fish   autocompletado de fish
#   assets/man/magi.1              página man
#
# El .desktop y los iconos son estáticos: no se generan aquí. El test
# tests/recursos.rs falla si estos ficheros no coinciden con la salida del
# binario; si es el caso, ejecuta este script y revisa el diff.
#
# Uso: scripts/generar-recursos.sh
set -euo pipefail

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$raiz"

# El binario se compila con el Cargo.lock fijado (releases reproducibles).
cargo build --locked --bin magi
binario="${CARGO_TARGET_DIR:-target}/debug/magi"

mkdir -p assets/completions assets/man
"$binario" autocompletado bash > assets/completions/magi.bash
"$binario" autocompletado zsh > assets/completions/_magi
"$binario" autocompletado fish > assets/completions/magi.fish
"$binario" man > assets/man/magi.1

echo "Recursos regenerados en assets/."
