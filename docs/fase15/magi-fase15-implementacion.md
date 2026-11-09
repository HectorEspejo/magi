# MAGI — Fase 15: Terminal, Segunda Vuelta — Informe de implementación

**Estado:** en desarrollo (sesión 1).

> Documento vivo: se actualiza en cada sesión, no se regenera desde cero.
> El informe de fase (`magi-fase15-informe.md`) manda sobre el criterio del
> implementador; las desviaciones se anotan aquí para que el analista funcional
> actualice la especificación.

## Resumen de lo implementado

Esta primera sesión deja fijado el **contrato compilable** de la fase y las
piezas centrales del núcleo, sobre las que trabajan en paralelo el servidor y
el cliente:

- **Emulador del núcleo** (`crates/magi-nucleo/src/terminal/emulador.rs`): los
  `Callbacks` de vt100 0.16 con los modos 1004 y 2026, las banderas del teclado
  de kitty con una pila por pantalla (16), ED 3, título, avisos (BEL, OSC 9 y
  OSC 777), OSC 52 y las respuestas a DA1, DA2, DSR 5 y 6, DECRQM, `CSI ? u`,
  XTVERSION, XTWINOPS 18 y OSC 10, 11 y 12. El servidor lo crea respondiendo y
  el cliente sin responder. Incluye `PantallaEmulada`, que envuelve el parser,
  parte los lotes en `CSI 3 J` (rehaciéndolo con su estado para vaciar el
  historial), conserva la pantalla alternativa al rehacer y da el número de
  líneas del historial.
- **Teclado de kitty (bandera 1)** en `terminal/teclas.rs`: Mayús+Intro es
  `CSI 13;2u`, Esc `CSI 27u`, Ctrl y Alt con tecla van en `CSI u`; sin
  banderas se mantiene la codificación clásica. `terminal/raton.rs` codifica
  pulsar, soltar, arrastrar y rueda con modificadores en las codificaciones por
  defecto, UTF-8 (1005) y SGR (1006) para los modos 1000, 1002 y 1003.
  `terminal/pegado.rs` sanea el pegado y añade las marcas de 2004.
- **Protocolo v9** en `src/protocolo.rs`: todos los mensajes de la fase
  (`CapturarHistorial`/`HistorialLote`, `Foco`, escritura en varios, túneles
  temporales, tmux, portapapeles, `RenombrarSesion` y `MoverSesion`), los
  campos nuevos de `Adjuntar`, `AbrirSesion` y `Sesiones`, y
  `TrabajoServidor.tuneles_temporales`; `peticion_id` del terminal desde 2^49.
- **Migración 10**: `HOSTS.reconectar` (1), `tmux_persistente` (0) y
  `portapapeles_remoto` (0), propias del equipo, con setters en el almacén y
  sus tipos en `modelo::Host`; tipos de `REGISTRO` nuevos.
- **Configuración** `[terminal]` completa (historial, ratón, teclado, pegado,
  enlaces y reconexión) con validación y acotado al arrancar.
- **Documentación**: CHANGELOG, README (sección «Terminal: paneles, historial y
  Claude Code»), versión 1.4.0 y fecha de la página man.

El servidor (pestañas con historial y respuestas, foco, OSC 52, reconexión
automática, tmux, escritura en varios y túneles temporales) y el cliente
(paneles, modo historial, enlaces, ratón, pegado, portapapeles, ficha y
ayuda) están en desarrollo en esta misma sesión; al cerrarla se completa este
resumen con lo que quede en verde y lo que quede pendiente.

## Desviaciones respecto a la especificación

- **Los ajustes del host no viajan por `DatosHost`.** La ficha guarda
  `reconectar`, `tmux_persistente` y `portapapeles_remoto` con setters propios
  (`almacen::hosts::fijar_*`) en vez de añadirlos a `DatosHost`: así la
  importación de `~/.ssh/config` y el resto de la ficha no los pisan y no
  cambia el contrato de la sincronización. El comportamiento es el del
  informe (§3.1): son propios del equipo, no suben la versión del host y no
  viajan.
- **ED 3 en la pantalla alternativa.** vt100 no deja leer la pantalla
  principal desde la alternativa; rehacer el parser ahí perdería la principal
  (la shell al salir de Claude Code). En la alternativa no hay historial que
  vaciar, así que `CSI 3 J` se ignora en esa pantalla; en la principal se
  aplica como manda el informe (el lote se parte y el parser se rehace con su
  estado).
- **Recalcular el tope del historial al redimensionar** no puede hacerse con
  la API pública de vt100 0.16: no hay forma de cambiar la capacidad del
  `scrollback` sin rehacer el parser, y rehacerlo no conserva el historial.
  La implementación rehace el parser solo si el tope **baja** (se pierden las
  líneas más viejas, como dice el informe); si sube, se conserva la capacidad
  anterior para no perder el historial. Se anotará como limitación si la
  revisión adversarial no encuentra una alternativa.
- **La versión del producto en el emulador la fija el servidor** con
  `Emulador::fijar_version(env!("CARGO_PKG_VERSION"))`: el núcleo no conoce
  la versión de la app (su crate es `0.1.0`) y DA2/XTVERSION tienen que
  informar 1.4.0 (10400).

## Estructura de archivos creada/modificada

Núcleo (`crates/magi-nucleo`):

- `src/terminal/emulador.rs` (**nuevo**): emulador y `PantallaEmulada`.
- `src/terminal/raton.rs`, `pegado.rs`, `enlaces.rs`, `historial.rs` y
  `tmux.rs` (**nuevos**): ratón, pegado, enlaces, historial y tmux.
- `src/terminal/mod.rs`: declara los módulos nuevos.
- `src/terminal/teclas.rs`: banderas de kitty y `desde_emulador`.
- `src/terminal/tramos.rs`: `Serialize`/`Deserialize` y `tramos_de_fila`
  público para el historial.
- `src/conexion/reconexion.rs` (**nuevo**) y `src/conexion/mod.rs`.
- `src/almacen/migraciones.rs`: migración 10 y `VERSION_BD` 10.
- `src/almacen/hosts.rs`: lectura de las tres columnas y setters.
- `src/modelo.rs`: los tres campos en `Host`.
- `src/registro.rs`: `reconexion_agotada`, `escritura_varios_activada`,
  `escritura_varios_terminada`, `historial_guardado` y `tmux_terminada`.
- `scripts/terminal_tmux.sh` (**nuevo**): subórdenes de tmux.

Escritorio (`src`):

- `src/protocolo.rs`: protocolo v9.
- `src/config.rs`: sección `[terminal]`.
- `src/servidor/**` y `src/app/**`: en desarrollo (pistas del servidor y del
  cliente).
- `tests/fijos/terminal/tui_prueba.rs` (**nuevo**): programa que hace lo que
  Claude Code para la prueba de extremo a extremo.
- `tests/almacen.rs`: pruebas de la migración 10.

Documentación: `CHANGELOG.md`, `README.md`, `Cargo.toml` (1.4.0),
`src/lib.rs` (fecha de versión) y `assets/` regenerado.

## Decisiones técnicas tomadas durante el desarrollo

- **Un solo emulador para servidor y cliente**, con `responder` como única
  diferencia: el servidor escribe las respuestas al canal en el mismo lote y
  el cliente nunca responde (D310). Los `Callbacks` de vt100 permiten cubrir
  lo que el parser no lleva sin buscar secuencias en los bytes a mano.
- **`PantallaEmulada` rehace el parser cuando hay que vaciar el historial**
  (`state_formatted` + restauración de la pantalla alternativa), que es la
  única vía con vt100 0.16; se retiene el prefijo de un `CSI 3 J` partido
  entre lotes para no perderlo.
- **Los tipos del protocolo reutilizan los del núcleo** (`ColoresTema`,
  `LineaHistorial`, `SesionTmux`) para no duplicar la representación.
- **`tramos_de_fila` público en `tramos.rs`** para que el historial use el
  mismo conversor de celdas a tramos que pinta la app móvil.
- **El cliente seguirá usando el parser con emulador sin responder** en
  `src/cliente/pantallas.rs`; la salida sincronizada se retiene en el cliente
  hasta el cierre o 200 ms (en desarrollo).
- **La versión de la app se inyecta en el emulador** en vez de leerla del
  crate del núcleo.

## Funcionalidades del checklist completadas

Se irán copiando aquí al cerrar la sesión, con el texto exacto del checklist.
De momento, completadas y verificadas con pruebas:

- «`crates/magi-nucleo/src/terminal/emulador.rs` con los `Callbacks` de vt100
  0.16: modos 1004 y 2026, banderas de kitty con una pila por pantalla (16 como
  mucho), ED 3, título, avisos (BEL, OSC 9 y OSC 777) y OSC 52; el mismo
  código en el servidor (responde) y en el cliente (no responde)».
- «Codificación de teclas con la bandera 1 de kitty, como Ghostty: Intro, Tab y
  Retroceso sin modificadores no cambian; Mayús+Intro es `CSI 13;2u`, Esc es
  `CSI 27u` y Ctrl o Alt + tecla van en `CSI u`; sin banderas, la codificación
  clásica de la F11» (núcleo; falta la parte de eventos del cliente).
- «`terminal/raton.rs`: codificación de pulsar, soltar, arrastrar y rueda con
  modificadores para los modos 1000, 1002 y 1003 y las codificaciones por
  defecto, UTF-8 (1005) y SGR (1006)».
- «`terminal/pegado.rs`: saneado (sin `ESC[200~`, `ESC[201~` ni controles salvo
  tabulador y saltos de línea; finales de línea a CR) y marcas si el remoto
  activó 2004».
- «Migración 10: HOSTS `reconectar` (1), `tmux_persistente` (0) y
  `portapapeles_remoto` (0), propios del equipo».
- «`[terminal]`: `historial_lineas`, `historial_mib`, `raton`,
  `teclado_mejorado`, `avisar_pegado`, `abrir_enlaces` y `reconexion_max_min`,
  con sus rangos y aviso al arrancar» (configuración validada; falta que el
  arranque del cliente lea el aviso y la vista lo use).

## Pendientes y bloqueos

- Servidor: parser por pestaña con historial y respuestas, `CapturarHistorial`
  → `HistorialLote`, foco, OSC 52, título/aviso, reconexión automática, tmux,
  escritura en varios y túneles temporales (en curso).
- Cliente: emulador sin responder y salida sincronizada, paneles, modo
  historial, enlaces, ratón, pegado, portapapeles, reconexión visible, tmux,
  ficha, ayuda e instantáneas (en curso).
- Núcleo: historial (captura, búsqueda, selección), enlaces, tmux y reconexión
  (en curso; enlaces/tmux/reconexión con pruebas ya escritas).
- `tests/terminal_servidor.rs`, `tests/terminal_ui.rs`, `tests/terminal_e2e.rs`
  y las instantáneas de `tests/redimensionado.rs`.
- Revisión adversarial de cierre, checklist marcado y este informe completo.

## Ejecución y pruebas

- Compilar: `cargo build --locked`; la TUI: `cargo run`.
- Migrar: al abrir MAGI, la migración 10 se aplica sola (user_version 10); una
  base anterior se actualiza y una futura no se abre.
- Pruebas del núcleo de esta sesión: `TZ=UTC cargo test -p magi-nucleo --lib`
  (242 pruebas, incluidas las 18 del emulador nuevo).
- Pruebas de la migración: `TZ=UTC cargo test -p magi-ssh --test almacen`.
- Protocolo: `TZ=UTC cargo test -p magi-ssh --test protocolo`.
- Sin red externa ni Claude Code: las pruebas del terminal usan el servidor SSH
  de prueba y `tests/fijos/terminal/tui_prueba.rs` (se compila con rustc desde
  la prueba de extremo a extremo).
- Guion de validación manual con Claude Code real (para el desarrollador, sin
  red externa en las pruebas automáticas):

  1. Claude Code en modo clásico: pegar 20 líneas (un solo pegado, ningún
     mensaje por línea), Mayús+Intro (salto de línea sin enviar), `Ctrl+J`,
     `Shift+RePág` y búsqueda en su salida; cambiar el tamaño de la ventana sin
     conversaciones duplicadas.
  2. Claude Code a pantalla completa (`/tui fullscreen`): rueda, RePág/AvPág,
     selección con el ratón y copia (con el permiso de `prefijo P`), sin
     parpadeo.
  3. Avisos: `preferredNotifChannel = "terminal_bell"` y una pestaña en
     segundo plano esperando permiso: `!` y el texto en la barra.
  4. Alacritty y Ghostty (y foot, si está): Mayús+Intro; `teclado_mejorado =
     false` como respaldo.
  5. Historial: `seq 1 20000`, buscar `19999`, copiar un rango y guardarlo en
     un fichero.
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

- Revisión de la codificación del ratón: durante la sesión se detectó (y se
  corrigió) un desfase de una celda en la codificación clásica de xterm, que
  usaba `33 + coordenada` en vez de `32 + coordenada` y desbordaba a 223; la
  prueba de caja de `terminal_raton` lo cubre desde ahora.
