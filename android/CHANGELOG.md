# Registro de cambios de la app Android

Todos los cambios visibles de la app Android de MAGI se anotan en este fichero.
El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el
proyecto usa [Versionado Semántico](https://semver.org/lang/es/) orientado al
usuario: la política completa está en
[`docs/publicar-android.md`](../docs/publicar-android.md) y la del escritorio
en [`docs/publicar.md`](../docs/publicar.md).

## [Sin publicar]

<!-- Añade aquí las líneas de la próxima versión (Añadido, Cambiado,
     Corregido, Eliminado y Seguridad) en el mismo PR que haga el cambio. Al
     publicar, pasa esta sección a «## [X.Y.Z] - fecha», actualiza
     android/version.properties y crea la etiqueta android-vX.Y.Z. -->

### Cambiado

- En plegable o tableta, al abrir una sesión el terminal ocupa toda la
  pantalla, y cuando no hay nada seleccionado la lista de Hosts, Sesiones,
  Túneles o Ajustes ocupa todo el ancho: desaparece el panel vacío con «Elige
  un host…».

### Cambiado

- La cabecera de Túneles dice «1 activo» en singular; «auto» va en una línea y
  los nombres largos no se parten.

### Cambiado

- Al abrir una sesión, el terminal ocupa toda la pantalla: se ocultan la barra
  inferior, el riel y la lista del plegable, y las barras del sistema se
  esconden mientras está abierto (un deslizamiento las muestra). Al volver se
  restaura la navegación.

### Corregido

- El túnel local ya no se queda «activo» si cae la conexión del host: pasa a
  caído con el motivo y se puede relanzar. La fila muestra el último error con
  su hora y los contadores de conexiones abiertas y aceptadas, que se
  actualizan cada segundo.
- La nota de puerto libre solo aparece al abrir en un puerto libre; la fila
  muestra siempre la escucha real.

### Corregido

- Fallo de desbloqueo en algunos teléfonos (p. ej. Honor MagicOS): las claves
  del Keystore ya no llevan el requisito de «dispositivo desbloqueado» que
  Android no reconoce al desbloquear con la cara. Las claves antiguas se migran
  una sola vez; la clave del móvil cambia de identidad, así que hay que
  reinstalarla en los hosts.

## [0.1.0] - 2026-10-31

Primera versión de la app Android de MAGI (Fase 11). Es **lectora** de la
sincronización cifrada del inventario: nunca escribe en la carpeta compartida
ni exporta. Requiere Android 11 o superior y APK `arm64-v8a`. Sin Google Play
Services, sin cuenta y sin telemetría; se instala y actualiza con Obtainium
desde las releases del repositorio.

Limitaciones conocidas: un fichero a medio copiar puede dejar la sincronización
en «esperando clave» hasta la pasada siguiente (R85); un elemento inválido hace
fallar la pasada entera (R86); aceptar la huella de otro equipo sustituye todas
las del host (R87); el contenido del terminal no se lee con TalkBack en esta
fase.

### Añadido

- **Inventario de solo lectura.** Hosts por grupos plegables (el plegado es
  propio del móvil y no viaja), búsqueda por subcadena, fila con estado,
  `usuario@dirección:puerto` y etiquetas, ficha de solo lectura, botón de
  conexión y estados vacíos. Ninguna pantalla permite crear, editar ni borrar.
- **Sincronización como lector.** Elección de la carpeta con el selector del
  sistema (permiso persistente de solo lectura), unión con la clave del QR o
  pegada y validada antes de guardarla, pasadas al abrir la app, al volver a
  ella, cada 60 s mientras está visible y al deslizar; borrados de hosts y
  túneles con trabajo vivo aplazados, huellas que llegan de otros equipos con
  conflicto resoluble en el móvil, «volver a escanear la clave», «dejar de
  sincronizar» e indicador `⇅` en la barra con texto además del color.
- **Clave del móvil.** ECDSA P-256 en el Android Keystore (StrongBox si
  existe, TEE si no), no exportable y usable solo con el teléfono desbloqueado;
  línea OpenSSH y QR para compartirla, y «instalar mi clave en este host» con
  un script embebido que la recibe como argumento validado.
- **Bloqueo de la app.** Huella o PIN del teléfono al abrir y tras 5 minutos
  fuera; sin bloqueo de pantalla configurado la app no arranca; las pantallas
  con secretos llevan `FLAG_SECURE` y la vista previa de recientes se oculta
  en Android 13 o superior.
- **Conexión SSH.** Autenticación en cadena (clave del móvil, contraseña
  guardada en el móvil y diálogo con «recordar»), verificación estricta de
  huellas con diálogo de huella nueva y de huella cambiada (escribir el nombre
  del host y queda en REGISTRO), saltos en cadena hasta 3 niveles, keepalive,
  una sola conexión por host compartida por sesiones y túneles, y errores con
  motivo legible.
- **Terminal.** Varias sesiones a la vez (también al mismo host), estados de
  sesión con reconexión, pintado en Compose con JetBrains Mono y colores de 16,
  256 y 24 bits, teclado del sistema en modo bruto, fila deslizable de teclas
  extra (Ctrl/Alt de una pulsación o fijados), teclado físico, zoom de 8 a 24
  sp, historial de 1 000 líneas, selección con tiradores, pegado entre
  corchetes y confirmación al pegar varias líneas, y redimensionado agrupado a
  50 ms.
- **Túneles.** Local, remoto y dinámico (SOCKS5, solo CONNECT) con la misma
  mecánica que el escritorio; en el móvil los locales y dinámicos escuchan
  siempre en `127.0.0.1`, el puerto reservado o menor que 1024 ofrece «Abrir
  en un puerto libre» y hay «Abrir en el navegador» para un túnel local activo.
- **Segundo plano.** Servicio en primer plano `specialUse` con la notificación
  «MAGI · N sesiones · M túneles» mientras haya alguna sesión o túnel; la
  notificación solo abre la app. Guía para sacar MAGI del ahorro de batería y
  aviso al volver si Android cerró la app con sesiones abiertas.
- **Interfaz.** Tema oscuro de la cabina sobre Material 3 con la paleta fija y
  JetBrains Mono; barra inferior Hosts · Sesiones · Túneles · Ajustes y riel
  con dos paneles en plegable; accesibilidad para TalkBack y objetivos de 48
  dp; Ajustes con sincronización, clave del móvil, terminal, batería, «borrar
  todos los datos» y Acerca de con la pantalla de licencias.
- **Publicación.** `android-release.yml` compila el APK release
  `arm64-v8a`, lo firma con el keystore de los secretos (esquemas v2 y v3),
  lo verifica con `apksigner`, publica su `.sha256`, atesta la procedencia y
  crea una GitHub Release que nunca se marca como «Latest»; con `-rc.N` es una
  prerelease. `docs/publicar-android.md` es la guía para el desarrollador.

### Seguridad

- La clave del móvil nunca sale del Keystore: el núcleo firma a través de
  `FirmanteMovil` y la clave privada no pasa por la memoria del proceso.
- La clave de sincronización (actual y anterior) y las contraseñas de los hosts
  viven cifradas con AES-256-GCM y una clave del Keystore
  (`LlaveroMovil`), nunca en `magi.db`, DataStore ni logcat.
- La carpeta compartida es entrada no fiable: solo se aceptan ficheros
  `magi-<uuid>.age` con el uuid canónico en minúsculas, sin seguir enlaces y
  con el tamaño máximo; nada se importa sin firma válida.
- El APK lo firma el CI con un keystore que solo recibe
  `android-release.yml`; las notas de cada release publican la huella SHA-256
  del certificado y la atestación de procedencia.
- Sin Google Play Services, Firebase, ML Kit, analítica ni copia en la nube
  (`allowBackup="false"`); las únicas conexiones salientes son a los hosts del
  usuario.

[Sin publicar]: https://github.com/4d3-org/magi/compare/android-v0.1.0...HEAD
[0.1.0]: https://github.com/4d3-org/magi/releases/tag/android-v0.1.0
