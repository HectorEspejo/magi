# Publicar MAGI para Termux

MAGI corre en Android dentro de [Termux](https://termux.dev) (Fase 17): es el
mismo binario del escritorio compilado para `aarch64-linux-android`, con lo
propio de Termux en `src/termux/`. Este documento lo sigue el desarrollador para
publicar el paquete `.deb` y para proponer la receta a TUR. El agente prepara el
workflow, el script, la receta y esta guía, pero **no etiqueta versiones, no sube
paquetes ni abre la PR a TUR** (T69, D372).

Canales de distribución:

| Canal | Qué publica |
|---|---|
| GitHub Releases | `magi-ssh_X.Y.Z_aarch64.deb`, su `.sha256` y la atestación de procedencia, junto a los tarballs de `dist` de la misma release |
| TUR (Termux User Repository) | La receta `packaging/termux/build.sh`, que TUR compila desde el tarball del tag (`pkg install tur-repo` y `pkg install magi-ssh`) |

No se pide entrar en el repositorio oficial de Termux (D372): exige proyectos
conocidos y activos, y su ritmo no es el nuestro. Tampoco hay `curl | sh`.

## 1. Reparto de responsabilidades

| Tarea | Quién |
|---|---|
| Fijar la versión, la fecha y el contenido de la release | Desarrollador |
| Actualizar `Cargo.toml`, el CHANGELOG y los recursos (`assets/`) | Agente |
| Dejar `termux-release.yml`, `scripts/paquete-termux.sh`, la receta y esta guía preparados | Agente |
| Etiquetar (`git tag`) y empujar la etiqueta | Desarrollador |
| Relanzar el workflow (`workflow_dispatch`) y verificar la release | Desarrollador |
| Probar el `.deb` en el móvil (§5) | Desarrollador |
| Abrir y mantener la PR a TUR con la receta (§7) | **Desarrollador**; el agente no la abre |
| Mover una etiqueta, subir un `.deb` a mano, crear secretos o cuentas | **Nadie más que el desarrollador**; el agente no lo hace |

`termux-release.yml` solo usa `GITHUB_TOKEN`: no hay secretos nuevos que crear.

## 2. Requisitos para armar el `.deb` en local

Solo hace falta para probar antes de etiquetar; el que se publica sale siempre de
CI.

- Rust con el objetivo `aarch64-linux-android`, `cargo-ndk` 4.1.2 y el NDK r30
  (`30.0.16248370`), como para la app Android (ver
  [`publicar-android.md`](publicar-android.md) y la sección «Comandos de Android»
  de [`CLAUDE.md`](../CLAUDE.md)).
- `dpkg-deb`: en Arch y Omarchy, `sudo pacman -S dpkg`; en Debian y Ubuntu ya
  viene en `dpkg`. Sin él, `scripts/paquete-termux.sh` sale con código 1 y dice
  cómo instalarlo.

```sh
export ANDROID_HOME="$HOME/Android/Sdk" ANDROID_SDK_ROOT="$HOME/Android/Sdk" \
       ANDROID_NDK_HOME="$HOME/Android/Sdk/ndk/30.0.16248370"

# Binario para Termux (target/aarch64-linux-android/release/magi)
cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked

# El .deb y su .sha256 en target/termux/ (la versión es la de Cargo.toml)
scripts/paquete-termux.sh 1.6.1

# La estructura del .deb armado, como la comprueba la CI
MAGI_DEB="$PWD/target/termux/magi-ssh_1.6.1_aarch64.deb" MAGI_EXIGIR_DEB=1 \
  TZ=UTC cargo test --locked -p magi-ssh --test termux_paquete
```

`scripts/paquete-termux.sh` acepta el binario como segundo argumento (o en
`MAGI_BINARIO_TERMUX`) y deja el resultado en `MAGI_DEB_SALIDA`. Antes de armar
comprueba con `readelf -d` que el binario solo pide bibliotecas del NDK estable
(`libc`, `libm`, `libdl`, `liblog` y `libz`) y, tras armar, que `dpkg-deb
--contents` lista exactamente las rutas de §3 y que el `control` es el esperado.
`scripts/paquete-termux.sh --comprobar-binario` hace solo la primera parte y no
necesita `dpkg-deb`.

Comprobaciones de lo que se toca (como pide `CLAUDE.md`):

```sh
shellcheck packaging/termux/build.sh scripts/paquete-termux.sh tests/fijos/termux/falsos/doble
actionlint
```

## 3. Qué lleva el `.deb`

Todo bajo `/data/data/com.termux/files/usr/`, con propietario `root:root`:

| Ruta | Contenido |
|---|---|
| `bin/magi` | El binario (modo 700, como los paquetes de Termux) |
| `share/man/man1/magi.1.gz` | La página man de `assets/` |
| `share/bash-completion/completions/magi`, `share/zsh/site-functions/_magi`, `share/fish/vendor_completions.d/magi.fish` | El autocompletado de `assets/` |
| `share/doc/magi-ssh/` | `LICENSE-MIT`, `LICENSE-APACHE` y `README.md` |

`control`: `Package: magi-ssh`, `Version`, `Architecture: aarch64`, `Maintainer:
4d3`, `Installed-Size`, `Suggests: termux-api, openssh`, `Homepage` y
`Description`. **Sin `Depends`**: el binario solo pide bibliotecas del NDK estable,
que Android trae siempre. Se arma con `dpkg-deb -Zxz --root-owner-group` (xz, que
el `dpkg` de Termux lee siempre).

Si algún día el binario pidiera `libc++_shared.so` (por una dependencia nueva), el
paquete tendría que depender de `libc++`: el script y el workflow **fallan** y lo
dicen, y esa dependencia se decide con el analista y queda anotada en el informe
de la fase antes de aceptarla.

## 4. Versión, etiqueta y candidatas

El `.deb` lleva la misma versión que el escritorio: la de `Cargo.toml`
(política en [`publicar.md`](publicar.md), §3). La etiqueta `vX.Y.Z` lanza a la
vez `release.yml` (los tarballs de `dist`) y `termux-release.yml` (el `.deb`):

1. Los cambios entran en `main` con su línea en «Sin publicar» del CHANGELOG.
2. Actualiza `version` en `Cargo.toml` (y lo demás de §5 de `publicar.md`).
3. PR → `ci.yml` verde (incluido el job `termux`) → merge a `main`.
4. Etiqueta anotada y empújala:

   ```sh
   git tag -a vX.Y.Z -m "MAGI X.Y.Z"
   git push origin vX.Y.Z
   ```

5. `termux-release.yml` hace el resto (§6); vigílalo hasta el final.
6. **Una etiqueta publicada no se mueve ni se reutiliza.** Si hay que cambiar algo,
   se publica un PATCH (o una candidata nueva). Y una etiqueta tampoco se borra y se
   vuelve a crear con una ejecución de «Release Termux» suya en curso o en cola
   (§9): cancélala antes.

**Candidatas `vX.Y.Z-rc.N`.** Sirven para probar el paquete en el móvil antes de la
versión estable (§5). `release.yml` las publica como prerelease y este workflow
sube el `.deb` a ella. El `control` lleva `Version: X.Y.Z~rc.N`, con tilde: en
Debian la tilde ordena **antes** que `X.Y.Z`, así que la versión final actualiza
a la candidata con `apt` (con un guion ordenaría después y no se actualizaría).

> **El nombre del fichero de una candidata lleva guion, no tilde.** El fichero se
> llama `magi-ssh_X.Y.Z-rc.N_aarch64.deb` (y su `.sha256`,
> `magi-ssh_X.Y.Z-rc.N_aarch64.deb.sha256`), aunque el `Version` de su `control`
> sea `X.Y.Z~rc.N`. GitHub puede cambiar algunos caracteres del nombre de un
> adjunto y la tilde puede acabar como un punto; entonces el `.sha256`, que nombra
> el fichero, ya no cuadraría con lo que se descarga. Por eso el nombre de una
> candidata **no sigue la convención de Debian** (`nombre_versión_arquitectura`,
> con la versión del `control`); `dpkg` y `apt` no miran el nombre del fichero,
> solo el `control`. El paso «Comprobar los ficheros de la release» **falla** si
> los adjuntos no quedan con el nombre esperado. Las versiones estables no
> cambian: `magi-ssh_X.Y.Z_aarch64.deb`.

**La etiqueta de una candidata tiene que ser idéntica a la `version` de
`Cargo.toml` del commit que se etiqueta.** `verificar` (en este workflow) y `plan`
(en `release.yml`, que lo exige `dist`) comparan la etiqueta, sin la `v`, con la
`version` del paquete `magi-ssh`: con la etiqueta `v1.6.1-rc.1`, `Cargo.toml` debe
decir `version = "1.6.1-rc.1"`. Como una etiqueta publicada no se mueve, un
`Cargo.toml` que diga otra cosa quema la etiqueta y obliga a `-rc.2`. Es lo mismo
que hace [`publicar.md`](publicar.md) (§4, paso 8) con `v1.6.1-rc.1`:

1. En un commit propio, pon `version = "X.Y.Z-rc.N"` en `Cargo.toml` y deja
   `Cargo.lock` al día (`cargo metadata --locked` falla si no lo está). Cambiar la
   `version` arrastra lo que lista `CLAUDE.md` para un cambio de versión
   (`FECHA_DE_VERSION`, `assets/`, la DA2 de `tui_prueba.rs` y las instantáneas
   `servidor_anterior_*`): lo hace el agente en ese mismo PR. Ojo con la DA2:
   con una candidata (`1.6.1-rc.1`) el emulador no lee el parche (`1-rc`) y
   responde `10600`, no `10601`; la prueba `tui_prueba.rs` debe esperar esa en el
   commit de la candidata y volver a `10601` en el de la estable.
2. PR → `ci.yml` verde → merge a `main`. Comprueba que `Cargo.toml` de `main` dice
   `X.Y.Z-rc.N` antes de etiquetar.
3. Etiqueta y empuja `vX.Y.Z-rc.N`, como en el paso 4 de la lista de arriba.
4. Para volver a la estable, haz otro commit con `version = "X.Y.Z"` (y
   `Cargo.lock`), pasa la sección «Sin publicar» del CHANGELOG a `[X.Y.Z] - fecha`
   y sigue §4 con `vX.Y.Z`. Si la candidata falla, corrige y publica
   `vX.Y.Z-rc.N+1` (nunca muevas la etiqueta).

Las etiquetas `android-vX.Y.Z` (la app Android) **no** disparan este workflow: sus
patrones (`v[0-9]+.[0-9]+.[0-9]+` y la variante `-rc.N`) no encajan con
`android-v…`, y `release.yml` las excluye con su patrón negativo.

## 5. Probar el `.deb` en el móvil antes de publicar

Se hace con una candidata (`vX.Y.Z-rc.1`), que no toca las releases estables.

La del primer lanzamiento del proyecto es `v1.6.1-rc.1`: la misma candidata que
lanza el escritorio (ver [`publicar.md`](publicar.md) §4), con `Cargo.toml` en
`1.6.1-rc.1` en el commit etiquetado.

**La primera publicación de Termux tiene que ser siempre una candidata
`-rc.N`**, nunca una versión estable. Cuando se escribió esta guía, el script
`scripts/paquete-termux.sh` solo se había ejecutado con el doble de `dpkg-deb`
de las pruebas y el workflow nunca había corrido en GitHub: el primer armado con
`dpkg-deb` real es el que descubre lo que el doble no ve (avisos por campos del
`control`, el formato de `--contents`). Un fallo así en una etiqueta estable deja
la release del escritorio sin `.deb` y solo se arregla con un PATCH (§9). Antes de
etiquetar la candidata:

- Ejecuta el script con `dpkg-deb` real y pasa `termux_paquete` con el `.deb`
  resultante (§2).
- Comprueba que el job `termux` de `ci.yml` de la PR de la versión terminó en
  verde: arma el `.deb` con `dpkg-deb` real y pasa la misma prueba.

Una vez que una candidata ha salido bien de punta a punta, las siguientes
versiones pueden ir directamente como estables.

1. En Termux (la de F-Droid o la de GitHub; la de Google Play no se soporta),
   descarga el `.deb` y su `.sha256` de la release, o pásalos desde el equipo con
   `scp` (`pkg install openssh`):

   ```sh
   scp usuario@192.0.2.10:magi-ssh_1.6.1-rc.1_aarch64.deb .
   ```

2. Comprueba la integridad (y, si tienes `gh`, la procedencia):

   ```sh
   sha256sum -c magi-ssh_1.6.1-rc.1_aarch64.deb.sha256
   gh attestation verify magi-ssh_1.6.1-rc.1_aarch64.deb --repo HectorEspejo/magi
   ```

3. Instálalo y mira que arranca:

   ```sh
   apt install ./magi-ssh_1.6.1-rc.1_aarch64.deb
   magi --version
   magi diagnostico
   ```

4. Sigue el guion de validación en el plegable del informe de implementación
   ([`docs/fase17/magi-fase17-implementacion.md`](fase17/magi-fase17-implementacion.md),
   sección «Ejecución y pruebas»): arranque y conexión, llavero con frase
   maestra, sincronización con la carpeta del almacenamiento compartido,
   portapapeles con y sin Termux:API, enlaces y escritorios en el navegador,
   wake lock, avisos de Android y fila de teclas.
5. Con la candidata dada por buena, publica la versión estable con una etiqueta
   nueva (`vX.Y.Z`): `apt` la actualiza sobre la candidata.

Una arquitectura distinta (`arm`, `i686`, `x86_64`) la rechaza `dpkg` al instalar:
el `.deb` es solo `aarch64`.

## 6. Qué hace `termux-release.yml`

Solo se dispara con un push de etiqueta `vX.Y.Z` o `vX.Y.Z-rc.N`, o a mano con
`workflow_dispatch` (§8). Una ejecución por etiqueta, sin cancelación. Los pull
requests no lo ejecutan ni reciben permisos de escritura.

| Job | Qué hace | Permisos |
|---|---|---|
| `verificar` | Comprueba el formato de la etiqueta (estricto: sin ceros a la izquierda, `-rc.N` con N desde 1) y que coincide con la `version` de `Cargo.toml`; fija el commit de la etiqueta (su SHA, una sola vez) y, con un push de etiqueta, comprueba que es el que lanzó el workflow | `contents: read` |
| `compilar` | Descarga el repositorio en ese commit (por SHA, no por el nombre de la etiqueta) y lo anota como salida; instala el SDK, el NDK r30 y `cargo-ndk` 4.1.2 (descargado y verificado por SHA-256); compila `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked`; falla si `readelf -d` pide algo fuera de `libc`, `libm`, `libdl`, `liblog` y `libz`; arma el `.deb` con `scripts/paquete-termux.sh` y comprueba su estructura; escribe el `.sha256`; atesta la procedencia y guarda los artefactos | `contents: read`, `id-token: write`, `attestations: write` |
| `publicar` | Espera a que exista la release de la etiqueta (la crea `release.yml` al terminar; cada 30 s, hasta 60 min); justo antes de subir, comprueba que la etiqueta sigue en el commit que se compiló y **falla** si no; sube el `.deb` y el `.sha256` con `gh release upload` y **falla** si los adjuntos no quedan con el nombre esperado | `contents: write` |

- **Nunca crea la release ni toca la marca «Latest»**: esa marca es de la release
  del escritorio. Con `-rc.N` el `.deb` sube a la prerelease que crea `dist`.
- Si la release no aparece en 60 minutos (por ejemplo, porque falló
  `release.yml`), el job falla con «repítelo con `workflow_dispatch`».
- `publicar` identifica la release por el **nombre** de la etiqueta, así que no
  basta con que exista: si la etiqueta se borró y se volvió a crear en otro commit
  mientras esperaba, la release que aparece es la del commit nuevo. Por eso
  compara el SHA que compiló `compilar` con el de la etiqueta actual (`gh api
  repos/…/commits/<etiqueta>`) justo antes de `gh release upload`, y falla
  sin subir nada si difieren (§9).
- Solo se distingue «la release no existe todavía» de un error real de `gh`: un
  fallo de permisos o de red no se queda esperando una hora.
- La CI de cada PR (`ci.yml`) tiene un job `termux`: en las PR que cambian el
  binario o el contenido del `.deb` (`src/`, `crates/magi-nucleo/`, `Cargo.toml`,
  `Cargo.lock`, `build.rs`, `vendor/`, `visor/dist/`, `assets/`, `README.md`,
  `LICENSE-*`, `packaging/termux/`, `scripts/paquete-termux.sh`,
  `tests/termux_paquete.rs`, `ci.yml` y `termux-release.yml`), compila para
  aarch64 (y, solo como artefacto de 7 días, para x86_64), arma el `.deb` y pasa
  `termux_paquete` con `MAGI_DEB` y `MAGI_EXIGIR_DEB=1`.
- Las herramientas que descarga (`cargo-ndk`) van fijadas por SHA-256, como en el
  resto de workflows.
- **Riesgo conocido y mejora pendiente: las acciones van fijadas por etiqueta, no
  por SHA de commit.** `actions/checkout`, `android-actions/setup-android`,
  `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `actions/attest-build-provenance`
  y `actions/upload-artifact` o `download-artifact` se citan por etiqueta de versión
  (`@v6`…), como en `android-release.yml`, `android.yml`, `ci.yml` y `release.yml`
  (ningún workflow del repositorio las fija por SHA), y una etiqueta se puede mover.
  Aquí pesa más porque el job `compilar` tiene `id-token: write` y
  `attestations: write` y ejecuta acciones de terceros. No se cambia en esta fase
  (se sigue el patrón del repositorio). Pendiente: fijar las acciones por SHA en
  todos los workflows y, en este, mover la atestación a un job propio que solo use
  `actions/download-artifact` y `actions/attest-build-provenance`, para que los
  permisos de atestación no convivan con la compilación.

## 7. Receta de TUR

[TUR](https://github.com/termux-user-repository/tur) compila los paquetes de
usuarios con las herramientas de Termux. La receta de MAGI es
[`packaging/termux/build.sh`](../packaging/termux/build.sh): compila desde el
tarball del tag con `termux_setup_rust` (`cargo build --release --locked -p
magi-ssh`), con `TERMUX_PKG_EXCLUDED_ARCHES="arm, i686"`, e instala el binario, la
página man, el autocompletado y el README (las licencias las instala Termux por
`TERMUX_PKG_LICENSE_FILE`).

**Proponerla (la primera vez, la hace el desarrollador):**

1. Haz un fork de TUR y crea `tur/magi-ssh/build.sh` con el contenido de
   `packaging/termux/build.sh`.
2. Calcula el SHA-256 del tarball del tag **estable** y ponlo en
   `TERMUX_PKG_SHA256` (el fichero trae un marcador de 64 ceros que hace fallar la
   compilación si se olvida):

   ```sh
   curl -fsSL https://github.com/HectorEspejo/magi/archive/refs/tags/v1.6.1.tar.gz | sha256sum
   ```

   Es el tarball que genera GitHub para la etiqueta, no el de `dist`. Ajusta
   también `TERMUX_PKG_VERSION` y `TERMUX_PKG_MAINTAINER` (con tu usuario de
   GitHub si TUR lo pide así).
3. Compila el paquete para `aarch64` con la guía de TUR y comprueba el `.deb` que
   sale: `readelf -d` de `bin/magi` (la receta declara `TERMUX_PKG_DEPENDS="zlib"`
   por si el enlazador de Termux resuelve `libz` contra su paquete; ajústalo a lo
   que pida de verdad), `apt install` del `.deb` generado y `magi --version`.
4. Abre la PR a TUR. Los usuarios instalarán con:

   ```sh
   pkg install tur-repo
   pkg install magi-ssh
   ```

   Mientras no esté aceptada, `apt install ./magi-ssh_X.Y.Z_aarch64.deb` (§5) es el
   camino.

**Actualizarla en cada versión estable:** cambia `TERMUX_PKG_VERSION`, recalcula
`TERMUX_PKG_SHA256` con el comando de arriba para la etiqueta nueva y abre la PR
a TUR. Haz el mismo cambio en `packaging/termux/build.sh` del repositorio, para
que no se desfase de lo que hay en TUR. No se activa `TERMUX_PKG_AUTO_UPDATE`:
cada versión se revisa a mano. Las candidatas no van a TUR.

**x86_64.** La receta excluye solo `arm` e `i686`. Si TUR no compila bien x86_64,
descomenta la segunda línea de `TERMUX_PKG_EXCLUDED_ARCHES` en la receta (queda
explicado en el propio fichero). El `.deb` de GitHub es solo `aarch64`.

## 8. Repetir la subida: «Re-run failed jobs» o `workflow_dispatch`

Hay dos formas de repetir, y no son equivalentes:

- **«Re-run failed jobs»** (en la web, Actions → la ejecución → Re-run failed
  jobs, o `gh run rerun <id> --failed`) repite solo los jobs que fallaron, con el
  **mismo workflow, la misma etiqueta y el mismo commit** de la ejecución original,
  y conserva lo que ya salió bien (si `compilar` terminó, `publicar` sube el mismo
  `.deb` ya atestado, mientras GitHub retenga el artefacto). Es lo que hay que
  usar para un fallo transitorio: una caída de red, un runner, la espera de la
  release que se agotó. No aplica ningún cambio posterior del YAML ni del código.
- **`workflow_dispatch`** empieza una ejecución nueva: compila y atesta otra vez, y
  usa el `termux-release.yml` de la rama que elijas. Úsalo cuando hay que subir el
  `.deb` de una etiqueta que ya existe y no hay una ejecución que repetir, o
  cuando el fallo es del propio YAML de `termux-release.yml` y ya está corregido en
  esa rama. Desde la web (Actions → Release Termux → Run workflow, con la
  etiqueta) o con `gh`:

```sh
gh workflow run termux-release.yml --ref main -f tag=v1.6.1
```

Compila y arma el `.deb` otra vez **a partir del árbol de la etiqueta** y lo sube a
la release del tag. La etiqueta debe cumplir las mismas reglas de §4 y coincidir
con `Cargo.toml` de ese commit (con una candidata, `version = "X.Y.Z-rc.N"`); una
etiqueta anterior a la Fase 17 no tiene `scripts/paquete-termux.sh` y falla.

**El script y todo lo que se compila salen siempre del árbol de la etiqueta.**
`--ref main` solo cambia el YAML del workflow: arregla un fallo de
`termux-release.yml`, pero **no** uno de `scripts/paquete-termux.sh`, de la receta,
del código ni de `Cargo.toml`, porque todo eso vive en el commit de la etiqueta y
una etiqueta publicada no se mueve (§9, «Falla el armado del .deb»).

**La atestación de un `.deb` rehecho con `workflow_dispatch` lleva el commit de
la rama desde la que se lanzó**, no el de la etiqueta: GitHub registra como fuente
el commit que lanzó el workflow, y con `--ref main` es la punta de `main`. Lo
compilado sí es el árbol de la etiqueta: `verificar` fija su SHA, `compilar` lo
descarga por SHA y lo escribe en su log («Se compila el commit …»), y `publicar`
comprueba que la etiqueta sigue en él antes de subir. `gh attestation verify
… --repo HectorEspejo/magi` da por buenos los dos casos; si quieres comprobar además el
origen, `--source-ref refs/tags/vX.Y.Z` solo vale para un `.deb` salido de un push
de etiqueta.

La subida no usa `--clobber`: **si el `.deb` ya está en la release, no lo
sustituye**. Estaba atestado y su `.sha256` es el que usan los usuarios. Si de
verdad hay que reemplazarlo, bórralo antes desde la página de la release (junto a
su `.sha256`) y repite.

## 9. Si falla una publicación

- **Falla `verificar`:** la etiqueta no tiene la forma esperada o no coincide con
  `Cargo.toml`. Como la etiqueta no se mueve, corrige con un commit y publica una
  etiqueta nueva (PATCH o `-rc.N`). Si falla el paso «Fijar el commit de la
  etiqueta» con un push, la etiqueta apunta ya a otro commit que el que lanzó esa
  ejecución (se movió o se recreó): cancélala, la de la etiqueta actual es la que
  vale.
- **Falla `readelf` (bibliotecas):** el binario pide algo fuera del NDK estable.
  Si es `libc++_shared.so`, una dependencia nueva trae C++; se decide con el
  analista (el paquete pasaría a depender de `libc++`) y se anota en el informe.
- **No se compila el NDK o `cargo-ndk`:** son las mismas versiones que usa
  `android-release.yml`; mira el log del paso y, si cambió la descarga de
  `cargo-ndk`, su suma SHA-256 de `termux-release.yml` y de `ci.yml`.
- **Falla «Comprobar los ficheros de la release»:** el `.deb` y el `.sha256` ya
  están subidos, pero GitHub no los dejó con el nombre esperado (por ejemplo, otro
  carácter cambiado). El nombre de una candidata ya evita la tilde, la causa
  conocida, así que es un caso nuevo: descarga los dos ficheros, renombra el `.deb`
  al nombre que dice el `.sha256` antes de `sha256sum -c`, y deja el caso anotado
  (hay que cambiar el nombre en `scripts/paquete-termux.sh` y en el workflow).
  No sustituyas el adjunto a ciegas: está atestado.
- **Falla el armado del .deb (script o estructura):** falla el paso «Comprobar las
  bibliotecas que pide el binario» o «Armar y comprobar el .deb y su .sha256» de
  `compilar`. `scripts/paquete-termux.sh` y todo lo que se compila salen siempre del
  árbol de la etiqueta, y `workflow_dispatch --ref main` solo arregla fallos del
  YAML de `termux-release.yml` (§8): no arregla el script, la receta, el código ni
  `Cargo.toml`. Un fallo del script con `dpkg-deb` real en una etiqueta estable
  deja la release del escritorio (que `release.yml` crea igualmente) **sin `.deb`**,
  y solo se arregla con un PATCH nuevo: la etiqueta publicada no se mueve. Por eso
  la primera publicación es siempre una candidata `-rc.N` y el script se ejecuta
  antes con `dpkg-deb` real (§5 y §2). Si falla en una candidata, corrige con un
  commit y publica `-rc.N+1`.
- **Falla `publicar`:** la release del escritorio no existe o no es accesible.
  Arregla `release.yml` (§7 de [`publicar.md`](publicar.md)) y repite (§8): con
  «Re-run failed jobs» si el fallo fue transitorio, o con `workflow_dispatch`. El
  `.deb` ya compilado queda como artefacto del workflow (`termux-release`)
  mientras GitHub lo retenga (90 días por defecto); «Re-run failed jobs» lo
  reutiliza, y `workflow_dispatch` lo compila de nuevo desde la etiqueta.
- **Falla `publicar` con «el .deb se compiló desde el commit …»:** la etiqueta ya
  no apunta al commit que compiló esta ejecución. Se borró y se volvió a crear
  (`publicar.md`, §7) mientras `publicar` esperaba a la release, y la release que
  apareció es la del commit nuevo. No se sube nada, a propósito: el `.deb` sería
  de otro commit.
  - **No se recrea una etiqueta con una ejecución de «Release Termux» suya en
    curso o en cola: cancélala antes** (Actions → la ejecución → Cancel workflow).
    Una ejecución vieja retiene el grupo de concurrencia de la etiqueta (hasta 60
    min de espera más la subida) y la de la etiqueta nueva queda en cola detrás.
  - Es mejor **«Re-run failed jobs»** del workflow que falló (el mismo commit y la
    misma etiqueta) que borrar y recrear la etiqueta: solo hay que recrearla si lo
    que falló está en el código o en el YAML del commit etiquetado.
  - Si ya se recreó: cancela la ejecución vieja y comprueba que la de la etiqueta
    nueva sube su `.deb`; si no llega a la release, repite con `workflow_dispatch`
    (§8).

## 10. Comprobaciones posteriores a publicar

```sh
gh release view vX.Y.Z --json assets --jq '.assets[].name'
```

- La release lleva `magi-ssh_X.Y.Z_aarch64.deb` y `magi-ssh_X.Y.Z_aarch64.deb.sha256`
  (con una candidata, `magi-ssh_X.Y.Z-rc.N_aarch64.deb` y su `.sha256`, con
  guion) junto a los tarballs de `dist`.
- `sha256sum -c`, `gh attestation verify` y `dpkg-deb --info` funcionan con el
  `.deb` descargado: `Architecture: aarch64`, `Suggests: termux-api, openssh` y
  ningún `Depends`.
- En el móvil, `apt install ./magi-ssh_X.Y.Z_aarch64.deb` sobre la candidata o la
  versión anterior actualiza sin errores, y `magi --version` dice la versión.
- Si la versión es estable, actualizada la receta de TUR (§7).

## 11. Referencias

- Informe de la Fase 17: §4.1 (instalar), §4.9 (publicar), §5.1 (el `.deb`) y §5.7
  (compilación y CI); desviaciones 1, 3 y 4 de su informe de implementación.
- [`publicar.md`](publicar.md): publicación del escritorio y política de versiones.
- [`publicar-android.md`](publicar-android.md): la app Android, de la que este
  workflow toma el NDK y `cargo-ndk`.
- [`docs/manual/termux.md`](manual/termux.md): el capítulo de Termux en el manual.
- Termux: <https://termux.dev> · TUR: <https://github.com/termux-user-repository/tur>
- Atestaciones de procedencia:
  <https://docs.github.com/es/actions/security-for-github-actions/using-artifact-attestations>
