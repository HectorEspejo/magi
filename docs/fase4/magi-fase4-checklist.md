# MAGI - Checklist Fase 4: Archivos (SFTP en panel doble)

## Almacén y protocolo
- [ ] Migración 3 por `PRAGMA user_version`: columnas `HOSTS.sftp_dir_local` y `HOSTS.sftp_dir_remoto`
- [ ] `VERSION_PROTOCOLO = 2`; un servidor v1 se rechaza en el saludo y la TUI muestra «servidor de una versión anterior: magi servidor parar»
- [ ] Mensajes nuevos en `protocolo.rs`: `AbrirSftp`, `SftpAbierto`, `ListarDir`, `DirListado`, `Transferir`, `Transferencias`, `CancelarTransferencia`, `LimpiarTransferencias`, `BorrarRemoto`, `RenombrarRemoto`, `CrearDirRemoto`, `DescargarTemporal`, `BorrarTemporal`, `RutaTemporal`, `Hecho`; `Error` gana `peticion_id` opcional
- [ ] `Bienvenida` incluye `transferencias` (cola actual) para que una ventana nueva la vea
- [ ] Tests del protocolo v2: ida y vuelta de los mensajes nuevos y rechazo de v1
- [ ] Tipos nuevos en `REGISTRO`: `transferencia` (dirección, rutas, bytes, resultado) y `borrado_remoto`; el filtro `t` de la vista Registro los incluye en «archivos»

## Servidor: SFTP
- [ ] `russh-sftp` fijado a una versión compatible con russh 0.63.3; `servidor/sftp.rs` abre una `SftpSession` por host sobre un canal de la conexión del pool
- [ ] `AbrirSftp{host_id}` sin conexión viva abre una con el flujo de la Fase 3 (huella, frase, contraseña al solicitante) y la mete en el pool respetando `multiplexar`; responde `SftpAbierto{host_id, dir_inicio}` o `Error`
- [ ] El canal SFTP no aparece como pestaña ni cuenta como sesión; se cierra tras 10 min sin actividad y sin transferencias
- [ ] `ListarDir` devuelve `DirListado` con nombre, tipo (dir, fichero, enlace), tamaño, mtime, permisos y propietario; ruta inexistente o sin permiso devuelve `Error{peticion_id}`
- [ ] `BorrarRemoto` (recursivo para directorios), `RenombrarRemoto` y `CrearDirRemoto` responden `Hecho{peticion_id}` o `Error`; el borrado anota `borrado_remoto`
- [ ] `DescargarTemporal` copia el fichero a `$XDG_RUNTIME_DIR/magi/tmp/<id>-<nombre>` (directorio 700, fichero 600) y responde `RutaTemporal`; `BorrarTemporal` lo elimina; el apagado del servidor vacía el directorio
- [ ] Host sin subsistema SFTP: `Error «el host no ofrece SFTP»` y la vista queda solo con el panel local
- [ ] Conexión del pool caída: transferencias en curso pasan a `error «conexión caída»` y el siguiente `AbrirSftp` reabre el canal
- [ ] `magi servidor estado` lista la cola de transferencias (en curso y en cola por host)

## Servidor: cola de transferencias
- [ ] `Transferir{host_id, direccion, elementos, politica, borrar_origen}` encola una transferencia con id creciente; el servidor expande directorios (bajada: `readdir` recursivo; subida: lista enviada por el cliente) y calcula `bytes_total`
- [ ] Una transferencia en curso por host, FIFO; hosts distintos avanzan en paralelo
- [ ] Bloques de 64 KiB con hasta 16 peticiones en vuelo por fichero; destino escrito como `<nombre>.magi-parcial` y renombrado al terminar
- [ ] Las bajadas se escriben directamente en el disco local por el servidor, creando los directorios necesarios; las subidas crean los directorios remotos que falten
- [ ] mtime del origen conservado en el destino (`setstat` remoto, `filetime` local)
- [ ] Política de conflicto por elemento (`sobrescribir` / `omitir`) aplicada a todo el contenido de un directorio; el servidor recomprueba la existencia justo antes de escribir y cuenta los omitidos en el detalle
  - AC: Dado un directorio con 3 ficheros de los que 2 existen en destino, cuando se transfiere con `omitir`, entonces se copia 1, el detalle indica «2 omitidos» y la transferencia termina `hecha`
- [ ] `CancelarTransferencia` en cola → `cancelada`; en curso → se comprueba por bloque, se borra el parcial y queda `cancelada`
- [ ] Error de E/S o permiso en un fichero deja la transferencia en `error` con detalle y la cola continúa con la siguiente
- [ ] `borrar_origen` (mover): el origen se borra solo tras `hecha` (remoto por el servidor; local por el cliente)
- [ ] Enlaces simbólicos: a fichero se copian como el fichero apuntado; a directorio se omiten con nota en el detalle
- [ ] `Transferencias{lista}` difundida en cada cambio de estado y como máximo 4 veces por segundo durante el progreso
- [ ] Terminadas (hecha, error, cancelada) se conservan 1 h o hasta `LimpiarTransferencias`
- [ ] Cada transferencia terminada anota `transferencia` en `REGISTRO` con dirección, rutas, bytes y resultado; nunca contenido
- [ ] Tests con el `sshd` efímero: listar, subir y bajar fichero y directorio, conflicto con `omitir`, cancelación en curso, mtime conservado, borrado remoto

## Cliente: paneles
- [ ] `F4` abre Archivos con el host de la pestaña activa o del host seleccionado; `s` en Hosts y Flota; prefijo + `f` en Sesión; paleta `sftp · <host>`
- [ ] Panel local con `std::fs`: arranca en `HOSTS.sftp_dir_local` o `~`; panel remoto en `HOSTS.sftp_dir_remoto` o `dir_inicio`; ambos se guardan al cambiar de directorio
- [ ] Ruta remota guardada inexistente cae a `dir_inicio` con mensaje; ruta local inexistente cae a `~`
- [ ] Mismo widget en ambos paneles: columnas nombre (truncado con `…`, `@` para enlaces), tamaño (`—` para directorios, unidades legibles) y fecha (`hoy`, `ayer`, `12 sep`, `2025`); directorios primero, orden por nombre; entrada `..`
- [ ] Panel activo con nombre en ámbar y el inactivo con marco tenue; `Tab` cambia de panel sin perder la selección de ninguno
- [ ] Navegación con `↑` `↓` / `j` `k`, `PgUp` `PgDn`, `Home` `End`; `↵` entra en directorios y sigue enlaces; `Backspace` / `-` sube al padre
- [ ] `g` abre el diálogo «ir a ruta» con la ruta actual editable y expansión de `~`
- [ ] `.` alterna ocultos (por defecto `[archivos] mostrar_ocultos`); `/` filtra por nombre en el panel activo; `R` refresca ambos
- [ ] `Espacio` marca/desmarca y baja una fila; `a` marca todo y `A` desmarca; pie del panel con «N elementos · tamaño · M marcados»
- [ ] `i` abre el detalle: tipo, tamaño exacto, fecha completa, permisos, propietario, ruta, destino del enlace y comparación con el otro lado
- [ ] `h` cambia de host (paleta filtrada a `sftp · <host>`) conservando el panel local
- [ ] `Esc` limpia primero el filtro o los marcados; `q` vuelve a la vista anterior sin preguntar aunque haya transferencias

## Cliente: marcas y avisos
- [ ] `archivos/marcas.rs`: `✕` si no existe al otro lado; `≠` si existe con tipo, tamaño o mtime (tolerancia 2 s) distintos; los directorios solo `✕`; recalculadas al cambiar de directorio o refrescar
  - AC: Dado `config.yaml` de 2 kB en local y 1 kB en remoto, cuando se listan ambos, entonces las dos filas muestran `≠` y el detalle indica «≠ tamaño»
- [ ] Marcas pintadas al final de la fila en ámbar (`≠`) y rojo (`✕`); en ASCII `!=` y `x`
- [ ] `archivos/sensibles.rs`: globs de `[archivos] avisar` (por defecto `.env*`, `*.pem`, `*.key`, `id_*`) comprobados solo en subidas, incluidos los ficheros dentro de directorios; diálogo con hasta 5 coincidencias y «+N»; cancelar no encola nada
  - AC: Dado un directorio que contiene `.env`, cuando se copia al remoto, entonces aparece el aviso antes de encolar y `Esc` deja la cola sin cambios
- [ ] Lista `avisar` vacía: la barra lo indica al abrir Archivos

## Cliente: operaciones
- [ ] `c` copia los marcados (o la fila actual) al directorio del otro panel; para subidas el cliente recorre los directorios locales y envía la lista de ficheros
- [ ] Conflictos de primer nivel detectados con el listado del otro panel: diálogo sobrescribir / omitir / sobrescribir todos / omitir todos con tamaño y fecha de ambos; `Esc` cancela toda la operación
- [ ] `m` entre paneles = transferencia con `borrar_origen`; en el mismo panel equivale a `r`
- [ ] `x` borra los marcados tras confirmación con recuento y «sin papelera»; local con `fs::remove_*`, remoto con `BorrarRemoto`
- [ ] `r` renombra (diálogo con el nombre actual); `d` crea directorio; local por `std::fs`, remoto por protocolo
- [ ] `↵` sobre un fichero lo abre con `[archivos] pager` (`$PAGER` o `less`) suspendiendo y restaurando la TUI; remoto vía `DescargarTemporal` y `BorrarTemporal` al cerrar el visor
- [ ] Ver un fichero remoto de más de 10 MB pide confirmación
- [ ] `$PAGER` inexistente: mensaje y oferta de `less` o `more` si se detectan
- [ ] Tras `hecha`, `Error` o cualquier operación remota, el cliente refresca el panel afectado y recalcula marcas

## Cliente: cola
- [ ] Panel de cola de 3 filas al pie de Archivos: transferencia en curso con barra `█░` (`#.` en ASCII), porcentaje y bytes, más «en cola N · tamaño»; oculto si la cola está vacía
- [ ] `t` abre la vista Transferencias con estado, dirección, origen → destino y progreso de todas; detalle inferior con bytes, velocidad y restante calculados con los dos últimos `Transferencias`
- [ ] `x` cancela la seleccionada (confirmación si está en curso); `C` limpia terminadas; `↵` muestra el detalle (rutas, bytes, error, omitidos)
- [ ] Una ventana nueva ve la cola desde `Bienvenida`; todas las ventanas ven el mismo progreso
- [ ] Entradas de paleta: `sftp · <host>`, `transferencias`, `cancelar transferencias`

## Configuración y navegación
- [ ] `config.toml`: `[archivos] avisar = [".env*", "*.pem", "*.key", "id_*"]`, `mostrar_ocultos = false`, `pager`
- [ ] `F4` operativo; `F6` sigue reservada («vista no disponible en esta fase»)
- [ ] Ayuda `?` en Archivos y Transferencias; ninguna acción destructiva (borrar, sobrescribir, cancelar en curso, subir sensible) sin confirmación

## Calidad
- [ ] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [ ] `cargo test` verde con los tests nuevos de protocolo, marcas, sensibles y transferencias
- [ ] README actualizado: Archivos, marcas, cola, aviso de sensibles y `[archivos]`

---

**Progreso Fase 4:** 0 / 65 funcionalidades

**Total MAGI (Fases 1-4):** 289 / 356 funcionalidades
