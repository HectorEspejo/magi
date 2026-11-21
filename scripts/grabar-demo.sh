#!/usr/bin/env bash
# Graba assets/demo/magi.gif (la animación del README) con VHS.
#
# Arranca el servidor de pruebas del repositorio (crates/magi-movil, ejemplo
# servidor_pruebas) en 127.0.0.1 y prepara un entorno aislado y desechable
# (HOME, XDG_*, runtime, clave y huella del host de pruebas) donde MAGI importa
# assets/demo/inventario. Nunca toca tus ficheros ni tu servidor de MAGI, y
# nunca se graba contra hosts reales. Se ejecuta a mano, en local: necesita vhs,
# ttyd y ffmpeg (no corre en CI).
#
# Uso: scripts/grabar-demo.sh
set -euo pipefail

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
limite_bytes=$((2 * 1024 * 1024))

faltan=()
for herramienta in vhs ttyd ffmpeg cargo ssh-keygen; do
    command -v "$herramienta" >/dev/null 2>&1 || faltan+=("$herramienta")
done
if ((${#faltan[@]} > 0)); then
    echo "error: faltan herramientas: ${faltan[*]}" >&2
    echo "       en Arch: sudo pacman -S vhs ttyd ffmpeg" >&2
    exit 1
fi

# El servidor de pruebas usa puertos fijos: avisa si ya hay algo escuchando.
for puerto in 12222 12223 19091; do
    if (exec 3<>"/dev/tcp/127.0.0.1/$puerto") 2>/dev/null; then
        echo "error: el puerto $puerto de 127.0.0.1 ya está en uso (¿otro servidor_pruebas?)" >&2
        exit 1
    fi
done

echo "==> Compilando MAGI y el servidor de pruebas"
(cd "$raiz" && cargo build --locked -p magi-ssh --bin magi && cargo build --locked -p magi-movil --example servidor_pruebas)
objetivo="${CARGO_TARGET_DIR:-$raiz/target}/debug"

# Directorio corto: el socket de Unix no puede pasar de ~108 bytes.
destino="$(mktemp -d /tmp/magi-demo.XXXXXX)"
servidor_pid=""
aislado=0
limpiar() {
    # Para el servidor de MAGI solo si el entorno aislado ya estaba en marcha y
    # su XDG_RUNTIME_DIR es el de este directorio: nunca el tuyo. Con `--si` y
    # sin leer del terminal: sin ellos preguntaría «¿Cerrarlas?» a una entrada
    # que nadie contesta.
    if [ "$aislado" = 1 ] && [ "${XDG_RUNTIME_DIR:-}" = "$destino/run" ]; then
        if command -v timeout >/dev/null 2>&1; then
            timeout 20 "$objetivo/magi" servidor parar --si </dev/null >/dev/null 2>&1 || true
        else
            "$objetivo/magi" servidor parar --si </dev/null >/dev/null 2>&1 || true
        fi
    fi
    if [ -n "$servidor_pid" ]; then
        kill "$servidor_pid" 2>/dev/null || true
        wait "$servidor_pid" 2>/dev/null || true
    fi
    rm -rf "$destino"
}
trap limpiar EXIT

mkdir -p "$destino/casa/.ssh" "$destino/run" "$destino/trabajo"
chmod 700 "$destino/run" "$destino/casa/.ssh"
export HOME="$destino/casa"
export XDG_DATA_HOME="$destino/datos"
export XDG_CONFIG_HOME="$destino/config"
export XDG_STATE_HOME="$destino/estado"
export XDG_RUNTIME_DIR="$destino/run"
export PATH="$objetivo:$PATH"
export TERM="${TERM:-xterm-256color}"
aislado=1

echo "==> Arrancando el servidor de pruebas (127.0.0.1)"
(cd "$destino/trabajo" && exec "$objetivo/examples/servidor_pruebas") >"$destino/servidor.log" 2>&1 &
servidor_pid=$!
for _ in $(seq 1 60); do
    grep -q '^listo:' "$destino/servidor.log" 2>/dev/null && break
    kill -0 "$servidor_pid" 2>/dev/null || { cat "$destino/servidor.log" >&2; exit 1; }
    sleep 0.5
done
grep -q '^listo:' "$destino/servidor.log" || { echo "error: el servidor de pruebas no arrancó" >&2; exit 1; }

# Clave del usuario de la demo (sin frase, solo para este entorno) y huella del
# host de pruebas: es la clave pública fija de crates/magi-movil/tests/comun/
# servidor_pruebas.rs (solo pruebas), para que no salga el diálogo de huella.
ssh-keygen -q -t ed25519 -N '' -C 'demo' -f "$HOME/.ssh/id_demo"
echo '[127.0.0.1]:12222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMDUg4hwJY3WuFnQXmPzlo2c28c7JgaJ2zkxyf8OGHJs' >"$HOME/.ssh/known_hosts"
chmod 600 "$HOME/.ssh/known_hosts"

# Tema fijo: el aspecto de la animación no depende de la máquina que graba (sin
# Omarchy, el tema «auto» no encuentra su paleta y avisa en la barra de estado).
mkdir -p "$XDG_CONFIG_HOME/magi"
printf 'tema = "fijo"\n' >"$XDG_CONFIG_HOME/magi/config.toml"
chmod 600 "$XDG_CONFIG_HOME/magi/config.toml"

echo "==> Importando el inventario de ejemplo"
magi importar "$raiz/assets/demo/inventario"

echo "==> Comprobando la cinta"
(cd "$raiz" && vhs validate assets/demo/magi.tape)

echo "==> Grabando con VHS"
(cd "$raiz" && vhs assets/demo/magi.tape)

gif="$raiz/assets/demo/magi.gif"
tamano="$(stat -c %s "$gif" 2>/dev/null || stat -f %z "$gif")"
echo "==> assets/demo/magi.gif: $tamano bytes"
if [ "$tamano" -gt "$limite_bytes" ]; then
    echo "error: pesa más de 2 MiB; recorta la cinta (menos pasos, menos tamaño o menos color)" >&2
    exit 1
fi
echo "Revisa los fotogramas antes de subirlo: no debe verse nada real (nombres, rutas, usuarios, cifras de tu equipo)."
