# MAGI - Checklist Fase 5: Túneles

## Almacén y modelo
- [x] Migración 4 por `PRAGMA user_version`: tabla `TUNELES` (id, host_id FK CASCADE, nombre, tipo, escucha, destino, automatico, creado_en, actualizado_en) con `UNIQUE(host_id, nombre)`
- [x] `almacen/tuneles.rs`: CRUD desde el cliente; al borrar un host sus túneles caen en cascada
- [x] Sin `CHECK` sobre `tipo`; validación en código: nombre `[A-Za-z0-9._-]+` único por host, tipo `local | remoto | dinamico`, escucha `dirección:puerto` (1-65535), destino obligatorio salvo en dinámico
- [x] Rechazo de duplicados: mismo tipo y misma escucha en local/dinámico (cualquier host) o en remoto (mismo host)
- [x] Tipos nuevos en `REGISTRO`: `tunel_abierto`, `tunel_cerrado` (con motivo y totales), `tunel_fallido`; el filtro `t` de la vista Registro los incluye en «conexiones»
- [x] Tests del almacén: migración 3→4, cascada por host, unicidad por host

## ssh_config
- [x] `sshconfig/tuneles.rs`: `LocalForward [bind:]puerto host:hostport` → local, `RemoteForward` → remoto, `DynamicForward [bind:]puerto` → dinámico; bind por defecto `127.0.0.1`; nombres `local-<puerto>`, `remoto-<puerto>`, `socks-<puerto>`; `automatico = 1`
- [x] La importación de `~/.ssh/config` crea filas en `TUNELES` en lugar de dejar los reenvíos en `opciones_extra`
- [x] La exportación a `magi_config` emite `LocalForward`/`RemoteForward`/`DynamicForward` por cada túnel del host
- [x] `LocalForward`, `RemoteForward` y `DynamicForward` pasan a directivas gestionadas: se rechazan al editar `opciones_extra` a mano
- [x] Ficha de host con reenvíos en `opciones_extra` (de la F1): aviso «hay N reenvíos en opciones extra · i importar a túneles»; `i` crea las filas y quita las líneas
- [x] Test de ida y vuelta ampliado: túneles de los tres tipos sobreviven a `importar(exportar(hosts))`

## Protocolo
- [x] `VERSION_PROTOCOLO = 3`; mensajes `ActivarTunel`, `PararTunel`, `RelanzarTunel`, `RecargarTuneles`, `Tuneles`, respondidos con `Hecho{peticion_id}` o `Error{peticion_id, mensaje}`; `Bienvenida` incluye los túneles activos
- [x] `Tuneles{lista}` se difunde en cada cambio de estado y, para contadores, como máximo 2 veces por segundo y solo si cambian
- [x] Tests del protocolo v3: ida y vuelta de los mensajes nuevos y rechazo de v2

## Servidor: túneles
- [x] `servidor/tuneles.rs`: `TunelActivo {tunel_id, host_id, tipo, escucha, destino, estado, origen, conexiones abiertas y aceptadas, bytes_subidos, bytes_bajados, desde, ultimo_error, solicitante}` con estados `activando | activo | parando | caido`
- [x] `ActivarTunel` lee la fila de `TUNELES` con la conexión de solo lectura, toma la conexión del pool o abre una nueva con el flujo de la Fase 3 (diálogos al solicitante) y la mete en el pool respetando `multiplexar`
- [x] Un túnel activo cuenta como canal del pool y se libera al parar o caer, por identidad y en todas las ramas de error
- [x] Túnel local: `TcpListener::bind(escucha)`; por conexión aceptada `channel_open_direct_tcpip(destino, origen)` y `copy_bidirectional` con búfer de 64 KiB; el rechazo del host cierra solo esa conexión y anota `ultimo_error`
  - AC: Dado un túnel local a `10.0.0.5:5432`, cuando `psql` conecta a `127.0.0.1:5432`, entonces el detalle muestra 1 conexión abierta y los bytes crecen en ambas direcciones
- [x] Túnel dinámico: `servidor/socks5.rs` con saludo (método 0x00), `CONNECT` a IPv4, IPv6 o dominio, respuesta 0x05 si el host rechaza y 0x07 para BIND/UDP; el nombre se envía al host sin resolver en local
- [x] Túnel remoto: `handle.tcpip_forward(dirección, puerto)`; registro (dirección, puerto) → túnel en la conexión; `Handler::server_channel_open_forwarded_tcpip` abre `TcpStream::connect(destino)` y copia; canales con clave no registrada se rechazan; parar hace `cancel_tcpip_forward` sin afectar a las pestañas
  - AC: Dado un túnel remoto `127.0.0.1:9000 → 127.0.0.1:9000`, cuando en el host se hace `curl localhost:9000`, entonces la petición llega al servicio local y el contador de aceptadas sube
- [x] `bind` fallido (`EADDRINUSE`, permiso, puerto < 1024) y reenvío rechazado por el host dejan el túnel `caido` con motivo legible y anotan `tunel_fallido`
- [x] `tcpip_forward` devuelve el puerto real: se muestra (útil con puerto 0); puerto 0 en local/dinámico asigna uno libre y se muestra sin persistir
- [x] `PararTunel`: `parando` → cierra listener o cancela el reenvío, corta las conexiones abiertas, anota `tunel_cerrado` con totales, libera el canal del pool
- [x] Conexión del pool caída: los túneles del host pasan a `caido` («conexión caída») y anotan `tunel_cerrado`; `RelanzarTunel` los vuelve a activar; no hay reintento automático
- [x] Automáticos: al abrir el primer canal de pestaña o SFTP de un host se activan sus túneles con `automatico = 1` que no estén activos (origen `automatico`); al cerrar el último se paran solo los de origen `automatico`
  - AC: Dado un host con un túnel automático y otro manual activo, cuando se cierra la última pestaña, entonces el automático se para y el manual sigue
- [x] Un túnel no cuenta como pestaña ni SFTP para el ciclo automático; un automático parado a mano pasa a manual hasta el siguiente ciclo
- [x] Contadores: bytes por dirección, conexiones abiertas y aceptadas; se reinician al relanzar; el detalle de `tunel_cerrado` lleva los totales
- [x] Al borrar un host o un túnel activo, el servidor lo para antes (`RecargarTuneles`)
- [x] `magi servidor estado` lista los túneles activos con tipo, escucha, destino, host, conexiones y tráfico; el servidor no se apaga por inactividad con túneles activos
- [x] Tests contra el `sshd` efímero: local (eco por el túnel), dinámico (CONNECT a dominio e IP), remoto (petición desde el host), bind ocupado, rechazo del host, parada con conexión abierta, ciclo automático

## Vista Túneles (F6)
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

## Ficha de host y otras vistas
- [x] Bloque «Túneles» en la ficha con la lista del host (`[a]`, tipo, escucha → destino, nombre) y `n` / `e` / `x` con la misma lógica que la vista
- [x] Barra de estado de Sesión muestra «túneles N» si el host tiene túneles activos
- [x] Glifo `⇅` tras el nombre en Hosts y Flota cuando el host tiene túneles activos
- [x] Entradas de paleta: `túnel · <host> · <nombre>` (activa o para), `ir a túneles`, `nuevo túnel`
- [x] Ayuda `?` en Túneles; ninguna acción destructiva (borrar, parar con conexiones abiertas, escucha en `0.0.0.0`) sin confirmación o aviso
- [x] `F1`-`F7` todas operativas; el mensaje «vista no disponible en esta fase» desaparece

## CLI
- [x] `magi tunel activar <host> <nombre>` y `magi tunel parar <host> <nombre>` hablan con el servidor (lo autolanzan si hace falta); sin credenciales disponibles fallan con «abre una sesión desde la TUI primero»
- [x] `magi tuneles` lista túneles definidos y activos con estado, escucha, destino y tráfico

## Calidad
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de almacén, ssh_config, protocolo y servidor
- [x] Revisión adversarial del código nuevo (servidor y cliente) antes de cerrar, con resultado en el informe de implementación
- [x] README actualizado: túneles, tipos, automáticos, `magi tunel`, aviso de `0.0.0.0`

---

**Progreso Fase 5:** 54 / 54 funcionalidades

**Total MAGI (Fases 1-5):** 408 / 410 funcionalidades
