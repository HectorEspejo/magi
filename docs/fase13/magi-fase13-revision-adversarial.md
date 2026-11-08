# MAGI — Fase 13: revisión adversarial de cierre

**Fecha:** 8 de noviembre de 2026
**Rama:** `fase13-servicios_contenedores_logs`
**Revisor:** revisor (equipo de cierre de la Fase 13)
**Tarea compartida:** task-6 (T31, T54)
**Estado:** verificación final del 8-nov-2026. Todos los hallazgos de esta
revisión están corregidos por el equipo y verificados por el revisor (salvo B4,
que queda como desviación documentada y aprobada: ver §4.4 y §7).

---

## 1. Alcance revisado

Revisión adversarial del código **nuevo** de la Fase 13, con el árbol de trabajo
ya implementado (servidor y cliente por separado), buscando pérdida de datos,
fugas de recursos, esperas sin plazo, estado leído a destiempo, contraseñas de
sudo, entrada no fiable, contrapresión, apagado con trabajo en curso y huecos
visibles de la especificación §8:

- Servidor: `src/servidor/servicios.rs` (completo), enganches de
  `src/servidor/mod.rs` (despacho, `Bienvenida`, `TrabajoServidor`,
  `revisar_pool`, `revisar_inactividad`, `apagar_limpio`, `limpiar_cliente`,
  cola serial por cliente), `src/servidor/cliente_remoto.rs` (contador de
  pendientes), `src/protocolo.rs` (v8, rangos, códec) y la línea de
  `deliberacion_usada` en `src/servidor/ejecuciones.rs`.
- Cliente: `src/app/servicios.rs`, `src/ui/servicios.rs`,
  `src/app/dialogo_magi.rs`, `src/ui/deliberacion.rs` y su integración en
  `src/app.rs` (peticiones en vuelo, descarte de respuestas viejas, sudo en
  memoria, anillo de logs, refrescos y deliberación `PlanDeliberado::Servicio`).
- Núcleo: `crates/magi-nucleo/src/servicios/**` (tipos, órdenes, lectura de
  salidas, acciones, validadores y límites),
  `crates/magi-nucleo/scripts/servicios.sh`, `src/flota/sondeo.sh` (bloque de
  fallos y su parser en `src/flota/parser.rs`) y la sincronización de
  `COMPROBACIONES_SERVICIO` (`almacen/comprobaciones_servicio.rs`,
  `sincronizacion/{elementos,fusion,aplicacion,formato}.rs`, migración 9).
- Pruebas y fixtures de la fase en `tests/` y `crates/magi-nucleo/tests/`;
  pruebas manuales guiadas y no destructivas en `/tmp` (fuera del repositorio).
- No se ha modificado código del repositorio; el único fichero escrito por el
  revisor es este informe.

**Estado del árbol.** La revisión inicial se hizo sobre el árbol de las 14:30;
las correcciones se han revisado y verificado sobre el árbol final (servidor
14:41, cliente 14:36, script 14:47, parser 14:35, pruebas 14:48). Los números de
línea de la evidencia de las correcciones son del árbol final; los de la
evidencia de los defectos originales van marcados «árbol previo».

---

## 2. Método

1. **Lectura completa** del código nuevo del servidor, del script, del núcleo y
   de las zonas sensibles del cliente (estado y eventos), con el informe §3,
   §4, §5.2, §7, §8 y el checklist como referencia.
2. **Ejecutar primero lo que ya existe**: suites dirigidas de la fase para fijar
   el comportamiento esperado antes de intentar romperlo.
3. **Intentos de rotura**, con programas de prueba temporales fuera del
   repositorio (`/tmp/revserv`):
   - *Fuzzing de la lectura*: 20 000 entradas de basura (con `\n`, `\r`,
     `\x1b`, `\0`, `\x7f`, JSON a medias, `MAGI_*` malformados, números
     enormes/NaN/infinitos, nombres de 300+ caracteres) y casos enormes de
     64 KiB a 2 MiB inyectados en `lectura::{listado, detalle, consumo, accion,
     linea_diario, linea_contenedor, contenedor_json, privilegio, sudo, fallos}`
     con `std::panic::catch_unwind`: **0 pánicos**.
   - *Validación del script*: ejecutar `servicios.sh` de verdad con argumentos
     con metacaracteres (`; rm -rf /`, `$(id)`, comillas, saltos de línea en la
     comprobación propia, `--help`, proyectos con mayúsculas, aridad
     incorrecta): todo rechazado con `ERROR objetivo`/`ERROR comprobacion`/
     `ERROR argumentos` (códigos 64/77/66) y **sin ejecutar nada** en el host.
   - *Inyección en la orden remota*: comprobar que `orden::comando` y
     `orden::entrecomillar` entregan `a; touch /tmp/pwned`, `$(...)`, comillas
     simples y barras invertidas como **un solo argumento literal** (volcador
     `printf '%s\n' "$@"`): sin inyección. La comprobación propia llega a
     `sh -c` solo como texto ya validado y entrecomillado (diseño aprobado).
   - *Script con binarios falsos*: reproducir con los `falsos` de
     `crates/magi-nucleo/tests/fijos/servicios/falsos` los caminos de
     comprobación propia de proyecto y de privilegios (así se descubrieron H1 y
     H2, y se comprobaron después sus correcciones).
   - *Simulación de shell* de la parada del seguimiento de logs (así se
     descubrió M5).
4. **Revisión de concurrencia y ciclo de vida** leyendo el despacho por tareas
   del servidor, los mapas en memoria, el pool y el apagado, y contrastándolo
   con los patrones ya usados en F6/F8/F12 (claves por cliente, plazos de
   diálogo, liberación por identidad).
5. **Segunda pasada de verificación (8-nov, 14:50-15:00)**: lectura de cada
   corrección en el árbol final, comprobación de que no introduce riesgos
   nuevos y ejecución de las suites dirigidas con regresiones (ver §5).

---

## 3. Resumen ejecutivo

- Se encontraron **dos fallos críticos** en el árbol previo: (C1) colisión de
  `peticion_id` entre ventanas en los mapas del servidor, con pérdida de filas
  de REGISTRO y respuestas cruzadas; y (C2) la suscripción de log soltaba la
  `ConexionTomada` antes de abrir el canal, con lo que el pool desconectaba el
  seguimiento a los ~30 s.
- El equipo corrigió además **2 altos, 5 medios y 7 bajos**; el octavo bajo
  (B4) queda como **desviación documentada y aprobada**. El revisor ha
  verificado **las 16 correcciones** (código + pruebas de regresión; resultados
  en §5).
- **No queda ningún hallazgo abierto de gravedad crítica, alta ni media.** No
  se han detectado regresiones ni riesgos nuevos que bloqueen el cierre; solo
  quedan notas residuales menores y lo pendiente de revisar de §8.
- Lo que era un *hueco de especificación* (presupuesto de bytes del anillo,
  tamaño codificado del lote, contraseña de sudo en lecturas y línea de
  comprobación en la deliberación) está ahora implementado y probado; conviene
  reflejarlo en la próxima revisión del informe §8/§6.5/§3.5 (ver §7).

---

## 4. Hallazgos y estado final

> **Formato:** defecto original (evidencia en el árbol previo), corrección
> aplicada (evidencia en el árbol final), prueba de regresión y verificación del
> revisor. Todo lo listado está **corregido y verificado** salvo B4, que se
> queda como desviación documentada.

### 4.1 Críticos (corregidos y verificados)

#### C1 — Dos ventanas comparten `peticion_id` de servicios: colisión de mapas, pérdida de REGISTRO y respuestas cruzadas → **CORREGIDO**

- **Defecto (árbol previo):** `src/servidor/servicios.rs:116` (`acciones`) y
  `:118` (`logs`) indexaban solo por `peticion_id` y la inserción sobrescribía
  (`:1444`, `:1907`). Cada ventana arranca su contador en el mismo valor
  (`src/app.rs:2815`, `RANGO_SERVICIOS`, `src/protocolo.rs:78`). Con dos
  ventanas podía perderse una acción: `terminar_accion` leía la entrada de la
  otra ventana (REGISTRO con objetivo/deliberación equivocados y
  `AccionServicioHecha` al cliente equivocado) y la segunda terminaba sin fila
  de REGISTRO ni respuesta (T47), con `TrabajoServidor.acciones_servicio` a
  cero aunque quedara trabajo (T67). En logs, se pisaba la suscripción de la
  otra ventana.
- **Corrección:** clave `(cliente_id, peticion_id)` en todo el ciclo de vida:
  `type ClaveServicio = (u32, u64)` (`src/servidor/servicios.rs:123`), mapas
  en `:129` y `:131`, inserción de acción `:1482`, fase
  `actualizar_fase` `:1660-1684`, fin `terminar_accion` `:1710-1793`,
  logs `seguir_log`/`terminar_log`/`parar_log` (`:1910-2253`), desconexión
  `:321-347` y apagado `:286-317`.
- **Pruebas de regresión:** `tests/servicios_servidor.rs:1708`
  (`dos_ventanas_con_el_mismo_peticion_id_custodian_sus_acciones`: comprueba
  las dos respuestas, que no se cruzan y las dos filas de REGISTRO) y `:1812`
  (`dos_ventanas_con_el_mismo_peticion_id_tienen_sus_logs`).
- **Verificación:** lectura del árbol final y 23/23 de `servicios_servidor` en
  40,8 s (las dos pruebas pasan).

#### C2 — El seguimiento de logs soltaba la `ConexionTomada` antes de abrir el canal: el pool lo cortaba a los ~30 s → **CORREGIDO**

- **Defecto (árbol previo):** `src/servidor/servicios.rs:1948` tomaba la
  conexión y `:1966` hacía `tomada.soltar()` inmediatamente, antes de que el
  `tokio::spawn` de `lanzar_canal` abriese el canal. `Pool::liberar_si_es`
  dejaba `canales = 0`; `revisar_pool` (`src/servidor/mod.rs:594-601`)
  desconectaba a los 30 s (`GRACIA_SIN_CANALES`, `src/servidor/conexiones.rs:35`)
  y el seguimiento moría con `FinLog(conexion_caida)` (~35 s), especialmente con
  `[servicios] refresco_seg = 0` o tras un fallo del listado que deja de
  refrescar.
- **Corrección:** `Suscripcion.tomada: Option<ConexionTomada>`
  (`servicios.rs:105-117`), guardada antes de abrir el canal
  (`ejecutar_log`, `:2041-2097`) y soltada al retirar la entrada en todas las
  ramas (`terminar_log` `:2200-2228`, `parar_log` `:2231-2253`,
  `cliente_desconectado` `:321-347`, `apagar` `:286-317`); si la
  suscripción desaparece durante la apertura, la toma se suelta
  (`:2085-2097`).
- **Prueba de regresión:** `tests/servicios_servidor.rs:2037`
  (`un_seguimiento_sigue_vivo_pasada_la_gracia_del_pool`): pasados 40 s reales
  no llega `FinLog(conexion_caida)` y siguen llegando `LineasLog`.
- **Verificación:** lectura del árbol final y prueba en verde (40,8 s de suite).

### 4.2 Altos (corregidos y verificados)

#### H1 — Un usuario sin sudo pero con el grupo `docker` no podía gestionar contenedores ni proyectos → **CORREGIDO**

- **Defecto (árbol previo):** `src/app/servicios.rs:917-934` y `:2134-2149`
  exigían sudo para toda acción de contenedor/proyecto salvo Podman, y con
  privilegio `SinSudo` devolvían cero acciones o avisaban «usuario no puede usar
  sudo», aunque el listado probase que el motor Docker era accesible en directo
  (grupo `docker`).
- **Corrección:** la acción solo se considera privilegiada si el motor es Docker
  **y** el host usa sudo; con Docker disponible y `SinSudo` se actúa en directo,
  y con Podman nunca hay sudo. Sin motor disponible (`SinPermiso`) se mantiene el
  bloqueo y el aviso nuevo: `servicios_acciones_ofrecidas` `:947-958` y
  `iniciar_accion_servicio` `:2204-2247` (mensaje en `:2209-2215`).
- **Pruebas de regresión:** `tests/servicios_ui.rs:1927`
  (`sin_sudo_con_docker_directo_se_gestionan_contenedores_y_proyectos`: ofrece
  y ejecuta sin `sudo`, y el proyecto pasa por deliberación) y `:1988`
  (`sin_motor_disponible_siguen_sin_ofrecerse_las_acciones_de_contenedor`).
- **Verificación:** lectura y 46/46 de `servicios_ui`.

#### H2 — La comprobación propia de un proyecto se ejecutaba con sudo en vez de con los privilegios de la acción (Podman + `sudo_con_clave` = rechazo sistemático) → **CORREGIDO**

- **Defecto (árbol previo):** `crates/magi-nucleo/scripts/servicios.sh:675-701`
  ejecutaba la comprobación propia con `con_priv 20 sh -c "$orden_propia"`
  también para proyectos. Con Podman sin root y privilegio `sudo_con_clave`
  (sin contraseña en la petición, porque Podman no usa sudo), la comprobación
  caía a `sudo -n` y devolvía `rechazada_configuracion` sin ejecutar la acción;
  con Docker directo se ejecutaba como root.
- **Corrección:** en `comprobar_antes` (`servicios.sh:661-706`), la comprobación
  propia de `proyecto` usa la vía del motor: con `MODO = privilegiado`,
  `con_priv`; en directo, `timeout 20 sh -c "$orden_propia"` (`:681-690`).
  `unidad` sigue con `con_priv` (sus acciones sí son privilegiadas).
- **Pruebas de regresión:** `crates/magi-nucleo/tests/servicios_script.rs:863`
  (Podman + `sudo_con_clave` sin clave: `MAGI_COMPROBACION=ok` y
  `MAGI_RESULTADO=hecho`, sin `sudo` en la comprobación) y `:896`
  (comprobación propia que falla: rechaza y no reinicia).
- **Verificación:** lectura y 32/32 de `servicios_script`.

### 4.3 Medios (corregidos y verificados)

#### M1 — Mensajes de servicios del mismo cliente despachados en tareas independientes: suscripciones de log huérfanas → **CORREGIDO**

- **Defecto (árbol previo):** cada mensaje de servicios iba a su propio
  `tokio::spawn` (`src/servidor/mod.rs:1292-1306`), sin orden por cliente.
  `SeguirLog` podía insertar su suscripción después de que `cliente_desconectado`
  ya hubiera pasado, o `PararLog` podía adelantarse a la inserción: quedaba una
  suscripción sin dueño, con `journalctl -f` en el host y la conexión del pool
  retenida, rompiendo el AC «al cerrar el panel ya no corre el `journalctl -f`».
- **Corrección:** una **cola FIFO por ventana** con worker propio:
  `EstadoServidor.servicios_tx` (`src/servidor/mod.rs:150-154`),
  `encolar_servicios` (`:829-862`, el worker comprueba que el cliente sigue
  conectado antes de cada mensaje), despacho por la cola (`:1337-1349`), cola
  retirada en `limpiar_cliente` (`:1425-1428`) y vaciada al apagar
  (`:679-680`). Además, `seguir_log` comprueba el cliente bajo el mismo bloqueo
  que inserta (`src/servidor/servicios.rs:2004-2010`) y la `ConexionTomada` se
  guarda antes de abrir el canal (`:2071-2097`), cerrando también el caso de la
  caída durante la apertura.
- **Pruebas de regresión:** `tests/servicios_servidor.rs:1923`
  (`un_parar_log_inmediato_no_deja_el_seguimiento_huerfano`: exige `FinLog` y,
  si el `journalctl` arrancó, que su falso registre la parada) y `:1998`
  (`un_cliente_que_cae_con_seguir_log_en_vuelo_no_deja_nada_corriendo`).
- **Verificación:** lectura del worker y de la sección crítica; 23/23 de
  `servicios_servidor`. No se observa regresión: la cola es por cliente, sin
  límite explícito (mismo orden práctico que antes; un cliente malicioso solo
  puede encolarse a sí mismo).
- **Riesgo residual (menor):** la cola de un cliente no tiene tope de mensajes.
  El cliente real envía unos pocos por acción de teclado; no se considera un
  problema para el cierre, pero podría acotarse si se quiere endurecer.

#### M2 — El plazo de 10 s de `tomar_conexion` incluía los diálogos al usuario (huella, frase, contraseña), que esperan 5 min → **CORREGIDO**

- **Defecto (árbol previo):** `servicios.rs:42` y `:445-472` envolvían toda la
  `conexion_para_canal` (`PoliticaConexion::Interactiva`) en 10 s, incluyendo
  los diálogos, que por diseño esperan 5 min (`TIMEOUT_DECISION`,
  `src/servidor/sesiones.rs:31`). La primera conexión de la vista a un host
  fuera del pool podía fallar si el usuario tardaba >10 s.
- **Corrección:** `PLAZO_CONEXION` eliminado; `tomar_conexion` ya no tiene
  plazo propio (`servicios.rs:458-478`) y los 10 s / `espera_seg + 30` se
  aplican solo al canal y a la lectura, contados desde que la conexión está
  tomada (`ejecutar_suborden` `:709-768`: `restante = plazo` tras
  `tomar_conexion`; `ejecutar_accion` `:1522-1602`). Los diálogos mantienen
  su plazo de 5 min y la fase de red el suyo.
- **Verificación:** lectura del flujo completo. **Sin prueba de regresión
  específica** (los diálogos de conexión no se simulan en el arnés actual); es
  una corrección de plazo que no altera el resto del camino.

#### M3 — El anillo de logs del cliente solo acotaba líneas, no bytes (hasta 160 MiB por defecto) → **CORREGIDO**

- **Defecto (árbol previo):** `src/app/servicios.rs:1919-1946` recortaba solo
  por número de líneas; con 10 000 líneas de 16 KiB el anillo podía llegar a
  160 MiB (1,6 GB con `lineas_log = 100000`), con el host controlando el
  contenido.
- **Corrección:** presupuesto de bytes `PRESUPUESTO_LOG_BYTES = 8 MiB`
  (`src/app/servicios.rs:38`), contabilidad `logs.bytes` y
  `omitidas_locales` (`:316-322`, `:345-349`) y descarte por el frente por
  bytes **y** por líneas (`anadir_lineas_log` `:1972-2031`), marcando las
  omitidas en la primera línea que se guarda.
- **Prueba de regresión:** `tests/servicios_ui.rs:2012`
  (`el_anillo_de_logs_tiene_presupuesto_de_bytes`: 600 líneas de 16 KiB no
  superan 8 MiB, hay descartes locales y se pinta «líneas omitidas»).
- **Verificación:** lectura y 46/46 de `servicios_ui`; `redimensionado`
  303/303.

#### M4 — El lote `LineasLog` se acotaba por texto crudo (4 MiB) y podía superar `LINEA_MAXIMA` tras el escapado JSON → **CORREGIDO**

- **Defecto (árbol previo):** `ColaSuscripcion` acotaba por `texto.len()`
  (4 MiB) y `mandar_lote` no miraba el tamaño serializado; con líneas ricas en
  comillas (logs JSON) el mensaje podía pasar de 4 MiB y el decodificador del
  cliente lo descartaba con «mensaje corrupto del servidor» (pérdida del lote).
- **Corrección:** `TOPE_LOTE_CODIFICADO = 2 MiB` (`servicios.rs:78-81`);
  `ColaSuscripcion::lote(presupuesto)` corta por tamaño codificado y siempre
  saca al menos una línea (`:1860-1883`), con `tamano_codificado` calculado
  con `serde_json` (`:1890-1897`); `mandar_lote` usa el presupuesto
  (`:2158-2178`).
- **Prueba de regresión:** `src/servidor/servicios.rs:2461`
  (`el_lote_no_pasa_de_linea_maxima_codificado`: 500 líneas de 8 KiB de
  comillas, el lote codificado queda por debajo de `LINEA_MAXIMA`).
- **Verificación:** lectura y 23/23 de `servicios_servidor`.

#### M5 — Los seguimientos de logs con sudo o de contenedores mataban la subshell, no el proceso real → **CORREGIDO**

- **Defecto (árbol previo):** `logs_unidad` lanzaba
  `con_priv_log journalctl ... &` (una función: `$!` era la subshell, no
  `sudo/journalctl`) y `logs_contenedor` metía cada flujo en
  `( … | transformar_log … ) &`. El `kill -TERM` solo mataba la subshell; un
  `journalctl -f` o un `docker logs -f` callado podía sobrevivir al cierre del
  panel (verificado con una simulación `sh`: el proceso nieto sobrevivía).
- **Corrección:** `logs_unidad` (`servicios.sh:1050-1107`) lanza directamente
  `journalctl` o `sudo` según el privilegio (sin función intermedia), de modo
  que `$!` es el proceso real, y el espía hace TERM y, si hiciera falta,
  KILL (`:1094-1104`). `logs_contenedor` (`:1109-1200`) se reescribió con
  tuberías con nombre (`mkfifo`, `:1118-1124`), un único proceso
  `docker|podman logs -f` que escribe en las dos (productor real), lectores de
  shell que emiten `MAGI_LOG` y parada TERM+KILL al productor
  (`:1180-1198`), limpiando el directorio temporal.
- **Pruebas de regresión:** `crates/magi-nucleo/tests/servicios_script.rs:936`
  (el `journalctl` falso registra que recibió TERM), `:953` (un
  `journalctl` que ignora TERM muere con KILL; se comprueba con `kill -0`) y
  `:975` (el `docker logs` falso registra su parada).
- **Verificación:** lectura y 32/32 de `servicios_script`.
- **Riesgo residual (menor):** si la sesión SSH muriera de golpe antes de la
  limpieza, podría quedar un directorio temporal vacío en `/tmp` del host; no
  afecta al seguimiento ni deja procesos.

### 4.4 Bajos (corregidos y verificados, salvo B4)

#### B1 — La caducidad de la contraseña de sudo se renovaba con cada refresco → **CORREGIDO**

- **Corrección:** dos vías separadas en `src/app/servicios.rs`:
  `servicios_sudo_lectura` (`:1191-1207`) devuelve la contraseña sin renovar
  `ultimo_uso`; `servicios_sudo_accion` (`:1209-1228`) sí lo renueva y es la
  única que usan las acciones (`:2242`). Las lecturas usan la primera
  (`:1355`) y las acciones la segunda (`:1362`).
- **Prueba de regresión:** `tests/servicios_ui.rs:2060`
  (`la_contrasena_de_sudo_no_caduca_con_los_refrescos`: a los 4 min un refresco
  la usa sin renovar; a los 6 min sin uso ya no se adjunta y se olvida).
- **Verificación:** lectura y 46/46 de `servicios_ui`.

#### B2 — La salida de una acción se cortaba a 1 MiB sin avisar → **CORREGIDO**

- **Corrección:** si `SalidaCanal.truncada` y no hay `MAGI_RESULTADO`,
  `resultado_de_salida` devuelve un mensaje explícito («la salida del host pasó
  de 1 MiB y no se pudo leer el resultado»): `src/servidor/servicios.rs:1633-1656`.
- **Prueba de regresión:** `servicios.rs:2434`
  (`una_salida_truncada_sin_resultado_lo_dice`).
- **Verificación:** lectura y suite del servidor en verde.

#### B3 — `inspect` y `ps` de los motores sin `timeout` → **CORREGIDO**

- **Corrección:** `timeout 10` en `docker/podman ps -a -q`
  (`servicios.sh:153-176`) y en los `inspect` directos y con sudo
  (`:214-242`).
- **Verificación:** lectura del script y 32/32 de `servicios_script`.

#### B4 — El sondeo ampliado ignora `[servicios] contenedores` (orden y desactivación) → **DESVIACIÓN DOCUMENTADA Y ACEPTADA**

- **Situación:** `src/flota/sondeo.sh:30` autodetecta `docker` y luego
  `podman`, sin recibir los motores configurados; con
  `[servicios] contenedores = []` el sondeo sigue mirando si hay motor y Flota
  puede marcar `⚠N` por contenedores que la vista no enseña.
- **Decisión:** se queda así, documentado en el informe de implementación
  (`docs/fase13/magi-fase13-implementacion.md:125-130`). El revisor lo
  considera **aceptable para el cierre**: el sondeo es el camino automático de la
  F2 y `PeticionSondeo` no lleva hoy el dato de motores (cambiarlo toca
  constructores fuera del alcance de la F13); las consecuencias se limitan a las
  marcas de Flota y «Toda la flota», que por D240 no cambian el estado del host ni
  afectan a acciones o datos. Recomendación: reflejarlo en §7.8 cuando el
  analista actualice la especificación, o abrir seguimiento en una fase futura.

#### B5 — Nombres de fallo del sondeo con caracteres de control → **CORREGIDO**

- **Corrección:** `nombre_de_fallo` filtra C0, C1, DEL, marcas bidi y
  zero-width (`src/flota/parser.rs:142-166`).
- **Prueba de regresión:** `src/flota/parser.rs:269`
  (`los_nombres_de_fallo_no_llevan_caracteres_de_control`).
- **Verificación:** lectura; el binario de la fase compila y las pruebas del
  parser están en verde (la suite completa la confirmó el Lead).

#### B6 — La deliberación no enseñaba la línea «Antes de …: comprobación» de §6.5 → **CORREGIDO**

- **Corrección:** `PlanServicio` gana `comprobacion: Option<String>`
  (`src/app/dialogo_magi.rs:56-75`), `abrir_deliberacion_servicio` la calcula
  y la mete en el plan (`:648-675`) y `src/ui/deliberacion.rs:93-102` pinta
  «Antes de reiniciar: nginx -t» reservando su fila en el alto del diálogo.
- **Pruebas de regresión:** `tests/servicios_ui.rs:2192`
  (`la_deliberacion_ensena_la_comprobacion_de_configuracion`) y las tres
  instantáneas nuevas
  `tests/snapshots/servicios_deliberacion_{40x12,80x24,200x60}.snap`.
- **Verificación:** lectura y `redimensionado` 303/303.

#### B7 — Ventana de reutilización de deliberación antes de la Barrera → **CORREGIDO**

- **Corrección:** `Servicios.deliberaciones_en_vuelo`
  (`src/servidor/servicios.rs:132-135`), consultado por
  `deliberacion_usada` (`:183-194`); `terminar_accion` marca la deliberación
  antes de quitar la acción del mapa y la retira solo después de
  `confirmar_escritura` (`:1724-1777`, con la rama de la acción ya cancelada).
- **Prueba de regresión:** `servicios.rs:2492`
  (`una_deliberacion_en_vuelo_cuenta_como_usada`).
- **Verificación:** lectura y suite del servidor en verde.
- **Riesgo residual (menor):** si la Barrera expira (escritor lento) la marca se
  retira igualmente; la fila puede llegar después. Es la misma semántica
  «mejor esfuerzo» que ya tenía el resto del servidor y no empeora nada.

#### B8 — `emitir_lineas` perdía la última línea sin salto y podía partir UTF-8 → **CORREGIDO**

- **Corrección:** `emitir_lineas` recorta por líneas completas con `awk`
  (límite 16 KiB) y no parte caracteres ni pierde la última línea
  (`servicios.sh:284-300`).
- **Prueba de regresión:** `crates/magi-nucleo/tests/servicios_script.rs:994`
  (`las_lineas_de_una_accion_no_parten_caracteres_ni_pierden_la_ultima`).
- **Verificación:** lectura y 32/32 de `servicios_script`.

---

## 5. Verificación ejecutada por el revisor (árbol final)

Comandos y resultados (TZ=UTC, `--locked`, sobre el árbol con todas las
correcciones):

```sh
cargo test -p magi-ssh --test servicios_servidor --test servicios_ui \
  --test servicios_e2e --test redimensionado --locked
# redimensionado:      303/303  (20,40 s)
# servicios_e2e:         1/1    (3,14 s)
# servicios_servidor:   23/23   (40,84 s; incluye las regresiones de C1, C2 y M1)
# servicios_ui:         46/46   (0,99 s; incluye H1, M3, B1 y B6)

cargo test -p magi-nucleo --test servicios_script --test servicios_lectura \
  --test sincronizacion_comprobaciones --locked
# servicios_lectura:    31/31
# servicios_script:     32/32   (incluye H2, M5, B3 y B8)
# sincronizacion_comprobaciones: 17/17
```

**Total dirigido: 453 pruebas en verde, 0 fallos.** El Lead confirma además la
suite completa del workspace, clippy `-D warnings`, `cargo fmt --check` y
`cargo deny` en verde (con la excepción documentada de RUSTSEC-2026-0441).

Además de ejecutar las pruebas, el revisor ha **leído cada corrección** en el
árbol final y comprobado que:
- C1 y M1 no dejan huecos nuevos: la cola por cliente se retira al desconectar
  el cliente, se vacía al apagar y los workers comprueban la conexión; la
  inserción de la suscripción comprueba el cliente bajo el mismo bloqueo.
- C2 y M1 conviven: la `ConexionTomada` se guarda antes de abrir el canal, se
  suelta si la suscripción desapareció y no se retiene en ninguna rama de error.
- M2 no elimina plazos de red: los 10 s / `espera_seg + 30` siguen aplicándose
  al canal y a la salida.
- M3 y M4 acotan por bytes sin romper el conteo de líneas ni la marca de
  omitidas; los lotes siempre llevan al menos una línea.
- M5 termina el proceso real (o lo remata con KILL) y limpia su temporal.
- H1/H2 respetan la regla «sudo orden a orden» y «con Podman nunca sudo», y los
  casos SinPermiso/sin motor siguen bloqueados.
- B1, B2, B5, B6, B7 y B8 no alteran el resto de contratos (sudo solo en
  memoria, sin secretos en log/REGISTRO ni en la línea de órdenes).

**Riesgos residuales introducidos por las correcciones (ninguno bloqueante,
anotados en cada hallazgo):** cola de servicios por cliente sin tope de
mensajes (M1); directorio temporal de `mkfifo` que podría quedar si la sesión
SSH muere de golpe (M5); la marca de deliberación en vuelo se retira aunque la
Barrera expire (B7); el aviso de motor sin permiso sigue mencionando «!» aunque
el usuario no pueda sudo (H1, cosmético y fuera del caso corregido).

---

## 6. Comprobaciones que salieron bien (no requieren cambios)

- **Contraseñas de sudo:** viajan por la primera línea de la entrada estándar,
  nunca por la línea de órdenes; `printf` es interno del shell; `Secreto` es
  `Zeroizing` y su `Debug` es `***`; las pruebas del script comprueban que la
  clave no aparece en la salida ni en el registro de `$*` de un sudo falso
  (`crates/magi-nucleo/tests/servicios_script.rs:381-383`). No hay logs con
  diálogos ni con mensajes que contengan secretos.
- **Validación en Rust y en el script:** nombres de unidad/timer/contenedor/
  proyecto, motor, espera, comprobación, motores de `listar` y la orden propia
  (una línea, ≤1 024, sin NUL ni saltos). Probado con metacaracteres y aridad
  incorrecta: rechazo con 64/77/66 y sin ejecutar nada (ver §2).
- **Entrecomillado de la orden remota:** `orden::comando`/`entrecomillar`
  entregan los argumentos, incluida la comprobación propia con `;`,
  `$(...)`, comillas y barras invertidas, como argumentos literales; no hay
  inyección en el shell exterior. La comprobación propia llega a `sh -c` solo
  como texto validado y entrecomillado (diseño aprobado).
- **Lectura de salidas del host:** 20 000 entradas de basura + casos de hasta
  2 MiB sin un solo pánico (fuzzing en `/tmp`); los topes de §8 se aplican en
  `lectura.rs` (2 MiB listado, 2 000 unidades, 500 contenedores, 100 proyectos,
  256 KiB de texto, 16 KiB por línea, 500 dependencias, nombres sin controles).
- **Pool y canales:** `ConexionTomada` se suelta por identidad en todas las
  ramas de listado/consumo/detalle/sudo/acción y, desde C2, también en logs;
  `lanzar_canal` cierra el canal y termina su tarea en todos los caminos (fin,
  plazo, cancelación, receptor caído).
- **Apagado:** `apagar_limpio` cancela acciones y logs antes de vaciar el pool
  y de cerrar clientes; `servicios_tx` se vacía y los workers paran;
  `TrabajoServidor.acciones_servicio` cuenta las acciones en curso
  (`src/servidor/mod.rs:182`) y `revisar_inactividad` las considera trabajo
  (`:538`); `Bienvenida` las manda y el cliente las aplica.
- **Deliberación:** el servidor recomprueba que haga falta, valida que sea de esa
  acción/host y no usada (con la marca en vuelo de B7), el cliente crea la fila
  antes de enviar la acción y MELCHIOR-1 excluye el objetivo propio y, en un
  timer, su servicio.
- **Refrescos:** sin solapes por host, sin diálogos y sin refrescos al salir de
  la vista; respuestas de peticiones viejas descartadas por
  `peticiones_servicio` y limpiadas al caer el servidor.
- **Sincronización:** los 17 casos de `sincronizacion_comprobaciones` cubren
  formatos 1/2/3, `formato_nuevo`, lápida, huérfana (host ausente o no ssh) y
  colisión de objetivo; la fila se valida antes de aplicarla.

---

## 7. Huecos de especificación y desviaciones (para la próxima revisión del informe)

1. **§8 no fija presupuesto de bytes para el anillo de logs del cliente.** La
   implementación lo resuelve con 8 MiB (`PRESUPUESTO_LOG_BYTES`) además del
   tope de líneas; conviene reflejarlo en §8/§3.5 para que no dependa del valor
   del código.
2. **§8 no fija el tamaño máximo de un mensaje del protocolo.** La
   implementación acota el lote codificado a 2 MiB (margen bajo los 4 MiB del
   códec); conviene fijar el criterio en §8.
3. **§6.5** ya está implementado: la deliberación enseña «Antes de reiniciar:
   …». Actualizar el mockup si hiciera falta.
4. **§3.5 y el AC de `sudo_memoria_min`:** la interpretación implementada y
   probada es «las lecturas usan la contraseña pero no renuevan su caducidad;
   solo las acciones la renuevan». Conviene dejarlo escrito en el informe.
5. **B4 (desviación aceptada):** el sondeo ampliado autodetecta los motores y no
   respeta `[servicios] contenedores`; documentado en el informe de
   implementación. Reflejarlo en §7.8 o abrir seguimiento.

---

## 8. Pendiente de revisar

- **`src/ui/servicios.rs` completo** (2 100 líneas): se revisaron los puntos de
  riesgo (pintado, saneado de texto no fiable, mínimos y modo estrecho,
  diálogos), y las instantáneas/redimensionado están en verde, pero no se hizo
  una lectura línea a línea de todo el pintado ni de todas las instantáneas
  nuevas.
- **Integración visual en Flota/Hosts/ficha** (marcas `⚠N`, bloque FALLOS,
  bloque de comprobaciones): confiada a las pruebas de UI e instantáneas; no
  revisada a mano contra el mockup.
- **`android/` y `docs/formato-sincronizacion.md`** (lectura del formato 3):
  fuera del foco pedido; los tests de sincronización del núcleo están en verde.
- **Prueba de extremo a extremo con hosts reales** (§11 del informe) y guion de
  validación manual: no ejecutados (requieren hosts reales con systemd, Docker y
  Nginx); las pruebas usan el servidor SSH de prueba y binarios falsos.
- **Revisión de fases anteriores** (F2, F3, F6, F8, F10, F12) más allá de lo
  necesario para entender los contratos: no incluida en el alcance.
- **Rendimiento real** (2 000 unidades pintadas, 2 000 líneas de log/s): no
  medido; solo revisado el diseño (pintado O(n) del panel, límites de §8).

---

## 9. Conclusión

Con el árbol final, **no queda ningún hallazgo abierto de gravedad crítica, alta
ni media** en el código nuevo de la Fase 13. Los dos críticos y los dos altos
están corregidos con pruebas de regresión específicas; los cinco medios y los
siete bajos también están corregidos y probados, y B4 queda como desviación
documentada y aceptada (no cambia estado del host ni datos, solo marcas de
Flota). Las 453 pruebas dirigidas de la fase pasan y el Lead confirma la suite
completa del workspace en verde.

Para el cierre solo quedan los trámites ya conocidos: validación manual con
hosts reales del §11, reflejar en la especificación los cuatro criterios de §7
(anillo de 8 MiB, lote codificado de 2 MiB, semántica de la caducidad de sudo y
la limitación del sondeo) y la entrega del informe de implementación al
analista. La revisión adversarial de cierre puede darse por **completada**.
