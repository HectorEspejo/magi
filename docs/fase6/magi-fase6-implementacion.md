# MAGI - Fase 6: Snippets y Deliberación MAGI

## Informe de Implementación

**Última actualización:** 27 de septiembre de 2026 (sesión 1: fase cerrada sin la revisión adversarial de S1-S3)

---

## Resumen de lo implementado

La Fase 6 está **implementada** (79/80 del checklist): todas las funcionalidades salvo la
revisión adversarial de cierre del código de S1-S3, que el desarrollador decidió saltar para
cerrar la fase (ver «Pendientes y bloqueos»). Son cuatro commits de sprint sobre la rama
`fase6-snippets` más el de cierre:

| Sprint | Commit | Contenido |
|---|---|---|
| S0 | `7406a4d` | Corrección 3b del pool (checklist 1-9) |
| S1 | `dc37eba` | Almacén (migración 5), lógica de snippets y variables, vista `F8`, modelo de la deliberación |
| S2 | `5f38a90` | Servidor de ejecuciones, protocolo v4 de ejecución, lanzamiento, vista Resultados, snippet al conectar, atajos |
| S3 | `f8206e8` | Deliberación MAGI, SFTP de comprobación, ficha (snippet al conectar y verificaciones), detalle en Registro, README |
| Cierre | — | Checklist e informe (la revisión adversarial de cierre se saltó por decisión del desarrollador) |

- **Corrección 3b** (`src/servidor/conexiones.rs`, reescrito): `conexion_para_canal(host,
  política, uso)` con un cerrojo por host que cubre «mirar el pool / conectar / guardar» es la
  única forma de obtener conexión para pestañas, SFTP, túneles, `Ejecutar` y ejecuciones.
  `Pool::guardar` ya no desplaza nunca (devuelve la conexión si hay entrada). Las pestañas
  reutilizan el pool con `multiplexar`; sin él abren conexión propia que no entra en el pool.
  Además: aperturas cancelables y con plazo de red, canales abiertos con plazo sin huérfanos,
  `Hecho` después de que el hilo escritor confirme la fila, `SIGTERM` tratado como `Parar`,
  apagado que anota antes de tocar la red, pid del servidor de otra versión en el aviso y en
  `magi servidor parar` (con `SIGTERM` tras confirmar; `--si` la omite).
- **Almacén** (migración 5): `SNIPPETS`, `SNIPPET_DESTINOS` (índices únicos parciales por
  etiqueta y por host), `VERIFICACIONES_HOST`, `DELIBERACIONES` y
  `HOSTS.snippet_al_conectar_id` con `ON DELETE SET NULL`; sin `CHECK`, validación en código.
  Tipos nuevos de `REGISTRO` (`snippet_ejecutado`, `deliberacion_aprobada|forzada|cancelada`)
  y grupo «snippets» del filtro `t`.
- **Lógica pura** (`src/snippets/`): validación, destinos por etiqueta y host resueltos sin
  duplicados en el orden de Hosts, variables `{{nombre}}`/`{{nombre:defecto}}` sustituidas
  con `shell-escape` (y rechazadas dentro de comillas o de heredoc), saneado de salida
  (CSI/OSC/ESC, controles, `\r`), fichero de salida y `magi snippets`.
- **Vista Snippets (`F8`)**: lista con destino resumido, `CRÍTICO` y total; detalle con
  comando, destinos resueltos, confirmación y por qué, timeout y uso; filtro `/`; formulario
  modal (comando multilínea, destinos con autocompletado de etiquetas y desplegable de hosts,
  crítico, timeout, parar al primer fallo, `Ctrl+S`); borrado confirmado que avisa si es
  snippet al conectar; entradas de paleta; `!` en Hosts y Flota.
- **Lanzamiento** (`src/app/lanzar.rs`, `src/ui/ejecutar.rs`): diálogo EJECUTAR (rejilla de
  casillas por host, variables con defecto, parar al primer fallo, timeout), `a` (todos los
  destinos), `p` (una pestaña por host con el comando escrito al abrir, confirmación si son
  más de 5, seguimiento que anota `snippet_ejecutado` «en pestaña»), decisión de deliberación
  y `LanzarEjecucion` con el comando ya sustituido.
- **Servidor de ejecuciones** (`src/servidor/ejecuciones.rs`): despachador FIFO por ejecución
  con semáforo de 8 hosts, conexión no interactiva por `conexion_para_canal`, canal `exec` con
  stdout y stderr separados (tope 1 MiB por flujo, marca `truncada`), código de salida,
  timeout del snippet (cierra el canal y el pool sigue vivo), «parar al primer fallo»,
  cancelación, estados de host y de ejecución, difusión `Ejecuciones{lista}` sin salida,
  `PedirSalida` → `Salida` solo al que la pide, `snippet_ejecutado` por host, cierre de la
  deliberación (`ejecucion_resultado` y `deliberacion_aprobada|forzada`) por el hilo escritor,
  retención de 1 h, `LimpiarEjecuciones`, no apagado por inactividad con ejecuciones en curso y
  listado en `magi servidor estado`. `AbrirSesion` lleva los comandos iniciales.
- **Vista Resultados** (`src/app/resultados.rs`, `src/ui/resultados.rs`): ejecuciones (`⚑`
  si forzada) y hosts de la seleccionada, visor de stdout/stderr saneado y desplazable que se
  refresca cada 1 s mientras el host corre, `s` guardar en fichero (600, `ficheros.rs`), `x`
  cancelar, `r` repetir (volviendo a deliberar si procede), `C` limpiar. Las ejecuciones llegan
  también en `Bienvenida`.
- **Deliberación MAGI** (`src/deliberacion/`, `src/app/dialogo_magi.rs`,
  `src/ui/deliberacion.rs`): MELCHIOR-1 salud (último sondeo NOMINAL reciente o sondeo nuevo no
  interactivo), BALTHASAR-2 backup (`AbrirSftp` de comprobación + `ListarDir`, fichero más
  reciente que casa con el patrón), CASPER-3 tests (`sh -c` local en su propia sesión, grupo
  matado al vencer el plazo); todas en paralelo con plazo duro de 2 s desde el inicio; tabla
  host × comprobación con `✓` `✕` `—` `◐` y el dato; consenso por unanimidad; `Ctrl+K` única
  tecla que ejecuta; `f` + motivo ≥ 10 caracteres para forzar; `Esc` cancela y anota
  `deliberacion_cancelada`; fila en `DELIBERACIONES` al resolver; detalle en Registro.
- **Ficha de host**: «Snippet al conectar» en «Al conectar» (desplegable de no críticos sin
  variables) y bloque «Verificaciones previas (deliberación MAGI)» (salud, backup con ruta y
  patrón, tests con comando local).
- **Atajos de Flota** (`[flota.atajos]`): tecla → snippet sobre el host seleccionado, validados
  al arrancar (teclas de Flota o snippets inexistentes se ignoran con aviso) y en la barra.
- **Ayuda `?`** en Snippets, Resultados y el diálogo MAGI; README con las secciones nuevas.

## Desviaciones respecto a la especificación (qué y por qué)

Decisiones del desarrollador (27 sep 2026) que concretan puntos abiertos del informe:

1. **Pool salvo pestañas.** SFTP, túneles, `Ejecutar` y ejecuciones usan y guardan siempre la
   conexión del pool, aunque el host no tenga `multiplexar`; `multiplexar` solo decide si las
   pestañas la comparten. Una única función (`usa_pool(host, uso)`) concentra la regla.
2. **Al reconectar solo se repite el «snippet al conectar»**; el comando de `p` se escribe una
   sola vez (la línea 69 dice «también al reconectar» sin distinguir: repetir el comando de un
   `p`, que puede ser un reinicio, en cada reconexión sería peligroso).
3. **Una variable dentro de comillas o de un heredoc se rechaza al guardar** (y otra vez al
   sustituir): ahí el escape de `shell-escape` no protege, porque el valor ya está dentro de
   un entrecomillado.
4. **Un atajo de Flota, `!` o `snippet · <nombre> · <host>` sobre un host que no es destino
   del snippet avisa ««snippet» no apunta a «host»» y no ejecuta.**

Protocolo y servidor:

5. **Protocolo v4 más allá de la tabla §5.2**:
   - `Ejecutar`, `Ejecutado` y `SinSesion` llevan `peticion_id` (antes se emparejaban por
     `host_id` y dos sondeos al mismo host se cruzaban; el cliente espera por id).
   - `AbrirSftp{peticion_id?, no_interactivo}` y `SftpAbierto{peticion_id?}`: el SFTP de
     BALTHASAR-2 es una petición esperable que no dialoga.
   - `PideLlavero{sesion_id, host, usuario}`: la contraseña de una ejecución solo sale del
     llavero de la ventana que la lanzó; es un mensaje distinto de `PideContrasena` para que
     el cliente no pueda abrir un diálogo por error.
   - `AbrirSesion.comandos_iniciales: Vec<ComandoInicial{texto, repetir}>` en vez de
     `comando_inicial`: una pestaña de `p` a un host con snippet al conectar lleva los dos (el
     del host con `repetir`, el de `p` sin él; desviación 2).
   - `VersionIncompatible{version, pid?}`.
   - `Cerrar{sesion_id}` también cancela los diálogos pendientes de SFTP, túnel o ejecución de
     esa ventana.
6. **pid del par con `tokio::net::UnixStream::peer_cred()`** (que es `SO_PEERCRED` en Linux y
   `LOCAL_PEERPID` en macOS) en vez de `nix::getsockopt`, que solo existe en Linux.
7. **«Parar al primer fallo» también ante `error`** (conexión o timeout), como dice la línea 61;
   el diagrama de estados solo mencionaba `fallo`.
8. **`conexion_abierta` se anota solo en la primera pestaña sobre una conexión** (lo pide el AC
   de la línea 6); una pestaña que reutiliza el pool no la vuelve a anotar.
9. **El SFTP de comprobación no cuenta para el ciclo automático de túneles**
   (`ModoSftp::Comprobacion`): si contara, cada deliberación con backup levantaría los túneles
   automáticos del host. Cuando Archivos usa después ese mismo canal, pasa a contar.
10. **Arreglos de las Fases 3-5 necesarios para la 3b** (fuera del checklist): el `sshd` de
    pruebas atiende `exec`; `Ejecutar` tiene plazo (4 s en el servidor, por debajo de los 5 s
    del cliente, para que el paso a la conexión efímera llegue), se ejecuta en su tarea y lee
    el código de salida tras el EOF; `Cliente::ejecutar` espera por `peticion_id` y no se
    cuelga si el socket cae; los túneles automáticos ya no dialogan con el cliente 0; `Cerrar`
    cancela diálogos que no son de sesión; `apagar_limpio` no pierde anotaciones y vacía el
    hilo escritor al salir; una fila de `REGISTRO` de un host borrado se reintenta con
    `host_id` NULL en vez de perderse por la FK.

Cliente:

11. **Pila de diálogos del servidor**: una pregunta del servidor (huella, frase, contraseña)
    apila el diálogo abierto en vez de pisarlo. Con `p` en N hosts pueden llegar N preguntas
    seguidas, y un formulario a medio escribir no debe perderse.
12. **`DELIBERACIONES.usuario` es el usuario del uid del proceso**, no `$USER` (se puede
    falsear y es un registro de auditoría); `$USER` solo si el sistema no lo sabe.
13. **La confirmación «más de 5 pestañas» de `p` va antes de la deliberación**: `Ctrl+K` debe
    ser la última tecla antes de ejecutar.
14. **`p` con deliberación la cierra el cliente**: no hay ejecución en el servidor que lo haga,
    así que cuando las pestañas se resuelven el cliente escribe `ejecucion_resultado` y anota
    `deliberacion_aprobada|forzada`; también anota `snippet_ejecutado` «en pestaña».
15. **Si un lanzamiento deliberado no llega a ejecutarse** (`LanzarEjecucion` devuelve `Error`,
    el servidor está caído o ninguna pestaña se pudo pedir), el cliente cierra la fila con
    `ejecucion_resultado = error`: nadie más lo haría.
16. **Añadidos menores de interfaz** respecto a las maquetas:
    - Diálogo MAGI: una línea «requerida por: crítico · 3 hosts · verificaciones en …» y, bajo
      la tabla, los datos completos de los rechazos del host seleccionado (las celdas se
      recortan).
    - EJECUTAR en modo pestaña no enseña «parar al primer fallo» (no aplica); con `a`, `p` o un
      solo host, cualquier variable abre el diálogo aunque tenga defecto, para confirmarla.
    - Resultados: `↵` en el panel de ejecuciones pasa al de hosts; `←` `→` desplazan en
      horizontal en el visor; `x` y `r` funcionan dentro del visor.
    - La ficha gana desplazamiento vertical: con los dos bloques nuevos pasa de 17 a 23 líneas
      y no cabía en un terminal de 24 filas.
17. **La salida de una ejecución nunca se guarda dentro de `~/.ssh`** (ni por `..` ni por
    enlaces): `s` en Resultados lo rechaza, en línea con «no escribas en `~/.ssh` desde otro
    sitio».
18. **Un sondeo nuevo de MELCHIOR-1 se guarda en `SONDEOS`** como cualquier otro (y puede
    anotar las transiciones `sondeo_fallido`/`sondeo_recuperado`): es una medida real y Flota
    la aprovecha. El registro del sondeo se separó de los contadores del lote de Flota.
19. **Arreglos fuera del checklist encontrados durante la fase**:
    - La ficha de un host cuya identidad no está en la lista del desplegable (clave revocada,
      fichero que ya no existe) la insertaba pero dejaba seleccionada «auto»: la ficha se
      abría «sucia» y `Ctrl+S` cambiaba la identidad del host sin que nadie la tocara
      (pérdida de datos, anterior a esta fase; con prueba de regresión).
    - Los diálogos de confirmación y de detalle recortaban su última línea (la de teclas).
    - La columna de tipo de la vista Registro recortaba `deliberacion_cancelada`.
20. **La línea desmarcada de la Fase 3** (reutilización del pool, línea 31 de su checklist) ya
    figura `[x]` en `docs/fase3/magi-fase3-checklist.md`; solo el maestro la había descontado.
    Con la 3b completa, el total acumulado la vuelve a contar (487/490: 407 + 79 de esta fase
    + esa línea).
21. **Revisión adversarial de cierre no realizada** (línea 110 del checklist, que queda sin
    marcar): la pide CLAUDE.md antes de cerrar cada fase, pero el desarrollador decidió
    saltarla y cerrar. Solo tuvo revisión adversarial la corrección 3b (S0). La revisión de
    S1-S3 se había lanzado (cuatro revisores con verificación independiente) y se detuvo antes
    de dar resultados.

## Estructura de archivos creada/modificada

**Nuevos**

| Fichero | Contenido |
|---|---|
| `src/almacen/snippets.rs` | CRUD de `SNIPPETS` y `SNIPPET_DESTINOS`, uso, hosts con snippet al conectar |
| `src/almacen/verificaciones.rs` | `VERIFICACIONES_HOST` por host |
| `src/almacen/deliberaciones.rs` | Alta de `DELIBERACIONES`, lectura y `ejecucion_resultado` |
| `src/snippets/mod.rs` | Modelo, validación, destinos, motivos de deliberación, listado de la CLI |
| `src/snippets/variables.rs` | Detección, contexto (comillas/heredoc) y sustitución escapada |
| `src/snippets/salida.rs` | Saneado de salida remota, fichero de salida, duraciones |
| `src/deliberacion/mod.rs` | Modelo (veredictos, consenso, filas), detalle para Registro y orquestación con fuentes inyectables |
| `src/deliberacion/estado.rs` | Máquina del diálogo (Comprobando/Aprobada/Bloqueada/Motivo) |
| `src/deliberacion/salud.rs` · `backup.rs` · `tests.rs` | MELCHIOR-1, BALTHASAR-2 y CASPER-3 |
| `src/servidor/ejecuciones.rs` | Registro de ejecuciones, despachador, `exec`, cancelación, cierre y anotaciones |
| `src/app/snippets.rs` · `formulario_snippet.rs` | Vista `F8` y formulario |
| `src/app/lanzar.rs` | EJECUTAR, `a`, `p`, paleta, `!`, atajos, snippet al conectar |
| `src/app/resultados.rs` | Estado y teclas de Resultados |
| `src/app/dialogo_magi.rs` | Diálogo de deliberación: comprobaciones, teclas, filas y anotaciones |
| `src/ui/snippets.rs` · `formulario_snippet.rs` · `ejecutar.rs` · `resultados.rs` · `deliberacion.rs` | Pintado de las vistas y diálogos nuevos |
| `tests/pool.rs` | Regresiones de la 3b (pestañas, SFTP, túneles, `Ejecutar`, `SIGTERM`, v3) |
| `tests/ejecuciones.rs` | Ejecuciones contra el servidor SSH en proceso |
| `tests/deliberacion.rs` | BALTHASAR-2 de extremo a extremo con el `sftp-server` real |
| `docs/fase6/magi-fase6-implementacion.md` | Este informe |

**Modificados**

| Fichero | Qué cambió |
|---|---|
| `Cargo.toml` | `shell-escape`; `tokio/signal`, `nix/signal` |
| `src/servidor/conexiones.rs` | Reescrito: `Pool` sin desplazamientos, `conexion_para_canal`, `ConexionTomada`, `abrir_con_plazo` |
| `src/servidor/sesiones.rs` | Pestañas sobre el pool, apertura cancelable, diálogos con generación, comandos iniciales |
| `src/servidor/mod.rs` | Estado (cerrojos, apagando, ejecuciones), `SIGTERM`, apagado, `Ejecutar`, despacho v4, escritor (`Barrera`, `Terminar`, `ResultadoDeliberacion`), parada de otra versión |
| `src/servidor/sftp.rs` · `tuneles.rs` · `transferencias.rs` | Conexión por `conexion_para_canal`, `ModoSftp`, túneles automáticos sin diálogo |
| `src/conexion/{cliente,mod}.rs` | `abrir_canal`, contraseña del llavero del solicitante, textos con instrucción |
| `src/protocolo.rs` | `VERSION_PROTOCOLO = 4` y mensajes nuevos |
| `src/cliente/mod.rs` | `Cliente::peticion` con esperas por `peticion_id`, pid en `VersionIncompatible` |
| `src/flota/mod.rs` | `sondear_via_servidor` con plazo |
| `src/almacen/{migraciones,hosts,mod}.rs` | Migración 5, `snippet_al_conectar_id`, fachada |
| `src/registro.rs` · `src/config.rs` · `src/modelo.rs` | Tipos y filtro, `[deliberacion]` y `[flota.atajos]`, columna nueva del host |
| `src/app.rs` | Vistas, eventos y ganchos nuevos; ficha con los bloques nuevos; detalle de Registro |
| `src/ui/{mod,barra,ayuda,dialogos,ficha,paleta,registro}.rs` | Pintado de lo nuevo, barra de Flota con atajos, ayudas |
| `src/main.rs` · `src/lib.rs` | `magi snippets`, `magi servidor parar` con `SIGTERM`, módulos nuevos |
| `tests/comun/mod.rs` | `exec` en el `sshd` de pruebas, contadores de conexiones, `Escenario` |
| `tests/{almacen,servidor,sftp,tuneles,password,cliente}.rs` | Migración 5 y protocolo v4 |
| `README.md` | Multiplexar, `SIGTERM`, snippets, deliberación, verificaciones, atajos, pruebas |

## Decisiones técnicas tomadas durante el desarrollo

1. **Una sola puerta a las conexiones.** `conexion_para_canal` toma primero la vía rápida
   (`reutilizar`, sin cerrojo), después, con `SoloViva`, presta el handle de una pestaña abierta,
   y solo entonces toma el cerrojo del host, que cubre mirar/conectar/guardar. Devuelve una
   `ConexionTomada` que se suelta por identidad con `soltar()` y, si alguien la olvida, con un
   `Drop` de respaldo que guarda una referencia `Weak` al estado.
2. **Plazos** (ninguna espera de red del servidor queda sin plazo): apertura interactiva 20 s
   de red (el tiempo en diálogos no cuenta), no interactiva 30 s de cerrojo y 30 s de
   conexión, canal 10 s, `Ejecutar` 4 s (canal 2 s), desconexión 5 s, apagado 10 s,
   contraseña del llavero 10 s, `exec` de ejecución con el timeout del snippet.
3. **Rangos de `peticion_id` por subsistema** (T39): Archivos desde 0, túneles desde 2^40,
   ejecuciones desde 2^44 y peticiones esperables del cliente desde 2^48. `Cliente::peticion`
   registra la espera antes de enviar, la retira con una guardia `Drop` y, tras el EOF del
   socket, falla al momento en vez de esperar a nadie.
4. **Hilo escritor con barrera**: `OrdenBd::Barrera` confirma que las filas anteriores están
   escritas antes de enviar un `Hecho` que las implica (arregla la intermitencia de
   `borrar_remoto_es_recursivo_y_anota_el_borrado`); `Terminar` lo vacía al apagar.
5. **Ejecuciones**: la salida se acumula en `SalidaHost` fuera del mutex del estado (tope 1 MiB
   por flujo, se sigue drenando para no bloquear al remoto); la máquina de estados es pura y
   devuelve la anotación solo en la transición, así que nada se anota dos veces; la
   deliberación se valida una vez al lanzar (fila existente, resultado coherente, sin
   `ejecucion_resultado`, no usada por otra ejecución).
6. **Deliberación con fuentes inyectables** (`Fuentes{sondear, listar}`): la orquestación se
   prueba sin red y las fuentes reales van por el servidor (`Fuentes::reales`). Cada
   deliberación lleva un `token`; al cerrarse, sus tareas se abortan (`TareasComprobacion`) y
   el proceso de CASPER-3 muere con su guardia (`setsid` + `killpg`). Los veredictos se sanean
   y acotan al recibirlos, antes de pintarlos o guardarlos.
7. **La deliberación vive fuera del hueco de diálogos** (`App.deliberacion`): una pregunta del
   servidor puede taparla un momento sin perderla, y las teclas le llegan antes que a la vista.
8. **Lógica de las vistas sin red ni disco** (`EstadoResultados`, `preparar()` del
   lanzamiento, `Ficha::ficha_de`): se prueban con relojes y datos inyectados y con
   `ratatui::backend::TestBackend` para el pintado.
9. **Organización del trabajo**: un commit por sprint; S1-S3 con pistas en paralelo en
   worktrees sobre un contrato fijado antes (tipos, firmas y enganches en `app.rs`), integradas
   con cherry-pick y aplastadas en el commit del sprint.

### Revisión adversarial (T31)

**S0 (corrección 3b).** Antes del commit de S0 se pasó una revisión con cuatro revisores
(fugas e identidad, esperas sin plazo, carreras de cerrojo, apagado y señales) y verificación
independiente de cada hallazgo: 18 confirmados y 1 descartado. Todos corregidos en S0:

1. Una apertura interactiva sin plazo retenía la tarea, el socket y el cerrojo del host, y
   cerrar la pestaña no la cancelaba (y su gemelo sobre el cerrojo de SFTP).
2. Un plazo que vencía al abrir el canal dejaba huérfano el canal que el host confirmaba tarde
   en la conexión compartida.
3. El canal SFTP de una conexión caída no se recogía mientras se usara y bloqueaba la
   reapertura.
4. El plazo único del apagado se gastaba en parar túneles (un `-R` lento) y se perdían las
   anotaciones de cierre de sesiones, transferencias y pool (dos hallazgos).
5. El plazo de `Ejecutar` en el servidor (10 s) superaba al del cliente (5 s): el paso a la
   conexión efímera no llegaba nunca.
6. Un `SIGTERM` durante un apagado en curso salía con ese apagado a medias (tres hallazgos).
7. El ciclo automático levantaba túneles huérfanos si la pestaña se cerraba durante la
   activación.
8. La identidad de una activación de túnel no era única: una antigua podía instalarse en la
   nueva o matarla.
9. Las aperturas que esperaban el cerrojo no comprobaban al obtenerlo que su solicitud
   siguiera viva.
10. Durante una reconexión la pestaña dejaba de contar para el ciclo automático de túneles.
11. Una petición hecha justo tras el EOF del socket esperaba para siempre.
12. Al apagar, una pestaña «abriendo» se anotaba dos veces y el host quedaba en error.
13. «apagando» solo se miraba en dos sitios: durante el apagado se seguían abriendo
    conexiones, túneles y transferencias.
14. `parar_otra_version` mandaba `SIGTERM` a un pid leído antes de una pregunta sin plazo y
    daba la parada por buena mirando solo la ruta del lock.

Descartado: «cerrar la ventana que reconecta borra una sesión compartida con otras ventanas».
Es lo que dice la especificación de la Fase 3 (si el solicitante de una apertura o
reconexión se va, la apertura se cancela y la sesión fallida se elimina) y lo que ya pasaba con
cualquier reconexión fallida; el cambio solo hace que quien recibe los diálogos sea quien la
cancela. Volver a «caída» en vez de eliminarla sería una decisión de producto.

**Cierre (S1-S3): no realizada.** El desarrollador decidió saltarla para cerrar la fase; se
detuvo antes de devolver hallazgos, así que no hay resultados que anotar. El código de S1-S3
solo pasó las pruebas de cada sprint, la integración y la prueba manual descrita en «Ejecución y
pruebas». Durante la integración salieron y se corrigieron:

- La identidad de la ficha que no estaba en el desplegable se cambiaba a «auto» al guardar
  (pérdida de datos anterior a la fase; prueba `una_identidad_que_no_esta_en_la_lista_se_conserva_seleccionada`).
- Con el servidor sin arrancar, BALTHASAR-2 rechazaba «sin acceso SFTP: sin servidor de
  sesiones»: no era un fallo de MAGI (la ruta del socket del entorno de prueba superaba
  `SUN_LEN`), pero confirmó que la comprobación falla con motivo y sin colgarse.
- Altura del diálogo MAGI, pista «? ayuda» dentro del campo motivo (ahí `?` es texto) y
  columna de tipo del Registro.

## Funcionalidades del checklist completadas (copiando su texto exacto)

### Corrección 3b: pool de conexiones

- [x] `conexion_para_canal(host, politica)` es la única vía para obtener conexión en pestañas, SFTP, túneles, sondeo por `Ejecutar` y ejecuciones, con un cerrojo por host que cubre «mirar el pool / conectar / guardar»
- [x] Pestañas sobre el pool: con `multiplexar` marcado una segunda sesión al mismo host abre un canal nuevo sin reautenticar; sin marcar, conexión propia que se cierra con su sesión (al completarlo, marca también la línea desmarcada del checklist de la Fase 3)
- [x] Una pestaña sin `multiplexar` abre conexión propia que no entra en el pool; `Pool::guardar` solo inserta si no hay entrada y nunca desplaza una conexión viva ni en gracia
- [x] Aperturas simultáneas del mismo host (pestaña + SFTP + túnel) comparten una única conexión con `multiplexar`; test de regresión con las tres a la vez
- [x] Aviso de versión incompatible con el pid del servidor antiguo (de `VersionIncompatible` o, si no lo trae, por `SO_PEERCRED` del socket)
- [x] `magi servidor parar` contra un servidor de otra versión muestra su pid y le envía `SIGTERM` tras confirmación (`--si` la omite)
- [x] El servidor trata `SIGTERM` como `Parar`: cierra sesiones, túneles y ejecuciones con sus anotaciones y borra socket y lock
- [x] Un `Hecho` de una operación que anota en `REGISTRO` se envía después de que el hilo escritor confirme la fila; `borrar_remoto_es_recursivo_y_anota_el_borrado` deja de ser intermitente
- [x] README corregido: la reutilización con `multiplexar` describe el comportamiento real

### Almacén y modelo

- [x] Migración 5 por `PRAGMA user_version`: tablas `SNIPPETS`, `SNIPPET_DESTINOS`, `VERIFICACIONES_HOST`, `DELIBERACIONES` y columna `HOSTS.snippet_al_conectar_id` (FK `SNIPPETS` `ON DELETE SET NULL`)
- [x] Tabla `SNIPPETS` (id, nombre UQ, comando, descripcion, etiquetas, critico, timeout_seg, parar_al_fallo, usado_veces, ultimo_uso_en, creado_en, actualizado_en)
- [x] Tabla `SNIPPET_DESTINOS` (id, snippet_id FK CASCADE, etiqueta TEXT NULL, host_id FK HOSTS CASCADE NULL) con exactamente uno de los dos validado en código; etiqueta por nombre
- [x] Tabla `VERIFICACIONES_HOST` (host_id PK FK CASCADE, salud, backup, backup_ruta, backup_patron, tests, tests_comando, actualizado_en)
- [x] Tabla `DELIBERACIONES` (id, fecha, snippet_id FK SET NULL, accion, hosts_json, comprobaciones_json, resultado, bloqueada, motivo, usuario, ejecucion_resultado)
- [x] Sin `CHECK` sobre enumeraciones; `resultado` y `ejecucion_resultado` validados en código
- [x] Tipos nuevos en `REGISTRO`: `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`; el filtro `t` de la vista Registro gana el grupo «snippets»
- [x] Tests del almacén: migración 4→5, cascadas de destinos y verificaciones, `SET NULL` en deliberaciones y en `snippet_al_conectar_id`

### Snippets: modelo y validación

- [x] Validación: nombre único y no vacío, comando no vacío, timeout 5-3600, al menos un destino; etiquetas del snippet normalizadas a minúsculas
- [x] Variables `{{nombre}}` y `{{nombre:defecto}}` detectadas al guardar (nombre `[a-z_][a-z0-9_]*`) y listadas en el detalle
- [x] Resolución de destinos: hosts con alguna etiqueta destino ∪ hosts sueltos, sin duplicados, ordenados por grupo y nombre; guardar con cero hosts resueltos avisa sin bloquear
- [x] Sustitución de variables escapada para el shell con `shell-escape`; una variable vacía sin defecto no deja continuar
- [x] Tests unitarios de resolución de destinos y de variables

### Vista Snippets (F8)

- [x] `F8` abre Snippets: lista nombre · destino resumido (etiqueta o «todos») → N hosts · `CRÍTICO`; cabecera con el total; vacía con «Sin snippets: n para crear uno»
- [x] Panel inferior: comando, destinos resueltos con nombres, confirmación («requiere deliberación MAGI» y por qué), timeout, usado N veces y última vez
- [x] Navegación con `↑` `↓` / `j` `k`; `/` filtra por nombre, comando, etiquetas del snippet o etiqueta destino
- [x] `n` / `e` abren el formulario: nombre, comando (multilínea), descripción, etiquetas, destinos (etiquetas con autocompletado de las de host y hosts sueltos con desplegable), crítico, timeout, parar al primer fallo; `Ctrl+S` guarda
- [x] `x` borra con confirmación; si es snippet al conectar de algún host, lo dice y esos hosts quedan sin él
- [x] `t` abre Resultados; `q` / `Esc` vuelve
- [x] Entradas de paleta: `snippet · <nombre>`, `snippet · <nombre> · <host>`, `ir a snippets`, `ir a resultados`, `nuevo snippet`
- [x] `!` en Hosts y Flota abre la paleta filtrada a los snippets que apuntan al host seleccionado, para ejecutarlos solo en ese host
- [x] `magi snippets` lista los snippets con destinos resueltos, crítico y último uso

### Lanzar una ejecución

- [x] `↵` abre el diálogo EJECUTAR: casillas por host resuelto (todas marcadas), variables con su defecto, casilla «parar al primer fallo» con el valor del snippet y timeout visible
- [x] `a` ejecuta en todos los destinos sin diálogo de hosts (solo pide variables si las hay)
- [x] Tras el diálogo, se decide si hay deliberación (crítico, más de un host o algún host con verificaciones activas); sin deliberación se lanza directamente
- [x] `LanzarEjecucion` lleva el comando ya sustituido, los host_ids, timeout, parar_al_fallo y, si hubo, la deliberación (id, forzada, motivo); el servidor no lee `SNIPPETS`
- [x] Al lanzar, el cliente incrementa `usado_veces` y `ultimo_uso_en` y abre Resultados en la ejecución nueva
- [x] `p` abre en pestaña: una pestaña por host con `AbrirSesion{…, comando_inicial}` (confirmación si son más de 5), pasando antes por deliberación si procede; anota `snippet_ejecutado` con «en pestaña»

### Servidor: ejecuciones

- [x] `VERSION_PROTOCOLO = 4`; mensajes `LanzarEjecucion` (respondido con `Hecho{peticion_id}` o `Error{peticion_id, mensaje}`), `CancelarEjecucion`, `LimpiarEjecuciones`, `PedirSalida`, `Ejecuciones`, `Salida`; `AbrirSesion` gana `comando_inicial`; `Bienvenida` incluye las ejecuciones; tests de ida y vuelta y rechazo de v3
- [x] `servidor/ejecuciones.rs`: `Ejecucion {id, snippet_id, nombre, comando, hosts, timeout_seg, parar_al_fallo, deliberacion, solicitante, creada_en, terminada_en, estado}` y `EjecucionHost {host_id, nombre, estado, codigo, inicio, fin, stdout, stderr, error}`
- [x] Hasta 8 hosts en paralelo; cada host toma conexión con `conexion_para_canal` y política no interactiva (huella, frase o contraseña sin llavero → error del host con instrucción)
- [x] Canal `exec` con stdout y stderr separados, código de salida y timeout del snippet (al vencer se cierra el canal y el host queda en error «tiempo agotado»)
- [x] Estados de host `en_cola`, `conectando`, `ejecutando`, `ok`, `fallo`, `error`, `cancelado`, `omitido`; estados de ejecución `en_curso`, `terminada`, `cancelada`
- [x] «Parar al primer fallo»: tras un `fallo` o `error`, los hosts aún en cola pasan a `omitido`; los que están en marcha terminan
- [x] Salida con tope de 1 MiB por flujo y marca `truncada`; nunca se escribe en `REGISTRO` ni en el log
- [x] `CancelarEjecucion`: hosts en cola → `omitido`, en marcha → canal cerrado y `cancelado`; ejecución `cancelada`
- [x] `Ejecuciones{lista}` con metadatos por host (sin salida) difundida en cada cambio de estado; `PedirSalida` devuelve `Salida` del host
- [x] `snippet_ejecutado` en `REGISTRO` por host al llegar a `ok`, `fallo`, `error` o `cancelado` (snippet, host, código, duración, bytes)
- [x] Al terminar una ejecución con deliberación, el servidor anota `deliberacion_aprobada` o `deliberacion_forzada` (acción, hosts, comprobaciones, motivo, usuario, resultado) y rellena `DELIBERACIONES.ejecucion_resultado` (`ok`, `parcial`, `error`, `cancelada`) por su hilo escritor
- [x] Terminadas conservadas 1 h o hasta `LimpiarEjecuciones`; el servidor no se apaga por inactividad con ejecuciones en curso; `magi servidor estado` las lista
- [x] `comando_inicial` de `AbrirSesion` se escribe en la pestaña tras abrir el shell, también al reconectar
- [x] Tests contra el servidor SSH en proceso: ejecución en varios hosts, código ≠ 0, timeout, parar al primer fallo, cancelación en curso, tope de salida, anotaciones

### Vista Resultados

- [x] Panel de ejecuciones (hora, snippet, hosts, ✓ ✕ en marcha, estado, `⚑` si forzada) y panel de hosts de la seleccionada (glifo, estado, código, duración, bytes); `Tab` cambia de panel
- [x] `↵` sobre un host muestra stdout y stderr separados y desplazables, con las secuencias de escape eliminadas; se refresca cada 1 s mientras el host está en marcha
- [x] `s` guarda la salida del host (o de todos) en un fichero vía `ficheros.rs` con permisos 600
- [x] `x` cancela (confirmación si está en curso); `r` repite con el mismo snippet, hosts y variables pasando de nuevo por deliberación si procede; `C` limpia terminadas
- [x] Una ventana nueva ve las ejecuciones desde `Bienvenida`; todas las ventanas ven el mismo progreso

### Deliberación MAGI

- [x] Bloque «Verificaciones previas (deliberación MAGI)» en la ficha: casillas salud, backup (ruta y patrón) y tests (comando local); guarda en `VERIFICACIONES_HOST`
- [x] Deliberación obligatoria para snippets críticos, para toda ejecución en más de un host y para hosts con alguna verificación activa
- [x] MELCHIOR-1 salud: aprueba con último sondeo NOMINAL de menos de `salud_max_min`; si es viejo o no existe, sondea (no interactivo); CARGA, ALCANZABLE, CAÍDA o sin datos en plazo rechazan con detalle
- [x] BALTHASAR-2 backup: `AbrirSftp` no interactivo + `ListarDir(backup_ruta)`; aprueba si el fichero más reciente que casa con `backup_patron` tiene menos de `backup_horas`; más viejo, vacío o sin acceso rechaza con el dato
- [x] CASPER-3 tests: `sh -c tests_comando` en local; código 0 aprueba, ≠ 0 rechaza con la última línea de stderr; al vencer el plazo se mata el proceso y rechaza
- [x] Todas las comprobaciones de todos los hosts en paralelo con plazo de `limite_seg` (2 s) cada una; ninguna abre diálogos
- [x] Diálogo MAGI: acción, tabla host × comprobación con `✓` / `✕` / `—` / `◐` y el dato de cada veredicto, barra de consenso N / M y estado APROBADO, BLOQUEADO o FORZADO
- [x] Consenso = unanimidad de las comprobaciones activas; sin comprobaciones activas muestra «sin comprobaciones configuradas» y sigue pidiendo `Ctrl+K`
- [x] `Ctrl+K` ejecuta solo con consenso o forzada; `↵` no ejecuta
- [x] `f` en BLOQUEADO abre el campo motivo; con menos de 10 caracteres no se acepta; con motivo, estado FORZADO y `Ctrl+K` ejecuta
- [x] Al resolverse, el cliente inserta la fila de `DELIBERACIONES` (resultado, bloqueada, motivo, comprobaciones_json, usuario local); `Esc` anota además `deliberacion_cancelada`
- [x] El detalle de una entrada `deliberacion_*` en la vista Registro muestra sus comprobaciones leídas de `DELIBERACIONES`
- [x] `config.toml`: `[deliberacion] backup_horas = 24, salud_max_min = 5, limite_seg = 2, motivo_min = 10`
- [x] Tests de las tres comprobaciones (sondeo NOMINAL/CARGA/viejo, backup reciente/viejo/vacío, tests `true`/`false`/`sleep 3`) y del consenso

### Snippet al conectar y atajos de Flota

- [x] Ficha de host: «Snippet al conectar» en el bloque «Al conectar», desplegable limitado a snippets no críticos y sin variables; guarda `HOSTS.snippet_al_conectar_id`
- [x] Al abrir o reconectar una pestaña a ese host, el cliente envía el comando como `comando_inicial`; nunca delibera
- [x] `[flota.atajos]` en `config.toml` asigna teclas a nombres de snippet sobre el host seleccionado en Flota; teclas ya usadas por Flota o snippets inexistentes se ignoran con aviso al arrancar; los atajos aparecen en la barra inferior
- [x] Un atajo de Flota sigue el mismo flujo que `snippet · <nombre> · <host>` (variables y deliberación si procede)

### Navegación y ayuda

- [x] `F8` operativo; ayuda `?` en Snippets, Resultados y el diálogo MAGI
- [x] Ninguna acción destructiva (borrar snippet, cancelar ejecución en curso, ejecutar con deliberación) sin confirmación

### Calidad

- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de 3b, almacén, snippets, ejecuciones y deliberación
- [x] README actualizado: snippets, variables, ejecución, Resultados, deliberación MAGI, verificaciones por host, atajos de Flota

Sin marcar: «Revisión adversarial del código nuevo (servidor y cliente) antes de cerrar, con
resultado en el informe de implementación» (desviación 21).

## Pendientes y bloqueos

1. **Revisión adversarial de S1-S3 sin hacer** (decisión del desarrollador al cerrar). Es lo
   más urgente de esta lista: el código nuevo (servidor de ejecuciones, lanzamiento,
   Resultados, deliberación, ficha) no ha pasado la búsqueda de pérdida de datos, fugas de
   recursos, esperas sin plazo y estado leído a destiempo que CLAUDE.md exige. Se recomienda
   hacerla al empezar la siguiente fase o en una corrección propia, antes de construir encima.
2. **Validación manual con hosts reales (R15).** La suite prueba contra un servidor SSH en
   proceso y la prueba manual se hizo con hosts inalcanzables (todas las comprobaciones
   rechazan por plazo). Falta ver con hosts de verdad: una deliberación aprobada con `Ctrl+K`,
   BALTHASAR-2 contra un directorio de backups real, una ejecución con salida y el snippet al
   conectar. Guion al final de este informe.
3. **Una comprobación de backup abandonada sigue en el servidor.** Si el host no responde,
   BALTHASAR-2 rechaza a los 2 s en el cliente, pero el `AbrirSftp` no interactivo sigue en el
   servidor con el cerrojo del host hasta su plazo (10 s de TCP). Una ejecución lanzada justo
   después (forzando) espera ese cerrojo antes de intentar su propia conexión: en la prueba
   manual, el host inalcanzable acabó en error a los 15,5 s en vez de a los 10 s. No se pierde
   nada y todo tiene plazo, pero la espera se nota.
4. **Observaciones de las pistas sin corregir** (anteriores o fuera del alcance):
   - `aperturas_pendientes` del cliente solo se limpia cuando aparece la pestaña: si el
     servidor rechaza un `AbrirSesion` sin crear sesión, el host se queda marcado y el
     seguimiento de `p` cae a los 6 min.
   - `Bienvenida` vacía `aperturas_pendientes` al reconciliar: un `p` hecho justo antes daría
     sus pestañas por fallidas (poco probable).
   - Con `p` sobre varios hosts, cada pestaña nueva se activa al aparecer (comportamiento
     anterior), así que la vista acaba en la última.
   - Si el servidor cae con una ejecución deliberada en curso, su fila de `DELIBERACIONES` se
     queda sin `ejecucion_resultado` (el cliente no la cierra por no pisar un resultado que
     el servidor aún pudiera escribir).
   - El diálogo EJECUTAR no tiene ayuda `?` (el checklist la pide en Snippets, Resultados y el
     diálogo MAGI, que sí la tienen).
   - El desplegable de snippet de la ficha lleva el id en un vector paralelo a las opciones;
     una variante `ValorOpcion::Snippet(i64)` sería más limpia.
5. **Las Fases 2-5 siguen sin validación manual completa (R15)**; la 3b cambió el pool que
   usan todas, así que conviene repetir las pruebas manuales de la vista Sesión, Archivos y
   Túneles con hosts reales (pestañas con y sin `multiplexar`, túnel activo al abrir una
   pestaña, SFTP y túnel a la vez).

## Ejecución y pruebas

**Arrancar**

- TUI: `cargo run` (Snippets en `F8`; Resultados con `t` desde Snippets; el servidor de
  sesiones se autolanza si no está).
- Servidor en primer plano: `cargo run -- --servidor` en una terminal y `cargo run` en otra;
  log en `~/.local/state/magi/logs/servidor.log.<fecha>`.
- CLI: `magi snippets` (destinos resueltos, crítico y último uso); `magi servidor estado`
  (lista las ejecuciones en curso); `magi servidor parar` (contra un servidor de otra versión
  enseña su pid y le manda `SIGTERM` tras confirmar; `--si` omite la pregunta).
- Tras actualizar: un servidor de la Fase 5 (protocolo 3) se rechaza con su pid;
  `magi servidor parar` lo detiene.

**Migrar**

Nada a mano: al abrir la base, `PRAGMA user_version` la lleva de 4 a 5 y crea `SNIPPETS`,
`SNIPPET_DESTINOS`, `VERIFICACIONES_HOST`, `DELIBERACIONES` y `HOSTS.snippet_al_conectar_id`.
`config.toml` admite las secciones nuevas `[deliberacion]` y `[flota.atajos]` (con valores
por defecto si no están).

**Testear**

- `cargo test`: 445 pruebas en verde y 1 ignorada (la del llavero del escritorio). Por suite:
  biblioteca 315, `almacen` 34, `pool` 17, `ejecuciones` 17, `sftp` 18, `sshconfig_ida_vuelta`
  14, `servidor` 11, `tuneles` 11, `password` 4, `deliberacion` 3, `cliente` 1.
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: limpios.
- `cargo test --test pool` (3b: una conexión con `multiplexar`, pestaña que no tumba el túnel,
  pestaña + SFTP + túnel a la vez, `Ejecutar` con plazo, `SIGTERM` al binario real, cliente v3
  rechazado con pid, SFTP de comprobación que no levanta túneles automáticos).
- `cargo test --test ejecuciones` (varios hosts, código ≠ 0, timeout de 5 s con el pool vivo,
  parar al primer fallo en 10 hosts, cancelación, tope de 1 MiB, 8 a la vez, deliberación
  aprobada/forzada y rechazos, huella desconocida y contraseña sin llavero sin diálogo,
  `Bienvenida`, host borrado, comandos iniciales al reconectar).
- `cargo test --test deliberacion` (BALTHASAR-2 con el `sftp-server` real: backup reciente y
  viejo, sin acceso, huella desconocida sin diálogo). Las pruebas de SFTP se saltan con aviso
  si el sistema no trae `sftp-server`.
- Unitarias de la deliberación: CASPER-3 real con `true`, `false` y `sleep 3` (rechaza «plazo
  vencido» a los 2 s y el proceso ya no existe), backup, salud, consenso, máquina del diálogo
  («ok» + `Ctrl+K` no ejecuta) y pintado con `TestBackend`.
- Pruebas aisladas: `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` y
  `XDG_RUNTIME_DIR` a un directorio temporal. **Ojo:** la ruta de `XDG_RUNTIME_DIR` tiene que
  ser corta (el socket no puede pasar de ~108 bytes) o el servidor no arranca.

**Prueba manual hecha (27 sep 2026, entorno aislado, hosts inalcanzables)**

Snippet crítico «backup postgres» sobre `db` (salud, backup y tests `sleep 3` activos) y
`hetzner-01` (sin verificaciones): el diálogo MAGI mostró `◐` y a los 2 s los tres rechazos
de `db` con su dato y `—` en `hetzner-01`; consenso 0/3 y BLOQUEADO. `Ctrl+K` y `↵` no
hicieron nada; `f` + «ok» + `Ctrl+K` enseñó «mínimo 10 caracteres (2/10)»; `Esc` volvió a
BLOQUEADO y otro `Esc` canceló (fila `cancelada` y `deliberacion_cancelada`, con las
comprobaciones en el detalle de `F7`). Forzando con motivo, `Ctrl+K` abrió Resultados con
`⚑`; los dos hosts acabaron en error de conexión, con `snippet_ejecutado` por host,
`deliberacion_forzada` con «resultado error» y `ejecucion_resultado = error`. La ficha enseñó
los dos bloques nuevos como en la maqueta §6.5.

**Guion de validación manual sugerido (hosts reales)**

1. Crear un snippet no crítico con un solo destino y sin verificaciones (`uptime`): `↵` →
   EJECUTAR → se lanza sin deliberar; Resultados enseña `ok`, código 0 y la salida con `↵`;
   `s` la guarda (comprobar permisos 600).
2. Snippet con variable (`systemctl status {{servicio:nginx}}`): cambiar el valor a
   `a b; rm -rf /` y ver en la salida que llega como un único argumento.
3. `sleep 10` con timeout 5: el host queda en error «tiempo agotado» a los 5 s y una pestaña
   al mismo host (con `multiplexar`) se abre sin volver a autenticar.
4. En la ficha de un host real: salud, backup (ruta de backups de verdad y patrón) y tests
   (`true`). Lanzar un snippet sobre él: los tres `✓` con su dato, APROBADO, `Ctrl+K`
   ejecuta. Cambiar los tests a `false`: BLOQUEADO con la última línea de stderr.
5. Snippet al conectar (`uptime`) en un host: al abrir una pestaña se escribe solo; al
   reconectarla, otra vez; un `p` de otro snippet sobre ese host escribe los dos, y al
   reconectar solo el de «al conectar».
6. Atajo `[flota.atajos] u = "uptime"`: pulsar `u` en Flota sobre un host destino lo ejecuta;
   sobre uno que no es destino, avisa y no ejecuta. Una tecla de Flota (`r`) como atajo se
   ignora con aviso al arrancar.
7. Dos ventanas de MAGI: una lanza, la otra ve el mismo progreso en Resultados; `x` cancela
   desde la segunda.
