# MAGI - Fase 11: App Android — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. No se
> regenera desde cero. La especificación de la fase está en
> [magi-fase11-informe.md](magi-fase11-informe.md) y el alcance verificable en
> [magi-fase11-checklist.md](magi-fase11-checklist.md).

**Estado de la sesión (borrador, cierre de la fase, instantánea del
2026-10-31 23:36):** quedan las validaciones externas y los trámites de cierre. El núcleo compartido y
`crates/magi-movil` están implementados y probados; la app Android está
completa (proyecto Gradle, plataforma, UI Compose y adaptadores UniFFI) y la
revisión adversarial de cierre encontró y corrigió un crítico (la clave del
móvil no se creaba y toda conexión fallaba), cinco altos, siete medios y diez
bajos, con prueba instrumentada real de Keystore. La verificación final está en
verde: 126 pruebas JVM sin fallos, 3/3 instrumentadas con el AVD `magi-api30`
(API 30, x86_64, PIN 1234), `lint` con 0 errores, `assembleRelease`
compilando sin firma, Rust (fmt, clippy, test --workspace y deny) en verde y
`dist plan` publicando solo `magi-ssh` con sus cuatro objetivos. Faltan la
validación manual en el plegable, la CI de GitHub, la publicación real con el
keystore del desarrollador, la corrección de `docs/formato-sincronizacion.md`
§11.9 y R85-R87; el checklist ya está marcado (110/110; 823/827 en el total)
por el Lead.

## Resumen de lo implementado

### Workspace de Cargo y núcleo compartido (implementado; commit `ef4e18f`)

- Commits: `a5f1e1f` (workspace con el núcleo compartido), `82bff82`
  (mecánica de túneles y SOCKS5 al núcleo), `ef4e18f` (lógica del núcleo móvil
  y contrato UniFFI), `419ebe1` (pulido del núcleo móvil, rotación de clave y
  pruebas) y `78e2cc2` (app Android completa e integración con el núcleo, 129
  ficheros). El árbol de trabajo conserva sin commit la ronda de correcciones de
  la revisión adversarial (44 ficheros, +1422/−302).
- `Cargo.toml` raíz: `[workspace]` con `crates/magi-nucleo` y
  `crates/magi-movil`, `resolver = "2"`; `magi-ssh` sigue en la raíz con
  su nombre, versión 1.1.0, `publish = false` y `dist` sin tocar.
  `Cargo.lock` está al día: `cargo metadata --locked` termina con código 0.
- `crates/magi-nucleo` (`publish = false`, versión 0.1.0): almacén y
  migraciones; sincronización compartida (formato, cifrado, fusión, aplicación,
  elementos, huellas, reloj, carpeta, pasada común y
  `CarpetaSincronizacion`); conexión SSH (cliente, saltos, huellas, reenvíos,
  terminal y `abrir_con_plazo`); túneles (mod, escucha, local y socks5);
  terminal (tramos y teclas); claves (DER a SSH, X.509 a OpenSSH, común y
  `Firmante`); `instalar_clave` con `scripts/instalar_clave.sh`; llavero,
  registro, ficheros, configuración, deliberación, snippets, `trabajo_vivo` y
  `rutas`.
- Implementado en esta sesión y con pruebas en verde: **tramos de pantalla**
  (estilos, colores y tope de 30 fps; 9 pruebas), **codificación de teclas
  §7.6** (10 pruebas; el escritorio `src/teclas.rs` delega en la tabla
  común), **claves** DER→SSH y X.509→OpenSSH con verificación real con
  `ssh-key` (4 y 3 pruebas), **instalar clave §7.9** con 11 pruebas que
  ejecutan el script con `sh` y un `HOME` temporal, y **pasada en modo
  lector** con `CarpetaSincronizacion` y 8 pruebas (borrado de 10 de 12 hosts
  sin copia ni confirmación, aplazados con trabajo vivo, huellas, determinismo
  y T81).
- El escritorio mantiene sus rutas de importación: `src/lib.rs` reexporta
  `magi_nucleo::{ficheros, modelo, registro, rutas}` y
  `src/sincronizacion/mod.rs` reexporta la sincronización común y añade la
  pasada en modo escritor, los equipos y el QR. `src/teclas.rs` solo traduce
  los eventos de crossterm a la tabla del núcleo.
- Los documentos de reglas (`AGENTS.md`, `CLAUDE.md` y
  `docs/fase11/CLAUDE.md`) están actualizados con las rutas nuevas.

### Entorno Android (hecho)

- SDK en `/home/hector/Android/Sdk`: commandline-tools 23.0.0, platform-tools
  37.0.1, build-tools 36.0.0 y 37.0.0, plataformas `android-37.0/37.1/37.2`,
  NDK r30 `30.0.16248370`, cmake 3.31.6, emulador 37.2.13 e imagen
  `system-images/android-30/default/x86_64` (AOSP, sin Google Play Services);
  Gradle 9.8.2, AGP 9.5.0, JDK 26 del sistema, Rust 1.98.1, cargo-ndk 4.1.2 y
  los objetivos `aarch64-linux-android` y `x86_64-linux-android`.
- **R91 retirado**: russh 0.63.3 con su cripto por defecto (`aws-lc-rs`)
  compila y enlaza para `arm64-v8a` y `x86_64` con el NDK r30; no hace
  falta activar la feature `ring`.
- Emulador viable con KVM (`emulator -accel-check` correcto): el AVD
  `magi-api30` (API 30, x86_64, AOSP) arranca headless en unos 8-10 s y
  aceptó un APK de prueba con `adb install`.
- Todo ello está documentado en `/tmp/entorno-android.md` y
  `/tmp/entorno-android.sh` (fuera del repositorio, como pedía el encargo).

### Proyecto Gradle `android/` (app completa y verificada)

- `settings.gradle.kts` (proyecto `magi-android`, módulo `:app`), build
  raíz con AGP 9.5.0 y el Kotlin integrado de AGP 9, catálogo
  `gradle/libs.versions.toml` con versiones fijas (Compose BOM 2026.09.00,
  JNA 5.19.1, CameraX 1.6.2, ZXing core 3.5.4, biometric 1.1.0, DataStore
  1.3.0, etc.), wrapper de Gradle 9.8.2 con `distributionSha256Sum` y
  `gradle/verification-metadata.xml` (448 componentes con SHA-256).
- `app/build.gradle.kts`: `applicationId org.cuatrodetres.magi`, minSdk 30,
  compileSdk/targetSdk 37, `ndkVersion 30.0.16248370`, `versionName` y
  `versionCode` desde `version.properties` con la fórmula §7.10, firma de
  release solo si existen las variables `MAGI_KEYSTORE_*` (v2 y v3, v1
  desactivado), release solo `arm64-v8a` y debug `arm64-v8a` + `x86_64`,
  las tareas `compilarNucleoAnfitrion/Release/Debug`,
  `generarBindingsNucleo`, la tarea de licencias de «Acerca de» y sus
  enganches a `compile*Kotlin`, `merge*JniLibFolders` y
  `testDebugUnitTest`.
- **Cadena completa validada**: `assembleDebug` genera `app-debug.apk`
  (≈50 MB) con `libmagi_movil.so` en `arm64-v8a` y `x86_64`,
  `versionCode = 1009` y `versionName = 0.1.0`; los bindings Kotlin se
  regeneran en cada compilación en
  `app/build/generated/uniffi/kotlin/org/cuatrodetres/magi/nucleo/magi_movil.kt`
  (≈266 KB, no versionado).
- **Capa de plataforma escrita y compilando**: SAF (`CarpetaSaf`), Keystore
  con AES-256-GCM (`LlaveroKeystore`), clave P-256 StrongBox/TEE
  (`ClaveMovil`), bloqueo con 5 minutos (`Bloqueo`, `ReglaBloqueo`),
  `FLAG_SECURE` (`PantallaSegura`), servicio en primer plano
  `specialUse` (`ServicioSesiones`), batería (`Bateria`,
  `ReglasBateria`), DataStore (`PreferenciasMagi`), cámara/QR y las reglas
  de carpeta, llavero y servicio.
- **Plataforma, UI y adaptadores integrados.** La capa de plataforma (arriba),
  la interfaz Compose (Hosts, ficha, Sesiones, terminal, túneles, Ajustes,
  diálogos, asistente, bloqueo y componentes) y los adaptadores reales
  (`NucleoRepositorio`, `AdaptadoresPlataforma`, `AdaptadorOyenteNucleo`,
  `Conversiones` y `FuenteNucleoReal`) están escritos y probados; el manifest
  mantiene los permisos exactos del checklist, `allowBackup="false"` y las
  reglas de extracción.
- La supresión `NewApi` de los bindings generados por UniFFI
  (`java.lang.ref.Cleaner`, API 33, con minSdk 30) sigue siendo necesaria y está
  acotada al fichero generado; `lint` termina con **0 errores** y 14 avisos.

### Puerta UniFFI `crates/magi-movil` (implementada; commit `ef4e18f`)

- Crate `magi-movil` 0.1.0 (`publish = false`,
  `crate-type = ["cdylib", "lib"]`, cdylib `libmagi_movil.so`), UniFFI
  fijado en `=0.32.2` con runtime propio de tokio y `uniffi.toml` con el
  paquete Kotlin `org.cuatrodetres.magi.nucleo`.
- Ya no es un stub: implementa el arranque con `Rutas`/`Equipo` inyectados
  y **un único hilo escritor de SQLite**; inventario con uuid; sincronización
  en modo lector (elegir carpeta SAF, unir con clave validando descifrado y
  firma, pasada, rotación de clave con la anterior 30 días, conflictos de
  huella, dejar de sincronizar y borrar datos); clave pública del móvil;
  sesiones con una conexión viva por host; terminal por tramos con 1000 líneas
  de historial, teclas, pegado, redimensionar, desplazar y selección; túneles
  local/remoto/dinámico en `127.0.0.1` con puerto libre y automáticos; e
  instalar mi clave. `version()` devuelve datos reales.
- Contrato ampliado: `Nucleo.pegado_entre_corchetes(sesion)` y
  `FirmanteMovil.olvidar_clave()`; el resto de §5.1 está implementado y las
  retrollamadas (`CarpetaMovil`, `LlaveroMovil`, `FirmanteMovil` y
  `OyenteNucleo`) son las que implementa Kotlin.
- La lógica está ejercitada en el emulador por las pruebas instrumentadas; lo
  que falta es la validación en el móvil real (ver pendientes).

### Integración continua y publicación Android (hecho)

- `ci.yml` pasa a `--workspace` en clippy y en los tests de Linux, y en
  build/clippy/test de macOS; `cargo deny` se mantiene como
  `cargo deny check` porque 0.20.2 no admite `--workspace` y ya analiza el
  grafo completo del workspace.
- `release.yml` (dist) excluye las etiquetas `android-v*` con el patrón
  negativo `!android-v*`; no se toca su lógica y la cabecera de ajustes
  manuales lo documenta.
- `android.yml` (nuevo): compila el núcleo con cargo-ndk 4.1.2
  (`arm64-v8a` en release y `x86_64` en debug), ejecuta
  `./gradlew lint testDebugUnitTest assembleDebug`, publica el APK de
  depuración y, en `main` y etiquetas `android-v*`, corre la prueba de humo
  en un emulador API 30 x86_64 con KVM.
- `android-release.yml` (nuevo): solo con etiqueta `android-v*`;
  `verificar` comprueba el formato, la coincidencia exacta con
  `android/version.properties` y calcula el `versionCode` §7.10;
  `compilar` firma con el keystore de los cuatro secretos (y solo este
  workflow los recibe), exige APK release `arm64-v8a` con
  `versionName`/`versionCode` correctos, verifica la firma v2 y v3 con
  `apksigner`, genera el `.sha256`, atesta la procedencia y sube los
  artefactos; `publicar` crea la GitHub Release con las notas del CHANGELOG y
  la huella SHA-256 del certificado, **sin marcarla nunca como «Latest»** y
  como prerelease si es `-rc.N`.
- `.github/actionlint.yaml` (nuevo): ignora solo SC2086/SC2129/SC2001 del
  `release.yml` generado por dist, para que actionlint con shellcheck siga
  siendo una puerta real sobre el resto.
- Verificación disponible hoy: actionlint 1.7.12 + shellcheck 0.11.0 en los
  cinco workflows (código de salida 0), YAML válido, los 21 scripts `run:`
  pasan `bash -n` y están probados los casos 0.1.0-rc.1→1001, 0.1.0→1009 y
  1.2.3→102039.

### Publicación y documentación

- `docs/publicar-android.md`: keystore y secretos (los crea el
  desarrollador), flujo de etiqueta y ensayo `rc`, qué hace
  `android-release.yml`, verificación del APK/certificado/atestación/ABI y
  Obtainium, Syncthing-Fork en «solo recibir» y qué hacer al perder el móvil o
  reinstalar la app. Está commiteada en `419ebe1`.
- `android/CHANGELOG.md` en formato Keep a Changelog, con `[Sin publicar]` y
  `[0.1.0]` (fecha provisional hasta publicar).
- `README.md`: sección «App Android» antes de «Atajos».
- `android/version.properties`: `version=0.1.0` con la fórmula en
  comentario; la leen Gradle y los workflows.

### Revisión adversarial de cierre (resultado)

Hecha con revisión independiente y un informe de hallazgos por gravedad; todos
los hallazgos se corrigieron y se volvieron a probar. Resumen:

- **Crítico C1 — la clave del móvil no se creaba y toda conexión fallaba.**
  Corregido y validado con una prueba instrumentada real contra el Keystore del
  emulador. En el camino apareció un defecto real en `ClaveMovil.generar`:
  `KeyGenerator` para EC lanza `NoSuchAlgorithmException` en AndroidKeyStore;
  ahora se usa `KeyPairGenerator`.
- **Altos corregidos.** A1: conexiones caídas cacheadas; A2: comandos perdidos
  durante «abriendo» (receptor guardado, tamaño aplicado al PTY y cierre antes
  de abrir); A3: esperas sin plazo (handshake SSH, canal, PTY, `exec` y lectura
  del script de instalación); A4: la carpeta SAF no se recordaba tras reiniciar
  (ahora se elige una vez al arrancar con `elegirCarpeta`); A5: una pasada
  cancelada dejaba la fase en `SINCRONIZANDO` (ahora corre en el runtime del
  núcleo y la fase se restaura siempre).
- **Medios corregidos.** M1: trabajo vivo con saltos y solo túneles activos, con
  túneles caídos descartables; M2: peticiones y pantallas sin conflación
  (`Channel(UNLIMITED)`) y sin borrar peticiones; M3/M4: parada y doble
  activación de túneles con limpieza y comprobación+inserción atómicas; M5:
  «borrar todo» coherente (SAF, clave recreada y caché invalidada); M6: borrar
  contraseñas solo con «borrar todo»; M7: entrada del terminal serializada.
- **Bajos corregidos.** B1: no resolver borrados aplazados con lecturas
  parciales; B2: contraseñas en `Zeroizing` y UTF-8 estricto; B3: comentario de
  la clave solo ASCII; B4: SAF acepta tamaño desconocido; B5: `known_hosts` con
  cerrojo `flock`; B6: documentación de `magi-movil` actualizada; B7: el
  cierre del núcleo anula la fachada; B8: pruebas nuevas de `magi-movil`
  (diálogos, hilo escritor, formato de la clave anterior, URI SAF, túnel en
  puerto libre y validaciones de escucha); B9: identidad tipada con constante;
  B10: `clave_generada` solo se anota si la clave cambia.

Los hallazgos no dejan código abierto: lo que queda anotado como pendiente son
las validaciones que solo se pueden hacer en el plegable, la CI real, la
publicación con el keystore del desarrollador, la corrección del documento de
formato §11.9 y R85-R87. El emulador valida API 30 x86_64 (AVD `magi-api30`,
PIN 1234).

Corrección posterior al cierre (2026-11-01): al validar en un Honor MagicOS
se encontró que el requisito setUnlockedDeviceRequired del Keystore falla al
desbloquear con la cara (fallo conocido de Android/OEM). Se corrigió y se
documentó como desviación 15; la migración y el llavero se validaron con
pruebas instrumentadas. Queda confirmarlo en el móvil real.

### Corrección 11b: el túnel local del móvil (4 nov 2026)

La validación en el plegable encontró que un túnel local activo
(127.0.0.1:9090 → 127.0.0.1:9090) no reenviaba: Chrome y Brave daban
ERR_SOCKET_NOT_CONNECTED, la fila seguía en «0 conex. · ↑0 B ↓0 B» sin error y
aparecía la nota de puerto libre aunque se había activado con el interruptor.
El túnel equivalente funcionaba desde otro ordenador.

Con la prueba instrumentada nueva se comprobó que, con la clave del host
aceptada, el reenvío sí funciona en Android (el eco llega por el agujero y los
bytes suben y bajan). No se reprodujo el error literal del navegador; el
síntoma observable encaja con la causa que sí se reprodujo en Rust y se
corrigió:

- **Causa raíz (conexión del host perdida sin vigilante).** Mientras había una
  sesión abierta, su bucle vigilaba la conexión SSH; con solo un túnel, no
  había nadie mirándola. Si la conexión se caía (cambio de red, doze o
  keepalive sin respuesta), el túnel seguía marcado «activo» con la escucha
  abierta, pero cada conexión del navegador fallaba al pedir el canal
  direct-tcpip, se cerraba y no movía ni un byte; el error quedaba solo en los
  contadores internos, que la fila no enseñaba.
- **Causa secundaria (nota de puerto libre siempre).** El puerto efectivo se
  rellenaba en toda activación, así que la nota «(puerto libre, no se guarda)»
  salía siempre y escondía el puerto real.
- **Causa secundaria (contadores engañosos).** La fila solo enseñaba
  conexiones abiertas: al cerrar el navegador volvían a 0 y parecía que no
  había pasado nada.

Correcciones:

1. **Vigilante por conexión (cada 500 ms).** Si el host cierra la conexión sin
   que nadie la pare, se suelta la conexión, las sesiones de ese host pasan a
   caídas y sus túneles a caídos con el motivo, de forma que se puede pulsar
   «Relanzar»; el último estado del host queda en error.
2. **TunelVivo gana aceptadas, ultimo_error y ultimo_error_en** (contrato
   aditivo). La fila enseña «N abiertas · M aceptadas · ↑… ↓…» y el último
   error con su hora, y se refresca cada segundo mientras se ven túneles o la
   ficha (antes solo cambiaba al activar o parar).
3. **Puerto efectivo solo al abrir en un puerto libre**; con el interruptor
   normal la fila enseña siempre la escucha de verdad (127.0.0.1:9090) y no la
   nota.
4. **Cabecera «1 activo»** en singular y «auto» en una línea; los nombres
   largos no parten (elipsis).

Pruebas nuevas que cubren el fallo:

- **Rust, sin red externa** (crates/magi-movil/tests/tunel_local_e2e.rs):
  servidor SSH en proceso y servicio de eco; el túnel reenvía de verdad, los
  contadores lo reflejan, el puerto libre se marca solo con esa opción y, al
  tirar la conexión del host, el túnel pasa a caído con motivo. Sin el
  vigilante, la prueba de caída se queda esperando un «caído» que nunca llega.
- **Android, instrumentada** (PruebaTunelE2E.kt): dos pruebas con el núcleo
  nativo. La primera importa la fixture (host 10.0.2.2:12222 y túnel
  127.0.0.1:9090 → 127.0.0.1:19091), activa el túnel y comprueba el eco por un
  socket local y los contadores (abiertas y aceptadas). La segunda conecta,
  tira todas las conexiones SSH desde el canal de control del servidor de
  pruebas (10.0.2.2:12223, orden «tirar») y comprueba que el túnel pasa a caído
  con motivo. Necesitan el servidor de pruebas del anfitrión y se omiten solas
  si no está (CI).

Revisión adversarial (T31) de esta corrección: se repasaron las carreras de
parada/relanzado y doble activación, la identidad del vigilante (usa Weak y
comprueba Arc::ptr_eq antes de tocar el mapa), las sesiones y túneles que
comparten conexión, el refresco de contadores desde la interfaz y el caso de
CI sin servidor de pruebas. Se corrigieron en el acto los avisos de Clippy que
aparecieron (paréntesis y complejidad de tipo en el gancho de copia) y se dejó
la prueba instrumentada con omisión automática si falta el servidor. Se
comprobó además que la prueba de caída falla si se desactiva el vigilante y
vuelve a pasar al restaurarlo, que es la reproducción automatizada del fallo
que faltaba. No se encontraron fugas nuevas ni cambios en el escritorio.

Validado en el Honor real (4 nov 2026): tras instalar la compilación con la
corrección, el túnel local reenvía y Chrome y Brave abren http://127.0.0.1:9090
por el agujero del túnel. Es la confirmación que faltaba del síntoma original.

### Verificación final (2026-10-31 23:41)

- **Rust:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -D warnings`, `TZ=UTC cargo test --workspace --locked` y
  `cargo deny check` en verde. En máquinas con cuota de disco o `/tmp` justa,
  las pruebas del escritorio pueden necesitar `--test-threads=1`; en CI no hace
  falta.
- **Gradle:** `assembleDebug`, `lint`, `testDebugUnitTest` y
  `connectedDebugAndroidTest` en verde con el AVD `magi-api30` (API 30,
  x86_64, PIN 1234): **126 pruebas JVM, 0 fallos**, **lint 0 errores** (14
  avisos) y **3/3 pruebas instrumentadas** (`PruebaHumo`,
  `PruebaHumoNucleo.cargaElNucleoImportaLaFixtureYPintaSusHosts` con la
  fixture real `.age` y el inventario pintado por la API, y
  `PruebaHumoNucleo.elArranqueCreaLaClaveDelMovilYQuedaLista` con el Keystore
  real). `assembleRelease` compila y deja `app-release-unsigned.apk` (sin
  firma, porque en local no hay keystore).
- **dist:** `plan` sigue publicando solo `magi-ssh` con sus cuatro objetivos
  (validado en local con 0.32.0; la CI fija 0.33.1).
- **actionlint:** 0 errores en los cinco workflows.
- **Corrección 11b (2026-11-04).** Rust: fmt, clippy, cargo test --workspace y
  cargo deny en verde, incluidas las pruebas E2E nuevas del túnel (reenvío
  real, puerto libre, caída de la conexión del host y comprobación de que la
  prueba de caída falla sin el vigilante). Android: lint 0 errores, **127**
  pruebas JVM y assembleDebug; **8/8** pruebas instrumentadas en el AVD API 30
  (las 4 anteriores y las dos de PruebaTunelE2E: reenvío por 127.0.0.1:9090 y
  caída al tirar la conexión SSH del host). Además, **validado en el Honor
  real**: Chrome y Brave abren el túnel local tras instalar la corrección.
- **Corrección posterior (2026-11-01, desbloqueo facial).** Tras probar la app
  en un Honor MagicOS se reprodujo el fallo de setUnlockedDeviceRequired
  (desviación 15). Con las claves ya migradas, la cadena se repitió: lint 0
  errores, 126 pruebas JVM y assembleDebug, más connectedDebugAndroidTest con
  **6/6** pruebas instrumentadas (las 3 de siempre y las 3 de PruebaKeystore:
  llavero real, firma P-256 y migración del llavero antiguo sin perder el
  secreto). Queda pendiente confirmarlo en el Honor.
- **Commit de cierre:** 5495161 (ronda de correcciones de la revisión
  adversarial, cambios de la app Android y checklist marcada). La cadena de
  Gradle se repitió después, sobre ese árbol, con el mismo resultado verde.

## Desviaciones respecto a la especificación (qué y por qué)

1. **`ci.yml`: cargo-deny sin `--workspace`.** El checklist pide pasar a
   `--workspace`, pero cargo-deny 0.20.2 no admite esa opción (verificado:
   `unexpected argument`) y analiza por sí mismo el grafo completo del
   workspace, incluidos `magi-nucleo` y `magi-movil`. Se deja
   `cargo deny check` con un comentario en el workflow.
2. **`release.yml` con patrón negativo.** El informe §5.3 lo prevé así («sin
   cambios, salvo excluir `android-v*` si su patrón lo cubre»): el patrón de
   dist `**[0-9]+.[0-9]+.[0-9]+*` sí casa `android-v0.1.0`, así que se
   añade `!android-v*` a `on.push.tags`. No se toca la lógica de dist.
3. **`.github/actionlint.yaml` no previsto en el plan.** El `release.yml`
   generado por dist arrastra avisos de ShellCheck (SC2086/SC2129/SC2001)
   inmutables aquí; se ignoran solo esos códigos para ese fichero, con
   comentario, y no para los workflows nuevos, que pasan limpios.
4. **`android.yml` instala solo el NDK con `sdkmanager`; la plataforma y las
   build-tools las deja a AGP.** La CLI de Android pasó a identificar paquetes
   con rutas (`ndk/30.0.16248370`) y la vieja usa `;`; el workflow prueba
   las dos formas para valer con cualquier runner. Fijar a mano plataforma y
   build-tools añadía fragilidad; AGP las descarga si faltan (licencias ya
   aceptadas).
5. **JDK 21 en CI frente al JDK 26 del entorno local.** AGP 9.5 exige JDK ≥ 17;
   Temurin 21 (LTS) es lo que instala `actions/setup-java` en el runner,
   mientras que la máquina de desarrollo usa el único JDK del sistema (26), ya
   validado en el entorno.
6. **Fecha de `[0.1.0]` en `android/CHANGELOG.md`.** Se deja 2026-10-31
   como fecha de referencia del borrador; se actualizará al publicar la
   primera versión.
7. **Marcado de la checklist al cierre.** La regla de trabajo marca las casillas
   al terminar la fase y el Lead la ha marcado en la última ronda: 110/110 y
   823/827 en el total del proyecto. No es una desviación, sino el orden previsto
   de cierre; este informe reproduce ya las 110 funcionalidades.
8. **Nota de entorno, no de especificación:** con API 37 el repositorio de
   Android publica plataformas menores (`platforms/android-37.0`, 37.1,
   37.2) en vez de `android-37`; `compileSdk = 37` resuelve a
   `android-37.0` (verificado). No cambia ningún requisito del informe.
9. **Ctrl/Mayús/Alt+flecha ahora con `CSI 1;m`.** Al delegar la codificación
   de teclas en la tabla común del núcleo, el escritorio pasa a emitir
   `CSI 1;m` con el modificador (p. ej. Ctrl+↑ = ESC `[1;5A`) en vez de la
   forma anterior. Es una mejora de §7.6; la API del núcleo y las pruebas del
   escritorio no cambian.
10. **La clave anterior del móvil vive en el secreto del llavero.** Para no
    añadir tablas ni columnas, la rotación guarda la clave anterior con su
    fecha dentro del mismo secreto cifrado (`sincronizacion-anterior`); la
    fecha límite de 30 días se comprueba al leer.
11. **Borrados masivos en el móvil: F11 manda sobre el documento de formato.**
    `docs/formato-sincronizacion.md` §11.9 pide una «red de seguridad» para
    los borrados masivos en Android, pero el informe de la Fase 11 §7.3 manda
    aplicarlos sin copia ni confirmación. Se ha implementado según F11 y queda
    pendiente corregir el documento de formato para que diga lo mismo.
12. **`dist plan` validado en local con 0.32.0.** El índice local de
    cargo-dist no tiene la 0.33.1 que fija la CI; con la 0.32.0 el plan sigue
    publicando solo `magi-ssh` con sus cuatro objetivos. La versión de CI no
    cambia.
13. **Contrato UniFFI ampliado.** Se añaden
    `Nucleo.pegado_entre_corchetes(sesion)` (para el pegado entre corchetes
    del terminal) y `FirmanteMovil.olvidar_clave()` (rotación de la clave
    del móvil), sin cambiar nada de lo existente.
15. **Sin «setUnlockedDeviceRequired» en las claves del Keystore
    (corrección tras probar en móvil real).** En un Honor MagicOS se reprodujo
    el fallo conocido de Android (issue 506989112): con claves marcadas con
    «setUnlockedDeviceRequired», Android devuelve «UserNotAuthenticatedException»
    aunque el teléfono esté desbloqueado si se desbloqueó con reconocimiento
    facial (biometría no fuerte). Impedía unir la sincronización y habría roto
    la firma de sesiones hasta forzar la parada y desbloquear con PIN. Las
    claves se crean ahora sin ese requisito (siguen en TEE/StrongBox y no son
    exportables); la protección equivalente la da el bloqueo propio de MAGI
    (§4.1 y §7.8) y el mensaje de «teléfono bloqueado» se conserva para los
    casos reales. Las instalaciones existentes migran una sola vez: el llavero
    re-cifra sus secretos con una clave nueva («secretos-v2») y la clave del
    móvil se recrea con otro alias, lo que cambia la identidad y exige
    reinstalarla en los hosts (la interfaz lo avisa). Requiere actualizar §7.8
    y D174/D176 del informe y validarlo en el plegable.
16. **Terminal a pantalla completa al abrir una sesión.** A petición del
    desarrollador tras probar en el plegable: mientras el detalle es el
    terminal se ocultan la barra inferior, el riel y la lista del plegable, y
    las barras del sistema se esconden (se muestran con un deslizamiento) para
    que el terminal ocupe todo el ancho y todo el alto. Al volver se restaura
    la disposición normal, incluida la de dos paneles. Sustituye en ese caso a
    la disposición de dos paneles que describe el informe §6.4.
14. **Estado del árbol al cerrar la ronda.** Los commits `419ebe1` y
    `78e2cc2` dejan el núcleo y la app completos; en el momento de esta
    instantánea queda sin commit la ronda de correcciones de la revisión
    adversarial (44 ficheros) y algún fichero nuevo de pruebas. Los pendientes
    reales son externos al código: validación en el plegable, CI de GitHub,
    publicación con keystore y la corrección documental de §11.9.

## Estructura de archivos creada/modificada

Raíz del repositorio:

- `Cargo.toml`: `[workspace]` con los miembros `crates/magi-nucleo` y
  `crates/magi-movil`, y dependencia por ruta a `magi-nucleo`. El paquete
  `magi-ssh` conserva nombre, versión y `dist`. `Cargo.lock` está al día
  en el commit; el árbol de trabajo lo tiene modificado por la ronda de la
  revisión adversarial.
- `src/lib.rs`: reexporta los módulos del núcleo que el escritorio sigue
  usando con las rutas antiguas.
- `src/teclas.rs`: traduce los eventos de crossterm y delega la codificación
  en la tabla común del núcleo (§7.6).
- `src/servidor/tuneles.rs`, `src/servidor/conexiones.rs` y
  `src/servidor/mod.rs`: usan la mecánica y el cliente del núcleo.
- `AGENTS.md`, `CLAUDE.md` y `docs/fase11/CLAUDE.md`: rutas nuevas de los
  módulos compartidos.

Núcleo compartido:

- `crates/magi-nucleo/Cargo.toml` y `crates/magi-nucleo/src/`: `lib.rs`,
  `rutas.rs`, `modelo.rs`, `configuracion.rs`, `trabajo_vivo.rs`,
  `llavero.rs`, `registro.rs`, `ficheros.rs`, `instalar_clave.rs` y
  `deliberacion.rs`; `almacen/` (migraciones, hosts, grupos, identidades,
  túneles, snippets, verificaciones, sondeos, equipos, lápidas, conflictos,
  sincronizaciones, copias, versiones, deliberaciones, registro y
  `movil.rs`); `sincronizacion/` (formato, cifrado, fusión, aplicación,
  elementos, huellas, reloj, carpeta, pasada común y `lector.rs` con
  `CarpetaSincronizacion`); `conexion/` (cliente, saltos, huellas, reenvíos,
  terminal y `abrir_con_plazo`); `tuneles/` (mod, escucha, local y socks5);
  `terminal/` (mod, tramos y teclas); `claves/` (mod, común, DER a SSH,
  X.509 a OpenSSH y firmante); `snippets/` (mod, salida y variables);
  `scripts/instalar_clave.sh`; `tests/` (`instalar_clave.rs` con 11
  pruebas, `sincronizacion_lector.rs` con 8 y
  `generar_fixture_android.rs`, que regenera la fixture de la prueba
  instrumentada).
- `crates/magi-movil/`: `Cargo.toml`, `uniffi.toml` y
  `src/{lib,api,clave,estado,escritor,gestor,inventario,sincro,dialogos,retrollamadas}.rs`
  más `src/bin/uniffi-bindgen.rs`. La ronda de correcciones añade pruebas en
  `escritor.rs`, `dialogos.rs` y `sincro.rs` y actualiza la documentación
  de los módulos (B6).

Android:

- `android/`: `settings.gradle.kts`, `build.gradle.kts`,
  `gradle.properties`, `version.properties`, `CHANGELOG.md`,
  `.gitignore`; `gradle/libs.versions.toml`,
  `gradle/verification-metadata.xml` y wrapper (`gradlew`,
  `gradlew.bat` y `gradle/wrapper/...` con `distributionSha256Sum`);
  `app/build.gradle.kts`, `app/lint.xml` y `app/proguard-rules.pro`.
- `android/app/src/main/`: `AndroidManifest.xml`; `kotlin/org/cuatrodetres/magi/`
  (`MagiApp`, `MainActivity`, `nucleo/` con `NucleoRepositorio`,
  `AdaptadoresPlataforma`, `AdaptadorOyenteNucleo`, `Conversiones` y
  `FuenteNucleoReal`; `plataforma/` con SAF, Keystore, clave P-256,
  bloqueo, servicio, batería, `Portapapeles` y las reglas; `preferencias/`;
  `navegacion/`; `ui/` completa con hosts, ficha, sesiones, terminal,
  túneles, ajustes, diálogos, asistente, bloqueo, componentes, modelos,
  `vistaModelo/`, `fuente/` con `AvisosPlataformaReal` y
  `PantallaEsqueleto`); `res/` (textos, temas y colores, fuentes JetBrains
  Mono, iconos, `xml/reglas_extraccion.xml` y la licencia OFL). Los bindings
  generados por UniFFI no se versionan
  (`app/build/generated/uniffi/.../magi_movil.kt`).
- `android/app/src/test/`: 17 clases con 126 pruebas JVM (contratos, tema,
  modelos, vista-modelo, plataforma, preferencias y fuente real).
- `android/app/src/androidTest/`: `PruebaHumo` y `PruebaHumoNucleo`
  (carga el núcleo, importa la fixture real y valida el Keystore), con la
  fixture en `androidTest/assets/sincronizacion/magi-aaaaaaaa-…age`.

CI, publicación y documentación:

- `.github/workflows/ci.yml` y `.github/workflows/release.yml`
  (modificados); `.github/workflows/android.yml` y
  `.github/workflows/android-release.yml` (nuevos);
  `.github/actionlint.yaml` (nuevo).
- `docs/publicar-android.md` (nuevo, commit `419ebe1`),
  `android/CHANGELOG.md` (nuevo), `README.md` (modificado, sección «App
  Android») y este `docs/fase11/magi-fase11-implementacion.md`.

### Corrección 11b (4 nov 2026)

- NUEVO crates/magi-movil/tests/tunel_local_e2e.rs (E2E Rust del túnel local).
- NUEVO crates/magi-movil/examples/servidor_pruebas_tunel.rs (servidor SSH,
  eco y canal de control para tirar las conexiones; puertos 12222, 19091 y
  12223).
- NUEVO crates/magi-nucleo/tests/generar_fixture_tunel_android.rs y
  android/app/src/androidTest/assets/tunel/magi-bbbbbbbb-cccc-4ddd-8eee-ffffffffffff.age
  (fixture del E2E con host 10.0.2.2:12222 y túnel 127.0.0.1:9090 → 19091).
- NUEVO android/app/src/androidTest/kotlin/org/cuatrodetres/magi/PruebaTunelE2E.kt.
- MODIFICADO crates/magi-movil/src/gestor.rs (vigilante de conexión, caída de
  túneles, puerto efectivo, contadores), crates/magi-movil/src/api.rs
  (TunelVivo), crates/magi-nucleo/src/conexion/reenvios.rs (hora del último
  error) y la interfaz de túneles (Conversiones, ModelosTuneles, FilaTunel,
  PantallaTuneles, AppMagi, VistaModeloMagi, strings.xml).

## Decisiones técnicas tomadas durante el desarrollo

- **Un crate núcleo único y el paquete raíz intacto.** `magi-ssh` sigue en la
  raíz para no tocar `dist`, el nombre del paquete, `build.rs`, `assets/`
  ni los tests; lo compartido vive en `magi-nucleo` (`publish = false`,
  fuera de dist) y lo móvil en `magi-movil` (también `publish = false`).
  Las rutas de importación del escritorio se conservan con reexportaciones.
- **El núcleo no conoce el entorno.** `Rutas` y `Equipo` se construyen
  fuera (el escritorio las descubre con `directories` y el móvil usa su
  almacenamiento privado) y se inyectan; los servicios de plataforma son
  traits (`LlaveroSecreto`, `Firmante`, `TrabajoVivo`,
  `CarpetaSincronizacion`) y la pasada recibe su política de borrados y el
  gancho de trabajo vivo. Comprobado en el árbol: `magi-nucleo` no menciona
  `directories`, `HOME` ni `XDG_`.
- **Una sola fuente de versión para la app.** `android/version.properties` es
  la fuente; Gradle deriva `versionName` y `versionCode` con la fórmula
  §7.10 y `android-release.yml` recalcula lo mismo y lo comprueba sobre el
  APK con `aapt2`, además de exigir que la etiqueta coincida exactamente con
  el fichero. Doble comprobación a propósito: un fallo de fórmula rompería las
  actualizaciones, no solo la release.
- **Firma solo por el workflow de release.** `app/build.gradle.kts` crea la
  `signingConfig` de release únicamente si existen las cuatro variables
  `MAGI_KEYSTORE_*`; el keystore llega como base64 a `$RUNNER_TEMP`, se
  comprueba con `keytool` y no se imprime. Esquemas v2 y v3, v1 desactivado
  (minSdk 30). En local, sin esas variables, `assembleRelease` compila pero no
  firma: no debe publicarse.
- **Etiquetas de Android separadas del escritorio.** `release.yml` (dist)
  excluye `android-v*`; `android-release.yml` es el único que firma y
  publica la app; `android.yml` verifica los tags con la CI normal, incluida
  la prueba de humo.
- **Release de Android nunca «Latest».** `gh release create --latest=false` y
  comprobación posterior de `isLatest`; las `-rc.N` se marcan con
  `--prerelease`. Las notas incluyen la huella SHA-256 del certificado (en
  mayúsculas y con dos puntos) y las órdenes de verificación con `apksigner`
  y `gh attestation verify`.
- **Herramientas descargadas, fijadas por SHA-256.** cargo-ndk 4.1.2 se
  descarga como binario musl y se comprueba su SHA-256 antes de instalarlo. El
  wrapper de Gradle lleva `distributionSha256Sum` y las dependencias de
  Gradle están en `verification-metadata.xml`.
- **CI de Android pragmática.** Se instala el NDK r30 explícitamente (lo
  necesitan el paso de cargo-ndk y Gradle) con doble sintaxis de
  `sdkmanager`; la plataforma y las build-tools los resuelve AGP. La prueba
  de humo exige KVM de verdad: regla udev, comprobación de `/dev/kvm`,
  `emulator -accel-check`, `-accel on` y perfil `pixel_5` (el mismo AVD
  probado en local).
- **Entorno: aws-lc-rs por defecto, sin `ring`.** La prueba de compilación
  cruzada cubrió rusqlite bundled, russh 0.63, age y la cripto con las features
  por defecto; R91 queda retirado y no se activa la feature `ring` de russh.
- **App Android: AGP 9 con Kotlin integrado.** No se aplica
  `org.jetbrains.kotlin.android` (AGP 9 lo prohíbe); el proyecto aplica el
  plugin de Compose y fija Kotlin 2.4.21 en el build raíz y el catálogo. Lint
  silencia solo tres avisos justificados (iconos adaptativos, licencia OFL
  todavía sin pantalla que la lea y JNA como `aar`).
- **Un contrato compilable antes de la lógica.** `crates/magi-movil` y las
  interfaces de `FachadaPlataforma` fijan nombres, paquete Kotlin, cdylib y
  tareas de Gradle antes de implementar nada, para que las pistas de núcleo,
  plataforma y UI puedan avanzar en paralelo sin reescribirse (regla «antes de
  lanzar pistas en paralelo, fija un contrato compilable»).

- **Delegar las teclas en la tabla común mejora §7.6.** El escritorio ya no
  codifica por su cuenta: traduce los eventos de crossterm y el núcleo emite
  `CSI 1;m` con el modificador para Ctrl/Mayús/Alt+flecha (ver desviación
  9). La API y las pruebas del escritorio no cambian.
- **La rotación de la clave del móvil no añade tablas.** La clave anterior se
  guarda con su fecha dentro del secreto cifrado del llavero
  (`sincronizacion-anterior`); el llavero ya servía secretos opacos y la
  comprobación de los 30 días es una lectura más.
- **En los borrados masivos del móvil manda el informe de la fase.** §7.3 pide
  aplicarlos sin copia ni confirmación (el móvil no escribe la carpeta); se
  implementa así y se anota corregir `docs/formato-sincronizacion.md` §11.9,
  que pedía una red de seguridad.
- **`dist` local con 0.32.0 y CI con 0.33.1.** La validación local se hizo
  con la versión disponible en el índice; el workflow del escritorio no cambia.
- **Ampliación del contrato UniFFI.** `pegado_entre_corchetes` y
  `olvidar_clave` se añaden sin cambiar lo existente; los bindings Kotlin se
  regeneran solos en cada compilación.
- **Supresión puntual del `NewApi` de los bindings generados.** UniFFI genera
  una comprobación con `java.lang.ref.Cleaner` (API 33); con minSdk 30, el
  build añade la supresión solo al fichero generado, no al resto de la app.
- **La revisión adversarial es la última puerta, y se corrige la causa.** El
  crítico C1 (clave del móvil sin crear → toda conexión fallaba) se validó con
  una prueba instrumentada real de Keystore; el defecto de fondo era usar
  `KeyGenerator` para una clave EC, que lanza `NoSuchAlgorithmException` en
  AndroidKeyStore, y se corrigió a `KeyPairGenerator`.
- **Las esperas de la cadena móvil llevan plazo.** Handshake SSH, apertura de
  canal, PTY, `exec` y lectura del script de instalación dejaron de esperar sin
  límite (A3); el cierre del núcleo anula la fachada (B7) para que ninguna
  llamada tardía toque servicios ya cerrados.
- **Estado de UI y peticiones sin conflación.** Peticiones y pantallas usan
  `Channel(UNLIMITED)` y no se borran peticiones (M2); la entrada del terminal se
  serializa (M7) y los comandos durante «abriendo» se guardan y se aplican al
  PTY cuando existe, cerrando antes de abrir (A2).
- **La carpeta SAF se elige una vez al arrancar.** En vez de depender de volver a
  leer el permiso persistente tras reiniciar, el arranque llama una vez a
  `elegirCarpeta` (A4); «borrar todo» deja SAF, clave recreada y caché
  coherentes (M5).
- **La pasada corre en el runtime del núcleo.** Una pasada cancelada ya no deja
  la fase en `SINCRONIZANDO`: se ejecuta en el runtime del núcleo y la fase se
  restaura siempre (A5). Los túneles se paran y activan con limpieza y
  comprobación+inserción atómicas (M3/M4).
- **La prueba instrumentada con fixture es el contrato de integración.** El AVD
  `magi-api30` (API 30, x86_64, PIN 1234) carga la cdylib, importa un `.age`
  real, pinta el inventario por la API y crea la clave del móvil en el Keystore
  real; la fixture se regenera con
  `cargo test -p magi-nucleo --test generar_fixture_android -- --ignored`.

### Corrección 11b (4 nov 2026)

- El vigilante de la conexión del host vive en el gestor del móvil y se apoya
  en Handle::is_closed del cliente SSH; no cambia nada del escritorio.
- Se amplía el contrato UniFFI de forma aditiva (aceptadas, ultimo_error y
  ultimo_error_en en TunelVivo) y se regeneran los bindings.
- El E2E Android depende de un servidor de pruebas en el anfitrión; si no está,
  se omite con Assume en vez de fallar, para no romper la CI.

## Funcionalidades del checklist completadas

Copia literal de las 110 funcionalidades marcadas `[x]` en la checklist al
cerrar la fase (110/110 y 823/827 en el total del proyecto). Los criterios de
aceptación de cada bloque están en la checklist; los pendientes externos a la
implementación (validación manual en el plegable, CI de GitHub y publicación)
están en «Pendientes y bloqueos».

**Núcleo compartido (workspace)**

- [x] Workspace de Cargo con el paquete `magi-ssh` en la raíz y los miembros `crates/magi-nucleo` y `crates/magi-movil` (`publish = false`, fuera de `dist`)
- [x] `magi-nucleo` reúne, movidos sin reescribir, el modelo, el almacén con sus migraciones, la sincronización (formato, cifrado, fusión, aplicación, huellas, reloj), la conexión SSH (saltos, huellas, autenticación), la mecánica de los túneles (local, remoto, SOCKS5), el parser de terminal y `registro::anotar`
- [x] El escritorio depende de `magi-nucleo` y no cambia su comportamiento, su CLI, el protocolo v6 ni `magi.db` (`user_version` 7)
- [x] El núcleo no lee el entorno ni calcula rutas: recibe al iniciarse la carpeta de datos, la ruta de `known_hosts`, la de temporales y el nombre y la plataforma del equipo (en el móvil, el modelo y `android`)
- [x] Servicios inyectables en el núcleo: `CarpetaSincronizacion` (listar y leer; escribir y borrar solo en el escritorio), `LlaveroSecreto`, `Firmante` (firma con una clave que no está en memoria) y `TrabajoVivo`
- [x] Lo que es solo del escritorio (`magi_config`, `~/.ssh/config`, vigilancia con `notify`, copias de `magi.db`, servidor de sesiones y protocolo, ratatui y tui-term, `secret-tool` y Keychain, QR a medios bloques) se queda en `magi-ssh` y se engancha al núcleo desde fuera
- [x] Pasada de sincronización en modo lector: nunca exporta ni escribe en la carpeta, no hace copia de `magi.db`, no pide confirmación de borrados masivos y solo guarda conflictos de tipo `huella` y `borrado`
- [x] Conversión de la firma DER del Keystore al formato SSH `ecdsa-sha2-nistp256` (r y s como mpint) y de la clave pública X.509 a una línea OpenSSH
- [x] Pantalla del terminal hacia la app como tramos de texto con el mismo estilo, solo de las filas que cambian y a 30 fps como máximo
- [x] Codificación de teclas en el núcleo: Ctrl, Alt, flechas en modo normal y en modo aplicación, F1-F12, Inicio, Fin, RePág, AvPág, Supr y pegado entre corchetes

**Interfaz del núcleo (UniFFI)**

- [x] `magi-movil` es una cdylib con UniFFI fijado en `=0.32.2` (el mismo en el generador de bindings), con su propio runtime de tokio y un único hilo escritor de SQLite
- [x] `Nucleo::iniciar(rutas, equipo, retrollamadas)` y `Nucleo::version()` (app, núcleo, `formato` de sincronización y `user_version`)
- [x] `inventario_grupos()`, `inventario_hosts(filtro)`, `ficha_host(uuid)` y `plegar_grupo(uuid, plegado)`
- [x] `sincronizacion_estado()`, `elegir_carpeta(uri)`, `unir_con_clave(texto)`, `sincronizar_ahora()` y `dejar_de_sincronizar()`
- [x] `conflictos_huella()`, `aceptar_huella_de_otro_equipo(id)` y `mantener_mi_huella(id)`
- [x] `abrir_sesion(host_uuid)`, `escribir`, `tecla`, `pegar`, `redimensionar`, `desplazar`, `texto_seleccion`, `reconectar` y `cerrar_sesion`
- [x] `tuneles(host_uuid)` con estado en vivo, `activar_tunel`, `parar_tunel`, `relanzar_tunel` y `activar_en_puerto_libre`
- [x] `responder_dialogo(id, respuesta)` para huella nueva, huella cambiada y contraseña
- [x] `clave_publica_movil()` (línea OpenSSH y huella SHA256), `instalar_clave_en_host(host_uuid)`, `olvidar_contrasena(host_uuid)` y `borrar_datos(alcance)`
- [x] Retrollamadas que implementa Kotlin: `CarpetaMovil` (SAF), `LlaveroMovil` (Keystore), `FirmanteMovil` (clave del móvil) y `OyenteNucleo` (pantallas, listas completas de sesiones y túneles, diálogos y estado de la sincronización)

**Proyecto Android**

- [x] Proyecto Gradle en `android/` (Kotlin, Jetpack Compose, Material 3) con `applicationId` `org.cuatrodetres.magi`, `minSdk` 30, `targetSdk` 37, versiones fijadas en el catálogo y wrapper con `distributionSha256Sum`
- [x] Gradle compila el núcleo con `cargo-ndk` y el NDK r30 para `arm64-v8a` (release) y además `x86_64` (debug y emulador), y genera los bindings de Kotlin en cada compilación
- [x] Verificación de dependencias de Gradle con SHA-256 (`gradle/verification-metadata.xml`)
- [x] Permisos: solo `INTERNET`, `CAMERA` (pedido al escanear), `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `USE_BIOMETRIC` y `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; ningún permiso de almacenamiento
- [x] Sin copia de seguridad en la nube: `allowBackup="false"` y reglas de extracción de datos que lo excluyen todo
- [x] Sin Google Play Services, Firebase, ML Kit ni analítica

**Seguridad del móvil**

- [x] Clave del móvil ECDSA P-256 creada en el Android Keystore al primer arranque (StrongBox si existe; si no, TEE), no exportable y usable solo con el teléfono desbloqueado
- [x] Sin bloqueo de pantalla configurado en el teléfono, MAGI explica que lo necesita y no continúa
- [x] Bloqueo de la app con huella o PIN del teléfono (BiometricPrompt, biometría fuerte o credencial) al abrirla en frío y al volver tras 5 minutos fuera
- [x] Con la app bloqueada siguen vivas las sesiones y los túneles, y tocar la notificación lleva primero al desbloqueo
- [x] `LlaveroMovil` guarda la clave de sincronización, la anterior y las contraseñas de host (por `uuid` del host) cifradas con AES-GCM y una clave del Keystore; nunca en `magi.db`, DataStore ni logcat
- [x] Las pantallas con secretos (escanear o pegar la clave, contraseña) llevan `FLAG_SECURE`, y en Android 13 o superior la vista previa de recientes sale vacía

**Sincronización (lector)**

- [x] Asistente de primer arranque: bloqueo, clave del móvil creada, elegir carpeta y unirse
- [x] Elegir la carpeta con el selector del sistema (`ACTION_OPEN_DOCUMENT_TREE`) con permiso persistente solo de lectura
- [x] Una carpeta sin ficheros de MAGI se rechaza con «crea la sincronización en el escritorio primero»
- [x] Unirse escaneando el QR que enseña el escritorio (CameraX + ZXing) o pegando la clave; se valida descifrando un fichero y comprobando su firma, y tras pegarla se vacía el portapapeles
- [x] `CarpetaMovil` lista y lee por SAF solo `magi-<uuid>.age` con uuid canónico en minúsculas y hasta el tamaño máximo; ignora copias en conflicto y subcarpetas (T81)
- [x] Pasada al abrir la app, al volver a ella, cada 60 s mientras está visible y al deslizar hacia abajo en Hosts
- [x] Borrados de hosts o túneles con trabajo vivo en el móvil (sesiones, túneles activos o un salto en uso) se aplazan y se aplican solos en la primera pasada sin trabajo vivo, que relee todos los ficheros
- [x] Huellas que llegan de otros equipos: se añaden si el host no tiene ninguna en el móvil; si tiene otra, conflicto de huella que se resuelve en el móvil (aceptar la del otro equipo o mantener la mía)
- [x] `esperando_clave` como aviso no bloqueante («un fichero no abre con la clave: si la cambiaste en el escritorio, escanea la nueva»); el móvil sigue intentándolo y vuelve solo a `activa` si el fichero vuelve a abrir
- [x] Pantalla Sincronización: carpeta, huella corta de la clave, estado, última pasada y equipos con plataforma, versión, último fichero y estado
- [x] «Volver a escanear la clave»: la nueva pasa a ser la actual y la anterior solo lee durante 30 días, como en la F10
- [x] «Dejar de sincronizar» con confirmación: borra la clave y el inventario del móvil (exige cerrar antes sesiones y túneles) y conserva la clave del móvil, las contraseñas y las huellas
- [x] Indicador `⇅` en la barra de Hosts (al día, sincronizando, clave nueva, error) con texto además del color

**Inventario (solo lectura)**

- [x] Hosts por grupos plegables (el plegado es propio del móvil y no viaja), con «sin grupo» al final
- [x] Fila de host con glifo de estado (`●N` sesiones, `◐` conectando, `○`, `✕` último error), nombre, `usuario@dirección:puerto` y etiquetas; `ultimo_estado` y `ultima_conexion_en` se guardan en el móvil como campos propios
- [x] Búsqueda por subcadena en nombre, dirección, usuario y etiquetas
- [x] Ficha de host de solo lectura: dirección, puerto, usuario, grupo, etiquetas, salto, keepalive, identidad en el escritorio, cómo entra el móvil, huellas conocidas en el móvil y si hay contraseña guardada en este móvil
- [x] Botón de conexión en cada fila y «Abrir otra sesión» en la ficha
- [x] Ninguna pantalla permite crear, editar ni borrar hosts, grupos, identidades o túneles
- [x] Estado vacío «Aún no hay inventario» con acceso al asistente de sincronización

**Conexión y autenticación**

- [x] Autenticación en cadena: clave del móvil, luego contraseña guardada en el móvil, luego diálogo de contraseña (3 intentos, «recordar en este móvil»)
- [x] Diálogo de contraseña enmascarado; la contraseña viaja al núcleo como bytes que se borran tras usarla
- [x] Huella desconocida: diálogo con tipo y SHA256; aceptarla la guarda en el `known_hosts` del móvil y queda en REGISTRO
- [x] Huella cambiada: bloqueo en rojo con la anterior y la nueva; sustituirla exige escribir el nombre del host y queda en REGISTRO
- [x] Saltos (`ProxyJump`) en cadena hasta 3 niveles, cada uno con su huella y su autenticación
- [x] Keepalive del host o, si no tiene, 30 s en el móvil
- [x] Una sola conexión por host compartida por sus sesiones y túneles en el móvil (`multiplexar` no se aplica)
- [x] Errores con motivo legible (no se resuelve, plazo de 10 s, rechazo, autenticación rechazada, teléfono bloqueado) y `ultimo_estado` = error

**Clave del móvil**

- [x] Ajustes > Clave del móvil: línea OpenSSH, huella SHA256, dónde vive (StrongBox o TEE) y fecha de creación
- [x] Copiar y compartir la clave pública (hoja de compartir de Android) y mostrarla como QR
- [x] «Instalar mi clave en este host»: entra con contraseña, añade la clave a `~/.ssh/authorized_keys` con un script embebido y comprueba que la clave entra
- [x] El script de instalación crea `~/.ssh` (700) y `authorized_keys` (600) si faltan, no toca otras líneas, ejecuta `restorecon` si existe y recibe la clave como argumento validado, nunca interpolada; deja `clave_instalada` en REGISTRO
- [x] «Olvidar contraseña» en la ficha, con confirmación

**Sesiones y terminal**

- [x] Varias sesiones a la vez, también al mismo host (`host (n)`), listadas en Sesiones y con conmutador en la barra del terminal
- [x] Estados de sesión: abriendo (resolviendo, conectando, verificando huella, autenticando), abierta, caída con la pestaña conservada y «Reconectar», y eliminada al salir con `exit`
- [x] Cerrar una sesión pide confirmación, y «atrás» desde el terminal vuelve sin cerrarla
- [x] Terminal pintado en Compose con JetBrains Mono, colores de 16, 256 y 24 bits (paleta de §6), negrita, subrayado, inverso y cursor de bloque ámbar
- [x] Teclado del sistema en modo bruto (sin autocorrección ni sugerencias) que escribe en el remoto
- [x] Fila deslizable de teclas extra sobre el teclado: Esc, Tab, Ctrl, Alt, ←, ↑, ↓, →, `|`, `~`, `/`, `-`, Inicio, Fin, RePág, AvPág y F1-F12
- [x] Ctrl y Alt de la fila: una pulsación vale para la tecla siguiente y dos lo fijan hasta volver a tocarlo, con el estado visible
- [x] Teclado físico: Ctrl, Alt, Esc, flechas, F1-F12, Inicio, Fin, RePág, AvPág y Supr; Ctrl+Mayús+C copia y Ctrl+Mayús+V pega
- [x] Pellizcar cambia el tamaño de letra (8-24 sp), que se recuerda, y redimensiona el remoto
- [x] Deslizar arriba y abajo recorre el historial (1 000 líneas); al escribir vuelve al final
- [x] Mantener pulsado selecciona por palabras con tiradores y «Copiar»; sin selección ofrece «Pegar»
- [x] Pegar varias líneas sin pegado entre corchetes pide confirmación («se ejecutarán en el shell»)
- [x] El remoto recibe el tamaño nuevo al girar, plegar o desplegar, o al abrir y cerrar el teclado (agrupado 50 ms; mínimo 2×1)

**Segundo plano**

- [x] Servicio en primer plano `specialUse` con notificación «MAGI · N sesiones · M túneles» mientras haya alguna sesión o túnel; se para con el último
- [x] La notificación solo abre la app: no tiene acciones que cierren nada (T6)
- [x] Guía para sacar MAGI del ahorro de batería (petición del sistema y pasos de MagicOS), ofrecida al abrir la primera sesión y siempre desde Ajustes
- [x] Si Android cerró MAGI con sesiones abiertas, al volver lo dice y ofrece la guía de batería
- [x] Sin sesiones ni túneles, MAGI no hace ningún trabajo en segundo plano

**Túneles**

- [x] Pantalla Túneles con los túneles del inventario agrupados por host y bloque «Túneles» en la ficha: tipo, escucha → destino, automático, estado (`○` `◐` `●` `✕`), conexiones y bytes
- [x] Activar y parar un túnel con un interruptor; uno caído muestra su error y «Relanzar»
- [x] Túneles local, remoto y dinámico (SOCKS5, solo CONNECT) con la misma mecánica que el escritorio
- [x] En el móvil todo túnel local o dinámico escucha en `127.0.0.1`, diga lo que diga el inventario, con aviso en la fila si la dirección era otra
- [x] Puerto ocupado o menor que 1024: error con «Abrir en un puerto libre», que muestra el puerto asignado y no lo guarda
- [x] Automáticos: se activan con la primera sesión del host y se paran con la última; los manuales siguen
- [x] «Abrir en el navegador» en un túnel local activo (`http://127.0.0.1:<puerto>`)
- [x] Los túneles activos cuentan para el servicio en primer plano y mantienen viva la conexión del host

**Interfaz y estética**

- [x] Tema oscuro de la cabina de MAGI sobre Material 3: paleta fija (`#FFA000`, `#00FF6A`, `#FF2A1A`, `#5A5A5A`, `#EDE8DC` sobre `#0A0A0A`) y JetBrains Mono en toda la app
- [x] Barra inferior Hosts · Sesiones · Túneles · Ajustes en pantalla compacta
- [x] Pantalla plegable o ancha: riel de navegación y dos paneles (lista y ficha o terminal); al desplegar, la sesión abierta sigue en el panel derecho
- [x] Glifo y texto además del color en todos los estados (T5), descripciones para TalkBack y objetivos táctiles de 48 dp
- [x] Indicadores de carga (conectando, sincronizando) y mensajes de error con instrucción en todas las pantallas
- [x] Ajustes: Sincronización, Clave del móvil, Terminal (tamaño de letra por defecto), Batería, Borrar datos y Acerca de
- [x] «Borrar todos los datos de MAGI» con confirmación escribiendo «borrar»: inventario, clave de sincronización, contraseñas, huellas y clave del móvil
- [x] Acerca de: versión de la app y del núcleo, `formato` de sincronización, `user_version`, licencia y pantalla Licencias generada en la compilación (dependencias Kotlin, crates Rust y la OFL de JetBrains Mono)

**Calidad, CI y publicación**

- [x] `ci.yml` pasa a `--workspace`: tests, clippy y `cargo deny` cubren `magi-nucleo` y `magi-movil`
- [x] `android.yml`: compila el núcleo para Android, ejecuta `lint` y las pruebas de Kotlin en la JVM (con el núcleo compilado para el anfitrión) y genera el APK de depuración
- [x] Prueba de humo en el emulador (API 30, x86_64) en `main` y en las etiquetas `android-v*`
- [x] `release.yml` (dist) no se dispara con las etiquetas `android-v*`
- [x] `android-release.yml` con la etiqueta `android-vX.Y.Z`: APK `arm64-v8a` firmado (esquemas v2 y v3), `apksigner verify`, `.sha256`, atestación y GitHub Release que nunca se marca como «Latest»; con `-rc.N`, prerelease
- [x] La firma usa un keystore que solo vive en los secretos de ese workflow (`ANDROID_KEYSTORE_BASE64`, sus contraseñas y el alias); las notas de la release incluyen la huella SHA-256 del certificado
- [x] `versionCode` derivado de la versión con la fórmula del informe (§7.10) y `versionName` igual a la versión
- [x] `android/CHANGELOG.md` (Keep a Changelog) y notas de la release tomadas de su sección
- [x] README con la sección «App Android» (Obtainium filtrando `android-v`, verificar el APK y su certificado, Syncthing-Fork en «solo recibir», qué hacer si pierdes el móvil) y `docs/publicar-android.md`
- [x] Pruebas del núcleo en el anfitrión: firma DER a SSH, modo lector (borrados masivos, aplazados y conflictos), codificación de teclas, tramos de pantalla, túnel en puerto libre y script de instalación con `sh` en un HOME temporal
- [x] Revisión adversarial de cierre (T31, T54) sobre el núcleo movido, `magi-movil` y la app, anotada en el informe de implementación

## Pendientes y bloqueos

1. **Validación manual en el plegable real.** Es lo único que el emulador no
   cubre: StrongBox/TEE, biometría, SAF tras reiniciar el móvil, MagicOS
   (batería y cierres), plegado/desplegado, teclado Bluetooth y cámara/QR. Se
   sigue el guion del informe §11. Incluye validar la corrección del
   desbloqueo facial (claves sin «setUnlockedDeviceRequired», desviación 15):
   en el Honor se comprobó el diagnóstico y el remedio manual; falta confirmar
   la compilación corregida, la migración de las claves existentes y que
   unir/conectar funcionan tras desbloquear con la cara.
2. **CI de GitHub sin ejecutar.** Los cinco workflows no se han ejecutado en
   Actions (solo actionlint y la verificación local). Falta comprobar el KVM del
   runner de humo y que AGP descargue la plataforma 37.0 y las build-tools en el
   runner, además de la descarga del wrapper y de cargo-ndk.
3. **Release Android sin ensayar.** Faltan el keystore, sus cuatro secretos (los
   crea el desarrollador), el ensayo con `android-v0.1.0-rc.1` y el APK release
   firmado; `android-release.yml` no se ha ejecutado nunca. La fecha de
   `[0.1.0]` en el CHANGELOG es provisional.
4. **Corrección documental pendiente.** `docs/formato-sincronizacion.md` §11.9
   («red de seguridad» para borrados masivos en Android) contradice el informe de
   la Fase 11 §7.3, que manda aplicarlos sin copia ni confirmación. Hay que
   corregir el documento de formato.
5. **Limitaciones heredadas.** R85-R87 (informe §7.13) siguen abiertas; anotar su
   impacto real en el móvil al cerrar.
6. **Cierre documental.** La checklist ya está marcada (110/110) y el total del
   proyecto en 823/827. Falta actualizar este informe con el resultado de la
   validación manual y las comprobaciones posteriores a la primera publicación.

8. **Checklist de la reanudación.** El prompt de la corrección 11b habla de
   109/117 funcionalidades; el checklist del repositorio sigue en 110/110
   (Fase 11 cerrada). Falta que el desarrollador traiga al repositorio la
   versión actualizada del analista.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

**Entorno de Android (máquina de desarrollo).** Antes de usar Gradle o
cargo-ndk:

    source /tmp/entorno-android.sh

Deja `ANDROID_HOME` y `ANDROID_SDK_ROOT` en `/home/hector/Android/Sdk`,
`ANDROID_NDK_HOME` en `.../ndk/30.0.16248370` y el `PATH` con
`cmdline-tools/latest/bin`, `platform-tools`, `emulator` y
`build-tools/36.0.0`. Requisitos verificados: SDK con la plataforma 37.0 y
build-tools 36/37, NDK r30, Gradle 9.8.2, JDK 26, cargo-ndk 4.1.2 y los objetivos
`aarch64-linux-android` y `x86_64-linux-android`. El AVD de la verificación es
`magi-api30` (API 30, x86_64, AOSP, PIN 1234).

**Escritorio.**

    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    TZ=UTC cargo test --workspace --locked
    cargo deny check

Las pruebas de pintado necesitan `TZ=UTC` y las de SFTP necesitan
`sftp-server` o `MAGI_SFTP_SERVER`. Las cuatro órdenes están en verde en la
verificación final. **Avisos:** `/tmp` es un tmpfs y puede llenarse (en esta
máquina llegó al 80 %); si `cargo test` falla con errores raros de SQLite o de
temporales, usa un directorio temporal fuera de `/tmp`:

    mkdir -p "$HOME/.cache/tmp"
    TMPDIR="$HOME/.cache/tmp" TZ=UTC cargo test --workspace --locked

Y en máquinas con cuota de disco o `/tmp` muy justa, las pruebas del escritorio
pueden necesitar ejecutarse de una en una:

    TMPDIR="$HOME/.cache/tmp" TZ=UTC cargo test --workspace --locked -- --test-threads=1

En la CI de GitHub no hace falta ninguna de las dos cosas.

**App Android.**

    cd android
    ./gradlew assembleDebug               # APK debug con las dos ABI y el núcleo
    ./gradlew lint                        # 0 errores, 14 avisos
    ./gradlew testDebugUnitTest           # 126 pruebas JVM, 0 fallos
    ./gradlew connectedDebugAndroidTest   # 3/3 instrumentadas en magi-api30 (API 30, x86_64, PIN 1234)
    ./gradlew :app:assembleRelease        # compila; sin firma si no hay MAGI_KEYSTORE_*

`assembleDebug` y `assembleRelease` compilan antes el núcleo con cargo-ndk y
generan los bindings Kotlin en
`app/build/generated/uniffi/kotlin/org/cuatrodetres/magi/nucleo/magi_movil.kt`
(no versionados). La release firmada se genera con las cuatro variables
`MAGI_KEYSTORE_*` (las pone la CI a partir de los secretos) en
`android/app/build/outputs/apk/release/app-release.apk`, solo `arm64-v8a`. La
fixture de la prueba instrumentada se regenera con
`cargo test -p magi-nucleo --test generar_fixture_android -- --ignored`.

**Integración continua y workflows.**

    actionlint .github/workflows/*.yml      # 0 errores (con .github/actionlint.yaml)

`android.yml` se ejecuta en cada PR que toca `crates/` o `android/`, en push
a `main` y en etiquetas `android-v*`. La prueba de humo (job `humo`) corre en
`ubuntu-latest` con KVM y el mismo emulador API 30 x86_64.
`android-release.yml` solo con etiqueta; el flujo completo (keystore, secretos,
ensayo `rc` y comprobaciones) está en `docs/publicar-android.md`.

**Verificación de un APK publicado.**

    sha256sum -c magi-android-v0.1.0.apk.sha256
    "$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --print-certs magi-android-v0.1.0.apk
    gh attestation verify magi-android-v0.1.0.apk --repo 4d3-org/magi

La huella del certificado debe coincidir con la de las notas de la release y con
la de versiones anteriores; el APK de release solo debe llevar `lib/arm64-v8a`.

**Qué se ha validado ya y qué no.** Hecho: entorno Android completo y compilación
cruzada con R91 retirado; núcleo con pruebas verdes (tramos, teclas, claves,
instalar clave y pasada lectora); `cargo
fmt`/`clippy`/`check`/`test --workspace --locked`/`deny` en verde;
`dist plan` en local (0.32.0) publicando solo `magi-ssh`; actionlint y sintaxis
de los 21 scripts `run:`; fórmulas de `versionCode` y coincidencia con
`version.properties`; extracción de notas del CHANGELOG; app Android completa con
`assembleDebug`, `lint` (0 errores), 126 pruebas JVM, 3/3 instrumentadas en el
emulador API 30 x86_64 (núcleo real, fixture `.age`, inventario pintado y clave
del móvil en el Keystore real) y `assembleRelease` compilando sin firma.
Pendiente de validar: la validación manual en el plegable, la CI de GitHub, el
ensayo de `android-release.yml` con keystore y la corrección documental de §11.9.

---

*Borrador actualizado el 2026-10-31 (23:36); se actualiza al cerrar cada
sesión.*

### Corrección 11b

- Rust, sin red externa: cargo test -p magi-movil --test tunel_local_e2e
  (servidor SSH en proceso y servicio de eco).
- Android instrumentado: arrancar antes
  cargo run -p magi-movil --example servidor_pruebas_tunel (SSH 0.0.0.0:12222
  y eco 127.0.0.1:19091) y ejecutar
  ./gradlew connectedDebugAndroidTest
  -Pandroid.testInstrumentationRunnerArguments.class=org.cuatrodetres.magi.PruebaTunelE2E
  (o la cadena completa; la prueba se omite si el servidor no está).
- Fixture del túnel: cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored
