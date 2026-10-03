# MAGI

Gestor SSH de terminal (TUI) con estética de cabina técnica, operado por
teclado, para Linux (Omarchy) y macOS. Inventario de hosts en SQLite, ficha
completa, importación sin pérdida de `~/.ssh/config`, exportación a
`~/.ssh/magi_config`, conexión SSH embebida con verificación estricta de
huellas, **flota con sondeo de carga/memoria/disco/red/servicios**,
**identidades** (generar, importar, copiar, revocar), **registro** de todo lo
que hace MAGI, **pestañas con servidor de sesiones** (las sesiones sobreviven
a la ventana y se comparten entre terminales), **túneles** local, remoto y
SOCKS5 que también siguen vivos al cerrarla, y reconexión tras caída. Sin
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
cargo run -- tuneles            # lista los túneles definidos y activos
cargo run -- tunel activar <host> <nombre>   # levanta ese túnel
cargo run -- tunel parar <host> <nombre>     # lo para
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
- Cada host tiene como mucho **una conexión compartida** en el servidor. El
  SFTP, los túneles, el sondeo por la conexión viva y las ejecuciones de
  snippets la usan siempre; las pestañas, solo si el host tiene
  `multiplexar`: entonces una segunda pestaña abre un canal nuevo sin
  reautenticar ni volver a preguntar la huella. Sin `multiplexar`, cada
  pestaña abre su propia conexión, que se cierra con ella. Abrir una pestaña
  nunca tumba un túnel ni el SFTP del host, y dos aperturas simultáneas del
  mismo host esperan una a la otra en vez de abrir dos conexiones. La
  conexión compartida se cierra tras 30 s sin canales.
- Sin clientes, sesiones, transferencias ni túneles levantados durante 10 s el
  servidor se apaga solo (`[servidor] gracia_apagado_seg`). Si muere, las
  ventanas lo detectan y ofrecen relanzarlo.
- `SIGTERM` para el servidor igual que `magi servidor parar`: cierra sesiones
  y túneles anotándolo y borra socket y lock. Si el servidor en marcha es de
  otra versión (tras actualizar MAGI), `magi servidor parar` muestra su pid y
  le envía `SIGTERM` tras confirmar (`--si` no pregunta).
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

## Tamaño de la terminal

MAGI se adapta a cualquier cambio de tamaño: mosaico de Hyprland, mover la
ventana, pantalla completa o zoom de fuente de Alacritty (`Ctrl+=` `Ctrl+-`
`Ctrl+0`). Los cambios se agrupan (50 ms sin eventos) y se aplica solo el
último: una limpieza de pantalla y un repintado por tamaño, sin pulsar ninguna
tecla, aunque una animación mande treinta. Al volver del visor (`F4`) se lee
el tamaño real.

En la vista Sesión el remoto recibe el tamaño nuevo (`window-change`, el
SIGWINCH de `vim` o `htop`) en cuanto se aplica. Una pestaña abierta en varias
ventanas usa el mínimo de todas: la mayor rellena con `░` lo que sobra y la
barra de estado dice `cols×filas (mín. ventana N)`; cuando la pequeña crece,
sale de la pestaña o se cierra, el remoto vuelve a crecer.

**Modo estrecho** (menos de 100 columnas):

- Flota y Archivos enseñan un solo panel y `Tab` alterna. La cabecera dice cuál
  se ve: `⇥ detalle` / `⇥ lista`, `[local]` / `[remoto]`.
- Las tablas ocultan columnas por prioridad: Hosts, la dirección por debajo de
  80 columnas y usuario·puerto por debajo de 60. El nombre y el glifo de estado
  no se ocultan nunca.
- La barra inferior muestra los atajos más importantes y termina en `? más`;
  la ayuda (`?`) los lista todos.

**Vistas bajas** (menos de 20 filas):

- Los detalles inferiores de Transferencias, Túneles, Registro, Identidades y
  Snippets se pliegan. Se abren en un diálogo con `↵` (`i` en Snippets, donde
  `↵` ejecuta).
- En Resultados, con menos de 24 filas, la salida solo se ve con `↵`.
- Diálogos, paleta y ayuda se recolocan y encogen; lo que no cabe se desplaza
  con `↑` `↓` `PgUp` `PgDn`.

En ventanas muy grandes (200×60 o más) los paneles de detalle y los diálogos
tienen ancho máximo y van centrados.

**Tamaños mínimos.** Por debajo del mínimo se ve un aviso con el tamaño actual,
el mínimo y la vista que lo exige. Con el aviso siguen activas `q`, `F1`-`F8` y
`Ctrl+P`; en Sesión las teclas siguen yendo al remoto y este recibe su tamaño
real. Con menos de 20×3 solo se pinta `MAGI cols×filas`.

| Vista | Mínimo |
|---|---|
| Flota, Hosts, Sesiones (y cualquier otra) | 40×12 |
| Sesión | 40×8 |
| Ficha, Archivos | 50×14 |
| Transferencias, Túneles, Registro, Identidades, Snippets | 50×12 |
| Resultados | 60×14 |
| Diálogo MAGI | 50×12 |

El modo ASCII (`MAGI_ASCII=1` o `terminal_ascii = true`) degrada todos los
glifos en todos los modos, también en el aviso.

## Archivos

`F4` (o `s` en Hosts y Flota, o el prefijo `f` en Sesión, o `sftp · <host>` en
la paleta) abre la vista Archivos: un panel doble local ⇄ remoto. Todo lo
remoto lo hace el servidor sobre un canal SFTP por host, así que las
transferencias siguen cuando se cierra la ventana.

- **Paneles**: `Tab` cambia de panel; el activo lleva el nombre en ámbar. Las
  columnas son nombre, tamaño y fecha (`hoy`, `ayer`, `12 sep`, `2025`), con
  los directorios primero y una fila `..` para subir.
- **Marcas**: `≠` si el elemento existe al otro lado con otro tamaño o fecha
  (tolerancia de 2 s) y `✕` si no está. Los directorios solo llevan `✕`. Se
  comparan los dos listados visibles, así que sirven para ver qué cambió en un
  despliegue.
- **Copiar y mover**: `c` copia y `m` mueve lo marcado (o la fila actual) al
  directorio del otro panel. Si el destino ya existe, se pregunta sobrescribir
  u omitir, con el tamaño y la fecha de los dos lados; `S` y `O` aplican la
  decisión a todo lo que queda. Un «mover» borra el origen solo cuando la
  copia ha terminado bien.
- **Aviso de sensibles**: antes de subir, se avisa si algún fichero coincide
  con los patrones de `[archivos] avisar` (por defecto `.env*`, `*.pem`,
  `*.key`, `id_*`), recorriendo también los directorios. Con la lista vacía el
  aviso queda desactivado y la barra lo dice.
- **Cola**: al pie, tres filas con la transferencia en curso y lo que espera
  turno. `t` abre la vista ampliada (`x` cancelar, `C` limpiar terminadas, `↵`
  detalle), y `magi servidor estado` lista la cola desde fuera de la TUI. Una
  transferencia en curso se cancela por bloques de 64 KiB: el parcial se borra
  y nada queda a medias.
- **Ver ficheros**: `↵` sobre un fichero lo abre con `[archivos] pager`
  (`$PAGER` o `less`): la TUI se suspende, se ve el fichero y se restaura. Un
  remoto se trae antes a un temporal en `$XDG_RUNTIME_DIR/magi/tmp` (700, 600)
  que se borra al cerrar el visor; si pasa de 10 MB se pide confirmación.
- **Borrar, renombrar y crear**: `x` borra sin papelera tras confirmar con el
  recuento, `r` renombra y `d` crea un directorio. En remoto lo hace el
  servidor (el borrado, recursivo) y queda anotado en el historial.

Últimos directorios: cada host recuerda dónde se quedó cada panel
(`HOSTS.sftp_dir_local` y `HOSTS.sftp_dir_remoto`), así que al volver se abre
donde se estaba.

## Túneles

`F6` abre **Túneles**: los reenvíos definidos en la ficha de cada host y lo
que el servidor tiene levantado en ese momento. Los ejecuta el **servidor de
sesiones**, así que cerrar la ventana no los tira.

| Tipo | Qué hace |
|---|---|
| `local` | MAGI escucha en `escucha` y el host abre el destino (`ssh -L`). |
| `remoto` | El host escucha en `escucha` y MAGI abre el destino en local (`ssh -R`). |
| `dinámico` | Proxy SOCKS5 que escucha en `escucha` y sale por el host (`ssh -D`); no lleva destino. |

- **Alta**: `n` crea un túnel (host, nombre, tipo, escucha y destino), `e`
  edita la fila y `x` la borra tras confirmar. `a` marca el túnel como
  automático y `↵` abre el detalle (origen, conexiones, tráfico, último error).
- **Marcha**: `Espacio` activa un inactivo, para lo que esté en marcha y
  descarta el error de uno caído; `r` relanza un caído con los contadores a
  cero. `/` filtra y `Esc` limpia el filtro o vuelve a la vista anterior.
- **Credenciales**: levantar un túnel abre la conexión del host, así que si
  falta aceptar una huella o la clave pide frase, el diálogo sale igual que en
  una sesión. El ciclo automático nunca dialoga: deja el túnel caído con el
  motivo.
- **Automáticos**: se levantan con la primera pestaña o canal SFTP del host y
  se paran con el último. Un túnel manual nunca se para solo, y uno automático
  parado a mano no vuelve hasta el siguiente ciclo.

Desde fuera de la TUI:

```sh
magi tuneles                        # definidos y activos, con estado y tráfico
magi tunel activar <host> <nombre>  # lo levanta (autolanza el servidor si no está)
magi tunel parar <host> <nombre>    # lo para
```

Estas órdenes no dialogan: si el host necesita credenciales, avisan de que hay
que abrir una sesión desde la TUI primero. Y un túnel levantado mantiene el
servidor en pie: no se apaga por inactividad mientras haya alguno en marcha.

Los reenvíos de `~/.ssh/config` (`LocalForward`, `RemoteForward`,
`DynamicForward`) entran en la tabla de túneles al importar y vuelven a
`magi_config` al exportar; ya no se guardan en las «opciones extra» del host.

**Aviso de seguridad**: por defecto todo escucha en `127.0.0.1`. Una escucha
en `0.0.0.0` o `::` la puede usar cualquier equipo de tu red (el diálogo lo
avisa en ámbar) y el SOCKS5 no pide autenticación a quien lo use. En un túnel
remoto, el host solo expondrá esa escucha si tiene `GatewayPorts`.

## Snippets

`F8` abre **Snippets**: comandos guardados que se ejecutan en varios hosts a
la vez. Cada snippet tiene nombre, comando (multilínea), descripción,
etiquetas propias, destinos, `crítico`, timeout (5-3600 s) y «parar al primer
fallo». Los **destinos** son etiquetas de host (`web` = todos los hosts con esa
etiqueta) y hosts sueltos; se resuelven al ejecutar, sin duplicados y en el
orden de Hosts.

- **Variables**: `{{servicio}}` o `{{servicio:nginx}}` (con valor por
  defecto); el nombre es `[a-z_][a-z0-9_]*` y todo lo demás (`{{.Names}}` de
  docker) es literal. El valor se sustituye **escapado para el shell** con
  `shell-escape`: `a b; rm -rf /` llega como un único argumento entrecomillado.
  Una variable vacía sin defecto no deja continuar, y una variable dentro de
  comillas o de un heredoc se rechaza al guardar (el escape no serviría ahí).
- **Ejecutar**: `↵` abre EJECUTAR (casillas por host, variables, «parar al
  primer fallo» y timeout); `a` ejecuta en todos los destinos pidiendo solo las
  variables; `p` abre una pestaña por host con el comando escrito al abrir el
  shell (confirma si son más de 5). Desde la paleta: `snippet · <nombre>` y
  `snippet · <nombre> · <host>`; en Hosts y Flota, `!` lista los snippets que
  apuntan al host seleccionado.
- **En el servidor**: la ejecución la hace el servidor de sesiones, hasta 8
  hosts en paralelo sobre la conexión del pool, con stdout y stderr separados,
  código de salida y el timeout del snippet (al vencer, el host queda en error
  «tiempo agotado» y la conexión sigue viva). Nunca dialoga: una huella sin
  aceptar, una clave con frase fuera del agente o una contraseña que no esté en
  el llavero dejan el host en error con la instrucción. «Parar al primer
  fallo» deja `omitido` lo que aún no había empezado.
- **Resultados** (`t` en Snippets): ejecuciones y hosts con su estado, código,
  duración y bytes; `↵` enseña stdout y stderr sin secuencias de escape y se
  refresca mientras corre; `s` guarda la salida en un fichero (600), `x`
  cancela, `r` repite (volviendo a deliberar si procede) y `C` limpia las
  terminadas. Cada flujo se guarda hasta 1 MiB; la salida **nunca** va al
  registro ni al log. Todas las ventanas ven el mismo progreso y las terminadas
  se conservan una hora.
- **Snippet al conectar**: en la ficha, bloque «Al conectar», un snippet no
  crítico y sin variables que se escribe en cada pestaña nueva a ese host y al
  reconectarla. Nunca delibera.
- **Atajos de Flota**: `[flota.atajos]` asigna una tecla a un snippet sobre el
  host seleccionado; sigue el mismo flujo que `snippet · <nombre> · <host>`.
  Las teclas que ya usa Flota o los snippets que no existen se ignoran con
  aviso al arrancar.

```sh
magi snippets    # destinos resueltos, crítico y último uso
```

## Deliberación MAGI

Antes de ejecutar un snippet **crítico**, en **más de un host** o en un host
con **verificaciones previas**, MAGI delibera: tres comprobaciones por host,
todas en paralelo y con un plazo duro de 2 s cada una, que nunca abren
diálogos.

| Comprobación | Aprueba si | Rechaza si |
|---|---|---|
| MELCHIOR-1 · salud | El último sondeo es NOMINAL y tiene menos de 5 min, o lo es uno nuevo | CARGA (con los culpables), ALCANZABLE, CAÍDA o sin datos a tiempo |
| BALTHASAR-2 · backup | El fichero más reciente de la ruta que casa con el patrón tiene menos de 24 h | Más viejo, directorio vacío, sin acceso SFTP o plazo vencido |
| CASPER-3 · tests | `sh -c <comando>` **en local** termina con 0 | Otro código (con la última línea de stderr) o plazo vencido (el proceso se mata) |

Qué comprueba cada host se elige en su ficha, bloque «Verificaciones previas»:
salud, backup (ruta remota y patrón, p. ej. `*.sql.gz`) y tests (comando
local, p. ej. `gh run list … | grep -qx success`). El SFTP de BALTHASAR-2 no
dialoga ni levanta los túneles automáticos del host.

El diálogo enseña una tabla host × comprobación (`✓` aprueba, `✕` rechaza,
`—` no activa, `◐` comprobando) con el dato de cada veredicto, la barra de
consenso y el estado. Hace falta **unanimidad**:

- **APROBADO**: `Ctrl+K` ejecuta. `↵` **no** ejecuta nunca.
- **BLOQUEADO**: `f` abre el campo motivo; con al menos 10 caracteres pasa a
  **FORZADO** y `Ctrl+K` ejecuta. El motivo queda registrado con el usuario y
  la hora.
- Sin comprobaciones activas (un snippet crítico en un host sin
  verificaciones) muestra «sin comprobaciones configuradas» y sigue pidiendo
  `Ctrl+K`.
- `Esc` cancela: no se ejecuta nada y el registro anota
  `deliberacion_cancelada`.

Cada deliberación resuelta queda en `DELIBERACIONES` (acción, hosts,
comprobaciones, resultado, motivo y usuario); al terminar la ejecución, el
servidor anota `deliberacion_aprobada` o `deliberacion_forzada` con el
resultado (`ok`, `parcial`, `error`, `cancelada`). En `F7`, el detalle de esas
entradas enseña las comprobaciones.

```toml
[deliberacion]
backup_horas = 24    # antigüedad máxima del backup
salud_max_min = 5    # antigüedad máxima del último sondeo
limite_seg = 2       # plazo de cada comprobación
motivo_min = 10      # caracteres mínimos del motivo de un forzado

[flota.atajos]
d = "desplegar"      # tecla → nombre del snippet
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
`F4` Archivos · `F5` Identidades · `F6` Túneles · `F7` Registro · `F8`
Snippets · `Ctrl+P` paleta · `?` ayuda · `Esc` cierra diálogos y filtros ·
`q` vuelve o sale.

Flota: `↑` `↓` / `j` `k` mover · `↵` conectar · `r` sondear el host · `R`
sondear todos los visibles · `e` editar la ficha · `/` filtro · `a`
activar/pausar el auto-refresco · `!` snippets del host · `Tab` alterna
lista y detalle en modo estrecho · las teclas de `[flota.atajos]`, que
aparecen en la barra. `↵` abre siempre una sesión nueva.

Hosts: `↑` `↓` / `j` `k` mover · `←` `→` / `h` `l` plegar grupo · `↵`
conectar (siempre sesión nueva) · `e` editar · `n` nuevo · `g` menú de
grupos · `x` borrar · `Tab` alternar usuario·puerto / etiquetas · `/` filtro ·
`I` importar · `E` exportar · `!` snippets del host. `●N` junto al nombre
indica N sesiones vivas.

Identidades: `n` generar · `i` importar · `c` copiar la pública · `e` alias ·
`x` revocar/reactivar · `s` reescanear · `v` ver revocadas · `↵` detalle
(con la vista baja).

Registro: `↵` detalle · `/` filtrar · `t` ciclar el tipo · `p` purgar >90
días · `x` exportar CSV/JSON.

Ficha: `Tab` / `Shift+Tab` campos · `↵` abrir desplegable · `Espacio`
marcar casilla · `Ctrl+S` guardar · `Ctrl+T` probar la conexión · `Esc`
descartar.

Sesión: todas las teclas van al host remoto salvo el prefijo configurado
(`Ctrl+]` por defecto). Con el prefijo: navegación de pestañas (`1-9`, `n`,
`p`), lista (`l`), nueva sesión (`c`), archivos (`f`), cerrar pestaña (`x`),
reconectar (`r`), ventana nueva (`w`), volver (`q`/`Esc`) y el prefijo
literal (`prefijo prefijo`). Las sesiones viven en el servidor: cerrar la
ventana no las cierra.

Archivos: `Tab` panel · `↑` `↓` / `j` `k` mover · `PgUp` `PgDn` página ·
`Home` `End` extremos · `↵` entrar o ver · `⌫` / `-` subir · `Espacio` marcar y
bajar · `a` / `A` marcar todo / desmarcar · `c` copiar · `m` mover · `x`
borrar · `r` renombrar · `d` crear directorio · `.` ocultos · `/` filtro · `R`
refrescar · `g` ir a ruta · `i` detalle · `h` cambiar de host · `t` cola ·
`Esc` limpiar filtro o marcas · `q` volver.

Transferencias: `↑` `↓` mover · `x` cancelar · `C` limpiar terminadas · `↵`
detalle · `q` volver.

Túneles: `↑` `↓` / `j` `k` mover · `Espacio` activar, parar o descartar el
error · `n` nuevo · `e` editar · `x` borrar · `a` automático · `r` relanzar ·
`↵` detalle · `/` filtro · `Esc` limpiar el filtro o volver · `q` volver.

Snippets: `↑` `↓` / `j` `k` mover · `↵` ejecutar (diálogo) · `a` en todos los
destinos · `p` en pestañas · `n` nuevo · `e` editar · `x` borrar · `/`
filtro · `t` Resultados · `i` detalle (con la vista baja) · `q` volver. En el
formulario, `Ctrl+S` guarda.

Resultados: `↑` `↓` / `j` `k` mover · `Tab` panel · `↵` ver la salida (`Tab`
alterna stdout/stderr; con menos de 24 filas es la única forma de verla) ·
`s` guardar la salida · `x` cancelar · `r` repetir · `C` limpiar terminadas ·
`q` volver.

Deliberación: `Ctrl+K` ejecutar (solo con consenso o forzada) · `f` forzar con
motivo · `↑` `↓` recorrer hosts · `Esc` cancelar o salir del motivo.

En `config.toml`:

```toml
[archivos]
avisar = [".env*", "*.pem", "*.key", "id_*"]
mostrar_ocultos = false
pager = ""        # vacío usa $PAGER y, si no hay, less
```

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
- Los túneles escuchan en `127.0.0.1` salvo que la escucha diga otra cosa: una
  escucha en `0.0.0.0` o `::` queda al alcance de toda la red local y el SOCKS5
  no pide autenticación. Un canal `forwarded-tcpip` que no corresponda a un
  túnel remoto registrado se rechaza.
- MAGI genera claves en `~/.ssh` (nunca sobrescribe) pero jamás borra ficheros
  de `~/.ssh` ni quita claves del agente: revocar es una baja lógica.
- Los snippets se ejecutan con sus variables escapadas para el shell
  (`shell-escape`) y la salida de los comandos remotos nunca se escribe en el
  registro ni en el log; al pintarla se quitan las secuencias de escape.
- Las ejecuciones y la deliberación nunca dialogan: la contraseña de una
  ejecución solo sale del llavero de la ventana que la lanzó. CASPER-3 ejecuta
  en **local** el comando que pongas en la ficha, con tu usuario.
- Un snippet crítico, en varios hosts o en un host con verificaciones no se
  ejecuta sin deliberación, y forzarla exige un motivo que queda registrado.
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

El pool de conexiones (`tests/pool.rs`) comprueba que pestañas, SFTP y
túneles comparten una única conexión con `multiplexar`, que una pestaña no
tumba un túnel, el `Ejecutar` con plazo y el `SIGTERM` al binario real. Las
ejecuciones (`tests/ejecuciones.rs`) corren contra un servidor SSH en proceso
que atiende `exec`: varios hosts, código ≠ 0, timeout con el pool vivo, parar
al primer fallo, cancelación, tope de 1 MiB, anotaciones sin salida y
deliberaciones. La deliberación tiene pruebas de las tres comprobaciones
(CASPER-3 con `true`, `false` y `sleep 3` reales) y del diálogo.

El redimensionado (`tests/redimensionado.rs`) reproduce los dos fallos de la
Fase 7 contra el servidor SSH de pruebas, que registra cada `window-change`.
Prueba la tubería de tamaño con un `TestBackend` que cuenta las limpiezas y la
App real contra el servidor en proceso, con una y con dos ventanas. Además
compara instantáneas `insta` de todas las vistas y diálogos a 40×12, 80×24 y
200×60 (en `tests/snapshots/`), recorre una rejilla de tamaños sin pánico y
comprueba que las secuencias de cambio conservan el estado. Tras un cambio
visual deliberado, se regeneran con `INSTA_UPDATE=always cargo test --test
redimensionado` y se revisan con `git diff tests/snapshots` (o con `cargo insta
review`).

Las pruebas de archivos (`tests/sftp.rs`) levantan un servidor SSH en proceso
que sirve el subsistema `sftp` con el **`sftp-server` real de OpenSSH**: listar,
subir y bajar ficheros y directorios, conflicto con `omitir`, cancelación en
curso, `mtime` conservado, temporales en 600, borrado recursivo y la cola que
ve una segunda ventana. Si el sistema no trae `sftp-server`, esas pruebas se
saltan con un aviso en lugar de fallar. El resto (marcas, avisos de sensibles,
paneles, orden y fechas) son pruebas unitarias puras.
