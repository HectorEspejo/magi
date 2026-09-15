# MAGI - Checklist Fase 3: Pestañas y Servidor de Sesiones

## Protocolo
- [x] `protocolo.rs` con `VERSION_PROTOCOLO = 1` y todos los mensajes serde: `Hola`, `Bienvenida`, `VersionIncompatible`, `AbrirSesion`, `Adjuntar`, `Desadjuntar`, `Teclas`, `Redimensionar`, `Cerrar`, `Reconectar`, `DecisionHuella`, `Frase`, `Contrasena`, `Ejecutar`, `Listar`, `Adios`, `Sesiones`, `PantallaCompleta`, `Datos`, `Redimensionada`, `Estado`, `HuellaDesconocida`, `HuellaCambiada`, `PideFrase`, `PideContrasena`, `Ejecutado`, `SinSesion`, `Error`
- [x] Codificación JSON por líneas (`LinesCodec`) con bytes en base64 en `Teclas`, `Datos` y `PantallaCompleta`
- [x] Un mensaje desconocido o mal formado produce `Error` y desconexión de ese cliente, nunca un panic del servidor
- [x] `Frase` y `Contrasena` viajan en `Zeroizing` a ambos lados y nunca se escriben en logs ni en `REGISTRO`
- [x] Tests del protocolo: ida y vuelta de todos los mensajes y rechazo de versión distinta

## Servidor: ciclo de vida
- [x] `magi --servidor` ejecuta el servidor en primer plano con `UnixListener` en `$XDG_RUNTIME_DIR/magi/servidor.sock` (fallback `/tmp/magi-<uid>/`; macOS `~/Library/Caches/magi/`), directorio 700 y umask 077
- [x] Fichero `servidor.lock` con `flock`; un segundo `magi --servidor` termina con mensaje sin tocar el socket
- [x] El primer cliente que no encuentra servidor lo lanza desacoplado (`setsid`, stdio al log) y espera el socket hasta 2 s
  - AC: Dado que no hay servidor, cuando se ejecuta `magi`, entonces la TUI arranca con un servidor nuevo en menos de 2 s y `magi servidor estado` lo muestra
- [x] Socket presente pero sin respuesta y lock libre: el cliente borra el socket huérfano y relanza; con lock ocupado reintenta 3 veces cada 1 s antes de fallar con mensaje
- [x] Apagado por inactividad: sin clientes ni sesiones durante `[servidor] gracia_apagado_seg` (10 s por defecto) el servidor anota `servidor_detenido`, borra socket y lock y sale
  - AC: Dado un servidor con una sesión abierta, cuando se cierran todas las ventanas, entonces el servidor sigue vivo y la sesión reaparece al abrir `magi` de nuevo
- [x] Con sesiones abiertas el servidor nunca se apaga solo
- [x] `magi servidor estado` imprime pid, versión de protocolo, sesiones abiertas (pestaña, host, estado, ventanas) y clientes conectados
- [x] `magi servidor parar` cierra todas las sesiones (anotando `sesion_cerrada`), apaga el servidor y pide confirmación si hay sesiones salvo `--si`
- [x] Saludo `Hola`/`Bienvenida` con versión; con versión distinta el servidor responde `VersionIncompatible` y la TUI arranca sin sesiones explicándolo en la barra
- [x] Log del servidor en `~/.local/state/magi/logs/servidor.log.<fecha>`; un panic se captura, se anota en el log y el proceso sale con código 2
- [x] El servidor anota `servidor_arrancado` y `servidor_detenido` en `REGISTRO` mediante `registro::anotar`
- [x] Cliente y servidor abren `magi.db` con WAL y `busy_timeout = 5000`; el servidor solo escribe `REGISTRO` (eventos de sesión y servidor) y `HOSTS.ultimo_estado` / `ultima_conexion_en`
- [x] Tests del servidor con socket en directorio temporal: arranque, saludo, apagado por inactividad, lock duplicado

## Servidor: sesiones y conexiones
- [x] `conexion/` (cliente russh, salto, huellas, autenticación) se ejecuta en el servidor sin cambiar su lógica; los diálogos se convierten en mensajes al solicitante
- [x] `RegistroSesiones` a N entradas: `Sesion {id creciente, host_id, nombre, estado, conexion, canal, parser vt100, tamano, adjuntos, solicitante, abierta_en, ultima_actividad, actividad_no_vista}`
- [x] `AbrirSesion` crea la sesión con el flujo de la Fase 1 (salto, huella, autenticación, pty, shell) y anota `conexion_abierta` o `conexion_fallida`
- [x] Pool de conexiones `host_id → Arc<Handle>`: con `multiplexar` marcado una segunda sesión al mismo host abre un canal nuevo sin reautenticar; sin marcar, conexión propia que se cierra con su sesión
  - AC: Dado un host con multiplexar y una sesión abierta, cuando se abre otra, entonces no aparece diálogo de huella ni de frase y `conexion_abierta` se anota una sola vez por conexión
- [x] Una conexión del pool sin canales se cierra tras 30 s de gracia; la conexión al host de salto también se comparte por el pool
- [x] Nombres de pestaña: `<host>` o `<host> (n)` con el menor n ≥ 2 libre; renombrar el host no renombra pestañas abiertas
- [x] Sin límite de sesiones; aviso en la barra a partir de la décima
- [x] `HuellaDesconocida`, `HuellaCambiada`, `PideFrase` y `PideContrasena` se envían solo al solicitante; los demás clientes reciben `Estado{abriendo, "esperando decisión en otra ventana"}`
- [x] Si el solicitante se desconecta antes de responder, la apertura se cancela con motivo «ventana cerrada» y se anota `conexion_fallida`; timeout de decisión de 5 min
- [x] El servidor nunca acepta ni sustituye huellas ni usa frases sin `DecisionHuella` / `Frase` del solicitante; la contraseña del llavero la resuelve el cliente solicitante y la envía en `Contrasena`
- [x] `exit` remoto: estado `cerrada`, `sesion_cerrada` en `REGISTRO`, la sesión se elimina y se difunde `Sesiones`
- [x] Red caída o EOF inesperado: estado `caida` con motivo, `Estado` a los adjuntos, `sesion_cerrada` en `REGISTRO`; la sesión se conserva hasta `Reconectar` o `Cerrar`
- [x] `Reconectar` sobre una sesión caída abre conexión nueva con la misma identidad y el mismo id y nombre, parser nuevo, y anota `sesion_reconectada`; sobre una sesión abierta se rechaza
- [x] `Reconectar` sobre un host ya borrado del inventario se rechaza y la pestaña solo puede cerrarse
- [x] `Cerrar` cierra el canal (y la conexión si no es del pool), anota `sesion_cerrada` y elimina la sesión; los ids no se reutilizan
- [x] `Adjuntar`/`Desadjuntar` mantienen la lista de adjuntos por sesión; al adjuntar se envía `PantallaCompleta` con el volcado del parser del servidor
- [x] Tamaño de sesión = mínimo de columnas y filas de los adjuntos; al cambiar se envía `window_change` al remoto, se ajusta el parser y se difunde `Redimensionada`; sin adjuntos se conserva el último tamaño
  - AC: Dado una pestaña adjunta en una ventana de 200×50 y otra de 120×40, cuando se redimensiona la segunda a 100×30, entonces el remoto recibe 100×30 y ambas ventanas muestran `Redimensionada`
- [x] `Teclas` de cualquier adjunto se escriben al canal en orden de llegada, sin arbitraje
- [x] `Datos` del remoto se difunden a los adjuntos sin coalescer y alimentan el parser del servidor; `actividad_no_vista` se marca cuando no hay adjuntos visibles
- [x] `Ejecutar{host_id, comando}` ejecuta el comando en un canal `exec` de la conexión del pool o de cualquier sesión abierta al host y devuelve `Ejecutado`; sin conexión devuelve `SinSesion`
- [x] `Sesiones{lista}` se difunde a todos los clientes en cada alta, baja o cambio de estado
- [x] Al arrancar, el servidor lee `multiplexar` y `keepalive_seg` de la ficha en cada apertura; cambios de identidad, salto o puerto aplican en la siguiente apertura

## Cliente: conexión al servidor
- [x] `cliente/mod.rs`: conexión al socket al arrancar la TUI, saludo, tarea de lectura que convierte mensajes en eventos de la UI y tarea de escritura
- [x] Un `vt100::Parser` por pestaña en el cliente, alimentado por `PantallaCompleta` y `Datos`, con la coalescencia a ~30 fps de la Fase 1
- [x] Entrar en la vista Sesión adjunta la pestaña activa; salir de la vista (o cambiar de pestaña) desadjunta la que deja de verse
- [x] EOF del socket sin `Adios`: todas las pestañas pasan a `✕ servidor caído`, se anota `servidor_caido` y se abre el diálogo de relanzado
  - AC: Dado un servidor al que se envía SIGKILL, cuando la TUI lo detecta, entonces muestra el diálogo y, al aceptar, el servidor nuevo responde al saludo en menos de 2 s
- [x] Diálogo SERVIDOR CAÍDO: `s` relanza (borrando lock huérfano) y `n` sigue sin sesiones
- [x] `Adios` al salir de la TUI; salir con `q` no confirma y muestra «N sesiones siguen abiertas en el servidor»
- [x] El sondeo de Flota con sesión viva pide `Ejecutar` al servidor; con `SinSesion` cae a la conexión efímera de la Fase 2
- [x] `magi conectar <host>` abre la TUI en la vista Sesión con una sesión nueva a ese host; nombre inexistente → error y código 1
- [x] `config.toml`: `[terminal] comando = "alacritty -e magi"` por defecto en Linux (`open -a Terminal magi` en macOS) y `[servidor] gracia_apagado_seg = 10`

## Vista Sesión con pestañas
- [x] Barra de pestañas siempre visible bajo el marco: glifo + nombre (truncado a 14 con `…`) por sesión y `+ nueva` al final; más de 9 pestañas se indican con `‹ ›`
- [x] Indicadores: `●` abierta, `◐` abierta no visible con actividad nueva (se limpia al entrar), `✕` caída; `▸` marca la activa
- [x] Prefijo `Ctrl+]` (configurable, como en la Fase 1) seguido de `1`-`9` va a la pestaña N; prefijo + `n` / `p` siguiente / anterior
- [x] prefijo + `l` abre la vista Sesiones (lista)
- [x] prefijo + `c` abre la paleta filtrada a «conectar · <host>» y la sesión nueva se convierte en la pestaña activa
- [x] prefijo + `x` cierra la pestaña actual tras confirmación; el foco pasa a la anterior y, si era la última, se vuelve a la vista previa
- [x] prefijo + `r` reconecta la pestaña caída
- [x] prefijo + `w` lanza una ventana nueva de terminal con `magi` mediante `[terminal] comando`
- [x] prefijo + `q` / `Esc` vuelve a la vista anterior dejando las pestañas en el servidor; prefijo + prefijo envía el prefijo literal
- [x] Pestaña caída: aviso centrado con motivo, tiempo y atajos `r`/`x` sobre la última pantalla en gris
- [x] Cuando el tamaño remoto es menor que la ventana, la zona sobrante se rellena con `░` tenue y la barra indica `cols×filas (mín. ventana N)`
- [x] Barra de estado: glifo, posición `i/n`, host, tiempo conectado, identidad, carga reciente, número de ventanas adjuntas y atajo de ayuda
- [x] Cierre por `exit` remoto elimina la pestaña y pasa el foco a la anterior; si era la última, vuelve a la vista previa con mensaje
- [x] Todas las teclas salvo el prefijo van al remoto (como en la Fase 1); `Ctrl+P` sigue sin estar disponible en Sesión

## Vista Sesiones (F3 sin pestaña activa)
- [x] `F3` con pestañas abiertas va a la última usada; sin pestaña activa muestra la lista con estado, pestaña, host, tiempo, identidad y ventanas adjuntas
- [x] Navegación de la lista con `↑` `↓` / `j` `k`
- [x] Panel inferior con el detalle de la sesión seleccionada (motivo de caída, hora) y datos del servidor (pid, protocolo, desde cuándo)
- [x] `↵` adjunta y entra; `n` abre la paleta filtrada a hosts; `r` reconecta; `x` cierra con confirmación
- [x] `S` apaga el servidor tras confirmación con recuento de sesiones
- [x] Con el servidor de versión incompatible o caído, la lista está vacía y muestra el aviso y la instrucción

## Otras vistas y paleta
- [x] `↵` en Hosts y Flota abre siempre una sesión nueva; desaparece el diálogo «cerrar la sesión activa» de la Fase 1
- [x] Glifo `●N` en Hosts y Flota cuando hay más de una sesión al host
- [x] Entradas nuevas en la paleta: `nueva ventana`, `cerrar sesión · <pestaña>`, `reconectar · <pestaña>`, `apagar servidor`
- [x] Ayuda `?` actualizada en Sesión y Sesiones; ninguna acción destructiva (cerrar pestaña, apagar servidor, sustituir huella) sin confirmación

## Registro
- [x] Tipos nuevos en `REGISTRO`: `sesion_cerrada` (con motivo), `sesion_reconectada`, `servidor_arrancado`, `servidor_detenido`, `servidor_caido`; el filtro `t` de la vista Registro los incluye en «conexiones»
- [x] Arreglo R12: el sondeo anota `sondeo_fallido` solo en la transición a CAÍDA y `sondeo_recuperado` al volver a NOMINAL/CARGA/ALCANZABLE, no en cada refresco
- [x] Arreglo R13: en macOS el llavero usa el crate `security-framework` (API de Keychain) en lugar del subproceso `security`; Linux sigue con `secret-tool` por stdin

## Calidad
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de protocolo y servidor
- [x] README actualizado: servidor, pestañas, `magi conectar`, `magi servidor`, ventana nueva y qué pasa al cerrar ventanas

---

**Progreso Fase 3:** 79 / 79 funcionalidades

**Total MAGI (Fases 1-3):** 289 / 291 funcionalidades
