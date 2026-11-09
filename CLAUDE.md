# CLAUDE.md — MAGI

## Contexto

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy) y macOS, con una app Android compañera (desde la Fase 11). Cliente: 4d3 (producto propio).
Desde la Fase 9 es software libre (MIT OR Apache-2.0) en `4d3-org/magi` y se distribuye como paquete `magi-ssh` (el comando es `magi`): AUR `magi-ssh-bin`, tap `4d3-org/homebrew-tap` y tarballs de GitHub Releases.
Stack: Rust stable, ratatui 0.30 + crossterm 0.29, tokio, russh 0.63 (`russh::keys`; no uses el crate `russh-keys`), tui-term 0.3 + vt100 0.16, rusqlite 0.40 (bundled, WAL), clap, toml + serde, directories, nucleo-matcher, zeroize, tracing + tracing-appender, chrono, unicode-normalization, anyhow + thiserror, serde_json, csv, ssh-key (fijado a la misma versión que trae russh), base64, nix (setsid/flock), security-framework (macOS), russh-sftp (`=3.0.0`, atado a russh 0.63), globset, filetime, shell-escape, ignore (gitignore), sha2, clap_complete, clap_mangen, age, hmac, hkdf, uuid, notify, qrcode, ironrdp (fijado con `=`), axum 0.8, des, flate2; en desarrollo, insta (instantáneas de pintado con `TestBackend`). Releases con `dist`; licencias y avisos con `cargo-deny`.
Desde la Fase 3 hay dos procesos: la TUI `magi` (cliente) y `magi --servidor` (custodia sesiones), comunicados por socket Unix con JSON por líneas.
Desde la Fase 11 el repositorio es un workspace de Cargo: `magi-ssh` (raíz: TUI, servidor, CLI), `crates/magi-nucleo` (lo compartido: almacén, sincronización, conexión, túneles, terminal y registro) y `crates/magi-movil` (cdylib con UniFFI `=0.32.2`, la puerta hacia Kotlin). La app Android vive en `android/`: Kotlin, Jetpack Compose y Material 3, `minSdk` 30, `targetSdk` 37, NDK r30 con `cargo-ndk`, JNA, `androidx.biometric`, CameraX + ZXing, DataStore y JetBrains Mono. Se publica aparte, con etiquetas `android-vX.Y.Z`.
Desde la Fase 12 abre escritorios remotos: RDP con IronRDP, VNC con un cliente RFB propio y voxl arrancado por SSH. Se ven en un visor web servido por `magi --servidor` solo en `127.0.0.1`: el cliente web de voxl adaptado, en `visor/` (TypeScript, generado a `visor/dist/` con esbuild).
Desde la Fase 13 maneja los servicios y timers de systemd, sus logs y los contenedores de Docker o Podman (con Compose) en la vista Servicios (`F9`): todo lo remoto va por el servidor con el script embebido `servicios.sh`.
Desde la Fase 14 la app Android es también el móvil de guardia: Flota con sondeo (su pantalla de inicio), Servicios, snippets con deliberación y Registro, con la lógica del núcleo; sigue siendo lectora de la sincronización.

## Documentación del proyecto

La documentación funcional vive en `docs/`, con una carpeta por fase:

- `docs/magi-maestro.md` — visión global, mapa de fases y estado del proyecto
- `docs/faseN/magi-faseN-informe.md` — especificación funcional completa de la fase
- `docs/faseN/magi-faseN-checklist.md` — alcance verificable de la fase
- `docs/faseN/magi-faseN-prompt.md` — prompt de arranque de la fase
- `docs/faseN/magi-faseN-prompt-continuacion.md` — prompt de reanudación (si la fase queda a medias)
- `docs/faseN/magi-faseN-revision-adversarial.md` — informe de la revisión adversarial de cierre, si la fase lo tiene (desde la Fase 12; lo escribes tú)
- `docs/faseN/magi-faseN-implementacion.md` — informe de implementación (lo escribes tú)
- `docs/publicar.md` — cómo se publica una versión (lo creas en la Fase 9; lo sigue el desarrollador)
- `docs/formato-sincronizacion.md` — formato del fichero de sincronización (lo creas en la Fase 10; referencia del formato: la app Android usa el mismo código del núcleo)
- `docs/publicar-android.md` — cómo se publica la app Android (lo creas en la Fase 11; lo sigue el desarrollador)
- `android/CHANGELOG.md` — cambios de la app Android (Keep a Changelog; desde la Fase 11)
- `visor/README.md` — origen (voxl), cambios y cómo se genera el visor (lo creas en la Fase 12)

El informe de fase manda sobre tu criterio: si algo te parece incorrecto o
incompleto, no lo cambies por tu cuenta — impleméntalo como está o párate y
coméntalo con el desarrollador.

## Reglas de trabajo

- Escribe siempre en castellano: respuestas, comentarios de código, mensajes de
  commit, documentación y textos de interfaz. Solo se mantienen en inglés los
  términos técnicos estándar (API, CRUD, endpoint, commit...).
- Tu effort por defecto es `ultracode`: una vez aprobado el plan que presentes,
  utiliza los workflows y todo lo disponible en ese effort.
- No añadas funcionalidades que no estén en el checklist de la fase. Si detectas
  algo necesario que falta, propónlo antes de implementarlo y déjalo anotado
  como desviación.
- Nomenclatura: tablas SQL en `MAYUSCULAS_SNAKE_CASE`, campos en
  `minusculas_snake_case`. Identificadores de Rust (módulos, funciones,
  variables) en castellano y `snake_case`; tipos en `CamelCase` en castellano.
- Sin `CHECK` sobre enumeraciones en SQLite: los valores se validan en código.
- Migraciones en `crates/magi-nucleo/src/almacen/migraciones.rs` (desde la Fase 11), numeradas y aplicadas por
  `PRAGMA user_version`; nunca modifiques una migración ya aplicada, añade otra.
- Nunca guardes claves privadas ni frases en disco ni en el log; en memoria
  usa `zeroize`.
- Toda operación de red va en una tarea tokio y comunica por el canal de
  eventos; la UI no bloquea nunca.
- Las versiones de `ratatui`, `crossterm`, `tui-term`, `vt100` y `russh` están
  fijadas en `Cargo.toml` y son compatibles entre sí: no subas una sin subir el
  conjunto y repetir las pruebas manuales de la vista Sesión.
- Comandos: `cargo run` arranca la TUI; `cargo run -- importar [ruta]`,
  `cargo run -- exportar`, `cargo run -- sincronizaciones`, `cargo run --
  diagnostico`, `cargo run -- autocompletado <shell>`, `cargo run -- man` y
  `cargo run -- sincronizar [estado]` (desde la Fase 10) son subcomandos;
  `scripts/generar-recursos.sh` regenera `assets/`.
  `TZ=UTC cargo test --workspace --locked` ejecuta todos los tests (el
  paquete `magi-ssh` expone `src/lib.rs` para los de `tests/`; las pruebas de pintado necesitan
  `TZ=UTC` y las de SFTP, `sftp-server` o la variable `MAGI_SFTP_SERVER`).
  Antes de cada commit: `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all --check` y `cargo deny check`. Si tocas workflows o
  empaquetado: `actionlint`, `dist plan --tag vX.Y.Z` y
  `desktop-file-validate assets/magi.desktop`. Si tocas scripts: `shellcheck
  scripts/*.sh crates/magi-nucleo/scripts/*.sh` (y `src/flota/sondeo.sh`
  mientras siga ahí: la Fase 14 lo mueve a `crates/magi-nucleo/scripts/`). Para pruebas aisladas usa
  `HOME` + `XDG_DATA_HOME` / `XDG_CONFIG_HOME` / `XDG_STATE_HOME`.
- Toda escritura en `~/.ssh` y en los ficheros de MAGI (`magi.db`,
  `magi_config`, `known_hosts`, `config.toml`) pasa por `ficheros.rs` (desde la Fase 11 en
  `crates/magi-nucleo/src/`; el escritorio lo reexporta):
  atómica, permisos 600 y copia con sufijo de fecha antes de tocar
  `~/.ssh/config` o `known_hosts`. No escribas en `~/.ssh` desde otro sitio.
- La UI nunca recibe bytes del remoto: el parser `vt100` vive en la tarea de
  conexión y notifica `Pantalla` coalescido a ~30 fps. Los diálogos que esperan
  al usuario (huella, frase) no tienen timeout; el de 10 s solo cubre DNS/TCP.
- Log: `~/.local/state/magi/logs/magi.log.<fecha>` (un fichero por día).
- Todo evento que deba quedar en el historial pasa por `registro::anotar`;
  nadie escribe en `REGISTRO` directamente y sus filas nunca se editan.
- Las operaciones automáticas (sondeo y las que vengan) nunca abren diálogos:
  reportan error con instrucción. Solo las conexiones interactivas aceptan
  huellas o piden frases.
- MAGI puede generar claves en `~/.ssh` (nunca sobrescribe) pero jamás borra
  ficheros de `~/.ssh` ni quita claves del agente.
- Todo lo que se ejecuta en un host remoto va en scripts embebidos con
  `include_str!` y recibe los datos del inventario como argumentos validados,
  nunca interpolados en el texto del comando.
- El bucle de UI es el único escritor de SQLite: las tareas de red emiten
  eventos y nunca abren la base de datos.
- Toda conexión viva está en `conexion::RegistroSesiones`; antes de abrir una
  conexión efímera, pide la existente ahí.
- Un secreto que no es una clave (contraseñas) solo vive en el llavero a
  través de `LlaveroSecreto` (en `magi-nucleo`; en el escritorio, secret-tool o
  Keychain; en Android, `LlaveroMovil`); en `magi.db` van referencias, nunca
  secretos, tampoco cifrados.
- Anota en el registro cuando el efecto se ha producido (fichero escrito,
  comando ejecutado), no cuando se decide.
- Servidor de sesiones (`src/servidor/`): custodia solo sesiones, pool de
  conexiones y lo que deba sobrevivir a la ventana. No dialoga nunca: reenvía
  huellas, frases y contraseñas al cliente solicitante. Escribe en SQLite solo
  `REGISTRO` (eventos de sesión y servidor), `HOSTS.ultimo_estado` /
  `ultima_conexion_en`, `DELIBERACIONES.ejecucion_resultado` y
  `SINCRONIZACIONES_DIR.ultimo_resultado`; el resto lo escribe el cliente. Ambos abren la BD con
  `busy_timeout = 5000`.
- Protocolo en `src/protocolo.rs`, `VERSION_PROTOCOLO` explícita: si cambias
  la semántica de un mensaje existente, incrementa la versión. Un mensaje
  desconocido produce `Error` y desconexión, nunca un panic. Nunca mates un
  servidor con sesiones abiertas sin orden explícita del usuario (la única
  parada sin preguntar es la de un servidor anterior sin trabajo ni otras
  ventanas, desde la Fase 9).
- Secretos por el socket (`Frase`, `Contrasena`) van en `Zeroizing` y no se
  escriben en ningún log; `Datos`/`PantallaCompleta` solo a clientes adjuntos.
- Depuración del servidor: `cargo run -- --servidor` en primer plano en una
  terminal y `cargo run` en otra; log en `~/.local/state/magi/logs/servidor.log.<fecha>`.
- En el servidor las escrituras de SQLite van por el hilo escritor (`OrdenBd`);
  no abras conexiones de escritura desde tareas async. El servidor nunca toca
  el llavero: `FuenteContrasena::Solicitante`.
- El estado cliente-servidor se sincroniza solo por difusión de listas
  completas (`Sesiones{lista}`); no añadas mensajes de sincronización finos.
- Sanea todo tamaño de terminal que llegue de fuera (mín. 2×1) y calcula el
  alto del PTY con `alto_pty`; `vt100` entra en pánico con un 0.
- SFTP y transferencias (`src/servidor/sftp.rs`, `transferencias.rs`): todo lo
  remoto en el servidor sobre el pool; la cola es FIFO con una transferencia
  en curso por host; nunca preguntes desde el servidor: el cliente decide
  conflictos y avisos antes de `Transferir` y tú aplicas la política. Ninguna
  ruta pasa por un shell.
- Toda espera de red en el servidor lleva plazo (10 s DNS/TCP/saludos de
  subsistema, 60 s por bloque de transferencia); solo los diálogos con el
  usuario esperan 5 min. `request_subsystem` devuelve `Ok` aunque el host
  diga que no: envuélvelo en el plazo. El plazo de una operación cubre la red
  y el canal, nunca el tiempo de sus diálogos (desde la Fase 13).
- Un diálogo lleva fijados los datos sobre los que actúa (rutas, ids) y no
  relee el estado al confirmar; descarta respuestas de peticiones que ya no
  están en vuelo. Libera cada recurso del pool por identidad y en todas las
  ramas de error.
- Antes de cerrar una fase, pasa una revisión adversarial sobre el código
  nuevo (servidor y cliente por separado) buscando pérdida de datos, fugas de
  recursos, esperas sin plazo y estado leído a destiempo; corrige lo que
  encuentres y añade test de regresión a lo que sea pérdida de datos. Anota
  el resultado en el informe de implementación.
- Túneles (`src/servidor/tuneles.rs`, con la mecánica en
  `crates/magi-nucleo/src/tuneles/` desde la Fase 11): el CRUD de `TUNELES` lo hace el cliente;
  el servidor solo lee la tabla y ejecuta. Un túnel activo cuenta como canal
  del pool pero no como pestaña ni SFTP para el ciclo automático. Todo lo que
  escuche en local va en `127.0.0.1` salvo aviso explícito; los canales
  `forwarded-tcpip` con clave no registrada se rechazan.
- Valida toda fila que el servidor lea de SQLite antes de ejecutarla (la
  escribe otro proceso). Nunca emitas en `magi_config` una línea que `ssh` no
  sepa leer: sustitúyela por un comentario y valida el fichero con el `ssh`
  real en los tests. Los `peticion_id` de subsistemas distintos no comparten
  rango. Difunde un cambio de estado antes de su efecto lento y retira
  listeners y reenvíos en todas las ramas de salida, con plazo.
- Conexiones (desde la corrección 3b de la Fase 6): `conexion_para_canal` con
  cerrojo por host es la única forma de obtener una conexión para pestañas,
  SFTP, túneles, `Ejecutar` y ejecuciones de snippets; `usa_pool(host, uso)`
  decide (todo usa el pool salvo las pestañas sin `multiplexar`). Se toma una
  `ConexionTomada` y se suelta por identidad. `Pool::guardar` nunca desplaza
  una entrada viva ni en gracia. No abras conexiones por otra vía.
- `peticion_id` por rangos: Archivos (panel) desde 0, túneles 2^40,
  operaciones de Archivos en la `App` (edición, permisos, sincronización) 2^42,
  ejecuciones 2^44, servicios 2^45 (desde la Fase 13), escritorios 2^46 y
  diálogos que el servidor inicia desde el visor 2^47 (desde la Fase 12), peticiones esperables del cliente 2^48. El contador del
  panel se reinicia al cambiar de host: lo que deba sobrevivir a eso vive en la
  `App` y usa su propio rango. Un `Hecho` que implica filas de
  `REGISTRO` se envía después de `OrdenBd::Barrera`.
- Las preguntas del servidor (huella, frase, contraseña) se apilan sobre el
  diálogo abierto, nunca lo pisan. Una operación masiva pide la contraseña con
  `PideLlavero`, que nunca abre diálogo. El usuario de auditoría sale del uid.
- Snippets (`src/servidor/ejecuciones.rs`): el cliente resuelve destinos,
  variables (escapadas con `shell-escape`) y deliberación; el servidor recibe
  el comando ya resuelto, lo ejecuta sin diálogos y nunca lee `SNIPPETS`. La
  salida de comandos remotos jamás va a `REGISTRO` ni al log.
- Deliberación (`src/deliberacion/`): comprobaciones en paralelo con plazo duro
  de 2 s; `Ctrl+K` es la única tecla que ejecuta desde el diálogo MAGI (`↵`
  no). El servidor escribe `DELIBERACIONES.ejecucion_resultado` y las
  anotaciones `deliberacion_*` al terminar la ejecución, por su hilo escritor.
- Tamaño de terminal (desde la Fase 7): un solo punto (`Geometria` en `app.rs`)
  aplica los cambios de tamaño, agrupados 50 ms, y es el único que envía
  `Redimensionar`. Ninguna vista guarda tamaños entre pintados: todo se deriva
  del área en cada pintado. Toda vista declara su mínimo y su modo estrecho en
  `ui/disposicion.rs` y tiene instantáneas `insta` a 40×12, 80×24 y 200×60 en
  `tests/redimensionado.rs`; si añades una vista, añade sus instantáneas.
  Tras un cambio visual deliberado: `INSTA_UPDATE=always cargo test --test
  redimensionado` y revisa con `git diff tests/snapshots` o `cargo insta
  review`. Las pruebas de pintado corren con `TZ=UTC`.
- Pruebas de interfaz con `App::de_prueba` (sin hilos, red ni servidor) y el
  bucle real `ciclo`/`paso` sobre `TestBackend`; `Cliente::de_prueba` captura
  lo enviado. Esa superficie es `#[doc(hidden)] pub`: no la uses fuera de tests.
- El hilo de teclas (`src/app/teclado.rs`) nunca lee del tty en pausa; pausa
  siempre con acuse antes de ceder la terminal (visor, paginador).
- `Redimensionada` y cualquier cambio que afecte a cómo se interpretan los
  `Datos` se aplican en la tarea de lectura del cliente, en orden con ellos.
- Un solo degradador ASCII: `disposicion::texto_ascii`.
- Archivos (desde la Fase 8): el temporal de una edición remota nunca se borra
  hasta que la subida termine, el usuario confirme «sin cambios» o descarte; ante
  cualquier error se conserva y se dice su ruta. Sobrescribir reaplica los
  permisos del original tras el `rename`. En una sincronización, el cliente
  planifica y fija las rutas; el servidor solo borra (`borrar_al_terminar`) si la
  transferencia terminó sin errores y nunca borra rutas excluidas, bajo una ruta
  omitida ni bajo un enlace.
- Sobrescribir en remoto pasa siempre por el renombrador de `servidor/sftp.rs`
  (`posix-rename@openssh.com` por el canal SFTP raw, o tres pasos con deshacer):
  nunca uses el `rename` de SFTP v3 sobre un destino que puede existir, en
  OpenSSH da EEXIST. Cada canal SFTP ocupa dos canales de sesión.
- Temporales: nombre único con `create_new` y 600; los del visor en `tmp`, los de
  edición en `ediciones`, que no vacía nadie (ni arranque ni apagado). Nunca
  vacíes uno de esos directorios en bloque; `BorrarTemporal` solo admite ficheros
  cuyo padre sea exactamente uno de los dos.
- El servidor valida toda ruta de una transferencia al encolar: absoluta, sin
  `.` ni `..`, colgando estrictamente de la raíz. No te fíes de `starts_with`
  sobre rutas sin normalizar.
- Una deliberación autoriza una sola acción: la validación es común a
  ejecuciones y transferencias y «ya usada» vale en los dos sentidos. Para una
  acción nueva, añade una variante a `PlanDeliberado`.
- Tests aislados: `directories` resuelve el home por uid, así que fija
  `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` y `XDG_RUNTIME_DIR`,
  no solo `HOME`. La ruta de `XDG_RUNTIME_DIR` debe ser corta: el socket no
  puede pasar de ~108 bytes o el servidor no arranca. `servidor::arrancar` pone
  la umask del proceso en 077 un instante: en pruebas que arrancan el servidor
  en el mismo proceso, fija los modos de los ficheros de prueba de forma
  explícita.
- Antes de lanzar pistas en paralelo, fija un contrato compilable (tipos,
  mensajes, módulos vacíos y enganches) en un commit provisional.
- La revisión adversarial de cierre no es opcional: si el desarrollador pide
  saltarla, anótalo como pendiente con lo que queda sin revisar.
- Distribución (desde la Fase 9): el paquete es `magi-ssh`, pero el comando, la
  biblioteca, las rutas de datos y el socket se llaman `magi`. Nunca derives una
  ruta de `CARGO_PKG_NAME`.
- `Hola` y `VersionIncompatible` son contrato estable: nunca quites ni renombres
  sus campos, solo añade opcionales. Si el servidor empieza a custodiar un tipo
  de trabajo nuevo, súmalo a `VersionIncompatible.trabajo`: un servidor
  anterior solo se sustituye sin preguntar si no custodia nada.
- Versiones (política en `docs/publicar.md`): MAJOR si cambia algo incompatible
  para el usuario (config, CLI, formatos exportados); MINOR para funcionalidades,
  migraciones y cambios de protocolo; PATCH para correcciones. Todo cambio
  visible añade su línea en la sección «Sin publicar» de `CHANGELOG.md` en el
  mismo PR. Si cambias la CLI, regenera `assets/` con
  `scripts/generar-recursos.sh` (un test lo exige).
- Una `magi.db` con `user_version` mayor que la que conoce el binario no se abre
  ni se toca: las migraciones solo van hacia delante.
- Los binarios publicados salen solo de CI a partir de un tag. Tú no etiquetas
  versiones, no cambias la visibilidad del repositorio, no creas secretos ni
  cuentas y no subes binarios: tú lo preparas y lo hace el desarrollador.
- `release.yml` está retocado a mano (`allow-dirty = ["ci"]`): no lo regeneres
  con `dist init` sin reaplicar los ajustes que explica `docs/publicar.md`, ni
  quites `allow-dirty`. Cada workflow recibe solo su secreto (nunca
  `secrets: inherit`); las herramientas que se descargan van fijadas por
  SHA-256. `dist` no pasa `--locked`: el `cargo metadata --locked` del plan es
  el que lo garantiza.
- La excepción de `cargo deny` RUSTSEC-2023-0071 (`rsa`) tiene fecha límite
  comprobada en CI: si el job `deny` falla por la fecha, revisa el aviso antes
  de prorrogarla. La fecha de la página man es la de la versión (la constante
  `FECHA_DE_VERSION` de `src/lib.rs`): cámbiala al cambiar `version`, regenera
  `assets/` y compruébalo con `cargo test -p magi-ssh --test recursos`.
- Señales a otros procesos: el pid sale de `peer_cred()` y solo si falta, del
  mensaje, siempre validado (1 < pid ≤ i32::MAX y distinto del propio). Nunca
  señales a un servidor más nuevo, tampoco desde `magi servidor parar`.
- Arranque del servidor, en este orden: manejador de `SIGTERM`, lock, abrir y
  validar `magi.db`, socket. Para dar por terminado un proceso mira sus recursos
  (lock libre, socket que no atiende), no `kill(pid, 0)`, y no borres el socket
  mientras esperas.
- Para leer `magi.db` sin modificarla (comprobar la versión, diagnosticar) no
  abras el original en lectura/escritura ni en WAL: mira la cabecera o lee una
  copia temporal con sus laterales. Toda orden externa lleva plazo.
- `TrabajoServidor` rechaza campos desconocidos: si el servidor empieza a
  custodiar un tipo de trabajo nuevo, los clientes anteriores preguntarán en vez
  de sustituirlo (es lo correcto).
- MAGI no comprueba actualizaciones ni abre conexiones salientes que no sean a
  los hosts del usuario (sin telemetría).
- Sincronización del inventario (desde la Fase 10; lo común en
  `crates/magi-nucleo/src/sincronizacion/` y, en `src/sincronizacion/`, la pasada
  escritora, los equipos y el QR del escritorio):
  toda tabla que viaja lleva `uuid`, `version` y `vector`. Las escrituras del
  almacén de campos que viajan suben la versión con el reloj híbrido
  (`almacen/versiones.rs`); los campos propios del equipo no. Un `SET NULL`
  de clave ajena cuenta como edición de los hosts afectados. Borrar deja
  lápida, salvo un borrado aplazado (sus referencias se mantienen y el
  aplazamiento se propaga al padre). Si añades un campo o una tabla que deba
  viajar, actualiza `sincronizacion/elementos.rs` y
  `docs/formato-sincronizacion.md`, y sube `formato` si cambia el significado.
  `uuid`, `version` y `vector` admiten NULL en el esquema, pero nunca
  dejes una fila sin rellenar.
- La importación escribe las versiones tal cual (nunca por el camino que sube
  versión), en una transacción, bajo el cerrojo y con copia previa de
  `magi.db`. Nada se importa sin firma válida. Cada equipo escribe solo su
  fichero; el de otro solo se borra al quitar ese equipo.
- La clave de sincronización vive solo en el llavero (`llavero.rs`): nunca en
  logs, `REGISTRO`, `magi.db` ni la carpeta. Contraseñas y claves privadas no
  viajan. Una importación nunca cierra trabajo vivo (aplaza el borrado) ni
  sustituye una huella sin que el usuario lo acepte.
- La firma se deriva del texto canónico de la identidad (en mayúsculas) y se
  comprueba antes de mirar `formato` (`docs/formato-sincronizacion.md` §3). La
  clave anterior solo lee: nunca exportes con ella. Todo cambio de clave olvida
  el hash del fichero propio para que se reescriba con la nueva. Crear, unir,
  rotar, quitar, desactivar y reunir toman el mismo cerrojo que la pasada.
- La carpeta compartida es entrada no fiable: no sigas enlaces simbólicos
  (`O_NOFOLLOW`, temporales con `create_new` y nombre único), acepta solo
  nombres con el uuid canónico en minúsculas y respeta el tamaño máximo.
- En pruebas, el llavero se inyecta (`LlaveroSecreto`, `LlaveroEnMemoria`);
  nunca uses el llavero real. Tests de la fase: `TZ=UTC cargo test --test
  sincronizacion_equipos --test sincronizacion_elementos --test
  sincronizacion_fusion --test sincronizacion_aplicacion --test
  sincronizacion_pasada --test sincronizacion_pasada_e2e --test
  sincronizacion_huellas --test sincronizacion_ui`.
- `age` está fijado en 0.11.5: la 0.12 no compila con el `ml-kem` que trae
  russh 0.63. No lo subas sin subir el conjunto russh, y al actualizar `age`
  revisa las excepciones RUSTSEC-2026-0173 y RUSTSEC-2026-0374 de
  `deny.toml`.
- Ejemplos, tests, instantáneas y mockups usan direcciones de documentación
  (`192.0.2.x`), dominios `ejemplo.org`, el usuario `usuario` y
  `/home/usuario`; nunca datos reales.
- Workspace (desde la Fase 11): lo que deban hacer igual el escritorio y el
  móvil vive en `magi-nucleo`. No dupliques en Kotlin la fusión, el formato, el
  cifrado, SSH, los túneles, el parser ni la codificación de teclas. Buena parte
  de las rutas `src/…` de este fichero (almacén, sincronización, conexión,
  túneles, `llavero.rs`, `registro.rs`, `ficheros.rs`) pasan a
  `crates/magi-nucleo/src/…`: las reglas siguen igual. Al mover un módulo,
  actualiza aquí su ruta y anótalo en el informe de implementación.
- `magi-nucleo` no lee el entorno ni calcula rutas (ni `directories`, ni
  `HOME`, ni `XDG_*`): recibe rutas, nombre y plataforma del equipo y servicios
  (`CarpetaSincronizacion`, `LlaveroSecreto`, `Firmante`, `TrabajoVivo`). No
  depende de ratatui, crossterm, tui-term, notify ni de nada de Android. Lo que
  es solo del escritorio (`magi_config`, `~/.ssh/config`, notify, copias de
  `magi.db`, servidor y protocolo, llavero del sistema, QR) se queda en
  `magi-ssh`.
- `magi-movil` es la única puerta de UniFFI; el escritorio no compila UniFFI.
  UniFFI está fijado en `=0.32.2` en el crate y en el generador de bindings:
  súbelos juntos.
- La app Android es lectora de la sincronización: llama siempre a la pasada en
  modo lector, nunca escribe en la carpeta ni exporta, y su inventario es de
  solo lectura. Solo escribe campos propios (`plegado`, `ultimo_estado`,
  `ultima_conexion_en` y, desde la Fase 14, `SNIPPETS.usado_veces` y
  `ultimo_uso_en`), huellas en su `known_hosts`, REGISTRO, SONDEOS,
  DELIBERACIONES y conflictos de huella y de borrado. Nunca crea ni edita
  snippets, comprobaciones propias ni verificaciones.
- Secretos en Android solo por `LlaveroMovil` (AES-GCM con una clave del
  Keystore). La clave del móvil nunca sale del Keystore: el núcleo firma a
  través de `Firmante`. Nada secreto en logcat, DataStore, `magi.db` ni
  ficheros en claro.
- Las claves del Keystore se crean sin `setUnlockedDeviceRequired`: con
  desbloqueo facial, Android las rechaza aunque el teléfono esté desbloqueado.
  Lo que impide usarlas sin permiso es el bloqueo de MAGI. Si cambias cómo se
  crea una clave del Keystore, migra las instalaciones existentes y, si cambia
  la clave del móvil, avisa en la interfaz de que hay que reinstalarla.
- En el móvil, los túneles locales y dinámicos escuchan siempre en
  `127.0.0.1` y no se intenta escuchar por debajo del puerto 1024 (se ofrece un
  puerto libre).
- Un túnel del móvil solo está `activo` si la conexión de su host está viva y
  su escucha acepta; si no, pasa a `caido` con el motivo. La fila enseña el
  último error y los contadores de conexiones abiertas y aceptadas. Todo cambio
  en túneles o conexiones del móvil pasa la prueba de extremo a extremo (Rust y
  emulador, sin red externa) antes de marcarse en el checklist.
- Toda conexión del móvil tiene su vigilante en el gestor de `magi-movil`,
  también cuando solo la usan túneles: si el host la cierra, todo lo que la usa
  pasa a caído con el motivo. Desde la Fase 14 hay una sola conexión por host
  para todo (sesiones, túneles, sondeo, servicios, logs, ejecuciones y el SFTP
  de BALTHASAR-2), con el mismo vigilante y 60 s de gracia sin usos; cada uso
  se suelta por identidad.
- Los servidores de pruebas (SSH, eco, canales de control) escuchan en
  `127.0.0.1`, nunca en `0.0.0.0`: el emulador llega al anfitrión por
  `10.0.2.2`, que es su loopback. Su SSH solo reenvía a los servicios de la
  prueba.
- Sin Google Play Services, Firebase, ML Kit ni analítica en la app; el QR se
  lee con ZXing. Dependencias de Gradle fijadas en `gradle/libs.versions.toml` y
  verificadas por SHA-256: al añadir o subir una, ejecuta `./gradlew
  --write-verification-metadata sha256 help` y revisa el diff.
- Kotlin: nombres en castellano (clases en `CamelCase`; funciones y variables
  en `camelCase`, como manda Kotlin); textos de interfaz en
  `res/values/strings.xml`, en castellano.
- Comandos de Android (en `android/`): `./gradlew assembleDebug` (compila
  también el núcleo con `cargo-ndk` y genera los bindings), `./gradlew lint
  testDebugUnitTest` (pruebas en la JVM con el núcleo compilado para el
  anfitrión) y `./gradlew connectedDebugAndroidTest` (emulador `x86_64`,
  API 30). Requisitos: SDK de Android, NDK r30 (`30.0.16248370`), `cargo-ndk` y
  los objetivos `aarch64-linux-android` y `x86_64-linux-android` de rustup.
- Releases de Android: solo `android-release.yml`, con la etiqueta
  `android-vX.Y.Z` que coincide con `android/version.properties`; el
  `versionCode` sale de la versión (informe de la Fase 11, §7.10); nunca se
  marcan como «Latest» y `release.yml` no debe dispararse con `android-v*`. El
  keystore y sus secretos (`ANDROID_KEYSTORE_BASE64`, contraseñas y alias) los
  crea el desarrollador, no tú.
- Pruebas del móvil: el Keystore, SAF, la cámara y la biometría se sustituyen
  por dobles en la JVM; en el emulador corren la prueba de humo, la de extremo
  a extremo de los túneles (desde la corrección 11b) y las de Flota, Servicios
  y Snippets (desde la Fase 14). Necesitan el servidor de pruebas del
  anfitrión, que desde la Fase 14 arranca también el job `humo` de CI. Lo que solo se puede
  comprobar en un móvil real va al guion de validación del informe de
  implementación.
- La codificación de teclas es una sola tabla, en
  `crates/magi-nucleo/src/terminal/teclas.rs`: el escritorio (`src/teclas.rs`)
  solo traduce a ella los eventos de crossterm y la app Android la usa igual.
- AGP 9 trae Kotlin integrado: no apliques el plugin
  `org.jetbrains.kotlin.android` (AGP 9 lo rechaza). Hace falta JDK 17 o más (en
  CI, 21).
- Los bindings Kotlin se generan en cada compilación en
  `app/build/generated/uniffi/…` y no se versionan. La supresión `NewApi` de
  `java.lang.ref.Cleaner` se limita a ese fichero generado.
- La release solo se firma si existen las variables `MAGI_KEYSTORE_*`, que pone
  `android-release.yml` a partir de sus secretos. Una release compilada en local
  sale sin firmar y nunca se publica.
- La fixture `.age` de la prueba de humo se regenera con `cargo test -p
  magi-nucleo --test generar_fixture_android -- --ignored` (también la del
  `formato` 3, en `android/app/src/androidTest/assets/`), y la del túnel, con
  `cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored`.
- Pruebas de extremo a extremo del móvil: en Rust, `cargo test -p magi-movil
  --test tunel_local_e2e` y, desde la Fase 14, `--test flota_e2e --test
  servicios_movil_e2e --test snippets_movil_e2e --test deliberacion_movil_e2e`;
  en el emulador, arranca antes `cargo run -p magi-movil --example
  servidor_pruebas` (se llamaba `servidor_pruebas_tunel` hasta la Fase 14) y
  ejecuta `./gradlew connectedDebugAndroidTest` (sin ese servidor, esas pruebas
  se omiten solas).
- Antes de usar Gradle o cargo-ndk, exporta `ANDROID_HOME`, `ANDROID_SDK_ROOT`
  y `ANDROID_NDK_HOME` (NDK r30). Si lo haces con un script de entorno, que viva
  fuera de `/tmp` (se vacía al reiniciar). En documentos del repositorio, nunca
  escribas rutas con el usuario del desarrollador: usa `$HOME`.
- Si `cargo test` falla con errores raros de SQLite o de temporales porque
  `/tmp` (tmpfs) se ha llenado, usa `TMPDIR="$HOME/.cache/tmp"`; en máquinas muy
  justas, añade `-- --test-threads=1`. En CI no hace falta ninguna de las dos
  cosas.
- `.github/actionlint.yaml` ignora tres avisos de ShellCheck solo en el
  `release.yml` generado por dist; no añadas exclusiones para los workflows
  propios. `cargo deny check` va sin `--workspace`: la versión 0.20.2 no lo
  admite y ya analiza todo el workspace.
- Escritorios (desde la Fase 12): un host tiene `tipo` (`ssh`, `rdp`, `vnc` o
  `voxl`), fijo al crearlo. Solo los `ssh` se exportan a `magi_config` y pueden
  ser destino de terminal, SFTP, túneles, snippets o salto; «a través de»
  (`salto_host_id`) apunta siempre a un host `ssh`. Lo común (cliente RFB,
  adaptador de IronRDP, puente, servidor del visor, voxl) vive en
  `crates/magi-nucleo/src/escritorio/`; la custodia, en
  `src/servidor/escritorios.rs`.
- Las sesiones de escritorio las custodia el servidor, como las de terminal:
  lista completa `Escritorios{lista}`, `TrabajoServidor.escritorios`, gracia
  `[escritorio] gracia_seg` sin visor y un visor por sesión. El canal va por
  `conexion_para_canal` (uso `Escritorio`) y `direct-tcpip`, y se suelta por
  identidad.
- El servidor del visor escucha solo en `127.0.0.1` y solo mientras hay
  sesiones de escritorio. Se entra con un enlace de un solo uso (60 s) que se
  canjea por una clave por sesión; `Host` se comprueba siempre y `Origin` es
  obligatorio en `/api/*`; CSP estricta y nada cargado de fuera. Nunca
  `0.0.0.0`, ni cookies, ni tokens fijos en la URL.
- Sin dependencias GPL ni recompresión de vídeo: nada de x264, FFmpeg ni
  OpenH264. Los cambios van al visor como rectángulos en bruto y el AVC420 de
  Windows, tal cual. `cargo deny check` lo vigila.
- IronRDP 0.17 e `ironrdp-egfx` `=0.3.0` están fijados con `=` (TLS con rustls
  y la cripto de russh, sin OpenSSL). `ironrdp-connector` e `ironrdp-acceptor`
  van vendorizados en `vendor/` con `sspi` 0.23.1 y `picky` 7.0.0-rc.27 para
  compartir `ecdsa` con russh 0.63: no toques su código salvo las dos líneas de
  adaptación anotadas. Al subir russh o IronRDP, mira si sus versiones de
  `sspi`, `picky` y `ecdsa` ya coinciden y quita el vendor; mientras exista,
  revisa a mano las correcciones de seguridad de IronRDP, `sspi` y `picky`
  (RustSec no avisa de una copia local). `deny.toml` lleva la excepción
  RUSTSEC-2026-0441 (`pin-utils` sin mantener, transitiva de `sspi` en el
  `ironrdp-connector` vendorizado, desde la Fase 13): revísala al subir IronRDP
  o `sspi`. No subas nada de esto sin repetir las pruebas de RDP y el paso 1 del
  guion de validación de la Fase 12.
- El `ActiveStage` de IronRDP no es `Send`: cada sesión de escritorio corre en
  un hilo con runtime propio (`lanzar_en_hilo_con`) y el resto del servidor
  solo ve canales. El AVC420 sale por el `DvcProcessor` propio de
  `escritorio/rdp/egfx.rs`, sin decodificar.
- Lo que llega de un servidor VNC o RDP y lo que llega del visor es entrada no
  fiable: tamaños acotados (escritorio ≤ 8192×8192, cursor ≤ 256×256, texto
  ≤ 1 MB, ZRLE acotado) y nunca pánico. La decodificación va en un hilo por
  sesión, fuera del runtime de red. Ni píxeles, ni teclas, ni portapapeles, ni
  contraseñas van al log ni a REGISTRO.
- Certificados RDP: la misma política que las huellas SSH (nuevo, con diálogo;
  cambiado, en rojo y escribiendo el nombre del host), comprobados antes de
  mandar credenciales. Los guarda el cliente que los acepta, en
  `HOSTS.huella_escritorio`, que no viaja. El servidor pide certificado y
  credenciales al solicitante y nunca toca el llavero.
- voxl se arranca solo con `crates/magi-nucleo/scripts/escritorio_voxl.sh`
  (embebido con `include_str!`): argumentos validados en Rust y en el script, contraseña
  aleatoria por la entrada estándar (nunca en la línea de órdenes) y voxl en
  `127.0.0.1` del host; el fin de la entrada estándar lo para. MAGI no instala
  voxl.
- El navegador lo abre el cliente, nunca el servidor, y sin shell
  (`[escritorio] navegador`).
- El visor vive en `visor/` (copiado de voxl 0.3.3, MIT: conserva
  `visor/LICENSE-voxl` y apunta el origen y los cambios en `visor/README.md`).
  `visor/dist/` se genera con `scripts/generar-visor.sh` y se versiona: tras
  tocar `visor/src`, ejecuta `npm --prefix visor ci`, `npm --prefix visor run
  check` y `scripts/generar-visor.sh`. Compilar MAGI no necesita Node. Las
  dependencias de npm son solo de desarrollo y van fijadas con integridad.
- La sincronización escribe `formato` 2 solo si hay hosts que no son `ssh`;
  viajan `tipo`, `escritorio_dominio` y `escritorio_opciones`. Desde la Fase 13
  escribe `formato` 3 solo si hay alguna comprobación de servicio propia
  (elemento `comprobacion_servicio`). Mantén `docs/formato-sincronizacion.md` al
  día.
- Las pruebas de escritorio no usan red externa (servidor RFB de prueba,
  `ironrdp-server` y el servidor SSH de prueba con un `voxl` falso):
  `cargo test -p magi-nucleo --test escritorio_rfb --test escritorio_visor
  --test escritorio_rdp --test escritorio_voxl --test escritorio_e2e
  --test sincronizacion_escritorios` y `TZ=UTC cargo test -p magi-ssh --test
  escritorio_servidor --test escritorio_ui --test redimensionado`.
- Servicios (desde la Fase 13): lo común (tipos, órdenes, lectura de salidas,
  acciones permitidas, validadores y límites) vive en
  `crates/magi-nucleo/src/servicios/`; la custodia, en `src/servidor/servicios.rs`,
  y la vista (`F9`), en `src/ui/servicios.rs`. Solo los hosts `ssh` tienen
  servicios.
- Todo lo remoto de servicios va con `crates/magi-nucleo/scripts/servicios.sh`
  (`include_str!`), con argumentos validados en Rust y en el script, por un canal
  del pool (`UsoCanal::Servicios`). Las propiedades se leen con `systemctl show`
  (clave=valor) y los JSON de Docker, Podman y `journalctl`; de `list-units` solo
  se toma el nombre (primera columna, con `--plain --no-legend`). En el host solo
  usa `sh` POSIX, coreutils (`timeout`, `mktemp`), `mkfifo` y `awk`; si falta
  algo, el script sale con su `ERROR` y su código (70: sin temporales).
- sudo orden a orden, nunca `sudo sh`, con `timeout` por fuera de sudo y
  `LC_ALL=C` para leer sus mensajes. La única excepción es la comprobación
  propia, la única orden libre: va con `sh -c` bajo los privilegios de la acción.
  Con Podman nunca se usa sudo, y un usuario del grupo `docker` gestiona Docker
  sin sudo. La contraseña la pide el cliente, la comprueba con `ComprobarSudo`
  antes de confirmar o deliberar y la guarda solo en memoria (`Zeroizing`,
  `[servicios] sudo_memoria_min`; solo las acciones cuentan como uso); viaja
  dentro de cada petición y llega al host por la primera línea de la entrada
  estándar. Nunca va a disco, al llavero, al log ni a REGISTRO.
- Lo que deja algo parado (parar, reiniciar, desactivar, y parar o reiniciar un
  contenedor o un proyecto) pasa por la deliberación (`PlanDeliberado::Servicio`)
  y el servidor lo recomprueba; MELCHIOR-1 no cuenta el estado del propio
  objetivo. Lo demás pide confirmación: ninguna acción con una tecla sola, y las
  teclas que cambian algo en el host van en mayúsculas.
- La comprobación de configuración (Nginx, Caddy, Apache, HAProxy, Compose o la
  propia de COMPROBACIONES_SERVICIO) va dentro de la acción, en el mismo canal y
  justo antes; si falla, da 127 o pasa su plazo (20 s), no se ejecuta nada.
- Logs: el servidor manda lotes cada 100 ms (hasta 500 líneas y 2 MiB
  codificados), descarta las más viejas con recuento si la cola de una
  suscripción pasa de 4 MiB y corta el seguimiento al cerrarse el panel (fin de
  la entrada estándar del script, que manda TERM al proceso real y no a su
  envoltorio). El cliente los guarda solo en un anillo en memoria (10 000
  líneas y 8 MiB). Ni logs, ni unidades, ni `inspect`, ni la salida de las
  comprobaciones van al log o a REGISTRO.
- Lo que llega del host es entrada no fiable, con los límites del informe de la
  Fase 13 (§8); nunca pánico.
- Las pestañas con orden (shell de un contenedor, seguimiento de logs) las arma el
  servidor desde `OrdenPestana`; el cliente nunca manda texto para ejecutar.
- Las acciones de servicio en curso son trabajo del servidor
  (`TrabajoServidor.acciones_servicio`, `AccionesServicio{lista}`); los logs no.
  Una sola acción a la vez por objetivo.
- Lo que el servidor guarda por petición (acciones, suscripciones de log,
  escritorios) se indexa por `(cliente_id, peticion_id)`: dos ventanas pueden
  repetir ids. Los mensajes de cada ventana se atienden en orden y, antes de
  abrir algo para un cliente, el servidor comprueba que sigue conectado.
- Lo que mantiene un canal abierto (túneles, escritorios, seguimientos de logs)
  conserva su `ConexionTomada` mientras dura y la suelta al terminar, en todas
  las ramas: si la suelta antes, el pool recicla la conexión al acabar su gracia.
- Ningún mensaje del protocolo pasa del tamaño máximo del códec: corta los lotes
  por tamaño codificado, no solo por número; y todo lo que se acumula en memoria
  (anillos, colas) lleva tope de bytes.
- El sondeo sigue sin sudo y sin diálogos; sus fallos (`SONDEOS.fallos_json`) no
  cambian el estado del host: solo los vigilados lo hacen.
- Las pruebas de servicios no usan red externa (servidor SSH de prueba con
  `systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos, y
  salidas grabadas en `tests/fijos/servicios/`, que la Fase 14 mueve a
  `crates/magi-nucleo/tests/fijos/` para compartirlas con el móvil): `cargo test -p magi-nucleo
  --test servicios_lectura --test servicios_script --test
  sincronizacion_comprobaciones` y `TZ=UTC cargo test -p magi-ssh --test
  servicios_servidor --test servicios_ui --test servicios_e2e --test
  redimensionado` (la prueba de logs que pasa la gracia del pool tarda ~40 s a
  propósito).
- Móvil de guardia (desde la Fase 14): `magi-movil` hace en el móvil lo que el
  servidor hace en el escritorio para servicios, ejecuciones de snippets, logs y
  deliberaciones, con la lógica del núcleo (`flota/`, `servicios/`, `snippets/` y
  `deliberacion/`). Kotlin no decide qué acción se permite, qué exige deliberación
  ni cómo se lee lo que llega de un host, y `magi-movil` recomprueba la acción y
  la deliberación antes de ejecutar.
- El sondeo vive en `crates/magi-nucleo/src/flota/` y `scripts/sondeo.sh`, y las
  comprobaciones de la deliberación en `crates/magi-nucleo/src/deliberacion/`, con
  proveedores por plataforma (desde la Fase 14). En `src/flota/` y
  `src/deliberacion/` el escritorio solo guarda su orquestación, su diálogo y sus
  proveedores. Una comprobación activa que no puede hacerse en una plataforma
  rechaza («solo en el escritorio»); nunca se omite.
- Lo automático del móvil (sondeo, refrescos, consumo, comprobaciones de la
  deliberación y ejecuciones) abre conexiones sin diálogos: una huella sin aceptar
  o la falta de credenciales dan un error con instrucción. Solo lo que pide el
  usuario (conectar, activar un túnel, abrir los servicios de un host, una acción,
  los logs) puede dialogar.
- En el móvil, el sondeo, los refrescos de Servicios y los logs solo corren con
  su pantalla a la vista (ciclo de vida de Android), nunca con la app en segundo
  plano. Las acciones y las ejecuciones sí siguen y cuentan para el servicio en
  primer plano.
- En el móvil, lo deliberado solo se ejecuta manteniendo pulsado el botón 1 s (el
  `Ctrl+K` del móvil); ningún toque simple ejecuta desde el diálogo MAGI. Lo que no
  delibera pide una confirmación que nombra la acción, el objetivo y el host.
- En el móvil, la contraseña de sudo vive solo en `magi-movil` (`Zeroizing`), por
  host, hasta `sudo_memoria_min` sin uso, hasta que se bloquea MAGI o hasta que el
  host la rechaza. Nunca va a `LlaveroMovil`, DataStore, logcat ni REGISTRO, y su
  diálogo lleva `FLAG_SECURE`.
- El anillo de logs del móvil vive en `magi-movil` (10 000 líneas y 8 MiB) y la app
  pide ventanas filtradas de hasta 500 líneas: por la FFI no cruzan listas sin
  acotar. La salida de un snippet y los logs nunca van a logcat, a REGISTRO ni a
  disco.
- Los ajustes del móvil que en el escritorio están en `config.toml` (umbrales y
  refresco de Flota; refresco, espera, motores y sudo de Servicios; horas del
  backup y minutos de salud) viven en DataStore, con los valores por defecto y los
  rangos del escritorio, y llegan al núcleo con `aplicar_ajustes`.
  `DELIBERACIONES.usuario` en el móvil es «móvil (<nombre del equipo>)».

## Al terminar una fase (o al pausar la sesión)

1. Busca el archivo markdown que tenga `checklist` en el nombre correspondiente
   a esa fase y marca `[x]` todo lo que se haya realizado. Actualiza los
   contadores de progreso del final del archivo. No reescribas el texto de las
   funcionalidades: es la clave de trazabilidad con el resto de documentos.
2. Genera o actualiza `magi-fase[N]-implementacion.md` con exactamente
   esta estructura:
   - Resumen de lo implementado
   - Desviaciones respecto a la especificación (qué y por qué)
   - Estructura de archivos creada/modificada
   - Decisiones técnicas tomadas durante el desarrollo
   - Funcionalidades del checklist completadas (copiando su texto exacto)
   - Pendientes y bloqueos
   - Ejecución y pruebas (cómo arrancar, migrar y testear)

   Es un documento vivo: actualízalo de forma incremental en cada sesión, nunca
   lo regeneres desde cero. Sé honesto en las desviaciones y en los pendientes;
   ese informe es lo único que el analista funcional verá de lo que realmente
   pasó, y con él se actualiza la especificación.
3. Recuérdale al desarrollador que debe entregar el informe de implementación al
   analista funcional antes de especificar la siguiente fase.
4. Invita al desarrollador a hacer PR. Si te dice que lo hagas, usa el comando
   `gh`.

## Commits

- NUNCA firmes ni te atribuyas los commits: la autoría es del usuario.
- Sin `Co-Authored-By`, sin líneas de "Generated with", sin menciones a Claude
  en el mensaje.
- Mensajes en castellano, en imperativo y concisos.
