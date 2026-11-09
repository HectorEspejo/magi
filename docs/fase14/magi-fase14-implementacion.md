# MAGI - Fase 14: Android, Segunda Vuelta — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. Refleja lo
> que de verdad se ha implementado, las desviaciones y lo que queda pendiente.
> El análisis funcional parte de este informe para actualizar la especificación.

**Fecha:** 9 de noviembre de 2026
**Rama:** `fase14`

## Resumen de lo implementado

La Fase 14 está implementada y verificada de extremo a extremo. El móvil pasa a
ser el móvil de guardia con la lógica del núcleo y sigue siendo lector de la
sincronización.

### Núcleo compartido

- **Sondeo en el núcleo (D274, R125).** `crates/magi-nucleo/src/flota/` con el
  script embebido `crates/magi-nucleo/scripts/sondeo.sh` (argumento
  `<motores>`), lectura de las líneas `MAGI_*`, estados (FRÍA, CAÍDA,
  ALCANZABLE, CARGA, NOMINAL) con culpables, fallos, tasa de red y formatos
  legibles. El escritorio conserva solo su orquestación; sus pruebas de pintado
  pasan sin cambios (303 instantáneas de `redimensionado` en verde).
- **Deliberación en el núcleo (D275).** `crates/magi-nucleo/src/deliberacion/`:
  plan, consenso, MELCHIOR-1, BALTHASAR-2, CASPER-3, orquestación con plazo duro
  de 2 s y proveedores por plataforma. El escritorio delibera con
  `ProveedoresEscritorio` (servidor de sesiones, SFTP no interactivo y
  `sh -c` local).
- **`sftp_lectura.rs`**: listar un directorio por SFTP sin recursión ni
  enlaces, con plazo y tope de 10 000 entradas.
- **`snippets/`**: resolución de destinos y motivos de deliberación
  (`destinos.rs`), estados por host, resultado de la ejecución y recorte a
  1 MiB con saneado (`ejecucion.rs`).
- **Fijos compartidos de la F13 (D283)**: guiones falsos únicos en
  `crates/magi-nucleo/tests/fijos/servicios/falsos/` y helper
  `tests/fijos/servicios/binarios.rs` usado con `#[path]` por escritorio,
  núcleo y móvil.
- **Autenticación automática del móvil**: `autenticar_movil` prueba la
  contraseña guardada también sin diálogo (§7.2).

### Móvil (`magi-movil`)

- **Contrato UniFFI completo (§5.1, §5.2, §5.4)**: 29 funciones nuevas, 5
  métodos nuevos en `OyenteNucleo`, DTO propios para flota, servicios, logs,
  sesiones con orden, snippets, ejecuciones, deliberación, registro y ajustes,
  y errores con instrucción.
- **Conexiones (D277)**: una conexión por host para todo, usos RAII por
  identidad, gracia de 60 s con temporizador, cerrojo por host, vigilante de
  500 ms con `OyenteConexion`, políticas interactiva/automática (la automática
  nunca abre diálogos) y ejecutor remoto con plazo total y tope por flujo.
- **Flota (D266, D285)**: sondeo con semáforo de 8 y 5 s por host, sin solapes,
  SONDEOS con `fallos_json`, `sondeo_fallido`/`sondeo_recuperado` y nunca
  `HOSTS.ultimo_estado`; lista completa por `OyenteNucleo::flota`; ficha con
  bloque FLOTA, verificaciones y snippet al conectar.
- **Ajustes del móvil (D280)**: normalización con avisos y guardado en el
  estado del núcleo.
- **Servicios, sudo y logs (F13 en el móvil)**: toda la flota, listado con
  single-flight y caché, consumo, detalle, preparar/ejecutar acciones con
  fases en vivo, una acción por objetivo, recompra de deliberación, REGISTRO
  `servicio_accion`, `ejecucion_resultado`, sudo en memoria (`Zeroizing`),
  y logs con anillo (10 000 líneas y 8 MiB), avisos cada 100 ms y ventanas
  filtradas de hasta 500.
- **Snippets y registro**: lista y preparación con variables escapadas,
  validación de contexto y motivos de deliberación; ejecuciones con semáforo
  de 8, tope de 1 MiB por flujo, parar al primer fallo, cancelar/repetir/limpiar
  y `snippet_ejecutado` por host; registro por cursor con filtros y detalle de
  la deliberación; purga de más de 90 días al arrancar.
- **Deliberación del móvil (D275, D282)**: proveedores del móvil, tabla,
  consenso por unanimidad, forzar con motivo ≥ 10, DELIBERACIONES con usuario
  «móvil (<equipo>)», `consumir` de un solo uso y `registrar_resultado`.
- **Snippet al conectar (§7.9)**: al abrir o reconectar una sesión normal se
  escribe el comando si no es crítico y no tiene variables; si no, se avisa en
  la pantalla de la sesión. Las sesiones con orden no lo llevan.
- **Servidor de pruebas**: `examples/servidor_pruebas.rs` como envoltorio de
  `tests/comun/servidor_pruebas.rs` (SSH con exec/PTY/SFTP, eco, control,
  binarios falsos y backups, todo en 127.0.0.1). El job `humo` de
  `android.yml` lo arranca antes del emulador, espera «listo:» y lo para
  siempre (R106/R107).
- **Pruebas de extremo a extremo en Rust**: `flota_e2e` (4), `snippets_movil_e2e`
  (2), `servicios_movil_e2e` (4), `deliberacion_movil_e2e` (5),
  `tunel_local_e2e` (2) y `registro_purga_e2e` (1): 18 pruebas en verde.

### App Android

- **Contrato y adaptador real**: modelos de vista nuevos, `FuenteNucleo` con
  5 flujos y 34 funciones, `AdaptadorOyenteNucleo` cableado (listas completas
  por `StateFlow`, avisos sin conflar) y `FuenteNucleoReal` contra el núcleo
  real.
- **Navegación (D265)**: FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS, con
  barra inferior, riel y MÁS con Snippets, Resultados, Registro y Ajustes;
  pantallas autocontenidas con dos paneles en Flota, Servicios, Snippets y
  Registro; terminal a pantalla completa.
- **Flota**: pantalla de inicio con grupos plegables, búsqueda, fila de dos
  líneas con glifo y palabra del estado, ⚠N, culpables y motivo; sondeo al
  abrir/volver, al deslizar (con sincronización) y refresco automático solo a
  la vista y solo de hosts visibles; ficha con bloque FLOTA, verificaciones,
  snippet al conectar y botones SERVICIOS/SNIPPETS; tocar un fallo de «Toda la
  flota» abre el host con ese elemento.
- **Servicios y logs**: toda la flota, secciones con fallidos y vigilados
  arriba, filtro y recuentos, detalle con pestañas, acciones permitidas con
  confirmación o deliberación, aviso rojo de unidad crítica, resultado, sudo
  enmascarado con `FLAG_SECURE`, acciones en curso, logs con ventanas,
  filtros, pausa/AL FINAL, «N omitidas», fin con motivo, A SESIÓN y SHELL.
- **Snippets, Resultados, Registro y deliberación**: ejecutar con casillas de
  hosts, variables y parar al primer fallo; resultados con salida por host,
  copia sensible, cancelar/repetir/limpiar; registro con filtros y detalle;
  diálogo MAGI con tabla host × comprobación, consenso, FORZAR con motivo y
  botón de mantener pulsado 1 s con relleno, vibración y TalkBack.
- **Ajustes, segundo plano y accesibilidad**: ajustes de Flota, Servicios y
  Deliberación en DataStore con acotación y aviso, aplicación al núcleo al
  arrancar/volver, olvido del sudo al bloquear, servicio en primer plano con
  cuatro recuentos, visibilidad con `ProcessLifecycleOwner`, aviso al volver
  si Android cerró MAGI con trabajo pendiente, glifo+texto, TalkBack y 48 dp.
- **Calidad**: `:app:compileDebugKotlin` y `:app:testDebugUnitTest`
  (203 pruebas, 0 fallos), `:app:lint` con 0 errores (73 avisos heredados) y
  `connectedDebugAndroidTest` 12/12 en el emulador.

### Cierre

- **Revisión adversarial** en tres frentes (núcleo/escritorio, móvil Rust y app
  Android) con correcciones de seguridad y de pérdida de datos, documentada en
  `docs/fase14/magi-fase14-revision-adversarial.md`.
- App **0.2.0** (`versionCode` 2009), `android/CHANGELOG.md` y la sección
  «App Android» del README al día. El escritorio no cambia de versión.

## Desviaciones respecto a la especificación

1. **`PlanDeliberado` del núcleo con dos variantes** (`Snippet` y
   `Servicio`): el escritorio conserva su enum de tres variantes, que añade la
   sincronización, para no arrastrar al núcleo sus tipos.
2. **`Limites::desde_config` en el escritorio mediante el trait local
   `LimitesDeConfig`**, porque no se puede añadir un impl inherente a un tipo
   del núcleo desde otro crate.
3. **Rangos de los ajustes del móvil**: donde la especificación remite a «los
   rangos del escritorio» y `config.toml` no valida (umbrales de Flota, horas
   de backup y minutos de salud), se fijan en el móvil: carga por núcleo
   0,1-16,0; memoria y disco 0-100; backup 1-720 h; salud 1-1440 min.
4. **`sondeo_recuperado`** además de `sondeo_fallido` al volver de CAÍDA,
   por paridad con el escritorio (R12).
5. **La deliberación del móvil es una `suspend` que devuelve la tabla final**,
   sin retrollamada nueva en `OyenteNucleo` (el contrato §5.2 no la tiene).
6. **Memoria de deliberaciones acotada (64)** y consumibles una sola vez por
   sesión: al reiniciar MAGI hay que volver a deliberar.
7. **Registro por cursor de id**, no por offset.
8. **Propiedades de unidades y contenedores como lista clave/valor** en la FFI.
9. **Suscripciones de log paradas**: se sustituyen al volver a pedir el mismo
   objetivo y se descartan a los 10 min o si hay más de 8.
10. **`servidor_pruebas`**: implementación en `tests/comun/servidor_pruebas.rs`
    para usarse in-process en Rust; el ejemplo queda como envoltorio. El job
    `humo` instala `openssh-server` para el `sftp-server` de BALTHASAR-2.
11. **El job `humo` corre también en pull requests** (antes solo en main y
    etiquetas): el AC del checklist exige que una PR que rompe un túnel falle
    en CI. Cuesta ~75 min por PR que toca el núcleo o la app.
12. **Nueva excepción de `cargo deny` RUSTSEC-2026-0459** (`libsqlite3-sys`
    0.38.2 con SQLite embebido antiguo, vector local con un `-wal` ajeno; el
    parche exige rusqlite 0.41 y Rust 1.89). Con fecha de caducidad 2027-05-09
    comprobada en CI.
13. **Aviso local del snippet al conectar no válido**: se escribe en la
    pantalla de la sesión (nunca al host) en lugar de un diálogo, para no
    romper el flujo automático de la conexión.
14. **`tests/servicios_servidor.rs`** conserva su copia incrustada de los
    binarios falsos: migrarla a los fijos compartidos es un refactor grande
    fuera del alcance de la fase.
15. **`Visibilidad.kt`** (ProcessLifecycleOwner) existe y se expone en el
    contenedor de plataforma, pero el gate «a la vista» de cada pantalla se
    hace con `repeatOnLifecycle`/`Lifecycle`; el objeto no se usa
    directamente (equivalente funcional).

## Estructura de archivos creada/modificada

### Núcleo

- Creados: `crates/magi-nucleo/scripts/sondeo.sh`;
  `src/flota/{mod,lectura,estado}.rs`;
  `src/deliberacion/{plan,consenso,salud,backup,tests,comprobaciones}.rs`;
  `src/sftp_lectura.rs`; `src/snippets/{destinos,ejecucion}.rs`;
  `tests/fijos/servicios/binarios.rs`;
  `tests/generar_fixture_flota_android.rs`.
- Modificados: `src/lib.rs`, `src/deliberacion/mod.rs`,
  `src/snippets/mod.rs`, `src/conexion/cliente.rs`,
  `src/snippets/variables.rs`, `Cargo.toml`, `tests/servicios_script.rs`,
  `tests/generar_fixture_tunel_android.rs`.

### Escritorio

- Modificados: `src/flota/mod.rs`, `src/deliberacion/mod.rs`,
  `src/app/dialogo_magi.rs`, `src/main.rs`, `src/app.rs`,
  `tests/deliberacion.rs`, `tests/servicios_e2e.rs`, `tests/servicios_ui.rs`.
- Borrados: `src/flota/{parser,estado}.rs`,
  `src/deliberacion/{salud,backup,tests}.rs`,
  `tests/servicios_e2e/binarios.rs`.

### Móvil

- Creados: `crates/magi-movil/src/{conexiones,flota,ajustes,servicios,logs,
  ejecuciones,deliberacion,registro,snippets}.rs`;
  `tests/comun/{mod,servidor_pruebas}.rs`;
  `tests/{flota,snippets_movil,deliberacion_movil,servicios_movil,
  registro_purga}_e2e.rs`; `examples/servidor_pruebas.rs` (renombrado).
- Modificados: `src/{api,retrollamadas,lib,estado,gestor,inventario}.rs`,
  `Cargo.toml`, `tests/tunel_local_e2e.rs`.

### Android

- Creados: `ui/modelos/Modelos{Flota,Servicios,Logs,Snippets,Ejecuciones,
  Deliberacion,Registro}.kt`; `ui/flota/`; `ui/mas/`; `ui/servicios/`;
  `ui/logs/`; `ui/snippets/`; `ui/resultados/`; `ui/registro/`;
  `ui/dialogos/{DialogoDeliberacion,BotonMantener,VistaModeloDeliberacion,
  AsistenteDeliberacion}.kt`; `ui/ajustes/PantallaAjustes{Flota,Servicios,
  Deliberacion,Componentes}.kt`; `plataforma/{Visibilidad,ContadorRecintos}.kt`;
  `androidTest/{AyudasPrueba,PruebaFlota,PruebaServicios,PruebaSnippets}.kt`;
  `androidTest/assets/flota/`.
- Modificados: `ui/AppMagi.kt`, `navegacion/{Destino,Rutas}.kt`,
  `ui/fuente/FuenteNucleo.kt` y dobles, `ui/modelos/`, `ui/vistaModelo/`,
  `preferencias/PreferenciasMagi.kt`, `plataforma/`, `nucleo/`,
  `res/values/strings*.xml`, `gradle/`, `androidTest/` existentes.

### Documentación y CI

- `.github/workflows/android.yml` (job humo con servidor de pruebas y en PRs),
  `.github/workflows/ci.yml` (caducidad de la excepción RUSTSEC-2026-0459),
  `deny.toml`, `AGENTS.md`/`CLAUDE.md`, `README.md`,
  `android/CHANGELOG.md`, `docs/fase14/magi-maestro.md`,
  `docs/publicar-android.md`, `docs/fase14/magi-fase14-revision-adversarial.md`.

## Decisiones técnicas tomadas durante el desarrollo

- **Contrato antes que pistas (T63)**: primero el contrato compilable del
  núcleo, del móvil (UniFFI) y de la app (Kotlin); después las pistas en
  paralelo con ámbitos de escritura disjuntos.
- **Proveedores por plataforma (D275)**: sondeo nuevo, listado de un
  directorio y comando local; el móvil devuelve `NoDisponible` para CASPER-3.
- **Conexión única con usos RAII**: `UsoConexion` se suelta con `Drop`; nadie
  descuenta dos veces y una conexión antigua no toca la nueva del mismo host.
- **Ejecutor remoto único** (`ejecutar_remoto` y `abrir_canal_remoto`) para
  sondeo, servicios, sudo, logs y ejecuciones, con plazo total y tope por
  flujo.
- **El motor de ejecuciones usa `snippets::preparar_interno`** y valida la
  deliberación con `deliberacion::consumir` antes de lanzar.
- **Pantallas autocontenidas** con su ViewModel y su disposición de dos
  paneles: AppMagi solo cablea destinos y callbacks, lo que permitió construir
  Flota, Servicios, Snippets, Resultados y Registro en paralelo.
- **Sondeo y refrescos solo a la vista** con `repeatOnLifecycle`; acciones y
  ejecuciones en el runtime del núcleo para sobrevivir a la cancelación de la
  corrutina de la app (D279).

## Funcionalidades del checklist completadas

Las 98 funcionalidades del checklist de la Fase 14 están implementadas y
verificadas; el checklist se ha marcado completo. Texto exacto:

- [x] `crates/magi-nucleo/src/flota/` y `scripts/sondeo.sh` con el sondeo movido del escritorio (con el argumento `<motores>`, R125): lectura de las líneas `MAGI_*`, estados (FRÍA, CAÍDA, ALCANZABLE, CARGA y NOMINAL) con sus culpables, fallos y tasa de red
- [x] `crates/magi-nucleo/src/deliberacion/` con `PlanDeliberado`, MELCHIOR-1, BALTHASAR-2 y CASPER-3 detrás de proveedores que pone cada plataforma (sondeo, listado de un directorio y orden local), el plazo de 2 s y el consenso por unanimidad; el escritorio delibera con sus proveedores de siempre
- [x] `crates/magi-nucleo/src/sftp_lectura.rs`: listar un directorio por SFTP (nombre, tipo, tamaño y mtime), sin recursión ni enlaces, con plazo para abrir el subsistema y 10 000 entradas como mucho; el SFTP del escritorio no cambia
- [x] `snippets/` del núcleo gana la resolución de destinos, cuándo se delibera, los estados de cada host de una ejecución y el resultado de la ejecución (`ok`, `parcial`, `error` o `cancelada`)
- [x] Las salidas grabadas y los binarios falsos de la F13 (`systemctl`, `journalctl`, `docker`, `podman`, `sudo`, `nginx`) pasan a `crates/magi-nucleo/tests/fijos/`, compartidos por las pruebas del escritorio y las del móvil
- [x] Pruebas del núcleo: lectura del sondeo con las salidas de la F2 y la F13, estados y culpables, BALTHASAR-2 con patrón y antigüedad, CASPER-3 no disponible, consenso, destinos y estados de una ejecución
- [x] Una conexión por host para todo (sesiones, túneles, sondeo, servicios, logs, ejecuciones y el listado de BALTHASAR-2) que, sin usos, sigue abierta 60 s (gracia) y luego se cierra, también con la app en segundo plano
- [x] Las aperturas automáticas (sondeo, refrescos, consumo, comprobaciones de la deliberación y ejecuciones) nunca abren un diálogo: una huella sin aceptar, o no tener ni clave del móvil instalada ni contraseña guardada, dan un error con instrucción
- [x] El vigilante de cada conexión (T97) cubre todo lo nuevo: si el host la cierra, las acciones en curso acaban en `error`, los logs en «se cortó la conexión» y los hosts de una ejecución en `error`, con el motivo
- [x] Flota sustituye a Hosts y es la pantalla de inicio: los mismos grupos plegables, la búsqueda, `▶` para conectar, la fila que abre la ficha, el indicador `⇅` y el estado vacío que invita a configurar la sincronización
- [x] Fila de cada host con el glifo y la palabra del estado (FRÍA, sondeando, CAÍDA, ALCANZABLE, CARGA o NOMINAL), `⚠N` si tiene fallos y una segunda línea con las sesiones abiertas y las métricas, los culpables en ámbar, el motivo de la caída o la dirección
- [x] Los hosts que no son `ssh` salen sin sondear, con su último estado y «se abre en el ordenador»
- [x] Sondeo al abrir Flota y al volver a ella, con semáforo de 8, 5 s por host y sin solapar sondeos del mismo host
- [x] Deslizar hacia abajo sondea los hosts de la lista con el filtro aplicado y lanza la pasada de sincronización
- [x] Refresco automático cada 30 s (`flota_refresco_seg`) solo con Flota a la vista y solo de los hosts que se ven (que casan con el filtro y fuera de grupos plegados); con la app en segundo plano o en otra pantalla no se sondea nada
- [x] Cada sondeo se guarda en SONDEOS (20 por host, con `fallos_json`), el paso a CAÍDA anota `sondeo_fallido` y un sondeo nunca cambia `HOSTS.ultimo_estado`
- [x] Ficha del host con el bloque FLOTA: barras de carga, memoria y disco, red, tiempo encendido, vigilados con su estado, FALLOS con sus nombres, y hora y duración del último sondeo
- [x] La ficha enseña, de solo lectura, las verificaciones previas (salud, backup con ruta y patrón, tests) y el snippet al conectar, y gana los botones SERVICIOS y SNIPPETS
- [x] Barra inferior FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS en pantalla compacta y riel con los mismos destinos en el plegable abierto o en pantalla ancha; MÁS agrupa Snippets, Resultados, Registro y Ajustes
- [x] Plegable abierto o pantalla ancha: dos paneles (lista y detalle) en Flota, Servicios, Snippets y Registro, sin perder nada al plegar o desplegar; el terminal sigue a pantalla completa (D209)
- [x] Glifo y texto en todos los estados nuevos (T5), descripciones para TalkBack y objetivos táctiles de 48 dp
- [x] Indicadores de carga y mensajes de error con instrucción en Flota, Servicios, Logs, Snippets, Resultados y Registro
- [x] SERVICIOS abre «Toda la flota»: los fallos de todos los hosts según el último sondeo del móvil, con su hora, y una línea por host caído o sin sondeo; entrar no abre conexiones
- [x] Debajo, la lista de hosts `ssh` con su estado y `⚠N`; tocar un fallo abre ese host con ese elemento, y tocar un host abre sus servicios
- [x] Servicios de un host con tres secciones (Servicios, Timers y Contenedores), los fallidos (primero) y los vigilados `★` arriba y separados del resto, filtro por nombre y recuentos al pie
- [x] Listado con `servicios.sh listar` por la conexión del host, con plazo de 10 s, los límites de la F13 y su hora; si el host deja de responder, se queda el anterior con su hora
- [x] Refresco cada 10 s solo con la pantalla a la vista y sin solapes; tras «no responde» o «sin conexión» se para, y REINTENTAR lo vuelve a lanzar
- [x] Un host sin systemd, o con systemd anterior a 230, abre directamente en Contenedores y no deja elegir las secciones vacías; sin Docker ni Podman, Contenedores lo dice
- [x] Contenedores por proyecto Compose (y «sin proyecto» al final) con estado, imagen, CPU y memoria; el consumo se pide aparte y solo con Contenedores a la vista
- [x] Detalle con tres pestañas: Estado (con la comprobación que se aplicará y si es vigilada o crítica), Dependencias y Unidad (`inspect` en un contenedor; próxima y última ejecución en un timer)
- [x] Sin permiso para el diario o para Docker, aviso con «Dar contraseña de sudo»; con la contraseña en memoria, esa parte se lista otra vez con sudo
- [x] Las acciones en curso del host se ven en su fila con `◐` y la acción, vengan de esta pantalla o de otra
- [x] Botones solo para las acciones permitidas en ese estado (F13 §7.2): arrancar, parar, reiniciar, recargar, activar o desactivar el arranque, lanzar un timer, y arrancar, parar o reiniciar un contenedor o un proyecto
- [x] Lo que no deja nada parado pide una confirmación que nombra la acción, el objetivo, el host y la comprobación previa («¿Recargar nginx.service en web-01? Antes: nginx -t»)
- [x] Lo que deja algo parado (parar, reiniciar, desactivar el arranque, y parar o reiniciar un contenedor o un proyecto) pasa por la deliberación con `PlanDeliberado::Servicio`, y MELCHIOR-1 no cuenta el estado del propio objetivo
- [x] Las unidades críticas (la lista por defecto de la F13) añaden en rojo «⚑ puede cortar tu conexión con web-01» y los hosts que van a través de él
- [x] `magi-movil` ejecuta la acción con `servicios.sh accion`: comprobación de configuración dentro de la acción (conocida o propia de COMPROBACIONES_SERVICIO), espera al estado final y segunda mirada a los 2 s
- [x] Resultado con hecho, fallido (con las últimas 20 líneas del diario), sin confirmar, rechazada por la configuración (con su salida) o error, y refresco del listado
- [x] Una sola acción a la vez por objetivo («hay otra acción en curso»), y `magi-movil` recomprueba la acción y la deliberación (que exista, sea de esta acción y no esté usada) antes de ejecutar
- [x] Cada acción anota `servicio_accion` al terminar (también si falla o la para la comprobación), y su deliberación, `ejecucion_resultado`, antes de devolver el resultado
- [x] Las acciones siguen con la app en segundo plano; si al volver ya acabó, el resultado está esperando
- [x] Si el listado dice `sudo_con_clave` y no hay contraseña en memoria, se pide antes de confirmar o deliberar, con 3 intentos, y se comprueba con `servicios.sh sudo`
- [x] Diálogo de la contraseña con campo enmascarado, «se recuerda 5 min sin uso; nunca se guarda» y `FLAG_SECURE`
- [x] La contraseña vive solo en `magi-movil` (`Zeroizing`), por host, hasta `sudo_memoria_min` sin uso (solo las acciones cuentan), al bloquearse MAGI, si el host la rechaza o con «Olvidar contraseña de sudo»; nunca va a `LlaveroMovil`, DataStore, logcat ni REGISTRO
- [x] LOGS en un servicio, en un timer (su servicio) o en un contenedor: las últimas 200 líneas y el seguimiento con `servicios.sh logs`
- [x] El anillo vive en `magi-movil` (10 000 líneas y 8 MiB) y la app pide ventanas ya filtradas de hasta 500 líneas; el anillo entero nunca cruza la FFI
- [x] Avisos de líneas nuevas cada 100 ms como mucho, y «… N líneas omitidas» donde faltan si el anillo o la lectura descartan
- [x] Filtro por texto y por nivel (todo, info, aviso o error; en contenedores, todo o solo stderr), pausa al desplazarse hacia arriba sin que la vista se mueva, y AL FINAL para seguir
- [x] El seguimiento se para al salir de la pantalla o al pasar MAGI a segundo plano (fin de la entrada estándar del script); las líneas se quedan con «seguimiento parado» y SEGUIR OTRA VEZ
- [x] Fin con motivo (cerrado, terminado, sin permiso con «Dar contraseña de sudo», conexión caída) sin perder las líneas
- [x] A SESIÓN abre el seguimiento completo en una sesión con orden (`journalctl -fu` o `docker logs -f`), con sus colores
- [x] SHELL en un contenedor en marcha abre una sesión con orden (`docker exec -it` con bash o sh); si sudo necesita contraseña, la pide dentro de la sesión
- [x] Las sesiones con orden se nombran «web-01 · web-app-1 (shell)» o «web-01 · app.service (logs)», las arma `magi-movil` desde un tipo cerrado (T108) y no tienen snippet al conectar
- [x] Snippets (en MÁS) con los sincronizados: nombre, destinos («web → 3 hosts»), CRÍTICO, comando, descripción, timeout y último uso, con búsqueda; sin crear, editar ni borrar
- [x] Desde la ficha, SNIPPETS enseña los que tienen ese host entre sus destinos y prepara la ejecución solo en él
- [x] EJECUTAR: los hosts de los destinos con casillas (todos marcados), las variables con su valor por defecto, «parar al primer fallo» (el del snippet) y el timeout
- [x] Las variables se sustituyen en el núcleo, siempre escapadas; una vacía sin valor por defecto no deja seguir y una dentro de comillas o de un heredoc se rechaza
- [x] Deliberación si el snippet es crítico, si va a más de un host o si algún host tiene verificaciones activas; si no, CONTINUAR lanza
- [x] `magi-movil` ejecuta: semáforo de 8, conexión automática por host, salida de hasta 1 MiB por flujo, timeout del snippet y parar al primer fallo, con los estados de la F6
- [x] RESULTADOS (en MÁS y al lanzar): las ejecuciones en curso y terminadas, con hosts, recuentos, `⚑` si fue forzada y el estado de cada host con su código, duración y bytes
- [x] Salida de un host: stdout y stderr por separado, sin secuencias de escape, «… truncada» si pasó de 1 MiB, y COPIAR, marcado como sensible en el portapapeles
- [x] Cancelar (con confirmación si está en curso), repetir (vuelve a deliberar si hace falta) y limpiar las terminadas; las ejecuciones se olvidan 1 h después de acabar
- [x] REGISTRO `snippet_ejecutado` por host y, al acabar una ejecución deliberada, `deliberacion_aprobada` o `deliberacion_forzada` y `ejecucion_resultado`; `usado_veces` y `ultimo_uso_en` como campos propios, sin subir versión
- [x] Snippet al conectar: al abrir o reconectar una sesión a ese host, su comando se escribe al arrancar el shell, sin deliberar; si es crítico o tiene variables, se ignora con aviso
- [x] Diálogo MAGI a pantalla completa: la acción, la tabla host × comprobación (`✓` `✕` `—` `◐`) con el dato de cada veredicto, la barra de consenso N / M y el estado APROBADO, BLOQUEADO o FORZADO
- [x] MELCHIOR-1 usa el último sondeo del móvil si es NOMINAL y de menos de 5 min (`deliberacion_salud_max_min`); si no, sondea dentro del plazo
- [x] BALTHASAR-2 lista `backup_ruta` por SFTP y aprueba si el fichero más reciente que casa con `backup_patron` tiene menos de `deliberacion_backup_horas`
- [x] CASPER-3 no se puede hacer en el móvil: si está activa en un host, rechaza con «solo en el escritorio» y la deliberación queda BLOQUEADA
- [x] Todas las comprobaciones en paralelo, con 2 s cada una y sin diálogos; sin ninguna activa, «sin comprobaciones configuradas», y se sigue pidiendo mantener pulsado
- [x] El botón que ejecuta lo deliberado solo actúa manteniéndolo pulsado 1 s (el `Ctrl+K` del móvil), con relleno de progreso y vibración; soltar antes no hace nada, y con TalkBack se toca dos veces y se mantiene
- [x] FORZAR en BLOQUEADO abre el motivo; con menos de 10 caracteres no se acepta y, con motivo, pasa a FORZADO y se ejecuta manteniendo pulsado
- [x] Al resolverse, `magi-movil` guarda la fila de DELIBERACIONES (resultado, bloqueada, motivo, comprobaciones y usuario «móvil (<equipo>)»), y cancelar anota `deliberacion_cancelada`
- [x] REGISTRO (en MÁS) con las entradas del propio móvil: fecha, tipo, host y resultado, de la más nueva a la más vieja y por páginas
- [x] Filtros por host, por tipo (conexiones, huellas, túneles, sondeos, snippets, deliberaciones, servicios, claves y sincronización) y por texto
- [x] Detalle de una entrada; en las `deliberacion_*`, sus comprobaciones leídas de DELIBERACIONES, y una deliberación sin `ejecucion_resultado` dice «sin resultado: MAGI se cerró durante la ejecución»
- [x] La purga de las entradas de más de 90 días sigue como en la F11; sin exportar
- [x] Ajustes gana FLOTA (refresco y umbrales de carga, memoria y disco), SERVICIOS (refresco, espera, motores de contenedores y minutos de sudo) y DELIBERACIÓN (horas del backup y minutos de salud), con los valores por defecto y los rangos del escritorio
- [x] Los ajustes viven en DataStore y llegan al núcleo con `aplicar_ajustes`; un valor fuera de rango se acota con aviso, y el sondeo recibe los motores de contenedores de los ajustes
- [x] El servicio en primer plano sigue mientras haya sesiones, túneles, acciones o ejecuciones («MAGI · 2 sesiones · 1 acción») y se para sin ninguna
- [x] Sondeo, refrescos de Servicios y logs solo con su pantalla a la vista, según el ciclo de vida de Android; nada de eso con la app en segundo plano
- [x] Si Android cerró MAGI con acciones o ejecuciones en curso, al volver lo dice (las órdenes pueden haber seguido en el host) y ofrece la guía de batería
- [x] `aplicar_ajustes(ajustes)`, `flota(filtro)`, `sondear(hosts)` y `ficha_host(uuid)` ampliada con el detalle de Flota, las verificaciones y el snippet al conectar
- [x] `servicios_toda_la_flota()`, `servicios_listar(host, refresco)`, `servicios_consumo(host)` y `servicios_detalle(host, objetivo)`
- [x] `preparar_accion(host, objetivo, accion)` y `servicio_accion(host, objetivo, accion, deliberacion)`
- [x] `sudo_comprobar(host, contrasena)` y `sudo_olvidar(host)`
- [x] `logs_seguir(host, objetivo, lineas)`, `logs_ventana(suscripcion, filtro)`, `logs_parar(suscripcion)` y `abrir_sesion_con_orden(host, orden)`
- [x] `snippets(filtro)`, `snippets_de_host(host)`, `snippet(uuid)`, `preparar_ejecucion(snippet, hosts, variables)` y `ejecutar_snippet(preparacion, parar_al_fallo, deliberacion)`
- [x] `ejecuciones()`, `salida_ejecucion(ejecucion, host)`, `cancelar_ejecucion(id)`, `repetir_ejecucion(id)` y `limpiar_ejecuciones()`
- [x] `deliberar(plan)` y `resolver_deliberacion(id, decision)`
- [x] `registro(filtro, desde, cuantas)` y `registro_detalle(id)`
- [x] `OyenteNucleo` gana `flota(lista)`, `acciones_servicio(lista)`, `accion_servicio_hecha(resultado)`, `logs(estado)` y `ejecuciones(lista)`; el contrato de la F11 no cambia
- [x] Pruebas de extremo a extremo en Rust (`magi-movil` con el servidor SSH de prueba y los binarios falsos): sondeo y gracia, listado, acción con sudo y deliberación, logs con ventanas, snippet en dos hosts y BALTHASAR-2 por SFTP (T95)
- [x] `servidor_pruebas` (antes `servidor_pruebas_tunel`) gana hosts con los binarios falsos y un directorio de backups y sigue escuchando en `127.0.0.1`; pruebas instrumentadas de Flota, Servicios y Snippets en el emulador
- [x] El job `humo` de `android.yml` arranca el servidor de pruebas antes del emulador, así que las pruebas de extremo a extremo del móvil también corren en CI (R106)
- [x] Pruebas JVM de los ViewModels nuevos y del botón de mantener pulsado (soltar antes, completar, una sola vez y TalkBack)
- [x] El escritorio no cambia: `TZ=UTC cargo test --workspace --locked`, clippy, fmt, `cargo deny` y `shellcheck` en verde sin tocar ninguna aserción ni instantánea
- [x] App 0.2.0 en `android/version.properties` (`versionCode` 2009), con `android/CHANGELOG.md` y la sección «App Android» del README al día; el escritorio no cambia de versión
- [x] Revisión adversarial de cierre (T31, T54) sobre el núcleo movido, `magi-movil` y la app, con su informe en `docs/fase14/magi-fase14-revision-adversarial.md`

## Pendientes y bloqueos

- **Validación manual en el plegable real (R100)**: MagicOS, plegado,
  batería (R127), Tailscale y TalkBack solo se pueden comprobar en el móvil de
  validación; el guion está en §11 del informe de la fase y se ha ejecutado en
  lo que permite el emulador. No bloquea la entrega del código.
- **Primera ejecución real del job `humo` en CI**: el workflow está
  configurado y validado con actionlint, y las mismas pruebas instrumentadas
  corren en local (12/12); falta verlo en GitHub Actions.
- **Deuda de lint**: 73 avisos de `:app:lint` heredados de la F11 o de otras
  pistas (BatteryLife, PluralsCandidate, UnusedResources, ModifierParameter en
  firmas fijadas, LocalContext en ajustes). 0 errores.
- **Riesgos aceptados de la revisión adversarial** (detallados en
  `docs/fase14/magi-fase14-revision-adversarial.md`): canales internos de
  `CanalRemoto` sin tope, `con_almacen` síncrono en rutas async, anotaciones
  best-effort, `forzar_cierre`, ruta de backup relativa heredada, `Consenso`
  duplicado, `Listado.truncado` en BALTHASAR, posible pánico de proveedor,
  canales UNLIMITED de `FuenteNucleoReal` y `catch(Throwable)` heredado.
- **Cierres de fases anteriores** que el prompt daba por hechos (R125 sí se ha
  hecho aquí; la release 0.1.0, los trámites R103/R104/R107 y la corrección 10b
  siguen fuera de esta fase).
- El emulador y el `servidor_pruebas` de las pruebas locales quedaron parados
  al cerrar.

## Ejecución y pruebas

```sh
# Núcleo y escritorio (el escritorio no cambia: instantáneas incluidas)
TZ=UTC cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
cargo deny check
shellcheck scripts/*.sh crates/magi-nucleo/scripts/*.sh

# Móvil (Rust): unitarias y extremo a extremo sin red externa
cargo test -p magi-movil --lib --offline
cargo test -p magi-movil --test tunel_local_e2e --test flota_e2e \
  --test snippets_movil_e2e --test servicios_movil_e2e \
  --test deliberacion_movil_e2e --test registro_purga_e2e --offline

# Servidor de pruebas (para las pruebas instrumentadas del emulador)
cargo run -p magi-movil --example servidor_pruebas

# Android (SDK + NDK r30; el wrapper usa local.properties)
cd android
./gradlew :app:compileDebugKotlin :app:testDebugUnitTest   # 203 pruebas, 0 fallos
./gradlew :app:lint                                        # 0 errores
./gradlew :app:connectedDebugAndroidTest                   # 12/12 con el emulador
```

Regenerar las fixtures de androidTest:

```sh
cargo test -p magi-nucleo --test generar_fixture_android -- --ignored
cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored
cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored
```

No hay migración: la base sigue en `user_version` 9 y el móvil solo escribe
SONDEOS, DELIBERACIONES, REGISTRO y los campos propios de SNIPPETS.
