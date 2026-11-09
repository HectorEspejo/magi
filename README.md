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
cuenta, sin suscripción y sin telemetría. Primera versión pública: **1.0.0**
(30 de octubre de 2026), software libre bajo MIT OR Apache-2.0.

## Requisitos

- Linux (Omarchy/Arch, o cualquier distribución con glibc ≥ 2.35) o macOS.
- Opcional: `ssh-agent` en `SSH_AUTH_SOCK` para autenticar sin frase.
- Solo para compilar desde el código: Rust stable (edición 2021, MSRV 1.86).

## Instalación

MAGI se distribuye como paquete **`magi-ssh`**; el comando sigue siendo
`magi`. Primera versión pública: **1.0.0**.

### Omarchy y Arch (AUR)

```sh
yay -S magi-ssh-bin
```

El paquete instala `/usr/bin/magi`, el autocompletado de bash, zsh y fish, la
página man, la entrada de escritorio y el icono. Al terminar, `pacman` sugiere
ejecutar `magi diagnostico`.

Nota: en AUR existe otro paquete llamado `magi` (una TUI de Git) que también
instala `/usr/bin/magi`; `magi-ssh-bin` lo declara en `conflicts`, así que
`pacman` pedirá quitarlo antes de instalar MAGI.

### macOS (Homebrew)

```sh
brew install 4d3-org/tap/magi-ssh
```

La fórmula instala el binario `magi` en Apple Silicon e Intel y no pasa por
Gatekeeper, así que no hay que quitar la cuarentena. Si el autocompletado o la
página man no aparecen en las rutas de Homebrew, actívalos a mano (ver
[Autocompletado y man en macOS](#autocompletado-y-man-en-macos)).

### Tarball de la última release

Para otras distribuciones de Linux o para macOS sin Homebrew, descarga el
tarball de [la última release](https://github.com/4d3-org/magi/releases/latest):

| Plataforma | Tarball |
|---|---|
| Linux x86_64 | `magi-ssh-x86_64-unknown-linux-gnu.tar.xz` |
| Linux aarch64 | `magi-ssh-aarch64-unknown-linux-gnu.tar.xz` |
| macOS Apple Silicon | `magi-ssh-aarch64-apple-darwin.tar.xz` |
| macOS Intel | `magi-ssh-x86_64-apple-darwin.tar.xz` |

```sh
tar -xf magi-ssh-x86_64-unknown-linux-gnu.tar.xz
sudo install -m755 magi-ssh-x86_64-unknown-linux-gnu/magi /usr/local/bin/magi
```

Los binarios de Linux necesitan **glibc ≥ 2.35** (Ubuntu 22.04 o posterior); en
Omarchy y Arch usa el paquete de AUR. En macOS, si el tarball lo bajaste con el
navegador, quita la cuarentena antes de ejecutarlo:

```sh
xattr -d com.apple.quarantine magi
```

### Verificación

Cada release publica un `.sha256` por tarball y atestaciones de procedencia de
GitHub. Verifica ambos antes de instalar:

```sh
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo 4d3-org/magi
```

En macOS, si no tienes `sha256sum`, usa `shasum -a 256 -c`. `gh` es el
[GitHub CLI](https://cli.github.com/); la atestación comprueba que el binario
salió del CI del repositorio.

### Desde el código

```sh
cargo install --path .
# o, para probar sin instalar:
cargo build --release && ./target/release/magi
```

## Desarrollo

```sh
cargo run                       # abre la TUI en la vista Flota
cargo run -- importar [ruta]    # importa ~/.ssh/config (o la ruta indicada)
cargo run -- exportar           # regenera ~/.ssh/magi_config
cargo run -- sondear [host…]    # sondea la flota sin TUI e imprime la tabla
cargo run -- registro exportar <ruta> [--json] [--desde AAAA-MM-DD]
cargo run -- diagnostico        # comprueba lo que MAGI necesita, sin escribir nada
cargo run -- autocompletado bash # escribe el autocompletado en la salida estándar
cargo run -- man                # escribe la página man en la salida estándar
cargo run -- servidor estado    # pid, versión, sesiones y clientes del servidor
cargo run -- servidor parar     # cierra las sesiones y apaga el servidor
cargo run -- tuneles            # lista los túneles definidos y activos
cargo run -- tunel activar <host> <nombre>   # levanta ese túnel
cargo run -- tunel parar <host> <nombre>     # lo para
cargo run -- conectar <host>    # abre la TUI con una sesión nueva a ese host
cargo run -- sincronizaciones   # lista las sincronizaciones guardadas
scripts/generar-recursos.sh     # regenera assets/ tras cambiar la CLI
TZ=UTC cargo test               # suite completa (las pruebas de pintado necesitan TZ=UTC)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Primeros pasos

1. Ejecuta `magi diagnostico` para comprobar rutas, permisos, socket, llavero,
   portapapeles y `~/.ssh/config`; cada aviso trae la instrucción de arreglo.
2. Arranca `magi` (abre la vista **Flota**). En **Hosts** (`F2`), pulsa `I`
   para importar `~/.ssh/config` o `magi importar [ruta]` desde fuera de la TUI.
3. Revisa la ficha con `e` (dirección, usuario, identidad, salto, keepalive,
   multiplexado, servicios y verificaciones previas) y guarda con `Ctrl+S`.
4. Conecta con `↵` (siempre abre una sesión nueva) y vuelve a la TUI con el
   prefijo `Ctrl+]` y `q` (o `Esc`). Cerrar la ventana no cierra las sesiones.
5. Explora con `Ctrl+P` (paleta) y `?` (ayuda): `F4` Archivos, `F5`
   Identidades, `F6` Túneles, `F7` Registro, `F8` Snippets y `F9` Servicios.
6. Si quieres que `ssh <nombre>` vea los hosts de MAGI, exporta con `E` en
   Hosts (o `magi exportar`) y acepta el `Include` en `~/.ssh/config`.

## Diagnóstico

`magi diagnostico` comprueba, sin escribir nada, sin arrancar el servidor y sin
leer secretos del llavero:

- versión, protocolo y commit;
- rutas de datos, configuración, estado y runtime con sus permisos, y la
  longitud de la ruta del socket;
- `config.toml`, `magi.db` (en solo lectura) y el servidor de sesiones (si está
  en marcha, su versión y lo que custodia);
- agente SSH, llavero, portapapeles, editor, paginador, terminal e `Include` de
  `magi_config` en `~/.ssh/config`.

Cada línea sale como `✓` correcto, `!` aviso o `✕` error (en ASCII `ok`, `!` y
`x`), con la instrucción de arreglo; no hay color si `NO_COLOR` está definido o
la salida no es una terminal. El código de salida es 0 si no hay errores (los
avisos no cuentan) y 1 si hay alguno. Es la primera comprobación que conviene
ejecutar tras instalar o actualizar MAGI.

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
  le envía `SIGTERM` tras confirmar (`--si` no pregunta); al abrir `magi`, el
  servidor anterior se sustituye solo si no custodia nada y, si no, se pregunta
  (ver [Actualizar MAGI](#actualizar-magi)).
- El sondeo de Flota, con sesión viva, ejecuta el script sobre esa conexión;
  sin ella cae a su conexión efímera.
- El log del servidor va en `~/.local/state/magi/logs/servidor.log.<fecha>`;
  para depurarlo, `cargo run -- --servidor` en primer plano.

## Actualizar MAGI

Con el gestor de paquetes, la actualización es la habitual:

```sh
yay -Syu                 # Omarchy y Arch (magi-ssh-bin)
brew upgrade magi-ssh    # macOS (tap 4d3-org/tap)
```

Con el tarball, descarga la versión nueva, verifícala como en la instalación y
sustituye el binario.

**Servidor de sesiones anterior.** Si tenías MAGI abierto, el servidor de
sesiones sigue siendo el del binario anterior. Al abrir el `magi` nuevo:

- si el servidor anterior **no custodia nada** (ninguna sesión, transferencia,
  túnel, ejecución ni otra ventana), se sustituye solo y la barra lo dice: no se
  pierde nada porque no había nada;
- si **custodia algo**, o es una versión que no informa de lo que custodia,
  aparece el diálogo **SERVIDOR DE SESIONES ANTERIOR** con las dos versiones, el
  pid y lo que tiene: `p` lo para y arranca el nuevo tras confirmar («cerrará 2
  sesiones y 1 túnel»), `Esc` sigue sin sesiones y `q` sale. Nunca se para solo;
- si el servidor es **más nuevo** que el binario, se avisa de que actualices MAGI
  y no se para desde ese binario.

La actualización no migra nada por sí sola: la migración de `magi.db` ocurre al
abrir la app, hacia delante. Si vuelves a un binario anterior con una base de
datos ya migrada, se negará a abrirla (sin tocarla): reinstala la versión nueva.

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

- Flota, Archivos y Servicios enseñan un solo panel y `Tab` alterna. La cabecera dice cuál
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
el mínimo y la vista que lo exige. Con el aviso siguen activas `q`, `F1`-`F9` y
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
- **Editar** (`E`): un fichero local se abre con tu editor y el panel se
  refresca al volver. Uno remoto se baja a un temporal privado
  (`$XDG_RUNTIME_DIR/magi/ediciones`, 700 y 600), se abre con el editor
  (`[archivos] editor`, `$VISUAL`, `$EDITOR`, `nvim` o `vi`, lanzado sin shell
  y con la TUI suspendida) y, si el SHA-256 cambió, se pregunta si subirlo.
  Antes de subir se comprueba el remoto: si cambió mientras editabas sale el
  conflicto (`s` sobrescribir, `c` guardar tu versión como copia local
  `<nombre>.magi-<AAAAMMDD-HHMMSS>` en el panel local, `d` descartar); si pasará
  a ser de otro usuario o es un fichero sensible, se avisa. La subida conserva
  los permisos del original. **El temporal no se borra hasta que la subida
  termina, dices «sin cambios», descartas o guardas la copia local**: si algo
  falla se conserva, se dice su ruta y se ofrece reintentar. Por encima de
  `editar_max_mb` (10) o si parece binario, se pide confirmación. Sobre un
  enlace a fichero se edita su destino; sobre un directorio, nada.
- **Permisos** (`p`): rejilla rwx (usuario, grupo, otros) y octal de 3 o 4
  dígitos sincronizados sobre lo marcado. Con modos distintos, las casillas que
  no coinciden salen `[~]` y se respetan en cada fichero; las casillas solo
  tocan los 9 bits rwx y los especiales solo cambian si se escriben en el
  octal. Con directorios, recursivo con alcance todo, solo directorios o solo
  ficheros, con confirmación y recuento; un aviso en ámbar dice si los
  directorios se quedarán sin `x`. Los enlaces nunca se tocan. `Ctrl+S` aplica.
  En remoto lo hace el servidor y queda anotado (`permisos_cambiados`).
- **Propietario**: el detalle `i` enseña propietario y grupo por nombre con el
  número entre paréntesis (`www-data (33)`). En remoto los nombres salen de
  `/etc/passwd` y `/etc/group` del host, leídos por SFTP al abrir el canal (sin
  ejecutar nada); usuarios de LDAP/SSSD se quedan con el número.
- **Sincronizar** (`S`): sincroniza el directorio del panel activo con el del
  otro, en cualquier dirección (activo local → subida, remoto → bajada). El
  diálogo SINCRONIZAR ofrece borrar en el destino lo que no está en el origen
  (desmarcado), exclusiones extra y guardar la sincronización con un nombre. El
  plan es recursivo: crear lo que falta, actualizar lo que difiere en tamaño o
  fecha (±2 s), borrar lo que sobra solo con la casilla y omitir, con su motivo,
  los enlaces a directorio y los choques fichero/directorio. Al crear se usan
  los permisos del origen; al actualizar se conservan los del destino. La vista
  previa es la confirmación: resumen `+ crear · ~ actualizar · − borrar`,
  bytes, excluidos y omitidos, lista con `↑` `↓` `PgUp` `PgDn`, filtro `f` y
  `↵` para seguir (si el plan está vacío, «todo al día»). En subidas se avisa de
  ficheros sensibles; si se sube a un host con verificaciones o se marca
  «borrar», pasa por la deliberación MAGI (`Ctrl+K` ejecuta). Se ejecuta en la
  cola del servidor (sobrevive a cerrar la ventana) y **solo borra al terminar,
  si todo se copió bien**: si falla un fichero, no se borra nada y queda
  «parcial». Transferencias enseña la fase `borrando`.
- **Exclusiones y `.magiignore`**: con la sintaxis de gitignore y en este
  orden: `[archivos] excluir` (por defecto `.git/`, `target/`, `node_modules/`,
  `__pycache__/`, `.DS_Store` y `.magiignore`), el `.magiignore` de la raíz del
  origen y las exclusiones extra. **Lo excluido no se crea, no se actualiza y
  nunca se borra** en el destino.
- **Sincronizaciones guardadas** (`L`): las del host, con dirección (`−` si
  borra), rutas y último resultado. `↵` planifica y abre la vista previa; `n`
  nueva, `e` editar, `x` borrar (con confirmación). También desde la paleta
  (`sync · <host> · <nombre>`) y con `magi sincronizaciones`.

Últimos directorios: cada host recuerda dónde se quedó cada panel
(`HOSTS.sftp_dir_local` y `HOSTS.sftp_dir_remoto`), así que al volver se abre
donde se estaba.

```sh
magi sincronizaciones   # host, nombre, dirección, rutas, borrar y último resultado
```

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

## Sincronización del inventario

Desde la 1.1.0 el mismo inventario puede estar en todos tus equipos sin cuenta
ni servicio: MAGI escribe un fichero cifrado por equipo en una carpeta que tú
eliges (Syncthing, Nextcloud, iCloud Drive, Dropbox, un repositorio git…) y lee
los de los demás. `magi.db` no se cifra (no guarda secretos y tiene permisos
600); lo que sale del equipo va cifrado con age (X25519) y firmado con
HMAC-SHA256, y la clave vive solo en el llavero del sistema.

### Activar la sincronización

1. Añade la carpeta a `config.toml` (o hazlo desde la vista SINCRONIZACIÓN):

   ```toml
   [sincronizacion]
   carpeta = "~/Sync/magi"   # sin carpeta, la sincronización está desactivada
   retraso_seg = 5           # espera tras el último cambio local antes de exportar
   copias = 10               # copias de magi.db que se conservan antes de importar
   umbral_borrado_hosts = 5  # más de N hosts borrados piden confirmación
   umbral_borrado_pct = 20   # o al menos el N % de los hosts
   ```

2. Abre MAGI, `Ctrl+P` → «sincronización» y pulsa `n` para elegir la carpeta.
3. Si la carpeta no tiene ficheros de MAGI, MAGI crea la clave y la muestra como
   texto y QR: guárdala en tu gestor de contraseñas y pulsa `s` para confirmar.
   Si ya tiene ficheros de otros equipos, pega la clave compartida (diálogo
   UNIR).
4. Repite en los demás equipos con la misma clave (o escanea el QR desde la app
   Android, F11).

### Añadir un equipo

En el equipo nuevo: vista SINCRONIZACIÓN → `n` → elegir la misma carpeta →
UNIR → pegar la clave. Los elementos con el mismo nombre se unen (gana la
versión más reciente y la otra queda como conflicto recuperable); el resto se
suma. La primera pasada importa todo.

### La clave

- Se muestra con `k` (texto y QR) y se puede rotar con `r`.
- Al quitar un equipo con `x`, MAGI borra su fichero y **cambia la clave**; los
  demás equipos pedirán la nueva la próxima vez.
- Si un equipo conocido deja de abrirse con tu clave, la sincronización se
  detiene hasta que introduzcas la clave nueva; no se pierde nada.
- Sin la clave no se puede leer ni fabricar un fichero, pero tus datos siguen en
  el `magi.db` de cada equipo: puedes desactivar (`d`) y crear otra
  sincronización.

### Cuándo sincroniza

- Al abrir MAGI, al cambiar la carpeta, unos segundos después de cada cambio
  local y al pulsar `s`; también con `magi sincronizar` (código 0 bien, 1
  error, 2 si hace falta decidir) y `magi sincronizar estado` para ver el estado
  sin descifrar nada.
- Solo un proceso sincroniza a la vez (cerrojo en
  `$XDG_RUNTIME_DIR/magi/sincronizacion.lock`).
- Antes de importar algo que cambia el inventario guarda una copia de `magi.db`
  en `~/.local/share/magi/copias/` y, si va a borrar muchos hosts, pide
  confirmación.
- Los conflictos (dos equipos editaron lo mismo a la vez, dos nombres iguales,
  huellas distintas, borrados con sesiones abiertas) se ven en la vista
  CONFLICTOS: `r` recupera tu versión, `a` acepta la del otro, `d` descarta.
- El indicador `⇅` de la barra inferior resume el estado: al día, conflictos,
  pausa, clave nueva, error o sincronizando (en ASCII, «sync ok», «sync N!»,
  «sync pausa», «sync clave», «sync error», «sync …»).

### Abrir un fichero fuera de MAGI

El fichero `magi-<uuid>.age` es un age estándar. Guarda la clave en un fichero
con permisos 600 y:

```sh
age -d -i clave.txt magi-<uuid>.age
```

La cabecera, la firma y el JSON están documentados en
[`docs/formato-sincronizacion.md`](docs/formato-sincronizacion.md) para la app
Android.

## Escritorio remoto

Además de hosts SSH, el inventario admite **hosts de escritorio**: `RDP`
(Windows, xrdp, GNOME Remote Desktop y KDE), `VNC` (QEMU/libvirt/Proxmox,
TigerVNC y x11vnc) y `voxl` (un host Linux con X11 que tenga `voxl-server`
instalado). Se crean como cualquier host, con grupo y etiquetas; el **tipo se
elige al crearlo y no se cambia** después. Los escritorios no salen en
`magi_config`, ni en el terminal, ni en SFTP, túneles, snippets o saltos: solo
los hosts SSH hacen esas cosas.

El escritorio se ve en una **ventana del navegador** (Chromium, Brave, Chrome o
Edge en modo aplicación), porque una TUI no pinta imágenes. La página la sirve
el propio `magi --servidor` **solo en `127.0.0.1`**, en un puerto libre, y se
entra con un enlace de un solo uso que caduca a los 60 s y se gasta al usarlo.
La clave del visor vive en el `sessionStorage` de la pestaña y la URL se
limpia al canjearla. El servidor comprueba `Host` siempre y `Origin` en
`/api/*`, y sirve una CSP estricta sin recursos externos.

En RDP, la imagen llega al visor como **rectángulos** (mapas de bits y RemoteFX
decodificados por IronRDP) y el cursor se pinta aparte. El H.264 de Windows
(AVC420) todavía no se anuncia: exige decodificar también los demás códecs del
canal EGFX, que es lo que manda Windows para la interfaz; está anotado como
pendiente en el informe de la Fase 12.

### Abrir un escritorio

`↵` sobre un escritorio lo abre; si ya tenía sesión, va a su pestaña y abre el
visor con un enlace nuevo. La sesión la custodia `magi --servidor`: sobrevive a
cerrar la TUI y se ve desde todas las ventanas. Mientras no haya ningún visor,
la sesión se cierra sola al cabo de `[escritorio] gracia_seg` (60 s por
defecto); se puede volver a abrir el visor desde la pestaña antes de que expire.
Un segundo visor sustituye al primero.

Siempre que se puede, la conexión va **por SSH**: se toma la conexión del host
«a través de» y se abre un canal `direct-tcpip` hasta el servidor de
escritorio, sin abrir puertos en tu equipo. Un VNC que vaya directo a una
dirección que no es de loopback avisa de que la imagen y el teclado viajarán sin
cifrar. RDP directo va cifrado con TLS.

### Certificado y credenciales

En RDP, antes de mandar ninguna credencial se comprueba el certificado del
servidor con la misma política que las huellas SSH: si es la primera vez, se
pide aceptarlo (y se guarda en `HOSTS.huella_escritorio`, propia de ese
equipo); si cambia, el aviso es rojo y hay que escribir el nombre del host para
sustituirlo. Las contraseñas de RDP y VNC viven solo en el llavero del sistema;
el servidor de sesiones nunca lo toca: se las pide al cliente que abrió el
escritorio.

### voxl

`voxl` se arranca **en el host SSH** con un script embebido y una contraseña
aleatoria por sesión que viaja por la entrada estándar, nunca en la línea de
órdenes. Escucha solo en `127.0.0.1` del host y se llega por un túnel de la
sesión. Al cerrar la sesión (o si se cae la conexión SSH) el script para voxl y
lo mata si no termina. MAGI no instala voxl: si falta, lo dice.

### Configuración

```toml
[escritorio]
# auto o una orden con {url}, por ejemplo "brave --app={url}"
navegador = "auto"
# segundos sin visor antes de cerrar la sesión (10-3600)
gracia_seg = 60
# puerto del servidor del visor; 0 = uno libre
puerto_visor = 0
# auto (sale de LANG) o una distribución como es-ES
distribucion_teclado = "auto"
# reservado para el H.264 de Windows (AVC420); hoy RDP va por rectángulos
h264 = true
```

`magi diagnostico` dice qué navegador abrirá el visor y avisa si no encuentra
ninguno. El portapapeles es solo de texto y no se guarda: ni la imagen, ni las
teclas, ni el portapapeles pasan por el log ni por el registro.

## Servicios, contenedores y logs

Desde la Fase 13 MAGI maneja lo que corre en cada host desde la vista
**Servicios** (`F9`; también `u` desde Flota, Hosts y la ficha, con ese host
elegido). A la izquierda, los hosts SSH con el estado de su último sondeo y
`⚠N` de fallos; a la derecha, tres secciones que se cambian con `1`
**Servicios**, `2` **Timers** y `3` **Contenedores**, con su recuento. Arriba
salen los servicios vigilados (`★`) y los fallidos, separados del resto por una
línea; `/` filtra por nombre y `Tab` alterna hosts y lista en modo estrecho.
En «Toda la flota» se juntan los fallos del último sondeo de todos los hosts,
sin abrir ninguna conexión: `↵` va a ese host con el elemento elegido y `r`
sondea todos. Flota marca `⚠N` en la fila del host y su detalle enseña el
bloque FALLOS.

`↵` abre el detalle: estado y subestado desde cuándo, arranque, PID principal,
memoria, fichero de la unidad, si admite recargar, qué comprobación de
configuración se aplicará y sus dependencias («necesita» y «la necesitan»), o
el texto de `systemctl cat`; en un contenedor, id, imagen, creado, estado y
código de salida, reinicios, salud, puertos, montajes, redes, proyecto Compose
y el texto de `inspect`; en un timer, su próxima y última ejecución y el
servicio que activa. `l` abre los logs en vivo, con filtro por texto (`/`) y
por nivel (`n`), pausa (`Espacio`) sin cortar la llegada de líneas, un anillo
de 10 000 líneas y `t` para el seguimiento completo en una pestaña. Al cerrar
el panel se corta el `journalctl` de MAGI en el host.

Por ejemplo, en `web-01` (`192.0.2.10`, que sirve `web.ejemplo.org` con
Nginx delante de una aplicación systemd): `F9`, el host arriba con su `⚠N`,
`R` sobre `app.service`, deliberación, contraseña de sudo si hace falta y el
resultado con las últimas líneas del diario si no vuelve; `l` para ver los
logs y `3` para los contenedores de su proyecto Compose.

**Acciones.** `A` arrancar (en un timer, lanzar ahora su servicio, con
confirmación), `P` parar, `R` reiniciar, `G` recargar y `H` activar o
desactivar el arranque, ofrecidas solo cuando valen en el estado de la unidad:
`G` en un servicio sin `CanReload` avisa «nginx.service no admite recargar».
En un contenedor, `A` `P` `R` según su estado y `S` abre una shell en una
pestaña nueva; sobre la cabecera de un proyecto Compose, `A` `P` y `R`
actúan sobre el proyecto entero con `-p` y los ficheros de sus etiquetas, y
sin ficheros conocidos no se ofrecen y se dice por qué. Tras una acción, MAGI
espera al estado final hasta `espera_seg` (30 s por defecto) y vuelve a mirar a
los 2 s; si la unidad no vuelve a `active`, el resultado es «fallido» con sus
últimas 20 líneas del diario, y si se agota el plazo, «sin confirmar» con la
orden todavía en el host. Una sola acción a la vez por objetivo: otra sobre el
mismo objetivo se rechaza con «hay otra acción en curso».

**Deliberación y confirmación.** Lo que deja algo parado —parar, reiniciar o
desactivar un servicio, y parar o reiniciar un contenedor o un proyecto— pasa
por la deliberación MAGI (F6) con las comprobaciones del host y
`PlanDeliberado::Servicio`; MELCHIOR-1 no cuenta el estado del propio objetivo,
porque reiniciar lo que falla es justo lo que se quiere hacer. Arrancar,
recargar, activar el arranque y lanzar un timer piden una confirmación simple.
Ninguna acción se hace con una pulsación: las teclas que cambian algo en el host
van en mayúsculas. Las unidades críticas (`[servicios] criticas`: sshd, la red,
el cortafuegos, Tailscale, Docker…) avisan en rojo de que la acción «puede
cortar tu conexión» con el host y con los hosts que van a través de él.

**sudo.** Las órdenes privilegiadas van una a una con `sudo`, nunca con
`sudo sh`, y con `timeout` por fuera de sudo: así basta un sudoers limitado y
el registro del host (`journalctl _COMM=sudo` o `auth.log`) enseña la orden
real. Si el usuario es root o `sudo -n` vale, MAGI no pregunta nada; si hace
falta contraseña, la pide en un diálogo, la comprueba con `ComprobarSudo` antes
de confirmar o deliberar y la recuerda solo en memoria (`Zeroizing`) hasta
`sudo_memoria_min` minutos sin uso. Viaja al host por la primera línea de la
entrada estándar, nunca en la línea de órdenes, y no va a disco, al llavero, al
log ni a REGISTRO. `!` pide la contraseña de sudo para el host elegido, para
ver lo que exige permisos; un usuario sin sudo ve «usuario no puede usar sudo
en <host>» y no se le ofrecen las acciones que lo necesitan. Con Podman nunca se
usa sudo: se ven los contenedores del usuario SSH.

**Comprobación de configuración.** Antes de recargar o reiniciar Nginx
(`nginx -t`), Caddy (`caddy validate`), Apache (`apachectl configtest`),
HAProxy (`haproxy -c`) o un proyecto Compose (`compose config -q`), y antes
de aplicar la comprobación propia de un servicio o de un proyecto
(COMPROBACIONES_SERVICIO), MAGI valida la configuración dentro de la propia
acción, en el mismo canal y justo antes: si falla, devuelve 127 (no encontrada)
o pasa de 20 s, no se ejecuta nada y el resultado dice «rechazada por la
configuración» con la salida del validador. Las comprobaciones propias se crean,
editan y borran en la ficha del host y con `c` sobre el servicio o el proyecto;
borrar pide confirmación.

**Permisos que hacen falta en el host.** MAGI usa lo que el host ya tiene y no
instala nada:

- systemd 230 o posterior, con `systemctl` y `journalctl`. Para leer el diario
  del sistema, el usuario SSH debe estar en `systemd-journal`, `adm` o
  `wheel`, o usar sudo. Un host sin systemd se ve como «sin systemd», con solo
  sus contenedores, y uno con systemd anterior a 230, como «systemd demasiado
  antiguo».
- Docker 20.10 o posterior con Compose v2 (`docker compose`): el socket exige
  el grupo `docker` o sudo. Sin permiso, la sección lo dice («añade el usuario
  al grupo docker o pulsa `!`»).
- Podman 4 o posterior, sin root: se ven los contenedores del usuario SSH y con
  Podman nunca se usa sudo. Compose v2 también con `podman compose`.
- `sudo` 1.8 o posterior, `timeout` (coreutils), `awk` y, para los logs de
  contenedor, `mktemp` y `mkfifo` (coreutils/util-linux). Un sudoers limitado
  a las órdenes que MAGI ejecuta basta, por ejemplo:

```sudoers
# /etc/sudoers.d/magi — sudo orden a orden, nunca «sudo sh»
Cmnd_Alias MAGI_SERVICIOS = /usr/bin/systemctl, /usr/bin/journalctl, \
    /usr/bin/docker, /usr/bin/nginx, /usr/bin/apachectl
usuario ALL=(root) NOPASSWD: MAGI_SERVICIOS
```

Con `NOPASSWD`, MAGI detecta `sudo_sin_clave` y no pide contraseña; sin él, la
pide y la comprueba con `ComprobarSudo`. Los hosts sin Docker ni Podman lo
dicen en la sección Contenedores; con los dos, se usa el primero de
`[servicios] contenedores` y la cabecera dice cuál.

```toml
[servicios]
# refresco de la vista abierta en segundos (5-300; 0 = solo al entrar y con r)
refresco_seg = 10
# espera máxima al estado final de una acción en segundos (5-300)
espera_seg = 30
# líneas del anillo del panel de logs (1 000-100 000)
lineas_log = 10000
# líneas anteriores al abrir los logs (0-5 000)
lineas_iniciales = 200
# motores de contenedores por orden de preferencia ([] desactiva)
contenedores = ["docker", "podman"]
# unidades que pueden cortar la conexión con el host (admite * como patrón)
criticas = ["sshd.service", "ssh.service", "networking.service",
  "NetworkManager.service", "systemd-networkd.service",
  "systemd-resolved.service", "firewalld.service", "ufw.service",
  "nftables.service", "iptables.service", "tailscaled.service",
  "wg-quick@*.service", "docker.service", "containerd.service",
  "podman.service"]
# minutos sin uso tras los que se olvida la contraseña de sudo (0-15; 0 = pedirla siempre)
sudo_memoria_min = 5
```

El fichero de sincronización sube a `formato` 3 en cuanto hay alguna
comprobación de servicio propia: los equipos con MAGI 1.2 lo verán como
«formato nuevo» hasta que se actualicen. La app Android ya lee el formato 3 y
guarda las comprobaciones en su copia sin usarlas; la vista de servicios del
móvil llega en la Fase 14.

## App Android

La app compañera para Android 11 o superior vive en [`android/`](android/) y se
publica aparte del escritorio, con etiquetas `android-vX.Y.Z`: es **lectora**
de la sincronización (el inventario llega del escritorio; el móvil nunca escribe
en la carpeta) y permite conectar a los hosts, abrir el terminal y usar túneles.

Desde la Fase 14 es también el **móvil de guardia**: Flota es su pantalla de
inicio, con el sondeo del escritorio (estados, métricas, vigilados y fallos) y
refresco solo con la pantalla a la vista; Servicios maneja los servicios,
timers y contenedores del host como en el escritorio, con acciones, sudo en
memoria y logs en vivo; Snippets ejecuta los sincronizados en uno o varios
hosts con variables, resultados y deliberación MAGI (lo deliberado se ejecuta
manteniendo pulsado 1 s); y Registro enseña lo que ha hecho el móvil, con
filtros y el detalle de cada deliberación. Toda la lógica (sondeo, acciones,
lectura de salidas, snippets y deliberación) vive en el núcleo compartido; la
app solo pinta y no decide qué se permite. Las acciones y las ejecuciones
siguen en segundo plano con el servicio en primer plano; el sondeo, los
refrescos y los logs solo corren con su pantalla a la vista. Funciona sin
Google Play Services, sin cuenta y sin telemetría.

Se instala y actualiza con [Obtainium](https://github.com/ImranR98/Obtainium)
desde las [releases](https://github.com/4d3-org/magi/releases) del repositorio:
añade la app con la URL del repositorio y filtra las etiquetas por `android-v`
y los APK por `magi-android-`. Las releases de Android **nunca** se marcan como
«Latest» (esa marca es del escritorio), así que en la lista busca la etiqueta
`android-v…`; si quieres probar candidatas, activa las prereleases (`-rc.N`).

Antes de instalar o actualizar, comprueba el APK y su certificado:

```sh
sha256sum -c magi-android-v0.2.0.apk.sha256
apksigner verify --print-certs magi-android-v0.2.0.apk
gh attestation verify magi-android-v0.2.0.apk --repo 4d3-org/magi
```

`apksigner` viene con las build-tools del SDK de Android y `gh` es el GitHub
CLI. La huella SHA-256 que imprime `apksigner` debe coincidir con la de las
notas de la release y con la de las versiones anteriores de la app: si cambia,
no instales el APK y avisa. La guía completa (keystore, secretos, etiquetas y
qué hace el CI) está en
[`docs/publicar-android.md`](docs/publicar-android.md).

En el móvil, la carpeta de sincronización debe compartirse con Syncthing-Fork en
modo **«solo recibir»**: así el teléfono nunca escribe en la carpeta y no puede
pisar los datos de los escritorios.

Si pierdes el móvil, o lo reinstalas: cambia la clave de sincronización en el
escritorio (`r` en la vista SINCRONIZACIÓN), quita el teléfono de Syncthing y
borra de `~/.ssh/authorized_keys` la línea de la clave del móvil (identifícala
con la huella que mostraba Ajustes → Clave del móvil). Las contraseñas y la
clave del móvil estaban cifradas con el Keystore y protegidas por el bloqueo del
teléfono; si hay sospecha, cambia también las contraseñas de los hosts. Al
instalar de nuevo la app hay que crear otra clave del móvil, volver a unirse a
la sincronización y reaceptar las huellas.

## Atajos

Globales: `F1` Flota · `F2` Hosts · `F3` pestaña activa o lista de sesiones ·
`F4` Archivos · `F5` Identidades · `F6` Túneles · `F7` Registro · `F8`
Snippets · `F9` Servicios · `Ctrl+P` paleta · `?` ayuda · `Esc` cierra
diálogos y filtros · `q` vuelve o sale.

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
borrar · `E` editar · `p` permisos · `S` sincronizar · `L` sincronizaciones
guardadas · `r` renombrar · `d` crear directorio · `.` ocultos · `/` filtro ·
`R` refrescar · `g` ir a ruta · `i` detalle · `h` cambiar de host · `t` cola ·
`Esc` limpiar filtro o marcas · `q` volver.

PERMISOS: `↑` `↓` `←` `→` rejilla · `Espacio` marcar · `Tab` / `Shift+Tab`
octal, recursivo y alcance · `Ctrl+S` aplicar · `Esc` cancelar.

SINCRONIZAR: `Tab` / `Shift+Tab` campos · `Espacio` casillas · `↵` planificar ·
`Esc` cancelar. Vista previa: `↑` `↓` `PgUp` `PgDn` recorrer · `f` filtrar por
tipo de cambio · `↵` continuar · `Esc` cancelar. Guardadas (`L`): `↵`
ejecutar · `n` nueva · `e` editar · `x` borrar · `q` volver.

Transferencias: `↑` `↓` mover · `x` cancelar · `C` limpiar terminadas · `↵`
detalle · `q` volver.

Túneles: `↑` `↓` / `j` `k` mover · `Espacio` activar, parar o descartar el
error · `n` nuevo · `e` editar · `x` borrar · `a` automático · `r` relanzar ·
`↵` detalle · `/` filtro · `Esc` limpiar el filtro o volver · `q` volver.

Snippets: `↑` `↓` / `j` `k` mover · `↵` ejecutar (diálogo) · `a` en todos los
destinos · `p` en pestañas · `n` nuevo · `e` editar · `x` borrar · `/`
filtro · `t` Resultados · `i` detalle (con la vista baja) · `q` volver. En el
formulario, `Ctrl+S` guarda.

Servicios: `↑` `↓` / `j` `k` mover · `Tab` hosts ⇄ lista (en el detalle,
Estado · Dependencias · Unidad) · `1` `2` `3` sección · `/` filtro · `↵`
detalle · `l` logs · `A` `P` `R` `G` `H` acciones · `S` shell del
contenedor · `c` comprobación propia · `!` contraseña de sudo · `r` refrescar
· `Esc` volver. En el panel de logs: `/` texto · `n` nivel · `Espacio` pausa ·
`Fin` al final · `t` pestaña. En «Toda la flota», `↵` va al host y `r`
sondea todos.

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
editor = ""       # vacío usa $VISUAL, $EDITOR, nvim y vi (programa y argumentos, sin shell)
excluir = [".git/", "target/", "node_modules/", "__pycache__/", ".DS_Store", ".magiignore"]
editar_max_mb = 10   # por encima, E pide confirmación antes de bajar el fichero
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
- Un temporal de edición nunca se borra mientras tenga trabajo sin poner a
  salvo; el editor se lanza como programa y argumentos, sin shell. Sobrescribir
  un fichero remoto es atómico (`posix-rename@openssh.com`, o en tres pasos sin
  perder nunca el original) y conserva sus permisos; si el propietario va a
  cambiar, se avisa antes.
- Una sincronización solo borra en el destino rutas fijadas en el plan, nunca
  excluidas, solo tras copiar todo sin errores y siempre con deliberación MAGI.
  `chmod` nunca sigue enlaces y el recursivo pide confirmación con el recuento.
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
TZ=UTC cargo test
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
ve una segunda ventana. Desde la Fase 8 también sobrescribir un fichero
existente (posix-rename y los tres pasos), los temporales de edición, `StatRemoto`,
`CambiarPermisos` con sus alcances, `ListarArbol` por bloques con exclusiones y
`.magiignore`, los nombres de propietario, los permisos tras sobrescribir, el
borrado al terminar solo con éxito y las anotaciones de sincronización y
deliberación. Si el sistema no trae `sftp-server`, esas pruebas se saltan con
un aviso en lugar de fallar. El resto (marcas, avisos de sensibles,
paneles, orden y fechas) son pruebas unitarias puras.

Las pruebas de servicios no usan red externa: el servidor SSH de prueba con
`systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos, y
salidas grabadas de Debian 12, Ubuntu 24.04, Rocky 9 y Arch con Docker 27 y
Podman 5. Cubren el núcleo y el script (`servicios_lectura`,
`servicios_script`), la sincronización de las comprobaciones
(`sincronizacion_comprobaciones`), el servidor (`servicios_servidor`) y la
interfaz con sus instantáneas (`servicios_ui`, `redimensionado`).

## Autocompletado y man en macOS

La fórmula de Homebrew instala el binario y, si la versión lo incluye, el
autocompletado y la página man. Si no aparecen, genéralos con el propio binario:

```sh
# zsh (el shell por defecto de macOS)
magi autocompletado zsh > "$(brew --prefix)/share/zsh/site-functions/_magi"
# bash
magi autocompletado bash > "$(brew --prefix)/etc/bash_completion.d/magi"
# fish
magi autocompletado fish > "$(brew --prefix)/share/fish/vendor_completions.d/magi.fish"
# página man
magi man > "$(brew --prefix)/share/man/man1/magi.1"
```

En Linux, el paquete de AUR ya coloca esos ficheros en las rutas del sistema; si
usas el tarball, instálalos con `sudo install -Dm644` en
`/usr/share/bash-completion/completions/magi`,
`/usr/share/zsh/site-functions/_magi`,
`/usr/share/fish/vendor_completions.d/magi.fish` y
`/usr/share/man/man1/magi.1`.

## Desinstalación

```sh
sudo pacman -Rns magi-ssh-bin   # Omarchy y Arch
brew uninstall magi-ssh         # macOS
```

Con el tarball, borra el binario (`sudo rm /usr/local/bin/magi`). Desinstalar
MAGI **no borra los datos del usuario**; quedan en:

| Qué | Dónde |
|---|---|
| Inventario | `~/.local/share/magi/` (`magi.db` y sus ficheros WAL) |
| Configuración | `~/.config/magi/config.toml` |
| Estado y logs | `~/.local/state/magi/` |
| `magi_config` | `~/.ssh/magi_config` |
| `Include` | la línea `Include ~/.ssh/magi_config` de `~/.ssh/config` |
| Socket y lock | `$XDG_RUNTIME_DIR/magi/` (se borran al parar el servidor) |
| Copias de seguridad | `~/.ssh/config.bak-<fecha>` y `known_hosts.old` |
| Contraseñas recordadas | el llavero del sistema (`secret-tool`/Keychain) |

Para empezar de cero: cierra MAGI, para el servidor (`magi servidor parar`),
borra esas rutas y quita la línea `Include`. MAGI nunca borra claves de `~/.ssh`
ni entradas de `known_hosts`; si quieres quitar las que aceptaste, usa
`ssh-keygen -R <host>`.

## Licencia

MAGI es software libre bajo doble licencia **MIT OR Apache-2.0**, a elección de
quien lo use:

- [LICENSE-MIT](LICENSE-MIT)
- [LICENSE-APACHE](LICENSE-APACHE)

Salvo que se indique lo contrario, cualquier contribución enviada al proyecto se
licencia del mismo modo, siguiendo la convención de Rust.

Copyright (c) 2026 Cuatro de Tres S. Coop. And.
