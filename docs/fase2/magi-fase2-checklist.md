# MAGI - Checklist Fase 2: Flota, Identidades y Registro

## Almacén y migración
- [x] Migración 2 por `PRAGMA user_version`: columna `HOSTS.servicios`, tablas `SONDEOS`, `IDENTIDADES` y `REGISTRO`
- [x] Tabla `SONDEOS` (id, host_id, fecha, resultado, error, nucleos, carga_1m, carga_5m, carga_15m, mem_total_kb, mem_disponible_kb, disco_total_kb, disco_usado_kb, red_rx_bytes, red_tx_bytes, uptime_seg, servicios_json, duracion_ms) con borrado en cascada por host
- [x] Tabla `IDENTIDADES` (id, alias UQ, tipo, huella UQ, origen, ruta, comentario, anadida_en, ultimo_uso_en, revocada_en)
- [x] Tabla `REGISTRO` (id, fecha, tipo, host_id, identidad_id, detalle, resultado) con `ON DELETE SET NULL` e índices por fecha y tipo
- [x] Se conservan los 20 últimos sondeos por host; al insertar se purgan los anteriores
- [x] Sin `CHECK` sobre enumeraciones; `resultado`, `origen` y `tipo` se validan en código
- [x] Tests del almacén: migración 1→2 sobre una BD de Fase 1 con datos, purga de sondeos, cascadas y `SET NULL`

## Registro (infraestructura)
- [x] `registro::anotar(tipo, host_id, identidad_id, detalle, resultado)` como única función de escritura en `REGISTRO`
- [x] Apertura y fallo de conexión (Fase 1) anotan `conexion_abierta` / `conexion_fallida` con el motivo
- [x] Aceptación y sustitución de huella anotan `huella_aceptada` / `huella_sustituida` con las huellas anterior y nueva
- [x] Importación y exportación de ssh_config anotan `importacion` (recuentos) y `exportacion` (número de hosts)
- [x] Las entradas de `REGISTRO` nunca contienen frases, claves ni salida de sesiones

## Vista Registro (F7)
- [x] `F7` abre la vista Registro con tabla fecha · tipo · host · resultado, ordenada de más reciente a más antigua y paginada de 200 en 200
- [x] Cabecera con número total de entradas y el filtro de tipo activo
- [x] Navegación con `↑` `↓` / `j` `k` y panel inferior con el detalle de la entrada seleccionada
- [x] `↵` abre un diálogo con el detalle completo
- [x] `/` filtra por tipo, nombre de host o texto del detalle
- [x] `t` cicla el filtro rápido por tipo: todos → conexiones → huellas → claves → importación → sondeos
- [x] `p` purga las entradas de más de 90 días tras confirmación con recuento
  - AC: Dado un registro con 30 entradas antiguas y 10 recientes, cuando se pulsa `p` y se confirma, entonces quedan 10 y el diálogo previo indicaba «30»
- [x] `x` exporta a CSV o JSON mediante diálogo de formato y ruta; el fichero se escribe vía `ficheros.rs` con permisos 600
- [x] CSV con cabecera `fecha,tipo,host,identidad,resultado,detalle` (host e identidad por nombre); JSON como lista de objetos con los mismos campos
- [x] `magi registro exportar <ruta> [--json] [--desde AAAA-MM-DD]` exporta sin TUI

## Ficha de host
- [x] Campo «Servicios» en el bloque «Al conectar»: área de texto con unidades systemd, una por línea, guardado en `HOSTS.servicios`
- [x] Validación de cada unidad contra `[A-Za-z0-9@._-]+`; se normaliza quitando `.service` final y líneas vacías
- [x] Importación y exportación de ssh_config ignoran `servicios` (no existe en ssh_config) y lo conservan al sobrescribir

## Sondeo
- [x] Script `sondeo.sh` embebido con `include_str!` que emite marcadores `MAGI_NUCLEOS`, `MAGI_LOADAVG`, `MAGI_MEM_*`, `MAGI_DISCO`, `MAGI_RED`, `MAGI_UPTIME` y `MAGI_SVC=<unidad>=<estado>`
- [x] Las unidades de `servicios` se pasan al script como argumentos entrecomillados, nunca interpolados en el texto del script
- [x] Ejecución por canal `exec` de russh; si hay sesión viva con el host se abre el canal sobre el mismo `Handle`, si no se usa una conexión efímera que se cierra al terminar
- [x] Orquestación concurrente con semáforo de 8 tareas y timeout de 5 s por host (DNS, TCP, autenticación y comando incluidos)
  - AC: Dado 20 hosts de los que 5 no responden, cuando se sondean todos, entonces los 15 restantes muestran resultado antes de 10 s y los 5 quedan CAÍDA por timeout
- [x] El sondeo nunca abre diálogos: huella desconocida o cambiada y clave que requiere frase se reportan como `error` con instrucción («conéctate una vez con ↵»)
- [x] Parser de marcadores a `Sondeo`; sin `MAGI_LOADAVG` o `MAGI_UPTIME` el resultado es `sin_metricas`
- [x] Cálculo de tasa de red con el sondeo anterior si tiene menos de 1 h y los contadores no retrocedieron; en caso contrario «—»
- [x] Estado derivado: FRÍA (sin sondeo), CAÍDA (`error`), ALCANZABLE (`sin_metricas`), CARGA (umbral superado o servicio no `active`), NOMINAL
- [x] Umbrales en `config.toml` `[flota.umbrales]`: `carga_por_nucleo` (1.0), `memoria_pct` (90), `disco_pct` (90)
- [x] Un servicio listado que no existe en el host aparece como `unknown`, cuenta como no activo y muestra «unidad no encontrada»
- [x] Un sondeo fallido anota `sondeo_fallido` en `REGISTRO` con el motivo
- [x] Sondeo de un host cuyo salto falla marca CAÍDA al host con motivo «salto <nombre>: …» y también al salto si está en la lista
- [x] Un sondeo nunca modifica `HOSTS.ultimo_estado` ni `ultima_conexion_en`
- [x] `magi sondear [host…]` sondea sin TUI e imprime una tabla estado · carga · memoria · disco · servicios
- [x] Tests del parser con salidas reales de Linux, salida sin `/proc` (sin_metricas) y servicio `unknown`

## Vista Flota (F1)
- [x] `F1` abre Flota; MAGI arranca en Flota y con inventario vacío muestra «Sin hosts: pulsa n en Hosts o I para importar»
- [x] Panel izquierdo con glifo + palabra de estado por host (`●` NOMINAL, `◐` CARGA, `○` FRÍA, `✕` CAÍDA, `●` ALCANZ.) y resumen «N hosts · M nominal · sondeo hace X s»
- [x] Panel derecho del host seleccionado: barras de carga (x.x/núcleos), memoria %, disco %, red ↓↑, uptime en «d h», y pie «sondeado hace X s en N ms»
- [x] Sección SERVICIOS con glifo por unidad (`●` active, `✕` otro estado); oculta si `servicios` está vacío
- [x] Barras con `█`/`░` y `#`/`.` en modo ASCII; barra o servicio culpable en ámbar cuando el estado es CARGA
- [x] Mientras un host se sondea su glifo es `◐` y la palabra «…»; la cabecera muestra «sondeando i/n»
- [x] Cada resultado repinta su fila al llegar, sin esperar al resto
- [x] `r` sondea el host seleccionado y `R` todos los visibles; entrar en Flota sondea los hosts sin sondeo reciente (> 60 s)
- [x] `↵` conecta (flujo de la Fase 1) y `e` abre la ficha
- [x] `/` filtra con el mismo filtro que Hosts
- [x] Auto-refresco cada `[flota] auto_refresco_seg` (0 = desactivado, mínimo 15), conmutable con `a` durante la ejecución y pausado mientras la vista activa es Sesión
- [x] La barra de estado de Sesión añade «carga X.X» si existe un sondeo del host de menos de 10 min

## Identidades (tabla y sincronización)
- [x] Cada escaneo (arranque, `s`, abrir el desplegable de identidad) hace upsert en `IDENTIDADES` por huella: alta con `anadida_en`, actualización de tipo, comentario, ruta y origen
- [x] Claves `ed25519-sk` / `ecdsa-sk` se registran con origen `token`
- [x] Una clave revocada que reaparece en el escaneo sigue revocada; una que desaparece no se borra y muestra «no encontrada»
- [x] `USADA EN` cuenta los hosts cuyo `identidad_ref` coincide por huella (`agente:`) o por ruta (`fichero:`)
- [x] `ultimo_uso_en` se actualiza desde `conexion/` al autenticar con éxito, buscando por huella de la clave usada
- [x] El desplegable de identidad de la ficha no ofrece identidades revocadas

## Vista Identidades (F5)
- [x] `F5` abre Identidades: tabla alias · tipo · usada en · origen y panel de detalle (huella, vista/uso, fichero, hosts)
- [x] Navegación con `↑` `↓` / `j` `k`; `v` muestra u oculta las revocadas
- [x] `n` abre el diálogo de generación: fichero (por defecto `id_ed25519_<alias>`), tipo ed25519 | rsa 4096, comentario, frase repetida (opcional, con aviso si queda vacía), casillas «añadir al agente» y «copiar la pública»
- [x] Generación con `ssh-key` y `OsRng`, cifrado OpenSSH con la frase, escritura de `~/.ssh/<nombre>` (600) y `<nombre>.pub` (644) vía `ficheros.rs`
  - AC: Dado un nombre que ya existe en `~/.ssh`, cuando se pulsa `Ctrl+S`, entonces se rechaza con «ya existe» y no se escribe nada
- [x] Alta en el agente con `AgentClient::add_identity` si la casilla está marcada; si el agente no está disponible, aviso y la clave queda solo en fichero
- [x] La clave descifrada y la frase se destruyen con `zeroize` tras generar y añadir
- [x] Generar anota `clave_generada` en `REGISTRO` y crea la fila en `IDENTIDADES`
- [x] `i` importa una ruta: lee la huella de `<ruta>.pub` o, si no existe, de la privada (pidiendo frase si está cifrada) y anota `clave_importada`
- [x] `c` copia la clave pública al portapapeles con `wl-copy` o `pbcopy`; si no hay binario, diálogo con la línea para copiar a mano
- [x] `e` edita el alias (único, no vacío)
- [x] `x` revoca tras confirmación con el número de hosts que pasan a `auto`; pone `identidad_ref = NULL` en ellos, marca `revocada_en`, anota `referencia_revocada` y regenera `magi_config` si `exportar_al_guardar`
  - AC: Dado 3 hosts que usan la identidad, cuando se revoca, entonces los 3 quedan en «auto» y el mensaje recuerda que la clave sigue en `~/.ssh` y en el agente
- [x] `x` sobre una revocada la reactiva (`revocada_en = NULL`)
- [x] Revocar nunca borra ficheros de `~/.ssh` ni quita claves del agente
- [x] Las claves de origen `token` no ofrecen `i` y solo ofrecen `c` si el agente aporta la pública

## Navegación y paleta
- [x] `F1`, `F5` y `F7` operativos; `F4` y `F6` siguen mostrando «vista no disponible en esta fase»
- [x] Entradas nuevas en la paleta: `sondear · <host>`, `sondear todos`, `ir a flota`, `ir a identidades`, `ir a registro`, `generar clave`, `exportar registro`
- [x] Ayuda `?` actualizada en Flota, Identidades y Registro
- [x] Ninguna acción destructiva (purgar registro, revocar, sobrescribir en importación) se ejecuta con una sola pulsación

## Calidad
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de almacén y parser
- [x] README actualizado con las vistas nuevas, el script de sondeo y los subcomandos `sondear` y `registro exportar`

---

**Progreso Fase 2:** 79 / 79 funcionalidades

**Total MAGI (Fases 1-2):** 205 / 207 funcionalidades
