# MAGI - Fase 7: Redimensionado Adaptable

## Informe de Implementación

**Última actualización:** 27 de septiembre de 2026 (sesión 1: fase cerrada, 46/46; pendiente la validación manual)

---

## Resumen de lo implementado

La fase se desarrolla en la rama `fase7-fix_redimensionado_ventana`, con un commit por sprint:

| Sprint | Commit | Contenido |
|---|---|---|
| S1 | `8dd98fb` | Diagnóstico de los dos fallos con tests en rojo, arreglo del servidor y del hilo de teclas, tubería única de tamaño (`Geometria`), extremo a extremo |
| S2 | `6c42856` | Disposición adaptable: `ui/disposicion.rs`, aviso de tamaño, todas las vistas y diálogos, instantáneas |
| S3 | `ab7e82e` | Barrido sin pánico, secuencias de cambio, revisión adversarial con sus arreglos, README |
| Cierre | (este) | Checklist e informe |

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
    después el id menor. En S3 pasa a calcularse para cada ventana (ver «S3»).
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

### S2 — Disposición adaptable

**Base común**
- **`src/ui/disposicion.rs`**
  - Puntos de corte: estrecho < 100, columnas mínimas < 80, sin usuario·puerto < 60, bajo < 20 y grande ≥ 200×60.
  - `ModoAncho {Normal, Estrecho, Minimo}` y mínimos por vista con `minimo_de`, que sube al del diálogo MAGI si está abierto.
  - Columnas y atajos por prioridad, con «? más».
  - Encaje: `centrar_limitado` y `limitar_ancho`.
  - `ventana()` para las listas.
  - Texto: `recortar`/`columna`, y `texto_ascii`/`adaptar` como único degradador ASCII.
  - `Disposicion` del último pintado.
- **`src/ui/aviso_tamano.rs`**
  - Aviso centrado «ventana demasiado pequeña · actual · mínimo».
  - «(Vista necesita W×H)»: en S2 solo cuando lo que se muestra es el mínimo global; desde S3,
    siempre.
  - Tres niveles: con borde, sin borde y solo `MAGI c×f` por debajo de 20×3.
- **`ui::dibujar` devuelve la `Disposicion`.**
  - Cada vista recibe `&mut Disposicion` y registra la ventana de sus listas.
  - Tras pintar, `App::sincronizar_con` (y `sincronizar_resultados`) reescribe los desplazamientos guardados con lo pintado: la selección queda siempre a la vista y sin huecos al final.
  - Las teclas de página usan `disposicion.filas(Lista::X)`.
  - Desaparecen `terminal_alto` y todas las funciones `alto_lista`/`alto_panel`/`alturas_*`.
  - `PgUp`/`PgDn` avanzan una página visible, no 10 fijo.
- **Con el aviso visible**
  - Solo pasan `q`, `F1`-`F8` y `Ctrl+P`. En la ficha, `q` equivale a `Esc`, salvo con cambios
    sin guardar (S3).
  - Con una deliberación tapada por el aviso solo pasa `Esc`, para cancelarla, también en
    Sesión (S3).
  - En Sesión todo sigue yendo al remoto.
  - Diálogos, paleta y ayuda se pintan encima del aviso y conservan sus teclas.
- **`Tema.glifos`** amplía sus glifos (selección, relleno, puntos, ×, ·, ↵, ⇥, líneas, ↑↓) con su equivalente ASCII.

**Vistas.** Siete grupos en paralelo, cada uno en su worktree y con un revisor adversarial que corrigió lo que encontró. Luego se integraron aquí.
- **A. Flota, Hosts, Sesiones y barra**
  - Flota en estrecho enseña un panel; `Tab` alterna lista y detalle, con la cabecera «⇥ detalle/lista».
  - Las barras de carga, memoria y disco tienen ancho variable.
  - A ≥ 200×60, Flota es un bloque centrado (lista de 48 + detalle de 120 como máximo).
  - Columnas por prioridad en Hosts (dirección < 80, usuario·puerto < 60) y en Sesiones.
  - La barra inferior de todas las vistas tiene atajos por prioridad y termina en «? más» o «? ayuda».
- **B. Ficha**
  - Desplazamiento vertical con el campo con foco siempre visible y marcas ↑/↓.
  - En estrecho, las etiquetas van encima de los campos.
  - Desplegables y sugerencias anclados según el modo.
- **C. Sesión**
  - Pestañas compactas: solo glifo y número si no cabe el nombre.
  - Barra de estado por prioridad; el indicador «cols×filas (mín. ventana N)» se mantiene.
  - Relleno `░` (`.` en ASCII).
  - Sin cursor sobre una pestaña caída.
- **D. Archivos y Transferencias**
  - Archivos en estrecho: un panel con `[local]`/`[remoto] ⇥`. La cola se resume en el pie con alto < 20 y en estrecho.
  - Los paneles no pasan de 240 columnas.
  - Transferencias: columnas por prioridad y detalle plegado con alto < 20.
- **E. Túneles, Registro, Identidades y Snippets**
  - Columnas por prioridad y host abreviado en Túneles.
  - Con alto < 20 el detalle se pliega y se abre como `Dialogo::Detalle` con todo su contenido: `↵` en Túneles, Registro e Identidades; `i` en Snippets.
- **F. Resultados, diálogo MAGI, EJECUTAR y formulario**
  - Resultados: con alto < 24 la salida solo se ve con `↵`; paneles apilados.
  - Diálogo MAGI compacto: `M-1`, `B-2`, `C-3`, datos recortados, hosts con desplazamiento y `PgUp`/`PgDn`.
  - Rejilla de EJECUTAR con columnas según el ancho (`columnas_rejilla`).
  - Formulario que sigue al foco.
- **G. Diálogos, ayuda y paleta**
  - Todos los diálogos de `dialogos.rs` se recolocan y encogen con desplazamiento: `↑` `↓` `PgUp` `PgDn` e indicador «↑↓ i/n».
  - Ayuda con desplazamiento y las teclas nuevas de la fase.
  - Paleta que encoge, con `PgUp`/`PgDn`.

**Integración**
- `?` abre la ayuda también en Archivos, Transferencias, Sesiones y con `prefijo ?` en Sesión: la barra ya lo anunciaba y no hacía nada.
- La barra anuncia «↵ detalle» (Identidades) e «i detalle» (Snippets) en vista baja.
- EJECUTAR se pinta con la `Disposicion` (`columnas_rejilla` registrada).
- Los tres degradadores ASCII duplicados se unifican en `disposicion::texto_ascii`.
- Las pruebas ASCII de todos los grupos incluyen ya la barra inferior.

### S3 — Pruebas, revisión y cierre

**Pruebas de pintado**
- `tests/redimensionado/barrido.rs`: todas las vistas, la paleta, la ayuda, el diálogo MAGI,
  EJECUTAR, el formulario, el visor de Resultados y las 17 variantes de diálogo pasan por la
  rejilla de anchos {1, 2, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100, 200} × altos {1, 2, 3, 7, 8,
  11, 12, 19, 20, 23, 24, 60}. En cada tamaño se comprueba que no hay pánico, que el pintado
  tiene el tamaño nuevo y que toda lista deja la selección a la vista y sin huecos. Se hace en
  Unicode y en ASCII.
- `tests/redimensionado/secuencias.rs`: 200×60 → 80×24 → 40×12 → 200×60 con:
  - Hosts con 50 hosts más, la selección en la fila 30 y un filtro;
  - Flota con el detalle a la vista en estrecho;
  - Archivos con marcados y el panel remoto activo;
  - una entrada de texto a medio escribir.

  En cada paso se comprueba el estado y al final hay una instantánea.
- En total, 178 pruebas en `tests/redimensionado.rs` y 128 instantáneas en `tests/snapshots/`.

**Revisión adversarial de cierre** (obligatoria, servidor y cliente por separado)
- Un workflow con cinco revisores: servidor, tubería y estado del cliente, y tres bloques de UI.
  Cada hallazgo lo verificaron tres escépticos con enfoques distintos (trazar el código,
  reproducirlo, contrastarlo con la especificación); se confirmaba con al menos 2 de 3.
- Resultado: 19 hallazgos, **14 confirmados y corregidos**, 5 refutados.

| # | Zona | Hallazgo | Arreglo |
|---|---|---|---|
| 1 | servidor | Una sola `ventana_minima` para todas: con una ventana que impone las columnas y otra las filas, la de más filas se veía «(mín. esta ventana)» | `ventana_minima_para(adjuntos, tamaño, destino)`: cada adjunto recibe la ventana que explica su propio relleno. `Redimensionada` se envía por ventana y cambia solo lo que cambia. Pruebas unitarias y `cada_ventana_sabe_quien_explica_su_relleno` |
| 2 | cliente | `Redimensionada` se aplicaba al parser en el bucle de la App, después de datos que el remoto ya había pintado a ese tamaño (pantalla corrupta al crecer) | La tarea de lectura aplica el tamaño en orden con los datos (`Pantallas::redimensionar`) y reenvía el mensaje a la App |
| 3 | servidor | `PantallaCompleta` declaraba el tamaño de la sesión y no el del parser del que sale el volcado (sesión caída, o `AplicarTamano` en cola) | El volcado lleva el tamaño del parser, leído bajo el mismo cerrojo |
| 4 | pruebas | `ac_dos_ventanas_la_pequena_se_cierra` aceptaba el `Redimensionada` del primer `Adjuntar` | Se vacía `vistos` antes de cerrar la ventana |
| 5 | cliente | En Sesión, con el aviso tapando la deliberación, Ctrl+K ejecutaba el plan sin que se viera el diálogo | Con una deliberación tapada solo pasa `Esc`, también en Sesión. Prueba de regresión |
| 6 | cliente | La paleta encogida a la línea de la consulta ejecutaba con `↵` una entrada que no se veía | `↵` solo ejecuta si la selección estaba a la vista en el último pintado. Prueba |
| 7 | cliente | Un redimensionado de ida y vuelta dentro de la agrupación dejaba restos: no había `clear` ni repintado | `Geometria::vencido` devuelve `Vencido::Repintar`: una limpieza y un repintado, sin `Redimensionar`. Pruebas |
| 8 | UI | El aviso no decía qué exigía el mínimo si se cumplía el global (por ejemplo, el diálogo MAGI tapado) | El aviso dice siempre «(X necesita W×H)» |
| 9 | UI (pérdida de datos) | Con el aviso, una «q» tecleada a ciegas en la ficha con cambios se volvía `Esc`, y una «s» o un `↵` detrás descartaban los cambios | Con cambios sin guardar, `q` se ignora; salir sigue siendo posible con `F1`-`F8`, que confirman. **Prueba de regresión, roja con el código anterior** |
| 10 | UI | Transferencias no limitaba el ancho a ≥ 200×60 | `limitar_ancho(ANCHO_MAX_DETALLE)`, como las demás tablas |
| 11 | UI | La pregunta de contraseña o frase ocultaba el host al desplazarse hasta el campo | El host va en el título («CONTRASEÑA · host»). Los formularios muestran la posición sin flechas, porque ↑↓ no los desplazan. Prueba a 40×8 |
| 12 | UI | En Túnel y Generar clave, el aviso de 0.0.0.0, el de clave sin frase y las casillas que aplica `^s` no se veían en su mínimo | Pasan al pie fijo del diálogo. Pruebas a 50×12 y 60×12 con el foco en todos los campos |
| 13 | UI | Formulario de snippet bajo: la opción resaltada del desplegable quedaba fuera y la pregunta de descartar no se pintaba | Con poco hueco se quitan antes las marcas «más» y el filtro que la resaltada; con una fila manda el pie. Prueba |
| 14 | UI | `modal()` (EJECUTAR, formulario, deliberación) no degradaba a ASCII («», ¿) | Título y contenido pasan por `linea_ascii`. Prueba |

- **Refutados** (sin cambio):
  - tamaños enormes que tumbarían el servidor por memoria: vt100 acota y el cliente sanea;
  - el arreglo no llegaría con un servidor anterior vivo: misma semántica, sin cambio de
    protocolo, y el servidor se relanza con cada versión;
  - teclas perdidas durante los 50 ms de agrupación: se procesan igual;
  - datos sin degradar en Archivos y Transferencias en ASCII;
  - un duplicado del hallazgo 5.

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
- **Detalle plegado de Snippets con `i` y no con `↵`** (checklist: «se abre con `↵`»): en Snippets `↵` ejecuta. En Identidades `↵` estaba libre y se usa.
- **Aviso de tamaño.**
  - Los diálogos, la paleta y la ayuda se pintan encima del aviso y conservan sus teclas: una
    pregunta del servidor nunca queda bloqueada. La paleta no ejecuta una entrada que no se ve.
  - Con una deliberación tapada por el aviso solo pasa `Esc`, que la cancela, también en
    Sesión, donde el resto de teclas va al remoto.
  - En la ficha con cambios sin guardar, la `q` del aviso se ignora; con la ficha limpia
    equivale a `Esc`.
  - El aviso dice siempre qué vista o diálogo exige el mínimo, también cuando se muestra el
    propio (la maqueta solo lo enseñaba con el global).
- **Un tamaño igual al ya aplicado no hace nada**, salvo si la ráfaga pasó por otro tamaño (ida
  y vuelta): entonces se limpia y se repinta una vez, sin `Redimensionar`. El terminal ya había
  truncado lo pintado.
- **Sesiones no tiene dirección ni usuario·puerto.** Oculta por prioridad sus propias columnas:
  - HOST por debajo de 60 columnas;
  - IDENTIDAD y VENT. por debajo de 80;
  - ESTADO y PESTAÑA nunca.

  Su panel inferior se pliega con alto < 20.
- **Flota** (siguiendo la maqueta §6.1):
  - el nombre va antes de la palabra de estado;
  - la barra dice «↵ ssh»;
  - el resumen pasa al borde superior derecho (también en Hosts);
  - hay un separador vertical entre lista y detalle;
  - a ≥ 200×60 también se limita la lista (48), además del detalle (120), para que el bloque se vea centrado.
- **Archivos:** la cola se resume en el pie también en modo estrecho (§7.2 y maqueta §6.4), no solo con alto < 20. Los paneles no pasan de 240 columnas.
- **Paneles inferiores de Túneles y Snippets:** dejan de montar sobre el borde del marco y van anidados, como el de Identidades. Registro enmarca su detalle.
- **Sesiones:** sus atajos pasan a la barra global y «desde {Duration:?}» pasa a «desde hace N s».
- **Formulario de snippet:** además de no desbordar, desplaza el cuerpo hasta el campo con foco (§7.5).
- **Diálogos y ayuda.** El indicador «↑↓ i/n» va en el borde inferior del recuadro. «↑↓ PgUp PgDn desplazar diálogos» se añade al final de la ayuda de todas las vistas.
- **Mejoras en el servidor fuera del arreglo estricto:**
  - `Redimensionada` a la ventana que se adjunta aunque el tamaño no cambie;
  - `ventana_minima` calculada para cada ventana (la que explica su relleno) y enviada por
    ventana;
  - el volcado de `PantallaCompleta` declara el tamaño de su parser.

  Son necesarias para el ítem «La ventana mayor rellena con `░`… en cada cambio». Sin
  mensajes nuevos.

## Estructura de archivos creada/modificada

- **S3, nuevos:** `tests/redimensionado/{barrido,secuencias,revision}.rs` y 4 instantáneas de
  secuencias.
- **S3, modificados:**
  - `src/servidor/{mod,sesiones}.rs`: `ventana_minima_para`, `Redimensionada` por ventana,
    tamaño del volcado.
  - `src/cliente/{mod,pantallas}.rs`: tamaño aplicado en la tarea de lectura.
  - `src/app.rs`: filtro del aviso, `q` en la ficha, paleta y repintado de ida y vuelta.
  - `src/app/geometria.rs`: `Vencido`.
  - `src/ui/{aviso_tamano,dialogos,formulario_snippet,transferencias}.rs`.
  - `README.md`: sección «Tamaño de la terminal», atajos y pruebas.

- **S2, nuevos:**
  - `src/ui/disposicion.rs`, `src/ui/aviso_tamano.rs`
  - `tests/redimensionado/{semilla,vistas_a…vistas_g}.rs`
  - `tests/snapshots/` (124 instantáneas)
- **S2, modificados:**
  - todas las vistas de `src/ui/` y `src/tema.rs`;
  - `src/app.rs`: disposición del último pintado, sincronización, teclas de página, filtro del aviso, desplazamiento de modales, `Tab` de Flota, detalle de Identidades y `?` en más vistas;
  - `src/app/{resultados,lanzar,snippets,dialogo_magi}.rs`;
  - `Cargo.toml` (`insta` en desarrollo).
- **S1, nuevos:**
  - `src/app/teclado.rs`, `src/app/geometria.rs`
  - `tests/redimensionado.rs`
  - `tests/redimensionado/{arnes,remoto,tuberia,extremo}.rs`
  - `docs/fase7/magi-fase7-implementacion.md`
- **S1, modificados:**
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
- **Instantáneas deterministas sin reloj inyectable.** El plan preveía un `App.reloj`, pero
  basta con esto:
  - las fechas absolutas de la semilla son fijas y las pruebas corren con `TZ=UTC`;
  - las relativas se siembran respecto a la hora real, lejos de los redondeos;
  - un filtro de insta cambia «hace N s» por un texto del mismo ancho;
  - la ruta temporal tiene longitud fija (`/tmp/magiXXXXXX`).

  Así la App no cambia para las pruebas.
- **Reparto de S2 en siete grupos en paralelo** (worktrees aislados, con ficheros disjuntos y un
  revisor por grupo), integrados aquí sin conflictos. Los pendientes cruzados que anotaron (`?`
  en más vistas, degradadores ASCII duplicados, rejilla de EJECUTAR) se resolvieron al integrar.
- **Un solo degradador ASCII** (`disposicion::texto_ascii`). Conserva letras con tilde y quita
  `¿`/`¡`, porque se lee mejor «Seguro?» que «?Seguro?». El criterio de las pruebas ASCII es
  `c.is_ascii() || c.is_alphabetic()`.

## Funcionalidades del checklist completadas (copiando su texto exacto)

46 de 46.

**S3**
- `tests/redimensionado.rs` con `TestBackend`: todas las vistas y diálogos pintan sin panic a 40×12, 80×24 y 200×60
- Instantáneas `insta` de cada vista a los tres tamaños, revisadas y guardadas en `tests/snapshots/`
- Secuencias de cambio (200×60 → 80×24 → 40×12 → 200×60) con estado coherente: selección visible, filtro y marcados conservados, diálogo abierto recolocado
- Toda vista nueva que se añada en el futuro debe incluirse en estas pruebas (anotado en `CLAUDE.md`)
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- `cargo test` verde, incluidas las instantáneas
- Revisión adversarial del código tocado antes de cerrar, con resultado en el informe de implementación
- README: comportamiento al redimensionar, modo estrecho, tamaños mínimos

**S2**
- `ui/disposicion.rs` con los puntos de corte (estrecho < 100 columnas, columnas mínimas < 80, bajo < 20 filas), `ModoAncho {Normal, Estrecho, Minimo}` y los mínimos por vista
- Ninguna vista guarda tamaños entre pintados; las teclas de página y filas visibles usan la disposición del último pintado
- Tras aplicar un tamaño, la fila seleccionada sigue visible y el desplazamiento no deja huecos al final de la lista
- El cambio de modo conserva selección, marcados, filtro, panel activo y el texto de los diálogos
- Tablas con columnas por prioridad: el identificador principal y el glifo de estado nunca se ocultan
- Barra inferior con atajos por prioridad y `? más` cuando no caben todos
- Paneles, detalles y diálogos con ancho máximo y centrados en ventanas muy grandes (≥ 200×60)
- Mínimo global 40×12 y mínimos por vista: Ficha 50×14, Sesión 40×8, Archivos 50×14, Transferencias/Túneles/Registro/Identidades/Snippets 50×12, Resultados 60×14, diálogo MAGI 50×12
- Por debajo del mínimo se pinta el aviso centrado «ventana demasiado pequeña · actual · mínimo» con la vista que lo exige, en lugar de la vista
- Si ni el aviso cabe (menos de 20×3), se pinta solo `MAGI cols×filas`
- Con el aviso visible siguen activas `q`, `F1`-`F8` y `Ctrl+P`
- Por debajo del mínimo de Sesión el remoto sigue recibiendo su tamaño real saneado (mín. 2×1) y las teclas siguen yendo al remoto
- Flota: en estrecho un panel y `Tab` alterna lista y detalle; barras de carga, memoria y disco con ancho variable
- Hosts y Sesiones: ocultan dirección por debajo de 80 columnas y usuario·puerto por debajo de 60
- Ficha de host: desplazamiento vertical con el campo con foco siempre visible; en estrecho, etiquetas encima de los campos
- Sesión: barra de pestañas compacta (solo glifo y número si no cabe el nombre) y barra de estado compactada por prioridad
- Archivos: en estrecho un panel con `Tab` e indicador `[local]` / `[remoto]` en la cabecera; con alto < 20 la cola se resume en el pie
- Transferencias, Túneles, Registro, Identidades y Snippets: columnas por prioridad y detalle inferior plegado con alto < 20 (se abre con `↵`)
- Resultados: con alto < 24 la salida solo se ve con `↵`; paneles de ejecuciones y hosts apilados
- Diálogos, paleta y ayuda `?`: se recolocan en cada cambio y encogen al área con desplazamiento (`↑` `↓`, `PgUp` `PgDn`)
- Diálogo MAGI compacto: nombres abreviados `M-1`, `B-2`, `C-3`, datos recortados y hosts con desplazamiento
- Modo ASCII degradado correcto en todos los modos (sin glifos Unicode en estrecho ni en el aviso)

**S1**

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

- **La fase está completa en código y pruebas: 46/46.**
- **Menores que quedan fuera del checklist:**
  - `tests/pool.rs::sigterm_para_el_servidor_limpio` falló una vez con varios worktrees
    compilando y probando a la vez. Pasó al repetir y pasa en todas las ejecuciones completas
    de esta sesión. Parece sensible a la carga de la máquina.
  - La pista de Sesión «N sesiones: usa ‹ › o la lista» nombra marcas, no teclas (viene de la
    Fase 3).
  - La barra del modo prefijo no enseña `f archivos` (viene de la Fase 4).
- **Menores anotados por los grupos, fuera del checklist:**
  - En ASCII, Túneles pinta `o` tanto para «activando/parando» como para «inactivo»; el texto
    los distingue.
  - Un desplegable encogido no marca que hay opciones por encima.
  - El título de un host nuevo sale «nuevo · nuevo» (viene de antes).
  - `transferencias::confirmar`/`atajos` son código muerto de la Fase 4.
- **Validación manual pendiente, a cargo de Hector:**
  - mosaico de Hyprland;
  - `togglefloating` y pantalla completa;
  - zoom de Alacritty con `stty size` en una pestaña;
  - `htop` al crecer y al encoger;
  - dos ventanas en la misma pestaña;
  - redimensionar con `less` abierto y volver;
  - el aviso por debajo del mínimo;
  - `MAGI_ASCII=1`.

  Si el fallo 1 se reproduce sin haber usado el visor, la causa no está localizada.
- **Las instantáneas solo recogen símbolos, no colores.** El resaltado de la selección y los
  colores del tema se comprueban a mano.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

- Arranque sin cambios: `cargo run`. Para depurar el servidor, `cargo run -- --servidor` en
  otra terminal. No hay migraciones nuevas.
- Pruebas de la fase: `cargo test --test redimensionado`.
  - `remoto`: servidor y host SSH de pruebas.
  - `tuberia`: App de prueba con `TestBackend`.
  - `extremo`: App real contra el servidor en proceso.
  - `vistas_a`…`vistas_g`: disposición e instantáneas por vista.
  - `barrido` y `secuencias`: rejilla de tamaños y cambios de modo.
  - `revision`: regresiones de la revisión adversarial.
- Instantáneas: tras un cambio visual deliberado, regenerar con
  `INSTA_UPDATE=always cargo test --test redimensionado` y revisar con
  `git diff tests/snapshots` o `cargo insta review`.
- Tests unitarios: `cargo test --lib app::teclado app::geometria servidor::sesiones`.
- Antes de cada commit: `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`.
