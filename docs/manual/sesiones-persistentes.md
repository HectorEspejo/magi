# Sesiones persistentes

[← Índice del manual](README.md)

## Sesiones persistentes

Desde la 1.5.0, una sesión de terminal puede **sobrevivir a MAGI y al equipo**.
Una *sesión persistente* tiene nombre y vive en un **tmux propio de MAGI** en
el host (`tmux -L magi`, aparte del tmux que ya uses): cierras MAGI, apagas el
portátil o cambias de equipo y, al volver, te enganchas a la misma sesión. Está
pensada para agentes que siguen trabajando (Claude Code) mientras no estás. Lo
que corre en tu propio equipo no se puede salvar: tiene que correr en un host.

Hace falta **tmux 3.0 o posterior en el host** (MAGI no lo instala). Con uno
anterior, o sin tmux, no se crea la sesión y MAGI dice por qué («instala tmux en
el host», «tmux demasiado antiguo»); en un host marcado, la pestaña se abre
normal, con ese aviso. Para que **Mayús+Intro** llegue a
Claude Code (salto de línea sin enviar) conviene tmux 3.5; la barra de la
pestaña enseña la versión del host. El tmux de tu usuario no se toca: MAGI usa
su propio servidor, con su configuración (sin barra, sin prefijo, ratón,
portapapeles, foco, color completo y la campana) y no carga tu `~/.tmux.conf`.

### Crear una sesión

- **Una sesión con nombre:** `n` en el bloque «Persistentes» de Sesiones,
  `prefijo N`, o «nueva sesión persistente» en la paleta, en Hosts o en Flota.
  El diálogo pide el host (solo SSH), el **nombre** (letras sin tilde, números,
  `-` y `_`, hasta 64), la **carpeta** del host (absoluta o `~/…`; vacía es la
  personal), el **comando de arranque** (vacío es el shell de acceso), el
  **comando de reanudación** (el que se usa al reabrir una terminada, por
  ejemplo `claude --continue`) y una nota. Propone como nombre el de la carpeta.
- **Todo un host:** «Todas las sesiones persistentes» en el bloque SESIONES de
  la ficha del host. Cada pestaña nueva a ese host es persistente, con nombre
  automático `<equipo>-<n>`. Esta marca viaja con la sincronización (la
  «sesión persistente con tmux» de la 1.4.0 pasa a ella al abrir la 1.5.0, con
  un aviso, y deja de ser propia de cada equipo).
- El comando de arranque es **texto tuyo, como un snippet**: corre en el host
  con tu shell de acceso, y al acabar escribe «[MAGI] <comando> terminó con
  código N» y deja un shell en la carpeta. **No pongas secretos en el
  comando**: queda guardado en la ficha, y viaja cifrada a tus otros equipos si
  tienes la sincronización configurada.
- La ficha solo se guarda cuando la sesión ya existe en el host: si falta tmux,
  la carpeta no existe o el nombre ya está en uso, no queda ficha y el mensaje
  dice qué hacer (con el nombre en uso, MAGI ofrece engancharse a la existente
  o cambiar el nombre). Si la carpeta no existe en el host, el diálogo vuelve con
  lo que habías escrito y el motivo en el pie: corrige la carpeta y pulsa `↵`.
- La ficha automática de «Todas las sesiones persistentes» la guarda solo la
  pestaña que **creó** la sesión: engancharse a una que ya existía no crea ficha.
- Si la sesión se crea en el host pero la pestaña no llega a abrirse, MAGI avisa
  («la sesión «X» se creó en «host», pero la pestaña no llegó a abrirse») y la
  sesión sigue ahí: engánchate desde Sesiones.

### La lista

`prefijo l` (o `F3` sin pestañas) abre Sesiones, con el bloque **Persistentes**
arriba: las fichas de todos los hosts con `◆` (en ASCII, `P`), su estado
(**libre**, **enganchada**, **terminada**, **sin comprobar**, **inalcanzable** o
**sin tmux**), el nombre, el host, quién la tiene, la última actividad, `!` si
pide atención, el título del programa (no se enseña si es el nombre del host,
que es el que tmux pone por defecto) y «antigua» si lleva más de
`antigua_dias` sin actividad. Bajo cada host comprobado salen las sesiones del
tmux de MAGI sin ficha («sin ficha»: `a` las añade) y las de tu tmux de siempre
(«de tu tmux»: se enganchan y se terminan, sin ficha). `Tab` pasa a «Pestañas» y
vuelve.

| Tecla (bloque Persistentes) | Acción |
|---|---|
| `↵` | engancharse, soltando a los demás; en una terminada, reabrirla |
| `c` | engancharse compartiendo |
| `v` | vista previa de las últimas líneas (`↵` engancharse, `r` refrescar, `Esc` volver) |
| `n` · `e` | nueva sesión · editar la ficha (carpeta, comando, reanudar y nota) |
| `,` | renombrar, en el host y en la ficha |
| `T` | terminar (confirmación: se cierra lo que corre dentro) |
| `x` | borrar la ficha (confirmación; **nunca** termina la sesión) |
| `a` | añadir a la lista una sesión sin ficha |
| `C` | borrar las fichas terminadas (confirmación con recuento) |
| `r` · `/` | refrescar · filtrar |
| `t` (en Pestañas) | comprobar el host de la pestaña elegida |

El estado de una sesión **no se guarda ni viaja**: el host es la fuente de
verdad y MAGI lo lee de él. Un host que no responde, o que pediría una huella,
una frase o una contraseña, deja sus fichas «inalcanzable» (conéctate a mano):
una ficha nunca pasa a «terminada» por no haber podido mirar.

### Engancharse, soltar y compartir

`↵` engancha **soltando a los demás**: quien la tuviera ve su pestaña como
«soltada» («Ahora la usa <equipo> · `↵` volver · `c` compartir · `x` cerrar») y
la recupera con `↵`. Con `c` os enganchais a la vez y el tamaño lo marca el
último que escribe. Si la red cae, la reconexión automática vuelve sola solo si
nadie más la tiene (lo comprueba y se engancha en un solo paso, así que otro
equipo que se enganche en ese instante la conserva); si la tiene otro equipo, la
pestaña queda soltada y la recuperas tú a mano. Una sesión que otro equipo
renombra sigue siendo la misma: la pestaña soltada sabe qué equipo la tiene y con
qué nombre. Cerrar la pestaña (`prefijo x`) no termina la sesión.

`r` en «Pestañas», «reconectar» de la paleta y `prefijo r` sobre una pestaña
soltada piden antes confirmación («VOLVER A LA SESIÓN», con `↵`), porque le
quitan la sesión al otro equipo.

#### Si el shell del host ya arranca tmux

Las sesiones persistentes corren en un tmux propio de MAGI (`tmux -L magi`) y su
shell es el de acceso de tu usuario en el host. Si ese shell arranca tmux por su
cuenta (un `exec tmux` en `~/.bashrc` o `~/.zprofile`, o un snippet «al
conectar» que lo lance), la sesión queda con **tmux dentro de tmux**: dos
prefijos, dos barras, y teclas como Mayús+Intro, el ratón o el portapapeles no
llegan a donde esperas. MAGI no lo evita ni lo detecta (solo avisa en la ficha
del host cuando el snippet «al conectar» arranca tmux); se arregla en el host:
haz que tmux solo arranque fuera de uno. tmux pone la variable `TMUX` dentro de
sus sesiones, así que basta con comprobarla: `[ -z "$TMUX" ] && exec tmux` en el
`~/.bashrc` del host (o, en el snippet, `[ -n "$TMUX" ] || tmux new -As main`).
Las pestañas normales (no persistentes) no se ven afectadas.

### Retomarla desde otro equipo

Las **fichas** (host, nombre, carpeta, comandos y nota) viajan con la
[sincronización del inventario](sincronizacion.md#sincronización-del-inventario), cifradas; el
estado y el contenido, no. En el otro equipo la sesión aparece en la lista y
`↵` la engancha. Con un reinicio del host, tmux termina sus sesiones: la ficha
queda «terminada» y `↵` la reabre con su comando de reanudación.

### Avisos de atención

La atención es la **campana** del terminal: tmux marca la sesión cuando el
programa de dentro la toca, también sin nadie enganchado. Mientras haya alguna
ventana de MAGI abierta, el servidor comprueba las sesiones cada `vigilar_seg`
y la barra de cualquier vista avisa («◆ agente-api pide atención · web-01») y
lo quita cuando deja de pedirla. Al conectar una ventana, MAGI no avisa de una
campana comprobada antes de esa conexión: el servidor vuelve a mirar los hosts
enseguida y, en cuanto llega la lista fresca, el aviso sale si la sesión sigue
pidiendo atención. **Claude Code** debe avisar con la campana: en su
configuración, `preferredNotifChannel` a `terminal_bell`. Los avisos OSC 9 y
OSC 777 no atraviesan tmux, así que no valen como canal.

### Historial

`prefijo [` en una pestaña persistente abre el modo historial con el historial
**de tmux** (búsqueda, selección, copia y guardado como en una pestaña normal).
MAGI no pide el prefijo de tmux: el multiplexor eres tú desde MAGI, y la rueda
llega al modo copia de tmux o al programa que pide el ratón.

### Desde la línea de órdenes

```sh
magi sesion agente-api                 # abre MAGI enganchado a esa sesión
magi sesion agente-api --host web-01   # si el nombre está en varios hosts
magi sesiones                          # las fichas con su estado
magi sesiones --json                   # lo mismo, para scripts
```

`magi sesion <nombre> [--host <host>]` abre MAGI enganchado a la sesión,
soltando a los demás. Si el nombre está en varios hosts pide `--host` y dice
cuáles; si no hay ficha lo dice y sugiere las de nombre parecido. Si la sesión
ya no existe en el host (terminada), MAGI dice que ya no existe y no la
vuelve a crear: reábrela desde la lista con `↵`. Sale con código 1 si no puede
resolver la sesión (un nombre que no vale, ninguna ficha o varios hosts).

`magi sesiones [--json]` lista las fichas con su estado. Pregunta al servidor de
sesiones (lo arranca si hace falta, como `magi tunel`; esta orden cuenta como
una ventana conectada para el vigilante) y espera a que cada host se haya
comprobado **después** de la pregunta: lo que no llega a tiempo sale
`sin_comprobar`, nunca un estado viejo. **No abre diálogos**: un host que pide
huella, frase o contraseña sale `inalcanzable` y la salida de errores dice que
te conectes a mano desde MAGI. Sin servidor sale con código 1. Sin fichas no
arranca nada y dice cómo crearlas.

La salida de `--json` es una lista (vacía si no hay fichas), ordenada por host y
nombre, con un objeto por ficha. Los campos son estables; solo se añadirán
campos nuevos al final:

| Campo | Tipo | Significado |
|---|---|---|
| `uuid` | texto | identidad de la ficha, la misma en todos los equipos |
| `host` | texto | nombre del host en el inventario |
| `nombre` | texto | nombre de la sesión en el tmux de MAGI |
| `estado` | texto | `viva_libre`, `viva_enganchada`, `terminada`, `sin_comprobar`, `inalcanzable` o `sin_tmux` |
| `equipo` | texto o `null` | quién la tiene: la etiqueta del equipo si está enganchada; `null` si está libre o no consta |
| `actividad` | entero o `null` | última actividad en segundos Unix según el reloj de este equipo (corregida con la hora del host); `null` si no vive o el host no la dio |
| `atencion` | booleano | la sesión pide atención (la campana de tmux) |
| `titulo` | texto o `null` | título del programa que corre dentro, si vive |
| `comprobada_en` | entero o `null` | cuándo se miró su host, en segundos Unix; `null` si `sin_comprobar` |

La salida no lleva comandos, carpetas ni notas, ni nada de lo que pasa dentro
de la sesión más allá de su título.

### Configuración

En `config.toml`, la sección `[persistentes]`:

| Clave | Por defecto | Uso |
|---|---|---|
| `vigilar_seg` | `60` | cada cuánto comprueba el servidor las sesiones, con alguna ventana conectada (30-600) |
| `refresco_seg` | `30` | refresco con la lista a la vista (10-300) |
| `antigua_dias` | `7` | días sin actividad para marcar una sesión «antigua» (1-365) |
| `vista_previa_lineas` | `20` | líneas de la vista previa (5-100) |

Un valor fuera de rango no impide arrancar: se acota y MAGI avisa al empezar.
`[terminal] historial_lineas` fija también el `history-limit` de las sesiones
nuevas.

### Límites

- Un reinicio del host termina sus sesiones: se reabren con su comando de
  reanudación (`claude --continue`, por ejemplo).
- Una pestaña que ya está abierta no se puede hacer persistente: un proceso en
  marcha no se puede meter dentro de tmux.
- Con tmux anterior a 3.5, Mayús+Intro puede no llegar a Claude Code.
- La app Android lee el formato de sincronización que lleva las fichas, pero no
  las enseña ni las usa todavía.
