# MAGI - Fase 3: Pestañas y Servidor de Sesiones

## Informe de Implementación

**Última actualización:** 15 de septiembre de 2026 (sesión 1: fase completa + primer arreglo por la validación de Hector)

---

## Resumen de lo implementado

La Fase 3 está **implementada al completo** (79/79 del checklist). MAGI pasa
de un proceso con una sesión a dos procesos con N sesiones:

- **`magi --servidor`** (`src/servidor/`): pequeño servidor local que custodia
  las sesiones SSH, el pool de conexiones y el estado de cada pestaña. Socket
  Unix en `$XDG_RUNTIME_DIR/magi/servidor.sock` (macOS `~/Library/Caches/magi/`,
  fallback Linux `/tmp/magi-<uid>/`), directorio 700, umask 077 para el socket,
  `flock` en `servidor.lock`, apagado por inactividad (gracia de
  `gracia_apagado_seg`, 10 s por defecto), log propio
  `servidor.log.<fecha>` y pánico capturado con salida 2. Sin sesiones ni
  clientes, borra socket y lock y sale; con sesiones abiertas nunca se apaga
  solo.
- **Protocolo JSON por líneas** (`src/protocolo.rs`): `VERSION_PROTOCOLO = 1`,
  un mensaje por línea con `tokio_util::codec::LinesCodec`, bytes del terminal
  en base64 y secretos (`Frase`, `Contrasena`) en `Zeroizing` con `Debug` que
  muestra `***`. Mensaje desconocido o mal formado → `Error` + desconexión de
  ese cliente, nunca un pánico. Saludo `Hola`/`Bienvenida` versionado; con
  versión distinta, `VersionIncompatible` y la TUI arranca sin sesiones con el
  aviso persistente en la barra.
- **Sesiones y pool**: `RegistroSesiones` a N (`Sesion` con id creciente sin
  reutilizar, nombre `<host>` / `<host> (n)` con el menor n ≥ 2 libre, parser
  `vt100` del servidor, adjuntos con su tamaño, solicitante,
  `actividad_no_vista`). `conexion/` (salto, huellas, autenticación de F1/F2)
  se ejecuta en el servidor sin cambiar su lógica; los diálogos se convierten
  en mensajes `HuellaDesconocida`/`HuellaCambiada`/`PideFrase`/
  `PideContrasena` **solo al solicitante** (timeout de 5 min; cancelación
  «ventana cerrada» si este se va). Pool `host_id → Arc<Handle>` con
  `multiplexar`, gracia de 30 s sin canales y `Ejecutar{host_id, comando}`
  para el sondeo sobre la conexión viva o cualquier sesión abierta.
  Tamaños: mínimo de los adjuntos, `window_change` al remoto, `parser
  set_size` en el servidor y `Redimensionada{…, ventana_minima}` difundida.
- **Cliente** (`src/cliente/`): conexión al socket con autolanzado del
  servidor (`setsid` vía `pre_exec`, stdio al log del día, espera ≤ 2 s),
  manejo de socket huérfano (lock libre → borrar y relanzar; lock ocupado →
  3 reintentos de 1 s), tareas de lectura/escritura, parser `vt100` por
  pestaña con coalescencia a ~30 fps, puente `Ejecutar` para el sondeo,
  detección de servidor caído con diálogo de relanzado y `Adios` al salir.
- **Interfaz**: barra de pestañas siempre visible (`●` `◐` `✕`, truncado a 14
  con `…`, `+ nueva`, `‹ ›` con más de 9 y aviso a partir de la décima),
  prefijo `Ctrl+]` ampliado (`1-9 n p l c x r w q`, prefijo literal), pestaña
  caída con la última pantalla atenuada y aviso centrado con atajos, relleno
  `░` con `cols×filas (mín. ventana N)`, barra de estado con posición
  `i/n`, tiempo, identidad, carga y ventanas adjuntas, **vista Sesiones**
  (F3 sin pestaña activa) con panel de detalle y datos del servidor, `●N` en
  Hosts y Flota, entradas nuevas de paleta (`nueva ventana`, `cerrar sesión ·
  <pestaña>`, `reconectar · <pestaña>`, `apagar servidor`), `↵` en Hosts y
  Flota abre siempre sesión nueva, `q` sale sin confirmar avisando de las
  sesiones que siguen abiertas y `magi conectar <host>` para lanzadores.
- **CLI**: `magi --servidor`, `magi servidor estado` (pid, versión, sesiones,
  clientes), `magi servidor parar [--si]` (confirmación con recuento) y
  `magi conectar <host>` (error y código 1 si el host no existe).
- **Arreglos de la Fase 2**: R12 — el sondeo anota `sondeo_fallido` solo en la
  transición a CAÍDA y `sondeo_recuperado` al volver, en la TUI y en el CLI.
  R13 — el llavero de macOS usa `security-framework` (Keychain por API, sin
  argv); Linux sigue con `secret-tool` por stdin.
- **Registro**: tipos nuevos `sesion_cerrada`, `sesion_reconectada`,
  `servidor_arrancado`, `servidor_detenido`, `servidor_caido` y
  `sondeo_recuperado`; el filtro «conexiones» de la vista Registro los
  incluye.
- **SQLite**: sin tablas nuevas; cliente y servidor abren `magi.db` con WAL y
  `busy_timeout = 5000`. Un escritor por proceso: el servidor concentra sus
  escrituras (REGISTRO y `HOSTS.ultimo_estado`/`ultima_conexion_en`) en un
  hilo propio con canal de órdenes; el cliente mantiene el bucle de UI como
  único escritor de lo suyo.

**Verificación en la máquina de desarrollo (Omarchy):** servidor en primer
plano + `servidor estado` + `servidor parar --si` ida y vuelta; TUI lanzada
bajo `script` que autolanza el servidor, lo sobrevive y `servidor estado`
encuentra la sesión; `conectar <inexistente>` con código 1; 71 tests verdes
con `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios.

---

## Correcciones tras la validación (Hector, misma sesión)

1. **CRÍTICO — la apertura se quedaba colgada tras la autenticación.**
   `abrir_y_servir` esperaba la terminación del puente de eventos
   (`puente.await`), pero el handler russh conserva un clon del canal de
   eventos mientras la conexión vive, así que el puente nunca termina y la
   tarea no difundía `Sesiones` ni entraba en el bucle: la pestaña quedaba
   clavada en «autenticando» (20–40 s de silencio y `conexion_fallida
   «ventana cerrada»` al cerrarla). Arreglo: el puente vive desacoplado y
   muere solo cuando la conexión cae. Cubierto ahora por un test de
   integración completo (apertura → `PideContrasena` → respuesta → sesión
   `Abierta` difundida) contra un servidor russh de pruebas con
   `channel_open_session`/`pty`/`shell` aceptados.
2. **El llavero de Linux no encontraba las contraseñas de la Fase 2.** El
   lookup/store/clear añadía el atributo `magi-cuenta`, que las entradas
   guardadas por la Fase 2 no tienen: `recuperar` devolvía vacío y la
   conexión pedía la contraseña en un diálogo que debía ser automático.
   Arreglo: se volvió a los atributos de la Fase 2 (`magi-host` + `magi-user`).
3. **Tamaños degenerados derribaban el servidor con un pánico de `vt100`**
   (`grid.rs:683`, «attempt to subtract with overflow»): un terminal que
   reporta 0 columnas (o una `Redimensionar` con 0) hacía que el parser del
   servidor quedara en 1 columna y el remoto, con un `window_change` a 0,
   dibujara anchos imposibles. Arreglo: el servidor sanea todo tamaño
   entrante a mínimo 2 columnas y 1 fila (`AbrirSesion`, `Adjuntar`,
   `Redimensionar`). Detectado reproduciendo la TUI con un pty sin tamaño.
4. **CRÍTICO — la pantalla de la pestaña quedaba en blanco aunque la sesión
   funcionase.** La tarea de lectura del cliente creaba su **propio** registro
   de parsers (`Pantallas::default()` local), distinto del registro que pinta
   la UI: los `Datos`/`PantallaCompleta` del remoto se procesaban en parsers
   fantasma y la ventana mostraba una pantalla vacía; las teclas sí llegaban
   al remoto (se verificó ejecutando un `echo` y leyendo la pantalla del
   servidor por protocolo). Arreglo: el `Cliente` posee el registro y comparte
   el mismo con la tarea de lectura; la App adopta el registro del cliente al
   conectar (`App::nuevo` y relanzado). De paso: la `Bienvenida` reconcilia
   las sesiones existentes (al abrir una ventana nueva reaparecen sus
   pestañas), activar una pestaña nueva desadjunta la visible, `crear()` no
   pisa un parser ya volcado y el indicador ◐ se marca en pestañas no
   visibles. Regresión cubierta por un test que abre una sesión con el
   `Cliente` real y comprueba que el volcado llega a su registro de pantallas.
5. **La vista Sesión se quedaba bloqueada al desaparecer la pestaña activa.**
   Al cerrarse la pestaña adjunta, el foco pasaba a la anterior **sin
   re-adjuntarla** al servidor: pantalla congelada y teclas ignoradas. Si
   desaparecía la última, la vista seguía en Sesión sin nada que pintar
   (área negra) y, como todas las teclas se enrutan a la sesión, F3 no
   abría la lista. Arreglo: `quitar_pestaña` pasa el foco a la anterior y la
   re-adjunta; si no queda ninguna, la vista sale a la previa (o a la lista)
   y nunca se queda en negro. `prefijo q`/`Esc` desde Sesión respeta la
   lista como vista anterior, y desde la lista vuelve a la pestaña
   re-adjuntándola; el servidor caído y el apagado por orden también salen
   de la vista Sesión. Verificado con la TUI real en un pty aislado: cerrar
   la pestaña adjunta deja la otra adjunta (1 ventana) y el eco vuelve.

## Desviaciones respecto a la especificación

1. **Mensajes extra en el protocolo v1 (acordado con Hector).** La tabla de
   mensajes de §5.2 no daba para `magi servidor estado` (pid, nº de clientes)
   ni para el apagado inmediato de `magi servidor parar`. La v1, que se
   define en esta fase, incluye además: `Parar` (cliente → servidor) y
   `Bienvenida{pid, cliente_id, clientes}` (pid y clientes para `estado`,
   `cliente_id` para el aviso «mín. ventana N»). No hay cambio de
   `VERSION_PROTOCOLO` porque la v1 no existía aún.
2. **`Estado{sesion_id, Cerrada, motivo}` en vez de `SesionFallida`.** El
   flujo de §4.2 menciona `SesionFallida{motivo}`, que no está en la tabla de
   mensajes; un fallo de apertura se comunica con `Estado{Cerrada, motivo}` +
   difusión `Sesiones` (equivalente funcional: la pestaña desaparece y la
   barra muestra el motivo).
3. **«mín. ventana N» usa el `cliente_id` del servidor** (contador creciente
   de ventanas) y distingue «mín. esta ventana» cuando la propia ventana
   impone el mínimo. La especificación no definía qué era «N».
4. **Reconexión anota solo `sesion_reconectada`**, no además
   `conexion_abierta` (el flujo §4.4 solo menciona el primero; la identidad
   queda en el detalle). El AC del pool se cumple: `conexion_abierta` una
   sola vez por conexión.
5. **El `stdio` del servidor autolanzado va al fichero del log del día** con
   `O_APPEND` mientras `tracing_appender` escribe también en él; se eligió
   sobre `/dev/null` para no perder el pánico si el appender tarda en
   montarse.
6. **`magi sondear` (CLI) no habla con el servidor** (decidido con Hector):
   conexión efímera como en la Fase 2. El sondeo de la TUI sí usa `Ejecutar`.

---

## Estructura de archivos creada/modificada

```
Cargo.toml                    + base64, tokio-stream, tokio-util (codec),
                                futures-util (sink), nix (fs/process/user),
                                security-framework (target macOS)
README.md                     servidor, pestañas, CLI, atajos, seguridad

src/
├── protocolo.rs              NUEVO  VERSION_PROTOCOLO=1, mensajes serde,
│                                    base64, Secreto (Zeroizing), codec líneas
├── servidor/
│   ├── mod.rs                NUEVO  arranque, socket+lock+umask, bucle de
│   │                                aceptación, apagado por inactividad y por
│   │                                orden, escritor BD único, estado_cli/parar_cli
│   ├── sesiones.rs           NUEVO  Sesion a N, apertura con diálogos al
│   │                                solicitante, bucle de sesión, tamaños,
│   │                                caída/reconexión, difusión de Sesiones
│   ├── conexiones.rs         NUEVO  pool host_id → Handle, gracia 30 s
│   ├── cliente_remoto.rs     NUEVO  cliente adjunto, tamaños, tarea de escritura
│   └── difusion.rs           NUEVO  envío a cliente, a todos y a adjuntos
├── cliente/
│   ├── mod.rs                NUEVO  conexión+autolanzado (setsid), saludo,
│   │                                lectura→eventos con coalescencia,
│   │                                escritura, puente Ejecutar, lock_libre
│   ├── pantallas.rs          NUEVO  registro de parsers vt100 por pestaña
│   └── ventana.rs            NUEVO  [terminal] comando con sh -c
├── conexion/
│   ├── mod.rs                EventoConexion propio (sin app::Evento);
│   │                                FuenteContrasena; puente en lanzar();
│   │                                usuario_local()
│   ├── cliente.rs            Contexto sin app::Evento + fuente_contrasena;
│   │                                el servidor no toca el llavero;
│   │                                sesion_completa pública; abrir_canal pública
├── app.rs                    pestañas (PestanaUI), evento_servidor,
│                                reconciliación de Sesiones, diálogos por
│                                sesion_id, prefijo ampliado, sondeo con
│                                Ejecutar, R12, salida sin confirmar
├── config.rs                 [terminal] comando, [servidor]
│                                gracia_apagado_seg, Rutas.runtime
├── registro.rs               tipos de F3 + filtro «conexiones»
├── llavero.rs                backend por plataforma: security-framework
│                                (macOS) / secret-tool (Linux)
├── main.rs                   --servidor, servidor estado|parar, conectar
├── modelo.rs                 fecha_hoy()
└── ui/
    ├── mod.rs                Vista::Sesiones
    ├── sesion.rs             barra de pestañas, relleno ░, pestaña caída,
    │                         barra de estado con posición/ventanas
    ├── sesiones.rs           NUEVO  lista de sesiones del servidor
    ├── barra.rs / ayuda.rs   ramas de las vistas nuevas
    └── dialogos.rs           ServidorCaido, huellas/frase/contraseña por
                             sesion_id

tests/
├── servidor.rs               NUEVO  arranque, saludo, versión, lock,
│                                    apagado por orden e inactividad, mensaje
│                                    mal formado
└── password.rs               adaptado al EventoConexion propio
```

---

## Decisiones técnicas tomadas durante el desarrollo

- **Desacoplo previo de `conexion/`**: antes del servidor se retiró la
  dependencia de `app::Evento` (los eventos de conexión son su propio tipo y
  el cliente los envuelve en un puente). Eso permite que `conexion/` corra en
  el servidor sin cambios de lógica y que los tests de F2 sigan siendo la red
  de seguridad.
- **`FuenteContrasena::Llavero | Solicitante` en `Contexto`**: las conexiones
  efímeras del cliente leen el llavero como en F2; las sesiones del servidor
  emiten `PideContrasena` y el cliente solicitante lo resuelve (llavero o
  diálogo). El servidor jamás toca el llavero (T15/T19).
- **Cancelación de aperturas sin mensajes nuevos**: cancelar un diálogo (Esc
  en frase/contraseña/huella cambiada) o cerrar la pestaña durante la
  apertura manda `Cerrar{sesion_id}`; el servidor retira la sesión y los
  `oneshot` pendientes, y el flujo F1 termina en `Cancelado` por sí mismo.
- **`Flock` de nix en vez de `flock` deprecado**: el guardia `Flock<File>` se
  conserva vivo hasta la salida del servidor (primera versión lo soltaba al
  terminar la sentencia: bug detectado por el test de lock duplicado).
- **`process_group(0)` eliminado**: entra en conflicto con `setsid`
  (`setpgid` hace al hijo líder y `setsid` devolvía EPERM). `setsid` solo ya
  da la sesión propia.
- **Escritor de BD como hilo con canal** (`OrdenBd`): la `Connection` de
  rusqlite no es `Send`; el hilo posee la conexión de escritura y las tareas
  async le mandan órdenes. Para las fichas hay una segunda conexión de solo
  lectura bajo `Mutex` (WAL permite el lector concurrente).
- **Difusión `Sesiones` como mecanismo de reconciliación**: el cliente
  reconstruye sus pestañas de la lista completa (alta, baja, estado,
  identidad, ventanas, actividad) conservando parsers, orden y pestaña
  activa; es el único canal de estado, sin mensajes de sincronización extra.
- **Apagado por inactividad como revisora de 1 s + gracia**, y el apagado
  final (borrado de socket/lock y `servidor_detenido`) siempre en la salida
  del bucle principal, tanto por orden (`Parar`) como por inactividad.
- **Sin `JoinHandle` en `Sesion`**: no se abortan tareas; cancelar una
  apertura es retirarla del estado y soltar los pendientes, y la tarea se
  limpia sola al encontrar la sesión ausente (sin carreras de abort).
- **El sondeo con `Ejecutar` devuelve `SinSesion` también cuando el exec
  falla** (conexión del pool cerrándose), para que el cliente caiga a su
  conexión efímera en vez de perder el sondeo.
- **R12 por transición del estado de flota** (`evaluar` con el sondeo
  anterior: en la TUI desde memoria, en el CLI desde el último sondeo de la
  BD), y no por `resultado == Error`.

---

## Funcionalidades del checklist completadas

*(texto copiado del checklist; todas marcadas `[x]`)*

### Protocolo
- `protocolo.rs` con `VERSION_PROTOCOLO = 1` y todos los mensajes serde: `Hola`, `Bienvenida`, `VersionIncompatible`, `AbrirSesion`, `Adjuntar`, `Desadjuntar`, `Teclas`, `Redimensionar`, `Cerrar`, `Reconectar`, `DecisionHuella`, `Frase`, `Contrasena`, `Ejecutar`, `Listar`, `Adios`, `Sesiones`, `PantallaCompleta`, `Datos`, `Redimensionada`, `Estado`, `HuellaDesconocida`, `HuellaCambiada`, `PideFrase`, `PideContrasena`, `Ejecutado`, `SinSesion`, `Error`
- Codificación JSON por líneas (`LinesCodec`) con bytes en base64 en `Teclas`, `Datos` y `PantallaCompleta`
- Un mensaje desconocido o mal formado produce `Error` y desconexión de ese cliente, nunca un panic del servidor
- `Frase` y `Contrasena` viajan en `Zeroizing` a ambos lados y nunca se escriben en logs ni en `REGISTRO`
- Tests del protocolo: ida y vuelta de todos los mensajes y rechazo de versión distinta

### Servidor: ciclo de vida
- `magi --servidor` ejecuta el servidor en primer plano con `UnixListener` en `$XDG_RUNTIME_DIR/magi/servidor.sock` (fallback `/tmp/magi-<uid>/`; macOS `~/Library/Caches/magi/`), directorio 700 y umask 077
- Fichero `servidor.lock` con `flock`; un segundo `magi --servidor` termina con mensaje sin tocar el socket
- El primer cliente que no encuentra servidor lo lanza desacoplado (`setsid`, stdio al log) y espera el socket hasta 2 s
  - AC: Dado que no hay servidor, cuando se ejecuta `magi`, entonces la TUI arranca con un servidor nuevo en menos de 2 s y `magi servidor estado` lo muestra
- Socket presente pero sin respuesta y lock libre: el cliente borra el socket huérfano y relanza; con lock ocupado reintenta 3 veces cada 1 s antes de fallar con mensaje
- Apagado por inactividad: sin clientes ni sesiones durante `[servidor] gracia_apagado_seg` (10 s por defecto) el servidor anota `servidor_detenido`, borra socket y lock y sale
  - AC: Dado un servidor con una sesión abierta, cuando se cierran todas las ventanas, entonces el servidor sigue vivo y la sesión reaparece al abrir `magi` de nuevo
- Con sesiones abiertas el servidor nunca se apaga solo
- `magi servidor estado` imprime pid, versión de protocolo, sesiones abiertas (pestaña, host, estado, ventanas) y clientes conectados
- `magi servidor parar` cierra todas las sesiones (anotando `sesion_cerrada`), apaga el servidor y pide confirmación si hay sesiones salvo `--si`
- Saludo `Hola`/`Bienvenida` con versión; con versión distinta el servidor responde `VersionIncompatible` y la TUI arranca sin sesiones explicándolo en la barra
- Log del servidor en `~/.local/state/magi/logs/servidor.log.<fecha>`; un panic se captura, se anota en el log y el proceso sale con código 2
- El servidor anota `servidor_arrancado` y `servidor_detenido` en `REGISTRO` mediante `registro::anotar`
- Cliente y servidor abren `magi.db` con WAL y `busy_timeout = 5000`; el servidor solo escribe `REGISTRO` (eventos de sesión y servidor) y `HOSTS.ultimo_estado` / `ultima_conexion_en`
- Tests del servidor con socket en directorio temporal: arranque, saludo, apagado por inactividad, lock duplicado

### Servidor: sesiones y conexiones
- `conexion/` (cliente russh, salto, huellas, autenticación) se ejecuta en el servidor sin cambiar su lógica; los diálogos se convierten en mensajes al solicitante
- `RegistroSesiones` a N entradas: `Sesion {id creciente, host_id, nombre, estado, conexion, canal, parser vt100, tamano, adjuntos, solicitante, abierta_en, ultima_actividad, actividad_no_vista}`
- `AbrirSesion` crea la sesión con el flujo de la Fase 1 (salto, huella, autenticación, pty, shell) y anota `conexion_abierta` o `conexion_fallida`
- Pool de conexiones `host_id → Arc<Handle>`: con `multiplexar` marcado una segunda sesión al mismo host abre un canal nuevo sin reautenticar; sin marcar, conexión propia que se cierra con su sesión
  - AC: Dado un host con multiplexar y una sesión abierta, cuando se abre otra, entonces no aparece diálogo de huella ni de frase y `conexion_abierta` se anota una sola vez por conexión
- Una conexión del pool sin canales se cierra tras 30 s de gracia; la conexión al host de salto también se comparte por el pool
- Nombres de pestaña: `<host>` o `<host> (n)` con el menor n ≥ 2 libre; renombrar el host no renombra pestañas abiertas
- Sin límite de sesiones; aviso en la barra a partir de la décima
- `HuellaDesconocida`, `HuellaCambiada`, `PideFrase` y `PideContrasena` se envían solo al solicitante; los demás clientes reciben `Estado{abriendo, "esperando decisión en otra ventana"}`
- Si el solicitante se desconecta antes de responder, la apertura se cancela con motivo «ventana cerrada» y se anota `conexion_fallida`; timeout de decisión de 5 min
- El servidor nunca acepta ni sustituye huellas ni usa frases sin `DecisionHuella` / `Frase` del solicitante; la contraseña del llavero la resuelve el cliente solicitante y la envía en `Contrasena`
- `exit` remoto: estado `cerrada`, `sesion_cerrada` en `REGISTRO`, la sesión se elimina y se difunde `Sesiones`
- Red caída o EOF inesperado: estado `caida` con motivo, `Estado` a los adjuntos, `sesion_cerrada` en `REGISTRO`; la sesión se conserva hasta `Reconectar` o `Cerrar`
- `Reconectar` sobre una sesión caída abre conexión nueva con la misma identidad y el mismo id y nombre, parser nuevo, y anota `sesion_reconectada`; sobre una sesión abierta se rechaza
- `Reconectar` sobre un host ya borrado del inventario se rechaza y la pestaña solo puede cerrarse
- `Cerrar` cierra el canal (y la conexión si no es del pool), anota `sesion_cerrada` y elimina la sesión; los ids no se reutilizan
- `Adjuntar`/`Desadjuntar` mantienen la lista de adjuntos por sesión; al adjuntar se envía `PantallaCompleta` con el volcado del parser del servidor
- Tamaño de sesión = mínimo de columnas y filas de los adjuntos; al cambiar se envía `window_change` al remoto, se ajusta el parser y se difunde `Redimensionada`; sin adjuntos se conserva el último tamaño
  - AC: Dado una pestaña adjunta en una ventana de 200×50 y otra de 120×40, cuando se redimensiona la segunda a 100×30, entonces el remoto recibe 100×30 y ambas ventanas muestran `Redimensionada`
- `Teclas` de cualquier adjunto se escriben al canal en orden de llegada, sin arbitraje
- `Datos` del remoto se difunden a los adjuntos sin coalescer y alimentan el parser del servidor; `actividad_no_vista` se marca cuando no hay adjuntos visibles
- `Ejecutar{host_id, comando}` ejecuta el comando en un canal `exec` de la conexión del pool o de cualquier sesión abierta al host y devuelve `Ejecutado`; sin conexión devuelve `SinSesion`
- `Sesiones{lista}` se difunde a todos los clientes en cada alta, baja o cambio de estado
- Al arrancar, el servidor lee `multiplexar` y `keepalive_seg` de la ficha en cada apertura; cambios de identidad, salto o puerto aplican en la siguiente apertura

### Cliente: conexión al servidor
- `cliente/mod.rs`: conexión al socket al arrancar la TUI, saludo, tarea de lectura que convierte mensajes en eventos de la UI y tarea de escritura
- Un `vt100::Parser` por pestaña en el cliente, alimentado por `PantallaCompleta` y `Datos`, con la coalescencia a ~30 fps de la Fase 1
- Entrar en la vista Sesión adjunta la pestaña activa; salir de la vista (o cambiar de pestaña) desadjunta la que deja de verse
- EOF del socket sin `Adios`: todas las pestañas pasan a `✕ servidor caído`, se anota `servidor_caido` y se abre el diálogo de relanzado
  - AC: Dado un servidor al que se envía SIGKILL, cuando la TUI lo detecta, entonces muestra el diálogo y, al aceptar, el servidor nuevo responde al saludo en menos de 2 s
- Diálogo SERVIDOR CAÍDO: `s` relanza (borrando lock huérfano) y `n` sigue sin sesiones
- `Adios` al salir de la TUI; salir con `q` no confirma y muestra «N sesiones siguen abiertas en el servidor»
- El sondeo de Flota con sesión viva pide `Ejecutar` al servidor; con `SinSesion` cae a la conexión efímera de la Fase 2
- `magi conectar <host>` abre la TUI en la vista Sesión con una sesión nueva a ese host; nombre inexistente → error y código 1
- `config.toml`: `[terminal] comando = "alacritty -e magi"` por defecto en Linux (`open -a Terminal magi` en macOS) y `[servidor] gracia_apagado_seg = 10`

### Vista Sesión con pestañas
- Barra de pestañas siempre visible bajo el marco: glifo + nombre (truncado a 14 con `…`) por sesión y `+ nueva` al final; más de 9 pestañas se indican con `‹ ›`
- Indicadores: `●` abierta, `◐` abierta no visible con actividad nueva (se limpia al entrar), `✕` caída; `▸` marca la activa
- Prefijo `Ctrl+]` (configurable, como en la Fase 1) seguido de `1`-`9` va a la pestaña N; prefijo + `n` / `p` siguiente / anterior
- prefijo + `l` abre la vista Sesiones (lista)
- prefijo + `c` abre la paleta filtrada a «conectar · <host>» y la sesión nueva se convierte en la pestaña activa
- prefijo + `x` cierra la pestaña actual tras confirmación; el foco pasa a la anterior y, si era la última, se vuelve a la vista previa
- prefijo + `r` reconecta la pestaña caída
- prefijo + `w` lanza una ventana nueva de terminal con `magi` mediante `[terminal] comando`
- prefijo + `q` / `Esc` vuelve a la vista anterior dejando las pestañas en el servidor; prefijo + prefijo envía el prefijo literal
- Pestaña caída: aviso centrado con motivo, tiempo y atajos `r`/`x` sobre la última pantalla en gris
- Cuando el tamaño remoto es menor que la ventana, la zona sobrante se rellena con `░` tenue y la barra indica `cols×filas (mín. ventana N)`
- Barra de estado: glifo, posición `i/n`, host, tiempo conectado, identidad, carga reciente, número de ventanas adjuntas y atajo de ayuda
- Cierre por `exit` remoto elimina la pestaña y pasa el foco a la anterior; si era la última, vuelve a la vista previa con mensaje
- Todas las teclas salvo el prefijo van al remoto (como en la Fase 1); `Ctrl+P` sigue sin estar disponible en Sesión

### Vista Sesiones (F3 sin pestaña activa)
- `F3` con pestañas abiertas va a la última usada; sin pestaña activa muestra la lista con estado, pestaña, host, tiempo, identidad y ventanas adjuntas
- Navegación de la lista con `↑` `↓` / `j` `k`
- Panel inferior con el detalle de la sesión seleccionada (motivo de caída, hora) y datos del servidor (pid, protocolo, desde cuándo)
- `↵` adjunta y entra; `n` abre la paleta filtrada a hosts; `r` reconecta; `x` cierra con confirmación
- `S` apaga el servidor tras confirmación con recuento de sesiones
- Con el servidor de versión incompatible o caído, la lista está vacía y muestra el aviso y la instrucción

### Otras vistas y paleta
- `↵` en Hosts y Flota abre siempre una sesión nueva; desaparece el diálogo «cerrar la sesión activa» de la Fase 1
- Glifo `●N` en Hosts y Flota cuando hay más de una sesión al host
- Entradas nuevas en la paleta: `nueva ventana`, `cerrar sesión · <pestaña>`, `reconectar · <pestaña>`, `apagar servidor`
- Ayuda `?` actualizada en Sesión y Sesiones; ninguna acción destructiva (cerrar pestaña, apagar servidor, sustituir huella) sin confirmación

### Registro
- Tipos nuevos en `REGISTRO`: `sesion_cerrada` (con motivo), `sesion_reconectada`, `servidor_arrancado`, `servidor_detenido`, `servidor_caido`; el filtro `t` de la vista Registro los incluye en «conexiones»
- Arreglo R12: el sondeo anota `sondeo_fallido` solo en la transición a CAÍDA y `sondeo_recuperado` al volver a NOMINAL/CARGA/ALCANZABLE, no en cada refresco
- Arreglo R13: en macOS el llavero usa el crate `security-framework` (API de Keychain) en lugar del subproceso `security`; Linux sigue con `secret-tool` por stdin

### Calidad
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- `cargo test` verde con los tests nuevos de protocolo y servidor
- README actualizado: servidor, pestañas, `magi conectar`, `magi servidor`, ventana nueva y qué pasa al cerrar ventanas

---

## Pendientes y bloqueos

- **Validación manual con hosts reales** (como en F2, que sigue pendiente —
  riesgo R15): abrir varias pestañas al mismo host, compartir una pestaña
  entre dos terminales con tamaños distintos (relleno `░` y `window_change`),
  `multiplexar` (sin huella/frase en la segunda), `exit` remoto, caída de red
  con `prefijo r`, SIGKILL al servidor con relanzado, apagado por inactividad
  de 10 s y `magi servidor parar` con sesiones.
- **macOS sin verificar** (sin equipo; R9/R17): rutas `~/Library/Caches/magi/`
  y el backend de `security-framework` compilan como diseño, pero no se han
  ejecutado. En cuanto haya un mac, probar autolanzado, `setsid` y el
  Keychain por API.
- El rendimiento objetivo (20 sesiones × 3 ventanas, eco < 20 ms) no se ha
  medido; no hay nada que lo haga sospechar (un parser y un difusión por
  mensaje), pero queda para la validación.

---

## Ejecución y pruebas

```sh
# Desarrollo en dos terminales:
cargo run -- --servidor          # servidor en primer plano (log: servidor.log.<fecha>)
cargo run                        # la TUI conecta con ese servidor

# Sin servidor, la TUI lo autolanza (setsid) y `servidor estado` lo muestra:
cargo run
cargo run -- servidor estado
cargo run -- servidor parar      # pide confirmación; --si la omite
cargo run -- conectar <host>     # TUI con una sesión nueva a ese host

# Aislado (ojo: `directories` resuelve el home por uid, no por $HOME):
HOME=... XDG_DATA_HOME=... XDG_CONFIG_HOME=... XDG_STATE_HOME=...
XDG_RUNTIME_DIR=... target/debug/magi --servidor

# Verificación:
cargo test                       # 71 tests: almacén, ssh_config, contraseña,
                                 # protocolo, servidor (socket temporal) y cliente
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Qué probar a mano en la validación: `↵` en un host (pestaña `●`), otra
pestaña al mismo host con `multiplexar` (sin reautenticar), `exit` en la
pestaña (desaparece, foco a la anterior), matar la red y `prefijo r`,
compartir la pestaña desde otra terminal (tamaño mínimo + `░`), `q` (siguen
abiertas), volver a abrir (reaparecen), `SIGKILL` al servidor (diálogo `s`) y
`magi servidor estado|parar`.
