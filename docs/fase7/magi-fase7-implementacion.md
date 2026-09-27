# MAGI - Fase 7: Redimensionado Adaptable

## Informe de Implementación

**Última actualización:** 27 de septiembre de 2026 (sesión 1: S1 cerrado, S2 y S3 en curso)

---

## Resumen de lo implementado

La fase se desarrolla en la rama `fase7-fix_redimensionado_ventana`, con un commit por sprint:

| Sprint | Commit | Contenido |
|---|---|---|
| S1 | (este) | Diagnóstico de los dos fallos con tests en rojo, arreglo del servidor y del hilo de teclas, tubería única de tamaño (`Geometria`), extremo a extremo |
| S2 | — | Disposición adaptable |
| S3 | — | Instantáneas, secuencias, revisión adversarial y cierre |

### Diagnóstico de R36 (primer paso de la fase)

Las dos reproducciones se escribieron antes de tocar el código y fallaron contra la lógica
vieja. Hay **dos causas independientes**, una en cada proceso.

**Fallo 2: el remoto no recibe el tamaño nuevo.** La causa está en el servidor:
`src/servidor/sesiones.rs::tamano_minimo`.
- El mínimo entre ventanas se calculaba como `adjuntos.values().fold(actual, min)`: partía del
  tamaño ya aplicado. El resultado nunca podía superar al aplicado, así que el tamaño de una
  sesión podía encoger pero **nunca crecía**.
- Viene de la Fase 3 (commit `1e1470c`).
- **Lo que fallaba:**
  - crecer la ventana;
  - el zoom con `Ctrl+-`;
  - salir de un mosaico;
  - cerrar la ventana pequeña de una pestaña compartida;
  - adjuntar una ventana más grande que el tamaño con el que se abrió la pestaña (por ejemplo,
    `AbrirSesion` a 80×24 y después `Adjuntar` a 100×26).
- **Lo que sí funcionaba:** encoger, porque el mínimo bajaba. Por eso el fallo parecía
  intermitente.
- **Tests en rojo:** los cinco de `tests/redimensionado/remoto.rs` fallaban con «el host
  esperaba 1 window-change y recibió []».
- **Hallazgos colaterales:**
  - `limpiar_cliente` duplicaba el cálculo y difundía `ventana_minima: None`.
  - `adjuntar` no avisaba a la ventana que se adjuntaba si el tamaño no cambiaba, así que esa
    ventana no pintaba `░` ni «(mín. ventana N)».
  - La vista Sesión pintaba el terminal sobre toda el área, tapando el relleno `░`.

**Fallo 1: la interfaz no se repinta hasta pulsar una tecla.** La causa está en el cliente: el
hilo de teclas, con la pausa del visor F4 (`src/app.rs::lanzar_hilo_teclas`).
- Durante la pausa, el hilo ejecutaba `poll(ZERO); read()` y descartaba lo leído. Ese `read()`
  **bloquea**.
- Al cerrar el paginador, el hilo seguía bloqueado en la rama de pausa. Tiraba el primer evento
  que llegase:
  - si era un `Resize`, no había repintado ni `Redimensionar` hasta pulsar una tecla;
  - si esa tecla llegaba en la vista Sesión, el remoto no se enteraba nunca del tamaño nuevo.
- Mientras el paginador estaba abierto, además, le robaba pulsaciones a `less`.
- Viene de la Fase 4 (commit `3ac9efe`).
- **Tests en rojo contra la lógica vieja:** `la_pausa_no_lee_el_tty` y
  `el_resize_tras_la_pausa_no_se_pierde` en `src/app/teclado.rs`.

**Fuera del visor, el bucle viejo sí repintaba.** Con `sucio` más el `autoresize` de ratatui,
un `Resize` repintaba. La reproducción literal del checklist (pintar a 120×40, `Resize(80,24)`
y ningún evento más) no falla fuera del visor. Queda como regresión de la tubería nueva
(`fallo1_repinta_sin_tecla`), en la que el riesgo real es no despertarse con un tamaño
pendiente. Si Hector vio el fallo 1 sin haber abierto el visor en esa sesión, la causa no está
localizada. Queda anotado en «Pendientes y bloqueos» para la validación manual.

### S1 — Tubería de redimensionado

- **Servidor**
  - `tamano_minimo` es el mínimo por componente de los adjuntos y, sin adjuntos, conserva el
    último.
  - `ventana_minima`: la ventana que impone las columnas; a igualdad, la de menos filas y
    después el id menor.
  - `aplicar_tamano(estado, sesion, avisar_a)`:
    - es el único punto que envía `AplicarTamano` (`window_change` y parser);
    - difunde `Redimensionada` cuando cambia el tamaño o la ventana que lo impone;
    - al adjuntar avisa a la ventana nueva aunque nada cambie.
  - `limpiar_cliente` lo reutiliza.
  - La apertura sanea el tamaño también para el parser.
  - Sin cambios de protocolo ni de `VERSION_PROTOCOLO`: misma semántica de mensajes.
- **Hilo de teclas** (`src/app/teclado.rs`)
  - Tiene una `FuenteEventos` inyectable y un `ControlTeclas` con pausa con acuse (`Condvar`).
  - Con la pausa **no toca el tty**. La UI espera el acuse, como mucho 200 ms, antes de lanzar
    el paginador.
  - `Resize` siempre pasa como `Evento::Redimension`.
- **`Geometria`** (`src/app/geometria.rs`)
  - Guarda el tamaño aplicado, el pendiente y la hora del último evento.
  - Agrupa durante 50 ms y solo aplica el último tamaño; uno igual al aplicado no hace nada.
  - `espera()` pide un tick de ≤ 16 ms solo mientras hay un tamaño pendiente.
- **Bucle** (`App::ciclo` y `App::paso`, genéricos sobre `Backend`)
  - Sin nada pendiente, `blocking_recv`; con un tamaño pendiente, `timeout` de ≤ 16 ms dentro
    del runtime.
  - Con un tamaño pendiente no se pinta, para no limpiar dos veces.
  - `aplicar_tamano`:
    1. `terminal.resize`, que limpia la pantalla y el búfer anterior;
    2. confirma el tamaño y repinta;
    3. en la vista Sesión envía `Redimensionar` de la pestaña adjunta con `alto_pty` del
       tamaño aplicado, también por debajo del mínimo (saneado a 2×1).
- **Un solo origen del tamaño del PTY:** `App::tamano_pty()`. Lo usan `AbrirSesion`,
  `Adjuntar` (al entrar en Sesión, cambiar de pestaña y readjuntar tras relanzar el servidor) y
  `Redimensionar`. Desaparecen las llamadas a `crossterm::terminal::size()` fuera del bucle.
- **Vuelta del visor** (`ver_en_paginador` y `volver_de_suspension`): se lee el tamaño real. Si
  cambió se aplica; si no, solo se repinta.
- **App testeable**
  - `App::nuevo` = `construir` (solo estado) + efectos: hilo de teclas, tick, identidades,
    sondeos, Flota y `conectar_con_servidor`.
  - `App::de_prueba` no lanza hilos, no conecta con el servidor y no sale a la red
    (`efectos_externos = false`). Captura lo enviado con `Cliente::de_prueba`.
- **Vista Sesión:** el terminal se pinta solo en la zona del tamaño remoto y deja visible el
  relleno `░` de la ventana mayor.

## Desviaciones respecto a la especificación (qué y por qué)

- **Mínimo de Sesión 40×8 por debajo del global 40×12.** Decisión del desarrollador: el
  mínimo de cada vista sustituye al global, que es el valor por defecto. El informe decía
  «global 40×12 para cualquier vista» y a la vez «Sesión 40×8».
- **`Geometria` vive en `src/app/geometria.rs`**, reexportada como `app::Geometria`. Desde la
  Fase 6, `app` es un módulo con submódulos y `app.rs` pasa de 9 000 líneas.
- **Superficie de pruebas pública (`#[doc(hidden)] pub`):**
  - `App::de_prueba`, `conectar_con_servidor`, `procesar`/`procesar_en`;
  - `iniciar_pintado`, `ciclo`, `paso`, `bombear` y `volver_de_suspension`;
  - `geometria()` y `Cliente::de_prueba`.

  Hacen falta para que `tests/redimensionado.rs` ejerza el bucle real.
- **Mejoras en el servidor fuera del arreglo estricto:**
  - `Redimensionada` a la ventana que se adjunta aunque el tamaño no cambie;
  - difusión cuando cambia solo la ventana que impone el mínimo;
  - nuevo criterio de `ventana_minima`.

  Son necesarias para el ítem «La ventana mayor rellena con `░`… en cada cambio». Sin
  mensajes nuevos.

## Estructura de archivos creada/modificada

- **Nuevos:**
  - `src/app/teclado.rs`, `src/app/geometria.rs`
  - `tests/redimensionado.rs`
  - `tests/redimensionado/{arnes,remoto,tuberia,extremo}.rs`
  - `docs/fase7/magi-fase7-implementacion.md`
- **Modificados:**
  - `src/app.rs`: bucle, constructor, `procesar_en`, `tamano_pty` y visor.
  - `src/app/{resultados,snippets}.rs`: alto aplicado.
  - `src/cliente/mod.rs`: `Cliente::de_prueba`.
  - `src/servidor/{mod,sesiones}.rs`: tamaño mínimo, `aplicar_tamano` y `limpiar_cliente`.
  - `src/ui/sesion.rs`: área remota.
  - `tests/comun/mod.rs`: el host de pruebas registra `pty-req` y `window-change` y escribe
    `TAM c×f`; `Escenario::otra_ventana` y `cliente_id`.
  - `CLAUDE.md`: regla de tamaño de terminal de la Fase 7.

## Decisiones técnicas tomadas durante el desarrollo

- **No se pinta con un tamaño pendiente.** Un pintado durante la agrupación haría que el
  `autoresize` de ratatui limpiase por su cuenta, y serían dos limpiezas por tamaño.
- **`aplicar_tamano` usa `terminal.resize`,** que ya limpia la pantalla y resetea el búfer
  anterior: no se añade otro `clear`. Al encoger en horizontal, ratatui emite dos
  `clear_region(All)` seguidos; el test los cuenta como una sola ráfaga.
- **El `timeout` del bucle se crea dentro de `runtime.block_on(async { … })`.** Fuera del
  runtime no hay reloj y `tokio::time::timeout` entra en pánico. Lo detectó el test
  `fallo1_repinta_sin_tecla`.
- **El host SSH de pruebas contesta cada `window-change` con `TAM c×f`.** Automatiza el AC de
  `stty size` tras el zoom de Alacritty y deja comprobarlo en la pantalla de la App.
- **Riesgo aceptado:** si entra otro SIGWINCH entre `aplicar_tamano` y el `draw`, ratatui
  vuelve a limpiar. Es inherente a `Viewport::Fullscreen`, y el `Resize` que llega después
  lo corrige.

## Funcionalidades del checklist completadas (copiando su texto exacto)

- Test que reproduce el fallo 1: tras pintar a 120×40 y recibir `Resize(80, 24)` sin más eventos, la vista debe quedar pintada a 80×24 sin pulsar ninguna tecla
- Test que reproduce el fallo 2: con una pestaña adjunta, un `Resize` debe llegar al remoto como `window_change` con el tamaño nuevo (el servidor SSH de pruebas registra los `window_change` recibidos)
- Causa de los dos fallos documentada en el informe de implementación (dónde se perdía el evento o el tamaño) y ambos tests convertidos en regresión permanente
- `Event::Resize` nunca se descarta ni se filtra en el hilo de eventos; la pausa del teclado durante el visor (F4) no afecta a los eventos de tamaño
- `Geometria` en `app.rs` con tamaño aplicado, pendiente y hora del último evento; agrupación de 50 ms que aplica solo el último tamaño
- El bucle se despierta por tick (≤ 16 ms) mientras hay un tamaño pendiente, sin esperar a una tecla
- Un tamaño igual al ya aplicado no hace nada
- Al volver del visor o de cualquier suspensión de la TUI se lee el tamaño real y se aplica
- Al relanzar el servidor y readjuntar pestañas, `Adjuntar` lleva el tamaño aplicado
- En la vista Sesión, cada tamaño aplicado envía `Redimensionar{sesion_id, cols, alto_pty(filas)}` de la pestaña adjunta
- `alto_pty` se calcula siempre con el tamaño aplicado, nunca con uno guardado al abrir la pestaña
- Cambiar de pestaña adjunta la nueva con el tamaño aplicado y la desadjuntada deja de contar para el mínimo
- Pestaña compartida: el servidor recalcula el mínimo de los adjuntos en cada `Redimensionar`, `Adjuntar` y `Desadjuntar`, envía `window_change` si cambia y difunde `Redimensionada`
- La ventana mayor rellena con `░` la zona sobrante e indica `cols×filas (mín. ventana N)` en cada cambio
- Test de extremo a extremo: `vim` o `htop` simulados reciben el `window_change` tras cada tamaño aplicado, con una y con dos ventanas

## Pendientes y bloqueos

- **Queda todo S2** (disposición adaptable, mínimos y aviso) **y S3** (instantáneas,
  secuencias, revisión adversarial, README).
- **`aplicar_tamano` todavía no recalcula ninguna disposición ni recorta desplazamientos.** Lo
  hará en S2, con `ui/disposicion.rs`. Mientras tanto, las teclas de página usan el alto
  aplicado (`App::terminal_alto()`).
- **Validación manual pendiente, a cargo de Hector:**
  - mosaico de Hyprland;
  - `togglefloating` y pantalla completa;
  - zoom de Alacritty con `stty size` en una pestaña;
  - `htop` al crecer y al encoger;
  - dos ventanas en la misma pestaña;
  - redimensionar con `less` abierto y volver.

  Si el fallo 1 se reproduce sin haber usado el visor, la causa no está localizada.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

- Arranque sin cambios: `cargo run`. Para depurar el servidor, `cargo run -- --servidor` en
  otra terminal. No hay migraciones nuevas.
- Pruebas de la fase: `cargo test --test redimensionado`.
  - `remoto`: servidor y host SSH de pruebas.
  - `tuberia`: App de prueba con `TestBackend`.
  - `extremo`: App real contra el servidor en proceso.
- Tests unitarios: `cargo test --lib app::teclado app::geometria servidor::sesiones`.
- Antes de cada commit: `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`.
