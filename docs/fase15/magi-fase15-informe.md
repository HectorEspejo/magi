# MAGI - Fase 15: Terminal, Segunda Vuelta

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 9 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-14 con informe procesado. La F14 (98/98) tiene pendiente su prompt de cierre, que recoge también los cierres de la F11, la F12 y la F13 sobre la rama `fase14`: esta fase parte de esa rama ya cerrada y fusionada. La F10 sigue sin validación manual ni corrección 10b; la F9, pendiente de su primera publicación.
>
> Estado (9 nov 2026): especificada; a la espera del informe de implementación.

---

## 1. Visión General

La Fase 15 es la **segunda vuelta del terminal del escritorio**. La vista Sesión de la F1 y las pestañas de la F3 enseñan un remoto cada vez y no guardan nada de lo que sale por arriba: no se puede volver atrás, ni buscar, ni copiar con el teclado; los enlaces hay que copiarlos a mano; si la red cae, la pestaña se queda caída hasta pulsar `prefijo r`, y lo que corría se pierde.

Esta fase añade **paneles** (varias pestañas a la vista en disposiciones fijas), un **historial** de 10 000 líneas por pestaña con **búsqueda** y **copia con el teclado**, **enlaces** que se abren desde el teclado (también los de `localhost` del host, por un túnel temporal), **escritura en varios paneles** a la vez con deliberación, **reconexión automática** y **sesión persistente con tmux**.

Además, a petición de Hector, MAGI pasa a comportarse como un terminal de verdad para lo que corre dentro, con **Claude Code** como referencia: el pegado de varias líneas llega como un solo pegado, Mayús+Intro inserta un salto de línea, la rueda desplaza, no hay parpadeo, y los avisos y el título del programa se ven en la pestaña.

**Objetivos principales**

1. Paneles en la vista Sesión (cinco disposiciones fijas por ventana) y escritura en varios paneles a la vez, con deliberación cuando hay más de un host.
2. Historial de 10 000 líneas por pestaña, guardado en el servidor, con modo historial al estilo tmux: búsqueda (texto o expresión regular), selección, copia al portapapeles y guardado en un fichero.
3. Enlaces `http` y `https` marcados con letras y abiertos en el navegador; los de `localhost` del host, por un túnel temporal.
4. Reconexión automática sin diálogos y sesión persistente con tmux por host.
5. Compatibilidad con Claude Code y otras TUI modernas: pegado entre corchetes de punta a punta, respuestas a las consultas del terminal, teclado de kitty (bandera 1), salida sincronizada, ratón para el programa que lo pide, foco, avisos y título.
6. Lógica en `magi-nucleo` (lista para llevarla al móvil más adelante); protocolo v9, migración 10 y versión 1.4.0.

**Contexto.** Producto propio. Hector trabaja a diario con Claude Code en sus hosts a través de MAGI, y hoy le pasan dos cosas:

- **Al pegar texto con varias líneas, cada salto de línea se envía como un mensaje distinto.** MAGI no le pasa al remoto el pegado entre corchetes (modo 2004): el texto llega como teclas sueltas, y cada salto de línea es un Intro.
- **Para desplazarse hay que pulsar Fn+flechas** (RePág y AvPág). Desde el 6 de mayo de 2026, Claude Code arranca por defecto a pantalla completa para quien empieza a usarlo: dibuja en la pantalla alternativa y pide el ratón al terminal para la rueda. MAGI no propaga esa petición, así que la rueda no llega. En el modo clásico, lo que haría falta es el historial del propio terminal, y la vista Sesión no lo tiene.

Hay más piezas que Claude Code espera de un terminal:

- Mayús+Intro llega como Intro: Claude Code lo distingue con el protocolo de teclado de kitty, que MAGI no anuncia.
- Hay parpadeo: Claude Code pregunta si el terminal admite la salida sincronizada (DEC 2026) y nadie le contesta.
- Los avisos de «necesito tu permiso» se pierden en las pestañas que no se ven.

Respuestas de Hector (9 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | Las siete piezas (paneles, historial con búsqueda, copiar con el teclado, enlaces, escribir en varios, reconexión automática y tmux), solo en el escritorio; lo común, en el núcleo para el móvil más adelante (a) |
| 2 | Extras: renombrar y reordenar pestañas, guardar el historial en un fichero solo cuando se pida, y avisar al pegar varias líneas si el remoto no usa el pegado entre corchetes (a) |
| 3 | 10 000 líneas por pestaña, configurables y con tope de memoria, guardadas en el servidor: cualquier ventana ve el mismo historial; solo en memoria (a) |
| 4 | Modo historial al estilo tmux con `prefijo [` o `Shift+RePág`; flechas y páginas, `/` para buscar (texto o expresión regular), `n`/`N`, `v` para seleccionar, `y` o `↵` para copiar, `Esc` para salir; la salida nueva no mueve la vista (a) |
| 5 | Portapapeles del sistema con `wl-copy`/`pbcopy` y, si no hay, OSC 52; el remoto solo escribe en él si se activa en la ficha del host; leerlo, nunca (a) |
| 6 | `prefijo o` marca los enlaces con letras; se ve la URL entera antes de abrirla; los de `localhost` del host, por un túnel temporal que se cierra con la pestaña (a) |
| 7 | Disposiciones fijas por ventana (1, 2 lado a lado, 2 apilados, 1 grande + 2 y rejilla de 4), `prefijo` + flechas, `prefijo z`, abrir hasta 4 hosts o un grupo en rejilla desde la paleta; no se guardan (a) |
| 8 | Escribir solo en los paneles visibles; con más de un host, deliberación MAGI (`Ctrl+K`); aviso rojo fijo; se apaga con la misma tecla, al cambiar de disposición o si cae un panel (a) |
| 9 | Ratón opcional y apagado por defecto (`[terminal] raton`): rueda para el historial, clic para elegir panel y reenvío al remoto (a) |
| 10 | Reconexión automática con esperas de 2, 5, 10 y 30 s y luego cada minuto hasta 15 min, sin diálogos, con cuenta atrás y `prefijo r`; activa por defecto y desactivable por host (a) |
| 11 | Opción por host «sesión persistente»: la pestaña abre o recupera su sesión de tmux; cerrar la pestaña la deja viva; la vista Sesiones lista las sesiones de tmux del host (a) |
| Añadido | «Que funcione correctamente Claude Code en las sesiones»: el pegado de varias líneas no puede enviar un mensaje por línea, y desplazarse no puede exigir Fn+flechas |

Dos respuestas se precisan para cumplir el añadido sin cambiar lo que Hector eligió:

- **Ratón (9).** El ratón se le pasa al programa remoto que lo pide (Claude Code a pantalla completa, htop, Vim con ratón) también con `raton = false`, como hace cualquier terminal. La opción solo enciende los usos propios de MAGI: la rueda en el historial de MAGI y el clic para elegir panel (D306).
- **Portapapeles (5).** Claude Code copia por OSC 52 cuando corre por SSH. La primera vez que un host lo intenta, MAGI avisa en la barra y `prefijo P` lo permite en ese host; la copia que lo provocó se entrega si no han pasado 60 s (D302).

**Fuera de alcance**

- Todo lo de esta fase en el móvil: la lógica queda en `magi-nucleo` y la app no la activa (D298).
- División libre de paneles y espacios de trabajo guardados (Hector, 7a).
- Grabar sesiones y buscar en lo grabado: va con la Caja negra (F19). El historial de esta fase vive en memoria.
- El modo de control de tmux (`tmux -CC`), zellij y screen.
- Reconectar solos los túneles y el SFTP: siguen como en la F5 y la F4.
- Imágenes en el terminal (sixel, gráficos de kitty), enlaces OSC 8 y el protocolo de estado OSC 7501 (sin especificación pública).
- Avisos del sistema (notificaciones de escritorio), como en la F13.
- Que el remoto lea el portapapeles (OSC 52 de lectura): nunca.
- Las banderas del teclado de kitty más allá de la 1 (soltar teclas, todas las teclas como secuencias, texto asociado).

**Versión prevista:** escritorio **1.4.0** (MINOR: funcionalidad nueva, migración 10 y protocolo v9; `formato` de sincronización sin cambios, T68). La app Android no cambia: recibe la migración 10 al compilarse con el núcleo nuevo, sin nada visible.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Terminal exterior | Alacritty, Ghostty, foot, kitty, WezTerm, Terminal.app… | MAGI pide el pegado entre corchetes, los eventos de foco y, si el terminal la admite, la desambiguación de kitty (crossterm 0.29); el ratón, solo cuando hace falta |
| Emulación | vt100 0.16 con sus `Callbacks` | Lo que vt100 no lleva (modos 1004 y 2026, banderas de kitty, ED 3, consultas, avisos) en `magi-nucleo/src/terminal/emulador.rs` |
| Núcleo | `crates/magi-nucleo/src/terminal/` (emulador, teclas, raton, pegado, historial, enlaces, tmux) y `conexion/reconexion.rs` | Sin interfaz ni red; lo usan el cliente y el servidor |
| Host remoto | `sh` POSIX y, para la sesión persistente, tmux 1.8 o posterior | `crates/magi-nucleo/scripts/terminal_tmux.sh`, embebido con `include_str!` |
| Servidor | `src/servidor/sesiones.rs` (reconexión, tmux, foco, respuestas, avisos), `historial.rs`, `escritura_varios.rs`, `tuneles_temporales.rs` | Protocolo v9 |
| Cliente | `src/app/paneles.rs`, `historial.rs`, `enlaces.rs`, `escritura_varios.rs`, `portapapeles.rs`, `raton.rs`, `pegado.rs`, `src/cliente/pantallas.rs` y `src/ui/sesion.rs` | Vista Sesión con paneles y modos |
| Datos | SQLite, migración 10 | Tres columnas de HOSTS, propias del equipo |
| Dependencia nueva | `regex` (MIT o Apache-2.0) | Búsqueda con expresión regular en tiempo lineal |

### Estructura de carpetas (novedades)

```
crates/magi-nucleo/
├── src/terminal/
│   ├── emulador.rs     Callbacks de vt100: modos 1004 y 2026, banderas de kitty por pantalla, ED 3, título,
│   │                   avisos (BEL, OSC 9, OSC 777), OSC 52 y respuestas a consultas (solo si responde)
│   ├── teclas.rs       (F11) + codificación con la bandera 1 de kitty (Mayús+Intro → CSI 13;2u)
│   ├── raton.rs        codificación del ratón: modos 1000/1002/1003; por defecto, UTF-8 y SGR
│   ├── pegado.rs       saneado y marcas del pegado entre corchetes
│   ├── historial.rs    captura en tramos, búsqueda, selección y texto plano
│   ├── enlaces.rs      enlaces http/https, localhost y reescritura para el túnel
│   └── tmux.rs         nombres, argumentos del script y lectura de su salida
├── src/conexion/reconexion.rs   causa recuperable o no y esperas
├── src/almacen/        hosts (tres columnas) y migración 10
├── scripts/terminal_tmux.sh     comprobar · abrir · listar · terminar
└── tests/              terminal_emulador, terminal_teclas, terminal_raton, terminal_pegado,
                        terminal_historial, terminal_enlaces, tmux_script, reconexion
src/servidor/sesiones.rs          parser con historial y emulador, respuestas, foco, avisos, reconexión, tmux
src/servidor/historial.rs         CapturarHistorial → HistorialLote
src/servidor/escritura_varios.rs  concesiones y escritura en las sesiones concedidas
src/servidor/tuneles_temporales.rs  túneles a localhost del host mientras vive la pestaña
src/cliente/pantallas.rs          parser con emulador (modos, sin responder) y salida sincronizada
src/app/                          paneles, historial, enlaces, escritura_varios, portapapeles, raton, pegado
src/app/teclado.rs                pegado entre corchetes, foco y banderas de kitty del terminal exterior
src/ui/sesion.rs · sesiones.rs · ficha    paneles, modos, marcas, título, tmux y bloque TERMINAL
src/config.rs                     [terminal] con sus rangos
tests/terminal_servidor.rs · tests/terminal_ui.rs · tests/terminal_e2e.rs · tests/redimensionado
tests/fijos/terminal/             programa de prueba «tui_prueba» y tmux falso
docs/fase15/magi-fase15-revision-adversarial.md
```

### Diagrama de arquitectura

```
 EQUIPO DEL USUARIO (Omarchy o macOS)                                                           HOST SSH
 terminal exterior: Alacritty, Ghostty, foot, kitty, Terminal.app…
    ▲ pegado entre corchetes · foco · teclado de kitty (bandera 1) · ratón si hace falta · OSC 52
    │
 ┌──────────────────────────────────┐     ┌──────────────────────────────────────────┐      ┌───────────────────────┐
 │ magi (cliente)                   │     │ magi --servidor                          │      │ web-01                │
 │ vista Sesión con paneles         │ v9  │ sesiones (pestañas) con parser vt100     │ SSH  │ shell · vim · htop    │
 │ modo historial: captura,         │◀───▶│   + emulador del núcleo:                 │◀────▶│ Claude Code           │
 │   buscar, copiar, guardar        │     │   historial (tope de líneas y MiB)       │      │   (clásico o a        │
 │ enlaces · escritura en varios    │     │   respuestas a consultas (una vez)       │      │   pantalla completa)  │
 │ ratón · pegado · portapapeles    │     │   título · avisos · OSC 52               │      │ tmux (opcional)       │
 │ parser vt100 + emulador          │     │ reconexión automática (sin diálogos)     │      │ terminal_tmux.sh      │
 │   (modos, sin responder)         │     │ tmux: comprobar · abrir · listar         │      │                       │
 │ capturas: solo mientras miras    │     │ escritura en varios: concede y           │      └───────────────────────┘
 │                                  │     │   recomprueba                            │
 └──────────────────────────────────┘     │ túneles temporales (127.0.0.1)           │
                                          │ pool SSH · hilo escritor (T18)           │
                                          │ REGISTRO · DELIBERACIONES                │
                                          └──────────────────────────────────────────┘
 magi-nucleo/src/terminal: emulador · teclas (kitty) · raton · pegado · historial · enlaces · tmux;
 conexion/reconexion.rs: causas y esperas; scripts/terminal_tmux.sh embebido con include_str!.
 Lo usan el cliente y el servidor; el móvil no las activa en esta fase (D298).

 Las respuestas a las consultas del remoto las da solo el servidor, en orden con la salida (D310).
 El cliente nunca recibe bytes en bruto para pintar: Datos → su parser → Pantalla (T13).
```

La TUI sigue sin hablar con el host: abre pestañas, pide capturas del historial, listas de tmux y túneles temporales al servidor, que custodia las sesiones. El servidor y el cliente procesan la misma salida con el mismo emulador del núcleo, pero **solo el servidor responde** a las consultas del remoto (posición del cursor, atributos, modos, teclado de kitty, versión, colores), una vez por sesión y en orden con la salida, aunque haya varias ventanas o ninguna.

---

## 3. Modelo de Datos

La **migración 10** (`user_version` 10) añade tres columnas a HOSTS, con valor por defecto y propias del equipo: no viajan con la sincronización, así que `formato` no cambia (D311). El historial, los modos del remoto, las reconexiones, la escritura en varios y los túneles temporales viven en la memoria del servidor; las capturas, los paneles y la disposición, en la del cliente.

### Diagrama E-R (cambios de la fase)

```
 ┌────────────────────────────────────────────────┐       ┌────────────────────────────────────────────┐
 │ HOSTS                         (migración 10)   │       │ DELIBERACIONES (F6, sin columnas nuevas)   │
 │ id uuid tipo nombre …           (F1-F12)       │       │ snippet_id NULL                            │
 │ reconectar          1 = sí (por defecto)       │       │ accion «escritura en varios:               │
 │ tmux_persistente    0 = no                     │       │   web-01 · web-02 · db-01»                 │
 │ portapapeles_remoto 0 = no                     │       │ hosts_json · comprobaciones_json           │
 │ los tres: propios del equipo, no viajan        │       │ ejecucion_resultado «terminada ·           │
 │ solo hosts de tipo ssh            (T100)       │       │   12 min · 41 envíos» (servidor)           │
 └────────────────────────────────────────────────┘       └────────────────────────────────────────────┘
            │ 1:N
            ▼
 ┌────────────────────────────────────────────────┐       ┌────────────────────────────────────────────┐
 │ REGISTRO (tipos nuevos)                        │       │ en memoria (sin tabla)                     │
 │ reconexion_agotada · historial_guardado        │       │ servidor: historial por pestaña,           │
 │ escritura_varios_activada                      │       │   modos del remoto, reconexiones,          │
 │ escritura_varios_terminada · tmux_terminada    │       │   escritura en varios, túneles             │
 │ y los de antes: sesion_reconectada             │       │   temporales, portapapeles (60 s)          │
 │   «automática», tunel_* «temporal»             │       │ cliente: capturas, paneles y la            │
 │ nunca lo tecleado ni la salida (T44)           │       │   disposición de cada ventana              │
 └────────────────────────────────────────────────┘       └────────────────────────────────────────────┘

 La migración 10 solo añade columnas con valor por defecto; `formato` de sincronización sigue en 3.
```

### 3.1 HOSTS: columnas nuevas (migración 10)

| Campo | Tipo | Por defecto | Descripción |
|---|---|---|---|
| `reconectar` | INTEGER (0/1) | 1 | Reconexión automática de las pestañas de este host cuando cae la red (§7.9) |
| `tmux_persistente` | INTEGER (0/1) | 0 | Las pestañas de este host abren o recuperan una sesión de tmux (§7.10) |
| `portapapeles_remoto` | INTEGER (0/1) | 0 | El host puede escribir en el portapapeles con OSC 52 (§7.4); leerlo, nunca |

Las tres son **propias del equipo**, como `plegado` o `huella_escritorio`: no suben la versión del host ni entran en el fichero de sincronización. Solo tienen sentido en hosts de tipo `ssh` (T100); en los demás se ignoran. Se validan en código (0 o 1), sin `CHECK`.

### 3.2 REGISTRO: tipos nuevos

| Tipo | Cuándo | Detalle |
|---|---|---|
| `reconexion_agotada` | La reconexión automática se rinde (`reconexion_max_min`) | Pestaña, intentos y última causa |
| `escritura_varios_activada` | Empieza una escritura en varios | Pestañas, hosts y `deliberacion_id` o «confirmada» |
| `escritura_varios_terminada` | Termina | Duración, envíos y motivo (apagada, disposición, panel caído, salida de la vista, inactividad o ventana cerrada) |
| `historial_guardado` | Se guarda una captura en un fichero | Ruta, líneas y bytes; nunca el contenido |
| `tmux_terminada` | MAGI termina una sesión de tmux del host | Host y nombre |

Se reutilizan `sesion_reconectada` (F3), con «automática» e intentos en el detalle, y `tunel_abierto`, `tunel_cerrado` y `tunel_fallido` (F5), con «temporal» y la pestaña. Nunca va a REGISTRO lo tecleado, la salida del remoto, el historial, un título, un aviso, una URL ni el portapapeles (T44).

### 3.3 DELIBERACIONES

Sin columnas nuevas. Una escritura en varios que delibera guarda una fila con `snippet_id` NULL, `accion` «escritura en varios: web-01 · web-02 · db-01» y los hosts en `hosts_json`. La escribe el cliente, como en la F6; al terminar, el servidor escribe `ejecucion_resultado` con la duración y el número de envíos (T18).

### 3.4 En la memoria del servidor (sin tabla)

| Estructura | Contenido | Vida |
|---|---|---|
| Historial de una pestaña | Las filas que salen por arriba de la pantalla principal (historial de vt100), con tope de líneas y de bytes (§7.1) | Lo que vive la pestaña; se conserva al reconectar; `CSI 3 J` lo vacía |
| Modos del remoto | 1004, 2026, banderas de kitty de cada pantalla, título, último aviso y última petición de portapapeles | Lo que vive la pestaña |
| Reconexión | Intento, próxima espera, desde cuándo y última causa | Mientras la pestaña está `reconectando` |
| Escritura en varios | `id`, cliente, sesiones concedidas, hosts, deliberación, inicio, envíos y último envío | Hasta que termina (§7.7) |
| Túnel temporal | `id`, sesión, puerto local, destino (`localhost`, `127.0.0.1` o `::1`) y puerto, conexiones abiertas | Hasta que se cierra la pestaña o el usuario lo cierra |

### 3.5 En la memoria del cliente (sin tabla)

| Estructura | Contenido |
|---|---|
| Disposición de la ventana | Una de las cinco, panel enfocado, ampliado o no, y la pestaña de cada panel |
| Modo de cada panel | Vivo, historial, búsqueda, selección o enlaces (§3.7) |
| Captura | Líneas en tramos con `envuelta`, posición, búsqueda (patrón, tipo y coincidencias) y selección; se libera al volver al vivo |
| Estado de los parsers | El emulador de cada pestaña adjunta: modos (2004, ratón, kitty, 2026) para codificar teclas, pegado y ratón |

### 3.6 Relaciones

- HOSTS 1:N REGISTRO (F2), como siempre; los tipos nuevos llevan el host de la pestaña.
- DELIBERACIONES lleva los hosts en `hosts_json` (F6); una escritura en varios autoriza una sola concesión (§7.7).
- Un túnel temporal pertenece a una pestaña y usa la conexión de su host (T42); no tiene fila en TUNELES.
- Una pestaña con `tmux_persistente` apunta a una sesión de tmux del host por su nombre (§7.10); MAGI no guarda la lista de tmux.

### 3.7 Diagramas de estados

**Pestaña (amplía la F3)**

```
  abriendo ─ ok ─▶ abierta ─ exit (estado de salida) ─▶ cerrada ─▶ eliminada                (F3)
  abierta ─ caída por la red, reconectar = 1 ─▶ reconectando(intento 1, espera 2 s)
  abierta ─ caída por la red, reconectar = 0 ─▶ caida (prefijo r, como en la F3)
  reconectando ─ espera cumplida ─▶ intentando (sin diálogos, plazo 20 s)
  intentando ─ ok ─▶ abierta (REGISTRO sesion_reconectada «automática»)
             ─ fallo recuperable ─▶ reconectando(intento + 1, siguiente espera)
             ─ credenciales, huella, frase pendiente ─▶ caida «necesita tu respuesta: prefijo r»
  reconectando ─ reconexion_max_min sin éxito ─▶ caida «reconexión agotada» (REGISTRO)
  reconectando ─ prefijo r ─▶ intento a mano ya (con diálogos al solicitante, F3)
  reconectando ─ prefijo x ─▶ cerrada
  reconectando ─ vuelta de una suspensión ─▶ intentando ya
  Inválidas: reintentar tras exit o tras cerrar · abrir diálogos en un intento automático ·
             reconectando ─▶ abierta sin intento · dos intentos a la vez de la misma pestaña
```

**Modo de un panel (cliente)**

```
  vivo ─ prefijo [ · Shift+RePág · rueda (raton = true) ─▶ capturando  (solo pantalla principal)
  vivo ─ prefijo [ con pantalla alternativa ─▶ vivo + aviso «este programa tiene su propio historial»
  capturando ─ último HistorialLote ─▶ historial
  capturando ─ error o pestaña cerrada ─▶ vivo + aviso
  historial ─ / ─▶ buscando ─ ↵ ─▶ historial (coincidencias) · Esc ─▶ historial
  historial ─ v · V ─▶ seleccionando ─ y · ↵ ─▶ historial (copiado) · Esc ─▶ historial
  historial ─ w ─▶ diálogo guardar ─▶ historial
  historial ─ o ─▶ enlaces (sobre lo visible de la captura)
  vivo ─ prefijo o ─▶ enlaces ─ letra ─▶ diálogo del enlace ─ ↵ · c · t · Esc ─▶ vivo
  historial ─ q · Esc ─▶ vivo (la captura se libera)
  Inválidas: historial de la pantalla alternativa · seleccionar fuera de la captura ·
             escribir al remoto desde el modo historial (las teclas son del modo)
```

**Escritura en varios**

```
  apagada ─ prefijo E con 2 o más paneles ─▶ preparando
  preparando ─ un host y sin comprobaciones activas ─▶ confirmando ─ ↵ ─▶ pedida · Esc ─▶ apagada
  preparando ─ más de un host o comprobaciones activas ─▶ deliberando (F6)
  deliberando ─ Ctrl+K (aprobada o forzada con motivo) ─▶ pedida · Esc ─▶ apagada
  pedida ─ EscrituraVariosIniciada ─▶ activa
  pedida ─ Error (deliberación no válida o usada, pestaña no abierta) ─▶ apagada + aviso
  activa ─ prefijo E · otra disposición · otra pestaña en un panel · panel caído ·
           salir de la vista Sesión · 10 min sin escribir · ventana cerrada ─▶ terminada ─▶ apagada
  Inválidas: activa con un solo panel · varios hosts sin deliberación · escribir en una
             pestaña no concedida (el servidor la rechaza) · reutilizar una deliberación
```

**Túnel temporal**

```
  pedido ─ TunelTemporal ─▶ activo ─ pestaña cerrada · CerrarTunelTemporal ─▶ cerrado
  pedido ─ puerto local o direct-tcpip rechazado ─▶ fallido (aviso; REGISTRO tunel_fallido)
  activo ─ cae la conexión del host ─▶ cerrado (se vuelve a pedir desde el enlace)
  Inválidas: escuchar fuera de 127.0.0.1 · destino que no sea localhost del host ·
             sobrevivir a su pestaña
```

**Sesión de tmux en el host (vista por MAGI)**

```
  (no existe) ─ abrir pestaña con nombre nuevo ─▶ enganchada
  enganchada ─ pestaña cerrada (prefijo x) · red caída ─▶ libre (sigue en el host)
  libre ─ reconexión de su pestaña · ↵ en la lista de Sesiones ─▶ enganchada
  enganchada · libre ─ T terminar (confirmación) ─▶ terminada (REGISTRO tmux_terminada)
  enganchada ─ exit del último shell ─▶ terminada (la pestaña se cierra, F3)
  Inválidas: nombre sin validar · terminar sin confirmación · crear sesiones desde la lista
```

**Petición de portapapeles del remoto (OSC 52)**

```
  llega ─ host con portapapeles_remoto = 1 ─▶ copiada (aviso en la barra con host y tamaño)
  llega ─ sin permiso ─▶ pendiente (solo la última, 60 s) ─ prefijo P ─▶ permitida y copiada
                                                          ─ 60 s ─▶ descartada
  OSC 52 de lectura ─▶ ignorada siempre
  Inválidas: copiar sin aviso · guardar más de una pendiente · más de 1 MiB
```

---

## 4. Flujos de Trabajo

### 4.1 Abrir una pestaña (con tmux o sin él)

```
  (cliente) ↵ en Hosts o Flota · paleta · «abrir en rejilla…» · ↵ en la lista de tmux
      ▼
  AbrirSesion{host_id, cols, filas, tmux?} ─▶ servidor: conexion_para_canal (T42; diálogos al solicitante, F3)
      ├─ falla ─▶ Estado{Cerrada, motivo} (F3, fin)
      ▼
  ¿host.tmux_persistente, o una sesión de tmux pedida desde la lista?
      ├─ no ─▶ PTY (xterm-256color) · COLORTERM=truecolor (el host puede ignorarlo) · shell ─▶ abierta
      └─ sí ─▶ exec terminal_tmux.sh listar (plazo 10 s)
                ├─ MAGI_TMUX=no ─▶ PTY · shell normal ─▶ abierta + aviso «sin tmux en web-01: shell normal»
                ├─ error o plazo ─▶ PTY · shell normal ─▶ abierta + aviso con el motivo
                └─ MAGI_TMUX=<versión> ─▶ nombre: el pedido, o uno libre magi-<equipo>-<n> (§7.10)
                       ▼
                   PTY · terminal_tmux.sh abrir <nombre> ─▶ abierta (tmux)
                       └─ nombre nuevo ─▶ snippet al conectar y comando inicial (F6), solo entonces
      ▼
  parser con historial (§7.1) y emulador que responde · Sesiones{lista} a todas las ventanas
```

### 4.2 Caída y reconexión automática

```
  vigilante de la conexión, o fin del canal de la pestaña
      ▼
  ¿llegó exit-status o exit-signal? ─sí─▶ cerrada (F3, fin)
      ▼ no
  causa (§7.9): ¿recuperable? ─no─▶ caida con el motivo (prefijo r, fin)
      ▼ sí
  ¿host.reconectar? ─no─▶ caida (F3, fin)
      ▼ sí
  reconectando: intento n, espera e(n) · Sesiones{lista} con la cuenta atrás
      ▼ (espera cumplida, o vuelta de una suspensión)
  conexión sin diálogos (contraseña solo del llavero, T49; huella ya aceptada) · plazo 20 s
      ├─ ok ─▶ PTY (con tmux, el mismo nombre) ─▶ abierta · línea «── reconectada HH:MM ──» · REGISTRO
      ├─ fallo recuperable ─▶ n + 1 ─▶ (pasado reconexion_max_min) caida «reconexión agotada» · REGISTRO
      └─ credenciales rechazadas · huella nueva o cambiada · frase pendiente
              ─▶ caida «necesita tu respuesta: prefijo r» (fin)
```

### 4.3 Pegar

```
  terminal exterior con pegado entre corchetes (lo pide MAGI) ─▶ Event::Paste(texto)
      ▼
  ¿vista Sesión en vivo? ─no─▶ el texto va al campo con foco (diálogo, búsqueda) o se descarta
      ▼ sí
  pegado.rs: saneado (§7.12) ─▶ ¿vacío? ─sí─▶ nada (fin)
      ▼
  ¿escritura en varios activa? ─sí─▶ lo mismo para cada pestaña concedida, con sus modos (§4.7)
      ▼
  ¿el remoto activó 2004? ─sí─▶ ESC[200~ texto ESC[201~ en escrituras propias ─▶ Teclas (fin)
      ▼ no
  ¿varias líneas y avisar_pegado? ─sí─▶ diálogo PEGAR VARIAS LÍNEAS ─ Esc ─▶ nada
      │                                                           ─ ↵ ─▶ texto con CR
      └─no─▶ texto con CR
```

### 4.4 Modo historial: capturar, buscar, copiar y guardar

```
  prefijo [ · Shift+RePág (solo pantalla principal) · rueda con raton = true
      ▼
  ¿pantalla alternativa? ─sí─▶ aviso «este programa tiene su propio historial»; Shift+RePág va al remoto (fin)
      ▼
  CapturarHistorial ─▶ servidor: historial + pantalla en tramos ─▶ HistorialLote… (lotes por tamaño)
      ├─ plazo de 5 s o pestaña cerrada ─▶ vivo + aviso (fin)
      ▼
  modo historial (la pestaña sigue recibiendo; «↓ salida nueva» si llega algo)
      ├─ / patrón ─▶ búsqueda en líneas lógicas (§7.2) ─▶ resaltado · n · N · «sin coincidencias»
      ├─ v · V ─▶ selección ─ y · ↵ ─▶ texto (§7.3) ─▶ portapapeles ─▶ «copiadas N líneas»
      │                       └─ más de 1 MiB ─▶ «demasiado grande: w para guardarlo» (no copia)
      ├─ w ─▶ diálogo con ruta propuesta ─▶ ficheros.rs (create_new, 600) ─▶ REGISTRO historial_guardado
      │        └─ ya existe o sin permiso ─▶ error con la ruta (nunca sobrescribe)
      └─ q · Esc ─▶ vivo (la captura se libera)
```

### 4.5 Abrir un enlace

```
  prefijo o (u o en el modo historial)
      ▼
  enlaces.rs sobre lo visible del panel (líneas lógicas) ─▶ ninguno ─▶ «sin enlaces a la vista» (fin)
      ▼
  letras a-z sobre los enlaces (los 26 más cercanos al final) ─ Esc ─▶ (fin)
      ▼ letra
  diálogo ENLACE: URL completa con el host resaltado
      ├─ c ─▶ portapapeles (fin)
      ├─ ↵ ─▶ ¿http o https? ─no─▶ solo se puede copiar (fin)
      │       └─sí─▶ [terminal] abrir_enlaces <url>, sin shell, desde el cliente ─ falla ─▶ aviso con el error
      └─ t (solo si apunta a localhost, 127.0.0.1, ::1 o 0.0.0.0)
              ▼
         AbrirTunelTemporal{sesion, destino, puerto} ─▶ servidor: 127.0.0.1:<libre> → destino:puerto (T42)
              ├─ falla ─▶ aviso · REGISTRO tunel_fallido «temporal» (fin)
              ▼
         TunelTemporal{puerto_local} ─▶ abrir http://127.0.0.1:<puerto_local> con la misma ruta, consulta y fragmento
```

### 4.6 Paneles y abrir en rejilla

```
  prefijo Espacio ─▶ siguiente disposición ─▶ ¿cabe en la ventana (§7.6)?
      ├─ no ─▶ solo el panel enfocado + «disposición en espera: ventana pequeña» (vuelve al agrandar)
      ▼ sí
  reparto: los paneles nuevos reciben las pestañas siguientes a la enfocada que no estén a la vista
      ▼
  cada panel ─▶ Geometria (T46) ─▶ Redimensionar de su pestaña (mínimo entre ventanas, F3)

  paleta «abrir en rejilla…» ─▶ diálogo: hasta 4 hosts con casillas, o un grupo
      ├─ grupo con más de 4 hosts ─▶ se eligen 4 en las casillas
      ▼
  AbrirSesion de cada uno ─▶ 2: lado a lado · 3: 1 grande + 2 · 4: rejilla
```

### 4.7 Escritura en varios

```
  prefijo E ─▶ ¿2 o más paneles visibles con pestañas abiertas? ─no─▶ «hace falta más de un panel» (fin)
      ▼
  hosts distintos y comprobaciones activas (VERIFICACIONES_HOST) ─▶ ¿delibera (§7.7)?
      ├─ no ─▶ confirmación con el host y los paneles ─ Esc ─▶ (fin)
      └─ sí ─▶ deliberación MAGI (F6, PlanDeliberado::EscrituraVarios) ─ Esc ─▶ (fin)
      ▼ ↵ o Ctrl+K
  IniciarEscrituraVarios ─▶ el servidor recomprueba (§7.7) ─ Error ─▶ aviso (fin)
      ▼
  activa: aviso rojo fijo y » en cada panel · cada tecla y cada pegado ─▶ EscribirVarios (bytes por pestaña)
      ▼ prefijo E · otra disposición · otra pestaña en un panel · panel caído · salir de la vista · 10 min
  TerminarEscrituraVarios ─▶ servidor: ejecucion_resultado · REGISTRO escritura_varios_terminada
```

### 4.8 Ratón

```
  evento del terminal exterior (solo llega con la captura activa, §7.8)
      ▼
  ¿sobre el panel enfocado, y su remoto pidió ratón? ─sí─▶ raton.rs (su modo y codificación) ─▶ Teclas (fin)
      ▼ no
  ¿raton = true? ─no─▶ se ignora (fin)
      ▼ sí
  clic en otro panel ─▶ foco a ese panel (no se reenvía)
  rueda en la pantalla principal ─▶ modo historial (captura) y 3 líneas por paso
  rueda en la alternativa, sin ratón pedido ─▶ 3 flechas por paso (con el modo de cursor del remoto)
  arrastre en el modo historial ─▶ selección
```

### 4.9 Consultas y modos del remoto (en el servidor)

```
  Datos del host ─▶ parser vt100 con el emulador (responde = sí)
      ├─ consulta (§5.5) ─▶ respuesta escrita en el canal en el mismo lote, en orden con la salida
      ├─ DECSET/DECRST 1004 · 2026 · banderas de kitty ─▶ modos del remoto
      ├─ CSI 3 J ─▶ historial vacío (el lote se parte en esa secuencia)
      ├─ BEL · OSC 9 · OSC 777 ─▶ aviso (≤ 120 caracteres, uno cada 5 s) ─▶ Sesiones{lista}
      ├─ OSC 0 · OSC 2 ─▶ título (≤ 120 caracteres, saneado) ─▶ Sesiones{lista}
      ├─ OSC 52 de escritura ─▶ §4.10 · OSC 52 de lectura ─▶ nada
      └─ los mismos bytes ─▶ Datos a las ventanas adjuntas (su emulador lleva los modos sin responder)
```

### 4.10 Portapapeles del remoto

```
  OSC 52 ; c ; <base64> ─▶ decodificar (≤ 1 MiB; inválido ─▶ se descarta)
      ▼
  ¿HOSTS.portapapeles_remoto? (el servidor lo lee en cada petición)
      ├─ sí ─▶ Portapapeles{sesion, texto} a la ventana con la pestaña enfocada (o la última que la tuvo)
      │        ─▶ portapapeles del sistema u OSC 52 ─▶ aviso «web-01 copió 1,2 KB»
      └─ no ─▶ pendiente (solo la última, 60 s) ─▶ PortapapelesPedido{sesion, bytes} ─▶ aviso con ^] P
               prefijo P ─▶ el cliente pone portapapeles_remoto = 1 ─▶ PortapapelesPermitido ─▶ se entrega si sigue
```

### 4.11 Diagrama de secuencia: Claude Code arranca, se pega y se usa Mayús+Intro

```
terminal exterior   magi (cliente)                magi --servidor                         web-01: Claude Code
   │                      │                               │◀───────── CSI ? u · CSI c (consultas) │
   │                      │                               │ emulador: banderas 0 · DA1            │
   │                      │                               │─CSI ? 0 u · CSI ? 62;22 c────────────▶│
   │                      │                               │◀──────────── CSI > 1 u · CSI ? 2004 h │
   │                      │                               │◀───────────── CSI ? 2026 $ p (DECRQM) │
   │                      │                               │─CSI ? 2026 ; 2 $ y───────────────────▶│
   │                      │◀──── Datos (los mismos bytes) │                                       │
   │                      │ su emulador: bandera 1 · 2004 │                                       │
   │─pegado (5 líneas)───▶│                               │                                       │
   │                      │ pegado.rs: sanea · 200~ … 201~│                                       │
   │                      │─Teclas (el pegado solo)──────▶│─ESC[200~ … ESC[201~──────────────────▶│
   │                      │                               │ escritura propia, nada detrás         │
   │                      │                               │           [Pasted text #1 +5 lines]   │
   │─Mayús+Intro─────────▶│                               │                                       │
   │                      │ kitty: CSI 13 ; 2 u           │                                       │
   │                      │─Teclas (CSI 13;2u)───────────▶│─CSI 13;2u: salto de línea────────────▶│
   │                      │                               │◀────────── CSI ? 2026 h … pintado … l │
   │                      │◀─────────────────────── Datos │                                       │
   │                      │ pinta al cerrar (≤ 200 ms)    │                                       │
```

El servidor contesta antes de reenviar nada a las ventanas: Claude Code ve un terminal que admite el teclado de kitty y la salida sincronizada, y lo usa. El pegado viaja entero en sus propias escrituras, sin teclas detrás, porque algunas versiones de Claude Code pierden el pegado si llega en el mismo bloque que otra tecla.

### 4.12 Diagrama de secuencia: reconexión automática con tmux

```
magi (cliente)                  magi --servidor                                         web-01
   │                                    │ vigilante: conexión perdida (sin exit-status)     │
   │                                    │ causa recuperable · reconectar = 1                │
   │◀──── Sesiones (↻ intento 1 en 2 s) │                                                   │
   │                                    │─conectar sin diálogos (plazo 20 s)───────────────▶│
   │                                    │◀──────────────────────────────── sin ruta al host │
   │◀──── Sesiones (↻ intento 2 en 5 s) │                                                   │
   │                                    │─conectar sin diálogos────────────────────────────▶│
   │                                    │◀────────────────────────────────── conexión lista │
   │                                    │─exec terminal_tmux.sh comprobar magi-portatil-1──▶│
   │                                    │◀────────────── MAGI_TMUX=3.4 · MAGI_SESION=existe │
   │                                    │─PTY · terminal_tmux.sh abrir magi-portatil-1─────▶│
   │                                    │◀─────────────────── tmux redibuja: el sleep sigue │
   │                                    │ historial: «── reconectada 14:33 · tmux ──»       │
   │                                    │ REGISTRO sesion_reconectada (automática, 2)       │
   │◀───────────── Sesiones (● abierta) │                                                   │
   │◀───────────────── PantallaCompleta │                                                   │
```

### 4.13 Diagrama de secuencia: escritura en varios con deliberación

```
magi (cliente)                      magi --servidor                              web-01 · web-02 · db-01
   │ ^] E · 4 paneles · 3 hosts             │                                               │
   │ deliberación MAGI (F6)                 │                                               │
   │─Ejecutar (sondeo) · SFTP──────────────▶│─sondeo.sh · listar backups───────────────────▶│
   │                                        │◀───────────────────────── métricas · ficheros │
   │◀─────────────────────────── resultados │                                               │
   │ 9/9 APROBADA · Ctrl+K                  │                                               │
   │ DELIBERACIONES (cliente)               │                                               │
   │─IniciarEscrituraVarios(4 ses., delib.)▶│                                               │
   │                                        │ recomprueba: abiertas · 3 hosts               │
   │                                        │ deliberación válida y sin usar                │
   │◀────────── EscrituraVariosIniciada(id) │                                               │
   │ aviso rojo fijo en los 4 paneles       │                                               │
   │─EscribirVarios(id, bytes por sesión)──▶│─canal ×4─────────────────────────────────────▶│
   │                                        │◀───────────────────────── salida de cada host │
   │◀───────────────────────────── Datos ×4 │                                               │
   │ ^] E (o 10 min sin escribir)           │                                               │
   │─TerminarEscrituraVarios(id, apagada)──▶│                                               │
   │                                        │ ejecucion_resultado · REGISTRO                │
   │◀───────── EscrituraVariosTerminada(id) │                                               │
```

---

## 5. Script, Protocolo, Configuración y Teclas

### 5.1 `terminal_tmux.sh`: subórdenes

MAGI lo ejecuta con `sh -c "$SCRIPT" magi-tmux <suborden> [<nombre>]`, por un canal `exec` del pool (T42), salvo `abrir`, que va en el canal con PTY de la pestaña.

| Suborden | Argumentos | Qué devuelve |
|---|---|---|
| `comprobar` | `<nombre>` | `MAGI_TMUX=<versión>\|no` y `MAGI_SESION=existe\|nueva` |
| `abrir` | `<nombre>` | Sustituye el proceso por `tmux new-session -A -s <nombre>`; sin tmux, por el shell de acceso (`${SHELL:-/bin/sh} -l`) |
| `listar` | — | `MAGI_TMUX=<versión>\|no` y una línea `MAGI_SESION <nombre> <enganchada> <ventanas> <creada>` por sesión, separadas por tabuladores (`list-sessions -F`) |
| `terminar` | `<nombre>` | `MAGI_RESULTADO=ok\|no_existe` tras `kill-session -t <nombre>` |

El nombre se valida en Rust y en el script (§7.10) y va siempre como argumento, nunca dentro del texto del script. Lo que devuelve `listar` es entrada no fiable: 200 sesiones como mucho, nombres de hasta 64 caracteres y líneas de hasta 1 KiB.

### 5.2 Protocolo cliente ⇄ servidor (`VERSION_PROTOCOLO = 9`)

| Mensaje | Sentido | Contenido |
|---|---|---|
| `CapturarHistorial` → `HistorialLote` | cliente ⇄ servidor | `peticion_id`, `sesion_id` → `lote`, `ultimo`, `total` y líneas en tramos con `envuelta`, cortadas por tamaño codificado |
| `Foco` | cliente → servidor | `sesion_id`, `enfocada`: la ventana tiene o deja de tener esa pestaña en el panel enfocado, con el terminal exterior enfocado |
| `IniciarEscrituraVarios` → `EscrituraVariosIniciada` | cliente ⇄ servidor | `peticion_id`, pestañas y `deliberacion_id?` → `id` (o `Error`) |
| `EscribirVarios` | cliente → servidor | `id` y, por pestaña concedida, sus bytes ya codificados |
| `TerminarEscrituraVarios` / `EscrituraVariosTerminada` | cliente ⇄ servidor | `id` y motivo; el servidor también la termina si cae una pestaña o pasan 10 min sin envíos |
| `AbrirTunelTemporal` → `TunelTemporal` | cliente ⇄ servidor | `peticion_id`, `sesion_id`, `destino` (`localhost`, `127.0.0.1` o `::1`) y `puerto` → `id` y `puerto_local` |
| `CerrarTunelTemporal` | cliente → servidor | `id` |
| `TunelesTemporales{lista}` | servidor → todos | Lista completa en cada cambio (T27): id, pestaña, host, puerto local, destino, puerto y conexiones |
| `ListarTmux` → `SesionesTmux` | cliente ⇄ servidor | `peticion_id`, `host_id` → sesiones (nombre, enganchada, ventanas, creada) o `sin_tmux` |
| `TerminarTmux` → `TmuxTerminada` | cliente ⇄ servidor | `peticion_id`, `host_id`, `nombre` → `ok` o `no_existe` |
| `Portapapeles` | servidor → una ventana | `sesion_id` y texto (≤ 1 MiB), a la ventana con la pestaña enfocada |
| `PortapapelesPedido` | servidor → ventanas adjuntas | `sesion_id` y tamaño, sin el contenido |
| `PortapapelesPermitido` | cliente → servidor | `sesion_id`: entrega la petición pendiente si no han pasado 60 s |
| `RenombrarSesion` / `MoverSesion` | cliente → servidor | `sesion_id` y nombre / posición; el resultado llega en `Sesiones{lista}` |
| `Adjuntar` con `colores` | cliente → servidor | Campo opcional nuevo de la F3: primer plano, fondo y cursor del tema, para responder a OSC 10, 11 y 12 |
| `AbrirSesion` con `tmux` | cliente → servidor | Campo opcional nuevo: el nombre de una sesión de tmux existente (desde la lista) |
| `Sesiones{lista}` | servidor → todos | Campos nuevos por pestaña: `reconexion{intento, proximo_en, desde, causa}`, `titulo`, `aviso{texto, hora}` y `tmux` |

`TrabajoServidor` gana `tuneles_temporales`: un servidor con túneles temporales no se sustituye sin preguntar (T67). La escritura en varios no es trabajo custodiado: muere con su ventana. Los `peticion_id` del terminal (capturas, tmux y túneles temporales) empiezan en 2^49, por encima de los demás rangos.

Plazos en el servidor (T32): 5 s para una captura; 10 s para `comprobar`, `listar`, `terminar` y abrir un túnel temporal; 20 s por intento de reconexión. Las respuestas a consultas no esperan a nada: salen en el mismo lote.

### 5.3 Configuración (`config.toml`)

| Clave | Por defecto | Uso |
|---|---|---|
| `[terminal] historial_lineas` | `10000` | Líneas de historial por pestaña (0-100 000; `0` = sin historial) |
| `[terminal] historial_mib` | `64` | Tope de memoria del historial de una pestaña (8-512); manda sobre las líneas (§7.1) |
| `[terminal] raton` | `false` | Usos del ratón propios de MAGI: rueda en el historial, clic para elegir panel y arrastre en el modo historial |
| `[terminal] teclado_mejorado` | `true` | Pide al terminal exterior la desambiguación de kitty si la admite (para distinguir Mayús+Intro) |
| `[terminal] avisar_pegado` | `true` | Aviso al pegar varias líneas en un remoto sin pegado entre corchetes |
| `[terminal] abrir_enlaces` | `"xdg-open"` (macOS: `"open"`) | Programa que abre un enlace, sin shell; recibe la URL como único argumento |
| `[terminal] reconexion_max_min` | `15` | Minutos de reconexión automática antes de rendirse (1-120) |

Sigue `[terminal] comando` (F3). Un valor fuera de rango no impide arrancar: se acota donde se usa y MAGI avisa al arrancar (D261).

### 5.4 Teclas

**Vista Sesión (prefijo `Ctrl+]`)**

| Tecla | Acción |
|---|---|
| prefijo · `1`-`9` · `n` · `p` | Pestaña en el panel enfocado (si ya está en otro panel, se intercambian) (F3, ampliada) |
| prefijo · `←` `↑` `→` `↓` | Cambiar de panel |
| prefijo · `Espacio` | Siguiente disposición |
| prefijo · `z` | Ampliar el panel enfocado y volver |
| prefijo · `[` | Modo historial |
| prefijo · `/` | Modo historial con la búsqueda abierta |
| prefijo · `o` | Enlaces |
| prefijo · `E` | Escritura en varios: activar o parar |
| prefijo · `P` | Permitir el portapapeles a este host (con un aviso pendiente) |
| prefijo · `,` | Renombrar la pestaña |
| prefijo · `<` · `>` | Mover la pestaña a la izquierda o a la derecha |
| prefijo · `r` · `x` | Reintentar ya · cerrar (también durante la reconexión) (F3) |
| `Shift+RePág` | Modo historial, ya subiendo (solo con la pantalla principal; con la alternativa va al remoto) |

El resto de teclas sigue yendo al remoto, también `RePág`, `AvPág`, `Ctrl+J` y `Alt+Intro`. Mayús+Intro llega distinta solo si el terminal exterior admite la desambiguación de kitty.

**Modo historial**

| Tecla | Acción |
|---|---|
| `↑` `↓` · `j` `k` | Una línea |
| `RePág` `AvPág` · `Ctrl+U` `Ctrl+D` | Una página · media página |
| `g` · `G` | Principio · final de la captura |
| `/` | Buscar (hacia arriba desde donde se está); `Tab` alterna texto y expresión regular; `↵` acepta, `Esc` cancela |
| `n` · `N` | Coincidencia siguiente (más arriba) · anterior (más abajo) |
| `v` · `V` | Seleccionar por caracteres · por líneas |
| `y` · `↵` | Copiar la selección |
| `w` | Guardar la captura en un fichero |
| `o` | Enlaces de lo visible |
| `q` · `Esc` | Volver al vivo (`Esc` cancela antes la selección o la búsqueda) |

**Enlaces**: `a`-`z` elige; en el diálogo, `↵` abre, `c` copia, `t` abre por túnel temporal (solo con `localhost`) y `Esc` sale.

**Sesiones** (`prefijo l`, F3): `t` lista las sesiones de tmux del host de la pestaña elegida; en esa lista, `↵` abre una pestaña enganchada, `T` termina (confirmación) y `r` refresca. `,` renombra la pestaña elegida.

### 5.5 Consultas del remoto y respuestas del servidor

| Consulta | Respuesta |
|---|---|
| DA1 `CSI c` · `CSI 0 c` | `CSI ? 62 ; 22 c` |
| DA2 `CSI > c` | `CSI > 0 ; <MAYOR·10 000 + MENOR·100 + PARCHE> ; 0 c` (1.4.0 → 10400) |
| DSR `CSI 5 n` · `CSI 6 n` | `CSI 0 n` · `CSI <fila> ; <columna> R` (posición tras lo procesado hasta la consulta) |
| DECRQM `CSI ? <modo> $ p` | `CSI ? <modo> ; <1 activo · 2 inactivo · 0 no reconocido> $ y` para 1, 25, 47, 1000, 1002, 1003, 1004, 1005, 1006, 1049, 2004 y 2026 |
| Teclado de kitty `CSI ? u` | `CSI ? <banderas> u` de la pantalla activa (solo la bandera 1) |
| XTVERSION `CSI > 0 q` · `CSI > q` | `DCS > \| MAGI <versión> ST` |
| OSC 10 · 11 · 12 con `?` | `OSC 1x ; rgb:RRRR/GGGG/BBBB ST` con los colores del tema de la ventana (o la paleta de respaldo) |
| XTWINOPS `CSI 18 t` | `CSI 8 ; <filas> ; <columnas> t` |
| OSC 52 con `?` (leer el portapapeles) | Ninguna, nunca |

Una consulta que no está en la tabla no se contesta, como en tmux. Ninguna respuesta lleva datos del equipo más allá de la versión, el tamaño y los colores.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
Vista SESIÓN (F3)
├── pestañas: prefijo 1-9 · n · p · , renombrar · < > mover
├── paneles: prefijo Espacio disposición · prefijo ←↑→↓ foco · prefijo z ampliar
├── prefijo [ · prefijo / · Shift+RePág ──▶ MODO HISTORIAL
│     ├── / ──▶ BÚSQUEDA (Tab texto o regex · ↵ · Esc)
│     ├── v · V ──▶ SELECCIÓN ──▶ y · ↵ copiar
│     ├── w ──▶ diálogo GUARDAR HISTORIAL
│     └── o ──▶ ENLACES
├── prefijo o ──▶ ENLACES ──▶ letra ──▶ diálogo ENLACE (↵ abrir · c copiar · t túnel temporal)
├── prefijo E ──▶ deliberación MAGI (F6) o confirmación ──▶ ESCRITURA EN VARIOS (aviso rojo)
├── prefijo P ──▶ permitir el portapapeles al host (con aviso pendiente)
├── pegar ──▶ (si hace falta) diálogo PEGAR VARIAS LÍNEAS
├── pestaña caída ──▶ RECONECTANDO (cuenta atrás) · prefijo r · prefijo x
└── prefijo l ──▶ SESIONES
      └── t ──▶ SESIONES DE TMUX DEL HOST (↵ abrir · T terminar · r refrescar)
Paleta: abrir en rejilla… · disposición · historial · buscar · sesiones de tmux · escritura en varios
Ficha de host: bloque TERMINAL (reconectar · tmux · portapapeles), solo en este equipo
```

### 6.1 Rejilla de cuatro paneles con escritura en varios

```
┌ MAGI · SESIÓN · REJILLA ─────────────────────────────────────────────────────┐
│ ● web-01 │ ● web-02 │ ● db-01 │ ● web-01 (2) │ ◐ nas │ + nueva               │
├─ ESCRIBIENDO EN 4 PANELES · ^] E para parar ─────────────────────────────────┤
│▸● web-01 » ──────────────────────────┬ ● web-02 » ───────────────────────────┤
│ usuario@web-01:~$ sudo apt update    │ usuario@web-02:~$ sudo apt update     │
│ [sudo] contraseña para usuario:      │ [sudo] contraseña para usuario:       │
│                                      │                                       │
│                                      │                                       │
├ ● db-01 » ───────────────────────────┼ ● web-01 (2) » ───────────────────────┤
│ usuario@db-01:~$ sudo apt update     │ usuario@web-01:~$ sudo apt update     │
│ [sudo] contraseña para usuario:      │ [sudo] contraseña para usuario:       │
│                                      │                                       │
│                                      │                                       │
├──────────────────────────────────────────────────────────────────────────────┤
│ » 4 paneles · 3 hosts · deliberación aprobada 9/9 · ^] E parar               │
│ ^] ←↑→↓ panel · ^] z ampliar · ^] Espacio disposición · ^] [ historial       │
└──────────────────────────────────────────────────────────────────────────────┘
```

`▸` marca el panel enfocado y `»` los que reciben lo que se escribe. La línea roja de arriba no se puede ocultar mientras la escritura en varios está activa.

### 6.2 Disposiciones

```
 1 · completa      2 · lado a lado   2 · apilados      1 grande + 2      rejilla de 4
 ┌────────────┐    ┌─────┬──────┐    ┌────────────┐    ┌──────┬─────┐    ┌─────┬──────┐
 │            │    │     │      │    │     A      │    │      │  B  │    │  A  │  B   │
 │     A      │    │  A  │  B   │    ├────────────┤    │  A   ├─────┤    ├─────┼──────┤
 │            │    │     │      │    │     B      │    │      │  C  │    │  C  │  D   │
 │            │    │     │      │    │            │    │      │     │    │     │      │
 └────────────┘    └─────┴──────┘    └────────────┘    └──────┴─────┘    └─────┴──────┘
 mínimo 40×12       mínimo 60×12       mínimo 40×16       mínimo 80×18       mínimo 80×20
```

Cada panel tiene una línea de cabecera con el glifo y el nombre de su pestaña. Por debajo del mínimo de una disposición, solo se ve el panel enfocado con el aviso «disposición en espera: ventana pequeña».

### 6.3 Modo historial con búsqueda y selección

```
┌ MAGI · SESIÓN · HISTORIAL ───────────────────────────────────────────────────┐
│ ● web-01 │ ◐ db-01 │ + nueva                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ 14:02:09 GET /api/pedidos 200 11 ms                                          │
│ 14:02:10 GET /api/clientes 200 9 ms                                          │
│▸14:02:11 POST /api/pedidos 500 3 ms ⟨ERROR⟩ conexión rechazada (db-01:5432)  │
│ 14:02:11 ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒     │
│ 14:02:12 GET /api/pedidos 200 12 ms                                          │
│ 14:02:15 POST /api/pedidos 500 4 ms ⟨ERROR⟩ conexión rechazada (db-01:5432)  │
│ 14:02:15 reintento 1/3 en 2 s                                                │
│ 14:02:17 POST /api/pedidos 201 48 ms                                         │
├──────────────────────────────────────────────────────────────────────────────┤
│ /error · texto · 3 de 17 · líneas 8 412-8 420 de 10 000 · ↓ salida nueva     │
│ v selec. · y copiar · w guardar · o enlaces · n N · Tab regex · q volver     │
└──────────────────────────────────────────────────────────────────────────────┘
```

Las coincidencias van resaltadas (aquí, entre `⟨ ⟩`), la actual con `▸`, y la selección en vídeo inverso (aquí, `▒`). La barra dice la búsqueda, el tipo, la coincidencia, las líneas a la vista y si ha llegado salida nueva desde la captura.

### 6.4 Enlaces

```
┌ MAGI · SESIÓN · ENLACES ─────────────────────────────────────────────────────┐
│ ● web-01 │ ◐ db-01 │ + nueva                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ usuario@web-01:~/app$ npm run dev                                            │
│   VITE listo en 412 ms                                                       │
│   Local:   [a]http://localhost:5173/                                         │
│   Red:     [b]http://192.0.2.10:5173/                                        │
│   Guía:    [c]https://ejemplo.org/guia#arranque                              │
│          ┌ ENLACE a ──────────────────────────────────────────────┐          │
│          │ http://localhost:5173/                                 │          │
│          │ apunta a localhost de web-01                           │          │
│          │ t abrir por un túnel temporal · c copiar · Esc         │          │
│          └────────────────────────────────────────────────────────┘          │
├──────────────────────────────────────────────────────────────────────────────┤
│ ENLACES 3 · pulsa la letra · Esc salir                                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.5 Pestaña reconectando

```
┌ MAGI · SESIÓN ───────────────────────────────────────────────────────────────┐
│ ↻ web-01 │ ● db-01 │ + nueva                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ (la última pantalla, en gris)                                                │
│                                                                              │
│          ↻ Reconectando con web-01 · intento 3 en 8 s                        │
│            caída hace 41 s: conexión perdida (el host no responde)           │
│            tmux magi-portatil-1: lo que corría sigue en el host              │
│                                                                              │
│            ^] r  reintentar ya        ^] x  cerrar                           │
│                                                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ ↻ 1/2 · reconectando · tmux magi-portatil-1 · se para a los 15 min           │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.6 Sesiones con título y tmux

```
┌ MAGI · SESIONES ─────────────────────────────────────────────────────────────┐
│  ESTADO  PESTAÑA       HOST     TIEMPO  TÍTULO                       VENT.   │
│  ──────────────────────────────────────────────────────────────────────────  │
│▸ ● !     api           web-01   2 h     Claude Code                  1       │
│  ↻       web-01 (2)    web-01   41 s    reconectando · intento 3     0       │
│  ●       db-01         db-01    18 m    usuario@db-01: ~             1       │
│  ──────────────────────────────────────────────────────────────────────────  │
│  api · aviso 14:31: Claude necesita tu permiso                               │
│  TMUX EN web-01                                                              │
│   magi-portatil-1    enganchada   2 ventanas   creada hace 3 h               │
│   magi-portatil-2    libre        1 ventana    creada ayer                   │
│   despliegue         libre        1 ventana    creada hace 2 d               │
├──────────────────────────────────────────────────────────────────────────────┤
│ ↵ entrar · t tmux · , renombrar · r reconectar · x cerrar · S apagar         │
└──────────────────────────────────────────────────────────────────────────────┘
```

El título es el que pone el programa remoto (Claude Code pone el suyo); `!` marca un aviso sin ver, cuyo texto sale debajo.

### 6.7 Aviso de pegado y permiso de portapapeles

```
┌ MAGI · SESIÓN ───────────────────────────────────────────────────────────────┐
│ ● web-01 │ + nueva                                                           │
├──────────────────────────────────────────────────────────────────────────────┤
│ usuario@web-01:~$                                                            │
│          ┌ PEGAR VARIAS LÍNEAS ───────────────────────────────────┐          │
│          │ Vas a pegar 5 líneas en web-01 y el programa no        │          │
│          │ protege el pegado: cada línea se ejecutaría al llegar. │          │
│          │   1  cd /srv/app                                       │          │
│          │   2  git pull                                          │          │
│          │   … 3 más                                              │          │
│          │ ↵ pegar · Esc cancelar                                 │          │
│          └────────────────────────────────────────────────────────┘          │
├──────────────────────────────────────────────────────────────────────────────┤
│ web-01 quiere copiar 1,2 KB al portapapeles · ^] P permitir en este host     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.8 Ficha de host: bloque TERMINAL

```
 ┌─ TERMINAL (solo en este equipo) ─────────────────────────┐
 │ [x] Reconectar solo si se cae la red                     │
 │ [ ] Sesión persistente con tmux                          │
 │ [ ] Permitir que el host copie al portapapeles           │
 │     (OSC 52; leerlo, nunca)                              │
 │ Espacio marca · Tab siguiente · Ctrl+S guardar           │
 └──────────────────────────────────────────────────────────┘
```

Solo en hosts de tipo `ssh`. El bloque dice «solo en este equipo» porque estos ajustes no viajan con la sincronización.

### Marcas de las pestañas y de los paneles

| Marca | Texto en la barra | Significado |
|---|---|---|
| `●` | abierta | Pestaña abierta y a la vista (F3) |
| `◐` | actividad | Salida nueva en una pestaña que no se ve (F3) |
| `✕` | caída | Caída sin reconexión, o reconexión agotada (F3) |
| `↻` | reconectando | Reconexión automática en curso, con la cuenta atrás |
| `!` | aviso | Campana, OSC 9 u OSC 777 sin ver; el texto sale en la barra |
| `▸` | enfocado | Panel enfocado |
| `»` | escribiendo en varios | Panel que recibe la escritura en varios |

En modo ASCII (`disposicion::texto_ascii`): `↻` → `@`, `▸` → `>`, `»` → `>>`; `!` no cambia.

### Notas de UX

- **Nada se escribe en el remoto por sorpresa:** el modo historial y el de enlaces se quedan con sus teclas; la escritura en varios no se puede activar sin ver el aviso, y su línea roja no desaparece hasta pararla.
- **El historial no se mueve solo:** lo que llega mientras se mira se cuenta («↓ salida nueva»), y `q` vuelve al vivo.
- **La reconexión se ve y se puede cortar:** cuenta atrás en la pestaña, causa y `prefijo r` o `prefijo x` en todo momento.
- **El terminal exterior queda como estaba:** al salir, al ceder la terminal (visor, paginador) y tras un pánico, MAGI quita el pegado entre corchetes, el foco, las banderas de kitty y la captura del ratón que pidió.
- **Con el ratón capturado** (porque el remoto lo pidió o con `raton = true`), la selección propia del terminal exterior sigue con Mayús+arrastre; la ayuda lo dice.
- Todas las vistas y modos nuevos tienen modo estrecho e instantáneas a 40×12, 80×24 y 200×60 (T45).

---

## 7. Lógica de Negocio

### 7.1 Historial: cuántas líneas

```python
BYTES_POR_CELDA = 40      # vt100 0.16; la prueba terminal_historial lo mide y falla si cambia

def lineas_de_historial(cfg, columnas):
    if cfg.historial_lineas == 0:
        return 0
    por_bytes = cfg.historial_mib * 1024 * 1024 // (max(columnas, 2) * BYTES_POR_CELDA)
    return max(0, min(cfg.historial_lineas, por_bytes))
```

Con los valores por defecto (10 000 líneas, 64 MiB): 10 000 líneas hasta 167 columnas; 8 388 con 200; 5 592 con 300. El tope se recalcula al redimensionar la pestaña: si baja, se pierden las líneas más viejas.

- El historial es el de la **pantalla principal** (el que guarda vt100). La pantalla alternativa (Vim, htop, tmux, Claude Code a pantalla completa) no tiene historial en MAGI: cada programa lleva el suyo.
- `CSI 3 J` (borrar el historial guardado) lo vacía. vt100 0.16 no sabe hacerlo, así que el servidor parte el lote en esa secuencia y rehace el parser con su estado (`state_formatted`). Claude Code en modo clásico lo usa al redibujar tras un cambio de tamaño, y así no quedan conversaciones duplicadas.
- Al reconectar se conserva el historial, y el servidor añade en su parser (y en las ventanas adjuntas, nunca en el remoto) una línea atenuada: `── reconectada 14:33 · shell nuevo ──` o `── reconectada 14:33 · tmux magi-portatil-1 ──`.
- Una captura es el historial más la pantalla, en tramos (F11, §7.7), con `envuelta` en cada línea que siguió en la siguiente por el ancho.

### 7.2 Búsqueda

```python
def buscar(lineas, patron, es_regex, desde, hacia_arriba=True):
    if not 1 <= len(patron) <= 256:
        raise Invalido("patrón vacío o demasiado largo")
    sensible = any(c.isupper() for c in patron)      # como `less -i`: mayúsculas solo si las escribes
    expr = regex.compile(patron if es_regex else regex.escape(patron),
                         ignore_case=not sensible, size_limit=1 << 20)
    for n in recorrido(lineas, desde, hacia_arriba):  # líneas lógicas: las envueltas, unidas
        for m in expr.finditer(lineas[n].texto):
            yield (n, m.start(), m.end())
```

La búsqueda empieza donde está la vista y va hacia arriba (lo más viejo), porque el modo historial se abre abajo. Cuenta hasta 10 000 coincidencias («más de 10 000»). Una expresión no válida se dice en la barra y no busca.

### 7.3 Selección, copia y portapapeles

```python
def texto_seleccion(lineas, inicio, fin, por_lineas):
    partes = []
    for n in range(inicio.linea, fin.linea + 1):
        t = lineas[n].texto
        a = 0 if por_lineas or n > inicio.linea else inicio.col
        b = len(t) if por_lineas or n < fin.linea else fin.col + 1
        trozo = t[a:b]
        ultima = n == fin.linea
        partes.append(trozo if lineas[n].envuelta and not ultima else trozo.rstrip(" ") + "\n")
    return "".join(partes).rstrip("\n")

def copiar(texto, entorno):                              # en el cliente
    if len(texto.encode()) > 1 << 20:
        raise Aviso("demasiado grande para copiar: w para guardarlo")
    orden = entorno.orden_portapapeles()                 # wl-copy (Wayland) o pbcopy (macOS), como en la F2
    if orden and not entorno.dentro_de_ssh:              # SSH_CONNECTION o SSH_TTY en el entorno
        return ejecutar_sin_shell(orden, entrada=texto)
    return terminal_exterior.escribir(f"\x1b]52;c;{base64(texto)}\x07")   # OSC 52 hacia fuera
```

Si la orden falla, se usa OSC 52 y se dice. Ninguna copia va al log ni a REGISTRO.

### 7.4 Portapapeles del remoto (OSC 52)

```python
PENDIENTE_SEG = 60

def osc52(sesion, datos, ahora):                         # en el servidor
    if datos == "?":
        return None                                      # leer el portapapeles: nunca
    texto = decodificar_base64(datos, maximo=1 << 20)    # inválido o grande → None
    if texto is None:
        return None
    if almacen.host(sesion.host_id).portapapeles_remoto:     # se lee en cada petición
        return Entregar(sesion.ventana_enfocada(), texto)    # o la última que la tuvo enfocada
    sesion.portapapeles_pendiente = (texto, ahora + PENDIENTE_SEG)   # solo la última
    return Avisar(sesion.ventanas_adjuntas(), len(texto))
```

El selector de OSC 52 (`c`, `p`, `s`…) da igual: todo va al portapapeles. Cada copia del remoto se avisa en la barra con el host y el tamaño.

### 7.5 Enlaces y túnel temporal

```python
ENLACE = r"https?://[^\s<>\"'`\x00-\x1f\x7f]+"
FINAL = ".,;:!?)]}'\""
LOCALES = {"localhost", "127.0.0.1", "::1", "0.0.0.0"}

def enlaces(lineas_logicas):
    for n, texto in lineas_logicas:
        for m in regex.finditer(ENLACE, texto):
            url = m.group(0)
            while url and url[-1] in FINAL and not cierra_un_parentesis(url):
                url = url[:-1]
            if 10 <= len(url) <= 2048:
                yield (n, m.start(), url)

def destino_del_tunel(url):
    u = analizar(url)
    if u.esquema not in ("http", "https") or u.host not in LOCALES:
        return None
    destino = "127.0.0.1" if u.host == "0.0.0.0" else u.host
    return destino, u.puerto or (443 if u.esquema == "https" else 80)
```

- Las letras van de la `a` a la `z` sobre los 26 enlaces más cercanos al final de lo visible.
- Abrir: `[terminal] abrir_enlaces` con la URL como único argumento, desde el cliente y sin shell. Una URL que no es `http` ni `https` solo se puede copiar.
- Túnel temporal: el servidor escucha en `127.0.0.1` en un puerto libre y reenvía al destino por `direct-tcpip` en la conexión del host (F5). La URL abierta es la misma con `127.0.0.1:<puerto local>`: ruta, consulta y fragmento no cambian. Con `https`, el navegador avisará del certificado: es lo esperado.

### 7.6 Paneles

| Disposición | Paneles | Mínimo de la vista | Reparto |
|---|---|---|---|
| completa | 1 | 40×12 | Todo |
| lado a lado | 2 | 60×12 | Dos columnas, la izquierda redondeada hacia abajo |
| apilados | 2 | 40×16 | Dos filas |
| 1 grande + 2 | 3 | 80×18 | Izquierda la mitad; a la derecha, dos filas |
| rejilla | 4 | 80×20 | Dos por dos |

- Entre paneles hay una columna o una fila de separación, y cada panel usa su primera fila como cabecera. El PTY de la pestaña recibe el resto (saneado: mínimo 2×1, `alto_pty`), por `Geometria` (T46). Con la misma pestaña en otra ventana, manda el mínimo (F3).
- Al cambiar a una disposición con más paneles, los nuevos reciben las pestañas siguientes a la enfocada que no estén a la vista; si no hay, el panel queda vacío con «prefijo 1-9 para elegir pestaña».
- `prefijo z` amplía el enfocado como «completa» sin perder la disposición; otra vez, vuelve.
- Una pestaña está como mucho en un panel de cada ventana: elegirla en otro panel las intercambia.

### 7.7 Escritura en varios

```python
INACTIVIDAD_SEG = 10 * 60

def delibera(pestanas, verificaciones):                           # en el cliente; lo repite el servidor
    hosts = {p.host_id for p in pestanas}
    return len(hosts) > 1 or any(verificaciones[h].alguna_activa() for h in hosts)   # T41

def conceder(servidor, cliente, peticion):                        # IniciarEscrituraVarios
    pestanas = [servidor.sesion(i) for i in peticion.sesiones]
    if len(pestanas) < 2 or any(p is None or p.estado != "abierta" for p in pestanas):
        raise Rechazo("todas las pestañas tienen que estar abiertas")
    if delibera(pestanas, servidor.verificaciones()):
        d = servidor.deliberacion(peticion.deliberacion_id)
        if (d is None or d.usada or d.resultado not in ("aprobada", "forzada")
                or d.plan != EscrituraVarios(hosts=sorted({p.host_id for p in pestanas}))):
            raise Rechazo("deliberación no válida o ya usada")
        d.marcar_usada()                                          # una deliberación, una concesión
    return Concesion(cliente, pestanas, desde=ahora())
```

- Cada `EscribirVarios` lleva los bytes de cada pestaña, codificados por el cliente con los modos de esa pestaña (teclado de kitty, pegado entre corchetes): Mayús+Intro puede ser `CSI 13;2u` en Claude Code y `CR` en bash en el mismo envío. Si nombra una pestaña no concedida, se rechaza entero.
- Termina con `prefijo E`, al cambiar de disposición o la pestaña de un panel, si cae una pestaña, al salir de la vista Sesión, a los 10 min sin envíos (en el cliente y en el servidor) y si la ventana se cierra.
- No se difunden las teclas del prefijo ni el ratón. El pegado sí, con el aviso de §7.12 por cada pestaña que no proteja el pegado.

### 7.8 Ratón

```python
def capturar(cfg, panel_enfocado):
    return cfg.raton or (panel_enfocado and panel_enfocado.modo == "vivo"
                         and panel_enfocado.modos.raton_pedido())     # 1000, 1002 o 1003

def al_evento(ev, cfg, paneles):
    p = paneles.en(ev.columna, ev.fila)
    if p is paneles.enfocado and p.modo == "vivo" and p.modos.raton_pedido():
        return enviar(p, raton.codificar(ev.relativo_a(p.contenido), p.modos))   # su modo y codificación
    if not cfg.raton:
        return None
    if ev.clic and p is not paneles.enfocado:
        return paneles.enfocar(p)
    if ev.rueda:
        if p.pantalla_alternativa():
            return enviar(p, flechas(3 * ev.pasos, p.modos.cursor_aplicacion))
        return p.historial_desplazar(3 * ev.pasos)       # abre el modo historial si hace falta
    if ev.arrastre and p.modo == "historial":
        return p.seleccionar_hasta(ev)
```

- La captura del terminal exterior se pide y se suelta según `capturar` al cambiar de panel, de modo, de vista o los modos del remoto.
- Coordenadas desde 1, relativas al contenido del panel (sin su cabecera). Con la codificación por defecto, más allá de la columna o fila 223 no se envía nada.
- Mientras MAGI captura, la selección propia del terminal exterior sigue con Mayús+arrastre.

### 7.9 Reconexión automática

```python
ESPERAS = [2, 5, 10, 30]                    # segundos; después, 60 cada vez

def espera(intento):                        # intento ≥ 1
    return ESPERAS[intento - 1] if intento <= len(ESPERAS) else 60

RECUPERABLES = {"conexion_perdida", "plazo_agotado", "keepalive_sin_respuesta",
                "conexion_rechazada", "sin_ruta", "dns_sin_respuesta"}
# no recuperables: exit, credenciales, huella_nueva, huella_cambiada, frase_pendiente,
#                  cerrada_por_el_usuario, host_borrado

SUSPENSION_SEG = 30

def vuelve_de_suspension(antes, ahora):
    # el reloj monotónico no avanza con el equipo suspendido; el de pared, sí
    return (ahora.pared - antes.pared) - (ahora.monotono - antes.monotono) > SUSPENSION_SEG
```

- Cada intento pide la conexión con `conexion_para_canal` (T42), sin diálogos: solo contraseñas del llavero (`PideLlavero`, T49), solo huellas ya aceptadas y solo claves sin frase o en el agente. Plazo de 20 s por intento.
- Varias pestañas del mismo host se reconectan cada una con su cuenta atrás, pero comparten la conexión del pool: la primera que la abre se la deja a las demás, y esperar al cerrojo no cuenta como fallo.
- `reconexion_max_min` cuenta desde la caída. Al agotarse: `caida` con «reconexión agotada» y REGISTRO `reconexion_agotada`.
- Los túneles y el SFTP del host no se reconectan solos (F5, F4).

### 7.10 tmux

```python
NOMBRE = r"^[A-Za-z0-9_-]{1,64}$"            # lo mismo en Rust y en el script

def etiqueta_equipo(nombre_equipo):          # «Portátil de Hector» → «portatil-de-hector»
    t = quitar_tildes(nombre_equipo).lower()
    t = re.sub(r"[^a-z0-9]+", "-", t).strip("-")[:20]
    return t or "equipo"

def nombre_nuevo(equipo, existentes):         # existentes: lo que devuelve `listar`
    prefijo = f"magi-{etiqueta_equipo(equipo)}-"
    usados = {int(n[len(prefijo):]) for n in existentes
              if n.startswith(prefijo) and n[len(prefijo):].isdigit()}
    n = 1
    while n in usados:
        n += 1
    return prefijo + str(n)
```

- El equipo es el de la sincronización (F10) o, sin sincronización, el nombre del sistema.
- Pestaña nueva: `listar` y nombre libre; reconexión: `comprobar` y `abrir` con el mismo nombre; desde la lista: `abrir` con el nombre elegido (solo si cumple `NOMBRE`; si no, se ve con «nombre no admitido»).
- El snippet al conectar y el comando inicial (F6) solo se escriben en una sesión nueva.
- Cerrar una pestaña con tmux (`prefijo x`) pregunta: «la sesión magi-portatil-1 sigue viva en web-01 · ↵ cerrar · T cerrar y terminarla · Esc».
- Si el shell del host ya arranca tmux, la opción daría tmux dentro de tmux: la ficha lo advierte.

### 7.11 Compatibilidad con Claude Code y las TUI modernas

| Modo o secuencia | Quién lo lleva | Qué hace MAGI |
|---|---|---|
| 2004 pegado entre corchetes | vt100 | Marca el pegado (§7.12) |
| 1000, 1002, 1003 y 1005, 1006 (ratón) | vt100 | Captura y reenvío (§7.8) |
| 1 cursor de aplicación | vt100 | Flechas como en la F11, también las de la rueda |
| 47, 1049 pantalla alternativa | vt100 | Sin historial; `Shift+RePág` va al remoto |
| 1004 foco | emulador | El servidor envía `CSI I` y `CSI O` cuando cambia el foco agregado |
| 2026 salida sincronizada | emulador | El cliente no pinta la pestaña hasta el cierre o 200 ms |
| Teclado de kitty (`CSI > u`, `CSI < u`, `CSI = u`) | emulador, una pila por pantalla (16 como mucho) | Codifica las teclas con la bandera 1; las demás banderas se ignoran |
| ED 3 (`CSI 3 J`) | emulador | Vacía el historial (§7.1) |
| BEL, OSC 9, OSC 777 | emulador | Aviso y `!` en la pestaña |
| OSC 0, OSC 2 | vt100 (callback) | Título |
| OSC 52 | vt100 (callback) | §7.4 |

**Teclado de kitty, bandera 1 (desambiguar)**, como lo hacen Ghostty y kitty:

| Tecla | Sin banderas | Con la bandera 1 |
|---|---|---|
| Intro · Tab · Retroceso | `CR` · `HT` · `DEL` | Igual |
| Mayús+Intro · Ctrl+Intro · Alt+Intro | `CR` · `CR` · `ESC CR` | `CSI 13;2u` · `CSI 13;5u` · `CSI 13;3u` |
| Esc | `ESC` | `CSI 27u` |
| Ctrl+tecla · Alt+tecla · Ctrl+Alt+tecla | Clásico (F11) | `CSI <código>;<1 + modificadores>u` (Mayús 1, Alt 2, Ctrl 4) |
| Flechas, Inicio, Fin, F1-F12 | Clásico (F11) | Igual (ya no son ambiguas) |

La tabla completa sale de la especificación del protocolo de kitty y va con pruebas en `terminal_teclas`.

**El terminal exterior.** MAGI pide al terminal exterior:

- el pegado entre corchetes (`Event::Paste`);
- los eventos de foco;
- con `teclado_mejorado`, si `supports_keyboard_enhancement()` lo confirma, la desambiguación de kitty, para recibir Mayús+Intro distinta de Intro.

El prefijo se reconoce en las dos codificaciones (`0x1d` y `CSI 93;5u`).

**Claude Code, en concreto:**

- En modo clásico se desplaza con el historial de MAGI.
- A pantalla completa, con su propio desplazamiento: RePág/AvPág y la rueda, que MAGI le pasa.
- Su copia por OSC 52 pide permiso la primera vez en cada host.
- Sus avisos marcan la pestaña: con `preferredNotifChannel = "terminal_bell"`, por la campana.

### 7.12 Pegado

```python
def sanear(texto):
    texto = texto.replace("\x1b[201~", "").replace("\x1b[200~", "")
    texto = "".join(c for c in texto if c in "\t\n\r" or not es_control(c))   # fuera ESC y los demás C0/C1
    return texto.replace("\r\n", "\r").replace("\n", "\r")

def pegar(texto, modos, cfg):
    t = sanear(texto)
    if not t:
        return None
    if modos.pegado_entre_corchetes:                       # el remoto activó 2004
        return Escritura(b"\x1b[200~" + t.encode() + b"\x1b[201~", sola=True)   # nada detrás
    if "\r" in t.rstrip("\r") and cfg.avisar_pegado:
        return Pedir("PEGAR VARIAS LÍNEAS", t)
    return Escritura(t.encode(), sola=True)
```

Un pegado grande se trocea por el tamaño máximo del códec, pero sale entero y en orden antes que la siguiente tecla. Si la pestaña se cierra a medias, se descarta el resto.

### 7.13 Validaciones

- **Nombre de pestaña:** 1-32 caracteres, sin controles; si ya existe, se añade « (n)».
- **Ruta para guardar:** absoluta, hasta 4 KiB, y que no exista (`create_new`). La propuesta es `$HOME/magi-historial-<pestaña>-<AAAAMMDD-HHMMSS>.txt`.
- **Patrón de búsqueda:** 1-256 caracteres.
- **Nombres de tmux:** §7.10.
- **URL:** `http` o `https`, 10-2048 caracteres. Puerto del túnel: 1-65535.
- **Título y aviso:** sin controles, hasta 120 caracteres; un aviso cada 5 s por pestaña.
- **OSC 52:** base64 válido, hasta 1 MiB decodificado.
- **Banderas de kitty:** solo se guarda la 1; la pila, 16 por pantalla (al llenarse sale la más vieja).
- **`EscribirVarios`:** solo pestañas concedidas.
- **Tamaño de cada panel:** saneado como en la F7.
- **Configuración:** los rangos de §5.3.

### 7.14 Casos especiales

- **Dos ventanas con la misma pestaña en paneles de distinto tamaño:** manda el mínimo (F3); el panel mayor pinta el resto en gris.
- **Escritura en varios con una pestaña también abierta en otra ventana:** solo escribe la ventana que la activó; la otra ve la marca `»`.
- **Reconectar un host borrado de la ficha:** no se reintenta; `caida` con «el host ya no existe» (F3).
- **Suspensión del portátil:** al volver, las pestañas que reconectan adelantan su intento, y las que cayeron durante la suspensión empiezan enseguida.
- **Pestaña con tmux en un host que ya no tiene tmux:** la reconexión abre un shell normal con aviso; la sesión se perdió.
- **Programa que abre la salida sincronizada y no la cierra:** a los 200 ms se pinta igual.
- **Remoto que pide el ratón y se cuelga sin soltarlo:** al salir de la vista o cambiar de panel, MAGI suelta la captura. El teclado no se captura, así que el prefijo funciona siempre.
- **Pegado de 100 MiB:** se trocea, sale en orden y la interfaz no se bloquea (va por la tarea de envío).
- **Enlace partido en dos líneas por el ancho:** se reconoce entero (líneas lógicas).
- **URL con usuario y contraseña** (`https://usuario:clave@…`): se ve tal cual en el diálogo y no se guarda en ningún sitio.
- **Sesión de tmux con el mismo nombre creada fuera de MAGI:** `new-session -A` se engancha a ella; la ficha lo advierte.

### 7.15 Limitaciones heredadas

- vt100 0.16 no guarda historial de la pantalla alternativa ni sabe borrar el suyo (§7.1).
- Con columnas muy anchas el historial tiene menos líneas, para no pasar de `historial_mib` (R138).
- R83-R85 de la F10 (sin la corrección 10b) siguen igual; los campos nuevos no viajan.

---

## 8. Requisitos No Funcionales

**Seguridad**

- **Portapapeles:** el remoto nunca lo lee; escribir en él exige permiso por host (propio del equipo), y cada escritura se avisa (T123).
- **Enlaces:** solo `http` y `https`, con la URL entera a la vista, sin shell y desde el cliente. Los túneles temporales van solo a `localhost` del host y escuchan solo en `127.0.0.1` (T35).
- **Pegado saneado:** sin marcas ni controles, para que un texto pegado no pueda salirse del modo de pegado y ejecutar órdenes.
- **Escritura en varios:**
  - deliberación con más de un host (T41) y una concesión que el servidor recomprueba;
  - aviso rojo fijo y apagado a los 10 min sin escribir;
  - REGISTRO sin lo tecleado.
- **Reconexión sin diálogos (T15, T49):** ni huellas nuevas, ni frases, ni contraseñas fuera del llavero.
- **tmux:** script embebido con nombres validados en Rust y en el script (nada interpolado).
- **Respuestas a consultas:** solo versión, tamaño, colores y modos; nunca rutas, usuarios ni datos del equipo.
- **Entrada no fiable (T38):**
  - títulos y avisos de hasta 120 caracteres;
  - OSC 52 de hasta 1 MiB y URLs de hasta 2 KiB;
  - salida de `listar` acotada y pila de kitty de 16;
  - nunca pánico.
- **Nada al log ni a REGISTRO** (T44): ni historial, capturas, títulos, avisos, portapapeles ni URLs.

**Protección de datos (RGPD)**

- El historial puede llevar datos personales (lo que salga en pantalla). Vive en la memoria del servidor y se borra al cerrar la pestaña o parar el servidor. Solo va a disco por orden expresa (`w`), en un fichero 600 que nunca sobrescribe otro.
- REGISTRO guarda metadatos (ruta y tamaño al guardar; hosts, duración y envíos de la escritura en varios), con la retención de 90 días de la F2.

**Backups y recuperación**

- La migración 10 añade tres columnas con valor por defecto. Un binario 1.3 no abre una `magi.db` en la versión 10 (T74).
- Lo que vive en memoria (historial, reconexiones, túneles temporales) se pierde al parar el servidor. `magi servidor parar` con túneles temporales pregunta antes (T67).

**Rendimiento**

- Captura de 10 000 líneas de 200 columnas en menos de 100 ms, y por el socket en menos de 200 ms.
- Búsqueda en 10 000 líneas en menos de 50 ms, con texto o con expresión regular.
- Cuatro paneles con salida continua a 30 fps, sin pasar del 25 % de un núcleo en el cliente.
- Memoria: como mucho `historial_mib` por pestaña en el servidor; una captura ocupa 1-2 MiB mientras se mira.
- Las respuestas a consultas salen en el mismo lote, sin esperas añadidas.

**Accesibilidad**

- Glifo y texto en todas las marcas (T5) y modo ASCII.
- Todo se maneja con el teclado: el ratón es opcional y nada depende de él.
- La escritura en varios se dice con texto en la barra, no solo con color.

---

## 9. Integraciones

| Integración | Uso | Notas |
|---|---|---|
| Claude Code (referencia, no dependencia) | Pegado entre corchetes, teclado de kitty, salida sincronizada, ratón SGR, OSC 52, campana y avisos, título | Se valida con la versión del momento (§11); las pruebas usan un programa propio que habla los mismos protocolos |
| tmux 1.8 o posterior | `new-session -A`, `list-sessions -F`, `kill-session` | Solo con `terminal_tmux.sh` |
| Terminal exterior (crossterm 0.29) | `EnableBracketedPaste`, `EnableFocusChange`, `PushKeyboardEnhancementFlags`, `EnableMouseCapture` y OSC 52 hacia fuera | Se deshace al salir, al ceder la terminal y en un pánico (D314) |
| Navegador del sistema | `xdg-open` (Linux) u `open` (macOS) | `[terminal] abrir_enlaces`, sin shell |
| Portapapeles | `wl-copy` o `pbcopy` (F2), u OSC 52 | Sin `xclip` ni `xsel` (Omarchy es Wayland) |

Protocolos: xterm (DA1, DA2, DSR, DECRQM, XTVERSION, XTWINOPS 18, OSC 10, 11, 12 y 52), teclado de kitty (mejora progresiva, bandera 1), salida sincronizada (modo 2026), eventos de foco (1004), pegado entre corchetes (2004) y ratón SGR (1006).

---

## 10. Decisiones Técnicas (ADR-lite)

- **D298 — Alcance: las siete piezas solo en el escritorio, con la lógica común en el núcleo** (Hector, 1a). El móvil ya tiene historial y selección (F11), y el terminal del escritorio es el que se usa a diario. Los módulos nuevos de `magi-nucleo/src/terminal/` no cambian el móvil: la app no los activa hasta su fase. Descartado: llevar ahora la reconexión y tmux al móvil (más pruebas instrumentadas, con la validación de la F14 pendiente).
- **D299 — Extras pequeños** (Hector, 2a): renombrar y mover pestañas, guardar la captura en un fichero solo por orden y avisar al pegar varias líneas sin pegado entre corchetes, como ya hace el móvil (D196 de la F11).
- **D300 — El historial lo guarda el servidor, con el de vt100 y un tope de bytes; el cliente trabaja sobre capturas** (Hector, 3a). El servidor ya custodia la pestaña, así que cualquier ventana ve lo mismo. Una captura congelada evita numerar líneas que se van cayendo del historial, y la búsqueda y la selección se hacen en el cliente sin ir y venir. Descartados: pedir al servidor ventanas vivas (más mensajes y numeración absoluta) y guardar el historial en el cliente (otra ventana no lo vería).
- **D301 — Modo historial al estilo tmux** (Hector, 4a). `Shift+RePág` entra solo con la pantalla principal; con la alternativa va al remoto, porque ese programa tiene su propio historial.
- **D302 — Portapapeles del sistema u OSC 52; el remoto, solo con permiso por host** (Hector, 5a). Para no perder la copia de Claude Code (OSC 52 por SSH), la primera petición de un host sin permiso se guarda 60 s y `prefijo P` la permite y la entrega. Leer el portapapeles, nunca.
- **D303 — Enlaces con letras, y túnel temporal para `localhost`** (Hector, 6a). Solo `http` y `https`, siempre a la vista antes de abrirlos y sin shell. El túnel temporal usa la mecánica de los túneles locales (F5), sin fila en TUNELES: dura lo que su pestaña.
- **D304 — Cinco disposiciones fijas por ventana** (Hector, 7a). Se dibujan, se prueban con instantáneas y se manejan con una tecla; no se guardan. Descartados: división libre y espacios de trabajo.
- **D305 — Escritura en varios solo en los paneles visibles, con deliberación si hay más de un host** (Hector, 8a; T41). El servidor concede y recomprueba (`IniciarEscrituraVarios`) y solo escribe en lo concedido. Cada pestaña recibe sus bytes, codificados con sus modos. Se apaga sola a los 10 min sin escribir.
- **D306 — Ratón transparente para el remoto y opcional para MAGI** (Hector, 9a, precisada por el añadido). Un terminal le pasa el ratón al programa que lo pide; si MAGI no lo hace, la rueda no llega a Claude Code a pantalla completa. Con `raton = false`, MAGI solo captura cuando el remoto del panel enfocado lo pide; `raton = true` añade la rueda en el historial, el clic en los paneles y el arrastre.
- **D307 — Reconexión automática en el servidor, sin diálogos** (Hector, 10a; T15, T49). Esperas de 2, 5, 10 y 30 s y luego cada minuto hasta `reconexion_max_min`, solo para caídas de red; la vuelta de una suspensión adelanta el intento. `prefijo r` sigue siendo la reconexión a mano, con diálogos.
- **D308 — tmux por host con `new-session -A` y nombres por equipo** (Hector, 11a). `magi-<equipo>-<n>` evita que dos equipos se enganchen a la misma sesión por casualidad. Cerrar la pestaña desengancha, y la lista de Sesiones deja volver o terminar. Descartados: `tmux -CC` y adivinar sesiones ajenas.
- **D309 — Compatibilidad con Claude Code y las TUI modernas** (petición de Hector). Pegado entre corchetes de punta a punta, respuestas a consultas, teclado de kitty (bandera 1, como Ghostty), salida sincronizada, ED 3, foco y avisos. No se toca Claude Code ni se depende de él: se implementan los protocolos que usa y un programa de prueba los ejercita.
- **D310 — Las respuestas las da solo el servidor, con los `Callbacks` de vt100 0.16 y en orden con la salida.** Así salen una vez aunque haya varias ventanas o ninguna, y con el estado exacto (la posición del cursor tras lo ya procesado). El cliente usa el mismo emulador sin responder.
- **D311 — Los ajustes nuevos del host son propios del equipo y no viajan.** Dependen del equipo (su portapapeles, su red, su nombre en tmux). Así `formato` no sube y un equipo con la 1.3 no los pisa; si algún día deben viajar, irán con un `formato` 4.
- **D312 — Protocolo v9**, `peticion_id` del terminal desde 2^49 y `TrabajoServidor.tuneles_temporales`.
- **D313 — Dependencia `regex`** para la búsqueda: tiempo lineal y tope de tamaño de la expresión, sin expresiones que se cuelguen. MIT o Apache-2.0 (`cargo deny`).
- **D314 — El terminal exterior queda como estaba.** MAGI deshace lo que pidió (pegado entre corchetes, foco, banderas de kitty y ratón) al salir, al ceder la terminal (visor, paginador) y en un pánico, con un gancho de pánico.
- **D315 — Versión 1.4.0;** la app Android no cambia.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Compatibilidad (Claude Code primero) | Contrato compilable (protocolo v9, tipos, módulos vacíos) antes de las pistas (T63); emulador del núcleo (modos, consultas y respuestas desde el servidor), pegado de punta a punta, teclado de kitty dentro y fuera, salida sincronizada, ED 3, foco, avisos y título, ratón transparente; programa de prueba y prueba de extremo a extremo | 1,5 semanas |
| S2 — Historial, búsqueda y copia | Historial en el servidor con topes y capturas; modo historial, búsqueda, selección, portapapeles (sistema y OSC 52), portapapeles del remoto con permiso, guardar; enlaces y túnel temporal | 1,5 semanas |
| S3 — Paneles y escritura en varios | Cinco disposiciones, foco, ampliar, abrir en rejilla, `Geometria` por panel; escritura en varios con deliberación y concesión; renombrar y mover pestañas; ratón opcional | 1,5 semanas |
| S4 — Reconexión, tmux y cierre | Reconexión automática; tmux (script, nombres, lista, cerrar sin terminar); migración 10 y ficha; configuración y ayuda; README, CHANGELOG y 1.4.0; revisión adversarial (T31, T54) | 1 semana |

**Total: 4 sprints, ~5,5 semanas.** S1 va primero porque arregla lo que Hector usa cada día con Claude Code.

**Supuestos**

- Las pruebas no usan Claude Code, hosts reales ni red externa. Usan:
  - el servidor SSH de prueba, que puede cortar una conexión a propósito;
  - un `tmux` falso en el `PATH`;
  - un programa de prueba (`tui_prueba`) que hace lo que hace Claude Code: consulta DA1, `CSI ? u` y DECRQM 2026, activa 2004, el teclado de kitty, 2026 y el ratón, escribe OSC 52, campana y título, y devuelve lo que recibe.
- Los hosts tienen `sh` POSIX y, para la sesión persistente, tmux.
- Hector valida con Claude Code de verdad (guion de abajo).

**Guion de validación manual**

1. Claude Code en modo clásico: pegar 20 líneas (un solo pegado), Mayús+Intro (salto sin enviar), `Ctrl+J`, `Shift+RePág` y búsqueda en su salida; cambiar el tamaño sin conversaciones duplicadas.
2. Claude Code a pantalla completa (`/tui fullscreen`): rueda, RePág/AvPág, selección con el ratón y copia (permiso con `prefijo P`), sin parpadeo.
3. Avisos: `preferredNotifChannel = "terminal_bell"` y una pestaña en segundo plano esperando permiso: `!` y el texto en la barra.
4. Alacritty y Ghostty (y foot, si está): Mayús+Intro; `teclado_mejorado = false` como respaldo.
5. Historial: `seq 1 20000`, buscar `19999`, copiar un rango y guardarlo en un fichero.
6. Enlaces: `python3 -m http.server 8000` en el host y abrir `http://localhost:8000` por un túnel temporal; un enlace `https` normal.
7. Rejilla con 4 hosts desde la paleta, escritura en varios con deliberación, `uptime` en los cuatro, parar y mirar REGISTRO.
8. Reconexión: cortar la Wi-Fi 30 s con una pestaña abierta (cuenta atrás y vuelta) y suspender el portátil 2 min.
9. tmux: sesión persistente, `sleep 600`, cortar la red y volver; cerrar la pestaña, recuperarla desde Sesiones y terminarla.
10. htop con ratón (clic en una columna) y Vim con `set mouse=a`.
11. Salir de MAGI con todo activado: el terminal exterior queda normal (sin ratón capturado y con el pegado de siempre).
12. macOS, si hay equipo: `pbcopy`, `open` y Terminal.app.

---

## 12. Conexiones con Otras Fases

- **F1:** vista Sesión y ficha de host (bloque TERMINAL).
- **F3:** servidor de sesiones, pestañas, `Sesiones{lista}`, protocolo v9, reconexión a mano (`prefijo r`) y `TrabajoServidor`.
- **F5:** los túneles temporales usan la mecánica de los locales; REGISTRO `tunel_*` con «temporal».
- **F6:** deliberación (`PlanDeliberado::EscrituraVarios`, solo en el escritorio, R135) y DELIBERACIONES.
- **F7:** `Geometria` por panel, mínimos, modo estrecho e instantáneas.
- **F9:** versión 1.4.0 y CHANGELOG (sin cambios de CLI).
- **F10:** los campos nuevos de HOSTS no viajan; `formato` sigue en 3.
- **F11:** el núcleo compartido (`terminal/`) gana módulos; el móvil no los activa todavía.
- **F13:** las pestañas con orden (seguir logs, shell de un contenedor) también tienen historial, búsqueda y enlaces.
- **F14:** su prompt de cierre va antes; esta fase parte de `fase14` cerrada y fusionada.
- **F16 (Despliegues):** podrá usar la reconexión y tmux en los pasos largos.
- **F19 (Caja negra):** grabará lo que hoy solo vive en el historial en memoria.
- **F21 (Asistente):** podrá explicar lo seleccionado en el modo historial.
- **Móvil (más adelante):** respuestas, pegado, teclado, historial con búsqueda y tmux, con el núcleo de esta fase.
