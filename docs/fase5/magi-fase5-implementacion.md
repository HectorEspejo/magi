# MAGI - Fase 5: Túneles

## Informe de Implementación

**Última actualización:** 15 de septiembre de 2026 (sesión 1: fase completa)

---

## Resumen de lo implementado

La Fase 5 está **implementada al completo** (54/54 del checklist). MAGI gana los túneles
SSH de los tres tipos —locales (`-L`), remotos (`-R`) y dinámicos (`-D`, SOCKS5)— como
filas de la tabla `TUNELES` con interruptor, **ejecutados por el servidor de sesiones**
sobre la conexión del pool del host, de modo que sobreviven a cerrar la ventana, y
gobernados desde la vista **Túneles** (`F6`).

- **Migración 4** (`src/almacen/migraciones.rs`): tabla `TUNELES` (host, nombre, tipo,
  escucha, destino, automático, marcas de tiempo) con `UNIQUE(host_id, nombre)` y cascada
  por host; sin `CHECK` sobre `tipo`, la validación va en código
  (`src/almacen/tuneles.rs`, `src/modelo.rs`). El CRUD lo hace el cliente (D52) y el
  servidor solo lee la tabla con su conexión de solo lectura.
- **Conversión de ssh_config** (`src/sshconfig/tuneles.rs`): `LocalForward` →
  local, `RemoteForward` → remoto, `DynamicForward` → dinámico, con bind por defecto
  `127.0.0.1`, nombres `local-<puerto>` / `remoto-<puerto>` / `socks-<puerto>` y
  `automatico = 1` como hace `ssh`. La importación crea filas en `TUNELES` en vez de
  dejarlas en `opciones_extra`, la exportación las devuelve a `magi_config` y las tres
  directivas pasan a estar prohibidas a mano en `opciones_extra`. La ficha avisa de los
  reenvíos que queden en opciones extra y `i` los importa.
- **Protocolo v3** (`src/protocolo.rs`): `ActivarTunel`, `PararTunel`, `RelanzarTunel`,
  `RecargarTuneles` y la difusión `Tuneles{lista}`; `Bienvenida` lleva los túneles. Un
  servidor o un cliente de la versión anterior no cooperan.
- **Servidor** (`src/servidor/tuneles.rs`, `socks5.rs`, `conexion/reenvios.rs`): un
  `TunelActivo` por túnel levantado, con conexión tomada del pool o abierta con el flujo
  de la Fase 3 (diálogos al solicitante) y devuelta al pool por identidad en todas las
  ramas de error. Local con `TcpListener` + `direct-tcpip`; dinámico con SOCKS5 propio
  (solo `CONNECT`, sin autenticación, dominio sin resolver en local); remoto con
  `tcpip_forward` y rechazo de todo canal `forwarded-tcpip` cuya clave no esté registrada
  para ese host. Contadores de bytes por dirección, conexiones abiertas y aceptadas;
  estados `activando | activo | parando | caido`; ciclo automático ligado a la primera y
  la última pestaña o canal SFTP; corte de las conexiones abiertas al parar; difusión
  inmediata en cada cambio de estado y de los contadores como mucho dos veces por segundo
  y solo si cambian.
- **Registro**: `tunel_abierto`, `tunel_cerrado` (con motivo y totales) y `tunel_fallido`,
  dentro del filtro «conexiones».
- **Cliente**: vista `F6` con tabla, filtro, alta/edición/borrado, relanzado, detalle y
  panel inferior; diálogo con host, nombre, tipo, escucha, destino, casilla de automático y
  avisos en ámbar; bloque «Túneles» en la ficha; glifo `⇅` en Hosts y Flota; «túneles N» en
  la barra de Sesión; entradas de paleta; ayuda `?`.
- **CLI**: `magi tunel activar|parar <host> <nombre>`, `magi tuneles` y los túneles en
  `magi servidor estado`; el servidor no se apaga por inactividad con túneles levantados.

## Desviaciones respecto a la especificación (qué y por qué)

1. **El puerto 0 se admite en la escucha como «el que quede libre».** El checklist pide
   «escucha `dirección:puerto` (1-65535)» (línea 6) y a la vez «puerto 0 en local/dinámico
   asigna uno libre y se muestra sin persistir» (línea 34) y «`tcpip_forward` devuelve el
   puerto real (útil con puerto 0)». Para que las tres cosas sean verdad, la validación
   acepta `0` con ese significado y rechaza el resto fuera de rango. Consecuencia: dos
   túneles con la misma escucha en puerto 0 **no** se consideran duplicado (no se pisan:
   cada uno recibe un puerto distinto). El puerto asignado vive solo en memoria
   (`escucha_efectiva`), nunca se escribe en `TUNELES`.
2. **`Tuneles{lista}` lleva solo lo que el servidor tiene en memoria** (activos, activando,
   parando y caídos), no todas las filas de `TUNELES`: las filas las conoce el cliente por
   su propia lectura de la tabla (D52) y las cruza por `tunel_id`. La vista pinta una fila
   por túnel definido y le pone el estado que venga de la difusión (inactivo si no está).
   Así la difusión no necesita leer SQLite dos veces por segundo.
3. **La tabla de `F6` son columnas hechas a mano.** En todo el proyecto no se usa ni un
   `Table` de ratatui; la vista sigue el patrón de Transferencias y Archivos
   (`Paragraph` + `Line` + `title_bottom`).
4. **El bloque «Túneles» de la ficha tiene su propia región**, entre los campos y las dos
   áreas de texto, en vez de estirar los 17 renglones de campos: con 16 justos, estirar
   dejaba el bloque pegado al borde o recortado. Además de `n`/`e`/`x` (línea 59) el bloque
   admite `a` para alternar automático, que es lo que muestra el `[a]` de la maqueta del
   informe; `Espacio` y `r` siguen solo en `F6`.
5. **Interacción conocida con el pool de la Fase 3 (acordada con el desarrollador).** Con
   `multiplexar`, abrir una pestaña nueva a un host no reutiliza la conexión del pool: la
   sustituye y desconecta la anterior (`abrir_y_servir`/`Pool::guardar`), así que un túnel
   activo de ese host pasa a `caido` con el motivo «se cayó la conexión con el host» y `r`
   lo relanza (D55: sin reintento automático). Se decidió no tocar la Fase 3 y dejarlo
   anotado aquí. Afecta también a dos pestañas entre sí, y es anterior a esta fase: ver el
   hallazgo de la F3 en «Pendientes».
6. **Dos arreglos fuera del checklist, autorizados por el desarrollador:**
   - `sesiones::caida()` ponía `handle = None` sin liberar el pool, de modo que la entrada
     quedaba contada para siempre y `revisar_pool` no la recogía nunca. Ahora libera **por
     identidad** (`liberar_si_es`).
   - `servidor::ejecutar` liberaba el pool con `liberar` a ciegas; ahora usa `liberar_si_es`
     por identidad, para no descuadrar el contador de un túnel activo.
7. **Un tipo de túnel desconocido en la tabla deja de interpretarse como local.** El
   servidor ejecuta lo que lee y `TUNELES` la escribe el cliente: una fila con un `tipo`
   que no se reconoce (base tocada a mano, cliente de otra versión) hacía que un reenvío
   remoto se ejecutara como un `TcpListener` en la máquina del usuario. Ahora esa fila es
   ilegible (se avisa y el host se queda sin túneles) y, al levantar, se valida también el
   destino: sin él, el túnel queda caído con motivo en vez de aceptar conexiones y no
   llevarlas a ninguna parte.
8. **Un automático parado a mano no vuelve hasta el ciclo siguiente** (línea 39): se apunta
   el `(host, túnel)` en `EstadoServidor.tuneles_parados` y se olvida cuando el host se
   queda sin pestañas ni SFTP. Sin ese apunte, cualquier cambio del ciclo lo resucitaba.
9. **`Parando` se enseña de verdad**: `PararTunel` marca el estado, difunde y solo después
   cierra (listener o reenvío, conexiones abiertas, canal del pool) y vuelve a difundir.
10. **La CLI no anota en `REGISTRO`**: el efecto (túnel arriba o abajo) lo anota el
    servidor, que es quien lo produce; la orden no es el efecto.
11. **Los túneles no se apagan con la ventana ni el servidor con ellos**: `revisar_inactividad`
    cuenta los túneles en marcha y `apagar_limpio` los para (anotando sus totales) antes de
    cerrar nada.
12. **Lo que `ssh` no sabría leer no se exporta** (solo el puerto 0 en local/dinámico, que sí
    vale al vuelo en MAGI): en su lugar queda un comentario en `magi_config`. El informe no lo
    decía, pero sin esto una sola línea inválida dejaría al usuario sin `ssh` para todos los
    hosts (los ficheros de `~/.ssh` se incluyen desde `~/.ssh/config`).
13. **En `opciones_extra` solo se rechazan los reenvíos que MAGI sabe representar.** El
    checklist dice que las tres directivas pasan a ser gestionadas, y lo son; pero `ssh` admite
    formas que MAGI no representa (varios destinos en una directiva, reenvíos a un socket
    local) y prohibirlas dejaría hosts importados imposibles de guardar.
14. **Al importar, un reenvío que no se puede convertir en túnel no se pierde**: vuelve tal
    cual a `opciones_extra` y la importación lo avisa (antes se quedaba solo en el log).

## Estructura de archivos creada/modificada

**Nuevos**

| Fichero | Contenido |
|---|---|
| `src/almacen/tuneles.rs` | CRUD de `TUNELES`, rechazo de duplicados de escucha y validación en código |
| `src/sshconfig/tuneles.rs` | `LocalForward`/`RemoteForward`/`DynamicForward` ⇄ túnel (conversión pura) |
| `src/servidor/tuneles.rs` | `TunelActivo`, activar/parar/relanzar/recargar, ciclo automático, revisora |
| `src/servidor/socks5.rs` | Saludo y `CONNECT` de SOCKS5 (sin autenticación, dominio sin resolver) |
| `src/conexion/reenvios.rs` | Registro de reenvíos remotos por host, contadores y copia de 64 KiB |
| `src/ui/tuneles.rs` | Vista `F6` |
| `tests/tuneles.rs` | Pruebas de extremo a extremo de los tres tipos y del ciclo automático |
| `docs/fase5/magi-fase5-implementacion.md` | Este informe |

**Modificados**

| Fichero | Qué cambió |
|---|---|
| `src/almacen/migraciones.rs` · `src/almacen/mod.rs` | Migración 4 y fachada de túneles |
| `src/modelo.rs` | `TipoTunel`, `Tunel`, `DatosTunel`, validación y direcciones `[dir:]puerto` |
| `src/registro.rs` | `tunel_abierto`, `tunel_cerrado`, `tunel_fallido` y el filtro «conexiones» |
| `src/protocolo.rs` | `VERSION_PROTOCOLO = 3`, mensajes y `InfoTunel` |
| `src/servidor/mod.rs` | Estado, despacho, apagado, `servidor estado` y enganches del ciclo |
| `src/servidor/conexiones.rs` | `conexion_para_canal` y `soltar` compartidos con SFTP |
| `src/servidor/sesiones.rs` · `src/servidor/sftp.rs` | Enganches del ciclo automático y liberación del pool por identidad |
| `src/conexion/cliente.rs` · `src/conexion/salto.rs` | Handler `forwarded-tcpip` (rechaza claves no registradas) y registro en el contexto |
| `src/app.rs` | Vista, diálogo, paleta, acciones y bloque de la ficha |
| `src/ui/{ficha,dialogos,hosts,flota,sesion,barra,ayuda,mod}.rs` · `src/tema.rs` | Interfaz |
| `src/main.rs` | `magi tunel activar|parar` y `magi tuneles` |
| `tests/comun/mod.rs` | El host de pruebas concede o rechaza reenvíos y abre destinos como un `sshd` |
| `tests/{almacen,servidor,sftp,password,sshconfig_ida_vuelta}.rs` | Pruebas nuevas y ajustes |
| `README.md` | Sección de túneles, atajos y aviso de seguridad |

## Decisiones técnicas tomadas durante el desarrollo

1. **Registro de reenvíos a nivel de proceso, por `(host, dirección, puerto)`**
   (`src/conexion/reenvios.rs`), no por conexión: un túnel remoto puede vivir sobre una
   conexión del pool que abrió una pestaña o el SFTP, así que el handler que recibe el canal
   `forwarded-tcpip` no tiene por qué ser el de la conexión que pidió el reenvío. La búsqueda
   prueba la clave exacta y, si no está, por puerto cuando en ese host solo haya uno (el host
   puede contestar con `0.0.0.0` o `localhost`); si hay más de uno, se rechaza en vez de
   adivinar.
2. **El handler de russh por defecto acepta los canales `forwarded-tcpip`**: hay que
   sobrescribirlo o el requisito «clave no registrada se rechaza» no se cumple.
3. **`tcpip_forward` devuelve 0 cuando se pidió un puerto concreto.** OpenSSH solo contesta
   con el puerto si se le pidió el 0, y russh traduce esa respuesta vacía a `Some(0)`. Dar
   ese 0 por bueno dejaba el reenvío registrado en un puerto inexistente: los canales del
   host se rechazaban y la escucha remota no se podía cancelar. Ahora se usa el puerto
   pedido cuando no es 0 (lo encontró la revisión adversarial; hay prueba de regresión).
4. **Contadores con atómicos compartidos** entre las tareas que copian y difusión por
   comparación de listas: la revisora arma la lista cada 500 ms y solo la manda si cambió
   respecto a la última enviada. Los cambios de estado difunden al momento.
5. **Cortar las conexiones al parar** con un `watch` por túnel: sin él, con la conexión del
   pool viva (host que multiplexa), las copias seguían llevando tráfico de un túnel que ya
   no existía y sin contarlo (hallazgo de la revisión; hay prueba de regresión con un host
   que multiplexa).
6. **`conexion_para_canal` se extrajo a `conexiones.rs`** y lo comparten los canales SFTP y
   los túneles: evita que las dos copias del «toma del pool o abre con los diálogos» se
   desvíen.
7. **Cerrojo por host** al levantar túneles: dos túneles del mismo host activados a la vez
   abrirían dos conexiones y, con `multiplexar`, la segunda desplazaría a la primera del
   pool y la mataría.
8. **`Contadores` lleva también el último error de una conexión suelta** (un canal que el
   host rechaza): no tumba el túnel, pero se enseña en el detalle.
9. **Las filas se validan al ejecutarlas** (ver desviación 7).
10. **Plazos**: 10 s para pedir y cancelar reenvíos, abrir canales y conectar destinos; la
    revisora corre cada 500 ms. Ninguna espera de red del servidor queda sin plazo.
11. **El almacén se lee sin el mutex del estado en la mano** en `activar` y `recargar`, para
    que una lectura de SQLite lenta no pare el bucle de mensajes ni la difusión.

## Funcionalidades del checklist completadas (copiando su texto exacto)

### Almacén y modelo

- [x] Migración 4 por `PRAGMA user_version`: tabla `TUNELES` (id, host_id FK CASCADE, nombre, tipo, escucha, destino, automatico, creado_en, actualizado_en) con `UNIQUE(host_id, nombre)`
- [x] `almacen/tuneles.rs`: CRUD desde el cliente; al borrar un host sus túneles caen en cascada
- [x] Sin `CHECK` sobre `tipo`; validación en código: nombre `[A-Za-z0-9._-]+` único por host, tipo `local | remoto | dinamico`, escucha `dirección:puerto` (1-65535), destino obligatorio salvo en dinámico
- [x] Rechazo de duplicados: mismo tipo y misma escucha en local/dinámico (cualquier host) o en remoto (mismo host)
- [x] Tipos nuevos en `REGISTRO`: `tunel_abierto`, `tunel_cerrado` (con motivo y totales), `tunel_fallido`; el filtro `t` de la vista Registro los incluye en «conexiones»
- [x] Tests del almacén: migración 3→4, cascada por host, unicidad por host

### ssh_config

- [x] `sshconfig/tuneles.rs`: `LocalForward [bind:]puerto host:hostport` → local, `RemoteForward` → remoto, `DynamicForward [bind:]puerto` → dinámico; bind por defecto `127.0.0.1`; nombres `local-<puerto>`, `remoto-<puerto>`, `socks-<puerto>`; `automatico = 1`
- [x] La importación de `~/.ssh/config` crea filas en `TUNELES` en lugar de dejar los reenvíos en `opciones_extra`
- [x] La exportación a `magi_config` emite `LocalForward`/`RemoteForward`/`DynamicForward` por cada túnel del host
- [x] `LocalForward`, `RemoteForward` y `DynamicForward` pasan a directivas gestionadas: se rechazan al editar `opciones_extra` a mano
- [x] Ficha de host con reenvíos en `opciones_extra` (de la F1): aviso «hay N reenvíos en opciones extra · i importar a túneles»; `i` crea las filas y quita las líneas
- [x] Test de ida y vuelta ampliado: túneles de los tres tipos sobreviven a `importar(exportar(hosts))`

### Protocolo

- [x] `VERSION_PROTOCOLO = 3`; mensajes `ActivarTunel`, `PararTunel`, `RelanzarTunel`, `RecargarTuneles`, `Tuneles`, respondidos con `Hecho{peticion_id}` o `Error{peticion_id, mensaje}`; `Bienvenida` incluye los túneles activos
- [x] `Tuneles{lista}` se difunde en cada cambio de estado y, para contadores, como máximo 2 veces por segundo y solo si cambian
- [x] Tests del protocolo v3: ida y vuelta de los mensajes nuevos y rechazo de v2

### Servidor: túneles

- [x] `servidor/tuneles.rs`: `TunelActivo {tunel_id, host_id, tipo, escucha, destino, estado, origen, conexiones abiertas y aceptadas, bytes_subidos, bytes_bajados, desde, ultimo_error, solicitante}` con estados `activando | activo | parando | caido`
- [x] `ActivarTunel` lee la fila de `TUNELES` con la conexión de solo lectura, toma la conexión del pool o abre una nueva con el flujo de la Fase 3 (diálogos al solicitante) y la mete en el pool respetando `multiplexar`
- [x] Un túnel activo cuenta como canal del pool y se libera al parar o caer, por identidad y en todas las ramas de error
- [x] Túnel local: `TcpListener::bind(escucha)`; por conexión aceptada `channel_open_direct_tcpip(destino, origen)` y `copy_bidirectional` con búfer de 64 KiB; el rechazo del host cierra solo esa conexión y anota `ultimo_error`
- [x] Túnel dinámico: `servidor/socks5.rs` con saludo (método 0x00), `CONNECT` a IPv4, IPv6 o dominio, respuesta 0x05 si el host rechaza y 0x07 para BIND/UDP; el nombre se envía al host sin resolver en local
- [x] Túnel remoto: `handle.tcpip_forward(dirección, puerto)`; registro (dirección, puerto) → túnel en la conexión; `Handler::server_channel_open_forwarded_tcpip` abre `TcpStream::connect(destino)` y copia; canales con clave no registrada se rechazan; parar hace `cancel_tcpip_forward` sin afectar a las pestañas
- [x] `bind` fallido (`EADDRINUSE`, permiso, puerto < 1024) y reenvío rechazado por el host dejan el túnel `caido` con motivo legible y anotan `tunel_fallido`
- [x] `tcpip_forward` devuelve el puerto real: se muestra (útil con puerto 0); puerto 0 en local/dinámico asigna uno libre y se muestra sin persistir
- [x] `PararTunel`: `parando` → cierra listener o cancela el reenvío, corta las conexiones abiertas, anota `tunel_cerrado` con totales, libera el canal del pool
- [x] Conexión del pool caída: los túneles del host pasan a `caido` («conexión caída») y anotan `tunel_cerrado`; `RelanzarTunel` los vuelve a activar; no hay reintento automático
- [x] Automáticos: al abrir el primer canal de pestaña o SFTP de un host se activan sus túneles con `automatico = 1` que no estén activos (origen `automatico`); al cerrar el último se paran solo los de origen `automatico`
- [x] Un túnel no cuenta como pestaña ni SFTP para el ciclo automático; un automático parado a mano pasa a manual hasta el siguiente ciclo
- [x] Contadores: bytes por dirección, conexiones abiertas y aceptadas; se reinician al relanzar; el detalle de `tunel_cerrado` lleva los totales
- [x] Al borrar un host o un túnel activo, el servidor lo para antes (`RecargarTuneles`)
- [x] `magi servidor estado` lista los túneles activos con tipo, escucha, destino, host, conexiones y tráfico; el servidor no se apaga por inactividad con túneles activos
- [x] Tests contra el `sshd` efímero: local (eco por el túnel), dinámico (CONNECT a dominio e IP), remoto (petición desde el host), bind ocupado, rechazo del host, parada con conexión abierta, ciclo automático

### Vista Túneles (F6)

- [x] `F6` abre Túneles: tabla estado · tipo · escucha → destino · host · `a` (automático), ordenada por host y nombre; cabecera «N · M activos»; vacía con «Sin túneles: n para crear uno, o importa los reenvíos de tus hosts»
- [x] Glifos `●` activo, `◐` activando/parando, `○` inactivo, `✕` caído (ASCII `* o x`) con color
- [x] Navegación con `↑` `↓` / `j` `k`; `/` filtra por nombre, host, puerto o tipo
- [x] `Espacio` activa un inactivo, para un activo y descarta el error de un caído
- [x] `r` relanza un túnel caído
- [x] `n` abre el diálogo de túnel: host (desplegable), nombre, tipo (radio), escucha (dirección + puerto), destino (dirección + puerto, deshabilitado en dinámico), casilla automático; `Ctrl+S` guarda y avisa al servidor con `RecargarTuneles`
- [x] Aviso en ámbar en el diálogo con escucha `0.0.0.0` o `::` («cualquier equipo de tu red podrá usar este túnel» / «el host solo lo expondrá con GatewayPorts») y con puerto < 1024 en remoto
- [x] `e` edita (si está activo, se para primero y al guardar se ofrece relanzar); `x` borra con confirmación parando antes si está activo; `a` alterna automático
- [x] `↵` abre el detalle: desde cuándo, origen (automático / manual y ventana), conexiones abiertas y aceptadas, bytes ↑↓, último error; en caído, `r` relanzar y `Espacio` descartar
- [x] Panel inferior con el resumen del túnel seleccionado (mismos datos en dos líneas)
- [x] `q` / `Esc` vuelve dejando los túneles en el servidor

### Ficha de host y otras vistas

- [x] Bloque «Túneles» en la ficha con la lista del host (`[a]`, tipo, escucha → destino, nombre) y `n` / `e` / `x` con la misma lógica que la vista
- [x] Barra de estado de Sesión muestra «túneles N» si el host tiene túneles activos
- [x] Glifo `⇅` tras el nombre en Hosts y Flota cuando el host tiene túneles activos
- [x] Entradas de paleta: `túnel · <host> · <nombre>` (activa o para), `ir a túneles`, `nuevo túnel`
- [x] Ayuda `?` en Túneles; ninguna acción destructiva (borrar, parar con conexiones abiertas, escucha en `0.0.0.0`) sin confirmación o aviso
- [x] `F1`-`F7` todas operativas; el mensaje «vista no disponible en esta fase» desaparece

### CLI

- [x] `magi tunel activar <host> <nombre>` y `magi tunel parar <host> <nombre>` hablan con el servidor (lo autolanzan si hace falta); sin credenciales disponibles fallan con «abre una sesión desde la TUI primero»
- [x] `magi tuneles` lista túneles definidos y activos con estado, escucha, destino y tráfico

### Calidad

- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de almacén, ssh_config, protocolo y servidor
- [x] Revisión adversarial del código nuevo (servidor y cliente) antes de cerrar, con resultado en el informe de implementación
- [x] README actualizado: túneles, tipos, automáticos, `magi tunel`, aviso de `0.0.0.0`

## Revisión adversarial (T31)

Se pasaron tres revisiones independientes —servidor y capa de conexión, almacén y
ssh_config, y cliente (TUI y CLI)— sobre el código nuevo, buscando pérdida de datos,
fugas de recursos, esperas sin plazo y estado leído a destiempo. Salieron 25 hallazgos;
se corrigieron 24 y sus pruebas de regresión están en `tests/tuneles.rs`,
`tests/almacen.rs` y `tests/sshconfig_ida_vuelta.rs`.

**Servidor y conexión**

1. **(alta) El reenvío remoto con un puerto concreto se registraba en el puerto 0.** OpenSSH
   solo contesta con el puerto cuando se le pide el 0, y russh traduce esa respuesta vacía a
   `Some(0)`: el túnel quedaba «activo» con la escucha `…:0`, los canales del host se
   rechazaban por no encontrar su clave y `cancel_tcpip_forward(…, 0)` no cancelaba nada, así
   que la escucha se quedaba abierta en el host. Corregido: se usa el puerto pedido cuando no
   es 0. Regresión: `el_reenvio_remoto_con_puerto_fijo_usa_ese_puerto`.
2. **(alta) `PararTunel` no cortaba las conexiones ya establecidas.** Con la conexión del
   host viva en el pool (host que multiplexa, o pestaña abierta), las copias seguían llevando
   tráfico de un túnel que ya no existía y sin contarlo. Corregido con un aviso `watch` por
   túnel que corta las copias al parar. Regresión:
   `parar_corta_las_conexiones_aunque_el_host_multiplexe`.
3. **(media) El reenvío del host se quedaba registrado y sin cancelar** al caer el túnel, al
   descartar su error o al levantarlo y quedarse sin dueño. Corregido: se quita del registro y
   se cancela (con plazo) en las tres ramas.
4. **(media) `cancel_tcpip_forward` y `channel_open_direct_tcpip` no tenían plazo.** Un host
   en un agujero negro colgaba el apagado del servidor y el bucle de lectura de un cliente.
   Corregido: los dos van con el plazo de 10 s.
5. **(media) `activar` leía SQLite con el mutex global del servidor tomado.** Una lectura
   lenta paraba el bucle de mensajes y la difusión. Corregido: la lectura se hace fuera.
6. **(media) Un automático parado a mano volvía en el mismo ciclo.** Corregido: se apunta
   `(host, túnel)` y se olvida cuando el host se queda sin canales. Regresión:
   `un_automatico_parado_a_mano_no_vuelve_en_el_mismo_ciclo`.
7. **(media) Una activación cancelada resucitaba como activa** al terminar de abrir, ocupando
   el canal del pool sin que nadie la hubiera pedido. Corregido: la rama de éxito comprueba
   que siga siendo la misma activación.
8. **(baja) Las filas se ejecutaban sin validar.** Se comprueba el destino al levantar y un
   tipo desconocido ya no se interpreta como local.
9. **(baja) `parar_de_host` no se usaba.** Ahora para los túneles de un host que ya no está.
10. **(media, NO corregido) Carrera entre subsistemas al abrir conexión**: el trío
    «mirar el pool / conectar / guardar» no está serializado entre pestañas, SFTP y túneles,
    así que con `multiplexar` dos aperturas simultáneas pueden desplazarse la una a la otra.
    Es el patrón preexistente de las Fases 3 y 4 (desviación 5) y el desarrollador pidió no
    tocar la Fase 3 en esta fase.

**Almacén y ssh_config**

11. **(alta) La importación perdía reenvíos en silencio.** Dos reenvíos del mismo tipo y
    puerto con distinto `bind` daban el mismo nombre derivado, chocaban con
    `UNIQUE(host_id, nombre)` y la línea desaparecía: ni túnel, ni opciones extra, ni aviso
    (solo un `warn!` en el log). Corregido: la línea que no se puede convertir vuelve a
    `opciones_extra` tal cual venía y la importación lo avisa en el resumen. Los nombres
    siguen siendo los del informe (`local-5432`…). Regresión:
    `un_reenvio_que_no_cabe_en_tuneles_vuelve_a_opciones_extra` y
    `reimportar_el_mismo_fichero_no_cambia_nada`.
12. **(alta) La exportación escribía directivas que `ssh` rechaza.** Un túnel local o dinámico
    con la escucha en puerto 0 se exportaba como `LocalForward 0 destino`, que es inválido; y
    como el `magi_config` se incluye desde `~/.ssh/config`, una sola línea mala dejaba al
    usuario sin `ssh` para **ningún** host. Corregido: no se emite lo que `ssh` no sabría
    leer (se deja un comentario en su lugar); el 0 sigue valiendo en remoto, donde lo elige el
    host. Regresión: el fichero generado se valida con el `ssh` real
    (`el_magi_config_generado_lo_acepta_ssh`).
13. **(alta/media) Borrar un host no paraba sus túneles.** La cascada borraba las filas, pero
    el servidor no se enteraba y el túnel seguía escuchando (y manteniendo vivo el servidor).
    Corregido: el cliente avisa con `RecargarTuneles` al borrar, y el servidor para los
    túneles de un host que ya no existe.
14. **(media) Un reenvío que MAGI no representa dejaba el host imposible de guardar.** La
    importación conserva en `opciones_extra` lo que no entiende, pero la validación rechazaba
    *cualquier* línea que empezara por `LocalForward`/`RemoteForward`/`DynamicForward`, así
    que ese host fallaba al guardar para siempre. Corregido: solo se rechazan los reenvíos que
    MAGI **sí** sabe representar (`sshconfig::tuneles::reenvio_gestionado`); las formas que
    `ssh` admite y MAGI no (varios destinos, socket local) siguen siendo opciones extra.
15. **(baja) Una fila con un tipo desconocido tumbaba el listado y la exportación enteros.**
    Corregido: esa fila se salta con un aviso y las demás siguen.
16. **(baja) `es_conflicto_unico` diagnosticaba como «ya existe» cualquier violación de
    restricción**, incluidas las de clave ajena. Corregido mirando el código extendido.
17. **(baja) `*` no contaba como escucha expuesta** y en un remoto con `0.0.0.0:80` solo
    salía uno de los dos avisos. Corregido: las tres formas de «cualquiera» avisan y los dos
    avisos se juntan.
18. **(baja) Cosméticos:** se quitó una función sin uso; el índice `idx_tuneles_host` se ha
    dejado (es redundante con `UNIQUE(host_id, nombre)`, pero **la migración 4 no se toca**
    una vez aplicada).

**Cliente (TUI y CLI)**

19. **(alta) Volver de Túneles a una ficha abierta dejaba la pantalla en blanco y el teclado
    mudo** (la ficha se cierra al entrar en Túneles y se volvía a ella). Corregido: si no hay
    ficha, se vuelve a Hosts.
20. **(media) Con el servidor caído quedaban túneles fantasma** en la vista y en los glifos, y
    la guarda de «sin servidor» se desactivaba al contestar el diálogo aunque el relanzamiento
    fallara. Corregido: al caer se limpian los estados y la marca se levanta solo cuando el
    relanzamiento cuaja.
21. **(media) Los `peticion_id` de túneles y del panel de Archivos eran dos contadores que
    empezaban en 0**, y la respuesta se resolvía por orden de llegada: un `Hecho`/`Error` de
    SFTP podía interpretarse como de un túnel. Corregido: el contador de túneles arranca en
    2^40.
22. **(media) El detalle de un túnel caído anunciaba `r` y `espacio` sin atenderlos.**
    Corregido: el detalle lleva el id del túnel caído y esas dos teclas hacen lo que dice.
23. **(media) La selección no se reanclaba por id al recargar la lista**, así que renombrar un
    túnel dejaba el resaltado sobre otro. Corregido (como ya hacía Hosts).
24. **(media) Un puerto ilegible (vacío o fuera de rango) se guardaba como 0 sin avisar**,
    cambiando el significado del túnel. Corregido: se avisa y no se guarda.
25. **(bajas) Corregidas:** la importación de reenvíos de la ficha deshace los túneles que
    creó si no puede guardar el host; `Ctrl+S` funciona con el desplegable de host abierto;
    cancelar la edición de un túnel en marcha avisa de que quedó parado y ofrece levantarlo;
    `n`/`i` en el bloque de la ficha dicen que hay que guardar el host; `cargo fmt --check`
    volvió a quedar limpio.

## Pendientes y bloqueos

1. **Validación manual del desarrollador (R15).** Los AC que necesitan un host de verdad
   (un `psql` por un túnel local, un `curl localhost:9000` en el host para el remoto, un
   navegador apuntando al SOCKS) no los puede hacer la suite: se prueban contra un servidor
   SSH en proceso con un servicio de eco local. Guion sugerido al final de este informe.
   - **Verificado a mano el 16 de septiembre** (host real `168.119.10.189`, clave del
     agente): el **AC del túnel remoto** —`RemoteForward 127.0.0.1:9000 → 127.0.0.1:8996`
     apuntando a un `python3 -m http.server` local— devuelve el listado del servicio local
     al hacer `curl` desde el host, y el detalle del túnel pasa a 1 conexión con los bytes
     subiendo. También se comprobó el fallo de `bind` en local (escucha en el puerto 22):
     el túnel queda `✕` con «no hay permiso para escuchar en el puerto 22» y anota
     `tunel_fallido`, en vez de quedarse a medias.
2. **Interacción con `multiplexar` (desviación 5).** Sin cerrar por decisión explícita: no
   se toca la Fase 3. Se nota al abrir una pestaña en un host que multiplexa y tenga un
   túnel levantado: el túnel cae y hay que relanzarlo con `r`.
3. **La Fase 5 se especificó sobre las Fases 2-4 sin validación manual completa** (riesgo
   R15 del maestro). Todo lo que esta fase reutiliza de ellas (pool, solicitante, hilo
   escritor, difusión, SFTP) sigue sin pasar por manos humanas.
4. **Ruido observado una vez:** en una ejecución completa de `cargo test`, la prueba de la
   Fase 4 `borrar_remoto_es_recursivo_y_anota_el_borrado` falló una vez (lee `REGISTRO` justo
   después de recibir el `Hecho`, y el hilo escritor puede no haber confirmado la fila
   todavía). No se reprodujo en tres ejecuciones aisladas ni en dos completas. Es anterior a
   esta fase y no se ha tocado.
5. **Tras subir el protocolo, un servidor de la versión anterior no se puede parar con el
   comando que MAGI mismo recomienda.** La barra y el aviso de conexión dicen «servidor de una
   versión anterior: `magi servidor parar` y volver a abrir», pero ese comando aborta con
   «El servidor habla la versión de protocolo N y este MAGI la 3; no se puede parar con este
   comando». Es lo que pasa al subir `VERSION_PROTOCOLO` (ya pasó en la Fase 4) y deja al
   usuario sin salida salvo `kill` al proceso. Propuesta (no implementada, fuera del checklist):
   que ese aviso diga el pid del servidor antiguo —se saca con `SO_PEERCRED` del socket, que
   `nix` ya está en el proyecto— para poder cerrarlo a mano sin buscarlo. Pasó de verdad
   durante esta sesión con el servidor v2.
6. **Hallazgo sobre la Fase 3 (no corregido: fuera del alcance de esta fase).** El checklist
   de la Fase 3 (línea 31, marcada `[x]`) y su informe dicen que «con `multiplexar` marcado
   una segunda sesión al mismo host abre un canal nuevo sin reautenticar», y el README lo
   repite. En el código, esa reutilización **no existe** para pestañas: `sesiones::abrir_y_servir`
   siempre abre conexión nueva y `Pool::guardar` sustituye la entrada anterior (desconectándola),
   así que dos pestañas a un host con `multiplexar` dejan solo la última conexión viva. Quien
   sí reutiliza el pool es el canal SFTP (`sftp::asegurar`) y, desde esta fase, los túneles.
   Es la raíz de la desviación 5. Se deja anotado para que el analista decida si abre una
   corrección de la Fase 3 (y si hay que ajustar el README, que hoy promete lo que no hace).

## Ejecución y pruebas

**Arrancar**

- TUI: `cargo run` (la vista Túneles está en `F6`; el servidor de sesiones se autolanza si
  no está corriendo).
- Servidor en primer plano: `cargo run -- --servidor` en una terminal y `cargo run` en otra.
- Estado y parada: `magi servidor estado` (lista los túneles activos) y `magi servidor parar`.
- CLI de túneles: `magi tuneles`, `magi tunel activar <host> <nombre>`, `magi tunel parar <host> <nombre>`.

**Migrar**

No hay nada que hacer a mano: al abrir la base, `PRAGMA user_version` la lleva de 3 a 4 y
crea `TUNELES`. Los reenvíos que estuvieran en `opciones_extra` siguen ahí hasta que se
importe el `~/.ssh/config` o se pulse `i` en la ficha del host.

**Testear**

- `cargo test` (194 pruebas: 117 de la biblioteca y el resto de integración).
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`.
- `cargo test --test tuneles` (las 11 pruebas de la fase contra el servidor SSH en proceso;
  la de la parada con una conexión abierta usa un host que multiplexa, que es donde se nota
  el corte).
- `cargo test --test sshconfig_ida_vuelta` valida el `magi_config` generado con el `ssh` del
  sistema (se salta si no hay `ssh`).
- Para pruebas aisladas: `HOME` y `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_STATE_HOME`/`XDG_RUNTIME_DIR`
  apuntando a un directorio temporal.

**Guion de validación manual sugerido (lo que la suite no puede probar)**

1. Un túnel local contra un servicio del host: crear `pg-prod` (`127.0.0.1:5432` →
   `127.0.0.1:5432` de un host con PostgreSQL), activarlo con `Espacio` y conectar desde
   otra terminal con `psql -h 127.0.0.1 -p 5432`. El detalle (`↵`) debe mostrar 1 conexión
   abierta y los bytes creciendo.
2. Un túnel remoto: crear `webhook` (`127.0.0.1:9000` → `127.0.0.1:8000`), activarlo y, en
   el host, `curl localhost:9000`: la petición debe llegar al servicio local. El contador de
   aceptadas sube. Con `0.0.0.0` como escucha, comprobar que el host solo lo expone si tiene
   `GatewayPorts`.
3. Un túnel dinámico: crear `socks` (`127.0.0.1:1080`, dinámico), activarlo y apuntar un
   navegador al proxy SOCKS5 `127.0.0.1:1080`; comprobar que resuelve nombres (los resuelve
   el host).
4. El ciclo automático: marcar un túnel como automático (`a`), abrir una pestaña al host
   (el túnel se levanta solo) y cerrarla (se para solo); con un túnel manual levantado a la
   vez, comprobar que el manual no se para.
5. Cerrar la ventana con túneles levantados y volver a abrirla: siguen ahí. `magi servidor
   estado` los lista y el servidor no se apaga solo mientras estén.
6. Importar un `~/.ssh/config` con `LocalForward`/`RemoteForward`/`DynamicForward` y
   comprobar que aparecen en `F6`, que `magi_config` los devuelve y que escribirlos a mano
   en «opciones extra» se rechaza.
