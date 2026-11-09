# MAGI - Fase 14: Revisión adversarial de cierre

**Fecha:** 9 de noviembre de 2026
**Rama:** `fase14`
**Alcance:** código nuevo de la Fase 14 en el núcleo compartido, el escritorio,
`magi-movil` y la app Android. Revisión por separado de servidor/escritorio y
cliente/móvil/app, buscando pérdida de datos, fugas de recursos, esperas sin
plazo y estado leído a destiempo (T31, T54).

## Metodología

- Lectura del código nuevo módulo a módulo (núcleo movido, conexiones del móvil,
  flota, servicios, logs, ejecuciones, deliberación, registro, adaptador
  UniFFI y pantallas/ViewModels de la app).
- Revisión cruzada entre pistas: núcleo/escritorio (contrato-movil), móvil Rust
  (nucleo-deliberacion) y app Android (android-contrato); el Lead revisó además
  las zonas de concurrencia de `conexiones.rs` (orden de cerrojos, temporizador
  de gracia, cierre de canales) y el ejecutor remoto.
- Pruebas de extremo a extremo sin red externa (servidor SSH de pruebas
  in-process) para reproducir los caminos críticos.
- Corrección de lo encontrado y test de regresión cuando el fallo podía perder
  datos, dejar trabajo sin anotar o abrir una brecha de seguridad.

## Hallazgos y correcciones

### Núcleo y escritorio (revisión de contrato-movil)

1. **Inyección de shell con una marca escapada (seguridad).** En
   `snippets/variables.rs`, `\{{x}}` (marca escapada con un número impar de
   barras) se sustituía igualmente y la barra anulaba el entrecomillado del
   valor, de modo que un valor con `;` podía ejecutar órdenes. Ahora la marca
   escapada es texto literal. Dos tests de regresión.
2. **Entrada no fiable sin sanear en el sondeo.** `flota/lectura.rs` aceptaba
   `MAGI_SVC=` con caracteres de control, bidi y nombres de miles de
   caracteres, y el mapa no tenía tope. Ahora nombre saneado y acotado (256),
   estado saneado (32, vacío → `unknown`) y 2 000 servicios como máximo.
3. **El script del sondeo no revalidaba sus argumentos.** `sondeo.sh` podía
   recibir `<motores>` con cualquier binario del PATH y comodines que se
   expandían. Ahora valida unidades, solo ejecuta `docker`/`podman` y usa
   `set -f`; shellcheck en verde.
4. **Salida del sondeo sin tope en el escritorio.** El canal efímero de
   `src/flota/mod.rs` acumulaba stdout/stderr sin límite; ahora 4 MiB, como el
   servidor.
5. **Desbordamiento en la tasa de red.** `tasa_red` restaba contadores `i64`
   sin comprobar: contadores hostiles provocaban pánico en debug. Ahora
   `checked_sub` con test.
6. **Corte UTF-8 en la salida de un snippet.** `recortar_flujo` podía partir un
   carácter multibyte al llegar a 1 MiB; ahora recorta al inicio de carácter.
7. **Falta de validación en `sftp_lectura`.** Sin validación de ruta
   (vacía/control/larga) ni saneado de nombres (control/bidi); añadidos, con
   tests. Se verificó el cierre del canal en todas las ramas y se documentó la
   limitación de `russh-sftp::read_dir` (acumula el directorio; el plazo de
   10 s acota tiempo y memoria, no hay API para cortar a 10 000 en lectura).

Se revisaron como equivalentes al código anterior los movimientos puros
(parser, estado, salud, backup, CASPER-3, destinos, salida, plan, consenso), la
orquestación de la deliberación (plazo duro único, un resultado por trabajo,
token y abort handles) y los proveedores del escritorio (peticiones fijadas al
abrir, T33; sondeo nuevo guardado aunque la deliberación se cierre).

### `magi-movil` (revisión de nucleo-deliberacion)

1. **Apertura automática sin plazo.** Un host que completaba el saludo SSH y no
   contestaba a la autenticación o al canal de un salto dejaba la operación
   colgada con su uso de conexión. Añadido `PLAZO_APERTURA_AUTOMATICA` (30 s)
   solo para la política automática; la interactiva sigue esperando diálogos.
2. **Actualización perdida en la vista de Flota.** Dos sondeos simultáneos
   podían entregar sus listas completas en orden inverso. Las emisiones se
   serializan ahora con un mutex; además el sondeo se guarda **antes** de anotar
   su transición (si fallaba la anotación se perdía el dato primario).
3. **Deliberación cerrada antes de persistir.** `ejecuciones.rs` emitía la
   lista antes de registrar `ejecucion_resultado` y `deliberacion_*`; ahora se
   persiste primero.
4. **Faltaba la purga de REGISTRO de más de 90 días al arrancar** (informe
   §7.10 y checklist). Implementada en `Nucleo::iniciar` (best-effort) con
   test de regresión `registro_purga_e2e`.
5. **Poda de deliberaciones en memoria** en orden equivocado (se iban antes las
   no usadas); corregida, con test.

Hallazgos delegados y corregidos por sus dueños:

- **`reconectar` forzaba el cierre de la conexión compartida** del host (D277),
  tirando sesiones y túneles de otros sin marcarlos caídos: ya no la fuerza y
  el vigilante retira la caída.
- **Logs**: sustituir la suscripción anterior antes de abrir la nueva perdía el
  anillo si la apertura fallaba (ahora solo se sustituye con el canal abierto);
  la última línea sin salto de línea se perdía al cerrar (ahora se vuelca al
  anillo); condición duplicada en la tarea de avisos eliminada.

### App Android (revisión de android-contrato)

1. **Pérdida del id de deliberación con sudo.** Al confirmar una acción que
   exige deliberación y contraseña de sudo, el id se perdía al abrir el diálogo
   de sudo y la acción se ejecutaba sin él. Ahora se guarda
   (`deliberacionPendiente`) y sin id no se ejecuta. Tests de regresión.
2. **Guardas por host** en el OK de sudo y el refresco posterior a una acción;
   el tercer rechazo de sudo cierra y olvida la contraseña como dice el texto.
3. **Ficha de Flota leída a destiempo**: una petición en vuelo podía repoblar el
   panel tras cerrar la ficha o cambiar de host; guardas por selección y carga.
4. **Los logs seguían en segundo plano** (fallaba el AC del checklist): el
   `journalctl` seguía en el host y no existía SEGUIR OTRA VEZ. Añadido gate
   ON_STOP y reanudación; el parar al destruir la pantalla va por un ámbito de
   vida corta que no muere con el ViewModel.
5. **El servicio en primer plano contaba ejecuciones terminadas** (se conservan
   1 h) y provocaba un falso «Android cerró MAGI con N ejecuciones»; ahora solo
   cuentan las no terminadas.
6. **Salida de Resultados** podía sobrescribir la de otro host/ejecución
   seleccionada después; guarda por selección.
7. **Registro**: la siguiente página o el detalle podían aplicarse a un
   filtro/selección ya cambiados; se cancelan y descartan las respuestas viejas.
8. **Snippets**: una preparación en vuelo podía lanzarse sobre otro snippet;
   snapshot de hosts/variables y guarda por uuid.
9. **Deliberación**: una tabla en vuelo podía aplicarse a un plan ya cerrado;
   guarda por plan.
10. **FLAG_SECURE anidado (seguridad).** `RecintoSeguro` retiraba la marca al
    desmontarse el recinto interior aunque quedara otro activo, dejando
    capturable un diálogo de contraseña sobre una pantalla con secretos. Nuevo
    `ContadorRecintos` por Activity, con tests.

Revisado y correcto: adaptador real (listas completas por `StateFlow`, avisos
por canales sin conflar), contraseña de sudo en `ByteArray` borrada en
`finally`, sin `println`/`Log`/`printStackTrace` en el código principal,
copia de salida marcada como sensible, `BotonMantener` con sus pruebas, refresco
de Servicios y de Flota gateados a la vista, y notificación del servicio en
primer plano sin acciones.

### Pasada del Lead

- Orden de cerrojos de `conexiones.rs`: mapa → contabilidad en todas las
  rutas (el `Drop` de `UsoConexion` suelta la contabilidad antes de programar
  el cierre, que no toma el mapa en línea); sin inversiones ni esperas sin
  plazo en el temporizador de gracia.
- `ejecutar_remoto` cierra el canal en segundo plano en todas las ramas
  (incluidas cancelación, `ExitSignal` y `Failure`) y acota cada flujo sin
  reservar de más.
- El vigilante retira la conexión por identidad y avisa a los oyentes; el cierre
  por gracia comprueba identidad y usos antes de cerrar.

## Riesgos aceptados y pendientes

- **Canales internos sin tope** en `CanalRemoto` (`conexiones.rs`): el
  productor es el host y, si un consumidor se atascara, la memoria podría
  crecer. Hoy los consumidores drenan (anillo de logs acotado a 10 000 líneas y
  8 MiB) y el riesgo es bajo; se propone acotarlos con descarte contado en una
  fase de mantenimiento.
- **`con_almacen` síncrono en rutas async**: patrón del crate para operaciones
  cortas; sin deadlock posible, pero con dos workers una importación larga
  podría hacer esperar.
- **Anotaciones best-effort**: si la BD fallara, una anotación de REGISTRO se
  perdería sin rastro (se propone un contador en memoria para diagnóstico).
- **`forzar_cierre`** sigue existiendo y solo lo usa la prueba de Flota; con
  sesiones o túneles vivos no los marcaría caídos (documentado).
- **`backup_ruta` relativa** admitida por `validar_verificaciones` (heredado de
  la F6; el informe F14 §7.11 dice absoluta): se deja como está para no cambiar
  comportamiento visible; consultar con el analista.
- **`Consenso` duplicado** en `src/deliberacion/estado.rs` (idéntico al del
  núcleo): limpieza opcional.
- **BALTHASAR-2 ignora `Listado.truncado`**: con más de 10 000 entradas podría
  rechazar aunque el backup reciente quedara fuera del corte (dirección segura).
- **`lanzar` no aísla un pánico de un proveedor**: con la entrada saneada los
  proveedores actuales no panican.
- **`Visibilidad.kt`** no la usa nadie: el gate se hace con
  `repeatOnLifecycle`/`Lifecycle` en las pantallas (equivalente para «a la
  vista»).
- **Canales UNLIMITED de `FuenteNucleoReal`** (avisos de logs y acciones
  hechas) no se cierran en `cerrarNucleo()`: retienen como mucho avisos de la
  sesión en curso.
- **`catch(Throwable)`** en varios ViewModels atrapa también
  `CancellationException` (patrón heredado de la F11, sin efecto conocido).
- **Punto aislado de `alDeliberar`** en `PantallaServicios`: con la corrección
  1 ya no ejecuta nada y avisa; queda pasarle siempre el diálogo MAGI.
- No se revisaron a fondo `gestor.rs` ni `sincro.rs` (código de la F11, fuera
  del alcance de la Fase 14).

## Conclusión

Las tres revisiones cruzadas encontraron y corrigieron diez fallos con efecto
real (una inyección de shell, un FLAG_SECURE anidado, un permiso de deliberación
que se perdía, logs que seguían en el host en segundo plano, una purga de
REGISTRO que faltaba y varias actualizaciones de estado a destiempo), con tests
de regresión en los casos de pérdida de datos y seguridad. El resto del código
nuevo se revisó sin hallazgos. Los riesgos aceptados quedan arriba con su motivo
y su propuesta de mantenimiento.
