# MAGI - Fase 9: Empaquetado y Releases

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 30 de octubre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-8 con informe procesado. La Fase 8 está al 63/64: falta su revisión adversarial (R50, aceptado) y su validación manual. Las Fases 2-6 siguen sin validación manual completa (R15) y S1-S3 de la Fase 6 sin revisión adversarial (R38). Hector decide publicar como `1.0.0` igualmente (R59).

---

## 1. Visión General

Hasta ahora MAGI se instala compilando el repositorio. La Fase 9 lo convierte en un programa **instalable y actualizable** con los gestores de paquetes de las dos plataformas objetivo, publicado como software libre: `yay -S magi-ssh-bin` en Omarchy/Arch y `brew install 4d3-org/tap/magi-ssh` en macOS, con tarballs verificables en cada GitHub Release. Cada versión sale de un tag por integración continua, nunca de un binario compilado a mano.

Empaquetar obliga a resolver lo que pasa al **actualizar**: el servidor de sesiones de la versión anterior puede seguir vivo con pestañas, túneles o una sincronización en curso. La fase define un saludo estable entre versiones para que un binario nuevo sepa qué custodia el servidor viejo y lo sustituya solo cuando no hay nada que perder.

**Objetivos principales**

1. Paquete `magi-ssh` (comando `magi`), versión `1.0.0`, licencia MIT OR Apache-2.0, repositorio público en `4d3-org/magi`.
2. Releases automáticas con `dist`: un tag `vX.Y.Z` compila Linux x86_64/aarch64 y macOS arm64/x86_64, publica checksums y atestaciones, la fórmula de Homebrew y el paquete AUR `magi-ssh-bin`.
3. Integración continua en cada PR (formato, clippy, tests, macOS, licencias y avisos de seguridad). Cierra el pendiente de CI de la Fase 1.
4. Actualización segura: protocolo v6 con `Hola` y `VersionIncompatible` estables; un servidor anterior sin trabajo se sustituye solo, uno con trabajo lo decide el usuario.
5. Instalación completa: autocompletado (bash, zsh, fish), página man, entrada de escritorio con icono y `magi diagnostico`.
6. Publicación limpia: auditoría de secretos y datos reales antes de hacer público el repositorio, README, CHANGELOG, SECURITY y guía de publicación.

**Contexto.** Fase de infraestructura: casi no toca las vistas. Fuera de alcance: notarización de Apple, paquetes .deb/.rpm, crates.io, instalador `curl | sh`, Windows, comprobación de actualizaciones desde la app (T10) y distribución de la app Android (F11).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Releases | `dist` (antes cargo-dist) | Genera `release.yml`, tarballs, checksums, atestaciones y la fórmula de Homebrew (D111) |
| CI | GitHub Actions | `ci.yml` propio; `release.yml` lo genera `dist`; `publicar-aur.yml` propio |
| Licencias y avisos | `cargo-deny` | `deny.toml`: licencias permitidas, RustSec, fuentes |
| Autocompletado y man | `clap_complete`, `clap_mangen` | Generados por el binario y versionados en `assets/` (D119) |
| Integridad | SHA-256 + atestaciones de GitHub (Sigstore) | `gh attestation verify`; sin claves propias (D117) |
| Linux | glibc dinámica, Ubuntu 22.04 (glibc ≥ 2.35) | x86_64 y arm64 en runners nativos (D113) |
| macOS | arm64 y x86_64 sin notarizar | Homebrew no pone cuarentena a las fórmulas (D115) |
| AUR | `magi-ssh-bin` | PKGBUILD que reempaqueta el tarball de la release (D114) |
| Auditoría | `gitleaks` | Historial completo antes de publicar |
| Protocolo | `VERSION_PROTOCOLO = 6` | `VersionIncompatible` ampliado; `Hola` y `VersionIncompatible` estables (D120) |

### Estructura de carpetas (novedades)

```
Cargo.toml                    magi-ssh · [lib] magi · [[bin]] magi · 1.0.0 · perfil de distribución
dist-workspace.toml           configuración de dist (la crea `dist init`)
deny.toml                     licencias permitidas, RustSec, fuentes
build.rs                      commit corto para --version
README.md · CHANGELOG.md · SECURITY.md · LICENSE-MIT · LICENSE-APACHE
assets/
├── completions/              magi.bash · _magi · magi.fish   (generados)
├── man/magi.1                                               (generado)
├── magi.desktop
└── iconos/                   magi.svg · magi-256.png
packaging/aur/
├── PKGBUILD                  plantilla de magi-ssh-bin
├── magi-ssh-bin.install      post_install / post_upgrade
└── generar.sh                rellena versión y sumas desde una release
scripts/generar-recursos.sh   regenera assets/ con el binario
.github/workflows/
├── ci.yml                    PR y main: fmt, clippy, tests Linux, macOS, deny, plantilla AUR
├── release.yml               (dist) tag vX.Y.Z → GitHub Release → Homebrew → AUR
└── publicar-aur.yml          job de publicación propio, reutilizable
src/
├── main.rs                   + diagnostico · autocompletado · man · --version largo
├── diagnostico.rs            comprobaciones y salida
├── protocolo.rs              v6: VersionIncompatible ampliado y estable; Bienvenida.version_app
├── servidor/mod.rs           rellena trabajo y clientes al rechazar un cliente de otra versión
├── app/servidor_anterior.rs  decisión sustituir / preguntar / más nuevo
├── ui/servidor_anterior.rs   diálogo SERVIDOR DE SESIONES ANTERIOR
└── almacen/migraciones.rs    rechaza una user_version futura
tests/
├── protocolo.rs · servidor.rs · diagnostico.rs · recursos.rs
└── redimensionado/vistas_l.rs   diálogo SERVIDOR DE SESIONES ANTERIOR
docs/publicar.md              guía de publicación para el desarrollador
docs/fase9/
├── magi-fase9-informe.md · magi-fase9-prompt.md · magi-fase9-checklist.md
└── magi-fase9-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 desarrollador                     GitHub                                          usuarios
 ┌──────────────┐  PR       ┌───────────────────────────┐
 │ rama · PR    │──────────▶│ ci.yml                    │  fmt · clippy · tests Linux · macOS
 └──────┬───────┘           │                           │  cargo deny · plantilla AUR
        │ git tag v1.0.0    └───────────────────────────┘
        ▼                   ┌───────────────────────────┐
 ┌──────────────┐  push tag │ release.yml (dist)        │
 │ main         │──────────▶│  plan ─▶ compilar ×4 ─────┼──▶ GitHub Release ──────────────┐
 └──────────────┘           │  checksums · atestaciones │    tarballs · .sha256 ·         │ tarball
                            │  publicar ────────────────┼─┐  atestaciones · notas         ├────────▶ cualquier
                            └───────────────────────────┘ │                                │          Linux/macOS
                              HOMEBREW_TAP_TOKEN ─────────┼──▶ 4d3-org/homebrew-tap ──────┼────────▶ brew install
                              AUR_SSH_PRIVATE_KEY ────────┴──▶ AUR magi-ssh-bin ──────────┼────────▶ yay -S
                                                                                           │
 instalado: /usr/bin/magi (+ autocompletado, man, .desktop, icono)                         │
 ┌──────────────────────────┐   Hola{6}            ┌────────────────────────────┐          │
 │ magi 1.1.0 (cliente)     │─────────────────────▶│ magi --servidor 1.0.0      │◀─────────┘
 │ app/servidor_anterior.rs │◀─────────────────────│ VersionIncompatible{       │
 │  sustituir · preguntar   │   versión, pid,      │  version_app, clientes,    │
 └──────────────────────────┘   trabajo            │  trabajo}                  │
                                                   └────────────────────────────┘
```

**Reparto.** La app solo cambia en tres puntos: el saludo con un servidor de otra versión, la negativa a abrir una base de datos más nueva y los subcomandos `diagnostico`, `autocompletado` y `man`. Todo lo demás es configuración del repositorio y de CI. Las acciones irreversibles (hacer público el repositorio, crear secretos y cuentas, etiquetar una versión) las hace el desarrollador siguiendo `docs/publicar.md`; el agente las prepara pero no las ejecuta (D124).

---

## 3. Modelo de Datos

**Sin tablas nuevas:** `magi.db` sigue en `user_version` 6. El modelo de esta fase son los artefactos de publicación, que viven en GitHub y en los canales, no en SQLite.

### Diagrama E-R conceptual (artefactos de publicación)

```
 ┌──────────────────────────┐ 1      N ┌────────────────────────────────────┐
 │ RELEASE                  │─────────<│ ARTEFACTO                          │
 │ tag          vX.Y.Z      │          │ objetivo    (4 por release)        │
 │ version      X.Y.Z       │          │ tarball     magi-ssh-<obj>.tar.xz  │
 │ prerelease   sí/no       │          │ sha256      <tarball>.sha256       │
 │ notas        CHANGELOG   │          │ atestacion  Sigstore (GitHub)      │
 └────────────┬─────────────┘          └────────────────────────────────────┘
              │ 1
              │ N
 ┌────────────▼─────────────┐
 │ PUBLICACION              │  canal: github | homebrew | aur
 │ canal · estado           │  estado: pendiente | publicada | fallida
 │ artefacto_publicado      │  fórmula magi-ssh.rb · PKGBUILD + .SRCINFO
 └──────────────────────────┘
```

Restricciones: una RELEASE no se reutiliza (un tag publicado no se mueve; se corrige con una versión nueva). Una prerelease solo tiene PUBLICACION en `github`.

### Diagrama de estados: release

```
   tag vX.Y.Z empujado
        ▼
   planificando (dist plan) ── tag ≠ versión de Cargo.toml ─▶ fallida (nada publicado)
        ▼
   compilando ×4 ── falla un objetivo ─▶ fallida (nada publicado; se corrige y se etiqueta otra versión)
        ▼
   publicando GitHub Release (tarballs, .sha256, atestaciones, notas) ── error ─▶ fallida
        ▼
   ¿prerelease? ── sí ─▶ publicada (solo GitHub) ─▶ fin
        ▼ no
   publicando canales ── Homebrew falla ─▶ canal fallido (release válida; se relanza el job)
        │               └─ AUR falla ─────▶ canal fallido (release válida; se relanza el job)
        ▼
   publicada (GitHub + Homebrew + AUR)
  Inválidas: publicar en canales sin GitHub Release; reutilizar o mover un tag publicado;
  publicar un binario compilado fuera de CI; publicar en canales una prerelease.
```

### Diagrama de estados: servidor de otra versión al abrir `magi`

```
   cliente ─ Hola{6} ─▶ servidor responde
        ├─ Bienvenida ─▶ conectado (misma versión) ─▶ fin
        └─ VersionIncompatible{version, …}
             ├─ version > 6 ─▶ más_nuevo ─▶ aviso «actualiza magi» · TUI sin sesiones ─▶ fin
             ├─ sin trabajo/clientes (servidor v5) ─▶ preguntar
             ├─ trabajo = 0 y clientes = 0 ─▶ sustituyendo ─ SIGTERM ─▶ esperando (≤ 5 s)
             │        ├─ socket y lock liberados ─▶ arrancar servidor nuevo ─▶ conectado ─▶ fin
             │        └─ no liberados ─▶ error en el diálogo (pid) · TUI sin sesiones ─▶ fin
             └─ trabajo > 0 o clientes > 0 ─▶ preguntar
   preguntar ─▶ [diálogo SERVIDOR DE SESIONES ANTERIOR]
        ├─ p ─▶ [confirmación «cerrará …»] ── sí ─▶ sustituyendo
        │                                   └─ no ─▶ preguntar
        ├─ Esc ─▶ TUI sin sesiones (como hasta la F8) ─▶ fin
        └─ q ─▶ salir sin tocar el servidor
  Inválidas: parar un servidor con trabajo u otras ventanas sin confirmación; parar desde un binario
  más antiguo que el servidor; lanzar un segundo servidor mientras el anterior conserva el socket.
```

---

## 4. Flujos de Trabajo

### 4.1 Publicar una versión

```
  desarrollador: actualiza CHANGELOG (sección «Sin publicar» ─▶ «[1.1.0] - fecha»), version en Cargo.toml
      ▼ PR ─▶ ci.yml verde ─▶ merge a main
  git tag v1.1.0 · git push --tags
      ▼
  release.yml (dist):
      plan ─── tag ≠ Cargo.toml ─▶ falla (nada publicado)
      ▼
      compilar x86_64-linux · aarch64-linux · aarch64-macos · x86_64-macos ── falla uno ─▶ falla (nada publicado)
      ▼
      GitHub Release: tarballs + .sha256 + atestaciones + notas de la sección del CHANGELOG
      ▼ (solo versiones estables)
      publicar Homebrew (fórmula magi-ssh en 4d3-org/homebrew-tap) ── error ─▶ relanzar el job
      publicar AUR (publicar-aur.yml: PKGBUILD + .SRCINFO + namcap + push) ── error ─▶ relanzar el job
```

Un error tras crear la GitHub Release no invalida la versión: se relanza el job del canal. Un error antes la deja sin publicar y se corrige con un commit y un tag nuevo (nunca moviendo el tag).

### 4.2 Primera publicación (una sola vez)

```
  agente: prepara todo (S1, S2) y pasa la auditoría (gitleaks + datos reales) ── secreto en el historial ─▶ PARAR y avisar
      ▼
  desarrollador (docs/publicar.md):
      1. transfiere el repositorio a 4d3-org si vive en otra cuenta
      2. comprueba que magi-ssh-bin y magi-ssh están libres en AUR ── ocupado ─▶ PARAR y consultar nombre
      3. hace público el repositorio
      4. crea 4d3-org/homebrew-tap, la cuenta de AUR con su clave SSH y los secretos
         HOMEBREW_TAP_TOKEN y AUR_SSH_PRIVATE_KEY
      5. ensayo: tag v1.0.0-rc.1 ─▶ prerelease en GitHub (sin canales) ─▶ instalar el tarball a mano
      6. tag v1.0.0 ─▶ release completa ─▶ yay -S magi-ssh-bin · brew install 4d3-org/tap/magi-ssh
```

El ensayo va después de hacer público el repositorio: las atestaciones de GitHub y los runners arm64 gratuitos son para repositorios públicos.

### 4.3 Instalar y actualizar en Omarchy

```
  yay -S magi-ssh-bin
      ▼ pacman: /usr/bin/magi, autocompletado, man, .desktop, icono, licencias
      ▼ post_install: «Ejecuta magi diagnostico para comprobar lo que MAGI necesita»
  magi (desde la terminal o el lanzador) ─▶ crea ~/.local/share/magi, ~/.config/magi… como siempre

  yay -Syu (llega 1.1.0)
      ▼ post_upgrade: mensaje sobre el servidor de sesiones anterior (no para nada)
  magi ─▶ Hola{7} ─▶ servidor 1.0.0 responde VersionIncompatible{6, trabajo, clientes} ─▶ §4.4
```

### 4.4 Abrir `magi` con un servidor de otra versión

```
  conectar al socket ─▶ Hola{version: 6, pid}
      ▼
  primera línea del servidor, leída como JSON genérico
      ├─ Bienvenida ─▶ normal
      └─ VersionIncompatible
           ▼
      decidir (§7.2)
           ├─ más_nuevo ─▶ aviso en la barra · TUI sin sesiones
           ├─ sustituir ─▶ SIGTERM al pid ─▶ esperar ≤ 5 s a que socket y lock queden libres
           │                ├─ libres ─▶ autolanzar servidor nuevo ─▶ Hola ─▶ Bienvenida
           │                │            barra: «servidor de sesiones actualizado a 1.1.0»
           │                └─ ocupados ─▶ diálogo con el error y el pid · TUI sin sesiones
           └─ preguntar ─▶ diálogo SERVIDOR DE SESIONES ANTERIOR (§6.2)
                            p ─▶ confirmación ─▶ sustituir · Esc ─▶ sin sesiones · q ─▶ salir
```

### 4.5 Diagrama de secuencia: release estable

```
 desarrollador     GitHub          release.yml (dist)        runners ×4        tap / AUR
     │──push v1.1.0──▶│                    │                       │                 │
     │                │──workflow──────────▶│ plan (tag = Cargo)    │                 │
     │                │                    │──compilar────────────▶│ cargo build     │
     │                │                    │◀──tarballs + sha256───│ --locked        │
     │                │                    │ atestaciones (Sigstore)                 │
     │                │◀──GitHub Release───│ notas del CHANGELOG   │                 │
     │                │                    │──fórmula magi-ssh.rb (HOMEBREW_TAP_TOKEN)──▶│ tap
     │                │                    │──publicar-aur.yml: PKGBUILD, .SRCINFO,       │
     │                │                    │   namcap, git push (AUR_SSH_PRIVATE_KEY)───▶│ AUR
     │◀──release publicada─────────────────│                       │                 │
```

### 4.6 Diagrama de secuencia: sustituir un servidor anterior sin trabajo

```
 magi 1.1.0 (cliente)              servidor 1.0.0                 sistema
     │──connect $XDG_RUNTIME_DIR/magi/socket──▶│                       │
     │──Hola{version: 7, pid}──────────────────▶│                       │
     │◀──VersionIncompatible{version: 6, version_app: "1.0.0", pid: 48211,
     │     clientes: 0, trabajo{0,0,0,0}}───────│ cierra la conexión    │
     │ decidir ─▶ sustituir                     │                       │
     │──SIGTERM 48211──────────────────────────────────────────────────▶│
     │                                          │ Parar: anota y apaga  │
     │                                          │ borra socket y lock   │
     │ espera ≤ 5 s (lock libre) ◀─────────────────────────────────────│
     │──setsid magi --servidor (autolanzado)───────────────────────────▶│
     │──Hola{7} ─▶ Bienvenida{7, version_app: "1.1.0", …}               │
```

---

## 5. Acciones, Protocolo y CI

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi --version` | `magi 1.0.0 (protocolo 6, abc1234)`; sin git al compilar, «commit desconocido» |
| `magi diagnostico` | Comprueba lo que MAGI necesita (§7.3); no modifica nada; código 0 o 1 |
| `magi autocompletado <bash\|zsh\|fish>` | Script de autocompletado en la salida estándar |
| `magi man` | Página man `magi.1` en la salida estándar |
| `magi servidor estado` | Añade la versión de la app del servidor y avisa si no coincide con la del binario |

### 5.2 Protocolo (`VERSION_PROTOCOLO = 6`)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `Hola{version, pid}` (**estable**) | `Bienvenida{…}` gana `version_app` |
| — | `VersionIncompatible{version, pid?, version_app?, clientes?, trabajo?}` (**estable**); `trabajo` = `{sesiones, transferencias, tuneles, ejecuciones}`; `clientes` = ventanas adjuntas además de la que pregunta |

**Contrato estable.** `Hola` y `VersionIncompatible` no cambian nunca de forma: no se quitan ni se renombran campos, solo se añaden opcionales. El cliente lee la primera respuesta como JSON genérico, así entiende el rechazo de cualquier versión, pasada o futura. `transferencias` cuenta las que están en cola o en curso; `tuneles`, los activos; `ejecuciones`, las que están en curso.

### 5.3 Workflows

| Workflow | Disparo | Jobs |
|---|---|---|
| `ci.yml` | PR y push a `main` | `formato` (fmt), `clippy`, `tests-linux` (`TZ=UTC cargo test --locked`), `macos` (build, clippy, `cargo test --lib`), `deny` (`cargo deny check`), `aur-plantilla` (contenedor Arch: `makepkg --printsrcinfo` y `namcap`) |
| `release.yml` | tag `v*` | Lo genera `dist`: `plan`, compilación por objetivo, artefactos globales (checksums, fórmula), `host` (GitHub Release), `publish-homebrew-formula`, `publicar-aur` (propio), `announce` |
| `publicar-aur.yml` | lo llama `release.yml` | `packaging/aur/generar.sh` con la versión y los `.sha256` de la release → `.SRCINFO` → `namcap` → push a `aur@aur.archlinux.org:magi-ssh-bin.git` |

Secretos: `HOMEBREW_TAP_TOKEN` (token de grano fino con escritura solo en `4d3-org/homebrew-tap`) y `AUR_SSH_PRIVATE_KEY` (clave dedicada, solo registrada en la cuenta de AUR). Permisos por defecto `contents: read`; `contents: write`, `id-token: write` y `attestations: write` solo en los jobs que publican.

### 5.4 Ficheros instalados por el paquete AUR

| Ruta | Origen |
|---|---|
| `/usr/bin/magi` | binario |
| `/usr/share/bash-completion/completions/magi` | `assets/completions/magi.bash` |
| `/usr/share/zsh/site-functions/_magi` | `assets/completions/_magi` |
| `/usr/share/fish/vendor_completions.d/magi.fish` | `assets/completions/magi.fish` |
| `/usr/share/man/man1/magi.1` | `assets/man/magi.1` |
| `/usr/share/applications/magi.desktop` | `assets/magi.desktop` |
| `/usr/share/icons/hicolor/scalable/apps/magi.svg` | `assets/iconos/magi.svg` |
| `/usr/share/licenses/magi-ssh-bin/LICENSE-MIT`, `LICENSE-APACHE` | licencias |

Los datos del usuario (`~/.local/share/magi`, `~/.config/magi`, `~/.local/state/magi`, `~/.ssh/magi_config` y la línea `Include` de `~/.ssh/config`) no son del paquete: ni la instalación ni la desinstalación los tocan.

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   terminal
   ├── magi --version
   ├── magi diagnostico ─▶ informe ✓ ! ✕ ─▶ código 0 / 1
   ├── magi autocompletado <shell> · magi man ─▶ salida estándar
   └── magi ─▶ ¿servidor de otra versión?
              ├─ sin trabajo ─▶ sustitución silenciosa ─▶ Flota (barra: «servidor actualizado»)
              ├─ con trabajo / desconocido ─▶ [SERVIDOR DE SESIONES ANTERIOR] ─ p ─▶ [confirmación]
              │                                                            ├ Esc ─▶ Flota sin sesiones
              │                                                            └ q ─▶ salir
              └─ más nuevo ─▶ Flota sin sesiones (barra: «actualiza magi»)
   lanzador de Omarchy ─▶ «MAGI» (magi.desktop, Terminal=true) ─▶ terminal por defecto ─▶ magi
```

### 6.1 `magi diagnostico`

```
$ magi diagnostico
MAGI 1.0.0 · protocolo 6 · commit abc1234

  ✓ datos          ~/.local/share/magi (700) · magi.db versión 6
  ✓ configuración  ~/.config/magi/config.toml
  ✓ estado         ~/.local/state/magi (logs)
  ✓ runtime        /run/user/1000/magi (700) · socket 27 bytes
  ✓ servidor       en marcha · 1.0.0 · 2 sesiones, 1 túnel
  ✓ agente SSH     2 claves cargadas
  ✓ llavero        secret-tool
  ! portapapeles   wl-copy no encontrado → instala wl-clipboard
  ✓ editor         nvim ($EDITOR)
  ✓ paginador      less
  ✓ terminal       alacritty ([terminal] comando)
  ! ssh_config     sin «Include magi_config» → ssh fuera de MAGI no verá tus hosts
                   (MAGI lo añade al exportar desde la ficha o con `magi exportar`)

2 avisos, 0 errores
```

### 6.2 Diálogo SERVIDOR DE SESIONES ANTERIOR

```
        ┌─ SERVIDOR DE SESIONES ANTERIOR ──────────────────────┐
        │  Este magi:       1.1.0 · protocolo 7                │
        │  Servidor activo: 1.0.0 · protocolo 6 · pid 48211    │
        │                                                      │
        │  Custodia:  2 sesiones · 1 túnel                     │
        │             0 transferencias · 0 ejecuciones         │
        │             1 ventana más de la versión anterior     │
        │                                                      │
        │  Pararlo cierra todo lo anterior.                    │
        │  p parar y arrancar el nuevo                         │
        │  esc seguir sin sesiones           q salir           │
        └──────────────────────────────────────────────────────┘
```

Con un servidor v5, «Custodia» dice «la versión anterior no informa de lo que custodia» y la confirmación de `p` lo advierte. Mínimo 40×12; en estrecho, las líneas de custodia se parten. Título sin signos de apertura (`texto_ascii`).

### 6.3 `magi --version` y `magi servidor estado`

```
$ magi --version
magi 1.1.0 (protocolo 7, 9f3c2e1)

$ magi servidor estado
servidor 1.0.0 · protocolo 6 · pid 48211
! el servidor es de otra versión que este magi (1.1.0); al abrir magi se sustituirá o te preguntará
sesiones 2 · clientes 1 · transferencias 0 · túneles 1 · ejecuciones 0
```

### 6.4 Mensaje de `pacman` al actualizar

```
:: Ejecutando hooks…
==> MAGI actualizado a 1.1.0.
    Si tenías MAGI abierto, su servidor de sesiones sigue siendo el anterior.
    Al abrir magi: si no tiene sesiones, túneles, transferencias ni otras ventanas,
    se sustituye solo; si tiene algo, MAGI te pregunta antes de pararlo.
```

### 6.5 Instalación en el README

```
## Instalación

Omarchy / Arch       yay -S magi-ssh-bin
macOS (Homebrew)     brew install 4d3-org/tap/magi-ssh
Otros Linux / macOS  tarball de la última release · verificar:
                     sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
                     gh attestation verify magi-ssh-…tar.xz --repo 4d3-org/magi

Después:  magi diagnostico
```

### Notas de UX y diseño

- La sustitución silenciosa solo avisa en la barra; el usuario no pierde nada porque no había nada.
- `magi diagnostico` usa los mismos signos que el resto de MAGI (T5) y degrada a ASCII; sin color si `NO_COLOR` o si la salida no es una terminal.
- La entrada de escritorio abre MAGI en la terminal por defecto del sistema (`Terminal=true`); en Omarchy se valida a mano (R65).
- El icono es un monograma geométrico original en la paleta de MAGI; no reproduce la pantalla MAGI de Evangelion, el logotipo de NERV ni ningún diseño de terceros.

---

## 7. Lógica de Negocio

### 7.1 Política de versiones (SemVer orientado al usuario)

| Cambio | Versión |
|---|---|
| Incompatible para el usuario: clave de `config.toml` quitada o con otro significado, subcomando u opción de CLI quitados, formato de `magi exportar` o de `magi_config` incompatible | MAJOR |
| Funcionalidad nueva, migración de `magi.db`, cambio de `VERSION_PROTOCOLO` | MINOR |
| Corrección sin migración ni cambio de protocolo | PATCH |

- El protocolo es interno (cliente y servidor salen del mismo binario): cambiarlo es MINOR, porque el saludo estable resuelve la convivencia (§7.2).
- No hay vuelta atrás: las migraciones solo van hacia delante y un binario se niega a abrir una `magi.db` de una versión más nueva (`user_version` mayor que la suya), sin tocarla.
- Cada cambio visible entra en la sección «Sin publicar» del CHANGELOG en el mismo PR. Al publicar, esa sección pasa a `[X.Y.Z] - fecha` y sus notas son las de la GitHub Release.

### 7.2 Decisión ante un servidor de otra versión

```python
CAMPOS_TRABAJO = ("sesiones", "transferencias", "tuneles", "ejecuciones")

def decidir(rechazo: dict, mi_protocolo: int) -> str:
    """rechazo: VersionIncompatible leído como JSON genérico."""
    if rechazo.get("version", 0) > mi_protocolo:
        return "mas_nuevo"            # este binario es el viejo: nunca para el servidor
    trabajo = rechazo.get("trabajo")
    clientes = rechazo.get("clientes")
    if trabajo is None or clientes is None:
        return "preguntar"            # servidor anterior a v6: no sabemos qué custodia
    if clientes == 0 and all(trabajo.get(c, 1) == 0 for c in CAMPOS_TRABAJO):
        return "sustituir"
    return "preguntar"

def sustituir(pid: int, esperar=5.0) -> bool:
    enviar_sigterm(pid)               # el servidor trata SIGTERM como Parar (F6)
    return esperar_libres(socket=True, lock=True, plazo=esperar)
    # True → autolanzar el servidor nuevo; False → diálogo con el error y el pid
```

- Un campo de `trabajo` que falte cuenta como «hay trabajo» (`get(c, 1)`), así un servidor futuro que añada otro tipo de trabajo nunca se para por error.
- `clientes = 0` es condición de la sustitución silenciosa: sin otras ventanas, nadie puede abrir una sesión entre el rechazo y el `SIGTERM`.
- El `pid` sale de `VersionIncompatible` o, si falta, de `peer_cred()` como desde la F6.

### 7.3 Comprobaciones de `magi diagnostico`

| Comprobación | ✓ | ! | ✕ |
|---|---|---|---|
| Versión | siempre (versión, protocolo, commit) | — | — |
| Datos, configuración, estado, runtime | existen o se pueden crear, permisos 700/600 | permisos más abiertos | no se puede crear o escribir |
| Socket | ruta < 100 bytes | 100-107 bytes | ≥ 108 bytes (R41) |
| `config.toml` | se lee bien o no existe | — | error de sintaxis (línea) |
| `magi.db` | `user_version` conocida | no existe («se creará al arrancar») | versión futura o no se puede abrir |
| Servidor | no está en marcha («se arrancará solo») o misma versión | otra versión | socket ocupado sin servidor que responda |
| Agente SSH | `SSH_AUTH_SOCK` con claves | sin claves o sin agente | — |
| Llavero | `secret-tool` (Linux) o `security` (macOS) presente | ausente («no se podrán recordar contraseñas») | — |
| Portapapeles | `wl-copy` o `pbcopy` | ausente | — |
| Editor y paginador | resolubles (`[archivos] editor`, `$VISUAL`, `$EDITOR`, `nvim`, `vi`; `$PAGER`, `less`) | ninguno | — |
| Terminal | `[terminal] comando` ejecutable | no encontrado | — |
| `~/.ssh/config` | contiene el `Include` de `magi_config` | no lo contiene | — |

Reglas: `magi.db` se abre en solo lectura; el servidor se consulta por el socket local con `servidor estado` (nunca se arranca); no se lee ningún secreto del llavero (solo se mira que la herramienta exista); ninguna conexión de red. Código de salida 1 si hay algún `✕`.

### 7.4 Recursos generados al día

```python
def recursos_al_dia(binario, assets) -> list[str]:
    esperados = {
        "completions/magi.bash": ejecutar(binario, "autocompletado", "bash"),
        "completions/_magi":     ejecutar(binario, "autocompletado", "zsh"),
        "completions/magi.fish": ejecutar(binario, "autocompletado", "fish"),
        "man/magi.1":            ejecutar(binario, "man"),
    }
    return [ruta for ruta, texto in esperados.items() if leer(assets / ruta) != texto]
    # test: la lista debe estar vacía; si no, «ejecuta scripts/generar-recursos.sh»
```

La página man no lleva fecha de generación variable (si `clap_mangen` la pone, se fija a la fecha de la versión) para que el test sea estable.

### 7.5 PKGBUILD de `magi-ssh-bin` (plantilla)

```bash
pkgname=magi-ssh-bin
pkgver=@VERSION@
pkgrel=1
pkgdesc="Gestor SSH de terminal con estética de cabina técnica"
arch=('x86_64' 'aarch64')
url="https://github.com/4d3-org/magi"
license=('MIT OR Apache-2.0')
depends=('glibc' 'gcc-libs')
optdepends=('wl-clipboard: copiar claves públicas en Wayland'
            'libsecret: recordar contraseñas en el llavero (secret-tool)'
            'openssh: usar ~/.ssh/magi_config con ssh fuera de MAGI')
provides=('magi-ssh')
conflicts=('magi-ssh')
install=magi-ssh-bin.install
source_x86_64=("$url/releases/download/v$pkgver/magi-ssh-x86_64-unknown-linux-gnu.tar.xz")
source_aarch64=("$url/releases/download/v$pkgver/magi-ssh-aarch64-unknown-linux-gnu.tar.xz")
sha256sums_x86_64=('@SHA256_X86_64@')
sha256sums_aarch64=('@SHA256_AARCH64@')

package() {
  cd "magi-ssh-$CARCH-unknown-linux-gnu"   # nombre del directorio: el que fije dist
  install -Dm755 magi "$pkgdir/usr/bin/magi"
  install -Dm644 assets/completions/magi.bash "$pkgdir/usr/share/bash-completion/completions/magi"
  install -Dm644 assets/completions/_magi     "$pkgdir/usr/share/zsh/site-functions/_magi"
  install -Dm644 assets/completions/magi.fish "$pkgdir/usr/share/fish/vendor_completions.d/magi.fish"
  install -Dm644 assets/man/magi.1            "$pkgdir/usr/share/man/man1/magi.1"
  install -Dm644 assets/magi.desktop          "$pkgdir/usr/share/applications/magi.desktop"
  install -Dm644 assets/iconos/magi.svg       "$pkgdir/usr/share/icons/hicolor/scalable/apps/magi.svg"
  install -Dm644 LICENSE-MIT    "$pkgdir/usr/share/licenses/$pkgname/LICENSE-MIT"
  install -Dm644 LICENSE-APACHE "$pkgdir/usr/share/licenses/$pkgname/LICENSE-APACHE"
}
```

`depends` lo confirma el agente con `namcap` y `ldd` sobre el binario real; si enlaza algo más, se añade.

### 7.6 Casos especiales

- **Binario más antiguo que su servidor** (por ejemplo, tras instalar a mano una versión vieja): aviso y TUI sin sesiones; nunca para el servidor (§7.2).
- **Volver a una versión anterior** con `magi.db` ya migrada: el binario viejo se niega a abrirla con un mensaje claro (§7.1). La solución es reinstalar la versión nueva.
- **Tag que no coincide con `Cargo.toml`**: el workflow falla en `plan` y no publica nada.
- **Falla un canal** tras la GitHub Release: la versión es válida; se relanza el job de ese canal (`docs/publicar.md`).
- **Nombre ocupado en AUR**: se para antes de la primera publicación y se consulta otro nombre (R60).
- **Tarball de macOS bajado con el navegador**: queda en cuarentena; el README explica `xattr -d com.apple.quarantine magi`. Con Homebrew no pasa.
- **macOS y autocompletado**: la fórmula que genera `dist` instala `magi`. Si no coloca el autocompletado y la man en las rutas de Homebrew, el README explica cómo activarlos desde `magi autocompletado` y `magi man`.
- **Desinstalar**: el paquete se va; los datos del usuario, `~/.ssh/magi_config` y la línea `Include` se quedan. Un `Include` de un fichero que no existe no rompe `ssh`. El README dice qué borrar a mano para empezar de cero.
- **Repositorio en otra cuenta**: se transfiere a `4d3-org` antes de publicar (GitHub redirige la URL antigua); `repository` de `Cargo.toml`, PKGBUILD y tap usan `4d3-org/magi`.

---

## 8. Requisitos No Funcionales

**Seguridad (cadena de suministro)**

- Los binarios publicados salen solo de CI a partir de un tag, con `--locked` (el `Cargo.lock` manda).
- SHA-256 y atestaciones de procedencia de GitHub por artefacto; el README enseña a verificarlas.
- `cargo deny check` en cada PR: licencias, avisos de RustSec y fuentes de crates permitidas.
- Workflows con permisos mínimos; los secretos solo dan acceso a su canal (token de grano fino al tap; clave SSH dedicada a AUR). Rotación documentada en `docs/publicar.md`.
- Auditoría con `gitleaks` de todo el historial antes de hacer público el repositorio; documentos y tests sin IPs reales, credenciales, correos personales ni rutas con nombres de usuario reales.
- El agente no cambia la visibilidad del repositorio, no crea secretos ni cuentas y no etiqueta versiones (D124).
- `SECURITY.md`: reporte privado de vulnerabilidades por GitHub.

**Protección de datos (RGPD).** Sin telemetría ni comprobación de actualizaciones (T10): MAGI no conecta con nada que no sean los hosts del usuario. Los paquetes no contienen datos personales; la instalación y la desinstalación no tocan los datos del usuario.

**Backups y recuperación.** Actualizar no migra nada por sí solo: la migración ocurre al abrir MAGI, como siempre, y hacia delante. Una `magi.db` más nueva que el binario no se abre ni se toca. El servidor anterior solo se para sin preguntar cuando no custodia nada.

**Rendimiento.** Una release completa debería terminar en menos de 30 minutos (compilaciones en paralelo); el tamaño de cada binario se mide y se anota en el informe de implementación. `magi diagnostico` responde en menos de 1 s sin servidor que consultar.

**Accesibilidad.** `magi diagnostico` con signos y texto, degradado ASCII y `NO_COLOR`; página man en castellano; el diálogo nuevo cumple T45.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| GitHub Actions | `ci.yml`, `release.yml` (dist), `publicar-aur.yml`; runners Ubuntu 22.04 x86_64 y arm64, macOS |
| GitHub Releases | Tarballs `.tar.xz` por objetivo, `.sha256`, notas del CHANGELOG |
| Atestaciones de GitHub | Procedencia Sigstore por artefacto; `gh attestation verify --repo 4d3-org/magi` |
| Homebrew | Tap `4d3-org/homebrew-tap`, fórmula `magi-ssh` (la escribe `dist`) |
| AUR | `magi-ssh-bin` por SSH (`aur@aur.archlinux.org`), PKGBUILD + `.SRCINFO` validados con `namcap` |
| `dist` | Configuración en el repositorio; versión de `dist` fijada en su configuración |
| `cargo-deny` | `deny.toml` |
| `gitleaks` | Auditoría única antes de publicar (y opcional en CI) |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D111 — Releases con `dist`.** Genera el workflow, los tarballs, los checksums, las atestaciones y la fórmula de Homebrew a partir de una configuración pequeña; sigue mantenido (0.33.1 en Arch, octubre de 2026). Descartado un workflow escrito a mano (más código propio que mantener) y compilar en local (sin trazabilidad) (Hector).
- **D112 — Paquete `magi-ssh`, comando `magi`, biblioteca `magi`.** El nombre `magi` parece ocupado en AUR y hay crates `magi-*` en crates.io. El comando no cambia para el usuario, los tests siguen usando `magi::` y las rutas de datos siguen siendo «magi» (Hector).
- **D113 — Linux con glibc dinámica en Ubuntu 22.04.** Funciona en Arch y en distros desde 2022 (glibc ≥ 2.35). Descartado musl estático: el backend criptográfico de russh y la resolución de nombres dan más sorpresas con musl, y el objetivo es Omarchy.
- **D114 — Solo `magi-ssh-bin` en AUR.** Instala en segundos sin compilar Rust en la máquina del usuario y reempaqueta exactamente el binario atestado. Descartado el paquete desde fuente en esta fase (Hector).
- **D115 — Homebrew con tap propio y sin notarizar.** Las fórmulas no reciben cuarentena y el linker firma ad hoc los binarios arm64. Descartada la notarización (cuenta de desarrollador de Apple, 99 $ al año) (Hector).
- **D116 — Sin crates.io.** Es una aplicación, no una biblioteca; `publish = false` evita publicarla por error (Hector).
- **D117 — Atestaciones de GitHub en vez de claves propias.** Sigstore da procedencia verificable sin custodiar una clave de firma. Descartadas minisign y GPG (Hector).
- **D118 — Primera versión pública `1.0.0`.** Decisión de Hector, aun con las Fases 2-8 sin validación manual completa y la F6 y la F8 sin revisión adversarial (R59).
- **D119 — Autocompletado y man generados por el binario y versionados en `assets/`.** `dist` empaqueta ficheros estáticos; un test garantiza que coinciden con el binario. Descartado generarlos en `build.rs` (no llegan a los tarballs) y generarlos al instalar (cada canal lo haría distinto).
- **D120 — Protocolo v6 con saludo estable.** `Hola` y `VersionIncompatible` no cambian nunca de forma y el rechazo informa del trabajo del servidor. Es lo mínimo para que cualquier versión futura reconozca a una anterior y decida sin perder sesiones.
- **D121 — Un servidor anterior se sustituye solo si no custodia nada.** Ni sesiones, ni transferencias, ni túneles, ni ejecuciones, ni otras ventanas. En cualquier otro caso decide el usuario con confirmación. Descartado que el paquete pare el servidor al actualizar (perdería sesiones) y descartado un servicio de systemd o LaunchAgent (Hector).
- **D122 — Sin comprobación de actualizaciones en la app.** Las trae el gestor de paquetes; consultar GitHub sería una conexión saliente fuera de los hosts del usuario (T10).
- **D123 — Licencia MIT OR Apache-2.0.** Lo habitual en Rust y compatible con las dependencias (Hector).
- **D124 — Las acciones irreversibles las hace el desarrollador.** Hacer público el repositorio, crear secretos o cuentas y etiquetar versiones quedan fuera del agente: lo prepara todo y lo documenta en `docs/publicar.md`.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Listo para distribuir | Nombre y metadatos, `1.0.0`, `--version` con commit, licencias, CHANGELOG y SECURITY; protocolo v6 con saludo estable; sustitución del servidor anterior y su diálogo; rechazo de una BD futura; `magi diagnostico`; `autocompletado` y `man`; `assets/` con `.desktop` e icono; tests e instantáneas | 1 semana |
| S2 — CI, releases y canales | `ci.yml` con macOS, `cargo deny` y plantilla AUR; `dist` con cuatro objetivos, atestaciones y Homebrew; `packaging/aur/` y `publicar-aur.yml`; README y `docs/publicar.md`; auditoría de publicación; revisión adversarial (T31, T54) | 1 semana |
| Desarrollador | Transferir y hacer público el repositorio, tap, cuenta de AUR, secretos, ensayo `v1.0.0-rc.1`, tag `v1.0.0` (§4.2) | ½ día |

**Total: 2 sprints, ~2 semanas.** Supuestos: la organización `4d3-org` existe en GitHub; el repositorio es público antes del ensayo (atestaciones y runners arm64 gratuitos); los tests de la suite actual pasan en los runners de Ubuntu con `sftp-server` instalado.

---

## 12. Conexiones con Otras Fases

- **F1:** cierra la funcionalidad pendiente «`cargo test` verde en CI de GitHub Actions (Linux)…» cuando se procese el informe de esta fase. «Compila y arranca en Linux y macOS…» sigue abierta: CI compila en macOS, pero falta arrancarlo en un equipo real (R62).
- **F3 y F6:** el saludo (`Hola`, `Bienvenida`, `VersionIncompatible`) y la parada por `SIGTERM` de la 3b son la base de la sustitución del servidor anterior.
- **F7:** el diálogo nuevo cumple T45.
- **F8:** se publica con su revisión adversarial y su validación pendientes (R50, R59). Validar la F8 antes de etiquetar `v1.0.0` reduciría el riesgo.
- **F10 (Sincronización cifrada, antes F9):** se distribuirá como MINOR, con migración de `magi.db`.
- **F11 (Android, antes F10):** su distribución (Play Store o F-Droid) se decide en su fase; esta fase no la cubre.
