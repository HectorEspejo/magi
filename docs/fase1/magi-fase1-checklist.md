# MAGI - Checklist Fase 1: Inventario y Conexión

## Esqueleto y CLI
- [x] Crate binario `magi` con clap: `magi`, `magi importar [ruta]`, `magi exportar`, `magi --version`
- [x] `magi importar [ruta]` funciona sin TUI e imprime el resumen (importados / sobrescritos / omitidos con motivo)
- [x] `magi exportar` funciona sin TUI y regenera `~/.ssh/magi_config`
- [x] Rutas XDG (`directories`): datos en `~/.local/share/magi`, config en `~/.config/magi`, log en `~/.local/state/magi`; equivalentes en macOS
- [x] `config.toml` con `prefijo_escape` (por defecto `Ctrl+]`), `exportar_al_guardar` (por defecto true), `tema` (auto | fijo | ruta) y `terminal_ascii`
- [x] Log con `tracing` en `~/.local/state/magi/magi.log`, rotación diaria, sin frases ni claves
- [x] Bucle de eventos con crossterm + canal `mpsc` de eventos de conexión; la UI nunca bloquea en red
  - AC: Dado un host que no responde, cuando se conecta, entonces la UI sigue respondiendo a teclas y el intento caduca a los 10 s con mensaje
- [x] Restauración del terminal (raw mode, pantalla alternativa) al salir y ante `panic` mediante hook
- [x] Redimensionado de la terminal repinta la vista actual
- [ ] Compila y arranca en Linux y macOS sin código específico de plataforma fuera de las rutas

## Almacén SQLite
- [x] `magi.db` en WAL con `foreign_keys=ON`, creado con permisos 600
- [x] Migraciones numeradas por `PRAGMA user_version` en `almacen/migraciones.rs`
- [x] Tabla `GRUPOS` (id, nombre UQ, orden, plegado, creado_en)
- [x] Tabla `HOSTS` (id, nombre UQ, grupo_id, direccion, puerto, usuario, identidad_ref, salto_host_id, multiplexar, keepalive_seg, opciones_extra, origen, ultimo_estado, ultima_conexion_en, creado_en, actualizado_en)
- [x] Tabla `ETIQUETAS` (id, nombre UQ) y tabla `HOST_ETIQUETAS` (host_id, etiqueta_id) con borrado en cascada
- [x] Sin `CHECK` sobre enumeraciones; `origen` y `ultimo_estado` se validan en código
- [x] Al borrar un grupo sus hosts pasan a `grupo_id = NULL`, nunca se borran
- [x] Al borrar un host que es salto de otros, los dependientes quedan con `salto_host_id = NULL` tras confirmación
- [x] Una etiqueta que se queda sin hosts se elimina
- [x] Checkpoint de WAL al salir
- [x] Tests del almacén con SQLite en memoria (CRUD, cascadas, migración desde vacío)

## Tema y sistema visual
- [x] Lectura de `~/.config/omarchy/current/theme/alacritty.toml` al arrancar: primary.background/foreground, normal.yellow, normal.green, normal.red, bright.black
- [x] Paleta fija de respaldo (`#0A0A0A`, `#EDE8DC`, `#FFA000`, `#00FF6A`, `#FF2A1A`, `#5A5A5A`) cuando no existe el tema de Omarchy
- [x] `tema` en `config.toml` tiene prioridad sobre la detección automática
- [x] Glifos de estado `●` `◐` `○` `✕` `▸` siempre acompañados de color, nunca solo color
- [x] Modo degradado ASCII (`*` `o` `x` `>` y marcos `+ - |`) cuando el locale no es UTF-8 o `terminal_ascii = true`
- [x] Barra inferior siempre visible con los atajos del contexto actual
- [x] Mensajes en la barra inferior durante 4 s o hasta la siguiente tecla; errores en rojo con `✕`
- [x] Destello de una fila del inventario cuando cambia de estado (única animación de la fase)
- [x] Ayuda contextual con `?` que lista los atajos de la vista actual

## Vista Hosts
- [x] Listado agrupado con cabecera de grupo (`▼`/`▶` + nombre + recuento) y hosts «sin grupo» al final
- [x] Columnas: glifo de estado, nombre, dirección, usuario, puerto; indicador `⤴ <salto>` si el host usa salto
- [x] Contador «N hosts» en la cabecera del marco
- [x] Navegación con `↑` `↓` / `j` `k`; un grupo plegado cuenta como una fila
- [x] Plegar y desplegar grupo con `←` `→` / `h` `l`; el estado se persiste en `GRUPOS.plegado`
- [x] Filtro incremental con `/` sobre nombre, dirección, usuario, grupo y etiquetas (subcadena, sin mayúsculas ni acentos, tokens AND)
  - AC: Dado el filtro «prod root», cuando se escribe, entonces solo aparecen hosts cuyo pajar contiene ambos tokens y los grupos vacíos se ocultan
- [x] Los grupos plegados se despliegan temporalmente mientras hay filtro activo
- [x] `Esc` limpia el filtro y devuelve el plegado anterior
- [x] `Tab` alterna la columna derecha entre usuario·puerto y etiquetas
- [x] Glifo de estado derivado: `●` con sesión viva, `✕` si `ultimo_estado = error`, `○` en el resto
- [x] `e` abre la ficha del host seleccionado; `n` abre la ficha vacía con el grupo del host seleccionado
- [x] `x` borra el host tras diálogo de confirmación
  - AC: Dado un host seleccionado, cuando se pulsa `x` y luego `n` o `Esc`, entonces no se borra nada
- [x] `I` lanza la importación de `~/.ssh/config` y `E` la exportación de `magi_config`

## Grupos
- [x] Menú de grupo con `g`: nuevo grupo, renombrar, mover host seleccionado a grupo, borrar grupo, subir/bajar orden
- [x] Crear grupo con nombre único (validación de duplicado con mensaje)
- [x] Renombrar grupo conserva sus hosts y su plegado
- [x] Mover host a grupo mediante desplegable filtrable
- [x] Borrar grupo con confirmación indicando cuántos hosts pasarán a «sin grupo»
- [x] Reordenar grupos actualiza `orden` y el listado

## Etiquetas
- [x] Campo de etiquetas en la ficha separado por espacios; se normalizan a minúsculas y sin duplicados
- [x] Autocompletado de etiquetas existentes al escribir en el campo
- [x] Las etiquetas aparecen en la columna alternativa del inventario y en el filtro

## Ficha de host
- [x] Formulario en cuatro bloques: Identificación, Acceso, Al conectar, Opciones extra
- [x] Campos: nombre, dirección, puerto, grupo (desplegable), etiquetas, usuario, identidad (desplegable), salto (desplegable), multiplexar (casilla), mantener (casilla + segundos), opciones extra (área de texto)
- [x] Valores por defecto al crear: puerto 22, keepalive 30 s marcado, identidad «auto», multiplexar desmarcado
- [x] `Tab` / `Shift+Tab` recorren los campos; `↵` abre desplegables; `Espacio` marca casillas
- [x] Los desplegables filtran al escribir y muestran «ninguno» como primera opción donde aplica
- [x] Validación de nombre: obligatorio, único, `[A-Za-z0-9._-]+`
- [x] Validación de dirección obligatoria y puerto 1-65535
- [x] Validación de salto: distinto del propio host, sin ciclos y máximo 3 niveles
  - AC: Dado A→B y B→A, cuando se guarda B con salto A, entonces se rechaza con mensaje «ciclo de saltos»
- [x] Validación de keepalive 5-600 s cuando «Mantener» está marcado
- [x] Validación de opciones extra: una directiva por línea; se rechazan las que MAGI ya gestiona (HostName, Port, User, IdentityFile, ProxyJump, ControlMaster, ServerAliveInterval)
- [x] `Ctrl+S` guarda, actualiza `actualizado_en` y vuelve al inventario con el host seleccionado
- [x] Si `exportar_al_guardar` está activo, guardar regenera `magi_config`
- [x] `Esc` descarta; con cambios sin guardar pide confirmación
- [x] `Ctrl+T` prueba la conexión sin abrir shell (resolución, handshake, huella, autenticación) y muestra el resultado en la barra
  - AC: Dado un host con usuario incorrecto, cuando se pulsa `Ctrl+T`, entonces la barra muestra «autenticación rechazada» y no se abre ninguna sesión

## Identidades
- [x] Escaneo de `~/.ssh/*.pub` con tipo y huella SHA256 de cada clave
- [x] Listado de claves cargadas en el agente vía `SSH_AUTH_SOCK` con tipo, comentario y huella
- [x] Desplegable de identidad con entradas «auto», «agente · <comentario> · <huella>» y «fichero · <ruta>»
- [x] `identidad_ref` se guarda como NULL, `agente:SHA256:…` o `fichero:<ruta>`
- [x] Aviso (no bloqueo) al guardar si la clave `agente:` no está cargada en ese momento
- [x] Si el agente no está disponible, el desplegable lo indica y ofrece solo ficheros

## Importación de ssh_config
- [x] Parser propio de ssh_config: bloques `Host`, `Match`, directiva `Include` (un nivel, ignorando `magi_config`), comentarios y comillas
- [x] Solo se importan bloques `Host` con un único patrón sin `*` ni `?`; el resto se omite con motivo
- [x] Mapeo: HostName→direccion (o nombre si falta), Port→puerto, User→usuario, IdentityFile→identidad_ref fichero, ControlMaster→multiplexar, ServerAliveInterval→keepalive_seg
- [x] Resto de directivas a `opciones_extra` línea a línea, sin pérdida
- [x] `ProxyJump` se resuelve por nombre al terminar la importación; si no existe, la directiva queda en `opciones_extra`
- [x] Hosts importados van al grupo «~/.ssh/config» con `origen = ssh_config`
- [x] Conflicto por nombre existente: diálogo sobrescribir / omitir / sobrescribir todos / omitir todos
- [x] Diálogo resumen final: importados, sobrescritos y omitidos con motivo
- [x] Ruta inexistente o ilegible produce mensaje claro y no modifica el inventario
- [x] Test de ida y vuelta: `importar(exportar(hosts))` reproduce el inventario, incluidas las opciones extra
  - AC: Dado un inventario con salto, multiplexar, keepalive y dos opciones extra, cuando se exporta e importa, entonces todos los campos coinciden

## Exportación de magi_config
- [x] Generación de `~/.ssh/magi_config` con cabecera de fecha, un bloque `Host` por host, comentario de grupo y bloques ordenados por grupo y nombre
- [x] Directivas emitidas: HostName, Port, User, IdentityFile (solo `fichero:`), ProxyJump (nombre del salto), ControlMaster auto + ControlPath + ControlPersist 10m si multiplexar, ServerAliveInterval si keepalive, opciones extra literales
- [x] Un host de salto se emite antes que los hosts que lo usan
- [x] Escritura atómica (temporal + rename) con permisos 600
- [x] Detección de `Include ~/.ssh/magi_config` en `~/.ssh/config`; si falta, diálogo para insertarla en la primera línea
- [x] Al insertar el Include se guarda copia `config.bak-<fecha>`
- [x] Si `~/.ssh` no existe o no es escribible, mensaje claro y el inventario sigue operativo

## Conexión SSH (russh)
- [x] Cliente russh en tarea tokio con máquina de estados: inactiva, resolviendo, conectando, verificando_huella, autenticando, abierta, en_segundo_plano, cerrada, error, cancelada
- [x] Resolución DNS y conexión TCP con timeout de 10 s; errores con motivo legible en la barra
- [x] Autenticación `auto`: todas las claves del agente y después `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`
- [x] Autenticación `agente:<huella>`: solo esa clave; si no está cargada, error «ejecuta ssh-add»
- [x] Autenticación `fichero:<ruta>`: carga de la clave; si está cifrada, diálogo de frase con 3 intentos
- [x] Frases y claves descifradas en memoria con `zeroize`, destruidas al cerrar la sesión
- [x] Keepalive de russh según `keepalive_seg`
- [x] Salto (`ProxyJump`): sesión al host de salto y canal direct-tcpip hacia el destino, en cadena hasta 3 niveles
  - AC: Dado backup-nas con salto hetzner-01, cuando se conecta, entonces se verifica la huella y se autentica en hetzner-01 primero y después en backup-nas
- [x] Canal de sesión con request-pty (`xterm-256color`, tamaño real) y shell
- [x] `HOSTS.ultimo_estado` y `ultima_conexion_en` se actualizan al abrir; `ultimo_estado = error` al fallar
- [x] Al conectar con una sesión ya viva, diálogo para cerrarla antes de abrir la nueva
- [x] Al salir de MAGI con sesión viva, confirmación y cierre limpio del canal

## Verificación de huellas
- [x] Comprobación contra `~/.ssh/known_hosts` (entradas hasheadas incluidas) por `direccion` y `[direccion]:puerto` si el puerto no es 22
- [x] Huella desconocida: diálogo con tipo y huella SHA256; `a` acepta y añade línea, `Esc` cancela
- [x] Huella cambiada: diálogo de bloqueo en rojo con huella anterior y nueva; sustituir exige `r` y escribir el nombre del host
  - AC: Dado un host cuya clave cambió, cuando se escribe un nombre incorrecto, entonces no se sustituye y la conexión sigue bloqueada
- [x] Sustituir una huella deja copia `known_hosts.old` y elimina las líneas previas de esa dirección
- [x] Aceptaciones y sustituciones se registran en el log con fecha, host y huellas

## Vista Sesión
- [x] Terminal remoto renderizado con `tui-term` sobre un `vt100::Parser` que ocupa todas las filas entre el marco y la barra
- [x] Todas las teclas se envían al remoto salvo el prefijo de escape
- [x] Prefijo `Ctrl+]` (configurable) + `q`/`Esc` vuelve a Hosts dejando la sesión en segundo plano
- [x] Prefijo + `x` cierra la sesión tras confirmación
- [x] Prefijo + prefijo envía el prefijo literal al remoto
- [x] `F3` o `↵` sobre el host de la sesión en segundo plano vuelve a la vista Sesión
- [x] Redimensionar la terminal envía `window_change` al remoto y reajusta el parser
- [x] Barra de estado: glifo, host, tiempo conectado, identidad usada y atajos del prefijo
- [x] Cierre del shell remoto (exit) o caída de red devuelve a Hosts con mensaje y estado `cerrada`
- [x] Salida del remoto coalescida a ~30 fps; solo se repinta cuando cambia el buffer

## Paleta de comandos
- [x] `Ctrl+P` abre la paleta centrada desde cualquier vista
- [x] Búsqueda difusa (`nucleo-matcher`) sobre hosts y acciones, con la categoría a la derecha
- [x] Entradas: `conectar · <host>`, `editar host · <host>`, `nuevo host`, `importar ~/.ssh/config`, `exportar magi_config`, `ir a sesión`
- [x] `↑` `↓` mueven, `↵` ejecuta, `Esc` cierra

## Navegación global
- [x] `F2` va a Hosts y `F3` a la sesión activa (si no hay, mensaje en la barra)
- [x] `F1`, `F4`-`F7` muestran «vista no disponible en esta fase» en la barra
- [x] `q` vuelve a la vista anterior o sale; con sesión viva pide confirmación
- [x] `Esc` cierra el diálogo o la paleta abiertos antes que cualquier otra acción
- [x] Ninguna acción destructiva (borrar host, borrar grupo, sustituir huella, cerrar sesión, modificar `~/.ssh/config`) se ejecuta con una sola pulsación

## Calidad
- [x] `cargo clippy` sin avisos y `cargo fmt` aplicado
- [ ] `cargo test` verde en CI de GitHub Actions (Linux) con los tests de almacén y de ida y vuelta
- [x] README con instalación (`cargo install --path .`), atajos y explicación del `Include`

---

**Progreso Fase 1:** 126 / 128 funcionalidades

**Total MAGI (Fase 1):** 126 / 128 funcionalidades
