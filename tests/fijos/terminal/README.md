# Apoyos de las pruebas del terminal (Fase 15)

Este directorio guarda ficheros que usan las pruebas del terminal del
escritorio y del núcleo. No forma parte del binario ni se instala: solo lo leen
las pruebas.

Contenido:

- `tmux`: un tmux falso, en `sh` POSIX, para probar la sesión persistente.
- `tui_prueba.rs`: programa de prueba que habla como una TUI moderna (lo usa
  `tests/terminal_e2e.rs`); este README no lo documenta.

## `tmux`: tmux falso

Responde a las órdenes que usan `magi_nucleo::terminal::tmux` y
`crates/magi-nucleo/scripts/terminal_tmux.sh`. Nunca invoca al tmux real, ni
toca sesiones del usuario, ni abre red: solo lee y escribe el fichero de estado
de la prueba.

Necesita `sh` POSIX, `awk`, `sed` y `mkdir`; no necesita tmux instalado.
Pasa `shellcheck` y `sh -n`.

### Variables de entorno

| Variable | Obligatoria | Qué hace |
|---|---|---|
| `MAGI_TMUX_ESTADO` | sí, salvo en `tmux -V` | Ruta del **fichero** de estado. Si no existe o está vacío, no hay sesiones. |
| `MAGI_TMUX_LLAMADAS` | no | Ruta de un fichero donde se anota cada invocación (una línea con los argumentos). Útil para comprobar qué orden se ejecutó. |

### Formato del estado

Una línea por sesión, con cuatro campos separados por **tabuladores**:

```
nombre<TAB>adheridos<TAB>ventanas<TAB>creada
```

- `nombre`: nombre de la sesión tal cual (la prueba lo siembra; los nombres
  que genera MAGI cumplen `^[A-Za-z0-9_-]{1,64}$`).
- `adheridos`: número de clientes enganchados (0 o más); es lo que devuelve
  `#{session_attached}`.
- `ventanas`: número de ventanas (lo devuelve `#{session_windows}`).
- `creada`: descripción de cuándo se creó (la devuelve
  `#{session_created_string}`); pueden ser espacios.

Ejemplo (dos sesiones, la primera con un cliente):

```sh
estado=$temporal/estado
printf 'magi-portatil-1\t1\t2\tMon Nov  9 19:20:00 2026\nmagi-portatil-2\t0\t1\tTue Nov 10 08:00:00 2026\n' > "$estado"
```

Una sesión creada con `new-session` se añade con `0` adheridos, `1` ventana
y la cadena `2026-11-09 19:00:00` como fecha.

### Órdenes que entiende

| Orden | Salida | Código | Efecto |
|---|---|---|---|
| `tmux -V` | `tmux 3.4` | 0 | Ninguno; no hace falta estado. |
| `tmux has-session -t <nombre>` | — | 0 si existe, 1 si no | Ninguno. |
| `tmux list-sessions -F '<formato>'` | Una línea por sesión, sustituyendo `#{session_name}`, `#{session_attached}`, `#{session_windows}` y `#{session_created_string}` en el formato. Sin sesiones: error por stderr y código 1 (como un tmux sin servidor). | 0/1 | Ninguno. |
| `tmux new-session -A -s <nombre>` | `MAGI_TMUX_SESION=<nombre>` y, por cada línea que llegue por la entrada estándar, `MAGI_TMUX_ENTRADA=<línea>` hasta el fin de la entrada. | 0 | Crea la sesión si no existe y se queda «enganchado» leyendo la entrada (simula el PTY). |
| `tmux kill-session -t <nombre>` | — | 0 si existía (la borra), 1 si no | Quita su línea del estado. |
| Otra orden | Error por stderr | 2 | — |

En `list-sessions`, el formato se sustituye literalmente: el separador que
ponga la prueba (MAGI usa `|`) se conserva. Los valores del estado no deben
llevar saltos de línea; `@`, `&` y `\\` se escapan antes de sustituir.

### Cómo usarlo desde las pruebas

La prueba debe ejecutar el script o el binario con el directorio del tmux falso
en el `PATH` **del hijo** (nunca en el `PATH` global del proceso de pruebas) y
con `MAGI_TMUX_ESTADO` apuntando a un fichero de su temporal:

```sh
PATH="$directorio_falso:$PATH" MAGI_TMUX_ESTADO="$temporal/estado" \
    sh -c "$SCRIPT" magi-tmux listar
```

Desde Rust se puede:

- Copiar este fichero a un directorio temporal con permisos 0755 y poner ese
  directorio en el `PATH` del hijo, como hace
  `crates/magi-nucleo/tests/tmux_script.rs` con `include_str!`. Es lo más
  aislado y no depende de los permisos del árbol de trabajo.
- Poner `tests/fijos/terminal` directamente en el `PATH` del hijo (el fichero
  va versionado con permisos de ejecución).

En los dos casos, fijar `MAGI_TMUX_ESTADO` a un fichero del temporal de la
prueba (y, si se quiere, `MAGI_TMUX_LLAMADAS`) y sembrarlo antes de cada
escenario. Las pruebas del núcleo que lo usan son
`cargo test -p magi-nucleo --test tmux_script`; las del escritorio, las de
`tests/terminal_servidor.rs` y `tests/terminal_e2e.rs`.

### Comprobación

```sh
shellcheck tests/fijos/terminal/tmux
sh -n tests/fijos/terminal/tmux
```
