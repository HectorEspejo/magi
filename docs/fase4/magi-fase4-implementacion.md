# MAGI - Fase 4: Archivos (SFTP en panel doble)

## Informe de Implementación

**Última actualización:** 15 de septiembre de 2026 (sesión 1: fase completa)

---

## Resumen de lo implementado

La Fase 4 está **implementada al completo** (65/65 del checklist). MAGI gana la
vista **Archivos** (`F4`): un panel doble local ⇄ remoto sobre SFTP con marcas
de diferencia y una cola de transferencias que vive en el servidor de sesiones,
de modo que sobrevive a cerrar la ventana y la ven todas las ventanas abiertas.

- **Protocolo v2** (`src/protocolo.rs`): `VERSION_PROTOCOLO = 2`. Diez mensajes
  nuevos del cliente (`AbrirSftp`, `ListarDir`, `Transferir`,
  `CancelarTransferencia`, `LimpiarTransferencias`, `BorrarRemoto`,
  `RenombrarRemoto`, `CrearDirRemoto`, `DescargarTemporal`, `BorrarTemporal`) y
  cinco del servidor (`SftpAbierto`, `DirListado`, `Transferencias`, `Hecho`,
  `RutaTemporal`). `Error` gana `peticion_id` opcional y `Bienvenida` lleva la
  cola actual, para que una ventana nueva la vea entera. Un servidor de la
  versión anterior se rechaza en el saludo y la barra dice «servidor de una
  versión anterior: magi servidor parar».
- **Migración 3**: `HOSTS.sftp_dir_local` y `HOSTS.sftp_dir_remoto` (nulos
  mientras no se haya navegado con ese host). Sin tablas nuevas. `REGISTRO`
  gana `transferencia` y `borrado_remoto`, y el filtro `t` de la vista Registro
  estrena el grupo «archivos».
- **Servidor SFTP** (`src/servidor/sftp.rs`): un canal `SftpSession` por host,
  abierto sobre un canal de la conexión del pool (`multiplexar`, o conexión
  propia si el host no lo tiene) y compartido por todas las ventanas. Si no hay
  conexión viva, la abre con el flujo de la Fase 3 —huella, frase y contraseña
  al solicitante— reutilizando el mismo puente de eventos. Listar, borrar
  recursivo, renombrar, crear directorio, temporales para el visor (700/600) y
  cierre del canal tras diez minutos sin actividad y sin transferencias. El
  canal **no** es una sesión: no aparece en pestañas ni en la difusión
  `Sesiones`.
- **Cola de transferencias** (`src/servidor/transferencias.rs`): una en curso
  por host y FIFO, con hosts distintos en paralelo; expansión de directorios
  (bajada con `readdir` recursivo, subida con la lista que ya ha expandido el
  cliente), bloques de 64 KiB, destino escrito como `.magi-parcial` y
  renombrado al terminar, `mtime` conservado, política de conflicto recomprobada
  justo antes de escribir, cancelación comprobada por bloque con borrado del
  parcial, `borrar_origen` solo tras `hecha`, terminadas conservadas una hora,
  difusión `Transferencias{lista}` coalescida a cuatro por segundo y anotación
  en `REGISTRO` al terminar (nunca contenido).
- **Cliente**: `archivos/` (modelo de panel, listado local, marcas y avisos de
  sensibles como módulos puros y probados), `ui/archivos.rs` con los dos
  paneles del mismo widget —activo en ámbar, inactivo tenue— columnas de
  nombre/tamaño/fecha relativa, marcas al final de fila y cola de tres filas al
  pie, y `ui/transferencias.rs` con la cola ampliada, el detalle inferior y la
  velocidad y el restante calculados con las dos últimas difusiones. Copiar,
  mover, borrar, renombrar y crear directorio; diálogo de conflicto con tamaño
  y fecha de los dos lados (`s`/`o`/`S`/`O`); aviso de sensibles recorriendo
  directorios y `Esc` sin encolar nada; ver con `$PAGER` suspendiendo la TUI.
- **Navegación**: `F4`, `s` en Hosts y Flota, prefijo `f` en Sesión, paleta
  (`sftp · <host>`, `transferencias`, `cancelar transferencias`) y `t` para la
  cola. `F6` sigue reservada.

## Desviaciones respecto a la especificación (qué y por qué)

1. **Peticiones en vuelo por fichero: las gobierna `russh-sftp`, no un 16
   fijo.** El checklist pide «bloques de 64 KiB con hasta 16 peticiones en
   vuelo por fichero». Los bloques son de 64 KiB y el fichero va canalizado,
   pero el número de peticiones simultáneas lo negocia `russh-sftp` con la
   extensión `limits@openssh.com` que anuncia el servidor (OpenSSH pide 64), y
   el tipo `File` de la biblioteca no expone una ventana configurable sin bajar
   a su capa raw, que `SftpSession` no deja alcanzar. Funcionalmente es lo
   pedido (canalización con varios bloques en vuelo) con otro número; queda
   anotado por si el analista quiere el 16 exacto.
2. **`VersionIncompatible` cambia de sentido: ahora lleva la versión del
   servidor.** Antes (v1) el servidor devolvía la versión que le había mandado
   el cliente, así que `magi servidor estado` imprimía la del propio MAGI como
   si fuera la del servidor, y era imposible distinguir «servidor viejo» de
   «cliente viejo», que es justo lo que pide el checklist. Como es un cambio de
   semántica de un mensaje existente, va cubierto por el salto a v2. La
   prueba `una_version_distinta_no_coopera` se ha actualizado.
3. **El «id de sesión» de los diálogos de conexión pasa a ser «id de
   solicitud».** Una apertura de canal SFTP necesita los mismos diálogos
   (huella, frase, contraseña) y no tiene sesión propia. En vez de duplicar
   cinco mensajes nuevos del protocolo, el campo `sesion_id` de
   `HuellaDesconocida`, `HuellaCambiada`, `PideFrase`, `PideContrasena` y de
   sus respuestas significa ahora «id de la solicitud de conexión», que sale
   del mismo contador de sesiones (nunca colisiona) y es opaco para el cliente,
   que ya mostraba el `host` de cada mensaje. Documentado en el módulo del
   protocolo.
4. **Plazo de 10 s en el saludo del subsistema SFTP.** `request_subsystem` en
   russh devuelve `Ok` aunque el servidor conteste «no»: la negativa llega
   como un mensaje del canal que `SftpSession::new` no mira, de modo que un
   host sin subsistema `sftp` se quedaba esperando para siempre. Se envuelve
   el saludo en el mismo plazo de 10 s que ya usa DNS/TCP y se contesta «el
   host no ofrece SFTP».
5. **El modelo del panel vive en `archivos/panel.rs`.** El informe situaba el
   modelo de panel en `archivos/mod.rs`; se ha separado en `panel.rs` para que
   `mod.rs` quede con las utilidades puras (fechas, tamaños, permisos) y el
   modelo con su estado. Misma carpeta, mismos tipos.
6. **El visor vive en `visor.rs` + `ui/pager.rs`.** El informe decía «la TUI se
   suspende y se restaura, como al conectar en F1»; hoy no existe tal
   suspensión, así que se añade: `visor.rs` decide el paginador (programa
   + argumentos, sin shell) y `ui/pager.rs` hace la secuencia de suspender,
   ejecutar y restaurar. Además el hilo que lee el teclado se pausa mientras el
   paginador está delante; sin eso sus pulsaciones se encolaban y se ejecutaban
   todas al volver.
7. **Aviso de que el visor configurado no está.** `[archivos] pager` (o
   `$PAGER`) que no exista no deja al usuario tirado: se usa `less` o `more` si
   están y la barra lo dice, en vez de callarse el cambio.
8. **La cola de transferencias se implementó con el servidor SFTP, en el mismo
   bloque de trabajo.** El plan de la fase repartía `servidor/transferencias.rs`
   al sprint de transferencias; al ir a enlazar los mensajes del protocolo
   quedaba un `Transferir` que no hacía nada, así que el motor se hizo junto al
   canal. El reparto por sprints es una estimación, no alcance.
9. **Supervisor de la cola.** Para que el motor no se lance a sí mismo (el
   compilador no sabe probar que ese futuro recursivo sea `Send`) hay una tarea
   supervisora que recibe avisos y arranca lo que pueda empezar. Es un detalle
   interno; el comportamiento es el del informe.
10. **Propietario numérico.** SFTP v3 solo manda `uid`/`gid`; los nombres que
    algunos servidores añaden no llegan con OpenSSH. En remoto se muestra
    `1000:1000` y en local `usuario:grupo` resuelto con `nix`. No hay forma de
    evitarlo sin ejecutar un comando en el host, y el proyecto prohíbe
    interpolar rutas y comandos.
11. **«`m` en el mismo panel equivale a `r`».** En esta vista los dos paneles
    están siempre presentes, así que `m` siempre es un movimiento entre
    paneles. El caso «mismo panel» del informe no se puede dar.
12. **La marca `≠` de los directorios.** El pseudocódigo del §7.1 marcaría `≠`
    cuando un lado es directorio y el otro no, pero el propio informe («los
    directorios solo `✕`») y el checklist dicen lo contrario. Se implementa lo
    que dice el checklist: un directorio solo se marca `✕` cuando falta.

## Estructura de archivos creada/modificada

**Nuevos**

| Fichero | Contenido |
|---|---|
| `src/archivos/mod.rs` | Utilidades puras: fechas relativas y completas, tamaños legibles, permisos en notación de `ls` |
| `src/archivos/marcas.rs` | `Entrada`, `TipoEntrada`, `Marca` y el cálculo de `≠`/`✕` (lineal) |
| `src/archivos/local.rs` | Listado local, recorrido recursivo, borrar/renombrar/crear y metadatos |
| `src/archivos/panel.rs` | `Panel`, `EstadoArchivos`, peticiones en vuelo, operación pendiente y aviso |
| `src/archivos/sensibles.rs` | Globs de `[archivos] avisar` sobre nombres de fichero |
| `src/visor.rs` | Comando del paginador sin shell y búsqueda de alternativa |
| `src/servidor/sftp.rs` | Canal por host, apertura sobre el pool, operaciones, temporales y cierre por inactividad |
| `src/servidor/transferencias.rs` | Cola, expansión, motor de copia, cancelación, retención y registro |
| `src/ui/archivos.rs` | Vista de los dos paneles, cola al pie y aviso de sensibles |
| `src/ui/transferencias.rs` | Cola ampliada con detalle, velocidad y restante |
| `src/ui/pager.rs` | Suspender la TUI, ejecutar el paginador y restaurarla |
| `tests/comun/mod.rs` | Harness compartido: entorno aislado y servidor SSH en proceso con subsistema `sftp` |
| `tests/sftp.rs` | 17 pruebas de extremo a extremo contra el `sftp-server` real |

**Modificados**

`Cargo.toml` (+`russh-sftp = "=3.0.0"`, `globset`, `filetime`; features `fs` y
`process` de tokio), `src/lib.rs`, `src/protocolo.rs`, `src/registro.rs`,
`src/modelo.rs`, `src/config.rs`, `src/almacen/migraciones.rs`,
`src/almacen/hosts.rs`, `src/almacen/mod.rs`, `src/cliente/mod.rs`,
`src/servidor/mod.rs`, `src/servidor/sesiones.rs`, `src/servidor/conexiones.rs`,
`src/servidor/difusion.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/archivos.rs`,
`src/ui/barra.rs`, `src/ui/ayuda.rs`, `src/ui/dialogos.rs`, `tests/servidor.rs`,
`README.md`.

## Decisiones técnicas tomadas durante el desarrollo

- **`AbrirSftp` sin `peticion_id`.** La respuesta `SftpAbierto{host_id,
  dir_inicio}` ya identifica al host, y el cliente solo mantiene una apertura
  en vuelo a la vez (la del host que muestra). Los errores de la apertura
  llegan como `Error` sin `peticion_id`, y el cliente los interpreta como «este
  host no tiene remoto» solo si había una apertura pendiente.
- **Serialización de la apertura por host.** Un cerrojo por host
  (`Arc<Mutex<()>>` en el estado) con doble comprobación: dos ventanas que
  pidan el mismo host a la vez comparten una única conexión. Sin él,
  `pool.guardar` de la segunda habría borrado el contador de canales de la
  primera y podía dejar colgando la conexión de una sesión viva.
- **El canal SFTP cuenta como canal del pool.** Se contabiliza al reutilizar y
  se libera al cerrar; si no, la gracia de 30 s del pool habría cerrado la
  conexión a los 30 s de inactividad y la siguiente operación habría fallado de
  forma intermitente.
- **El servidor no se apaga con transferencias vivas.** `revisar_inactividad`
  solo cuenta clientes y sesiones; ahora también la cola, porque cerrar la
  última ventana no puede dejar a medias una copia encargada.
- **Todas las operaciones de archivos se atienden en una tarea propia.** El
  bucle de lectura de cada cliente no puede quedarse esperando a la red: abrir
  un canal puede tardar (diálogos incluidos) y expandir un directorio remoto es
  potencialmente largo. Atenderlas en línea habría bloqueado al cliente, que
  no habría podido ni mandar su respuesta al diálogo.
- **`peticion_id` resuelto por eventos, sin `oneshot`.** El bucle de UI es
  síncrono y no puede esperar; el cliente guarda qué pidió con cada id y
  descarta las respuestas de peticiones que ya no están en vuelo (evita que un
  `g` rápido pinte el directorio abandonado).
- **La expansión de una subida la hace el cliente** (§7.5): manda la lista
  plana de directorios y ficheros, con la política de su elemento de primer
  nivel en cada uno, que es como se cumple que «la decisión de un directorio
  valga para todo su contenido».
- **Difusión de la cola con marca sucia y una revisora de 250 ms.** Los cambios
  de estado salen de inmediato; el progreso, como mucho cuatro veces por
  segundo. La lista se construye con el bloqueo tomado y se difunde con él
  suelto.
- **El progreso no se publica en cada bloque** (serían miles de bloqueos por
  segundo sobre el mutex global del servidor): como mucho cada 100 ms y siempre
  al terminar cada fichero.
- **Cancelación por bandera atómica**, leída sin tomar el bloqueo en cada
  bloque de 64 KiB. El parcial se borra y el estado final lo pone el motor.
- **`$PAGER` con la ruta como argumento suelto** (T30) y sin shell; el comando
  se parte por espacios en blanco en `visor.rs`.
- **`..` es una fila más del panel**, no un caso especial del widget: la añade
  el listado, de modo que la selección, el filtro, las marcas y el pie la
  tratan igual. Se ve aunque haya filtro, porque si no no habría forma de subir
  con el filtro puesto.
- **Sin listado remoto no se marca nada.** Marcar todo como «no está al otro
  lado» mientras el canal abre (o cuando el host no tiene SFTP) sería mentira.
- **Cambiar de host con `h` conserva el panel local**, como pide el checklist.
- **El borrado del origen local de un «mover» lo hace la ventana que lo
  encoló**, y solo cuando llega `hecha`: el servidor no puede tocar el disco
  local de otro modo que no sea una bajada.

## Funcionalidades del checklist completadas

Las 65 del fichero `docs/fase4/magi-fase4-checklist.md`, que quedan marcadas
como hechas con su texto intacto. Por bloques:

- **Almacén y protocolo (6)**: migración 3, `VERSION_PROTOCOLO = 2` y rechazo
  del servidor anterior, los quince mensajes nuevos con `peticion_id` en
  `Error`, la cola en `Bienvenida`, los tests de protocolo v2 y los tipos
  `transferencia`/`borrado_remoto` con el filtro «archivos».
- **Servidor: SFTP (9)**: `russh-sftp` fijado, canal por host sobre el pool con
  los diálogos de la Fase 3, canal que no cuenta como sesión y se cierra a los
  diez minutos, `ListarDir` con nombre/tipo/tamaño/mtime/permisos/propietario,
  borrado recursivo, renombrado y creación de directorio con su anotación,
  temporales 700/600 que se vacían al apagar, host sin SFTP, conexión caída con
  reapertura y la cola en `magi servidor estado`.
- **Servidor: cola (15)**: `Transferir` con id creciente y expansión, una en
  curso por host con FIFO y hosts en paralelo, bloques de 64 KiB con parciales,
  bajadas escritas en disco local y directorios remotos creados, `mtime`
  conservado, política por elemento recomprobada con su recuento de omitidos,
  cancelación en cola y en curso, error que no para la cola, `borrar_origen`
  solo tras `hecha`, enlaces simbólicos, difusión coalescida, retención de una
  hora, anotación en `REGISTRO` y las pruebas de extremo a extremo.
- **Cliente: paneles (13)**, **marcas y avisos (4)**, **operaciones (9)** y
  **cola (5)**: la vista con sus dos paneles, navegación, filtro, ocultos,
  marcados, detalle, ir a ruta, cambio de host, copiar/mover/borrar/renombrar/
  crear, ver con `$PAGER`, el diálogo de conflicto, el aviso de sensibles, la
  cola de tres filas, la vista ampliada y las entradas de paleta.
- **Configuración y navegación (3)** y **Calidad (3)**: sección `[archivos]`,
  `F4` operativo con `F6` reservada, ayuda en las dos vistas, `clippy`, `fmt`,
  `cargo test` y README.

## Correcciones tras la revisión adversarial (misma sesión)

Antes de dar la fase por cerrada se pasaron dos revisiones adversariales sobre
el código nuevo (servidor y cliente). Encontraron trece defectos reales; se han
corregido todos y el que era una pérdida de datos tiene prueba de regresión.

**Pérdida de datos**

1. Un «mover» con política `omitir` borraba del origen lo que **no** se había
   copiado: el motor daba la transferencia por `hecha` y borraba el origen
   entero, incluidos los ficheros omitidos. Ahora solo se borra lo que se copió
   de verdad (`copiados` en el motor) y, en una subida, la ventana no borra
   nada si la transferencia tuvo omitidos: avisa de que el origen sigue donde
   estaba. Prueba nueva:
   `mover_con_omitir_no_borra_del_origen_lo_que_se_omite`.
2. El emparejamiento entre una subida con `borrar_origen` y su fila en la cola
   era **posicional** (la primera entrada de una lista global): con dos hosts a
   la vez podía borrar los orígenes equivocados, y al cambiar de host se perdía
   la asociación y el «mover» quedaba en copia. Ahora se empareja por
   (host, primer origen) y el estado vive en el `App`, no en la vista.

**Servidor**

3. Fuga en la contabilidad del pool: si `channel_open_session`, el subsistema o
   el saludo SFTP fallaban en un host con `multiplexar`, el canal contado no se
   devolvía nunca y la conexión no se cerraba jamás. Ahora todas las ramas de
   error sueltan lo que tomaron.
4. `Pool::guardar` sustituía la entrada del pool sin desconectar la anterior
   (conexión huérfana) y `liberar` descontaba a ciegas, de modo que el cierre
   de un canal SFTP podía dejar a cero el contador de la conexión de una sesión
   viva y tumbarla. Ahora `guardar` devuelve la desplazada para cerrarla y el
   canal suelta **su** conexión (`liberar_si_es`, por identidad).
5. Una transferencia podía quedarse `EnCurso` para siempre con un host que deja
   de responder sin cerrar la conexión, y eso bloqueaba hasta el apagado
   automático del servidor. Cada bloque tiene ahora un plazo de 60 s.
6. Los `.magi-parcial` solo se borraban al cancelar: cualquier error de lectura,
   escritura o renombrado dejaba el parcial en el destino para siempre. Ahora
   se borra en todas las salidas de error.
7. El motor no refrescaba el reloj de actividad del canal, así que una copia de
   más de diez minutos dejaba el canal «inactivo» y se cerraba nada más
   terminar. Ahora lo refresca al publicar progreso.
8. La purga por retención no se difundía: las ventanas seguían enseñando
   transferencias que el servidor ya había quitado.

**Cliente**

9. La altura con la que se movía el cursor de un panel contaba una fila de más
   cuando la cola estaba visible, así que el cursor podía salirse de la ventana
   y quedarse sin fila resaltada. La regla vive ahora en un solo sitio
   (`ui::archivos::alto_panel`).
10. La vista Transferencias no se desplazaba nunca: `desplazamiento_cola` solo
    se ponía a cero. Ahora la lista se desplaza como la de un panel.
11. Cualquier error del servidor sin `peticion_id` que llegara mientras había
    una apertura de canal en vuelo se tomaba por «este host no ofrece SFTP» y
    dejaba el panel remoto inservible (por ejemplo, «el host ya está abierta»
    de una sesión). Ahora solo se interpreta así si el mensaje lo dice.
12. Renombrar y borrar volvían a mirar el panel al confirmar en vez de usar lo
    que el usuario tenía delante: un refresco entre abrir el diálogo y
    contestar podía renombrar el directorio padre (`..`) o borrar en otro
    directorio. Ahora el diálogo lleva las rutas completas fijadas al abrirse.
13. Detalles: refrescar un directorio ya no pierde las marcas ni el cursor;
    entrar en `..` sube al padre en vez de encadenar `/..` en la ruta; un
    directorio local que no se puede leer deja el panel donde estaba; el visor
    no se abre si ya no se está en Archivos (y borra el temporal); el alto de
    la terminal se refresca al volver del visor; y el pie de la cola cuenta
    «en cola» por estado, no por resta.

## Pendientes y bloqueos

- **Nada pendiente de la fase.** Queda por hacer la validación manual de la
  vista (ver «Ejecución y pruebas»), que en este proyecto es la única forma de
  comprobar la interfaz: no hay pruebas automáticas de render.
- **R15 sigue abierto**: F2 y F3 se dieron por buenas sin validación manual
  completa, y F4 se ha construido encima. Durante este trabajo no ha aparecido
  ningún defecto de F2/F3 salvo el de `VersionIncompatible` (desviación 2), que
  se ha arreglado aquí.
- **`russh-sftp` está fijado a `=3.0.0`.** No subirlo sin subir `russh` y
  repetir las pruebas de la vista Sesión, como manda el `CLAUDE.md`.

## Ejecución y pruebas

**Arrancar**

```sh
cargo run -- --servidor   # en una terminal (primer plano, con su log)
cargo run                 # en otra: la TUI
```

Para probar sin tocar el inventario real, aislar el entorno (recordando que
`directories` resuelve el hogar por uid, así que hay que fijar los cuatro):

```sh
export HOME=$PWD/prueba XDG_DATA_HOME=$PWD/prueba/datos \
       XDG_CONFIG_HOME=$PWD/prueba/config XDG_STATE_HOME=$PWD/prueba/estado \
       XDG_RUNTIME_DIR=$PWD/prueba/runtime
```

**Migrar**: la migración 3 se aplica sola al abrir la base (`PRAGMA
user_version` pasa de 2 a 3). No hay que hacer nada a mano; los hosts
existentes quedan con los dos directorios a nulo y arrancan en `~` y en el
directorio de inicio del usuario remoto.

**Probar a mano** (lo que falta por validar):

1. `F4` con un host: se abre Archivos, el panel local en `~` y el remoto en el
   directorio de inicio del host (o donde se quedó la última vez).
2. `Tab` cambia de panel; `↑` `↓` mueven; `↵` entra en directorios; `-` sube.
3. Marcar dos o tres ficheros con `Espacio` y copiarlos con `c` a un directorio
   del otro panel; comprobar el aviso de sensibles con un `.env` y el diálogo de
   conflicto copiando un fichero que ya exista (probar `o` y `O`).
4. Con una copia en curso, `t` para ver la cola, `x` para cancelarla y
   comprobar que no queda ningún `.magi-parcial`.
5. Abrir una segunda ventana (`Ctrl+P` → «nueva ventana») y comprobar que su
   bienvenida ya trae la cola; `magi servidor estado` desde fuera, también.
6. `↵` sobre un fichero para verlo con el paginador y comprobar que la TUI
   vuelve limpia (sin teclas perdidas) y que el temporal se ha borrado de
   `$XDG_RUNTIME_DIR/magi/tmp`.
7. Un host sin subsistema SFTP: el panel remoto queda con el aviso.
8. `x`, `r` y `d` en local y en remoto; `i` para el detalle y `h` para cambiar
   de host conservando el panel local.
9. En la vista Registro, `t` hasta «archivos»: ahí deben aparecer las
   transferencias y los borrados remotos.

**Pruebas automáticas** (`cargo test`): 146 en total. Las nuevas de esta fase:

- `src/archivos/` — marcas (incluido el criterio de aceptación de
  `config.yaml`), sensibles (con el `.env` dentro de un directorio y el `+N`),
  listado y recorrido local, fechas y tamaños, paneles (filtro, marcas,
  selección, `..`, marcas sin listado remoto).
- `src/servidor/` — cola (FIFO por host, paralelo entre hosts, cancelación,
  caída, retención, detalle del registro), difusión coalescida, cierre por
  inactividad y sus temporales.
- `src/protocolo.rs` — ida y vuelta de los quince mensajes nuevos, `Error` sin
  `peticion_id` y versión 2.
- `tests/sftp.rs` — 17 pruebas de extremo a extremo contra el `sftp-server`
  real de OpenSSH: abrir, listar (tipos, tamaños, mtime, permisos), ruta
  inexistente con su `peticion_id`, host sin subsistema, borrado recursivo con
  su anotación, renombrar y crear directorio, temporales con sus permisos,
  subir y bajar conservando el `mtime`, bajar un árbol entero, omitir enlaces a
  directorio, conflicto con `omitir` y su recuento, cancelar en curso sin
  dejar parcial, anotación en `REGISTRO`, crear los directorios que falten y la
  cola en la bienvenida de una segunda ventana. Si el sistema no trae
  `sftp-server`, se saltan con un aviso.
