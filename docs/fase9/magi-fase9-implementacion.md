# MAGI — Fase 9: Empaquetado y Releases — Informe de implementación

**Fecha de inicio:** 30 de octubre de 2026
**Fecha de cierre:** 30 de octubre de 2026
**Rama:** fase9-empaquetado
**Estado:** implementación completa (54 / 54 funcionalidades del checklist); las verificaciones que exigen una release publicada quedan para el desarrollador (docs/publicar.md).

## Resumen de lo implementado

La Fase 9 convierte MAGI en un programa instalable y publicado como software libre. Todo lo que depende del agente está hecho, probado y documentado; lo irreversible (hacer público el repositorio, crear cuentas y secretos, etiquetar versiones) lo hace el desarrollador siguiendo docs/publicar.md.

- **Paquete.** Cargo.toml pasa a magi-ssh 1.0.0, licencia MIT OR Apache-2.0, publish = false, metadatos completos, [lib] magi y [[bin]] magi, rust-version = "1.86" (MSRV real de las dependencias fijadas) y perfil dist con LTO fino y strip. El comando, la biblioteca, las rutas de datos y el socket siguen llamándose magi; ninguna ruta deriva de CARGO_PKG_NAME. build.rs captura el commit corto de git para magi --version, que imprime «magi 1.0.0 (protocolo 6, <commit>)» y «desconocido» si se compila sin repositorio (verificado compilando una exportación sin .git).
- **Protocolo v6 y actualización segura.** VersionIncompatible gana version_app, clientes y trabajo{sesiones, transferencias, tuneles, ejecuciones}; Bienvenida gana version_app; Hola y VersionIncompatible quedan como contrato estable documentado y fijado con tests de forma JSON. El cliente lee la primera respuesta como JSON genérico, así entiende el rechazo de un servidor v5 (sin los campos nuevos) y el de uno futuro. Decisión: servidor más nuevo → aviso persistente en la barra y nunca se para; sin trabajo ni otras ventanas → SIGTERM, espera de 5 s a que libere socket y lock, autolanzado del servidor nuevo y aviso en la barra; con trabajo, otras ventanas o recuento desconocido → diálogo SERVIDOR DE SESIONES ANTERIOR con p (y confirmación), Esc y q. Si no libera en 5 s, no se lanza un segundo servidor y el diálogo lo dice con el pid.
- **Base de datos.** Una magi.db con user_version mayor que la que conoce el binario no se abre ni se toca: la comprobación ocurre antes de WAL y de los permisos, en cliente y servidor (el servidor abre la base de datos antes que el socket para no dejar un socket huérfano). magi.db sigue en user_version 6, sin tablas nuevas.
- **CLI e instalación.** magi diagnostico comprueba versión/protocolo/commit, rutas y permisos, longitud del socket, config.toml, magi.db en solo lectura, servidor por socket (sin arrancarlo), agente SSH, llavero, portapapeles, editor, paginador, terminal e Include de magi_config; imprime los signos de MAGI (correcto/aviso/error, ASCII ok/!/x), degrada sin Unicode cuando toca y no escribe, no arranca el servidor, no lee secretos ni abre red. magi autocompletado bash|zsh|fish y magi man generan los recursos, versionados en assets/ junto con magi.desktop e icono original SVG+PNG; un test de frescura falla si no coinciden con el binario.
- **Integración continua y releases.** .github/workflows/ci.yml (fmt, clippy, tests Linux en 22.04 y 24.04 con sftp-server, macOS build+clippy+cargo test --lib, cargo deny, plantilla AUR en contenedor Arch con makepkg --printsrcinfo y namcap; contents: read por defecto). dist-workspace.toml + release.yml (dist 0.33.1): cuatro objetivos (Linux x86_64/aarch64 en Ubuntu 22.04, macOS arm64/x86_64), .tar.xz con binario, README, CHANGELOG, licencias y assets/, SHA-256, atestaciones de GitHub, notas del CHANGELOG y fórmula magi-ssh en 4d3-org/homebrew-tap; prereleases marcadas como tales y sin tocar canales; un tag que no coincide con Cargo.toml falla en plan antes de compilar (verificado con dist plan).
- **AUR.** packaging/aur/ con la plantilla PKGBUILD de magi-ssh-bin (4d3-org/magi, sumas por arquitectura, provides/conflicts, dependencias y opcionales, /usr/bin/magi, autocompletado bash/zsh/fish, man, .desktop, icono hicolor y licencias), magi-ssh-bin.install y generar.sh; .github/workflows/publicar-aur.yml reutilizable que rellena el PKGBUILD, genera .SRCINFO, pasa namcap y empuja a AUR con AUR_SSH_PRIVATE_KEY, solo en versiones estables.
- **Publicación y auditoría.** LICENSE-MIT, LICENSE-APACHE, CHANGELOG.md (Keep a Changelog; «Sin publicar» + 1.0.0), SECURITY.md, README ampliado (instalación AUR/Homebrew/tarball, verificación sha256sum y gh attestation verify, magi diagnostico, actualización con servidor anterior, desinstalación y qué datos quedan, licencia) y docs/publicar.md (política de versiones, primera publicación, flujo de release, fallo de canal, rotación de secretos y reparto agente/desarrollador). Auditoría con gitleaks de todo el historial sin hallazgos y saneamiento de datos reales en documentación, código, pruebas e instantáneas.
- **Calidad.** Tests nuevos de contrato JSON, lectura de rechazos v5/v6, decisión ante el servidor anterior, espera de 5 s sin liberación, rechazo de base de datos futura (cliente y servidor), diagnóstico simulado, frescura de recursos y nueve instantáneas del diálogo (40×12, 80×24 y 200×60). TZ=UTC cargo test --locked, cargo clippy --all-targets -- -D warnings, cargo fmt --check, cargo deny check, actionlint, dist plan, makepkg --printsrcinfo, desktop-file-validate y gitleaks en verde. Revisión adversarial de código y cadena de suministro realizada (resultado en su sección).

### Revisión adversarial (fase 9)

Revisión adversarial hecha en dos frentes —código de servidor/cliente y cadena de suministro/CI/empaquetado— con verificación independiente de cada hallazgo antes de corregirlo (reproducción con test o comando, o traza exacta del camino de código).

**Hallazgos confirmados y corregidos, cada uno con su comprobación:**

1. **Carrera de SIGTERM al arrancar el servidor.** El manejador de SIGTERM se instalaba después de crear el socket: entre que el socket ya era conectable y el bucle select, un SIGTERM mataba el proceso con la acción por defecto (reproducido por tests/pool.rs::sigterm_para_el_servidor_limpio bajo carga). Se registra antes del lock y del socket; el test se dejó cinco veces en verde.
2. **Un servidor más nuevo podía pararse desde este binario.** Dos vías: magi servidor parar [--si] mandaba SIGTERM a un servidor de protocolo mayor, y varios mensajes de «operación sin servidor» invitaban a pararlo con «magi servidor parar». Ahora la CLI se niega con «actualiza MAGI» sin señal (test de regresión con un servidor falso de protocolo 99 y un proceso víctima que no debe morir) y los mensajes distinguen «actualiza magi» de «magi servidor parar» (test unitario de motivo_sin_servidor).
3. **Socket huérfano con una base de datos futura.** El servidor creaba lock y socket antes de abrir magi.db; si la base de datos era de una versión futura, salía con error pero dejaba el socket. Ahora abre la base de datos (y valida su versión) antes del bind; test de integración comprueba salida con error, mensaje exacto, fichero intacto y ausencia de socket.
4. **El workflow de AUR recibía todos los secretos del repositorio** (secrets: inherit), incluido el token del tap de Homebrew. Ahora el workflow declara solo AUR_SSH_PRIVATE_KEY y release.yml le pasa solo ese secreto; actionlint valida la llamada.
5. **ssh-keyscan de AUR era vulnerable a un MITM.** Ahora se comprueban las tres huellas publicadas por AUR (ED25519, ECDSA y RSA) y el job se para si alguna clave escaneada no está publicada; el procedimiento ante rotación está en docs/publicar.md.
6. **La página man remitía a páginas inexistentes.** clap_mangen referenciaba magi-importar(1), etc., que no se instalan; se reescriben como «magi importar». Recursos regenerados y test de frescura en verde.

**Revisado sin hallazgos:** contrato JSON de Hola y VersionIncompatible y lectura genérica del rechazo v5/v6 (tests fijan la forma); decisión sustituir/preguntar/más-nuevo (unitarios y e2e); recuentos de trabajo y ventanas bajo el mutex del servidor (sin esperas largas dentro); rechazo de base de datos futura antes de WAL y permisos; magi diagnostico sin escrituras (test con XDG temporales y comprobación manual), sin red y sin secretos, despachado antes del log; permisos mínimos de los workflows y validación del tag antes de usarlo en run:; prereleases que no tocan canales; tarballs con los ficheros exigidos y sus sumas; PKGBUILD con las rutas de §5.4 y las sumas de la release; deny.toml con la única excepción justificada y fechada; gitleaks limpio; datos reales saneados.

**Dudas aceptadas (no bloquean):** el pid del rechazo se prefiere al de peer_cred porque lo fija la especificación (§7.2), con la ventana teórica de reutilización de pid entre el rechazo y el SIGTERM; las acciones de GitHub van fijadas por etiqueta (como genera dist) y no por SHA; la fórmula de Homebrew se commitea al tap con la identidad por defecto de dist; magi --help conserva los rótulos en inglés de clap (el man y los textos propios sí están en castellano); namcap solo se ejecuta en el contenedor Arch de CI.

## Desviaciones respecto a la especificación (qué y por qué)

1. **allow-dirty = ["ci"] y release.yml retocado a mano.** dist 0.33.1 no permite declarar permisos por job; se generó el workflow con dist init (en una copia temporal) y se ajustó a mano: contents: read global, contents: write/atestaciones solo en los jobs que publican y AUR. Se añadió además la validación previa del formato del tag en plan, que también cierra una inyección de github.ref_name en un run:. allow-dirty evita que dist sobrescriba esos retoques.
2. **dist = true en dist-workspace.toml.** Cargo.toml tiene publish = false (D116) y dist ignora el paquete sin forzarlo; funciona desde el workspace y no se tocó Cargo.toml para ello.
3. **Runners de compilación.** Linux: ubuntu-22.04 y ubuntu-22.04-arm (existe como runner alojado para repositorios públicos desde 2025; glibc 2.35, que es lo que exige el checklist). macOS: macos-15 (arm64) y macos-15-intel (x86_64) en vez de macos-14, que figura como deprecado en actions/runner-images; los objetivos publicados son los cuatro del checklist.
4. **magi servidor estado conserva las tablas de detalle.** Tras la nueva cabecera (versión de app, protocolo, pid, aviso si no coincide y recuento de trabajo) se siguen imprimiendo las tablas de sesiones, cola, túneles y ejecuciones que ya existían: la especificación pinta solo el resumen, pero quitar información útil para depurar parecía peor.
5. **CHANGELOG 1.0.0 incluye los añadidos visibles de la Fase 9.** El checklist pide que la entrada resuma las Fases 1-8; 1.0.0 también publica diagnostico, autocompletado, man, --version con commit y la sustitución del servidor anterior, y omitirlos dejaría el CHANGELOG incompleto.
6. **Saneamiento amplio de datos de ejemplo.** Además de la IP real y las rutas/usuarios personales, se sustituyeron todas las IPs privadas de fixtures y documentos por rangos de documentación (192.0.2.x) y los usuarios de ejemplo por «usuario», tal como pide el checklist; las 10.9.0.x sintéticas del test de SOCKS5/barrido pasaron también a 192.0.2.x.
7. **--version y -V.** --version imprime la versión larga con protocolo y commit (lo pedido); -V mantiene la forma corta de clap («magi 1.0.0»), comportamiento estándar de clap.
8. **Fecha fija de la página man.** clap_mangen pondría una fecha de generación variable; se fija a la fecha de la versión (2026-10-30) y se quita el long_version al generar el .1 para que el test de frescura no dependa del commit ni del día de compilación.
9. **Excepción de RustSec en deny.toml.** RUSTSEC-2023-0071 (rsa 0.10.0-rc.18, sin parche a la fecha) es dependencia transitiva obligatoria de russh 0.63/ssh-key; se ignora con motivo y fecha (MAGI usa RSA solo como cliente para firmar, nunca descifra datos de terceros). Los avisos con parche (crypto-bigint, ctutils, tokio) se resolvieron actualizando el lock sin cambiar Cargo.toml.
10. **namcap no se ejecutó en local.** No está instalado y no hay permisos de contenedor en la máquina; el resto del job AUR sí se reprodujo en local (generación con versión ficticia, makepkg --printsrcinfo como usuario no root y construcción real del paquete con un tarball simulado). namcap queda cubierto por el job aur-plantilla de ci.yml.
11. **Verificaciones que exigen una release publicada.** Las AC de v1.0.0 → release con cuatro tarballs y atestaciones, yay -S magi-ssh-bin y brew install 4d3-org/tap/magi-ssh no se pueden ejecutar sin publicar; los workflows están validados con dist plan, actionlint y la construcción local del paquete. Quedan en «Pendientes».
12. **Auditoría de secretos.** gitleaks no se añadió a CI: la especificación lo pide como auditoría única antes de publicar (y opcional en CI). Se ejecutó sobre todo el historial (más de 50 commits) sin hallazgos.

## Estructura de archivos creada/modificada

Metadatos, licencias y documentación:
- Cargo.toml, Cargo.lock, build.rs (nuevo)
- LICENSE-MIT, LICENSE-APACHE, CHANGELOG.md, SECURITY.md, README.md, .gitignore
- docs/publicar.md (nuevo); documentación histórica saneada (docs/fase1, fase2, fase4, fase5, fase6, fase7, fase8)

CLI, diagnóstico y recursos:
- src/main.rs, src/lib.rs, src/diagnostico.rs (nuevo)
- scripts/generar-recursos.sh (nuevo)
- assets/completions (magi.bash, _magi, magi.fish), assets/man/magi.1, assets/magi.desktop, assets/iconos (magi.svg, magi-256.png) (nuevos)
- tests/recursos.rs, tests/diagnostico.rs (nuevos)

Protocolo, servidor y actualización:
- src/protocolo.rs, src/cliente/mod.rs, src/servidor/mod.rs, src/app/servidor_anterior.rs, src/app.rs, src/ui/servidor_anterior.rs (nuevo), src/ui (mod, barra, dialogos, disposicion), src/almacen (mod, migraciones)
- tests/protocolo.rs, tests/servidor_anterior.rs, tests/servidor.rs, tests/cliente.rs, tests/almacen.rs, tests/redimensionado.rs, tests/redimensionado (vistas_l, arnes)
- tests/snapshots/servidor_anterior_*.snap (9 nuevas) y el resto de instantáneas actualizadas por el saneamiento de datos y el protocolo 6.

CI, releases y canales:
- .github/workflows (ci, release, publicar-aur) (nuevos)
- deny.toml, dist-workspace.toml (nuevos)
- packaging/aur (PKGBUILD, magi-ssh-bin.install, generar.sh) (nuevos)

Cierre de fase:
- docs/fase9/magi-fase9-checklist.md (marcado), docs/fase9/magi-fase9-implementacion.md (este informe)

## Decisiones técnicas tomadas durante el desarrollo

1. **Commit en --version con build.rs** (git en tiempo de compilación, fallback «desconocido»); no se toca CARGO_PKG_VERSION ni se añade dependencia.
2. **Saludo genérico y contrato estable.** La primera respuesta se parsea a serde_json::Value y solo después se decodifica; RechazoVersion guarda los campos opcionales. Un campo de trabajo ausente vale 1 (hay trabajo) y un trabajo o clientes ausente deja la decisión en «preguntar»: nunca se para por una lectura optimista.
3. **Sustitución sin pérdida.** El cliente envía SIGTERM (ESRCH = ya parado), espera 5 s a que muera el proceso, el lock quede libre y el socket deje de atender; solo entonces lanza el servidor nuevo y reconecta. En el arranque se hace antes de pintar (bloqueo máximo 5 s); desde el diálogo va en una tarea y vuelve por Evento::Sustitucion, así la UI nunca se bloquea.
4. **Carrera de SIGTERM corregida.** El manejador de SIGTERM se registra antes de crear el lock y el socket; antes había una ventana (socket ya conectable, handler aún sin instalar) en la que un SIGTERM mataba el servidor con la acción por defecto. Se reprodujo con tests/pool.rs::sigterm_para_el_servidor_limpio bajo carga y se dejó el test cinco veces en verde.
5. **El servidor abre la base de datos antes del socket.** Una magi.db futura hace salir al servidor con error sin dejar socket huérfano; el lock se toma antes de abrir para que no haya dos servidores migrando a la vez.
6. **Rechazo de base de datos futura antes de tocar nada.** migraciones::comprobar_version se ejecuta antes de WAL y de set_permissions en Almacen::abrir; un test compara el contenido del fichero antes y después.
7. **Diálogo como máquina de estados pura + Hoja.** DialogoServidorAnterior{Aviso,Confirmacion,Sustituyendo,Error} fija el rechazo al abrirse (no relee el servidor al confirmar); el pintado reutiliza Hoja (encoge, desplaza y degrada a ASCII), con mínimo 40×12 y modo estrecho declarados en disposicion.rs.
8. **Aviso persistente del servidor más nuevo.** La barra lo muestra mientras el servidor siga siendo de otra versión; no caduca como un mensaje normal, porque no hay nada que el usuario pueda hacer desde este binario salvo actualizar.
9. **Diagnóstico simulable.** Datos (hechos observados) + comprobar (interpretación pura) permiten probar sin portapapeles, con base de datos futura, con NO_COLOR/ASCII o con el servidor apagado sin tocar la máquina ni escribir nada.
10. **Recursos generados y deterministas.** clap_complete/clap_mangen en el binario; assets/ versionado y regenerable con scripts/generar-recursos.sh; man con fecha fija y sin commit; test de frescura que ejecuta el binario de pruebas (CARGO_BIN_EXE_magi).
11. **CI con herramientas fijadas.** cargo-deny 0.20.2 descargado con SHA-256 fijada; actionlint para validar localmente; contenedor archlinux:base-devel para makepkg/namcap; MAGI_SFTP_SERVER para que los tests encuentren sftp-server en cualquier runner.
12. **dist con cuatro objetivos nativos.** Sin compilación cruzada: Ubuntu 22.04 x86_64/arm64 (glibc 2.35) y macOS 15 arm64/Intel; include explícito de README, CHANGELOG, licencias y assets/; atestaciones de GitHub en vez de claves propias (D117); Homebrew por tap con token de grano fino; AUR por workflow reutilizable con clave SSH dedicada.
13. **Un solo paquete AUR, magi-ssh-bin.** Reempaqueta el tarball atestado (D114); generar.sh rechaza versiones con guion (AUR no admite prereleases) y el workflow lo vuelve a comprobar.
14. **Saneamiento de datos con mapa único.** /home/hector → /home/usuario, «hector» → «usuario», IP real → 192.0.2.45 y todas las IPs de fixture a rangos de documentación; se regeneraron las instantáneas una sola vez y se revisó el diff.
15. **Nunca se para un servidor más nuevo.** Ni desde la TUI (sin diálogo ni opción), ni desde `magi servidor parar` (se niega incluso con `--si`: podría custodiar trabajo que esta versión no entiende), ni desde las operaciones que necesitan servidor, cuyos mensajes ahora distinguen «actualiza magi» de «magi servidor parar» según la versión del servidor. Test de regresión con un servidor falso de protocolo 99 y un proceso víctima que no debe recibir la señal.

## Funcionalidades del checklist completadas (copiando su texto exacto)

[x] `Cargo.toml` con `[package] name = "magi-ssh"`, `[lib] name = "magi"` y `[[bin]] name = "magi"`: el paquete se llama `magi-ssh` y el comando sigue siendo `magi`

[x] `version = "1.0.0"` y metadatos completos en `Cargo.toml`: `description`, `license = "MIT OR Apache-2.0"`, `repository`, `homepage`, `readme`, `keywords`, `categories`, `rust-version` y `publish = false`

[x] `magi --version` muestra versión, protocolo y commit (`magi 1.0.0 (protocolo 6, abc1234)`); compilado sin git, dice «commit desconocido»

[x] `LICENSE-MIT` y `LICENSE-APACHE` con titular «Cuatro de Tres S. Coop. And.», y sección de licencia en el README

[x] `CHANGELOG.md` en castellano con formato Keep a Changelog: sección «Sin publicar» y entrada 1.0.0 que resume las Fases 1-8

[x] `SECURITY.md`: las vulnerabilidades se reportan por el aviso privado de GitHub, con versiones soportadas (la última MINOR)

[x] Política de versiones (MAJOR, MINOR, PATCH; §7.1 del informe) escrita en `docs/publicar.md`

[x] `VERSION_PROTOCOLO = 6`: `VersionIncompatible` gana `version_app`, `clientes` y `trabajo{sesiones, transferencias, tuneles, ejecuciones}`; `Bienvenida` gana `version_app`

[x] `Hola` y `VersionIncompatible` quedan como contrato estable: nunca se quitan ni se renombran sus campos, solo se añaden opcionales (comentario en `protocolo.rs` y test que fija su forma JSON)

[x] El cliente lee la primera respuesta del servidor como JSON genérico: un `VersionIncompatible` de un servidor v5 (sin `trabajo` ni `clientes`) se trata como «recuento desconocido»

[x] Servidor anterior que no custodia nada (`trabajo` todo a cero y `clientes = 0`): el cliente le envía `SIGTERM`, espera hasta 5 s a que libere socket y lock, arranca el servidor nuevo y lo dice en la barra, sin diálogo

[x] Servidor anterior con trabajo, con otras ventanas o con recuento desconocido: diálogo SERVIDOR DE SESIONES ANTERIOR con versiones, pid y lo que custodia; `p` lo para tras confirmar («cerrará 2 sesiones y 1 túnel»), `Esc` sigue sin sesiones, `q` sale; nunca se para solo

[x] Servidor más nuevo que el binario (protocolo mayor): aviso «este magi es más antiguo que su servidor; actualiza magi» sin opción de pararlo desde ese binario

[x] Si el servidor anterior no libera el socket en 5 s tras `SIGTERM`, el diálogo lo dice con su pid y no se lanza un segundo servidor

[x] `magi servidor estado` muestra la versión de la app del servidor y avisa si no coincide con la del binario

[x] Una `magi.db` con `user_version` mayor que la que conoce el binario no se abre: mensaje «esta base de datos es de una versión más nueva de MAGI; actualiza MAGI», código de salida ≠ 0 y el fichero intacto (cliente y servidor)

[x] `magi diagnostico` comprueba versión y protocolo, rutas de datos, configuración, estado y runtime con sus permisos, longitud de la ruta del socket (R41), `config.toml`, `magi.db` (en solo lectura) y servidor de sesiones (en marcha, versión, trabajo)

[x] `magi diagnostico` comprueba agente SSH (`SSH_AUTH_SOCK` y claves cargadas), llavero (`secret-tool` en Linux, `security` en macOS), portapapeles (`wl-copy` o `pbcopy`), editor, paginador, comando de terminal para nueva ventana e `Include` de `magi_config` en `~/.ssh/config`

[x] Cada comprobación se imprime como `✓` correcto, `!` aviso o `✕` error con una instrucción de arreglo; en ASCII `ok`, `!` y `x`; sin color si `NO_COLOR` está definido o la salida no es una terminal

[x] `magi diagnostico` no escribe ficheros, no arranca el servidor, no consulta secretos del llavero y no abre conexiones de red; código de salida 0 sin errores (los avisos no cuentan) y 1 con algún error

[x] `magi autocompletado <bash|zsh|fish>` escribe el script de autocompletado en la salida estándar (`clap_complete`)

[x] `magi man` escribe la página man `magi.1` en castellano en la salida estándar (`clap_mangen`)

[x] Recursos generados versionados en `assets/` (`completions/magi.bash`, `completions/_magi`, `completions/magi.fish`, `man/magi.1`) y `scripts/generar-recursos.sh` que los regenera; un test falla si no coinciden con la salida del binario

[x] `assets/magi.desktop` (`Name=MAGI`, `Exec=magi`, `Terminal=true`, `Icon=magi`, `Categories=Network;RemoteAccess;ConsoleOnly;`) que pasa `desktop-file-validate`

[x] Icono original `assets/iconos/magi.svg` y `magi-256.png`: monograma geométrico propio en la paleta de MAGI, sin reproducir logotipos ni pantallas de terceros

[x] `.github/workflows/ci.yml` en cada PR y push a `main`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `TZ=UTC cargo test --locked` en Linux

[x] Job de macOS en `ci.yml`: compila con `--locked` y pasa `cargo clippy` y `cargo test --lib`

[x] `cargo deny check` en CI con `deny.toml`: licencias compatibles con MIT OR Apache-2.0, avisos de RustSec y fuentes de crates; cada excepción lleva motivo y fecha

[x] Los tests de SFTP encuentran `sftp-server` en Ubuntu, Arch y macOS (rutas conocidas o variable `MAGI_SFTP_SERVER`)

[x] Job de CI que valida la plantilla del PKGBUILD en un contenedor Arch (`makepkg --printsrcinfo` y `namcap`) con una versión ficticia

[x] Permisos mínimos en los workflows: `contents: read` por defecto y escritura solo en los jobs que publican

[x] `dist` configurado con los objetivos `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` y `aarch64-apple-darwin`, checksums SHA-256 y atestaciones de GitHub

[x] Linux compilado en runners Ubuntu 22.04 x86_64 y arm64 (glibc ≥ 2.35); perfil de distribución con LTO fino y `strip`

[x] Cada tarball incluye `magi`, `README.md`, `CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE` y `assets/` (autocompletado, man, `.desktop` e icono)

[x] Un tag `vX.Y.Z` lanza `release.yml`: compila los cuatro objetivos y crea la GitHub Release con las notas de su sección del CHANGELOG, los `.sha256` y las atestaciones

[x] Un tag que no coincide con la `version` de `Cargo.toml` hace fallar el workflow antes de compilar

[x] Un tag de prerelease (`v1.0.0-rc.1`) publica una GitHub Release marcada como prerelease y no toca Homebrew ni AUR

[x] `dist` publica la fórmula `magi-ssh` en el tap `4d3-org/homebrew-tap` en cada release estable, con el secreto `HOMEBREW_TAP_TOKEN` (token de grano fino con escritura solo en el tap)

[x] `brew install 4d3-org/tap/magi-ssh` instala `magi` en arm64 y x86_64 sin aviso de Gatekeeper; el README explica cómo activar autocompletado y man en macOS y cómo quitar la cuarentena a un tarball bajado con el navegador

[x] Plantilla `packaging/aur/PKGBUILD` de `magi-ssh-bin`: `source_x86_64` y `source_aarch64` desde la release, `sha256sums` de los `.sha256`, `provides=('magi-ssh')`, `conflicts=('magi-ssh')`, `depends=('glibc' 'gcc-libs')` y `optdepends` (`wl-clipboard`, `libsecret`, `openssh`)

[x] El paquete instala `/usr/bin/magi`, autocompletado de bash, zsh y fish, `magi.1`, `magi.desktop`, el icono en `hicolor` y las licencias en `/usr/share/licenses/magi-ssh-bin/`

[x] `packaging/aur/magi-ssh-bin.install`: `post_install` sugiere `magi diagnostico`; `post_upgrade` explica qué pasa con el servidor de sesiones anterior y no para nada

[x] Job de publicación propio `publicar-aur.yml` (reutilizable, lo llama `dist`): rellena versión y sumas, genera `.SRCINFO` en un contenedor Arch, pasa `namcap` y empuja a AUR con `AUR_SSH_PRIVATE_KEY`

[x] Antes de la primera publicación se comprueba que `magi-ssh-bin` y `magi-ssh` están libres en AUR y que ningún paquete oficial instala `/usr/bin/magi`; si no, se para y se consulta

[x] Auditoría antes de hacer público el repositorio: `gitleaks` sobre todo el historial sin hallazgos; si aparece un secreto, se para y se avisa al desarrollador sin publicar nada

[x] Documentación, tests y ejemplos sin IPs reales, credenciales, correos personales ni rutas con nombres de usuario reales (se sustituyen por `192.0.2.x`, `ejemplo.org`, `/home/usuario`)

[x] `docs/publicar.md`: transferir el repo a `4d3-org` si hace falta, hacerlo público, crear el tap y la cuenta de AUR, crear los secretos, política de versiones, publicar con un tag, ensayar con una prerelease y qué hacer si falla un canal

[x] README: instalación por canal (AUR, Homebrew, tarball), verificación (`sha256sum -c`, `gh attestation verify`), primeros pasos, `magi diagnostico`, actualizar con el servidor anterior y desinstalar (qué datos quedan y dónde)

[x] El agente no cambia la visibilidad del repositorio, no crea secretos ni cuentas y no etiqueta versiones: lo hace el desarrollador siguiendo `docs/publicar.md`

[x] Tests: forma JSON estable de `Hola` y `VersionIncompatible`, lectura de un `VersionIncompatible` v5 y v6, decisión sustituir / preguntar / más nuevo, espera de 5 s sin liberar, BD de versión futura rechazada, comprobaciones de `diagnostico` simuladas y recursos al día

[x] Diálogo SERVIDOR DE SESIONES ANTERIOR con mínimo y modo estrecho en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)

[x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios

[x] `TZ=UTC cargo test` verde, incluidas las instantáneas nuevas

[x] Revisión adversarial del código y los workflows nuevos (sustitución del servidor, permisos de CI, cadena de suministro, PKGBUILD) con verificación independiente de cada hallazgo, resultado en el informe de implementación

## Pendientes y bloqueos

**Del desarrollador (fuera del agente, D124):** transferir/hacer público el repositorio en 4d3-org, comprobar otra vez los nombres en AUR, crear 4d3-org/homebrew-tap, la cuenta de AUR y los secretos HOMEBREW_TAP_TOKEN y AUR_SSH_PRIVATE_KEY, ensayar con v1.0.0-rc.1 y etiquetar v1.0.0 siguiendo docs/publicar.md.

**Verificaciones que exigen publicación real (no bloquean la implementación):** release con los cuatro tarballs, .sha256 y atestaciones verificables con gh attestation verify; yay -S magi-ssh-bin; brew install 4d3-org/tap/magi-ssh sin aviso de Gatekeeper; ejecución real de los workflows de GitHub (validados en local con dist plan, actionlint y makepkg).

**Limitaciones de la máquina:** namcap se ejecuta solo en el contenedor Arch de CI (no está instalado en local); las acciones de GitHub no se pueden ejecutar sin push; sftp-server de macOS se resolverá por las rutas conocidas o MAGI_SFTP_SERVER si el runner no lo trae.

**Heredadas de fases anteriores (no se abordan en esta fase, R59):** validación manual completa de las Fases 2-6 (R15), la revisión adversarial pendiente de S1-S3 de la Fase 6 (R38) y la validación manual de la Fase 8 (R50); el desarrollador decidió publicar 1.0.0 igualmente. La revisión adversarial de esta fase sí se ha hecho (siguiente sección) y los tests de la suite están en verde.

## Ejecución y pruebas (cómo arrancar, migrar y testear)

    # TUI (autolanza el servidor de sesiones si hace falta)
    cargo run

    # Servidor de sesiones en primer plano (depuración)
    cargo run -- --servidor

    # Subcomandos nuevos
    cargo run -- --version              # magi 1.0.0 (protocolo 6, <commit>)
    cargo run -- diagnostico            # no escribe nada; código 0 sin errores, 1 con error
    cargo run -- autocompletado bash    # bash | zsh | fish
    cargo run -- man                    # página man en castellano
    cargo run -- servidor estado        # versión de app, protocolo, pid y trabajo

    # Recursos de instalación (tras cambiar la CLI)
    scripts/generar-recursos.sh

    # Verificación
    TZ=UTC cargo test --locked
    cargo fmt --all --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo deny check
    desktop-file-validate assets/magi.desktop
    gitleaks detect --source . --log-opts="--all" --no-banner --redact
    # con dist 0.33.1 en PATH:
    dist plan --tag v1.0.0             # release completa anunciada
    dist plan --tag v1.1.0             # falla (tag distinto de Cargo.toml) antes de compilar
    dist plan --tag v1.0.0-rc.1        # prerelease sin canales

Pruebas aisladas: fijar HOME y XDG_DATA_HOME/XDG_CONFIG_HOME/XDG_STATE_HOME/XDG_RUNTIME_DIR (ruta corta para el socket). Los tests de pintado necesitan TZ=UTC; los de SFTP, sftp-server o MAGI_SFTP_SERVER.

Migración: no hay tablas nuevas; magi.db sigue en user_version 6. Actualizar MAGI no migra nada por sí solo: la migración ocurre al abrir la app, hacia delante. Una base de datos de una versión más nueva no se abre y se avisa de que hay que actualizar MAGI (cliente y servidor).

Verificación de extremo a extremo con el binario real y un servidor falso por socket (30 de octubre de 2026):
- servidor v5 sin trabajo: al abrir la TUI se sustituye solo (SIGTERM al falso, servidor 1.0.0/protocolo 6 arrancado y conectado, sin diálogo);
- servidor v5 con 2 sesiones: aparece el diálogo SERVIDOR DE SESIONES ANTERIOR con las dos versiones, el pid y la custodia, y el servidor viejo sigue vivo hasta que el usuario decide (p → confirmación «Al pararlo, cerrará 2 sesiones y 1 túnel.», n vuelve, s sustituye y el viejo muere);
- servidor de protocolo 13 (más nuevo): solo la barra «este magi es más antiguo que su servidor; actualiza magi», sin diálogo y sin tocar el servidor;
- magi servidor estado contra un servidor v6: «servidor 1.0.0 · protocolo 6 · pid …» con el recuento de trabajo; contra el servidor v5: aviso de versión distinta y «la versión anterior no informa de lo que custodia»;
- magi --version = «magi 1.0.0 (protocolo 6, <commit>)» con git y «desconocido» en una compilación desde una exportación sin .git; magi diagnostico sobre un entorno aislado imprime los signos, avisos sin errores y código 0, y no crea ni un fichero; autocompletado y man producen los recursos versionados.

Resultado de la última verificación completa (30 de octubre de 2026): TZ=UTC cargo test --locked en verde (942 pruebas superadas y 1 ignorada: 482 unitarias y 460 de integración en 15 binarios; incluye las suites nuevas de protocolo, servidor anterior, recursos y diagnóstico y las 264 de pintado), cargo clippy --all-targets --locked -- -D warnings limpio, cargo fmt --check limpio, cargo deny check (advisories ok, bans ok, licenses ok, sources ok), gitleaks sin hallazgos (historial completo, 6,41 MB), dist plan con los cuatro tarballs y sus .sha256/fórmula, makepkg --printsrcinfo y construcción local del paquete AUR, desktop-file-validate sin errores, actionlint limpio en los tres workflows, y magi --version / magi diagnostico / autocompletado / man comprobados a mano contra el binario real.
