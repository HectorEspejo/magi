# MAGI — Fase 15: Terminal, Segunda Vuelta — Informe de implementación

**Estado:** implementación terminada, en verde y commiteada; pendiente la
validación manual con Claude Code real del desarrollador.

> Documento vivo: se actualiza en cada sesión. El informe de fase
> (`magi-fase15-informe.md`) manda sobre el criterio del implementador; las
> desviaciones se anotan aquí para que el analista funcional actualice la
> especificación.

## Resumen de lo implementado

La Fase 15 deja la vista Sesión convertida en un terminal de verdad, con Claude
Code como referencia, y con el trabajo repartido en paneles. Lo común vive en
`magi-nucleo`; el servidor custodia las pestañas, el historial y las respuestas
al remoto; el cliente pinta, captura y decide. Protocolo v9, migración 10 y
versión 1.4.0; el móvil no cambia.

**Núcleo (`crates/magi-nucleo`).** Emulador sobre vt100 0.16 con `Callbacks`
para los modos 1004 y 2026, banderas del teclado de kitty por pantalla (pila de
16), ED 3, título, avisos (BEL, OSC 9 y OSC 777), OSC 52 y las respuestas a
DA1, DA2, DSR 5 y 6, DECRQM, `CSI ? u`, XTVERSION, XTWINOPS 18 y OSC 10, 11 y
12; el servidor responde y el cliente no. `PantallaEmulada` acota los OSC/DCS
a 1 MiB antes de vte, procesa los ED 3 de un lote con un solo rehacer (sin DoS
de CPU), conserva las respuestas de antes del ED 3, aplaza el borrado y los
rehaceres mientras está en la pantalla alternativa y conserva las líneas más
nuevas al bajar el tope del historial. Codificación de teclas con la bandera 1
de kitty, ratón de los modos 1000/1002/1003 y codificaciones por defecto/UTF-8/
SGR, pegado saneado con marcas de 2004, historial con captura en tramos,
búsqueda de texto o regex y selección, enlaces http/https (con túnel a
localhost del host), tmux (nombres, script y lectura acotada de su salida) y
reconexión (causas, esperas y detección de suspensión). Migración 10 con
`reconectar`, `tmux_persistente` y `portapapeles_remoto` en `HOSTS`, propios
del equipo y sin viajar; tipos nuevos de `REGISTRO`.

**Servidor.** Cada pestaña tiene un parser con historial (topes de
`[terminal]` recalculados al redimensionar) y el emulador respondiendo: las
respuestas salen al canal en el mismo lote y una sola vez por pestaña. Captura
del historial por lotes cortados por tamaño codificado con plazo de 5 s; título
y aviso (uno cada 5 s) y estado de reconexión en `Sesiones`; foco agregado de
las ventanas (`Foco`, `CSI I`/`CSI O` con 1004); OSC 52 con permiso por host
(sin permiso, solo la última petición 60 s y `PortapapelesPedido`);
`COLORTERM=truecolor` y colores del tema para OSC 10-12; renombrar y mover
pestañas; reconexión automática sin diálogos con esperas 2/5/10/30/60 s, plazo
de 20 s por intento y `reconexion_max_min` desde la caída; tmux con
`terminal_tmux.sh` (comprobar, abrir, listar, terminar) y `magi-<equipo>-<n>`;
escritura en varios con concesión recomprobada (deliberación validada y de un
solo uso), escritura solo en lo concedido, apagado por todas las causas y
marca `»` difundida en `InfoSesion`; túneles temporales a localhost del host,
escuchando en `127.0.0.1` y muriendo con su pestaña. `TrabajoServidor` gana
`tuneles_temporales`.

**Cliente.** Terminal exterior con pegado entre corchetes, foco, kitty (si se
admite y `teclado_mejorado`) y ratón solo cuando el remoto lo pide o con
`raton`; todo se deshace al salir, al ceder la terminal y en el pánico. El
parser del cliente usa el mismo emulador sin responder y retiene el pintado
cuando el remoto activa la salida sincronizada (cierre o 200 ms). Cinco
disposiciones de paneles con cabecera, foco, ampliar y «abrir en rejilla…» con
casillas/grupos; modo historial con navegación, búsqueda (texto o regex),
selección, copia (wl-copy/pbcopy u OSC 52, tope de 1 MiB) y guardado
(`create_new`, 600, `historial_guardado`); enlaces con letras y diálogo, abrir
sin shell y túnel temporal; escritura en varios con deliberación, aviso rojo y
`»`; portapapeles del remoto con aviso y `prefijo P`; avisos y título en las
pestañas y en Sesiones; renombrar y mover pestañas; reconexión visible con
cuenta atrás y `prefijo r`/`x`; tmux en la vista Sesiones (listar, enganchar,
terminar) y al cerrar una pestaña persistente; ficha con el bloque TERMINAL;
ayuda y paleta con las teclas nuevas; aviso al pegar varias líneas; e
instantáneas de todas las disposiciones y modos a 40×12, 80×24 y 200×60.


**Corrección posterior al cierre (10 de noviembre de 2026, rama
`correccion-seleccion-copiar-pegar`).** La validación manual del desarrollador
destapó que pegar, seleccionar con el ratón y el foco exterior no funcionaban:
el hilo de teclas descartaba los eventos `Paste`, `Mouse`, `FocusGained` y
`FocusLost` de crossterm (las pruebas inyectaban `Evento::Pegado` y
`Evento::Raton` directamente, así que no lo veían). Se reenvían ya al bucle de
la UI con test de regresión del hilo, y pegar fuera de una sesión en vivo va al
campo con foco (diálogos, búsqueda del historial, paleta, filtros, ficha y
Servicios) como pide §4.3; al ceder la terminal al visor o al editor se
deshacen otra vez pegado, foco, kitty y ratón (D314). Con esto, el checklist de
la fase queda igualmente en 95/96 (sigue pendiente solo la ejecución de las
pruebas instrumentadas de Android).
## Desviaciones respecto a la especificación

- **`DatosHost` no lleva los ajustes nuevos del host.** La ficha guarda
  `reconectar`, `tmux_persistente` y `portapapeles_remoto` con setters propios
  (`almacen::hosts::fijar_*`) en vez de añadirlos a `DatosHost`, para que la
  importación de `~/.ssh/config` no los pise y no cambie el contrato de la
  sincronización. El comportamiento es el del informe §3.1: son propios del
  equipo, no suben la versión del host y no viajan.
- **`InfoSesion` gana `escritura_varios: bool`.** El informe §5.2 no lo
  incluye, pero §7.14 exige que la otra ventana vea la marca `»` y el protocolo
  v9 no tenía forma de difundirla; la alternativa (mensaje nuevo) era peor. Se
  difunde con la lista completa de sesiones (T27) al iniciar y al terminar.
- **Tope máximo de pantalla (1000×2000).** El informe solo exige mínimo 2×1;
  la revisión adversarial mostró que un cliente roto con 65535×65535 haría que
  vt100 reservara decenas de GB y abortara el servidor. El núcleo recorta y el
  servidor sanea al recibir.
- **Tope de captura del historial (64 tramos por fila y 8 MiB por captura) y
  tope de 1 MiB para OSC/DCS.** Salvaguardas de entrada no fiable que el
  informe no fija (solo pide la captura de 1-2 MiB); sin ellas un remoto podía
  agotar la memoria con estilos por celda o un OSC sin terminar.
- **ED 3 en la pantalla alternativa y rehaceres aplazados.** vt100 no deja
  leer la pantalla principal desde la alternativa; el borrado del historial y
  el rehacer por bajada del tope se aplican al volver a la principal, para no
  perderla (antes se perdía).
- **Bajar el tope de historial al redimensionar conserva las líneas más
  nuevas** (reinyectadas como texto, sin estilos) en vez de perderlas todas:
  es lo que pide §7.1 sin poder cambiar la capacidad de vt100 0.16.
- **Al rehacer el parser se pierde la región de scroll (DECSTBM).** vt100 no
  expone su estado; queda como limitación documentada (las apps que la usan
  redibujan al vuelo en la práctica).
- **Presentación del modo historial y de los enlaces.** Las letras de enlace
  se pintan en una caja sobre el panel (no incrustadas en la URL como en el
  mockup) y la barra del historial es de una línea con los atajos en la ayuda.
  El flujo completo (letras, diálogo, copiar, abrir, túnel) es el del informe;
  queda anotado como diferencia de presentación para el analista.
- **«Abrir en rejilla…» de la paleta** se implementó primero pidiendo nombres
  separados por comas y se sustituyó por el diálogo con casillas y grupos que
  pide §4.6 (grupos y hosts, límite de 4, 2 → lado a lado, 3 → 1 grande + 2,
  4 → rejilla). No queda desviación.
- **El servidor no escribe `HOSTS.portapapeles_remoto`:** el permiso lo guarda
  el cliente al pulsar `prefijo P` (como dice §4.10) y el servidor solo lo lee,
  respetando la lista de escrituras permitidas del servidor.
- **Los túneles temporales se cierran también si cae la conexión de su
  pestaña**, además de con su pestaña (§3.4): sin conexión no pueden servir y
  el informe §4.5 los ata a la pestaña.
- **La rama de ejecuciones del servidor ya no admite una deliberación usada
  por una escritura en varios activa:** se unificó «una deliberación, una
  acción» en los dos sentidos (R135), como pedía la revisión.
- **Varias copias OSC 52 en un mismo lote:** el emulador del núcleo guarda solo
  la última (el servidor la entrega y la avisa); encolar todas exigiría una
  cola en el núcleo y queda documentado como limitación. Lo normal es una
  copia por lote.
- **El volcado de `Adjuntar` activa la pantalla alternativa antes del
  `state_formatted()`** (y no después): así el cliente no pinta primero el
  contenido en la principal para cambiarla luego por una alternativa vacía.

## Estructura de archivos creada/modificada

Núcleo (`crates/magi-nucleo`):

- `src/terminal/emulador.rs` (**nuevo**): emulador y `PantallaEmulada`.
- `src/terminal/raton.rs`, `pegado.rs`, `enlaces.rs`, `historial.rs` y
  `tmux.rs` (**nuevos**).
- `src/terminal/mod.rs`: declara los módulos nuevos.
- `src/terminal/teclas.rs`: banderas de kitty y `desde_emulador`.
- `src/terminal/tramos.rs`: serde y `tramos_de_fila` público.
- `src/conexion/reconexion.rs` (**nuevo**) y `src/conexion/mod.rs`.
- `src/almacen/migraciones.rs`: migración 10; `src/almacen/hosts.rs`: lectura
  y setters; `src/almacen/mod.rs`: fachada.
- `src/modelo.rs`: campos nuevos de `Host`.
- `src/registro.rs`: tipos nuevos de `REGISTRO`.
- `scripts/terminal_tmux.sh` (**nuevo**): subórdenes de tmux.
- Pruebas nuevas: `tests/terminal_emulador` (unitaria en el módulo),
  `terminal_teclas` (unitaria), `terminal_raton`, `terminal_pegado`,
  `terminal_historial`, `terminal_enlaces`, `tmux_script` y `reconexion`.

Escritorio (`src`):

- `src/protocolo.rs`: protocolo v9 completo.
- `src/config.rs`: sección `[terminal]` con validación y acotado.
- `src/servidor/sesiones.rs` (reescrito sobre `PantallaEmulada`), `mod.rs`
  (despacho v9, estado nuevo, apagado) y `src/servidor/{historial,escritura_varios,tmux,tuneles_temporales}.rs`
  (**nuevos**).
- `src/cliente/pantallas.rs` (emulador sin responder y retención por 2026) y
  `src/cliente/mod.rs`.
- `src/app.rs` y `src/app/{paneles,sesion,historial,enlaces,escritura_varios,portapapeles,raton,pegado,rejilla}.rs`
  (**nuevos los ocho últimos**).
- `src/app/teclado.rs`, `src/app/dialogo_magi.rs` (PlanDeliberado::EscrituraVarios),
  `src/teclado.rs`.
- `src/ui/{sesion,sesiones,mod,dialogos,ficha,ayuda,disposicion,tema}.rs`.
- `src/main.rs` (gancho de pánico) y `src/portapapeles.rs` (adaptado).
- Pruebas nuevas: `tests/terminal_servidor.rs`, `tests/terminal_ui.rs`,
  `tests/terminal_e2e.rs`; `tests/fijos/terminal/{tui_prueba.rs,tmux,README.md}`
  (**nuevos**); `tests/comun/mod.rs` (programa de shell y opciones nuevas);
  `tests/{almacen,pool,ejecuciones}.rs` ajustados y `tests/redimensionado/**`
  con las instantáneas nuevas.

Documentación y publicación: `CHANGELOG.md`, `README.md` (sección «Terminal»),
`Cargo.toml` (1.4.0), `src/lib.rs` (fecha de versión), `assets/` regenerado y
este informe.

## Decisiones técnicas tomadas durante el desarrollo

- **Un solo emulador para servidor y cliente**, con `responder` como única
  diferencia (D310); los `Callbacks` de vt100 cubren lo que el parser no lleva
  sin buscar secuencias en los bytes por fuera.
- **Respuestas en el mismo lote y una vez por pestaña**; el cliente descarta
  las suyas (nunca responde).
- **Filtro de tamaño de OSC/DCS antes de vte** (vte acumula el OSC entero en
  memoria) y **un solo rehacer del parser por lote con ED 3**, con las
  respuestas previas conservadas y los ED 3/rehaceres pendientes en la
  pantalla alternativa.
- **Historial acotado por bytes** (captura de 64 tramos por fila y 8 MiB) y
  reinyección de las líneas nuevas al bajar el tope.
- **Salida sincronizada en el cliente**: se retiene el pintado de la pestaña
  hasta el cierre del modo 2026 o 200 ms, con el parser real alimentado en la
  tarea de lectura.
- **La marca `»` viaja en `InfoSesion.escritura_varios`** (lista completa, sin
  mensajes finos).
- **La deliberación de escritura en varios** guarda `hosts_json` único y
  ordenado por host_id; el servidor compara con los hosts de las pestañas
  concedidas y la consume una sola vez.
- **Los túneles temporales** usan la mecánica de los locales con `direct-tcpip`
  y `SoloViva`; escuchan en `127.0.0.1`, no abren conexiones ni dialogan y
  mueren con su pestaña.
- **tmux**: script embebido y nombre validado en Rust y en el script; el
  binario `tmux` del `PATH`, nunca interpolado. La comprobación de sesión usa
  lista de nombres exacta (no `has-session -t`, que casa prefijos).
- **El terminal exterior se restaura** al salir, al ceder la terminal y en el
  pánico; con el ratón capturado, Mayús+arrastre sigue seleccionando.
- **Corrección del ratón clásico**: xterm usa `32 + coordenada` (no `33 +`);
  se corrigió el desfase de una celda y el límite 223/2015.
- **Versión del producto en el emulador**: la inyecta el servidor con
  `fijar_version(env!("CARGO_PKG_VERSION"))`; DA2 responde 10400.

## Funcionalidades del checklist completadas

### Núcleo: emulador y compatibilidad

- `crates/magi-nucleo/src/terminal/emulador.rs` con los `Callbacks` de vt100 0.16: modos 1004 y 2026, banderas de kitty con una pila por pantalla (16 como mucho), ED 3, título, avisos (BEL, OSC 9 y OSC 777) y OSC 52; el mismo código en el servidor (responde) y en el cliente (no responde)
  - AC: Dado `CSI ? 2026 h` partido entre dos lotes, cuando se procesan, entonces el modo 2026 queda activo
- Respuestas a DA1, DA2, DSR 5 y 6, DECRQM (§5.5 del informe), `CSI ? u`, XTVERSION, XTWINOPS 18 y OSC 10, 11 y 12, en orden con la salida; OSC 52 de lectura nunca se responde
  - AC: Dado un remoto que escribe «hola» al principio de la fila 3 y pide `CSI 6 n`, cuando el servidor lo procesa, entonces responde `CSI 3;5R` en el mismo lote
- Codificación de teclas con la bandera 1 de kitty, como Ghostty: Intro, Tab y Retroceso sin modificadores no cambian; Mayús+Intro es `CSI 13;2u`, Esc es `CSI 27u` y Ctrl o Alt + tecla van en `CSI u`; sin banderas, la codificación clásica de la F11
  - AC: Dado un remoto que empujó `CSI > 1 u`, cuando se pulsa Mayús+Intro, entonces se envía `CSI 13;2u`; tras `CSI < 1 u`, se envía `CR`
- `terminal/raton.rs`: codificación de pulsar, soltar, arrastrar y rueda con modificadores para los modos 1000, 1002 y 1003 y las codificaciones por defecto, UTF-8 (1005) y SGR (1006)
- `terminal/pegado.rs`: saneado (sin `ESC[200~`, `ESC[201~` ni controles salvo tabulador y saltos de línea; finales de línea a CR) y marcas si el remoto activó 2004
  - AC: Dado un texto con `ESC[201~rm -rf ~` dentro, cuando se pega en un remoto con 2004, entonces llega un solo bloque `ESC[200~…ESC[201~` sin la marca interior
- `terminal/historial.rs`: captura del historial y la pantalla principal en tramos con `envuelta`, búsqueda (texto, con mayúsculas solo si el patrón las tiene, o expresión regular con `regex`) sobre líneas lógicas, texto de una selección y texto plano para guardar
  - AC: Dada una línea de 300 caracteres partida en dos filas, cuando se busca una palabra que cruza el corte, entonces se encuentra, y al copiarla sale en una sola línea
- `terminal/enlaces.rs`: enlaces `http` y `https` en líneas lógicas (sin la puntuación final, 10-2048 caracteres), destinos locales (`localhost`, `127.0.0.1`, `::1`, `0.0.0.0`) y URL reescrita para el túnel temporal
- `conexion/reconexion.rs`: causas recuperables y no recuperables, esperas (2, 5, 10 y 30 s, luego 60 s) y detección de la vuelta de una suspensión
- `terminal/tmux.rs` y `scripts/terminal_tmux.sh` (`comprobar`, `abrir`, `listar`, `terminar`) con nombres validados en Rust y en el script, y `magi-<equipo>-<n>` para las sesiones nuevas
  - AC: Dado un host sin tmux, cuando se ejecuta `abrir`, entonces se abre el shell de acceso, y `listar` devuelve `MAGI_TMUX=no`
- Migración 10: HOSTS `reconectar` (1), `tmux_persistente` (0) y `portapapeles_remoto` (0), propios del equipo
  - AC: Dado un equipo con la 1.4 que cambia esos campos, cuando sincroniza, entonces el fichero no los lleva y `formato` sigue en 3

### Servidor

- Parser de cada pestaña con historial de `historial_lineas` y tope `historial_mib` según las columnas, recalculado al redimensionar
- `CSI 3 J` vacía el historial de la pestaña (lote partido y parser rehecho con su estado)
  - AC: Dado un historial de 500 líneas, cuando llega `CSI 2 J CSI 3 J`, entonces la captura siguiente no tiene historial
- Las respuestas las escribe solo el servidor en el canal, una vez por pestaña, con o sin ventanas adjuntas
  - AC: Dada una pestaña adjunta en dos ventanas, cuando el remoto pide DA1, entonces recibe una sola respuesta
- `CapturarHistorial` → `HistorialLote` en lotes cortados por tamaño codificado, con plazo de 5 s
- Título, aviso (uno cada 5 s) y estado de reconexión en `Sesiones{lista}`, saneados y de hasta 120 caracteres
- Foco agregado de las ventanas (`Foco`); con 1004 activo, `CSI I` y `CSI O` al remoto cuando cambia
- OSC 52 del remoto: con permiso, `Portapapeles` a la ventana con la pestaña enfocada; sin permiso, `PortapapelesPedido` y la última petición 60 s en memoria
- `COLORTERM=truecolor` pedido al abrir cada pestaña (el host puede ignorarlo) y colores del tema recibidos en `Adjuntar` para OSC 10, 11 y 12
- `RenombrarSesion` y `MoverSesion` para todas las ventanas, con nombres únicos («(n)» si se repite)

### Terminal exterior y pegado

- MAGI pide al terminal exterior el pegado entre corchetes, los eventos de foco y, con `teclado_mejorado` y si lo admite, la desambiguación de kitty
- Al salir, al ceder la terminal (visor, paginador) y en un pánico, MAGI deshace lo que pidió al terminal exterior (pegado, foco, banderas de kitty y ratón)
  - AC: Dado MAGI con el ratón capturado y las banderas de kitty, cuando se fuerza un pánico en una prueba, entonces el terminal queda sin captura ni banderas
- El prefijo se reconoce en la codificación clásica (`0x1d`) y en la de kitty (`CSI 93;5u`)
- Pegar en una pestaña usa `pegado.rs`: un solo pegado entre corchetes, en escrituras propias y sin teclas detrás
  - AC: Dado Claude Code en el remoto, cuando se pegan 5 líneas, entonces llegan como un solo pegado y no se envía ningún mensaje
- Aviso PEGAR VARIAS LÍNEAS (`[terminal] avisar_pegado`) cuando el remoto no activó 2004, con las primeras líneas; `↵` pega y `Esc` cancela
- Salida sincronizada en el cliente: con 2026 abierto, la pestaña no se pinta hasta el cierre o 200 ms
  - AC: Dado un remoto que abre 2026, pinta 40 filas y lo cierra, cuando el cliente pinta, entonces no se ve ningún estado intermedio

### Ratón

- Si el remoto del panel enfocado pide ratón (1000, 1002 o 1003), MAGI lo captura y se lo pasa con su modo y codificación, también con `raton = false`; lo suelta cuando el remoto deja de pedirlo, al cambiar de panel o de modo y al salir de la vista
  - AC: Dado Claude Code a pantalla completa, cuando se gira la rueda sobre su panel, entonces su conversación se desplaza
- Con `raton = true`: la rueda en la pantalla principal abre el modo historial, en la alternativa sin ratón envía 3 flechas por paso, el clic enfoca otro panel y el arrastre selecciona en el modo historial
- La ayuda dice que, con el ratón capturado, la selección del terminal exterior sigue con Mayús+arrastre

### Historial, búsqueda y copia

- `prefijo [` y `Shift+RePág` (este, solo con la pantalla principal) abren el modo historial del panel enfocado
  - AC: Dado Vim abierto en la pestaña, cuando se pulsa `Shift+RePág`, entonces la tecla llega al remoto y no se abre el modo historial
- Navegación con flechas, `j`/`k`, RePág/AvPág, `Ctrl+U`/`Ctrl+D` y `g`/`G`; la salida nueva no mueve la vista y se avisa con «↓ salida nueva»
- `/` y `prefijo /`: búsqueda hacia arriba con `Tab` para texto o expresión regular, `n`/`N`, coincidencias resaltadas y contador; una expresión no válida se dice en la barra
- Con la pantalla alternativa, `prefijo [` avisa «este programa tiene su propio historial»
- `q` o `Esc` vuelven al vivo y liberan la captura
- `v` y `V` seleccionan; `y` o `↵` copian uniendo las líneas envueltas y sin espacios finales
- Copia al portapapeles con `wl-copy` o `pbcopy` (como en la F2) y, si no hay o MAGI corre por SSH, con OSC 52; más de 1 MiB no se copia y se propone guardar
- `w` guarda la captura con una ruta propuesta (`ficheros.rs`, `create_new`, 600), nunca sobrescribe y anota `historial_guardado` sin el contenido
- Al reconectar se conserva el historial con la línea «── reconectada HH:MM ──», que no llega al remoto

### Portapapeles del remoto

- Ficha: «Permitir que el host copie al portapapeles» (propio del equipo); con el permiso, cada copia del remoto se avisa con el host y el tamaño
- Sin permiso, aviso «<host> quiere copiar al portapapeles · ^] P permitir»; `prefijo P` activa el permiso y entrega la petición si no han pasado 60 s
  - AC: Dado un host sin permiso que copia por OSC 52, cuando se pulsa `prefijo P` a los 20 s, entonces el texto llega al portapapeles y la ficha queda con el permiso

### Enlaces y túneles temporales

- `prefijo o` (y `o` en el modo historial) marca con letras los enlaces visibles (hasta 26, los más cercanos al final)
- Diálogo del enlace con la URL completa y el host resaltado: `↵` abre (solo `http` y `https`), `c` copia y `Esc` sale
- Abrir con `[terminal] abrir_enlaces` (`xdg-open` u `open`), con la URL como único argumento, sin shell y desde el cliente
- Enlace a localhost del host: `t` abre un túnel temporal en `127.0.0.1:<libre>` y la misma URL con ese puerto
  - AC: Dado `http://localhost:8000/informe?x=1` en web-01, cuando se elige `t`, entonces se abre `http://127.0.0.1:<libre>/informe?x=1` y la página llega del host
- Túneles temporales como trabajo del servidor (`TrabajoServidor.tuneles_temporales`, `TunelesTemporales{lista}`): se ven en Túneles como «temporal», se cierran con su pestaña y anotan `tunel_*` con «temporal»

### Paneles

- Cinco disposiciones (completa, lado a lado, apilados, 1 grande + 2 y rejilla) y `prefijo Espacio` para pasar a la siguiente
- `prefijo 1`-`9`, `n` y `p` ponen la pestaña en el panel enfocado; si ya está en otro panel, se intercambian
- `prefijo` + flechas cambia de panel y `prefijo z` amplía el panel enfocado y vuelve
- Cada panel pasa su tamaño por `Geometria` (T46), con una fila de cabecera; con la misma pestaña en otra ventana, manda el mínimo
  - AC: Dada una ventana de 80×24 en rejilla, cuando se mira el tamaño de cada pestaña, entonces es el de su panel sin la cabecera
- Por debajo del mínimo de la disposición, solo el panel enfocado con «disposición en espera: ventana pequeña», y vuelve al agrandar
- Cabecera de cada panel con glifo y nombre; el enfocado, con `▸` y resaltado
- Paleta «abrir en rejilla…»: hasta 4 hosts con casillas o un grupo; con 2, lado a lado; con 3, «1 grande + 2»; con 4, rejilla
- La disposición es de la ventana: no se guarda ni se comparte

### Escritura en varios

- `prefijo E` con dos o más paneles: confirmación si es un solo host sin comprobaciones; si no, deliberación MAGI (`PlanDeliberado::EscrituraVarios`, `Ctrl+K`, forzado con motivo)
- `IniciarEscrituraVarios`: el servidor recomprueba pestañas abiertas, hosts y deliberación válida y sin usar, y solo escribe en lo concedido
  - AC: Dado un cliente que manda `EscribirVarios` con una pestaña no concedida, cuando llega al servidor, entonces se rechaza entero
- Cada pestaña recibe sus bytes codificados con sus modos (teclado de kitty y pegado)
- Aviso rojo fijo «ESCRIBIENDO EN N PANELES» y `»` en cada panel mientras dura
- Se apaga con `prefijo E`, al cambiar de disposición o de pestaña en un panel, si cae una pestaña, al salir de la vista, a los 10 min sin escribir y al cerrar la ventana
- REGISTRO `escritura_varios_activada` y `escritura_varios_terminada`, y `ejecucion_resultado` con la duración y los envíos; nunca lo tecleado
- El prefijo y el ratón no se difunden; el pegado sí, con el aviso de varias líneas por pestaña

### Reconexión automática

- Con `reconectar`, una caída de red pasa a `reconectando` y el servidor reintenta sin diálogos (llavero, huellas aceptadas, plazo de 20 s)
  - AC: Dado un host que corta la conexión y vuelve a los 20 s, cuando llega el siguiente intento, entonces la pestaña se reabre sola y REGISTRO anota `sesion_reconectada` «automática»
- No se reintenta tras `exit`, credenciales rechazadas, huella nueva o cambiada, frase pendiente, cierre del usuario ni host borrado; se dice por qué y queda `prefijo r`
- Pasado `reconexion_max_min`: `caida` con «reconexión agotada» y REGISTRO `reconexion_agotada`
- La vuelta de una suspensión adelanta el siguiente intento
- Cuenta atrás en la pestaña y en la barra; `prefijo r` reintenta ya (con diálogos) y `prefijo x` cierra
- Varias pestañas del mismo host comparten la conexión del pool al reconectar (T42)

### tmux

- Ficha: «Sesión persistente con tmux» (propio del equipo), con el aviso de tmux dentro de tmux
- Pestaña nueva con la opción: `listar`, nombre libre `magi-<equipo>-<n>` y `abrir`; sin tmux, shell normal con aviso
- La reconexión vuelve a la misma sesión de tmux con `comprobar` y `abrir`
  - AC: Dado `sleep 600` en una pestaña con tmux, cuando la red cae y vuelve, entonces la pestaña reconectada enseña el `sleep` en marcha
- Cerrar una pestaña con tmux pregunta: `↵` la deja viva en el host y `T` la termina también
- Sesiones: `t` lista las sesiones de tmux del host (nombre, enganchada, ventanas, creada); `↵` abre una pestaña enganchada y `T` la termina con confirmación y REGISTRO `tmux_terminada`
- El snippet al conectar y el comando inicial solo se escriben en una sesión de tmux nueva

### Pestañas, avisos y título

- Campana, OSC 9 y OSC 777 en una pestaña que no se ve: `!` y el texto en la barra; se limpia al verla; nunca avisos del sistema
- Título del remoto (OSC 0 y 2) en la lista de Sesiones y en la barra de la pestaña
- `prefijo ,` renombra la pestaña (1-32 caracteres) para todas las ventanas
- `prefijo <` y `>` mueven la pestaña; la numeración de `prefijo 1`-`9` sigue el orden nuevo
- Marcas `↻` (reconectando), `!` (aviso), `▸` (enfocado) y `»` (escritura en varios) con su texto y su equivalente ASCII

### Configuración, ficha y ayuda

- `[terminal]`: `historial_lineas`, `historial_mib`, `raton`, `teclado_mejorado`, `avisar_pegado`, `abrir_enlaces` y `reconexion_max_min`, con sus rangos y aviso al arrancar
- Ficha de host: bloque TERMINAL «solo en este equipo» con los tres campos, solo en hosts `ssh`
- Ayuda `?`, barra de la vista Sesión, Sesiones y paleta con las teclas nuevas

### Interfaz: instantáneas

- Instantáneas a 40×12, 80×24 y 200×60 (T45) de las cinco disposiciones, el modo historial con búsqueda y selección, los enlaces y su diálogo, la escritura en varios, una pestaña reconectando, Sesiones con tmux, el aviso de pegado y la ficha con el bloque TERMINAL

### API: protocolo cliente ⇄ servidor (v9)

- `CapturarHistorial` → `HistorialLote`
- `Foco`
- `IniciarEscrituraVarios` → `EscrituraVariosIniciada`, `EscribirVarios` y `TerminarEscrituraVarios` / `EscrituraVariosTerminada`
- `AbrirTunelTemporal` → `TunelTemporal`, `CerrarTunelTemporal` y `TunelesTemporales{lista}`
- `ListarTmux` → `SesionesTmux` y `TerminarTmux` → `TmuxTerminada`
- `Portapapeles`, `PortapapelesPedido` y `PortapapelesPermitido`
- `RenombrarSesion` y `MoverSesion`
- `Adjuntar` con `colores`, `AbrirSesion` con `tmux` y `Sesiones{lista}` con `reconexion`, `titulo`, `aviso` y `tmux`
- `VERSION_PROTOCOLO = 9`, `peticion_id` del terminal desde 2^49 y `TrabajoServidor.tuneles_temporales`

### Calidad, pruebas y publicación

- Pruebas del núcleo: `terminal_emulador`, `terminal_teclas`, `terminal_raton`, `terminal_pegado`, `terminal_historial` (también los bytes por celda de §7.1), `terminal_enlaces`, `tmux_script` y `reconexion`
- Programa de prueba `tui_prueba`, que hace lo que Claude Code (consultas, 2004, teclado de kitty, 2026, ratón, OSC 52, campana y título), y prueba de extremo a extremo `terminal_e2e` con el servidor SSH de prueba, sin red externa
  - AC: Dado `tui_prueba` en el servidor SSH de prueba, cuando arranca, entonces recibe las respuestas a sus consultas, y un pegado de 5 líneas y un Mayús+Intro le llegan con los bytes esperados
- `terminal_servidor` (reconexión con un servidor que corta la conexión, tmux falso, escritura en varios, capturas, portapapeles y túnel temporal) y `terminal_ui` (paneles, modos y avisos) con `App::de_prueba`
- Dependencia `regex` con `cargo deny` en verde; clippy, fmt y `shellcheck` del script nuevo
- Versión 1.4.0, CHANGELOG y sección «Terminal» del README con Claude Code (pegado, Mayús+Intro, rueda y permiso del portapapeles)
- Revisión adversarial de cierre (T31, T54), servidor y cliente por separado, con su informe en `docs/fase15/magi-fase15-revision-adversarial.md`

## Pendientes y bloqueos

- **Validación manual con Claude Code real** (guion en «Ejecución y pruebas»):
  requiere el equipo y los terminales del desarrollador.
- **Pruebas instrumentadas de Android**: no se han ejecutado en esta máquina
  (necesitan emulador y el servidor de pruebas del anfitrión; el job `humo` de
  CI las cubre). Las pruebas JVM y las de extremo a extremo en Rust están en
  verde y el móvil no se ha tocado: por eso el checklist queda en 95/96.
- Limitaciones documentadas arriba: región de scroll al rehacer el parser y
  diferencias de presentación del modo historial/enlaces.
- Cualquier hallazgo de la revisión adversarial del servidor que no se haya
  corregido en esta sesión queda anotado aquí al cerrar.

## Ejecución y pruebas

- Compilar: `cargo build --locked`; la TUI: `cargo run`.
- Migrar: la migración 10 se aplica sola al abrir MAGI (user_version 10); una
  base anterior se actualiza y una futura no se abre.
- Pruebas sin red externa: `TZ=UTC cargo test --workspace --locked` → **64
  binarios de pruebas en verde y 1.907 pruebas pasadas, 0 fallos** (incluye
  las 493 unitarias del escritorio, las 500+ del núcleo, `terminal_servidor`
  18/18, `terminal_ui` 37/37, `terminal_e2e` 1/1, `redimensionado` 303/303,
  `escritorio_*`, `servicios_*`, `sincronizacion_*` y las del móvil).
- Núcleo del terminal: `cargo test -p magi-nucleo --test terminal_emulador`...
  (las suites listadas en `CLAUDE.md`, todas en verde).
- Escritorio: `TZ=UTC cargo test -p magi-ssh --test terminal_servidor --test
  terminal_ui --test terminal_e2e --test redimensionado`.
- Móvil sin tocar: `cargo check -p magi-movil --all-targets`, las pruebas de
  extremo a extremo en Rust y `./gradlew testDebugUnitTest` en `android/`.
- Calidad: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo deny check` y `shellcheck scripts/*.sh
  crates/magi-nucleo/scripts/*.sh` → todos limpios.
- Móvil: `cargo check -p magi-movil --all-targets --locked` sin errores, las
  cinco suites de extremo a extremo en Rust en verde y
  `./gradlew testDebugUnitTest` en verde. Las instrumentadas de Android quedan
  para el emulador/CI (job `humo`).

### Guion de validación manual con Claude Code real

1. Claude Code en modo clásico: pegar 20 líneas (un solo pegado, ningún
   mensaje por línea), Mayús+Intro (salto de línea sin enviar), `Ctrl+J`,
   `Shift+RePág` y búsqueda en su salida; cambiar el tamaño de la ventana sin
   conversaciones duplicadas.
2. Claude Code a pantalla completa (`/tui fullscreen`): rueda, RePág/AvPág,
   selección con el ratón y copia (con el permiso de `prefijo P`), sin parpadeo.
3. Avisos: `preferredNotifChannel = "terminal_bell"` y una pestaña en segundo
   plano esperando permiso: `!` y el texto en la barra.
4. Alacritty y Ghostty (y foot, si está): Mayús+Intro; `teclado_mejorado =
   false` como respaldo.
5. Historial: `seq 1 20000`, buscar `19999`, copiar un rango y guardarlo en un
   fichero.
6. Enlaces: `python3 -m http.server 8000` en el host y abrir
   `http://localhost:8000` por un túnel temporal; un enlace `https` normal.
7. Rejilla con 4 hosts desde la paleta, escritura en varios con deliberación,
   `uptime` en los cuatro, parar y mirar REGISTRO.
8. Reconexión: cortar el Wi-Fi 30 s con una pestaña abierta (cuenta atrás y
   vuelta) y suspender el portátil 2 min.
9. tmux: sesión persistente, `sleep 600`, cortar la red y volver; cerrar la
   pestaña, recuperarla desde Sesiones y terminarla.
10. htop con ratón (clic en una columna) y Vim con `set mouse=a`.
11. Salir de MAGI con todo activado: el terminal exterior queda normal (sin
    ratón capturado y con el pegado de siempre).
12. macOS, si hay equipo: `pbcopy`, `open` y Terminal.app.
