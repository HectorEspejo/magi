# MAGI - Fase 10: Sincronización Cifrada — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. No se
> regenera desde cero. La especificación de la fase está en
> [magi-fase10-informe.md](magi-fase10-informe.md) y el alcance verificable en
> [magi-fase10-checklist.md](magi-fase10-checklist.md).

**Estado de la sesión:** fase cerrada: implementación completa, revisión
adversarial de cierre hecha y corregida, y suite completa (tests, clippy, fmt
y cargo-deny) en verde. Quedan solo las validaciones manuales que necesitan
llavero real, una carpeta sincronizada de verdad o dos equipos.

## Resumen de lo implementado

- **Migración 7** (`src/almacen/migraciones.rs`): añade `uuid`, `version` y
  `vector` a GRUPOS, HOSTS, IDENTIDADES, TUNELES, SNIPPETS y
  SINCRONIZACIONES_DIR con índice único por `uuid`; crea `EQUIPOS` (con la fila
  de este equipo), `LAPIDAS` y `CONFLICTOS`; rellena las filas existentes
  (identidades con UUIDv5 de la huella) y deja `PRAGMA user_version = 7`.
- **Reloj híbrido y versiones** (`src/almacen/versiones.rs`,
  `src/sincronizacion/reloj.rs`): `max(ahora, reloj + 1)`, versión
  `<ms:013>.<uuid equipo>`, vector JSON, dominancia, comparación determinista y
  máximo componente a componente.
- **Almacén instrumentado**: toda escritura de un campo que viaja sube versión y
  vector; los campos propios del equipo no; los borrados dejan lápida (incluida
  la cascada de túneles y sincronizaciones al borrar un host) y los `SET NULL`
  de clave ajena cuentan como edición de los hosts afectados.
- **Cifrado y firma** (`src/sincronizacion/cifrado.rs`): identidad X25519 de
  age, cifrado/descifrado binario, HKDF-SHA256 + HMAC-SHA256 del JSON,
  hexadecimal, huella corta de la clave y `Zeroizing` para el secreto.
- **Formato** (`src/sincronizacion/formato.rs`): cabecera
  `MAGI-SINCRONIZACION 1`, línea de firma y documento JSON con `formato`,
  `app`, `equipo`, `generado_en`, `reloj_ms`, `elementos`, `lapidas`,
  `huellas` y `equipos_quitados`.
- **Exportación canónica** (`src/sincronizacion/elementos.rs`): mapeo de cada
  tipo a los campos que viajan (con etiquetas y verificaciones dentro del host,
  destinos dentro del snippet y rutas del home como `~/…`), lista de candidatos
  vivos y lápidas.
- **Fusión** (`src/sincronizacion/fusion.rs`): dominancia por vector,
  concurrentes por versión más reciente, vector máximo, lápidas, huérfanos y
  renombrado determinista `(2)`; tests de determinismo.
- **Aplicación** (`src/sincronizacion/aplicacion.rs`): escritura transaccional
  con versiones del fichero, resolución de referencias por uuid, ETIQUETAS y
  destinos, verificaciones, lápidas, borrados aplazados y recuperación de la
  versión perdedora.
- **Pasada completa** (`src/sincronizacion/mod.rs`): lectura con hash y estados
  de equipo, descifrado y firma, detección de equipo quitado, fusión, copia de
  `magi.db`, umbral de borrados masivos, aplicación, huellas, registro,
  regeneración de `magi_config` y exportación atómica del fichero propio solo
  si cambia el contenido.
- **Carpeta y cerrojo** (`src/sincronizacion/carpeta.rs`): lectura estricta de
  `magi-<uuid>.age` (ignora temporales y copias en conflicto), escritura atómica
  600, hash SHA-256 y `flock` en `$XDG_RUNTIME_DIR/magi/sincronizacion.lock`.
- **Asistentes de equipo** (`src/sincronizacion/equipos.rs`): crear, unir
  (validando clave contra un fichero), quitar con rotación de clave, rotar y
  desactivar; clave en el llavero con la anterior vigente 30 días.
- **Configuración** (`src/config.rs`): sección `[sincronizacion]` con
  `carpeta`, `retraso_seg`, `copias`, umbrales y fecha límite de la clave
  anterior; sin `carpeta` la sincronización está desactivada.
- **Llavero** (`src/llavero.rs`): secretos genéricos por cuenta
  (`sincronizacion`, `sincronizacion-anterior`) con el mismo backend
  `secret-tool`/Keychain que las contraseñas, más una implementación en memoria
  para pruebas.
- **QR** (`src/sincronizacion/qr.rs`): módulos del QR convertidos a medios
  bloques con zona quieta y relleno de fila.
- **Registro** (`src/registro.rs`): tipos `inventario_*` y
  `huella_sincronizada`, y filtro rápido «inventario».
- **Documentación del formato**: `docs/formato-sincronizacion.md`, completo para
  la app Android (cifrado, firma, JSON, tipos de elemento, reglas de fusión).
- **Interfaz** (`src/app/sincronizacion.rs`, `src/ui/sincronizacion.rs`,
  `src/app.rs`, `src/ui/{mod,disposicion,barra,dialogos,ayuda}.rs`): vistas
  SINCRONIZACIÓN y CONFLICTOS desde la paleta, teclas del informe, diálogos
  CLAVE (texto y QR en negro sobre blanco), UNIR (campo enmascarado con Tab),
  IMPORTACIÓN CON BORRADOS (`a`/`m`/`Esc`), elegir carpeta y confirmaciones de
  peligro; indicador `⇅` con color y ASCII, entradas de paleta
  «sincronización», «sincronizar ahora» y «conflictos de sincronización»,
  mínimos/modo estrecho e instantáneas a 40×12, 80×24 y 200×60.
- **Pasada en la TUI**: al abrir, al configurar la carpeta, tras cada cambio
  local con `retraso_seg`, con `s` y al salir si quedaba una exportación
  pendiente; vigilancia con `notify` agrupada 2 s y sondeo de respaldo cada
  30 s. Las esperas (llavero, cerrojo, vigilancia) van en tareas tokio; la
  pasada la ejecuta el bucle de la UI, único escritor de SQLite, con el cerrojo
  tomado. Tras importar se recarga el inventario cacheado.
- **CLI** (`src/main.rs`): `magi sincronizar` (0/1/2 y mensajes por estado,
  cerrojo con espera de 10 s) y `magi sincronizar estado` (no descifra ni lee la
  clave); `magi diagnostico` añade la comprobación de sincronización.
- **Huellas importadas** (`src/sincronizacion/huellas.rs`): solo se añaden si
  no hay ninguna (con copia de `known_hosts` y REGISTRO
  `huella_sincronizada`), conflicto `huella` si hay otra, y acción «aceptar la
  del otro equipo» que sustituye con copia e REGISTRO `huella_sustituida`.
- **Calidad y entrega**: versión 1.1.0, `FECHA_DE_VERSION` 2026-10-31,
  `assets/` (completados y man) regenerados por el subcomando nuevo,
  `deny.toml` con dos avisos de RustSec ignorados y justificados (solo
  compilación, transitivos de `age`), y README/CHANGELOG actualizados.

### Revisión adversarial de cierre (resultado)

Hecha con revisión independiente (un revisor dedicado, solo lectura) sobre el
commit d2a9572 y el árbol de corrección, con reproducciones fuera del
repositorio. Hallazgos verificados y corregidos:

1. **ALTO — cambiar la clave no recifraba el fichero propio.** El paso de
   exportación solo miraba el hash canónico y la existencia del fichero, así
   que tras rotar/quitar/unir el fichero seguía cifrado con la clave anterior
   (la clave posiblemente comprometida seguía abriéndolo, y un equipo con la
   clave nueva no podía leerlo). **Corregido**: crear/unir/rotar/quitar olvidan
   el hash del fichero propio (`EQUIPOS.hash_ultimo_fichero` a NULL) para que
   la pasada siguiente lo reescriba con la clave nueva.
2. **MEDIO — un borrado aplazado vaciaba referencias de otros hosts sin subir
   su versión.** Al resolver las referencias solo contra los vivos del plan,
   un host que saltaba por un host aplazado (o que lo usaba como grupo/snippet)
   perdía la referencia sin subir su versión, rompiendo el invariante «mismo
   contenido, misma versión» y perdiendo configuración mientras el elemento
   seguía vivo. **Corregido**: las referencias se resuelven contra los vivos
   **más los aplazados**.

Además se corrigieron dos detalles de integración detectados en la revisión:
no se guarda en `LAPIDAS` la lápida de un borrado aplazado (evitaba que la
pasada lo re-aplazara en cada arranque con copia y REGISTRO de más) y la firma
se comprueba ahora **antes** del número de `formato`, como documenta el
formato (§13).

**Hallazgo grave posterior, también corregido:** un nombre reutilizado por un
elemento nuevo mientras el antiguo tenía el borrado aplazado chocaba con el
UNIQUE al aplicar, abortaba la importación y dejaba la sincronización
atascada (cada pasada reintentaba el mismo fichero). Ahora la aplicación
propaga el aplazamiento al padre si un hijo sigue vivo (la cascada no borra
sin lápida) y renombra como edición nueva los elementos del plan cuyo nombre
ocupa un aplazado, con conflicto `nombre`; esto cubre hosts, túneles,
sincronizaciones, grupos, identidades y snippets.

**Segunda pasada, de seguridad** (revisor independiente, con 16
reproducciones fuera del repositorio). No encontró fugas de la clave (log,
REGISTRO, `magi.db`, `config.toml`, carpeta, argv, panic ni QR). Se
corrigieron:

- **Enlaces simbólicos en la carpeta** (ALTA): la sonda de escritura, el
  temporal de `escribir_atomico` y el listado seguían enlaces y podían
  escribir/leer fuera de la carpeta. Ahora la sonda y el temporal usan
  `create_new` con nombre único y el listado/lectura usan `O_NOFOLLOW`.
- **Un equipo quitado se importaba en la misma pasada en que se descubría la
  baja** (ALTA/MEDIA): se filtran sus documentos antes de fusionar y no se
  marcan como importados.
- **La clave anterior se usaba para exportar** si faltaba la actual (MEDIA):
  sin clave actual la pasada dice `SinClave`; la anterior solo lee.
- **Quitar un equipo con un uuid no canónico** podía construir rutas fuera de
  la carpeta (MEDIA): se valida el uuid y `carpeta::escribir/borrar` exige la
  forma canónica. Los nombres de fichero solo aceptan el uuid canónico en
  minúsculas.
- **Huella de otro algoritmo añadida sin consentimiento** (MEDIA): solo se
  añade si el host no tiene NINGUNA huella; con cualquier otra se abre un
  único conflicto `huella`.
- **Escrituras de ciclo de vida sin cerrojo** (MEDIA): crear, unir, rotar,
  quitar, desactivar y reunir toman el `flock`.
- **Clave anterior huérfana** al crear/unir, purga de la caducada en la CLI,
  y no se borra la fecha si el llavero falla (MEDIA): corregido.
- **Esc en el diálogo CLAVE** tras quitar/rotar no exportaba (MEDIA): ahora
  dispara la pasada de exportación.
- **Borrar en cascada un túnel aplazado sin lápida** y **conflicto de borrado
  obsoleto que borraba un host resucitado** (MEDIA): corregidos (propagación
  de aplazados y resolución del conflicto al aplicar una edición nueva).
- **Dirección de `known_hosts` sin sanear** (BAJA), **tamaño máximo de
  fichero** (BAJA), **desbordamiento del reloj** (BAJA), **copias
  `known_hosts` en el mismo segundo** (BAJA), permisos 700 del runtime
  (BAJA) y `Debug` del llavero en memoria sin secretos (BAJA): corregidos.

Quedan como limitaciones conocidas, anotadas para la validación manual y no
bloqueantes de la fase: `magi sincronizar` y la pasada de cierre de la TUI no
pueden consultar el trabajo vivo del servidor (la CLI no tiene TUI; en la TUI
el cierre sí lo consulta ya); las actualizaciones del almacén hacen el UPDATE
y la subida de versión en dos pasos (una caída entre ambos dejaría la versión
atrás; es una ventana de durabilidad, no de la lógica de fusión); un fichero
truncado y una clave incorrecta son indistinguibles al descifrar; un elemento
con firma válida pero datos semánticamente inválidos hace fallar la pasada
entera sin aplicar nada (no a medias); y aceptar una huella sustituye todas
las claves del host, no solo el tipo en conflicto.

Cada corrección tiene test de regresión en
`tests/sincronizacion_pasada_e2e.rs`, `tests/sincronizacion_aplicacion.rs`,
`tests/sincronizacion_equipos.rs`, `tests/sincronizacion_huellas.rs` y los
unitarios de `carpeta.rs`. Áreas revisadas sin hallazgos: fusión/reloj,
serialización canónica, instrumentación del almacén, migración 7,
carpeta/cerrojo/escritura atómica y manejo de la clave. No revisadas en esas
pasadas (pendientes de validación manual): interfaz completa, CLI con llavero
real, diagnóstico, backends reales del llavero, huellas hasheadas reales y
concurrencia multi-documento más allá de la suite.

## Desviaciones respecto a la especificación (qué y por qué)

1. **`age` 0.11.5 en vez de 0.12** (decisión técnica). `age` 0.12.1 depende de
   `ml-kem` 0.2.1, que deja de compilar cuando el grafo une `kem` a 0.3.0 por
   `russh` 0.63 (`ml-kem` 0.3.2). El formato age v1 es el mismo: los ficheros
   cifrados con 0.11 se abren con 0.12 y al contrario. Se documenta aquí para
   que el analista lo sepa; no cambia el formato de sincronización.
2. **Clave de firma derivada del texto canónico de la identidad**, no de los 32
   bytes crudos X25519. `age` no expone el `StaticSecret` y decodificar el
   bech32 habría añadido `bech32` como dependencia directa fuera del stack
   aprobado. El IKM documentado es el texto `AGE-SECRET-KEY-1…` en mayúsculas.
   Es determinista y verificable desde Android; el formato lo fija en §3.
3. **Columnas `uuid`, `version` y `vector` nullables en SQLite.** `ALTER TABLE`
   no permite añadir columnas `NOT NULL` sin reconstruir la tabla (y las tablas
   tienen claves ajenas). La migración y el código garantizan que ninguna fila
   queda sin rellenar; el índice único por `uuid` sí está creado.
4. **`[sincronizacion] clave_anterior_hasta`**: campo de configuración añadido
   para saber cuándo caduca la clave anterior tras una rotación (30 días). La
   especificación pide el plazo pero no dice dónde guardarlo; no es un secreto y
   `config.toml` ya es local.
5. **`EQUIPOS.hash_ultimo_fichero` de este equipo** guarda el SHA-256 del
   contenido canónico (no del fichero cifrado), porque el fichero cifrado cambia
   en cada exportación por `generado_en`. Es lo que permite cumplir «solo se
   escribe cuando cambia su contenido». Para los equipos ajenos guarda el hash
   del fichero cifrado, como dice la especificación.
6. **Módulos `aplicacion.rs` y `qr.rs`**: la especificación los reparte entre
   `elementos.rs`, `mod.rs` y la interfaz; se separan para que la aplicación
   transaccional y el QR tengan tests propios sin mezclar responsabilidades.
7. **Llavero inyectable** (`LlaveroSecreto`, `LlaveroEnMemoria` marcado
   `#[doc(hidden)]`): permite probar crear/unir/rotar/quitar/desactivar y la
   pasada sin depender de `secret-tool` ni del Keychain reales. No cambia el
   comportamiento de producción.
8. **Dos avisos de `cargo deny` ignorados con justificación**:
   `RUSTSEC-2026-0173` (`proc-macro-error2`) y `RUSTSEC-2026-0374`
   (`basic-toml`). Ambos son transitivos de `age` a través de
   `i18n-embed`/`i18n-embed-fl` (localización de sus textos), son de
   compilación o de una ruta que MAGI no usa, no tienen parche y no hay versión
   de `age` sin ellos (0.12 arrastra el mismo árbol y además no compila en este
   grafo). La excepción está fechada en `deny.toml` y hay que revisarla al
   actualizar `age`.
9. **Atribución del equipo en conflictos de huella**: `huellas::importar`
   recibe las huellas de los documentos en una lista plana (contrato de la
   pasada), así que con más de un equipo en la carpeta `equipo_ganador` y el
   nombre del autor quedan vacíos y la vista muestra «otro equipo». El dato que
   importa (tipo, clave y fecha de aceptación) sí se guarda y se muestra. No es
   una limitación del formato, solo de la atribución visual cuando concurren
   varios equipos; se puede mejorar pasando los documentos al importador.
10. **El «estado» del QR**: se pinta solo con glifos de medios bloques (no hay
    QR en modo ASCII). Por debajo del tamaño se muestra el texto con «amplía la
    terminal para ver el QR», como pide el informe; en ASCII, directamente el
    texto con ese aviso.

## Estructura de archivos creada/modificada

Creados:

- `src/sincronizacion/mod.rs`, `carpeta.rs`, `cifrado.rs`, `formato.rs`,
  `fusion.rs`, `elementos.rs`, `aplicacion.rs`, `huellas.rs`, `reloj.rs`,
  `equipos.rs`, `qr.rs`.
- `src/almacen/versiones.rs`, `equipos.rs`, `lapidas.rs`, `conflictos.rs`,
  `copias.rs`.
- `src/app/sincronizacion.rs` (estado de las vistas/pasada en la TUI) y
  `src/ui/sincronizacion.rs` (pintado de las vistas y diálogos).
- `docs/formato-sincronizacion.md` y este informe.
- `tests/sincronizacion_equipos.rs`, `tests/sincronizacion_elementos.rs`,
  `tests/sincronizacion_fusion.rs`, `tests/sincronizacion_aplicacion.rs`,
  `tests/sincronizacion_pasada.rs`, `tests/sincronizacion_pasada_e2e.rs`,
  `tests/sincronizacion_huellas.rs`, `tests/sincronizacion_ui.rs`.
- `tests/redimensionado/vistas_m.rs` y 15 instantáneas nuevas de las dos
  vistas y los diálogos (`sincronizacion_*`, `conflictos_*`,
  `clave_sincronizacion_*`, `unir_equipo_*`, `importacion_borrados_*`).

Modificados:

- `Cargo.toml` (age, hmac, hkdf, uuid, notify, qrcode; versión 1.1.0) y
  `Cargo.lock`.
- `src/almacen/migraciones.rs`, `src/almacen/mod.rs`, `src/almacen/hosts.rs`,
  `grupos.rs`, `identidades.rs`, `tuneles.rs`, `snippets.rs`,
  `sincronizaciones.rs`, `etiquetas.rs`, `verificaciones.rs`.
- `src/lib.rs`, `src/config.rs`, `src/llavero.rs`, `src/registro.rs`,
  `src/main.rs` (subcomando `sincronizar`), `src/diagnostico.rs`
  (comprobación de sincronización).
- `src/app.rs` y `src/ui/{mod,disposicion,barra,dialogos,ayuda,servidor_anterior}.rs`.
- `CHANGELOG.md`, `README.md`, `AGENTS.md`, `CLAUDE.md`, `deny.toml` y
  `assets/{completions,man}` regenerados por el subcomando nuevo.
- `docs/fase10/magi-fase10-checklist.md` e
  `docs/fase10/magi-fase10-implementacion.md` (este documento).
- `tests/almacen.rs`, `tests/redimensionado.rs` y sus vistas,
  `tests/diagnostico.rs`, `tests/recursos.rs` y las instantáneas de
  `tests/snapshots/`.

## Decisiones técnicas tomadas durante el desarrollo

- **Una identidad X25519 compartida por todo el inventario** y guardada en el
  llavero; la firma HMAC se deriva de ella con HKDF, de modo que tener la clave
  pública no basta para fabricar un fichero válido.
- **Orden de lectura**: descifrar → comprobar cabecera y firma → comprobar
  `equipo.uuid` contra el nombre del fichero → comprobar `formato` → parsear.
  Un fichero que falla no se aplica nunca a medias.
- **El equipo que pierde un borrado aplazado** no se marca como conflicto de
  contenido (no hay contenido que recuperar); el conflicto `borrado` apunta al
  elemento y se resuelve con `a` (aplicar), `r` (edición nueva que lo
  mantiene) o `d` (dejarlo para otra pasada).
- **«Mantener los míos»** sube el reloj al máximo visto, crea una edición nueva
  de los hosts que iban a borrarse y vuelve a fusionar antes de aplicar, para
  que la resurrección gane en todos los equipos.
- **La copia de `magi.db` se hace antes de aplicar** (también antes de mostrar
  el diálogo de borrados masivos, como pide el AC) con `VACUUM INTO` y rotación
  de las `copias` últimas.
- **El cerrojo es un `flock` no bloqueante en la TUI** (reintenta en la
  siguiente vuelta) y con espera de hasta 10 s en la CLI.
- **La exportación solo se escribe si cambia el hash canónico** del contenido
  (sin `generado_en`), y al salir la TUI fuerza una última pasada si quedaba
  una exportación pendiente.
- **La clave anterior tras rotar se conserva solo para leer** durante 30 días y
  caduca por fecha (`config.toml`), momento en el que se olvida del llavero.
- **La unión por nombre adopta el uuid del fichero** en los elementos locales
  con el mismo nombre natural y después la fusión normal resuelve versiones.
- **La interfaz no bloquea**: la pasada se dispara desde el bucle de la UI con
  el cerrojo no bloqueante y el trabajo pesado (esperas de cerrojo, vigilancia
  con `notify`) va en tareas tokio que emiten eventos; el bucle sigue siendo el
  único escritor de SQLite.
- **La pasada de la TUI se ejecuta en el hilo de la UI** una vez que una tarea
  de tokio ha esperado la clave y el cerrojo (opción acordada para no romper la
  regla del único escritor); el bloqueo está acotado por el objetivo de
  rendimiento de la fase (<100 ms sin cambios; <1 s con cambios).
- **Los conflictos los persiste la aplicación de la fusión**; la pasada anota en
  REGISTRO solo los conflictos nuevos (comparando ids) y los borrados
  aplazados. Un conflicto descartado por el usuario no se vuelve a preguntar
  para la misma versión ganadora (salvo los borrados aplazados, donde
  «descartar» significa decidir más adelante).
- **Al salir queda garantizada la exportación pendiente**: si el reloj híbrido
  cambió desde la última pasada, el cierre espera como máximo 3 s al cerrojo y
  hace una pasada antes de restaurar el terminal.
- **La CLI devuelve 2 sin aplicar nada** cuando haría falta decidir (borrados
  sobre el umbral o clave nueva): el uso previsto es manual o desde un
  temporizador, así que no hay diálogo posible.
- **Las huellas se añaden con `conexion::huellas::aprender`** (que ya entiende
  `known_hosts` con entradas hasheadas) tras una copia con fecha, y solo se
  sustituyen con `sustituir` cuando el usuario acepta el conflicto.
- **Cada cambio de clave invalida el hash del fichero propio** (crear, unir,
  rotar, quitar): el fichero que existe en la carpeta está cifrado con la clave
  anterior y hay que reescribirlo con la nueva aunque el inventario no cambie.
- **Las referencias a un elemento cuyo borrado está aplazado se mantienen**:
  mientras el elemento siga vivo en este equipo, quien lo usa (salto, grupo,
  snippet al conectar, destinos de snippet) lo sigue apuntando; vaciarlas
  cambiaría contenido que viaja sin subir su versión.
- **La lápida de un borrado aplazado no se guarda en `LAPIDAS`**: el borrado
  sigue en el fichero del otro equipo y se aplica con la acción «a» (o cuando
  llegue un fichero nuevo). Guardarla haría que el elemento vivo local perdiera
  contra ella en cada pasada y se re-aplazara el borrado una y otra vez.

## Funcionalidades del checklist completadas

Se copian literalmente del checklist de la fase, en su orden, las 63 funcionalidades dadas por terminadas:

- Migración 7 por `PRAGMA user_version`: `uuid`, `version` y `vector` en GRUPOS, HOSTS, IDENTIDADES, TUNELES, SNIPPETS y SINCRONIZACIONES_DIR, con índice único por `uuid`; las filas existentes reciben `uuid` nuevo y versión de este equipo
- Tabla `EQUIPOS` (id, uuid, nombre, plataforma, es_este, app_version, ultimo_fichero_en, ultima_importacion_en, hash_ultimo_fichero, estado, quitado_en, reloj_ms) con la fila de este equipo creada en la migración (nombre = hostname)
- Tabla `LAPIDAS` (uuid PK, tipo, nombre, version, vector, borrado_en) y tabla `CONFLICTOS` (id, tipo, elemento_tipo, uuid, nombre, version_perdedora_json, version_ganadora, equipo_ganador, detectado_en, resuelto_en, resolucion); sin `CHECK` en sus enumeraciones
- `IDENTIDADES.uuid` es determinista (UUIDv5 de la huella): la misma clave tiene el mismo `uuid` en todos los equipos
- Reloj híbrido: cada versión nueva usa `max(ahora_ms, reloj_ms + 1)` y actualiza `EQUIPOS.reloj_ms`; al importar, `reloj_ms` sube a la mayor versión vista
- Toda escritura desde el almacén de un campo que viaja actualiza `version` (`<ms>.<uuid del equipo>`) y `vector[este equipo]`; los campos propios del equipo no tocan la versión
  - AC: Dado un host, cuando se conecta (cambia `ultima_conexion_en`) o se cambia su último directorio SFTP, entonces su `version` no cambia y no se exporta nada
- Cambiar etiquetas o verificaciones de un host sube la versión del host; cambiar los destinos de un snippet sube la versión del snippet
- Borrar un elemento que viaja deja su lápida con versión y vector; el borrado en cascada de un host deja lápidas de sus túneles y sincronizaciones de directorio
- Un fichero por equipo, `magi-<uuid del equipo>.age`, en la carpeta configurada; cada equipo solo escribe el suyo
- Contenido en claro: línea `MAGI-SINCRONIZACION 1`, línea `firma: <hex>` y el JSON (`formato`, `app`, `equipo`, `generado_en`, `reloj_ms`, `elementos`, `lapidas`, `huellas`, `equipos_quitados`), cifrado con `age` a la clave X25519 de la sincronización
- Firma HMAC-SHA256 del JSON con una clave derivada de la identidad (HKDF-SHA256, info `magi-sincronizacion-firma-v1`); un fichero sin firma válida se rechaza aunque se descifre
  - AC: Dado un fichero cifrado a la clave pública correcta pero con el JSON alterado, cuando se importa, entonces se rechaza con «firma no válida» y no se aplica nada
- Se rechaza un fichero cuyo `equipo.uuid` no coincide con su nombre de fichero o cuyo equipo está en `equipos_quitados`
- Viajan: grupos (sin `plegado`), hosts con etiquetas por nombre y verificaciones (sin estado, última conexión ni directorios SFTP), identidades por huella (alias, tipo, comentario, revocada_en), túneles, snippets con destinos (sin `usado_veces` ni `ultimo_uso_en`) y sincronizaciones de directorio (sin `ultima_ejecucion_en` ni `ultimo_resultado`)
- No viajan REGISTRO, SONDEOS, DELIBERACIONES, `config.toml`, contraseñas del llavero, claves privadas ni la clave de sincronización
- Las rutas bajo el home (`identidad_ref fichero:`, `ruta_local`) se exportan como `~/…` y se expanden al importar
- El fichero propio se escribe de forma atómica por `src/ficheros.rs` (600) y solo cuando cambia su contenido
- Un fichero con `formato` mayor que el que conoce el binario se ignora con el aviso «el equipo X usa una versión más nueva de MAGI; actualiza»
- La fusión lee el estado local y todos los ficheros ajenos legibles: por elemento, si un vector domina al otro gana el dominante; si son concurrentes gana la versión más reciente y el vector resultante es el máximo de ambos
  - AC: Dado un host editado en A y después en B tras importar lo de A, cuando A importa, entonces aplica la versión de B sin registrar conflicto
- Si la versión local pierde frente a una concurrente con contenido distinto, se guarda en `CONFLICTOS` (tipo `elemento`) con su contenido completo
  - AC: Dado un host cambiado en A y en B antes de sincronizar, cuando se sincronizan, entonces en los dos queda la versión más reciente y el equipo cuya versión perdió tiene un conflicto con ella
- Lápidas: un borrado gana a una edición anterior y pierde ante una posterior (la edición resucita el elemento); las lápidas se conservan siempre
- Una referencia a un elemento inexistente o borrado (grupo, salto, snippet al conectar) se resuelve como vacía; un túnel o una sincronización de directorio cuyo host no existe se descarta como conflicto `huerfano`
- Colisión de nombre entre elementos distintos (nombre de host, grupo, snippet, alias de identidad; nombre de túnel o sincronización dentro del host): se renombra el de `uuid` mayor a `<nombre> (2)` como edición nueva y se registra conflicto `nombre`
- Un borrado entrante de un host con sesiones, túneles activos, transferencias o ejecuciones en curso, o de un túnel activo, no se aplica: queda como conflicto `borrado` aplazado
- Una importación que cambia el inventario regenera `~/.ssh/magi_config` como cualquier edición local
- Conflictos: `r` recupera la versión perdedora como edición nueva, `d` la descarta, `a` acepta (huellas y borrados aplazados sin trabajo); los resueltos se conservan 90 días
- La fusión es determinista: el mismo conjunto de ficheros da el mismo resultado en cualquier orden y en cualquier equipo (test de propiedades)
- El fichero incluye las huellas de `~/.ssh/known_hosts` de los hosts del inventario (host, tipo y clave pública)
- Al importar, una huella se añade a `known_hosts` (por `ficheros.rs`, con copia y REGISTRO `huella_sincronizada`) solo si este equipo no tiene ninguna para ese host
- Si este equipo ya tiene otra huella para ese host, nunca se sustituye sola: conflicto `huella` con «aceptar la del otro equipo» o «mantener la mía»
  - AC: Dado db-01 con huella X aquí y huella Y aceptada en otro equipo, cuando se importa, entonces `known_hosts` sigue con X y aparece el conflicto con la fecha en que el otro equipo aceptó Y
- `[sincronizacion]` en `config.toml`: `carpeta`, `retraso_seg = 5`, `copias = 10`, `umbral_borrado_hosts = 5`, `umbral_borrado_pct = 20`; sin `carpeta` la sincronización está desactivada
- Al abrir MAGI se hace una pasada (importar lo que cambió, fusionar, aplicar, exportar si hace falta)
- Mientras MAGI está abierto vigila la carpeta (`notify`, o sondeo cada 30 s si no puede) e importa cuando cambia el fichero de otro equipo, agrupando 2 s
- Exporta `retraso_seg` segundos después del último cambio local; al salir, una exportación pendiente se hace antes de cerrar
- Solo un proceso sincroniza a la vez (cerrojo `flock` en `$XDG_RUNTIME_DIR/magi/sincronizacion.lock`); otra ventana o `magi sincronizar` esperan o lo dicen
- Se leen solo ficheros `magi-<uuid>.age`; se ignoran temporales y copias en conflicto de las herramientas de sincronización; un fichero que no se descifra o no tiene firma válida se reintenta en la siguiente pasada
- Crear (carpeta sin ficheros de MAGI): genera la identidad `age`, la guarda en el llavero (`src/llavero.rs`), la muestra como texto y QR, pide confirmar «la he guardado» y exporta el primer fichero
- Unir (carpeta con ficheros de MAGI): pide la clave pegada (oculta, con opción de verla), la valida descifrando y comprobando la firma de un fichero, la guarda en el llavero e importa
  - AC: Dado una clave equivocada, cuando se intenta unir, entonces sale «la clave no abre los ficheros de esta carpeta» y no se guarda nada
- Al unir, los elementos locales con el mismo nombre que uno del fichero adoptan su `uuid` y se fusionan (gana la versión más reciente, la otra queda como conflicto); el resto se suma
- «Mostrar clave» (texto y QR) desde el llavero, con confirmación
- «Quitar equipo» (otro equipo): confirmación, lo añade a `equipos_quitados`, borra su fichero de la carpeta y cambia la clave (nueva identidad mostrada como texto y QR; la anterior solo para leer, 30 días como máximo)
- Si el fichero de un equipo conocido deja de abrirse con la clave de este equipo, la sincronización se detiene aquí hasta introducir la clave nueva; con ella, este equipo vuelve a cifrar su fichero
- Un equipo que se encuentra en `equipos_quitados` deja de sincronizar y ofrece unirse de nuevo como equipo nuevo
- «Desactivar en este equipo»: borra su fichero de la carpeta y la clave del llavero y quita `carpeta`; `magi.db` queda intacta
- Antes de aplicar una importación que cambia algo, copia de `magi.db` en `~/.local/share/magi/copias/magi-<AAAAMMDD-HHMMSS>.db` (600, `VACUUM INTO`); se conservan las `copias` últimas
- Si una importación va a borrar más de `umbral_borrado_hosts` hosts o al menos `umbral_borrado_pct` % de ellos, diálogo IMPORTACIÓN CON BORRADOS: `a` aplicar, `m` mantener los míos (edición nueva que los devuelve a todos), `Esc` decidir luego (sincronización en pausa)
  - AC: Dado otro equipo que ha borrado 7 de 23 hosts, cuando este equipo importa, entonces aparece el diálogo con los 7 nombres, ya existe la copia de `magi.db` y no se ha borrado nada
- En pausa no se importa ni se exporta hasta que el usuario decide desde el diálogo o desde el panel
- Vista SINCRONIZACIÓN desde la paleta (`sincronización`): carpeta, huella corta de la clave, este equipo, lista de equipos (nombre, plataforma, versión de MAGI, último fichero, estado) y conflictos pendientes; `q`/`Esc` vuelve
- Teclas de la vista: `s` sincronizar ahora, `c` conflictos, `k` mostrar clave, `x` quitar equipo, `r` cambiar clave, `d` desactivar aquí; sin configurar, `n` configurar carpeta (crea o une según lo que encuentre)
- Vista CONFLICTOS: tipo, elemento, quién ganó y cuándo, con el detalle de los campos que difieren; acciones `r`, `a`, `d` según el tipo
- Indicador en la barra inferior: `⇅ al día`, `⇅ N conflictos`, `⇅ pausa`, `⇅ clave nueva`, `⇅ error` y `⇅ …` sincronizando, con color y en ASCII (`sync`); no aparece si la sincronización está desactivada
- QR de la clave pintado con medios bloques; por debajo de su tamaño se muestra solo el texto con «amplía la terminal para ver el QR»
- Vistas SINCRONIZACIÓN y CONFLICTOS y diálogos (crear, unir, clave, importación con borrados) con mínimo y modo estrecho en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)
- Entradas de paleta `sincronización`, `sincronizar ahora` y `conflictos de sincronización`
- `magi sincronizar` hace una pasada sin TUI: código 0 si va bien, 1 con error y 2 si hace falta decidir (borrados por encima del umbral o clave nueva), sin aplicar nada en ese caso
- `magi sincronizar estado` muestra carpeta, equipos con su estado y último fichero, y conflictos pendientes, sin descifrar nada
- `magi diagnostico` añade la comprobación de sincronización (carpeta accesible y escribible, estado de los equipos) sin leer la clave del llavero
- Tipos nuevos en REGISTRO: `inventario_importado` (equipos, creados, actualizados, borrados, conflictos), `inventario_conflicto`, `inventario_equipo_quitado`, `inventario_clave_cambiada`, `inventario_sinc_activada`, `inventario_sinc_desactivada` e `inventario_sinc_error` (solo al cambiar de estado); el filtro `t` los agrupa en «inventario»
- `docs/formato-sincronizacion.md`: formato completo del fichero (cabecera, firma, JSON, tipos de elemento y campos, reglas de fusión) para la app Android (F11)
- README: activar la sincronización, añadir un equipo, guardar la clave, quitar un equipo y abrir un fichero con `age` fuera de MAGI
- CHANGELOG en «Sin publicar» con la sincronización cifrada (versión MINOR: migración 7)
- Tests: migración 6→7, reloj híbrido, versiones solo en campos que viajan, fusión (dominancia, concurrencia, lápidas, huérfanos, nombres, determinismo), firma y equipo del fichero, huellas, umbral de borrados, unión por nombre y rotación de clave
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check` y `TZ=UTC cargo test` limpios y en verde
- Revisión adversarial del código nuevo (fusión, borrados, huellas, clave y llavero, ficheros de la carpeta) con verificación independiente de cada hallazgo, resultado en el informe de implementación

## Pendientes y bloqueos

Pendiente al cerrar esta sesión:

- **Revisión adversarial de cierre**: hecha (ver el resultado y las
  correcciones en el resumen, arriba). Sus dos hallazgos están corregidos con
  tests de regresión y anotados en este informe.
- **Validaciones manuales que necesitan el entorno real** (no cubiertas por la
  suite): crear y unir con `secret-tool`/Keychain desbloqueados, rotar/quitar y
  volver a leer con la clave nueva, abrir un fichero con el `age` real fuera de
  MAGI, escanear el QR con un móvil, y una pasada real entre dos equipos con
  Syncthing de por medio (incluida la vigilancia `notify` en vivo y el
  aplazamiento de borrados con sesiones/túneles activos). Los tests cubren la
  lógica con llavero en memoria y carpetas temporales.
- **Entrega al analista funcional**: el desarrollador debe entregarle este
  informe antes de especificar la Fase 11 (app Android). Hasta entonces la
  documentación maestra no refleja lo implementado.

Bloqueos: ninguno de producto. Durante el desarrollo, `age` 0.12 no compila en
este árbol (conflicto `ml-kem`, ver desviación 1) y se usa 0.11.5. Las dos
excepciones de `cargo deny` (desviación 8) hay que revisarlas al actualizar
`age`.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

- Compilar: `cargo build` (o `cargo run` para la TUI). La migración 7 se aplica
  sola al abrir `magi.db`; una base de datos de una versión mayor no se toca.
- Sincronizar sin TUI: `cargo run -- sincronizar` (código 0/1/2) y
  `cargo run -- sincronizar estado`.
- Configurar: añadir a `~/.config/magi/config.toml`:

  ```toml
  [sincronizacion]
  carpeta = "~/Sync/magi"
  retraso_seg = 5
  copias = 10
  umbral_borrado_hosts = 5
  umbral_borrado_pct = 20
  ```

  y crear/unir desde la vista SINCRONIZACIÓN de la TUI (la clave va al llavero).
- Tests: `TZ=UTC cargo test` (suite completa; requiere `TZ=UTC` por las
  instantáneas). Resultado de la sesión: **todo en verde**, incluidos lib 499,
  almacén 56, redimensionado 283 (con las instantáneas nuevas), las siete
  suites de sincronización (aplicación 23, elementos 14, equipos 8, fusión 19,
  huellas 8, pasada 10, e2e 15), interfaz de sincronización 9, diagnóstico 15,
  recursos 6, servidor 13, sftp 48, etc. Los tests de la fase:
  `TZ=UTC cargo test --test sincronizacion_equipos --test
  sincronizacion_elementos --test sincronizacion_fusion --test
  sincronizacion_aplicacion --test sincronizacion_pasada --test
  sincronizacion_pasada_e2e --test sincronizacion_huellas --test
  sincronizacion_ui --test redimensionado`.
- Calidad: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo deny check` (con las dos excepciones fechadas de la desviación 8) y
  `TZ=UTC cargo test`, todo limpio y en verde en la sesión de cierre.
- Recursos: si cambia la CLI, `scripts/generar-recursos.sh` regenera
  `assets/completions` y `assets/man`; `tests/recursos.rs` lo exige.
- Abrir el fichero fuera de MAGI: guardar la clave en un fichero 600 y
  `age -d -i clave.txt magi-<uuid>.age`; la comprobación de la firma se
  describe en `docs/formato-sincronizacion.md` §3.
- Entornos aislados: fijar `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`,
  `XDG_STATE_HOME` y `XDG_RUNTIME_DIR` (corto) como en el resto de la suite.