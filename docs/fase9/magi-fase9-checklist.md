# MAGI - Checklist Fase 9: Empaquetado y Releases

## Nombre, versión y metadatos
- [x] `Cargo.toml` con `[package] name = "magi-ssh"`, `[lib] name = "magi"` y `[[bin]] name = "magi"`: el paquete se llama `magi-ssh` y el comando sigue siendo `magi`
  - AC: Dado un usuario con datos de la versión de desarrollo, cuando instala `magi-ssh` 1.0.0, entonces MAGI abre la misma `magi.db`, el mismo `config.toml` y el mismo directorio del socket (las rutas usan «magi», nunca el nombre del paquete)
- [x] `version = "1.0.0"` y metadatos completos en `Cargo.toml`: `description`, `license = "MIT OR Apache-2.0"`, `repository`, `homepage`, `readme`, `keywords`, `categories`, `rust-version` y `publish = false`
- [x] `magi --version` muestra versión, protocolo y commit (`magi 1.0.0 (protocolo 6, abc1234)`); compilado sin git, dice «commit desconocido»
- [x] `LICENSE-MIT` y `LICENSE-APACHE` con titular «Cuatro de Tres S. Coop. And.», y sección de licencia en el README
- [x] `CHANGELOG.md` en castellano con formato Keep a Changelog: sección «Sin publicar» y entrada 1.0.0 que resume las Fases 1-8
- [x] `SECURITY.md`: las vulnerabilidades se reportan por el aviso privado de GitHub, con versiones soportadas (la última MINOR)
- [x] Política de versiones (MAJOR, MINOR, PATCH; §7.1 del informe) escrita en `docs/publicar.md`

## Servidor de sesiones al actualizar
- [x] `VERSION_PROTOCOLO = 6`: `VersionIncompatible` gana `version_app`, `clientes` y `trabajo{sesiones, transferencias, tuneles, ejecuciones}`; `Bienvenida` gana `version_app`
- [x] `Hola` y `VersionIncompatible` quedan como contrato estable: nunca se quitan ni se renombran sus campos, solo se añaden opcionales (comentario en `protocolo.rs` y test que fija su forma JSON)
- [x] El cliente lee la primera respuesta del servidor como JSON genérico: un `VersionIncompatible` de un servidor v5 (sin `trabajo` ni `clientes`) se trata como «recuento desconocido»
- [x] Servidor anterior que no custodia nada (`trabajo` todo a cero y `clientes = 0`): el cliente le envía `SIGTERM`, espera hasta 5 s a que libere socket y lock, arranca el servidor nuevo y lo dice en la barra, sin diálogo
  - AC: Dado un servidor 1.0.0 sin sesiones, transferencias, túneles, ejecuciones ni otras ventanas, cuando se abre el binario 1.1.0, entonces la TUI arranca conectada al servidor nuevo sin preguntar
- [x] Servidor anterior con trabajo, con otras ventanas o con recuento desconocido: diálogo SERVIDOR DE SESIONES ANTERIOR con versiones, pid y lo que custodia; `p` lo para tras confirmar («cerrará 2 sesiones y 1 túnel»), `Esc` sigue sin sesiones, `q` sale; nunca se para solo
  - AC: Dado un servidor con 2 pestañas abiertas, cuando se abre el binario nuevo, entonces las pestañas siguen vivas hasta que el usuario pulsa `p` y confirma
- [x] Servidor más nuevo que el binario (protocolo mayor): aviso «este magi es más antiguo que su servidor; actualiza magi» sin opción de pararlo desde ese binario
- [x] Si el servidor anterior no libera el socket en 5 s tras `SIGTERM`, el diálogo lo dice con su pid y no se lanza un segundo servidor
- [x] `magi servidor estado` muestra la versión de la app del servidor y avisa si no coincide con la del binario
- [x] Una `magi.db` con `user_version` mayor que la que conoce el binario no se abre: mensaje «esta base de datos es de una versión más nueva de MAGI; actualiza MAGI», código de salida ≠ 0 y el fichero intacto (cliente y servidor)

## Diagnóstico
- [x] `magi diagnostico` comprueba versión y protocolo, rutas de datos, configuración, estado y runtime con sus permisos, longitud de la ruta del socket (R41), `config.toml`, `magi.db` (en solo lectura) y servidor de sesiones (en marcha, versión, trabajo)
- [x] `magi diagnostico` comprueba agente SSH (`SSH_AUTH_SOCK` y claves cargadas), llavero (`secret-tool` en Linux, `security` en macOS), portapapeles (`wl-copy` o `pbcopy`), editor, paginador, comando de terminal para nueva ventana e `Include` de `magi_config` en `~/.ssh/config`
- [x] Cada comprobación se imprime como `✓` correcto, `!` aviso o `✕` error con una instrucción de arreglo; en ASCII `ok`, `!` y `x`; sin color si `NO_COLOR` está definido o la salida no es una terminal
- [x] `magi diagnostico` no escribe ficheros, no arranca el servidor, no consulta secretos del llavero y no abre conexiones de red; código de salida 0 sin errores (los avisos no cuentan) y 1 con algún error
  - AC: Dado un sistema sin `wl-copy`, cuando se ejecuta `magi diagnostico`, entonces aparece «! portapapeles: instala wl-clipboard» y el código de salida es 0

## Recursos de instalación
- [x] `magi autocompletado <bash|zsh|fish>` escribe el script de autocompletado en la salida estándar (`clap_complete`)
- [x] `magi man` escribe la página man `magi.1` en castellano en la salida estándar (`clap_mangen`)
- [x] Recursos generados versionados en `assets/` (`completions/magi.bash`, `completions/_magi`, `completions/magi.fish`, `man/magi.1`) y `scripts/generar-recursos.sh` que los regenera; un test falla si no coinciden con la salida del binario
- [x] `assets/magi.desktop` (`Name=MAGI`, `Exec=magi`, `Terminal=true`, `Icon=magi`, `Categories=Network;RemoteAccess;ConsoleOnly;`) que pasa `desktop-file-validate`
- [x] Icono original `assets/iconos/magi.svg` y `magi-256.png`: monograma geométrico propio en la paleta de MAGI, sin reproducir logotipos ni pantallas de terceros

## Integración continua
- [x] `.github/workflows/ci.yml` en cada PR y push a `main`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `TZ=UTC cargo test --locked` en Linux
- [x] Job de macOS en `ci.yml`: compila con `--locked` y pasa `cargo clippy` y `cargo test --lib`
- [x] `cargo deny check` en CI con `deny.toml`: licencias compatibles con MIT OR Apache-2.0, avisos de RustSec y fuentes de crates; cada excepción lleva motivo y fecha
- [x] Los tests de SFTP encuentran `sftp-server` en Ubuntu, Arch y macOS (rutas conocidas o variable `MAGI_SFTP_SERVER`)
- [x] Job de CI que valida la plantilla del PKGBUILD en un contenedor Arch (`makepkg --printsrcinfo` y `namcap`) con una versión ficticia
- [x] Permisos mínimos en los workflows: `contents: read` por defecto y escritura solo en los jobs que publican

## Releases con dist
- [x] `dist` configurado con los objetivos `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin` y `aarch64-apple-darwin`, checksums SHA-256 y atestaciones de GitHub
- [x] Linux compilado en runners Ubuntu 22.04 x86_64 y arm64 (glibc ≥ 2.35); perfil de distribución con LTO fino y `strip`
- [x] Cada tarball incluye `magi`, `README.md`, `CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE` y `assets/` (autocompletado, man, `.desktop` e icono)
- [x] Un tag `vX.Y.Z` lanza `release.yml`: compila los cuatro objetivos y crea la GitHub Release con las notas de su sección del CHANGELOG, los `.sha256` y las atestaciones
  - AC: Dado el tag `v1.0.0`, cuando termina el workflow, entonces la release tiene 4 tarballs con su `.sha256` y `gh attestation verify` valida cada uno contra el repositorio
- [x] Un tag que no coincide con la `version` de `Cargo.toml` hace fallar el workflow antes de compilar
- [x] Un tag de prerelease (`v1.0.0-rc.1`) publica una GitHub Release marcada como prerelease y no toca Homebrew ni AUR

## Homebrew (macOS)
- [x] `dist` publica la fórmula `magi-ssh` en el tap `4d3-org/homebrew-tap` en cada release estable, con el secreto `HOMEBREW_TAP_TOKEN` (token de grano fino con escritura solo en el tap)
- [x] `brew install 4d3-org/tap/magi-ssh` instala `magi` en arm64 y x86_64 sin aviso de Gatekeeper; el README explica cómo activar autocompletado y man en macOS y cómo quitar la cuarentena a un tarball bajado con el navegador

## AUR (Omarchy y Arch)
- [x] Plantilla `packaging/aur/PKGBUILD` de `magi-ssh-bin`: `source_x86_64` y `source_aarch64` desde la release, `sha256sums` de los `.sha256`, `provides=('magi-ssh')`, `conflicts=('magi-ssh')`, `depends=('glibc' 'gcc-libs')` y `optdepends` (`wl-clipboard`, `libsecret`, `openssh`)
- [x] El paquete instala `/usr/bin/magi`, autocompletado de bash, zsh y fish, `magi.1`, `magi.desktop`, el icono en `hicolor` y las licencias en `/usr/share/licenses/magi-ssh-bin/`
- [x] `packaging/aur/magi-ssh-bin.install`: `post_install` sugiere `magi diagnostico`; `post_upgrade` explica qué pasa con el servidor de sesiones anterior y no para nada
- [x] Job de publicación propio `publicar-aur.yml` (reutilizable, lo llama `dist`): rellena versión y sumas, genera `.SRCINFO` en un contenedor Arch, pasa `namcap` y empuja a AUR con `AUR_SSH_PRIVATE_KEY`
  - AC: Dado `v1.0.0` publicado, cuando termina el job, entonces `yay -S magi-ssh-bin` instala la 1.0.0 y `magi --version` dice 1.0.0
- [x] Antes de la primera publicación se comprueba que `magi-ssh-bin` y `magi-ssh` están libres en AUR y que ningún paquete oficial instala `/usr/bin/magi`; si no, se para y se consulta

## Publicación y auditoría
- [x] Auditoría antes de hacer público el repositorio: `gitleaks` sobre todo el historial sin hallazgos; si aparece un secreto, se para y se avisa al desarrollador sin publicar nada
- [x] Documentación, tests y ejemplos sin IPs reales, credenciales, correos personales ni rutas con nombres de usuario reales (se sustituyen por `192.0.2.x`, `ejemplo.org`, `/home/usuario`)
- [x] `docs/publicar.md`: transferir el repo a `4d3-org` si hace falta, hacerlo público, crear el tap y la cuenta de AUR, crear los secretos, política de versiones, publicar con un tag, ensayar con una prerelease y qué hacer si falla un canal
- [x] README: instalación por canal (AUR, Homebrew, tarball), verificación (`sha256sum -c`, `gh attestation verify`), primeros pasos, `magi diagnostico`, actualizar con el servidor anterior y desinstalar (qué datos quedan y dónde)
- [x] El agente no cambia la visibilidad del repositorio, no crea secretos ni cuentas y no etiqueta versiones: lo hace el desarrollador siguiendo `docs/publicar.md`

## Calidad
- [x] Tests: forma JSON estable de `Hola` y `VersionIncompatible`, lectura de un `VersionIncompatible` v5 y v6, decisión sustituir / preguntar / más nuevo, espera de 5 s sin liberar, BD de versión futura rechazada, comprobaciones de `diagnostico` simuladas y recursos al día
- [x] Diálogo SERVIDOR DE SESIONES ANTERIOR con mínimo y modo estrecho en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `TZ=UTC cargo test` verde, incluidas las instantáneas nuevas
- [x] Revisión adversarial del código y los workflows nuevos (sustitución del servidor, permisos de CI, cadena de suministro, PKGBUILD) con verificación independiente de cada hallazgo, resultado en el informe de implementación

---

**Progreso Fase 9:** 54 / 54 funcionalidades

**Total MAGI (Fases 1-9):** 650 / 654 funcionalidades
