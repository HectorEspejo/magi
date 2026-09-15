# MAGI

Gestor SSH de terminal (TUI) con estética de cabina técnica, operado por
teclado, para Linux (Omarchy) y macOS. Inventario de hosts en SQLite, ficha
completa, importación sin pérdida de `~/.ssh/config`, exportación a
`~/.ssh/magi_config` y conexión SSH embebida con verificación estricta de
huellas. Sin cuenta, sin suscripción y sin telemetría.

## Requisitos

- Rust stable (edición 2021)
- Linux o macOS
- Opcional: `ssh-agent` en `SSH_AUTH_SOCK` para autenticar sin frase

## Instalación

```sh
cargo install --path .
```

Para desarrollo:

```sh
cargo run                       # abre la TUI en la vista Hosts
cargo run -- importar [ruta]    # importa ~/.ssh/config (o la ruta indicada)
cargo run -- exportar           # regenera ~/.ssh/magi_config
cargo test                      # tests de almacén e ida y vuelta
cargo clippy -- -D warnings
cargo fmt
```

## Primer arranque

MAGI crea su configuración, su base de datos y su log en las rutas XDG
(equivalentes en macOS vía `directories`):

| Qué | Ruta |
|---|---|
| Inventario | `~/.local/share/magi/magi.db` (SQLite en WAL, 600) |
| Configuración | `~/.config/magi/config.toml` |
| Log | `~/.local/state/magi/logs/magi.log.<fecha>` (rotación diaria) |

`config.toml`:

```toml
prefijo_escape = "Ctrl+]"   # prefijo de la vista Sesión
exportar_al_guardar = true  # regenera magi_config al guardar una ficha
tema = "auto"               # auto | fijo | /ruta/a/alacritty.toml
terminal_ascii = false      # fuerza el modo degradado ASCII
```

Con `tema = "auto"` se leen los colores de
`~/.config/omarchy/current/theme/alacritty.toml`; si no existe se usa la
paleta fija de respaldo.

## Atajos

Globales: `F2` Hosts · `F3` sesión activa · `Ctrl+P` paleta · `?` ayuda
(en Hosts) · `Esc` cierra diálogos y filtros · `q` vuelve o sale.

Hosts: `↑` `↓` / `j` `k` mover · `←` `→` / `h` `l` plegar grupo · `↵`
conectar · `e` editar · `n` nuevo · `g` menú de grupos · `x` borrar ·
`Tab` alternar usuario·puerto / etiquetas · `/` filtro · `I` importar ·
`E` exportar.

Ficha: `Tab` / `Shift+Tab` campos · `↵` abrir desplegable · `Espacio`
marcar casilla · `Ctrl+S` guardar · `Ctrl+T` probar la conexión · `Esc`
descartar.

Sesión: todas las teclas van al host remoto salvo el prefijo configurado
(`Ctrl+]` por defecto): `prefijo q` o `prefijo Esc` vuelve a Hosts dejando
la sesión en segundo plano, `prefijo x` la cierra y `prefijo prefijo`
envía el prefijo literal.

## El `Include` de `~/.ssh/config`

MAGI nunca edita `~/.ssh/config` por su cuenta. Para que `ssh <nombre>`
siga funcionando con los hosts de MAGI, al exportar se ofrece insertar
esta línea al principio del fichero (con copia `config.bak-<fecha>`):

```
Include ~/.ssh/magi_config
```

`magi_config` se regenera con `E`, con `magi exportar` o al guardar una
ficha si `exportar_al_guardar` está activo. Es un fichero ssh_config
normal, legible y reimportable con `magi importar ~/.ssh/magi_config`.

## Seguridad

- MAGI no guarda claves privadas ni frases: solo referencias
  (`agente:SHA256:…` o `fichero:<ruta>`). Las frases y los ficheros de
  clave pasan por memoria con `zeroize` al cerrar la sesión.
- Verificación estricta de huellas contra `~/.ssh/known_hosts` (entradas
  hasheadas incluidas). Una huella cambiada bloquea la conexión y sustituirla
  exige escribir el nombre del host; antes se deja copia `known_hosts.old`.
- `magi.db`, `magi_config`, `known_hosts` y las copias se escriben con
  permisos 600 y de forma atómica.
- Sin telemetría: las únicas conexiones salientes son a los hosts del
  usuario.

## Tests

```sh
cargo test
```

Cubren el almacén (CRUD, cascadas, migración desde vacío) y la ida y
vuelta `importar(exportar(hosts))`, incluidos salto, multiplexado,
keepalive y opciones extra.
