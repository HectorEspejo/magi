# MAGI - Checklist Fase 6: Snippets y Deliberación MAGI

## Corrección 3b: pool de conexiones
- [x] `conexion_para_canal(host, politica)` es la única vía para obtener conexión en pestañas, SFTP, túneles, sondeo por `Ejecutar` y ejecuciones, con un cerrojo por host que cubre «mirar el pool / conectar / guardar»
- [x] Pestañas sobre el pool: con `multiplexar` marcado una segunda sesión al mismo host abre un canal nuevo sin reautenticar; sin marcar, conexión propia que se cierra con su sesión (al completarlo, marca también la línea desmarcada del checklist de la Fase 3)
  - AC: Dado un host con `multiplexar` y una pestaña abierta, cuando se abre otra, entonces no aparece diálogo de huella ni de frase y `conexion_abierta` se anota una sola vez
- [x] Una pestaña sin `multiplexar` abre conexión propia que no entra en el pool; `Pool::guardar` solo inserta si no hay entrada y nunca desplaza una conexión viva ni en gracia
  - AC: Dado un host con un túnel activo, cuando se abre una pestaña a ese host, entonces el túnel sigue `●` y sin anotación `tunel_cerrado`
- [x] Aperturas simultáneas del mismo host (pestaña + SFTP + túnel) comparten una única conexión con `multiplexar`; test de regresión con las tres a la vez
- [x] Aviso de versión incompatible con el pid del servidor antiguo (de `VersionIncompatible` o, si no lo trae, por `SO_PEERCRED` del socket)
- [x] `magi servidor parar` contra un servidor de otra versión muestra su pid y le envía `SIGTERM` tras confirmación (`--si` la omite)
- [x] El servidor trata `SIGTERM` como `Parar`: cierra sesiones, túneles y ejecuciones con sus anotaciones y borra socket y lock
- [x] Un `Hecho` de una operación que anota en `REGISTRO` se envía después de que el hilo escritor confirme la fila; `borrar_remoto_es_recursivo_y_anota_el_borrado` deja de ser intermitente
- [x] README corregido: la reutilización con `multiplexar` describe el comportamiento real

## Almacén y modelo
- [x] Migración 5 por `PRAGMA user_version`: tablas `SNIPPETS`, `SNIPPET_DESTINOS`, `VERIFICACIONES_HOST`, `DELIBERACIONES` y columna `HOSTS.snippet_al_conectar_id` (FK `SNIPPETS` `ON DELETE SET NULL`)
- [x] Tabla `SNIPPETS` (id, nombre UQ, comando, descripcion, etiquetas, critico, timeout_seg, parar_al_fallo, usado_veces, ultimo_uso_en, creado_en, actualizado_en)
- [x] Tabla `SNIPPET_DESTINOS` (id, snippet_id FK CASCADE, etiqueta TEXT NULL, host_id FK HOSTS CASCADE NULL) con exactamente uno de los dos validado en código; etiqueta por nombre
- [x] Tabla `VERIFICACIONES_HOST` (host_id PK FK CASCADE, salud, backup, backup_ruta, backup_patron, tests, tests_comando, actualizado_en)
- [x] Tabla `DELIBERACIONES` (id, fecha, snippet_id FK SET NULL, accion, hosts_json, comprobaciones_json, resultado, bloqueada, motivo, usuario, ejecucion_resultado)
- [x] Sin `CHECK` sobre enumeraciones; `resultado` y `ejecucion_resultado` validados en código
- [x] Tipos nuevos en `REGISTRO`: `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`; el filtro `t` de la vista Registro gana el grupo «snippets»
- [x] Tests del almacén: migración 4→5, cascadas de destinos y verificaciones, `SET NULL` en deliberaciones y en `snippet_al_conectar_id`

## Snippets: modelo y validación
- [x] Validación: nombre único y no vacío, comando no vacío, timeout 5-3600, al menos un destino; etiquetas del snippet normalizadas a minúsculas
- [x] Variables `{{nombre}}` y `{{nombre:defecto}}` detectadas al guardar (nombre `[a-z_][a-z0-9_]*`) y listadas en el detalle
- [x] Resolución de destinos: hosts con alguna etiqueta destino ∪ hosts sueltos, sin duplicados, ordenados por grupo y nombre; guardar con cero hosts resueltos avisa sin bloquear
  - AC: Dado un snippet con destino etiqueta «web» y el host suelto hetzner-01 que también es «web», cuando se resuelve, entonces hetzner-01 aparece una sola vez
- [x] Sustitución de variables escapada para el shell con `shell-escape`; una variable vacía sin defecto no deja continuar
  - AC: Dado `systemctl restart {{servicio}}` y el valor `a b; rm -rf /`, cuando se sustituye, entonces el comando resultante pasa el valor como un único argumento entrecomillado
- [x] Tests unitarios de resolución de destinos y de variables

## Vista Snippets (F8)
- [x] `F8` abre Snippets: lista nombre · destino resumido (etiqueta o «todos») → N hosts · `CRÍTICO`; cabecera con el total; vacía con «Sin snippets: n para crear uno»
- [x] Panel inferior: comando, destinos resueltos con nombres, confirmación («requiere deliberación MAGI» y por qué), timeout, usado N veces y última vez
- [x] Navegación con `↑` `↓` / `j` `k`; `/` filtra por nombre, comando, etiquetas del snippet o etiqueta destino
- [x] `n` / `e` abren el formulario: nombre, comando (multilínea), descripción, etiquetas, destinos (etiquetas con autocompletado de las de host y hosts sueltos con desplegable), crítico, timeout, parar al primer fallo; `Ctrl+S` guarda
- [x] `x` borra con confirmación; si es snippet al conectar de algún host, lo dice y esos hosts quedan sin él
- [x] `t` abre Resultados; `q` / `Esc` vuelve
- [x] Entradas de paleta: `snippet · <nombre>`, `snippet · <nombre> · <host>`, `ir a snippets`, `ir a resultados`, `nuevo snippet`
- [x] `!` en Hosts y Flota abre la paleta filtrada a los snippets que apuntan al host seleccionado, para ejecutarlos solo en ese host
- [x] `magi snippets` lista los snippets con destinos resueltos, crítico y último uso

## Lanzar una ejecución
- [x] `↵` abre el diálogo EJECUTAR: casillas por host resuelto (todas marcadas), variables con su defecto, casilla «parar al primer fallo» con el valor del snippet y timeout visible
- [x] `a` ejecuta en todos los destinos sin diálogo de hosts (solo pide variables si las hay)
- [x] Tras el diálogo, se decide si hay deliberación (crítico, más de un host o algún host con verificaciones activas); sin deliberación se lanza directamente
- [x] `LanzarEjecucion` lleva el comando ya sustituido, los host_ids, timeout, parar_al_fallo y, si hubo, la deliberación (id, forzada, motivo); el servidor no lee `SNIPPETS`
- [x] Al lanzar, el cliente incrementa `usado_veces` y `ultimo_uso_en` y abre Resultados en la ejecución nueva
- [x] `p` abre en pestaña: una pestaña por host con `AbrirSesion{…, comando_inicial}` (confirmación si son más de 5), pasando antes por deliberación si procede; anota `snippet_ejecutado` con «en pestaña»

## Servidor: ejecuciones
- [x] `VERSION_PROTOCOLO = 4`; mensajes `LanzarEjecucion` (respondido con `Hecho{peticion_id}` o `Error{peticion_id, mensaje}`), `CancelarEjecucion`, `LimpiarEjecuciones`, `PedirSalida`, `Ejecuciones`, `Salida`; `AbrirSesion` gana `comando_inicial`; `Bienvenida` incluye las ejecuciones; tests de ida y vuelta y rechazo de v3
- [x] `servidor/ejecuciones.rs`: `Ejecucion {id, snippet_id, nombre, comando, hosts, timeout_seg, parar_al_fallo, deliberacion, solicitante, creada_en, terminada_en, estado}` y `EjecucionHost {host_id, nombre, estado, codigo, inicio, fin, stdout, stderr, error}`
- [x] Hasta 8 hosts en paralelo; cada host toma conexión con `conexion_para_canal` y política no interactiva (huella, frase o contraseña sin llavero → error del host con instrucción)
- [x] Canal `exec` con stdout y stderr separados, código de salida y timeout del snippet (al vencer se cierra el canal y el host queda en error «tiempo agotado»)
  - AC: Dado un snippet `sleep 10` con timeout 5 s, cuando se ejecuta, entonces el host queda en error «tiempo agotado» a los 5 s y la conexión del pool sigue viva
- [x] Estados de host `en_cola`, `conectando`, `ejecutando`, `ok`, `fallo`, `error`, `cancelado`, `omitido`; estados de ejecución `en_curso`, `terminada`, `cancelada`
- [x] «Parar al primer fallo»: tras un `fallo` o `error`, los hosts aún en cola pasan a `omitido`; los que están en marcha terminan
  - AC: Dado 10 hosts con parar al primer fallo y el segundo devuelve código 1, cuando termina, entonces los que no habían empezado quedan `omitido`
- [x] Salida con tope de 1 MiB por flujo y marca `truncada`; nunca se escribe en `REGISTRO` ni en el log
- [x] `CancelarEjecucion`: hosts en cola → `omitido`, en marcha → canal cerrado y `cancelado`; ejecución `cancelada`
- [x] `Ejecuciones{lista}` con metadatos por host (sin salida) difundida en cada cambio de estado; `PedirSalida` devuelve `Salida` del host
- [x] `snippet_ejecutado` en `REGISTRO` por host al llegar a `ok`, `fallo`, `error` o `cancelado` (snippet, host, código, duración, bytes)
- [x] Al terminar una ejecución con deliberación, el servidor anota `deliberacion_aprobada` o `deliberacion_forzada` (acción, hosts, comprobaciones, motivo, usuario, resultado) y rellena `DELIBERACIONES.ejecucion_resultado` (`ok`, `parcial`, `error`, `cancelada`) por su hilo escritor
- [x] Terminadas conservadas 1 h o hasta `LimpiarEjecuciones`; el servidor no se apaga por inactividad con ejecuciones en curso; `magi servidor estado` las lista
- [x] `comando_inicial` de `AbrirSesion` se escribe en la pestaña tras abrir el shell, también al reconectar
- [x] Tests contra el servidor SSH en proceso: ejecución en varios hosts, código ≠ 0, timeout, parar al primer fallo, cancelación en curso, tope de salida, anotaciones

## Vista Resultados
- [x] Panel de ejecuciones (hora, snippet, hosts, ✓ ✕ en marcha, estado, `⚑` si forzada) y panel de hosts de la seleccionada (glifo, estado, código, duración, bytes); `Tab` cambia de panel
- [x] `↵` sobre un host muestra stdout y stderr separados y desplazables, con las secuencias de escape eliminadas; se refresca cada 1 s mientras el host está en marcha
- [x] `s` guarda la salida del host (o de todos) en un fichero vía `ficheros.rs` con permisos 600
- [x] `x` cancela (confirmación si está en curso); `r` repite con el mismo snippet, hosts y variables pasando de nuevo por deliberación si procede; `C` limpia terminadas
- [x] Una ventana nueva ve las ejecuciones desde `Bienvenida`; todas las ventanas ven el mismo progreso

## Deliberación MAGI
- [x] Bloque «Verificaciones previas (deliberación MAGI)» en la ficha: casillas salud, backup (ruta y patrón) y tests (comando local); guarda en `VERIFICACIONES_HOST`
- [x] Deliberación obligatoria para snippets críticos, para toda ejecución en más de un host y para hosts con alguna verificación activa
- [x] MELCHIOR-1 salud: aprueba con último sondeo NOMINAL de menos de `salud_max_min`; si es viejo o no existe, sondea (no interactivo); CARGA, ALCANZABLE, CAÍDA o sin datos en plazo rechazan con detalle
- [x] BALTHASAR-2 backup: `AbrirSftp` no interactivo + `ListarDir(backup_ruta)`; aprueba si el fichero más reciente que casa con `backup_patron` tiene menos de `backup_horas`; más viejo, vacío o sin acceso rechaza con el dato
- [x] CASPER-3 tests: `sh -c tests_comando` en local; código 0 aprueba, ≠ 0 rechaza con la última línea de stderr; al vencer el plazo se mata el proceso y rechaza
- [x] Todas las comprobaciones de todos los hosts en paralelo con plazo de `limite_seg` (2 s) cada una; ninguna abre diálogos
  - AC: Dado un host con tests `sleep 3`, cuando se delibera, entonces CASPER-3 rechaza «plazo vencido» a los 2 s y el proceso ya no existe
- [x] Diálogo MAGI: acción, tabla host × comprobación con `✓` / `✕` / `—` / `◐` y el dato de cada veredicto, barra de consenso N / M y estado APROBADO, BLOQUEADO o FORZADO
- [x] Consenso = unanimidad de las comprobaciones activas; sin comprobaciones activas muestra «sin comprobaciones configuradas» y sigue pidiendo `Ctrl+K`
- [x] `Ctrl+K` ejecuta solo con consenso o forzada; `↵` no ejecuta
- [x] `f` en BLOQUEADO abre el campo motivo; con menos de 10 caracteres no se acepta; con motivo, estado FORZADO y `Ctrl+K` ejecuta
  - AC: Dado una deliberación bloqueada, cuando se escribe «ok» y se pulsa `Ctrl+K`, entonces no se ejecuta nada y el campo indica el mínimo
- [x] Al resolverse, el cliente inserta la fila de `DELIBERACIONES` (resultado, bloqueada, motivo, comprobaciones_json, usuario local); `Esc` anota además `deliberacion_cancelada`
- [x] El detalle de una entrada `deliberacion_*` en la vista Registro muestra sus comprobaciones leídas de `DELIBERACIONES`
- [x] `config.toml`: `[deliberacion] backup_horas = 24, salud_max_min = 5, limite_seg = 2, motivo_min = 10`
- [x] Tests de las tres comprobaciones (sondeo NOMINAL/CARGA/viejo, backup reciente/viejo/vacío, tests `true`/`false`/`sleep 3`) y del consenso

## Snippet al conectar y atajos de Flota
- [x] Ficha de host: «Snippet al conectar» en el bloque «Al conectar», desplegable limitado a snippets no críticos y sin variables; guarda `HOSTS.snippet_al_conectar_id`
- [x] Al abrir o reconectar una pestaña a ese host, el cliente envía el comando como `comando_inicial`; nunca delibera
- [x] `[flota.atajos]` en `config.toml` asigna teclas a nombres de snippet sobre el host seleccionado en Flota; teclas ya usadas por Flota o snippets inexistentes se ignoran con aviso al arrancar; los atajos aparecen en la barra inferior
- [x] Un atajo de Flota sigue el mismo flujo que `snippet · <nombre> · <host>` (variables y deliberación si procede)

## Navegación y ayuda
- [x] `F8` operativo; ayuda `?` en Snippets, Resultados y el diálogo MAGI
- [x] Ninguna acción destructiva (borrar snippet, cancelar ejecución en curso, ejecutar con deliberación) sin confirmación

## Calidad
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde con los tests nuevos de 3b, almacén, snippets, ejecuciones y deliberación
- [ ] Revisión adversarial del código nuevo (servidor y cliente) antes de cerrar, con resultado en el informe de implementación
- [x] README actualizado: snippets, variables, ejecución, Resultados, deliberación MAGI, verificaciones por host, atajos de Flota

---

**Progreso Fase 6:** 79 / 80 funcionalidades

**Total MAGI (Fases 1-6):** 487 / 490 funcionalidades
