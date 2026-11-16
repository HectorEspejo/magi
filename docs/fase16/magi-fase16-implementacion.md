# MAGI - Fase 16: Sesiones Persistentes — Informe de implementación

> Documento vivo: se actualiza en cada sesión. Estado: **Sprint 1 de 3 cerrado** (núcleo, script de tmux, sincronización y teclado). Faltan el servidor y el protocolo v10 (Sprint 2) y el cliente, la CLI y el cierre (Sprint 3).
> Rama `fase16`. Contrato del Sprint 1 en el commit `68ab095`.

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

---

## 5. Funcionalidades del checklist completadas (texto exacto)

**18 de 67 ítems del checklist** (Sprint 1). El resto se marca al cerrar los Sprints 2 y 3; los dos ítems del Sprint 1 que siguen sin marcar y por qué están en §6.

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

## CLI y configuración
- [x] `[persistentes]` en `config.toml`: `vigilar_seg` (60; 30-600), `refresco_seg` (30; 10-300), `antigua_dias` (7; 1-365) y `vista_previa_lineas` (20; 5-100), acotadas con aviso al arrancar

## Calidad, pruebas y publicación
- [x] `sincronizacion_sesiones`: `formato` 4 solo cuando toca, unión de fichas con el mismo nombre, lápidas y la marca del host

---

## 6. Pendientes y bloqueos

**Del Sprint 1, sin cerrar**

- **Ítem «MAGI entiende `modifyOtherKeys`…» sin marcar.** Está implementado y probado en MAGI (emulador, tabla, `Adjuntar` y `terminal_e2e` sin tmux), pero su criterio de aceptación exige un Claude Code real dentro de una sesión persistente con tmux 3.5: queda para el guion de validación manual (paso 7), junto con la decisión de D338 (`on` o `always`). Tampoco está marcada la parte «`tui_prueba` ampliado… dentro del tmux de MAGI» (Sprint 3).
- **Ítem de `tmux_real` sin marcar:** hecho y pasando, pero el criterio incluye «en CI, instalado con apt»; el `ci.yml` se toca en el Sprint 3.
- **Ítem «La app Android lee el `formato` 4» (marcado):** lo cubren la prueba de Rust con la misma fixture y las pruebas JVM de Gradle (`lint testDebugUnitTest`, `BUILD SUCCESSFUL`), pero **las pruebas instrumentadas de Kotlin no se han ejecutado** (`PruebaFormato4SincronizacionAndroid`, y las aserciones 4 y 11 de `PruebaFormatoSincronizacionAndroid` y `PruebaHumoNucleo`): el AVD local `magi-api30` tiene un bloqueo de pantalla configurado y arranca con el usuario bloqueado (`Package … is not encryption aware!`), y no se tocó el AVD del desarrollador. Las ejecutará el job `humo` de CI en la PR; si se quiere antes, hay que arrancar el AVD y desbloquearlo a mano.
- tmux 3.0a y 3.1c no se han probado con binario real (la descarga de sus fuentes la bloqueó el clasificador de permisos y no se rodeó); su configuración está probada con el tmux falso y sale de la lectura del CHANGES de 3.2a. Tampoco se ha probado el script con `dash` (el `/bin/sh` de Ubuntu y de CI; aquí `/bin/sh` es bash), ni con el `awk` de macOS ni con mawk 1.3.3: la primera ejecución en CI es la comprobación real.
- `nombre_valido` del script usa un glob `[A-Za-z0-9_-]` sin `LC_ALL=C`; en un bash antiguo con locale no C el rango por colación podría admitir letras acentuadas que Rust rechaza. No se fija `LC_ALL` en todo el script porque el servidor de tmux heredaría esa variable y las sesiones del usuario perderían UTF-8.

**Para el Sprint 2 (servidor y protocolo v10)**

1. El script pasó de 4 KB a 26 KB y la Fase 15 lo escribe entero en el PTY de la pestaña como `exec sh -c '…'`. Con el script nuevo eso muestra 45 KB de eco con el prompt de continuación antes de que tmux tome la pantalla, y ese texto acaba en el historial. **`enganchar` debe entregarse al host de otra forma** (un `exec` por un canal aparte con el script por la entrada estándar y el PTY, o `request_exec` con el comando, valorándolo con el informe).
2. Pasar el servidor a `OrdenTmux`, `leer_estado`, `leer_comprobar`, `leer_crear`, `leer_renombrar`, `leer_terminar` y `fin_de_enganchar`, y **quitar el legado marcado `LEGADO-S2`**. `leer_terminar` y `ResultadoTerminar` deben quedarse (los usa `sesiones_persistentes::lectura`).
3. `enganchar habitual … soltar=1` ejecuta `attach -d`, que desengancha al propio terminal del usuario de su sesión del tmux habitual; el informe §7.8 dice «como antes» (la Fase 15 compartía). El servidor debe pasar `soltar = 0` por defecto al tmux habitual.
4. Los dos fallos heredados de la Fase 15 (§2).
5. Si se renombra una sesión mientras una pestaña está enganchada y otro equipo la suelta, el script sale con 0 («terminada») en lugar de 100 («soltada»), porque la existencia tras el `attach` se comprueba con el nombre antiguo.
6. Reconexión: apagar `modifyOtherKeys` (y las banderas de kitty, que ya tenían el mismo problema) al abrir el canal nuevo de una sesión reconectada que no es persistente, antes de leer su salida; hace falta un método pequeño en `PantallaEmulada`. En el cliente, `volcar()` reutiliza el emulador: si el servidor ya lo tiene apagado y la ventana lo tenía encendido, el volcado no manda un `CSI > 4 ; 0 m`.
7. `src/servidor/sesiones.rs` y el diálogo del host siguen leyendo y escribiendo `tmux_persistente`: tras la migración, un host marcado perdería sus pestañas persistentes hasta que el servidor use `sesiones_persistentes`. Va con el Sprint 2 (servidor) y el Sprint 3 (diálogo; `app.rs:5990` debe llamar a `fijar_sesiones_persistentes`).
8. Unificar las reglas duplicadas entre `terminal/tmux.rs` y `sesiones_persistentes/validadores.rs`.
9. `escribir_tuneles` y las otras escrituras de la sincronización tienen el mismo patrón de `continue` con el host sin resolver que dejaba una ficha con el nombre temporal `__magi_sync__<uuid>`; por lectura del código, sus filas locales quedarían igual. **Es de fases anteriores y no está probado**: lo corregí solo para las fichas. Conviene decidirlo.

**Para el Sprint 3 (cliente, CLI y cierre)**

- El diálogo y la CLI deben validar con `validar_datos` (que normaliza espacios) y usar el resultado normalizado; los validadores sueltos no recortan.
- Cablear `sesion_persistente_borrada` y las demás anotaciones de REGISTRO (hoy solo existen las constantes y `registro::anotar` las admite).
- Etiqueta legible en la vista CONFLICTOS para `sesion_persistente` (hoy sale en crudo) y qué hace `r` sobre un conflicto `nombre` de unión (da un error claro).
- El aviso de la migración solo lo ve el proceso que migra: si lo hace antes el servidor o un subcomando como `magi sincronizar`, la TUI no avisa. No hay persistencia del «ya avisado».
- Cada lectura del almacén emite un `warn` por ficha ilegible sin deduplicar: con el vigilante cada 60 s una fila corrupta llenaría el log.
- Importar acepta `sesiones_persistentes: true` en un host que no es `ssh`, y `creada_por` solo se comprueba que no esté vacío; el CRUD sí lo rechaza. Solo lo produciría un escritor con errores.
- Ficha local apartada que choca con otra local ilegible con el mismo host y nombre: se queda con el nombre temporal (solo con filas locales ilegibles).
- `README.md` (~l. 1043, dice que el formato 3 es el máximo y que tmux no viaja) y `CHANGELOG.md` («Sin publicar»), `FECHA_DE_VERSION`, versión 1.5.0 y `assets/`.
- `tmux_real` deja un directorio `magi-falsos-tmux-<pid>` en el temporal en cada ejecución (heredado de `tmux_script`).

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

**Android (formato 4).** En `android/`, con `ANDROID_HOME`, `ANDROID_SDK_ROOT` y `ANDROID_NDK_HOME` (NDK `30.0.16248370`) exportados: `./gradlew lint testDebugUnitTest` → `BUILD SUCCESSFUL`. Las pruebas instrumentadas (`connectedDebugAndroidTest`) no se pudieron ejecutar en local (ver §6); la lógica del lector con la misma fixture está cubierta en Rust (`la_fixture_android_del_formato_4_se_lee_en_modo_lector`).

**Regenerar la fixture del formato 4:** `cargo test -p magi-nucleo --test generar_fixture_android -- --ignored`.

**Repetir la prueba de concepto de D338:** el harness (cliente tmux por pty + sonda) no está en el repositorio; el método y la matriz están descritos en §1. Para repetirlo con Claude Code real, ver el paso 7 del guion de validación (§11 del informe de la fase).
