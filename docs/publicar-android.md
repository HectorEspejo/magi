# Publicar la app Android

La app Android de MAGI vive en [`android/`](../android/) y se publica aparte
del escritorio: tiene su propia versión ([`android/version.properties`](../android/version.properties)),
sus etiquetas (`android-vX.Y.Z` y `android-vX.Y.Z-rc.N`) y su workflow
(`.github/workflows/android-release.yml`). Este documento lo sigue el
desarrollador para publicar una versión. El agente prepara el árbol, los
workflows y la documentación, pero **no crea el keystore, ni los secretos, ni
etiqueta versiones** (T69).

Canales de distribución:

| Canal | Qué publica |
|---|---|
| GitHub Releases | `magi-android-vX.Y.Z.apk` (`arm64-v8a`), su `.sha256` y la atestación de procedencia |
| Obtainium | Instala y actualiza desde esas releases, filtrando la etiqueta `android-v` |

No hay Google Play ni otro canal: la app no se publica en tiendas.

## 1. Reparto de responsabilidades

| Tarea | Quién |
|---|---|
| Fijar la versión, la fecha y el contenido de la release | Desarrollador |
| Actualizar `android/version.properties`, `android/CHANGELOG.md` y el README | Agente |
| Dejar los workflows y esta guía preparados | Agente |
| Crear el keystore de firma y los cuatro secretos | Desarrollador |
| Etiquetar (`git tag`) y empujar la etiqueta | Desarrollador |
| Relanzar jobs y verificar la release | Desarrollador |
| Mover una etiqueta, subir binarios a mano, crear cuentas o cambiar la visibilidad del repositorio | **Nadie más que el desarrollador**; el agente no lo hace |

## 2. Requisitos previos

- SDK de Android con la plataforma `android-37.0`, build-tools 36 o superior
  y el NDK r30 (`30.0.16248370`); JDK 17 o superior (el CI usa el 21) y
  Gradle 9.8.2, que ya trae el wrapper de `android/`.
- `cargo-ndk` 4.1.2 y los objetivos `aarch64-linux-android` y
  `x86_64-linux-android` de rustup. El entorno completo está descrito en
  [`CLAUDE.md`](../CLAUDE.md) (sección «Comandos de Android»).
- En local, antes de etiquetar:

  ```sh
  cd android
  ./gradlew lint testDebugUnitTest assembleDebug

  # Prueba de humo con un emulador x86_64 API 30 (como el del CI). Las
  # pruebas instrumentadas de extremo a extremo necesitan antes el servidor de
  # pruebas del móvil en el anfitrión (escucha solo en 127.0.0.1):
  #   cargo run -p magi-movil --example servidor_pruebas
  ./gradlew connectedDebugAndroidTest
  ```

  `./gradlew assembleRelease` sin las variables de firma genera un APK **sin
  firmar**: sirve para comprobar que compila, pero no se publica.
- El CI de cada cambio es `android.yml`: compila el núcleo con cargo-ndk para
  `arm64-v8a` y `x86_64`, pasa `lint` y las pruebas de Kotlin en la JVM,
  genera el APK de depuración y, en `main` y en las etiquetas `android-v*`,
  corre la prueba de humo en un emulador API 30 x86_64 con KVM.
- La versión vive en `android/version.properties`, en una sola línea
  `version=X.Y.Z` (o `X.Y.Z-rc.N`). El `versionName` del APK es esa cadena
  y el `versionCode` sale de ella (ver §4).

## 3. El keystore de firma (lo crea el desarrollador)

Android exige que todas las versiones de una app instalada estén firmadas con
la **misma** clave. El keystore es, por tanto, a la vez un secreto y la
identidad permanente de la app: si se pierde, los móviles que ya tienen MAGI no
pueden actualizar sin desinstalar y volver a instalar (se pierden los datos y
las claves del Keystore).

Créalo una sola vez, fuera del repositorio, con contraseña fuerte:

```sh
keytool -genkeypair -v \
  -keystore magi-android-release.jks \
  -alias magi-release \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=MAGI Android, O=Cuatro de Tres S.A., C=ES"
```

- Guarda el `.jks` en un gestor de contraseñas y en dos copias de seguridad
  separadas. Nunca lo subas al repositorio (tampoco el `.b64` ni la contraseña).
- Comprueba el certificado y anota su huella SHA-256 (la de §6):

  ```sh
  keytool -list -v -keystore magi-android-release.jks -alias magi-release
  ```

Después crea estos cuatro secretos en **Settings → Secrets and variables →
Actions** del repositorio:

| Secreto | Valor | Cómo obtenerlo |
|---|---|---|
| `ANDROID_KEYSTORE_BASE64` | el keystore codificado en base64, en una sola línea | Linux: `base64 -w0 magi-android-release.jks > keystore.b64` · macOS: `base64 -i magi-android-release.jks -o keystore.b64` |
| `ANDROID_KEYSTORE_PASSWORD` | la contraseña del almacén | la que elegiste al crearlo |
| `ANDROID_KEY_ALIAS` | el alias de la clave | `magi-release` en el ejemplo |
| `ANDROID_KEY_PASSWORD` | la contraseña de la clave | puede ser la misma que la del almacén |

Reglas de uso:

- **Solo `android-release.yml` recibe estos secretos.** No los añadas a
  `ci.yml` ni a `android.yml`, y no uses `secrets: inherit` en ningún
  workflow.
- El workflow descodifica el keystore en `$RUNNER_TEMP` (con permisos 600), lo
  comprueba con `keytool` y lo pasa a Gradle como `MAGI_KEYSTORE_PATH`,
  `MAGI_KEYSTORE_PASSWORD`, `MAGI_KEY_ALIAS` y `MAGI_KEY_PASSWORD`. GitHub
  enmascara los valores en el log; aun así, nunca los imprimas.
- **No se puede rotar la clave sin romper las actualizaciones**: un keystore
  nuevo cambia el certificado y Android rechaza el APK sobre una app ya
  instalada. Si sospechas que el keystore se ha filtrado, avisa y decide entre
  publicar una versión nueva con la misma clave (cambio de contraseñas y
  vigilancia) o asumir una identidad nueva con el coste de reinstalar. Las
  notas de cada release publican la huella del certificado para detectar APKs
  falsos.

## 4. Versión y etiqueta

La versión de la app sigue la política del escritorio (ver
[`publicar.md`](publicar.md)): **MAJOR** si algo deja de ser compatible para
el usuario, **MINOR** para funcionalidad nueva y **PATCH** para correcciones.
La app empieza en `0.1.0`.

El `versionCode` se deriva de la versión con la fórmula del informe §7.10:

```
versionCode = MAYOR * 100000 + MENOR * 1000 + PARCHE * 10 + R
```

donde `R` es el número de `rc` (1-8) o **9** en la versión final, para que
la final siempre supere a sus candidatas y se actualice sin desinstalar.

| Versión | versionCode |
|---|---|
| `0.1.0-rc.1` | 1001 |
| `0.1.0-rc.2` | 1002 |
| `0.1.0` | 1009 |
| `0.1.1` | 1019 |
| `0.2.0` | 2009 |
| `1.2.3` | 102039 |

- `MINOR` y `PARCHE` admiten hasta dos dígitos (0-99).
- `versionName` es la cadena tal cual, incluido `-rc.N`.
- El workflow falla si la etiqueta no coincide **exactamente** con
  `version=...` de `android/version.properties`, si la versión no encaja en
  la fórmula o si el `versionCode`/`versionName` del APK no son los
  esperados.

Flujo de una release:

1. Los cambios entran en `main` con su línea en `## [Sin publicar]` de
   [`android/CHANGELOG.md`](../android/CHANGELOG.md).
2. Decide la versión: pasa esa sección a `## [X.Y.Z] - fecha` y actualiza
   `version=` en `android/version.properties` (en el mismo PR). Para una
   candidata, usa `X.Y.Z-rc.N` en los dos sitios.
3. PR → `android.yml` verde → merge a `main`.
4. Crea la etiqueta anotada y empújala:

   ```sh
   git tag -a android-vX.Y.Z -m "MAGI Android X.Y.Z"
   git push origin android-vX.Y.Z
   ```

5. `android-release.yml` hace el resto (§5); vigílalo hasta el final.
6. **Una etiqueta publicada no se mueve ni se reutiliza.** Si hace falta
   cambiar el contenido, se publica un PATCH (o un nuevo `rc`).

Ensayo obligatorio antes de la primera estable: `version=0.1.0-rc.1`, etiqueta
`android-v0.1.0-rc.1`. El resultado debe ser una **prerelease** con el APK,
su `.sha256` y su atestación, sin tocar las releases del escritorio. Después,
`version=0.1.0` y etiqueta `android-v0.1.0` (el `versionCode` de la final,
1009, supera al de la candidata, 1001).

## 5. Qué hace `android-release.yml`

Solo se dispara con un push de etiqueta `android-v*`; los pull requests no
reciben permisos de escritura. Una ejecución por etiqueta, sin cancelación.

| Job | Qué hace | Permisos |
|---|---|---|
| `verificar` | Comprueba el formato de la etiqueta, que coincida con `android/version.properties` y calcula el `versionCode` con la fórmula §7.10 | `contents: read` |
| `compilar` | Instala JDK, SDK y NDK; descarga el keystore de los secretos; compila el APK release `arm64-v8a`; comprueba `versionName`, `versionCode` y ABI con `aapt2`; verifica la firma con `apksigner` (esquemas v2 y v3); genera el `.sha256`; atesta la procedencia y sube los artefactos | `contents: read`, `id-token: write`, `attestations: write` |
| `publicar` | Descarga los artefactos, comprueba sus sumas, toma las notas de `android/CHANGELOG.md` y crea la GitHub Release con la huella SHA-256 del certificado; **nunca** se marca como «Latest» y con `-rc.N` es una prerelease | `contents: write` |

- La Release lleva el APK `magi-android-vX.Y.Z.apk`, su `.sha256`, la
  atestación de procedencia y notas con la huella del certificado y los
  comandos de verificación.
- `release.yml` (las releases del escritorio, con `dist`) no se dispara con
  `android-v*`: su `on.push.tags` incluye el patrón negativo
  `!android-v*`.
- Las herramientas que descargan los workflows (por ejemplo `cargo-ndk`) van
  fijadas por SHA-256. La build-tools y el NDK se instalan con `sdkmanager`.

## 6. Verificar el APK antes de instalar

Descarga de la release el APK y su `.sha256` y comprueba todo esto:

```sh
# 1. Integridad (en macOS: shasum -a 256 -c)
sha256sum -c magi-android-v0.1.0.apk.sha256

# 2. Firma y certificado (apksigner viene con las build-tools del SDK)
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --print-certs magi-android-v0.1.0.apk

# 3. Procedencia: que lo compiló el CI de este repositorio
gh attestation verify magi-android-v0.1.0.apk --repo 4d3-org/magi

# 4. ABI: la release es solo arm64-v8a
unzip -l magi-android-v0.1.0.apk | grep lib/arm64-v8a
```

- `apksigner` debe decir que verifica con los esquemas **v2** y **v3** y
  mostrar la huella SHA-256 del certificado.
- Esa huella debe coincidir con la de las notas de la release y con la de las
  versiones anteriores de la app. **Si cambia, no instales el APK** y avisa:
  puede ser un keystore nuevo (rompe las actualizaciones) o un APK falsificado.
- `unzip -l` debe mostrar `lib/arm64-v8a/libmagi_movil.so` y ninguna otra
  ABI en la release.

## 7. Instalar y actualizar con Obtainium

1. Instala [Obtainium](https://github.com/ImranR98/Obtainium) (F-Droid o su
   release de GitHub).
2. «Añadir app» → pega la URL del repositorio:
   `https://github.com/4d3-org/magi`.
3. En las opciones de esa app, filtra las etiquetas por `android-v` y el
   nombre del APK por `magi-android-`. Activa las prereleases solo si quieres
   probar candidatas `-rc.N`.
4. Obtainium compara la firma del APK nuevo con la del instalado: si no
   coincide, no actualiza. Primera instalación desde la última release con
   etiqueta `android-v…` (la marca «Latest» de GitHub es del escritorio, no de
   la app).
5. Las actualizaciones de una candidata a la final (`rc` → final) se instalan
   encima sin desinstalar porque el `versionCode` de la final es mayor.

## 8. Syncthing-Fork en el móvil: «solo recibir»

La app es **lectora** de la sincronización: nunca escribe en la carpeta. En el
móvil, la carpeta compartida con los escritorios debe estar en modo **«solo
recibir»** (Receive Only) en Syncthing-Fork:

1. Instala Syncthing-Fork (F-Droid) y comparte la carpeta con el escritorio.
2. En el móvil, carpeta → tipo de carpeta → **Receive Only** («solo recibir»).
3. En MAGI, elige esa carpeta con el selector del sistema (permiso persistente,
   solo lectura) y únete a la sincronización con la clave o el QR del
   escritorio.
4. Si Syncthing muestra un conflicto o una copia en conflicto, no lo edites en
   el móvil: resuélvelo en el escritorio. El móvil nunca debe ser la fuente.

Así un error o un borrado en el teléfono no puede viajar a los escritorios.

## 9. Si pierdes el móvil o lo reinstalas

El móvil no figura como equipo en la sincronización (no escribe fichero), así
que no hay una «baja» suya concreta; hay que cortarle el acceso por los tres
lados:

1. **Clave de sincronización:** en el escritorio, MAGI → SINCRONIZACIÓN → `r`
   («cambiar la clave»). Los demás equipos pedirán la nueva; el móvil perdido
   ya no puede leer los ficheros.
2. **Syncthing:** quita el dispositivo del móvil de la carpeta compartida y
   revócalo en Syncthing. Así tampoco recibe los ficheros.
3. **Hosts:** borra de `~/.ssh/authorized_keys` la línea de la clave del móvil
   (identifícala con la huella que mostraba Ajustes → Clave del móvil). Si
   estaba instalada en varios hosts, en todos.
4. **El teléfono:** usa «Buscar mi dispositivo» / Find Hub de Android para
   bloquearlo y borrarlo. Las contraseñas guardadas y la clave de
   sincronización del móvil estaban cifradas con el Keystore y protegidas por
   el bloqueo del teléfono; si hay sospecha, cambia también las contraseñas de
   los hosts.
5. **Reinstalar la app:** Android borra los datos y las claves del Keystore;
   equivale a empezar de cero (§7.12 del informe de la Fase 11): crear otra
   del móvil, volver a unirse a la sincronización, reaceptar huellas y volver a
   escribir las contraseñas.

## 10. Si falla una release

- **Antes de crear la GitHub Release:** corrige el problema con un commit y
  publica una etiqueta nueva (`rc` o PATCH). **No muevas ni reutilices la
  etiqueta** que falló.
- **Después de crearla:** relanza solo los jobs fallidos
  (`gh run rerun <run-id> --failed`). No borres ni reemplaces la release: sus
  artefactos están atestados y sus `.sha256` son los que verifican los
  usuarios.
- **Si falla la firma:** revisa que los cuatro secretos existan y que el alias
  y las contraseñas sean correctos (no los pegues en ningún sitio); si el
  keystore no abre, para y avisa antes de tocar nada.
- **Si faltan notas del CHANGELOG:** el workflow usa la sección de la versión o,
  si no existe, la de «Sin publicar»; si tampoco hay, falla. Corrige el
  CHANGELOG y publica un PATCH.

## 11. Comprobaciones posteriores a publicar

```sh
gh release view android-vX.Y.Z
gh release list
```

Comprueba además:

- la release tiene el APK, su `.sha256` y la atestación;
- si la etiqueta es `-rc.N`, está marcada como **prerelease**;
- **no** está marcada como «Latest»: la última release del escritorio sigue
  siéndolo (`gh release view vX.Y.Z --json isLatest`);
- `sha256sum -c`, `apksigner verify --print-certs` y
  `gh attestation verify` funcionan con el APK descargado;
- instalado en el móvil, Acerca de muestra el `versionName` y el
  `versionCode` esperados;
- Obtainium detecta la versión nueva (o la candidata, si activaste
  prereleases).

Si algo falla, vuelve a §10: la release no se invalida y la etiqueta no se
mueve.

## 12. Referencias

- Informe de la Fase 11: §5.3 (workflows), §7.10 (versiones), §8 (requisitos
  no funcionales) y §11 (plan y guion de validación).
- [`publicar.md`](publicar.md): publicación del escritorio y política de
  versiones.
- [`android/CHANGELOG.md`](../android/CHANGELOG.md): cambios de la app.
- [`README.md`](../README.md): sección «App Android».
- Keep a Changelog: <https://keepachangelog.com/es-ES/1.1.0/>
- Obtainium: <https://github.com/ImranR98/Obtainium>
- Syncthing-Fork: <https://github.com/Catfriend1/syncthing-android>
- Atestaciones de procedencia:
  <https://docs.github.com/es/actions/security-for-github-actions/using-artifact-attestations>
- Firma de apps Android: <https://developer.android.com/studio/publish/app-signing>
- `apksigner`: <https://developer.android.com/tools/apksigner>
