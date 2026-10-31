# MAGI - Fase 11: App Android — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. No se
> regenera desde cero. La especificación de la fase está en
> [magi-fase11-informe.md](magi-fase11-informe.md) y el alcance verificable en
> [magi-fase11-checklist.md](magi-fase11-checklist.md).

**Estado de la sesión (borrador, instantánea del 2026-10-31 22:30):** fase en
curso. De la infraestructura están hechos el entorno Android (R91 retirado), el
esqueleto del proyecto Gradle, la integración continua y la publicación de la
app y su documentación; del núcleo, la reorganización como workspace y el
traslado de gran parte de los módulos compartidos (commits `a5f1e1f` y
`82bff82` más trabajo sin commit), y el contrato compilable de
`crates/magi-movil`. Faltan la lógica de esa puerta UniFFI, la pasada en modo
lector y el resto del núcleo en curso, y las capas de plataforma y de interfaz
de la app Android (que en este momento son un esqueleto). No hay todavía APK ni
ejecución de la CI de Android, y el checklist sigue sin marcar (0/110) a
propósito hasta el cierre de la fase. Como el árbol se está moviendo, esta
instantánea se revisará en cada sesión.

## Resumen de lo implementado

### Workspace de Cargo y núcleo compartido (en curso)

- Commit `a5f1e1f` («Reorganiza el repositorio como workspace con el núcleo
  compartido», 97 ficheros) y commit `82bff82` («Mueve al núcleo la mecánica
  de los túneles y el SOCKS5»).
- `Cargo.toml` de la raíz: `[workspace]` con
  `members = ["crates/magi-nucleo", "crates/magi-movil"]` y
  `resolver = "2"`. El paquete `magi-ssh` sigue en la raíz con su nombre,
  versión 1.1.0, `publish = false` y la configuración de `dist` sin tocar;
  depende de `magi-nucleo` por ruta.
- `crates/magi-nucleo` (`publish = false`, versión 0.1.0): almacén y
  migraciones; sincronización compartida (formato, cifrado, fusión,
  aplicación, elementos, huellas, reloj y carpeta, más la pasada común y el
  trait `CarpetaSincronizacion`); conexión SSH (cliente, saltos, huellas,
  reenvíos y terminal); túneles (mod, escucha, local y socks5); terminal
  (tramos y teclas); claves (DER a SSH, X.509 a OpenSSH y el trait
  `Firmante`); `instalar_clave` con su script embebido; llavero (trait
  `LlaveroSecreto` y llavero en memoria); registro; ficheros; configuración;
  deliberación; snippets; `trabajo_vivo` y `rutas` (`Rutas` y
  `Equipo` inyectados).
- El escritorio mantiene sus rutas de importación: `src/lib.rs` reexporta
  `magi_nucleo::{ficheros, modelo, registro, rutas}` y
  `src/sincronizacion/mod.rs` reexporta la sincronización del núcleo y añade
  lo que es solo del escritorio (pasada en modo escritor, equipos y QR).
- En el árbol de trabajo, sin commit, se está completando el traslado:
  `conexion/cliente.rs` y la mecánica de túneles ya viven en el núcleo y
  `src/servidor/tuneles.rs` y `src/servidor/conexiones.rs` se reducen a su
  uso; aparecen `crates/magi-nucleo/src/terminal/`, `claves/`,
  `instalar_clave.rs`, `almacen/movil.rs`, `tuneles/{mod,escucha,local}.rs`,
  `scripts/instalar_clave.sh` y los primeros tests dentro de
  `crates/magi-nucleo/tests/`. Los documentos de reglas (`AGENTS.md`,
  `CLAUDE.md` y `docs/fase11/CLAUDE.md`) se actualizan con las rutas nuevas.

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

### Proyecto Gradle `android/` (esqueleto, en curso)

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
  y las tareas `compilarNucleoAnfitrion/Release/Debug`,
  `generarBindingsNucleo` y sus enganches a `compile*Kotlin`,
  `merge*JniLibFolders` y `testDebugUnitTest`.
- Manifest con exactamente los permisos del checklist (ninguno de
  almacenamiento), `allowBackup="false"`, reglas de extracción, tema de
  cabina Material 3 con la paleta fija y JetBrains Mono, textos en castellano,
  navegación Hosts · Sesiones · Túneles · Ajustes con barra inferior/riel y una
  pantalla esqueleto.
- Contratos preparados todavía sin lógica: `NucleoRepositorio` (biblioteca
  `magi_movil`, paquete UniFFI y plataforma), `FachadaPlataforma`
  (`CarpetaMovil`, `LlaveroMovil`, `FirmanteMovil` y `BloqueoApp`) y las
  pruebas `PruebaContratoNucleo`, `PruebaPaleta` y `PruebaHumo` (emulador).
- Evidencia del esqueleto: `./gradlew lint` sin avisos («No issues found») y
  `testDebugUnitTest` con 6 pruebas y 0 fallos (informes en
  `android/app/build/`, no versionados). El `assembleDebug` completo y el
  APK quedan pendientes de cerrar la puerta UniFFI y de la integración en
  curso.

### Puerta UniFFI `crates/magi-movil` (contrato, en curso)

- Crate nuevo `magi-movil` 0.1.0, `publish = false`, `crate-type =
  ["cdylib", "lib"]`, cdylib `libmagi_movil.so`, UniFFI fijado en
  `=0.32.2` (con runtime de tokio) y `uniffi.toml` con el paquete Kotlin
  `org.cuatrodetres.magi.nucleo` y `cdylib_name = "magi_movil"`.
- En esta entrega es un **contrato compilable**: `api.rs` declara el error
  `ErrorNucleo`, los DTO y las funciones de §5.1 y las retrollamadas
  (`CarpetaMovil`, `LlaveroMovil`, `FirmanteMovil`, `OyenteNucleo`),
  pero solo `version()` está implementada; el resto son
  `unimplemented!()` con `TODO(F11)` que sustituye la integración. También
  hay un bin `uniffi-bindgen` tras la feature `bindgen`.

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
  en un emulador API 30 x86_64 con KVM (regla udev, comprobación de
  `/dev/kvm`, `emulator -accel-check` y `-accel on`).
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
  reinstalar la app.
- `android/CHANGELOG.md` en formato Keep a Changelog, con `[Sin publicar]` y
  `[0.1.0]` (fecha provisional hasta publicar).
- `README.md`: sección «App Android» antes de «Atajos».
- `android/version.properties`: `version=0.1.0` con la fórmula en
  comentario; la leen Gradle y los workflows.

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
9. **Instantánea en movimiento.** El árbol tiene trabajo sin commit de varios
   compañeros (núcleo, puerta UniFFI, plataforma y UI de la app). Este
   documento describe el estado en el momento indicado arriba y se actualizará
   en cada sesión; no debe leerse como el estado final de la fase.

## Estructura de archivos creada/modificada

Raíz del repositorio:

- `Cargo.toml`: `[workspace]` con los miembros `crates/magi-nucleo` y
  `crates/magi-movil`, y dependencia por ruta a `magi-nucleo`. El paquete
  `magi-ssh` conserva nombre, versión y `dist`. `Cargo.lock` se regeneró
  en el commit del workspace y hoy está desincronizado por los cambios en
  curso del núcleo (ver pendientes).
- `src/lib.rs`: reexporta los módulos del núcleo que el escritorio sigue
  usando con las rutas antiguas.
- `src/servidor/tuneles.rs`, `src/servidor/conexiones.rs`,
  `src/servidor/mod.rs`: usan la mecánica y el cliente del núcleo (en el
  árbol de trabajo, sin commit).
- `AGENTS.md`, `CLAUDE.md`, `docs/fase11/CLAUDE.md`: rutas nuevas de los
  módulos compartidos.

Núcleo compartido:

- `crates/magi-nucleo/Cargo.toml` y `crates/magi-nucleo/src/`: `lib.rs`,
  `rutas.rs`, `modelo.rs`, `configuracion.rs`, `trabajo_vivo.rs`,
  `llavero.rs`, `registro.rs`, `ficheros.rs`, `instalar_clave.rs`,
  `deliberacion.rs`; `almacen/` (migraciones, hosts, grupos, identidades,
  túneles, snippets, verificaciones, sondeos, equipos, lápidas, conflictos,
  sincronizaciones, copias, versiones, deliberaciones, registro y
  `movil.rs`); `sincronizacion/` (formato, cifrado, fusión, aplicación,
  elementos, huellas, reloj, carpeta y la pasada común);
  `conexion/` (cliente, saltos, huellas, reenvíos y terminal);
  `tuneles/` (mod, escucha, local y socks5); `terminal/` (mod, tramos y
  teclas); `claves/` (mod, común, DER a SSH, X.509 a OpenSSH y firmante);
  `snippets/` (mod, salida y variables); `scripts/instalar_clave.sh`;
  `tests/` incipiente.
- `crates/magi-movil/`: `Cargo.toml`, `uniffi.toml` y
  `src/{lib,api,dialogos,escritor,gestor,retrollamadas}.rs` más
  `src/bin/uniffi-bindgen.rs` (contrato compilable, lógica pendiente).

Android:

- `android/`: `settings.gradle.kts`, `build.gradle.kts`,
  `gradle.properties`, `version.properties`, `CHANGELOG.md`,
  `.gitignore`; `gradle/libs.versions.toml`,
  `gradle/verification-metadata.xml` y wrapper (`gradlew`,
  `gradlew.bat` y `gradle/wrapper/...` con `distributionSha256Sum`);
  `app/build.gradle.kts`, `app/lint.xml` y `app/proguard-rules.pro`.
- `android/app/src/main/`: `AndroidManifest.xml`;
  `kotlin/org/cuatrodetres/magi/` (`MagiApp.kt`, `MainActivity.kt`,
  `nucleo/NucleoRepositorio.kt`, `plataforma/FachadaPlataforma.kt`,
  `navegacion/{Destino,BarraNavegacion}.kt`, `ui/PantallaEsqueleto.kt` y
  `ui/tema/{Paleta,Tema,Tipografia}.kt`); `res/` (textos en castellano,
  temas y colores, fuentes JetBrains Mono, iconos, `xml/reglas_extraccion.xml`
  y la licencia OFL).
- `android/app/src/test/` (`PruebaContratoNucleo` y `PruebaPaleta`) y
  `android/app/src/androidTest/` (`PruebaHumo`).

CI, publicación y documentación:

- `.github/workflows/ci.yml` y `.github/workflows/release.yml`
  (modificados); `.github/workflows/android.yml` y
  `.github/workflows/android-release.yml` (nuevos);
  `.github/actionlint.yaml` (nuevo).
- `docs/publicar-android.md` (nuevo), `android/CHANGELOG.md` (nuevo),
  `README.md` (modificado, sección «App Android») y este
  `docs/fase11/magi-fase11-implementacion.md`.

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

## Funcionalidades del checklist completadas

Copia literal de los ítems que hoy se pueden considerar hechos (se marcarán
`[x]` en la checklist al cerrar la fase; el estado de cada uno se anota en
cursiva). El resto de los 110 ítems siguen pendientes y no se copian aquí.

**Proyecto Android**

- - [ ] Proyecto Gradle en `android/` (Kotlin, Jetpack Compose, Material 3) con `applicationId` `org.cuatrodetres.magi`, `minSdk` 30, `targetSdk` 37, versiones fijadas en el catálogo y wrapper con `distributionSha256Sum`
  *Hecho en el árbol (task del esqueleto en curso de cierre): Gradle 9.8.2 +
  AGP 9.5.0 + Kotlin 2.4.21, catálogo de versiones y wrapper con suma fijados.*
- - [ ] Verificación de dependencias de Gradle con SHA-256 (`gradle/verification-metadata.xml`)
  *Hecho: `gradle/verification-metadata.xml` con 448 componentes; se
  regenerará si cambian las dependencias.*
- - [ ] Permisos: solo `INTERNET`, `CAMERA` (pedido al escanear), `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `USE_BIOMETRIC` y `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; ningún permiso de almacenamiento
  *Hecho: el manifest declara exactamente esos permisos.*
- - [ ] Sin copia de seguridad en la nube: `allowBackup="false"` y reglas de extracción de datos que lo excluyen todo
  *Hecho: `allowBackup="false"`, reglas de extracción y
  `fullBackupContent="false"`.*
- - [ ] Sin Google Play Services, Firebase, ML Kit ni analítica
  *Hecho: el catálogo solo trae AndroidX, Compose, JNA, CameraX y ZXing core;
  sin dependencias de Play Services.*

**Calidad, CI y publicación**

- - [ ] `ci.yml` pasa a `--workspace`: tests, clippy y `cargo deny` cubren `magi-nucleo` y `magi-movil`
  *Hecho para clippy y tests (incluidos macOS); `cargo deny` cubre el
  workspace sin el flag (no lo admite 0.20.2). Pendiente: CI verde cuando
  cierre la migración.*
- - [ ] `android.yml`: compila el núcleo para Android, ejecuta `lint` y las pruebas de Kotlin en la JVM (con el núcleo compilado para el anfitrión) y genera el APK de depuración
  *Workflow hecho; no ejecutado aún. La compilación de Gradle hoy se queda en
  el esqueleto: faltan la lógica de `magi-movil` y el APK.*
- - [ ] Prueba de humo en el emulador (API 30, x86_64) en `main` y en las etiquetas `android-v*`
  *Job y prueba preparados; la prueba actual solo arranca `MainActivity` y no
  se ha ejecutado todavía en el emulador. El AC completo (cargar el núcleo,
  importar un fichero de ejemplo y pintar hosts) queda para cuando exista la
  puerta UniFFI funcional.*
- - [ ] `release.yml` (dist) no se dispara con las etiquetas `android-v*`
  *Hecho: patrón negativo `!android-v*`; pendiente de comprobarlo en una
  ejecución real.*
- - [ ] `android-release.yml` con la etiqueta `android-vX.Y.Z`: APK `arm64-v8a` firmado (esquemas v2 y v3), `apksigner verify`, `.sha256`, atestación y GitHub Release que nunca se marca como «Latest»; con `-rc.N`, prerelease
  *Workflow hecho y validado con actionlint y simulación de sus pasos; no
  ejecutado aún (faltan el APK y los secretos).*
- - [ ] La firma usa un keystore que solo vive en los secretos de ese workflow (`ANDROID_KEYSTORE_BASE64`, sus contraseñas y el alias); las notas de la release incluyen la huella SHA-256 del certificado
  *Workflow hecho; el keystore y los secretos los crea el desarrollador (T69):
  pendientes.*
- - [ ] `versionCode` derivado de la versión con la fórmula del informe (§7.10) y `versionName` igual a la versión
  *Hecho en Gradle y en el workflow; probadas las fórmulas 0.1.0-rc.1→1001,
  0.1.0→1009 y 1.2.3→102039, y la coincidencia con
  `android/version.properties`.*
- - [ ] `android/CHANGELOG.md` (Keep a Changelog) y notas de la release tomadas de su sección
  *Hecho el CHANGELOG y la extracción de notas en el workflow (probada con
  0.1.0, 0.1.0-rc.1 y una versión inexistente).*
- - [ ] README con la sección «App Android» (Obtainium filtrando `android-v`, verificar el APK y su certificado, Syncthing-Fork en «solo recibir», qué hacer si pierdes el móvil) y `docs/publicar-android.md`
  *Hecho: README y guía completos; pendiente la revisión del desarrollador y
  del analista.*

## Pendientes y bloqueos

1. **Lógica de `crates/magi-movil`.** Hoy solo `version()` está
   implementada; el resto de funciones de §5.1 son stubs con
   `unimplemented!()`. Sin esa integración la app no puede iniciar el núcleo,
   ni leer el inventario, ni abrir sesiones, y la prueba de humo no puede
   completar su AC. Es el bloqueante principal de la parte nativa.
2. **Núcleo en curso.** Faltan pruebas y cierre de la pasada en modo lector y
   de `CarpetaSincronizacion` (trait ya declarado y pasada común en el
   núcleo), los tramos de pantalla, la codificación de teclas, las conversiones
   DER/X.509, `instalar_clave` y el traslado de los tests a
   `crates/magi-nucleo/tests/`. Las funcionalidades correspondientes del
   checklist no se pueden marcar.
3. **Workspace sin cuajar.** El `Cargo.lock` del árbol está desincronizado:
   `cargo metadata --locked` falla porque hay dependencias nuevas del núcleo
   (y miembros nuevos) sin lock. Hay que regenerarlo y dejarlo cuadrado antes
   de la CI. Además, la migración es un commit parcial (los commits
   `a5f1e1f`/`82bff82` no incluyen el trabajo actual) y no se ha pasado
   `cargo fmt`/`clippy`/`test --workspace` ni `dist plan` sobre el
   conjunto.
4. **App Android sin capas reales.** Las capas de plataforma (SAF, Keystore,
   firma, bloqueo, cámara, biometría y servicio en primer plano) y de interfaz
   (Hosts, ficha, terminal, túneles, ajustes, diálogos) están en curso y hoy
   solo hay esqueleto. `assembleDebug` y `connectedDebugAndroidTest` no se
   han podido completar.
5. **CI sin ejecutar.** Los workflows nuevos no se han ejecutado en GitHub.
   `ci.yml --workspace` no puede estar verde hasta que cierre la migración;
   `android.yml` y `android-release.yml` dependen de la puerta UniFFI y de
   los secretos. Falta comprobar en un runner real: KVM/nested virtualization,
   que AGP resuelva la plataforma 37 y las build-tools, y la descarga del
   wrapper.
6. **Publicación de la app sin ensayar.** Faltan el keystore, sus cuatro
   secretos (los crea el desarrollador), el ensayo con
   `android-v0.1.0-rc.1` y cualquier APK firmado; `android-release.yml` no
   se ha ejecutado nunca. La fecha de `[0.1.0]` en el CHANGELOG es
   provisional.
7. **Prueba de humo incompleta.** `PruebaHumo` solo comprueba que arranca la
   actividad; el AC del checklist (cargar el núcleo nativo, importar un fichero
   de ejemplo y pintar sus hosts) requiere la puerta UniFFI funcional y una
   carpeta de prueba.
8. **Validación manual y revisión adversarial.** Faltan la revisión adversarial
   de cierre (T31, T54) y el guion de validación en el plegable real (R100:
   Syncthing-Fork, QR, MagicOS, StrongBox, plegado y teclado físico), descrito
   en el informe §11. Las limitaciones heredadas R85-R87 siguen abiertas
   (informe §7.13).
9. **Checklist y contadores.** Sigue en 0/110 y 713/827 a propósito; se
   actualizará al cerrar la fase, junto con este informe.
10. **Documentación viva.** `docs/publicar-android.md`, la sección del README,
   `android/CHANGELOG.md` y este informe deben revisarse cuando exista el
   proyecto completo; en particular, los nombres de tareas y artefactos de
   Gradle y de la puerta UniFFI pueden cambiar al integrar la lógica.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

**Entorno de Android (máquina de desarrollo).** Antes de usar Gradle o
cargo-ndk:

    source /tmp/entorno-android.sh

Deja `ANDROID_HOME` y `ANDROID_SDK_ROOT` en
`/home/hector/Android/Sdk`, `ANDROID_NDK_HOME` en
`.../ndk/30.0.16248370` y el `PATH` con `cmdline-tools/latest/bin`,
`platform-tools`, `emulator` y `build-tools/36.0.0`. Requisitos ya
instalados y verificados: SDK con la plataforma 37.0 y build-tools 36/37, NDK
r30, Gradle 9.8.2, JDK 26, cargo-ndk 4.1.2 y los objetivos
`aarch64-linux-android` y `x86_64-linux-android`. El emulador necesita KVM
(disponible aquí; en CI lo comprueba el workflow).

**Escritorio (lo que seguirá siendo válido tras la migración).**

    TZ=UTC cargo test --workspace --locked
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo fmt --all --check
    cargo deny check

Las pruebas de pintado necesitan `TZ=UTC` y las de SFTP necesitan
`sftp-server` o `MAGI_SFTP_SERVER`. Hoy `cargo metadata --locked` falla
por el `Cargo.lock` desincronizado del árbol de trabajo (ver pendientes); sin
`--locked` y con red, `cargo metadata` regenera el lock.

**App Android.**

    cd android
    ./gradlew lint testDebugUnitTest        # hoy: lint sin avisos y 6 pruebas JVM en verde
    ./gradlew assembleDebug                 # pendiente de cerrar la puerta UniFFI
    ./gradlew connectedDebugAndroidTest     # emulador API 30 x86_64, AVD magi-api30

`assembleDebug` y `assembleRelease` compilan antes el núcleo con cargo-ndk y
generan los bindings; mientras la puerta UniFFI siga en contrato, el código
nativo se compila pero la app no puede arrancar el núcleo. La release firmada
se genera con las cuatro variables `MAGI_KEYSTORE_*` (las pone el CI a partir
de los secretos) en
`android/app/build/outputs/apk/release/app-release.apk`, solo `arm64-v8a`.

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
compilación cruzada con R91 retirado; `lint` sin avisos y pruebas JVM de
Gradle; actionlint/shellcheck y YAML de los cinco workflows; sintaxis de los 21
scripts `run:`; fórmulas de `versionCode` y coincidencia con
`version.properties`; extracción de notas del CHANGELOG; y, en el entorno, el
emulador arrancando con KVM y aceptando un APK de prueba. Pendiente de
validar: `assembleDebug`/APK real, `connectedDebugAndroidTest` con la app
real, la primera ejecución de la CI (incluido `--workspace`), el ensayo
completo de `android-release.yml` con keystore y la validación manual en el
plegable.

---

*Borrador abierto el 2026-10-31; se actualiza al cerrar cada sesión.*
