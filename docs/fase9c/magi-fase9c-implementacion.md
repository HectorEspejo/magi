# MAGI - Fase 9c: Web de MAGI · Informe de implementación

> Documento vivo: se actualiza en cada sesión, nunca se regenera. Estado (22 nov 2026): **Sprints 1 y 2 hechos** (generador y descargas; portada, capturas y versiones; 41 / 73); falta el Sprint 3 (manual, `applicationId` y documentación), la revisión adversarial de cierre y los pasos del desarrollador.

## 1. Resumen de lo implementado

Rama `fase9c` desde `main` con la 1.6.1 publicada. Método de las fases anteriores: contrato compilable en un commit provisional, pistas en paralelo en worktrees con su autorrevisión adversarial, integración y un commit por sprint.

- **Paso 0.** Entrega copiada a su sitio (`docs/magi-maestro.md`, el informe v1.2 y el checklist de la 9b, el checklist y el prompt de reanudación de la F17). `CLAUDE.md` y `AGENTS.md` de la raíz ya eran idénticos al `CLAUDE.md` de la entrega: no había nada que aplicar, solo commitear (`e458401`).
- **Spike de mdBook 0.5.4** (antes del contrato, fuera del repositorio). Resultado en §4: mdBook respeta `{#id}`, sus scripts en línea son idénticos en todas las páginas, no hace falta `'unsafe-inline'` y la CSP candidata dio 0 violaciones en Chromium 152.
- **Sprint 1 · generador y descargas.** Crate `magi-web` (biblioteca y binario, `publish = false`, `dist = false`, versión 0.1.0) con 18 módulos, y `web/` con los datos, textos, plantillas, estilos y fuentes. Cinco pistas en paralelo sobre el contrato `e47b509`:
  - **P1A** (datos, releases y descargas): `ordenes` (plazo y tope de salida, sin shell), `datos` (`plataformas.toml` validado), `releases` (API paginada por `gh`, `.sha256` por `curl`, ambos como subprocesos) y `descargas` (la última versión por número y canal, Termux con `.deb`, huella del APK).
  - **P1B** (estático): `textos`, `rutas`, `plantillas` (minijinja con autoescape y `UndefinedBehavior::Strict`), `contraste` (WCAG), las páginas de descargas y 404 en los dos idiomas, `web.css` con las dos paletas y el favicon.
  - **P1C** (CSP y comprobar): tokenizador de HTML que falla cerrado, CSP por hashes calculada al final, clasificación de enlaces y `magi-web comprobar`.
  - **P1D** (CI): `web.yml`, los jobs `cambios-web` y `web` de `ci.yml`, `scripts/generar-web.sh` y `tests/workflows.rs`.
  - **P1E** (fuentes): JetBrains Mono 2.305 en woff2, el subconjunto `magi-simbolos.woff2` de DejaVu Sans Mono con sus nueve glifos, las licencias y el lector de `cmap` de la prueba de cobertura.
  - **Integración (S1-INT).** `construir.rs`: lee y **revalida** `releases.json`, renderiza las páginas, copia los estáticos con topes y sin seguir enlaces simbólicos, pone la CSP al final y escribe en un directorio hermano que sustituye al anterior solo si todo salió bien. Las anclas de GitHub pasaron ya a `magi-web/src/anclas.rs` (D452) y `repositorio_publico` las usa desde allí.
- **Sprint 2 · portada, capturas y versiones.** Contrato `3602b0e` y cuatro pistas en paralelo, cada una con su autorrevisión adversarial (8 agentes):
  - **P2A** (exportador de capturas): `capturas.rs` pasa una rejilla de celdas de la TUI a HTML con **clases** (`<span class="cN">`, numeradas por primera aparición en las seis capturas, sin atributos `style`), con la paleta xterm para `Indexado`, `Reset` resuelto antes de `REVERSED`, cobertura de glifos y degradación con `texto_ascii` solo si devuelve un único carácter distinto de `?`. Trae también las dos barreras de lo que la portada inserta sin escapar: `validar_fragmento` (solo un `<pre class="captura" aria-hidden="true">` con texto y `<span class="cN">`, sin nada antes ni después) y `validar_css` (parser propio con lista blanca de selectores, propiedades y valores). Probado con 6 mutaciones.
  - **P2B** (prueba de la TUI): `tests/capturas_web.rs` pinta las seis vistas con `App::de_prueba` y `TestBackend` 100×30, tema `Tema::respaldo()` y **solo** el inventario de la demo (`demo-web`, `demo-datos`, `demo-copias`); `generar_capturas` (ignorada) escribe `web/capturas/` y `las_capturas_estan_al_dia` compara byte a byte y dice cómo regenerar. `scripts/generar-capturas.sh` lanza las dos y enseña `git status`.
  - **P2C** (portada): botón «Descargar X.Y.Z», `<details open>` con la animación (`loading="lazy"`), la captura de Flota en su lugar con `prefers-reduced-motion`, seis figuras con `role="img"` y `aria-label` (el `<pre>` va `aria-hidden`), plataformas por familia con su estado en texto, sin telemetría y licencia.
  - **P2D** (versiones): `versiones.rs` parte los dos CHANGELOG (fuera «Sin publicar» y los comentarios) y pasa el detalle a HTML con `pulldown-cmark`: HTML en bruto como texto, sin imágenes, solo enlaces `https://` válidos y los relativos reescritos a GitHub en la etiqueta. La página enseña el resumen en el idioma de la página, el detalle en un `<details>` y el enlace a su release.
  - **Integración (S2-INT).** `construir` carga las capturas, lee los CHANGELOG, comprueba que traen la versión de la release y escribe portada y versiones en los dos idiomas; `magi-web etiqueta` dice de qué etiqueta sacar los CHANGELOG; `scripts/generar-web.sh` los extrae con `git show` (o del árbol con `--sin-red`) y retira `CONTRATO-S2-ALLOW`. La portada pesa **787 770 bytes** (HTML, hojas, GIF y fuentes): no hizo falta el subconjunto de JetBrains Mono.
- **Comprobación visual del S2** con Chromium headless (protocolo de depuración, con emulación de `prefers-color-scheme` y `prefers-reduced-motion`): las seis páginas propias, en claro y oscuro y a 320 y 1280 px, sin desplazamiento horizontal (`scrollWidth == innerWidth`), con cero atributos `style`, ninguna petición a otro dominio y ninguna violación de CSP; con movimiento reducido el GIF mide 0×0 y la captura de Flota ocupa su sitio. Solo el 404 pide recursos absolutos a `magi.erratia.com` (ver §6).
- **Comprobado con la API real** (solo lectura): `magi-web releases` leyó las 3 releases y las 5 sumas publicadas (los dos formatos de `.sha256`) y `construir` generó el sitio con ellas; la página lleva la suma real del tarball de Linux x86_64.
- **Comprobación visual del S1** con Chromium headless: la página de descargas se ve con la estética de la TUI y, a 320 px, sin desplazamiento horizontal.

## 2. Desviaciones respecto a la especificación (qué y por qué)

Decididas con el desarrollador el 22 nov (D-1 a D-4) y de diseño (D-5 en adelante); ninguna cambia lo que el checklist pide.

1. **Orden de Pages.** `docs/publicar-web.md` (S3) pedirá verificar el dominio y poner el origen «GitHub Actions» **antes** de fusionar, porque el push a `main` en `web/**` dispara `web.yml` y `desplegar` falla en rojo sin Pages. El informe lo ponía después (§11).
2. **Línea del `applicationId` en «Sin publicar»** de `android/CHANGELOG.md` (como el informe), aunque el extractor de `android-release.yml` lee primero `[0.2.0]` y no la recogería: `docs/publicar-android.md` recordará moverla y fecharla al publicar (S3).
3. **Restos de «4d3»/«Cuatro de Tres» fuera del checklist:** solo anotados (§6). Solo se toca el `-dname` de `docs/publicar-android.md` (S3).
4. **Cuatro extras fuera del checklist, autorizados:** excluir `crates/magi-web/**` de `android.yml` (S3), actualizar `docs/manual/estado.md` (S3), un favicon SVG propio (`web/favicon.svg`, hecho) y el subconjunto de JetBrains Mono **solo si** la portada pasa de 1 MB (S2, no hecho por ahora).
5. **Anclas en el Sprint 1** (el informe las pone en el S3): evita mantener dos copias durante la fase. `slug_base`, `slugs_de` y `encabezados_de` se movieron tal cual, y `magi-ssh` depende de `magi-web` como `dev-dependency`.
6. **Permisos de `web.yml` por job** (`docs/publicar.md` exige que los extra vayan solo en el job que los necesita): `contents: read` arriba y `pages: write` e `id-token: write` solo en `desplegar`. El informe (§5.4) los lista sin decir dónde.
7. **`releases.json` normalizado** (`{releases, sumas}`): `magi-web releases` lo escribe y `construir` lo vuelve a validar. Así los tests y `--sin-red` pasan por el mismo código, sin un modo «de confianza». `descargar_releases` quita el `body` de todas las releases salvo el de la release elegida de la app (lo único que se lee: la huella).
8. **Termux** elige la release más nueva con el `.deb` **y** su `.sha256` en estado `uploaded` (el informe solo pide el `.deb`): cierra la carrera de la subida.
9. **Validación más estricta que el informe**, anotada por P1A: `plataformas.toml` rechaza campos desconocidos y exige que cada id lleve el canal que le corresponde; la huella se lee línea a línea (tolera CRLF) y falla si alguna de varias difiere; `curl` lleva además `-q` (no leer `.curlrc`).
10. **`docs.github.com`** entra en la lista permitida de `comprobar` (enlace de privacidad del pie), en el mismo PR que lo enlaza, como manda la regla.
11. **`construir` no ejecuta `comprobar` por dentro:** son dos pasos del flujo (§4.1) y los lanza `scripts/generar-web.sh`. El informe los junta en la descripción de la CLI; el efecto es el mismo y las pruebas pueden construir un sitio incompleto.
12. **Marcas.** Lo que queda del manual y de los enlaces a páginas aún sin generar lleva `CONTRATO-S3-*` (no `LEGADO-S1-*`): la regla pide que al cerrar un sprint no quede ninguna marca suya.
13. **mdBook no publica un `.sha256` por activo** (404): el SHA-256 fijado (`5222beab…be89`) es el que calculé en local sobre el tarball y coincide con el `digest` de la API de GitHub. Se fija igual, por versión y suma, en `web.yml`, `ci.yml` y `scripts/generar-web.sh` (una prueba comprueba que coinciden).
14. **`magi-web etiqueta`** (subcomando que el informe no lista): imprime la etiqueta de la última versión publicada de un canal (`--canal escritorio|app`; sin release de la app, nada). Lo usa `generar-web.sh` para saber de qué etiqueta sacar los CHANGELOG (y, en el S3, el manual) sin repetir en `jq` la regla de «última por número, nunca borrador ni `-rc`»: llama a `descargas::elegir`, la misma que `construir`.
15. **`construir` valida los CHANGELOG contra la release** (el informe no lo pide): el del escritorio tiene que traer la versión elegida y, si hay release de la app, hace falta `--changelog-app` con la suya; sin release, ese fichero no se usa. Evita publicar el CHANGELOG de otra etiqueta o de otro canal.
16. **Sin `|safe` en las plantillas.** El informe y la tarea de las pistas lo pedían para las capturas y el detalle de cada versión, pero la prueba de S1 `las_plantillas_nunca_marcan_nada_como_seguro` lo prohíbe. P2C y P2D marcan como seguro en Rust (`minijinja::Value::from_safe_string`) solo el fragmento ya validado, justo antes de pintar. Efecto: ninguna plantilla puede marcar nada como seguro.
17. **`Capturas::cargar` quita un salto de línea final** (uno) antes de validar: P2B escribe los ficheros terminados en `\n` y `validar_fragmento` no admite nada tras `</pre>`. El validador no se afloja; hay prueba de que dos saltos se rechazan.
18. **Textos de la portada fuera de lo enumerado** (P2C): claves `portada.titulo_pagina`, `capturas_titulo`, `capturas_nota` (en inglés avisa de que la interfaz de las capturas está en castellano), `que_es.movil` y `plataformas.introduccion`. La portada reutiliza `descargas.estado.*`, y su fila por plataforma enseña el estado menos maduro de la familia.
19. **La captura de Flota está dos veces en el HTML de la portada** (la que sustituye a la animación con movimiento reducido y la de la rejilla), con una oculta por CSS: ~25 KB. Sin JavaScript no hay otra forma de cambiar una por otra.
20. **La prueba de capturas es larga** (`tests/capturas_web.rs`, ~2 450 líneas frente a las ~60 del arnés previsto): lleva el arnés propio, el adaptador `Buffer` → celdas, la cobertura, las seis vistas y las pruebas negativas. Escribe y lee `/tmp/magi-capturas-web/` (carpeta fija bajo `flock`, que sale entera en la captura de Archivos) y falla si sobra algo o es de otro usuario.
21. **Capturas**: en Flota se selecciona `demo-datos`, porque la TUI pinta la fila seleccionada en naranja y con `NOMINAL` en verde (≈1,5:1) o `⚠` naranja sobre naranja queda invisible: es el pintado real de la TUI y no se toca aquí (§6). En la sesión se usan los azules ANSI brillantes (`1;94`), porque el azul estándar (`#0000ee`) casi no se lee sobre `#0a0a0a`.
22. **Texto público con jerga** (P2D): «La primera release de la app sale después de esta fase» es literal del mockup del informe (§6.3) y se deja tal cual; desaparece cuando haya release de la app. Y la página de versiones, a diferencia de la de descargas, no enlaza al capítulo `app-android` (el §7.12 y el §6.3 no coinciden).

## 3. Estructura de archivos creada/modificada

- **Crate `crates/magi-web/`** (nuevo): `Cargo.toml`; `src/{lib,main,error,ordenes,datos,textos,rutas,plantillas,contraste,releases,descargas,anclas,html,csp,enlaces,comprobar,fuentes,construir}.rs` y `src/paginas/{mod,descargas,no_encontrada}.rs`; `tests/{comprobar,construir,contraste,csp,datos,descargas,enlaces,fuentes,html,ordenes,paginas,plantillas,releases,rutas,textos,workflows}.rs`; `tests/fijos/{releases,manual-html,sitio-bueno,sitio-malo}/` (datos de ejemplo, todos inventados).
- **`web/`** (nuevo): `datos/plataformas.toml`, `textos/{es,en}.toml`, `plantillas/{base,descargas,404}.html` y `parciales/{cabecera,pie}.html`, `estilos/web.css`, `fuentes/` (3 woff2, `magi-simbolos.unicodes`, `OFL.txt`, `LICENSE-DejaVu.txt`), `favicon.svg`, `README.md` (esqueleto y sección de fuentes).
- **Sprint 2:** `crates/magi-web/src/{capturas,versiones}.rs`, `src/paginas/{portada,versiones}.rs`, `tests/{capturas,portada,versiones}.rs` y `tests/fijos/{capturas,changelog}/`; `web/plantillas/{portada,versiones}.html` y `parciales/captura.html`; `web/capturas/{flota,sesion-paneles,archivos,tuneles,deliberacion,servicios}.html` y `capturas.css` (generados); `tests/capturas_web.rs` y `scripts/generar-capturas.sh`; secciones de la portada y de las versiones en `web/estilos/web.css` y en los dos TOML de textos; `construir.rs`, `main.rs` (subcomando `etiqueta`), `lib.rs` y `tests/construir.rs`; `scripts/generar-web.sh` (CHANGELOG).
- **Workflows y scripts:** `.github/workflows/web.yml` (nuevo), `.github/workflows/ci.yml` (jobs `cambios-web` y `web`), `scripts/generar-web.sh` (nuevo).
- **Modificados:** `Cargo.toml` (`members`, `homepage` = `https://magi.erratia.com`, `dev-dependencies`), `Cargo.lock`, `README.md` y `README.en.md` (una línea con la web), `tests/repositorio_publico.rs` (usa `magi_web::anclas`).
- **Documentación:** `docs/magi-maestro.md`, `docs/fase9b/` (informe v1.2 y checklist), `docs/fase17/` (checklist y prompt de reanudación), `docs/fase9c/` (entrega, este informe y el checklist), `CLAUDE.md` y `AGENTS.md`.

## 4. Decisiones técnicas tomadas durante el desarrollo

- **Spike de mdBook 0.5.4** (fuera del repositorio):
  - `## Título {#id}` genera ese `id`, el `{#…}` sale del título y del índice de búsqueda y `x.md#a` pasa a `x.html#a`: no hace falta reescribir ids en el HTML.
  - Seis `<script>` en línea por página (cinco con `copyable = false`), **idénticos en todas las páginas**: la CSP por hashes es viable y no lleva `'unsafe-inline'`.
  - Dos cosas chocan con `style-src 'self'` y se resuelven en el Sprint 3: `<div style="clear: both">` (se sustituye por una clase) y un `data:` SVG del botón de copiar (se apaga con `[output.html.playground] copyable = false`).
  - `toc.html` va en un `<iframe>` (relleno sin JavaScript): hace falta `frame-src 'self'`. `404.html` lleva `<base href="/">`: se quita con `input-404 = ""`. `print.html` se quita con `[output.html.print] enable = false`.
  - `theme/head.hbs`, `theme/header.hbs` (con `path_to_root`), `theme/css/variables.css` y `additional-css` funcionan; el selector de temas sigue en el HTML y se oculta por CSS. mdBook copia todo lo que no es `.md` de `src/` (salió un `.md.orig`): `preparar-manual` copiará solo `.md`.
  - La búsqueda funciona bajo la CSP (`?search=tunel` da un resultado); `localStorage` guarda `mdbook-theme` y `mdbook-sidebar`. Los `title` y `aria-label` de la interfaz de mdBook están en inglés: limitación a evaluar en el S3.
  - Con `--force-dark-mode` Chromium headless activa el tema oscuro; `--blink-settings=preferredColorScheme=1` no.
- **Entrada no fiable.** `ordenes::ejecutar_con_plazo` no usa shell, mata al vencer el plazo o el tope de bytes, y no pone argumentos ni entorno en sus errores. Todo texto de fuera que va en un mensaje pasa por `texto_seguro` (sin controles ni invisibles que falsean la lectura). `elegir` revalida desde `releases.json` los nombres, el tamaño, el estado `uploaded`, `browser_download_url` y `html_url` por igualdad exacta y las sumas.
- **HTML.** El tokenizador falla cerrado: etiqueta, comentario, comillas o script sin cerrar son un error. Lee `title`, `textarea`, `xmp`, `iframe`, `noembed`, `noframes` y `noscript` como texto en bruto (donde un `<script>` no cuenta) y rechaza `<select>`. `comprobar` descodifica las entidades de `http-equiv` y de `href` antes de decidir y escapa los controles de sus propios mensajes.
- **CSP.** Calculada al final sobre el HTML definitivo, se inserta tras `<meta charset>`; `insertar` es idempotente y `comprobar` la recalcula y exige el hash de cada script en línea. Las páginas propias llevan `script-src 'none'`.
- **Salida atómica.** `construir` escribe en `.<salida>.construyendo-<pid>`, rechaza una salida que se solape con una entrada o lleve `..`, y renombra al final devolviendo la anterior a su sitio si algo falla. Probado con una plantilla rota a mitad de generación.
- **Fuentes.** JetBrains Mono sale de los TTF de la app, convertidos con `woff2_compress`, y se comprobó que conservan los mismos puntos de código y glifos. `magi-simbolos.woff2` (1,2 KB) lleva solo nueve glifos de DejaVu Sans Mono; el décimo, `⤴`, no está en DejaVu y se degradará con `texto_ascii` (S2). El lector de `cmap` es solo `std`, con tope total de trabajo.
- **Capturas (S2).** El test es síncrono (`App::de_prueba` crea su runtime), con `TZ=UTC` fijado una vez, `Tema::respaldo()` (nunca `tema::cargar`), entorno aislado y el inventario de la demo importado antes de crear la `App`; se exige que los hosts sean exactamente los tres de la demo. Los tiempos relativos se siembran a mitad del intervalo de redondeo («hace 2 min» con 150 s, «2 h 03 m» con 2 h 03 m 30 s): salen estables salvo una pausa de ~30 s entre sembrar y pintar. La generación valida todo (nombres de la semilla, datos del equipo, cobertura) **antes** de escribir. El adaptador falla cerrado ante una celda de continuación con contenido o un modificador que no sabe decir.
- **Dos barreras para lo no escapado.** Solo dos cosas se insertan sin escapar en las páginas: los fragmentos de captura y el detalle de cada versión. Cada una pasa por su validador (`validar_fragmento`; `verificar_detalle`, con pila de etiquetas, atributos y `https://` comprobado con `comprobar::analizar_https`) al leerla y otra vez justo antes de marcarla como segura, en Rust. Ambos rechazan los caracteres invisibles que falsean la lectura (marcas de dirección, separadores de línea, U+061C y los de etiqueta U+E00xx).
- **CHANGELOG → HTML.** Un `## [` que empieza por cifra y no es un encabezado exacto es un error con su línea, no se ignora; la fecha se valida con calendario; un bloque de código sin cerrar al final del fichero falla (no se traga versiones). Los enlaces relativos se leen desde la carpeta del CHANGELOG (raíz para `vX.Y.Z`, `android/` para `android-vX.Y.Z`) y pasan a GitHub en la etiqueta; un `https://` solo queda si pasa el mismo análisis que `comprobar`. Un enlace sin texto lleva su destino como nombre.
- **Rejilla de capturas.** Una captura de 100 columnas a 9 px (el mínimo) ocupa 542 px: la rejilla pasa a dos columnas a partir de unos 1 144 px (mínimo de columna 34 rem) para que ninguna se corte entre 480 y 1 280 px. Por debajo, cada captura se desplaza dentro de su marco, no la página.
- **Peso de la portada** medido por una prueba (`la_portada_pesa_menos_de_un_megabyte_en_los_dos_idiomas`): suma el HTML y todo lo que carga, siguiendo `<link>`, `<img>` y los `url()` de las hojas (también las fuentes que una página quizá no use). 787 770 bytes en cada idioma frente a 1 000 000; el GIF de la demo son 536 370.
- **Filtro de `cambios-web`** (corregido en la revisión): usa `git diff -z` y `printf '%q'`, porque con nombres no ASCII el `case` no casaba y una PR que tocaba la web no la construía.

## 5. Funcionalidades del checklist completadas

**Generador y estructura**

- [x] Crate `crates/magi-web` en el workspace, biblioteca y binario `magi-web`, con `publish = false` y `dist = false`
- [x] `web/datos/plataformas.toml` con canal, estado, artefacto, orden e instrucciones de cada plataforma, validado al cargar
- [x] Textos en `web/textos/es.toml` y `web/textos/en.toml` con las mismas claves
- [x] Página 404 en castellano e inglés, con enlaces absolutos a `https://magi.erratia.com/`

**Datos de las releases**

- [x] `magi-web releases` lee las releases de la API de GitHub en cada construcción, paginadas, y las deja en un `releases.json` sin versionar
- [x] Por canal sale la última versión publicada por número: `vX.Y.Z` para escritorio y Termux y `android-vX.Y.Z` para la app, nunca un borrador ni una candidata
- [x] Lo que llega de la API se valida (etiqueta, artefacto con su nombre, tamaño, URL y `.sha256` de una línea con 64 hexadecimales y el nombre exacto) y cualquier dato raro para la construcción
- [x] Termux: si la última versión aún no tiene su `.deb`, sale el de la anterior que lo tiene, con su número
- [x] La huella del certificado del APK es una constante de `plataformas.toml` y se compara con la de las notas de la release de la app
- [x] Homebrew y AUR solo aparecen en descargas si `PUBLICAR_HOMEBREW` o `PUBLICAR_AUR` valen `true`

**Descargas**

- [x] Descargas en castellano e inglés con Linux x86_64 y ARM64, macOS Apple e Intel, Termux y la app, cada una con versión, estado, tamaño, enlace y SHA-256 completo
- [x] Bloque «Verificar» por canal con `sha256sum -c` y `gh attestation verify --repo HectorEspejo/magi`, y en la app también `apksigner verify --print-certs` con la huella
- [x] Instrucciones cortas de instalación por plataforma, con enlace a su capítulo del manual (`instalacion`, `termux` o `app-android`)
- [x] Las plataformas «en pruebas» y «beta» lo dicen en texto junto a la descarga y enlazan a «Qué se ha probado» del manual
- [x] App: instrucciones de Obtainium (URL del repositorio, etiquetas `android-v` y APK `magi-android-`) y, sin ninguna release, «aún sin publicar»
- [x] Enlace a todas las versiones en GitHub Releases
- [x] Descargas sin JavaScript, con todas las plataformas a la vista y en orden: Linux, macOS, Termux y app

**Diseño y accesibilidad**

- [x] Estética de la TUI: JetBrains Mono, bordes de caja y la paleta fija en oscuro, con su derivada en claro
- [x] Fuentes servidas desde la web (JetBrains Mono 2.305 en woff2 y el subconjunto de DejaVu Sans Mono), con sus licencias
- [x] Prueba de contraste de las dos paletas: texto a 4,5:1 o más y bordes a 3:1 o más

**Seguridad y privacidad**

- [x] CSP en un `<meta>` al principio de cada página, solo con recursos propios, y en el manual los scripts en línea por su hash, calculado al generar
- [x] `magi-web comprobar` revisa el sitio generado (enlaces internos, anclas, externos solo de la lista permitida y por `https://`, sin `style` ni scripts propios) y un fallo para el despliegue
- [x] Sin cookies ni analítica, con la nota de privacidad en el pie
- [x] Todo lo que llega de la API, de los `.sha256` o de los CHANGELOG se escapa en el HTML

**Publicación**

- [x] `web.yml` construye y despliega en GitHub Pages con `deploy-pages`, permisos `contents: read`, `pages: write` e `id-token: write`, y sin secretos
- [x] `web.yml` se dispara al terminar bien «Release», «Release Termux» y «Release Android», con un push a `main` que toque la web y a mano, y despliega solo desde `main` con concurrencia `pages`
- [x] `homepage` de `Cargo.toml` apunta a `https://magi.erratia.com` y los dos README enlazan la web

**Portada, capturas y versiones (S2)**

- [x] Portada en castellano e inglés: qué es MAGI, para quién, plataformas, sin telemetría, licencia y enlaces a descargas, manual, GitHub, seguridad y contribuir
- [x] Botón principal «Descargar» con la versión actual del escritorio
- [x] La animación del README va dentro de un `<details open>` que la oculta sin JavaScript
- [x] Seis capturas con título y descripción: Flota, sesión con paneles, archivos, túneles, deliberación MAGI y servicios
- [x] Prueba ignorada `capturas_web` que pinta cada vista con `App::de_prueba` y `TestBackend` a 100×30, con el tema fijo, y la exporta a HTML en `web/capturas/`
- [x] Las capturas usan solo el inventario de la demo (`assets/demo/inventario`), nunca la semilla de las pruebas
- [x] Colores por clases (sin atributos `style`), con la paleta fija y su fondo de terminal en los dos temas
- [x] Prueba normal `capturas_web` que falla si alguna captura no está al día y dice cómo regenerarla
- [x] Cobertura de glifos: todo carácter de las capturas está en JetBrains Mono o en el subconjunto de símbolos, y lo que no, se degrada con `texto_ascii`
- [x] Cada captura es una figura con su descripción, y el lector de pantalla no lee los bordes de caja
- [x] Versiones en castellano e inglés con el CHANGELOG del escritorio y el de la app, sacados de la etiqueta de su última versión publicada
- [x] Cada versión con su resumen en el idioma de la página, el detalle en castellano dentro de un `<details>` y el enlace a su release
- [x] Sin la sección «Sin publicar» ni los comentarios de plantilla, y con el HTML en bruto del CHANGELOG escapado
- [x] La portada pesa menos de 1 MB sin contar las descargas

Quedan sin marcar, por depender del manual (S3) o de la revisión de cierre: `construir` completo con el manual; las plantillas «también en el manual»; los enlaces internos relativos de todas las páginas; `scripts/generar-web.sh` completo; accesibilidad AA y cambio de idioma en todas las páginas (también en el manual); los jobs de CI con el manual de la rama; `applicationId`; la documentación; la revisión adversarial de cierre; los `GET` del bloque de endpoints, que se marcan al comprobar el sitio completo; y los cuatro pasos del desarrollador. «Versiones … el de la app» está comprobado con una release de ejemplo: hoy la app no tiene ninguna y la página dice «aún sin publicar».

## 6. Pendientes y bloqueos

- **Sprint 3** (manual, `applicationId` y documentación) y la **revisión adversarial de cierre** de `magi-web` y `web.yml`.
- **Pasos del desarrollador** (no los hace el agente): `erratia.com` verificado en GitHub y el CNAME `magi` → `hectorespejo.github.io`; Pages con origen «GitHub Actions», dominio `magi.erratia.com` y HTTPS (**antes de fusionar**); keystore sin «Cuatro de Tres», sus cuatro secretos y la huella en `web/datos/plataformas.toml`; primera publicación con el guion.
- **Restos de «4d3»/«Cuatro de Tres»** fuera de `docs/faseN/` y del maestro, sin tocar (decisión del 22 nov, para una PR de limpieza aparte): `LICENSE-MIT:3` y `LICENSE-APACHE:204` (copyright: decisión tuya), `docs/publicar.md:175`, `docs/publicar-termux.md:90`, `.github/workflows/publicar-aur.yml:123-124`, `packaging/aur/PKGBUILD:1`, `scripts/paquete-termux.sh:294,346`, `tests/termux_paquete.rs:436`, y ejemplos en `src/ui/ficha.rs`, `tests/redimensionado/vistas_b.rs`, `tests/almacen.rs`, `tests/sshconfig_ida_vuelta.rs` y `crates/magi-nucleo/src/modelo.rs:1062`. No se tocan `org.cuatrodetres.magi` ni `ProjectDirs` (`org.4d3.magi`), que son deliberados.
- **Observaciones de las revisiones de pista aceptadas** (bajas, sin riesgo): `leer_unicodes` expande rangos sin tope (la entrada es un fichero del repositorio); ninguna prueba automática lee `magi-simbolos.woff2` (no hay decodificador brotli sin dependencias: se comprueba a mano con fontTools, procedimiento en `web/README.md`); la suma SHA-256 de DejaVu contiene por casualidad la cadena `4d3` en `web/README.md` (un hash, `repositorio_publico` solo busca `4d3-org`); `datos::desde_texto` no exige que estén las seis plataformas (quitar una la hace desaparecer de la página sin avisar); una orden que pasa su plazo se mata sin sus descendientes; un `<iframe srcdoc>` esconde marcado en un atributo que `comprobar` no mira (la CSP lo bloquea); un `.sha256` de entre 1 y 4 KiB lo corta el tope de salida y no el de `curl`.
- **Observaciones sobre `web.yml`:** una ejecución disparada por una release que falló entra igual en el grupo de concurrencia `pages` y puede desplazar a una pendiente (rara; es el patrón de la plantilla de GitHub); el job `web` de la PR no pasa `PUBLICAR_HOMEBREW` ni `PUBLICAR_AUR` (si se activan, sus bloques se comprueban por primera vez en el despliegue, que falla cerrado); `workflow_dispatch` solo funciona con el workflow ya en `main`, así que `web.yml` no se puede probar desde la rama.
- **Observaciones del S2 (aceptadas, para el analista):**
  - **El 404** usa URL absolutas (`https://magi.erratia.com/…`, como pide el informe): servido desde otro host (por ejemplo `127.0.0.1` al probar en local) la CSP `'self'` bloquea su hoja y su icono y se ve sin estilo; en el dominio real son del mismo origen. Con el dominio personalizado, `hectorespejo.github.io/magi/` redirige a él.
  - **Pintado de la TUI**: en Flota, la fila seleccionada pinta `NOMINAL` en verde sobre naranja (≈1,5:1) y un `⚠` naranja sobre naranja, invisible. La captura es fiel y no se tocó `src/ui`; conviene una fase de interfaz.
  - **Capturas a 320 px** se desplazan dentro de su marco y su `<pre>` va `aria-hidden`, así que no se le puede dar `tabindex`: el desplazamiento por teclado depende de que el navegador haga enfocable el contenedor (Chromium y Firefox lo hacen, Safari no). Para la pasada de accesibilidad.
  - **Enlaces externos del CHANGELOG**: uno a un dominio que no esté en la lista de `comprobar` (por ejemplo `rustsec.org` en una sección «Seguridad») para el despliegue hasta ampliar la lista, a propósito (el CHANGELOG de una etiqueta publicada es inmutable). Los de 1.6.1 y 0.2.0 pasan.
  - **CHANGELOG**: una valla de código con sangría de 1 a 3 espacios no esconde un `## [X.Y.Z]` que lleve dentro (GitHub sí); falla cerrado, con la línea. Una nota al pie `[^1]: …` se toma por definición de enlace.
  - **Capturas**: dependen del reloj (la TUI pinta el año si no es el actual, así que las fechas «2024» envejecen bien; los «hace 2 min» necesitan que la prueba no se retrase ~30 s) y se probaron solo en Linux; el job de macOS de CI compila `capturas_web` pero no la ejecuta. No se amplió la demo (tres hosts): Flota, Túneles y Servicios quedan con espacio vacío a 100×30.
  - **Lectura de ficheros** (`leer_acotado`): entre `symlink_metadata` y `open` hay una ventana teórica (TOCTOU); son ficheros de un checkout propio y `std` no ofrece `O_NOFOLLOW` sin dependencias.
- **Para S3:** el script `generar-web.sh` aún no prepara el manual (`CONTRATO-S3-MANUAL`; ya extrae los CHANGELOG); `preparar-manual` debe sustituir el `style="clear: both"` de mdBook, reescribir los enlaces del CHANGELOG a GitHub y escapar los `<host>` sueltos; la CSP del manual no lleva `connect-src` ni `worker-src` (comprobada la búsqueda en Chromium: carga `searchindex` como script).
- **CI.** Los jobs `cambios-web` y `web` no entran en la regla de `main` (filtro de rutas, como los de Termux): los ocho checks obligatorios no cambian. La PR toca `ci.yml` y `Cargo.*`, así que `cambios-termux` marcará `termux=true` y el job de ~75 min correrá igualmente.

## 7. Ejecución y pruebas

```
# Construir la web en local (necesita gh; --sin-red usa unos datos de ejemplo)
scripts/generar-web.sh [--sin-red] [--con-manual-de-la-rama]   # hasta el S3 el manual no se prepara
cargo run -p magi-web -- construir --web web --releases crates/magi-web/tests/fijos/releases/sin-red.json --animacion assets/demo/magi.gif --salida target/web/sitio
cargo run -p magi-web -- etiqueta --releases target/web/releases.json --canal escritorio   # v1.6.1
cargo run -p magi-web -- comprobar target/web/sitio            # hoy solo se queja de los enlaces al manual (S3)
scripts/generar-capturas.sh                                    # regenera web/capturas/ (tras cambiar una vista capturada)

# Pruebas
TZ=UTC cargo test -p magi-web --locked
TZ=UTC cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo deny check
actionlint
shellcheck scripts/*.sh crates/magi-nucleo/scripts/*.sh tests/fijos/terminal/tmux packaging/aur/generar.sh
dist plan --tag v1.6.1      # sin cambios: solo magi-ssh
```
Resultado del Sprint 1: `TZ=UTC cargo test --workspace --locked`: 105 binarios, **3 671 pruebas en verde**, 0 fallos y 7 ignoradas (las de siempre). `cargo test -p magi-web`: unas 350 pruebas. `clippy --workspace --all-targets -D warnings`, `fmt`, `cargo deny check`, `actionlint`, `shellcheck` y `dist plan --tag v1.6.1` (solo `magi-ssh`; sin etiqueta no menciona `magi-web`) limpios; `repositorio_publico` pasa con las fuentes, los fixtures y los README nuevos.
Resultado del Sprint 2: `TZ=UTC cargo test --workspace --locked`: 109 binarios, **3 791 pruebas en verde**, 0 fallos y 8 ignoradas (las 7 de siempre y `generar_capturas`, que solo lanza el script). `cargo test -p magi-web`: 469 pruebas en verde y 1 ignorada (la que regenera el manual de ejemplo de la CSP). `clippy --workspace --all-targets -D warnings`, `fmt --check`, `cargo deny check`, `actionlint`, `shellcheck` y `dist plan --tag v1.6.1` (solo `magi-ssh`; sin etiqueta no menciona `magi-web`) limpios. `scripts/generar-capturas.sh` regenera las seis capturas sin cambiar nada (son reproducibles) y `scripts/generar-web.sh --sin-red` llega hasta `comprobar`, que solo se queja de los enlaces al manual (S3). Con la app publicada (release de ejemplo) y los dos CHANGELOG reales, `comprobar` tampoco encuentra nada más que el manual.
