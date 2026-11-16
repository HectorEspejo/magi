# MAGI - Fase 16: Sesiones Persistentes — Informe de implementación

> Documento vivo: se actualiza en cada sesión. Estado: **Sprint 3 de 3 cerrado, con dos ítems sin marcar**: el de `modifyOtherKeys` (su criterio exige Claude Code real, paso 7 del guion de §7) y la **revisión adversarial de cierre, que el desarrollador pidió saltarse** (ver §6). Versión 1.5.0.
> Rama `fase16`. Contrato del Sprint 1 en el commit `68ab095`, Sprint 1 en `ff1b9cc`, contrato del Sprint 2 en `1d26f87`, Sprint 2 en `2efccfa` y contrato del Sprint 3 en `660b2d0` y `c7e8d5b`.

---

## 1. Resumen de lo implementado

### Sprint 1 — Prueba de concepto, núcleo, script y sincronización

**Prueba de concepto de Mayús+Intro (D338).** Se hizo primero, como pedía el prompt, con tmux 3.2a, 3.4 y 3.5 compilados desde las fuentes oficiales (sin instalar) y con el 3.7c del sistema como referencia: 96 casos, sin errores. Un cliente de tmux enganchado por un pty hace de MAGI (contesta DA1, DA2 y XTVERSION como el emulador del núcleo y manda las teclas como las codificaría MAGI) y una sonda dentro de tmux vuelca los bytes que recibe. Por decisión del desarrollador no se probó con un Claude Code real (queda para el paso 7 del guion, ver §6).

| tarball | SHA-256 |
|---|---|
| tmux-3.2a.tar.gz | `551553a4f82beaa8dadc9256800bcc284d7c000081e47aa6ecbb6ff36eacd05f` |
| tmux-3.4.tar.gz | `551ab8dea0bf505c0ad6b7bb35ef567cdde0ccb84357df142c254f35a23e19aa` |
| tmux-3.5.tar.gz | `2fe01942e7e7d93f524a22f2c883822c06bc258a4d61dba4b407353d7081950f` |

Lo medido (Mayús+Intro con `terminal-features xterm*:extkeys`; «pide modifyOtherKeys» = la aplicación envía `CSI > 4 ; 2 m`):

| tmux | `extended-keys` | La aplicación pide modifyOtherKeys | La aplicación pide solo kitty, o nada |
|---|---|---|---|
| 3.2a y 3.4 | `on` | llega `CSI 13;2u` | **se pierde** |
| 3.2a y 3.4 | `always` | llega `CSI 13;2u` | llega `CSI 13;2u` |
| 3.5 y 3.7c | `on` | llega `CSI 27;2;13~` (formato `xterm`, el de por defecto) o `CSI 13;2u` (`csi-u`) | llega **Intro** (`\r`): envía |
| 3.5 y 3.7c | `always` | igual que arriba | llega en el formato elegido, también a quien no lo pidió |

Otros hallazgos de la prueba, ya aplicados en el script y en el emulador:

1. Lo que tmux le pide a MAGI: `CSI > 4 ; 1 m` (3.2a y 3.4, nivel 1) o `CSI > 4 ; 2 m` (3.5 y posteriores, nivel 2). Solo lo pide si el terminal tiene la característica `extkeys` (`terminal-features xterm*:extkeys`, porque tmux no reconoce el XTVERSION «MAGI») y solo **después** de recibir las respuestas DA del terminal exterior. MAGI debe entender los dos niveles.
2. En 3.5 el valor por defecto de `extended-keys-format` es `xterm`, no `csi-u`: el script lo fija a `csi-u` (como dice el informe) para que la aplicación reciba lo mismo en todas las versiones desde la 3.5.
3. Con `always`, tmux fija el modo extendido de cada pantalla **al crearla**: la configuración debe aplicarse antes de crear la sesión (`start-server \; set … \; new-session`). Una primera pasada de la prueba, con el `set` después de crear, dio un falso «`always` no entrega nada».
4. Solo en tmux 3.5, `always` manda Mayús+Tab como `CSI 9;2u` a quien no pidió nada (en 3.2a, 3.4 y 3.7c sigue siendo `CSI Z`).

**D338, decisión provisional.** El script lleva `extended-keys on` en una constante única, `EXTENDED_KEYS`, al principio de `terminal_tmux.sh`. Sin un Claude Code real no se puede saber si pide modifyOtherKeys (entonces `on` basta) o solo kitty (entonces con `on` Mayús+Intro **envía** en 3.5 o posterior y se pierde en 3.4). Si en el paso 7 del guion Mayús+Intro envía, hay que cambiar la constante a `always` (una línea). Una forma de saber qué pide Claude Code sin enviar nada: arrancarlo dentro de un tmux de prueba y volcar las secuencias que escribe al arrancar (`CSI > 4 ; 2 m`, `CSI > 1 u`, `CSI ? u`).

**Contrato compilable (commit `68ab095`).** Migración 11 (solo SQL), `Host.sesiones_persistentes`, `Tabla::SesionesPersistentes` con su lápida, `FORMATO_4` y `FORMATO_MAXIMO = 4`, el campo `sesiones_persistentes` del `DatosHost` que viaja (con `skip_serializing_if`, para que el JSON canónico de un host sin la marca no cambie) y el módulo `sesiones_persistentes` con los tipos compartidos.

**Cuatro pistas en paralelo, cada una con su revisor adversarial** (workflow de 8 agentes, ~52 min):

- **A — Script de tmux.** `terminal_tmux.sh` reescrito con `comprobar`, `crear`, `enganchar`, `estado`, `vista`, `historial`, `renombrar` y `terminar` sobre el servidor propio (`tmux -L magi -f /dev/null`), con la configuración de MAGI aplicada antes de crear y de nuevo al enganchar, `=<nombre>` en toda orden con `-t` y la existencia comprobada por la lista exacta (el fallo R146 de la Fase 15 queda corregido también en el código heredado). `terminal/tmux.rs` gana `OrdenTmux` y la lectura de versiones con letra, `next-` y `master`. `sesiones_persistentes/lectura.rs` lee `estado`, `crear`, `renombrar` y `terminar` con todos los topes y sin pánico. El tmux falso se amplió y hay pruebas con tmux real (`tmux_real`) contra 3.2a, 3.4, 3.5 y 3.7c.
- **B — Almacén y núcleo.** Migración 11 con el paso en Rust (los hosts con `tmux_persistente = 1` pasan a `sesiones_persistentes = 1` por el camino que sube la versión, con aviso una vez); CRUD de fichas con uuid, versión y vector siempre rellenos y lápida al borrar (también cuando cae un host); validadores, nombres automáticos y estado derivado; los cuatro tipos de REGISTRO; la sección `[persistentes]` de `config.toml`.
- **C — Sincronización y Android.** Elemento `sesion_persistente`, `formato` 4 solo cuando toca, unión por (host, nombre) con la ficha de `uuid` menor, lápida de la perdedora y conflicto `nombre`, `docs/formato-sincronizacion.md` al día, y la app Android lista para leer el formato 4 (fixture, prueba Kotlin y línea en su CHANGELOG).
- **D — Teclado.** `modifyOtherKeys` (niveles 1 y 2) en el emulador del núcleo, por pestaña; Mayús+Intro → `CSI 27;2;13~` y Ctrl+Intro → `CSI 27;5;13~`; el volcado de `Adjuntar` lo reinyecta; `tui_prueba` lo pide con `--modify-other-keys`.

**Lo que encontraron los revisores y quedó corregido** (con prueba de regresión cuando era pérdida de datos):

- Pista A, tres defectos reales del script que el tmux falso ocultaba: con un entorno sin UTF-8 (`LANG` sin definir, lo normal en un canal `exec` de ssh) tmux cambia el tabulador y el 0x1F de `list-sessions -F` por `_`, así que `estado` no devolvía ninguna sesión y **toda ficha habría pasado a «terminada»**; un comando acabado en `;` (por ejemplo `find … -exec … \;`) perdía el `;` porque tmux lo toma como fin de orden; y tmux expande formatos en `-c` (`#D`, `##`, `#(orden)`), de modo que una carpeta con `#` arrancaba en otro sitio y `#(orden)` ejecutaba una orden en el host. También `print` de un entero mayor que 2^31 sale en notación científica con mawk 1.3.3.
- Pista B, la migración 11 podía dejar la base sin abrir si una fila de host con `tmux_persistente = 1` estaba corrupta (ahora esa migración no depende de la validación completa de la fila).
- Pista C, pérdida de dato: una ficha local se quedaba para siempre con el nombre temporal `__magi_sync__<uuid>` si su host llegaba ilegible en el fichero.
- Pista D, el volcado de `Adjuntar` no apagaba `modifyOtherKeys` en una ventana que ya lo tenía activo.

**Estado de las comprobaciones del sprint:** `cargo fmt --all --check` limpio, `cargo clippy --workspace --all-targets --locked -- -D warnings` limpio, `shellcheck` limpio, `cargo deny check` limpio, y `TZ=UTC cargo test --workspace --locked --no-fail-fast`: **2115 pruebas pasan, 0 fallan, 6 ignoradas** (los generadores de fixtures, que se ejecutan con `--ignored`).

### Sprint 2 — Servidor y protocolo v10

Se hizo con tres pistas en paralelo sobre el contrato `1d26f87` (con un revisor adversarial por pista) y una integración posterior. **36 de 67 ítems del checklist** quedan marcados al cerrarlo.

**Pestaña persistente (servidor).** `AbrirSesion` con `tmux{servidor, nombre, crear, compartir}` valida, comprueba (`comprobar`), crea (`crear`) y engancha (`enganchar`). El enganche va por `exec` en el canal con PTY: el script ya no se escribe en el shell (resuelto el punto 1 de «Para el Sprint 2» del Sprint 1; ver el límite de los hosts con Dropbear en §6). El snippet al conectar y el comando inicial solo se escriben en una sesión nueva sin comando propio. Si `enganchar` sale con 100, la pestaña pasa a `soltada` con su última pantalla y se difunde `Sesiones{lista}` antes de leer con `estado` quién la tiene. `Reconectar{compartir}` vuelve a engancharse. La reconexión automática sale del estado vivo de la pestaña (`tmux_vivo`) y nunca le quita la sesión a otro equipo; antes salía de `datos.tmux`, que el bucle automático no actualizaba (arreglo del fallo heredado de la Fase 15, con test). `ListarTmux` devuelve las sesiones de los dos servidores y `TerminarTmux` va con servidor y `=nombre` exacto, con la anotación `tmux_terminada` solo en el servidor y tras la barrera. `CapturarHistorial` de una pestaña con tmux lee `historial` (5 s) y lo manda en tramos. Se retiró el legado de la Fase 15 (`abrir`, `listar`, las formas de un argumento de `comprobar` y `terminar`, `nombre_nuevo`, `SesionTmux`, `Listado` y `leer_listado`).

**Vigilante, consultas, vista previa y renombrar** (`src/servidor/sesiones_persistentes.rs`, nuevo). Solo trabaja con alguna ventana conectada y comprueba con `estado` los hosts `ssh` con fichas o marcados, 8 a la vez, 10 s por host, sin diálogos (`NoInteractiva`): un host que pediría huella, frase o contraseña queda con `ErrorComprobacion::NecesitaDialogo` y sus fichas `inalcanzable`, nunca `terminada`. Todo el estado vive en memoria. Difunde `SesionesPersistentes{lista}` solo si algo cambia (sin contar `comprobada_en` ni `ahora`), y una ventana nueva la recibe tras la `Bienvenida`. `ConsultarSesionesPersistentes` agrupa las peticiones seguidas y no repite un host comprobado hace menos de 5 s. `PedirVistaPrevia` y `RenombrarTmux` responden solo a la ventana que pregunta; tras un renombrado con `Ok` se renombra en memoria, se fuerza una recomprobación del host y cambia el nombre de las pestañas enganchadas. Nada de contenido, títulos ni historial llega al log ni a REGISTRO (la prueba de no-fuga cubre salida, título, historial, carpeta, comando y vista previa). El servidor lee y valida las filas de SESIONES_PERSISTENTES y HOSTS (`almacen::sesiones_persistentes::leer_para_vigilante`) y nunca escribe en ellas.

**Arreglos del cliente heredados de la Fase 15.** `reconciliar_sesiones` copia ahora `reconexion`, `titulo`, `tmux` y `nombre` a las pestañas que ya existen, y el aviso se decide con la hora (uno ya visto no vuelve con la siguiente difusión); `PestanaUI.tmux` pasa a `Option<TmuxPestana>`; el cliente deja de anotar `tmux_terminada`.

**Revisión adversarial por pista.** Los revisores encontraron y se corrigieron, con prueba de regresión: (a) `estado` y `comprobar` ignoraban el código de salida del script y una lectura cortada se leía como «la sesión ya no existe» (cerraba pestañas vivas) o como «sin sesiones» (todas las fichas «terminadas» y la tecla `C` las habría borrado); (b) el vigilante hacía lo mismo y una consulta se quedaba sin respuesta si fallaba la lectura de SQLite; (c) una comprobación en vuelo anterior a un renombrado pisaba el cambio; (d) hosts pedidos a mano con ids falsos ocupaban plazas durante 10 min; (e) el cliente descartaba `AperturaTmuxFallida` y dejaba el host en `aperturas_pendientes`; la lista de la Fase 15 mezclaba las sesiones de los dos servidores; una terminación tardía borraba la fila de otro host; (f) `Sesion.soltada` no se limpiaba al reconectar y un `exec` rechazado por el host dejaba un canal ocupado.

**Integración.** Tras las pistas se corrigieron además los pendientes que podían perder datos: el script `estado` salía con 0 y una lista vacía si `list-sessions` fallaba por otra causa que «no hay servidor» (por ejemplo, `protocol version mismatch` tras un `apt upgrade tmux` con sesiones vivas), y `vista` e `historial` daban éxito vacío si la captura fallaba (la salida era la de `awk` o `sed`); ahora `estado` sale con 77 y las capturas con 75. Un canal de tmux cerrado sin código de salida ya no cierra la pestaña como «terminada»: es una caída y la reconexión mira el host. Si no se pueden leer las fichas de un host marcado, la pestaña se abre normal con aviso en lugar de elegir un nombre a ciegas. Las pruebas del vigilante pasan a colgar de `sesiones_persistentes_servidor` (un solo objetivo).

**Verificación.** `cargo fmt --all --check`, `clippy --workspace --all-targets --locked -- -D warnings`, `shellcheck`, `cargo deny check` y `TZ=UTC cargo test --workspace --locked --no-fail-fast`: **2179 pasan, 0 fallan, 6 ignoradas** (70 objetivos). `tmux_real` (13 pruebas) pasa con tmux 3.2a, 3.4, 3.5 y 3.7c.

### Sprint 3 — Cliente, CLI y cierre

Se hizo con seis pistas en paralelo sobre el contrato `660b2d0` y `c7e8d5b`, cada una con su revisor adversarial, y después una **ronda de correcciones** (tres pistas con revisor) con lo que los revisores dejaron abierto. **65 de 67 ítems del checklist** quedan marcados; los dos que no, en el aviso de la cabecera.

- **A1 — Bloque «Persistentes» de la vista Sesiones.** Fichas de todos los hosts con marca, estado, nombre, host, quién la tiene, actividad, atención, título y «antigua»; bajo cada host comprobado, sus sesiones sin ficha y las del tmux de siempre («de tu tmux»); `Tab` pasa a «Pestañas» y vuelve; modo estrecho por debajo de 72 columnas. Teclas `↵ c v n e , T x a C r /` con confirmaciones que nombran la sesión y el host (terminar, borrar la ficha y borrar las terminadas llevan fijados host, nombre y `uuid`). Vista previa con la hora de lectura. Consultas `ConsultarSesionesPersistentes` al abrir la lista, con `r`, con `t` y cada `refresco_seg`. Una ventana nueva con la lista vieja dice «comprobadas hace X» con la edad real. Entradas de paleta «nueva sesión persistente» y «engancharse a <nombre> · <host>».
- **A2 — Diálogo «Nueva sesión persistente».** Host `ssh`, nombre (propone el de la carpeta o `sesion-<n>`), carpeta, comando, comando de reanudación y nota, validado con `validar_datos` del núcleo. Desde Hosts y Flota con `N`, la paleta y `prefijo N`. **La ficha se guarda y `sesion_persistente_creada` se anota solo cuando la pestaña ya existe en el host**; si algo falla antes, no queda ficha. Nombre ocupado: avisa y ofrece engancharse (o reabrirla, si la ficha está terminada) o cambiar el nombre, con lo escrito conservado. Reabrir una terminada usa `comando_reanudar` (o `comando`) en su carpeta y anota `sesion_persistente_reabierta`. Host marcado: la ficha `origen = host` de la pestaña nueva.
- **A3 — Ficha de host.** Bloque SESIONES («Todas las sesiones persistentes», que viaja) con el aviso de tmux dentro de tmux; el bloque TERMINAL conserva solo «Reconectar» y «Portapapeles remoto» (propios del equipo). El cliente ya no escribe `tmux_persistente`.
- **B — Pestaña, barra e historial.** Marca `◆` (`P` en ASCII) en pestañas y barra de la pestaña («persistente · <nombre> · tmux <versión>»); pestaña **soltada** con su recuadro y teclas `↵ c x`; aviso de atención en la barra global desde cualquier vista («◆ agente-api pide atención · web-01», «◆ N sesiones piden atención»); `prefijo [` con el historial de tmux; `prefijo N`; errores de apertura con instrucción por causa; ayuda (`?`) con las teclas nuevas y la recomendación de la campana.
- **C — CLI, recursos y publicación.** `magi sesion <nombre> [--host <host>]` y `magi sesiones [--json]` (autolanza el servidor, sin diálogos, y no toma por respuesta la lista que el servidor ya tenía); `assets/` regenerado; `tmux` en el `ci.yml` (`actionlint` limpio); README («Sesiones persistentes» y los pasajes que decían que tmux no viaja), CHANGELOG, `docs/publicar.md`.
- **D — Extremo a extremo con tmux real.** `tests/sesiones_persistentes_e2e.rs` (servidor SSH de pruebas y tmux real): crear, cerrar la ventana y volver desde otra, otro equipo suelta la pestaña, campana y atención, terminar con `magi-a-1` y `magi-a-10`, un host que cae, reconexión automática que vuelve a la sesión que nadie usa y que no se la quita a otro equipo, y Mayús+Intro con `modifyOtherKeys` dentro del tmux de MAGI. Pasa con tmux 3.2a, 3.4, 3.5 y 3.7c. `tui_prueba` ampliado (`TECLAU`, `INTRO`, `ENTRADA`).

**Lo que encontraron los revisores y quedó corregido** (con prueba de regresión que falla sin la corrección):

- **Pérdida o falsificación de datos.** (A2) Un plazo de 60 s para la creación pendiente caducaba mientras el usuario contestaba el diálogo de huella del host: la sesión quedaba creada sin ficha y se perdía lo escrito (carpeta, comando de reanudación, nota). (A2) Con dos ventanas del mismo equipo y un host marcado, la que no pidió la sesión guardaba antes una ficha automática vacía y la que sí la pidió salía sin volcar lo escrito; el revisor de la ronda de correcciones encontró además que la corrección de ese caso perdía lo escrito si la carrera caía justo en el `INSERT`. (A2) En un host marcado reaparecía una ficha borrada (y su lápida sincronizada) con cualquier pestaña antigua que el servidor difundiera. (A1) Con la selección sin identidad, un refresco con una ficha nueva por delante cambiaba la fila bajo `↵` y habría enganchado otra sesión, soltando a quien la tuviera. (A1) `a` guardaba una ficha (que viaja a los demás equipos) a partir de una lista vieja de una ventana nueva; `C` se niega ahora con una comprobación de más de 120 s.
- **La soltada.** (B) Las teclas de la pestaña soltada no miraban los modificadores: Ctrl+C (el reflejo ante una pestaña congelada) volvía compartiendo y Mayús+Intro o Ctrl+Intro volvían soltando al otro equipo, quitándole la sesión sin querer. Ahora solo valen `↵`, `c` y `x` sin modificadores. (D) **La pestaña soltada no conservaba la última pantalla con tmux real** (con el tmux falso no se veía, y el ítem estaba marcado sin prueba real): el cliente de tmux escribía «[detached …]» en la pantalla principal; ahora el servidor lee la vista de la sesión con `vista` y la pinta en el emulador de la pestaña.
- **Servidor.** (D) Carrera de saludo: una difusión de lista completa (otra ventana que se cierra, el vigilante) podía llegar antes que la `Bienvenida` y el cliente fallaba con «respuesta inesperada al saludo» (2 veces en ~120 ejecuciones); ahora la `Bienvenida` sale dentro del mismo bloqueo que registra al cliente.
- **REGISTRO.** `sesion_persistente_enganchada` **no se anotaba nunca con el servidor real**: la pestaña nace con `tmux: None` y lo rellena una lista después, y las pruebas ponían el tmux desde la primera lista. Ahora se anota una sola vez cuando la pestaña pasa a tener su tmux, nunca en reconexiones; los dos mecanismos de enganche pendiente (A1 y A2) se unificaron en `persistentes.creaciones`. (C) `magi sesion` no dejaba rastro en REGISTRO.
- **Honestidad de la interfaz.** Al caer el servidor la lista conservaba filas actuables; un enganche rechazado esperaba 120 s y lo consumía otra pestaña; una vista previa en vuelo se quedaba «leyendo…» para siempre al reconectar; el pie de una vista previa con error decía «últimas 20 líneas»; el aviso de tmux dentro de tmux no se veía en el mínimo de la ficha (50×14) y daba falsos positivos con `tmux new -d`; el aviso de atención se medía en caracteres y no en columnas; `magi sesiones` inventaba `actividad` cuando el host no daba su hora y avisaba de hosts con información anterior a la consulta; la pestaña soltada se pintaba como caída en la lista de «Pestañas»; `prefijo [` decía «este programa tiene su propio historial» en una persistente sin conexión.
- **Limpieza.** Retirado el legado de la lista de tmux de la Fase 15 (`EstadoTmux`, `SesionTmuxLista`, `App.tmux`, `PeticionTerminal::TmuxLista`, `tecla_tmux`, `AccionDialogo::TerminarTmux`…); no queda ningún `CONTRATO-S3`.

**Verificación.** `cargo fmt --all --check`, `clippy --workspace --all-targets --locked -- -D warnings`, `shellcheck`, `actionlint` y `cargo deny check` limpios. `TZ=UTC cargo test --workspace --locked --no-fail-fast`: **2430 pasan, 0 fallan, 6 ignoradas** (73 objetivos). El e2e con tmux real pasa con 3.2a, 3.4, 3.5 y 3.7c.

---

## 2. Desviaciones respecto a la especificación (qué y por qué)

**Antes del Sprint 1**

- **CLAUDE.md.** La versión nueva entregada con la fase (`docs/fase16/CLAUDE.md`) partía de una base anterior a las correcciones posteriores a la Fase 15 y borraba tres reglas que están en `main`: la excepción RUSTSEC-2026-0495 de russh, el `rust-embed` con `debug-embed` (pánico de `age` en Android) y la difusión de Flota con `emitir_flota`. Se restauraron en el `CLAUDE.md` de la raíz (decisión del desarrollador); `docs/fase16/CLAUDE.md` se versionó tal cual llegó y por tanto ya difiere de la raíz. `AGENTS.md`, que también está versionado, sigue desfasado respecto a ambos.
- **Prueba de concepto sin Claude Code real** (decisión del desarrollador): D338 queda provisional, ver §1 y §6.

**Script de tmux (pista A)**

- Separador entre tmux y `awk`: **tabulador**, no 0x1F. tmux 3.4 y 3.5 escriben los caracteres de control de `list-sessions -F` como `\037` literal (3.2a y 3.7c no), de modo que `-F` con 0x1F deja el estado ilegible justo en las versiones de Ubuntu 24.04 y Debian 13. La salida `MAGI_SES` del script sí lleva 0x1F entre campos, como manda el informe. Por lo mismo se corrigió el `listar` heredado de la Fase 15, que estaba roto con 3.4 y 3.5.
- `@magi_equipo` solo se pone en el servidor propio. En `enganchar habitual` no se toca ninguna opción (CLAUDE.md: nunca cambies opciones del tmux habitual), así que las sesiones del habitual salen siempre con equipo desconocido («?»).
- `set-option -t` y `capture-pane -t` llevan `=<nombre>:` (con dos puntos): medido en 3.2a, 3.4, 3.5 y 3.7c, con `=<nombre>` a secas tmux dice `no such session` / `can't find pane`. Attach, kill y rename llevan `=<nombre>` como pide el informe.
- Las `set` llevan `-q` además de la condición de versión; `window-size latest` solo desde 3.1; en 3.0 y 3.1, `terminal-overrides` con `Tc` solo al crear (añadirlo en cada enganche lo duplicaría); `history-limit` solo al crear (`enganchar` no recibe las líneas); `master` y `openbsd-*` cuentan como versión 99.0.
- Salidas añadidas a las de §5.1: `crear` imprime `MAGI_TMUX=<versión>|no` antes de `MAGI_RESULTADO` y admite `MAGI_RESULTADO=error` (también `renombrar`); `vista` e `historial` salen sin cabeceras y con 74, 75 o 76; `enganchar` sale además con 74 y 76, y con el código de tmux si el cliente no puede engancharse y la sesión sigue viva (eso no es «soltada»).
- El script hace `unset TMUX` al principio: con `TMUX` puesto (un shell que arranca tmux por su cuenta, R143) tmux se niega a engancharse y manda las órdenes al socket de ese tmux.
- `estado` no saca las sesiones cuyo nombre no cumple `^[A-Za-z0-9_-]{1,64}$` (por ejemplo, del tmux habitual con espacios): MAGI no podría engancharse ni terminarlas, porque toda orden valida el nombre. El `listar` de la Fase 15 sí las enseñaba. También descarta duplicados.
- Títulos: además de los caracteres de control se quitan los de formato invisibles (marcas de sentido, ancho cero, BOM).
- API de Rust: las órdenes nuevas son `OrdenTmux` (con `argumentos()`, `plazo()` y `linea_para_pty()`), no variantes nuevas de `Suborden`, porque `Comprobar` y `Terminar` ya significaban la forma de un argumento. El legado de la Fase 15 (`abrir`, `listar`, `comprobar` y `terminar` con un argumento) **se queda dentro del script y de `terminal/tmux.rs`, marcado `LEGADO-S2`**, porque `src/servidor/` lo usa hasta el Sprint 2; el informe (§5.1) dice que `abrir` y `listar` «desaparecen», y eso queda hecho al cerrar el Sprint 2.
- El tmux falso y su README viven en `tests/fijos/terminal/` de la raíz (los comparten `terminal_servidor` y `terminal_e2e`), no en `crates/magi-nucleo/tests/fijos/terminal/` como decía el encargo.

**Almacén y núcleo (pista B)**

- Un host que no es `ssh` con `tmux_persistente = 1` (no debería existir) no recibe la marca nueva: la migración solo le apaga la antigua y no lo cuenta en el aviso (§3.8 prohíbe encender la marca en un host que no es `ssh`).
- `actualizar` de una ficha edita nombre, carpeta, comando, comando de reanudación y nota; ignora `origen` y `creada_por` (metadatos de creación). Para las fichas no se usa `subir_si_cambio` sino la comparación antes y después y `versiones::subir`.
- `estado_visto` devuelve `sin_comprobar` si la comprobación es de otro host (salvaguarda que §7.3 no recoge).
- `fijar_sesiones_persistentes` solo escribe si el valor cambia y rechaza encenderla en un host que no es `ssh` o que no existe.
- `migraciones::aplicar` devuelve un `ResultadoMigracion` en lugar de `()`, para poder avisar de los hosts migrados.
- La lectura valida cada fila con las mismas reglas que al escribir y salta con un aviso las que no pasan; los nombres de las ilegibles siguen contando como ocupados (`UNIQUE(host_id, nombre)`).

**Sincronización (pista C)**

- Con el criterio de `DatosTunel` no viajan `creada_en` ni `actualizado_en` (marcas locales); **sí viaja `creada_por`**, que el encargo no listaba.
- Unión (§7.5): queda un conflicto `nombre` por cada perdedora, con su `uuid` y sin `version_perdedora_json`; «recuperar» da un error claro y no toca nada, porque recuperarla duplicaría el nombre. Una ficha que no valida no entra en la unión (si no, una ficha rota con el `uuid` menor se llevaría por delante a una buena).
- `aplicar_renombrado` busca ahora el id en la base de datos cuando no está en el mapa `ids`, que se calcula antes de escribir túneles, sincronizaciones, comprobaciones y fichas; hacía falta para la edición de la ganadora de una unión. Esa edición se cuenta en `ResumenAplicacion.renombrados`.
- `PlanFusion` gana el campo `unidas` (tipo `UnionFicha`).
- La validación de una ficha importada delega en los validadores de la pista B, así que hereda su regla de carpeta absoluta o `~/…`.
- Un equipo anterior al formato 4 que edite después un host marcado reescribe su versión sin el campo y, al importarla, la marca queda a 0. Es inherente a añadir un campo sin subir el formato de lo que se lee; no hay mitigación en el código.

**Teclado (pista D)**

- **RIS** (`ESC c`) no se puede detectar: vt100 0.16 lo resuelve sin llamar a ningún callback y CLAUDE.md prohíbe buscar secuencias en los bytes. «Al reiniciarse el terminal» (§5.6) se cumple solo con DECSTR (`CSI ! p`). `CSI > m` sin parámetros tampoco apaga el modo (en xterm reinicia todo); tmux manda `CSI > 4 m`, que sí está cubierto.
- Con el modo activo, Ctrl más un carácter sin byte de control (Ctrl+0, Ctrl+ñ, Ctrl+,) se manda como `CSI 27;5;<código>~` en los dos niveles; el informe no lo precisa y hasta ahora no se enviaba nada. Alt solo con Intro, Tab, Retroceso o Esc se queda en la forma clásica; Mayús+Tab sigue siendo `CSI Z`. Si la bandera 1 de kitty está activa, gana kitty.
- `tui_prueba` está en `tests/fijos/terminal/` de la raíz (no en `crates/`); con `--modify-other-keys` pide `CSI > 4 ; n m` en lugar de empujar la bandera de kitty, e informa `TECLA27 <m> <k>`. `compilar_tui_prueba()` pasa a un `OnceLock` (dos pruebas en paralelo podían ejecutar un binario a medio enlazar).

**Fallos heredados de la Fase 15, confirmados y todavía sin corregir (van al Sprint 2)**

- `preparar_tmux` en una reconexión automática usa `datos.tmux`, que el bucle no actualiza tras `marcar_abierta`: en un host marcado con nombre automático (`datos.tmux == None`) la reconexión automática vuelve con un shell normal y pierde la marca de tmux. Confirmado leyendo el código.
- `reconciliar_sesiones` (`src/app.rs`) no copia `titulo`, `aviso`, `reconexion` ni `tmux` a las pestañas que ya existen, solo al crearlas. Confirmado leyendo el código; son campos de los que depende la pestaña «soltada» y la atención.

**Sprint 2**

- **`AperturaTmuxFallida{host_id, nombre, causa, mensaje}` (mensaje nuevo, no está en el informe).** El servidor lo manda cuando no abre la pestaña de tmux o cuando una reconexión o un `enganchar` ya abierto fallan con 75, 74 o 76. `causa` es `sin_tmux`, `tmux_antiguo`, `existe`, `sin_carpeta`, `conexion` u `otra`. Hace falta para que el cliente muestre «instala tmux en el host», «tmux demasiado antiguo» o «ya existe» con su instrucción (checklist: «errores con instrucción») y libere la apertura pendiente; sin él, el cliente solo vería un fallo genérico. Va sin `peticion_id`: el cliente correlaciona por host y nombre. **Propuesta:** añadir la causa `no_existe` («la sesión ya no existe» al engancharse sin crear sale hoy como `otra` con ese mensaje) para que el cliente marque la ficha como terminada al momento.
- **Los datos no válidos de `AbrirSesion` (nombre, carpeta o comando) ya no dan `Error{peticion_id: None}`** como en la Fase 15: dan `AperturaTmuxFallida` con causa `otra`, para que lleguen al diálogo de la sesión.
- **`CapturarHistorial` lee el historial de tmux también en las pestañas del tmux habitual**, no solo del propio. El informe (§7.8) dice que el historial de MAGI se lee igual con `historial habitual`, y vt100 casi no guarda nada bajo tmux. Una pestaña sin tmux, o con tmux pero caída o soltada, sigue capturando del emulador como en la Fase 15.
- **En el tmux habitual `crear` se rechaza** (causa `otra`: «las sesiones nuevas solo se crean en el tmux de MAGI»), no solo se ignora; y `enganchar habitual` va con `soltar = 0`, como la Fase 15 (punto 3 de «Para el Sprint 2»).
- **Reconexión cuando la sesión ya no existe.** En el tmux propio la pestaña se cierra (como al hacer `exit`) y se avisa con `AperturaTmuxFallida`; en el habitual se mantiene lo de la Fase 15 (shell normal con aviso). La Fase 15 abría un shell normal en los dos casos.
- **Nombres automáticos de la Fase 15 (`magi-<equipo>-<n>`) retirados** (`nombre_nuevo`): los sustituye `nombres::nombre_automatico`. El nombre del host marcado lo elige el servidor y la ficha (origen `host`) la guarda el cliente cuando la pestaña queda abierta.
- **Script: salida 77 (`SALIDA_FALLO_TMUX`) en `estado`** cuando tmux falla por otra causa que «no hay servidor». No está en la tabla de salidas del informe (§7.4); cualquier salida distinta de 0 de `estado` o `comprobar` se trata como «sin respuesta». Además, `enganchar` mira si la sesión sigue viva por su identificador interno (`#{session_id}`) y no por el nombre (arreglo del punto 5 de «Para el Sprint 2»).
- **`ConsultarSesionesPersistentes` gana `host_id: Option<i64>`** (serde `default`, no se escribe si es `None`) para comprobar un host sin fichas ni marca (la lista lo conserva 10 min). Campo opcional nuevo, anotado en el comentario v10 del protocolo.
- **Vigilante con `NoInteractiva` también en la vista previa y el renombrado.** La Fase 15 usaba `Interactiva` en listar y terminar. El informe no lo concreta; pulsar `v` o `,` en una lista no debe abrir un diálogo de huella. La contraseña, si el host la pide, llega del llavero de esa ventana por `PideLlavero`.
- **`[persistentes]` gana `vigilar_ms_pruebas`** (`#[serde(skip)]`, `#[doc(hidden)]`): solo existe para acelerar las pruebas; con él puesto la regla de los 5 s se desactiva. No cambia ningún valor por defecto.
- **La lista de `SesionesPersistentes` se acota a 3 MiB** (primero acorta las sesiones por host y, si no basta, las quita) para que ningún mensaje pase del tamaño del códec. El informe no lo pide; es la regla de CLAUDE.md sobre el tamaño de los mensajes.
- **El renombrado no anota nada en REGISTRO:** el informe no define un tipo para eso (los cuatro nuevos son crear, reabrir, enganchar y borrar).
- **Primera comprobación del vigilante a los `vigilar_seg`** de conectar la primera ventana; la inmediata es `ConsultarSesionesPersistentes`.
- **Cliente (pista C), más allá de lo pedido:** `reconciliar_sesiones` copia también `nombre`; `AccionDialogo::CerrarPestanaTerminandoTmux` gana `servidor` (antes mandaba siempre `Habitual`); `App.avisos_vistos` (hora del último aviso visto por pestaña) en lugar de un campo en `PestanaUI`, porque esta se construye también en `src/app/escritorios.rs`; la lista de tmux de la Fase 15 se filtra al servidor habitual hasta que el bloque «Persistentes» (Sprint 3) enseñe el otro.
- **Doble anotación de `tmux_terminada`:** resuelta (solo el servidor, que sabe cuándo se produjo el efecto); el punto 4 de «Para el Sprint 2» (los dos fallos heredados de la Fase 15) queda corregido, con test de regresión.

**Sprint 3**

*Cliente: lista y diálogo (A1, A2)*

- **La lista de tmux de la Fase 15 desaparece** (el informe, §5.4 y §6, la sustituye por el bloque «Persistentes»): `t` en «Pestañas» y la entrada de paleta «sesiones de tmux» ya no mandan `ListarTmux`, sino `ConsultarSesionesPersistentes{host_id}` y llevan la selección a las filas de ese host. El cliente deja de usar `ListarTmux`/`SesionesTmux` (siguen en el protocolo y el servidor). `PeticionTerminal::TmuxTerminar` pierde `host_id` y `servidor`. Se reescribieron cinco pruebas de `terminal_ui.rs`.
- **Tecla `N` en Hosts y Flota** para abrir el diálogo (`n` es nuevo host en Hosts). Se añadió a `config::TECLAS_DE_FLOTA`: **un atajo `N` de `[flota.atajos]` se ignora ahora con el aviso de siempre** (cambio de configuración incompatible, con su línea en el CHANGELOG). La `N` no sale en la barra de Hosts y Flota, solo en `?`.
- **Foco inicial automático** del bloque (Persistentes si hay filas, Pestañas si no) hasta la primera tecla, para que una lista que llega tarde no cambie lo que hace `↵` bajo los dedos; `Tab` lo fija.
- **`C` (borrar las terminadas) se niega si la comprobación tiene más de 120 s** («pulsa `r` y vuelve a pulsar `C`»), y `a` aplica la misma guarda: no está en el informe; evita dar por terminada o guardar la ficha de una sesión a partir de la lista vieja de una ventana nueva.
- **`a` anota también `sesion_persistente_creada`** (origen `descubierta`), según §3.3. **`,` también renombra las sesiones «sin ficha»** (solo en el host) y, con `no_existe`, renombra solo la ficha y lo dice (§4.8); las del tmux habitual no se renombran.
- **Mientras el servidor no responde tras `T`**, la fila se enseña terminada porque el host contestó `ok`, hasta que llega una comprobación posterior: es la única excepción a «no inventes estados».
- **Columna NOMBRE de 16 columnas**; «antigua» no es columna propia, va al final de la celda del título. Los atajos del bloque en la barra tienen prioridad baja (4) para no desplazar a los de siempre.
- **`sesion_persistente_enganchada` se anota cuando la pestaña pasa a tener tmux propio** (el efecto), no al pulsar; un enganche pendiente caduca a los 60 s, o a los 6 min si el host está en un diálogo.
- **Pendientes de creación** (`CreacionPendiente`): ganan clase (`Nueva`, `Reabrir`, `Enganchar`), carpeta, comando, comando de reanudación y nota; caducan a los 60 s, o a los 6 min con la conexión del host en curso, y una pestaña tardía sigue guardando su ficha. Tras guardar una ficha se manda `ConsultarSesionesPersistentes` para que la lista la enseñe sin esperar al vigilante.
- **Host marcado: una sesión que ya existía no recibe ficha automática.** Engancharse (desde la lista, el diálogo o `magi sesion`) a una sesión sin ficha de un host marcado no crea ficha ni anota «creada» (queda «sin ficha», y `a` la añade como `descubierta`). `origen = host` es solo para pestañas nuevas que abre el host marcado, con una edad máxima de 30 s. Se descartó la heurística de «la última lista ya la vio» porque haría perder la ficha de una pestaña nueva si el vigilante difunde justo antes.
- **Un `NombreEnUso` al guardar la ficha es éxito silencioso** (otra ventana la guardó): se vuelcan en ella los datos del diálogo (`completar_ficha_automatica`) si es del mismo equipo, `origen = host` y está vacía.
- **Textos de interfaz nuevos que el analista debería revisar:** el aviso de nombre ocupado sin lista cargada («Ya hay una ficha «x» en host: puede que su sesión exista», con `↵` «abrirla»), los placeholders en gris del diálogo (`(carpeta personal)`, `(shell de acceso)`, `(el mismo comando)`, `(opcional)`) y los textos de ayuda «la ves en todos tus equipos» (inexactos si el equipo no tiene la sincronización).
- `abrir_dialogo_nueva_sesion`, `reabrir_ficha`, `pestana_persistente_abierta`, `persistentes_apertura_fallida`, `caducar_creaciones` y `abrir_sesion_persistente` pasan a `#[doc(hidden)] pub` (superficie de pruebas). `abrir_sesion_persistente` devuelve `bool`.

*Cliente: ficha, pestaña y barra (A3, B)*

- **La marca del host no va en `DatosHost`**: el cliente la escribe tras `crear_host`/`actualizar_host` con `fijar_sesiones_persistentes` (que sube la versión), para que la importación de `~/.ssh/config` no la pise. `CampoFicha::Tmux` se renombra `SesionesPersistentes` y va antes de `Reconectar` en `ORDEN_CAMPOS`; `PosicionCampo` gana `cola` para que el aviso se vea con la casilla enfocada.
- **El aviso de «tmux dentro de tmux» (R143) se detecta por el snippet «al conectar» elegido en la ficha**, no por el shell del host: MAGI no puede ver el `.bashrc` sin conectar y la operación no puede dialogar. `comando_arranca_tmux` es una heurística de lectura (falla con comillas y `sudo -u x tmux`; avisa aunque `[ -z "$TMUX" ] &&` lo proteja). El informe habla de «un shell del host que arranca tmux»: **el caso real queda sin avisar**; el analista debe decidir si basta o si se quiere una nota fija.
- La marca `◆` sustituye al punto `●` de «conectada» (`✕◆` en reconectando o caída; la soltada lleva solo `◆`, en acento y sin aspa). `'◆' => "P"` se añadió a `disposicion::texto_ascii` (un solo degradador).
- **Aviso de la barra global:** prioridad servidor incompatible > mensaje > atención > acciones de servicio > atajos; se acorta por variantes (hasta «◆ !») para no tapar los dos primeros atajos de prioridad 1 y `? más`; no sale para la sesión cuya pestaña se está mirando en la vista Sesión. Un aviso nuevo del servidor que llega a la pestaña que se mira mientras se abre o está caída («dev-01 no tiene tmux: esta pestaña no es persistente») se enseña como mensaje de la barra.
- **`prefijo [` abre el historial de tmux aunque la pantalla alternativa esté activa** (la de tmux), solo con la pestaña abierta; soltada, caída o reconectando dice qué hacer. **Cerrar con `x` una persistente** mantiene el diálogo de la Fase 15 (`n` dejarla viva, `T` terminarla también, Esc), no el «↵ cerrar» de §4.2; en la soltada añade «Ahora la usa <equipo>: terminarla se la cierra».
- Las teclas de la soltada (`↵`, `c`, `x`) solo valen sin modificadores. La `N` del modo prefijo no está en la barra del prefijo (la de 200 columnas está llena), solo en la ayuda.

*CLI (C)*

- `magi sesiones` lista **solo las fichas** (como pide el checklist), no las sesiones sin ficha ni las del tmux habitual. `equipo` del JSON es la etiqueta del equipo solo si la sesión está enganchada, y `actividad` son segundos Unix según el reloj de este equipo (`null` sin hora del host o sin actividad); `comprobada_en` es `null` si el estado es `sin_comprobar`. Sin servidor, imprime las fichas como `sin_comprobar` y sale con código 1; si el servidor responde y algún host no llega a tiempo, sale con 0 (el estado lo dice). El JSON (9 campos en orden fijo) está documentado en el README y en la cabecera de `src/sesiones_cli.rs`.
- `magi sesion` sobre una ficha terminada manda `crear: None`: el servidor dice «la sesión ya no existe» y **no la reabre** (el informe §5.5 solo pide engancharse; reabrir es el `↵` de la lista). Instala el gancho de pánico (`magi conectar` no lo hace, heredado). `abrir_al_arrancar` se extrajo a `App::abrir_lo_del_arranque` (`#[doc(hidden)] pub`) para probarlo sin TTY.

*Servidor (ronda de correcciones)*

- **La vista de la pestaña soltada es texto plano** (`capture-pane -p -J`, sin colores, con la geometría del panel en el host): recupera el texto de la última pantalla pero no los estilos ni la posición exacta del cursor. Se pide en paralelo con `estado` (`tokio::join!`), así que **el nombre de quien la tiene se difunde hasta 5 s más tarde** en un host lento (la pestaña ya consta soltada desde el primer momento). Si la captura falla o la sesión ya no existe, la pestaña queda soltada como antes.
- El volcado de una pestaña soltada se hace con el bloqueo del estado tomado y solo si la pestaña sigue en `Caida` con la misma marca `soltada.desde`, para que una captura lenta nunca pise la pantalla viva tras un `Reconectar`.

*Pruebas (D)*

- Para dar tty a `attach-session`, cada prueba del e2e pone delante del `PATH` del host un envoltorio de `tmux` en `sh` que solo mete `attach-session` dentro de `script -qefc` (el host de pruebas ejecuta los `exec` entubados y sin PTY). `Escenario::otra_ventana()` descarta la `Bienvenida`, así que el e2e conecta con su propia función (`Ven::conectar`).
- `tui_prueba` cambió su `stty` a `-icanon -echo -icrnl min 1 time 0`: con `min 0 time 5`, un PTY devolvía 0 bytes y el programa terminaba creyendo que era el fin de la entrada (solo funcionaba por tubo). `compilar_tui_prueba()` de `tests/comun` compila en `target/tmp` y coloca el binario con `rename` atómico; `terminal_e2e.rs` conserva su copia local (con la fuga en `/tmp` de antes).
- La aserción de Mayús+Intro del e2e asume tmux 3.2 o posterior; con 3.0 y 3.1 se omite con aviso. No se automatizó el caso «la aplicación pide solo kitty» (D338): sigue en el paso 7 del guion.

---

## 3. Estructura de archivos creada/modificada

**Documentación:** `CLAUDE.md` (3 reglas restauradas), `docs/fase16/` (informe, checklist, prompt y copias de entrega), `docs/formato-sincronizacion.md` (formato 4), este informe.

**Núcleo (`crates/magi-nucleo`)**
- `scripts/terminal_tmux.sh` — reescrito (≈ 780 líneas, 26 KB; ver §6: hay que cambiar cómo se entrega al host).
- `src/sesiones_persistentes/` — nuevo: `mod.rs` (tipos), `validadores.rs`, `nombres.rs`, `estado.rs`, `lectura.rs`, `union.rs`.
- `src/almacen/` — `migraciones.rs` (migración 11 y su paso en Rust), `sesiones_persistentes.rs` (nuevo), `hosts.rs`, `mod.rs`, `versiones.rs`, `lapidas.rs`.
- `src/sincronizacion/` — `formato.rs`, `elementos.rs`, `fusion.rs`, `aplicacion.rs`, `mod.rs`.
- `src/terminal/` — `tmux.rs`, `emulador.rs`, `teclas.rs`.
- `src/modelo.rs`, `src/registro.rs`, `src/lib.rs`.
- Pruebas: `tests/sesiones_persistentes.rs`, `tests/sincronizacion_sesiones.rs`, `tests/tmux_real.rs`, `tests/terminal_teclas.rs` (nuevas), `tests/tmux_script.rs`, `tests/sincronizacion_comprobaciones.rs`, `tests/sincronizacion_lector.rs`, `tests/generar_fixture_*.rs`, y `tests/fijos/terminal/teclas_clasicas.txt` (tabla de referencia de la codificación clásica).

**Escritorio (`magi-ssh`)**
- `src/config.rs` (`[persistentes]`), `src/main.rs` (aviso de la migración), `src/servidor/mod.rs` (volcado de `Adjuntar`), `src/teclas.rs`, `src/app/pegado.rs`, `src/app.rs` (literal de `Host`).
- Pruebas: `tests/almacen.rs`, `tests/terminal_e2e.rs`, `tests/password.rs`, `tests/sincronizacion_aplicacion.rs`; tmux falso y `tui_prueba` en `tests/fijos/terminal/`.

**Android:** `android/CHANGELOG.md`, `PruebaFormatoSincronizacionAndroid.kt`, `PruebaHumoNucleo.kt`, `PruebaConversiones.kt`, `PruebaFormato4SincronizacionAndroid.kt` (nueva) y la fixture `magi-ffffffff-aaaa-4bbb-8ccc-dddddddddddd.age`.

**Sprint 2**

- **Servidor (`src/servidor/`)**: `sesiones_persistentes.rs` (nuevo: vigilante, consultas, vista previa y renombrar), `sesiones.rs` (apertura, enganche, soltada, reconexión, `fin_de_canal`), `tmux.rs` (`comprobar`, `crear`, `estado`, `terminar`, `ejecutar_en_handle` por `exec`), `historial.rs` (historial de tmux por tramos) y `mod.rs` (manejadores y arranque del vigilante).
- **Protocolo y configuración**: `src/protocolo.rs` (`AperturaTmuxFallida`, `CausaAperturaTmux`, `host_id` opcional y el resto de v10), `src/config.rs` (`vigilar_ms_pruebas`).
- **Núcleo**: `scripts/terminal_tmux.sh` (legado retirado, `leer_lista`, salida 77, capturas en variable), `src/terminal/tmux.rs` (`OrdenTmux::comando_exec`, `fin_de_enganchar`), `src/almacen/sesiones_persistentes.rs` (`leer_para_vigilante`).
- **Cliente**: `src/app.rs`, `src/app/sesion.rs`, `src/ui/sesion.rs` (los tres arreglos heredados y `apertura_tmux_fallida`).
- **Pruebas**: `tests/sesiones_persistentes_servidor.rs` con sus módulos `sesiones_persistentes_servidor/{pestana,vigilante}.rs` (52 pruebas; el objetivo temporal `sesiones_persistentes_vigilante.rs` se borró), `tests/terminal_servidor.rs`, `tests/terminal_ui.rs` (45), `crates/magi-nucleo/tests/{tmux_script,tmux_real}.rs` y el tmux falso `tests/fijos/terminal/tmux` con su README (`#{session_id}`, `attach-session` que simula que otro equipo suelta, `MAGI_TMUX_AL_SOLTAR_RENOMBRA`, `MAGI_TMUX_FALLA` con `list-sessions`, `list-windows` y `capture-pane`).

**Sprint 3**

- **Cliente (`src/app/`)**: `persistentes.rs` (bloque Persistentes, consultas, vista previa, acciones y enganches), `persistentes/filas.rs` (filas derivadas de la lista), `persistentes_nueva.rs` (diálogo, creaciones pendientes, fichas, reabrir), `sesion.rs` (pestaña persistente, soltada, historial de tmux, errores por causa; sin el legado de la lista de tmux) y `src/app.rs` (enganches, entradas de paleta, `abrir_lo_del_arranque`, aviso de R143).
- **Interfaz (`src/ui/`)**: `persistentes.rs`, `persistentes_nueva.rs`, `sesiones.rs`, `sesion.rs`, `ficha.rs` (bloque SESIONES), `barra.rs` (aviso global), `ayuda.rs` y `disposicion.rs` (mínimos, modo estrecho y `◆ → P`).
- **CLI y configuración**: `src/sesiones_cli.rs` (nuevo), `src/main.rs` (`Comando::Sesion` y `Comando::Sesiones`), `src/config.rs` (`N` reservada en Flota), `src/lib.rs` (`pub mod sesiones_cli`, `FECHA_DE_VERSION`), `Cargo.toml`/`Cargo.lock` (1.5.0).
- **Servidor (ronda de correcciones)**: `src/servidor/mod.rs` (`enviar_bienvenida` dentro del bloqueo), `sesiones.rs` (`pintar_ultima_pantalla`, `bytes_de_vista`) y `tmux.rs` (`vista`).
- **Núcleo**: `crates/magi-nucleo/src/almacen/sesiones_persistentes.rs` (un ajuste de la pista A1).
- **Recursos y publicación**: `assets/` regenerado (man y tres autocompletados), `.github/workflows/ci.yml` (`tmux` junto a `openssh-server`), `README.md`, `CHANGELOG.md`, `docs/publicar.md`.
- **Pruebas**: `tests/sesiones_persistentes_ui.rs` con `{lista,nueva,ficha,pestana}.rs` (190), `tests/sesiones_persistentes_e2e.rs` (9, tmux real), `tests/sesiones_persistentes_cli.rs` (13) y las unitarias de `sesiones_cli` y `persistentes::filas`; `tests/servidor.rs` (la de la `Bienvenida`), `tests/sesiones_persistentes_servidor/pestana.rs`, `tests/terminal_ui.rs`, `tests/redimensionado/vistas_b.rs`, `tests/comun/mod.rs` y `tests/fijos/terminal/{tui_prueba.rs,README.md}`. Instantáneas: 76 nuevas y 35 actualizadas a propósito (26 por el cambio visual: Sesiones, barra, paleta, ayuda de Hosts y Flota y la ficha; 9 `servidor_anterior_*` solo por la versión 1.5.0), revisadas con `git diff`. `tests/fijos/terminal/tui_prueba.rs` espera ahora la DA2 de la 1.5.0 (`10500`): hay que subirla en cada cambio de versión.

---

## 4. Decisiones técnicas tomadas durante el desarrollo

- **El contrato primero.** Antes de lanzar las pistas se fijó un contrato compilable en un commit provisional; los puntos de sincronización que había que completar llevaban la etiqueta `CONTRATO-S1-C` y no queda ninguno. Los que quedan del legado llevan `LEGADO-S2`.
- **Pistas en el mismo árbol.** El directorio `target` ocupa 333 GB y no se podía copiar por pista; las cuatro trabajaron en el mismo árbol, con ficheros propios, sin hacer commit ni `cargo fmt --all`, y con un revisor adversarial por pista que corregía lo suyo con pruebas de regresión. El commit del sprint lo hace una sola persona al final.
- **Migración 11 en dos partes.** El SQL (columna y tabla) va en la migración y la copia de `tmux_persistente` en Rust, dentro de la misma transacción que sube `user_version`, por el camino que sube la versión del host (no por SQL: no viajaría, el fallo R84). Es idempotente.
- **Fichas: viajan los datos, no las fechas.** `creada_por` sí viaja; `creada_en` y `actualizado_en` son marcas locales.
- **Unión determinista sin reloj.** `fusionar` es una función pura; la lápida de la perdedora y la edición de la ganadora se materializan en `aplicar`, como el renombrado.
- **El estado nunca dice «terminada» por no poder mirar.** `estado_visto` devuelve `inalcanzable` ante cualquier error del host y `sin_comprobar` si la comprobación no es de su host.
- **Script robusto frente al entorno del host.** Los dos fallos más graves que encontró el revisor (locale sin UTF-8 y `;` final) vienen de que un canal `exec` de ssh no es una sesión de login: la prueba con tmux real y `LANG` sin definir queda como regresión.
- **`modifyOtherKeys` como en xterm.** Estado por emulador (es decir, por pestaña); nivel 1 y 2; la tabla de teclas no cambia ni un byte con el modo apagado (una prueba compara la tabla actual con `teclas_clasicas.txt`).

**Sprint 2**

- **Contrato compilable otra vez antes de las pistas** (`1d26f87`), con marcadores `CONTRATO-S2-*` que cada pista retiró: no queda ninguno ni `LEGADO-S2`.
- **El enganche va por `exec` en un canal con PTY.** El script (≈ 25 KB) viaja como comando del canal (`OrdenTmux::comando_exec`) y no se escribe en el shell: no deja eco ni historial. Comprobado con un `sshd` real y el `ssh` de línea de órdenes (25 016 bytes) y con el servidor de pruebas de russh; **no** con el cliente russh de MAGI contra un `sshd` real ni con Dropbear (ver §6).
- **Un fallo de lectura nunca se parece a una ausencia.** Es la regla que sale de la revisión: el código de salida del script se comprueba siempre, y ante un fallo `estado` da «sin respuesta» (pestaña caída o fichas `inalcanzable`), nunca «sin sesiones» ni «ya no existe». Por eso el script distingue «no hay servidor tmux» (lista vacía) de cualquier otro fallo.
- **Soltada sin cerrar el canal a ciegas.** El bucle de canal devuelve `FinSesion::Soltada` (código 100) y la pestaña conserva su última pantalla; `TmuxFallo{cerrar}` distingue entre cerrar la pestaña (sesión terminada, sin tmux, antiguo) y dejarla caída (no se pudo engancharse).
- **La reconexión automática parte del estado vivo** (`tmux_vivo`: servidor, nombre y si se enganchó compartiendo) y guarda `tmux_compartir` para no quitar a otro equipo que compartía.
- **El vigilante impone su plazo por fuera de `conexion_para_canal`** (`tokio::time::timeout`, 10 s por host) y escribe su propio `ejecutar_con_limite` en lugar de usar `ejecutar_en_handle` de `servidor/tmux.rs`: el límite debe cubrir también la apertura del canal, y el otro la abre con un plazo fijo fuera del plazo de la orden. Quedan dos funciones parecidas; unificarlas es posible pero se dejó para no mezclar semánticas.
- **La lista de sesiones persistentes se difunde antes de la espera de la red** cuando cambian las fichas o los hosts, y de nuevo a medida que llega cada host: un host colgado no retrasa la lista de los demás.
- **Pruebas con el tmux falso y con tmux real.** La lógica del servidor se prueba con el tmux falso y el servidor SSH de pruebas; el comportamiento del script, también con tmux real (3.2a, 3.4, 3.5 y 3.7c).

**Sprint 3**

- **Seis pistas y, después, una ronda de correcciones.** Los revisores de cada pista corrigieron lo suyo; lo que cruzaba pistas (el enganche que no se anotaba con el servidor real, la carrera de saludo, la última pantalla de la soltada) se dejó anotado y lo corrigió una segunda ronda de tres pistas con su propio revisor, que a su vez halló un defecto en una de las correcciones. Los dos mecanismos de «enganche pendiente» (A1 y A2) se unificaron en uno.
- **El efecto, no la intención.** `sesion_persistente_creada`, `_reabierta`, `_enganchada` y `_borrada` se anotan cuando la pestaña existe con su tmux (o la ficha se ha borrado), no al pulsar; con el servidor real el tmux de una pestaña llega una lista después de la `Abriendo`, así que el gancho vive en la transición.
- **Lo que el cliente no sabe, no lo afirma.** Sin lista del servidor, o con la ficha `sin_comprobar`, `inalcanzable` o `sin_tmux`, el aviso de nombre ocupado dice «puede que su sesión exista» y ofrece reabrir, que nunca pisa una sesión viva (el servidor responde `existe` y entonces se ofrece engancharse).
- **Una acción que escribe algo que viaja exige una comprobación fresca.** `a` y `C` se niegan con una comprobación de más de 120 s; la ficha de una sesión descubierta no se guarda a partir de la lista vieja de una ventana nueva.
- **Un diálogo lleva fijados los datos.** Terminar, borrar y borrar las terminadas llevan host, nombre y `uuid` desde que se abren; el diálogo de nombre ocupado conserva lo escrito.
- **El servidor pinta la última pantalla de la soltada por el emulador**, no buscando secuencias en los bytes: sale de la pantalla alternativa solo si estaba activa, borra y escribe las últimas `filas` líneas de la vista desde arriba, de modo que cae siempre en la principal y sobrevive a los rehaceres aplazados.
- **`magi sesiones` no confía en la lista que el servidor ya tenía**: una ficha cuenta solo si su `comprobada_en` es posterior a la consulta menos 5 s; lo que no llega fresco en 25 s sale `sin_comprobar`.
- **Pruebas con tmux real, sin red externa.** El e2e usa el servidor SSH de pruebas, tmux de verdad y un envoltorio con `script` para el tty; cada prueba lleva su `HOME`, su `TMUX_TMPDIR` y su `kill-server` en `Drop`, y se omite sin tmux (con `CI` puesta, falla).

---

## 5. Funcionalidades del checklist completadas (texto exacto)

**65 de 67 ítems del checklist** (18 del Sprint 1, 18 del Sprint 2 y 29 del Sprint 3). Los dos que siguen sin marcar y por qué están en §6.

## Núcleo: modelo, validación y almacén
- [x] Migración 11: `HOSTS.sesiones_persistentes` (0, viaja) y tabla SESIONES_PERSISTENTES (`uuid`, `version`, `vector`, `host_id` con CASCADE, `nombre` único por host, `carpeta`, `comando`, `comando_reanudar`, `nota`, `origen`, `creada_por`, `creada_en`, `actualizado_en`)
  - AC: Dado una `magi.db` en `user_version` 10, cuando la abre la 1.5.0, entonces queda en 11 con la tabla y la columna nuevas; una base en 12 no se abre
- [x] Validadores del núcleo: nombre `^[A-Za-z0-9_-]{1,64}$`, carpeta absoluta o `~/…` (hasta 1024 bytes, sin saltos de línea), comando y comando de reanudación (hasta 4096 bytes), nota (hasta 500 caracteres) y solo hosts `ssh`
- [x] Al abrir tras migrar, cada host con `tmux_persistente = 1` pasa a `sesiones_persistentes = 1` por el camino que sube la versión, `tmux_persistente` vuelve a 0 y se avisa una vez
  - AC: Dado un host con `tmux_persistente = 1`, cuando se abre la 1.5.0, entonces tiene `sesiones_persistentes = 1` con versión nueva y `tmux_persistente = 0`; al abrir otra vez no cambia nada
- [x] Nombre automático en un host marcado: `<etiqueta del equipo>-<n>` con el primer `n` libre entre las sesiones del host y sus fichas; el diálogo propone el nombre de la carpeta o `sesion-<n>`
- [x] Estado derivado de cada ficha (`viva_libre`, `viva_enganchada`, `terminada`, `sin_comprobar`, `inalcanzable`, `sin_tmux`), atención, título, actividad comparada con la hora del host y «antigua» (más de `antigua_dias` sin actividad)
  - AC: Dado una ficha cuya sesión no sale en `estado` de un host que responde, cuando se deriva su estado, entonces es `terminada`; si el host no responde, `inalcanzable`, nunca `terminada`
- [x] Lectura acotada de lo que devuelve el script: campos separados por 0x1F, títulos saneados de hasta 120 caracteres, 500 sesiones por host como mucho, vista previa de hasta 64 KiB e historial de hasta 8 MiB y 64 tramos por fila; nunca pánico

## Sincronización
- [x] Elemento `sesion_persistente` en `sincronizacion/elementos.rs` y campo `sesiones_persistentes` del host; `formato` 4 solo si hay alguna ficha o algún host marcado
  - AC: Dado un inventario sin fichas ni hosts marcados, cuando se exporta, entonces el fichero no pasa al formato 4; con una ficha, sí
- [x] Dos fichas del mismo host con el mismo nombre se unen: queda la de `uuid` menor, con lo que le falte de la otra; la otra se borra con lápida y queda un conflicto `nombre`
  - AC: Dado el mismo nombre en el mismo host creado en dos equipos sin conexión, cuando los dos sincronizan, entonces ambos acaban con una sola ficha, la misma
- [x] Borrar una ficha deja lápida y nunca termina la sesión de tmux del host
- [x] `docs/formato-sincronizacion.md` al día: `formato` 4, elemento `sesion_persistente`, campo del host y regla de unión
- [x] La app Android lee el `formato` 4 (guarda las fichas sin enseñarlas y no usa la marca del host), con su fixture y una línea en `android/CHANGELOG.md`
  - AC: Dado un fichero de sincronización en formato 4, cuando el móvil pasa en modo lector, entonces importa el resto del inventario y no se queda en «formato nuevo»

## tmux en el host
- [x] `terminal_tmux.sh` usa el servidor propio `tmux -L magi -f /dev/null` con la configuración de MAGI aplicada con `set` al crear y al enganchar: sin barra, sin prefijo, ratón, teclas extendidas (3.2+, `csi-u` en 3.5+), portapapeles, foco, color completo, título hacia fuera, campana y `history-limit` igual a `[terminal] historial_lineas`
  - AC: Dado un host con su propio `~/.tmux.conf`, cuando MAGI crea una sesión, entonces esa configuración no se carga y las sesiones del tmux habitual siguen igual
- [x] `crear` comprueba la carpeta y crea la sesión sin engancharse; con comando, lo ejecuta con el shell de acceso del usuario y, al acabar, escribe «[MAGI] <comando> terminó con código N» y deja un shell en la carpeta
  - AC: Dado `comando = "false"`, cuando la sesión lo ejecuta, entonces se ve «terminó con código 1» y queda un shell en la carpeta
- [x] `enganchar` suelta a los demás salvo al compartir, pone `@magi_equipo` y sale con 100 si a este cliente lo suelta otro, con 0 si la sesión terminó y con 75 si no existía
- [x] Toda orden con `-t` apunta a `=<nombre>` y la existencia se comprueba con la lista exacta, también en `terminar` y `renombrar` (T126, R146)
  - AC: Dado `magi-a-10` viva y ninguna `magi-a-1`, cuando se pide terminar `magi-a-1`, entonces da `no_existe` y `magi-a-10` sigue viva
- [x] `estado` lista las sesiones de los dos servidores, el propio y el habitual, con servidor, nombre, clientes, equipo, actividad, creada, campana, ventanas y título, más la hora del host
- [x] Subórdenes nuevas `crear`, `enganchar`, `estado`, `vista`, `historial` y `renombrar`; `comprobar` y `terminar` ganan el servidor; `abrir` y `listar` desaparecen
- [x] tmux 3.0 o posterior para el servidor propio: con uno anterior, «tmux demasiado antiguo (3.0 o posterior)» y una pestaña normal; sin tmux, «instala tmux en el host»

## Servidor
- [x] Pestaña persistente: `AbrirSesion` con `tmux` crea la sesión si hace falta (`comprobar` y `crear`) y la abre con `enganchar` en el canal con PTY; el snippet al conectar y el comando inicial (F6) solo se escriben en una sesión nueva sin comando propio
- [x] Pestaña soltada: si el script sale con 100, la pestaña pasa a `soltada` con el equipo que la tiene ahora (leído con `estado`) y conserva la última pantalla en vez de cerrarse
- [x] La reconexión automática (F15) de una pestaña persistente vuelve soltando a los demás solo si los clientes enganchados son de este equipo; si la tiene otro equipo, la pestaña queda soltada
  - AC: Dado una pestaña de sobremesa caída mientras el portátil usa la sesión, cuando sobremesa reconecta, entonces no se la quita al portátil y queda «soltada»
- [x] Vigilante: mientras haya alguna ventana conectada, cada `vigilar_seg` comprueba con `estado` los hosts con fichas o marcados, por el pool o con una conexión efímera sin diálogos, con 8 hosts a la vez como mucho y 10 s de plazo por host
  - AC: Dado un host que pide frase, cuando el vigilante lo comprueba, entonces no abre ningún diálogo y sus fichas quedan «sin comprobar: conéctate a mano»
- [x] `SesionesPersistentes{lista}` a todas las ventanas en cada cambio, con el estado visto por ficha y, por host comprobado, sus sesiones sin ficha y las del tmux habitual
- [x] `CapturarHistorial` en una pestaña persistente lee el historial de tmux con `historial` y lo manda en tramos como en la F15
- [x] `PedirVistaPrevia` devuelve las últimas `vista_previa_lineas` en texto plano solo a la ventana que la pide, sin guardarlas
- [x] El servidor lee y valida las filas de SESIONES_PERSISTENTES y HOSTS antes de usarlas y nunca escribe en ellas
- [x] Ni la salida de las sesiones, ni sus títulos, ni las vistas previas, ni el historial de tmux van al log ni a REGISTRO

## CLI y configuración
- [x] `[persistentes]` en `config.toml`: `vigilar_seg` (60; 30-600), `refresco_seg` (30; 10-300), `antigua_dias` (7; 1-365) y `vista_previa_lineas` (20; 5-100), acotadas con aviso al arrancar

## API: protocolo cliente ⇄ servidor (v10)
- [x] `AbrirSesion` con `tmux{servidor, nombre, crear, compartir}`
- [x] `ListarTmux` → `SesionesTmux` con `servidor`, `clientes`, `equipo`, `actividad`, `campana` y `titulo`, y `TerminarTmux` → `TmuxTerminada` con `servidor`
- [x] `ConsultarSesionesPersistentes` y `SesionesPersistentes{lista}`
- [x] `PedirVistaPrevia` → `VistaPrevia`
- [x] `RenombrarTmux` → `TmuxRenombrada`
- [x] `CapturarHistorial` → `HistorialLote` en pestañas persistentes y `Sesiones{lista}` con `InfoSesion.tmux` (`persistente` y `soltada`)

## Calidad, pruebas y publicación
- [x] Pruebas del núcleo: `sesiones_persistentes` (validadores, nombres automáticos, estado y lectura acotada) y `tmux_script` (subórdenes nuevas con el tmux falso)
- [x] `sincronizacion_sesiones`: `formato` 4 solo cuando toca, unión de fichas con el mismo nombre, lápidas y la marca del host

**Sprint 3: 29 ítems más (65 de 67 en total).** Quedan sin marcar el de `modifyOtherKeys` (necesita un Claude Code real) y la revisión adversarial de cierre (saltada a petición del desarrollador); ver §6.

## Servidor
- [x] `ConsultarSesionesPersistentes` comprueba enseguida (al abrir la lista, con `r` y cada `refresco_seg` con la lista a la vista), agrupa las peticiones seguidas y no repite un host comprobado hace menos de 5 s
- [x] `RenombrarTmux` renombra en el host y solo entonces el cliente cambia la ficha; las pestañas enganchadas siguen enganchadas
## Cliente: Persistentes en la vista Sesiones
- [x] Bloque «Persistentes» arriba en Sesiones (`prefijo l`): fichas de todos los hosts con marca, estado, nombre, host, quién la tiene, actividad, `!` de atención, título y «antigua»; `Tab` pasa a «Pestañas» y vuelve
- [x] Bajo cada host comprobado, sus sesiones sin ficha del tmux de MAGI («sin ficha», `a` las añade con `origen = descubierta`) y las del tmux habitual («de tu tmux»: se enganchan y se terminan, sin ficha)
- [x] Teclas del bloque: `↵` engancharse (en una terminada, reabrirla), `c` compartir, `v` vista previa, `n` nueva, `e` editar ficha, `,` renombrar, `T` terminar, `x` borrar ficha, `C` borrar las terminadas, `r` refrescar y `/` filtrar
- [x] Diálogo «Nueva sesión persistente» (host `ssh`, nombre, carpeta, comando, comando de reanudación y nota), también desde Hosts, Flota, la paleta y `prefijo N`
  - AC: Dado un nombre que ya existe en ese host, cuando se acepta, entonces avisa y ofrece engancharse a la existente o cambiar el nombre
- [x] El cliente guarda la ficha y anota `sesion_persistente_creada` solo cuando la sesión ya existe en el host; si algo falla antes, no queda ficha
- [x] Reabrir una terminada la crea con su `comando_reanudar` (o su `comando`) en su carpeta y anota `sesion_persistente_reabierta`
- [x] Terminar (`T`) con una confirmación que nombra la sesión y el host y dice que se cerrará lo que corre dentro; anota `tmux_terminada` y la ficha queda «terminada»
- [x] Borrar una ficha (`x`) con confirmación; si la sesión vive, avisa de que seguirá en el host sin ficha; anota `sesion_persistente_borrada`
- [x] Ficha de host: «Todas las sesiones persistentes» al crear y al editar, en el bloque SESIONES, que viaja, con el aviso de tmux dentro de tmux; sale del bloque TERMINAL de la F15
- [x] En un host marcado, toda pestaña nueva es persistente, con nombre automático y ficha (`origen = host`); sin tmux, pestaña normal con aviso; cerrarla (`prefijo x`) pregunta como en la F15
- [x] Marca `◆` (en ASCII, `P`) en las pestañas y fichas persistentes y barra de la pestaña con «persistente · <nombre> · tmux <versión>»; soltada: «Ahora la usa <equipo> · ↵ volver · c compartir · x cerrar»
- [x] Aviso en la barra global desde cualquier vista cuando una sesión pasa a pedir atención («◆ agente-api pide atención · web-01», o «◆ N sesiones piden atención»); se quita cuando deja de pedirla
  - AC: Dado Claude Code esperando permiso con la campana en una sesión libre, cuando el vigilante la comprueba, entonces el aviso sale en la barra de todas las ventanas antes de `vigilar_seg`
- [x] Vista previa (`v`): las últimas líneas con la hora de lectura y «↵ engancharse · r refrescar · Esc volver»
- [x] `prefijo [` en una pestaña persistente abre el modo historial con el historial de tmux: búsqueda, selección, copia y guardado como en la F15
- [x] Errores con instrucción: sin tmux, tmux antiguo, carpeta que no existe, host sin conexión o que pide un diálogo, y nombre ocupado
- [x] REGISTRO: `sesion_persistente_creada`, `sesion_persistente_reabierta`, `sesion_persistente_enganchada` (solo a petición del usuario, nunca en las reconexiones) y `sesion_persistente_borrada`, con host y nombre y sin comandos, títulos ni contenido
- [x] Ayuda (`?`) y paleta con las teclas y órdenes nuevas («nueva sesión persistente», «engancharse a <nombre>»), y la recomendación de usar la campana como canal de avisos de Claude Code
## CLI y configuración
- [x] `magi sesion <nombre> [--host <host>]` abre MAGI enganchado a esa sesión; si el nombre está en varios hosts, pide `--host` y dice cuáles
- [x] `magi sesiones [--json]` lista las fichas con su estado, que pregunta al servidor (lo arranca si hace falta)
- [x] `assets/` regenerado (página man y autocompletado) con las órdenes nuevas
## Interfaz: instantáneas
- [x] Instantáneas a 40×12, 80×24 y 200×60 (T45) de Sesiones con el bloque Persistentes (atención, enganchada, terminada, sin ficha y tu tmux), el diálogo de nueva sesión, la vista previa, una pestaña soltada, el aviso de la barra y la ficha con el bloque SESIONES
## API: protocolo cliente ⇄ servidor (v10)
- [x] `VERSION_PROTOCOLO = 10` y los `peticion_id` de las sesiones persistentes en el rango del terminal (2^49)
## Calidad, pruebas y publicación
- [x] `tmux_real`: con tmux de verdad (en CI, instalado con apt; se omite si no lo hay), crear, enganchar, soltar desde otro cliente (sale con 100), historial, renombrar y terminar con `magi-a-1` y `magi-a-10`
- [x] `sesiones_persistentes_servidor` (vigilante, soltada, reconexión sin quitarla a otro equipo, captura y vista previa) y `sesiones_persistentes_ui` con `App::de_prueba`
- [x] `sesiones_persistentes_e2e` con el servidor SSH de prueba y tmux real: crear, cerrar la ventana, volver desde otra, soltar, atención por la campana y terminar
- [x] `tui_prueba` ampliado: Mayús+Intro con `modifyOtherKeys` dentro del tmux de MAGI
- [x] Versión 1.5.0: `CHANGELOG.md` («Sin publicar»), `README.md` (sección «Sesiones persistentes»), `FECHA_DE_VERSION` y guion de validación manual con Claude Code real en el informe de implementación

---

## 6. Pendientes y bloqueos

**Del Sprint 1, sin cerrar**

- **Ítem «MAGI entiende `modifyOtherKeys`…» sin marcar.** Está implementado y probado en MAGI (emulador, tabla, `Adjuntar` y `terminal_e2e` sin tmux), pero su criterio de aceptación exige un Claude Code real dentro de una sesión persistente con tmux 3.5: queda para el guion de validación manual (paso 7), junto con la decisión de D338 (`on` o `always`). Tampoco está marcada la parte «`tui_prueba` ampliado… dentro del tmux de MAGI» (Sprint 3).
- **Ítem de `tmux_real` sin marcar:** hecho y pasando, pero el criterio incluye «en CI, instalado con apt»; el `ci.yml` se toca en el Sprint 3.
- **Ítem «La app Android lee el `formato` 4» (marcado):** lo cubren la prueba de Rust con la misma fixture y las pruebas JVM de Gradle (`lint testDebugUnitTest`, `BUILD SUCCESSFUL`), pero **las pruebas instrumentadas de Kotlin no se han ejecutado** (`PruebaFormato4SincronizacionAndroid`, y las aserciones 4 y 11 de `PruebaFormatoSincronizacionAndroid` y `PruebaHumoNucleo`): el AVD local `magi-api30` tiene un bloqueo de pantalla configurado y arranca con el usuario bloqueado (`Package … is not encryption aware!`), y no se tocó el AVD del desarrollador. Las ejecutará el job `humo` de CI en la PR; si se quiere antes, hay que arrancar el AVD y desbloquearlo a mano.
- tmux 3.0a y 3.1c no se han probado con binario real (la descarga de sus fuentes la bloqueó el clasificador de permisos y no se rodeó); su configuración está probada con el tmux falso y sale de la lectura del CHANGES de 3.2a. Tampoco se ha probado el script con `dash` (el `/bin/sh` de Ubuntu y de CI; aquí `/bin/sh` es bash), ni con el `awk` de macOS ni con mawk 1.3.3: la primera ejecución en CI es la comprobación real.
- `nombre_valido` del script usa un glob `[A-Za-z0-9_-]` sin `LC_ALL=C`; en un bash antiguo con locale no C el rango por colación podría admitir letras acentuadas que Rust rechaza. No se fija `LC_ALL` en todo el script porque el servidor de tmux heredaría esa variable y las sesiones del usuario perderían UTF-8.

**Para el Sprint 2 (servidor y protocolo v10) — resuelto en el Sprint 2, salvo lo que se dice en «Del Sprint 2»**

1. El script pasó de 4 KB a 26 KB y la Fase 15 lo escribe entero en el PTY de la pestaña como `exec sh -c '…'`. Con el script nuevo eso muestra 45 KB de eco con el prompt de continuación antes de que tmux tome la pantalla, y ese texto acaba en el historial. **`enganchar` debe entregarse al host de otra forma** (un `exec` por un canal aparte con el script por la entrada estándar y el PTY, o `request_exec` con el comando, valorándolo con el informe).
2. Pasar el servidor a `OrdenTmux`, `leer_estado`, `leer_comprobar`, `leer_crear`, `leer_renombrar`, `leer_terminar` y `fin_de_enganchar`, y **quitar el legado marcado `LEGADO-S2`**. `leer_terminar` y `ResultadoTerminar` deben quedarse (los usa `sesiones_persistentes::lectura`).
3. `enganchar habitual … soltar=1` ejecuta `attach -d`, que desengancha al propio terminal del usuario de su sesión del tmux habitual; el informe §7.8 dice «como antes» (la Fase 15 compartía). El servidor debe pasar `soltar = 0` por defecto al tmux habitual.
4. Los dos fallos heredados de la Fase 15 (§2).
5. Si se renombra una sesión mientras una pestaña está enganchada y otro equipo la suelta, el script sale con 0 («terminada») en lugar de 100 («soltada»), porque la existencia tras el `attach` se comprueba con el nombre antiguo.
6. Reconexión: apagar `modifyOtherKeys` (y las banderas de kitty, que ya tenían el mismo problema) al abrir el canal nuevo de una sesión reconectada que no es persistente, antes de leer su salida; hace falta un método pequeño en `PantallaEmulada`. En el cliente, `volcar()` reutiliza el emulador: si el servidor ya lo tiene apagado y la ventana lo tenía encendido, el volcado no manda un `CSI > 4 ; 0 m`.
7. `src/servidor/sesiones.rs` y el diálogo del host siguen leyendo y escribiendo `tmux_persistente`: tras la migración, un host marcado perdería sus pestañas persistentes hasta que el servidor use `sesiones_persistentes`. Va con el Sprint 2 (servidor) y el Sprint 3 (diálogo; `app.rs:5990` debe llamar a `fijar_sesiones_persistentes`).
8. Unificar las reglas duplicadas entre `terminal/tmux.rs` y `sesiones_persistentes/validadores.rs`.
9. `escribir_tuneles` y las otras escrituras de la sincronización tienen el mismo patrón de `continue` con el host sin resolver que dejaba una ficha con el nombre temporal `__magi_sync__<uuid>`; por lectura del código, sus filas locales quedarían igual. **Es de fases anteriores y no está probado**: lo corregí solo para las fichas. Conviene decidirlo.

**Del Sprint 2, sin cerrar**

- **Ítems sin marcar de la parte de servidor, porque su texto incluye algo del cliente o de CI:** «`ConsultarSesionesPersistentes` comprueba enseguida (al abrir la lista, con `r` y cada `refresco_seg`…)» (el servidor agrupa y respeta los 5 s; los disparadores son del cliente); «`RenombrarTmux` renombra en el host y solo entonces el cliente cambia la ficha» (el servidor renombra y recomprueba; el cambio de la ficha es del cliente); «`VERSION_PROTOCOLO = 10` y los `peticion_id` … (2^49)» (la versión está y la prueba la fija, pero el cliente aún no manda esos mensajes); «`tmux_real`» (hecho y pasando con cuatro versiones, falta `apt install tmux` en `ci.yml`); y «`sesiones_persistentes_servidor` … y `sesiones_persistentes_ui`» (el primero, hecho; falta el segundo). El ítem del vigilante está marcado, pero el texto «sin comprobar: conéctate a mano» de su criterio lo pinta el cliente (Sprint 3): el servidor deja `HostVisto.error = necesita_dialogo` y las fichas `inalcanzable`.
- **Hosts con Dropbear.** El script va entero (≈ 25 KB) en cada `exec` (`comprobar`, `crear`, `estado`, `historial`, `terminar` y `enganchar`). Dropbear limita la longitud del comando (la cifra, unos 9000 bytes, es de memoria del revisor y no se comprobó); la Fase 15, con un script de 4 KB, funcionaba ahí y la 16 deja de hacerlo en esos hosts. Con OpenSSH no hay problema. Hoy el error al menos se dice claro («el host no admitió ejecutar el comando de tmux»). **Hace falta decidir cómo entregar el script:** por la entrada estándar (sirve para todo salvo `enganchar`, que necesita el PTY), por un fichero temporal en el host o recortándolo por suborden. No se ha cambiado por tratarse de una decisión de diseño.
- **Carreras estrechas en la reconexión automática.** Lee `estado`, decide y luego hace `attach -d`: otro equipo puede engancharse en medio y perder la sesión, porque el script no tiene un modo atómico «solo si no hay clientes de otro equipo». Además, `compartir` se lee antes del `await` de `estado`, y `sesion_soltada` busca por el nombre del plan, de modo que tras un renombrado deja «otro equipo» sin nombre.
- **Sesión huérfana.** Si `crear` funciona pero `abrir_con_plazo_terminal` falla después, la sesión queda en el host y el cliente solo ve el fallo genérico de apertura, no `AperturaTmuxFallida`.
- **Un cliente nuevo recibe la lista que había al irse la última ventana** (hasta `vigilar_seg` sin comprobar, con avisos de atención caducados). El cliente y la CLI (`magi sesiones`) deben mirar `comprobada_en` y no tomar esa lista por la respuesta a su consulta: esperar una difusión posterior a su petición.
- **Aviso retenido en el servidor (heredado de la Fase 15).** El servidor guarda el aviso de una pestaña aunque alguna ventana la esté mirando y solo lo limpia al recibir `Foco(true)`: otra ventana que no la mira sigue enseñando el `!`. El cliente ya descarta el de la pestaña que mira; la raíz está en el servidor.
- **Plazo del vigilante.** Con una apertura interactiva esperando al usuario en el mismo host, `conexion_para_canal` espera el cerrojo hasta 30 s pero el vigilante corta a los 7 s: el host sale con `plazo` y no con `necesita_dialogo` (la ficha sale inalcanzable, nunca terminada). El plazo real por host puede llegar a ~12 s (el cierre del canal añade hasta 2 s).
- **Carga de conexiones.** Cada pasada abre una conexión nueva por host si no hay una del pool (que la conserva 30 s): con 60 s de periodo y 20 hosts, ≈ 20 conexiones por minuto. Es lo previsto en el informe (pool o conexión efímera); conviene vigilarlo en la validación manual.
- **Lectura de SQLite del vigilante.** Un error transitorio al leer un host concreto lo deja fuera de los vigilados durante una pasada (sus fichas pasan a «sin comprobar», no a «terminadas»). Y `almacen::sesiones_persistentes::listar` sigue avisando con `warn` en cada lectura de una fila ilegible, sin deduplicar (el vigilante ya no lo usa y avisa una vez por motivo).
- **Pruebas.** Las de soltada y reconexión de `pestana.rs` dependen de un `sleep` de 300 ms y de sondeos de 100 ms del tmux falso (seis ejecuciones seguidas sin intermitencias, pero es una dependencia del reloj); la del plazo del host colgado tarda ~45 s a propósito; la de no-fuga usa un suscriptor global (`OnceLock`) porque con uno de ámbito de hilo los eventos del servidor no llegaban (causa sin aclarar). La carrera «comprobación en vuelo anterior a un renombrado» solo tiene prueba unitaria de la función pura.
- **Revisión adversarial de cierre pendiente** (Sprint 3): estas revisiones por pista no la sustituyen.

**Para el Sprint 3 (cliente, CLI y cierre)**

- Del Sprint 2: el diálogo del host (`app.rs`) sigue leyendo y escribiendo `tmux_persistente` (debe pasar a `fijar_sesiones_persistentes`); el cliente debe mandar `compartir` en `Reconectar` para «`c` compartir» y enseñar `InfoSesion.tmux.soltada`; debe atender `AperturaTmuxFallida` con su instrucción y crear la ficha (origen `host`) del host marcado a partir de `InfoSesion.tmux.nombre`; y cambiar la ficha enseguida tras un `TmuxRenombrada` `Ok` y volver a consultar.
- El diálogo y la CLI deben validar con `validar_datos` (que normaliza espacios) y usar el resultado normalizado; los validadores sueltos no recortan.
- Cablear `sesion_persistente_borrada` y las demás anotaciones de REGISTRO (hoy solo existen las constantes y `registro::anotar` las admite).
- Etiqueta legible en la vista CONFLICTOS para `sesion_persistente` (hoy sale en crudo) y qué hace `r` sobre un conflicto `nombre` de unión (da un error claro).
- El aviso de la migración solo lo ve el proceso que migra: si lo hace antes el servidor o un subcomando como `magi sincronizar`, la TUI no avisa. No hay persistencia del «ya avisado».
- Cada lectura del almacén emite un `warn` por ficha ilegible sin deduplicar: con el vigilante cada 60 s una fila corrupta llenaría el log.
- Importar acepta `sesiones_persistentes: true` en un host que no es `ssh`, y `creada_por` solo se comprueba que no esté vacío; el CRUD sí lo rechaza. Solo lo produciría un escritor con errores.
- Ficha local apartada que choca con otra local ilegible con el mismo host y nombre: se queda con el nombre temporal (solo con filas locales ilegibles).
- `README.md` (~l. 1043, dice que el formato 3 es el máximo y que tmux no viaja) y `CHANGELOG.md` («Sin publicar»), `FECHA_DE_VERSION`, versión 1.5.0 y `assets/`.
- `tmux_real` deja un directorio `magi-falsos-tmux-<pid>` en el temporal en cada ejecución (heredado de `tmux_script`).

**Revisión adversarial de cierre: SALTADA a petición del desarrollador** (ítem sin marcar; no existe `docs/fase16/magi-fase16-revision-adversarial.md`). `CLAUDE.md` la declara obligatoria y manda anotar lo que queda sin revisar. **Sí hubo revisión adversarial por pista** en cada sprint: cuatro revisores en el Sprint 1, tres en el Sprint 2, seis en el Sprint 3 y tres más en la ronda de correcciones, cada uno con lectura del código real, mutaciones y pruebas repetidas; los hallazgos están en §1. **Esas revisiones no sustituyen a la de cierre**, que lee toda la fase de una vez, servidor y cliente por separado, y contrasta con el informe. Queda sin revisar de esa forma:

- **El servidor completo de la fase** (`sesiones.rs`, `tmux.rs`, `sesiones_persistentes.rs`, `historial.rs` y el despacho de `mod.rs`) como un solo conjunto: fugas de canales y de `ConexionTomada` en todas las ramas de error, esperas sin plazo, el orden de los cerrojos entre `estado` y `pantalla`, la memoria acotada de las listas y las vistas, y estado leído a destiempo entre el vigilante, la reconexión automática y el renombrado.
- **El cliente completo** (bloque Persistentes, diálogo, pestañas, CLI): anchos Unicode y tamaños mínimos en toda la interfaz (hay pánicos descartados solo en lo probado, y varias medidas cuentan caracteres y no columnas), el reparto entre los dos mecanismos de «creaciones» pendientes tras la unificación, y la combinación de varias ventanas del mismo equipo.
- **El núcleo**: inyección en el script de tmux más allá de lo probado (nombres, carpetas y comandos con comillas, `;`, `#`, saltos de línea y Unicode), la migración 11 con datos corruptos, la convergencia de la unión de fichas entre tres equipos y un fuzz del parser de `estado`, de la vista y del historial.
- **La auditoría del checklist** contra el código ítem por ítem: los 65 ítems marcados lo están porque las pruebas de su pista y los revisores de pista lo cubren, no por una comprobación aparte.

**Del Sprint 3, sin cerrar**

- **Ítem «MAGI entiende `modifyOtherKeys`…» sin marcar** (sigue igual): falta un Claude Code real dentro de una sesión persistente con tmux 3.5 (paso 7 del guion de §7) y decidir D338 (`EXTENDED_KEYS` en `on` o `always`).
- **Aviso global de atención con la lista vieja.** Una ventana nueva recibe en la `Bienvenida` la lista en caché del servidor; si no había ninguna ventana (el vigilante no corre), puede tener horas y traer `campana = true`, y el aviso sale hasta la siguiente pasada (hasta `vigilar_seg`). Filtrar en el cliente por `comprobada_en` ocultaría una campana real (el servidor no reenvía la lista si nada cambia). Arreglarlo bien toca servidor y protocolo (despertar al vigilante y forzar una difusión al conectar la primera ventana, o marcar las campanas como no comprobadas hasta la primera pasada): **decide el desarrollador.**
- **La ficha de un host marcado la crean otras ventanas del mismo equipo.** La corrección de «engancharse a una sesión que ya existía no crea ficha» solo vale en la ventana que pidió el enganche: otra ventana del mismo equipo ve la pestaña con tmux propio sin enganche pendiente y, en un host marcado sin ficha, crea una ficha `origen = host` y anota «creada». El cliente no puede distinguirlo de una pestaña nueva (`abierta_en` se renueva al engancharse o reconectar); lo correcto exige que el servidor diga si creó la sesión (un campo opcional en `TmuxPestana`, cambio de protocolo).
- **Tras un fallo de apertura no se reabre el formulario.** Con una carpeta mal escrita (`sin_carpeta`) hay que volver a teclear host, nombre, comando, reanudar y nota. No se perdió ninguna ficha; es una mejora que no está en el informe (exigiría que `apertura_tmux_fallida` pase el motivo y `persistentes_apertura_fallida` devuelva `true`).
- **El aviso de «tmux dentro de tmux» solo conoce el snippet «al conectar»**, no el shell del host (ver §2).
- **La pestaña soltada recupera el texto de su última pantalla, sin colores** (`capture-pane -p -J`), con la geometría del panel en el host: las líneas más anchas que la pestaña se parten y pueden empujar filas al historial del emulador. Mejorarlo (`capture-pane -e` y adaptar al ancho) no está en el informe. El equipo que la tiene se difunde hasta 5 s más tarde en un host lento.
- **Cerrar una pestaña persistente terminando tmux no refresca el bloque «Persistentes»**: la ficha sigue «viva» hasta el siguiente refresco (30 s con la lista a la vista). Solo `T` en la lista exige «terminada».
- **`r` en la lista de Pestañas y la entrada de paleta «reconectar» sobre una soltada** mandan `Reconectar{compartir:false}` y le quitan la sesión al otro equipo sin la confirmación ni el aviso «la suelta allí» que lleva `↵` en la vista Sesión. Es decisión del usuario (D333), pero conviene decidir si pide confirmación.
- **Otras limitaciones menores del cliente:** un segundo `↵` sobre la misma sesión mientras la pestaña sigue «Abriendo» abre una segunda pestaña (heredado); con `magi sesion X`, si otra ventana ya tiene una pestaña X, esa llega primero y consume el enganche; una ficha ilegible que ocupa el nombre da `NombreEnUso` sin mensaje; la etiqueta del equipo («aquí», «soltando a …») se calcula una vez por proceso y la regla está escrita cuatro veces (`etiqueta_equipo_para_ficha`, `nombre_de_este_equipo`, `etiqueta_de_este_equipo`, `nombre_equipo`); recortes y rellenos cuentan caracteres, no anchura de celda (CJK y emoji ensanchan la fila; ratatui recorta sin pánico); `limpiar_texto` de la vista previa no quita los caracteres de formato invisibles (marcas bidi) que sí quita el núcleo en los títulos; con el filtro `/` abierto en Persistentes, `?` abre la ayuda en vez de entrar en el filtro; la vista previa no se desplaza con `↑↓`; quien tiene la lista «de otra ventana» ve la edad de `comprobada_en` pero `Bienvenida` no invalida `persistentes.lista`; la ayuda del bloque con `/` ocupa lo que cabe.
- **Textos que afirman algo que depende de la sincronización** («y la ves en todos tus equipos», «la ficha viaja cifrada a tus otros equipos») salen aunque el equipo no tenga la sincronización configurada (es lo que dice la maqueta §6.2).
- **CLI:** `print!` entra en pánico con una tubería rota (`magi sesiones | head -1`; igual que los demás listados); con más de 16 hosts con fichas, o una ficha cuyo host el servidor no puede leer, la orden espera los 25 s completos y sale con `sin_comprobar` y código 0 (un script debe mirar el estado y no solo el código); `magi conectar` no instala el gancho de pánico (heredado); `magi sesion` y `magi sesiones` migran el almacén antes de que la TUI pueda enseñar el aviso de la migración 11.
- **Pruebas:** la del e2e de Mayús+Intro solo se ejecutó con tmux 3.7c, 3.5, 3.4 y 3.2a en esta máquina, no en CI; el `script` de util-linux, `dash` y `mawk` de los runners tampoco están probados (la primera ejecución en CI es la comprobación real); `terminal_e2e.rs` conserva su `compilar_tui_prueba` con la fuga en `/tmp`; los 14 directorios `magi-tui-prueba-<pid>` que dejó una versión anterior de las pruebas siguen en `/tmp`; faltan con tmux real la vista previa, renombrar, el historial de tmux y el tmux habitual (solo con el tmux falso y con el script), y el caso de la aplicación que pide solo kitty (D338).
- **`SesionVista.titulo` sale como el nombre del host** cuando el shell no pone título (es el `pane_title` por defecto de tmux): la lista de Persistentes enseñaría el hostname como título. Hay que decidir si se filtra cuando coincide con el nombre del host.
- **Intermitencias vistas en la suite completa** (pasan aisladas 3 de 3 y en la suite definitiva): `vistas_n::la_pestana_sin_visor_cuenta_la_gracia` (espera «se cierra en 45 s» y con la máquina cargada sale «44 s», un reloj; es de la Fase 12) y `magi-movil` `snippets_movil_e2e::un_snippet_se_ejecuta_en_dos_hosts_y_deja_salida_y_registro` (`terminada_en` aún vacío cuando la lista ya trae el resultado `Ok`; es de la Fase 14: esta fase no toca `magi-movil` ni los módulos de snippets y conexión del núcleo).
- **Las pruebas instrumentadas de Android del formato 4 siguen sin ejecutarse** (ver «Del Sprint 1»); las ejecutará el job `humo` de CI en la PR.
- **Ya resuelto de «Para el Sprint 3»:** el diálogo del host usa `fijar_sesiones_persistentes`; las cuatro anotaciones de REGISTRO están cableadas; el README (formato 4, tmux que viaja) y el CHANGELOG están al día; `ci.yml` instala tmux. **Sigue pendiente de esa lista:** la etiqueta legible de `sesion_persistente` y de los cuatro tipos en la vista CONFLICTOS/REGISTRO (salen en crudo); el aviso de la migración solo lo ve el proceso que migra; el `warn` por ficha ilegible sin deduplicar; la validación de `sesiones_persistentes: true` en un host que no es `ssh` y de `creada_por` al importar; la ficha local apartada con el nombre temporal cuando choca con otra local ilegible.

**Bloqueos:** ninguno.

---

## 7. Ejecución y pruebas

**Arrancar y migrar.** `cargo run` abre la TUI; una `magi.db` en `user_version` 10 pasa a 11 al abrirla (tabla `SESIONES_PERSISTENTES`, columna `HOSTS.sesiones_persistentes` y copia de `tmux_persistente`); una en 12 o más no se abre. Prueba aislada: `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` y `XDG_RUNTIME_DIR` (ruta corta) apuntando a un temporal.

**Antes de cada commit**
```
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny check
shellcheck scripts/*.sh crates/magi-nucleo/scripts/*.sh tests/fijos/terminal/tmux
```

**Pruebas del Sprint 1**
```
TZ=UTC cargo test --workspace --locked --no-fail-fast        # 2115 pasan, 0 fallan, 6 ignoradas
cargo test -p magi-nucleo --test sesiones_persistentes --test tmux_script --test tmux_real --test sincronizacion_sesiones --test terminal_teclas
cargo test -p magi-nucleo --lib terminal:: sesiones_persistentes::
```
`tmux_real` usa el `tmux` del `PATH` (o el directorio de `MAGI_TMUX_DIR`, para repetirlo con otras versiones) y necesita el comando `script` de util-linux para darle un tty; se omite con un aviso si falta cualquiera de los dos. Con 3.2a, 3.4, 3.5 y 3.7c pasan las 12 pruebas.

**Pruebas del Sprint 2**
```
TZ=UTC MAGI_SFTP_SERVER=/usr/lib/ssh/sftp-server cargo test --workspace --locked --no-fail-fast   # 2179 pasan, 0 fallan, 6 ignoradas
TZ=UTC cargo test -p magi-ssh --test sesiones_persistentes_servidor --test terminal_servidor --test terminal_ui --test terminal_e2e
cargo test -p magi-nucleo --test tmux_script --test tmux_real --test sesiones_persistentes
```
`tmux_real` con otras versiones: `MAGI_TMUX_DIR=<directorio con el tmux> cargo test -p magi-nucleo --test tmux_real` (pasa con 3.2a, 3.4, 3.5 y 3.7c). `sesiones_persistentes_servidor` tarda ~50 s por la prueba del host colgado.

**Android (formato 4).** En `android/`, con `ANDROID_HOME`, `ANDROID_SDK_ROOT` y `ANDROID_NDK_HOME` (NDK `30.0.16248370`) exportados: `./gradlew lint testDebugUnitTest` → `BUILD SUCCESSFUL`. Las pruebas instrumentadas (`connectedDebugAndroidTest`) no se pudieron ejecutar en local (ver §6); la lógica del lector con la misma fixture está cubierta en Rust (`la_fixture_android_del_formato_4_se_lee_en_modo_lector`).

**Regenerar la fixture del formato 4:** `cargo test -p magi-nucleo --test generar_fixture_android -- --ignored`.

**Pruebas del Sprint 3**
```
TZ=UTC MAGI_SFTP_SERVER=/usr/lib/ssh/sftp-server cargo test --workspace --locked --no-fail-fast   # 2430 pasan, 0 fallan, 6 ignoradas (73 objetivos)
TZ=UTC cargo test -p magi-ssh --test sesiones_persistentes_servidor --test sesiones_persistentes_ui --test sesiones_persistentes_e2e --test sesiones_persistentes_cli --test redimensionado --test terminal_ui --test terminal_servidor --test terminal_e2e --test servidor --test recursos
```
`sesiones_persistentes_e2e` necesita tmux y el comando `script` de util-linux; con `CI` puesta falla si faltan, y sin ella se omite con un aviso (solo se ve con `--nocapture`). Para repetirlo con otras versiones: `MAGI_TMUX_DIR=<directorio con el tmux> cargo test -p magi-ssh --test sesiones_persistentes_e2e` (pasa con 3.2a, 3.4, 3.5 y 3.7c). `sesiones_persistentes_cli` tarda unos 7 s (espera 6 s a propósito para comprobar que no toma por respuesta una lista vieja). Instantáneas: tras un cambio visual deliberado, `INSTA_UPDATE=always cargo test --test sesiones_persistentes_ui nombre_del_test` (con filtro) y revisa `git diff tests/snapshots`; con `TZ=UTC`.

**Nuevos subcomandos:** `cargo run -- sesion <nombre> [--host <host>]` y `cargo run -- sesiones [--json]`. Tras tocar la CLI: `scripts/generar-recursos.sh` y `cargo test -p magi-ssh --test recursos`.

**Guion de validación manual** (lo hace el desarrollador, con Claude Code real; es el del informe de la fase, §11, con lo que añade esta implementación entre corchetes):

1. Nueva sesión persistente en un host con tmux 3.5: nombre `agente`, la carpeta de un proyecto y el comando `claude`; trabajar un poco. [Probar también una carpeta que no existe (debe decir cuál y no dejar ficha), un nombre ya ocupado (ofrece engancharse o cambiarlo) y un host que pide la frase y se tarda más de 60 s en contestarla (la ficha debe guardarse igualmente).]
2. Cerrar MAGI del todo (también `magi servidor parar`) y volver: Sesiones la enseña viva y libre, y `↵` vuelve a la misma conversación.
3. Desde el portátil, ya sincronizado: la ficha está; `↵` la engancha y en sobremesa queda «soltada»; `↵` en sobremesa la recupera. [Comprobar que la pestaña soltada enseña el texto de su última pantalla y no «[detached …]».]
4. `c` en los dos equipos y escribir en uno: se ve en los dos.
5. Con `preferredNotifChannel = "terminal_bell"`, pedirle algo que necesite permiso y soltar la sesión: la barra avisa en menos de un minuto. [Comprobar que el aviso sale desde Hosts y desde Flota, y que se quita al engancharse.]
6. `prefijo [` en la sesión: buscar en lo que escribió Claude Code y copiarlo.
7. **Mayús+Intro dentro de la sesión con tmux 3.5 y con 3.4 (anotar si llega).** Si **envía** en lugar de insertar un salto de línea, Claude Code solo pide kitty: hay que cambiar `EXTENDED_KEYS` a `always` en `crates/magi-nucleo/scripts/terminal_tmux.sh` (una línea) y marcar entonces el ítem de `modifyOtherKeys` del checklist. Una forma de saber qué pide Claude Code sin enviar nada: arrancarlo dentro de un tmux de prueba y volcar las secuencias que escribe al arrancar (`CSI > 4 ; 2 m`, `CSI > 1 u`, `CSI ? u`).
8. Reiniciar el host: la ficha pasa a terminada; `↵` la reabre con `claude --continue`.
9. Marcar un host con «Todas las sesiones persistentes»: abrir dos pestañas (nombres automáticos), cerrar una sin terminarla y verla en la lista. [Con un host sin tmux, la pestaña es normal y avisa.]
10. Suspender el portátil 5 min con la sesión enganchada: al volver, la reconexión la recupera si nadie la tiene, o queda soltada si la tiene sobremesa.
11. `magi sesion agente` desde una terminal y `magi sesiones --json`. [`magi sesion` con un nombre en dos hosts debe pedir `--host`; es el único camino que arranca la TUI real y no se pudo probar sin TTY.]
12. El móvil sincroniza con el `formato` 4 sin quedarse en «formato nuevo».
13. [Añadido] Un host con Dropbear (si hay alguno): `comprobar` y `enganchar` deben funcionar; el script va entero en cada `exec` (≈ 25 KB) y Dropbear limita la longitud del comando (ver §6, «Hosts con Dropbear»).
14. [Añadido] Con más de una ventana de MAGI abierta, crear una sesión desde una y comprobar que la otra no duplica la ficha ni pierde lo escrito.

**Repetir la prueba de concepto de D338:** el harness (cliente tmux por pty + sonda) no está en el repositorio; el método y la matriz están descritos en §1. Para repetirlo con Claude Code real, ver el paso 7 del guion de validación (§11 del informe de la fase).
