# Registro de cambios

Todos los cambios visibles de MAGI se anotan en este fichero. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto usa
[Versionado Semántico](https://semver.org/lang/es/) orientado al usuario: la
política completa está en [`docs/publicar.md`](docs/publicar.md).

## [Sin publicar]

### Añadido

- **MAGI en Termux, llavero con frase maestra (Fase 17, primera entrega).**
  MAGI se prepara para correr en Android dentro de Termux: el mismo binario
  (TUI, servidor y CLI) compilado para `aarch64-linux-android`, con lo propio de
  Termux aislado en `src/termux/`. Allí las contraseñas de host y la clave de
  sincronización viven en `llavero.age` (age con una entrada scrypt, 600) y se
  abren con una **frase maestra** que solo existe en la memoria de MAGI: nunca
  va a disco, al log, a REGISTRO ni al servidor. Diálogos LLAVERO DE MAGI (al
  abrir), CREAR LA FRASE MAESTRA (al guardar el primer secreto: recordar una
  contraseña, crear o unir la sincronización; con Esc la contraseña se usa sin
  guardarse), CAMBIAR LA FRASE MAESTRA, BORRAR EL LLAVERO (escribiendo `borrar`)
  y LLAVERO DAÑADO (se aparta como `llavero.age.danado-<fecha>`; un llavero
  dañado nunca se borra solo), cinco frases incorrectas seguidas imponen 30 s de
  espera, la barra dice «llavero bloqueado» cuando algo lo necesita y la paleta
  trae «Desbloquear el llavero», «Bloquear el llavero», «Cambiar la frase
  maestra» y «Borrar el llavero». Con el llavero cerrado, una conexión
  interactiva pide primero la frase y lo automático (reconexión, sondeo,
  ejecuciones) da un error con instrucción y nunca abre un diálogo. Dos ventanas
  de MAGI escriben el fichero con `flock` y lo releen antes de escribir. En el
  escritorio no cambia nada (siguen secret-tool y Keychain). Cinco tipos nuevos
  en el historial: `llavero_creado`, `llavero_frase_cambiada`, `llavero_borrado`,
  `llavero_desbloqueo_fallido` y `llavero_apartado`.
- **`magi llavero estado|cambiar|borrar`**, solo en Termux (fuera de él dicen
  por qué no aplican y salen con código 1): `estado` enseña ruta, tamaño y fecha
  sin pedir la frase; `cambiar` y `borrar` piden la frase y la confirmación en
  el TTY, sin eco. `magi sincronizar` pide la frase una sola vez cuando el
  llavero está cerrado y `magi sondear` solo si algún host que sondea usa la
  contraseña del llavero; sin TTY dan un error con instrucción.

- **MAGI en Termux, segundo plano, avisos, sincronización y portapapeles (Fase
  17, segunda entrega).**
  - **`[termux]` en `config.toml`** (`wake_lock`, `avisos`, `api_plazo_seg` de 2 a
    30 y `sincronizacion_sondeo_seg` de 10 a 600): fuera de rango se acota y se
    avisa al arrancar; fuera de Termux se ignora y no se escribe mientras sea la
    de siempre (una sección escrita a mano se conserva).
  - **Wake lock.** El servidor toma el wake lock de Termux (`termux-wake-lock`)
    mientras tiene trabajo (sesiones, túneles, transferencias, escritorios) y lo
    suelta 30 s después de quedar sin él; con `wake_lock = "nunca"` no lo toma. Si
    el servidor muere sin soltarlo (Android cierra Termux), el siguiente lo suelta
    en cuanto no tenga trabajo y vuelve a tomarlo si lo hay. Si no se puede tomar,
    la barra dice «sin wake lock: Android puede dormir MAGI»; en Android 12 o
    posterior se avisa una sola vez de que Android puede cerrar procesos de Termux
    (`magi diagnostico`).
  - **Avisos de Android** (Termux:API, `[termux] avisos`): cuando una sesión
    persistente (vigilante) o una pestaña pide atención con nadie mirando, el
    servidor publica un aviso con solo el nombre y el host («◆ agente-api pide
    atención» · «dev-01 · MAGI»); se retira al quitarse la atención, al cerrar la
    pestaña o borrar la ficha y al terminar el servidor (los de un servidor muerto,
    al arrancar el siguiente). En Termux el vigilante de las sesiones persistentes
    corre también sin ventanas mientras viva el servidor y haya Termux:API.
    `magi termux aviso` publica uno de prueba y lo retira a los 10 s. Si Termux:API
    no responde, la barra lo dice una vez y todo sigue como en el escritorio.
  - **Sincronización.** En Termux la carpeta se guarda resuelta con `realpath`
    (`~/storage/shared/…` → `/storage/emulated/0/…`; sin `~/storage`, MAGI pide
    ejecutar `termux-setup-storage`), se sondea cada
    `[termux] sincronizacion_sondeo_seg` con una firma barata de la carpeta en vez
    de usar `notify` (más una pasada de seguridad cada 10 min y otra al volver a
    la vista; sin pasadas mientras el almacenamiento compartido no responde), y un
    `chmod 600` que el almacenamiento no admite no es un error (una línea en el
    log por sesión). El equipo se llama `termux-<modelo>` y escribe con la
    plataforma `android`, sin `formato` nuevo.
  - **Portapapeles y enlaces.** Copiar usa `termux-clipboard-set` por la entrada
    estándar (hasta 512 KiB) y, si Termux:API no responde, OSC 52 (hasta 6 000
    bytes codificados), con el aviso una sola vez; si no cabe, la barra dice el
    tamaño y la alternativa. `[terminal] abrir_enlaces` vale `termux-open-url` y
    `[escritorio] navegador = "auto"` abre el visor en el navegador de Android;
    `prefijo w` explica cómo abrir otra sesión de Termux. La barra dice, una vez por
    servidor, lo que este deja en el estado de Termux (Termux:API apagada, sin wake
    lock, límite de procesos). Copiar la clave pública, el enlace del visor y la
    clave generada comparten una sola vía; en el escritorio se comportan igual que
    antes.

- **Sesiones persistentes (Fase 16).** Una sesión de terminal con nombre que
  **sobrevive a MAGI y al equipo**: vive en un **tmux propio de MAGI** en el
  host (`tmux -L magi -f /dev/null`, con su configuración y sin tocar el tmux ni
  el `~/.tmux.conf` del usuario; hace falta tmux 3.0 o posterior), se marca al
  abrirla (nombre, carpeta, comando de arranque y de reanudación, y nota) o
  para todo un host («Todas las sesiones persistentes» en la ficha) y se
  retoma desde cualquier equipo de escritorio. El bloque **Persistentes** de
  la vista Sesiones (`prefijo l`) lista las fichas de todos los hosts con su
  estado (libre, enganchada, terminada, sin comprobar, inalcanzable o sin
  tmux), quién la tiene, la actividad, la atención, el título y «antigua»; con
  las sesiones del tmux de MAGI sin ficha y las del tmux de siempre; permite
  engancharse soltando a los demás (`↵`) o compartiendo (`c`), vista previa
  (`v`), editar, renombrar, terminar y borrar la ficha (que nunca termina la
  sesión), reabrir una terminada con su comando de reanudación y borrar las
  terminadas. Una pestaña cuya sesión coge otro equipo queda **soltada**, y la
  reconexión automática nunca se la quita. Un **vigilante** en el servidor
  comprueba las fichas con alguna ventana abierta (`[persistentes]
  vigilar_seg`, sin diálogos: un host que pide huella, frase o contraseña sale
  «inalcanzable», nunca «terminada») y avisa en la barra de cualquier vista
  cuando una sesión pide **atención** con la campana de tmux (Claude Code debe
  usar `preferredNotifChannel = "terminal_bell"`). El historial de una pestaña
  persistente sale de tmux (`prefijo [`). Las fichas viajan con la
  sincronización (el estado y el contenido, no); `formato` 4 solo si hay alguna
  ficha o algún host marcado. MAGI entiende `modifyOtherKeys` (`CSI > 4 ; n m`),
  que es como tmux pide las teclas extendidas (Mayús+Intro, Ctrl+Intro…).
  Nuevos subcomandos `magi sesion <nombre> [--host <host>]` (abre MAGI
  enganchado a la sesión) y `magi sesiones [--json]` (lista las fichas con su
  estado preguntando al servidor, sin diálogos; el JSON está documentado en el
  README) y sección `[persistentes]` en `config.toml` (`vigilar_seg`,
  `refresco_seg`, `antigua_dias` y `vista_previa_lineas`). Migración 11
  (tabla `SESIONES_PERSISTENTES` y `HOSTS.sesiones_persistentes`) y protocolo
  v10. La app Android lee el `formato` 4 (guarda las fichas y la marca del host
  sin enseñarlas ni usarlas); las sesiones persistentes en el móvil quedan
  pendientes.

- **Terminal, segunda vuelta (Fase 15).** La vista Sesión gana **paneles**
  (completa, lado a lado, apilados, 1 grande + 2 y rejilla de 4; `prefijo
  Espacio` para cambiar, flechas para mover el foco, `z` para ampliar y
  «abrir en rejilla…» en la paleta), **historial** de 10 000 líneas guardado en
  el servidor con modo historial (`prefijo [` o `Shift+RePág`: navegación,
  búsqueda de texto o expresión regular, selección, copia con el teclado y
  guardado en un fichero), **enlaces** `http`/`https` con letras y diálogo
  (abrir con el navegador del sistema, copiar, o túnel temporal para los de
  `localhost` del host), **escritura en varios** paneles con deliberación MAGI
  y concesión del servidor (aviso rojo fijo, `»` en cada panel, apagado a los
  10 minutos), **reconexión automática** sin diálogos (2, 5, 10 y 30 s y luego
  cada minuto hasta `reconexion_max_min`, cuenta atrás, `prefijo r` para
  reintentar a mano) y **sesión persistente con tmux** por host (la pestaña
  abre o recupera `magi-<equipo>-<n>`; la vista Sesiones lista, engancha y
  termina sesiones de tmux). Se pueden renombrar (`prefijo ,`) y mover
  (`prefijo <` `>`) pestañas, y el título y los avisos del remoto (campana,
  OSC 9 y OSC 777) se ven en la pestaña y en Sesiones.
  MAGI se comporta como un terminal de verdad para lo que corre dentro
  (Claude Code como referencia): pegado entre corchetes de punta a punta (un
  solo bloque, nunca como teclas sueltas), respuestas a DA1, DA2, DSR,
  DECRQM, XTVERSION, XTWINOPS y OSC 10-12 solo desde el servidor, teclado de
  kitty con la bandera 1 (Mayús+Intro es `CSI 13;2u`), salida sincronizada
  (2026), ED 3, eventos de foco (1004), ratón transparente para el programa
  que lo pide y portapapeles del remoto (OSC 52) con permiso por host
  (`prefijo P`); leer el portapapeles, nunca. El terminal exterior queda como
  estaba al salir, al ceder la terminal y tras un pánico. Migración 10
  (columnas propias del equipo `reconectar`, `tmux_persistente` y
  `portapapeles_remoto` en `HOSTS`, que no viajan), protocolo v9 y sección
  `[terminal]` en `config.toml`; el móvil no cambia.

- **Móvil de guardia (Fase 14).** La app Android añade Flota con sondeo como
  pantalla de inicio, Servicios con logs y acciones, snippets con deliberación
  y Registro, con la lógica compartida en `magi-nucleo`; sigue siendo lectora
  de la sincronización y no cambia en esta fase.
- **Servicios, contenedores y logs (Fase 13).** Vista **Servicios** (`F9`, y `u`
  desde Flota, Hosts y la ficha) con los hosts SSH a la izquierda y, a la
  derecha, los servicios y timers de systemd y los contenedores de Docker o
  Podman agrupados por proyecto Compose; arriba los vigilados (`★`) y los
  fallidos, «Toda la flota» con los fallos del último sondeo de cada host y
  `⚠N` en Flota. Detalle de estado, dependencias y unidad o `inspect`; acciones
  arrancar, parar, reiniciar, recargar y activar o desactivar el arranque
  (`A` `P` `R` `G` `H`, en mayúsculas) con espera al estado final, segunda
  mirada a los 2 s y las últimas líneas del diario si algo no vuelve; timers con
  su próxima y última ejecución y lanzar ahora; contenedores con acciones,
  consumo y shell en una pestaña (`S`); proyectos Compose enteros con `-p` y
  los ficheros de sus etiquetas. Lo que deja algo parado pasa por la
  deliberación MAGI (`PlanDeliberado::Servicio`, con MELCHIOR-1 sin contar el
  estado del propio objetivo) y lo demás pide confirmación. `sudo` orden a
  orden, nunca `sudo sh`: `sudo -n` o una contraseña pedida por el cliente y
  comprobada con `ComprobarSudo`, que llega al host por la entrada estándar y
  se recuerda solo en memoria hasta `sudo_memoria_min` minutos (nunca va a
  disco, al llavero, al log ni a REGISTRO). Antes de recargar o reiniciar
  Nginx, Caddy, Apache, HAProxy o un proyecto Compose, y con una comprobación
  propia por servicio o proyecto (COMPROBACIONES_SERVICIO), se valida la
  configuración dentro de la propia acción: si falla no se ejecuta nada. Logs
  en vivo en un panel con lotes de 100 ms, contrapresión, anillo de 10 000
  líneas, filtros por texto y nivel, pausa y `t` para el seguimiento completo
  en una pestaña; al cerrar el panel se corta `journalctl` en el host. El
  sondeo amplía sus fallos (servicios fallidos y contenedores caídos) sin
  cambiar el estado del host, y las acciones en curso las custodia el servidor
  (`AccionesServicio{lista}`, `TrabajoServidor`). Migración 9
  (`COMPROBACIONES_SERVICIO` y `SONDEOS.fallos_json`), protocolo v8
  (`peticion_id` de servicios desde 2^45) y sección `[servicios]` en
  `config.toml`. Las comprobaciones propias viajan como elemento
  `comprobacion_servicio` y suben el `formato` de sincronización a 3 solo si
  hay alguna; la app Android lee el formato 3 y las guarda en su copia sin
  usarlas hasta la Fase 14.

- **Escritorio remoto (Fase 12).** Hosts de tipo RDP, VNC y voxl en el mismo
  inventario que los hosts SSH, con «a través de» un host SSH y opciones por
  tipo. El escritorio se ve en una ventana del navegador servida por
  `magi --servidor` solo en `127.0.0.1`, con enlaces de un solo uso y clave
  por sesión. Cliente RDP con IronRDP (TLS con rustls, NLA/NTLM, huella del
  certificado con la misma política que las huellas SSH, y mapas de bits y
  RemoteFX como rectángulos; el H.264 de Windows queda pendiente de decodificar
  los demás códecs de EGFX, ver el informe de la fase) y cliente VNC propio
  (RFB 3.3/3.7/3.8, Raw, CopyRect, Hextile, ZRLE, portapapeles y cambio de
  tamaño). `voxl` se arranca
  por SSH con el script embebido `escritorio_voxl.sh` y se ve por el túnel de
  la sesión. Sesiones custodiadas por el servidor con pestaña propia, un minuto
  de gracia sin visor y un solo visor por sesión. Migración 8 (`tipo`,
  `escritorio_dominio`, `escritorio_opciones` y `huella_escritorio` en
  `HOSTS`), `formato` 2 de sincronización cuando hay escritorios, protocolo v7
  y sección `[escritorio]` en `config.toml`.

- **Sincronización cifrada del inventario (Fase 10).** El mismo inventario en
  todos los equipos del usuario mediante ficheros cifrados con age (X25519) y
  firmados con HMAC-SHA256 en una carpeta que él elige (Syncthing, Nextcloud,
  iCloud, git…). Cada equipo escribe su propio `magi-<uuid>.age` (instantánea
  completa) y lee los de los demás; `magi.db` no se cifra. Migración 7 con
  `uuid`, `version` y `vector` en las tablas que viajan y las tablas nuevas
  `EQUIPOS`, `LAPIDAS` y `CONFLICTOS`, reloj híbrido, fusión determinista por
  vectores con conflictos recuperables, lápidas permanentes, unión por nombre al
  añadir un equipo, huellas de `known_hosts` que solo se añaden si no había
  ninguna, copia de `magi.db` y confirmación de borrados masivos, clave en el
  llavero mostrada como texto y QR, rotación al quitar un equipo, vistas
  SINCRONIZACIÓN y CONFLICTOS con indicador `⇅` en la barra y
  `magi sincronizar [estado]`. El formato está documentado en
  [`docs/formato-sincronizacion.md`](docs/formato-sincronizacion.md) para la app
  Android.

### Cambiado

- Versión 1.5.0 (MINOR: funcionalidad nueva, migración de esquema 11,
  protocolo 10 y `formato` 4 de sincronización solo cuando hay fichas de
  sesiones persistentes o hosts marcados).

- **La marca «sesión persistente con tmux» de la 1.4.0 pasa a «Todas las
  sesiones persistentes» y ahora viaja (Fase 16).** `HOSTS.tmux_persistente`
  (propio de cada equipo y sin sincronizar) queda obsoleto: al abrir la 1.5.0,
  la migración 11 copia su valor a `HOSTS.sesiones_persistentes` por el camino
  que sube la versión del host, apaga la antigua y avisa una vez. La marca
  ahora se sincroniza como el resto de la ficha del host (y está en el bloque
  SESIONES, ya no en TERMINAL). Las sesiones nuevas ya no se llaman
  `magi-<equipo>-<n>` sino `<equipo>-<n>`, van al tmux propio de MAGI y no al
  habitual; las que creó la 1.4.0 en el tmux de siempre se siguen viendo,
  enganchando y terminando desde Sesiones, pero no se migran (un proceso no se
  puede mover de un servidor de tmux a otro).

- **La lista de tmux de la 1.4.0 se integra en «Persistentes» (Fase 16).** `t`
  en «Pestañas» y la entrada de paleta «sesiones de tmux» ya no abren una lista
  propia: comprueban el host de la pestaña y enseñan sus sesiones (las del tmux
  de MAGI sin ficha y las del tmux de siempre, «de tu tmux») en el bloque
  «Persistentes» de la vista Sesiones, desde donde se enganchan y se terminan
  igual que antes.

- **La tecla `N` queda reservada en Flota (Fase 16).** Abre el diálogo «Nueva
  sesión persistente» con el host de la fila (también desde Hosts). Un atajo
  `N` de `[flota.atajos]` en `config.toml` se ignora ahora con el aviso de
  siempre al arrancar; usa otra letra.

- **Protocolo cliente ⇄ servidor v10 (Fase 16).** `AbrirSesion.tmux` pasa a
  llevar `servidor`, `nombre`, `crear` y `compartir` (cambia la semántica: por
  eso sube la versión), `TerminarTmux` y `SesionesTmux` distinguen el servidor
  de tmux y se añaden `ConsultarSesionesPersistentes`, `SesionesPersistentes`,
  `PedirVistaPrevia`, `RenombrarTmux` y `AperturaTmuxFallida`. Un servidor de la
  1.4.0 en marcha se sustituye al abrir la 1.5.0 como siempre: solo si no
  custodia nada, y si no, MAGI pregunta.

- **Para quien actualiza de la 1.4.0 a la 1.5.0.** La base de datos pasa sola
  de `user_version` 10 a 11 al abrir MAGI (hacia delante: una `magi.db` migrada
  no la abre un binario anterior). Si tenías hosts con «sesión persistente con
  tmux», pasan a «Todas las sesiones persistentes», que ahora viaja a tus otros
  equipos (si dos equipos tenían valores distintos, gana el último cambio).
  Actualiza todos los equipos de escritorio: uno con MAGI 1.4.0 o anterior verá
  el fichero de sincronización como «formato nuevo» en cuanto otro equipo cree
  una ficha de sesión persistente o marque un host, y no importará nada de él
  hasta que se actualice; la app Android debe ser una versión que lea el
  `formato` 4 (la de esta fase). Si el servidor de sesiones de la 1.4.0 sigue
  abierto, `magi` lo sustituye solo si no custodia nada y, si no, te lo
  pregunta. En el host hace falta tmux 3.0 o posterior (MAGI no lo instala), y
  para ver los avisos de atención de Claude Code hay que activar en su
  configuración la campana del terminal (`preferredNotifChannel =
  "terminal_bell"`).

- Versión 1.4.0 (MINOR: funcionalidad nueva, migración de esquema 10 y
  protocolo 9; `formato` de sincronización sin cambios).

- Versión 1.3.0 (MINOR: funcionalidad nueva, migración de esquema 9,
  protocolo 8 y `formato` 3 de sincronización).

- Versión 1.2.0 (MINOR: funcionalidad nueva, migración de esquema 8,
  protocolo 7 y `formato` 2 de sincronización).

- **Codificación de teclas del terminal unificada (Fase 11).** La tabla
  de bytes de las teclas vive ahora en el núcleo compartido, como la usará la
  app Android. El escritorio gana la codificación estándar de flechas e
  Inicio/Fin con modificadores (`ESC [ 1 ; m X`), Ctrl+Alt+tecla y algunas
  combinaciones de Ctrl que antes se ignoraban; el resto no cambia.

- Versión 1.1.0 (MINOR: funcionalidad nueva y migración de esquema 7).

### Corregido

- **Un fallo al escribir un fichero de MAGI ya no deja su temporal en la
  carpeta.** La escritura atómica (`ficheros.rs`) borra el temporal si falla
  antes del renombrado.
- **Unirse a una sincronización pegando la clave fallaba con «invalid Bech32
  encoding».** Copiada de la terminal, la clave suele llegar partida en dos
  líneas o con espacios, y el escritorio solo quitaba los de los extremos.
  Ahora quita todos los espacios y la pasa a mayúsculas, como el móvil. Si la
  clave no vale, el aviso dice por qué en castellano: está vacía, es la clave
  pública (`age1…`), no empieza por `AGE-SECRET-KEY-1` o está incompleta.

- **Los errores del llavero en Linux no decían qué pasaba.** MAGI descartaba
  lo que `secret-tool` escribía por la salida de error y solo enseñaba «el
  llavero devolvió exit status: 1». Ahora el aviso lleva ese mensaje y, en los
  casos conocidos, qué hacer: sin servicio de llavero en la sesión, instalar
  `gnome-keyring` (o activar KWallet o KeePassXC); sin sesión de D-Bus, abrir
  MAGI desde la sesión gráfica. Si falta `secret-tool`, dice qué paquete
  instalar en Debian y Ubuntu, Arch o Fedora. El README lo pone en los
  requisitos de Linux.

- **Copiar colgaba la aplicación en Wayland.** `wl-copy` se queda como daemon
  sirviendo el portapapeles y hereda su salida de error; al esperar a leer su
  fin la interfaz se quedaba bloqueada para siempre (parecía un cierre). Ahora
  se espera un poco al proceso directo y, si sigue vivo sirviendo el
  portapapeles, se recoge en segundo plano: copiar vuelve al instante. Con
  pruebas de regresión, incluida la de `wl-copy` real cuando hay Wayland.

- **Pegar, seleccionar con el ratón y el foco exterior no funcionaban** (Fase
  15). El hilo de teclas descartaba los eventos `Paste`, `Mouse`, `FocusGained`
  y `FocusLost` del terminal exterior: pegar no hacía nada y, con el ratón
  capturado (Claude Code u otros programas que lo piden), la rueda, el clic y
  el arrastre se perdían; el remoto tampoco veía los cambios de foco. Ahora se
  reenvían al bucle de la UI con test de regresión del hilo.

- Dos aperturas simultáneas de una base de datos con WAL pendiente ya no
  chocan al crear su copia temporal (los nombres llevan un contador además del
  reloj): `magi.db` se valida igual y los tests dejan de fallar cuando corren
  en paralelo.

- **Pegar fuera de una pestaña va al campo con foco** (Fase 15). El texto
  pegado en un diálogo, la búsqueda del historial, la paleta, un filtro, la
  ficha o Servicios se inserta en su campo de texto (sin saltos de línea, que
  se juntan) en vez de descartarse; los diálogos que no tienen campo lo
  ignoran. Al ceder la terminal al visor o al editor se deshacen otra vez el
  pegado entre corchetes, el foco, las banderas de kitty y el ratón (D314).

- **Pantalla negra al abrir un escritorio RDP.** El visor recibía
  `imagen: "h264"` antes de que la fuente lo confirmara, mientras el cliente
  RDP mandaba mapas de bits como rectángulos; el visor los pintaba en su búfer
  y no los presentaba. Ahora la imagen de apertura es siempre «rectángulos» y
  solo se anuncia H.264 cuando llega el primer fotograma de vídeo; además el
  puente cachea el último GOP y pide un repintado completo (Refresh Rect) al
  visor que entra tarde o vuelve a la pestaña.

- Evita el error «database is locked» al escribir el inventario con el
  servidor de sesiones activo: las transacciones del almacén se abren con
  `BEGIN IMMEDIATE`, de modo que esperan el bloqueo en vez de fallar al
  promoverlo.

## [1.0.0] - 2026-10-30

Primera versión pública de MAGI. Reúne las Fases 1-8 del proyecto y el
empaquetado de la Fase 9.

### Añadido

- **Inventario y conexión (Fase 1).** Inventario de hosts en SQLite (WAL) con
  grupos, etiquetas, ficha completa, filtro incremental, paleta de comandos
  (`Ctrl+P`), ayuda contextual (`?`) y tema de Omarchy con paleta de respaldo
  y modo ASCII degradado.
- **Importación y exportación de `ssh_config`.** Importación sin pérdida de
  `~/.ssh/config` (con diálogo de conflictos) y exportación atómica a
  `~/.ssh/magi_config`, con inserción opcional del `Include` y copia
  `config.bak-<fecha>` antes de tocar el fichero del usuario.
- **Conexión SSH embebida (russh).** Tarea tokio con plazo de 10 s en DNS/TCP,
  autenticación con agente o fichero, saltos en cadena, keepalive, vista Sesión
  con `tui-term` y `vt100` a ~30 fps y verificación estricta de huellas contra
  `~/.ssh/known_hosts` (entradas hasheadas incluidas).
- **Flota (Fase 2).** Sondeo bajo demanda de carga, memoria, disco, red y
  servicios systemd con un único script embebido por canal `exec`, semáforo de
  8 y plazo de 5 s; estados FRÍA/CAÍDA/ALCANZABLE/CARGA/NOMINAL, auto-refresco
  opcional y `magi sondear`.
- **Identidades (Fase 2).** Sincronización por huella de `~/.ssh` y del agente,
  generación ed25519 y RSA 4096 con frase opcional, importación, copia de la
  pública, alias y revocación y reactivación (baja lógica) en la vista
  Identidades (`F5`).
- **Registro (Fase 2).** Historial de todo lo que hace MAGI, con filtro, detalle,
  purga de más de 90 días y exportación CSV/JSON; `registro::anotar` es la
  única puerta de escritura y las filas nunca se editan.
- **Contraseñas (Fase 2, anexo aprobado).** Autenticación por contraseña, pedida
  al conectar o recordada en el llavero del sistema (`secret-tool`/libsecret en
  Linux, Keychain en macOS); `magi.db` guarda solo la referencia.
- **Pestañas y servidor de sesiones (Fase 3).** `magi --servidor` custodia las
  sesiones SSH en un socket Unix; cerrar la ventana no las cierra, varias
  terminales se adjuntan a la misma pestaña, hay reconexión tras caída,
  apagado por inactividad y `magi servidor estado`/`magi servidor parar` y
  `magi conectar <host>`.
- **Archivos (Fase 4).** Vista Archivos (`F4`) con panel doble local ⇄ remoto
  sobre SFTP, marcas de diferencia, copiar/mover con política de conflicto,
  aviso de ficheros sensibles, cola de transferencias que vive en el servidor
  (sobrevive a cerrar la ventana) y visor con el paginador del sistema.
- **Túneles (Fase 5).** Túneles `local`, `remoto` y `dinámico` (SOCKS5)
  ejecutados por el servidor de sesiones, automáticos ligados a las pestañas,
  importación y exportación de `LocalForward`/`RemoteForward`/`DynamicForward`,
  vista Túneles (`F6`) y `magi tuneles`/`magi tunel activar|parar`.
- **Snippets y deliberación MAGI (Fase 6).** Comandos guardados con destinos por
  etiqueta o host, variables `{{nombre}}` escapadas con `shell-escape`,
  ejecución masiva en el servidor (hasta 8 hosts en paralelo, stdout y stderr
  separados, timeout y «parar al primer fallo»), vista Resultados, snippet al
  conectar, atajos de Flota y deliberación obligatoria MELCHIOR-1 (salud),
  BALTHASAR-2 (backup) y CASPER-3 (tests) con `Ctrl+K` como única confirmación.
- **Redimensionado adaptable (Fase 7).** Tubería única de tamaño agrupada 50 ms,
  modo estrecho por debajo de 100 columnas, mínimos por vista con aviso, plegado
  de detalles en ventanas bajas, ancho máximo en ventanas grandes e
  instantáneas de pintado a 40×12, 80×24 y 200×60.
- **Archivos, segunda vuelta (Fase 8).** Edición remota (`E`) con temporales
  privados y detección de conflictos, permisos (`p`) con rejilla rwx y octal,
  propietario por nombre, sincronización de directorios (`S`) con vista previa,
  `.magiignore`, sincronizaciones guardadas (`L`) y `magi sincronizaciones`.
- **Empaquetado y diagnóstico (Fase 9).** Paquete `magi-ssh` con `magi --version`
  que muestra versión, protocolo y commit; `magi diagnostico` comprueba sin
  escribir nada lo que MAGI necesita; `magi autocompletado <bash|zsh|fish>` y
  `magi man` generan los recursos de instalación; entrada de escritorio, icono
  propio y licencias MIT OR Apache-2.0.

### Cambiado

- El paquete se llama `magi-ssh`, pero el comando, la biblioteca, las rutas de
  datos y el socket siguen llamándose `magi`: actualizar el paquete nunca mueve
  los datos del usuario.
- Protocolo v6 con saludo estable: `Bienvenida` y `VersionIncompatible` informan
  de la versión de la aplicación y de lo que custodia el servidor; `Hola` y
  `VersionIncompatible` no cambian nunca de forma, solo se les añaden campos
  opcionales.
- Al abrir MAGI con un servidor de sesiones de otra versión: si no custodia nada
  (sesiones, transferencias, túneles, ejecuciones ni otras ventanas) se sustituye
  solo y se avisa en la barra; si custodia algo, se pregunta antes de pararlo;
  si el servidor es más nuevo que el binario, se avisa y no se para.
- Una `magi.db` con `user_version` mayor que la que conoce el binario no se abre
  ni se toca: hay que actualizar MAGI.
- Sobrescribir un fichero remoto es atómico (`posix-rename@openssh.com`, o tres
  pasos con deshacer) y conserva los permisos del original.
- Las versiones publicadas salen solo de CI a partir de un tag, con SHA-256 y
  atestaciones de procedencia de GitHub; Homebrew y AUR se actualizan en cada
  release estable.
- MAGI pasa a ser software libre bajo doble licencia MIT OR Apache-2.0.

### Corregido

- **Revisión adversarial de cierre (Fase 9).** El comando servidor parar cuenta las transferencias antes de pedir confirmación; el cliente valida el pid del rechazo y prefiere el del kernel; una magi.db futura con WAL pendiente ya no se modifica al comprobarla (y el diagnóstico no deja -shm/-wal); un servidor anterior zombi no bloquea la actualización; magi diagnostico no se cuelga con un agente SSH mudo y no cuenta el trabajo terminado; el aviso de sustitución dice la versión nueva y distingue «se paró pero el nuevo no arrancó»; TrabajoServidor rechaza campos desconocidos y build.rs no hereda el commit de un repositorio exterior.
- **Pestañas (Fase 3).** Seis fallos detectados en la validación manual, entre
  ellos la apertura colgada tras autenticar y la pantalla en blanco por parsers
  duplicados; todos con test de regresión.
- **Pool de conexiones (corrección 3b de la Fase 6).** El pool ya no desplaza una
  conexión viva, se cierra la carrera entre pestañas, SFTP y túneles, el
  `SIGTERM` de un servidor anterior se trata como parada ordenada y los
  `Hecho` que implican filas del registro se envían tras confirmar la escritura.
- **Redimensionado (Fase 7).** El tamaño mínimo entre ventanas nunca crecía y la
  pausa del visor descartaba eventos de tamaño; corregido en servidor y cliente
  con pruebas de extremo a extremo.
- **Archivos (Fase 8).** Tres defectos heredados de la Fase 4: sobrescribir un
  fichero remoto fallaba siempre en OpenSSH, cada descarga vaciaba el directorio
  de temporales y `BorrarTemporal` aceptaba rutas con `..`.
- **Ficha de host.** Una pérdida de datos de la ficha anterior a la Fase 6,
  corregida con test de regresión.

### Seguridad

- Verificación estricta de huellas contra `~/.ssh/known_hosts`, incluidas las
  entradas hasheadas; una huella cambiada bloquea la conexión y sustituirla
  exige escribir el nombre del host, con copia `known_hosts.old`.
- Nunca se guardan claves privadas ni frases: solo referencias en `magi.db`; los
  secretos en memoria usan `zeroize` y las contraseñas viven únicamente en el
  llavero del sistema.
- El servidor de sesiones no dialoga: reenvía huellas, frases y contraseñas solo
  al cliente solicitante por un socket del propio usuario (directorio 700) y las
  pantallas solo llegan a las ventanas adjuntas.
- Escrituras atómicas con permisos 600 en `magi.db`, `magi_config`,
  `known_hosts`, claves generadas y exportaciones; `~/.ssh/config` solo se toca
  para insertar el `Include`, con copia de seguridad.
- Las variables de los snippets se escapan con `shell-escape`, la salida de
  comandos remotos nunca va al registro ni al log y toda ejecución crítica,
  masiva o con verificaciones previas pasa por la deliberación MAGI.
- Los túneles escuchan en `127.0.0.1` por defecto y se rechaza cualquier canal
  `forwarded-tcpip` que no corresponda a un túnel remoto registrado.
- Sin telemetría: las únicas conexiones salientes son a los hosts del usuario.
- Releases solo desde CI con atestaciones de procedencia, `cargo deny` en la
  integración continua y auditoría de secretos y datos reales antes de hacer
  público el repositorio.

[Sin publicar]: https://github.com/4d3-org/magi/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/4d3-org/magi/releases/tag/v1.0.0
