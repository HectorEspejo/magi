# MAGI

Gestor SSH de terminal (TUI) con estética de cabina técnica, operado por
teclado, para Linux (Omarchy) y macOS. Inventario de hosts en SQLite, ficha
completa, importación sin pérdida de `~/.ssh/config`, exportación a
`~/.ssh/magi_config`, conexión SSH embebida con verificación estricta de
huellas, **flota con sondeo de carga/memoria/disco/red/servicios**,
**identidades** (generar, importar, copiar, revocar) y **registro** de todo lo
que hace MAGI. Sin cuenta, sin suscripción y sin telemetría.

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
cargo run                       # abre la TUI en la vista Flota
cargo run -- importar [ruta]    # importa ~/.ssh/config (o la ruta indicada)
cargo run -- exportar           # regenera ~/.ssh/magi_config
cargo run -- sondear [host…]    # sondea la flota sin TUI e imprime la tabla
cargo run -- registro exportar <ruta> [--json] [--desde AAAA-MM-DD]
cargo test                      # tests de almacén, parser, estados, ids y vuelta
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Sondeo de flota

`F1` abre **Flota**, la vista de arranque. El sondeo ejecuta un único script
`sh` embebido (`src/flota/sondeo.sh`) por canal `exec` de SSH, en paralelo con
un semáforo de 8 y timeout de 5 s por host. Devuelve núcleos, carga, memoria,
disco, red y el estado de las unidades systemd listadas en la ficha
(campo **Servicios**, una por línea). Los estados se derivan del último sondeo
con los umbrales de `config.toml`: `FRÍA`, `CAÍDA`, `ALCANZ.`, `CARGA` y
`NOMINAL`. Se conservan los 20 últimos sondeos por host.

El sondeo nunca abre diálogos: si falta aceptar una huella o la clave pide
frase, el host queda CAÍDA con la instrucción «conéctate una vez con ↵». Si hay
una sesión viva con el host, el sondeo abre un canal sobre la misma conexión.

## Autenticación

MAGI autentica con clave pública —agente o fichero, pidiendo la frase si está
cifrada— o con **contraseña**. En la ficha hay dos modos:

- «contraseña · se pide al conectar»: diálogo en cada conexión (3 intentos,
  `zeroize`); la contraseña no se guarda.
- «contraseña · llavero del sistema»: MAGI la recupera del llavero
  (`secret-tool`/libsecret en Linux, Keychain en macOS) y, si no existe, la
  pide con la casilla «recordar». El secreto nunca está en `magi.db`, solo la
  referencia `contrasena:llavero` en el host; con «recordar» marcada, el
  guardado ocurre tras autenticar. En la paleta, «olvidar contraseña · <host>»
  la borra del llavero tras confirmación.

Las operaciones automáticas nunca dialogan: con el llavero, el sondeo usa la
contraseña guardada sin preguntar; sin ella, el host queda CAÍDA con
instrucción, salvo que ya exista una sesión viva, en cuyo caso reutiliza su
canal.

## Identidades y registro

`F5` es la pantalla de identidades: sincroniza las claves de `~/.ssh` y del
agente por huella, permite generar ed25519/RSA 4096 (con frase opcional y alta
en el agente), importar, copiar la pública (`wl-copy`/`pbcopy`, con fallback a
un diálogo), editar alias y revocar/reactivar. Revocar es una baja lógica: los
hosts pasan a identidad `auto` y la clave sigue en `~/.ssh` y en el agente.

`F7` muestra el **Registro**: conexiones, huellas aceptadas o sustituidas,
importación/exportación, claves y sondeos fallidos, con filtro, detalle,
purga de más de 90 días y exportación CSV/JSON. Todo pasa por
`registro::anotar`; las entradas nunca se editan.

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

[flota]
auto_refresco_seg = 0       # 0 = desactivado; mínimo 15 s (se conmuta con «a»)

[flota.umbrales]
carga_por_nucleo = 1.0      # CARGA si carga_1m > umbral × núcleos
memoria_pct = 90.0          # CARGA si el uso de memoria supera el %
disco_pct = 90.0            # CARGA si el uso de disco supera el %
```

Con `tema = "auto"` se leen los colores de
`~/.config/omarchy/current/theme/alacritty.toml`; si no existe se usa la
paleta fija de respaldo.

## Atajos

Globales: `F1` Flota · `F2` Hosts · `F3` sesión activa · `F5` Identidades ·
`F7` Registro · `Ctrl+P` paleta · `?` ayuda · `Esc` cierra diálogos y
filtros · `q` vuelve o sale.

Flota: `↑` `↓` / `j` `k` mover · `↵` conectar · `r` sondear el host · `R`
sondear todos los visibles · `e` editar la ficha · `/` filtro · `a`
activar/pausar el auto-refresco.

Hosts: `↑` `↓` / `j` `k` mover · `←` `→` / `h` `l` plegar grupo · `↵`
conectar · `e` editar · `n` nuevo · `g` menú de grupos · `x` borrar ·
`Tab` alternar usuario·puerto / etiquetas · `/` filtro · `I` importar ·
`E` exportar.

Identidades: `n` generar · `i` importar · `c` copiar la pública · `e` alias ·
`x` revocar/reactivar · `s` reescanear · `v` ver revocadas.

Registro: `↵` detalle · `/` filtrar · `t` ciclar el tipo · `p` purgar >90
días · `x` exportar CSV/JSON.

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
- El sondeo nunca acepta ni sustituye huellas, no dialoga y no toca
  `HOSTS.ultimo_estado`: solo los sondeos van a `SONDEOS`, acotado a 20 por
  host.
- MAGI genera claves en `~/.ssh` (nunca sobrescribe) pero jamás borra ficheros
  de `~/.ssh` ni quita claves del agente: revocar es una baja lógica.
- Los nombres de unidad systemd se validan (`[A-Za-z0-9@._-]+`) antes de
  pasarlos al script de sondeo, que los recibe como argumentos.
- `magi.db`, `magi_config`, `known_hosts`, las claves generadas y las
  exportaciones se escriben con permisos 600 (644 el `.pub`) y de forma
  atómica.
- Sin telemetría: las únicas conexiones salientes son a los hosts del
  usuario.

## Tests

```sh
cargo test
```

Cubren el almacén (CRUD, cascadas, migración desde vacío) y la ida y
vuelta `importar(exportar(hosts))`, incluidos salto, multiplexado,
keepalive y opciones extra.
