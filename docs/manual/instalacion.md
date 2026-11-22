# Instalación, actualización y desinstalación

[← Índice del manual](README.md)

## Requisitos

- Linux (Omarchy/Arch, o cualquier distribución con glibc ≥ 2.35) o macOS.
- Opcional: `ssh-agent` en `SSH_AUTH_SOCK` para autenticar sin frase.
- Opcional, en los hosts: **tmux 3.0 o posterior** para las
  [sesiones persistentes](sesiones-persistentes.md#sesiones-persistentes) (MAGI no lo instala).
- En Linux, para recordar contraseñas y para la sincronización (su clave vive
  en el llavero): `secret-tool` y un servicio de llavero en la sesión (GNOME
  Keyring, KWallet con «Secret Service» o KeePassXC). En Debian y Ubuntu,
  `sudo apt install libsecret-tools gnome-keyring` y volver a iniciar sesión;
  en Arch, `libsecret` y `gnome-keyring`. En macOS se usa el Keychain.
- Solo para compilar desde el código: Rust stable (edición 2021, MSRV 1.86).

## Instalación

MAGI se distribuye como paquete **`magi-ssh`**; el comando sigue siendo
`magi`. Primera versión pública: **1.6.1**.

### Omarchy y Arch (AUR)

**Todavía no está publicado.** El paquete de AUR está preparado, pero aún no se
sube: por ahora, instala el [tarball](#tarball-de-la-última-release). Cuando se
publique, será:

```sh
yay -S magi-ssh-bin
```

El paquete instala `/usr/bin/magi`, el autocompletado de bash, zsh y fish, la
página man, la entrada de escritorio y el icono. Al terminar, `pacman` sugiere
ejecutar `magi diagnostico`.

Nota: en AUR existe otro paquete llamado `magi` (una TUI de Git) que también
instala `/usr/bin/magi`; `magi-ssh-bin` lo declara en `conflicts`, así que
`pacman` pedirá quitarlo antes de instalar MAGI.

### macOS (Homebrew)

**Todavía no está publicado.** El tap de Homebrew no existe aún: por ahora,
instala el [tarball](#tarball-de-la-última-release). Cuando se publique, será:

```sh
brew install hectorespejo/tap/magi-ssh
```

La fórmula instala el binario `magi` en Apple Silicon e Intel y no pasa por
Gatekeeper, así que no hay que quitar la cuarentena. Si el autocompletado o la
página man no aparecen en las rutas de Homebrew, actívalos a mano (ver
[Autocompletado y man en macOS](#autocompletado-y-man-en-macos)).

### Tarball de la última release

Para Linux (Omarchy y Arch incluidos) y macOS, descarga el
tarball de [la última release](https://github.com/HectorEspejo/magi/releases/latest):

| Plataforma | Tarball |
|---|---|
| Linux x86_64 | `magi-ssh-x86_64-unknown-linux-gnu.tar.xz` |
| Linux aarch64 | `magi-ssh-aarch64-unknown-linux-gnu.tar.xz` |
| macOS Apple Silicon | `magi-ssh-aarch64-apple-darwin.tar.xz` |
| macOS Intel | `magi-ssh-x86_64-apple-darwin.tar.xz` |

```sh
tar -xf magi-ssh-x86_64-unknown-linux-gnu.tar.xz
sudo install -m755 magi-ssh-x86_64-unknown-linux-gnu/magi /usr/local/bin/magi
```

Los binarios de Linux necesitan **glibc ≥ 2.35** (Ubuntu 22.04 o posterior); en
Omarchy y Arch la superan. En macOS, si el tarball lo bajaste con el
navegador, quita la cuarentena antes de ejecutarlo:

```sh
xattr -d com.apple.quarantine magi
```

### Verificación

Cada release publica un `.sha256` por tarball y atestaciones de procedencia de
GitHub. Verifica ambos antes de instalar:

```sh
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo HectorEspejo/magi
```

En macOS, si no tienes `sha256sum`, usa `shasum -a 256 -c`. `gh` es el
[GitHub CLI](https://cli.github.com/); la atestación comprueba que el binario
salió del CI del repositorio.

### Desde el código

```sh
cargo install --path .
# o, para probar sin instalar:
cargo build --release && ./target/release/magi
```

## Actualizar MAGI

Con el gestor de paquetes (cuando AUR y Homebrew estén publicados), la
actualización es la habitual:

```sh
yay -Syu                 # Omarchy y Arch (magi-ssh-bin)
brew upgrade magi-ssh    # macOS (tap hectorespejo/tap)
```

Con el tarball (por ahora, lo único en Linux y macOS), descarga la versión nueva, verifícala como en la instalación y
sustituye el binario.

**Servidor de sesiones anterior.** Si tenías MAGI abierto, el servidor de
sesiones sigue siendo el del binario anterior. Al abrir el `magi` nuevo:

- si el servidor anterior **no custodia nada** (ninguna sesión, transferencia,
  túnel, ejecución ni otra ventana), se sustituye solo y la barra lo dice: no se
  pierde nada porque no había nada;
- si **custodia algo**, o es una versión que no informa de lo que custodia,
  aparece el diálogo **SERVIDOR DE SESIONES ANTERIOR** con las dos versiones, el
  pid y lo que tiene: `p` lo para y arranca el nuevo tras confirmar («cerrará 2
  sesiones y 1 túnel»), `Esc` sigue sin sesiones y `q` sale. Nunca se para solo;
- si el servidor es **más nuevo** que el binario, se avisa de que actualices MAGI
  y no se para desde ese binario.

La actualización no migra nada por sí sola: la migración de `magi.db` ocurre al
abrir la app, hacia delante. Si vuelves a un binario anterior con una base de
datos ya migrada, se negará a abrirla (sin tocarla): reinstala la versión nueva.

## Autocompletado y man en macOS

La fórmula de Homebrew instala el binario y, si la versión lo incluye, el
autocompletado y la página man. Si no aparecen, genéralos con el propio binario:

```sh
# zsh (el shell por defecto de macOS)
magi autocompletado zsh > "$(brew --prefix)/share/zsh/site-functions/_magi"
# bash
magi autocompletado bash > "$(brew --prefix)/etc/bash_completion.d/magi"
# fish
magi autocompletado fish > "$(brew --prefix)/share/fish/vendor_completions.d/magi.fish"
# página man
magi man > "$(brew --prefix)/share/man/man1/magi.1"
```

En Linux, el paquete de AUR ya coloca esos ficheros en las rutas del sistema; si
usas el tarball, instálalos con `sudo install -Dm644` en
`/usr/share/bash-completion/completions/magi`,
`/usr/share/zsh/site-functions/_magi`,
`/usr/share/fish/vendor_completions.d/magi.fish` y
`/usr/share/man/man1/magi.1`.

## Desinstalación

```sh
sudo pacman -Rns magi-ssh-bin   # Omarchy y Arch
brew uninstall magi-ssh         # macOS
```

Con el tarball, borra el binario (`sudo rm /usr/local/bin/magi`). Desinstalar
MAGI **no borra los datos del usuario**; quedan en:

| Qué | Dónde |
|---|---|
| Inventario | `~/.local/share/magi/` (`magi.db` y sus ficheros WAL) |
| Configuración | `~/.config/magi/config.toml` |
| Estado y logs | `~/.local/state/magi/` |
| `magi_config` | `~/.ssh/magi_config` |
| `Include` | la línea `Include ~/.ssh/magi_config` de `~/.ssh/config` |
| Socket y lock | `$XDG_RUNTIME_DIR/magi/` (se borran al parar el servidor) |
| Copias de seguridad | `~/.ssh/config.bak-<fecha>` y `known_hosts.old` |
| Contraseñas recordadas | el llavero del sistema (`secret-tool`/Keychain) |

Para empezar de cero: cierra MAGI, para el servidor (`magi servidor parar`),
borra esas rutas y quita la línea `Include`. MAGI nunca borra claves de `~/.ssh`
ni entradas de `known_hosts`; si quieres quitar las que aceptaste, usa
`ssh-keygen -R <host>`.
