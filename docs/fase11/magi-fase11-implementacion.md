# MAGI - Fase 11: App Android — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. No se
> regenera desde cero. La especificación de la fase está en
> [magi-fase11-informe.md](magi-fase11-informe.md) y el alcance verificable en
> [magi-fase11-checklist.md](magi-fase11-checklist.md).

**Estado de la sesión (borrador, instantánea del 2026-10-31 22:59):** fase
muy avanzada. El núcleo compartido está implementado y con pruebas en verde
(terminal por tramos, teclas §7.6, claves DER/X.509, instalar clave, pasada en
modo lector con `CarpetaSincronizacion`, túneles y conexión/autenticación del
móvil), y `crates/magi-movil` ya integra la lógica real sobre ese núcleo
(arranque con `Rutas`/`Equipo` inyectados, un único hilo escritor de SQLite,
inventario, sincronización lectora, clave del móvil, sesiones, terminal,
túneles e instalar mi clave), con el contrato UniFFI ampliado. En Android, la
cadena Gradle está validada (`assembleDebug` con los dos `.so`,
`versionCode` 1009, tarea de licencias) y la capa de plataforma está escrita;
la UI Compose y los adaptadores reales (`NucleoRepositorio`) están en curso.
`cargo check`, `clippy --workspace -D warnings`, `test --workspace
--locked`, `fmt --check` y `deny check` están en verde, y `dist plan`
sigue publicando solo `magi-ssh`. Faltan la UI y los adaptadores,
`assembleRelease`, la prueba de humo, la CI real, el ensayo de publicación y
la validación en el plegable; el checklist sigue sin marcar (0/110) hasta que
se cierre la fase.

## Resumen de lo implementado

### Workspace de Cargo y núcleo compartido (implementado; commit `ef4e18f`)

- Commits: `a5f1e1f` (workspace con el núcleo compartido), `82bff82`
  (mecánica de túneles y SOCKS5 al núcleo) y `ef4e18f` («Integra la lógica del
  núcleo móvil y el contrato UniFFI», 58 ficheros). El árbol conserva cambios
  sin commit de `magi-movil` (`gestor.rs`, `inventario.rs` y `sincro.rs`), un
  ajuste en `magi-nucleo/src/rutas.rs` y el proyecto Android aún sin
  versionar.
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

### Proyecto Gradle `android/` (cadena validada; plataforma y UI en curso)

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
- **UI Compose completa en curso** (Hosts, ficha, Sesiones, terminal, túneles,
  Ajustes, diálogos, asistente, bloqueo, componentes y modelos) y
  **adaptadores reales en curso** (`NucleoRepositorio`,
  `AdaptadoresPlataforma`, `AdaptadorOyenteNucleo`, `Conversiones` y
  `FuenteNucleoReal`). El manifest mantiene los permisos exactos del
  checklist, `allowBackup="false"` y las reglas de extracción.
- Problema conocido de los bindings: el Kotlin generado por UniFFI usa
  `java.lang.ref.Cleaner` (API 33) y el minSdk es 30; `app/build.gradle.kts`
  añade una supresión `NewApi` al fichero generado. Falta la pasada final de
  `lint` que deje el informe limpio (ver pendientes).

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
- La lógica se apoya en el núcleo compartido y en las pruebas Rust; falta
  ejercitarla en el emulador y en el móvil (ver pendientes).

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
  reinstalar la app. En el árbol está aún sin commit.
- `android/CHANGELOG.md` en formato Keep a Changelog, con `[Sin publicar]` y
  `[0.1.0]` (fecha provisional hasta publicar).
- `README.md`: sección «App Android» antes de «Atajos».
- `android/version.properties`: `version=0.1.0` con la fórmula en
  comentario; la leen Gradle y los workflows.

### Verificaciones de la sesión (2026-10-31 22:59)

- `cargo metadata --locked`: código 0 (lock al día).
- `cargo check --workspace` y
  `cargo clippy --workspace --all-targets -D warnings`: en verde.
- `TZ=UTC cargo test --workspace --locked`: en verde (con `TMPDIR` fuera
  de `/tmp` cuando el tmpfs está lleno; ver «Ejecución y pruebas»).
- `cargo fmt --all --check` y `cargo deny check`: en verde.
- `dist plan`: sigue publicando solo `magi-ssh` con sus cuatro objetivos,
  validado en local con dist 0.32.0 (el índice local no tiene 0.33.1; la CI
  sigue fijada a 0.33.1).
- `actionlint .github/workflows/*.yml`: 0 errores (con
  `.github/actionlint.yaml`).
- Gradle: `assembleDebug` con los dos `.so`, `versionCode` 1009,
  `verification-metadata` SHA-256 y tarea de licencias; la pasada final de
  `lint` está pendiente (ver pendientes).

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
7. **Checklist sin marcar (0/110).** No es un olvido: la regla de trabajo marca
   las casillas al cerrar la fase, y esta sigue en curso. Este informe es el
   borrador que se irá completando.
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
14. **Instantánea en movimiento.** El árbol tiene cambios sin commit de
    `magi-movil` y el proyecto Android sin versionar. Este documento describe
    el estado en el momento indicado arriba y se actualizará en cada sesión; no
    debe leerse como el estado final de la fase.

## Estructura de archivos creada/modificada

Raíz del repositorio:

- `Cargo.toml`: `[workspace]` con los miembros `crates/magi-nucleo` y
  `crates/magi-movil`, y dependencia por ruta a `magi-nucleo`. El paquete
  `magi-ssh` conserva nombre, versión y `dist`. `Cargo.lock` está al día
  (`cargo metadata --locked` termina con código 0).
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
  elementos, huellas, reloj, carpeta y la pasada común con
  `CarpetaSincronizacion`); `conexion/` (cliente, saltos, huellas,
  reenvíos, terminal y `abrir_con_plazo`); `tuneles/` (mod, escucha, local
  y socks5); `terminal/` (mod, tramos y teclas); `claves/` (mod, común,
  DER a SSH, X.509 a OpenSSH y firmante); `snippets/` (mod, salida y
  variables); `scripts/instalar_clave.sh`; `tests/`
  (`instalar_clave.rs` con 11 pruebas y `sincronizacion_lector.rs` con 8).
- `crates/magi-movil/`: `Cargo.toml`, `uniffi.toml` y
  `src/{lib,api,clave,estado,escritor,gestor,inventario,sincro,dialogos,retrollamadas}.rs`
  más `src/bin/uniffi-bindgen.rs`; lógica real (commit `ef4e18f` y
  ajustes sin commit en `gestor.rs`, `inventario.rs` y `sincro.rs`, más un
  ajuste en `magi-nucleo/src/rutas.rs`).

Android:

- `android/`: `settings.gradle.kts`, `build.gradle.kts`,
  `gradle.properties`, `version.properties`, `CHANGELOG.md`,
  `.gitignore`; `gradle/libs.versions.toml`,
  `gradle/verification-metadata.xml` y wrapper (`gradlew`,
  `gradlew.bat` y `gradle/wrapper/...` con `distributionSha256Sum`);
  `app/build.gradle.kts`, `app/lint.xml` y `app/proguard-rules.pro`.
- `android/app/src/main/`: `AndroidManifest.xml`; `kotlin/org/cuatrodetres/magi/`
  con `MagiApp.kt` y `MainActivity.kt`; `nucleo/`
  (`NucleoRepositorio`, `AdaptadoresPlataforma`,
  `AdaptadorOyenteNucleo`, `Conversiones` y `FuenteNucleoReal`);
  `plataforma/` (`CarpetaSaf`, `LlaveroKeystore`, `ClaveMovil`,
  `Bloqueo`, `ReglaBloqueo`, `PantallaSegura`, `ServicioSesiones`,
  `Bateria`, `ReglasBateria`, `ContenedorPlataforma` y las reglas de
  carpeta, llavero y servicio); `preferencias/PreferenciasMagi.kt`;
  `navegacion/` (`Destino`, `BarraNavegacion` y `Rutas`); `ui/`
  (`AppMagi`, Hosts, ficha, Sesiones, terminal, túneles, Ajustes, diálogos,
  asistente, bloqueo, componentes, modelos, `vistaModelo/`, tema y
  `PantallaEsqueleto`); `res/` (textos en castellano, temas y colores,
  fuentes JetBrains Mono, iconos, `xml/reglas_extraccion.xml` y la licencia
  OFL). Los bindings generados por UniFFI no se versionan
  (`app/build/generated/uniffi/.../magi_movil.kt`, ≈266 KB).
- `android/app/src/test/` (`PruebaContratoNucleo` y `PruebaPaleta`) y
  `android/app/src/androidTest/` (`PruebaHumo`, pendiente de ampliar al AC
  del checklist).

CI, publicación y documentación:

- `.github/workflows/ci.yml` y `.github/workflows/release.yml`
  (modificados); `.github/workflows/android.yml` y
  `.github/workflows/android-release.yml` (nuevos);
  `.github/actionlint.yaml` (nuevo), todo ello en el commit `ef4e18f`.
- `docs/publicar-android.md` (nuevo, aún sin commit en el árbol),
  `android/CHANGELOG.md` (nuevo), `README.md` (modificado, sección «App
  Android») y este `docs/fase11/magi-fase11-implementacion.md`.

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

## Funcionalidades del checklist completadas

Copia literal de los ítems que hoy se pueden considerar hechos (se marcarán
`[x]` en la checklist al cerrar la fase; el estado de cada uno se anota en
cursiva). El resto de los 110 ítems siguen pendientes y no se copian aquí.

**Núcleo compartido (workspace)**

- [ ] Workspace de Cargo con el paquete `magi-ssh` en la raíz y los miembros `crates/magi-nucleo` y `crates/magi-movil` (`publish = false`, fuera de `dist`)
  *Hecho: workspace con los tres crates (`publish = false`, fuera de dist),
  `magi-ssh` en la raíz y `dist` sin tocar; `cargo metadata --locked`,
  `check`/`clippy`/`test --workspace --locked` y `dist plan` validados
  en local (dist 0.32.0; la CI fija 0.33.1).*
- [ ] `magi-nucleo` reúne, movidos sin reescribir, el modelo, el almacén con sus migraciones, la sincronización (formato, cifrado, fusión, aplicación, huellas, reloj), la conexión SSH (saltos, huellas, autenticación), la mecánica de los túneles (local, remoto, SOCKS5), el parser de terminal y `registro::anotar`
  *Hecho: módulos movidos sin reescribir y `TZ=UTC cargo test --workspace
  --locked` en verde, incluidas las pruebas antiguas del escritorio.*
- [ ] El escritorio depende de `magi-nucleo` y no cambia su comportamiento, su CLI, el protocolo v6 ni `magi.db` (`user_version` 7)
  *Hecho: CLI, protocolo v6 y `magi.db` (user_version 7) sin cambios; la suite
  completa (pintado, integración y servidor) está en verde.*
- [ ] El núcleo no lee el entorno ni calcula rutas: recibe al iniciarse la carpeta de datos, la ruta de `known_hosts`, la de temporales y el nombre y la plataforma del equipo (en el móvil, el modelo y `android`)
  *Hecho: el núcleo recibe `Rutas`/`Equipo` y no menciona `directories`,
  `HOME` ni `XDG_` (comprobado en el árbol).*
- [ ] Servicios inyectables en el núcleo: `CarpetaSincronizacion` (listar y leer; escribir y borrar solo en el escritorio), `LlaveroSecreto`, `Firmante` (firma con una clave que no está en memoria) y `TrabajoVivo`
  *Hecho: traits `CarpetaSincronizacion`, `LlaveroSecreto`, `Firmante` y
  `TrabajoVivo`, con implementación de escritorio y de móvil.*
- [ ] Pasada de sincronización en modo lector: nunca exporta ni escribe en la carpeta, no hace copia de `magi.db`, no pide confirmación de borrados masivos y solo guarda conflictos de tipo `huella` y `borrado`
  *Hecho: pasada en modo lector, sin escribir en la carpeta, sin copia y sin
  confirmación para borrados masivos; 8 pruebas, incluida la de 10 hosts de 12.*
- [ ] Conversión de la firma DER del Keystore al formato SSH `ecdsa-sha2-nistp256` (r y s como mpint) y de la clave pública X.509 a una línea OpenSSH
  *Hecho: conversión y verificación real con `ssh-key` (4 y 3 pruebas).*
- [ ] Pantalla del terminal hacia la app como tramos de texto con el mismo estilo, solo de las filas que cambian y a 30 fps como máximo
  *Hecho: `tramos.rs` con estilos, colores y tope de 30 fps; 9 pruebas.*
- [ ] Codificación de teclas en el núcleo: Ctrl, Alt, flechas en modo normal y en modo aplicación, F1-F12, Inicio, Fin, RePág, AvPág, Supr y pegado entre corchetes
  *Hecho: tabla común §7.6 (10 pruebas); el escritorio delega en ella.*

**Interfaz del núcleo (UniFFI)**

Los diez ítems están implementados en `crates/magi-movil` (commit
`ef4e18f`); falta ejercitarlos en el emulador y en la app real (ver
pendientes).

- [ ] `magi-movil` es una cdylib con UniFFI fijado en `=0.32.2` (el mismo en el generador de bindings), con su propio runtime de tokio y un único hilo escritor de SQLite
- [ ] `Nucleo::iniciar(rutas, equipo, retrollamadas)` y `Nucleo::version()` (app, núcleo, `formato` de sincronización y `user_version`)
- [ ] `inventario_grupos()`, `inventario_hosts(filtro)`, `ficha_host(uuid)` y `plegar_grupo(uuid, plegado)`
- [ ] `sincronizacion_estado()`, `elegir_carpeta(uri)`, `unir_con_clave(texto)`, `sincronizar_ahora()` y `dejar_de_sincronizar()`
- [ ] `conflictos_huella()`, `aceptar_huella_de_otro_equipo(id)` y `mantener_mi_huella(id)`
- [ ] `abrir_sesion(host_uuid)`, `escribir`, `tecla`, `pegar`, `redimensionar`, `desplazar`, `texto_seleccion`, `reconectar` y `cerrar_sesion`
- [ ] `tuneles(host_uuid)` con estado en vivo, `activar_tunel`, `parar_tunel`, `relanzar_tunel` y `activar_en_puerto_libre`
- [ ] `responder_dialogo(id, respuesta)` para huella nueva, huella cambiada y contraseña
- [ ] `clave_publica_movil()` (línea OpenSSH y huella SHA256), `instalar_clave_en_host(host_uuid)`, `olvidar_contrasena(host_uuid)` y `borrar_datos(alcance)`
- [ ] Retrollamadas que implementa Kotlin: `CarpetaMovil` (SAF), `LlaveroMovil` (Keystore), `FirmanteMovil` (clave del móvil) y `OyenteNucleo` (pantallas, listas completas de sesiones y túneles, diálogos y estado de la sincronización)

**Proyecto Android**

- [ ] Proyecto Gradle en `android/` (Kotlin, Jetpack Compose, Material 3) con `applicationId` `org.cuatrodetres.magi`, `minSdk` 30, `targetSdk` 37, versiones fijadas en el catálogo y wrapper con `distributionSha256Sum`
  *Hecho: Gradle 9.8.2 + AGP 9.5.0 + Kotlin 2.4.21, catálogo y wrapper con suma
  fijados.*
- [ ] Verificación de dependencias de Gradle con SHA-256 (`gradle/verification-metadata.xml`)
  *Hecho: `verification-metadata.xml` con 448 componentes; se regenerará si
  cambian las dependencias.*
- [ ] Permisos: solo `INTERNET`, `CAMERA` (pedido al escanear), `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `USE_BIOMETRIC` y `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; ningún permiso de almacenamiento
  *Hecho: el manifest declara exactamente esos permisos.*
- [ ] Sin copia de seguridad en la nube: `allowBackup="false"` y reglas de extracción de datos que lo excluyen todo
  *Hecho: `allowBackup="false"`, reglas de extracción y
  `fullBackupContent="false"`.*
- [ ] Sin Google Play Services, Firebase, ML Kit ni analítica
  *Hecho: el catálogo solo trae AndroidX, Compose, JNA, CameraX y ZXing core;
  sin dependencias de Play Services.*
- [ ] Gradle compila el núcleo con `cargo-ndk` y el NDK r30 para `arm64-v8a` (release) y además `x86_64` (debug y emulador), y genera los bindings de Kotlin en cada compilación
  *Cadena validada: `assembleDebug` con `libmagi_movil.so` en las dos ABI y
  `versionCode` 1009; el AC de arrancar en el emulador queda pendiente.*

**Calidad, CI y publicación**

- [ ] `ci.yml` pasa a `--workspace`: tests, clippy y `cargo deny` cubren `magi-nucleo` y `magi-movil`
  *Hecho y verificado en local: `cargo check`, `clippy --workspace -D
  warnings`, `test --workspace --locked` y `deny check` en verde; la
  ejecución en GitHub queda pendiente.*
- [ ] `android.yml`: compila el núcleo para Android, ejecuta `lint` y las pruebas de Kotlin en la JVM (con el núcleo compilado para el anfitrión) y genera el APK de depuración
  *Workflow hecho; la cadena Gradle ya está validada en local, pero el workflow
  no se ha ejecutado en GitHub.*
- [ ] Prueba de humo en el emulador (API 30, x86_64) en `main` y en las etiquetas `android-v*`
  *Job y prueba preparados; la prueba actual solo arranca `MainActivity` y no
  se ha ejecutado todavía en emulador. El AC completo queda pendiente.*
- [ ] `release.yml` (dist) no se dispara con las etiquetas `android-v*`
  *Hecho: patrón negativo `!android-v*`; pendiente de comprobarlo en una
  ejecución real.*
- [ ] `android-release.yml` con la etiqueta `android-vX.Y.Z`: APK `arm64-v8a` firmado (esquemas v2 y v3), `apksigner verify`, `.sha256`, atestación y GitHub Release que nunca se marca como «Latest»; con `-rc.N`, prerelease
  *Workflow hecho y validado con actionlint y simulación de sus pasos; no
  ejecutado aún (faltan el APK release y los secretos).*
- [ ] La firma usa un keystore que solo vive en los secretos de ese workflow (`ANDROID_KEYSTORE_BASE64`, sus contraseñas y el alias); las notas de la release incluyen la huella SHA-256 del certificado
  *Workflow hecho; el keystore y los secretos los crea el desarrollador (T69):
  pendientes.*
- [ ] `versionCode` derivado de la versión con la fórmula del informe (§7.10) y `versionName` igual a la versión
  *Hecho en Gradle y en el workflow; el APK de depuración real ya lleva
  `versionCode` 1009 y `versionName` 0.1.0, y están probadas las fórmulas
  0.1.0-rc.1→1001 y 1.2.3→102039.*
- [ ] `android/CHANGELOG.md` (Keep a Changelog) y notas de la release tomadas de su sección
  *Hecho el CHANGELOG y la extracción de notas en el workflow (probada con
  0.1.0, 0.1.0-rc.1 y una versión inexistente).*
- [ ] README con la sección «App Android» (Obtainium filtrando `android-v`, verificar el APK y su certificado, Syncthing-Fork en «solo recibir», qué hacer si pierdes el móvil) y `docs/publicar-android.md`
  *Hecho: README y guía completos; pendiente la revisión del desarrollador y
  del analista.*

**Pruebas del núcleo en el anfitrión**

- [ ] Pruebas del núcleo en el anfitrión: firma DER a SSH, modo lector (borrados masivos, aplazados y conflictos), codificación de teclas, tramos de pantalla, túnel en puerto libre y script de instalación con `sh` en un HOME temporal
  *Hecho: 11 pruebas de `instalar_clave` con `sh` y `HOME` temporal y 8
  de la pasada en modo lector; las de claves, teclas, tramos, túneles y puerto
  libre están en los módulos del núcleo y también en verde.*

## Pendientes y bloqueos

1. **UI Compose y adaptadores reales en curso.** Las pantallas y los
   adaptadores (`NucleoRepositorio`, `AdaptadoresPlataforma`,
   `AdaptadorOyenteNucleo` y `FuenteNucleoReal`) se están escribiendo
   ahora; pueden moverse mientras se actualiza este informe.
2. **`assembleRelease` y prueba de humo sin ejecutar.** No hay APK release
   firmado y `connectedDebugAndroidTest` no se ha corrido, ni en local ni en
   CI. El AC de la prueba de humo (cargar el núcleo nativo, importar un fichero
   de ejemplo y pintar sus hosts) sigue pendiente.
3. **Lint: falta la pasada final.** El último informe de lint en disco (22:36)
   es anterior al parche que suprime `NewApi` en los bindings generados, así
   que muestra los 3 avisos de `java.lang.ref.Cleaner`. Hay que ejecutar
   `./gradlew lint` y dejar el informe limpio.
4. **CI sin ejecutar.** Los cinco workflows no se han ejecutado en GitHub
   (solo `actionlint` y la simulación local). Falta comprobar en un runner
   real: `ci.yml --workspace`, KVM/nested virtualization del job de humo, que
   AGP descargue la plataforma 37.0 y las build-tools, y la descarga del
   wrapper y de cargo-ndk.
5. **Publicación de la app sin ensayar.** Faltan el keystore, sus cuatro
   secretos (los crea el desarrollador), el ensayo con
   `android-v0.1.0-rc.1` y cualquier APK release firmado;
   `android-release.yml` no se ha ejecutado nunca. La fecha de `[0.1.0]`
   en el CHANGELOG es provisional y `docs/publicar-android.md` está aún sin
   commit.
6. **Validación manual y revisión adversarial.** Faltan la revisión adversarial
   de cierre (T31, T54) y el guion de validación en el plegable real (R100:
   StrongBox/TEE, SAF, biometría, Syncthing-Fork, QR, MagicOS, plegado y
   teclado físico), descrito en el informe §11; su resultado debe anotarse
   aquí.
7. **Corrección documental pendiente.** `docs/formato-sincronizacion.md`
   §11.9 pide una red de seguridad para los borrados masivos en Android que
   contradice el informe de la Fase 11 §7.3 (implementado sin copia ni
   confirmación). Hay que corregir el documento de formato.
8. **Cerrar los commits.** El proyecto Android está sin versionar y
   `docs/publicar-android.md` sin commit; hay ajustes sin commit en
   `magi-movil` (`gestor.rs`, `inventario.rs` y `sincro.rs`) y en
   `magi-nucleo/src/rutas.rs`. Antes de cerrar la fase hay que dejarlo todo
   commiteado y con la CI en verde.
9. **`dist` local y de CI.** La validación local del plan se hizo con
   0.32.0 porque el índice no tiene 0.33.1; hay que confirmar el plan real en
   CI con la versión fijada.
10. **Limitaciones heredadas.** Si R85-R87 siguen abiertas (informe §7.13),
    dejarlas anotadas con su impacto real en el móvil.
11. **Checklist y contadores.** Sigue en 0/110 y 713/827 a propósito; lo cierra
    el Lead al final de la fase, junto con este informe.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

**Entorno de Android (máquina de desarrollo).** Antes de usar Gradle o
cargo-ndk:

    source /tmp/entorno-android.sh

Deja `ANDROID_HOME` y `ANDROID_SDK_ROOT` en `/home/hector/Android/Sdk`,
`ANDROID_NDK_HOME` en `.../ndk/30.0.16248370` y el `PATH` con
`cmdline-tools/latest/bin`, `platform-tools`, `emulator` y
`build-tools/36.0.0`. Requisitos ya instalados y verificados: SDK con la
plataforma 37.0 y build-tools 36/37, NDK r30, Gradle 9.8.2, JDK 26, cargo-ndk
4.1.2 y los objetivos `aarch64-linux-android` y `x86_64-linux-android`. El
emulador necesita KVM (disponible aquí; en CI lo comprueba el workflow).

**Escritorio.**

    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    TZ=UTC cargo test --workspace --locked
    cargo deny check

Las pruebas de pintado necesitan `TZ=UTC` y las de SFTP necesitan
`sftp-server` o `MAGI_SFTP_SERVER`. Las cuatro órdenes están en verde en
esta sesión. **Aviso de disco:** `/tmp` es un tmpfs y puede llenarse (hoy
está al 80 %); si `cargo test` falla con errores raros de SQLite o de
temporales, ejecútalo con el directorio temporal fuera de `/tmp`:

    mkdir -p "$HOME/.cache/tmp"
    TMPDIR="$HOME/.cache/tmp" TZ=UTC cargo test --workspace --locked

**App Android.**

    cd android
    ./gradlew assembleDebug               # validado: APK debug con las dos ABI y libmagi_movil.so
    ./gradlew lint testDebugUnitTest      # pendiente la pasada final de lint; 6 pruebas JVM en verde
    ./gradlew :app:assembleRelease        # pendiente: solo firma con MAGI_KEYSTORE_*
    ./gradlew connectedDebugAndroidTest   # pendiente: emulador API 30 x86_64, AVD magi-api30

`assembleDebug` y `assembleRelease` compilan antes el núcleo con cargo-ndk
y generan los bindings Kotlin en
`app/build/generated/uniffi/kotlin/org/cuatrodetres/magi/nucleo/magi_movil.kt`
(no versionados). La release firmada se genera con las cuatro variables
`MAGI_KEYSTORE_*` (las pone la CI a partir de los secretos) en
`android/app/build/outputs/apk/release/app-release.apk`, solo `arm64-v8a`.
La tarea de licencias alimenta la pantalla «Acerca de».

**Integración continua y workflows.**

    actionlint .github/workflows/*.yml      # 0 errores (con .github/actionlint.yaml)
    bash -n <script>                        # comprobado para los run: de android.yml y android-release.yml

`android.yml` se ejecuta en cada PR que toca `crates/` o `android/`, en
push a `main` y en etiquetas `android-v*`. La prueba de humo (job `humo`)
corre en `ubuntu-latest` con KVM y el mismo emulador API 30 x86_64.
`android-release.yml` solo con etiqueta; el flujo completo (keystore,
secretos, ensayo `rc` y comprobaciones) está en
`docs/publicar-android.md`.

**Verificación de un APK publicado.**

    sha256sum -c magi-android-v0.1.0.apk.sha256
    "$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --print-certs magi-android-v0.1.0.apk
    gh attestation verify magi-android-v0.1.0.apk --repo 4d3-org/magi

La huella del certificado debe coincidir con la de las notas de la release y
con la de versiones anteriores; el APK de release solo debe llevar
`lib/arm64-v8a`.

**Qué se ha validado ya y qué no.** Hecho: entorno Android completo y
compilación cruzada con R91 retirado; núcleo con pruebas verdes (tramos,
teclas, claves, instalar clave y pasada lectora); `cargo
check`/`clippy`/`test --workspace --locked`/`fmt`/`deny` en verde;
`dist plan` en local (0.32.0) publicando solo `magi-ssh`; actionlint y
sintaxis de los 21 scripts `run:`; fórmulas de `versionCode` y
coincidencia con `version.properties`; extracción de notas del CHANGELOG;
cadena Gradle completa (`assembleDebug` con las dos ABI, `versionCode`
1009, SHA-256 de dependencias y tarea de licencias); bindings Kotlin
regenerados; y el emulador arrancando con KVM y aceptando un APK de prueba.
Pendiente de validar: UI y adaptadores, la pasada final de `lint`,
`assembleRelease` con firma, `connectedDebugAndroidTest`, la primera
ejecución de la CI en GitHub, el ensayo completo de `android-release.yml`
con el keystore y la validación en el plegable real.

---

*Borrador actualizado el 2026-10-31 (22:59); se actualiza al cerrar cada
sesión.*
