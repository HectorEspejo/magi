# MAGI — Fase 13: Servicios, contenedores y logs — informe de implementación

**Estado:** implementación completa e integrada en esta sesión (rama
`fase13-servicios_contenedores_logs`). La suite completa del workspace está en
verde (49 binarios de prueba, 1 570 pruebas, 0 fallos), con `cargo clippy
--workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` y
`cargo deny check` limpios, y `assets/` regenerados y comprobados. Quedan como
pendientes externos el guion de validación manual del desarrollador (informe
§11) y la ejecución de las pruebas instrumentadas de Android en un emulador.
**Fecha de esta revisión:** 8 de noviembre de 2026.
**Versión:** escritorio **1.3.0** (MINOR: funcionalidad nueva, migración 9,
protocolo v8 y `formato` 3 de sincronización condicional). La app Android no
cambia de versión: solo incorpora la lectura del `formato` 3 y su línea en
`android/CHANGELOG.md`.

## Resumen de lo implementado

**Contrato compilable antes de repartir el trabajo (T63).** Se fijaron y
commitearon primero los tipos y enganches que comparten las pistas:
`crates/magi-nucleo/src/servicios/` con los tipos del objetivo, la acción, el
listado, el privilegio, el resultado y las órdenes de pestaña; `src/protocolo.rs`
con `VERSION_PROTOCOLO = 8`, `RANGO_SERVICIOS = 2^45` y los mensajes de §5.2;
la migración 9; la sección `[servicios]` de `config.toml`; `UsoCanal::Servicios` y el
tipo de REGISTRO `servicio_accion`.

**Núcleo y script (magi-nucleo).** `crates/magi-nucleo/src/servicios/`
(`mod`, `limites`, `orden`, `lectura`, `acciones` y `validadores`) contiene los
tipos que viajan, la validación de nombres de §7.11, la composición de las
órdenes remotas, la conversión de `OrdenPestana` a la orden de una pestaña,
las acciones permitidas por estado y tipo, la deliberación exigida, las
unidades críticas, los validadores de configuración (Nginx, Caddy, Apache,
HAProxy, Compose y propia) y todos los límites de §8, sin depender de la TUI ni
del servidor. `crates/magi-nucleo/scripts/servicios.sh` (embebido con
`include_str!`, ~1 260 líneas POSIX) implementa las subórdenes `privilegio`,
`sudo`, `listar`, `consumo`, `detalle`, `accion` y `logs`, con la contraseña de
sudo por la primera línea de la entrada estándar, validación de todos los
argumentos (códigos 64/66/69/77), sudo orden a orden con `timeout` por fuera y
`LC_ALL=C`, nunca `sudo sh` en las acciones, Podman nunca con sudo, la
comprobación de configuración en el mismo canal y justo antes de la acción, la
espera al estado final con segunda mirada a los 2 s y las últimas 20 líneas del
diario, y el corte del seguimiento cuando se cierra la entrada estándar.

**Servidor y protocolo.** `src/servidor/servicios.rs` implementa el listado, el
consumo, el detalle, `ComprobarSudo` y las acciones custodiadas sobre el pool
(`UsoCanal::Servicios`, `ConexionTomada` soltada por identidad en todas las
ramas), con una sola acción por objetivo, fases difundidas, deliberación
validada por el servidor (existe, es de esa acción, es de ese host y no está
usada; «ya usada» en los dos sentidos), `REGISTRO servicio_accion` tras
`OrdenBd::Barrera`, resultado de la deliberación en `DELIBERACIONES`, plazos de
10 s (listado, consumo, detalle y sudo), 20 s (comprobación) y
`espera_seg + 30 s` (acción), y logs en vivo con lotes cada 100 ms (máximo 500
líneas), cola de 4 MiB con descarte contado, contrapresión real por el socket
del solicitante y `PararLog` que cierra el canal. La contraseña de sudo viaja
en `Zeroizing` dentro de cada petición, se escribe en la primera línea de la
entrada estándar y no se guarda ni se registra. El protocolo v8 añade los 14
mensajes de §5.2, `AbrirSesion.orden` (pestañas con orden armadas siempre por el
servidor, nunca con texto del cliente), `TrabajoServidor.acciones_servicio` y las
acciones en curso en `Bienvenida`. Las acciones y las suscripciones de log se
indexan por `(cliente_id, peticion_id)` (corrección de la revisión adversarial:
dos ventanas pueden repetir ids sin pisarse) y una suscripción de log conserva
su `ConexionTomada` mientras dura (corrección de la revisión adversarial: el pool
ya no la recicla a los 30 s).

**Cliente y vista (F9).** `src/app/servicios.rs` y `src/ui/servicios.rs`
implementan el estado y la interfaz de §6: hosts SSH + «Toda la flota», secciones
1 Servicios, 2 Timers y 3 Contenedores con recuento, orden de §7.1 (fallidos y
vigilados ★ arriba, resto alfabético, línea separadora), filtro `/`, detalle con
Estado/Dependencias/Unidad (o `inspect` del contenedor y timer con próxima,
última y servicio), consumo de CPU y memoria aparte y solo con la sección de
contenedores a la vista, teclas `A P R G H` con confirmación o deliberación
(`PlanDeliberado::Servicio`, MELCHIOR-1 sin contar el objetivo, aviso en rojo de
unidades críticas con los hosts que van a través), diálogo de resultado (hecho,
fallido, sin confirmar, rechazada por la configuración o error) y refresco
inmediato, sudo por host en `Zeroizing` con caducidad de `sudo_memoria_min`,
`!` para pedirla y `ComprobarSudo` antes de confirmar o deliberar, comprobación
propia (`c` en F9 y bloque en la ficha) con aviso al guardar la primera
comprobación de que MAGI 1.2 dejará de leer el equipo, panel de logs con anillo
de `lineas_log`, filtros por texto y nivel, pausa, marcas de omitidas, `t` a una
pestaña, `S` para la shell de un contenedor y `PararLog` al cerrar o cambiar de
host, «Toda la flota» con los fallos del último sondeo de cada host sin abrir
conexiones, `⚠N` y bloque FALLOS en Flota, `u` desde Flota/Hosts/ficha y modo
estrecho con `Tab`. La barra solo ofrece las acciones que valen para el
elemento y su estado, y las acciones en curso se marcan también en la barra de
Sesión.

**Datos, sondeo y sincronización.** La migración 9 (`user_version` 9) crea
`COMPROBACIONES_SERVICIO` (con `uuid`, `version` y `vector`, solo hosts `ssh`,
UNIQUE por host/tipo/objetivo y lápida al borrar) y `SONDEOS.fallos_json`
(nulo en sondeos anteriores y si el host no respondió). El sondeo amplía
`sondeo.sh` con las unidades fallidas y los contenedores caídos (hasta 20
nombres y total real, sin sudo ni diálogos, sin cambiar el estado del host).
`COMPROBACIONES_SERVICIO` viaja como elemento propio `comprobacion_servicio` y el
fichero sube a `formato` 3 solo si hay alguna (si no, 2 o 1 como antes); borrar
deja lápida, una comprobación huérfana es un conflicto recuperable y un
`formato` mayor que 3 sigue siendo `formato_nuevo`. `docs/formato-sincronizacion.md`
documenta el formato 3 y el elemento.

**Documentación, versión y app Android.** `Cargo.toml`/`Cargo.lock` a 1.3.0,
fecha de la página man actualizada, `assets/` regenerados con
`scripts/generar-recursos.sh` y verificados con `cargo test -p magi-ssh --test
recursos`; `CHANGELOG.md` y `README.md` (sección «Servicios, contenedores y
logs»), `android/CHANGELOG.md`, prueba de humo del móvil esperando formato 3 /
`user_version` 9, contrato JVM del formato 3 y prueba instrumentada nueva con
una fixture del formato 3 generada por un test `#[ignore]`.

**Revisión adversarial de cierre (T31, T54).** Antes de cerrar se hizo una pasada
adversarial sobre el código nuevo del servidor, el script, los logs y el manejo
de sudo, con informe en `docs/fase13/magi-fase13-revision-adversarial.md`. Encontró
dos críticos, ya corregidos con prueba de regresión: (1) las acciones y
suscripciones de log se indexaban solo por `peticion_id`, de modo que dos
ventanas podían pisarse, cruzar respuestas y perder filas de REGISTRO; ahora la
clave es `(cliente_id, peticion_id)`. (2) una suscripción de log soltaba la
`ConexionTomada` al abrir el canal, y el pool cortaba la conexión a los ~30 s
sin canales; ahora la conexión se conserva mientras dura la suscripción y se
suelta al terminar en todas las ramas, con una prueba que exige que el
seguimiento siga vivo pasada la gracia del pool. Tras esas correcciones el revisor
añadió dos altos y cinco medios, todos corregidos también con prueba: usuario
con el grupo docker pero sin sudo (podía ver los contenedores y no gestionarlos),
comprobación propia de un proyecto ejecutada con sudo en vez de con los
privilegios de la acción (Podman rootless siempre la rechazaba), carreras de
mensajes por cliente con suscripciones de log huérfanas (ahora hay un worker FIFO
por ventana y se comprueba que el cliente sigue conectado), plazo de 10 s que
abarcaba los diálogos de huella/frase (ahora solo cubre red y canal), anillo de
logs sin presupuesto de bytes, lote de logs que podía superar el tamaño máximo
del códec y TERM que mataba el wrapper y no el journalctl/docker logs real.
Los bajos B1, B2, B3, B5, B6, B7 y B8 también están corregidos; solo B4 (el
sondeo no recibe [servicios] contenedores) se queda como desviación documentada
en el punto 1. Lo que el revisor no pudo revisar está listado en su informe §9 y
en «Pendientes y bloqueos».

## Desviaciones respecto a la especificación (qué y por qué)

Además de las correcciones de la revisión adversarial (que no son desviaciones
funcionales), el desarrollo tomó estas decisiones donde el informe no fijaba el
detalle o donde el contrato de datos del equipo no tenía el dato necesario:

1. **El sondeo ampliado autodetecta los motores de contenedores**
   (`docker` y después `podman`) en vez de recibir el argumento `<motores>` de
   §7.8: `PeticionSondeo` no lleva hoy ese dato y sus constructores están en
   `src/app.rs` y `src/main.rs`. Con `[servicios] contenedores = []` el sondeo sigue
   mirando si hay motor (limitación anotada); el listado de la vista sí respeta
   la preferencia. El `TIMEOUT_SEG` del sondeo se queda en 5 s.
2. **El motor de los objetivos de contenedor lo autodetecta el script** en
   `detalle`, `logs` y `accion` (el protocolo no lleva el motor en esos
   mensajes); el formato acepta además el motor como argumento opcional, como
   decía §5.1.
3. **La comprobación propia se ejecuta con `sh -c` bajo los privilegios de la
   acción.** Es la única orden libre, la escribe el usuario (como un snippet) y
   §4.3/§7.3 lo piden así; el resto de órdenes van con sudo orden a orden y nunca
   con `sudo sh`. Los argumentos y la orden se validan en Rust y en el script.
4. **La gramática interna de `servicios.sh` se amplía** con
   `MAGI_BLOQUE`/`MAGI_FIN_BLOQUE`, `MAGI_AVISO`, `MAGI_RELOJ=<epoch> <uptime>` y
   `MAGI_MENSAJE` (documentada en la cabecera del script y en
   `lectura.rs`) para transportar bloques, avisos, el reloj del host y el
   mensaje de una orden fallida. Lo que viaja por el protocolo sigue siendo
   `Listado`, `Detalle` y `ResultadoServicio` tipados.
5. **Dos comprobaciones para el mismo host + objetivo creadas por equipos
   distintos** se resuelven como conflicto de nombre al fusionar (no se renombra
   el objetivo, que dejaría de ser válido); la copia no se aplica y la
   importación no se rompe. Es una consecuencia de la UNIQUE(host_id,
   objetivo_tipo, objetivo) de §3.1 con uuids distintos.
6. **Vista y ficha:** `F9` desde Flota/Hosts hereda el host elegido y abre la
   columna de hosts; `u` y la ficha entran directos a la lista del host. En la
   ficha, las secciones de Túneles y Comprobaciones se reparten el sitio y se
   reserva una fila para las comprobaciones para que el foco nunca quede
   invisible.
7. **El anillo del cliente descarta las líneas más viejas sin recuento propio**:
   el recuento de omitidas que se enseña es el que manda el servidor con sus
   lotes (la contrapresión de §7.6 es del servidor).
8. **El refresco automático se detiene** si el último listado quedó «no
   responde» o «desconectado»; se reintenta con `r`, que relanza el servidor
   de sesiones si hace falta. El pie dice «desconectado · r reconectar» tanto si
   cae el servidor como si cae la conexión con el host.
9. **Android:** la lectura del `formato` 3 no añade lógica Kotlin; la hace el
   núcleo compartido (regla de no duplicar la fusión ni el formato). La fixture
   del formato 3 se genera con un test `#[ignore]` nuevo y se versiona en
   `android/app/src/androidTest/assets/`. No se añadió una prueba JVM que cargue
   el núcleo real porque el JNA de Gradle es el `@aar` de Android (sin
   `libjnidispatch` de escritorio); la lectura real se cubre con la prueba
   instrumentada y con la suite de Rust del núcleo.
10. **`deny.toml` gana una excepción** para RUSTSEC-2026-0441 (`pin-utils` sin
    mantener, transitiva de `sspi`/`async-dnssd` en el
    `ironrdp-connector` vendorizado). El aviso se publicó después de la fase y no
    lo introduce este trabajo; se revisa al subir IronRDP.
11. **Los logs de contenedor usan dos tuberías con nombre** (`mktemp` + `mkfifo`,
    utilidades estándar como `timeout`) y `awk`; si faltan en el host el script
    sale con `ERROR sin_temporales` (70). Queda documentado en el README, junto a
    los permisos del host.
12. **Detalles menores pendientes** (no bloquean el checklist): un host sin
    systemd sigue enseñando las secciones 1 y 2 vacías y seleccionables
    (cosmético) y la prueba del seguimiento que supera la gracia del pool dura
    ~40 s a propósito.

## Estructura de archivos creada/modificada

**Núcleo compartido (`crates/magi-nucleo`):**

| Ruta | Qué |
|---|---|
| `src/servicios/mod.rs` | tipos de §2: `Objetivo`, `Accion`, `Privilegio`, `Listado`, `Unidad`, `Timer`, `Contenedor`, `Proyecto`, `Detalle`, `Consumo`, `ResultadoServicio`, `InfoAccion`, `OrdenPestana`, `FallosSondeo` |
| `src/servicios/limites.rs` | topes de §8 y valores por defecto de `[servicios]` |
| `src/servicios/orden.rs` | nombres válidos, composición de órdenes, `comando_pestana`, validadores de críticas |
| `src/servicios/lectura.rs` | lectura de `MAGI_*`, `systemctl show`, JSON de Docker/Podman y `journalctl`, `inspect`, límites y recortes |
| `src/servicios/acciones.rs` | acciones permitidas, deliberación, comprobación, críticas y `exito` de §7.4 |
| `src/servicios/validadores.rs` | comprobaciones conocidas y textos para la confirmación |
| `scripts/servicios.sh` | script embebido completo (subórdenes de §5.1) |
| `src/almacen/comprobaciones_servicio.rs` | CRUD, lápida, versiones y validación de COMPROBACIONES_SERVICIO |
| `src/almacen/{mod,versiones,lapidas,sondeos,hosts}.rs` | alta del CRUD, `Tabla::ComprobacionesServicio`, lápida, `fallos_json` y borrado en cascada con lápida |
| `src/modelo.rs` | `ComprobacionServicio`, `Sondeo.fallos` y utilidades |
| `src/sincronizacion/{formato,elementos,fusion,aplicacion}.rs` | `formato` 3, elemento `comprobacion_servicio`, fusión/aplicación con huérfanas y lápidas |
| `src/registro.rs` | `servicio_accion` en tipos y filtros del REGISTRO |
| `src/almacen/migraciones.rs` | migración 9 y `VERSION_BD = 9` |

**Escritorio (`src`):**

| Ruta | Qué |
|---|---|
| `servidor/servicios.rs` | servidor completo de §3.3-§3.4, §4.2-§4.5, §5.2 y §7.3-§7.6 |
| `servidor/{mod,cliente_remoto,ejecuciones,conexiones}.rs` | despacho, hooks, contador de pendientes, «ya usada» en los dos sentidos y `UsoCanal::Servicios` |
| `protocolo.rs` | protocolo v8 completo y pestañas con orden |
| `config.rs` | sección `[servicios]`, validación y avisos de arranque |
| `flota/{mod,parser,estado}.rs` y `flota/sondeo.sh` | fallos del sondeo y su lectura |
| `app/servicios.rs` | estado del cliente: listados, anillo, sudo en memoria, filtros, peticiones y diálogos |
| `ui/servicios.rs` | vista F9, detalle, panel de logs, toda la flota y modo estrecho |
| `app.rs`, `app/dialogo_magi.rs` | `Vista::Servicios`, F9/`u`, modales, `PlanDeliberado::Servicio` y MELCHIOR-1 |
| `ui/{mod,disposicion,barra,ficha,flota,hosts,ayuda,deliberacion}.rs` | integración, mínimo/modo estrecho, barra dinámica, comprobaciones en la ficha, ⚠N/FALLOS, ayuda y aviso crítico |
| `lib.rs` | fecha de la versión 1.3.0 y comentarios de versión |

**Pruebas y fixtures:** `crates/magi-nucleo/tests/servicios_lectura.rs` (31),
`servicios_script.rs` (26), `sincronizacion_comprobaciones.rs` (17),
`tests/fijos/servicios/` (18 salidas grabadas y binarios falsos),
`tests/servicios_servidor.rs` (21), `tests/servicios_ui.rs` (41),
`tests/servicios_e2e.rs` + `tests/servicios_e2e/` (1, con binarios falsos),
`tests/redimensionado/vistas_o.rs` y 26 instantáneas nuevas,
`tests/comun/mod.rs` (PATH de binarios falsos por host de prueba) y los
ajustes de pruebas existentes que pedían los formatos y el protocolo nuevos
(`tests/{almacen,protocolo,sincronizacion_elementos,redimensionado/vistas_b,snapshots}`).

**Documentación, versión y Android:** `Cargo.toml`/`Cargo.lock`,
`assets/man/magi.1`, `CHANGELOG.md`, `README.md`, `deny.toml`,
`docs/formato-sincronizacion.md`, `docs/fase13/magi-fase13-checklist.md`,
`docs/fase13/magi-fase13-revision-adversarial.md`, `android/CHANGELOG.md`,
`android/app/src/test/.../PruebaConversiones.kt`,
`android/app/src/androidTest/.../PruebaHumoNucleo.kt`,
`android/app/src/androidTest/.../PruebaFormatoSincronizacionAndroid.kt` y la
fixture `.age` del formato 3.

## Decisiones técnicas tomadas durante el desarrollo

- **Contrato antes que pistas (T63):** tipos, rangos, migración, configuración y
  mensajes se fijaron en un commit compilable antes de repartir el trabajo; las
  pistas ampliaron sin romper el contrato.
- **La lógica de servicios vive en `magi-nucleo`** (tipos, órdenes, lectura,
  acciones permitidas, validadores y límites) para que la F14 del móvil la
  reutilice sin rehacerla; el servidor solo custodia y la vista solo pinta.
- **Los topes de la entrada no fiable están en un solo sitio**
  (`limites.rs`), compartidos por servidor, cliente y script (por argumentos).
- **Migración 9 como bloque propio** con `pragma_update` a 9, sin tocar
  migraciones ya aplicadas; `COMPROBACIONES_SERVICIO` nace con `uuid`,
  `version` y `vector` y las comprobaciones del host se borran con lápida.
- **Clave `(cliente_id, peticion_id)`** para acciones y suscripciones de log
  (corrección adversarial), igual que la F12 para escritorios, para que dos
  ventanas no compartan estado.
- **La `ConexionTomada` se guarda en la suscripción de log** y se suelta al
  quitarla del mapa en todas las ramas, como hacen túneles y escritorios, para
  que el pool no recicle la conexión mientras el seguimiento está abierto.
- **El servidor arma las pestañas con orden**: el cliente manda solo un tipo
  cerrado (`OrdenPestana`) y el servidor compone el comando inicial que se
  escribe en la pestaña; nunca recibe texto para ejecutar.
- **Una acción a la vez por objetivo**, comprobada en el servidor con el mapa de
  acciones; el cliente además bloquea la tecla para no lanzar dos.
- **La comprobación de configuración va dentro de la acción**, en el mismo canal
  y justo antes, con plazo de 20 s; si falla (también 127) no se ejecuta nada.
- **Los fallos del sondeo no cambian el estado del host** (solo los vigilados),
  para no alterar lo que aprueba MELCHIOR-1; se guardan en `fallos_json`.
- **El `formato` 3 se escribe solo si hay comprobaciones propias** (si no, 1 o
  2 como en la F12), de modo que un equipo con MAGI 1.2 sigue leyendo los
  inventarios sin comprobaciones y ve `formato_nuevo` en cuanto las hay.
- **Avisos de validación de `[servicios]` al cargar `config.toml`**: un valor
  fuera de rango no impide arrancar, se acota donde se usa y se avisa al
  usuario con el motivo (era el mecanismo de «validados con error claro»).
- **La fecha de la versión es una constante** (`FECHA_DE_VERSION`), no una
  fecha de compilación, para que los recursos sean deterministas.
- **Revisión adversarial obligatoria antes de cerrar**, con informe propio y
  pruebas de regresión para los dos hallazgos críticos.

## Funcionalidades del checklist completadas (copiando su texto exacto)

Las 95 funcionalidades del checklist de la Fase 13 quedan completadas; se
copian aquí con su texto exacto:

- `crates/magi-nucleo/src/servicios/` con los tipos (objetivo, acción, listado, privilegio, resultado, orden de pestaña), la lectura de las salidas, las acciones permitidas, los validadores y los límites, sin depender de la TUI ni del servidor
- Script embebido `crates/magi-nucleo/scripts/servicios.sh` (`include_str!`) con las subórdenes `privilegio`, `sudo`, `listar`, `consumo`, `detalle`, `accion` y `logs`, que vuelve a validar sus argumentos
- Nombres validados en Rust: unidades `[A-Za-z0-9@._-]+` acabadas en `.service` o `.timer`, contenedores por id o nombre, proyectos `[a-z0-9][a-z0-9_-]*` y motor `docker` o `podman`; las unidades con otros caracteres se ven, pero sin acciones
- Lectura con `systemctl show` (clave=valor) y con los JSON de Docker, Podman y `journalctl -o json`, probada con salidas grabadas de Debian 12, Ubuntu 24.04, Rocky 9 y Arch, Docker 27 y Podman 5
- Límites de la entrada no fiable (2 MiB por listado, 2 000 unidades, 500 contenedores, 100 proyectos, texto de unidad o de `inspect` hasta 256 KiB, líneas de log hasta 16 KiB): lo que se pasa se corta con aviso y nunca hay pánico
- Privilegio detectado por host con `LC_ALL=C`: `root`, `sudo_sin_clave`, `sudo_con_clave` o `sin_sudo`
- Las órdenes privilegiadas usan sudo orden a orden (nunca `sudo sh`), con `timeout` por fuera de sudo
- Vista nueva en `F9`, y `u` desde Flota, Hosts y la ficha (fuera de un campo) la abre con ese host elegido
- Columna de hosts solo de tipo `ssh`, con el estado de su último sondeo y `⚠N` de fallos, y «Toda la flota» arriba
- Secciones Servicios, Timers y Contenedores con su recuento, que se eligen con `1`, `2` y `3`; `Tab` cambia entre hosts y lista
- Servicios con unidad, estado (glifo y texto), arranque y desde; la descripción aparece a partir de 120 columnas
- Los vigilados (`★`) y los fallidos van arriba, los fallidos primero, y el resto debajo por orden alfabético, separados por una línea
- Se listan los servicios cargados y los instalados sin cargar; las plantillas (`@.service`) no, sus instancias sí
- Timers con próxima y última ejecución, el servicio que activan y su estado
- Contenedores agrupados por proyecto Compose (y «sin proyecto») con estado, código de salida, salud, imagen y puertos; la cabecera dice el motor y su versión
- CPU y memoria de los contenedores en marcha, pedidas aparte y solo con la sección Contenedores a la vista, sin frenar el listado
- `/` filtra por nombre en la sección visible
- Detalle (`↵`) de un servicio: estado y subestado desde cuándo, arranque, PID principal, memoria, fichero de la unidad, si admite recargar, dependencias («necesita» y «la necesitan») y el texto de `systemctl cat`
- Detalle de un contenedor (id, imagen, creado, estado y código, reinicios, salud, puertos, montajes, redes, proyecto y orden, con el texto de `inspect`) y de un timer (próxima y última ejecución, servicio que activa y unidad)
- Host sin systemd: «sin systemd», con solo los contenedores; systemd anterior a 230: «systemd demasiado antiguo»
- Sin Docker ni Podman, la sección Contenedores lo dice; con los dos, se usa el primero de `[servicios] contenedores` y la cabecera dice cuál
- Sin permiso para el diario del sistema o para Docker: aviso con la instrucción y `!` para dar la contraseña de sudo
- Refresco al entrar, con `r` y cada `refresco_seg` (10) mientras la vista está abierta, sin solapar refrescos y sin diálogos; al salir no queda nada en marcha
- Si la conexión se cae, el último listado se queda con su hora y «desconectado · r reconectar»
- Tamaño mínimo y modo estrecho (un panel con `Tab`) declarados en `ui/disposicion.rs`; instantáneas de la vista, el detalle, el panel de logs y los diálogos a 40×12, 80×24 y 200×60 (T45)
- El sondeo (F2) cuenta los servicios fallidos y los contenedores caídos (salieron con código distinto de 0, se reinician una y otra vez o están `unhealthy`), con hasta 20 nombres y sin sudo
- Migración 9: SONDEOS gana `fallos_json`, nulo en los sondeos anteriores; `user_version` 9
- Estos fallos no cambian el estado del host: solo lo cambian los servicios vigilados, como en la F2
- Flota marca `⚠N` en la fila del host y enseña el bloque FALLOS en su detalle
- «Toda la flota» junta los fallos de todos los hosts, del último sondeo de cada uno y con su hora, sin abrir conexiones; `↵` va al host con el elemento elegido y `r` sondea todos
- Arrancar (`A`), parar (`P`), reiniciar (`R`), recargar (`G`) y activar o desactivar el arranque (`H`), ofrecidas solo cuando valen en el estado de la unidad
- Tras una acción, MAGI espera al estado final hasta `espera_seg` (30) y vuelve a mirar a los 2 s; un `oneshot` que termina bien cuenta como hecho
- Si la unidad no vuelve a `active`, el resultado es «fallido» con las últimas 20 líneas del diario; si se acaba el plazo, «sin confirmar», y la orden sigue en el host
- En un timer, `A` lanza ahora el servicio que activa, con confirmación
- Una sola acción a la vez por objetivo: otra sobre el mismo se rechaza con «hay otra acción en curso»
- Unidades críticas (`[servicios] criticas`: sshd, red, cortafuegos, Tailscale, Docker…) con el aviso en rojo «puede cortar tu conexión con <host>» y los hosts que van a través de él
- Diálogo de resultado (hecho, fallido, sin confirmar, rechazada por la configuración o error) y refresco inmediato después
- Arrancar, parar y reiniciar un contenedor según su estado; tras arrancar o reiniciar, espera a `running` y, si tiene healthcheck, a que deje de estar `starting`
- Proyecto Compose: arrancar, parar y reiniciar el proyecto entero con `-p` y los ficheros de sus etiquetas; sin ficheros conocidos, no se ofrecen y se dice por qué
- `S` abre en Sesión una pestaña con una shell en el contenedor (`exec -it`, con bash si lo hay y si no sh), que se cierra al salir
- Con Podman sin root se ven los contenedores del usuario SSH, y con Podman nunca se usa sudo
- Parar, reiniciar y desactivar un servicio, y parar o reiniciar un contenedor o un proyecto, pasan por la deliberación MAGI con las comprobaciones activas del host (`PlanDeliberado::Servicio`)
- MELCHIOR-1 no cuenta el estado del propio objetivo al mirar la salud del host
- Arrancar, recargar, activar el arranque y lanzar un timer piden una confirmación simple (T6)
- El servidor recomprueba qué exige deliberación y valida la deliberación (que exista, sea de esa acción y no esté usada, T62); DELIBERACIONES guarda la acción y el servidor, su resultado
- Sin comprobaciones activas, la deliberación dice «sin comprobaciones configuradas» y sigue pidiendo `Ctrl+K`, como en la F6
- Si una acción necesita sudo con contraseña y no está en memoria, el cliente la pide en un diálogo y la comprueba con `ComprobarSudo` antes de confirmar o deliberar, con 3 intentos
- La contraseña viaja en `Zeroizing`, llega al host por la entrada estándar (nunca en la línea de órdenes) y nunca va a disco, al llavero, al log ni a REGISTRO
- Se recuerda en memoria del cliente, por host, hasta `sudo_memoria_min` (5) minutos sin uso o hasta salir de la vista
- `!` pide la contraseña de sudo para el host elegido, para ver lo que exige permisos
- Un usuario sin sudo ve «usuario no puede usar sudo en <host>» y no se le ofrecen las acciones que lo necesitan
- Antes de recargar o reiniciar Nginx, Caddy, Apache o HAProxy, y de reiniciar un proyecto Compose, se valida la configuración en el mismo canal, con los mismos privilegios y justo antes de la acción
- Caddy y HAProxy sacan la ruta de su configuración del `ExecStart` de la unidad o, si no está, usan la de siempre
- Comprobación propia por servicio o proyecto (COMPROBACIONES_SERVICIO, migración 9), que sustituye a la conocida y se ejecuta con `sh -c` y los privilegios de la acción
- Las comprobaciones propias se crean, se editan y se borran en la ficha del host y con `c` sobre el servicio o el proyecto en `F9`; borrar pide confirmación
- Un código distinto de 0 (también 127, no encontrada) o 20 s sin terminar paran la acción
- `l` abre el panel de logs de un servicio, del servicio de un timer o de un contenedor, con las últimas `lineas_iniciales` (200) y en vivo
- El servidor manda lotes cada 100 ms (hasta 500 líneas) y, si el cliente no da abasto (cola de más de 4 MiB), descarta las más viejas y las cuenta
- Anillo de `lineas_log` (10 000) en el cliente; el panel dice cuántas líneas hay y marca dónde se omitieron
- Filtro por texto (`/`) y por nivel (`n`: todo, info, aviso, error) en el diario; en contenedores, todo o solo stderr
- `Espacio` pausa y sigue sin cortar la llegada de líneas; desplazarse hacia arriba pausa, y `Fin` vuelve al final
- `t` abre en una pestaña el seguimiento completo (`journalctl -fu` o `docker logs -f`)
- Al cerrar el panel, cambiar de host o salir de la vista, `PararLog` cierra el canal y el script corta el seguimiento en el host
- Si el proceso remoto termina, se cae la conexión o falta permiso, el panel lo dice y conserva las líneas
- Todo lo remoto lo hace el servidor sobre el pool (`conexion_para_canal`, `UsoCanal::Servicios`) y suelta la conexión por identidad (T34)
- Plazos: 10 s por listado, consumo o detalle; 20 s por comprobación; `espera_seg` + 30 s por acción; los logs siguen mientras dura la suscripción
- Las acciones en curso se custodian: siguen si se cierra la ventana, salen en `AccionesServicio{lista}` y cuentan en `TrabajoServidor.acciones_servicio` (T67)
- Pestañas con orden: `AbrirSesion` gana `orden` (`ShellContenedor`, `SeguirDiario`, `SeguirContenedor`) y el servidor arma la orden, sin recibir nunca texto libre
- `peticion_id` de servicios en su propio rango, desde 2^45
- `VERSION_PROTOCOLO = 8`; `Hola` y `VersionIncompatible` sin cambios (T66)
- REGISTRO `servicio_accion` con objetivo, acción, resultado, duración y deliberación, tras la `Barrera` (T47); nunca la salida de las órdenes
- `config.toml` `[servicios]`: `refresco_seg`, `espera_seg`, `lineas_log`, `lineas_iniciales`, `contenedores`, `criticas` y `sudo_memoria_min`, validados con error claro
- COMPROBACIONES_SERVICIO viaja como elemento propio (`comprobacion_servicio`) con `uuid`, `version` y `vector`, y borrar una deja lápida
- El fichero sube a `formato` 3 solo si hay alguna comprobación propia; si no, sigue en 2 o en 1, como en la F12
- Al guardar la primera comprobación propia, MAGI avisa de que los equipos con MAGI 1.2 dejarán de leer este equipo
- MAGI 1.3 lee los formatos 1, 2 y 3, y un equipo con 1.2 ve el 3 como `formato_nuevo`
- `docs/formato-sincronizacion.md` documenta el `formato` 3 y el elemento nuevo
- La app Android lee el `formato` 3 y guarda las comprobaciones en su copia sin usarlas; `android/CHANGELOG.md` lo recoge en «Sin publicar»
- `ListarServicios` → `Servicios`
- `PedirConsumo` → `Consumo`
- `DetalleServicio` → `Detalle`
- `ComprobarSudo` → `SudoComprobado`
- `AccionServicio` → `AccionServicioHecha`
- `AccionesServicio{lista}`
- `SeguirLog`, `LineasLog`, `PararLog` y `FinLog`
- `AbrirSesion` con `orden`
- Pruebas del núcleo con salidas grabadas: unidades, timers, contenedores, proyectos, detalle y diario; límites y nombres inválidos
- Pruebas del script con el servidor SSH de prueba y `systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos: argumentos, privilegios, contraseña por la entrada estándar, comprobación y espera
- Pruebas del servidor: refrescos sin solapes, acciones custodiadas, deliberación exigida, sudo, plazos, y logs con contrapresión y parada
- Prueba de extremo a extremo sin red externa: vista → servidor → host de prueba, con un reinicio que falla y sus últimas líneas, y un panel de logs que al cerrarse para el seguimiento
- Pruebas de sincronización: formatos 1, 2 y 3, `formato_nuevo` y la comprobación de un host que ya no existe
- `shellcheck` en verde para `servicios.sh` y para el `sondeo.sh` ampliado
- Versión 1.3.0: `Cargo.toml`, fecha de la página man, `assets/` regenerados y `CHANGELOG.md`
- README con la sección «Servicios, contenedores y logs»: vista, acciones, deliberación, sudo, comprobaciones y permisos en el host
- Revisión adversarial de cierre (T31, T54) del servidor de servicios, el script, los logs y el manejo de sudo, anotada en el informe de implementación


## Pendientes y bloqueos

- **Guion de validación manual del informe §11** (12 pasos): pendiente de que el
  desarrollador lo ejecute en sus hosts (Nginx roto, sudo con contraseña,
  Docker Compose, Podman sin root, ssh.service, cierre de ventana a media
  acción, «Toda la flota», comprobación propia sincronizada, etc.). Es el
  pendiente externo principal de la fase.
- **App Android:** las pruebas de la JVM pasan y las instrumentadas compilan,
  pero `PruebaFormatoSincronizacionAndroid` y la de humo actualizada (3 / 9) no
  se han ejecutado en un emulador en esta sesión. Hay que pasarlas con
  `./gradlew connectedDebugAndroidTest` (emulador x86_64 API 30) antes de dar
  por validada la lectura del formato 3 en el móvil.
- **Host sin systemd:** la vista muestra el aviso «sin systemd: solo se ven los
  contenedores» y las secciones 1 y 2 quedan seleccionables aunque vacías
  (cosmético; no afecta a ninguna AC).
- **Funcionalidades de fases anteriores:** el total del proyecto queda en
  1 053 / 1 057; las 4 pendientes son de fases previas (sus guiones de
  validación y la R117 de los escritorios), no de la Fase 13.
- **Revisión adversarial:** vista `docs/fase13/magi-fase13-revision-adversarial.md`.
  Los dos críticos, los dos altos, los cinco medios y los bajos B1-B3 y B5-B8
  están corregidos y con prueba de regresión; B4 queda como desviación
  documentada (punto 1 de desviaciones). El revisor dejó sin revisar la lectura
  línea a línea de todo `src/ui/servicios.rs` y de las instantáneas, la
  integración visual de Flota/Hosts/ficha contra el mockup, `android/`,
  `docs/formato-sincronizacion.md`, el rendimiento real y el guion con hosts
  reales; se detalla en el §9 de su informe.
- **Especificación (analista):** la revisión recomienda reflejar en la próxima
  revisión del informe funcional cuatro criterios que la especificación no
  fijaba y que la implementación resuelve así: anillo de logs con presupuesto de
  8 MiB, lote de `LineasLog` acotado por tamaño codificado (2 MiB), caducidad de
  la contraseña de sudo en la que las lecturas no cuentan como uso, y la
  limitación ya descrita del sondeo (B4, punto 1 de desviaciones). Ninguno
  cambia el alcance del checklist.
- **Publicación:** los binarios publicados salen de CI a partir de un tag y las
  etiquetas y secretos los pone el desarrollador; esta sesión deja el árbol
  preparado, no publica. `android/version.properties` no se toca.
- **Aviso externo de dependencias:** RUSTSEC-2026-0441 (`pin-utils` sin
  mantener) se publicó después de la Fase 13; se añadió la excepción justificada
  en `deny.toml` y se revisará al subir IronRDP/sspi.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

**Arrancar la vista.**

```sh
cargo run                  # TUI; F9 abre Servicios y u desde Flota, Hosts o la ficha
cargo run -- --servidor    # servidor de sesiones, en otra terminal
```

`F9` lista servicios, timers y contenedores del host SSH elegido; `u` desde
Flota/Hosts/ficha abre la vista con ese host. Todo lo remoto lo ejecuta el
servidor con `crates/magi-nucleo/scripts/servicios.sh`.

**Migrar la base de datos.** Al abrir `magi.db` por primera vez con el binario
1.3.0 se aplica la migración 9 (`COMPROBACIONES_SERVICIO` y
`SONDEOS.fallos_json`) y la base queda en `user_version` 9; una `magi.db` en 9
no la abre un binario 1.2. `magi diagnostico` muestra la versión y el
`user_version`. La importación de inventario ya hace copia previa de `magi.db`.

**Pruebas (sin red externa).**

```sh
# Suite completa (la que se ejecutó para cerrar la fase)
TZ=UTC cargo test --workspace --locked

# Pruebas de la fase, por paquete
cargo test -p magi-nucleo --test servicios_lectura --test servicios_script     --test sincronizacion_comprobaciones
TZ=UTC cargo test -p magi-ssh --test servicios_servidor --test servicios_ui     --test servicios_e2e --test redimensionado

# Calidad obligatoria antes de commit
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo deny check
shellcheck scripts/*.sh crates/magi-nucleo/scripts/*.sh src/flota/sondeo.sh
```

Resultados de la sesión: 49 binarios de prueba, **1 570 pruebas en verde y 0
fallos** (incluidas `servicios_lectura` 31, `servicios_script` 32,
`sincronizacion_comprobaciones` 17, `servicios_servidor` 23, `servicios_ui` 46,
`servicios_e2e` 1, `redimensionado` 303, la biblioteca de `magi-ssh` 482 y la de
`magi-nucleo` 158); `cargo clippy -D warnings`, `cargo fmt --check`, `cargo
deny check` y `shellcheck` limpios; `cargo test -p magi-ssh --test recursos` en
verde con `assets/` regenerado. La prueba de logs que supera la gracia del pool
tarda ~40 s a propósito.

**App Android.** Con SDK, NDK r30 y `cargo-ndk`:

```sh
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.16248370"
cd android
./gradlew lint testDebugUnitTest          # JVM: pasa (incluye el contrato del formato 3)
./gradlew connectedDebugAndroidTest       # emulador x86_64 API 30: pendiente de ejecutar
./gradlew compileDebugAndroidTestKotlin   # compila (hecho)
```

**Generar la fixture del formato 3** (solo si cambia el formato):

```sh
cargo test -p magi-nucleo --test generar_fixture_android -- --ignored
```
