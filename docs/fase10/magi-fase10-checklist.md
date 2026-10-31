# MAGI - Checklist Fase 10: Sincronización Cifrada

## Modelo y migración
- [x] Migración 7 por `PRAGMA user_version`: `uuid`, `version` y `vector` en GRUPOS, HOSTS, IDENTIDADES, TUNELES, SNIPPETS y SINCRONIZACIONES_DIR, con índice único por `uuid`; las filas existentes reciben `uuid` nuevo y versión de este equipo
- [x] Tabla `EQUIPOS` (id, uuid, nombre, plataforma, es_este, app_version, ultimo_fichero_en, ultima_importacion_en, hash_ultimo_fichero, estado, quitado_en, reloj_ms) con la fila de este equipo creada en la migración (nombre = hostname)
- [x] Tabla `LAPIDAS` (uuid PK, tipo, nombre, version, vector, borrado_en) y tabla `CONFLICTOS` (id, tipo, elemento_tipo, uuid, nombre, version_perdedora_json, version_ganadora, equipo_ganador, detectado_en, resuelto_en, resolucion); sin `CHECK` en sus enumeraciones
- [x] `IDENTIDADES.uuid` es determinista (UUIDv5 de la huella): la misma clave tiene el mismo `uuid` en todos los equipos
- [x] Reloj híbrido: cada versión nueva usa `max(ahora_ms, reloj_ms + 1)` y actualiza `EQUIPOS.reloj_ms`; al importar, `reloj_ms` sube a la mayor versión vista
- [x] Toda escritura desde el almacén de un campo que viaja actualiza `version` (`<ms>.<uuid del equipo>`) y `vector[este equipo]`; los campos propios del equipo no tocan la versión
  - AC: Dado un host, cuando se conecta (cambia `ultima_conexion_en`) o se cambia su último directorio SFTP, entonces su `version` no cambia y no se exporta nada
- [x] Cambiar etiquetas o verificaciones de un host sube la versión del host; cambiar los destinos de un snippet sube la versión del snippet
- [x] Borrar un elemento que viaja deja su lápida con versión y vector; el borrado en cascada de un host deja lápidas de sus túneles y sincronizaciones de directorio

## Fichero de sincronización
- [x] Un fichero por equipo, `magi-<uuid del equipo>.age`, en la carpeta configurada; cada equipo solo escribe el suyo
- [x] Contenido en claro: línea `MAGI-SINCRONIZACION 1`, línea `firma: <hex>` y el JSON (`formato`, `app`, `equipo`, `generado_en`, `reloj_ms`, `elementos`, `lapidas`, `huellas`, `equipos_quitados`), cifrado con `age` a la clave X25519 de la sincronización
- [x] Firma HMAC-SHA256 del JSON con una clave derivada de la identidad (HKDF-SHA256, info `magi-sincronizacion-firma-v1`); un fichero sin firma válida se rechaza aunque se descifre
  - AC: Dado un fichero cifrado a la clave pública correcta pero con el JSON alterado, cuando se importa, entonces se rechaza con «firma no válida» y no se aplica nada
- [x] Se rechaza un fichero cuyo `equipo.uuid` no coincide con su nombre de fichero o cuyo equipo está en `equipos_quitados`
- [x] Viajan: grupos (sin `plegado`), hosts con etiquetas por nombre y verificaciones (sin estado, última conexión ni directorios SFTP), identidades por huella (alias, tipo, comentario, revocada_en), túneles, snippets con destinos (sin `usado_veces` ni `ultimo_uso_en`) y sincronizaciones de directorio (sin `ultima_ejecucion_en` ni `ultimo_resultado`)
- [x] No viajan REGISTRO, SONDEOS, DELIBERACIONES, `config.toml`, contraseñas del llavero, claves privadas ni la clave de sincronización
- [x] Las rutas bajo el home (`identidad_ref fichero:`, `ruta_local`) se exportan como `~/…` y se expanden al importar
- [x] El fichero propio se escribe de forma atómica por `src/ficheros.rs` (600) y solo cuando cambia su contenido
- [x] Un fichero con `formato` mayor que el que conoce el binario se ignora con el aviso «el equipo X usa una versión más nueva de MAGI; actualiza»

## Fusión y conflictos
- [x] La fusión lee el estado local y todos los ficheros ajenos legibles: por elemento, si un vector domina al otro gana el dominante; si son concurrentes gana la versión más reciente y el vector resultante es el máximo de ambos
  - AC: Dado un host editado en A y después en B tras importar lo de A, cuando A importa, entonces aplica la versión de B sin registrar conflicto
- [x] Si la versión local pierde frente a una concurrente con contenido distinto, se guarda en `CONFLICTOS` (tipo `elemento`) con su contenido completo
  - AC: Dado un host cambiado en A y en B antes de sincronizar, cuando se sincronizan, entonces en los dos queda la versión más reciente y el equipo cuya versión perdió tiene un conflicto con ella
- [x] Lápidas: un borrado gana a una edición anterior y pierde ante una posterior (la edición resucita el elemento); las lápidas se conservan siempre
- [x] Una referencia a un elemento inexistente o borrado (grupo, salto, snippet al conectar) se resuelve como vacía; un túnel o una sincronización de directorio cuyo host no existe se descarta como conflicto `huerfano`
- [x] Colisión de nombre entre elementos distintos (nombre de host, grupo, snippet, alias de identidad; nombre de túnel o sincronización dentro del host): se renombra el de `uuid` mayor a `<nombre> (2)` como edición nueva y se registra conflicto `nombre`
- [x] Un borrado entrante de un host con sesiones, túneles activos, transferencias o ejecuciones en curso, o de un túnel activo, no se aplica: queda como conflicto `borrado` aplazado
- [x] Una importación que cambia el inventario regenera `~/.ssh/magi_config` como cualquier edición local
- [x] Conflictos: `r` recupera la versión perdedora como edición nueva, `d` la descarta, `a` acepta (huellas y borrados aplazados sin trabajo); los resueltos se conservan 90 días
- [x] La fusión es determinista: el mismo conjunto de ficheros da el mismo resultado en cualquier orden y en cualquier equipo (test de propiedades)

## Huellas
- [x] El fichero incluye las huellas de `~/.ssh/known_hosts` de los hosts del inventario (host, tipo y clave pública)
- [x] Al importar, una huella se añade a `known_hosts` (por `ficheros.rs`, con copia y REGISTRO `huella_sincronizada`) solo si este equipo no tiene ninguna para ese host
- [x] Si este equipo ya tiene otra huella para ese host, nunca se sustituye sola: conflicto `huella` con «aceptar la del otro equipo» o «mantener la mía»
  - AC: Dado db-01 con huella X aquí y huella Y aceptada en otro equipo, cuando se importa, entonces `known_hosts` sigue con X y aparece el conflicto con la fecha en que el otro equipo aceptó Y

## Carpeta, cuándo y cerrojo
- [x] `[sincronizacion]` en `config.toml`: `carpeta`, `retraso_seg = 5`, `copias = 10`, `umbral_borrado_hosts = 5`, `umbral_borrado_pct = 20`; sin `carpeta` la sincronización está desactivada
- [x] Al abrir MAGI se hace una pasada (importar lo que cambió, fusionar, aplicar, exportar si hace falta)
- [x] Mientras MAGI está abierto vigila la carpeta (`notify`, o sondeo cada 30 s si no puede) e importa cuando cambia el fichero de otro equipo, agrupando 2 s
- [x] Exporta `retraso_seg` segundos después del último cambio local; al salir, una exportación pendiente se hace antes de cerrar
- [x] Solo un proceso sincroniza a la vez (cerrojo `flock` en `$XDG_RUNTIME_DIR/magi/sincronizacion.lock`); otra ventana o `magi sincronizar` esperan o lo dicen
- [x] Se leen solo ficheros `magi-<uuid>.age`; se ignoran temporales y copias en conflicto de las herramientas de sincronización; un fichero que no se descifra o no tiene firma válida se reintenta en la siguiente pasada

## Crear, unir, quitar y clave
- [x] Crear (carpeta sin ficheros de MAGI): genera la identidad `age`, la guarda en el llavero (`src/llavero.rs`), la muestra como texto y QR, pide confirmar «la he guardado» y exporta el primer fichero
- [x] Unir (carpeta con ficheros de MAGI): pide la clave pegada (oculta, con opción de verla), la valida descifrando y comprobando la firma de un fichero, la guarda en el llavero e importa
  - AC: Dado una clave equivocada, cuando se intenta unir, entonces sale «la clave no abre los ficheros de esta carpeta» y no se guarda nada
- [x] Al unir, los elementos locales con el mismo nombre que uno del fichero adoptan su `uuid` y se fusionan (gana la versión más reciente, la otra queda como conflicto); el resto se suma
- [x] «Mostrar clave» (texto y QR) desde el llavero, con confirmación
- [x] «Quitar equipo» (otro equipo): confirmación, lo añade a `equipos_quitados`, borra su fichero de la carpeta y cambia la clave (nueva identidad mostrada como texto y QR; la anterior solo para leer, 30 días como máximo)
- [x] Si el fichero de un equipo conocido deja de abrirse con la clave de este equipo, la sincronización se detiene aquí hasta introducir la clave nueva; con ella, este equipo vuelve a cifrar su fichero
- [x] Un equipo que se encuentra en `equipos_quitados` deja de sincronizar y ofrece unirse de nuevo como equipo nuevo
- [x] «Desactivar en este equipo»: borra su fichero de la carpeta y la clave del llavero y quita `carpeta`; `magi.db` queda intacta

## Red de seguridad
- [x] Antes de aplicar una importación que cambia algo, copia de `magi.db` en `~/.local/share/magi/copias/magi-<AAAAMMDD-HHMMSS>.db` (600, `VACUUM INTO`); se conservan las `copias` últimas
- [x] Si una importación va a borrar más de `umbral_borrado_hosts` hosts o al menos `umbral_borrado_pct` % de ellos, diálogo IMPORTACIÓN CON BORRADOS: `a` aplicar, `m` mantener los míos (edición nueva que los devuelve a todos), `Esc` decidir luego (sincronización en pausa)
  - AC: Dado otro equipo que ha borrado 7 de 23 hosts, cuando este equipo importa, entonces aparece el diálogo con los 7 nombres, ya existe la copia de `magi.db` y no se ha borrado nada
- [x] En pausa no se importa ni se exporta hasta que el usuario decide desde el diálogo o desde el panel

## Interfaz
- [x] Vista SINCRONIZACIÓN desde la paleta (`sincronización`): carpeta, huella corta de la clave, este equipo, lista de equipos (nombre, plataforma, versión de MAGI, último fichero, estado) y conflictos pendientes; `q`/`Esc` vuelve
- [x] Teclas de la vista: `s` sincronizar ahora, `c` conflictos, `k` mostrar clave, `x` quitar equipo, `r` cambiar clave, `d` desactivar aquí; sin configurar, `n` configurar carpeta (crea o une según lo que encuentre)
- [x] Vista CONFLICTOS: tipo, elemento, quién ganó y cuándo, con el detalle de los campos que difieren; acciones `r`, `a`, `d` según el tipo
- [x] Indicador en la barra inferior: `⇅ al día`, `⇅ N conflictos`, `⇅ pausa`, `⇅ clave nueva`, `⇅ error` y `⇅ …` sincronizando, con color y en ASCII (`sync`); no aparece si la sincronización está desactivada
- [x] QR de la clave pintado con medios bloques; por debajo de su tamaño se muestra solo el texto con «amplía la terminal para ver el QR»
- [x] Vistas SINCRONIZACIÓN y CONFLICTOS y diálogos (crear, unir, clave, importación con borrados) con mínimo y modo estrecho en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)
- [x] Entradas de paleta `sincronización`, `sincronizar ahora` y `conflictos de sincronización`

## CLI, diagnóstico y registro
- [x] `magi sincronizar` hace una pasada sin TUI: código 0 si va bien, 1 con error y 2 si hace falta decidir (borrados por encima del umbral o clave nueva), sin aplicar nada en ese caso
- [x] `magi sincronizar estado` muestra carpeta, equipos con su estado y último fichero, y conflictos pendientes, sin descifrar nada
- [x] `magi diagnostico` añade la comprobación de sincronización (carpeta accesible y escribible, estado de los equipos) sin leer la clave del llavero
- [x] Tipos nuevos en REGISTRO: `inventario_importado` (equipos, creados, actualizados, borrados, conflictos), `inventario_conflicto`, `inventario_equipo_quitado`, `inventario_clave_cambiada`, `inventario_sinc_activada`, `inventario_sinc_desactivada` e `inventario_sinc_error` (solo al cambiar de estado); el filtro `t` los agrupa en «inventario»

## Documentación y calidad
- [x] `docs/formato-sincronizacion.md`: formato completo del fichero (cabecera, firma, JSON, tipos de elemento y campos, reglas de fusión) para la app Android (F11)
- [x] README: activar la sincronización, añadir un equipo, guardar la clave, quitar un equipo y abrir un fichero con `age` fuera de MAGI
- [x] CHANGELOG en «Sin publicar» con la sincronización cifrada (versión MINOR: migración 7)
- [x] Tests: migración 6→7, reloj híbrido, versiones solo en campos que viajan, fusión (dominancia, concurrencia, lápidas, huérfanos, nombres, determinismo), firma y equipo del fichero, huellas, umbral de borrados, unión por nombre y rotación de clave
- [x] `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check` y `TZ=UTC cargo test` limpios y en verde
- [x] Revisión adversarial del código nuevo (fusión, borrados, huellas, clave y llavero, ficheros de la carpeta) con verificación independiente de cada hallazgo, resultado en el informe de implementación

---

**Progreso Fase 10:** 63 / 63 funcionalidades

**Total MAGI (Fases 1-10):** 659 / 717 funcionalidades