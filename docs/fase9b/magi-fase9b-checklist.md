# MAGI - Checklist Fase 9b: Primera Publicación

## Corrección 10b: sincronización
- [x] `magi sincronizar` pregunta al servidor por el socket, como `magi servidor estado`, antes de aplicar borrados de hosts y túneles (R83): sin servidor, los aplica; con el servidor de esta versión, aplaza los de hosts con trabajo vivo; con otra versión o sin respuesta en 5 s, aplaza todos los de hosts y túneles; nunca arranca el servidor
  - AC: Dado un borrado de web-01 que llega por la sincronización con una pestaña abierta en web-01, cuando se ejecuta `magi sincronizar`, entonces el borrado queda aplazado, la salida dice por qué y el código de salida es 0
  - **Precisión (21 nov 2026, D427, D428):** pregunta por un camino propio (`Listar` tras la bienvenida, sin el segundo de espera de `estado_cli`), solo si hay clave y fuera del cerrojo; los aplazados van a CONFLICTOS y se aplican con `a`: la pasada del escritorio no los reintenta
- [x] El cambio de un elemento que viaja y su versión nueva se escriben en la misma transacción del almacén, en todas las tablas que viajan (R84)
  - AC: Dado un fallo provocado entre el cambio y la versión, cuando se reabre la base, entonces no hay ningún cambio sin su versión
- [x] Al descifrar el fichero de otro equipo, solo «ninguna identidad coincide» pide la clave; un fichero truncado, a medio copiar o dañado da el aviso «a medio copiar o dañado», no para la pasada y se reintenta en la siguiente, igual en el escritorio y en la app (núcleo) (R85)
  - AC: Dado un fichero cortado por la mitad en la carpeta, cuando se sincroniza, entonces se importan los demás ficheros, sale el aviso con el nombre del fichero y no se pide ninguna clave
- [x] Con la clave anterior, durante los 30 días tras una rotación, solo se abren ficheros de equipos conocidos y no quitados; el de un equipo desconocido se rechaza con aviso y una línea en el log sin su contenido (R97)
- [x] Pruebas de la 10b en el núcleo (fichero truncado, otra clave, clave anterior con equipo conocido y desconocido, transacción) y de `magi sincronizar` sin servidor, con servidor de prueba con y sin trabajo vivo y con uno de otra versión

## Cierre de la F16
- [x] Las subórdenes sin PTY de `terminal_tmux.sh` reciben el script por la entrada estándar del `exec`, leído entero antes de ejecutar nada; en la línea de órdenes solo van un arranque mínimo y los argumentos validados (D357)
  - AC: Dado el tmux falso, cuando se crea, se comprueba o se consulta una sesión, entonces la línea de órdenes que llega al host no contiene el script
  - **Precisión (21 nov 2026, D429):** el script acaba en `#MAGI-FIN`; si no llega la marca, el arranque sale con 70 sin ejecutar nada
- [x] `enganchar` partido en una preparación por la entrada estándar (comprobar, configuración y `@magi_equipo`) y un enganche mínimo con PTY en `terminal_enganche.sh`, de pocos KB (D357)
  - **Precisión (21 nov 2026, D429):** `@magi_equipo` lo pone el enganche, después de la guarda, y no la preparación, que devuelve también la hora de creación de la sesión
- [x] Los demás scripts embebidos, medidos frente al límite de la línea de órdenes de Dropbear y anotados en el informe de implementación, sin cambiarlos
  - **Precisión (21 nov 2026, D436):** `servicios.sh` ya rebasa el límite (37 977 B; 39 016 escapado) y `crear` mete la carpeta y el comando en la línea de órdenes (R189, R190)
- [x] La reconexión de una pestaña persistente se engancha en el mismo paso solo si la sesión está libre o es de este equipo, con su propio código de salida; tras un renombrado, la pestaña soltada sabe qué equipo la tiene (R157)
  - AC: Dado que otro equipo se engancha entre la comprobación y el enganche, cuando la reconexión termina, entonces la pestaña queda soltada y el otro equipo conserva la sesión
  - **Precisión (21 nov 2026, D429):** sale con 101 si otro equipo tiene la sesión, y la sesión se sigue por su identificador y su hora de creación
- [x] Protocolo v10 con campos opcionales: la causa `no_existe` en `AperturaTmuxFallida` (D349), `creada` en `InfoSesion.tmux` (D369) y el aviso de una sesión que se crea pero no llega a abrirse
  - AC: Dadas dos ventanas del mismo equipo y un host marcado, cuando una abre una pestaña sobre una sesión que ya existía, entonces ninguna de las dos guarda una ficha `origen = host` (R163)
- [x] Aviso de atención sin la lista vieja: al conectar la primera ventana, el vigilante hace una pasada inmediata y difunde la lista aunque no cambie, y el cliente no avisa con una comprobación anterior a su conexión (D368)
  - **Precisión (21 nov 2026, D430):** la pasada inmediata vale para toda ventana con lista guardada, no solo la primera, y el umbral del cliente lleva 2 s de margen
- [x] Retoques de D370: `r` y «reconectar» sobre una pestaña soltada piden la confirmación de `↵` (R164); el título igual al nombre del host no se enseña; los textos de la sincronización solo salen con ella configurada; cerrar terminando refresca la lista; tras un fallo de apertura por la carpeta o el nombre, el diálogo vuelve con lo escrito
- [x] El aviso de una pestaña (F15) se limpia cuando alguna ventana la mira, no solo con `Foco(true)`, y `almacen::sesiones_persistentes::listar` da un solo aviso por fila ilegible
- [x] `nombre_valido` del script con una lista explícita de caracteres (R159), y la importación valida la marca en un host que no es `ssh` y `creada_por` como el CRUD
- [x] Prueba de `escribir_tuneles` y de las demás escrituras de la sincronización por si pierden el nombre como las fichas (R160); si se confirma, corregido y anotado como desviación
- [x] Etiquetas legibles de `sesion_persistente` y de los cuatro tipos de la F16 en CONFLICTOS y REGISTRO, y la línea del CHANGELOG sobre el aviso de la migración (D352)
- [x] La ayuda (`?`) y el manual explican el caso del shell del host que arranca tmux por su cuenta (D367)
- [x] `tmux_real` y `terminal_e2e` sin temporales sueltos y, con tmux real, también la vista previa, renombrar, el historial y el tmux habitual

## Repositorio público
- [x] Las referencias funcionales a `4d3-org` pasan a `HectorEspejo`: `Cargo.toml` (los tres paquetes), `README.md`, `SECURITY.md`, los dos CHANGELOG, `dist-workspace.toml` y `release.yml` (tap `HectorEspejo/homebrew-tap`), `publicar-aur.yml`, `android-release.yml`, `packaging/aur/PKGBUILD`, `docs/publicar.md`, `docs/publicar-android.md`, `CLAUDE.md` y `AGENTS.md`; `docs/faseN/` no se toca
  - AC: Dado el árbol, cuando corre `repositorio_publico`, entonces no hay ningún `4d3-org` fuera de `docs/fase*/`, el maestro y `docs/README.md`, y `dist plan --tag v1.5.1` y `makepkg --printsrcinfo` pasan
  - **Precisión (21 nov 2026, D425, D431):** con la 1.6.1, `dist plan --tag v1.6.1`; cambian también cuatro ficheros de la F17 que el ítem no nombra (`docs/publicar-termux.md`, `packaging/termux/build.sh`, `scripts/paquete-termux.sh` y `tests/termux_paquete.rs`), y el mantenedor de la receta de TUR pasa a `@HectorEspejo`
- [x] El correo de mantenedor del PKGBUILD y el de los commits a AUR (`publicar-aur.yml`) es `hola@4d3.org`
- [x] Ficheros actuales sin datos personales: la ruta personal del desarrollador pasa a `/home/usuario` en los informes de implementación de la F9 y la F11, y nada más en el árbol tiene IP públicas reales, rutas personales ni correos personales
- [x] Prueba `repositorio_publico` en CI: ninguna IP pública fuera de los rangos de documentación (salvo las citas de especificaciones y `vendor/`), ninguna ruta `/home/<nombre>` fuera de su lista, ningún correo fuera de su lista y ningún `4d3-org` fuera del archivo de diseño (`docs/fase*/` y el maestro) y de `docs/README.md`, con cada excepción y su motivo en la prueba
  - AC: Dado un fichero nuevo con `HostName` y una IP pública, cuando corre la prueba, entonces falla y dice el fichero, la línea y el motivo
- [x] `gitleaks` sobre todo el historial, una vez, con el resultado en el informe de implementación (un resumen, sin copiar lo que encuentre); si encuentra un secreto real, se para y se avisa para revocarlo (el historial no se reescribe)
  - **Precisión (21 nov 2026, D434):** 14 hallazgos, todos en ficheros de pruebas (11 `age-secret-key` de fixtures y 3 `private-key` del servidor de pruebas y del certificado de RDP); ninguno parece real, lo confirma Hector, y `docs/publicar.md` §2 cuenta el resultado en vez de esperar «no leaks found»
- [x] `docs/README.md` explica qué hay en `docs/`: el manual, la publicación, el formato de sincronización y el archivo de diseño (`docs/faseN/`, con la especificación, el checklist y el informe de implementación de cada fase, y por qué dice `4d3-org`, con el aviso de que esas direcciones ya no valen)
- [x] `CONTRIBUTING.md` y `CONTRIBUTING.en.md`: compilar y probar, convenciones (castellano en el código y la interfaz), commits, sin acuerdo de cesión (lo que entra lo hace con MIT OR Apache-2.0), propuesta antes de los cambios grandes y agentes de código (`CLAUDE.md`, `AGENTS.md`)
- [x] Plantillas de issue en `.github/ISSUE_TEMPLATE/`: `fallo.yml` (versión, sistema y terminal, qué pasó, pasos, qué se esperaba, `magi diagnostico` y la casilla de datos privados), `propuesta.yml` y `config.yml` (sin issues en blanco y con el enlace al aviso privado), en castellano con el inglés debajo
- [x] `.github/pull_request_template.md`: pruebas, línea en «Sin publicar», manual al día y sin datos reales, en castellano e inglés
- [x] `SECURITY.md` al día: se mantiene la 1.5.x, el aviso privado de GitHub en `HectorEspejo/magi`, el alcance y un párrafo en inglés de cómo avisar
  - **Precisión (21 nov 2026, D425):** con la 1.6.1, la versión que se mantiene es la 1.6.x

## README, manual y demo
- [x] `README.md` corto en castellano: qué es, la animación, el estado (Linux de uso diario, macOS en pruebas, app Android en beta, Termux en la 1.6.0), la instalación por canal con su verificación, los primeros pasos y los enlaces al manual, `CONTRIBUTING`, `SECURITY` y la licencia
  - AC: Dado el README, cuando se cuentan sus líneas, entonces no pasan de 200 y todos los encabezados del README antiguo están en el manual
  - **Precisión (21 nov 2026, D426, D433):** Termux sale ya en esta release, «en pruebas»; el README tiene 102 líneas y el AC se cumple con tres excepciones deliberadas (`Desarrollo` y `Tests` en `CONTRIBUTING`, `Licencia` en el README), que `repositorio_publico` comprueba con esa lista
- [x] `README.en.md` con el mismo contenido en inglés, y el enlace al otro idioma al principio de los dos
- [x] El manual en `docs/manual/`: el contenido del README antiguo, movido sin reescribir, en los capítulos de §5.1 del informe, con su índice (`docs/manual/README.md`) y los enlaces internos arreglados
  - **Precisión (21 nov 2026, D433):** con `termux.md` (D401) además de los capítulos de §5.1; el reparto deja 19 cambios comprobables (13 enlaces y 6 frases de versión), listados por su script
- [x] `docs/manual/estado.md`: qué se ha probado a mano y qué no, por plataforma y por fase, con la fecha de cada validación
- [x] Las referencias al README antiguo apuntan al manual: `CLAUDE.md`, el JSON de `magi sesiones`, la ayuda y los documentos de publicación
- [x] `repositorio_publico` comprueba también los enlaces relativos y las anclas de los dos README, `CONTRIBUTING*.md`, `SECURITY.md`, `docs/README.md`, `docs/manual/` y `docs/publicar*.md`, sin red
- [x] `assets/demo/magi.tape` (VHS), `assets/demo/inventario` con tres hosts de nombres inventados contra el servidor de pruebas del repositorio y `scripts/grabar-demo.sh`, que lo arranca en un entorno aislado y graba la animación
- [x] `assets/demo/magi.gif` versionado, de 2 MiB como mucho, con texto alternativo en los dos README y sin ningún dato real

## Versiones y CHANGELOG
- [x] Versión 1.5.1 del escritorio: `Cargo.toml`, `FECHA_DE_VERSION`, la DA2 de `tui_prueba` (`10501`), las instantáneas `servidor_anterior_*` y `assets/` regenerado
  - **Precisión (21 nov 2026, D425):** la versión es la **1.6.1** y la DA2, `10601`: `main` ya traía la F17 en la 1.6.0 y las versiones solo van hacia delante (T149)
- [x] `CHANGELOG.md`: «Sin publicar» y la 1.0.0, que nunca salió, se juntan en `[1.5.1]`, con el resumen en castellano e inglés y una lista corta por áreas delante del detalle; incluye las correcciones de esta fase y las tres hechas fuera del método (copia con wl-copy, pegado y foco, age y Flota en Android)
  - AC: Dado el CHANGELOG, cuando `dist` arma las notas de `v1.5.1`, entonces salen el resumen bilingüe y el detalle, y no queda ninguna entrada de una versión que no se publicó
  - **Precisión (21 nov 2026, D425):** la entrada es `[1.6.1]`, y `dist` arma las notas de `v1.6.1`
- [x] `android/CHANGELOG.md`: «Sin publicar», la 0.2.0 y la 0.1.0 se juntan en `[0.2.0]`, la primera publicada, con su resumen bilingüe y las correcciones de la 10b que afectan a la app
- [x] Desde esta fase, cada versión de los CHANGELOG empieza con un resumen de una o dos líneas en castellano e inglés, y el detalle sigue en castellano (regla en `CLAUDE.md`)

## Guías y pasos de publicación
- [x] `docs/publicar.md` §4, reescrito para la primera publicación desde `HectorEspejo/magi`: sin transferencia (ya es público), comprobación de nombres en AUR y de quién es `4d3-org` en GitHub (R182), tap `HectorEspejo/homebrew-tap`, cuenta de AUR, secretos, ajustes del repositorio y orden de las etiquetas, con el ensayo ya hecho
- [x] `docs/publicar-android.md` al día: keystore y sus cuatro secretos, `version.properties` a `0.2.0-rc.1` y luego a `0.2.0`, la candidata instalada con Obtainium en el plegable y los versionCode 2001 y 2009
- [x] Los ajustes del repositorio de §5.5 del informe, descritos para el desarrollador: avisos privados, Dependabot de seguridad sin `dependabot.yml`, regla de `main` con los siete jobs de `ci.yml` obligatorios y sin `push --force` ni borrado, borrar las ramas al fusionar, issues, descripción y temas
  - **Precisión (21 nov 2026, D432):** son ocho checks de `ci.yml`: `tests-linux` es una matriz y da dos, y los dos condicionales de la F17 (`cambios-termux` y `termux`) no se exigen; `docs/publicar.md` §4 los lista con su nombre
- [x] Guion corto de macOS (5 pasos) en el informe de implementación, con la regla de D395: si falla algo grave, los objetivos de macOS salen de esta release y vuelven en una PATCH
- [x] Guion corto de la versión candidata de la app (5 pasos) en el informe de implementación
- [x] `docs/publicar.md` §9 con las comprobaciones posteriores desde `HectorEspejo`: `yay -S magi-ssh-bin`, `brew install hectorespejo/tap/magi-ssh`, el tarball con `sha256sum -c` y `gh attestation verify --repo HectorEspejo/magi`, y el APK con `apksigner verify --print-certs`

## Publicación (la hace el desarrollador)
- [ ] Ajustes del repositorio aplicados, con los secretos `HOMEBREW_TAP_TOKEN`, `AUR_SSH_PRIVATE_KEY` y los cuatro del keystore creados
  - **Precisión (22 nov 2026, D450 de la 9c):** el keystore, sin «Cuatro de Tres» en el `-dname`; si el que hay lo lleva, se crea otro
- [ ] `v1.5.1` publicada: GitHub Release «Latest» con los cuatro tarballs, `.sha256` y atestaciones, fórmula en el tap y `magi-ssh-bin` en AUR
  - **Precisión (21 nov 2026, D425, D426):** será `v1.6.1`, con la candidata `v1.6.1-rc.1` antes y el `.deb` de Termux en las dos
- [ ] `android-v0.2.0-rc.1` probada en el plegable y `android-v0.2.0` publicada, nunca como «Latest»
  - **Precisión (22 nov 2026, D450 de la 9c):** espera a la 9c, que cambia el `applicationId` a `io.github.hectorespejo.magi`

## API: CLI, protocolo y workflows
- [x] `magi sincronizar` y `magi sincronizar estado` con los borrados aplazados y su motivo (R83)
- [x] `AperturaTmuxFallida{causa: no_existe}` y el aviso de una sesión creada que no llega a abrirse
- [x] `InfoSesion.tmux.creada`, opcional dentro de v10
- [x] `release.yml` y `dist-workspace.toml` con el tap `HectorEspejo/homebrew-tap`, y `dist plan --tag v1.5.1` en verde
  - **Precisión (21 nov 2026, D425):** `dist plan --tag v1.6.1`
- [x] `publicar-aur.yml` y `packaging/aur/PKGBUILD` con `HectorEspejo/magi` y `hola@4d3.org`, y `makepkg --printsrcinfo` en verde
- [x] `android-release.yml` con la verificación `gh attestation verify … --repo HectorEspejo/magi`

## Calidad
- [x] Todo en verde: `TZ=UTC cargo test --workspace --locked`, las pruebas JVM de Android, `fmt`, `clippy`, `cargo deny`, `actionlint` y `shellcheck`; las instantáneas de redimensionado, sin cambios salvo las de los retoques de D370
  - **Precisión (21 nov 2026, D430):** las instantáneas cambian también por las etiquetas legibles (`conflictos_*`) y por la versión (`servidor_anterior_*`); la suite pasó entera el 20 nov (84 binarios y 3 280 pruebas) y `repositorio_publico`, con el GIF, el 21; falta la CI de GitHub, con la PR
- [x] Revisión adversarial de cierre (T31, T54) de las correcciones de la sincronización, el cierre de la F16 y la publicación (workflows, referencias y datos), con su informe en `docs/fase9b/magi-fase9b-revision-adversarial.md`
- [x] El informe de implementación lista los pasos del desarrollador en orden, con lo que ya esté hecho marcado

---

**Progreso Fase 9b:** 55 / 58 funcionalidades

**Total MAGI (Fases 1-17, 9b y 9c):** 1432 / 1520 funcionalidades
