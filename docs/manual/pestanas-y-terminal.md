# Pestañas y terminal

[← Índice del manual](README.md)

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
| `prefijo N` | nueva [sesión persistente](sesiones-persistentes.md#sesiones-persistentes) (diálogo) |
| `prefijo x` | cerrar la pestaña (confirma) |
| `prefijo r` | reconectar la pestaña caída |
| `prefijo w` | ventana nueva de terminal (`[terminal] comando`) |
| `prefijo q` / `Esc` | volver a la vista anterior (las sesiones siguen) |
| `prefijo prefijo` | enviar el prefijo literal |

`F3` va a la pestaña activa; sin ninguna abre la **lista de sesiones** del
servidor: `↵` adjuntar · `n` nueva · `r` reconectar · `x` cerrar · `S`
apagar el servidor. `↵` en Hosts y Flota abre siempre una sesión nueva; `q`
sale sin confirmar avisando de las sesiones que siguen abiertas.
`magi conectar <host>` abre una sesión desde un lanzador o atajo, y
`magi sesion <nombre>` engancha una [sesión persistente](sesiones-persistentes.md#sesiones-persistentes).

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

## Terminal: paneles, historial y Claude Code

Desde la 1.4.0 la vista Sesión es un terminal de verdad para lo que corre
dentro, con **Claude Code** como referencia, y el trabajo se reparte en
paneles.

### Paneles

Cada ventana tiene una disposición (no se guarda): **completa**, **lado a
lado**, **apilados**, **1 grande + 2** y **rejilla de 4**. `prefijo Espacio`
pasa a la siguiente; `prefijo` + flechas cambia de panel y `prefijo z` amplía
el enfocado. Cada panel tiene su cabecera con el glifo y el nombre de su
pestaña: `▸` marca el enfocado y `»` los que reciben la escritura en varios.
`prefijo 1`-`9`, `n` y `p` ponen una pestaña en el panel enfocado (si ya está
en otro, se intercambian). La paleta tiene «abrir en rejilla…» para abrir
hasta cuatro hosts de una vez. Por debajo del mínimo de la disposición solo se
ve el panel enfocado con «disposición en espera: ventana pequeña».

### Historial, búsqueda y copia

El servidor guarda por pestaña hasta `[terminal] historial_lineas` líneas
(10 000 por defecto) con el tope de memoria `historial_mib` (64 MiB). `prefijo
[` o `Shift+RePág` (con la pantalla principal) abren el modo historial del
panel:

| Tecla | Acción |
|---|---|
| `↑` `↓` · `j` `k` | una línea |
| `RePág` `AvPág` · `Ctrl+U` `Ctrl+D` | una página · media página |
| `g` · `G` | principio · final |
| `/` | buscar hacia arriba (texto; `Tab` cambia a expresión regular) |
| `n` · `N` | coincidencia siguiente · anterior |
| `v` · `V` | seleccionar por caracteres · por líneas |
| `y` · `↵` | copiar la selección al portapapeles |
| `w` | guardar la captura en un fichero |
| `o` | enlaces de lo visible |
| `q` · `Esc` | volver al vivo |

La salida nueva no mueve la vista (se avisa con «↓ salida nueva»). La copia
usa `wl-copy` o `pbcopy` y, si no hay o MAGI corre por SSH, OSC 52; lo que
pasa de 1 MiB no se copia y se propone guardarlo. `w` nunca sobrescribe y deja
el fichero con permisos 600.

### Enlaces y túneles temporales

`prefijo o` (u `o` en el modo historial) marca con letras los enlaces visibles
y abre un diálogo con la URL entera: `↵` la abre con `[terminal]
abrir_enlaces` (sin shell), `c` la copia y, si apunta a `localhost`,
`127.0.0.1`, `::1` o `0.0.0.0` del host, `t` levanta un túnel temporal en
`127.0.0.1` y abre la misma URL con ese puerto. El túnel vive lo que la
pestaña y se ve en Túneles como «temporal».

### Escritura en varios

`prefijo E` escribe a la vez en todos los paneles visibles. Con más de un host
o con comprobaciones activas pasa por la deliberación MAGI (`Ctrl+K`); si no,
una confirmación. Mientras dura hay un aviso rojo fijo y `»` en cada panel; se
apaga con `prefijo E`, al cambiar de disposición o de pestaña en un panel, si
cae una pestaña, al salir de la vista o a los 10 minutos sin escribir. Cada
pestaña recibe sus bytes (teclado de kitty, pegado) y lo tecleado no va a
REGISTRO.

### Reconexión y tmux

Si la red cae, el **servidor** reintenta solo (2, 5, 10 y 30 s y luego cada
minuto durante `[terminal] reconexion_max_min` minutos, 15 por defecto), sin
diálogos: solo contraseñas del llavero y huellas ya aceptadas. La pestaña
enseña `↻` con la cuenta atrás y `prefijo r` reintenta a mano; `prefijo x`
cierra. Tras `exit`, credenciales rechazadas, huella nueva o frase pendiente
no se reintenta y se dice por qué.

Las sesiones que sobreviven a la ventana y al equipo, con nombre y retomables
desde otro equipo, son las [sesiones persistentes](sesiones-persistentes.md#sesiones-persistentes) de
la 1.5.0 (la marca «sesión persistente con tmux» de la 1.4.0 pasó a «Todas las
sesiones persistentes» de la ficha del host). Las sesiones que hubiera creado
la 1.4.0 en tu tmux de siempre se siguen viendo, enganchando y terminando desde
Sesiones, pero no se migran. MAGI no instala tmux.

### Claude Code y el terminal exterior

MAGI pide al terminal exterior el pegado entre corchetes y los eventos de
foco y, con `teclado_mejorado` (y si el terminal lo admite), la
desambiguación del teclado de kitty: **Mayús+Intro** llega distinto de Intro
y Claude Code inserta un salto de línea en vez de enviar. Pegar varias líneas
llega como un solo pegado, nunca como teclas sueltas: sin el modo 2004, MAGI
avisa antes con las primeras líneas (`↵` pega, `Esc` cancela). El ratón se le
pasa al programa que lo pide (rueda y clic en Claude Code a pantalla completa,
htop o Vim con `set mouse=a`); con `raton = true`, la rueda en la pantalla
principal abre el historial y el clic elige panel. La selección del terminal
exterior sigue con Mayús+arrastre. La salida sincronizada evita el parpadeo, y
los avisos del remoto (campana, OSC 9 y OSC 777) marcan la pestaña con `!` y
su texto en la barra. El título del programa se ve en Sesiones.

El portapapeles del remoto (OSC 52) está apagado por host: la primera vez que
un host quiere copiar, la barra avisa «<host> quiere copiar al portapapeles ·
^] P permitir»; `prefijo P` lo permite para ese host y entrega esa copia si no
han pasado 60 s. Leer el portapapeles, nunca. Al salir, al ceder la terminal
(visor, paginador) y tras un pánico, MAGI deja el terminal exterior como
estaba.

### Configuración

Además de `comando`, la sección `[terminal]` admite:

| Clave | Por defecto | Uso |
|---|---|---|
| `historial_lineas` | `10000` | líneas de historial por pestaña (0-100 000; 0 = sin historial) |
| `historial_mib` | `64` | tope de memoria del historial de una pestaña (8-512); manda sobre las líneas |
| `raton` | `false` | usos propios del ratón: rueda en el historial, clic para elegir panel y arrastre |
| `teclado_mejorado` | `true` | pide la desambiguación de kitty si el terminal la admite |
| `avisar_pegado` | `true` | aviso al pegar varias líneas en un remoto sin pegado entre corchetes |
| `abrir_enlaces` | `"xdg-open"` (macOS: `"open"`; Termux: `"termux-open-url"`) | programa que abre un enlace, sin shell |
| `reconexion_max_min` | `15` | minutos de reconexión automática antes de rendirse (1-120) |

Un valor fuera de rango no impide arrancar: se acota y MAGI avisa al
empezar. Los dos ajustes del host del bloque **TERMINAL** de la ficha
(reconectar y portapapeles del remoto) son de este equipo: no viajan con la
sincronización. «Todas las sesiones persistentes», del bloque **SESIONES**,
sí viaja (ver [Sesiones persistentes](sesiones-persistentes.md#sesiones-persistentes)).
