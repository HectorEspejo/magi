# MAGI - Checklist Fase 9b: Primera Publicación

## Corrección 10b: sincronización
- [ ] `magi sincronizar` pregunta al servidor por el socket, como `magi servidor estado`, antes de aplicar borrados de hosts y túneles (R83): sin servidor, los aplica; con el servidor de esta versión, aplaza los de hosts con trabajo vivo; con otra versión o sin respuesta en 5 s, aplaza todos los de hosts y túneles; nunca arranca el servidor
  - AC: Dado un borrado de web-01 que llega por la sincronización con una pestaña abierta en web-01, cuando se ejecuta `magi sincronizar`, entonces el borrado queda aplazado, la salida dice por qué y el código de salida es 0
- [ ] El cambio de un elemento que viaja y su versión nueva se escriben en la misma transacción del almacén, en todas las tablas que viajan (R84)
  - AC: Dado un fallo provocado entre el cambio y la versión, cuando se reabre la base, entonces no hay ningún cambio sin su versión
- [ ] Al descifrar el fichero de otro equipo, solo «ninguna identidad coincide» pide la clave; un fichero truncado, a medio copiar o dañado da el aviso «a medio copiar o dañado», no para la pasada y se reintenta en la siguiente, igual en el escritorio y en la app (núcleo) (R85)
  - AC: Dado un fichero cortado por la mitad en la carpeta, cuando se sincroniza, entonces se importan los demás ficheros, sale el aviso con el nombre del fichero y no se pide ninguna clave
- [ ] Con la clave anterior, durante los 30 días tras una rotación, solo se abren ficheros de equipos conocidos y no quitados; el de un equipo desconocido se rechaza con aviso y una línea en el log sin su contenido (R97)
- [ ] Pruebas de la 10b en el núcleo (fichero truncado, otra clave, clave anterior con equipo conocido y desconocido, transacción) y de `magi sincronizar` sin servidor, con servidor de prueba con y sin trabajo vivo y con uno de otra versión

## Cierre de la F16
- [ ] Las subórdenes sin PTY de `terminal_tmux.sh` reciben el script por la entrada estándar del `exec`, leído entero antes de ejecutar nada; en la línea de órdenes solo van un arranque mínimo y los argumentos validados (D357)
  - AC: Dado el tmux falso, cuando se crea, se comprueba o se consulta una sesión, entonces la línea de órdenes que llega al host no contiene el script
- [ ] `enganchar` partido en una preparación por la entrada estándar (comprobar, configuración y `@magi_equipo`) y un enganche mínimo con PTY en `terminal_enganche.sh`, de pocos KB (D357)
- [ ] Los demás scripts embebidos, medidos frente al límite de la línea de órdenes de Dropbear y anotados en el informe de implementación, sin cambiarlos
- [ ] La reconexión de una pestaña persistente se engancha en el mismo paso solo si la sesión está libre o es de este equipo, con su propio código de salida; tras un renombrado, la pestaña soltada sabe qué equipo la tiene (R157)
  - AC: Dado que otro equipo se engancha entre la comprobación y el enganche, cuando la reconexión termina, entonces la pestaña queda soltada y el otro equipo conserva la sesión
- [ ] Protocolo v10 con campos opcionales: la causa `no_existe` en `AperturaTmuxFallida` (D349), `creada` en `InfoSesion.tmux` (D369) y el aviso de una sesión que se crea pero no llega a abrirse
  - AC: Dadas dos ventanas del mismo equipo y un host marcado, cuando una abre una pestaña sobre una sesión que ya existía, entonces ninguna de las dos guarda una ficha `origen = host` (R163)
- [ ] Aviso de atención sin la lista vieja: al conectar la primera ventana, el vigilante hace una pasada inmediata y difunde la lista aunque no cambie, y el cliente no avisa con una comprobación anterior a su conexión (D368)
- [ ] Retoques de D370: `r` y «reconectar» sobre una pestaña soltada piden la confirmación de `↵` (R164); el título igual al nombre del host no se enseña; los textos de la sincronización solo salen con ella configurada; cerrar terminando refresca la lista; tras un fallo de apertura por la carpeta o el nombre, el diálogo vuelve con lo escrito
- [ ] El aviso de una pestaña (F15) se limpia cuando alguna ventana la mira, no solo con `Foco(true)`, y `almacen::sesiones_persistentes::listar` da un solo aviso por fila ilegible
- [ ] `nombre_valido` del script con una lista explícita de caracteres (R159), y la importación valida la marca en un host que no es `ssh` y `creada_por` como el CRUD
- [ ] Prueba de `escribir_tuneles` y de las demás escrituras de la sincronización por si pierden el nombre como las fichas (R160); si se confirma, corregido y anotado como desviación
- [ ] Etiquetas legibles de `sesion_persistente` y de los cuatro tipos de la F16 en CONFLICTOS y REGISTRO, y la línea del CHANGELOG sobre el aviso de la migración (D352)
- [ ] La ayuda (`?`) y el manual explican el caso del shell del host que arranca tmux por su cuenta (D367)
- [ ] `tmux_real` y `terminal_e2e` sin temporales sueltos y, con tmux real, también la vista previa, renombrar, el historial y el tmux habitual

## Repositorio público
- [ ] Las referencias funcionales a `4d3-org` pasan a `HectorEspejo`: `Cargo.toml` (los tres paquetes), `README.md`, `SECURITY.md`, los dos CHANGELOG, `dist-workspace.toml` y `release.yml` (tap `HectorEspejo/homebrew-tap`), `publicar-aur.yml`, `android-release.yml`, `packaging/aur/PKGBUILD`, `docs/publicar.md`, `docs/publicar-android.md`, `CLAUDE.md` y `AGENTS.md`; `docs/faseN/` no se toca
  - AC: Dado el árbol, cuando corre `repositorio_publico`, entonces no hay ningún `4d3-org` fuera de `docs/fase*/`, el maestro y `docs/README.md`, y `dist plan --tag v1.5.1` y `makepkg --printsrcinfo` pasan
- [ ] El correo de mantenedor del PKGBUILD y el de los commits a AUR (`publicar-aur.yml`) es `hola@4d3.org`
- [ ] Ficheros actuales sin datos personales: la ruta personal del desarrollador pasa a `/home/usuario` en los informes de implementación de la F9 y la F11, y nada más en el árbol tiene IP públicas reales, rutas personales ni correos personales
- [ ] Prueba `repositorio_publico` en CI: ninguna IP pública fuera de los rangos de documentación (salvo las citas de especificaciones y `vendor/`), ninguna ruta `/home/<nombre>` fuera de su lista, ningún correo fuera de su lista y ningún `4d3-org` fuera del archivo de diseño (`docs/fase*/` y el maestro) y de `docs/README.md`, con cada excepción y su motivo en la prueba
  - AC: Dado un fichero nuevo con `HostName` y una IP pública, cuando corre la prueba, entonces falla y dice el fichero, la línea y el motivo
- [ ] `gitleaks` sobre todo el historial, una vez, con el resultado en el informe de implementación (un resumen, sin copiar lo que encuentre); si encuentra un secreto real, se para y se avisa para revocarlo (el historial no se reescribe)
- [ ] `docs/README.md` explica qué hay en `docs/`: el manual, la publicación, el formato de sincronización y el archivo de diseño (`docs/faseN/`, con la especificación, el checklist y el informe de implementación de cada fase, y por qué dice `4d3-org`, con el aviso de que esas direcciones ya no valen)
- [ ] `CONTRIBUTING.md` y `CONTRIBUTING.en.md`: compilar y probar, convenciones (castellano en el código y la interfaz), commits, sin acuerdo de cesión (lo que entra lo hace con MIT OR Apache-2.0), propuesta antes de los cambios grandes y agentes de código (`CLAUDE.md`, `AGENTS.md`)
- [ ] Plantillas de issue en `.github/ISSUE_TEMPLATE/`: `fallo.yml` (versión, sistema y terminal, qué pasó, pasos, qué se esperaba, `magi diagnostico` y la casilla de datos privados), `propuesta.yml` y `config.yml` (sin issues en blanco y con el enlace al aviso privado), en castellano con el inglés debajo
- [ ] `.github/pull_request_template.md`: pruebas, línea en «Sin publicar», manual al día y sin datos reales, en castellano e inglés
- [ ] `SECURITY.md` al día: se mantiene la 1.5.x, el aviso privado de GitHub en `HectorEspejo/magi`, el alcance y un párrafo en inglés de cómo avisar

## README, manual y demo
- [ ] `README.md` corto en castellano: qué es, la animación, el estado (Linux de uso diario, macOS en pruebas, app Android en beta, Termux en la 1.6.0), la instalación por canal con su verificación, los primeros pasos y los enlaces al manual, `CONTRIBUTING`, `SECURITY` y la licencia
  - AC: Dado el README, cuando se cuentan sus líneas, entonces no pasan de 200 y todos los encabezados del README antiguo están en el manual
- [ ] `README.en.md` con el mismo contenido en inglés, y el enlace al otro idioma al principio de los dos
- [ ] El manual en `docs/manual/`: el contenido del README antiguo, movido sin reescribir, en los capítulos de §5.1 del informe, con su índice (`docs/manual/README.md`) y los enlaces internos arreglados
- [ ] `docs/manual/estado.md`: qué se ha probado a mano y qué no, por plataforma y por fase, con la fecha de cada validación
- [ ] Las referencias al README antiguo apuntan al manual: `CLAUDE.md`, el JSON de `magi sesiones`, la ayuda y los documentos de publicación
- [ ] `repositorio_publico` comprueba también los enlaces relativos y las anclas de los dos README, `CONTRIBUTING*.md`, `SECURITY.md`, `docs/README.md`, `docs/manual/` y `docs/publicar*.md`, sin red
- [ ] `assets/demo/magi.tape` (VHS), `assets/demo/inventario` con tres hosts de nombres inventados contra el servidor de pruebas del repositorio y `scripts/grabar-demo.sh`, que lo arranca en un entorno aislado y graba la animación
- [ ] `assets/demo/magi.gif` versionado, de 2 MiB como mucho, con texto alternativo en los dos README y sin ningún dato real

## Versiones y CHANGELOG
- [ ] Versión 1.5.1 del escritorio: `Cargo.toml`, `FECHA_DE_VERSION`, la DA2 de `tui_prueba` (`10501`), las instantáneas `servidor_anterior_*` y `assets/` regenerado
- [ ] `CHANGELOG.md`: «Sin publicar» y la 1.0.0, que nunca salió, se juntan en `[1.5.1]`, con el resumen en castellano e inglés y una lista corta por áreas delante del detalle; incluye las correcciones de esta fase y las tres hechas fuera del método (copia con wl-copy, pegado y foco, age y Flota en Android)
  - AC: Dado el CHANGELOG, cuando `dist` arma las notas de `v1.5.1`, entonces salen el resumen bilingüe y el detalle, y no queda ninguna entrada de una versión que no se publicó
- [ ] `android/CHANGELOG.md`: «Sin publicar», la 0.2.0 y la 0.1.0 se juntan en `[0.2.0]`, la primera publicada, con su resumen bilingüe y las correcciones de la 10b que afectan a la app
- [ ] Desde esta fase, cada versión de los CHANGELOG empieza con un resumen de una o dos líneas en castellano e inglés, y el detalle sigue en castellano (regla en `CLAUDE.md`)

## Guías y pasos de publicación
- [ ] `docs/publicar.md` §4, reescrito para la primera publicación desde `HectorEspejo/magi`: sin transferencia (ya es público), comprobación de nombres en AUR y de quién es `4d3-org` en GitHub (R182), tap `HectorEspejo/homebrew-tap`, cuenta de AUR, secretos, ajustes del repositorio y orden de las etiquetas, con el ensayo ya hecho
- [ ] `docs/publicar-android.md` al día: keystore y sus cuatro secretos, `version.properties` a `0.2.0-rc.1` y luego a `0.2.0`, la candidata instalada con Obtainium en el plegable y los versionCode 2001 y 2009
- [ ] Los ajustes del repositorio de §5.5 del informe, descritos para el desarrollador: avisos privados, Dependabot de seguridad sin `dependabot.yml`, regla de `main` con los siete jobs de `ci.yml` obligatorios y sin `push --force` ni borrado, borrar las ramas al fusionar, issues, descripción y temas
- [ ] Guion corto de macOS (5 pasos) en el informe de implementación, con la regla de D395: si falla algo grave, los objetivos de macOS salen de esta release y vuelven en una PATCH
- [ ] Guion corto de la versión candidata de la app (5 pasos) en el informe de implementación
- [ ] `docs/publicar.md` §9 con las comprobaciones posteriores desde `HectorEspejo`: `yay -S magi-ssh-bin`, `brew install hectorespejo/tap/magi-ssh`, el tarball con `sha256sum -c` y `gh attestation verify --repo HectorEspejo/magi`, y el APK con `apksigner verify --print-certs`

## Publicación (la hace el desarrollador)
- [ ] Ajustes del repositorio aplicados, con los secretos `HOMEBREW_TAP_TOKEN`, `AUR_SSH_PRIVATE_KEY` y los cuatro del keystore creados
- [ ] `v1.5.1` publicada: GitHub Release «Latest» con los cuatro tarballs, `.sha256` y atestaciones, fórmula en el tap y `magi-ssh-bin` en AUR
- [ ] `android-v0.2.0-rc.1` probada en el plegable y `android-v0.2.0` publicada, nunca como «Latest»

## API: CLI, protocolo y workflows
- [ ] `magi sincronizar` y `magi sincronizar estado` con los borrados aplazados y su motivo (R83)
- [ ] `AperturaTmuxFallida{causa: no_existe}` y el aviso de una sesión creada que no llega a abrirse
- [ ] `InfoSesion.tmux.creada`, opcional dentro de v10
- [ ] `release.yml` y `dist-workspace.toml` con el tap `HectorEspejo/homebrew-tap`, y `dist plan --tag v1.5.1` en verde
- [ ] `publicar-aur.yml` y `packaging/aur/PKGBUILD` con `HectorEspejo/magi` y `hola@4d3.org`, y `makepkg --printsrcinfo` en verde
- [ ] `android-release.yml` con la verificación `gh attestation verify … --repo HectorEspejo/magi`

## Calidad
- [ ] Todo en verde: `TZ=UTC cargo test --workspace --locked`, las pruebas JVM de Android, `fmt`, `clippy`, `cargo deny`, `actionlint` y `shellcheck`; las instantáneas de redimensionado, sin cambios salvo las de los retoques de D370
- [ ] Revisión adversarial de cierre (T31, T54) de las correcciones de la sincronización, el cierre de la F16 y la publicación (workflows, referencias y datos), con su informe en `docs/fase9b/magi-fase9b-revision-adversarial.md`
- [ ] El informe de implementación lista los pasos del desarrollador en orden, con lo que ya esté hecho marcado

---

**Progreso Fase 9b:** 0 / 58 funcionalidades

**Total MAGI (Fases 1-17 y 9b):** 1311 / 1447 funcionalidades
