# Web de MAGI

Esta carpeta guarda lo propio de la web de MAGI (`https://magi.erratia.com`): las
plantillas, los estilos, las fuentes, los textos en castellano e inglés, los datos
de las plataformas y las capturas de la interfaz. Todo sale de `main`. El
generador es el crate `crates/magi-web` (biblioteca y binario `magi-web`), que
también trae de GitHub las versiones y los artefactos de cada release, prepara el
manual para mdBook y comprueba el sitio generado antes de desplegarlo.

Este fichero explica cómo se genera la web, de dónde sale cada cosa y cómo se
cambia. Cómo se pone en marcha (DNS, dominio, Pages), cómo se repite una
publicación y cómo se retira lo cuenta
[`docs/publicar-web.md`](../docs/publicar-web.md), que sigue el desarrollador.

## Índice

- [Qué hay en `web/`](#qué-hay-en-web)
- [Cómo se genera](#cómo-se-genera)
- [Las órdenes de `magi-web`](#las-órdenes-de-magi-web)
- [De dónde sale cada cosa](#de-dónde-sale-cada-cosa)
- [Cómo se cambia algo](#cómo-se-cambia-algo)
  - [Un texto](#un-texto)
  - [Una plataforma o su estado](#una-plataforma-o-su-estado)
  - [Una página](#una-página)
  - [Un capítulo del manual](#un-capítulo-del-manual)
- [Manual](#manual)
- [Capturas](#capturas)
- [Paletas y contraste](#paletas-y-contraste)
- [CSP](#csp)
- [Dominios permitidos](#dominios-permitidos)
- [Pruebas](#pruebas)
- [Fuentes](#fuentes)
  - [Qué fuentes hay y de dónde salen](#qué-fuentes-hay-y-de-dónde-salen)
  - [Cobertura de glifos](#cobertura-de-glifos)
  - [Rehacer el subconjunto de símbolos](#rehacer-el-subconjunto-de-símbolos)
  - [Reconvertir JetBrains Mono a woff2](#reconvertir-jetbrains-mono-a-woff2)

## Qué hay en `web/`

| Ruta | Qué es |
|---|---|
| `datos/plataformas.toml` | Las seis plataformas de descarga con su canal, estado, artefacto y orden, la huella del certificado del APK y los filtros de Obtainium |
| `textos/es.toml` y `textos/en.toml` | Los textos de las páginas, con las mismas claves en los dos idiomas |
| `plantillas/` | Las plantillas `minijinja`: `base.html`, una por página (`portada`, `descargas`, `versiones`, `404`) y, en `parciales/`, la cabecera, el pie y la figura de una captura |
| `estilos/web.css` | La hoja de estilos única: paletas, tipografía y cajas |
| `fuentes/` | JetBrains Mono y el subconjunto de símbolos, en woff2, con sus licencias (ver [Fuentes](#fuentes)) |
| `capturas/` | Las seis capturas de la TUI en HTML y `capturas.css`: **generadas** y versionadas, nunca a mano (ver [Capturas](#capturas)) |
| `manual/` | `book.toml` de mdBook y su tema propio, en `theme/` (ver [Manual](#manual)) |
| `favicon.svg` | El icono de la web |

El generador (`crates/magi-web`) y los scripts que lo lanzan
(`scripts/generar-web.sh` y `scripts/generar-capturas.sh`) viven fuera de `web/`.
`crates/magi-web` lleva `publish = false` y `dist = false`: `dist plan` no cambia
por él.

Lo que la web no hace, por regla: no lleva JavaScript propio (lo plegable va con
`<details>`; el único JavaScript es el de mdBook, en el manual), ni cookies, ni
analítica, ni nada cargado de fuera (fuentes, estilos e imágenes salen de la
propia web); ningún atributo `style` ni `<script>` en las plantillas; los
enlaces internos son relativos (solo el 404 usa URL absolutas) y los externos van
por `https://` y solo a la [lista permitida](#dominios-permitidos). Cumple WCAG 2.2
AA: `lang` y `hreflang`, enlace para saltar al contenido, foco visible, todo con
teclado, sin desplazamiento horizontal a 320 px, la animación plegable y oculta
con `prefers-reduced-motion`, y los estados en texto, no solo con color.

## Cómo se genera

`scripts/generar-web.sh` construye la web en local, en `target/web/`, con lo mismo
que hace `.github/workflows/web.yml`: compila `magi-web`, lee las releases de la
API de GitHub y sus `.sha256`, saca los CHANGELOG y el manual de la etiqueta de
la última versión publicada, prepara el manual, lo construye con mdBook, genera
el sitio y lo comprueba. El resultado queda en **`target/web/sitio`**; el resto
del trabajo (`releases.json`, los CHANGELOG sacados de la etiqueta…) queda en
`target/web/` y nada de eso se versiona.

```sh
scripts/generar-web.sh                           # con los datos reales de la API de GitHub
scripts/generar-web.sh --sin-red                 # con unos datos de ejemplo, sin llamar a GitHub
scripts/generar-web.sh --con-manual-de-la-rama   # el manual sale del árbol de trabajo
scripts/generar-web.sh --ayuda
```

| Opción o variable | Qué hace |
|---|---|
| `--sin-red` | No llama a GitHub: usa las releases de ejemplo de `crates/magi-web/tests/fijos/releases/sin-red.json`. Como esas etiquetas no existen en git, el manual y los CHANGELOG salen del árbol de trabajo y no de una etiqueta |
| `--con-manual-de-la-rama` | El manual sale de `docs/manual/` del árbol de trabajo y no de la etiqueta de la última versión publicada. Es lo que hace el job `web` de las PR en su segunda pasada, para comprobar un capítulo, un enlace o un ancla nuevos antes de que lleguen a una versión |
| `GH_TOKEN` o la sesión de `gh` | Credencial de la API de GitHub. En CI es el `GITHUB_TOKEN` del workflow (solo lectura); en local vale `gh auth login` |
| `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` | Con el valor exacto `true` (en minúsculas: los workflows de release comparan sin distinguir mayúsculas, la web no) encienden los bloques de Homebrew y de AUR en descargas; cualquier otro valor los apaga |
| `MAGI_WEB_MDBOOK` | Ruta de un `mdbook` que usar en vez del fijado (avisa si no es la versión fijada; también lo usan las pruebas que construyen el manual) |

Necesita `cargo`; para leer la API, `gh` y `curl` (con `--sin-red`, ninguno de
los dos), y, si no usas `--sin-red`, `git` con las etiquetas (si faltan, `git
fetch --tags`). Si hay que descargar mdBook (ver abajo), hacen falta también
`curl`, `sha256sum` y `tar`, con o sin `--sin-red`. **mdBook va fijado por versión (0.5.4) y SHA-256**, en
`web.yml`, en el job `web` de `ci.yml` y en el propio script, y una prueba de
`magi-web` comprueba que los tres coinciden. El script usa el `MAGI_WEB_MDBOOK`,
o el `mdbook` del `PATH` si es exactamente esa versión, o lo descarga a
`${XDG_CACHE_HOME:-$HOME/.cache}/magi/mdbook-<versión>/` y comprueba su suma
antes de usarlo (la descarga solo está preparada para Linux x86_64: en otro
sistema, instala esa versión y apunta `MAGI_WEB_MDBOOK`). mdBook no publica un
`.sha256`: la suma es la del tarball y coincide con el `digest` que da la API de
GitHub para ese fichero. Para subir de versión, cambia los tres sitios en el
mismo PR y repite la construcción y el guion de la web.

Códigos de salida: 0 bien, 1 fallo (falta una herramienta, un dato raro, la
construcción o la comprobación; el mensaje dice qué y dónde) y 2 uso incorrecto.
Si algo falla, no queda un sitio a medias donde estaba el anterior: el script
construye el sitio en `target/web/sitio.nuevo` (`magi-web construir` escribe en una
carpeta de trabajo y solo sustituye esa salida si todo salió bien), lo comprueba allí
y **solo entonces** sustituye a `target/web/sitio`; si la comprobación falla, el sitio
rechazado queda en `sitio.nuevo` para verlo y `sitio` no cambia. `magi-web construir`
solo sustituye una `--salida` que no existe, está vacía o es un sitio que construyó él
mismo: con otra cosa (la carpeta de descargas, por ejemplo) se niega y no la toca.

Para verlo en un navegador, sirve la carpeta solo en local:

```sh
python3 -I -m http.server 8000 --bind 127.0.0.1 --directory target/web/sitio
```

y abre `http://127.0.0.1:8000/`. El 404 se verá sin estilos: usa URL absolutas a
`https://magi.erratia.com/` y la CSP solo admite recursos de su propio origen.

## Las órdenes de `magi-web`

`cargo run -p magi-web -- <orden>` (o el binario de `target/release/`, que es el
que usa el script). Los errores salen como `error: dato raro en <dónde>: <qué>`
o, en `comprobar`, como una lista `<fichero>:<línea>: <qué>`; el código de salida
es distinto de 0.

| Orden | Qué hace |
|---|---|
| `releases --salida <json>` | Lee las releases de la API de GitHub (con `gh`, paginadas) y los `.sha256` de los artefactos elegidos (con `curl`), los valida y escribe `releases.json`. Lo que llega de fuera es entrada no fiable |
| `etiqueta --releases <json> --canal escritorio\|app` | Escribe la etiqueta de la última versión publicada del canal (nunca un borrador ni una candidata); sin release de la app, no escribe nada. Con ella el script sabe de qué etiqueta sacar el manual y los CHANGELOG |
| `preparar-manual --etiqueta <vX.Y.Z> [--repositorio .] [--arbol docs/manual] [--tema web/manual] --salida <dir>` | Copia `docs/manual/` de la etiqueta (o, con `--arbol`, de una carpeta de trabajo; en los dos casos los enlaces de fuera apuntan a esa etiqueta), escribe `SUMMARY.md`, fija las anclas y reescribe los enlaces que salen del manual. Deja la raíz de un libro (`book.toml`, `theme/` y `src/`) lista para `mdbook build`. Si la salida ya existe, tiene que estar vacía o ser un libro preparado antes |
| `construir --web web --releases <json> --changelog <md> [--changelog-app <md>] --manual <dir> [--animacion <gif>] --salida <dir> [--homebrew] [--aur]` | Genera portada, descargas, versiones y 404 en los dos idiomas, copia estilos, fuentes, capturas y animación, y pone cabecera, pie y CSP a todo, también al manual ya generado por mdBook, que es obligatorio: sin manual no hay web |
| `comprobar <dir>` | Revisa un sitio generado: enlaces internos y anclas, enlaces externos solo de la lista permitida y por `https://`, ningún atributo `style`, `srcdoc` ni `<script>` propio, nada antes de `<head>` que no sea el doctype, un comentario o `<html>` (con otra cosa el navegador no aplicaría la CSP), ningún SVG ni XML salvo `favicon.svg` y el favicon de mdBook (un SVG abierto directamente se ejecuta con el origen de la web) y la CSP de cada página. Un hallazgo para el despliegue |

`construir` no ejecuta `comprobar` por dentro: son dos pasos del flujo y los
lanza `scripts/generar-web.sh` (y con él, `web.yml`).

## De dónde sale cada cosa

| Qué | De dónde sale | Cuándo cambia en la web |
|---|---|---|
| Manual (`/manual/`) | `docs/manual/` de la **etiqueta de la última versión publicada del escritorio** (`git show`), nunca de `main` | Con la siguiente versión; un capítulo nuevo sale con ella |
| Versiones (`/versiones/`) | `CHANGELOG.md` y `android/CHANGELOG.md` de la etiqueta de la última versión publicada de su canal | Con la siguiente versión de cada canal |
| Versión, artefactos, tamaños, URL y SHA-256 de descargas | La API de GitHub y los `.sha256` publicados, en cada construcción (`releases.json`, sin versionar) | En cada construcción |
| Estado de cada plataforma, nombre de artefacto, huella del APK y Obtainium | `web/datos/plataformas.toml` (`main`) | Al fusionar en `main` |
| Plantillas, textos, estilos y fuentes | `web/plantillas/`, `web/textos/`, `web/estilos/` y `web/fuentes/` (`main`) | Al fusionar en `main` |
| Capturas | `web/capturas/` (`main`), generadas | Al fusionar en `main` |
| Animación de la portada | `assets/demo/magi.gif` (`main`) | Al fusionar en `main` |
| Bloques de Homebrew y AUR | Las variables del repositorio `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` | En la siguiente construcción |

Se elige la versión mayor **por número** (no la más reciente por fecha) de cada
canal: `vX.Y.Z` para el escritorio y Termux y `android-vX.Y.Z` para la app. Las
etiquetas con sufijo (`-rc.N`) y los borradores no cuentan. Termux comparte
release con el escritorio y puede ir un paso por detrás mientras
`termux-release.yml` sube su `.deb`: sale el de la última release que ya lo tiene,
con su número.

## Cómo se cambia algo

Después de cada cambio, lo mínimo es `cargo test -p magi-web --locked` y
`scripts/generar-web.sh --sin-red` (ver [Pruebas](#pruebas)). Un cambio que toca
`web/` lanza `web.yml` al fusionar en `main`.

### Un texto

Los textos están en `textos/es.toml` y `textos/en.toml`, con **las mismas claves**
en los dos y ninguna vacía: si una clave solo está en uno, la construcción falla
diciendo cuál y en qué fichero. Cambia siempre los dos.

- Sin HTML: lo que lleva un enlace se parte en `antes`, `enlace` y `despues`, y la
  plantilla pone el `<a>`; lo que va en código se escribe entre acentos graves y
  la plantilla lo pasa a `<code>` (filtro `con_codigo`).
- Un texto es una cadena o una tabla de textos (nada de números, listas ni
  fechas), y las claves solo llevan letras, cifras, `_` y `-`.
- La plantilla lo usa como `t.<tabla>.<clave>`; una clave que la plantilla pide y
  no existe rompe la construcción (las plantillas son estrictas).
- Un texto nuevo de una página nueva va en su propia tabla, sin tocar las demás.

### Una plataforma o su estado

`datos/plataformas.toml` tiene una sección `[app]` (huella del certificado y
filtros de Obtainium) y una `[[plataforma]]` por plataforma, con estos campos y
ninguno más (los desconocidos se rechazan):

| Campo | Qué es |
|---|---|
| `id` | Uno de `linux-x86_64`, `linux-arm64`, `macos-arm64`, `macos-x86_64`, `termux-arm64` o `android-arm64`, único |
| `canal` | `escritorio`, `termux` o `app`; el que corresponde al `id` |
| `estado` | `estable`, `en_pruebas` o `beta` |
| `artefacto` | Nombre del fichero publicado. Con `{version}` donde va el número de la versión; es obligatorio en Termux y en la app, y en el escritorio el nombre es fijo (sin él) |
| `orden` | Posición en la página: Linux, macOS, Termux y app; sin repetir |
| `instrucciones` | Clave de `textos/` con su instalación corta |

El **estado de una plataforma cambia en el mismo PR** que
[`docs/manual/estado.md`](../docs/manual/estado.md), y al revés. Para otra
plataforma que no sea una de las seis hay que tocar el código (`datos.rs`), los
textos y las plantillas. La huella del APK y qué hacer si cambia el keystore, en
[`docs/publicar-web.md`](../docs/publicar-web.md).

### Una página

1. La plantilla, en `plantillas/<pagina>.html` (hereda de `base.html`; sin `style`,
   sin `<script>` y sin `|safe`: una prueba prohíbe marcar nada como seguro en una
   plantilla). Si inserta HTML ya generado, se marca como seguro en Rust, tras
   validarlo.
2. La ruta, en `crates/magi-web/src/rutas.rs` (`Pagina`, `ruta`, `fichero`), y su
   módulo en `src/paginas/` con una función `renderizar` que parte de
   `contexto_comun` (declarada en `paginas/mod.rs`).
3. Sus textos, en `es.toml` y `en.toml`.
4. La lista de páginas de `construir` (`src/construir.rs`, la función `generar`),
   que las escribe en los dos idiomas con su CSP, y el menú de la cabecera
   (`plantillas/parciales/cabecera.html` y `nav` de `contexto_comun`) si va en él.
5. Sus pruebas, en `crates/magi-web/tests/`, y que `magi-web comprobar` siga
   dando el sitio por bueno.

### Un capítulo del manual

El manual es `docs/manual/`. Un capítulo nuevo es un fichero `docs/manual/<nombre>.md`
(solo `a-z`, cifras y `-`) **y su línea en `## Capítulos` de `docs/manual/README.md`**,
con la forma `- [Título](nombre.md) — descripción`. **Si el fichero no está en el
índice, la web no se construye** («capítulo fuera del índice: x.md»), y lo mismo si
el índice nombra un fichero que no existe. El capítulo sale en la web con la
siguiente versión, porque el manual sale de la etiqueta; el job `web` de la PR
construye también el manual de la rama y lo comprueba.

## Manual

`magi-web preparar-manual` toma `docs/manual/` de la etiqueta (o de un árbol de
trabajo) y deja la raíz de un libro de mdBook:

1. **`SUMMARY.md`** sale del índice `## Capítulos` de `README.md`, más `estado.md`
   («Qué se ha probado») al final.
2. **Anclas como en GitHub.** Cada título recibe su `{#id}` con el mismo algoritmo
   que usa la prueba `repositorio_publico` (vive en `magi_web::anclas`: hay una sola
   copia), así que `capitulo.md#ancla` lleva al mismo título que en GitHub.
3. **Enlaces que salen del manual.** Los de los README van a la portada (`../`) y
   a `/en/`; un fichero del repositorio, a `https://github.com/HectorEspejo/magi/blob/<etiqueta>/…`
   y una carpeta, a `…/tree/<etiqueta>/…`. Los que se quedan dentro los convierte
   mdBook a `.html`.
4. **HTML en bruto** (`<host>`, `<uuid>`…) se escapa como texto, y `{{#`, que mdBook
   toma por un preprocesador, se rechaza.
5. **Lo que mdBook escribiría con `style`** se rechaza antes de construir, con su
   fichero y su línea: una tabla con alineación de columnas (`|:--|--:|`), porque la
   CSP no admite atributos `style`. Quita los dos puntos de la fila de guiones.

6. **Más cosas que se rechazan al preparar, con su fichero y su línea:** un título (del
   capítulo o del índice) con «<» (el buscador de mdBook pone los títulos en la página
   como HTML y lo trataría como marcado); un capítulo con el nombre de una página de
   mdBook (`index`, `readme`, `toc`, `print`, `404`); un salto de línea solo CR; y
   cualquier SVG (se ejecutaría con el origen de la web: el manual solo admite
   imágenes `png`, `jpg`, `gif` y `webp`).

El índice del manual es `README.md` en el repositorio y `index.html` en la web:
mdBook convierte un enlace a `README.md` en `README.html`, que no existiría, así que
`preparar-manual` lo cambia por `index.md`.

Lo que se lee del repositorio es entrada no fiable: solo ficheros normales (un
enlace simbólico o un submódulo se rechazan), con tamaños acotados.

Después `mdbook build` usa `manual/book.toml` y el tema de `manual/theme/`: la
cabecera común de la web y sus paletas (claro u oscuro según el sistema, sin
selector de temas). La búsqueda de mdBook está activa, la ejecución de código
está desactivada y no se carga nada de fuera. Por último, `construir` mete el pie
común y la CSP en cada página del manual. La cabecera del manual es estática
(`manual/theme/header.hbs`) y una prueba comprueba que dice lo mismo que
`plantillas/parciales/cabecera.html`.

El manual está solo en castellano: lo avisa «Solo en castellano · Spanish only»
y el menú inglés dice «Manual (Spanish)». La interfaz propia de mdBook (la
búsqueda, los títulos de sus botones) sale en inglés, y su búsqueda encuentra
palabras enteras, no variantes («túnel» no encuentra «túneles»).

Límites conocidos del tema, que se resolverían con un `index.hbs` propio (una copia
de una plantilla de mdBook, con otra licencia y atada a la versión fijada) y no se
hizo: los textos de interfaz de mdBook (`title`, `aria-label`, el cuadro de
búsqueda) están en inglés dentro de una página `lang="es"`; el botón de la barra
lateral es una etiqueta y un teclado solo no lo alcanza con Tab en pantallas
estrechas; una tabla más ancha que su caja se desplaza dentro de ella, sin garantía
de foco de teclado; el DOM vivo lleva uno o dos `style` que escribe el JavaScript de
mdBook desde el navegador por CSSOM (el HTML entregado no lleva ninguno y la CSP no
bloquea esos), pero al cerrar la búsqueda con Esc mdBook usa `setAttribute('style')`,
que la CSP sí bloquea: sale un aviso en la consola («Applying inline style
violates…»), sin efecto visible (la búsqueda se cierra por clase); y mdBook copia sus fuentes (Open Sans y Source Code Pro, con sus
licencias) aunque el tema no las usa.

## Capturas

Las seis capturas de la portada (Flota, sesión con paneles, archivos, túneles,
deliberación MAGI y servicios) son la TUI pintada con `App::de_prueba` sobre un
`TestBackend` de 100×30, con el tema fijo (`Tema::respaldo()`) y el inventario de
la demo (`assets/demo/inventario`: `demo-web`, `demo-datos` y `demo-copias`),
exportada a HTML con **clases** (`<span class="cN">`, definidas en `capturas.css`;
nunca atributos `style`). Nunca se pinta con la semilla de las pruebas, cuyos
alias podrían ser reales: la prueba falla si una captura lleva un nombre de ella.

```sh
scripts/generar-capturas.sh      # regenera web/capturas/ y comprueba que quedan al día
git diff web/capturas            # revisa el diff antes de subirlo
```

El script lanza la prueba ignorada `generar_capturas` de `tests/capturas_web.rs` y
después la normal. La prueba normal, `las_capturas_estan_al_dia`, corre con
`cargo test --workspace` y **falla si una captura no está al día**, diciendo «captura
desfasada: ejecuta scripts/generar-capturas.sh». Por eso, cuando una PR cambia una
vista capturada, regenera su captura en el mismo PR. No se edita una captura a
mano. Lo único que necesita es `cargo`; escribe en `web/capturas/` y en una carpeta
fija bajo `/tmp/magi-capturas-web/`, cuya ruta sale entera en la captura de
Archivos.

Cada captura es una `<figure>` con su descripción como texto alternativo
(`role="img"` y `aria-label`) y el `<pre>` con `aria-hidden`, para que el lector de
pantalla no lea los bordes de caja. Cada carácter tiene que tener glifo en
JetBrains Mono o en el subconjunto de símbolos (ver [Cobertura de
glifos](#cobertura-de-glifos)); lo que no, se degrada con `texto_ascii`, el único
degradador.

## Paletas y contraste

La paleta oscura es la fija de la TUI (`src/tema.rs`) más un gris secundario
(`#9A968C`); la clara es su derivada. Las dos están en `estilos/web.css` como
variables en `:root` (la clara) y bajo `prefers-color-scheme: dark`. La prueba
`crates/magi-web/tests/contraste.rs` lee esas variables, calcula los contrastes
con la fórmula de WCAG y exige **4,5:1 o más en texto** (texto, secundario,
acento, correcto y crítico, sobre el fondo y sobre la superficie) y **3:1 o más
en bordes de controles y foco**. El borde decorativo (el «inactivo» de la TUI, que
en la oscura queda en 2,87:1) está exento a propósito: solo se usa en bordes que
no transmiten nada.

Si cambias un color de la TUI o de la web, repite `cargo test -p magi-web --test
contraste`; no cambies los nombres de las variables sin cambiar la prueba. Las
capturas conservan su fondo de terminal en los dos temas.

## CSP

GitHub Pages no deja poner cabeceras, así que la política de seguridad de contenido
va en un `<meta http-equiv="Content-Security-Policy">` justo después de `<meta
charset>`, en **cada página**, también en el manual. La pone `magi-web construir` al
final, con el HTML ya definitivo.

- Páginas propias: `script-src 'none'`, sin JavaScript.
- Manual: `script-src 'self'` más el hash `'sha256-…'` de **cada `<script>` en
  línea**, calculado al generar. Por eso no hace falta, ni se pone nunca,
  `'unsafe-inline'`: un script en línea nuevo del tema de mdBook entra solo en la
  CSP de esas páginas con su hash, y uno que cambie después de calcularlo hace
  fallar `comprobar`.
- El resto: `default-src 'none'; img-src 'self'; style-src 'self'; font-src
  'self'; base-uri 'none'; form-action 'none'` (y `frame-src 'self'` en el manual,
  por el índice lateral). Cada página lleva además `<meta name="referrer"
  content="strict-origin-when-cross-origin">`.
- Consecuencias para quien toque la web: ningún atributo `style` ni etiqueta
  `<style>` ni `<script>` en las plantillas, y los colores de las capturas, por
  clases. `frame-ancestors` no funciona en un `<meta>`: se acepta, porque la web
  no tiene nada que proteger de un marco.

## Dominios permitidos

`magi-web comprobar` solo deja enlaces externos por `https://` y a esta lista
(`ListaPermitida::por_defecto`, en `crates/magi-web/src/comprobar.rs`):

| Dominio | Qué se puede enlazar |
|---|---|
| `github.com` | Solo `/HectorEspejo/magi`, `/ImranR98/Obtainium` y `/termux-user-repository/` |
| `cli.github.com` | Todo (el GitHub CLI) |
| `docs.github.com` | Todo (la declaración de privacidad del pie) |
| `termux.dev` | Todo |
| `keepachangelog.com` y `semver.org` | Todo (los CHANGELOG) |
| `magi.erratia.com` | Todo (la propia web: sus recursos absolutos en el 404) |

**Un dominio nuevo entra en la lista en el mismo PR que lo enlaza**, con su prueba;
cualquier otro rompe `comprobar` y para el despliegue. Ojo con el CHANGELOG de una
etiqueta ya publicada: es inmutable, así que un enlace suyo a un dominio que no
esté en la lista para la construcción hasta ampliarla.

## Pruebas

| Qué | Cómo |
|---|---|
| El generador: datos, textos, plantillas, releases, CSP, `comprobar`, contraste, fuentes, workflows y construcción | `cargo test -p magi-web --locked` |
| Que las capturas están al día | `TZ=UTC cargo test -p magi-ssh --locked --test capturas_web` |
| Datos reales y restos de la organización anterior en todo el árbol, y enlaces relativos y anclas de los README, `CONTRIBUTING`, `SECURITY`, `docs/README.md`, el manual y `docs/publicar*.md` (**no** de este fichero: sus enlaces se revisan a mano) | `cargo test -p magi-ssh --locked --test repositorio_publico` |
| El sitio entero, hasta `comprobar` | `scripts/generar-web.sh --sin-red` (o sin `--sin-red`, con la API) |
| Los workflows y los scripts | `actionlint` y `shellcheck scripts/generar-web.sh scripts/generar-capturas.sh` |
| Antes de un commit | `cargo clippy -p magi-web --all-targets --locked -- -D warnings` y `cargo fmt --all --check` |

`cargo test --workspace` ya cubre `magi-web` (también que la portada pese menos
de 1 MB en los dos idiomas) y la prueba normal de capturas. Las pruebas que
ejecutan mdBook (`manual`, `manual_html` y `manual_mdbook`, que incluye el sitio
entero con el manual real y cero hallazgos) toman su ruta de `MAGI_WEB_MDBOOK`;
sin ella se omiten con un aviso (solo se ve con `--nocapture`), y con
`MAGI_EXIGIR_MDBOOK=1` fallan en vez de omitirse, que es lo que hace el job `web`
de `ci.yml`. Para correrlas en local:
`MAGI_WEB_MDBOOK=$(command -v mdbook) MAGI_EXIGIR_MDBOOK=1 cargo test -p magi-web
--test manual --test manual_html --test manual_mdbook`. Los
datos de ejemplo de las pruebas (`crates/magi-web/tests/fijos/`) son inventados:
nada real, como en el resto del repositorio. Antes de un PR que toque la web,
constrúyela y pasa `magi-web comprobar`.

## Fuentes

La web sirve sus fuentes desde sí misma (en `web/fuentes/`): ni Google Fonts ni
ningún otro dominio. Son tres ficheros woff2 y las licencias de lo que contienen.

### Qué fuentes hay y de dónde salen

| Fichero | Qué es | Origen | Licencia |
|---|---|---|---|
| `JetBrainsMono-Regular.woff2` | JetBrains Mono 2.305, peso normal | El TTF de la app Android (`android/app/src/main/res/font/jetbrains_mono_regular.ttf`), convertido a woff2 una vez | SIL OFL 1.1: [`fuentes/OFL.txt`](fuentes/OFL.txt) |
| `JetBrainsMono-Bold.woff2` | JetBrains Mono 2.305, negrita | El TTF de la app (`jetbrains_mono_bold.ttf`), convertido a woff2 una vez | La misma |
| `magi-simbolos.woff2` | Subconjunto de DejaVu Sans Mono 2.37 con los nueve glifos de la TUI que JetBrains Mono no trae | `DejaVuSansMono.ttf` del tarball oficial de DejaVu 2.37, recortado con `pyftsubset` | Bitstream Vera y dominio público (Arev): [`fuentes/LICENSE-DejaVu.txt`](fuentes/LICENSE-DejaVu.txt) |

`OFL.txt` es una copia literal de `android/app/src/main/res/raw/licencia_jetbrains_mono.txt`
(la misma que enseña la app en «Licencias») y `LICENSE-DejaVu.txt`, la del fichero
`LICENSE` del tarball de DejaVu, sin tocar. La lista de glifos del subconjunto, con
un comentario por símbolo, es `fuentes/magi-simbolos.unicodes`.

Lo que se descargó y las sumas SHA-256 de todo lo que interviene:

| Qué | SHA-256 |
|---|---|
| `dejavu-fonts-ttf-2.37.tar.bz2` | `fa9ca4d13871dd122f61258a80d01751d603b4d3ee14095d65453b4e846e17d7` |
| `ttf/DejaVuSansMono.ttf` (dentro del tarball) | `b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582` |
| `LICENSE` (dentro del tarball) = `LICENSE-DejaVu.txt` | `7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93` |
| `jetbrains_mono_regular.ttf` (de la app) | `e6fd0d7e91550b3ed2b735d4312474362c4716edc4fc0577a0f61ed782d5aed1` |
| `jetbrains_mono_bold.ttf` (de la app) | `d22c4f3821d725eb01210d278d95dfcfcaadc34699a06658d47c8a5cc5830ada` |
| `licencia_jetbrains_mono.txt` (de la app) = `OFL.txt` | `a76abf002c49097d146e86740a3105a5d00450b1592e820a1109a8c5680cd697` |
| `JetBrainsMono-Regular.woff2` (resultado) | `139a4008ab7480821fe5f7e2a42a1e39782a882ee7deb09e73eff4f9b7a00773` |
| `JetBrainsMono-Bold.woff2` (resultado) | `131a42ad32d2a136a9ec561497d6408732dbdafa888797a2a7601627179a7efd` |
| `magi-simbolos.woff2` (resultado) | `17981eac73141b2eff3fc55a592c08cbd3a284c3e3415ebe0d04d6c4afa4794d` |

El tarball se baja de la release oficial de DejaVu en GitHub:

```text
https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2
```

Herramientas con las que salieron los tres woff2 (versiones que dejan los mismos
bytes; con otras, lo que cuenta es que pasen las verificaciones de más abajo):

| Herramienta | Versión | Para qué |
|---|---|---|
| `fonttools` (`pyftsubset`) | 4.68.0 | Recortar DejaVu Sans Mono y comprobar glifos |
| `brotli` (módulo de Python) | 1.2.1 | Comprimir el woff2 que escribe `pyftsubset` |
| Python | 3.14 | Ejecutar fontTools |
| `woff2_compress` y `woff2_decompress` de Google (paquete `woff2`) | 1.0.2 | Convertir JetBrains Mono a woff2 y deshacerlo para verificar |

La licencia de DejaVu admite recortar y modificar la fuente siempre que el
resultado no se llame «Bitstream» ni «Vera». El subconjunto conserva el nombre
«DejaVu Sans Mono» y el aviso de copyright en su tabla `name`, así que cumple.

**Avance de los glifos.** JetBrains Mono avanza 0,600 em (600 de 1000 unidades) y
DejaVu Sans Mono 0,602 em (1233 de 2048, que son 0,6021): un 0,34 % más. Cada
símbolo del subconjunto ocupa 0,0034 columnas más que un carácter de JetBrains
Mono, y solo lo suma él: una línea con unos pocos símbolos se corre una centésima
de columna, y las cajas de las capturas no se descuadran. El `@font-face` de
`magi-simbolos` debe declarar con `unicode-range` los mismos puntos de código que
`fuentes/magi-simbolos.unicodes`.

### Cobertura de glifos

JetBrains Mono 2.305 no tiene diez de los glifos que usa la TUI en sus
instantáneas: `↵ ↻ ⇄ ⇅ ▣ ◐ ◑ ★ ⚑ ⤴`. Los nueve primeros están en el subconjunto. El
décimo, `⤴` (U+2934, el «salto» de la vista Hosts), tampoco está en DejaVu Sans
Mono: en las capturas se cambia por su equivalente ASCII de
`disposicion::texto_ascii`, que es el único degradador.

`crates/magi-web/src/fuentes.rs` lo vigila sin salir de Rust:

- `codigos_de_ttf` lee la tabla `cmap` de un TTF (formatos 4 y 12) y devuelve los
  puntos de código con glifo; se usa sobre los TTF de la app, que son los mismos
  que se convirtieron a woff2.
- `leer_unicodes` lee `fuentes/magi-simbolos.unicodes`, que es a la vez la entrada
  de `pyftsubset` y la fuente de verdad de la prueba. Los woff2 no se leen:
  el subconjunto se comprueba contra esa lista y, al rehacerlo, con fontTools.
- La prueba de cobertura de las capturas falla si una captura trae un carácter
  que no está en JetBrains Mono, ni en el subconjunto, ni en la tabla de degradación.

Si cambias la lista, cambia en el mismo PR el subconjunto (con el procedimiento de
abajo), el `unicode-range` del CSS y la prueba `el_subconjunto_declara_nueve_de_los_diez`.

### Rehacer el subconjunto de símbolos

Se hace a mano, una vez por cambio de la lista. Todo se descarga y se prepara fuera
del repositorio, en `$HOME/.cache/magi/fuentes/`, con cada descarga en su propio
directorio nuevo y vacío. Los scripts de Python viven fuera de lo descargado y se
ejecutan siempre con `python3 -I`. Lanza los bloques en orden, en la misma terminal
(comparten variables) y empezando desde dentro del repositorio.

**Paso 1.** Descarga DejaVu 2.37 y comprueba su suma:

```sh
BASE="$HOME/.cache/magi/fuentes"
mkdir -p "$BASE/scripts"

mkdir "$BASE/dejavu-2.37"
curl -fL -o "$BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" \
  https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2
echo "fa9ca4d13871dd122f61258a80d01751d603b4d3ee14095d65453b4e846e17d7  $BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" | sha256sum -c -

mkdir "$BASE/dejavu-2.37-extraido"
tar -xjf "$BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" -C "$BASE/dejavu-2.37-extraido"
DEJAVU="$BASE/dejavu-2.37-extraido/dejavu-fonts-ttf-2.37"
echo "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582  $DEJAVU/ttf/DejaVuSansMono.ttf" | sha256sum -c -
echo "7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93  $DEJAVU/LICENSE" | sha256sum -c -
```

En macOS, `sha256sum` es `shasum -a 256`.

**Paso 2.** Crea el entorno de Python con las versiones de arriba:

```sh
python3 -I -m venv "$BASE/venv"
"$BASE/venv/bin/python" -I -m pip install fonttools==4.68.0 brotli==1.2.1
```

**Paso 3.** Comprueba que DejaVu Sans Mono tiene los nueve glifos y no tiene `⤴`, y mira su
avance:

```sh
"$BASE/venv/bin/python" -I - "$DEJAVU/ttf/DejaVuSansMono.ttf" <<'EOF'
import sys
from fontTools.ttLib import TTFont

fuente = TTFont(sys.argv[1])
cmap = fuente.getBestCmap()
for codigo in (0x21B5, 0x21BB, 0x21C4, 0x21C5, 0x25A3, 0x25D0, 0x25D1, 0x2605, 0x2691, 0x2934):
    glifo = cmap.get(codigo)
    print(f"U+{codigo:04X} {chr(codigo)}", glifo or "SIN GLIFO")
print("unidades por em:", fuente["head"].unitsPerEm)
EOF
```

Salen los nueve primeros con glifo y `U+2934` con «SIN GLIFO».

**Paso 4.** Edita `web/fuentes/magi-simbolos.unicodes` si hace falta y genera el subconjunto
en un directorio nuevo (el aviso `FFTM NOT subset` es inocuo):

```sh
REPO="$(git rev-parse --show-toplevel)"
mkdir "$BASE/salida-simbolos"
cd "$BASE"
"$BASE/venv/bin/pyftsubset" "$DEJAVU/ttf/DejaVuSansMono.ttf" \
  --unicodes-file="$REPO/web/fuentes/magi-simbolos.unicodes" \
  --flavor=woff2 --layout-features= --no-hinting --desubroutinize \
  --output-file="$BASE/salida-simbolos/magi-simbolos.woff2"
ls -l "$BASE/salida-simbolos/magi-simbolos.woff2"
```

- `--unicodes-file` es la lista de este repositorio, tal cual.
- `--layout-features=` (vacío) quita las características OpenType: los símbolos
  no las necesitan.
- `--no-hinting` quita las instrucciones de ajuste y `--desubroutinize` aplana
  las subrutinas (solo importa en fuentes CFF; en TrueType es inocuo).
- `--flavor=woff2` pide el módulo `brotli` del paso 2.

Debe pesar unos pocos KB (hoy, 1168 bytes).

**Paso 5.** Verifica con fontTools que contiene exactamente los glifos de la lista (nueve) y
ninguno más, y apunta su avance:

```sh
cat > "$BASE/scripts/ver_simbolos.py" <<'EOF'
import re, sys
from fontTools.ttLib import TTFont

woff2, unicodes = sys.argv[1], sys.argv[2]

# Los puntos de código de la lista (U+XXXX o U+XXXX-YYYY; lo que sigue a «#» es un comentario).
esperados = set()
for linea in open(unicodes, encoding="utf-8"):
    for ini, fin in re.findall(r"U\+([0-9A-Fa-f]+)(?:-([0-9A-Fa-f]+))?", linea.split("#")[0]):
        esperados.update(range(int(ini, 16), int(fin or ini, 16) + 1))

fuente = TTFont(woff2)
cmap = fuente.getBestCmap()
assert set(cmap) == esperados, "sobran o faltan: " + ", ".join(f"U+{c:04X}" for c in sorted(set(cmap) ^ esperados))
assert len(cmap) == 9 and 0x2934 not in cmap
em = fuente["head"].unitsPerEm
for codigo in sorted(cmap):
    avance = fuente["hmtx"][cmap[codigo]][0]
    print(f"U+{codigo:04X} {chr(codigo)}  avance {avance}/{em} = {avance / em:.4f} em")
print("OK: exactamente los 9 glifos de la lista")
EOF
"$BASE/venv/bin/python" -I "$BASE/scripts/ver_simbolos.py" \
  "$BASE/salida-simbolos/magi-simbolos.woff2" "$REPO/web/fuentes/magi-simbolos.unicodes"
```

Hoy los nueve avanzan 1233/2048 = 0,6021 em, frente a 0,600 de JetBrains Mono.

**Paso 6.** Copia el resultado y repite las pruebas:

```sh
cp "$BASE/salida-simbolos/magi-simbolos.woff2" "$REPO/web/fuentes/magi-simbolos.woff2"
cd "$REPO" && cargo test -p magi-web --test fuentes
```

Si `LICENSE-DejaVu.txt` cambia con otra versión de DejaVu, cópiala tal cual del
tarball nuevo y actualiza las sumas de este fichero.

### Reconvertir JetBrains Mono a woff2

Los woff2 de JetBrains Mono salen de los TTF que ya lleva la app Android (la 2.305),
convertidos con `woff2_compress`, que escribe el resultado junto a su entrada: por
eso se trabaja sobre copias, en un directorio nuevo.

```sh
BASE="$HOME/.cache/magi/fuentes"
REPO="$(git rev-parse --show-toplevel)"
FUENTES_APP="$REPO/android/app/src/main/res/font"

mkdir -p "$BASE/scripts"
mkdir "$BASE/jetbrains-mono"
cp "$FUENTES_APP/jetbrains_mono_regular.ttf" "$BASE/jetbrains-mono/JetBrainsMono-Regular.ttf"
cp "$FUENTES_APP/jetbrains_mono_bold.ttf"    "$BASE/jetbrains-mono/JetBrainsMono-Bold.ttf"
(cd "$BASE/jetbrains-mono" && woff2_compress JetBrainsMono-Regular.ttf && woff2_compress JetBrainsMono-Bold.ttf)

cp "$BASE/jetbrains-mono/JetBrainsMono-Regular.woff2" "$BASE/jetbrains-mono/JetBrainsMono-Bold.woff2" "$REPO/web/fuentes/"
cp "$REPO/android/app/src/main/res/raw/licencia_jetbrains_mono.txt" "$REPO/web/fuentes/OFL.txt"
```

`woff2_compress` viene en el paquete `woff2` (Arch: `pacman -S woff2`; Debian y
Ubuntu: `apt install woff2`; macOS: `brew install woff2`). El resultado es
determinista: con la misma versión, las sumas de la tabla de arriba.

Para verificar, compara el woff2 con el TTF de la app (mismo orden de glifos, mismos
puntos de código, mismos avances y mismos contornos). Hace falta el entorno de
Python del apartado anterior:

```sh
cat > "$BASE/scripts/ver_jetbrains.py" <<'EOF'
import sys
from fontTools.ttLib import TTFont

original, convertida = TTFont(sys.argv[1]), TTFont(sys.argv[2])
assert original.getGlyphOrder() == convertida.getGlyphOrder(), "el orden de glifos cambió"
assert {(t.platformID, t.platEncID): t.cmap for t in original["cmap"].tables} == \
       {(t.platformID, t.platEncID): t.cmap for t in convertida["cmap"].tables}, "la tabla cmap cambió"
assert original["hmtx"].metrics == convertida["hmtx"].metrics, "los avances cambiaron"
for nombre in original.getGlyphOrder():
    a, b = original["glyf"][nombre], convertida["glyf"][nombre]
    assert a.numberOfContours == b.numberOfContours, nombre
    if a.numberOfContours:
        assert a.getCoordinates(original["glyf"])[:2] == b.getCoordinates(convertida["glyf"])[:2], nombre
print("OK:", len(original.getBestCmap()), "puntos de código y", len(original.getGlyphOrder()), "glifos idénticos")
EOF
for par in regular:Regular bold:Bold; do
  peso="${par%%:*}"
  nombre="${par##*:}"
  "$BASE/venv/bin/python" -I "$BASE/scripts/ver_jetbrains.py" \
    "$FUENTES_APP/jetbrains_mono_$peso.ttf" "$REPO/web/fuentes/JetBrainsMono-$nombre.woff2"
done
```

Cada uno debe terminar con `OK: 1372 puntos de código y 1754 glifos idénticos`.
Después, `cargo test -p magi-web --test fuentes` comprueba, además, que los TTF de la
app siguen sin los diez glifos de la TUI y que los tres ficheros son woff2.
