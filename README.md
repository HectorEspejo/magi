# MAGI

Gestor SSH de terminal (TUI) con estética de cabina técnica, operado por
teclado, para Linux (Omarchy) y macOS. Inventario de hosts en SQLite, ficha
completa, importación sin pérdida de `~/.ssh/config`, exportación a
`~/.ssh/magi_config`, conexión SSH embebida con verificación estricta de
huellas, **flota con sondeo de carga/memoria/disco/red/servicios**,
**identidades** (generar, importar, copiar, revocar), **registro** de todo lo
que hace MAGI, **pestañas con servidor de sesiones** (las sesiones sobreviven
a la ventana y se comparten entre terminales) y reconexión tras caída. Sin
cuenta, sin suscripción y sin telemetría.

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
cargo run -- servidor estado    # pid, protocolo, sesiones y clientes del servidor
cargo run -- servidor parar     # cierra las sesiones y apaga el servidor
cargo run -- conectar <host>    # abre la TUI con una sesión nueva a ese host
cargo test                      # tests de almacén, parser, estados, ids y vuelta
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## El servidor de sesiones

Desde la Fase 3 hay dos procesos con el mismo binario: la TUI (`magi`) es el
cliente y `magi --servidor` es un pequeño servidor local que **custodia las
sesiones SSH**. El primer cliente lo lanza desacoplado (`setsid`) y hablan por
un socket Unix en `$XDG_RUNTIME_DIR/magi/servidor.sock` (macOS:
`~/Library/Caches/magi/`), con directorio 700 y bloqueo por `flock`.

- Cerrar la ventana **no cierra las sesiones**: siguen en el servidor y
  reaparecen al abrir `magi` de nuevo.
- La misma pestaña puede abrirse desde varias terminales: todas ven y
  escriben (sin arbitraje, como tmux) y el tamaño remoto es el mínimo de las
  ventanas adjuntas.
- Varios terminales comparten el servidor; el diálogo de huella, frase o
  contraseña lo recibe solo la ventana que pidió la conexión.
- Hosts con `multiplexar` reutilizan la conexión entre pestañas sin
  reautenticar; la conexión libre se cierra tras 30 s sin canales.
- Sin clientes ni sesiones durante 10 s el servidor se apaga solo
  (`[servidor] gracia_apagado_seg`). Si muere, las ventanas lo detectan y
  ofrecen relanzarlo.
- El sondeo de Flota, con sesión viva, ejecuta el script sobre esa conexión;
  sin ella cae a su conexión efímera.
- El log del servidor va en `~/.local/state/magi/logs/servidor.log.<fecha>`;
  para depurarlo, `cargo run -- --servidor` en primer plano.

## Pestañas

En la vista Sesión hay una barra de pestañas siempre visible: `●` abierta,
`◐` con actividad sin ver, `✕` caída y `+ nueva` al final; con más de 9, `‹ ›`.
El prefijo (por defecto `Ctrl+]`) manda:

| Tecla | Acción |
|---|---|
| `prefijo 1-9` | ir a la pestaña N |
| `prefijo n` / `prefijo p` | pestaña siguiente / anterior |
| `prefijo l` | lista de sesiones |
| `prefijo c` | nueva sesión (paleta filtrada a hosts) |
| `prefijo x` | cerrar la pestaña (confirma) |
| `prefijo r` | reconectar la pestaña caída |
| `prefijo w` | ventana nueva de terminal (`[terminal] comando`) |
| `prefijo q` / `Esc` | volver a la vista anterior (las sesiones siguen) |
| `prefijo prefijo` | enviar el prefijo literal |

`F3` va a la pestaña activa; sin ninguna abre la **lista de sesiones** del
servidor: `↵` adjuntar · `n` nueva · `r` reconectar · `x` cerrar · `S`
apagar el servidor. `↵` en Hosts y Flota abre siempre una sesión nueva; `q`
sale sin confirmar avisando de las sesiones que siguen abiertas.
`magi conectar <host>` abre una sesión desde un lanzador o atajo.

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
una sesión viva con el host, el sondeo pide al servidor que ejecute el script
sobre esa conexión (`Ejecutar`); el registro solo anota las transiciones de
caída y de vuelta, no cada refresco.

## Autenticación

MAGI autentica con clave pública —agente o fichero, pidiendo la frase si está
cifrada— o con **contraseña**. En la ficha hay dos modos:

- «contraseña · se pide al conectar»: diálogo en cada conexión (3 intentos,
  `zeroize`); la contraseña no se guarda.
- «contraseña · llavero del sistema»: MAGI la recupera del llavero
  (`secret-tool`/libsecret en Linux, Keychain por API `security-framework` en
  macOS) y, si no existe, la pide con la casilla «recordar». El secreto nunca
  está en `magi.db`, solo la referencia `contrasena:llavero` en el host; con
  «recordar» marcada, el guardado ocurre al abrirse la sesión. En la paleta,
  «olvidar contraseña · <host>» la borra del llavero tras confirmación.

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
| Log del servidor | `~/.local/state/magi/logs/servidor.log.<fecha>` |
| Socket y lock | `$XDG_RUNTIME_DIR/magi/servidor.sock` + `servidor.lock` (700) |

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

[terminal]
comando = "alacritty -e magi"   # ventana nueva (prefijo w); macOS: open -a Terminal magi

[servidor]
gracia_apagado_seg = 10     # sin clientes ni sesiones, el servidor se apaga
```

Con `tema = "auto"` se leen los colores de
`~/.config/omarchy/current/theme/alacritty.toml`; si no existe se usa la
paleta fija de respaldo.

## Atajos

Globales: `F1` Flota · `F2` Hosts · `F3` pestaña activa o lista de sesiones ·
`F5` Identidades · `F7` Registro · `Ctrl+P` paleta · `?` ayuda · `Esc` cierra
diálogos y filtros · `q` vuelve o sale.

Flota: `↑` `↓` / `j` `k` mover · `↵` conectar · `r` sondear el host · `R`
sondear todos los visibles · `e` editar la ficha · `/` filtro · `a`
activar/pausar el auto-refresco. `↵` abre siempre una sesión nueva.

Hosts: `↑` `↓` / `j` `k` mover · `←` `→` / `h` `l` plegar grupo · `↵`
conectar (siempre sesión nueva) · `e` editar · `n` nuevo · `g` menú de
grupos · `x` borrar · `Tab` alternar usuario·puerto / etiquetas · `/` filtro ·
`I` importar · `E` exportar. `●N` junto al nombre indica N sesiones vivas.

Identidades: `n` generar · `i` importar · `c` copiar la pública · `e` alias ·
`x` revocar/reactivar · `s` reescanear · `v` ver revocadas.

Registro: `↵` detalle · `/` filtrar · `t` ciclar el tipo · `p` purgar >90
días · `x` exportar CSV/JSON.

Ficha: `Tab` / `Shift+Tab` campos · `↵` abrir desplegable · `Espacio`
marcar casilla · `Ctrl+S` guardar · `Ctrl+T` probar la conexión · `Esc`
descartar.

Sesión: todas las teclas van al host remoto salvo el prefijo configurado
(`Ctrl+]` por defecto). Con el prefijo: navegación de pestañas (`1-9`, `n`,
`p`), lista (`l`), nueva sesión (`c`), cerrar pestaña (`x`), reconectar
(`r`), ventana nueva (`w`), volver (`q`/`Esc`) y el prefijo literal
(`prefijo prefijo`). Las sesiones viven en el servidor: cerrar la ventana no
las cierra.

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
- El servidor de sesiones no dialoga: reenvía huellas, frases y contraseñas
  al cliente solicitante por el socket del propio usuario (directorio 700) y
  los secretos viajan en memoria `zeroize`, jamás al log ni al registro. Las
  pantallas (`Datos`/`PantallaCompleta`) solo llegan a las ventanas adjuntas.
- El servidor nunca acepta ni sustituye huellas por su cuenta: solo con la
  decisión del solicitante.
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
- Un mensaje desconocido en el protocolo produce error y desconexión de ese
  cliente, nunca un fallo del servidor; un servidor de otra versión de
  protocolo se rechaza en el saludo.
- Sin telemetría: las únicas conexiones salientes son a los hosts del
  usuario.

## Tests

```sh
cargo test
```

Cubren el almacén (CRUD, cascadas, migración desde vacío), la ida y vuelta
`importar(exportar(hosts))` (salto, multiplexado, keepalive y opciones
extra), el protocolo (ida y vuelta de todos los mensajes, bytes en base64,
secretos ocultos en el depurado, mensajes desconocidos) y el servidor con
socket temporal (arranque, saludo versionado, apagado por inactividad, lock
duplicado y desconexión por mensaje mal formado).
