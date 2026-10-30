# Publicar MAGI

Esta guía la sigue el desarrollador para publicar una versión de MAGI. El
agente prepara el árbol, la documentación y los workflows; **las acciones
irreversibles o que necesitan credenciales las hace el desarrollador** (D124).
Repositorio: `4d3-org/magi`. Paquete: `magi-ssh` (el comando sigue siendo
`magi`).

Canales de distribución:

| Canal | Qué publica |
|---|---|
| GitHub Releases | Tarballs de Linux (x86_64/aarch64) y macOS (arm64/x86_64), `.sha256` y atestaciones de procedencia |
| AUR | `magi-ssh-bin` (Omarchy y Arch) |
| Homebrew | Fórmula `magi-ssh` en el tap `4d3-org/homebrew-tap` (macOS) |

## 1. Reparto de responsabilidades

| Tarea | Quién |
|---|---|
| Fijar la versión, la fecha y el contenido de la release | Desarrollador |
| Actualizar `Cargo.toml`, el CHANGELOG y los recursos (`assets/`) | Agente |
| Pasar la auditoría de secretos y datos reales (§2) | Agente |
| Dejar CI, `dist`, PKGBUILD y fórmula preparados | Agente |
| Transferir el repositorio a `4d3-org` y hacerlo público | Desarrollador |
| Crear el tap de Homebrew, la cuenta de AUR y sus claves | Desarrollador |
| Crear los secretos `HOMEBREW_TAP_TOKEN` y `AUR_SSH_PRIVATE_KEY` | Desarrollador |
| Etiquetar (`git tag`) y empujar el tag | Desarrollador |
| Relanzar jobs de publicación y verificar los canales | Desarrollador |
| Cambiar la visibilidad del repositorio, crear cuentas o secretos, subir binarios a mano, mover un tag | **Nadie más que el desarrollador**; el agente no lo hace |

## 2. Requisitos previos y auditoría (antes de hacer público)

Hacer público el repositorio es irreversible en la práctica: el historial
completo pasa a ser visible. Antes de pulsar el botón:

1. **CI verde en `main`**: `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings`, `TZ=UTC cargo test --locked`,
   el job de macOS, `cargo deny check` y la validación de la plantilla AUR.
2. **Auditoría de secretos en todo el historial**. Es requisito previo a hacer
   público:

   ```sh
   gitleaks detect --source . --log-opts="--all" --no-banner --redact
   ```

   Resultado esperado: `no leaks found` y código de salida 0. Si aparece
   **cualquier** hallazgo: **PARAR**, no hacer público el repositorio, no
   reescribir el historial por cuenta propia y avisar al desarrollador con el
   commit y el fichero exactos. La decisión de rotar el secreto y reescribir el
   historial es suya.

   Nota: no uses `--no-git` para esta comprobación; sin git, gitleaks escanea
   el árbol de trabajo (incluido `target/`) y da falsos positivos con claves
   embebidas en artefactos de compilación de dependencias.

3. **Datos reales**. La documentación, los tests y los ejemplos usan datos de
   ejemplo: IPs `192.0.2.x`, dominios `ejemplo.org` y rutas `/home/usuario`.
   Revisa que no queden IPs reales, correos personales, credenciales ni rutas
   con nombres de usuario reales:

   ```sh
   git grep -nE '\b([0-9]{1,3}\.){3}[0-9]{1,3}\b' -- docs README.md tests src
   git grep -nE '/(home|Users)/[A-Za-z0-9._-]+' -- docs README.md tests src
   git grep -nE '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}' -- docs README.md tests src
   ```

   Las IPs de documentación (rangos `192.0.2.0/24`, `198.51.100.0/24` y
   `203.0.113.0/24`) y `127.0.0.1`/`0.0.0.0` son correctas.

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

Se hace una sola vez, en este orden, después de completar §2.

1. **Transferir el repositorio a `4d3-org`** si aún vive en otra cuenta
   (Settings → General → Transfer ownership). GitHub redirige la URL antigua;
   `repository` y `homepage` de `Cargo.toml` ya apuntan a
   `https://github.com/4d3-org/magi`. Comprueba que los secretos de Actions y
   los workflows siguen ahí después de la transferencia.

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

3. **Hacer público el repositorio** (Settings → General → Danger Zone → Change
   visibility), solo después de §2.

4. **Crear el tap `4d3-org/homebrew-tap`** (público, con un README mínimo). En
   cada release estable, `dist` empuja `Formula/magi-ssh.rb` con el secreto
   `HOMEBREW_TAP_TOKEN`.

5. **Crear la cuenta de AUR y su clave SSH.** Registra la cuenta en
   <https://aur.archlinux.org/> con tu correo y una clave pública SSH. Genera
   una **clave dedicada para CI** (sin frase, solo para publicar en AUR),
   registra la pública en el perfil de AUR y guarda la privada como secreto:

   ```sh
   ssh-keygen -t ed25519 -C "aur-magi-ssh" -f ~/.ssh/aur_magi -N ""
   cat ~/.ssh/aur_magi.pub   # pégala en «SSH Public Key» del perfil de AUR
   ```

   La clave de host de `aur.archlinux.org` la añade el workflow con
   `ssh-keyscan`, pero **comprueba sus huellas** contra las que AUR publica en
   <https://aur.archlinux.org> (ED25519, ECDSA y RSA): si un intermediario
   sirviera otra clave, el job se para. No versionamos `known_hosts`. Si AUR
   rotara sus claves, actualiza la lista `esperadas` de
   `.github/workflows/publicar-aur.yml` con las nuevas huellas publicadas; no
   desactives esa comprobación.

6. **Crear los secretos del repositorio** (Settings → Secrets and variables →
   Actions):

   | Secreto | Qué es | Alcance mínimo |
   |---|---|---|
   | `HOMEBREW_TAP_TOKEN` | Token de grano fino de GitHub (fine-grained PAT) | Solo el repositorio `4d3-org/homebrew-tap`, permiso **Contents: Read and write**; nada más. Caducidad de 1 año |
   | `AUR_SSH_PRIVATE_KEY` | Clave privada dedicada del paso 5, con sus líneas `BEGIN`/`END` | Solo la cuenta de AUR del proyecto |

   No pegues secretos en issues, PRs, logs ni en la documentación. Si un job
   los imprime, considéralos comprometidos y rótalos (§8).

7. **Proteger `main`**: PR obligatorio y `ci.yml` requerido antes de fusionar.

8. **Ensayo con `v1.0.0-rc.1`** (obligatorio antes de la primera estable).
   `dist` exige que el tag coincida con la `version` de `Cargo.toml`:

   ```sh
   # Cargo.toml: version = "1.0.0-rc.1"
   # commit + PR + CI verde + merge a main
   git tag -a v1.0.0-rc.1 -m "MAGI 1.0.0-rc.1"
   git push origin v1.0.0-rc.1
   ```

   El resultado debe ser una **GitHub Release marcada como prerelease**, con
   tarballs, `.sha256` y atestaciones, **sin tocar Homebrew ni AUR**. Descarga
   un tarball, verifícalo (`sha256sum -c` y `gh attestation verify`) e
   instálalo a mano; comprueba `magi --version` y `magi diagnostico`. Si algo
   falla, corrige y repite el ensayo con `v1.0.0-rc.2` (nunca muevas el tag).

9. **Publicar `v1.0.0`**: deja `version = "1.0.0"` en `Cargo.toml`, pasa la
   sección «Sin publicar» del CHANGELOG a `[1.0.0] - fecha`, PR, CI verde,
   merge y:

   ```sh
   git tag -a v1.0.0 -m "MAGI 1.0.0"
   git push origin v1.0.0
   ```

   Vigila `release.yml` hasta el final: release de GitHub, fórmula de Homebrew
   y PKGBUILD en AUR. Después, haz las comprobaciones de §9.

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

5. `release.yml` (generado por `dist`) hace el resto:
   - `plan`: **falla si el tag no coincide con la versión de `Cargo.toml`** y
     no publica nada;
   - compila los cuatro objetivos con `--locked` (Linux x86_64 y aarch64 en
     Ubuntu 22.04; macOS arm64 y x86_64); si falla uno, no hay release;
   - crea la GitHub Release con los tarballs, sus `.sha256`, las atestaciones y
     las notas de la sección del CHANGELOG;
   - **solo en versiones estables**, publica la fórmula en el tap de Homebrew y
     llama a `publicar-aur.yml`, que rellena el PKGBUILD con la versión y las
     sumas, genera `.SRCINFO` en un contenedor Arch, pasa `namcap` y empuja a
     AUR.
6. Haz las comprobaciones de §9.

Los binarios publicados salen **solo** del CI a partir del tag: no se suben
binarios a mano ni se publica en crates.io (`publish = false`).

## 6. Workflows, permisos y secretos

| Workflow | Disparo | Qué hace |
|---|---|---|
| `ci.yml` | PR y push a `main` | `fmt`, `clippy -D warnings`, `TZ=UTC cargo test --locked` en Linux, macOS (build, clippy y `cargo test --lib`), `cargo deny check` y validación de la plantilla AUR (`makepkg --printsrcinfo` y `namcap` en contenedor Arch) |
| `release.yml` | tag `v*` | Lo genera `dist` 0.33.1: `plan`, compilación por objetivo, checksums y fórmula, GitHub Release, Homebrew y `publicar-aur` |
| `publicar-aur.yml` | lo llama `release.yml` | Rellena versión y sumas del PKGBUILD, genera `.SRCINFO`, pasa `namcap` y hace `git push` a `aur@aur.archlinux.org:magi-ssh-bin.git` |

Permisos: `contents: read` por defecto en todos los workflows; `contents:
write`, `id-token: write` y `attestations: write` solo en los jobs que
publican. No añadas secretos a `ci.yml`: los PRs de forks no los reciben y no
los necesitan.

No edites `release.yml` a mano: se regenera con `dist` (por ejemplo,
`dist init`) a partir de `dist-workspace.toml`.

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
- Si falla Homebrew: revisa que el tap exista, que `HOMEBREW_TAP_TOKEN` no haya
  caducado y que tenga **Contents: Read and write** solo en
  `4d3-org/homebrew-tap`; corrige y relanza el job. Si la fórmula necesita un
  arreglo manual en el tap, hazlo y vuelve a lanzar el job para que quede
  versionada.
- Si falla AUR: revisa `AUR_SSH_PRIVATE_KEY` (clave registrada en el perfil de
  AUR), que el nombre `magi-ssh-bin` siga libre, que `.SRCINFO` se genere y
  que `namcap` pase; relanza el job.
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
   `4d3-org/homebrew-tap` y permiso **Contents: Read and write**, con
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
magi --version                    # magi 1.0.0 (protocolo 6, <commit>)
pacman -Ql magi-ssh-bin           # binario, completions, man, .desktop, icono, licencias
magi diagnostico

# macOS
brew update && brew install 4d3-org/tap/magi-ssh
magi --version

# Tarball
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo 4d3-org/magi
```

Comprueba además:

- la GitHub Release tiene los cuatro tarballs, sus `.sha256`, las notas de la
  sección del CHANGELOG y, si era una prerelease, está marcada como tal;
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
  libres.
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
