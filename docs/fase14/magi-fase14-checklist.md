# MAGI - Checklist Fase 14: Android, Segunda Vuelta

## Núcleo compartido
- [x] `crates/magi-nucleo/src/flota/` y `scripts/sondeo.sh` con el sondeo movido del escritorio (con el argumento `<motores>`, R125): lectura de las líneas `MAGI_*`, estados (FRÍA, CAÍDA, ALCANZABLE, CARGA y NOMINAL) con sus culpables, fallos y tasa de red
  - AC: Dado el escritorio tras el cambio, cuando se ejecutan sus pruebas de Flota y sus instantáneas, entonces pasan sin cambiar ninguna aserción ni instantánea
- [x] `crates/magi-nucleo/src/deliberacion/` con `PlanDeliberado`, MELCHIOR-1, BALTHASAR-2 y CASPER-3 detrás de proveedores que pone cada plataforma (sondeo, listado de un directorio y orden local), el plazo de 2 s y el consenso por unanimidad; el escritorio delibera con sus proveedores de siempre
- [x] `crates/magi-nucleo/src/sftp_lectura.rs`: listar un directorio por SFTP (nombre, tipo, tamaño y mtime), sin recursión ni enlaces, con plazo para abrir el subsistema y 10 000 entradas como mucho; el SFTP del escritorio no cambia
- [x] `snippets/` del núcleo gana la resolución de destinos, cuándo se delibera, los estados de cada host de una ejecución y el resultado de la ejecución (`ok`, `parcial`, `error` o `cancelada`)
- [x] Las salidas grabadas y los binarios falsos de la F13 (`systemctl`, `journalctl`, `docker`, `podman`, `sudo`, `nginx`) pasan a `crates/magi-nucleo/tests/fijos/`, compartidos por las pruebas del escritorio y las del móvil
- [x] Pruebas del núcleo: lectura del sondeo con las salidas de la F2 y la F13, estados y culpables, BALTHASAR-2 con patrón y antigüedad, CASPER-3 no disponible, consenso, destinos y estados de una ejecución

## Conexiones del móvil
- [x] Una conexión por host para todo (sesiones, túneles, sondeo, servicios, logs, ejecuciones y el listado de BALTHASAR-2) que, sin usos, sigue abierta 60 s (gracia) y luego se cierra, también con la app en segundo plano
  - AC: Dada Flota abierta con un host ya sondeado, cuando vuelve a sondear a los 30 s, entonces usa la misma conexión sin autenticarse otra vez
- [x] Las aperturas automáticas (sondeo, refrescos, consumo, comprobaciones de la deliberación y ejecuciones) nunca abren un diálogo: una huella sin aceptar, o no tener ni clave del móvil instalada ni contraseña guardada, dan un error con instrucción
  - AC: Dado un host con la huella sin aceptar, cuando Flota lo sondea, entonces queda CAÍDA con «huella sin aceptar: conéctate una vez desde la ficha» y no aparece ningún diálogo
- [x] El vigilante de cada conexión (T97) cubre todo lo nuevo: si el host la cierra, las acciones en curso acaban en `error`, los logs en «se cortó la conexión» y los hosts de una ejecución en `error`, con el motivo

## Flota
- [x] Flota sustituye a Hosts y es la pantalla de inicio: los mismos grupos plegables, la búsqueda, `▶` para conectar, la fila que abre la ficha, el indicador `⇅` y el estado vacío que invita a configurar la sincronización
- [x] Fila de cada host con el glifo y la palabra del estado (FRÍA, sondeando, CAÍDA, ALCANZABLE, CARGA o NOMINAL), `⚠N` si tiene fallos y una segunda línea con las sesiones abiertas y las métricas, los culpables en ámbar, el motivo de la caída o la dirección
- [x] Los hosts que no son `ssh` salen sin sondear, con su último estado y «se abre en el ordenador»
- [x] Sondeo al abrir Flota y al volver a ella, con semáforo de 8, 5 s por host y sin solapar sondeos del mismo host
- [x] Deslizar hacia abajo sondea los hosts de la lista con el filtro aplicado y lanza la pasada de sincronización
  - AC: Dada Flota con el filtro «web», cuando se desliza hacia abajo, entonces se sondean solo los hosts que casan y se hace una pasada
- [x] Refresco automático cada 30 s (`flota_refresco_seg`) solo con Flota a la vista y solo de los hosts que se ven (que casan con el filtro y fuera de grupos plegados); con la app en segundo plano o en otra pantalla no se sondea nada
  - AC: Dada Flota abierta, cuando la app pasa a segundo plano, entonces no sale ningún sondeo hasta volver
- [x] Cada sondeo se guarda en SONDEOS (20 por host, con `fallos_json`), el paso a CAÍDA anota `sondeo_fallido` y un sondeo nunca cambia `HOSTS.ultimo_estado`
- [x] Ficha del host con el bloque FLOTA: barras de carga, memoria y disco, red, tiempo encendido, vigilados con su estado, FALLOS con sus nombres, y hora y duración del último sondeo
- [x] La ficha enseña, de solo lectura, las verificaciones previas (salud, backup con ruta y patrón, tests) y el snippet al conectar, y gana los botones SERVICIOS y SNIPPETS

## Navegación e interfaz
- [x] Barra inferior FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS en pantalla compacta y riel con los mismos destinos en el plegable abierto o en pantalla ancha; MÁS agrupa Snippets, Resultados, Registro y Ajustes
- [x] Plegable abierto o pantalla ancha: dos paneles (lista y detalle) en Flota, Servicios, Snippets y Registro, sin perder nada al plegar o desplegar; el terminal sigue a pantalla completa (D209)
- [x] Glifo y texto en todos los estados nuevos (T5), descripciones para TalkBack y objetivos táctiles de 48 dp
- [x] Indicadores de carga y mensajes de error con instrucción en Flota, Servicios, Logs, Snippets, Resultados y Registro

## Servicios: vista y datos
- [x] SERVICIOS abre «Toda la flota»: los fallos de todos los hosts según el último sondeo del móvil, con su hora, y una línea por host caído o sin sondeo; entrar no abre conexiones
  - AC: Dado un host sin sondeo, cuando se abre Servicios, entonces sale «db-02: sin sondeo» y no se abre ninguna conexión
- [x] Debajo, la lista de hosts `ssh` con su estado y `⚠N`; tocar un fallo abre ese host con ese elemento, y tocar un host abre sus servicios
- [x] Servicios de un host con tres secciones (Servicios, Timers y Contenedores), los fallidos (primero) y los vigilados `★` arriba y separados del resto, filtro por nombre y recuentos al pie
- [x] Listado con `servicios.sh listar` por la conexión del host, con plazo de 10 s, los límites de la F13 y su hora; si el host deja de responder, se queda el anterior con su hora
- [x] Refresco cada 10 s solo con la pantalla a la vista y sin solapes; tras «no responde» o «sin conexión» se para, y REINTENTAR lo vuelve a lanzar
- [x] Un host sin systemd, o con systemd anterior a 230, abre directamente en Contenedores y no deja elegir las secciones vacías; sin Docker ni Podman, Contenedores lo dice
- [x] Contenedores por proyecto Compose (y «sin proyecto» al final) con estado, imagen, CPU y memoria; el consumo se pide aparte y solo con Contenedores a la vista
- [x] Detalle con tres pestañas: Estado (con la comprobación que se aplicará y si es vigilada o crítica), Dependencias y Unidad (`inspect` en un contenedor; próxima y última ejecución en un timer)
- [x] Sin permiso para el diario o para Docker, aviso con «Dar contraseña de sudo»; con la contraseña en memoria, esa parte se lista otra vez con sudo
- [x] Las acciones en curso del host se ven en su fila con `◐` y la acción, vengan de esta pantalla o de otra

## Acciones de servicio
- [x] Botones solo para las acciones permitidas en ese estado (F13 §7.2): arrancar, parar, reiniciar, recargar, activar o desactivar el arranque, lanzar un timer, y arrancar, parar o reiniciar un contenedor o un proyecto
- [x] Lo que no deja nada parado pide una confirmación que nombra la acción, el objetivo, el host y la comprobación previa («¿Recargar nginx.service en web-01? Antes: nginx -t»)
- [x] Lo que deja algo parado (parar, reiniciar, desactivar el arranque, y parar o reiniciar un contenedor o un proyecto) pasa por la deliberación con `PlanDeliberado::Servicio`, y MELCHIOR-1 no cuenta el estado del propio objetivo
- [x] Las unidades críticas (la lista por defecto de la F13) añaden en rojo «⚑ puede cortar tu conexión con web-01» y los hosts que van a través de él
- [x] `magi-movil` ejecuta la acción con `servicios.sh accion`: comprobación de configuración dentro de la acción (conocida o propia de COMPROBACIONES_SERVICIO), espera al estado final y segunda mirada a los 2 s
  - AC: Dada una configuración de Nginx rota, cuando se recarga, entonces no se recarga y el resultado enseña la salida de `nginx -t`
- [x] Resultado con hecho, fallido (con las últimas 20 líneas del diario), sin confirmar, rechazada por la configuración (con su salida) o error, y refresco del listado
- [x] Una sola acción a la vez por objetivo («hay otra acción en curso»), y `magi-movil` recomprueba la acción y la deliberación (que exista, sea de esta acción y no esté usada) antes de ejecutar
- [x] Cada acción anota `servicio_accion` al terminar (también si falla o la para la comprobación), y su deliberación, `ejecucion_resultado`, antes de devolver el resultado
- [x] Las acciones siguen con la app en segundo plano; si al volver ya acabó, el resultado está esperando
  - AC: Dado un reinicio lento en curso, cuando se sale de MAGI y se vuelve, entonces se ve el resultado y está en Registro

## sudo
- [x] Si el listado dice `sudo_con_clave` y no hay contraseña en memoria, se pide antes de confirmar o deliberar, con 3 intentos, y se comprueba con `servicios.sh sudo`
- [x] Diálogo de la contraseña con campo enmascarado, «se recuerda 5 min sin uso; nunca se guarda» y `FLAG_SECURE`
- [x] La contraseña vive solo en `magi-movil` (`Zeroizing`), por host, hasta `sudo_memoria_min` sin uso (solo las acciones cuentan), al bloquearse MAGI, si el host la rechaza o con «Olvidar contraseña de sudo»; nunca va a `LlaveroMovil`, DataStore, logcat ni REGISTRO
  - AC: Dada una contraseña de sudo en memoria, cuando MAGI se bloquea, entonces la siguiente acción la vuelve a pedir

## Logs y sesiones con orden
- [x] LOGS en un servicio, en un timer (su servicio) o en un contenedor: las últimas 200 líneas y el seguimiento con `servicios.sh logs`
- [x] El anillo vive en `magi-movil` (10 000 líneas y 8 MiB) y la app pide ventanas ya filtradas de hasta 500 líneas; el anillo entero nunca cruza la FFI
- [x] Avisos de líneas nuevas cada 100 ms como mucho, y «… N líneas omitidas» donde faltan si el anillo o la lectura descartan
- [x] Filtro por texto y por nivel (todo, info, aviso o error; en contenedores, todo o solo stderr), pausa al desplazarse hacia arriba sin que la vista se mueva, y AL FINAL para seguir
- [x] El seguimiento se para al salir de la pantalla o al pasar MAGI a segundo plano (fin de la entrada estándar del script); las líneas se quedan con «seguimiento parado» y SEGUIR OTRA VEZ
  - AC: Dado un panel de logs siguiendo, cuando MAGI pasa a segundo plano, entonces el proceso de `journalctl` del host termina
- [x] Fin con motivo (cerrado, terminado, sin permiso con «Dar contraseña de sudo», conexión caída) sin perder las líneas
- [x] A SESIÓN abre el seguimiento completo en una sesión con orden (`journalctl -fu` o `docker logs -f`), con sus colores
- [x] SHELL en un contenedor en marcha abre una sesión con orden (`docker exec -it` con bash o sh); si sudo necesita contraseña, la pide dentro de la sesión
- [x] Las sesiones con orden se nombran «web-01 · web-app-1 (shell)» o «web-01 · app.service (logs)», las arma `magi-movil` desde un tipo cerrado (T108) y no tienen snippet al conectar

## Snippets y resultados
- [x] Snippets (en MÁS) con los sincronizados: nombre, destinos («web → 3 hosts»), CRÍTICO, comando, descripción, timeout y último uso, con búsqueda; sin crear, editar ni borrar
- [x] Desde la ficha, SNIPPETS enseña los que tienen ese host entre sus destinos y prepara la ejecución solo en él
- [x] EJECUTAR: los hosts de los destinos con casillas (todos marcados), las variables con su valor por defecto, «parar al primer fallo» (el del snippet) y el timeout
- [x] Las variables se sustituyen en el núcleo, siempre escapadas; una vacía sin valor por defecto no deja seguir y una dentro de comillas o de un heredoc se rechaza
  - AC: Dado `systemctl restart {{servicio}}` con el valor `a b; rm`, cuando se prepara, entonces el comando queda `systemctl restart 'a b; rm'`
- [x] Deliberación si el snippet es crítico, si va a más de un host o si algún host tiene verificaciones activas; si no, CONTINUAR lanza
- [x] `magi-movil` ejecuta: semáforo de 8, conexión automática por host, salida de hasta 1 MiB por flujo, timeout del snippet y parar al primer fallo, con los estados de la F6
- [x] RESULTADOS (en MÁS y al lanzar): las ejecuciones en curso y terminadas, con hosts, recuentos, `⚑` si fue forzada y el estado de cada host con su código, duración y bytes
- [x] Salida de un host: stdout y stderr por separado, sin secuencias de escape, «… truncada» si pasó de 1 MiB, y COPIAR, marcado como sensible en el portapapeles
- [x] Cancelar (con confirmación si está en curso), repetir (vuelve a deliberar si hace falta) y limpiar las terminadas; las ejecuciones se olvidan 1 h después de acabar
- [x] REGISTRO `snippet_ejecutado` por host y, al acabar una ejecución deliberada, `deliberacion_aprobada` o `deliberacion_forzada` y `ejecucion_resultado`; `usado_veces` y `ultimo_uso_en` como campos propios, sin subir versión
- [x] Snippet al conectar: al abrir o reconectar una sesión a ese host, su comando se escribe al arrancar el shell, sin deliberar; si es crítico o tiene variables, se ignora con aviso

## Deliberación
- [x] Diálogo MAGI a pantalla completa: la acción, la tabla host × comprobación (`✓` `✕` `—` `◐`) con el dato de cada veredicto, la barra de consenso N / M y el estado APROBADO, BLOQUEADO o FORZADO
- [x] MELCHIOR-1 usa el último sondeo del móvil si es NOMINAL y de menos de 5 min (`deliberacion_salud_max_min`); si no, sondea dentro del plazo
- [x] BALTHASAR-2 lista `backup_ruta` por SFTP y aprueba si el fichero más reciente que casa con `backup_patron` tiene menos de `deliberacion_backup_horas`
  - AC: Dado un backup de hace 31 h, cuando se delibera, entonces BALTHASAR-2 rechaza con «último backup hace 31 h»
- [x] CASPER-3 no se puede hacer en el móvil: si está activa en un host, rechaza con «solo en el escritorio» y la deliberación queda BLOQUEADA
  - AC: Dado un host con tests activos, cuando se delibera desde el móvil, entonces CASPER-3 sale «✕ solo en el escritorio» y solo se puede forzar
- [x] Todas las comprobaciones en paralelo, con 2 s cada una y sin diálogos; sin ninguna activa, «sin comprobaciones configuradas», y se sigue pidiendo mantener pulsado
- [x] El botón que ejecuta lo deliberado solo actúa manteniéndolo pulsado 1 s (el `Ctrl+K` del móvil), con relleno de progreso y vibración; soltar antes no hace nada, y con TalkBack se toca dos veces y se mantiene
  - AC: Dada una deliberación aprobada, cuando se toca el botón una vez, entonces no se ejecuta nada y sale «mantén pulsado para ejecutar»
- [x] FORZAR en BLOQUEADO abre el motivo; con menos de 10 caracteres no se acepta y, con motivo, pasa a FORZADO y se ejecuta manteniendo pulsado
- [x] Al resolverse, `magi-movil` guarda la fila de DELIBERACIONES (resultado, bloqueada, motivo, comprobaciones y usuario «móvil (<equipo>)»), y cancelar anota `deliberacion_cancelada`

## Registro
- [x] REGISTRO (en MÁS) con las entradas del propio móvil: fecha, tipo, host y resultado, de la más nueva a la más vieja y por páginas
- [x] Filtros por host, por tipo (conexiones, huellas, túneles, sondeos, snippets, deliberaciones, servicios, claves y sincronización) y por texto
- [x] Detalle de una entrada; en las `deliberacion_*`, sus comprobaciones leídas de DELIBERACIONES, y una deliberación sin `ejecucion_resultado` dice «sin resultado: MAGI se cerró durante la ejecución»
- [x] La purga de las entradas de más de 90 días sigue como en la F11; sin exportar

## Ajustes y segundo plano
- [x] Ajustes gana FLOTA (refresco y umbrales de carga, memoria y disco), SERVICIOS (refresco, espera, motores de contenedores y minutos de sudo) y DELIBERACIÓN (horas del backup y minutos de salud), con los valores por defecto y los rangos del escritorio
- [x] Los ajustes viven en DataStore y llegan al núcleo con `aplicar_ajustes`; un valor fuera de rango se acota con aviso, y el sondeo recibe los motores de contenedores de los ajustes
- [x] El servicio en primer plano sigue mientras haya sesiones, túneles, acciones o ejecuciones («MAGI · 2 sesiones · 1 acción») y se para sin ninguna
- [x] Sondeo, refrescos de Servicios y logs solo con su pantalla a la vista, según el ciclo de vida de Android; nada de eso con la app en segundo plano
- [x] Si Android cerró MAGI con acciones o ejecuciones en curso, al volver lo dice (las órdenes pueden haber seguido en el host) y ofrece la guía de batería

## API: interfaz del núcleo (UniFFI)
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

## Calidad, pruebas y publicación
- [x] Pruebas de extremo a extremo en Rust (`magi-movil` con el servidor SSH de prueba y los binarios falsos): sondeo y gracia, listado, acción con sudo y deliberación, logs con ventanas, snippet en dos hosts y BALTHASAR-2 por SFTP (T95)
- [x] `servidor_pruebas` (antes `servidor_pruebas_tunel`) gana hosts con los binarios falsos y un directorio de backups y sigue escuchando en `127.0.0.1`; pruebas instrumentadas de Flota, Servicios y Snippets en el emulador
- [x] El job `humo` de `android.yml` arranca el servidor de pruebas antes del emulador, así que las pruebas de extremo a extremo del móvil también corren en CI (R106)
  - AC: Dada una PR que rompe el reenvío de un túnel en el móvil, cuando corre `android.yml`, entonces falla el job `humo`
- [x] Pruebas JVM de los ViewModels nuevos y del botón de mantener pulsado (soltar antes, completar, una sola vez y TalkBack)
- [x] El escritorio no cambia: `TZ=UTC cargo test --workspace --locked`, clippy, fmt, `cargo deny` y `shellcheck` en verde sin tocar ninguna aserción ni instantánea
- [x] App 0.2.0 en `android/version.properties` (`versionCode` 2009), con `android/CHANGELOG.md` y la sección «App Android» del README al día; el escritorio no cambia de versión
- [x] Revisión adversarial de cierre (T31, T54) sobre el núcleo movido, `magi-movil` y la app, con su informe en `docs/fase14/magi-fase14-revision-adversarial.md`

---

**Progreso Fase 14:** 98 / 98 funcionalidades

**Total MAGI (Fases 1-14):** 1151 / 1155 funcionalidades
