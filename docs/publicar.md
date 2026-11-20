# Publicar MAGI

Esta guía la sigue el desarrollador para publicar una versión de MAGI. El
agente prepara el árbol, la documentación y los workflows; **las acciones
irreversibles o que necesitan credenciales las hace el desarrollador** (D124).
Repositorio: `HectorEspejo/magi`. Paquete: `magi-ssh` (el comando sigue siendo
`magi`).

Canales de distribución:

| Canal | Qué publica |
|---|---|
| GitHub Releases | Tarballs de Linux (x86_64/aarch64) y macOS (arm64/x86_64), `.sha256` y atestaciones de procedencia |
| GitHub Releases (Termux) | `magi-ssh_X.Y.Z_aarch64.deb` (`magi-ssh_X.Y.Z-rc.N_aarch64.deb` en una candidata), su `.sha256` y su atestación, en la misma release; los sube `termux-release.yml` (guía en [`publicar-termux.md`](publicar-termux.md)) |
| AUR | `magi-ssh-bin` (Omarchy y Arch) |
| Homebrew | Fórmula `magi-ssh` en el tap `HectorEspejo/homebrew-tap` (macOS) |

## 1. Reparto de responsabilidades

| Tarea | Quién |
|---|---|
| Fijar la versión, la fecha y el contenido de la release | Desarrollador |
| Actualizar `Cargo.toml`, el CHANGELOG y los recursos (`assets/`) | Agente |
| Pasar la auditoría de secretos y datos reales (§2) | Agente |
| Dejar CI, `dist`, PKGBUILD y fórmula preparados | Agente |
| Ajustes del repositorio (avisos privados, Dependabot, regla de `main`, ramas, issues, descripción y temas; §4) | Desarrollador |
| Crear el tap de Homebrew, la cuenta de AUR y sus claves | Desarrollador |
| Crear los secretos `HOMEBREW_TAP_TOKEN` y `AUR_SSH_PRIVATE_KEY` | Desarrollador |
| Etiquetar (`git tag`) y empujar el tag | Desarrollador |
| Relanzar jobs de publicación y verificar los canales | Desarrollador |
| Cambiar la visibilidad o los ajustes del repositorio, crear cuentas o secretos, subir binarios a mano, mover un tag | **Nadie más que el desarrollador**; el agente no lo hace |

## 2. Requisitos previos y auditoría (antes de la primera publicación)

El repositorio es público desde antes de la Fase 9b, así que su historial ya es
visible: lo que se protege desde ahora es lo que entra. Antes de etiquetar:

1. **CI verde en `main`**: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --locked -- -D warnings`,
   `TZ=UTC cargo test --workspace --locked`, el job de macOS,
   `cargo deny check` y la validación de la plantilla AUR.
2. **Auditoría de secretos en todo el historial.** `gitleaks` se pasó **una
   vez**, antes de la primera publicación; desde entonces, los secretos se
   vigilan en el momento de publicar y no en la CI:

   ```sh
   gitleaks git --redact --no-banner .
   ```

   Resultado de la primera publicación (20 de noviembre de 2026, 133 commits):
   14 hallazgos, **todos en ficheros de pruebas** (11 claves `age` de
   fixtures de sincronización y de la prueba de humo de Android, la clave de
   host fija del servidor de pruebas y el certificado de pruebas de RDP), con
   UUID y rutas inventadas y comentados como «solo pruebas». Si en una
   publicación posterior aparece **cualquier hallazgo nuevo** o uno de esos
   parece una clave real: **PARAR**, no etiquetar, no reescribir el historial
   por cuenta propia y avisar al desarrollador con el commit y el fichero
   exactos. La decisión de rotar el secreto es suya (el historial no se
   reescribe, D391).

   Nota: no uses `--no-git` para esta comprobación; sin git, gitleaks escanea
   el árbol de trabajo (incluido `target/`) y da falsos positivos con claves
   embebidas en artefactos de compilación de dependencias.

3. **Datos reales.** La documentación, los tests y los ejemplos usan datos de
   ejemplo: IPs `192.0.2.x`, dominios `ejemplo.org`, el usuario `usuario` y
   rutas `/home/usuario`. La prueba `repositorio_publico` lo comprueba en cada
   PR (ninguna IP pública, ninguna ruta `/home/<nombre>`, ningún correo fuera
   de su lista y ninguna referencia a la organización anterior de GitHub fuera
   del archivo de diseño, además de los enlaces relativos de la documentación):

   ```sh
   cargo test -p magi-ssh --test repositorio_publico
   ```

   Cada excepción vive en la propia prueba, con su motivo.

4. **Revisión adversarial** del código y los workflows nuevos (sustitución del
   servidor, permisos de CI, cadena de suministro y PKGBUILD), con el resultado
   anotado en el informe de implementación de la fase.

5. **Licencias**: `LICENSE-MIT` y `LICENSE-APACHE` presentes, `cargo deny
   check` limpio y `license = "MIT OR Apache-2.0"` en `Cargo.toml`.

## 3. Política de versiones (SemVer orientado al usuario)

| Cambio | Versión |
|---|---|
| Incompatible para el usuario: clave de `config.toml` quitada o con otro significado, subcomando u opción de CLI quitados, formato de `magi exportar` o de `magi_config` incompatible | **MAJOR** |
| Funcionalidad nueva, migración de `magi.db`, cambio de `VERSION_PROTOCOLO` | **MINOR** |
| Corrección sin migración ni cambio de protocolo | **PATCH** |

- El protocolo es interno (cliente y servidor salen del mismo binario):
  cambiarlo es **MINOR**, porque el saludo estable (`Hola` y
  `VersionIncompatible`) resuelve la convivencia entre versiones.
- No hay vuelta atrás: las migraciones solo van hacia delante y un binario se
  niega a abrir una `magi.db` de una versión más nueva (`user_version` mayor
  que la suya), sin tocarla.
- Cada cambio visible añade su línea en la sección **«Sin publicar»** de
  `CHANGELOG.md` en el mismo PR. Al publicar, esa sección pasa a
  `[X.Y.Z] - fecha` y sus notas son las de la GitHub Release.
- El tag es `vX.Y.Z` (sin prefijo extra). Una prerelease es
  `vX.Y.Z-rc.N` y se marca como *prerelease* en GitHub: no toca Homebrew ni
  AUR.
- **Un tag publicado no se mueve ni se reutiliza.** Si hace falta cambiar el
  contenido, se publica un PATCH nuevo.
- Se mantiene la última MINOR de 1.x; el resto deja de recibir correcciones
  (ver [`SECURITY.md`](../SECURITY.md)).

## 4. Primera publicación (una sola vez)

El repositorio ya es público y vive en `HectorEspejo/magi` (no hay
transferencia). La primera release del escritorio es la **1.6.1** y la de la
app, la **0.2.0** en beta (guía en [`publicar-android.md`](publicar-android.md)).
Se hace una sola vez, en este orden, después de completar §2. Lo hace el
desarrollador: el agente no cambia ajustes del repositorio, no crea secretos ni
cuentas y no empuja etiquetas.

1. **Ajustes del repositorio** (Settings), todos a mano:

   | Ajuste | Valor |
   |---|---|
   | Avisos privados de vulnerabilidades | Activados (Security → Private vulnerability reporting) |
   | Dependabot | Alertas y actualizaciones **de seguridad** activadas; **sin** `dependabot.yml` (nada de actualizaciones de versión). Una PR que toque una versión fijada (russh, ratatui, crossterm, tui-term, vt100, rusqlite, IronRDP, `age`, UniFFI) no se fusiona tal cual: se cierra y se sube el conjunto a mano con sus pruebas |
   | Regla de `main` | PR obligatoria y estos checks de `ci.yml`, **con el nombre con que salen en la PR**: `Formato`, `Clippy`, `Tests Linux (ubuntu-22.04)`, `Tests Linux (ubuntu-24.04)` (son los dos del job `tests-linux`), `macOS (arm64)`, `Visor web (TypeScript)`, `Licencias y avisos (cargo-deny)` y `Plantilla AUR`. Sin `push --force` ni borrado de `main`. **No** se exigen los de `android.yml` (tienen filtros de rutas y bloquearían las PR que no los disparan) ni `¿Toca el paquete de Termux?` y `Paquete de Termux (.deb)`, que son condicionales. Si renombras, quitas o añades un job de `ci.yml`, cambia aquí la regla |
   | Ramas | Borrar la rama al fusionar; borrar las ya fusionadas (`fase*`, `correccion-*`), menos las abiertas |
   | Issues | Activadas (plantillas en `.github/ISSUE_TEMPLATE/`, sin issues en blanco); Discussions y wiki, desactivadas |
   | Descripción y temas | Una línea de qué es; temas `ssh`, `tui`, `terminal`, `rust`, `sftp`, `rdp`, `vnc` y `android` |

2. **Comprobar que los nombres están libres.** Repite esta comprobación justo
   antes de publicar, porque los nombres pueden ocuparse:

   ```sh
   # AUR: magi-ssh-bin y magi-ssh deben estar libres
   curl -fsS 'https://aur.archlinux.org/rpc/?v=5&type=info&arg[]=magi-ssh-bin&arg[]=magi-ssh&arg[]=magi' | jq .
   # Repos oficiales de Arch: ningún paquete con esos nombres
   curl -fsS 'https://archlinux.org/packages/search/json/?name=magi-ssh-bin' | jq '.results | length'
   # Que ningún paquete oficial instale /usr/bin/magi
   sudo pacman -Fy && pacman -F /usr/bin/magi
   ```

   Comprobación del **30 de octubre de 2026**: en AUR solo está ocupado
   `magi` (una TUI de Git); `magi-ssh-bin` y `magi-ssh` están libres. En los
   repos oficiales de Arch, `magi`, `magi-ssh` y `magi-ssh-bin` dan 0
   resultados y ningún paquete oficial instala `/usr/bin/magi`. Si alguno
   estuviera ocupado, **PARAR** y consultar otro nombre antes de publicar.

3. **Comprobar de quién es la organización anterior de GitHub** (R182). La
   Fase 9 preparó la publicación en una organización cuyo nombre y direcciones
   conservan los documentos de `docs/faseN/` y el maestro, y que ya no valen:
   [`docs/README.md`](README.md) lo explica y da su nombre. Mira esa
   organización en `https://github.com/<nombre>`: si está **libre**, créala
   vacía para reservarla, de modo que nadie pueda ocuparla y dar sentido a esas
   direcciones; si es de otra persona, no hace falta nada más (nada del código
   ni de los workflows la usa).

4. **Crear el tap `HectorEspejo/homebrew-tap`** (público, con un README
   mínimo). En cada release estable, `dist` empuja `Formula/magi-ssh.rb` con el
   secreto `HOMEBREW_TAP_TOKEN`. El comando de instalación queda
   `brew install hectorespejo/tap/magi-ssh`.

5. **Crear la cuenta de AUR y su clave SSH.** Registra la cuenta en
   <https://aur.archlinux.org/> con tu correo y una clave pública SSH. Genera
   una **clave dedicada para CI** (sin frase, solo para publicar en AUR),
   registra la pública en el perfil de AUR y guarda la privada como secreto:

   ```sh
   ssh-keygen -t ed25519 -C "aur-magi-ssh" -f ~/.ssh/aur_magi -N ""
   cat ~/.ssh/aur_magi.pub   # pégala en «SSH Public Key» del perfil de AUR
   ```

   El correo del mantenedor del PKGBUILD y el de los commits a AUR es
   `hola@4d3.org`. La clave de host de `aur.archlinux.org` la añade el workflow
   con `ssh-keyscan`, pero **comprueba sus huellas** contra las que AUR publica
   en <https://aur.archlinux.org> (ED25519, ECDSA y RSA): si un intermediario
   sirviera otra clave, el job se para. No versionamos `known_hosts`. Si AUR
   rotara sus claves, actualiza la lista `esperadas` de
   `.github/workflows/publicar-aur.yml` con las nuevas huellas publicadas; no
   desactives esa comprobación.

6. **Crear los secretos del repositorio** (Settings → Secrets and variables →
   Actions):

   | Secreto | Qué es | Alcance mínimo |
   |---|---|---|
   | `HOMEBREW_TAP_TOKEN` | Token de grano fino de GitHub (fine-grained PAT) | Solo el repositorio `HectorEspejo/homebrew-tap`, permiso **Contents: Read and write**; nada más. Caducidad de 1 año |
   | `AUR_SSH_PRIVATE_KEY` | Clave privada dedicada del paso 5, con sus líneas `BEGIN`/`END` | Solo la cuenta de AUR del proyecto |
   | `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` y `ANDROID_KEY_PASSWORD` | El keystore de la app y sus claves (se crean según [`publicar-android.md`](publicar-android.md) §3; solo para la app) | Solo `android-release.yml` |

   No pegues secretos en issues, PRs, logs ni en la documentación. Si un job
   los imprime, considéralos comprometidos y rótalos (§8).

7. **Guion corto de macOS** (D395), en el mac mini, con un binario compilado
   allí desde el commit de la release:

   1. `cargo build --release --locked`; `magi --version` y `magi diagnostico`.
   2. Conectar a un host con una clave del agente y a otro con una contraseña
      recordada en el Llavero.
   3. Abrir dos pestañas, cerrar la ventana y volver con `magi`: siguen ahí.
   4. SFTP: subir y bajar un fichero; un túnel local.
   5. `magi servidor parar`.

   Si falla el paso 1 o el 2, macOS **sale de esta release**: quita sus dos
   objetivos de `dist-workspace.toml` y de `release.yml` (y el job de Homebrew)
   y vuelven en una PATCH. Si el fallo es menor, sale «en pruebas» y se anota
   en [`manual/estado.md`](manual/estado.md).

8. **Candidata `v1.6.1-rc.1`** (obligatoria antes de la primera estable: es
   también la primera publicación del `.deb` de Termux, que siempre pasa por una
   candidata; ver [`publicar-termux.md`](publicar-termux.md) §4). `dist` exige
   que el tag coincida con la `version` de `Cargo.toml`:

   ```sh
   # Cargo.toml: version = "1.6.1-rc.1"  (y Cargo.lock al día)
   # commit + PR + CI verde + merge a main
   git tag -a v1.6.1-rc.1 -m "MAGI 1.6.1-rc.1"
   git push origin v1.6.1-rc.1
   ```

   Cambiar la `version` a una candidata arrastra lo que lista `CLAUDE.md` para
   un cambio de versión: `FECHA_DE_VERSION`, `assets/` (la página man), las
   instantáneas `servidor_anterior_*` y la DA2 de `tests/fijos/terminal/
   tui_prueba.rs`, que para `1.6.1-rc.1` es **`10600`** (el emulador no lee el
   parche `1-rc`) y para `1.6.1`, `10601`. El paso 9 lo deshace. Sin ello, la CI de
   esa PR falla en `terminal_e2e`, `recursos` y `redimensionado`; se lo puedes
   pedir al agente, que lo hace en el mismo PR.

   El resultado debe ser una **GitHub Release marcada como prerelease**, con
   tarballs, `.sha256`, atestaciones y el `.deb` de Termux
   (`magi-ssh_1.6.1-rc.1_aarch64.deb`), **sin tocar Homebrew ni AUR**. Descarga
   un tarball, verifícalo (`sha256sum -c` y `gh attestation verify`) e
   instálalo a mano; comprueba `magi --version` y `magi diagnostico`. Si algo
   falla, corrige y repite con `v1.6.1-rc.2` (nunca muevas el tag).

9. **Publicar `v1.6.1`**: deja `version = "1.6.1"` en `Cargo.toml` (y
   `Cargo.lock`), pon la fecha del día en el encabezado `## [1.6.1] - fecha` del
   CHANGELOG, PR, CI verde, merge y:

   ```sh
   git tag -a v1.6.1 -m "MAGI 1.6.1"
   git push origin v1.6.1
   ```

   Vigila `release.yml` y Release Termux hasta el final: release de GitHub
   («Latest»), fórmula de Homebrew, PKGBUILD en AUR y el `.deb`. Después, haz
   las comprobaciones de §9.

10. **La app Android**: `android-v0.2.0-rc.1`, su guion corto en el plegable y
    `android-v0.2.0`, según [`publicar-android.md`](publicar-android.md). Nunca
    se marcan como «Latest».

Orden de las etiquetas: `v1.6.1-rc.1` → `v1.6.1` → `android-v0.2.0-rc.1` →
`android-v0.2.0`.

## 5. Flujo de una release (rutina)

1. Los cambios entran en `main` con su línea en «Sin publicar» del CHANGELOG.
2. Decide la versión según §3; actualiza `version` en `Cargo.toml` y pasa la
   sección «Sin publicar» a `[X.Y.Z] - fecha` (con su enlace de comparación al
   final del CHANGELOG).
3. PR → `ci.yml` verde → merge a `main`.
4. Crea el tag anotado y empújalo:

   ```sh
   git tag -a vX.Y.Z -m "MAGI X.Y.Z"
   git push origin vX.Y.Z
   ```

5. `release.yml` (generado por `dist` y con ajustes propios) hace el resto:
   - `plan`: **falla si el tag no coincide con la versión de `Cargo.toml`** y
     no publica nada; además comprueba que el tag tiene la forma `vX.Y.Z` (o
     `vX.Y.Z-sufijo`) antes de usar su valor en ningún script y que
     `Cargo.lock` está al día con `cargo metadata --locked`;
   - compila los cuatro objetivos (Linux x86_64 y aarch64 en Ubuntu 22.04;
     macOS arm64 y x86_64); si falla uno, no hay release;
   - crea la GitHub Release con los tarballs, sus `.sha256`, las atestaciones y
     las notas de la sección del CHANGELOG;
   - **solo en versiones estables**, publica la fórmula en el tap de Homebrew y
     llama a `publicar-aur.yml`, que rellena el PKGBUILD con la versión y las
     sumas, genera `.SRCINFO` en un contenedor Arch, pasa `namcap` (y falla si
     namcap informa de errores `E:`) y empuja a AUR.

   El mismo tag lanza a la vez `termux-release.yml` (**Release Termux**), que
   compila el `.deb` para Termux, lo atesta y lo sube a la release cuando
   `release.yml` la crea. Vigila también Release Termux; ver
   [`publicar-termux.md`](publicar-termux.md).
6. Haz las comprobaciones de §9.

Los binarios publicados salen **solo** del CI a partir del tag: no se suben
binarios a mano ni se publica en crates.io (`publish = false`).

## 6. Workflows, permisos y secretos

| Workflow | Disparo | Qué hace |
|---|---|---|
| `ci.yml` | PR y push a `main` | `fmt`, `clippy -D warnings`, `TZ=UTC cargo test --locked` en Linux (instala `openssh-server`, por su `sftp-server`, y `tmux`: sin ellos, las pruebas de SFTP y las de sesiones persistentes con tmux de verdad se omiten), macOS (build, clippy y `cargo test --lib`), `cargo deny check` y validación de la plantilla AUR (`makepkg --printsrcinfo` y `namcap` en contenedor Arch) |
| `release.yml` | tag `v*` | Lo genera `dist` 0.33.1: `plan`, compilación por objetivo, checksums y fórmula, GitHub Release, Homebrew y `publicar-aur` |
| `publicar-aur.yml` | lo llama `release.yml` | Dos jobs: `empaquetar` (contenedor Arch sin secretos: rellena versión y sumas del PKGBUILD, genera `.SRCINFO`, pasa `namcap` como gate) y `publicar` (runner normal con `AUR_SSH_PRIVATE_KEY`: recibe el artefacto y hace `git push` a `aur@aur.archlinux.org:magi-ssh-bin.git`) |
| `termux-release.yml` | tag `vX.Y.Z` y `vX.Y.Z-rc.N` (nunca `android-v*`) y `workflow_dispatch` | Compila para `aarch64-linux-android` con el NDK r30, arma `magi-ssh_X.Y.Z_aarch64.deb` (`magi-ssh_X.Y.Z-rc.N_aarch64.deb` en una candidata) con `scripts/paquete-termux.sh`, comprueba su estructura, escribe el `.sha256`, atesta la procedencia y lo sube a la release del tag (espera hasta 60 min a que `release.yml` la cree; justo antes de subir comprueba que el tag sigue en el commit compilado y, si no, falla; falla si los adjuntos no quedan con el nombre esperado; nunca toca «Latest»), solo con `GITHUB_TOKEN`. La CI de cada PR tiene su job `termux` en `ci.yml`. Guía completa y receta de TUR en [`publicar-termux.md`](publicar-termux.md) |

Permisos: `contents: read` por defecto en todos los workflows, y cada job que
necesita algo más lo declara él mismo, nunca arriba. Esto es lo que hay
(todo lo que no aparece es solo lectura):

| Workflow | Job | Permisos añadidos |
|---|---|---|
| `ci.yml`, `android.yml` y `publicar-aur.yml` | todos | ninguno |
| `release.yml` | `plan` | `contents: write` (crea la GitHub Release en borrador) |
| | `build-local-artifacts` | `id-token: write` y `attestations: write` (atesta los artefactos) |
| | `host` | `contents: write` (sube los artefactos y publica la release) |
| | `custom-publicar-aur` | ninguno (llama a `publicar-aur.yml`, que solo recibe `AUR_SSH_PRIVATE_KEY`) |
| `android-release.yml` | `compilar` | `id-token: write` y `attestations: write` |
| | `publicar` | `contents: write` (crea la release de Android) |
| `termux-release.yml` | `compilar` | `id-token: write` y `attestations: write` |
| | `publicar` | `contents: write` (sube el `.deb` y su `.sha256` a la release que crea `release.yml`; nunca la crea ni toca «Latest») |

Los demás jobs (`verificar`, `build-global-artifacts`, `publish-homebrew-formula`,
`announce`…) no escriben con `GITHUB_TOKEN`; el de Homebrew usa su propio
secreto, `HOMEBREW_TAP_TOKEN`. Ningún workflow que escriba se dispara en pull
requests: `release.yml`, `android-release.yml` y `termux-release.yml` solo
arrancan con una etiqueta (`termux-release.yml` también a mano, con
`workflow_dispatch`), así que ningún PR necesita permisos de escritura. No
añadas secretos a `ci.yml`: los PRs de forks no los reciben y no los necesitan.

`release.yml` **sí se mantiene a mano** en algunos detalles (está marcado con
`allow-dirty = ["ci"]` en `dist-workspace.toml`): permisos mínimos por job,
validación del formato del tag, paso de secretos a AUR y comprobación del lock.
Si actualizas `dist`, regenera el workflow (`dist init` / `dist generate`) y
vuelve a aplicar esos ajustes; la cabecera del fichero los lista. No quites
`allow-dirty` sin reaplicarlos, porque la regeneración los borraría.

## 7. Si falla un canal

Un error **después** de crear la GitHub Release **no invalida la versión**: la
release y sus binarios ya son válidos y solo hay que relanzar el job del canal
que falló.

- Relanza solo los jobs fallidos: en GitHub, Actions → el run de la release →
  **Re-run failed jobs**; o `gh run rerun <run-id> --failed`.
- Un fallo **antes** de crear la release (por ejemplo, en `plan` o en una
  compilación) deja la versión sin publicar. Se corrige con un commit y un tag
  nuevo: **nunca moviendo el tag**. Si la release no llegó a crearse, puedes
  borrar el tag erróneo (`git push origin :refs/tags/vX.Y.Z`) y etiquetar de
  nuevo el commit corregido; si la release ya existe, el tag es inmutable y hay
  que publicar un PATCH.
  - **No recrees un tag con una ejecución de «Release Termux» suya en curso o en
    cola: cancélala antes** (Actions → Release Termux → la ejecución → Cancel
    workflow). Si no, esa ejecución sigue esperando la release y, cuando aparece la
    del commit nuevo, intentaría subirle un `.deb` compilado desde el commit
    antiguo (el workflow lo detecta y falla sin subir nada, pero mientras tanto
    retiene la cola de ese tag).
  - Si el fallo fue transitorio (red, un runner), es mejor **Re-run failed jobs**
    del workflow que falló: conserva el mismo tag y el mismo commit y no obliga a
    borrar nada. Recrear el tag solo hace falta si lo que falló está en el código
    o en el YAML del commit etiquetado.
- Si falla Homebrew: revisa que el tap exista, que `HOMEBREW_TAP_TOKEN` no haya
  caducado y que tenga **Contents: Read and write** solo en
  `HectorEspejo/homebrew-tap`; corrige y relanza el job. Si la fórmula necesita un
  arreglo manual en el tap, hazlo y vuelve a lanzar el job para que quede
  versionada.
- Si falla AUR: el workflow tiene dos jobs. Si falla `empaquetar` (contenedor
  Arch), revisa que `.SRCINFO` se genere, que `namcap` no dé errores `E:` y
  que el PKGBUILD no tenga placeholders; si falla `publicar`, revisa
  `AUR_SSH_PRIVATE_KEY` (clave registrada en el perfil de AUR), que el nombre
  `magi-ssh-bin` siga libre y las huellas de AUR. Relanza el job fallido.
- Mientras un canal esté roto, sus usuarios siguen en la versión anterior (no
  hay release a medias: cada canal publica la versión completa o no publica).
- No borres ni reemplaces la GitHub Release para «arreglar» un canal: los
  binarios ya publicados están atestados y sus `.sha256` son los que verifican
  los usuarios.

## 8. Rotación de secretos

Rota un secreto si caduca, si sospechas que se ha filtrado, si alguien con
acceso deja el proyecto o si un job lo ha impreso por error. Rotar **no**
invalida las releases ya publicadas.

### `HOMEBREW_TAP_TOKEN`

1. Revoca el token viejo en GitHub (Settings → Developer settings → Personal
   access tokens → Fine-grained tokens).
2. Crea uno nuevo con el mismo alcance mínimo: solo el repositorio
   `HectorEspejo/homebrew-tap` y permiso **Contents: Read and write**, con
   caducidad.
3. Actualiza el secreto del repositorio (Settings → Secrets and variables →
   Actions → `HOMEBREW_TAP_TOKEN` → Update).
4. Comprueba con un relanzado del job de Homebrew de la última release (debe
   terminar en verde) y revisa que la fórmula del tap apunta a la versión
   correcta.

### `AUR_SSH_PRIVATE_KEY`

1. Genera un par nuevo dedicado a CI (sin frase):
   `ssh-keygen -t ed25519 -C "aur-magi-ssh-$(date +%Y)" -f ~/.ssh/aur_magi -N ""`.
2. Sustituye la clave pública en el perfil de AUR por la nueva.
3. Actualiza el secreto `AUR_SSH_PRIVATE_KEY` con la privada nueva.
4. Comprueba el acceso:
   `git clone ssh://aur@aur.archlinux.org/magi-ssh-bin.git` y relanza el job de
   AUR de la última release.
5. Retira la clave vieja del perfil de AUR.

### Después de una filtración

- Rota el secreto afectado y revisa el tap y el paquete de AUR por cambios que
  no reconozcas (commits, `.SRCINFO`, PKGBUILD, fórmula).
- Las atestaciones permiten comprobar que un binario ya publicado salió del CI
  del repositorio; si un binario no verifica, retíralo de la release y publica
  un PATCH.
- Nunca pegues un secreto en un issue, un PR, un log de CI ni en esta guía.

## 9. Comprobaciones posteriores a publicar

En una máquina limpia (o al menos con el canal recién actualizado):

```sh
# Arch / Omarchy
yay -S magi-ssh-bin
magi --version                    # magi 1.6.1 (protocolo 10, <commit>)
pacman -Ql magi-ssh-bin           # binario, completions, man, .desktop, icono, licencias
magi diagnostico

# macOS
brew update && brew install hectorespejo/tap/magi-ssh
magi --version

# Tarball
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo HectorEspejo/magi

# App Android (la guía completa está en publicar-android.md)
sha256sum -c magi-android-v0.2.0.apk.sha256
apksigner verify --print-certs magi-android-v0.2.0.apk
gh attestation verify magi-android-v0.2.0.apk --repo HectorEspejo/magi
```

Comprueba además:

- la GitHub Release tiene los cuatro tarballs, sus `.sha256`, las notas de la
  sección del CHANGELOG y, si era una prerelease, está marcada como tal;
- la misma release lleva también el `.deb` y su `.sha256` para Termux
  (`magi-ssh_X.Y.Z_aarch64.deb`, con guion en una candidata:
  `magi-ssh_X.Y.Z-rc.N_aarch64.deb`), que sube `termux-release.yml` cuando
  `release.yml` ya ha creado la release; comprobaciones concretas en
  [`publicar-termux.md`](publicar-termux.md) (§10);
- `gh release view vX.Y.Z` y `gh attestation verify` funcionan para cada
  objetivo;
- el tap publica la versión nueva y AUR la muestra en
  <https://aur.archlinux.org/packages/magi-ssh-bin>;
- una prerelease **no** ha modificado el tap ni AUR;
- `magi --version` coincide con el tag y con `Cargo.toml`.

Si algo falla, vuelve a §7: la release no se invalida y el tag no se mueve.

## 10. Comprobación de nombres (registro)

Comprobación hecha el **30 de octubre de 2026** con:

```sh
curl -fsS 'https://aur.archlinux.org/rpc/?v=5&type=info&arg[]=magi-ssh-bin&arg[]=magi-ssh&arg[]=magi'
curl -fsS 'https://archlinux.org/packages/search/json/?name=magi-ssh-bin'
```

Resultado:

- AUR: solo `magi` ocupado (TUI de Git); `magi-ssh-bin` y `magi-ssh`
  libres. Ojo: el paquete `magi` también instala `/usr/bin/magi`, así que el
  PKGBUILD de `magi-ssh-bin` declara `conflicts=('magi')` además de
  `conflicts=('magi-ssh')`; si el usuario tiene `magi` instalado, pacman le
  pedirá quitarlo antes de instalar `magi-ssh-bin`.
- Repos oficiales de Arch: 0 resultados para `magi`, `magi-ssh` y
  `magi-ssh-bin`; ningún paquete oficial instala `/usr/bin/magi`.

Repite la comprobación antes de la primera publicación: la disponibilidad de
nombres cambia con el tiempo.

## 11. Referencias

- Informe de la Fase 9: §4.1 (publicar una versión), §4.2 (primera
  publicación), §5.3 (workflows), §7.1 (política de versiones) y §7.5
  (PKGBUILD de `magi-ssh-bin`).
- [`SECURITY.md`](../SECURITY.md): cómo reportar vulnerabilidades y versiones
  soportadas.
- [`CHANGELOG.md`](../CHANGELOG.md): formato Keep a Changelog y notas de cada
  release.
- `dist`: <https://opensource.axo.dev/cargo-dist/>
- Atestaciones de procedencia de GitHub:
  <https://docs.github.com/es/actions/security-for-github-actions/using-artifact-attestations>
- AUR: <https://wiki.archlinux.org/title/AUR_submission_guidelines>
- Tap de Homebrew: <https://docs.brew.sh/Taps>
