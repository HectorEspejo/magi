# Publicar la web de MAGI

MAGI tiene web propia, <https://magi.erratia.com> (Fase 9c): una portada, las
descargas con su suma SHA-256, las versiones y el manual, en castellano y en
inglés (el manual, solo en castellano). Es una web estática que sirve GitHub
Pages desde este mismo repositorio, desplegada con GitHub Actions y sin
secretos. Este documento lo sigue el desarrollador para ponerla en marcha, para
repetir una publicación y para retirarla. El agente prepara el generador, el
workflow, los scripts y esta guía, pero **no toca el DNS, ni el dominio, ni los
ajustes de Pages o del repositorio, ni crea secretos o variables**.

Cómo se genera la web, de dónde sale cada cosa y cómo se cambia (un texto, una
plataforma, una página, un capítulo del manual) está en
[`web/README.md`](../web/README.md); aquí solo se cuenta lo que hace el
desarrollador.

## 1. Reparto de responsabilidades

| Tarea | Quién |
|---|---|
| Dejar `web.yml`, los jobs `cambios-web` y `web` de `ci.yml`, el generador `crates/magi-web`, `scripts/generar-web.sh`, las plantillas, los textos, las capturas y esta guía preparados | Agente |
| Verificar `erratia.com` en tu cuenta de GitHub y crear sus registros DNS (el TXT de verificación y el CNAME `magi`) | **Desarrollador** |
| Ajustes de Pages del repositorio: origen «GitHub Actions», dominio `magi.erratia.com` y «Enforce HTTPS» | **Desarrollador** |
| Fusionar la PR de la 9c, relanzar `web.yml` y pasar el guion de validación (§4) | **Desarrollador** |
| Crear las variables `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` cuando esos canales existan (§7) | **Desarrollador** |
| Dar la huella del certificado de la app y revisar la PR que la pone en `web/datos/plataformas.toml` (§8) | Desarrollador (la huella) y agente (la PR) |
| Cambiar el estado de una plataforma o regenerar las capturas, en su PR (§9 y §10) | Agente |
| La regla de `main` (los jobs de la web no entran, §5) | **Desarrollador** |
| Retirar la web (§11) | **Desarrollador** |

`web.yml` no usa ningún secreto: le basta el `GITHUB_TOKEN` del propio workflow,
de solo lectura, para leer la API de GitHub.

## 2. De dónde sale cada cosa

La web no tiene base de datos ni nada que respaldar: todo sale del repositorio y
de las releases, y se reconstruye entero en cada ejecución.

| Qué | De dónde sale | Cuándo llega a la web |
|---|---|---|
| El manual (`/manual/`) | `docs/manual/` de la **etiqueta de la última versión publicada del escritorio** (`git show`), nunca de `main` | Con la siguiente versión: un capítulo nuevo sale con ella |
| Las versiones (`/versiones/`) | `CHANGELOG.md` y `android/CHANGELOG.md` de la etiqueta de la última versión publicada de su canal | Con la siguiente versión de cada canal |
| Las descargas: versión, artefactos, tamaños, enlaces y SHA-256 | La API de GitHub (las releases) y los `.sha256` publicados, en **cada construcción** (`releases.json`, sin versionar) | En cada construcción |
| El estado de cada plataforma, el nombre de cada artefacto, los filtros de Obtainium y la huella del APK | `web/datos/plataformas.toml` (de `main`) | Al fusionar en `main` |
| Plantillas, estilos, textos, fuentes, capturas y la animación | `web/` y `assets/demo/magi.gif` (de `main`) | Al fusionar en `main` |
| Los bloques de Homebrew y AUR en descargas | Las variables del repositorio `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` | En la siguiente construcción (§7) |

Por eso la web publicada enseña lo que está publicado, no lo que hay en `main`:
el manual de la 1.6.1 sigue siendo el de la 1.6.1 aunque `main` ya lo haya
cambiado. Las capturas y la animación, en cambio, salen de `main` y pueden
enseñar algo que aún está en «Sin publicar» durante unos días.

## 3. Ponerla en marcha (una sola vez)

El orden importa. `web.yml` se dispara con cualquier push a `main` que toque
`web/` (o su generador, o la demo), y la fusión de la PR de la 9c es uno: su job
`desplegar` **falla en rojo si Pages todavía no está configurado**. La web
anterior no existe, así que no se rompe nada, pero ensucia las ejecuciones y
obliga a repetir. Por eso el dominio y Pages se dejan listos **antes** de
fusionar.

1. **Verificar `erratia.com` en GitHub.** En los ajustes de tu **cuenta**
   (foto de perfil › Settings › Pages, no los del repositorio) elige «Add a
   domain», escribe `erratia.com` y GitHub te da un registro TXT con este
   nombre: `_github-pages-challenge-<usuario>` (tu usuario de GitHub) y un
   valor. Créalo en el DNS de `erratia.com`, pulsa «Verify» y espera: si no
   verifica, es que el DNS aún no ha propagado; repite más tarde. **No sigas sin
   el dominio verificado**: es lo que impide que otra cuenta de GitHub ponga tu
   dominio o tus subdominios en su Pages.
2. **Pages del repositorio.** Settings › Pages del repositorio
   `HectorEspejo/magi`:
   - «Build and deployment › Source»: **GitHub Actions**.
   - «Custom domain»: `magi.erratia.com` y «Save». Mientras el DNS no apunte
     (paso 3), GitHub avisa de que la comprobación del DNS falla: es normal.
   - No hace falta ningún fichero `CNAME` en el sitio: con origen «GitHub
     Actions» el dominio se guarda en estos ajustes.

   GitHub crea el entorno `github-pages`; `main` debe poder desplegar en él (es
   lo que trae por defecto).
3. **DNS: el CNAME.** En la zona de `erratia.com`, un registro `CNAME` con
   nombre `magi` y destino `hectorespejo.github.io`. Si el DNS está en
   Cloudflare, déjalo **sin proxy** («solo DNS», la nube gris): con el proxy
   puesto, GitHub puede no ver el CNAME y no emitir el certificado. El blog de
   `erratia.com` no cambia: solo se añade el subdominio.
4. **Esperar al certificado y forzar HTTPS.** Cuando la comprobación del DNS de
   Pages sale en verde, GitHub emite el certificado (puede tardar de minutos a
   una hora o más). Entonces marca **«Enforce HTTPS»**. Si sigue en gris y la web
   aún no se ha desplegado, sigue con 5 y 6 y vuelve a este paso.
5. **Solo entonces, fusiona la PR de la 9c en `main`**, con su CI en verde. Esa
   fusión toca `web/` y lanza `web.yml`: construye la web y la despliega.
6. **Si no se lanzó, o hay que repetirla**, lánzala a mano: Actions › Web › «Run
   workflow» (rama `main`), o

   ```sh
   gh workflow run web.yml --ref main
   ```

   (`workflow_dispatch` solo existe cuando `web.yml` ya está en `main`; antes de
   fusionar no se puede lanzar, §12.)
7. **Pasa el guion de validación (§4)** en el móvil y en el escritorio, y marca
   en el checklist de la fase
   ([`docs/fase9c/magi-fase9c-checklist.md`](fase9c/magi-fase9c-checklist.md))
   los cuatro pasos del desarrollador: dominio verificado y CNAME, Pages con
   dominio y HTTPS, keystore y huella (§8) y primera publicación comprobada.
   Anota el resultado en [`docs/manual/estado.md`](manual/estado.md) (fila de la
   fase 9c).

Hasta que el dominio esté puesto, la web también responde en
<https://hectorespejo.github.io/magi/>: todos sus enlaces internos son relativos
(solo el 404 usa direcciones absolutas, §12). Con el dominio personalizado,
esa dirección redirige a `magi.erratia.com`.

**La app.** La web enseña la app como «aún sin publicar» mientras no haya una
release estable de `android-vX.Y.Z`. El keystore, sus cuatro secretos y las
candidatas de la app se hacen según [`publicar-android.md`](publicar-android.md);
lo único que la web exige es la huella del certificado en
`web/datos/plataformas.toml` **antes de publicar la estable** (§8).

## 4. Guion de validación de la web

En el móvil y en el escritorio, contra <https://magi.erratia.com> (en el
escritorio, para el paso 5, estrecha la ventana del navegador a 320 px):

1. **Portada, claro y oscuro.** Cambia el tema del sistema y mira la portada en
   los dos. Pliega la animación de la demo (el «▾ Animación de la demo»). Con
   «reducir movimiento» activado en el sistema, la animación no sale y en su
   lugar se ve la captura de Flota.
2. **Descargas.** Baja el tarball de Linux con su `.sha256` desde el enlace de la
   página y pasa `sha256sum -c` y `gh attestation verify <fichero> --repo
   HectorEspejo/magi`. Comprueba que la suma que enseña la página es la del
   `.sha256`. Los estados «en pruebas» y «beta» salen en texto y enlazan a «Qué
   se ha probado».
3. **Manual.** Busca «túnel» en el cuadro de búsqueda, sigue un enlace con ancla
   de un capítulo a otro (debe llevar al mismo título que en GitHub) y uno que
   salga a GitHub (debe abrir el fichero o la carpeta en la etiqueta de la
   versión publicada). El manual dice «Solo en castellano · Spanish only».
4. **Idioma.** Pasa a inglés desde cada página (portada, descargas, versiones,
   404) y vuelve: se llega siempre a la equivalente. En inglés, el menú dice
   «Manual (Spanish)».
5. **320 px.** Sin desplazamiento horizontal de la página; las capturas que no
   caben se desplazan dentro de su marco.
6. **404.** Abre una ruta que no existe: sale el 404, en castellano y en inglés.
7. **Navegador.** Con las herramientas del navegador (red y consola): ninguna
   petición a otro dominio y ningún aviso de la CSP, salvo uno conocido de mdBook en
   el manual: al cerrar la búsqueda con Esc sale «Applying inline style violates…
   `style-src 'self'`». No tiene efecto visible (la búsqueda se cierra igual) y no es
   un defecto de la web.
8. **Dominio.** `http://magi.erratia.com` redirige a `https://`, el certificado
   es válido para `magi.erratia.com` y `https://hectorespejo.github.io/magi/`
   lleva al dominio.

Lo que falle va al informe de implementación de la fase como defecto, y lo que
dependa de la web (una plantilla, un texto, un estilo) se corrige en una PR
normal.

## 5. Cómo se publica una versión nueva de la web

No hay nada que hacer a mano en la rutina. `web.yml` (nombre «Web») se dispara
cuando:

| Disparo | Cuándo |
|---|---|
| `workflow_run` de «Release», «Release Termux» o «Release Android» | Solo cuando esa ejecución **termina bien** (`success`). Una release que falla no regenera la web |
| `push` a `main` | Solo si toca `web/**`, `crates/magi-web/**`, `assets/demo/**` o `.github/workflows/web.yml`. El manual (`docs/manual/`) **no** está en la lista a propósito: sale de la etiqueta, no de `main` |
| `workflow_dispatch` | A mano (§3 paso 6): la primera publicación, o cada vez que haya que quitar algo que enseña la web (una release borrada) |

Una versión nueva del escritorio dispara dos vueltas: al terminar «Release» sale
la 1.6.2 en descargas (Termux sigue en la 1.6.1 porque su `.deb` aún no está), y
al terminar «Release Termux» Termux pasa a la 1.6.2. Con `android-vX.Y.Z` pasa lo
mismo con «Release Android». No se usa `on: release` porque una release que crea
el `GITHUB_TOKEN` no dispara otros workflows.

Qué hace, en dos jobs:

| Job | Qué hace | Permisos |
|---|---|---|
| `construir` («Construir la web») | Instala mdBook fijado por versión y SHA-256, lee la API de GitHub y los `.sha256`, saca el manual y los CHANGELOG de la etiqueta publicada, construye el sitio, lo comprueba con `magi-web comprobar` y lo sube como artefacto de Pages. Todo con `scripts/generar-web.sh` | `contents: read` |
| `desplegar` («Desplegar en GitHub Pages») | `deploy-pages`, en el entorno `github-pages`. Solo corre desde `main` | `contents: read`, `pages: write` e `id-token: write` (OIDC hacia Pages), **solo en este job** |

- **Despliega solo desde `main`.** Lanzado a mano desde otra rama, `construir`
  construye y comprueba pero `desplegar` no corre.
- **Concurrencia `pages`, sin cancelar.** Un despliegue a la vez y ninguno se
  interrumpe a medias; si llegan varias ejecuciones seguidas, GitHub conserva
  una pendiente y descarta las anteriores, que no importa: cada construcción lee
  las releases en el momento y el contenido de `main`. Solo hay una excepción
  (§12): una ejecución que no despliega (de una release que falló o lanzada a
  mano desde otra rama) también ocupa el grupo y puede desplazar a una pendiente
  que sí iba a desplegar.
- **Sin secretos.** Usa el `GITHUB_TOKEN` y, como variables del repositorio (no
  secretos), `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` (§7).
- **Si algo falla, no se despliega nada** y la web publicada sigue siendo la
  anterior (§6).
- **En las PR**, `ci.yml` tiene dos jobs: `cambios-web` («¿Toca la web?»), que
  mira si la PR toca `web/`, su generador, la demo, el manual, los scripts y la
  prueba de las capturas, `web.yml`, `ci.yml` o `Cargo.toml` y `Cargo.lock`, y
  `web` («Web (construir y comprobar)»), que construye y comprueba
  con los datos reales de la API, dos veces: con el manual de la última etiqueta
  (lo que se publicaría) y con el de la propia rama (para que un capítulo, un
  enlace o un ancla nuevos se comprueben antes de llegar a una versión). No
  despliega nada. **No son obligatorios en la regla de `main`** (tienen filtro
  de rutas, como los de Termux): una PR que rompa la web puede fusionarse aunque
  su job `web` esté en rojo, así que mira su resultado antes de fusionar. La regla
  de `main` está en [`publicar.md`](publicar.md) (§4).

**Volver atrás.** Revierte el commit en `main` (su push lanza `web.yml`) o
relanza desde Actions una ejecución anterior de «Web»: usa el mismo commit que
aquella, pero las versiones y los artefactos se vuelven a leer de la API.

## 6. Si falla la construcción o el despliegue

Un dato raro (de la API, de un `.sha256`, de un CHANGELOG o de un fichero de
`web/`) para la construcción con un mensaje que dice **qué** y **dónde**, y la web
publicada sigue siendo la anterior: nunca se despliega a medias. El mensaje sale
al final del log del paso «Construir y comprobar la web» del job `construir`,
con una de estas formas:

```text
error: dato raro en <dónde>: <qué le pasa>
error: la comprobación encontró N problema(s):
  - <fichero del sitio>:<línea>: <qué pasa>
```

| Qué pasa | Causa probable y qué hacer |
|---|---|
| `dato raro en …` de una release, un artefacto o un `.sha256` | La API o un fichero publicado no cumple lo esperado (una etiqueta con otra forma, un artefacto con otro nombre, un `.sha256` que nombra otro fichero o no tiene 64 hexadecimales). Una release publicada no se corrige moviendo la etiqueta ni reemplazando adjuntos (`publicar.md` §7): si falta o sobra algo porque un workflow de la release no terminó, relánzalo; si el defecto es de la release, se publica un PATCH. Después, relanza la web (§3 paso 6) |
| `falta huella_certificado …` o `huella de la release X ≠ la del repositorio Y` | La huella del APK (§8). La construcción enseña las dos |
| `capítulo fuera del índice: x.md` | Un `.md` del manual de la etiqueta publicada no está en `## Capítulos` de `docs/manual/README.md`. La etiqueta no se mueve: se corrige en `main` y llega con la siguiente versión (PATCH). El job `web` de la PR lo avisa antes, porque construye también el manual de la rama |
| `la comprobación encontró N problema(s)` | Un enlace interno o un ancla rotos, un enlace externo fuera de la lista permitida, un atributo `style` o un `<script>` propio, o una CSP incompleta. Cada línea dice el fichero del sitio. Un dominio nuevo entra en la lista en el mismo PR que lo enlaza (`web/README.md`) |
| Falla la API de GitHub o `gh` (red, límite de peticiones) | Es transitorio: «Re-run failed jobs» desde Actions |
| `desplegar` falla porque `actions/configure-pages` no encuentra el sitio de Pages | Pages no está configurado: §3 pasos 1 a 4, y relanza |
| `desplegar` sale «skipped» | La ejecución no era de `main` (un `workflow_dispatch` desde otra rama) o `construir` falló o se omitió |
| `construir` no corre tras una release | Esa release terminó mal (`conclusion` distinto de `success`): arréglala (`publicar.md` §7) y la web se regenera con la siguiente ejecución buena, o lánzala a mano |

Para reproducirlo en local con los mismos datos: `scripts/generar-web.sh`
(necesita `gh` con sesión o `GH_TOKEN`; ver `web/README.md`).

## 7. Homebrew y AUR en la web

La página de descargas enseña el bloque de Homebrew
(`brew install HectorEspejo/tap/magi-ssh`) o el de AUR (`yay -S magi-ssh-bin`)
**solo** si la variable correspondiente del repositorio vale exactamente `true`, en
minúsculas. Ojo: en los workflows de release GitHub compara las variables sin
distinguir mayúsculas (`True` también publicaría en Homebrew o en AUR), pero la web no:
con `True` la release publicaría y la página no enseñaría el bloque. Escribe siempre
`true`. Son las mismas variables que encienden los jobs de publicación de
[`publicar.md`](publicar.md) (§5):

| Variable | Bloque |
|---|---|
| `PUBLICAR_HOMEBREW` | Homebrew |
| `PUBLICAR_AUR` | AUR |

1. Pon la variable a `true` **cuando el canal exista de verdad** (el tap creado y
   publicado, o el paquete en AUR): Settings › Secrets and variables › Actions ›
   pestaña «Variables», o

   ```sh
   gh variable set PUBLICAR_HOMEBREW --body true --repo HectorEspejo/magi
   ```

2. Relanza la web (§3 paso 6): el bloque sale en esa construcción.
3. Para apagarlo, borra la variable o ponle otro valor y relanza.

El job `web` de las PR no recibe estas variables: si se activan, los bloques se
comprueban por primera vez en el despliegue, y si algo falla ahí, la web
anterior sigue.

## 8. La huella del certificado de la app

La página de descargas enseña la huella SHA-256 del certificado de firma del APK
para que se compare con `apksigner verify --print-certs` (que la imprime en
minúsculas y sin los dos puntos: la página lo dice, y se compara sin distinguir eso). Esa huella es una
**constante del repositorio**, `huella_certificado` en la sección `[app]` de
`web/datos/plataformas.toml`, revisada por PR; la web no la saca del APK
publicado, porque no probaría nada si alguien lo cambiara.

- **Qué poner.** La huella SHA-256 del certificado del keystore, en mayúsculas y
  con dos puntos entre bytes (64 hexadecimales en 32 pares), como sale en la
  línea «SHA256:» de `keytool -list -v -keystore <keystore> -alias <alias>`
  (ver [`publicar-android.md`](publicar-android.md), la sección del keystore) y
  como la escribe `android-release.yml` en las notas de cada release:
  `- Certificado de firma (SHA-256): AB:CD:…:EF`.
- **Cuándo.** Vacía mientras no haya app publicada. Pónla por PR **antes de
  publicar la primera release estable de la app** (`android-v0.2.0`): una
  candidata (`-rc.N`) es prerelease y no sale en descargas, pero con una estable
  y la constante vacía la construcción falla con «falta `huella_certificado`».
- **Qué comprueba la construcción.** Si hay una release publicada de la app, la
  huella de sus notas tiene que ser la de la constante; si no, falla enseñando
  las dos y la web anterior sigue. Sin ninguna release de la app no se comprueba
  nada y el bloque dice «aún sin publicar».
- **Si cambia el keystore.** Un keystore nuevo cambia el certificado y los
  móviles que ya tienen MAGI no pueden actualizar sin reinstalar
  ([`publicar-android.md`](publicar-android.md) lo explica). Si ocurre, cambia en
  el mismo PR la huella de `plataformas.toml` y dilo en `android/CHANGELOG.md`;
  y pon la huella nueva **antes** de publicar la release firmada con él, o la web
  fallará hasta que lo hagas.

## 9. Cambiar el estado de una plataforma

El estado de cada plataforma (`estable`, `en_pruebas` o `beta`) vive en
`web/datos/plataformas.toml` y se cambia **en el mismo PR** que lo dice
[`docs/manual/estado.md`](manual/estado.md) (y al revés): nunca uno sin el otro.
Es el mismo caso para una mejora (macOS pasa de «en pruebas» a «estable» cuando
lo pruebas) que para una regresión.

La página de descargas cambia al fusionar en `main` (el push toca `web/`). El
manual, no: sale de la etiqueta de la última versión, así que su «Qué se ha
probado» se pone al día con la siguiente. Durante ese tiempo la web puede decir
«estable» y su manual, «en pruebas». Es una consecuencia de que el manual sea el
de lo publicado (§2), y desaparece con la siguiente versión.

## 10. Regenerar las capturas

Las seis capturas de la portada (Flota, sesión con paneles, archivos, túneles,
deliberación MAGI y servicios) salen del propio código de pintado de la TUI y se
versionan en `web/capturas/`. Cuando una PR cambia una de esas vistas, la prueba
normal `capturas_web` (que corre con `cargo test --workspace`, en `tests-linux`)
falla con «captura desfasada: ejecuta scripts/generar-capturas.sh». Entonces:

```sh
scripts/generar-capturas.sh
git diff web/capturas     # revisa el diff antes de subirlo
```

Súbelo en el mismo PR. No se edita una captura a mano, nunca. El script solo
necesita `cargo`; usa el inventario de la demo (`assets/demo/inventario`), nunca
tus hosts ni la semilla de las pruebas.

## 11. Retirar la web

Si algún día hay que quitar la web, **el orden importa**: primero se quita el
`CNAME` del DNS y después se despublica. Un `CNAME` que sigue apuntando a
`hectorespejo.github.io` cuando ya no hay un sitio de Pages con ese dominio deja
el subdominio «colgando»: otra cuenta de GitHub podría reclamar `magi.erratia.com`
en su Pages y servir lo que quiera bajo tu dominio. Con `erratia.com` verificado
en tu cuenta (§3 paso 1) eso no debería poder hacerse, pero no cuentes solo con
ello.

1. **Quita el `CNAME` `magi` del DNS de `erratia.com`.** Bórralo (en Cloudflare,
   borrar el registro, no basta con ponerlo con proxy). Es lo primero.
2. **Despublica Pages.** En Settings › Pages del repositorio, quita el dominio
   personalizado («Custom domain», vacío y «Save») y despublica el sitio (la
   opción «Unpublish site», junto a la dirección del sitio; su nombre puede
   variar con la interfaz de GitHub). Para que `web.yml` no vuelva a desplegarlo,
   desactívalo (Actions › Web › menú › «Disable workflow») o quita el workflow
   de `main`.
3. **Opcional: quita la verificación del dominio** en los ajustes de tu cuenta
   (Settings › Pages › el dominio `erratia.com` › «Delete») y el TXT
   `_github-pages-challenge-<usuario>` del DNS. Lo más seguro es **dejarla**:
   la verificación es lo que protege a `erratia.com` y a todos sus subdominios
   de que otra cuenta los use en su Pages.

Las releases, el README y el manual del repositorio siguen en GitHub sin cambios:
la web no es la única puerta de entrada.

## 12. Limitaciones conocidas

- **Las releases de GitHub son mutables.** Quien tenga permisos de escritura en
  el repositorio puede cambiar un artefacto y su `.sha256` a la vez, y la web
  enseña lo que dice la release en el momento de construir. La suma de la página
  sirve para comprobar una descarga, **no es una prueba independiente**: la
  confianza real está en `gh attestation verify`, que comprueba que el fichero
  lo compiló el CI de este repositorio.
- **`workflow_dispatch` solo funciona con `web.yml` ya en `main`**, así que el
  workflow no se puede probar desde una rama: se prueba en la PR con el job
  `web` de `ci.yml`, que construye y comprueba, pero no despliega.
- **El manual está solo en castellano**, y la interfaz de mdBook (el cuadro y los
  resultados de búsqueda, los títulos de los botones) sale en inglés. La búsqueda
  encuentra palabras enteras: «túnel» no encuentra «túneles».
- **El 404 usa URL absolutas a `https://magi.erratia.com/`**: servido desde otro
  sitio (por ejemplo, `hectorespejo.github.io/magi/` antes de poner el dominio, o
  `127.0.0.1` en local) se ve sin estilos, porque la CSP solo admite recursos
  propios. El resto de enlaces internos son relativos.
- **Una release borrada** sigue enseñándose hasta la siguiente construcción;
  para que sea ya, lanza la web a mano (§3 paso 6).
- **Sin `frame-ancestors`**: esa directiva de la CSP no funciona dentro de un
  `<meta>` y GitHub Pages no deja poner cabeceras. Otra web podría meter esta en
  un marco; no hay sesión, formularios ni acciones que proteger.
- **Los jobs `cambios-web` y `web` de `ci.yml` no son obligatorios** en la regla
  de `main` (§5).
- **Una ejecución que no despliega** (la disparada por una release que falló o la
  lanzada a mano desde otra rama) entra igual en el grupo de concurrencia `pages`
  y puede desplazar a una pendiente que sí iba a desplegar; es raro y lo arregla
  relanzar a mano desde `main`.
- **Depende de GitHub Pages.** Si Pages cae, la web cae; las releases y el README
  siguen en GitHub.

## 13. Referencias

- Informe de la Fase 9c: §4.1 (construir y desplegar), §4.6 (poner en marcha el
  dominio), §5.4 (workflows), §5.5 (pasos del desarrollador), §7.13
  (limitaciones), §11 (el guion de la web) y las decisiones D440 a D457.
- [`web/README.md`](../web/README.md): cómo se genera la web, de dónde sale cada
  cosa y cómo se cambia.
- [`publicar.md`](publicar.md): publicación del escritorio, política de
  versiones, la regla de `main` y los workflows.
- [`publicar-android.md`](publicar-android.md): la app Android, el keystore y la
  huella de su certificado.
- [`publicar-termux.md`](publicar-termux.md): el `.deb` de Termux, que también
  regenera la web al publicarse.
- [`docs/manual/estado.md`](manual/estado.md): qué se ha probado a mano.
- GitHub Pages con dominio propio:
  <https://docs.github.com/es/pages/configuring-a-custom-domain-for-your-github-pages-site>
- Verificar un dominio para GitHub Pages:
  <https://docs.github.com/es/pages/configuring-a-custom-domain-for-your-github-pages-site/verifying-your-custom-domain-for-github-pages>
- Publicar con una acción propia:
  <https://docs.github.com/es/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages>
