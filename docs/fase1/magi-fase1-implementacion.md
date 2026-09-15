# MAGI - Fase 1: Informe de Implementación

**Fecha:** 15 de septiembre de 2026
**Estado:** implementación completa; quedan 2 funcionalidades del checklist pendientes (CI en GitHub Actions, por decisión del desarrollador, y verificación en macOS, sin acceso a un equipo de esa plataforma).

## Resumen de lo implementado

Se ha implementado la Fase 1 completa de MAGI como crate binario `magi`
(con objetivo librería para los tests de integración):

- **CLI**: `magi`, `magi importar [ruta]`, `magi exportar` y `magi --version`
  con clap. Importar y exportar funcionan sin TUI e imprimen su resumen.
- **Almacén SQLite** en WAL con `foreign_keys=ON`, permisos 600, migraciones
  numeradas por `PRAGMA user_version`, checkpoint al salir y tests en memoria.
  Tablas `GRUPOS`, `HOSTS`, `ETIQUETAS` y `HOST_ETIQUETAS` sin `CHECK` sobre
  enumeraciones.
- **Inventario**: vista Hosts agrupada con plegado persistente, filtro
  incremental por subcadena sin mayúsculas ni acentos, tokens en AND,
  columnas conmutables, glifos `●` `○` `✕` (y `◐` en conexiones en vuelo),
  destello de fila al cambiar de estado y barra inferior contextual.
- **Grupos y etiquetas**: menú `g` completo (crear, renombrar, mover host,
  borrar, reordenar), autocompletado de etiquetas normalizadas a minúsculas.
- **Ficha de host** en cuatro bloques con validación completa (nombre único y
  sin comodines, dirección, puerto 1-65535, salto sin ciclos y máximo 3
  niveles, keepalive 5-600 s, opciones extra sin directivas gestionadas),
  desplegables filtrables, `Ctrl+S` y prueba de conexión `Ctrl+T`.
- **Identidades**: escaneo de `~/.ssh/*.pub` y del agente (`SSH_AUTH_SOCK`)
  con tipo, comentario y huella SHA256; `identidad_ref` como NULL,
  `agente:SHA256:…` o `fichero:<ruta>`; aviso no bloqueante si la clave del
  agente no está cargada.
- **ssh_config**: parser propio con `Host`, `Match`, `Include` (un nivel,
  ignorando `magi_config`), comentarios y comillas; importación sin pérdida
  con diálogo de conflictos y resumen; exportación atómica a
  `~/.ssh/magi_config` con inserción opcional del `Include` y copia
  `config.bak-<fecha>`; test de ida y vuelta.
- **Conexión embebida con russh**: tarea tokio con máquina de estados,
  timeout de 10 s en DNS/TCP, autenticación `auto` / `agente:` / `fichero:`
  con frase en diálogo (3 intentos, `zeroize`), keepalive, saltos en cadena
  con canales `direct-tcpip`, verificación estricta de huellas contra
  `~/.ssh/known_hosts` (entradas hasheadas incluidas) con bloqueo ante huella
  cambiada y copia `known_hosts.old`, una sola sesión con estado
  `en_segundo_plano`, y vista Sesión con `tui-term` + `vt100` a ~30 fps.
- **Paleta `Ctrl+P`** con búsqueda difusa (`nucleo-matcher`) sobre hosts y
  acciones, ayuda contextual `?`, y tema de Omarchy con paleta de respaldo y
  modo degradado ASCII.

## Desviaciones respecto a la especificación (qué y por qué)

1. **`russh-keys` no se usa como crate separado.** La spec nombra
   «russh + russh-keys», pero en russh 0.63 las claves, el agente y
   `known_hosts` viven dentro de `russh` (`russh::keys`); el crate
   `russh-keys` está en `0.50.0-beta` e incompatibilidad con el `ssh-key` de
   russh 0.63. Aprobado por el desarrollador.
2. **Crates añadidos fuera del stack del informe** (aprobados): `chrono`
   (fechas ISO 8601 locales), `unicode-normalization` (filtro sin acentos),
   `anyhow` + `thiserror` (errores) y `tempfile` (dev, tests).
3. **Crate librería + binario.** Se añadió `src/lib.rs` para que los tests de
   `tests/` puedan usar el código; sigue siendo un único crate binario `magi`
   (decisión D1) más su objetivo librería.
4. **`src/ficheros.rs`**: módulo auxiliar no listado en el informe con la
   escritura atómica, los permisos y las copias con fecha compartidas por
   config, exportación y `known_hosts`.
5. **Log diario real**: `tracing-appender` genera
   `~/.local/state/magi/logs/magi.log.<fecha>`; no existe un fichero plano
   `magi.log` continuo. Se eligió la rotación diaria pedida.
6. **`Ctrl+T` actualiza `ultimo_estado`** (ok/error) pero no
   `ultima_conexion_en`, que solo se marca al abrir una sesión real.
7. **`◐` en conexiones en vuelo**: la tabla de estado visible de la §3
   (`●`/`○`/`✕`) se respeta en el inventario; `◐` se usa como glifo del host
   mientras hay un intento en curso y en la barra, para no contradecir el
   glifo incluido en el checklist.
8. **Importación de `ControlPath`/`ControlPersist`**: el valor canónico que
   MAGI exporta (`~/.ssh/cm-%r@%h:%p` y `10m`) se consume al importar para
   que `importar(exportar(hosts))` reproduzca el inventario; cualquier valor
   propio del usuario se conserva en `opciones_extra`.
9. **`ProxyJump` no modelable** (`user@host`, `host:puerto` o varios saltos
   separados por comas) se conserva literal en `opciones_extra`; solo se
   resuelve por nombre el caso simple.
10. **Directivas globales** (antes del primer bloque `Host`) se omiten con
    aviso: la spec solo modela bloques `Host` y replicarlas en cada host
    habría multiplicado directivas.
11. **`Include` dentro de un bloque `Host`** se conserva literal en
    `opciones_extra` en lugar de expandirse (la spec solo describe Include de
    un nivel a nivel de documento).
12. **Al sobrescribir en la importación se conservan las etiquetas** del host
    existente, porque las etiquetas no existen en ssh_config y borrarlas
    sería destructivo.
13. **`magi importar` sin TUI omite los conflictos por defecto** y lo indica
    al final; el diálogo de sobrescritura solo existe en la TUI (decisión T6).
14. **La paleta `Ctrl+P` no se abre en la vista Sesión**: en Sesión todas las
    teclas van al remoto salvo el prefijo (§5.5), regla más específica que el
    atajo global.
15. **En la ficha, `?` y `q` escriben en los campos** en lugar de disparar la
    ayuda o salir, para no romper la edición de texto; el resto de atajos
    globales (F2, F3, Ctrl+P) sí funcionan.
16. **Evento de pantalla en lugar de `Datos(bytes)`**: el parser `vt100`
    compartido vive en la tarea de conexión (`Arc<Mutex>`); la UI recibe una
    notificación `Pantalla` coalescida a ~30 fps instead de los bytes, tal
    como describe el requisito de rendimiento.
17. **El desplegable de identidad incluye el tipo de clave** además del
    comentario y la huella («agente · comentario · ed25519 · SHA256:…»).
18. **Sin parpadeo de cursor**: la única animación es el destello de fila
    (§6, notas de UX).

## Estructura de archivos creada/modificada

```
magi/
├── Cargo.toml, Cargo.lock, .gitignore, README.md
├── docs/fase1/
│   ├── magi-fase1-checklist.md           (marcado: 126/128)
│   └── magi-fase1-implementacion.md      (este informe)
├── src/
│   ├── main.rs          CLI (clap), log, arranque
│   ├── lib.rs           módulos públicos del crate
│   ├── app.rs           estado global, bucle de eventos, acciones, diálogos
│   ├── config.rs        config.toml y rutas XDG
│   ├── ficheros.rs      escritura atómica, permisos 600, copias con fecha
│   ├── modelo.rs        Host, Grupo, Etiqueta, IdentidadRef, EstadoSesion,
│   │                    filtro y validaciones (+ tests)
│   ├── tema.rs          tema de Omarchy, respaldo, glifos ASCII
│   ├── teclas.rs        prefijo configurable y tecla → bytes del PTY
│   ├── identidades.rs   escaneo de ~/.ssh/*.pub y del agente
│   ├── almacen/
│   │   ├── mod.rs, migraciones.rs, hosts.rs, grupos.rs, etiquetas.rs
│   ├── sshconfig/
│   │   ├── parser.rs, importar.rs, exportar.rs
│   ├── conexion/
│   │   ├── mod.rs, cliente.rs, salto.rs, huellas.rs, terminal.rs
│   └── ui/
│       ├── mod.rs, hosts.rs, ficha.rs, sesion.rs, paleta.rs,
│       ├── dialogos.rs, barra.rs, ayuda.rs, componentes.rs
└── tests/
    ├── almacen.rs
    └── sshconfig_ida_vuelta.rs
```

## Decisiones técnicas tomadas durante el desarrollo

- **Versiones fijadas y compatibles** (R2 despejado): `ratatui 0.30.2` con
  `crossterm 0.29`, `tui-term 0.3.4` y `vt100 0.16.2`; `russh 0.63.3`,
  `rusqlite 0.40` (bundled), `nucleo-matcher 0.3`, `clap 4.6`, `toml 1`,
  `directories 6`, `zeroize 1.9`, `tracing 0.1` + `tracing-appender 0.2`.
- **Bucle de eventos**: hilo propio para crossterm (no bloquea la UI), tarea
  tokio de tick (200 ms) y canal `mpsc` de tokio ilimitado; el hilo principal
  dibuja solo cuando hay cambios.
- **Verificación de huellas**: `Handler::check_server_key` async de russh con
  `oneshot` hacia la UI; el timeout de 10 s se aplica solo a DNS/TCP, no a los
  diálogos, para no caducar mientras el usuario decide.
- **Saltos**: conexión recursiva por niveles, autenticando cada salto y
  abriendo un canal `direct-tcpip` que se convierte en transporte con
  `Channel::into_stream()`; los `Handle` intermedios se mantienen vivos.
- **Frase de claves**: `Zeroizing<String>` leído con `fs::read_to_string` y
  `decode_secret_key`; tres intentos con diálogo y cancelación con `Esc`.
- **Parser de pantalla**: `Arc<Mutex<vt100::Parser>>` compartido; la tarea
  procesa los datos y notifica cambios cada 33 ms.
- **Sin dependencia de ratón** y sin parpadeos; selección de filas en color
  invertido con el acento del tema.
- **Permisos y atomicidad**: `OpenOptions::mode(0o600)` + rename sobre
  temporal en el mismo directorio; `known_hosts.old`, `config.bak-<fecha>` y
  `known_hosts` siempre 600.
- **Tests**: 25 en total (10 unitarios de modelo y teclas, 9 de almacén en
  memoria y 6 de ssh_config e ida y vuelta), incluidos los casos límite del
  checklist.
- **Corrección posterior (15/09/2026)**: crossterm entrega los bytes de
  control `0x1C`–`0x1F` como `Ctrl+4`…`Ctrl+7`, de modo que `Ctrl+]` llegaba
  como `Ctrl+5` y el prefijo por defecto no coincidía (además de no enviarse
  al remoto). Se normalizan esas equivalencias en `teclas.rs`
  (`es_prefijo`/`bytes_de_tecla`) y se añaden tres tests; verificado de
  extremo a extremo contra un `sshd` efímero: huella nueva aceptada, shell
  remoto, `Ctrl+] q` a segundo plano, `F3` de vuelta, `Ctrl+] x` con
  confirmación y salida limpia.

## Funcionalidades del checklist completadas (copiando su texto exacto)

### Esqueleto y CLI
- Crate binario `magi` con clap: `magi`, `magi importar [ruta]`, `magi exportar`, `magi --version`
- `magi importar [ruta]` funciona sin TUI e imprime el resumen (importados / sobrescritos / omitidos con motivo)
- `magi exportar` funciona sin TUI y regenera `~/.ssh/magi_config`
- Rutas XDG (`directories`): datos en `~/.local/share/magi`, config en `~/.config/magi`, log en `~/.local/state/magi`; equivalentes en macOS
- `config.toml` con `prefijo_escape` (por defecto `Ctrl+]`), `exportar_al_guardar` (por defecto true), `tema` (auto | fijo | ruta) y `terminal_ascii`
- Log con `tracing` en `~/.local/state/magi/magi.log`, rotación diaria, sin frases ni claves
- Bucle de eventos con crossterm + canal `mpsc` de eventos de conexión; la UI nunca bloquea en red
- Restauración del terminal (raw mode, pantalla alternativa) al salir y ante `panic` mediante hook
- Redimensionado de la terminal repinta la vista actual

### Almacén SQLite
- `magi.db` en WAL con `foreign_keys=ON`, creado con permisos 600
- Migraciones numeradas por `PRAGMA user_version` en `almacen/migraciones.rs`
- Tabla `GRUPOS` (id, nombre UQ, orden, plegado, creado_en)
- Tabla `HOSTS` (id, nombre UQ, grupo_id, direccion, puerto, usuario, identidad_ref, salto_host_id, multiplexar, keepalive_seg, opciones_extra, origen, ultimo_estado, ultima_conexion_en, creado_en, actualizado_en)
- Tabla `ETIQUETAS` (id, nombre UQ) y tabla `HOST_ETIQUETAS` (host_id, etiqueta_id) con borrado en cascada
- Sin `CHECK` sobre enumeraciones; `origen` y `ultimo_estado` se validan en código
- Al borrar un grupo sus hosts pasan a `grupo_id = NULL`, nunca se borran
- Al borrar un host que es salto de otros, los dependientes quedan con `salto_host_id = NULL` tras confirmación
- Una etiqueta que se queda sin hosts se elimina
- Checkpoint de WAL al salir
- Tests del almacén con SQLite en memoria (CRUD, cascadas, migración desde vacío)

### Tema y sistema visual
- Lectura de `~/.config/omarchy/current/theme/alacritty.toml` al arrancar: primary.background/foreground, normal.yellow, normal.green, normal.red, bright.black
- Paleta fija de respaldo (`#0A0A0A`, `#EDE8DC`, `#FFA000`, `#00FF6A`, `#FF2A1A`, `#5A5A5A`) cuando no existe el tema de Omarchy
- `tema` en `config.toml` tiene prioridad sobre la detección automática
- Glifos de estado `●` `◐` `○` `✕` `▸` siempre acompañados de color, nunca solo color
- Modo degradado ASCII (`*` `o` `x` `>` y marcos `+ - |`) cuando el locale no es UTF-8 o `terminal_ascii = true`
- Barra inferior siempre visible con los atajos del contexto actual
- Mensajes en la barra inferior durante 4 s o hasta la siguiente tecla; errores en rojo con `✕`
- Destello de una fila del inventario cuando cambia de estado (única animación de la fase)
- Ayuda contextual con `?` que lista los atajos de la vista actual

### Vista Hosts
- Listado agrupado con cabecera de grupo (`▼`/`▶` + nombre + recuento) y hosts «sin grupo» al final
- Columnas: glifo de estado, nombre, dirección, usuario, puerto; indicador `⤴ <salto>` si el host usa salto
- Contador «N hosts» en la cabecera del marco
- Navegación con `↑` `↓` / `j` `k`; un grupo plegado cuenta como una fila
- Plegar y desplegar grupo con `←` `→` / `h` `l`; el estado se persiste en `GRUPOS.plegado`
- Filtro incremental con `/` sobre nombre, dirección, usuario, grupo y etiquetas (subcadena, sin mayúsculas ni acentos, tokens AND)
- Los grupos plegados se despliegan temporalmente mientras hay filtro activo
- `Esc` limpia el filtro y devuelve el plegado anterior
- `Tab` alterna la columna derecha entre usuario·puerto y etiquetas
- Glifo de estado derivado: `●` con sesión viva, `✕` si `ultimo_estado = error`, `○` en el resto
- `e` abre la ficha del host seleccionado; `n` abre la ficha vacía con el grupo del host seleccionado
- `x` borra el host tras diálogo de confirmación
- `I` lanza la importación de `~/.ssh/config` y `E` la exportación de `magi_config`

### Grupos
- Menú de grupo con `g`: nuevo grupo, renombrar, mover host seleccionado a grupo, borrar grupo, subir/bajar orden
- Crear grupo con nombre único (validación de duplicado con mensaje)
- Renombrar grupo conserva sus hosts y su plegado
- Mover host a grupo mediante desplegable filtrable
- Borrar grupo con confirmación indicando cuántos hosts pasarán a «sin grupo»
- Reordenar grupos actualiza `orden` y el listado

### Etiquetas
- Campo de etiquetas en la ficha separado por espacios; se normalizan a minúsculas y sin duplicados
- Autocompletado de etiquetas existentes al escribir en el campo
- Las etiquetas aparecen en la columna alternativa del inventario y en el filtro

### Ficha de host
- Formulario en cuatro bloques: Identificación, Acceso, Al conectar, Opciones extra
- Campos: nombre, dirección, puerto, grupo (desplegable), etiquetas, usuario, identidad (desplegable), salto (desplegable), multiplexar (casilla), mantener (casilla + segundos), opciones extra (área de texto)
- Valores por defecto al crear: puerto 22, keepalive 30 s marcado, identidad «auto», multiplexar desmarcado
- `Tab` / `Shift+Tab` recorren los campos; `↵` abre desplegables; `Espacio` marca casillas
- Los desplegables filtran al escribir y muestran «ninguno» como primera opción donde aplica
- Validación de nombre: obligatorio, único, `[A-Za-z0-9._-]+`
- Validación de dirección obligatoria y puerto 1-65535
- Validación de salto: distinto del propio host, sin ciclos y máximo 3 niveles
- Validación de keepalive 5-600 s cuando «Mantener» está marcado
- Validación de opciones extra: una directiva por línea; se rechazan las que MAGI ya gestiona (HostName, Port, User, IdentityFile, ProxyJump, ControlMaster, ServerAliveInterval)
- `Ctrl+S` guarda, actualiza `actualizado_en` y vuelve al inventario con el host seleccionado
- Si `exportar_al_guardar` está activo, guardar regenera `magi_config`
- `Esc` descarta; con cambios sin guardar pide confirmación
- `Ctrl+T` prueba la conexión sin abrir shell (resolución, handshake, huella, autenticación) y muestra el resultado en la barra

### Identidades
- Escaneo de `~/.ssh/*.pub` con tipo y huella SHA256 de cada clave
- Listado de claves cargadas en el agente vía `SSH_AUTH_SOCK` con tipo, comentario y huella
- Desplegable de identidad con entradas «auto», «agente · <comentario> · <huella>» y «fichero · <ruta>»
- `identidad_ref` se guarda como NULL, `agente:SHA256:…` o `fichero:<ruta>`
- Aviso (no bloqueo) al guardar si la clave `agente:` no está cargada en ese momento
- Si el agente no está disponible, el desplegable lo indica y ofrece solo ficheros

### Importación de ssh_config
- Parser propio de ssh_config: bloques `Host`, `Match`, directiva `Include` (un nivel, ignorando `magi_config`), comentarios y comillas
- Solo se importan bloques `Host` con un único patrón sin `*` ni `?`; el resto se omite con motivo
- Mapeo: HostName→direccion (o nombre si falta), Port→puerto, User→usuario, IdentityFile→identidad_ref fichero, ControlMaster→multiplexar, ServerAliveInterval→keepalive_seg
- Resto de directivas a `opciones_extra` línea a línea, sin pérdida
- `ProxyJump` se resuelve por nombre al terminar la importación; si no existe, la directiva queda en `opciones_extra`
- Hosts importados van al grupo «~/.ssh/config» con `origen = ssh_config`
- Conflicto por nombre existente: diálogo sobrescribir / omitir / sobrescribir todos / omitir todos
- Diálogo resumen final: importados, sobrescritos y omitidos con motivo
- Ruta inexistente o ilegible produce mensaje claro y no modifica el inventario
- Test de ida y vuelta: `importar(exportar(hosts))` reproduce el inventario, incluidas las opciones extra

### Exportación de magi_config
- Generación de `~/.ssh/magi_config` con cabecera de fecha, un bloque `Host` por host, comentario de grupo y bloques ordenados por grupo y nombre
- Directivas emitidas: HostName, Port, User, IdentityFile (solo `fichero:`), ProxyJump (nombre del salto), ControlMaster auto + ControlPath + ControlPersist 10m si multiplexar, ServerAliveInterval si keepalive, opciones extra literales
- Un host de salto se emite antes que los hosts que lo usan
- Escritura atómica (temporal + rename) con permisos 600
- Detección de `Include ~/.ssh/magi_config` en `~/.ssh/config`; si falta, diálogo para insertarla en la primera línea
- Al insertar el Include se guarda copia `config.bak-<fecha>`
- Si `~/.ssh` no existe o no es escribible, mensaje claro y el inventario sigue operativo

### Conexión SSH (russh)
- Cliente russh en tarea tokio con máquina de estados: inactiva, resolviendo, conectando, verificando_huella, autenticando, abierta, en_segundo_plano, cerrada, error, cancelada
- Resolución DNS y conexión TCP con timeout de 10 s; errores con motivo legible en la barra
- Autenticación `auto`: todas las claves del agente y después `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`
- Autenticación `agente:<huella>`: solo esa clave; si no está cargada, error «ejecuta ssh-add»
- Autenticación `fichero:<ruta>`: carga de la clave; si está cifrada, diálogo de frase con 3 intentos
- Frases y claves descifradas en memoria con `zeroize`, destruidas al cerrar la sesión
- Keepalive de russh según `keepalive_seg`
- Salto (`ProxyJump`): sesión al host de salto y canal direct-tcpip hacia el destino, en cadena hasta 3 niveles
- Canal de sesión con request-pty (`xterm-256color`, tamaño real) y shell
- `HOSTS.ultimo_estado` y `ultima_conexion_en` se actualizan al abrir; `ultimo_estado = error` al fallar
- Al conectar con una sesión ya viva, diálogo para cerrarla antes de abrir la nueva
- Al salir de MAGI con sesión viva, confirmación y cierre limpio del canal

### Verificación de huellas
- Comprobación contra `~/.ssh/known_hosts` (entradas hasheadas incluidas) por `direccion` y `[direccion]:puerto` si el puerto no es 22
- Huella desconocida: diálogo con tipo y huella SHA256; `a` acepta y añade línea, `Esc` cancela
- Huella cambiada: diálogo de bloqueo en rojo con huella anterior y nueva; sustituir exige `r` y escribir el nombre del host
- Sustituir una huella deja copia `known_hosts.old` y elimina las líneas previas de esa dirección
- Aceptaciones y sustituciones se registran en el log con fecha, host y huellas

### Vista Sesión
- Terminal remoto renderizado con `tui-term` sobre un `vt100::Parser` que ocupa todas las filas entre el marco y la barra
- Todas las teclas se envían al remoto salvo el prefijo de escape
- Prefijo `Ctrl+]` (configurable) + `q`/`Esc` vuelve a Hosts dejando la sesión en segundo plano
- Prefijo + `x` cierra la sesión tras confirmación
- Prefijo + prefijo envía el prefijo literal al remoto
- `F3` o `↵` sobre el host de la sesión en segundo plano vuelve a la vista Sesión
- Redimensionar la terminal envía `window_change` al remoto y reajusta el parser
- Barra de estado: glifo, host, tiempo conectado, identidad usada y atajos del prefijo
- Cierre del shell remoto (exit) o caída de red devuelve a Hosts con mensaje y estado `cerrada`
- Salida del remoto coalescida a ~30 fps; solo se repinta cuando cambia el buffer

### Paleta de comandos
- `Ctrl+P` abre la paleta centrada desde cualquier vista
- Búsqueda difusa (`nucleo-matcher`) sobre hosts y acciones, con la categoría a la derecha
- Entradas: `conectar · <host>`, `editar host · <host>`, `nuevo host`, `importar ~/.ssh/config`, `exportar magi_config`, `ir a sesión`
- `↑` `↓` mueven, `↵` ejecuta, `Esc` cierra

### Navegación global
- `F2` va a Hosts y `F3` a la sesión activa (si no hay, mensaje en la barra)
- `F1`, `F4`-`F7` muestran «vista no disponible en esta fase» en la barra
- `q` vuelve a la vista anterior o sale; con sesión viva pide confirmación
- `Esc` cierra el diálogo o la paleta abiertos antes que cualquier otra acción
- Ninguna acción destructiva (borrar host, borrar grupo, sustituir huella, cerrar sesión, modificar `~/.ssh/config`) se ejecuta con una sola pulsación

### Calidad
- `cargo clippy` sin avisos y `cargo fmt` aplicado
- README con instalación (`cargo install --path .`), atajos y explicación del `Include`

## Pendientes y bloqueos

1. **CI en GitHub Actions** (`cargo test` verde en CI de GitHub Actions
   (Linux) con los tests de almacén y de ida y vuelta): descartado en esta
   entrega por decisión del desarrollador. Los tests, `clippy -D warnings` y
   `fmt` se ejecutan en local y pasan; el flujo de Actions queda pendiente de
   crear cuando haya remoto.
2. **Compilación y arranque en macOS**: sin acceso a un equipo macOS no se ha
   podido verificar la funcionalidad; el código no contiene ramas específicas
   de plataforma más allá de las rutas XDG (vía `directories`) y de los
   permisos Unix (600), soportados en macOS.
3. **Pruebas manuales con hosts reales** pendientes de que el desarrollador
   las ejecute sobre Omarchy: conexión por agente, conexión por fichero con
   frase, salto en cadena, huella desconocida/cambiada, redimensionado con
   `vim` y `htop` remotos, y cierre por `exit`.
4. Verificación de la **autenticación `agente:<huella>`** con una clave real
   del agente y del **aviso de clave no cargada** al guardar.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

```sh
cargo run                       # TUI (vista Hosts); crea config, BD y log
cargo run -- importar [ruta]    # importa ~/.ssh/config o la ruta indicada
cargo run -- exportar           # regenera ~/.ssh/magi_config
cargo test                      # 25 tests: modelo, teclas, almacén y ssh_config
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

- **Migraciones**: automáticas al abrir; `PRAGMA user_version` en
  `src/almacen/migraciones.rs`. Nunca se edita una migración aplicada.
- **Rutas**: datos `~/.local/share/magi/magi.db`; config
  `~/.config/magi/config.toml`; log `~/.local/state/magi/logs/`. Se pueden
  aislar con `HOME` + `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_STATE_HOME`
  para pruebas.
- **Pruebas realizadas en esta sesión**:
  - `cargo test` (25 verdes), `cargo clippy --all-targets -- -D warnings`
    y `cargo fmt` sin avisos.
  - CLI contra un `HOME`/XDG aislado con el `~/.ssh/config` real: importa 2
    hosts, omite «Host ssh.4d3.org win-gp» con motivo, exporta `magi_config`
    correcto y crea BD/config/log con permisos 600.
  - TUI arrancada en un PTY de 42×130: render de Hosts, ficha con los cuatro
    bloques, menú de grupos, paleta difusa, ayuda `?`, mensaje de F3 y salida
    limpia con `q`, sin panics.
- **Pruebas manuales recomendadas** (desarrollador, con hosts reales):
  1. Crear un host con la clave del agente y conectar; comprobar glifo `●` y
     barra de estado.
  2. `Ctrl+T` con usuario incorrecto: debe mostrar «autenticación rechazada»
     sin abrir sesión.
  3. Conectar a un host con huella nueva (aceptar con `a`) y revisar la línea
     añadida a `known_hosts`.
  4. Forzar una huella cambiada (o usar un host reinstalado) y comprobar el
     bloqueo y la sustitución escribiendo el nombre.
  5. Conectar por un salto y comprobar la cadena y las huellas de cada nivel.
  6. Abrir `vim` y `htop` remotos, redimensionar la ventana y salir con
     `prefijo q`, `F3` y `prefijo x`.
  7. `E` sin `Include` en `~/.ssh/config`: aceptar y comprobar la copia
     `config.bak-<fecha>` y que `ssh <host>` sigue funcionando.
  8. `I` con un nombre en conflicto: probar sobrescribir, omitir y las
     variantes «todos».

> Aviso para el desarrollador: entregar este informe al analista funcional
> antes de especificar la Fase 2; hasta entonces, el checklist maestro y el
> informe maestro no reflejan la realidad implementada.
