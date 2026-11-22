# MAGI - Checklist Fase 9c: Web de MAGI

## Generador y estructura
- [ ] Crate `crates/magi-web` en el workspace, biblioteca y binario `magi-web`, con `publish = false` y `dist = false`
  - AC: Dado el workspace con `magi-web`, cuando se ejecuta `dist plan`, entonces salen los mismos artefactos que sin él
- [ ] `magi-web construir` genera el sitio completo en una carpeta a partir de `web/`, los datos de las releases y el manual preparado
  - AC: Dados unos datos de ejemplo, cuando se construye, entonces existen la portada, descargas, versiones y 404 en los dos idiomas, y el manual
- [ ] Plantillas `minijinja` en `web/plantillas/` con una cabecera y un pie comunes, también en el manual
- [ ] `web/datos/plataformas.toml` con canal, estado, artefacto, orden e instrucciones de cada plataforma, validado al cargar
  - AC: Dado un estado que no es `estable`, `en_pruebas` ni `beta`, cuando se construye, entonces falla con el id de la plataforma
- [ ] Textos en `web/textos/es.toml` y `web/textos/en.toml` con las mismas claves
  - AC: Dada una clave que solo está en uno de los dos, cuando se construye, entonces falla diciendo cuál y en qué fichero
- [ ] Enlaces internos relativos: la web funciona igual en `magi.erratia.com` y en `hectorespejo.github.io/magi/`
- [ ] Página 404 en castellano e inglés, con enlaces absolutos a `https://magi.erratia.com/`
- [ ] `scripts/generar-web.sh` construye la web en local en `target/web/`, con mdBook fijado y los datos de la API o los de ejemplo (`--sin-red`)

## Datos de las releases
- [ ] `magi-web releases` lee las releases de la API de GitHub en cada construcción, paginadas, y las deja en un `releases.json` sin versionar
- [ ] Por canal sale la última versión publicada por número: `vX.Y.Z` para escritorio y Termux y `android-vX.Y.Z` para la app, nunca un borrador ni una candidata
  - AC: Dadas `v1.6.1`, `v1.6.2-rc.1` y `android-v0.2.0`, cuando se construye, entonces descargas enseña el escritorio 1.6.1 y la app 0.2.0
- [ ] Lo que llega de la API se valida (etiqueta, artefacto con su nombre, tamaño, URL y `.sha256` de una línea con 64 hexadecimales y el nombre exacto) y cualquier dato raro para la construcción
  - AC: Dado un `.sha256` con otro nombre de fichero, cuando se construye, entonces falla y la web publicada sigue siendo la anterior
- [ ] Termux: si la última versión aún no tiene su `.deb`, sale el de la anterior que lo tiene, con su número
- [ ] La huella del certificado del APK es una constante de `plataformas.toml` y se compara con la de las notas de la release de la app
  - AC: Dada una release de la app cuya huella no coincide con la constante, cuando se construye, entonces falla enseñando las dos
- [ ] Homebrew y AUR solo aparecen en descargas si `PUBLICAR_HOMEBREW` o `PUBLICAR_AUR` valen `true`

## Descargas
- [ ] Descargas en castellano e inglés con Linux x86_64 y ARM64, macOS Apple e Intel, Termux y la app, cada una con versión, estado, tamaño, enlace y SHA-256 completo
- [ ] Bloque «Verificar» por canal con `sha256sum -c` y `gh attestation verify --repo HectorEspejo/magi`, y en la app también `apksigner verify --print-certs` con la huella
- [ ] Instrucciones cortas de instalación por plataforma, con enlace a su capítulo del manual (`instalacion`, `termux` o `app-android`)
- [ ] Las plataformas «en pruebas» y «beta» lo dicen en texto junto a la descarga y enlazan a «Qué se ha probado» del manual
- [ ] App: instrucciones de Obtainium (URL del repositorio, etiquetas `android-v` y APK `magi-android-`) y, sin ninguna release, «aún sin publicar»
- [ ] Enlace a todas las versiones en GitHub Releases
- [ ] Descargas sin JavaScript, con todas las plataformas a la vista y en orden: Linux, macOS, Termux y app

## Portada
- [ ] Portada en castellano e inglés: qué es MAGI, para quién, plataformas, sin telemetría, licencia y enlaces a descargas, manual, GitHub, seguridad y contribuir
- [ ] Botón principal «Descargar» con la versión actual del escritorio
- [ ] La animación del README va dentro de un `<details open>` que la oculta sin JavaScript
  - AC: Dado `prefers-reduced-motion: reduce`, cuando se abre la portada, entonces no se ve la animación y en su lugar está la captura de Flota
- [ ] Seis capturas con título y descripción: Flota, sesión con paneles, archivos, túneles, deliberación MAGI y servicios

## Capturas
- [ ] Prueba ignorada `capturas_web` que pinta cada vista con `App::de_prueba` y `TestBackend` a 100×30, con el tema fijo, y la exporta a HTML en `web/capturas/`
  - AC: Dado `scripts/generar-capturas.sh`, cuando se ejecuta, entonces queda un fichero por vista y `capturas.css`
- [ ] Las capturas usan solo el inventario de la demo (`assets/demo/inventario`), nunca la semilla de las pruebas
  - AC: Dado un nombre de host de la semilla en una captura, cuando se generan o se comprueban, entonces la prueba falla
- [ ] Colores por clases (sin atributos `style`), con la paleta fija y su fondo de terminal en los dos temas
- [ ] Prueba normal `capturas_web` que falla si alguna captura no está al día y dice cómo regenerarla
- [ ] Cobertura de glifos: todo carácter de las capturas está en JetBrains Mono o en el subconjunto de símbolos, y lo que no, se degrada con `texto_ascii`
- [ ] Cada captura es una figura con su descripción, y el lector de pantalla no lee los bordes de caja

## Versiones
- [ ] Versiones en castellano e inglés con el CHANGELOG del escritorio y el de la app, sacados de la etiqueta de su última versión publicada
- [ ] Cada versión con su resumen en el idioma de la página, el detalle en castellano dentro de un `<details>` y el enlace a su release
- [ ] Sin la sección «Sin publicar» ni los comentarios de plantilla, y con el HTML en bruto del CHANGELOG escapado

## Manual
- [ ] Manual con mdBook 0.5, fijado por versión y SHA-256, desde `docs/manual/` de la etiqueta de la última versión publicada del escritorio
- [ ] `SUMMARY.md` sale del índice de `docs/manual/README.md`, con los capítulos y `estado.md`
  - AC: Dado un `.md` del manual que no está en el índice, cuando se construye, entonces falla con su nombre
- [ ] Enlaces que salen del manual a GitHub en esa etiqueta (`blob` para ficheros, `tree` para carpetas), y los de los README, a la portada
- [ ] Anclas iguales que en GitHub: cada título lleva su `{#id}` con el algoritmo de `repositorio_publico`, que pasa a `magi-web`
  - AC: Dado un enlace con ancla que funciona en GitHub, cuando se sigue en la web, entonces lleva al mismo título
- [ ] Tema de mdBook con la estética de la web (paletas, fuentes, cabecera y pie comunes), claro u oscuro según el sistema y sin selector de temas
- [ ] Búsqueda de mdBook activa, ejecución de código desactivada y nada cargado de fuera
- [ ] Aviso «Solo en castellano · Spanish only» en el manual y «Manual (Spanish)» en el menú inglés

## Diseño y accesibilidad
- [ ] Estética de la TUI: JetBrains Mono, bordes de caja y la paleta fija en oscuro, con su derivada en claro
- [ ] Fuentes servidas desde la web (JetBrains Mono 2.305 en woff2 y el subconjunto de DejaVu Sans Mono), con sus licencias
- [ ] Prueba de contraste de las dos paletas: texto a 4,5:1 o más y bordes a 3:1 o más
- [ ] Accesibilidad AA: `lang` en cada página, enlace para saltar al contenido, foco visible, todo con teclado y sin desplazamiento horizontal a 320 px
- [ ] Cambio de idioma en cada página, a su equivalente, con `hreflang`
- [ ] La portada pesa menos de 1 MB sin contar las descargas

## Seguridad y privacidad
- [ ] CSP en un `<meta>` al principio de cada página, solo con recursos propios, y en el manual los scripts en línea por su hash, calculado al generar
  - AC: Dado un script en línea nuevo en el tema del manual, cuando se construye, entonces su hash entra en la CSP de esas páginas y no hace falta `'unsafe-inline'`
- [ ] `magi-web comprobar` revisa el sitio generado (enlaces internos, anclas, externos solo de la lista permitida y por `https://`, sin `style` ni scripts propios) y un fallo para el despliegue
- [ ] Sin cookies ni analítica, con la nota de privacidad en el pie
- [ ] Todo lo que llega de la API, de los `.sha256` o de los CHANGELOG se escapa en el HTML

## Publicación
- [ ] `web.yml` construye y despliega en GitHub Pages con `deploy-pages`, permisos `contents: read`, `pages: write` e `id-token: write`, y sin secretos
- [ ] `web.yml` se dispara al terminar bien «Release», «Release Termux» y «Release Android», con un push a `main` que toque la web y a mano, y despliega solo desde `main` con concurrencia `pages`
  - AC: Dada una «Release Termux» que falla, cuando termina, entonces la web no se regenera
- [ ] Jobs `cambios-web` y `web` en `ci.yml` (no obligatorios) que construyen y comprueban la web, también con el manual de la rama, en cada PR que la toque
- [ ] `homepage` de `Cargo.toml` apunta a `https://magi.erratia.com` y los dos README enlazan la web

## App Android
- [ ] `applicationId` `io.github.hectorespejo.magi`, sin cambiar el namespace ni el paquete Kotlin, y `android-release.yml` comprueba el nuevo
- [ ] `docs/publicar-android.md` al día: `-dname` del keystore sin «Cuatro de Tres», el `applicationId` nuevo y dónde va la huella del certificado

## Documentación
- [ ] `docs/publicar-web.md`: DNS, dominio verificado, ajustes de Pages, primera publicación, cómo relanzarla y cómo retirar la web sin dejar el subdominio expuesto
- [ ] `web/README.md`: cómo se genera la web, de dónde sale cada cosa y cómo se rehace el subconjunto de símbolos
- [ ] `CLAUDE.md`, `docs/README.md` y el capítulo `instalacion` del manual al día con la web
- [ ] Líneas en «Sin publicar» de `CHANGELOG.md` y `android/CHANGELOG.md`

## Cierre
- [ ] Revisión adversarial de cierre de `magi-web` y de `web.yml` (datos de fuera, escape, CSP y despliegue), anotada en el informe de implementación

## Pasos del desarrollador
- [ ] `erratia.com` verificado en GitHub y el CNAME `magi` → `hectorespejo.github.io` en su DNS
- [ ] Pages con origen «GitHub Actions», dominio `magi.erratia.com` y HTTPS forzado
- [ ] Keystore sin «Cuatro de Tres», con sus cuatro secretos y su huella en `plataformas.toml`
- [ ] Primera publicación de la web comprobada con el guion en el móvil y en el escritorio

## Rutas y llamadas a la API
- [ ] GET / y GET /en/ (portada)
- [ ] GET /descargas/ y GET /en/download/
- [ ] GET /versiones/ y GET /en/releases/
- [ ] GET /manual/ y sus capítulos
- [ ] GET /404.html
- [ ] GET https://api.github.com/repos/HectorEspejo/magi/releases (en la construcción, paginado)
- [ ] GET de los `.sha256` publicados (en la construcción)

---

**Progreso Fase 9c:** 0 / 73 funcionalidades

**Total MAGI (Fases 1-17, 9b y 9c):** 1432 / 1520 funcionalidades
