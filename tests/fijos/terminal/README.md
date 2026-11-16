# Apoyos de las pruebas del terminal (Fase 15, ampliado en la Fase 16)

Este directorio guarda ficheros que usan las pruebas del terminal del
escritorio y del núcleo. No forma parte del binario ni se instala: solo lo leen
las pruebas.

Contenido:

- `tmux`: un tmux falso, en `sh` POSIX, para probar `terminal_tmux.sh` y la
  sesión persistente sin tmux de verdad.
- `tui_prueba.rs`: programa de prueba que habla como una TUI moderna (lo usa
  `tests/terminal_e2e.rs`); este README no lo documenta.

Las pruebas con tmux de verdad (`crates/magi-nucleo/tests/tmux_real.rs`) no
usan este falso.

## `tmux`: tmux falso

Responde a las órdenes que usan `magi_nucleo::terminal::tmux` y
`crates/magi-nucleo/scripts/terminal_tmux.sh`. Nunca invoca al tmux real, ni
toca sesiones del usuario, ni abre red: solo lee y escribe los ficheros de
estado de la prueba.

Necesita `sh` POSIX, `awk`, `sed`, `cat`, `date` y `mkdir`; no necesita tmux
instalado. Pasa `shellcheck` y `sh -n`.

### Variables de entorno

| Variable | Obligatoria | Qué hace |
|---|---|---|
| `MAGI_TMUX_ESTADO` | sí, salvo en `tmux -V` | Ruta del **fichero** de estado del tmux **habitual** (sin `-L`). Si no existe o está vacío, no hay sesiones. |
| `MAGI_TMUX_ESTADO_MAGI` | no | Ruta del estado del servidor propio (`-L magi`). Por defecto, `MAGI_TMUX_ESTADO` con el sufijo `.magi`. Cualquier otro `-L nombre` usa `MAGI_TMUX_ESTADO.nombre`. |
| `MAGI_TMUX_LLAMADAS` | no | Fichero donde se anota cada invocación: una línea con los argumentos separados por espacios (`$*`). Sirve para comprobar qué orden se ejecutó y en qué orden. |
| `MAGI_TMUX_ARGUMENTOS` | no | Fichero donde se anota cada invocación con los argumentos **sueltos**: una línea con cada argumento seguido de `0x1F`. Sirve para comprobar que un dato va como un solo argumento (los argumentos con saltos de línea no se pueden distinguir). |
| `MAGI_TMUX_ENTORNO` | no | Fichero donde se anota, por invocación, `TMUX=<valor>`: lo que veía tmux en esa variable. |
| `MAGI_TMUX_VERSION` | no | Lo que imprime `tmux -V`. Por defecto `tmux 3.4`. Admite `tmux 3.3a`, `tmux next-3.6`, `tmux master`, `tmux 2.9a`… |
| `MAGI_TMUX_FALLA` | no | Órdenes de tmux que fallan con código 1 (separadas por espacios): `new-session`, `attach-session`, `rename-session` o `set`. |
| `MAGI_TMUX_ATTACH_MATA` | no | Con `1`, al acabar `attach-session` (fin de la entrada) la sesión desaparece, como si su programa hubiera terminado. |
| `MAGI_TMUX_ATTACH_ESTADO` | no | Código de salida de `attach-session` (por defecto 0), para simular un cliente que no puede engancharse. |

### Formato del estado

Una línea por sesión, con hasta nueve campos separados por **tabuladores**:

```
nombre  adheridos  ventanas  creada  [equipo  [actividad  [campana  [titulo  [creada_epoch]]]]]
```

- `nombre`: nombre de la sesión tal cual (la prueba lo siembra; los nombres
  que genera MAGI cumplen `^[A-Za-z0-9_-]{1,64}$`; para probar los demás, se
  pueden sembrar con espacios o puntos).
- `adheridos`: número de clientes enganchados; es `#{session_attached}`.
- `ventanas`: número de ventanas; es `#{session_windows}`.
- `creada`: descripción de cuándo se creó; es `#{session_created_string}`.
- `equipo`: lo que `#{@magi_equipo}` devuelve (la opción de sesión de MAGI).
- `actividad`: segundos Unix de la actividad más reciente; es
  `#{session_activity}` y, en `list-windows`, `#{window_activity}` de la
  última ventana (las anteriores, 10 s menos cada una).
- `campana`: `1` si la sesión tiene la campana; sale como
  `#{window_bell_flag}` solo en la última ventana.
- `titulo`: es `#{pane_title}` (puede llevar caracteres de control).
- `creada_epoch`: segundos Unix de la creación; es `#{session_created}`.

Los campos que faltan valen vacío o 0. Los cuatro primeros son los de la
Fase 15, así que una prueba antigua sigue valiendo. Ejemplo (dos sesiones, la
primera con un cliente, en el servidor propio):

```sh
printf 'agente\t1\t3\tMon Nov  9 19:20:00 2026\tportatil\t1794845343\t1\tclaude\t1794845000\nreposo\t0\t1\tx\n' > "$estado_magi"
```

Una sesión creada con `new-session` se añade con `0` adheridos, `1` ventana,
la cadena `2026-11-09 19:00:00`, sin equipo, sin campana y con la hora de
ahora como actividad y como creación.

Opcionalmente, el contenido del panel de una sesión (lo que devuelve
`capture-pane`) se siembra en `<fichero de estado>.pantalla.<nombre>`. Sin él,
el falso devuelve tres líneas con espacios al final y líneas vacías detrás.

### Opciones globales y secuencias

- `-L <nombre>` elige el servidor (ver arriba); `-f`, `-S`, `-c` y `-T` se
  saltan con su valor; `-V` imprime la versión y sale; el `-u` que el script
  pone siempre (tmux en modo UTF-8) y cualquier otra bandera suelta se ignoran.
- Varias órdenes en una invocación, separadas por un argumento `;`, como
  `start-server ; set -gq status off ; new-session -d -s x`. Se ejecutan en
  orden y, si una falla, sale con su código sin ejecutar las siguientes.
- Límite del falso: solo conoce el `;` suelto como separador. El tmux de verdad
  también toma como fin de la orden un argumento que **acaba** en `;` (y se lo
  quita; con `\;` deja solo el `;`): lo que se pasa al falso no lo reproduce, así
  que `tmux_script` comprueba en la bitácora de argumentos que ninguno acaba
  en `;` y `tmux_real` lo comprueba con tmux de verdad.

### Órdenes que entiende

| Orden | Salida | Código | Efecto |
|---|---|---|---|
| `tmux -V` | `MAGI_TMUX_VERSION` | 0 | Ninguno; no hace falta estado. |
| `start-server`, `kill-server` | — | 0 | `kill-server` vacía el estado. |
| `set` / `set-option [flags] [-t destino] opción valor` | — | 0 | Solo hace algo con `@magi_equipo` y `-t`: pone el equipo de la sesión. El resto solo queda en las bitácoras. Falla con `MAGI_TMUX_FALLA=set`. |
| `has-session -t <destino>` | — | 0 si existe, 1 si no | Ninguno. Con las reglas de destino de abajo. |
| `list-sessions [-F <formato>]` | Una línea por sesión, sustituyendo `#{session_name}`, `#{session_attached}`, `#{session_windows}`, `#{session_created_string}`, `#{session_created}`, `#{session_activity}`, `#{@magi_equipo}` y `#{pane_title}`. Sin formato: `nombre: N windows (created …)`. Sin sesiones: error por stderr y código 1 (como un tmux sin servidor). | 0/1 | Ninguno. |
| `list-windows -a [-F <formato>]` | Una línea por ventana (tantas como diga `ventanas`) con `#{session_name}`, `#{window_index}`, `#{window_activity}` y `#{window_bell_flag}`. | 0/1 | Ninguno. |
| `new-session [-d] [-A] -s <nombre> [-x n] [-y n] [-c carpeta] [-- programa…]` | Sin `-d` (o con `-A`): `MAGI_TMUX_SESION=<nombre>` y, por cada línea de la entrada, `MAGI_TMUX_ENTRADA=<línea>` hasta el fin de la entrada (simula el PTY). | 0 | Crea la sesión si no existe. Si existe y no hay `-A`: `duplicate session` y código 1. El programa y la carpeta solo quedan en la bitácora de argumentos. Falla con `MAGI_TMUX_FALLA=new-session`. |
| `attach-session [-d] -t <destino>` | `MAGI_TMUX_ADJUNTO=<nombre> soltar=<0\|1>` y las líneas de la entrada como `MAGI_TMUX_ENTRADA=<línea>`, hasta el fin de la entrada. | `MAGI_TMUX_ATTACH_ESTADO` (0) | Sin sesión: `can't find session` y código 1. Con `MAGI_TMUX_ATTACH_MATA=1`, quita la sesión al acabar. |
| `kill-session -t <destino>` | — | 0 si existía (la borra), 1 si no | Quita su línea del estado. |
| `rename-session -t <destino> [--] <nuevo>` | — | 0 / 1 | Cambia el nombre; si ya hay otra con el nombre nuevo, `duplicate session` y código 1. |
| `capture-pane [-p] [-e] [-J] [-S n] -t <nombre>:` | El contenido del panel (ver arriba). Ignora `-e`, `-J` y `-S`. | 0 / 1 | Como el tmux real, **exige** el destino con dos puntos (`=nombre:`): con `-t =nombre` da `can't find pane`. |
| Otra orden | Error por stderr | 2 | — |

**Destinos de sesión**, como los resuelve tmux: `=nombre` solo casa por
nombre exacto; sin `=`, casa el nombre exacto o, si no lo hay, un prefijo
único (por eso `kill-session -t magi-a-1` mataría a `magi-a-10`: ese es el
fallo que cubre R146). Lo que sigue a `:` se ignora.

En `list-sessions`, el formato se sustituye literalmente: el separador que
ponga la prueba se conserva (el script usa tabuladores; con 0x1F, tmux 3.4 y
3.5 escribirían `\037`, pero el falso no lo hace).

### Cómo usarlo desde las pruebas

La prueba debe ejecutar el script o el binario con el directorio del tmux falso
en el `PATH` **del hijo** (nunca en el `PATH` global del proceso de pruebas) y
con `MAGI_TMUX_ESTADO` apuntando a un fichero de su temporal:

```sh
PATH="$directorio_falso:$PATH" MAGI_TMUX_ESTADO="$temporal/estado" \
    sh -c "$SCRIPT" magi-tmux estado
```

Desde Rust se puede:

- Copiar este fichero a un directorio temporal con permisos 0755 y poner ese
  directorio en el `PATH` del hijo, como hace
  `crates/magi-nucleo/tests/tmux_script.rs` con `include_str!`. Es lo más
  aislado y no depende de los permisos del árbol de trabajo. Se escribe una
  sola vez para todas las pruebas: reescribirlo mientras otra lo ejecuta da
  `ETXTBSY`.
- Poner `tests/fijos/terminal` directamente en el `PATH` del hijo (el fichero
  va versionado con permisos de ejecución).

En los dos casos, fijar `MAGI_TMUX_ESTADO` (y, si se quiere,
`MAGI_TMUX_ESTADO_MAGI`, `MAGI_TMUX_LLAMADAS` y `MAGI_TMUX_ARGUMENTOS`) a
ficheros del temporal de la prueba y sembrarlos antes de cada escenario. Las
pruebas del núcleo que lo usan son `cargo test -p magi-nucleo --test
tmux_script`; las del escritorio, las de `tests/terminal_servidor.rs` y
`tests/terminal_e2e.rs`.

### Comprobación

```sh
shellcheck tests/fijos/terminal/tmux
sh -n tests/fijos/terminal/tmux
```
