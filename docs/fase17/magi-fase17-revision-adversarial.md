# MAGI — Fase 17: revisión adversarial de cierre

**Fecha:** 18 de noviembre de 2026.
**Alcance:** todo el código nuevo de la Fase 17 (`3dea674..8064ef1`, la rama `fase17` con los tres sprints integrados), servidor y cliente por separado, más el núcleo compartido, el empaquetado y que el escritorio y la app Android no cambien. Se buscaron pérdidas de datos, fugas de recursos (procesos, hilos, descriptores, avisos, wake lock), esperas sin plazo, estado leído a destiempo y entrada no fiable, como manda `CLAUDE.md`, y con atención a lo que cada sprint no pudo ver: las interacciones entre piezas hechas por pistas distintas y los arreglos de las rondas de corrección.

## Método

- **Siete revisores adversariales de solo lectura**, cada uno con su zona: el servidor (wake lock, avisos, vigilante, estado de la barra, cierre); el llavero con frase maestra (núcleo, interfaz y CLI); el cliente de Termux (órdenes, portapapeles, enlaces, barra, diagnóstico, rutas); la escritura de `termux.properties` y la fila de teclas; la sincronización en Termux; el empaquetado, la CI, TUR y las guías; y «el escritorio y la app no cambian» más la integración entre sprints. Ejecutaron las pruebas de su zona sobre cachés de compilación propias, y varios escribieron fuzz y programas mínimos fuera del repositorio (borrados al terminar).
- **Un verificador independiente por cada hallazgo de severidad media o alta**, con la orden de refutarlo leyendo el código real, comprobando si ya había una guarda o una prueba y si ya estaba aceptado como riesgo. De los 8 hallazgos medios o altos, los verificadores **confirmaron 7** (y rebajaron la severidad de 4 de ellos) y **refutaron 1 como defecto** (Termux:API apagada por un plazo vencido: es lo que manda §7.2; queda una aclaración).
- **Revisiones anteriores de la fase**, ya con sus correcciones y regresiones: una revisión por pista en cada sprint (4 en el Sprint 1, 3 en el Sprint 2 y 3 en el Sprint 3, más sus rondas de corrección). Esta revisión de cierre no las repite: busca lo que se les escapó.
- Tras la revisión, **una ronda de correcciones en seis pistas** con una prueba de regresión por hallazgo (comprobada neutralizando el arreglo a mano), y la verificación completa: `fmt`, `clippy`, `deny`, `shellcheck`, `actionlint`, `cargo test --workspace --locked --no-fail-fast`, compilación para `aarch64-linux-android` y comprobación de `x86_64-linux-android`.

## Hallazgos y resolución

Severidad: la que dejó el verificador (entre paréntesis, la del revisor si cambió). **Ninguno de severidad alta** tras verificar.

### Servidor

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Media (alta) | Una pestaña en estado `Caida` (reconexión agotada, `reconectar` desactivado o sesión soltada por otro equipo) se conserva hasta que el usuario la cierra y contaba como trabajo: el wake lock quedaba tomado toda la noche y el vigilante sin ventanas seguía sondeando hosts por SSH. | **Corregido**: `EstadoServidor::trabajo_vivo()` (sin las pestañas caídas; una reconexión en curso, `Abriendo` con `reconectando`, sigue contando) alimenta el wake lock; el vigilante sin ventanas se suprime solo cuando lo único que mantiene vivo al servidor son pestañas caídas. `TrabajoServidor`, el protocolo y cuándo se apaga el servidor no cambian. Pruebas con una pestaña caída, reconectando y con ventanas. |
| Baja (media) | Un `OSC 9;4;estado;porcentaje` (progreso de ConEmu/Windows Terminal) entraba como aviso «4 · 1 · 50» y publicaba un aviso de Android. | **Corregido solo en el servidor** (`atencion_remota`): se ignoran los avisos de forma `NN` o `NN · …`; el emulador del núcleo y el escritorio no se tocan (el aviso se sigue enseñando en la pestaña). Límite conocido: un aviso real que empiece por 1-2 dígitos y « · » tampoco marca atención. |
| Baja (media) | Un solo `termux-notification` que venza el plazo apaga Termux:API en ese servidor: los avisos publicados no se retiran (ni al terminar) y no habrá más hasta reiniciarlo. | **Documentado [analista]**: es lo que manda §7.2; se corrigió el comentario del código y se anota en §6 del informe de implementación. Los retira la purga de arranque del siguiente servidor. |
| Baja | `termux-wake-lock` que vence el plazo quitaba la marca aunque pudo tomarse (los avisos ya conservaban el id). | **Corregido**: con `SinRespuesta` se conserva la marca y el wake lock se trata como incierto (se suelta tras la gracia y al terminar). |
| Baja | `barra.quitar()` se ejecutaba después de borrar el lock: un servidor nuevo podía ver borrado el fichero `servidor` que acababa de escribir. | **Corregido**: antes de borrar el socket y el lock; prueba con un único `inotify` sobre el orden de los borrados. |

### Cliente: llavero

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Media | La sincronización leía la clave de la caché del llavero de fichero sin mirar si otra ventana lo había reescrito: con dos ventanas, una exportaba con la clave rotada vieja (y dejaba grabado su hash) y un «desactivar» en otra ventana resucitaba el fichero. Peor de lo anotado en la desviación 9. | **Corregido en lo que pasa fuera del hilo de UI**: `LlaveroFichero::refrescar()` (y método por defecto vacío en `LlaveroCliente`) relee bajo `flock` solo si cambió el fichero; se llama antes de cada pasada y antes de exportar al salir; si la frase ya no abre, la pasada no exporta. **Pendiente [analista]**: las operaciones de equipos (`rotar`, `quitar`, `mostrar clave`, `crear`, `unir`, `desactivar`) corren en el hilo de UI y siguen leyendo la memoria; una rotación simultánea en dos ventanas puede guardar como «anterior» la clave equivocada. |
| Baja (media) | La purga de la clave de sincronización anterior caducada solo se intentaba al construir la App, con el llavero siempre cerrado: la clave vieja quedaba en `llavero.age` para siempre. | **Corregido**: la purga se repite al desbloquear. |
| Baja | El análisis de la cabecera de `age` es cuadrático en el número de stanzas: un fichero forjado con miles de stanzas tardaba minutos sin gastar scrypt. | **Corregido**: solo se parsean los primeros 4 096 bytes; prueba con 20 000 stanzas. |
| Baja | Un enlace simbólico roto en `llavero.age` dejaba la TUI en un bucle CREAR → «otra ventana lo creó» → «ya no existe» → CREAR. | **Corregido**: un enlace roto es «dañado» (se ofrece apartarlo) en `estado`, `desbloquear`, `guardar`/`recuperar`, la CLI y el diagnóstico. |
| Baja | SERVIDOR CAÍDO y los diálogos de SERVIDOR ANTERIOR descartaban sin cancelar las continuaciones de un diálogo del llavero que no contestaban al servidor. | **Corregido**: se apila el diálogo del llavero y se cancelan las continuaciones. |
| Baja | Al cancelar con Esc el diálogo CREAR (o tras apartar un llavero dañado), las continuaciones decían «llavero bloqueado: desbloquéalo (Ctrl+P)», sin nada que desbloquear. | **Corregido**: con el llavero sin crear o dañado, el texto es «sin llavero no se puede guardar…». |

### Cliente: órdenes, portapapeles, diagnóstico y barra

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Baja | Al salir de la TUI, soltar el `Runtime` esperaba a que acabaran las órdenes de Termux en `spawn_blocking`: el proceso no terminaba hasta agotar su plazo. | **Corregido** (`apagar_runtime` con `shutdown_timeout` de 500 ms en la salida normal). Quedan los retornos con `?` de `ejecutar` (errores de terminal), que siguen soltando el runtime sin plazo. |
| Baja | `usuario_del_sistema()` no usaba `termux-<modelo>` como usuario de las deliberaciones (§3.2, §7.8). | **Corregido** en Termux; el escritorio, igual. |
| Baja | Una orden de la API muerta por una señal ajena (`codigo None`, R166) apaga Termux:API hasta reiniciar. | **Documentado [analista]**: §7.2 manda apagar con «código distinto de 0»; comentario en el código y anotado como riesgo. |
| Baja | `magi diagnostico` daba por bueno (`✓`) un `llavero.age` dañado. | **Corregido**: valida la cabecera sin scrypt ni frase (y un enlace roto) y avisa. |
| Baja | La copia de la clave pública recién generada pasó, en el escritorio, de la tarea tokio al hilo de UI (hasta 750 ms). | **Corregido**: vuelve a la tarea de `identidades::generar` en el escritorio (en Termux sigue siendo de `App::copiar`). La clave pública y el enlace del visor siguen en el hilo de UI como desde las Fases 2 y 12. |
| Baja | `magi diagnostico` toca `~/storage/shared` y la carpeta de sincronización con `metadata` y `read_dir` síncronos y sin plazo. | **Documentado**: con el almacenamiento colgado el diagnóstico espera (la TUI sí protege esa carpeta). |

### `termux.properties` y fila de teclas

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Baja (media) | La lectura, la copia y la escritura iban en `spawn_blocking` sin plazo y el diálogo se tragaba todas las teclas: con `termux.properties` enlazado a un almacenamiento que no responde, la TUI quedaba atascada en «instalando…». | **Corregido**: Esc cierra el diálogo aunque esté instalando (el resultado tardío se atiende por id), con `std::thread`; no se abre otro diálogo mientras tanto. |
| Baja | Si `termux.properties` es un enlace, el diálogo decía «se escribe en ~/.termux/…» y la copia iba junto al enlace, pero la escritura iba al destino canónico, con `chmod` estricto. | **Corregido**: se enseña «es un enlace a …», la copia va junto al destino y la escritura es tolerante a `chmod`. |
| Baja | Claves que Java lee como `extra-keys` (`extra\-keys`, `\extra-keys`, `extra-keys\` al final del fichero) no se reconocían: MAGI anotaba «instalada» y Termux seguía leyendo la fila del usuario. | **Corregido**: se decodifican los escapes de la clave y, como cinturón, tras fusionar se vuelve a analizar el resultado y, si no es la fila de MAGI, no se escribe. |
| Baja | La propuesta del primer arranque abría el diálogo sobre cualquier vista y un ↵ tecleado para otra cosa podía instalar sin leerlo. | **Corregido**: el diálogo del primer arranque ignora ↵ durante 500 ms. |
| Baja | «Ya es la de MAGI» comparaba quitando todo el espacio Unicode, también dentro de los literales. | **Corregido**: solo se quita el espacio ASCII que separa, fuera de comillas. |

### Sincronización

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Media | El listado de la pasada, de `unir` y de `hay_ficheros` solo filtraba por nombre y enlace: un directorio, un FIFO o un fichero disperso de GiB con nombre de MAGI tumbaba la pasada o se leía entero (con SHA-256) en el hilo de UI. Código de la F10; la exposición es nueva (almacenamiento compartido). | **Corregido** (código compartido, resultado idéntico para ficheros normales): solo ficheros normales, lectura con tope, lo demasiado grande se trata como dañado sin leerlo y un error en una entrada no tumba el listado (constancia en los avisos). Pruebas de directorio, FIFO, disperso e ilegible. |
| Baja | Una pasada aplazada por carpeta colgada perdía su disparador y pisaba estados `DecisionPendiente`, `EsperandoClave` o `Quitado`. | **Corregido**: se guarda con su política y se reintenta en el siguiente `tick`; no se pisan esos estados. `exportar_al_salir` no cubre la pasada aplazada (leería la carpeta sin plazo al cerrar): se exporta en el siguiente arranque. |
| Baja | Un fichero de un equipo conocido leído a medias se interpretaba como «no se abre con la clave» y abría UNIR ESTE EQUIPO. | **Corregido en el sondeo**: la firma tiene que ser igual en dos muestras consecutivas antes de pedir la pasada; la pasada del núcleo no cambia. |
| Baja | El nombre del equipo (`termux-<modelo>`) no se puede cambiar desde ninguna interfaz (en el escritorio tampoco), aunque §3.2 y §7.8 dicen «editable». | **Documentado [analista]**. |

### Empaquetado, CI y documentación

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Media | `publicar` identificaba la release solo por el nombre de la etiqueta: si se recreaba una etiqueta mientras una ejecución vieja esperaba (la guía de `publicar.md` §7 lo permite), subía el `.deb` compilado desde el commit anterior. | **Corregido**: `verificar` resuelve la etiqueta a SHA, `compilar` compila ese commit y `publicar` compara con la etiqueta actual justo antes de subir y falla si difieren; la guía manda cancelar la ejecución vieja y prefiere «Re-run failed jobs». |
| Baja | La atestación registra `GITHUB_SHA`, que en un `workflow_dispatch --ref main` es la punta de `main`, no el árbol de la etiqueta compilada. | **Documentado** (GitHub fija esa fuente); el árbol compilado ya sale del SHA de la etiqueta. Mejora pendiente: un job propio de atestación. |
| Baja | La guía no explicaba las vías de recuperación reales (un fallo del script con `dpkg-deb` real en la etiqueta estable solo se arregla con un PATCH; el dispatch no sirve). | **Corregido** (§5 y §9 de `publicar-termux.md`): la primera publicación de Termux debe ser una candidata `-rc.N`. |
| Baja | `git diff --name-only` aplicaba la detección de renombrados en el filtro del job `termux`. | **Corregido**: `--no-renames`. |
| Baja | `publicar.md` no sabía del `.deb` (canales, §5 paso 5, §9). | **Corregido**. |

### El escritorio y la app no cambian; integración

| Grav. | Hallazgo | Resolución |
|---|---|---|
| Baja | `magi sondear` aborta entero sin TTY si un solo host usa `contrasena:llavero` (el `LlaveroFichero` nace cerrado en cada proceso). | **Documentado**: es lo que manda §4.2 («CLI que lo necesita… sin TTY, error con instrucción»); anotado en la desviación 14. |
| Baja | `Cargo.lock` cambiaba también `tempfile` (`getrandom` 0.4.3 → 0.3.4). | **Anotado**: efecto de `cargo update -p libz-sys` (RUSTSEC-2026-0544), no de una dependencia nueva. |

## Verificado correcto (sin hallazgo)

- **Servidor:** con `OpcionesTermux = None` el servidor del escritorio es idéntico (todos los ganchos van tras `if let Some(termux)`); el fichero `servidor` se escribe antes del `bind` en el único punto de arranque y no lo pisa un segundo servidor que no consigue el lock; ninguna orden `termux-*` se espera con el mutex global ni con un `std::sync::Mutex` a través de un `.await` (los cerrojos `publicados`, `estado`, la barra y la máquina del wake lock no se toman en orden inverso); el cierre marca `terminando` antes de esperar y lleva plazo de la orden más 3 s; `termux-wake-lock` y `termux-wake-unlock` no son de la API y nunca la apagan; sin `unwrap`, `expect`, indexado por rango ni `panic!` fuera de pruebas; nada del inventario, de la salida de un programa ni del remoto llega al log, a REGISTRO ni a un aviso; los cinco ganchos que limpian `atencion_remota` (cierre, foco, teclas, escritura en varios y reconexión) están cubiertos.
- **Llavero:** fuzz de 4 000 ficheros mutados (bits, truncados, factores forjados `0`, `255`, `99999999999`, `-1`) y de contenidos `age` válidos con JSON raro (30 000 entradas, 100 000 niveles de anidado, BOM, NUL, UTF-8 inválido): sin pánico ni esperas de más de 5 s; concurrencia aleatoria con 4 hilos y 4 objetos sobre la misma ruta: no se pierde ninguna clave reconocida; `estado()` lee 4 096 bytes y `recuperar()` no toma `operacion`; no se puede perder un llavero válido sin una acción explícita (`apartar_danado` renombra y recomprueba, `borrar` rechaza un dañado); la frase no llega a `Debug`, errores, logs ni REGISTRO; el llavero del sistema y el de memoria quedan idénticos; las continuaciones no corren dos veces.
- **Cliente:** cada orden va en su grupo de procesos, se mata el grupo entero al vencer el plazo con el líder aún sin recoger, y 300 copias de 512 KiB seguidas no dejaron descriptores ni hilos; un `Io` no apaga la API; `magi diagnostico` con `termux-battery-status` colgado y SIGINT sale con 130, mata al nieto y no deja procesos; el OSC 52 solo lo escribe el hilo de UI y el tope se mide sobre el base64; ni el texto copiado ni las URLs llegan a `tracing`; `es_web` exige `http(s)://`, así que ningún argumento empieza por `-`; la lectura del fichero `servidor` es hostil-segura; todas las rutas salen del entorno.
- **`termux.properties`:** unos 8 millones de casos aleatorios contra `java.util.Properties` real (OpenJDK 26): ninguna divergencia fuera de las categorías raras que acabaron en los hallazgos; la escritura nunca ocurre sin confirmar, la copia va antes y con `create_new`, y si falla no se escribe nada; el REGISTRO se anota solo tras el efecto; el diálogo lleva fijados ruta, prefijo y fecha.
- **Sincronización:** sin pérdida por listado parcial, vacío o carpeta que desaparece (el borrado sale solo de lápidas); el hash guardado es el de los bytes realmente importados; el sondeo con lectura colgada, hilo que no se puede crear y receptor soltado se comporta; el núcleo y el móvil no deciden por la plataforma; `formato-sincronizacion.md` y `formato.rs` no cambian.
- **Empaquetado:** `shellcheck` y `actionlint` limpios; el script rechaza versiones hostiles (`../x`, `1.6.0/../../x`, `;id`, `-rc.0`, `v1.6.0`, salto de línea final…) y binarios que piden `libc++_shared.so` o no son aarch64, y nunca ejecuta el binario; el paso `verificar` con etiquetas hostiles no escribe nada en `GITHUB_OUTPUT`; todas las expresiones `${{ }}` van en `env:`, `with:` o `group:`, nunca en un `run:`; el único secreto es `GITHUB_TOKEN`; solo `compilar` tiene `id-token` y `attestations: write` y solo `publicar` tiene `contents: write`; `android-v*` no dispara `termux-release.yml`; no hay rutas con el usuario real ni instrucciones que pidan al agente etiquetar, subir o abrir PR.
- **Escritorio y app:** `VERSION_PROTOCOLO` sigue en 10, `user_version` en 11, el formato de sincronización y `android/` no cambian; las 62 instantáneas de la fase son todas altas, ninguna modificada; ninguna aserción preexistente se ha debilitado; `assets/` coincide con la CLI.

## Lo que no se pudo revisar ni verificar

- **El `.deb` real:** no hay `dpkg-deb` en esta máquina; el script se revisó con un doble y entradas hostiles, pero quedan sin ver los avisos de `dpkg-deb` por `Section` o `Priority`, el formato exacto de `--contents` y la reproducibilidad.
- **Los workflows en GitHub:** nunca se han ejecutado (`GITHUB_SHA` en etiquetas anotadas, la instalación del NDK r30 en `ubuntu-22.04`, el tiempo del job, `gh release upload`, la atestación, la espera a la release de `dist`).
- **Todo lo que depende de un móvil:** Termux:API real, el almacenamiento compartido (`chmod`, `realpath`, `O_NONBLOCK` sobre FUSE), `Metadata::created()` en bionic, los eventos de foco (`FOCO_FIABLE`), el tope real de OSC 52, el SDK real por `__system_property_get`, la entrega de SIGHUP, el teclado táctil y Android cerrando procesos. Va al guion de validación del informe de implementación (§7).
- **La rama `FOCO_FIABLE = true`** solo tiene prueba en la lógica pura (la constante no se puede cambiar desde una prueba).
- **La receta de TUR** no se ha compilado en TUR.

## Resultado

Sin pérdida de datos ni de secretos en lo revisado. Los 32 hallazgos (8 medios o altos según el revisor, de los que el verificador confirmó 7 y rebajó 4; 24 bajos) quedan así: **26 corregidos** con prueba de regresión (uno de ellos, la caché del llavero entre ventanas, de forma parcial: las operaciones de equipos —`rotar`, `quitar`, `mostrar clave`, `crear`, `unir`, `desactivar`— corren en el hilo de UI y siguen leyendo la memoria del llavero, así que una rotación simultánea en dos ventanas puede guardar como «anterior» la clave equivocada: **[analista]**) y **6 documentados** como decisión del analista o riesgo aceptado: Termux:API apagada por un solo plazo vencido, una orden de la API muerta por una señal ajena, el diagnóstico sin plazo sobre el almacenamiento compartido, el nombre del equipo no editable, la atestación de un `workflow_dispatch` y `magi sondear` sin TTY. Las decisiones para el analista están marcadas **[analista]** en §2 y §6 del informe de implementación.
