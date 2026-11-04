# MAGI - Fase 11: App Android

## Especificación Funcional

**Versión:** 1.4
**Fecha:** 31 de octubre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

**Changelog**
- 4 nov 2026 — v1.4: «Android, segunda vuelta» pasa a ser la F13, porque la F12 es Escritorio Remoto (decisión de Hector al especificarla). Se actualizan las referencias; sin cambios de alcance.
- 4 nov 2026 — v1.3: procesada la actualización del informe de implementación con la corrección 11b (117/117). Causa: con solo túneles, nadie vigilaba la conexión del host; si se caía, el túnel seguía `activo` y no movía un byte. Ahora cada conexión tiene vigilante, y la fila enseña abiertas · aceptadas, el último error con su hora y el puerto real. Prueba de extremo a extremo en Rust (falla sin el vigilante) y en el emulador (con servidor de pruebas en el anfitrión; se omite en CI). Validada en el Honor. Recoge además dos cambios ya implementados: claves del Keystore sin `setUnlockedDeviceRequired` (1 nov, por el desbloqueo facial) y, a petición de Hector, terminal a pantalla completa y sin panel vacío en el plegable. Cambian §6.2, §6.4, §6 (notas), §7.5, §7.8, §8, §9 y el guion de §11. D205-D209.
- 4 nov 2026 — v1.2: validación de Hector en el plegable. Un túnel local activo no reenvía: el navegador del móvil da `ERR_SOCKET_NOT_CONNECTED`, los contadores siguen en cero, la fila no enseña error y lleva la nota de puerto libre sin haber usado esa opción. El mismo túnel funciona desde otro ordenador. Se desmarca la mecánica de túneles y se añade la corrección 11b (7 funcionalidades): reenvío real con prueba de extremo a extremo, `caido` si cae la conexión del host, último error y contadores abiertas · aceptadas en la fila, nota de puerto libre solo con esa opción y arreglos de texto y disposición. D203-D204.
- 1 nov 2026 — v1.1: ajustada al informe de implementación (110/110). La revisión adversarial de cierre encontró un crítico (la clave del móvil no se creaba porque se usaba `KeyGenerator` en vez de `KeyPairGenerator`, y toda conexión fallaba), cinco altos, siete medios y diez bajos, todos corregidos. Verificación en local: 126 pruebas JVM, 3 instrumentadas en el emulador API 30 y Rust en verde. Desviaciones: `cargo deny` sin `--workspace`; `release.yml` con `!android-v*`, porque el patrón de dist sí casaba esas etiquetas; `.github/actionlint.yaml`; la clave anterior del móvil se guarda con su fecha dentro de su propio secreto; el contrato UniFFI gana `pegado_entre_corchetes` y `FirmanteMovil.olvidar_clave`; el escritorio usa la tabla común de teclas (Ctrl, Alt y Mayús + flecha con `CSI 1;m`); el núcleo incluye también configuración, snippets y deliberación. Versiones reales del stack (AGP 9.5 con Kotlin integrado, JDK 21 en CI). R91 cerrado. D191-D202. Pendiente: validación en el plegable, CI de GitHub, primera release con keystore y corregir `docs/formato-sincronizacion.md` §11.9.

> Base: Fases 1-10 con informe procesado. La Fase 10 está al 63/63, sin validación manual (llavero real, Syncthing entre dos equipos, QR y `age` real) y con las limitaciones R83-R88. Hector decidió no meter aquí la corrección 10b, así que R83-R85 siguen abiertos. La Fase 9 sigue pendiente de su primera publicación (R81). Las Fases 2-6, 8 y 10 siguen sin validación manual completa (R15).
>
> Estado (4 nov 2026): implementación reportada, 117/117. La corrección 11b está validada en el Honor (el túnel local reenvía). Quedan el resto del guion de validación en el plegable (con el desbloqueo facial), la CI de GitHub, la primera release con keystore y los trámites de cierre del prompt de reanudación: corregir `docs/formato-sincronizacion.md` §11.9 (R103), el servidor de pruebas en `127.0.0.1` (R107) y las rutas del informe de implementación (R104).

---

## 1. Visión General

La Fase 11 lleva MAGI al móvil como **compañera de guardia**: ver el inventario que ya tienes en tus equipos, abrir una terminal SSH y levantar un túnel desde un Android, sin servidor ni cuenta. La app es nativa (Kotlin + Jetpack Compose) y comparte con el escritorio un **núcleo en Rust**: el mismo almacén, la misma sincronización de la F10 (formato, cifrado y fusión), la misma conexión SSH y el mismo parser de terminal. Para compartirlo, el repositorio pasa a ser un workspace de Cargo.

En esta primera versión el móvil es un **lector**: recibe el inventario de la carpeta sincronizada, pero no lo edita ni escribe su propio fichero. Entra en los hosts con una clave propia, creada en el chip de seguridad del teléfono y que nunca sale de él, o con contraseña. Las sesiones y los túneles siguen vivos con la app en segundo plano.

**Objetivos principales**

1. Núcleo compartido `magi-nucleo` en un workspace, movido desde el escritorio sin cambiar su comportamiento, y `magi-movil` como puerta UniFFI hacia Kotlin.
2. Inventario sincronizado de solo lectura: carpeta elegida con el selector de Android, unión por QR o pegando la clave, pasada en modo lector.
3. Terminal SSH con varias sesiones, huellas estrictas, saltos y autenticación con la clave del móvil (Android Keystore) o con contraseña.
4. Túneles local, remoto y dinámico del inventario, siempre en `127.0.0.1` y con «puerto libre» como salida.
5. Sesiones y túneles vivos en segundo plano (servicio en primer plano) y app protegida con huella o PIN.
6. APK firmado en CI y publicado en GitHub Releases con atestación, instalable y actualizable con Obtainium.

**Contexto.** Producto propio. El móvil de validación es un plegable Honor con MagicOS: pantalla exterior estrecha, interior casi cuadrada, y un sistema que cierra con agresividad las apps en segundo plano. Hector sincroniza con Syncthing y llega a sus hosts por Tailscale.

Respuestas de Hector (31 oct 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | Sin corrección 10b en esta fase (b) |
| 2 | Inventario de solo lectura y terminal, más túneles (c + túneles) |
| 3 | Kotlin + Compose con núcleo Rust por UniFFI (a) |
| 4 | Mismo repositorio como workspace y numeración propia (a) |
| 5 | Android 11+, arm64 y diseño para plegable (a) |
| 6 | Clave del móvil en el Keystore, contraseña e «instalar mi clave» (a) |
| 7 | Huella o PIN con 5 minutos de margen (a) |
| 8 | Carpeta por SAF y QR, sincronizando solo con la app abierta (a) |
| 9 | Servicio en primer plano y guía de batería (a) |
| 10 | Fila de teclas extra, zoom, selección y teclado físico (a) |
| 11 | Cabina de MAGI sobre Material 3 (a) |
| 12 | APK en GitHub Releases con Obtainium (a) |

**Fuera de alcance**

- Editar el inventario desde el móvil y, por tanto, escribir su fichero.
- Crear una sincronización desde el móvil.
- Flota y sondeo.
- Snippets y deliberación, incluido el snippet al conectar.
- SFTP.
- Vista de registro.
- mosh, reenvío del agente y claves FIDO.
- iOS, Google Play y F-Droid.

Todo esto queda para la F13 (Android, segunda vuelta) o para más adelante. La corrección 10b sigue pendiente.

**Versión prevista:** app **0.1.0** (etiqueta `android-v0.1.0`). El escritorio no cambia de versión por esta fase, porque la reorganización no se ve desde fuera.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Núcleo compartido | Rust stable, crate `magi-nucleo` | Movido del escritorio: almacén (rusqlite 0.40 bundled), sincronización (`age` 0.11.5, HMAC), conexión (russh 0.63), túneles, `vt100` 0.16 y registro. Sin ratatui, crossterm, tui-term, notify ni nada de Android |
| Puerta al móvil | Crate `magi-movil` (cdylib) con UniFFI `=0.32.2` | Bindings de Kotlin generados en cada compilación con el mismo UniFFI; runtime de tokio propio; un solo hilo escritor de SQLite |
| Compilación cruzada | NDK r30 LTS (30.0.16248370) y `cargo-ndk` 4.1.2 | `arm64-v8a` en la release; además `x86_64` en debug y en el emulador. Cripto de russh: `aws-lc-rs` por defecto, que compila y enlaza para las dos ABI (R91 cerrado) |
| App | AGP 9.5.0 con Kotlin integrado (2.4.21), Gradle 9.8.2, Jetpack Compose (BOM 2026.09.00), Material 3 y clases de tamaño de ventana | `minSdk` 30 (Android 11), `compileSdk` y `targetSdk` 37 (plataforma `android-37.0`); JDK 21 en CI |
| Puente nativo | JNA 5.19.1, que usa el Kotlin generado por UniFFI | Licencia Apache-2.0 (dual con LGPL) |
| Claves y secretos | Android Keystore | Clave del móvil ECDSA P-256 (StrongBox o TEE) y clave AES-256-GCM para los secretos |
| Desbloqueo | `androidx.biometric` 1.1.0 (BiometricPrompt) | Biometría fuerte o credencial del teléfono |
| Carpeta | Storage Access Framework | Árbol elegido por el usuario, con permiso persistente solo de lectura |
| QR | CameraX 1.6.2 + ZXing core 3.5.4 | Sin ML Kit ni Play Services |
| Preferencias | Jetpack DataStore 1.3.0 | Tamaño de letra, carpeta, guías vistas; nada secreto |
| Fuente | JetBrains Mono (OFL 1.1) | En el terminal y en toda la interfaz |
| CI | GitHub Actions: `ci.yml` (`--workspace`), `android.yml` y `android-release.yml`; `.github/actionlint.yaml` | Emulador con KVM para la prueba de humo; cargo-ndk descargado y comprobado por SHA-256 |
| Distribución | GitHub Releases (`android-vX.Y.Z`) + Obtainium | APK firmado con los esquemas v2 y v3, `.sha256` y atestación |

### Estructura de carpetas (novedades)

```
Cargo.toml                  [workspace] members = ["crates/magi-nucleo", "crates/magi-movil"]
                            + [package] magi-ssh (sin cambios de nombre, versión ni dist)
src/                        escritorio: TUI, servidor y protocolo, CLI, sshconfig, llavero del
                            sistema, QR a medios bloques, notify, copias, magi_config
                            (src/teclas.rs solo traduce crossterm a la tabla del núcleo)
crates/
├── magi-nucleo/            publish = false · fuera de dist
│   ├── src/
│   │   ├── lib.rs · rutas.rs (Rutas y Equipo inyectados) · configuracion.rs
│   │   ├── modelo.rs · almacen/ (migraciones 1-7, versiones, equipos, lápidas, conflictos, movil)
│   │   ├── sincronizacion/ (formato, cifrado, fusion, elementos, huellas, reloj, aplicacion,
│   │   │                    equipos, carpeta.rs: trait + escritorio, lector.rs: modo lector)
│   │   ├── conexion/ (cliente, saltos, huellas, reenvíos, terminal, abrir_con_plazo)
│   │   ├── tuneles/ (mecánica: local, remoto, socks5)
│   │   ├── terminal/ (parser vt100, tramos.rs, teclas.rs)
│   │   ├── claves/ (común, der_a_ssh.rs, x509_a_openssh.rs, firmante)
│   │   ├── instalar_clave.rs + scripts/instalar_clave.sh
│   │   ├── llavero.rs (trait LlaveroSecreto + LlaveroEnMemoria) · registro.rs · ficheros.rs
│   │   ├── snippets/ (variables, salida) · deliberacion.rs
│   │   └── trabajo_vivo.rs (trait TrabajoVivo)
│   └── tests/ (instalar_clave.rs, sincronizacion_lector.rs, generar_fixture_android.rs)
└── magi-movil/             crate-type = ["cdylib", "lib"] · publish = false · fuera de dist
    ├── uniffi.toml         paquete Kotlin org.cuatrodetres.magi.nucleo
    └── src/
        ├── lib.rs (uniffi::setup_scaffolding!) · api.rs (objeto Nucleo y funciones)
        ├── gestor.rs · estado.rs  sesiones, una conexión por host, túneles y automáticos
        ├── inventario.rs · sincro.rs · clave.rs
        ├── dialogos.rs     peticiones a Kotlin (huella, contraseña) y respuestas
        ├── retrollamadas.rs CarpetaMovil, LlaveroMovil, FirmanteMovil, OyenteNucleo
        ├── escritor.rs     hilo escritor de SQLite
        └── bin/uniffi-bindgen.rs (generador de bindings con el mismo UniFFI)
android/
├── settings.gradle.kts · build.gradle.kts · version.properties · CHANGELOG.md
├── gradle/ (libs.versions.toml, verification-metadata.xml, wrapper con distributionSha256Sum)
└── app/
    ├── build.gradle.kts    org.cuatrodetres.magi · minSdk 30 · targetSdk 37 · ndkVersion 30.0.16248370
    └── src/
        ├── main/AndroidManifest.xml
        ├── main/kotlin/org/cuatrodetres/magi/
        │   ├── MagiApp.kt · MainActivity.kt · nucleo/ (bindings generados y NucleoRepositorio)
        │   ├── plataforma/ (CarpetaSaf, LlaveroKeystore, ClaveMovil, Bloqueo, ServicioSesiones, Bateria)
        │   ├── ui/tema/ (Paleta, Tipografia) · ui/hosts/ · ui/ficha/ · ui/sesiones/ · ui/tuneles/
        │   ├── ui/terminal/ (VistaTerminal, EntradaTerminal, FilaTeclas) · ui/ajustes/ · ui/dialogos/
        │   └── navegacion/ (barra inferior, riel y dos paneles)
        ├── main/res/ (strings.xml en castellano, font/jetbrains_mono_*, xml/reglas_extraccion.xml)
        ├── test/ (JVM, con el núcleo compilado para el anfitrión)
        └── androidTest/ (PruebaHumo y PruebaHumoNucleo; fixture .age en assets/sincronizacion/)
.github/workflows/
├── ci.yml                  + --workspace
├── android.yml             nuevo
├── android-release.yml     nuevo (etiquetas android-v*)
└── release.yml             sin cambios, salvo !android-v* (el patrón de dist las casaba)
.github/actionlint.yaml     ignora tres avisos de ShellCheck, solo en el release.yml de dist
docs/
├── publicar-android.md     nuevo (keystore, secretos, etiqueta, verificación)
└── fase11/ (magi-fase11-informe.md · -prompt.md · -checklist.md · -implementacion.md)
```

### Diagrama de arquitectura: repositorio

```
 repositorio 4d3-org/magi (workspace de Cargo)
 ┌────────────────────────────┐   usa   ┌───────────────────────────────┐   usa   ┌──────────────────────────────┐
 │ magi-ssh (raíz)            │────────▶│ crates/magi-nucleo            │◀────────│ crates/magi-movil (cdylib)   │
 │ TUI · servidor · protocolo │         │ modelo · almacén (mig. 1-7)   │         │ UniFFI 0.32.2 · tokio        │
 │ CLI · magi_config · notify │         │ sincronización (escritor y    │         │ gestor: sesiones, conexión   │
 │ llavero del sistema · QR   │         │ lector) · conexión · saltos   │         │ por host, túneles · diálogos │
 │ copias de magi.db          │         │ huellas · túneles · vt100     │         │ hilo escritor de SQLite      │
 └─────────────┬──────────────┘         │ teclas · tramos · registro    │         └──────────────┬───────────────┘
               │                        └───────────────────────────────┘                        │ bindings Kotlin
               ▼                                                                                 ▼ + libmagi_movil.so
   tag vX.Y.Z ─▶ release.yml (dist)                                               android/ (Gradle, Compose)
   tarballs, Homebrew, AUR                                       tag android-vX.Y.Z ─▶ android-release.yml (APK)
```

### Diagrama de arquitectura: el móvil en marcha

```
 móvil (Android 11+)
 ┌───────────────────────────────────────────────────────────────────┐
 │ app Kotlin + Compose                                              │
 │  Hosts · Ficha · Sesiones · Terminal · Túneles · Ajustes          │
 │  plataforma: CarpetaSaf · LlaveroKeystore · ClaveMovil · Bloqueo  │
 │              ServicioSesiones (primer plano) · Bateria            │
 └───────┬────────────────────────────────────────────▲──────────────┘
         │ llamadas (suspend) por UniFFI + JNA        │ OyenteNucleo: pantallas en tramos (≤ 30 fps),
         ▼                                            │ listas completas de sesiones y túneles, diálogos
 ┌────────────────────────────────────────────────────┴──────────────┐
 │ libmagi_movil.so                                                  │   SSH (russh) por la red
 │  magi-movil: Nucleo · gestor (una conexión por host) · diálogos   │──────────────────────────▶ hosts
 │  magi-nucleo: almacén · sincronización (modo lector) · conexión   │   o por Tailscale
 │               túneles en 127.0.0.1 · vt100 · registro             │
 │  ficheros privados: magi.db (user_version 7) · known_hosts        │
 └──────────┬────────────────────────────────────────────────────────┘
            │ lee (SAF, permiso solo de lectura)
            ▼
 ┌──────────────────────────────┐          ┌───────────────────────────────────────────────┐
 │ carpeta sincronizada         │◀─────────│ escritorios con MAGI 1.1 o posterior (F10):   │
 │ magi-<uuid A>.age            │ escriben │ cada uno escribe su fichero; el móvil no      │
 │ magi-<uuid B>.age            │          │ escribe ninguno                               │
 │ (Syncthing-Fork, Nextcloud…) │          └───────────────────────────────────────────────┘
 └──────────────────────────────┘
 Android Keystore: clave del móvil (P-256, no exportable) · clave AES de LlaveroMovil
```

**Reparto**

- Lo que debe comportarse igual en el escritorio y en el móvil vive en `magi-nucleo`. Kotlin no tiene fusión, formato, cifrado, SSH, túneles, parser ni codificación de teclas.
- `magi-movil` hace en el móvil lo que en el escritorio hace el servidor de sesiones (custodiar sesiones, conexiones y túneles), pero dentro del proceso de la app: no hay `magi --servidor` ni socket.
- Kotlin pone la interfaz y la plataforma: SAF, Keystore, biometría, cámara, notificación y servicio.
- Rige lo mismo que entre cliente y servidor en el escritorio (T13, T27): la app recibe pantallas ya interpretadas y listas completas de sesiones y túneles, nunca bytes del remoto ni mensajes finos.
- `docs/formato-sincronizacion.md` sigue siendo la referencia del formato. El móvil no lo reimplementa: usa el mismo código.

---

## 3. Modelo de Datos

### Diagrama E-R (`magi.db` del móvil)

```
 magi.db del móvil: el mismo esquema que el escritorio (user_version 7), creado por el núcleo
 ┌─────────────────────────── espejo de solo lectura: solo lo escribe la pasada ────────────────────────────┐
 │  GRUPOS ──< HOSTS >──< ETIQUETAS (vía HOST_ETIQUETAS)        HOSTS ──< TUNELES (los usa el móvil)        │
 │               │ salto_host_id (autorreferencia)              IDENTIDADES · SNIPPETS · SNIPPET_DESTINOS · │
 │               │                                              VERIFICACIONES_HOST · SINCRONIZACIONES_DIR  │
 │               │                                              (viajan y se guardan; el móvil no las usa)  │
 └───────────────┼──────────────────────────────────────────────────────────────────────────────────────────┘
                 │ campos propios del móvil (no suben versión ni viajan):
                 │ GRUPOS.plegado · HOSTS.ultimo_estado · HOSTS.ultima_conexion_en
 ┌───────────────▼───────────────┐  ┌──────────────────────────┐  ┌─────────────────────────────────────────┐
 │ EQUIPOS                       │  │ LAPIDAS                  │  │ CONFLICTOS                              │
 │ fila del móvil: es_este = 1,  │  │ las que llegan de otros  │  │ solo tipo huella (se resuelve aquí) y   │
 │ plataforma android (no sale   │  │ equipos                  │  │ borrado (se aplica solo al acabar el    │
 │ del móvil) · resto: su estado │  └──────────────────────────┘  │ trabajo vivo)                           │
 └───────────────────────────────┘                                └─────────────────────────────────────────┘
 ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │ REGISTRO (local): conexion_*, sesion_*, tunel_*, huella_*, inventario_importado, clave_generada,         │
 │ clave_instalada (nuevo)                                                                                  │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
 Fuera de magi.db: known_hosts privado · LlaveroMovil (clave de sincronización, anterior y contraseñas por
 uuid de host) · clave del móvil en el Keystore · DataStore (preferencias, nada secreto)
```

**Sin tablas nuevas ni migración 8.** El móvil crea `magi.db` con las migraciones 1-7 del núcleo. La fila de EQUIPOS de este equipo toma el nombre y la plataforma que pasa la app (el modelo del teléfono y `android`), no el hostname.

### Qué escribe el móvil y dónde

| Dato | Dónde | Quién lo escribe en el móvil |
|---|---|---|
| Tablas que viajan (GRUPOS, HOSTS con etiquetas y verificaciones, IDENTIDADES, TUNELES, SNIPPETS con destinos, SINCRONIZACIONES_DIR) | `magi.db` | Solo la pasada en modo lector, con las versiones tal cual |
| `GRUPOS.plegado`, `HOSTS.ultimo_estado`, `HOSTS.ultima_conexion_en` | `magi.db` | La app (campos propios: no suben versión ni viajan) |
| EQUIPOS | `magi.db` | La pasada; la fila del móvil (`es_este = 1`, plataforma `android`) solo existe en el móvil |
| LAPIDAS | `magi.db` | La pasada (las que llegan) |
| CONFLICTOS | `magi.db` | La pasada, solo de tipo `huella` y `borrado`; la app los resuelve |
| REGISTRO | `magi.db` | `registro::anotar`: `conexion_abierta`, `conexion_fallida`, `sesion_cerrada`, `sesion_reconectada`, `tunel_*`, `huella_aceptada`, `huella_sustituida`, `huella_sincronizada`, `inventario_importado`, `clave_generada` (clave del móvil) y `clave_instalada` (nuevo). Se purga a los 90 días; sin vista en esta fase |
| Huellas de los hosts | `known_hosts` privado de la app | Aceptar, sustituir e importar de otros equipos |
| Clave de sincronización (actual y anterior, esta con su fecha límite) y contraseñas | `LlaveroMovil`: ficheros en `filesDir/secretos/` cifrados con AES-256-GCM (clave del Keystore) | La app; contraseñas por `uuid` de host (los `id` cambian al rehacer el inventario). La caducidad de la anterior va dentro de su propio secreto, `sincronizacion-anterior` (D195) |
| Clave del móvil | Android Keystore (`magi-clave-movil`; otro alias tras la migración de D205) | Se crea una vez (y otra en esa migración); no se puede exportar |
| Preferencias (carpeta, tamaño de letra, sesiones abiertas para el aviso, guías vistas) | DataStore | La app; nada secreto |

### Diagrama de estados: sincronización en el móvil (lector)

```
   sin_configurar ─ elegir carpeta (SAF, solo lectura) ─▶ sin_clave
        sin_clave ─ la carpeta no tiene magi-*.age ─▶ sin_clave («crea la sincronización en el escritorio»)
        sin_clave ─ clave escaneada o pegada que abre un fichero con firma válida ─▶ activa (primera pasada)
        sin_clave ─ clave que no abre ─▶ sin_clave (reintentar) · atrás ─▶ sin_configurar
   activa ⇄ sincronizando (al abrir, al volver, cada 60 s visible, al deslizar en Hosts)
        sincronizando ─ un equipo conocido no abre con la clave ─▶ esperando_clave (aviso; inventario intacto)
        esperando_clave ─ el fichero vuelve a abrir (estaba a medio copiar) ─▶ activa
        esperando_clave ─ clave nueva escaneada ─▶ activa (la anterior solo lee, 30 días)
        sincronizando ─ carpeta inaccesible (permiso retirado, borrada) ─▶ error_carpeta
        error_carpeta ─ elegir otra vez la carpeta ─▶ activa
   activa · esperando_clave · error_carpeta ─ dejar de sincronizar (sin sesiones ni túneles) ─▶ sin_configurar
  Inválidas: escribir o borrar en la carpeta; crear una sincronización desde el móvil; importar sin firma
  válida; dejar de sincronizar con sesiones o túneles abiertos; pasar a esperando_clave por un fichero
  desconocido (solo avisa).
```

### Diagrama de estados: sesión (móvil, en memoria)

```
   [sin sesión] ─ conectar ─▶ abriendo (resolviendo · conectando · verificando_huella · autenticando)
        abriendo ─ huella desconocida ─▶ [diálogo] aceptar ─▶ sigue · cancelar ─▶ [sin sesión]
        abriendo ─ huella cambiada ─▶ [bloqueo] sustituir escribiendo el nombre ─▶ sigue · cancelar ─▶ [sin sesión]
        abriendo ─ clave rechazada ─▶ contraseña guardada ─▶ [diálogo contraseña ×3] ─ cancelar ─▶ [sin sesión]
        abriendo ─ error (DNS, plazo de 10 s, rechazo, teléfono bloqueado) ─▶ [sin sesión] (ultimo_estado = error)
        abriendo ─▶ abierta (ultimo_estado = ok · ultima_conexion_en)
   abierta ─ exit remoto ─▶ eliminada
   abierta ─ cerrar (confirmación) ─▶ eliminada
   abierta ─ red caída o conexión del host caída ─▶ caida ─ reconectar ─▶ abriendo (mismo nombre)
   caida ─ cerrar ─▶ eliminada
   (Android mata el proceso) ─▶ todas eliminadas; al volver, aviso y guía de batería
  Inválidas: reconectar una sesión abierta; caida → abierta sin pasar por abriendo; eliminada → cualquiera
  (el id no se reutiliza).
```

### Diagrama de estados: túnel (móvil, en memoria)

```
   inactivo ─ interruptor · automático (primera sesión del host) ─▶ activando
        activando ─ puerto ocupado o menor que 1024 ─▶ caido [Abrir en un puerto libre ─▶ activando (puerto 0)]
        activando ─ el host rechaza el reenvío o la conexión falla ─▶ caido
        activando ─▶ activo (local y dinámico escuchan en 127.0.0.1)
   activo ─ interruptor · automático (última sesión del host) ─▶ parando ─▶ inactivo
   activo ─ conexión del host caída ─▶ caido ─ relanzar ─▶ activando
   caido ─ interruptor ─▶ inactivo (descarta el error)
  Inválidas: activo → activando; parando → activo; caido → activo sin relanzar; escuchar fuera de 127.0.0.1
  (local y dinámico).
```

### Diagrama de estados: bloqueo de la app

```
   sin_bloqueo_de_pantalla (el teléfono no tiene PIN, patrón ni contraseña) ─▶ «configura un bloqueo» (no sigue)
   arranque en frío ─▶ bloqueada
   bloqueada ─ huella o PIN correctos ─▶ desbloqueada
   desbloqueada ─ sale de la app ─▶ fuera (anota la hora)
   fuera ─ vuelve antes de 5 min ─▶ desbloqueada
   fuera ─ vuelve después de 5 min ─▶ bloqueada
  Inválidas: enseñar inventario, terminal o ajustes en bloqueada; cortar sesiones o túneles por bloquear.
```

### Diagrama de estados: servicio en primer plano

```
   parado ─ se abre la primera sesión o el primer túnel ─▶ en_marcha («MAGI · N sesiones · M túneles»)
   en_marcha ─ cambia el número ─▶ en_marcha (actualiza la notificación)
   en_marcha ─ se cierran la última sesión y el último túnel ─▶ parado
   en_marcha ─ Android mata el proceso ─▶ parado (al volver: aviso y guía de batería)
  Inválidas: en_marcha sin sesiones ni túneles; una acción en la notificación que cierre algo (T6).
```

### Diagrama de estados: clave del móvil

```
   sin_crear ─ primer arranque desbloqueado ─▶ creada (StrongBox o TEE) · REGISTRO clave_generada
   creada ─ Android la invalida (se quitó el bloqueo de pantalla) ─▶ invalidada ─ crear otra ─▶ creada
   creada ─ borrar todos los datos (escribiendo «borrar») ─▶ sin_crear
  Inválidas: exportar la privada; firmar con el teléfono bloqueado.
```

### Diagrama de estados: conflicto de huella (móvil)

```
   pendiente ─ aceptar la del otro equipo ─▶ aceptado (sustituye en el known_hosts del móvil · huella_sustituida)
   pendiente ─ mantener la mía ─▶ mantenido (no toca known_hosts)
   aceptado · mantenido ─ 90 días ─▶ purgado
  Inválida: sustituir una huella sin pasar por «aceptar».
```

El estado de cada **otro equipo** (`al_dia`, `ilegible`, `danado`, `formato_nuevo`, `quitado`) es el mismo de la F10 (§3 de su informe).

---

## 4. Flujos de Trabajo

### 4.1 Primer arranque y unión

```
  abrir MAGI ─▶ ¿el teléfono tiene bloqueo de pantalla? ─no─▶ «MAGI necesita un PIN, patrón o contraseña» (fin)
      ▼ sí
  desbloqueo (huella o PIN) ─ cancelar ─▶ sigue bloqueada
      ▼
  ¿clave del móvil? ─no─▶ crear P-256 en el Keystore (StrongBox; si no hay, TEE) ─▶ REGISTRO clave_generada
      ▼
  ¿sincronización configurada? ─no─▶ asistente: «elige la carpeta que sincroniza tu escritorio»
      ▼ selector del sistema (solo lectura) ─ cancelar ─▶ Hosts vacío con «Configurar sincronización»
  listar magi-*.age ─ ninguno ─▶ «esta carpeta no tiene ficheros de MAGI: crea la sincronización en el escritorio»
      ▼ hay
  escanear el QR (cámara) o pegar la clave (y vaciar el portapapeles) ─▶ descifrar un fichero y comprobar su firma
      ├─ no abre ─▶ «la clave no abre los ficheros de esta carpeta» ─▶ reintentar
      └─ abre ─▶ LlaveroMovil ─▶ pasada completa (modo lector) ─▶ Hosts con el inventario
```

### 4.2 Una pasada en modo lector

```
  disparo: abrir · volver · cada 60 s con la app visible · deslizar hacia abajo en Hosts
      ▼ ¿pasada en curso? ─sí─▶ se ignora
  CarpetaMovil.listar (SAF) ─ permiso retirado o carpeta borrada ─▶ error_carpeta
      ▼ solo magi-<uuid>.age con uuid canónico y tamaño ≤ máximo (T81)
  leer los de hash distinto al último importado (todos, si hay borrados aplazados pendientes)
      ├─ no abre (equipo conocido) ─▶ esperando_clave · se sigue con el inventario actual
      ├─ firma mala o cortado ─▶ equipo danado · se reintenta en la siguiente
      ├─ formato nuevo ─▶ equipo formato_nuevo · aviso «actualiza MAGI»
      └─ válido
  fusionar (núcleo, igual que el escritorio) ─▶ plan
      ▼ borrados de hosts o túneles con trabajo vivo en el móvil ─▶ aplazados (CONFLICTOS borrado)
      ▼ aplazados cuyo trabajo ya terminó ─▶ se aplican
  aplicar en una transacción (sin copia, sin preguntar) ─▶ huellas: añadir si no hay; conflicto si hay otra
      ▼ REGISTRO inventario_importado si cambió algo
  nunca exportar · actualizar ⇅ y la pantalla Sincronización
```

### 4.3 Conectar

```
  ▶ en una fila · «Conectar» u «Otra sesión» en la ficha
      ▼ ¿hay conexión viva con ese host? ─sí─▶ canal nuevo sobre ella ────────────────────────────┐
      ▼ no                                                                                        │
  ¿salto? ─▶ abrir el salto (mismo flujo, máx. 3 niveles) ─ error ─▶ «salto X: motivo»            │
      ▼                                                                                           │
  resolver + TCP + saludo SSH (10 s) ─ falla ─▶ error con motivo · ultimo_estado = error          │
      ▼                                                                                           │
  huella (known_hosts del móvil): conocida ─▶ sigue · nueva ─▶ [diálogo] · cambiada ─▶ [bloqueo]  │
      ▼                                                                                           │
  autenticar: clave del móvil (firma en el Keystore) ─ rechazada ─▶ contraseña del LlaveroMovil   │
      ─ no hay o rechazada ─▶ [diálogo contraseña ×3, «recordar en este móvil»] ─ cancelar ─▶ fin │
      ▼                                                                                           │
  canal + pty (columnas × filas de la vista, TERM=xterm-256color) + shell ◀───────────────────────┘
      ▼ servicio en primer plano (si es la primera) · automáticos del host · ultimo_estado = ok
  terminal
```

### 4.4 Instalar mi clave en un host

```
  ficha ─ «Instalar mi clave» ─▶ confirmación («se añadirá a ~/.ssh/authorized_keys de usuario@192.0.2.5»)
      ▼ conexión efímera solo con contraseña (huella como siempre)
  exec: sh -c '<instalar_clave.sh>' magi '<línea pública validada>'
      ├─ YA_ESTABA ─▶ «la clave ya estaba»
      ├─ INSTALADA ─▶ REGISTRO clave_instalada
      └─ error (sin permisos, disco lleno) ─▶ mensaje con la salida de error
      ▼
  comprobar: conexión nueva solo con la clave del móvil
      ├─ entra ─▶ «lista: desde ahora entras con la clave del móvil»
      └─ no entra ─▶ «instalada, pero el host no la acepta (¿AuthorizedKeysFile en otro sitio?)»
```

### 4.5 Activar un túnel local

```
  interruptor ─▶ conexión del host (como 4.3, con diálogos) ─▶ bind 127.0.0.1:puerto
      ├─ en uso o menor que 1024 ─▶ caido «puerto 80 reservado en Android» [Abrir en un puerto libre]
      │                                  ─▶ bind 127.0.0.1:0 ─▶ activo en 127.0.0.1:41873 (se muestra; no se guarda)
      └─ ok ─▶ activo ─▶ [Abrir en el navegador] ─▶ http://127.0.0.1:puerto
  por cada conexión aceptada ─▶ direct-tcpip(destino) ─ rechazo ─▶ cierra esa conexión, último error, sigue activo
```

### 4.6 Segundo plano

```
  primera sesión o túnel ─▶ ¿notificaciones permitidas? ─no─▶ pedir permiso (Android 13+)
      ▼ servicio specialUse + notificación «MAGI · 1 sesión»
      ▼ la primera vez ─▶ guía de batería (petición del sistema + pasos de MagicOS) · «no volver a mostrar»
  salir de MAGI ─▶ sesiones y túneles siguen; la app se bloquea a los 5 min
  volver ─▶ desbloqueo si hace falta ─▶ todo donde estaba
  Android mata MAGI ─▶ al volver: «Android cerró MAGI con 2 sesiones abiertas» + guía de batería
  se cierran la última sesión y el último túnel ─▶ servicio parado
```

### 4.7 Diagrama de secuencia: abrir una sesión con la clave del móvil

```
 Compose   NucleoRepositorio     magi-movil (Rust)      FirmanteMovil (Keystore)                  host
   │─conectar(web)▶│─abrir_sesion(uuid)─▶│─ resolver · TCP · saludo ───────────────────────────────▶│
   │               │                     │◀────────────────────────────────────────── clave de host │
   │               │                     │ known_hosts del móvil: conocida                          │
   │               │                     │─ publickey ecdsa-sha2-nistp256 (consulta) ──────────────▶│
   │               │                     │◀────────────────────────────────────────────── aceptable │
   │               │                     │─firmar(datos)───────────▶│ SHA256withECDSA (TEE)         │
   │               │                     │◀────────────── firma DER │                               │
   │               │                     │─ DER → SSH (r, s) ──────────────────────────────────────▶│
   │               │                     │◀──────────────────────────────────────────── autenticado │
   │               │                     │─ pty 52×34 + shell ─────────────────────────────────────▶│
   │               │◀─── sesiones(lista) │                          │                               │
   │◀─── pantalla(tramos) · OyenteNucleo │◀────────────────────────────────────────────────── datos │
```

### 4.8 Diagrama de secuencia: pasada por SAF

```
 ViewModel           magi-movil  CarpetaMovil (SAF)         núcleo (fusión)                  magi.db
   │─sincronizar_ahora()─▶│─listar()─────▶│ hijos del árbol         │                           │
   │                      │◀── magi-*.age │                         │                           │
   │                      │ hash distinto │                         │                           │
   │                      │─leer(a, máx.)▶│                         │                           │
   │                      │◀─────── bytes │                         │                           │
   │                      │─ descifrar · firma · equipo · formato ─▶│                           │
   │                      │               │                         │◀─────────── lee el estado │
   │                      │               │                         │ fusionar · plan lector    │
   │                      │─ hilo escritor: aplicar en una transacción ────────────────────────▶│
   │◀───── estado(activa) │               │                         │                           │
```

---

## 5. Interfaz del Núcleo, Workflows y Configuración

En este proyecto la «API» es la interfaz que `magi-movil` expone a Kotlin con UniFFI. Las funciones de red son `suspend` en Kotlin. Lo que el núcleo necesita de la plataforma lo pide por retrollamadas.

### 5.1 Funciones del núcleo (UniFFI)

| Recurso | Función | Descripción |
|---|---|---|
| Núcleo | `Nucleo::iniciar(rutas, equipo, retrollamadas)` | Abre o crea `magi.db` (migraciones 1-7) y arranca el runtime y el hilo escritor |
| Núcleo | `version()` | Versión de la app, del núcleo, `formato` de sincronización y `user_version` |
| Inventario | `inventario_grupos()` | Grupos con número de hosts y plegado |
| Inventario | `inventario_hosts(filtro)` | Hosts con estado, sesiones abiertas y etiquetas; filtro por subcadena |
| Inventario | `ficha_host(uuid)` | Ficha de solo lectura, huellas del móvil y si hay contraseña guardada |
| Inventario | `plegar_grupo(uuid, plegado)` | Campo propio; no viaja |
| Sincronización | `sincronizacion_estado()` | Estado, carpeta, huella corta de la clave, última pasada, equipos y conflictos de huella |
| Sincronización | `elegir_carpeta(uri)` | Guarda el árbol SAF (la app ya tiene el permiso) y comprueba que hay ficheros de MAGI |
| Sincronización | `unir_con_clave(texto)` | Normaliza y valida la clave contra un fichero, la guarda en `LlaveroMovil` y lanza la primera pasada |
| Sincronización | `sincronizar_ahora()` | Pasada en modo lector; si ya hay una en curso, no hace nada |
| Sincronización | `dejar_de_sincronizar()` | Falla si hay sesiones o túneles; borra la clave y el inventario |
| Huellas | `conflictos_huella()` · `aceptar_huella_de_otro_equipo(id)` · `mantener_mi_huella(id)` | Conflictos de huella del móvil |
| Sesiones | `abrir_sesion(host_uuid)` | Flujo de §4.3; devuelve el id; los diálogos llegan por `OyenteNucleo` |
| Sesiones | `escribir(sesion, bytes)` · `tecla(sesion, tecla, modificadores)` · `pegar(sesion, texto)` | `pegar` usa el pegado entre corchetes si el remoto lo activó |
| Sesiones | `pegado_entre_corchetes(sesion)` | (Implementación, D196) Si el remoto activó el pegado entre corchetes; la app lo usa para decidir si pide confirmación al pegar varias líneas |
| Sesiones | `redimensionar(sesion, columnas, filas)` | Saneado (mín. 2×1) y agrupado 50 ms |
| Sesiones | `desplazar(sesion, filas)` · `texto_seleccion(sesion, desde, hasta)` | Historial y copia |
| Sesiones | `reconectar(sesion)` · `cerrar_sesion(sesion)` | |
| Túneles | `tuneles(host_uuid)` | Túneles del inventario con su estado en vivo (todos si `host_uuid` es nulo): estado y motivo, puerto real, conexiones abiertas y aceptadas, bytes y último error con su hora (D207) |
| Túneles | `activar_tunel(uuid)` · `parar_tunel(uuid)` · `relanzar_tunel(uuid)` · `activar_en_puerto_libre(uuid)` | |
| Diálogos | `responder_dialogo(id, respuesta)` | Huella nueva (aceptar o cancelar), huella cambiada (nombre escrito o cancelar), contraseña (bytes, recordar o cancelar) |
| Clave del móvil | `clave_publica_movil()` | Línea OpenSSH y huella SHA256, a partir de la X.509 que da `FirmanteMovil` |
| Clave del móvil | `instalar_clave_en_host(host_uuid)` | §4.4 y §7.9 |
| Datos | `olvidar_contrasena(host_uuid)` · `borrar_datos(alcance)` | `alcance`: `sincronizacion` (lo que hace «dejar de sincronizar») o `todo` |

### 5.2 Retrollamadas (las implementa Kotlin)

| Interfaz | Métodos | Implementación |
|---|---|---|
| `CarpetaMovil` | `listar()` → nombre, tamaño y fecha; `leer(nombre, maximo)` → bytes | `DocumentsContract` sobre el árbol elegido |
| `LlaveroMovil` | `leer(clave)`, `guardar(clave, secreto)`, `borrar(clave)` | Ficheros en `filesDir/secretos/` cifrados con AES-256-GCM (clave `magi-secretos` del Keystore). `magi-movil` lo adapta a `LlaveroSecreto` |
| `FirmanteMovil` | `clave_publica_x509()` → bytes; `firmar(datos)` → firma DER; `olvidar_clave()` (implementación, D196: clave invalidada o «borrar todo») | `SHA256withECDSA` con `magi-clave-movil`; el núcleo lo llama fuera del hilo de tokio |
| `OyenteNucleo` | `pantalla(sesion, tramos)`, `sesiones(lista)`, `tuneles(lista)`, `dialogo(peticion)`, `sincronizacion(estado)` | ViewModels de Compose (estado observable) |

### 5.3 Workflows

| Workflow | Disparo | Qué hace |
|---|---|---|
| `ci.yml` | PR y `main` | Lo mismo que en la F9, con `--workspace` en tests y clippy; `cargo deny check` va sin esa opción, porque ya analiza todo el workspace (D191) |
| `android.yml` | PR y `main` con cambios en `crates/` o `android/` | Núcleo con `cargo-ndk` (`arm64-v8a`, `x86_64`) y `./gradlew lint testDebugUnitTest assembleDebug`. En `main` y en etiquetas `android-v*`, además, prueba de humo en el emulador (API 30, x86_64) |
| `android-release.yml` | Etiqueta `android-vX.Y.Z[-rc.N]` | Comprueba que la etiqueta coincide con `android/version.properties`; compila el APK release `arm64-v8a` y comprueba con `aapt2` su `versionName` y su `versionCode`; firma con sus secretos (Gradle solo firma si recibe las variables `MAGI_KEYSTORE_*`); `apksigner verify`; `.sha256`; atestación; GitHub Release (prerelease si es `-rc`) con las notas del CHANGELOG y la huella del certificado; nunca «Latest» (`--latest=false` y comprobación de `isLatest`) |
| `release.yml` | Etiquetas de versión (patrón de dist) | Sin cambios de lógica. Su patrón (`**[0-9]+.[0-9]+.[0-9]+*`) casaba `android-v*`, así que se excluyen con `!android-v*` (D192) |

Secretos de `android-release.yml`, que no recibe ningún otro: `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` y `ANDROID_KEY_PASSWORD`. Los crea el desarrollador (T69).

### 5.4 Configuración en el móvil

No hay `config.toml`. DataStore guarda `carpeta_uri`, `tamano_letra` (8-24 sp; por defecto 13), `sesiones_abiertas` (para el aviso de «Android cerró MAGI») y `guia_bateria_vista`. La caducidad de la clave anterior va con ella en su secreto cifrado (D195). Los umbrales de la F10 no aplican: el móvil no pregunta por borrados (D173).

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 [arranque] ─▶ BLOQUEO ─▶ (sin sincronizar) ASISTENTE: carpeta ─▶ unir (QR o pegar)
                       └▶ HOSTS (inicio)
 Barra inferior (compacta) · riel de navegación (plegable abierto o pantalla ancha):
 ├── HOSTS ── buscar · deslizar hacia abajo = sincronizar · indicador ⇅
 │     ├── ▶ en una fila ─▶ TERMINAL
 │     └── fila ─▶ FICHA ─┬─ Conectar · Otra sesión ─▶ TERMINAL
 │                        ├─ Túneles del host (interruptores, abrir en el navegador)
 │                        ├─ Instalar mi clave ─▶ [confirmación] ─▶ [CONTRASEÑA]
 │                        ├─ Olvidar contraseña ─▶ [confirmación]
 │                        └─ Conflicto de huella ─▶ [aceptar la del otro equipo · mantener la mía]
 ├── SESIONES ── lista (● abierta · ◐ abriendo · ✕ caída) ─▶ TERMINAL
 │                 TERMINAL ⋮: reconectar · cerrar [confirmación] · pegar · tamaño de letra
 ├── TÚNELES ── por host: interruptor · relanzar · abrir en un puerto libre · abrir en el navegador
 └── AJUSTES ─┬─ SINCRONIZACIÓN ─ sincronizar ahora · volver a escanear la clave · dejar de sincronizar
              ├─ CLAVE DEL MÓVIL ─ copiar · compartir · QR
              ├─ TERMINAL ─ tamaño de letra por defecto
              ├─ BATERÍA ─ guía
              ├─ BORRAR TODOS LOS DATOS [escribir «borrar»]
              └─ ACERCA DE ─ LICENCIAS
 Desde cualquier conexión: [HUELLA NUEVA] · [HUELLA CAMBIADA] · [CONTRASEÑA]
```

### 6.1 Hosts (compacta)

```
┌──────────────────────────────────────┐
│ MAGI · HOSTS             ⇅ al día  ⋮ │
│ ┌──────────────────────────────────┐ │
│ │ buscar…                          │ │
│ └──────────────────────────────────┘ │
│ ▾ 4D3 · PRODUCCIÓN                   │
│ ●2 web-01                         ▶  │
│    usuario@192.0.2.10:22             │
│    [web] [prod]                      │
│ ○  db-01                          ▶  │
│    usuario@192.0.2.11:22             │
│ ✕  cache                          ▶  │
│    usuario@192.0.2.12:2222           │
│ ▸ LABORATORIO (3)                    │
│ ▾ SIN GRUPO                          │
│ ○  vps-salto                      ▶  │
│    usuario@salto.ejemplo.org:22      │
├──────────────────────────────────────┤
│ HOSTS   SESIONES 2   TÚNELES  AJUSTES│
└──────────────────────────────────────┘
```

### 6.2 Ficha de host

```
┌──────────────────────────────────────┐
│ ← web-01                          ●2 │
│ dirección   192.0.2.10:22            │
│ usuario     usuario                  │
│ grupo       4d3 · producción         │
│ etiquetas   web · prod               │
│ salto       vps-salto                │
│ keepalive   30 s (móvil)             │
│ escritorio  agente:SHA256:hq3K…      │
│ en el móvil clave del móvil, luego   │
│             contraseña (no guardada) │
│ huella      ed25519 SHA256:p91X…c2Qz │
│ ──────────────────────────────────── │
│ TÚNELES                              │
│ ● grafana  L 127.0.0.1:3000          │
│            → 127.0.0.1:3000 auto [■] │
│   2 abiertas · 9 aceptadas   [abrir] │
│   ↑14 KB ↓1,2 MB                     │
│ ○ socks    D 127.0.0.1:1080      [□] │
│ ──────────────────────────────────── │
│ [ CONECTAR ]  [ OTRA SESIÓN ]        │
│ [ INSTALAR MI CLAVE ]                │
└──────────────────────────────────────┘
```

### 6.3 Terminal (compacta, con el teclado abierto)

```
┌──────────────────────────────────────┐
│ ← web-01 (2) ▾      ● abierta      ⋮ │
│usuario@web-01:~$ systemctl status ngi│
│● nginx.service - A high performance  │
│     Loaded: loaded (/lib/systemd/sys │
│     Active: active (running) since … │
│   Main PID: 812 (nginx)              │
│usuario@web-01:~$ █                   │
│                                      │
├──────────────────────────────────────┤
│ Esc Tab Ctrl Alt ← ↑ ↓ → | ~ / -   › │
├──────────────────────────────────────┤
│                                      │
│        [ teclado del sistema ]       │
│                                      │
└──────────────────────────────────────┘
```

### 6.4 Plegable abierto (dos paneles: lista y ficha)

```
┌─────────┬──────────────────────────┬──────────────────────────────────────────────────────┐
│ ▣ Hosts │ HOSTS          ⇅ al día  │ ← web-01                                          ●2 │
│         │ buscar…                  │ dirección   192.0.2.10:22    usuario    usuario      │
│ □ Sesión│ ▾ 4D3 · PRODUCCIÓN       │ grupo       4d3 · producción etiquetas  web · prod   │
│         │ ●2 web-01            ▶   │ salto       vps-salto        keepalive  30 s (móvil) │
│ □ Túnel │ ○  db-01             ▶   │ huella      ed25519 SHA256:p91X…c2Qz                 │
│         │ ✕  cache             ▶   │ ──────────────────────────────────────────────────── │
│ □ Ajust.│ ▸ LABORATORIO (3)        │ TÚNELES                                              │
│         │ ▾ SIN GRUPO              │ ● grafana L 127.0.0.1:3000 → 127.0.0.1:3000 auto [■] │
│         │ ○  vps-salto         ▶   │   2 abiertas · 9 aceptadas · ↑14 KB ↓1,2 MB  [abrir] │
│         │                          │ ○ socks   D 127.0.0.1:1080                       [□] │
│         │                          │ [ CONECTAR ]  [ OTRA SESIÓN ]  [ INSTALAR MI CLAVE ] │
└─────────┴──────────────────────────┴──────────────────────────────────────────────────────┘
```

Desde la v1.3 (D209, petición de Hector): al abrir una sesión, el terminal ocupa toda la pantalla, sin riel, lista ni barra inferior y con las barras del sistema ocultas (aparecen al deslizar); al volver, se recupera esta disposición. Sin detalle seleccionado, la lista ocupa todo el ancho: no hay panel «Elige un host…».

### 6.5 Sincronización (Ajustes)

```
┌──────────────────────────────────────┐
│ ← SINCRONIZACIÓN                     │
│ ⇅ al día · hace 1 min                │
│ carpeta   Sync/magi (solo lectura)   │
│ clave     7f3a·91c2 (Keystore)       │
│ este móvil  lector: no escribe       │
│ ──────────────────────────────────── │
│ EQUIPOS                              │
│ omarchy-portatil  linux   1.1.0      │
│   hace 2 min      ● al día           │
│ mac-mini          macos   1.1.0      │
│   hace 1 h        ● al día           │
│ ──────────────────────────────────── │
│ ! 1 conflicto de huella (db-01)      │
│ ──────────────────────────────────── │
│ [ SINCRONIZAR AHORA ]                │
│ [ VOLVER A ESCANEAR LA CLAVE ]       │
│ [ DEJAR DE SINCRONIZAR ]             │
└──────────────────────────────────────┘
```

### 6.6 Diálogo HUELLA CAMBIADA

```
┌──────────────────────────────────────┐
│ ✕ HUELLA CAMBIADA · db-01            │
│ La clave del servidor NO coincide    │
│ con la que guarda este móvil.        │
│ Anterior  ed25519 SHA256:hq3K…8fMw   │
│ Nueva     ed25519 SHA256:p91X…c2Qz   │
│ Puede ser una reinstalación… o un    │
│ intermediario.                       │
│ Para sustituirla escribe «db-01»:    │
│ [____________]                       │
│ [CANCELAR (recomendado)] [SUSTITUIR] │
└──────────────────────────────────────┘
```

**Diálogo HUELLA NUEVA:** tipo y SHA256, con «Aceptar» y «Cancelar». **Diálogo CONTRASEÑA:** campo enmascarado, «recordar en este móvil», intentos restantes y «Cancelar». Ninguno tiene plazo propio: si el host corta la espera (`LoginGraceTime`), la apertura falla con su motivo.

### 6.7 Paleta del terminal

| Índice | Color | Índice | Color |
|---|---|---|---|
| 0 negro | `#1C1C1C` | 8 negro brillante | `#5A5A5A` |
| 1 rojo | `#FF2A1A` | 9 rojo brillante | `#FF6B5E` |
| 2 verde | `#00FF6A` | 10 verde brillante | `#6BFFA6` |
| 3 amarillo | `#FFA000` | 11 amarillo brillante | `#FFC65C` |
| 4 azul | `#4D8DFF` | 12 azul brillante | `#8AB4FF` |
| 5 magenta | `#D66BFF` | 13 magenta brillante | `#E6A3FF` |
| 6 cian | `#2ED8E6` | 14 cian brillante | `#8CEEF5` |
| 7 blanco | `#EDE8DC` | 15 blanco brillante | `#FFFFFF` |

Texto `#EDE8DC` sobre `#0A0A0A`, cursor `#FFA000` y selección `#FFA000` al 35 %. Los índices 16-255 siguen la tabla estándar de xterm y los colores de 24 bits se pintan tal cual.

### Notas de UX y diseño

- **Cabina de MAGI sobre Material 3.** Solo hay tema oscuro: interfaz en ámbar (`#FFA000`), correcto en verde (`#00FF6A`), crítico en rojo (`#FF2A1A`) e inactivo en gris (`#5A5A5A`), con texto `#EDE8DC` sobre `#0A0A0A` (paleta fija de la F1, D6). JetBrains Mono en toda la app y cabeceras en mayúsculas como en la TUI.
- **Glifo, texto y color a la vez (T5):**
  - Hosts: `●N` sesiones abiertas, `◐` conectando, `○` sin sesión, `✕` último intento con error.
  - Túneles: `○` inactivo, `◐` activando, `●` activo, `✕` caído.
  - Sincronización: `⇅` al día, `⇅ …` sincronizando, `⇅ clave nueva`, `⇅ error`.
- **Ninguna acción destructiva con una sola pulsación (T6):** cerrar sesión, dejar de sincronizar, olvidar contraseña, sustituir huella y borrar datos piden confirmación. La notificación no tiene acciones.
- **Plegable:** compacta (pantalla exterior o vertical estrecha) con barra inferior; media o expandida (interior abierto, horizontal o tableta) con riel y dos paneles. Al plegar o desplegar no se pierde nada: la sesión visible sigue visible. Precisión (v1.3, D209): los dos paneles son para lista y ficha; el terminal se abre a pantalla completa y, sin detalle seleccionado, la lista ocupa todo el ancho.
- **Edge to edge (Android 15+):** las barras del sistema y el teclado se respetan con insets; la fila de teclas extra va pegada encima del teclado.
- **Carga y errores:** un indicador mientras conecta o sincroniza, y errores con instrucción («la clave no abre los ficheros de esta carpeta», «puerto reservado en Android: abre en un puerto libre», «desbloquea el teléfono para usar la clave del móvil»).
- **Estado vacío:** sin inventario, Hosts invita a configurar la sincronización. Sin túneles, la pantalla Túneles explica que se crean en el escritorio.
- **Ejemplos:** los mockups usan direcciones de documentación (`192.0.2.x`) y `ejemplo.org`, como en el resto del proyecto.

---

## 7. Lógica de Negocio

### 7.1 Autenticación en cadena

```python
def autenticar(conexion, host, clave_movil, llavero, dialogo):
    metodos = conexion.metodos_aceptados()                    # tras el intento «none»
    if "publickey" in metodos and clave_movil.existe():
        if conexion.publickey(host.usuario, clave_movil.publica(), firmante=clave_movil.firmar):
            return "clave_movil"
    if "password" in metodos:
        guardada = llavero.leer(f"contrasena:{host.uuid}")
        if guardada and conexion.password(host.usuario, guardada):
            return "contrasena_guardada"
        for intento in range(3):
            r = dialogo.pedir_contrasena(host, recordar=(host.identidad_ref == "contrasena:llavero"),
                                         intentos_restantes=3 - intento)
            if r is None:
                raise Cancelada()
            if conexion.password(host.usuario, r.texto):      # bytes que se borran tras usarlos
                if r.recordar:
                    llavero.guardar(f"contrasena:{host.uuid}", r.texto)
                return "contrasena"
    raise AutenticacionRechazada()
```

La identidad del escritorio (`auto`, `agente:`, `fichero:`) no se usa en el móvil: ni hay agente ni ficheros de clave (T1). Probar primero la clave del móvil cuesta como mucho un intento fallido en los hosts que solo aceptan contraseña. A cambio, «instalar mi clave» sirve al momento sin ajustes por host (D175). `keyboard-interactive` no se usa, igual que en el escritorio.

### 7.2 Firma y clave pública de la clave del móvil

```python
def mpint(n: int) -> bytes:
    b = n.to_bytes((n.bit_length() + 7) // 8 or 1, "big")
    if b[0] & 0x80:
        b = b"\x00" + b                      # positivo: byte 0 delante si el bit alto está a 1
    return len(b).to_bytes(4, "big") + b

def ssh_string(b: bytes) -> bytes:
    return len(b).to_bytes(4, "big") + b

def der_a_ssh(der: bytes) -> bytes:          # SEQUENCE { INTEGER r, INTEGER s } del Keystore
    r, s = asn1_dos_enteros(der)
    return ssh_string(b"ecdsa-sha2-nistp256") + ssh_string(mpint(r) + mpint(s))

def x509_a_openssh(spki: bytes, comentario: str) -> str:
    q = punto_sin_comprimir(spki)            # 0x04 ‖ X ‖ Y (65 bytes), curva P-256
    blob = ssh_string(b"ecdsa-sha2-nistp256") + ssh_string(b"nistp256") + ssh_string(q)
    return f"ecdsa-sha2-nistp256 {base64(blob)} {comentario}"   # comentario: magi@<modelo>-<AAAAMMDD>
```

La firma se hace en el Keystore sobre los datos que da russh (`SHA256withECDSA`). El núcleo la convierte y se la entrega a russh mediante la interfaz `Firmante`. La clave privada nunca pasa por la memoria del proceso.

### 7.3 Pasada en modo lector

```python
def pasada_lector(nucleo, carpeta, claves, trabajo_vivo):
    ficheros = [f for f in carpeta.listar()
                if nombre_valido(f.nombre) and f.tamano <= TAMANO_MAXIMO]   # T81
    releer_todo = nucleo.hay_borrados_aplazados()
    estados = []
    for f in ficheros:
        datos = carpeta.leer(f.nombre, TAMANO_MAXIMO)
        if not releer_todo and sha256(datos) == nucleo.hash_importado(f.nombre):
            continue
        estados.append(nucleo.leer_fichero(datos, claves))   # descifrar → firma → equipo → formato
    plan = nucleo.fusionar(estados)                           # la misma fusión que el escritorio (F10 §7.2)
    plan.aplazar_borrados(trabajo_vivo)                       # hosts y túneles con trabajo en el móvil
    plan.aplicar_aplazados_sin_trabajo(trabajo_vivo)
    nucleo.aplicar(plan, copia=False, confirmar_borrados=False,
                   conflictos={"huella", "borrado"}, exportar=False)
```

- **Sin copia ni confirmación:** la `magi.db` del móvil es un espejo que se puede rehacer uniéndose otra vez, y los borrados masivos los confirma el escritorio que los recibe (F10 §7.5).
- **Conflictos:** el móvil no tiene ediciones propias. Los conflictos de elemento, nombre y huérfano son de los escritorios, y cada uno los registra donde pierde. Si el móvil los guardara, tendría avisos que nunca se resuelven allí.
- **Renombrados por colisión:** la fusión puede renombrar `(2)` en local para respetar los nombres únicos, como hacen los escritorios. Esas ediciones no salen del móvil.
- **Determinismo:** con los mismos ficheros, el móvil llega al mismo inventario que cualquier escritorio (test de propiedades de la F10, repetido en modo lector).

### 7.4 Trabajo vivo y borrados aplazados

`TrabajoVivo` en el móvil es el conjunto de hosts con una sesión (abriendo, abierta o caída con pestaña), con un túnel activo o usados como salto por una conexión viva, más el conjunto de túneles activos. Un borrado que los afecta se aplaza como conflicto `borrado`, sin lápida local (T83). Al cerrarse el último trabajo de ese host, la siguiente pasada relee todos los ficheros y lo aplica sin preguntar: en la F10 eso es la tecla `a` del escritorio, que aquí hace la app. Un cambio en un túnel activo se guarda y vale en su próxima activación (F10).

### 7.5 Túneles en el móvil

```python
def escucha_movil(tunel):
    direccion, puerto = separar(tunel.escucha)       # «0.0.0.0:8080», «[::]:1080», «localhost:5432»
    aviso = direccion not in ("127.0.0.1", "localhost")
    return ("127.0.0.1", int(puerto)), aviso         # solo local y dinámico; en remoto escucha el host

def activar(tunel, puerto_libre=False):
    (direccion, puerto), _ = escucha_movil(tunel)
    if puerto_libre:
        puerto = 0                                   # el sistema elige; se muestra y no se guarda
    elif 0 < puerto < 1024:
        raise PuertoReservado(puerto)                # Android no deja a las apps escuchar por debajo de 1024
    return bind(direccion, puerto)                   # EADDRINUSE → PuertoOcupado (ofrece puerto libre)
```

- Automáticos con la regla de la F5 (§7.2 de su informe), contando solo las sesiones del móvil. Un túnel manual no impide que los automáticos se paren.
- El remoto (`tcpip_forward`) y el SOCKS5 (solo CONNECT, sin autenticación) usan la mecánica del escritorio movida al núcleo. En Android los navegadores no suelen admitir SOCKS sin configurarlo: el dinámico sirve sobre todo a apps que sí lo aceptan.
- Un `destino` de un túnel remoto se abre desde el móvil: `127.0.0.1:3000` significa el propio teléfono.
- Contadores (conexiones y bytes) como en la F5. Se reinician al relanzar.
- Precisión (validación, 4 nov 2026): un túnel solo está `activo` mientras su conexión con el host está viva y su escucha acepta; si la conexión cae o se cierra, pasa a `caido` con el motivo. La fila de un túnel activo enseña el último error de una conexión fallida (con la hora) y los contadores de conexiones abiertas y aceptadas en total. La nota «(puerto libre, no se guarda)» solo aparece si se usó «Abrir en un puerto libre», y la fila enseña siempre el puerto en el que escucha de verdad.
- Implementación (11b, 4 nov 2026): la causa del fallo era que, con solo túneles, nadie vigilaba la conexión del host. Si se caía (cambio de red, Doze o keepalive sin respuesta), el túnel seguía `activo` con la escucha abierta y cada conexión fallaba al pedir el canal `direct-tcpip` sin mover un byte. Ahora cada conexión tiene un vigilante (D206). La fila dice «N abiertas · M aceptadas · ↑… ↓…» con el último error y su hora, y se refresca cada segundo mientras se ve Túneles o la ficha (D207).

### 7.6 Codificación de teclas

| Tecla | Modo normal | Modo aplicación del cursor |
|---|---|---|
| ↑ ↓ → ← | `ESC [ A` / `B` / `C` / `D` | `ESC O A` / `B` / `C` / `D` |
| Inicio · Fin | `ESC [ H` · `ESC [ F` | `ESC O H` · `ESC O F` |
| RePág · AvPág · Supr | `ESC [ 5 ~` · `ESC [ 6 ~` · `ESC [ 3 ~` | igual |
| F1-F4 | `ESC O P` · `Q` · `R` · `S` | igual |
| F5-F12 | `ESC [ 15 ~`, `17 ~`, `18 ~`, `19 ~`, `20 ~`, `21 ~`, `23 ~`, `24 ~` | igual |
| Ctrl + letra | letra AND 0x1F (Ctrl+espacio → NUL; Ctrl+[ → ESC) | igual |
| Alt + tecla | `ESC` seguido de la tecla | igual |
| Modificador + flecha | `ESC [ 1 ; m A` con m = 1 + Mayús·1 + Alt·2 + Ctrl·4 | igual |
| Pegado entre corchetes | `ESC [ 200 ~` texto `ESC [ 201 ~` si el remoto activó el modo 2004 | |

El modo de la sesión lo da `vt100` (cursor de aplicación y pegado entre corchetes). La fila de teclas extra, el teclado del sistema y el teclado físico pasan todos por `tecla()` o por `escribir()`, y así codifican igual. Desde la implementación, el escritorio usa esta misma tabla (`src/teclas.rs` solo traduce los eventos de crossterm), así que Ctrl, Alt y Mayús + flecha envían `CSI 1;m` también en la TUI (D197).

### 7.7 Pantalla por tramos

```python
class Tramo:
    texto: str                 # celdas seguidas con el mismo estilo; un carácter ancho ocupa dos celdas
    primer_plano: Color        # predeterminado | indice(0-255) | rgb
    fondo: Color
    negrita: bool; cursiva: bool; subrayado: bool; inverso: bool

class Pantalla:
    sesion: int
    columnas: int; filas: int
    filas_cambiadas: dict[int, list[Tramo]]   # solo las que cambiaron desde la última enviada
    cursor: tuple[int, int] | None            # None si el remoto lo oculta
    desplazamiento: int                       # filas de historial por encima (0 = al final)
    completa: bool                            # True al abrir, al redimensionar o al volver a la vista
```

El núcleo guarda el último estado del parser y, como mucho cada 33 ms, compara filas y envía las cambiadas. El parser vive en la tarea de conexión (T13). La vista dibuja por celdas, no por anchura de la fuente, y así los caracteres anchos (CJK, emoji) no descuadran las columnas.

### 7.8 Bloqueo de la app

```python
MARGEN_SEG = 5 * 60

def al_volver(ahora, salio_en, arranque_en_frio):
    if arranque_en_frio or salio_en is None:
        return "bloqueada"
    return "desbloqueada" if ahora - salio_en < MARGEN_SEG else "bloqueada"
```

El desbloqueo usa BiometricPrompt con `BIOMETRIC_STRONG | DEVICE_CREDENTIAL`. La clave del móvil y la de `LlaveroMovil` se crean sin `setUnlockedDeviceRequired` (D205): con desbloqueo facial, que es biometría no fuerte, Android las rechaza con `UserNotAuthenticatedException` aunque el teléfono esté desbloqueado (comprobado en MagicOS). Siguen en StrongBox o TEE y no se pueden exportar; lo que impide usarlas sin permiso es este bloqueo. No exigen biometría en cada uso, para que una reconexión funcione aunque la app esté bloqueada (D174). El aviso «desbloquea el teléfono para usar la clave del móvil» se mantiene para cuando Android rechace la clave de verdad.

Las instalaciones anteriores a D205 migran una sola vez: `LlaveroMovil` vuelve a cifrar sus secretos con una clave nueva (`secretos-v2`) y la clave del móvil se crea de nuevo con otro alias. La clave pública cambia, así que hay que volver a instalarla en los hosts; la app lo avisa.

### 7.9 Script «instalar mi clave»

```sh
#!/bin/sh
# instalar_clave.sh: recibe la línea pública como $1, ya validada en el núcleo
set -eu
clave="$1"
tipo_y_cuerpo=$(printf '%s' "$clave" | cut -d' ' -f1,2)
umask 077
dir="$HOME/.ssh"
fichero="$dir/authorized_keys"
mkdir -p "$dir" && chmod 700 "$dir"
[ -e "$fichero" ] || : > "$fichero"
chmod 600 "$fichero"
if grep -qF -- "$tipo_y_cuerpo" "$fichero"; then
    echo YA_ESTABA
else
    if [ -s "$fichero" ] && [ -n "$(tail -c 1 "$fichero")" ]; then
        printf '\n' >> "$fichero"            # el fichero no acababa en salto de línea
    fi
    printf '%s\n' "$clave" >> "$fichero"
    echo INSTALADA
fi
if command -v restorecon >/dev/null 2>&1; then restorecon -F "$dir" "$fichero" 2>/dev/null || true; fi
```

Va embebido con `include_str!`. Se lanza como `sh -c '<script>' magi '<clave>'` con `shell-escape`, así la clave es un argumento y nunca texto del comando. Antes de lanzarlo, el núcleo valida la línea con `^ecdsa-sha2-nistp256 [A-Za-z0-9+/]+={0,2} [A-Za-z0-9._@-]{1,64}$`.

### 7.10 Versiones de la app

```python
import re

def version_code(version: str) -> int:
    m = re.fullmatch(r"(\d+)\.(\d{1,2})\.(\d{1,2})(?:-rc\.([1-8]))?", version)
    if not m:
        raise ValueError("versión no válida")
    mayor, menor, parche, rc = m.groups()
    r = int(rc) if rc else 9                 # la final siempre supera a sus candidatas
    return int(mayor) * 100_000 + int(menor) * 1_000 + int(parche) * 10 + r

# 0.1.0-rc.1 → 1001 · 0.1.0 → 1009 · 0.2.0 → 2009 · 1.2.3 → 102039
```

La versión vive en `android/version.properties` (`version=0.1.0`). `versionName` es esa cadena y `versionCode` sale de la fórmula. `android-release.yml` falla si la etiqueta `android-v<version>` no coincide. La política de versiones de la F9 (T68) vale también para la app.

### 7.11 Validaciones

| Dato | Regla |
|---|---|
| Clave de sincronización | Texto `AGE-SECRET-KEY-1…` en bech32: se quitan espacios y saltos de línea, se pasa a mayúsculas (texto canónico, D156) y debe abrir un fichero de la carpeta con firma válida |
| Carpeta | Debe tener al menos un `magi-<uuid>.age` válido para unirse |
| Línea pública a instalar | Regex de §7.9 |
| Puerto de escucha (local y dinámico) | 0 (libre) o 1024-65535 en el móvil; menor que 1024, `PuertoReservado` con la salida de puerto libre |
| Tamaño de terminal | Saneado a mín. 2×1 (T28) |
| Tamaño de letra | 8-24 sp |
| Borrar todo | Hay que escribir «borrar» |

### 7.12 Casos especiales

- **Host borrado en el escritorio con una sesión abierta en el móvil:** se aplaza y se borra solo al acabar (§7.4).
- **Dirección cambiada con una sesión abierta:** la sesión sigue con la conexión que tiene; la siguiente conexión usa la nueva.
- **Teléfono bloqueado durante una reconexión:** la firma falla y la pestaña dice «desbloquea el teléfono para usar la clave del móvil».
- **Sin red:** conectar falla con su motivo; la pasada sigue funcionando (la carpeta es local).
- **Cambio de wifi a datos:** con Tailscale (WireGuard) la sesión suele sobrevivir, porque la IP del túnel no cambia. Sin VPN, cae y la pestaña queda con «Reconectar».
- **Huella que llega para un host sin huella en el móvil:** se añade (`huella_sincronizada`). Para uno con otra huella, conflicto que se resuelve en el móvil.
- **Clave cambiada en el escritorio** (quitar un equipo o `r`): el móvil pasa a `esperando_clave` y se vuelve a escanear. La anterior lee 30 días.
- **El móvil no figura como equipo** en el escritorio, porque no escribe fichero. Para cortarle el acceso: «cambiar la clave» (`r`) en el escritorio y quitarlo de Syncthing (R96, R97).
- **Android invalida las claves** al quitar el bloqueo de pantalla. MAGI lo detecta: hay que crear otra clave del móvil (y volver a instalarla en los hosts) y escanear de nuevo la clave de sincronización.
- **Reinstalar la app:** Android borra los datos y las claves del Keystore. Equivale a empezar de cero.
- **Proceso matado por Android:** se pierden sesiones y túneles; al volver, aviso y guía de batería.
- **Plegar, desplegar o girar con una sesión abierta:** la sesión sigue y el remoto recibe el tamaño nuevo.
- **`opciones_extra`:** no se interpretan, como en las conexiones del escritorio: son para el `ssh` del sistema vía `magi_config`.

### 7.13 Limitaciones heredadas

- **R85:** sin la corrección 10b, un fichero a medio copiar puede pasar el móvil a `esperando_clave`. Allí es solo un aviso y vuelve solo (D189).
- **R86:** un elemento inválido hace fallar la pasada entera. El móvil se queda con el inventario anterior y avisa.
- **R87:** aceptar la huella de otro equipo sustituye todas las del host, igual que en el escritorio.

---

## 8. Requisitos No Funcionales

**Seguridad**

- **Clave del móvil:**
  - Se crea en el hardware (StrongBox o TEE) y nunca sale del teléfono ni pasa por la memoria del proceso (T1).
  - Desde D205 el Keystore no exige el teléfono desbloqueado (fallaba con el desbloqueo facial): la protegen que solo MAGI puede usarla, que no se puede exportar y el bloqueo de la app.
- **Secretos (clave de sincronización y contraseñas):**
  - Viven cifrados con una clave del Keystore (`LlaveroMovil`): nunca en `magi.db`, en DataStore ni en logcat (T19).
  - Las contraseñas cruzan la frontera FFI como bytes que se borran tras usarlas.
  - Límite conocido: el texto del campo de Compose antes de leerlo (amplía R14).
- **App:**
  - Se bloquea con huella o PIN al abrirla y tras 5 minutos fuera.
  - Sin bloqueo de pantalla en el teléfono, no arranca.
  - Las pantallas con secretos llevan `FLAG_SECURE` y la vista previa de recientes se oculta (Android 13+).
- **Huellas:** estrictas (T4), con un `known_hosts` propio de la app. Una huella cambiada bloquea; sustituirla exige escribir el nombre del host y queda en REGISTRO.
- **Carpeta:**
  - El permiso es solo de lectura y el móvil nunca escribe en ella.
  - Nada se importa sin firma válida, y todo lo que llega se trata como entrada no fiable (T81).
- **Túneles:** solo en loopback en el móvil (T35).
- **Sin servicios de terceros:**
  - Sin Google Play Services, Firebase, ML Kit, analítica ni copia en la nube (`allowBackup="false"`). Las únicas conexiones salientes son a los hosts del usuario (T10).
  - Las dependencias de Gradle se verifican por SHA-256 y el wrapper por `distributionSha256Sum`.
- **Publicación:** el APK lo firma CI con un keystore que solo está en los secretos de `android-release.yml`, con atestación de procedencia. El agente no crea el keystore ni los secretos (T69).
- **Logs:** el núcleo usa `tracing` hacia logcat solo en debug. En release no hay log de contenido; nunca hay secretos, salida de comandos ni pantallas.

**Protección de datos (RGPD).** Solo datos del propio usuario (su inventario, sus huellas y sus contraseñas), guardados en el almacenamiento privado de la app y en el Keystore. Nada sale del teléfono salvo las conexiones SSH a sus hosts. La cámara solo se usa para leer el QR y no guarda imágenes. Sin telemetría.

**Backups y recuperación.** No hay copia en la nube a propósito. Lo que se puede perder se recupera así:

- **Inventario:** se rehace uniéndose otra vez (está en la carpeta y en los escritorios).
- **Clave del móvil:** se crea otra y se vuelve a instalar en los hosts.
- **Contraseñas:** se vuelven a escribir.
- **Huellas aceptadas solo en el móvil:** se vuelven a aceptar.

**Rendimiento** (en el móvil de validación)

| Métrica | Objetivo |
|---|---|
| Arranque en frío hasta Hosts (tras desbloquear) | < 1,5 s |
| Pasada sin cambios | < 300 ms |
| Pasada con cambios (1 000 hosts, 5 equipos) | < 2 s |
| Retraso añadido al eco de una tecla | < 30 ms |
| Pantallas por segundo | ≤ 30, y la interfaz responde con un remoto que imprime sin parar |
| Memoria con 5 sesiones | < 200 MB |
| APK `arm64-v8a` sin minificar | < 40 MB (el de depuración, con dos ABI, pesa ≈ 50 MB; la release está por medir) |
| Batería sin sesiones ni túneles | Ningún trabajo en segundo plano |

**Accesibilidad.** Glifo y texto además del color (T5); descripciones para TalkBack en controles, estados y filas («web-01, 2 sesiones abiertas, usuario arroba 192.0.2.10»); objetivos táctiles de 48 dp; el tamaño de letra del sistema escala la interfaz (no el terminal, que tiene su propio zoom). Contraste AA de la paleta sobre `#0A0A0A`, salvo el gris de inactivo y el negro brillante, que son atenuados a propósito. Límite: el contenido del terminal no se lee con TalkBack en esta fase.

**Compatibilidad.** Android 11 (API 30) a 17 (API 37), `arm64-v8a`. El emulador `x86_64` solo se usa en desarrollo y CI.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Android Keystore | Clave del móvil (`magi-clave-movil`, EC P-256, `PURPOSE_SIGN`, SHA-256, sin `setUnlockedDeviceRequired` desde D205, StrongBox si existe) y clave `magi-secretos` (AES-256-GCM; renovada en la migración de D205) para `LlaveroMovil` |
| BiometricPrompt (`androidx.biometric`) | Biometría fuerte o credencial del teléfono para el bloqueo de la app |
| Storage Access Framework | `ACTION_OPEN_DOCUMENT_TREE`, permiso persistente de lectura, `DocumentsContract` para listar y leer |
| Syncthing-Fork (F-Droid) | Replica la carpeta en el almacenamiento compartido. Se recomienda la carpeta en «solo recibir» en el móvil, porque el móvil no escribe y así nada hecho en el teléfono viaja a los escritorios |
| Nextcloud y otros proveedores | Cualquier proveedor de documentos que exponga la carpeta al selector de Android |
| CameraX + ZXing | Lectura del QR de la clave que muestra `magi` en el escritorio (F10 §6.2) |
| Tailscale y WireGuard | Sin integración: la app conecta a direcciones y nombres que resuelve el sistema (incluido MagicDNS) |
| UniFFI 0.32.2 + JNA | Bindings de Kotlin generados en la compilación; la versión del generador es la del crate |
| NDK r30 + `cargo-ndk` 4.1.2 | Compilación del núcleo para `arm64-v8a` y `x86_64` desde Gradle |
| GitHub Releases y atestaciones | `android-release.yml`: APK, `.sha256`, atestación (`gh attestation verify`) y huella del certificado en las notas; nunca «Latest» |
| Obtainium | Instala y actualiza desde las releases del repositorio filtrando por la etiqueta `android-v` y el nombre `magi-android-*.apk` |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D168 — Kotlin + Jetpack Compose con núcleo Rust compartido por UniFFI** (Hector, 3a). Una sola implementación de lo delicado (fusión, formato, cifrado, SSH, túneles, parser). Descartados: Kotlin puro, con dos fusiones que mantener idénticas; Flutter, con otro lenguaje y otro widget de terminal; y Tauri, con terminal web en una WebView.
- **D169 — Mismo repositorio como workspace; la app con numeración propia desde 0.1.0 y etiquetas `android-v*`** (Hector, 4a). Un CI comprueba que el escritorio y el móvil leen los mismos ficheros. Descartados la misma versión que el escritorio (la app saldría como 1.2.0 sin serlo) y un repositorio aparte.
- **D170 — `magi-ssh` sigue en la raíz como paquete raíz del workspace; los miembros nuevos van en `crates/`.** No cambia ninguna ruta de la F9 (dist, AUR, Homebrew, CI, assets). Descartado mover el escritorio a `crates/magi-ssh`: rompe rutas sin beneficio, con la F9 aún sin publicar.
- **D171 — Dos crates: `magi-nucleo` (Rust puro, sin FFI) y `magi-movil` (cdylib con UniFFI).** El escritorio no compila UniFFI ni nada del móvil. Descartado meter UniFFI en el núcleo detrás de una característica.
- **D172 — El móvil es un lector** (Hector, 2c): inventario de solo lectura y ningún fichero propio en la carpeta. Consecuencias:
  - No aparece como equipo en el escritorio y se le corta el acceso cambiando la clave.
  - Las huellas que acepta se quedan en él.
  - Un fallo del móvil no puede estropear el inventario compartido.

  Escribir su fichero queda para cuando pueda editar (F13).
- **D173 — Modo lector de la pasada:** sin copia, sin confirmar borrados masivos y solo con conflictos de huella y de borrado; los aplazados se aplican solos al acabar el trabajo vivo. El móvil refleja lo que deciden los escritorios y su `magi.db` se puede rehacer.
- **D174 — Clave del móvil ECDSA P-256 en el Keystore, no exportable, con `setUnlockedDeviceRequired` y sin biometría en cada firma** (Hector, 6a).
  - P-256 porque todos los Keystore la guardan en hardware y todos los OpenSSH la aceptan. Ed25519 en el Keystore solo existe desde Android 13 y StrongBox no la admite.
  - Sin biometría por firma para que reconectar funcione con el teléfono desbloqueado.
  - Precisión (implementación, 1 nov 2026): sin `setUnlockedDeviceRequired`; ver D205.
- **D175 — Autenticación en cadena fija** (clave del móvil, luego contraseña guardada, luego diálogo) en vez de un método por host en el móvil. Con el inventario de solo lectura, así «instalar mi clave» funciona al momento sin ajustes locales.
- **D176 — `LlaveroMovil`** (ficheros con AES-GCM y clave del Keystore) detrás de la misma interfaz `LlaveroSecreto` del escritorio. Descartados EncryptedSharedPreferences (obsoleta) y guardar secretos en `magi.db` (T19).
  - Precisión (implementación, 1 nov 2026): su clave del Keystore también va sin `setUnlockedDeviceRequired`, y las instalaciones anteriores vuelven a cifrar sus secretos una vez con una clave nueva (D205).
- **D177 — Bloqueo con BiometricPrompt al abrir y tras 5 minutos fuera** (Hector, 7a). MAGI exige que el teléfono tenga bloqueo de pantalla.
- **D178 — Carpeta por SAF con permiso solo de lectura; pasada al abrir, al volver, cada 60 s visible y a mano** (Hector, 8a). Sin permiso de «todos los ficheros» ni trabajo en segundo plano. Descartado WorkManager: MagicOS lo retrasa y gasta batería sin necesidad.
- **D179 — Servicio en primer plano `specialUse` sin acciones en la notificación** (Hector, 9a). `dataSync` tiene un límite de 6 h desde Android 15; ninguna acción destructiva con una pulsación (T6).
- **D180 — Una conexión por host en el móvil, compartida por sesiones y túneles; keepalive de 30 s si el host no tiene.** En el móvil la red cambia y los NAT de las operadoras cortan las conexiones quietas. `multiplexar` es una opción del escritorio.
- **D181 — Túneles del móvil siempre en `127.0.0.1`, con «abrir en un puerto libre» como salida a puertos ocupados o reservados** (Hector añadió los túneles al alcance). Un teléfono cambia de red a menudo: exponer un puerto en la wifi de turno es un riesgo (T35).
- **D182 — Terminal con el parser `vt100` del núcleo y pintado propio en Compose (Canvas), con entrada en bruto.** Descartados las bibliotecas de terminal de Termux (otro parser distinto del del escritorio) y xterm.js en una WebView.
- **D183 — Una fila deslizable de teclas extra, con Ctrl y Alt de una pulsación o fijos** (Hector, 10a). Sin gestos de deslizar para flechas o pestañas.
- **D184 — Cabina de MAGI sobre Material 3: solo tema oscuro, paleta fija y JetBrains Mono (OFL) en toda la app** (Hector, 11a). En el móvil no hay tema de Omarchy que leer: manda la paleta de respaldo de la F1.
- **D185 — Android 11 (API 30) como mínimo y solo `arm64-v8a` en la release** (Hector, 5a). Android 11 simplifica el Keystore y la biometría y cubre casi todos los móviles en uso. `x86_64` solo para el emulador.
- **D186 — Distribución por GitHub Releases y Obtainium** (Hector, 12a): APK firmado en CI, con atestación y nunca «Latest».
  - `applicationId` `org.cuatrodetres.magi`: `org.4d3.magi` no es válido, porque un segmento no puede empezar por un número.
  - El nombre de paquete y la clave de firma no cambian desde la 0.1.0, para poder registrarlos cuando Google exija la verificación de desarrolladores (R94).
- **D187 — Sin minificación (R8) en la 0.1.0.** Evita fallos que solo aparecen en la release con JNA y UniFFI. Se activará con su prueba de humo.
- **D188 — Sin servicios de Google:** QR con CameraX + ZXing, licencias generadas en la compilación y verificación de dependencias de Gradle. Deja abierta la puerta a F-Droid e IzzyOnDroid y cumple T10.
- **D189 — La corrección 10b queda fuera de esta fase** (Hector, 1b). R83-R85 siguen abiertos. El móvil mitiga R85 en su lado: `esperando_clave` es un aviso que vuelve solo.
- **D190 — `versionCode` derivado de la versión** (`MAYOR·100000 + MENOR·1000 + PARCHE·10 + R`, con R = número de rc o 9 en la final). Una rc se actualiza a la final sin desinstalar.
- **D191 — (implementación) `cargo deny check` sin `--workspace` en CI.** cargo-deny 0.20.2 no admite la opción y ya analiza todo el grafo del workspace, incluidos `magi-nucleo` y `magi-movil`.
- **D192 — (implementación) `release.yml` excluye `!android-v*`.** El patrón de etiquetas de dist (`**[0-9]+.[0-9]+.[0-9]+*`) casaba `android-v0.1.0`. `.github/actionlint.yaml` ignora solo SC2086, SC2129 y SC2001 en el `release.yml` generado por dist; los workflows propios pasan limpios.
- **D193 — (implementación) AGP 9.5 con Kotlin integrado.** AGP 9 no admite el plugin `org.jetbrains.kotlin.android`; se aplica el de Compose y se fija Kotlin 2.4.21. JDK 21 (LTS) en CI; AGP exige 17 o más. `compileSdk 37` resuelve a la plataforma `android-37.0`.
- **D194 — (implementación) Versión de la app comprobada dos veces y firma solo en CI.** Gradle deriva `versionName` y `versionCode` de `android/version.properties`; `android-release.yml` exige que la etiqueta coincida y lo comprueba en el APK con `aapt2`. La firma de release solo existe si llegan las variables `MAGI_KEYSTORE_*` (esquemas v2 y v3, v1 desactivado): en local la release sale sin firmar y no se publica.
- **D195 — (implementación) La clave anterior del móvil se guarda con su fecha límite dentro de su propio secreto** (`sincronizacion-anterior`); los 30 días se comprueban al leer. Sin tablas ni preferencias nuevas.
- **D196 — (implementación) Contrato UniFFI ampliado sin tocar lo existente:** `pegado_entre_corchetes(sesion)` y `FirmanteMovil.olvidar_clave()`.
- **D197 — (implementación) El escritorio delega la codificación de teclas en la tabla común del núcleo.** Ctrl, Alt y Mayús + flecha pasan a `CSI 1;m` también en la TUI. Es una mejora visible y va en el CHANGELOG del escritorio.
- **D198 — (implementación) El núcleo incluye también `configuracion`, `snippets` (variables y salida) y `deliberacion`,** porque el almacén movido los necesita.
- **D199 — (implementación) La clave del móvil se genera con `KeyPairGenerator`.** `KeyGenerator` para EC lanza `NoSuchAlgorithmException` en AndroidKeyStore: era el crítico C1 de la revisión (sin clave, toda conexión fallaba). Lo cubre una prueba instrumentada con el Keystore real.
- **D200 — (implementación) Al arrancar, la app vuelve a registrar en el núcleo la carpeta guardada** (`elegir_carpeta` con el URI de DataStore), sin pedirla otra vez. La pasada corre en el runtime del núcleo y su fase se restaura siempre, también si se cancela.
- **D201 — (implementación) Integración núcleo-app probada con una fixture `.age` real** que genera el núcleo (`cargo test -p magi-nucleo --test generar_fixture_android -- --ignored`) y se importa en el emulador.
- **D202 — (implementación) La supresión `NewApi` de los bindings de UniFFI** (`java.lang.ref.Cleaner`, API 33) se limita al fichero generado; las pruebas instrumentadas en API 30 pasan con ese código.
- **D203 — (validación) Corrección 11b dentro de la F11, como la 3b:** se desmarca la funcionalidad que falla en el móvil real y se añaden al checklist de la fase las funcionalidades de la corrección, en vez de abrir una fase nueva.
- **D204 — (validación) Los túneles del móvil se prueban de extremo a extremo,** sin red externa, en Rust y en el emulador. Las pruebas de escucha y de puerto libre de la F11 no detectaron que un túnel activo no reenviaba.
- **D205 — (implementación, 1 nov 2026) Claves del Keystore sin `setUnlockedDeviceRequired`.** Con desbloqueo facial (biometría no fuerte), Android rechaza esas claves aunque el teléfono esté desbloqueado; en el Honor impedía unir la sincronización y conectar. Las claves siguen en StrongBox o TEE y no se pueden exportar; la protección pasa al bloqueo de MAGI (§7.8). Las instalaciones anteriores migran una vez: los secretos se vuelven a cifrar con una clave nueva y la clave del móvil cambia, así que hay que reinstalarla en los hosts. Matiza D174 y D176.
- **D206 — (corrección 11b) Un vigilante por conexión del host** en el gestor del móvil, que mira cada 500 ms si el cliente SSH la ha cerrado (`Handle::is_closed`). Si el host la cierra sin que nadie la pare, se suelta la conexión, sus sesiones pasan a caídas, sus túneles a `caido` con el motivo y el host queda en error. Antes solo la vigilaba el bucle de una sesión abierta. No cambia nada del escritorio.
- **D207 — (corrección 11b) `TunelVivo` gana `aceptadas`, `ultimo_error` y `ultimo_error_en`** (contrato UniFFI aditivo). El puerto efectivo solo se guarda con «Abrir en un puerto libre». La fila se refresca cada segundo mientras se ve Túneles o la ficha; antes solo cambiaba al activar o parar.
- **D208 — (corrección 11b) La prueba de extremo a extremo del emulador usa un servidor de pruebas en el anfitrión** (SSH, servicio de eco y un canal de control para tirar las conexiones), al que el emulador llega por `10.0.2.2`, y una fixture propia con un host y un túnel. Si el servidor no está, la prueba se omite (`Assume`) en vez de fallar, así que la CI no la ejecuta (R106 del maestro). El servidor debe escuchar en `127.0.0.1`, no en `0.0.0.0` (R107).
- **D209 — (implementación, 4 nov 2026, petición de Hector) Terminal a pantalla completa y sin panel vacío en el plegable.** Al abrir una sesión, el terminal ocupa toda la pantalla: sin riel, lista ni barra inferior y con las barras del sistema ocultas (aparecen al deslizar); al volver, se recupera la disposición de dos paneles. Sin detalle seleccionado, la lista ocupa todo el ancho. Sustituye a los dos paneles con terminal de §6.4.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Núcleo compartido y esqueleto | Workspace; mover a `magi-nucleo` sin reescribir (pruebas e instantáneas idénticas); servicios inyectables y rutas; modo lector; conversiones de clave; tramos y teclas; `magi-movil` con UniFFI; proyecto `android/` que carga el núcleo; `ci.yml --workspace` y `android.yml`. Lo primero de todo, la compilación cruzada de russh (R91) | 1,5 semanas |
| S2 — Datos, seguridad y sincronización | Clave del móvil, `LlaveroMovil` y bloqueo; asistente; carpeta SAF; unir por QR o pegando; pasadas; aplazados; huellas que llegan y conflictos; pantalla Sincronización; Hosts y ficha de solo lectura; tema y fuente | 1 semana |
| S3 — Terminal y conexión | Autenticación en cadena, huellas, saltos, keepalive y una conexión por host; sesiones y estados; terminal (pintado, entrada en bruto, fila de teclas, teclado físico, zoom, historial, selección, pegado); redimensionado; servicio en primer plano; batería; instalar mi clave | 1,5 semanas |
| S4 — Túneles, plegable y publicación | Túneles con puerto libre y navegador; dos paneles en plegable; accesibilidad; licencias; Ajustes y borrar datos; `android-release.yml`, `versionCode`, CHANGELOG, README y `docs/publicar-android.md`; prueba de humo; revisión adversarial (T31, T54) | 1 semana |

**Total: 4 sprints, ~5 semanas.**

**Real (31 oct 2026):** implementada en Claude Code, con un Lead y pistas en paralelo sobre un contrato compilable (T63). Commits `a5f1e1f`, `82bff82`, `ef4e18f`, `419ebe1`, `78e2cc2` y `5495161`. La revisión adversarial encontró 23 hallazgos (1 crítico, 5 altos, 7 medios y 10 bajos), todos corregidos. Verificación en local:

- Rust: fmt, clippy, `test --workspace` y `deny` en verde.
- Android: 126 pruebas JVM, 3/3 instrumentadas en el AVD API 30 x86_64, `lint` con 0 errores y `assembleRelease` compilando sin firma.
- `dist plan` (0.32.0 en local) sigue publicando solo `magi-ssh`.

Pendiente: la validación en el plegable, la CI de GitHub y la primera release con keystore.

**Validación en el plegable (4 nov 2026):** un túnel local en «● activo» no reenvía. Chrome y Brave del móvil dan `ERR_SOCKET_NOT_CONNECTED` en `http://127.0.0.1:<puerto>`, los contadores siguen en cero y la fila no enseña error. Pasa igual con MAGI visible en una ventana flotante, y el mismo túnel funciona desde otro ordenador.

**Corrección 11b:**

- Reproducir el fallo con una prueba de extremo a extremo que hoy falle, en Rust y en el emulador.
- Corregir la causa.
- `caido` con motivo si cae la conexión del host.
- Último error y contadores en la fila.
- Nota de puerto libre solo con esa opción.
- Arreglos de texto y disposición.
- Repetir la revisión adversarial sobre túneles y conexiones del móvil.

Va en el prompt de reanudación.

**Real (4 nov 2026, corrección 11b):** hecha y validada en el Honor el mismo día: Chrome y Brave abren el túnel local. Causa y cambios en §7.5 (D206-D207). Pruebas:

- Rust: prueba de extremo a extremo del túnel con servidor SSH en proceso y servicio de eco (reenvío, contadores, puerto libre y caída de la conexión); la prueba de caída falla sin el vigilante.
- Emulador: dos pruebas instrumentadas con servidor de pruebas en el anfitrión (reenvío por `127.0.0.1:9090` y caída al tirar la conexión); sin ese servidor se omiten (D208).
- 127 pruebas JVM, 8/8 instrumentadas en el AVD API 30, `lint` con 0 errores y Rust en verde.
- Revisión adversarial (T31): carreras de parada, relanzado y doble activación, identidad del vigilante y conexiones compartidas; sin fugas nuevas ni cambios en el escritorio.

El mismo informe recoge D205 (Keystore, 1 nov) y D209 (plegable, petición de Hector).

Pendiente: el resto del guion en el plegable, la CI de GitHub, la primera release con keystore y los trámites de cierre (R103, R104 y R107), que van en el prompt de reanudación.

**Supuestos**

- El agente instala en Omarchy el SDK de Android, el NDK r30 y un emulador `x86_64` con KVM.
- La reorganización solo mueve código.
- La F10 funciona como se reportó.
- El desarrollador crea el keystore y los secretos.
- Hector valida en el plegable lo que el emulador no cubre: Syncthing-Fork, QR, MagicOS, StrongBox, plegado y teclado físico (R100).

**Guion de validación manual (en el plegable)**

1. Instalar el APK de depuración (`adb install`) o la `android-v0.1.0-rc.1`.
2. Primer arranque:
   - bloqueo;
   - clave del móvil (anotar si es StrongBox o TEE);
   - carpeta de Syncthing-Fork en «solo recibir»;
   - QR mostrado por `magi` en Omarchy.
3. Inventario: grupos, búsqueda y ficha. Cambiar un host en el escritorio y ver que llega.
4. Conectar con contraseña a un host de pruebas, «instalar mi clave», reconectar y entrar con la clave.
5. Huella cambiada con un host de pruebas reinstalado: bloqueo, sustitución escribiendo el nombre y REGISTRO.
6. En el terminal:
   - `htop` (F10 para salir);
   - `vim` (flechas en modo aplicación);
   - Ctrl+C y Alt;
   - pegar varias líneas (confirmación);
   - zoom, selección y copia;
   - historial.
7. Plegar y desplegar con `htop` abierto (el terminal sigue a pantalla completa, D209); teclado Bluetooth (Ctrl+Mayús+V).
8. Túnel local a un panel web y «Abrir en el navegador» (hecho el 4 nov: reenvía). Un túnel en el puerto 80: error y puerto libre. Con un túnel activo, reiniciar el `sshd` del host o cortarle la red: el túnel pasa a caído con el motivo y «Relanzar» lo recupera. Un túnel remoto y uno SOCKS5 con una app que lo admita.
9. Sesiones 30 minutos con la pantalla apagada, con y sin la exclusión de batería, y paso de wifi a datos con Tailscale.
10. Borrar en el escritorio un host con una sesión abierta en el móvil: queda aplazado y se borra al cerrarla.
11. Cambiar la clave en el escritorio (`r`): aviso en el móvil y volver a escanear.
12. Ajustes: dejar de sincronizar y volver a unirse; borrar todos los datos.
13. Medir el APK release `arm64-v8a` (objetivo < 40 MB) y comprobar que Licencias enseña la OFL de JetBrains Mono.
14. Desbloquear el teléfono con la cara y, después, unir la sincronización y conectar con la clave del móvil (D205). Si se actualiza desde una versión anterior, la app avisa de que la clave del móvil ha cambiado: reinstalarla en los hosts.

---

## 12. Conexiones con Otras Fases

- **F1:** conexión, saltos, huellas estrictas (diálogos de huella nueva y cambiada) y estados de sesión pasan al núcleo y se reutilizan; paleta fija como tema del móvil (D6).
- **F2:** contraseña con «recordar» y la interfaz de llavero (`LlaveroSecreto`) con otra implementación; `clave_generada` en REGISTRO para la clave del móvil. La Flota queda para la F12.
- **F3:** estados de pestaña, nombres `host (n)` y reconexión manual. En el móvil no hay servidor de sesiones: `magi-movil` hace su papel dentro de la app, con el mismo patrón de listas completas (T27).
- **F5:** mecánica de los túneles y ciclo automático movidos al núcleo; el móvil añade loopback forzado y puerto libre.
- **F7:** tamaños saneados, agrupación de 50 ms y redimensionado al plegar o girar con las mismas reglas.
- **F9:** CI con `--workspace`; `release.yml` separado de las releases de Android; política de versiones (T68) y auditoría de publicación (T69); `cargo deny` cubre los crates nuevos (MPL-2.0 de UniFFI ya permitida, R80).
- **F10:** formato, cifrado, firma y fusión se usan tal cual en modo lector. `EQUIPOS.plataforma = android` existe, pero el móvil no escribe fichero. Las limitaciones R85-R87 se heredan (§7.13). `docs/formato-sincronizacion.md` §11.9 pedía una red de seguridad para los borrados en Android: manda este informe (§7.3) y el documento se corrige (R103).
- **Corrección 10b (pendiente):** R83-R85 y la propuesta de R97 (la clave anterior solo abre ficheros de equipos conocidos).
- **F13 (prevista, Android segunda vuelta; antes F12):**
  - edición en el móvil, convertido en equipo escritor con su fichero;
  - Flota con sondeo;
  - snippets con deliberación;
  - SFTP;
  - vista de registro.
