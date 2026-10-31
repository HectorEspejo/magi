#!/bin/sh
# instalar_clave.sh: recibe la línea pública como $1, ya validada en el núcleo
set -eu
clave="$1"
tipo_y_cuerpo=$(printf '%s' "$clave" | cut -d' ' -f1,2)
umask 077
dir="$HOME/.ssh"
fichero="$dir/authorized_keys"
mkdir -p "$dir" && chmod 700 "$dir"
[ -e "$fichero" ] || : > "$fichero"
chmod 600 "$fichero"
if grep -qF -- "$tipo_y_cuerpo" "$fichero"; then
    echo YA_ESTABA
else
    if [ -s "$fichero" ] && [ -n "$(tail -c 1 "$fichero")" ]; then
        printf '\n' >> "$fichero"            # el fichero no acababa en salto de línea
    fi
    printf '%s\n' "$clave" >> "$fichero"
    echo INSTALADA
fi
if command -v restorecon >/dev/null 2>&1; then restorecon -F "$dir" "$fichero" 2>/dev/null || true; fi
