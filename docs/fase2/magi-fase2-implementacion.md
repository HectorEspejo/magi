# MAGI - Fase 2: Informe de Implementación

**Fecha:** 15 de septiembre de 2026
**Estado:** implementación completa (79/79 funcionalidades del checklist) más
un anexo aprobado fuera de checklist (autenticación por contraseña, con opción
de guardarla en el llavero del sistema). `cargo test` (56 tests verdes y 1
marcado `#[ignore]` por requerir escritorio, ejecutado a mano con éxito),
`cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` en verde; el
sondeo se ha validado de extremo a extremo contra un `sshd` efímero local, la
contraseña y el llavero contra un servidor russh de pruebas y las vistas se
han recorrido en PTY/tmux.

## Resumen de lo implementado

La Fase 2 convierte MAGI en panel de control sobre la base validada de la
Fase 1:

- **Migración 2** (`PRAGMA user_version = 2`): columna `HOSTS.servicios`,
  tablas `SONDEOS`, `IDENTIDADES` y `REGISTRO` con sus índices, sin `CHECK`
  sobre enumeraciones (validación en código).
- **Registro**: `registro::anotar` es la única puerta de escritura; los
  eventos de Fase 1 (apertura y fallo de conexión, huellas, importación y
  exportación) quedan reconectados. Vista `F7` con filtro, detalle, purga a
  90 días y exportación CSV/JSON; subcomando `magi registro exportar`.
- **Flota**: vista de arranque (`F1`) con sondeo bajo demanda mediante un
  único script `sh` embebido por canal `exec`, semáforo de 8, timeout de 5 s,
  reutilización de la sesión viva, estados FRÍA/CAÍDA/ALCANZABLE/CARGA/NOMINAL
  con umbrales en `config.toml`, tasa de red por diferencia, auto-refresco
  configurable y `magi sondear` sin TUI. Campo **Servicios** (systemd) en la
  ficha y carga reciente en la barra de Sesión.
- **Identidades**: sincronización por huella desde el escaneo de `~/.ssh` y el
  agente, vista `F5`, generación ed25519/RSA 4096 con frase y alta en el
  agente, importación, copia de la pública con `wl-copy`/`pbcopy` y fallback,
  alias, revocar/reactivar (baja lógica) y último uso desde la autenticación.
- **Navegación**: `F1`, `F5` y `F7` operativos; `F4` y `F6` siguen reservadas;
  paleta con las entradas nuevas y ayuda actualizada.

**Anexo fuera de checklist (aprobado por el desarrollador el 15 sep 2026):**
autenticación por **contraseña**, en dos modos. «contraseña · se pide al
conectar» abre un diálogo enmascarado en cada conexión (3 intentos,
cancelable con `Esc`); «contraseña · llavero del sistema» recupera la
contraseña de `secret-tool`/libsecret (Linux) o Keychain (macOS) y, si no
existe, pide con la casilla «recordar» marcada. En ambos casos se usa
`Handle::authenticate_password` de russh y la contraseña va en
`Zeroizing<String>`. Con `contrasena:llavero`, `magi.db` solo guarda la
referencia, nunca el secreto; al autenticar con «recordar», la contraseña se
guarda en el llavero y el host queda marcado para recuperarla. El sondeo de un
host con llavero la usa sin diálogo (sondeo automático); sin entrada, anota
`sondeo_fallido` con la instrucción de conectarse con `↵` o usar una clave,
salvo que exista una sesión viva, en la que reutiliza su canal. La paleta
ofrece «olvidar contraseña · <host>» con confirmación para borrarla del
llavero.

## Desviaciones respecto a la especificación (qué y por qué)

1. **`rand_core::OsRng` no existe en `rand_core 0.10`** (la versión que usa
   `ssh-key 0.7.0-rc.11`). La generación usa
   `ssh_key::getrandom::SysRng` con `rand_core::UnwrapErr`, que es el patrón
   documentado por el propio `ssh-key`. RSA 4096 confirmado:
   `PrivateKey::random(Algorithm::Rsa { .. })` usa `DEFAULT_RSA_KEY_SIZE = 4096`.
   Con esto se cierra el riesgo R10 (RSA sí está disponible).
2. **`ssh-key` entra como dependencia directa** (fijada `=0.7.0-rc.11`, la
   misma que russh 0.63.3) con las features `ed25519`, `rsa`, `encryption` y
   `getrandom`, porque `encryption`/`getrandom` no se activan desde russh.
3. **`sondeo.sh`: el bucle de servicios de la especificación no distingue
   `not-found` de `inactive`** (el `|| echo unknown` producía dos líneas y un
   `inactive` engañoso). Se añade `systemctl show --property=LoadState` para
   que una unidad inexistente quede como `unknown`, como exige el checklist.
4. **`huella_aceptada` / `huella_sustituida` se anotan tras escribir de verdad
   en `known_hosts`**, mediante el evento nuevo `HuellaRegistrada`; así el
   registro no miente si la escritura falla. El resto de la decisión de huella
   no cambia.
5. **La regeneración automática de `magi_config`** (`exportar_al_guardar`) no
   anota `exportacion` (decisión acordada con el desarrollador para no inundar
   el registro al guardar fichas) y, tras revocar una identidad, no abre el
   diálogo del `Include`; la exportación explícita (`E`, paleta y
   `magi exportar`) sí anota.
6. **`magi sondear` persiste los resultados** en `SONDEOS` (y anota
   `sondeo_fallido`), además de imprimir la tabla; decisión confirmada.
7. **`registro::anotar` recibe la conexión como primer argumento**
   (`anotar(conexion, tipo, host_id, identidad_id, detalle, resultado)`): la
   función vive donde puede ser la única puerta y sigue siendo la única
   escritura en `REGISTRO`.
8. **Alias de identidad al generar/importar**: si el alias derivado
   (comentario o nombre de fichero) ya existe se añade un sufijo `-2`, `-3`…;
   al editar a mano un alias duplicado sí se rechaza con mensaje.
9. **`zeroize`**: la frase va en `Zeroizing<String>`; la clave descifrada se
   descarta en cuanto termina el alta en el agente. `ssh-key 0.7` no
   implementa `Zeroize` sobre `PrivateKey`, así que la destrucción efectiva de
   su memoria no puede forzarse desde MAGI; se minimiza su vida al máximo.
10. **Los sondeos fallidos se anotan en cada intento** (también con
    auto-refresco), como pide el checklist; con un host caído y auto-refresco
    de 15 s esto genera una entrada cada refresco, riesgo anotado abajo.
11. **Corrección de un fallo de Fase 1 detectado por los tests nuevos**:
    `almacen::hosts::por_nombre` no cargaba las etiquetas, de modo que
    sobrescribir por importación las perdía. Se corrige y se añade test.
12. **Certificados del agente**: no se registran en `IDENTIDADES` (solo
    claves públicas); F1 los muestra en el desplegable con sufijo `-cert`.
13. **La vista Registro exporta todas las entradas** (con `--desde` en CLI),
    no solo la página o el filtro visible.
14. **Sin reloj en la cabecera de Flota**: la cabecera muestra
    «sondeando i/n» durante el refresco y «N hosts · M host nominales ·
    sondeo hace X s» en reposo (el reloj no está en el checklist).
15. **Anexo fuera de checklist: autenticación por contraseña.** No estaba en
    los checklists de Fase 1 ni Fase 2; se propuso al desarrollador y se
    aprobó el 15 sep 2026. Se añaden `IdentidadRef::Contrasena` y
    `IdentidadRef::ContrasenaLlavero` (marcas `contrasena` y
    `contrasena:llavero` en `HOSTS.identidad_ref`, sin secreto), el diálogo
    `CONTRASEÑA` (3 intentos, casilla «recordar») y el arm correspondiente en
    `autenticar`, que usa `Handle::authenticate_password` de russh.
    `magi_config` no emite ninguna directiva para esas marcas porque son
    autenticación interna de MAGI (ssh_config no guarda contraseñas).
    Límites conocidos: russh convierte la contraseña en `String`
    internamente, así que el `zeroize` de MAGI cubre la copia del diálogo y
    la variable del intento, no las copias de la librería; y en macOS
    `security add-generic-password -w` recibe la contraseña por argumento, por
    lo que puede verse un instante en la lista de procesos (el camino Linux,
    `secret-tool` con la contraseña por stdin, no tiene ese problema).

## Estructura de archivos creada/modificada

```
magi/
├── Cargo.toml                     + serde_json, csv, ssh-key (pinned)
├── README.md                      vistas, sondeo, subcomandos y [flota]
├── docs/fase2/
│   ├── magi-fase2-checklist.md            (79/79 marcadas)
│   └── magi-fase2-implementacion.md       (este informe)
├── src/
│   ├── lib.rs                     + flota, portapapeles, registro
│   ├── main.rs                    + sondear y registro exportar; anotar
│   ├── app.rs                     estados y acciones de Flota/Identidades/
│   │                              Registro, diálogos nuevos, paleta
│   ├── config.rs                  + [flota] auto_refresco_seg y umbrales
│   ├── modelo.rs                  + servicios, Sondeo, Identidad, Registro
│   ├── identidades.rs             + generar, importar, normalizar, sync
│   ├── llavero.rs                 nuevo: secret-tool/security (contraseñas)
│   ├── portapapeles.rs            nuevo: wl-copy/pbcopy con fallback
│   ├── registro.rs                nuevo: anotar, filtros, export CSV/JSON
│   ├── almacen/
│   │   ├── migraciones.rs         + MIGRACION_2_FLOTA
│   │   ├── mod.rs                 + métodos de sondeos/identidades/registro
│   │   ├── hosts.rs               + servicios; fix etiquetas en por_nombre
│   │   ├── sondeos.rs             nuevo: guardar + purga a 20, últimos
│   │   ├── identidades.rs         nuevo: upsert, usar, revocar/reactivar
│   │   └── registro.rs            nuevo: listar paginado, purgar, exportar
│   ├── flota/
│   │   ├── mod.rs                 nuevo: orquestación y canal exec
│   │   ├── sondeo.sh              nuevo: script embebido
│   │   ├── parser.rs              nuevo: marcadores → Sondeo
│   │   └── estado.rs              nuevo: umbrales, estados, tasa, formato
│   ├── conexion/
│   │   ├── mod.rs                 + RegistroSesiones, HuellaRegistrada,
│   │   │                          huella en Abierta/PruebaOk
│   │   ├── cliente.rs             política interactiva, IdentidadUsada,
│   │   │                          registro de la sesión viva
│   │   └── salto.rs               handle compartido (Arc)
│   └── ui/
│       ├── mod.rs                 + vistas Flota, Identidades, Registro
│       ├── flota.rs               nuevo: lista, barras, servicios
│       ├── identidades.rs         nuevo: tabla y detalle
│       ├── registro.rs            nuevo: tabla, detalle, fechas
│       ├── ficha.rs               Servicios junto a Opciones extra
│       ├── dialogos.rs            generar clave, frase de importación,
│       │                          detalle y exportación del registro
│       ├── sesion.rs              «carga X.X» del último sondeo (<10 min)
│       ├── barra.rs · ayuda.rs    atajos de las tres vistas
├── tests/
│   ├── almacen.rs                 + migración 1→2, purga 20, SET NULL,
│   │                              sincronización y export del registro
│   ├── sshconfig_ida_vuelta.rs    + servicios/etiquetas al sobrescribir
│   └── password.rs                anexo: servidor russh de pruebas con
│                                  contraseña (acierto, 3 fallos, sondeo)
```

## Decisiones técnicas tomadas durante el desarrollo

- **Registro de sesiones vivas** (`conexion::RegistroSesiones`): mapa
  `host_id → Arc<Handle<Cliente>>` que la tarea de F1 rellena al abrir y vacía
  al cerrar; el sondeo abre un canal `exec` sobre el mismo handle (8 ms frente
  a ~120 ms de conexión efímera en las pruebas) y cae a conexión efímera si el
  canal falla o la sesión no existe.
- **Un único escritor de base de datos**: las tareas de red (sondeo,
  generación de claves) no tocan SQLite; emiten eventos y el bucle de UI
  guarda y llama a `registro::anotar`. Mantiene la UI sin bloqueos y la
  puerta única del registro.
- **Política `interactivo`** en `Contexto`/`Cliente`: las conexiones de la UI
  dialogan; el sondeo devuelve error con instrucción («conéctate una vez con
  ↵») ante huella desconocida/cambiada o clave con frase.
- **`EventoConexion::HuellaRegistrada`** para anotar la aceptación o
  sustitución solo cuando `known_hosts` se ha escrito.
- **Umbrales como tipo compartido** `flota::estado::Umbrales`, serializado en
  `[flota.umbrales]`; `SeccionFlota` con `Default` derivado.
- **Desplegable de identidad desde `IDENTIDADES`** (no desde el escaneo en
  vivo): excluye revocadas y muestra alias · tipo · origen; si el valor actual
  no está en la tabla se inserta como opción, como hacía F1.
- **Último uso por huella**: `autenticar` devuelve `IdentidadUsada` (descripción
  + huella) y `Abierta`/`PruebaOk` la llevan; el fallo de `UPDATE` se ignora
  con aviso si la huella no está registrada.
- **Importación de claves**: primero `<ruta>.pub`; si no existe, la privada
  pidiendo frase (diálogo propio `FraseImportacion`); la clave descifrada se
  descarta al terminar.
- **Generación en `spawn_blocking`** (RSA 4096 es CPU) y alta en el agente con
  `AgentClient::add_identity`; si el agente falla, aviso y la clave queda solo
  en fichero.
- **Contraseñas en el llavero del sistema, nunca en `magi.db`**: se reutiliza
  el patrón del portapapeles (`src/llavero.rs` lanza `secret-tool` en Linux y
  `security` en macOS) y en el host solo queda la referencia
  `contrasena:llavero`. El guardado ocurre tras autenticar y solo si el
  usuario marca «recordar»; la recuperación se usa tanto en conexiones
  interactivas como en el sondeo. La paleta ofrece olvidar con confirmación.
- **Filtros del registro** por tipo (`t`) y texto (`/`) en SQL con parámetros
  enlazados; paginación de 200 con carga diferida al llegar al final.
- **Purga** con fecha de corte calculada en Rust (90 días) mostrada en el
  diálogo con el recuento previo.
- **Comando de sondeo** con `sh -c '<script>' magi <args>`: el script embebido
  va entrecomillado escapando comillas simples y las unidades —validadas antes
  contra `[A-Za-z0-9@._-]+`— como argumentos.
- **Sondeo por lotes** con un canal `mpsc<Sondeo>` y un reenviador hacia el
  canal de eventos; `magi sondear` reutiliza `flota::lanzar_lote` y guarda los
  resultados.

## Funcionalidades del checklist completadas (copiando su texto exacto)

### Almacén y migración
- Migración 2 por `PRAGMA user_version`: columna `HOSTS.servicios`, tablas `SONDEOS`, `IDENTIDADES` y `REGISTRO`
- Tabla `SONDEOS` (id, host_id, fecha, resultado, error, nucleos, carga_1m, carga_5m, carga_15m, mem_total_kb, mem_disponible_kb, disco_total_kb, disco_usado_kb, red_rx_bytes, red_tx_bytes, uptime_seg, servicios_json, duracion_ms) con borrado en cascada por host
- Tabla `IDENTIDADES` (id, alias UQ, tipo, huella UQ, origen, ruta, comentario, anadida_en, ultimo_uso_en, revocada_en)
- Tabla `REGISTRO` (id, fecha, tipo, host_id, identidad_id, detalle, resultado) con `ON DELETE SET NULL` e índices por fecha y tipo
- Se conservan los 20 últimos sondeos por host; al insertar se purgan los anteriores
- Sin `CHECK` sobre enumeraciones; `resultado`, `origen` y `tipo` se validan en código
- Tests del almacén: migración 1→2 sobre una BD de Fase 1 con datos, purga de sondeos, cascadas y `SET NULL`

### Registro (infraestructura)
- `registro::anotar(tipo, host_id, identidad_id, detalle, resultado)` como única función de escritura en `REGISTRO`
- Apertura y fallo de conexión (Fase 1) anotan `conexion_abierta` / `conexion_fallida` con el motivo
- Aceptación y sustitución de huella anotan `huella_aceptada` / `huella_sustituida` con las huellas anterior y nueva
- Importación y exportación de ssh_config anotan `importacion` (recuentos) y `exportacion` (número de hosts)
- Las entradas de `REGISTRO` nunca contienen frases, claves ni salida de sesiones

### Vista Registro (F7)
- `F7` abre la vista Registro con tabla fecha · tipo · host · resultado, ordenada de más reciente a más antigua y paginada de 200 en 200
- Cabecera con número total de entradas y el filtro de tipo activo
- Navegación con `↑` `↓` / `j` `k` y panel inferior con el detalle de la entrada seleccionada
- `↵` abre un diálogo con el detalle completo
- `/` filtra por tipo, nombre de host o texto del detalle
- `t` cicla el filtro rápido por tipo: todos → conexiones → huellas → claves → importación → sondeos
- `p` purga las entradas de más de 90 días tras confirmación con recuento
- `x` exporta a CSV o JSON mediante diálogo de formato y ruta; el fichero se escribe vía `ficheros.rs` con permisos 600
- CSV con cabecera `fecha,tipo,host,identidad,resultado,detalle` (host e identidad por nombre); JSON como lista de objetos con los mismos campos
- `magi registro exportar <ruta> [--json] [--desde AAAA-MM-DD]` exporta sin TUI

### Ficha de host
- Campo «Servicios» en el bloque «Al conectar»: área de texto con unidades systemd, una por línea, guardado en `HOSTS.servicios`
- Validación de cada unidad contra `[A-Za-z0-9@._-]+`; se normaliza quitando `.service` final y líneas vacías
- Importación y exportación de ssh_config ignoran `servicios` (no existe en ssh_config) y lo conservan al sobrescribir

### Sondeo
- Script `sondeo.sh` embebido con `include_str!` que emite marcadores `MAGI_NUCLEOS`, `MAGI_LOADAVG`, `MAGI_MEM_*`, `MAGI_DISCO`, `MAGI_RED`, `MAGI_UPTIME` y `MAGI_SVC=<unidad>=<estado>`
- Las unidades de `servicios` se pasan al script como argumentos entrecomillados, nunca interpolados en el texto del script
- Ejecución por canal `exec` de russh; si hay sesión viva con el host se abre el canal sobre el mismo `Handle`, si no se usa una conexión efímera que se cierra al terminar
- Orquestación concurrente con semáforo de 8 tareas y timeout de 5 s por host (DNS, TCP, autenticación y comando incluidos)
- El sondeo nunca abre diálogos: huella desconocida o cambiada y clave que requiere frase se reportan como `error` con instrucción («conéctate una vez con ↵»)
- Parser de marcadores a `Sondeo`; sin `MAGI_LOADAVG` o `MAGI_UPTIME` el resultado es `sin_metricas`
- Cálculo de tasa de red con el sondeo anterior si tiene menos de 1 h y los contadores no retrocedieron; en caso contrario «—»
- Estado derivado: FRÍA (sin sondeo), CAÍDA (`error`), ALCANZABLE (`sin_metricas`), CARGA (umbral superado o servicio no `active`), NOMINAL
- Umbrales en `config.toml` `[flota.umbrales]`: `carga_por_nucleo` (1.0), `memoria_pct` (90), `disco_pct` (90)
- Un servicio listado que no existe en el host aparece como `unknown`, cuenta como no activo y muestra «unidad no encontrada»
- Un sondeo fallido anota `sondeo_fallido` en `REGISTRO` con el motivo
- Sondeo de un host cuyo salto falla marca CAÍDA al host con motivo «salto <nombre>: …» y también al salto si está en la lista
- Un sondeo nunca modifica `HOSTS.ultimo_estado` ni `ultima_conexion_en`
- `magi sondear [host…]` sondea sin TUI e imprime una tabla estado · carga · memoria · disco · servicios
- Tests del parser con salidas reales de Linux, salida sin `/proc` (sin_metricas) y servicio `unknown`

### Vista Flota (F1)
- `F1` abre Flota; MAGI arranca en Flota y con inventario vacío muestra «Sin hosts: pulsa n en Hosts o I para importar»
- Panel izquierdo con glifo + palabra de estado por host (`●` NOMINAL, `◐` CARGA, `○` FRÍA, `✕` CAÍDA, `●` ALCANZ.) y resumen «N hosts · M nominal · sondeo hace X s»
- Panel derecho del host seleccionado: barras de carga (x.x/núcleos), memoria %, disco %, red ↓↑, uptime en «d h», y pie «sondeado hace X s en N ms»
- Sección SERVICIOS con glifo por unidad (`●` active, `✕` otro estado); oculta si `servicios` está vacío
- Barras con `█`/`░` y `#`/`.` en modo ASCII; barra o servicio culpable en ámbar cuando el estado es CARGA
- Mientras un host se sondea su glifo es `◐` y la palabra «…»; la cabecera muestra «sondeando i/n»
- Cada resultado repinta su fila al llegar, sin esperar al resto
- `r` sondea el host seleccionado y `R` todos los visibles; entrar en Flota sondea los hosts sin sondeo reciente (> 60 s)
- `↵` conecta (flujo de la Fase 1) y `e` abre la ficha
- `/` filtra con el mismo filtro que Hosts
- Auto-refresco cada `[flota] auto_refresco_seg` (0 = desactivado, mínimo 15), conmutable con `a` durante la ejecución y pausado mientras la vista activa es Sesión
- La barra de estado de Sesión añade «carga X.X» si existe un sondeo del host de menos de 10 min

### Identidades (tabla y sincronización)
- Cada escaneo (arranque, `s`, abrir el desplegable de identidad) hace upsert en `IDENTIDADES` por huella: alta con `anadida_en`, actualización de tipo, comentario, ruta y origen
- Claves `ed25519-sk` / `ecdsa-sk` se registran con origen `token`
- Una clave revocada que reaparece en el escaneo sigue revocada; una que desaparece no se borra y muestra «no encontrada»
- `USADA EN` cuenta los hosts cuyo `identidad_ref` coincide por huella (`agente:`) o por ruta (`fichero:`)
- `ultimo_uso_en` se actualiza desde `conexion/` al autenticar con éxito, buscando por huella de la clave usada
- El desplegable de identidad de la ficha no ofrece identidades revocadas

### Vista Identidades (F5)
- `F5` abre Identidades: tabla alias · tipo · usada en · origen y panel de detalle (huella, vista/uso, fichero, hosts)
- Navegación con `↑` `↓` / `j` `k`; `v` muestra u oculta las revocadas
- `n` abre el diálogo de generación: fichero (por defecto `id_ed25519_<alias>`), tipo ed25519 | rsa 4096, comentario, frase repetida (opcional, con aviso si queda vacía), casillas «añadir al agente» y «copiar la pública»
- Generación con `ssh-key` y `OsRng`, cifrado OpenSSH con la frase, escritura de `~/.ssh/<nombre>` (600) y `<nombre>.pub` (644) vía `ficheros.rs`
- Alta en el agente con `AgentClient::add_identity` si la casilla está marcada; si el agente no está disponible, aviso y la clave queda solo en fichero
- La clave descifrada y la frase se destruyen con `zeroize` tras generar y añadir
- Generar anota `clave_generada` en `REGISTRO` y crea la fila en `IDENTIDADES`
- `i` importa una ruta: lee la huella de `<ruta>.pub` o, si no existe, de la privada (pidiendo frase si está cifrada) y anota `clave_importada`
- `c` copia la clave pública al portapapeles con `wl-copy` o `pbcopy`; si no hay binario, diálogo con la línea para copiar a mano
- `e` edita el alias (único, no vacío)
- `x` revoca tras confirmación con el número de hosts que pasan a `auto`; pone `identidad_ref = NULL` en ellos, marca `revocada_en`, anota `referencia_revocada` y regenera `magi_config` si `exportar_al_guardar`
- `x` sobre una revocada la reactiva (`revocada_en = NULL`)
- Revocar nunca borra ficheros de `~/.ssh` ni quita claves del agente
- Las claves de origen `token` no ofrecen `i` y solo ofrecen `c` si el agente aporta la pública

### Navegación y paleta
- `F1`, `F5` y `F7` operativos; `F4` y `F6` siguen mostrando «vista no disponible en esta fase»
- Entradas nuevas en la paleta: `sondear · <host>`, `sondear todos`, `ir a flota`, `ir a identidades`, `ir a registro`, `generar clave`, `exportar registro`
- Ayuda `?` actualizada en Flota, Identidades y Registro
- Ninguna acción destructiva (purgar registro, revocar, sobrescribir en importación) se ejecuta con una sola pulsación

### Calidad
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- `cargo test` verde con los tests nuevos de almacén y parser
- README actualizado con las vistas nuevas, el script de sondeo y los subcomandos `sondear` y `registro exportar`

## Pendientes y bloqueos

1. **Validación manual del desarrollador** con hosts reales: sondeo con
   servicios y saltos, auto-refresco, generación RSA 4096, copiado al
   portapapeles en Wayland, revocación sobre hosts en uso y **conexión por
   contraseña contra un host real** (el acierto se ha probado contra el
   servidor russh de `tests/password.rs`; con el `sshd` del sistema solo se
   pudo verificar el flujo de fallo al no conocer la contraseña del usuario).
   El entorno de pruebas cubrió un `sshd` efímero local, PTY/tmux y un `HOME`
   aislado.
2. **`zeroize` de la clave descifrada**: limitado por `ssh-key 0.7` (ver
   desviación 9). La frase sí vive en `Zeroizing`.
3. **Ruido de `sondeo_fallido`**: con auto-refresco activo y un host caído se
   anota una entrada por refresco (decisión 10). Si molesta, la fase siguiente
   puede anotar solo la transición a CAÍDA.
4. **CI y macOS** siguen pendientes de Fase 1 (sin remoto y sin equipo). El
   camino macOS del llavero (`security`) está implementado pero sin validar;
   el de Linux (`secret-tool`) se probó contra gnome-keyring.
5. **Pruebas del checklist no automatizables** (AC del semáforo con 20 hosts y
   huellas reales) quedan cubiertas parcialmente: timeout de 5 s verificado
   con un host que no responde; el resto, en la validación manual.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

```sh
cargo run                       # TUI; arranca en Flota y migra a user_version 2
cargo run -- importar [ruta]    # importa ~/.ssh/config (anota importacion)
cargo run -- exportar           # regenera magi_config (anota exportacion)
cargo run -- sondear [host…]    # sondea, guarda y imprime la tabla
cargo run -- registro exportar <ruta> [--json] [--desde AAAA-MM-DD]
cargo test                      # 56 tests verdes (y 1 ignorado de llavero)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

- **Migración**: automática al abrir la BD; `magi.db` de Fase 1 pasa a
  `user_version = 2` conservando hosts, grupos y etiquetas (test dedicado con
  BD v1 y datos).
- **Aislamiento de pruebas**: `HOME` + `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/
  `XDG_STATE_HOME` a un directorio temporal.
- **Pruebas realizadas**: `cargo test` (30 unitarios + 15 de almacén + 7 de
  ssh_config + 4 de contraseña y llavero), `clippy -D warnings` y `fmt --check`;
  `magi sondear` contra un `sshd` efímero (NOMINAL, CAÍDA por conexión
  rechazada, CAÍDA por timeout de 5,01 s, servicio inexistente → CARGA) con
  filas en `SONDEOS` y `sondeo_fallido` en `REGISTRO`; sesión interactiva +
  sondeo sobre la sesión viva (8 ms) con carga en la barra de Sesión;
  generación ed25519 en la TUI, importación, alias, revocación/reactivación y
  copiado con fallback sin `wl-copy`/`pbcopy`; auto-refresco a 15 s verificado
  en BD; ficha con Servicios guardada y conservada al reimportar.
- **Anexo de contraseña**: tests de integración contra un servidor russh
  efímero en `tests/password.rs` (contraseña correcta aceptada, incorrecta
  reintentada 3 veces, el sondeo no dialoga, y con llavero da instrucción sin
  diálogo); test `#[ignore]` del llavero ejecutado a mano (`guarda recupera y
  olvida` contra gnome-keyring) y prueba manual en tmux contra el `sshd` real
  (diálogo «intento 3 de 3», «contraseña incorrecta tras 3 intentos» y
  `conexion_fallida`). Con un servidor russh de contraseña y métricas
  simuladas se validó de extremo a extremo: guardado en llavero al autenticar,
  sondeo automático con la contraseña guardada (sin diálogo), conexión
  interactiva sin diálogo, sondeo sobre la sesión viva (1 ms) y «olvidar
  contraseña» desde la paleta con borrado real de la entrada.

> Aviso para el desarrollador: entrega este informe al analista funcional
> antes de especificar la Fase 3; hasta entonces, el checklist maestro y el
> informe maestro no reflejan la realidad implementada.
