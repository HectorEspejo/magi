# MAGI — Fase 15: revisión adversarial de cierre

**Fecha:** 9 de noviembre de 2026.
**Alcance:** código nuevo de la Fase 15, servidor y cliente por separado, más
el núcleo compartido. Se buscaron pérdidas de datos, fugas de recursos, esperas
sin plazo, estado leído a destiempo y entrada no fiable, como manda
`CLAUDE.md`.

## Método

- Revisión adversarial del **núcleo** (emulador, teclas, ratón, pegado,
  historial, enlaces, tmux, reconexión, migración 10 y protocolo) con lectura
  completa, contraste con el informe (§5.5 y §7.1-7.13) y fuzz determinista de
  10 MB con ESC/CSI/OSC/ED3/ratón/kitty: sin pánicos.
- Revisión adversarial del **servidor** (sesiones, historial, escritura en
  varios, tmux, túneles temporales y el despacho v9) con lectura completa,
  contraste con §3-§5 y §7 y comprobación de las ramas de error de cada recurso.
- Revisión de las **pruebas**: las suites del núcleo, `terminal_servidor`,
  `terminal_ui`, `terminal_e2e` y `redimensionado` se ejecutaron durante la
  revisión.

## Hallazgos y resolución

### Núcleo

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Alta | Cada `ESC[3J` rehacía el parser: 5.000 ED 3 bloqueaban segundos el bucle de lectura (DoS de CPU con entrada no fiable). | **Corregido**: se procesa hasta el último ED 3 y se rehace una sola vez; prueba de 5.000 ED 3 en <500 ms y el historial queda vacío. |
| Alta | Un OSC sin terminar acumulaba memoria sin tope en vte (50 MB medidos) antes de comprobar el límite de OSC 52. | **Corregido**: filtro de OSC/DCS antes de vte (tope 1 MiB, corte con BEL y descarte hasta el terminador; el OSC 52 truncado no se guarda). Prueba con 2 MiB. |
| Alta | La captura con un estilo por celda podía generar cientos de MB de JSON/estilos. | **Corregido**: 64 tramos por fila y 8 MiB por captura, con las filas más viejas fuera; prueba con 24×200 de colores únicos. |
| Media | Un lote con consulta y ED 3 perdía la respuesta de la consulta al rehacer. | **Corregido**: las respuestas previas se conservan; prueba DA1 + ED3 en el mismo lote. |
| Media | Bajar el tope del historial al redimensionar perdía todo el historial (§7.1 pide solo las más viejas). | **Corregido**: se conservan las líneas más nuevas (reinyectadas sin estilos); prueba 8 líneas y tope 100→2. |
| Media | Redimensionar en la pantalla alternativa perdía la principal; y el ED 3 en la alternativa no vaciaba el historial de la principal al volver. | **Corregido**: los rehaceres y el borrado se aplazan hasta volver a la principal; pruebas de ambos casos. |
| Media | El estado que se conserva al rehacer no incluye la región de scroll (DECSTBM). | **Documentado** como limitación de vt100 0.16; las apps que la usan redibujan. |
| Media | La captura bloquea el mutex de la pantalla mientras materializa. | **Mitigado** por los topes de captura; se mantiene el plazo de 5 s del servidor y se documenta. |
| Baja | `trocear` partía un carácter UTF-8 si el máximo era < 4; `pegar` duplicado sin sanear en `teclas`; `sanear` con copias de más en pegados enormes. | `trocear` corregido y con prueba; los otros dos quedan documentados (el pegado real usa `pegado.rs` y tamaños de MiB). |
| Baja | `enlaces` sin tope de memoria y `http://[::1]` truncado al quitar el `]` final. | **Corregidos**: máximo 26 enlaces (los más cercanos al final) y corchete equilibrado; pruebas. |
| Baja | `has-session -t` casa por prefijo en tmux; separador `|` del script podía desplazar campos con nombres ajenos. | **Corregido**: comparación exacta con `list-sessions -F '#{session_name}'` y separador US (0x1f); pruebas actualizadas. |
| Baja | Tamaño de terminal sin máximo (un cliente con 65535×65535 podría abortar por OOM); `valor_da2` podía desbordar. | **Corregido** en el núcleo (recorte 1000×2000) y DA2 con aritmética saturante. |

### Servidor

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Alta | Sin tope de tamaño de terminal: `AbrirSesion`/`Adjuntar`/`Redimensionar` con 65535×65535 podían abortar el servidor. | **Corregido**: saneo explícito en el servidor además del recorte del núcleo, con prueba de que el servidor y las demás pestañas siguen vivos. |
| Alta | La deliberación de escritura en varios no se marcaba como usada: podía autorizar dos concesiones o reutilizarse en ejecuciones/transferencias/servicios mientras vivía. | **Corregido**: `ejecuciones::deliberacion_usada` incluye las concesiones de escritura y `iniciar` la comprueba; pruebas de doble uso. |
| Alta | `Adjuntar` mandaba `contents_formatted()`, que no lleva 2004, ratón, cursor de aplicación ni pantalla alternativa: una segunda ventana codificaba mal teclas, pegado y ratón. | **Corregido**: `state_formatted()` (contenido y modos de entrada) + pantalla alternativa + push de kitty; la alternativa se activa antes del volcado para no pintar el contenido en la principal y luego cambiar a una alternativa vacía. Prueba con ventana nueva y parser vt100 real. |
| Media | Faltaba `REGISTRO tunel_fallido` «temporal» en los fallos de `AbrirTunelTemporal`. | **Corregido** y con prueba. |
| Media | El servidor usaba los topes de `[terminal]` sin acotar (un config fuera de rango podía dejar el historial en GiB o la reconexión en días). | **Corregido**: usa los `*_efectivo()` del config. |
| Media | El plazo de 20 s por intento de reconexión incluía la espera del cerrojo del host, contra «esperar al cerrojo no cuenta como fallo» (§7.9). | **Corregido**: primero se espera (hasta 30 s) a que otra pestaña del mismo host acabe de abrir y solo después corre el intento con sus 20 s. |
| Baja | `ExitStatus`/`ExitSignal` ignorados; detalle de `reconexion_agotada` con el host en vez de la pestaña; tmux sin aviso si `comprobar` fallaba; capturas sin tope de concurrencia; túnel huérfano si la ventana se iba; captura siguiendo tras cerrar la pestaña; varias OSC 52 por lote; tormenta de listas por cambios de título; escritura del servidor en `HOSTS.portapapeles_remoto`. | **Corregidas** todas menos la cola de OSC 52 (el emulador del núcleo solo guarda la última copia del lote; queda documentado como limitación). El servidor ya no escribe `HOSTS.portapapeles_remoto` (lo hace el cliente). |

## Verificado correcto (sin hallazgo)

- El cliente nunca responde a las consultas del remoto; el servidor responde
  una sola vez por pestaña, en el mismo lote y antes de difundir los datos.
- OSC 52 de lectura nunca se contesta; sin permiso solo se guarda la última
  petición 60 s.
- ED 3 y 2026 partidos entre lotes se detectan; la pila de kitty es por
  pantalla y con tope 16; límites de tmux, título/aviso y ratón correctos.
- Migración 10 y setters: valores por defecto 1/0/0, sin `CHECK`, sin tocar
  `uuid`/`version`/`vector`, y el snapshot de sincronización no los incluye.
- Reconexión sin diálogos con sus causas, esperas, suspensión y REGISTRO; tmux
  con script embebido y nombre validado; escritura en varios recomprobada y
  sin lo tecleado; túneles temporales en 127.0.0.1 y con `SoloViva`; apagado
  limpio que anota antes de soltar la red.
- Nada de historial, salida, títulos, avisos, URLs ni portapapeles va a
  REGISTRO o al log.

## Resultado

La validación manual del desarrollador destapó después del cierre un fallo que
las pruebas automáticas no veían: el hilo de teclas descartaba los eventos
`Paste`, `Mouse` y `Focus*` del terminal exterior, de modo que pegar y
seleccionar con el ratón no funcionaban. Se corrigió en la rama
`correccion-seleccion-copiar-pegar` con test de regresión del hilo (y se
completó el pegado en los campos con foco y la restauración del terminal al
cederlo al visor o al editor). Es la clase de hueco que una revisión adversarial
del cliente debería haber cubierto: se anota como lección para la próxima fase.

Los hallazgos de gravedad alta y media se corrigieron con prueba de regresión
(ver el detalle en el informe de implementación). Quedan como limitaciones
documentadas la región de scroll al rehacer el parser (vt100 0.16), la región
de presentación del modo historial/enlaces y el bloqueo del mutex de la
pantalla durante la captura (mitigado por sus topes).
