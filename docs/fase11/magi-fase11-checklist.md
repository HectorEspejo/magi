# MAGI - Checklist Fase 11: App Android

## Núcleo compartido (workspace)
- [x] Workspace de Cargo con el paquete `magi-ssh` en la raíz y los miembros `crates/magi-nucleo` y `crates/magi-movil` (`publish = false`, fuera de `dist`)
  - AC: Dado el repositorio reorganizado, cuando se ejecutan `cargo build --workspace --locked` y `dist plan`, entonces compila todo y `dist` sigue publicando solo `magi-ssh` con sus cuatro objetivos
- [x] `magi-nucleo` reúne, movidos sin reescribir, el modelo, el almacén con sus migraciones, la sincronización (formato, cifrado, fusión, aplicación, huellas, reloj), la conexión SSH (saltos, huellas, autenticación), la mecánica de los túneles (local, remoto, SOCKS5), el parser de terminal y `registro::anotar`
  - AC: Dado el escritorio tras la reorganización, cuando se ejecuta `TZ=UTC cargo test --workspace --locked`, entonces pasan todas las pruebas que pasaban antes sin cambiar ninguna aserción
- [x] El escritorio depende de `magi-nucleo` y no cambia su comportamiento, su CLI, el protocolo v6 ni `magi.db` (`user_version` 7)
  - AC: Dada la suite de pintado, cuando se ejecuta tras la reorganización, entonces ninguna instantánea cambia
- [x] El núcleo no lee el entorno ni calcula rutas: recibe al iniciarse la carpeta de datos, la ruta de `known_hosts`, la de temporales y el nombre y la plataforma del equipo (en el móvil, el modelo y `android`)
  - AC: Dado `crates/magi-nucleo`, cuando se buscan `directories`, `HOME` o `XDG_`, entonces no aparecen
- [x] Servicios inyectables en el núcleo: `CarpetaSincronizacion` (listar y leer; escribir y borrar solo en el escritorio), `LlaveroSecreto`, `Firmante` (firma con una clave que no está en memoria) y `TrabajoVivo`
- [x] Lo que es solo del escritorio (`magi_config`, `~/.ssh/config`, vigilancia con `notify`, copias de `magi.db`, servidor de sesiones y protocolo, ratatui y tui-term, `secret-tool` y Keychain, QR a medios bloques) se queda en `magi-ssh` y se engancha al núcleo desde fuera
- [x] Pasada de sincronización en modo lector: nunca exporta ni escribe en la carpeta, no hace copia de `magi.db`, no pide confirmación de borrados masivos y solo guarda conflictos de tipo `huella` y `borrado`
  - AC: Dado un fichero de otro equipo que borra 10 de 12 hosts, cuando el móvil hace la pasada, entonces aplica los borrados sin preguntar, no crea copia y la carpeta queda byte a byte igual
- [x] Conversión de la firma DER del Keystore al formato SSH `ecdsa-sha2-nistp256` (r y s como mpint) y de la clave pública X.509 a una línea OpenSSH
  - AC: Dado un vector de prueba fijo, cuando se convierten la firma y la clave, entonces la firma verifica con la línea OpenSSH resultante
- [x] Pantalla del terminal hacia la app como tramos de texto con el mismo estilo, solo de las filas que cambian y a 30 fps como máximo
  - AC: Dado un remoto que imprime sin parar (`yes`), cuando se mira la app, entonces la interfaz responde al tacto y no llegan más de 30 pantallas por segundo
- [x] Codificación de teclas en el núcleo: Ctrl, Alt, flechas en modo normal y en modo aplicación, F1-F12, Inicio, Fin, RePág, AvPág, Supr y pegado entre corchetes
  - AC: Dado el modo aplicación del cursor activo (vim), cuando se pulsa ↑, entonces se envía `ESC O A`; en modo normal, `ESC [ A`

## Interfaz del núcleo (UniFFI)
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

## Proyecto Android
- [x] Proyecto Gradle en `android/` (Kotlin, Jetpack Compose, Material 3) con `applicationId` `org.cuatrodetres.magi`, `minSdk` 30, `targetSdk` 37, versiones fijadas en el catálogo y wrapper con `distributionSha256Sum`
- [x] Gradle compila el núcleo con `cargo-ndk` y el NDK r30 para `arm64-v8a` (release) y además `x86_64` (debug y emulador), y genera los bindings de Kotlin en cada compilación
  - AC: Dado un clon limpio con el SDK y el NDK instalados, cuando se ejecuta `./gradlew assembleDebug`, entonces sale un APK que arranca en el emulador
- [x] Verificación de dependencias de Gradle con SHA-256 (`gradle/verification-metadata.xml`)
- [x] Permisos: solo `INTERNET`, `CAMERA` (pedido al escanear), `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `USE_BIOMETRIC` y `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; ningún permiso de almacenamiento
- [x] Sin copia de seguridad en la nube: `allowBackup="false"` y reglas de extracción de datos que lo excluyen todo
- [x] Sin Google Play Services, Firebase, ML Kit ni analítica

## Seguridad del móvil
- [x] Clave del móvil ECDSA P-256 creada en el Android Keystore al primer arranque (StrongBox si existe; si no, TEE), no exportable y usable solo con el teléfono desbloqueado
  - AC: Dado un móvil sin StrongBox, cuando se crea la clave, entonces se usa el TEE sin error y Ajustes muestra «TEE»
  - AC: Dado que Android invalida las claves (se quitó el bloqueo de pantalla), cuando se abre MAGI, entonces lo explica y ofrece crear una clave nueva y volver a escanear la clave de sincronización
- [x] Sin bloqueo de pantalla configurado en el teléfono, MAGI explica que lo necesita y no continúa
- [x] Bloqueo de la app con huella o PIN del teléfono (BiometricPrompt, biometría fuerte o credencial) al abrirla en frío y al volver tras 5 minutos fuera
  - AC: Dado que se salió de MAGI hace 4 minutos, cuando se vuelve, entonces no pide desbloqueo; a los 6 minutos, sí
- [x] Con la app bloqueada siguen vivas las sesiones y los túneles, y tocar la notificación lleva primero al desbloqueo
- [x] `LlaveroMovil` guarda la clave de sincronización, la anterior y las contraseñas de host (por `uuid` del host) cifradas con AES-GCM y una clave del Keystore; nunca en `magi.db`, DataStore ni logcat
- [x] Las pantallas con secretos (escanear o pegar la clave, contraseña) llevan `FLAG_SECURE`, y en Android 13 o superior la vista previa de recientes sale vacía

## Sincronización (lector)
- [x] Asistente de primer arranque: bloqueo, clave del móvil creada, elegir carpeta y unirse
- [x] Elegir la carpeta con el selector del sistema (`ACTION_OPEN_DOCUMENT_TREE`) con permiso persistente solo de lectura
  - AC: Dado que se reinicia el móvil, cuando se abre MAGI, entonces la carpeta sigue accesible sin volver a elegirla
- [x] Una carpeta sin ficheros de MAGI se rechaza con «crea la sincronización en el escritorio primero»
- [x] Unirse escaneando el QR que enseña el escritorio (CameraX + ZXing) o pegando la clave; se valida descifrando un fichero y comprobando su firma, y tras pegarla se vacía el portapapeles
  - AC: Dada la clave de otra sincronización, cuando se escanea, entonces dice «la clave no abre los ficheros de esta carpeta» y no guarda nada
- [x] `CarpetaMovil` lista y lee por SAF solo `magi-<uuid>.age` con uuid canónico en minúsculas y hasta el tamaño máximo; ignora copias en conflicto y subcarpetas (T81)
- [x] Pasada al abrir la app, al volver a ella, cada 60 s mientras está visible y al deslizar hacia abajo en Hosts
  - AC: Dado que MAGI está en segundo plano, cuando pasan 10 minutos, entonces no se ha hecho ninguna pasada
- [x] Borrados de hosts o túneles con trabajo vivo en el móvil (sesiones, túneles activos o un salto en uso) se aplazan y se aplican solos en la primera pasada sin trabajo vivo, que relee todos los ficheros
  - AC: Dado un host borrado en el escritorio con una sesión abierta en el móvil, cuando llega la pasada, entonces el host sigue; al cerrar la sesión, la siguiente pasada lo borra
- [x] Huellas que llegan de otros equipos: se añaden si el host no tiene ninguna en el móvil; si tiene otra, conflicto de huella que se resuelve en el móvil (aceptar la del otro equipo o mantener la mía)
  - AC: Dado un conflicto de huella, cuando se acepta la del otro equipo, entonces se sustituye en el `known_hosts` del móvil y queda en REGISTRO
- [x] `esperando_clave` como aviso no bloqueante («un fichero no abre con la clave: si la cambiaste en el escritorio, escanea la nueva»); el móvil sigue intentándolo y vuelve solo a `activa` si el fichero vuelve a abrir
- [x] Pantalla Sincronización: carpeta, huella corta de la clave, estado, última pasada y equipos con plataforma, versión, último fichero y estado
- [x] «Volver a escanear la clave»: la nueva pasa a ser la actual y la anterior solo lee durante 30 días, como en la F10
- [x] «Dejar de sincronizar» con confirmación: borra la clave y el inventario del móvil (exige cerrar antes sesiones y túneles) y conserva la clave del móvil, las contraseñas y las huellas
- [x] Indicador `⇅` en la barra de Hosts (al día, sincronizando, clave nueva, error) con texto además del color

## Inventario (solo lectura)
- [x] Hosts por grupos plegables (el plegado es propio del móvil y no viaja), con «sin grupo» al final
- [x] Fila de host con glifo de estado (`●N` sesiones, `◐` conectando, `○`, `✕` último error), nombre, `usuario@dirección:puerto` y etiquetas; `ultimo_estado` y `ultima_conexion_en` se guardan en el móvil como campos propios
- [x] Búsqueda por subcadena en nombre, dirección, usuario y etiquetas
- [x] Ficha de host de solo lectura: dirección, puerto, usuario, grupo, etiquetas, salto, keepalive, identidad en el escritorio, cómo entra el móvil, huellas conocidas en el móvil y si hay contraseña guardada en este móvil
- [x] Botón de conexión en cada fila y «Abrir otra sesión» en la ficha
- [x] Ninguna pantalla permite crear, editar ni borrar hosts, grupos, identidades o túneles
- [x] Estado vacío «Aún no hay inventario» con acceso al asistente de sincronización

## Conexión y autenticación
- [x] Autenticación en cadena: clave del móvil, luego contraseña guardada en el móvil, luego diálogo de contraseña (3 intentos, «recordar en este móvil»)
  - AC: Dado un host con la clave del móvil instalada que en el escritorio entra por contraseña, cuando se conecta desde el móvil, entonces entra con la clave sin pedir nada
- [x] Diálogo de contraseña enmascarado; la contraseña viaja al núcleo como bytes que se borran tras usarla
- [x] Huella desconocida: diálogo con tipo y SHA256; aceptarla la guarda en el `known_hosts` del móvil y queda en REGISTRO
- [x] Huella cambiada: bloqueo en rojo con la anterior y la nueva; sustituirla exige escribir el nombre del host y queda en REGISTRO
- [x] Saltos (`ProxyJump`) en cadena hasta 3 niveles, cada uno con su huella y su autenticación
- [x] Keepalive del host o, si no tiene, 30 s en el móvil
- [x] Una sola conexión por host compartida por sus sesiones y túneles en el móvil (`multiplexar` no se aplica)
- [x] Errores con motivo legible (no se resuelve, plazo de 10 s, rechazo, autenticación rechazada, teléfono bloqueado) y `ultimo_estado` = error

## Clave del móvil
- [x] Ajustes > Clave del móvil: línea OpenSSH, huella SHA256, dónde vive (StrongBox o TEE) y fecha de creación
- [x] Copiar y compartir la clave pública (hoja de compartir de Android) y mostrarla como QR
- [x] «Instalar mi clave en este host»: entra con contraseña, añade la clave a `~/.ssh/authorized_keys` con un script embebido y comprueba que la clave entra
  - AC: Dado un `authorized_keys` sin salto de línea final, cuando se instala, entonces la clave queda en una línea propia; si ya estaba, no se duplica
  - AC: Dado un host que lee las claves de otro sitio (`AuthorizedKeysFile`), cuando la comprobación falla, entonces dice «instalada, pero el host no la acepta»
- [x] El script de instalación crea `~/.ssh` (700) y `authorized_keys` (600) si faltan, no toca otras líneas, ejecuta `restorecon` si existe y recibe la clave como argumento validado, nunca interpolada; deja `clave_instalada` en REGISTRO
- [x] «Olvidar contraseña» en la ficha, con confirmación

## Sesiones y terminal
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
  - AC: Dado `htop` abierto, cuando se despliega el móvil, entonces `htop` se redibuja al tamaño nuevo en menos de 1 s

## Segundo plano
- [x] Servicio en primer plano `specialUse` con notificación «MAGI · N sesiones · M túneles» mientras haya alguna sesión o túnel; se para con el último
- [x] La notificación solo abre la app: no tiene acciones que cierren nada (T6)
- [x] Guía para sacar MAGI del ahorro de batería (petición del sistema y pasos de MagicOS), ofrecida al abrir la primera sesión y siempre desde Ajustes
- [x] Si Android cerró MAGI con sesiones abiertas, al volver lo dice y ofrece la guía de batería
- [x] Sin sesiones ni túneles, MAGI no hace ningún trabajo en segundo plano

## Túneles
- [x] Pantalla Túneles con los túneles del inventario agrupados por host y bloque «Túneles» en la ficha: tipo, escucha → destino, automático, estado (`○` `◐` `●` `✕`), conexiones y bytes
- [x] Activar y parar un túnel con un interruptor; uno caído muestra su error y «Relanzar»
- [x] Túneles local, remoto y dinámico (SOCKS5, solo CONNECT) con la misma mecánica que el escritorio
- [x] En el móvil todo túnel local o dinámico escucha en `127.0.0.1`, diga lo que diga el inventario, con aviso en la fila si la dirección era otra
- [x] Puerto ocupado o menor que 1024: error con «Abrir en un puerto libre», que muestra el puerto asignado y no lo guarda
  - AC: Dado un túnel local en `127.0.0.1:80`, cuando se activa, entonces falla con «puerto reservado en Android», y «Abrir en un puerto libre» lo abre en uno libre que se muestra
- [x] Automáticos: se activan con la primera sesión del host y se paran con la última; los manuales siguen
- [x] «Abrir en el navegador» en un túnel local activo (`http://127.0.0.1:<puerto>`)
- [x] Los túneles activos cuentan para el servicio en primer plano y mantienen viva la conexión del host

## Interfaz y estética
- [x] Tema oscuro de la cabina de MAGI sobre Material 3: paleta fija (`#FFA000`, `#00FF6A`, `#FF2A1A`, `#5A5A5A`, `#EDE8DC` sobre `#0A0A0A`) y JetBrains Mono en toda la app
- [x] Barra inferior Hosts · Sesiones · Túneles · Ajustes en pantalla compacta
- [x] Pantalla plegable o ancha: riel de navegación y dos paneles (lista y ficha o terminal); al desplegar, la sesión abierta sigue en el panel derecho
- [x] Glifo y texto además del color en todos los estados (T5), descripciones para TalkBack y objetivos táctiles de 48 dp
- [x] Indicadores de carga (conectando, sincronizando) y mensajes de error con instrucción en todas las pantallas
- [x] Ajustes: Sincronización, Clave del móvil, Terminal (tamaño de letra por defecto), Batería, Borrar datos y Acerca de
- [x] «Borrar todos los datos de MAGI» con confirmación escribiendo «borrar»: inventario, clave de sincronización, contraseñas, huellas y clave del móvil
- [x] Acerca de: versión de la app y del núcleo, `formato` de sincronización, `user_version`, licencia y pantalla Licencias generada en la compilación (dependencias Kotlin, crates Rust y la OFL de JetBrains Mono)

## Calidad, CI y publicación
- [x] `ci.yml` pasa a `--workspace`: tests, clippy y `cargo deny` cubren `magi-nucleo` y `magi-movil`
- [x] `android.yml`: compila el núcleo para Android, ejecuta `lint` y las pruebas de Kotlin en la JVM (con el núcleo compilado para el anfitrión) y genera el APK de depuración
- [x] Prueba de humo en el emulador (API 30, x86_64) en `main` y en las etiquetas `android-v*`
  - AC: Dado el emulador, cuando corre la prueba, entonces la app carga el núcleo nativo, importa un fichero de ejemplo desde una carpeta de prueba y pinta sus hosts
- [x] `release.yml` (dist) no se dispara con las etiquetas `android-v*`
- [x] `android-release.yml` con la etiqueta `android-vX.Y.Z`: APK `arm64-v8a` firmado (esquemas v2 y v3), `apksigner verify`, `.sha256`, atestación y GitHub Release que nunca se marca como «Latest»; con `-rc.N`, prerelease
  - AC: Dada la etiqueta `android-v0.1.0-rc.1`, cuando termina el workflow, entonces hay una prerelease con el APK, su `.sha256` y su atestación, y la última release del escritorio sigue como «Latest»
- [x] La firma usa un keystore que solo vive en los secretos de ese workflow (`ANDROID_KEYSTORE_BASE64`, sus contraseñas y el alias); las notas de la release incluyen la huella SHA-256 del certificado
- [x] `versionCode` derivado de la versión con la fórmula del informe (§7.10) y `versionName` igual a la versión
- [x] `android/CHANGELOG.md` (Keep a Changelog) y notas de la release tomadas de su sección
- [x] README con la sección «App Android» (Obtainium filtrando `android-v`, verificar el APK y su certificado, Syncthing-Fork en «solo recibir», qué hacer si pierdes el móvil) y `docs/publicar-android.md`
- [x] Pruebas del núcleo en el anfitrión: firma DER a SSH, modo lector (borrados masivos, aplazados y conflictos), codificación de teclas, tramos de pantalla, túnel en puerto libre y script de instalación con `sh` en un HOME temporal
- [x] Revisión adversarial de cierre (T31, T54) sobre el núcleo movido, `magi-movil` y la app, anotada en el informe de implementación

---

**Progreso Fase 11:** 110 / 110 funcionalidades

**Total MAGI (Fases 1-11):** 823 / 827 funcionalidades
