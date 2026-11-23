# MAGI - Fase 9c: Web de MAGI

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 22 de noviembre de 2026
**Cliente:** ninguno (proyecto personal de Hector Espejo)

> Base: especificada a petición de Hector sobre una base sin confirmar. La 9b sigue abierta (55/58): la 1.6.1 se prepara en la rama `release-1.6.1`, con Homebrew y AUR apagados (su desviación 19), y su informe al día no está procesado. La F17 también sigue abierta (66/71). La 9c empieza cuando la 1.6.1 esté publicada, en una rama nueva desde `main`.

---

## 1. Visión General

La 9c hace **la web de MAGI**, en `https://magi.erratia.com`, servida por GitHub Pages desde el mismo repositorio. Tiene cuatro partes:

- una **portada** que presenta MAGI con su animación y seis capturas reales de la TUI;
- **descargas**, con la última versión de cada canal (escritorio, Termux y app), su SHA-256 y cómo verificarla;
- **versiones**, con los dos CHANGELOG;
- el **manual** de `docs/manual/`, publicado con mdBook, con índice y búsqueda.

Todo es estático y va en castellano y en inglés (el manual, solo en castellano). No hay JavaScript propio, ni analítica, ni nada cargado de fuera. La web se regenera sola cuando termina una release y cuando cambia en `main`.

La fase también cambia el `applicationId` de la app a `io.github.hectorespejo.magi` antes de su primera release, que espera a esta fase (Hector, 12b). La web enlazará el APK, y una vez publicado el identificador ya no puede cambiar sin que todos reinstalen.

Se llama 9c porque sigue la línea de publicación de la 9 y la 9b; así el roadmap no se renumera, y Accesos y certificados sigue siendo la F18 (D439).

**Objetivos principales**

1. Una web propia en `magi.erratia.com`, con HTTPS y el dominio verificado, desplegada desde `main` con GitHub Actions y sin secretos.
2. Descargas que llevan al fichero justo de cada plataforma, con su SHA-256, su atestación y, en la app, la huella del certificado. Salen en cada construcción de las releases publicadas.
3. El manual en la web, generado desde la etiqueta de la última versión publicada, con los mismos enlaces y anclas que en GitHub.
4. Una portada con capturas reales a color, generadas desde el código de pintado y siempre al día.
5. La estética de la TUI, accesible (WCAG 2.2 AA) y ligera, en claro y en oscuro.
6. La app, lista para su primera release con un identificador propio.

**Contexto.** MAGI es un proyecto personal de Hector (T151): no tiene cliente, y lo que antes decía «4d3» en la documentación era un error. La 9b publica la primera versión, la 1.6.1, en GitHub Releases. Homebrew y AUR quedan apagados con variables del repositorio, porque AUR no admite cuentas nuevas. La web es la puerta para quien llega por el repositorio compartido; el README sigue siendo la presentación en GitHub y enlaza aquí.

**Respuestas de Hector (22 nov 2026).** Eligió la propuesta «Sitio completo» de cuatro (portada, descargas, versiones y manual) y, para la especificación: 1b con `magi.erratia.com`, y todas las recomendadas (2a a 11a) salvo la 12, donde eligió la b (el `applicationId` cambia en esta fase y la app no se publica hasta entonces).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Pieza | Tecnología | Notas |
|---|---|---|
| Alojamiento | GitHub Pages, desplegado con Actions | `actions/configure-pages`, `actions/upload-pages-artifact` y `actions/deploy-pages` en su versión mayor vigente, como el resto de workflows; OIDC, sin secretos |
| Dominio | `magi.erratia.com` | CNAME a `hectorespejo.github.io` en el DNS de erratia.com; `erratia.com` verificado en GitHub; HTTPS forzado |
| Generador | crate `magi-web` (Rust, en el workspace) | Biblioteca y binario; `minijinja` (plantillas), `pulldown-cmark` (CHANGELOG), `serde`, `serde_json` y `toml`; `publish = false` y `dist = false` |
| Manual | mdBook 0.5.x | La última de esa serie al implementar, fijada por versión y SHA-256 como las demás herramientas descargadas; tema propio en `web/manual/theme/` |
| Capturas | prueba `capturas_web` de `magi-ssh` | `App::de_prueba` y el `TestBackend` de ratatui; exporta HTML |
| Datos de versiones | API REST de GitHub (`gh api`) | En cada construcción, con el `GITHUB_TOKEN` de solo lectura |
| Fuentes | JetBrains Mono 2.305 (OFL-1.1) y un subconjunto de DejaVu Sans Mono | woff2 servidos desde la propia web; la 2.305 es la de la app |
| Páginas | HTML5 y CSS propios | Sin JavaScript propio; el del manual es el de mdBook |

La fase no añade dependencias al escritorio ni a la app: `magi-web` es un crate aparte que no se distribuye, y lo único que cambia en Android es el `applicationId`.

### Estructura de carpetas (novedades)

```
magi/
├── web/                                  ← lo propio de la web (sale de main)
│   ├── README.md                         ← cómo se genera y de dónde sale cada cosa
│   ├── plantillas/                       ← minijinja: base, portada, descargas, versiones, 404,
│   │                                       y los parciales de cabecera y pie
│   ├── estilos/web.css                   ← paletas, tipografía, cajas
│   ├── fuentes/                          ← JetBrainsMono-Regular.woff2 · -Bold.woff2 · magi-simbolos.woff2
│   │                                       OFL.txt · LICENSE-DejaVu.txt
│   ├── textos/es.toml · en.toml          ← textos de las páginas, con las mismas claves
│   ├── datos/plataformas.toml            ← plataformas, estado, artefactos y huella del APK
│   ├── capturas/                         ← <vista>.html y capturas.css (generadas y versionadas)
│   └── manual/
│       ├── book.toml                     ← configuración de mdBook
│       └── theme/                        ← head.hbs, header.hbs, css/variables.css, fonts/fonts.css
├── crates/magi-web/
│   ├── Cargo.toml                        ← publish = false · [package.metadata.dist] dist = false
│   ├── src/main.rs                       ← magi-web construir | comprobar | preparar-manual
│   ├── src/lib.rs                        ← datos, releases, descargas, versiones, manual, anclas, csp, enlaces
│   └── tests/
├── tests/capturas_web.rs                 ← genera (ignorada) y comprueba (normal) las capturas
├── scripts/generar-capturas.sh           ← regenera web/capturas/
├── scripts/generar-web.sh                ← construye la web en local en target/web/
├── docs/publicar-web.md                  ← DNS, dominio, Pages, primera publicación, retirarla
└── .github/workflows/web.yml             ← construir y desplegar
```

### Diagrama de arquitectura

```
  REPOSITORIO HectorEspejo/magi
 ┌────────────────────────────────────────────────────────────────────────────┐
 │  etiqueta vX.Y.Z ──────────▶ «Release» (dist) ─────────┐                   │
 │                  └─────────▶ «Release Termux» ─────────┤  al terminar bien │
 │  etiqueta android-vX.Y.Z ──▶ «Release Android» ────────┤                   │
 │  push a main en web/, crates/magi-web/ o assets/demo/ ─┤                   │
 │  a mano (workflow_dispatch) ───────────────────────────┤                   │
 │                                                        ▼                   │
 │  ┌───────────────────────────── web.yml ──────────────────────────────┐    │
 │  │ construir                                                          │    │
 │  │   1 gh api …/releases ─────────▶ releases.json (sin versionar)     │    │
 │  │   2 los .sha256 publicados                                         │    │
 │  │   3 docs/manual y los CHANGELOG de cada etiqueta (git show)        │    │
 │  │   4 magi-web preparar-manual ─▶ mdBook ─▶ manual/                  │    │
 │  │   5 magi-web construir ─▶ portada, descargas, versiones, 404       │    │
 │  │   6 magi-web comprobar: enlaces, anclas, externos, CSP, huella     │    │
 │  │ desplegar: upload-pages-artifact ─▶ deploy-pages                   │    │
 │  └─────────────────────────────────┬──────────────────────────────────┘    │
 │  web/ (plantillas, textos, datos,  │                                       │
 │  capturas, fuentes y tema)         │                                       │
 └────────────────────────────────────┼───────────────────────────────────────┘
                                      ▼
                            ┌─────────────────────┐   CNAME en el DNS   ┌──────────────┐
                            │ GitHub Pages        │◀── de erratia.com ──│ navegador    │
                            │ (último despliegue) │     HTTPS           │ de quien     │
                            └─────────────────────┘                     │ visita       │
                                                                        └──────┬───────┘
                       los enlaces de descarga van a GitHub Releases,          │
                       no pasan por Pages ─────────────────────────────────────┘
                       github.com/HectorEspejo/magi/releases/download/…

  CAPTURAS (fuera del despliegue, en el repositorio)
  pintado de la TUI: App::de_prueba + TestBackend, tema fijo, inventario de la demo
        │ prueba ignorada capturas_web (scripts/generar-capturas.sh)
        ▼
  web/capturas/*.html + capturas.css ──(versionadas)──▶ magi-web construir ──▶ portada
        ▲
        └── prueba normal capturas_web, en tests-linux: falla si no están al día
```

---

## 3. Modelo de Datos

La 9c no añade tablas SQL, migraciones, protocolo ni `formato`: la web no tiene base de datos. Sus datos son ficheros de `web/` y lo que devuelve la API de GitHub en cada construcción. Las «tablas» de abajo son los registros que lee `magi-web`.

### Diagrama E-R

```
 ┌────────────────┐ 1          N ┌────────────────────┐
 │ CANAL          │──────────────│ PLATAFORMA         │  web/datos/plataformas.toml
 │ escritorio ·   │              │ id · estado ·      │
 │ termux · app   │              │ artefacto · orden  │
 └───────┬────────┘              └─────────┬──────────┘
         │ 1                               │ 1
         │ N                               │ 0..1 por versión
 ┌───────┴────────┐ 1          N ┌─────────┴──────────┐
 │ VERSION        │──────────────│ ARTEFACTO          │  API de GitHub (releases.json)
 │ etiqueta ·     │              │ nombre · tamaño ·  │  y los .sha256 publicados
 │ fecha · url    │              │ url · sha256       │
 └───────┬────────┘              └────────────────────┘
         │ 1
         │ 0..1
 ┌───────┴──────────────┐
 │ ENTRADA_CHANGELOG    │  CHANGELOG.md y android/CHANGELOG.md de la etiqueta
 │ número · fecha ·     │
 │ resumen es/en ·      │
 │ detalle              │
 └──────────────────────┘

 ┌────────────────┐ N          1 ┌────────────────────┐        ┌────────────────────┐
 │ PAGINA         │──────────────│ PLANTILLA          │  usa   │ TEXTO              │
 │ idioma · ruta  │              │ web/plantillas/    │◀───────│ clave · es · en    │
 └───────┬────────┘              └────────────────────┘        │ web/textos/*.toml  │
         │ portada: 1                                          └────────────────────┘
         │ N
 ┌───────┴────────┐
 │ CAPTURA        │  web/capturas/
 │ id · vista ·   │
 │ 100×30         │
 └────────────────┘
```

### Registros

**CANAL** (fijo en `magi-web`)

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | texto | `escritorio`, `termux` o `app` |
| `patron_etiqueta` | expresión | `^v\d+\.\d+\.\d+$` para escritorio y Termux (comparten release); `^android-v\d+\.\d+\.\d+$` para la app |
| `changelog` | ruta | `CHANGELOG.md` (escritorio y Termux) o `android/CHANGELOG.md` (app) |
| `capitulo` | texto | Capítulo del manual con su instalación: `instalacion`, `termux` o `app-android` |

**PLATAFORMA** (`[[plataforma]]` en `web/datos/plataformas.toml`)

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | texto | `linux-x86_64`, `linux-arm64`, `macos-arm64`, `macos-x86_64`, `termux-arm64` o `android-arm64` |
| `canal` | texto | Uno de los tres canales |
| `estado` | texto | `estable`, `en_pruebas` o `beta`; los iniciales, los de `docs/manual/estado.md` de la 1.6.1 |
| `artefacto` | texto | Nombre del fichero, con `{version}` si lo lleva: `magi-ssh-x86_64-unknown-linux-gnu.tar.xz`, `magi-ssh_{version}_aarch64.deb`, `magi-android-v{version}.apk`… |
| `orden` | entero | Orden en la página: Linux, macOS, Termux y app |
| `instrucciones` | clave | Clave de `textos` con su instalación corta |

**`[app]`** en `plataformas.toml`

| Campo | Tipo | Descripción |
|---|---|---|
| `huella_certificado` | texto | SHA-256 del certificado de firma en mayúsculas con `:` (como en las notas de la release); vacía hasta que el desarrollador cree el keystore |
| `obtainium_etiquetas` | texto | `android-v` |
| `obtainium_apk` | texto | `magi-android-` |

**VERSION** (de la API, en `releases.json`)

| Campo | Tipo | Descripción |
|---|---|---|
| `canal` | texto | Según la etiqueta |
| `etiqueta` | texto | `v1.6.1`, `android-v0.2.0`… |
| `numero` | texto | `1.6.1`, `0.2.0`… |
| `fecha` | fecha | `published_at` |
| `url` | URL | Página de la release en GitHub |
| `notas` | texto | Cuerpo de la release; solo se lee para la huella de la app |
| `borrador` · `prerelease` | booleanos | Lo que dice la API; ninguno de los dos sale en descargas |

**ARTEFACTO** (de la API y de los `.sha256`)

| Campo | Tipo | Descripción |
|---|---|---|
| `version` | ref. VERSION | |
| `plataforma` | ref. PLATAFORMA | Por el nombre del fichero |
| `nombre` | texto | Nombre publicado |
| `tamano` | entero | Bytes, según la API |
| `url` | URL | `browser_download_url`, siempre en `github.com/HectorEspejo/magi/releases/download/` |
| `sha256` | texto | 64 hexadecimales, sacados del `.sha256` publicado junto al artefacto |

**ENTRADA_CHANGELOG** (de la etiqueta)

| Campo | Tipo | Descripción |
|---|---|---|
| `canal` | texto | Escritorio o app |
| `numero` · `fecha` | texto · fecha | Del encabezado `## [X.Y.Z] - AAAA-MM-DD` |
| `resumen_es` · `resumen_en` | texto | Los párrafos `**Resumen.**` y `**Summary.**` |
| `detalle` | HTML | El resto, en castellano, con el HTML en bruto escapado |

**CAPTURA** (`web/capturas/`)

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | texto | `flota`, `sesion-paneles`, `archivos`, `tuneles`, `deliberacion` o `servicios` |
| `fichero` | ruta | `web/capturas/<id>.html` (y las clases en `capturas.css`) |
| `columnas` · `filas` | enteros | 100 × 30 |
| `titulo` · `descripcion` | claves | De `textos`; la descripción es el texto alternativo |

**TEXTO** (`web/textos/es.toml` y `en.toml`): `clave` → texto. Los dos ficheros tienen las mismas claves.

**PAGINA**: `ruta_es`, `ruta_en`, `plantilla` y `titulo` (rutas en §5.1).

### Relaciones

- Un CANAL tiene varias PLATAFORMAS; escritorio y Termux comparten la release `vX.Y.Z` y su CHANGELOG.
- Una VERSION tiene, como mucho, un ARTEFACTO por plataforma de su canal y, como mucho, una ENTRADA_CHANGELOG.
- La portada tiene las seis CAPTURAS; toda PAGINA sale de una PLANTILLA con los TEXTOS de su idioma.
- Fuera del modelo quedan `releases.json` (en la carpeta de trabajo de la construcción) y el sitio generado (el artefacto de Pages); ninguno se versiona.

### Diagramas de estados

**Construcción de la web**

```
  (disparo) ──▶ CONSTRUYENDO ──▶ COMPROBANDO ──▶ LISTA ──▶ DESPLEGADA
                     │                │              │
                     ▼                ▼              ▼
                  FALLIDA ◀───────────┴──────────────┘
                     │  (API caída, dato raro, huella distinta, capítulo fuera
                     │   del índice, enlace o ancla rotos, CSP incompleta)
                     ▼
                la web publicada sigue siendo la anterior

  Inválidas: desplegar sin pasar por COMPROBANDO; desplegar desde una rama que no sea main
  (en una PR solo se construye y se comprueba); desplegar tras una release que terminó mal;
  dos despliegues a la vez (concurrencia «pages»).
```

**Versión de un canal en la web**

```
  BORRADOR o CANDIDATA (-rc.N) ── nunca sale en descargas
  PUBLICADA (la última del canal) ──▶ sale en descargas y en versiones
        │ (se publica otra más nueva)
        ▼
  ANTERIOR ──▶ solo en versiones y en GitHub Releases

  Termux: si la última PUBLICADA aún no tiene su .deb, sale el de la ANTERIOR con su número.
  Inválidas: una candidata en descargas; una ANTERIOR en descargas habiendo una PUBLICADA más
  nueva; una release borrada que siga saliendo (la siguiente construcción la quita).
```

**Estado de una plataforma**

```
  BETA ───────────┐
                  ├──(PR que cambia plataformas.toml y docs/manual/estado.md)──▶ ESTABLE
  EN PRUEBAS ─────┘                                                                │
        ▲                                                                          │
        └──────────────(regresión: PR igual, en sentido contrario)─────────────────┘

  Inválidas: cambiar el estado en plataformas.toml sin tocar estado.md, o al revés.
```

**Dominio**

```
  SIN CONFIGURAR ──▶ VERIFICADO (TXT en erratia.com) ──▶ EN PAGES (dominio puesto)
      ──▶ APUNTADO (CNAME magi ─▶ hectorespejo.github.io) ──▶ CERTIFICADO EMITIDO
      ──▶ HTTPS FORZADO

  Retirada: primero se quita el CNAME del DNS y después el dominio de Pages.
  Inválidas: poner el dominio en Pages sin verificarlo; forzar HTTPS antes del certificado;
  quitar el dominio de Pages dejando el CNAME (el subdominio quedaría expuesto, R197).
```

**Identificador de la app**

```
  org.cuatrodetres.magi (sin publicar) ──9c──▶ io.github.hectorespejo.magi (sin publicar)
      ──android-v0.2.0──▶ PUBLICADO (fijo para siempre)

  Inválidas: publicar con org.cuatrodetres.magi; cambiarlo después de publicar.
```

**Captura**

```
  AL DÍA ──(cambia una vista capturada)──▶ DESFASADA ──(la prueba falla en CI)──▶
      ──scripts/generar-capturas.sh──▶ REGENERADA (diff revisado) ──commit──▶ AL DÍA

  Inválidas: fusionar con una captura desfasada; editar una captura a mano.
```

---

## 4. Flujos de Trabajo

### 4.1 Construir y desplegar

```
 disparo: una release que terminó bien · push a main en web/, crates/magi-web/ o assets/demo/ · a mano
   │
   ▼
 gh api releases (paginado) ──falla──▶ FALLIDA: sigue la web anterior
   │
   ▼
 elegir por canal la última versión publicada ──no hay ninguna del escritorio──▶ FALLIDA
   │  (sin app publicada: su bloque dice «aún sin publicar» y se sigue)
   ▼
 bajar los .sha256 y validar cada artefacto ──dato raro──▶ FALLIDA (dice qué y dónde)
   │
   ▼
 ¿huella de la release de la app = la de plataformas.toml? ──no, o falta──▶ FALLIDA (enseña las dos)
   │ sí (o no hay app publicada)
   ▼
 docs/manual/ y los CHANGELOG de cada etiqueta (git show) ──un .md fuera del índice──▶ FALLIDA
   │
   ▼
 preparar-manual: SUMMARY.md, enlaces de fuera y {#id} ─▶ mdBook ─▶ manual/
   │
   ▼
 construir: plantillas × (es, en), capturas, fuentes, cabecera y pie también en el manual, CSP
   │
   ▼
 comprobar: enlaces, anclas, externos permitidos, sin style ni script propio, CSP completa
   │ ──fallo──▶ FALLIDA
   ▼
 ¿main? ──no (PR)──▶ fin: construida y comprobada, sin desplegar
   │ sí
   ▼
 upload-pages-artifact ─▶ deploy-pages ─▶ DESPLEGADA en magi.erratia.com
```

### 4.2 Descargar y verificar

```
 portada ─▶ «Descargar 1.6.1» ─▶ descargas
   │
   ▼
 busca su plataforma (todas a la vista, en orden: Linux, macOS, Termux y app)
   ├─ «en pruebas» o «beta» ─▶ enlace a «Qué se ha probado» del manual
   ▼
 baja el artefacto y su .sha256 desde github.com (no desde la web)
   ▼
 sha256sum -c ──no coincide──▶ no lo instala; la página dice que avise (aviso privado, SECURITY.md)
   ▼
 gh attestation verify <fichero> --repo HectorEspejo/magi ──falla──▶ lo mismo
   ▼ en la app, además:
 apksigner verify --print-certs ─▶ ¿la huella es la de la web? ──no──▶ no lo instala y avisa
   ▼
 instala con su capítulo del manual (instalacion · termux · app-android)
```

### 4.3 Una versión nueva llega a la web

```
 desarrollador: git tag -a v1.6.2 && git push --tags
   ▼
 «Release» (dist) publica los tarballs ─▶ termina bien ─▶ web.yml (1.ª vuelta)
   │                                        la 1.6.2 sale en descargas; Termux sigue en 1.6.1
   ▼                                        porque aún no tiene su .deb (§7.1)
 «Release Termux» sube el .deb a la misma release ─▶ termina bien ─▶ web.yml (2.ª vuelta)
   ▼                                        Termux pasa a la 1.6.2
 el manual y las versiones del escritorio salen ya de la etiqueta v1.6.2

 Con android-vX.Y.Z pasa lo mismo, con «Release Android»; si falla una release, la web no se toca.
```

### 4.4 Cambiar una vista que sale en la portada

```
 PR que cambia, por ejemplo, la vista Flota
   ▼
 tests-linux: la prueba normal capturas_web pinta Flota y la compara con web/capturas/flota.html
   ├─ igual ──▶ nada
   └─ distinta ──▶ falla: «captura desfasada: ejecuta scripts/generar-capturas.sh»
        ▼
 el agente la regenera, revisa el diff (git diff web/capturas) y la sube en el mismo PR
        ▼
 al fusionar, el push a main en web/ regenera la web
```

### 4.5 Preparar el manual desde la etiqueta

```
 etiqueta de la última versión publicada del escritorio (p. ej. v1.6.1)
   ▼
 git show v1.6.1:docs/manual/<fichero> para cada .md ─▶ carpeta de trabajo src/
   ▼
 SUMMARY.md desde «## Capítulos» de README.md, más estado.md al final
   ├─ un .md que no está en el índice ──▶ FALLIDA («capítulo fuera del índice: x.md»)
   ▼
 cada título recibe su {#id} con el algoritmo de GitHub (el de repositorio_publico)
   ▼
 enlaces que salen de docs/manual/:
   ├─ ../../README.md y ../../README.en.md ──▶ la portada (es) y /en/
   ├─ un fichero ──▶ https://github.com/HectorEspejo/magi/blob/v1.6.1/<ruta>
   └─ una carpeta ──▶ https://github.com/HectorEspejo/magi/tree/v1.6.1/<ruta>
   ▼
 mdBook build con web/manual/book.toml y el tema propio ─▶ manual/
   ▼
 magi-web añade el pie común y la CSP a cada página del manual
```

### 4.6 Poner en marcha el dominio (desarrollador)

```
 1 GitHub › Settings › Pages › añadir el dominio erratia.com ─▶ GitHub da un TXT
 2 DNS de erratia.com: TXT _github-pages-challenge-HectorEspejo ─▶ «Verify»
   ├─ no verifica ──▶ esperar a que el DNS propague y repetir; sin verificar no se sigue
 3 repositorio › Settings › Pages: origen «GitHub Actions» y dominio magi.erratia.com
 4 DNS: CNAME magi ─▶ hectorespejo.github.io (si el DNS está en Cloudflare, sin proxy)
 5 esperar al certificado ─▶ «Enforce HTTPS»
 6 lanzar web.yml a mano ─▶ guion de la web (§11) en el móvil y en el escritorio
```

El blog de erratia.com no cambia: solo se añade el subdominio.

### 4.7 Diagrama de secuencia: construcción

```
 Actions (web.yml)        API de GitHub           repositorio (git)        Pages
       │ GET /repos/HectorEspejo/magi/releases?per_page=100 (y siguientes)    │
       │───────────────────────▶│                      │                     │
       │◀── releases (JSON) ────│                      │                     │
       │ GET <url>.sha256 (cada artefacto elegido)     │                     │
       │───────────────────────▶│ (302 al almacén de ficheros)               │
       │◀── «<hash>  <nombre>» ─│                      │                     │
       │ git show vX.Y.Z:docs/manual/… y los CHANGELOG │                     │
       │──────────────────────────────────────────────▶│                     │
       │◀───────────────────── ficheros de la etiqueta ─│                     │
       │ preparar-manual · mdBook · construir · comprobar                    │
       │ upload-pages-artifact (sitio) · deploy-pages (token OIDC)           │
       │────────────────────────────────────────────────────────────────────▶│
       │◀─────────────────────────────────── despliegue listo (page_url) ────│
```

### 4.8 Diagrama de secuencia: una descarga

```
 navegador            magi.erratia.com (Pages)        github.com             almacén de GitHub
     │ GET /descargas/          │                         │                         │
     │─────────────────────────▶│                         │                         │
     │◀── HTML y CSS (sin JS) ──│                         │                         │
     │ GET /HectorEspejo/magi/releases/download/v1.6.1/<fichero>                    │
     │───────────────────────────────────────────────────▶│                         │
     │◀────────────────────────────── 302 al almacén ─────│                         │
     │ GET (redirigido) ──────────────────────────────────────────────────────────▶│
     │◀─────────────────────────────────────────────────────────────── fichero ────│
     │ (lo mismo con el .sha256; la verificación es local: sha256sum, gh, apksigner)│
```

---

## 5. Rutas, CLI y Workflows

La web no tiene API propia ni formularios. Estas son sus rutas públicas, las llamadas que hace al construir y la CLI de su generador.

### 5.1 Rutas públicas

| Ruta (castellano) | Ruta (inglés) | Contenido |
|---|---|---|
| `/` | `/en/` | Portada |
| `/descargas/` | `/en/download/` | Descargas por canal y plataforma, con verificación |
| `/versiones/` | `/en/releases/` | CHANGELOG del escritorio y de la app |
| `/manual/` y `/manual/<capitulo>.html` | — (el menú inglés enlaza al de castellano) | Manual (mdBook), con búsqueda |
| `/404.html` | (la misma, en los dos idiomas) | No encontrada |

Cada página enlaza a su equivalente en el otro idioma (`hreflang`). No hay más rutas: ni `/api/*` ni formularios.

### 5.2 Llamadas en la construcción

| Llamada | Para qué | Notas |
|---|---|---|
| `GET https://api.github.com/repos/HectorEspejo/magi/releases?per_page=100` | Versiones y artefactos | Paginada; con el `GITHUB_TOKEN` (`contents: read`) |
| `GET <browser_download_url>.sha256` | La suma de cada artefacto elegido | Solo los `.sha256`; los artefactos no se bajan |
| `git show <etiqueta>:docs/manual/…`, `:CHANGELOG.md` y `:android/CHANGELOG.md` | Manual y versiones | El checkout trae las etiquetas (`fetch-depth: 0` o `git fetch --tags`) |

### 5.3 CLI de `magi-web`

| Orden | Qué hace |
|---|---|
| `magi-web construir --salida <dir> [--releases <json>] [--manual <dir>]` | Genera portada, descargas, versiones y 404 en los dos idiomas, copia capturas, fuentes y estilos, y pone cabecera, pie y CSP a todo, también al manual ya generado |
| `magi-web preparar-manual --etiqueta <vX.Y.Z> --salida <dir>` | Copia el manual de la etiqueta, escribe `SUMMARY.md`, fija las anclas y reescribe los enlaces de fuera |
| `magi-web comprobar <dir>` | Enlaces internos y anclas, externos solo de la lista permitida, sin atributos `style` ni scripts propios, CSP en cada página |
| `magi-web releases --salida <json>` | Lee la API y los `.sha256` y valida (lo usa `web.yml`; en local, `scripts/generar-web.sh`) |

`scripts/generar-web.sh` hace lo mismo que `web.yml` en local, en `target/web/`: baja mdBook fijado a `~/.cache/magi/` (comprobado por SHA-256) si no está, y con `--sin-red` usa unos `releases.json` y `.sha256` de ejemplo de `crates/magi-web/tests/fijos/`.

Los enlaces externos van siempre por `https://` y solo a la lista permitida de `comprobar`. Empieza con los que ya enlazan el manual, los CHANGELOG y los README de la 1.6.1: `github.com/HectorEspejo/magi` (repositorio, releases, issues y ficheros), `github.com/ImranR98/Obtainium`, `github.com/termux-user-repository`, `cli.github.com`, `termux.dev`, `keepachangelog.com` y `semver.org`, más `magi.erratia.com` en el 404. Un dominio nuevo entra en la lista en el mismo PR que lo enlaza; cualquier otro rompe `comprobar`.

### 5.4 Workflows

**`web.yml`** (nuevo)

| Elemento | Valor |
|---|---|
| Disparos | `workflow_run` de «Release», «Release Termux» y «Release Android» (`types: [completed]`, solo si `conclusion == 'success'`); `push` a `main` con rutas `web/**`, `crates/magi-web/**`, `assets/demo/**` y `.github/workflows/web.yml`; `workflow_dispatch` |
| Permisos | `contents: read`, `pages: write`, `id-token: write` |
| Concurrencia | grupo `pages`, sin cancelar el que está en curso |
| Jobs | `construir` (releases, manual, mdBook, construir, comprobar, `upload-pages-artifact`) y `desplegar` (`deploy-pages`, entorno `github-pages`) |
| Rama | despliega solo desde `main` |
| Herramientas | mdBook fijado por versión y SHA-256; Rust estable con caché de `target/` |

**`ci.yml`**: dos jobs nuevos, `cambios-web` (detecta si la PR toca `web/`, `crates/magi-web/`, `assets/demo/`, `docs/manual/` o `web.yml`) y `web` (construye con los datos reales de la API y comprueba, sin desplegar). En la PR construye dos veces el manual: el de la última etiqueta, que es el que se publica, y el de la propia rama, para que un capítulo, un enlace o un ancla nuevos se comprueben antes de llegar a una release. Como `cambios-termux` y `termux`, no entran en la regla de `main` (filtro de rutas). `cargo test --workspace` ya cubre `magi-web` y la prueba normal de capturas en `tests-linux`.

`dist plan` no debe cambiar: `magi-web` lleva `dist = false`.

### 5.5 Pasos del desarrollador (no los hace el agente)

- DNS de erratia.com: TXT de verificación y CNAME `magi` (§4.6).
- GitHub: dominio verificado en la cuenta; en el repositorio, Pages con origen «GitHub Actions», dominio `magi.erratia.com` y «Enforce HTTPS».
- Keystore de la app sin «Cuatro de Tres» en el `-dname`, si el que hay lo lleva o no existe; sus cuatro secretos; la huella en `web/datos/plataformas.toml` (por PR).
- Primera publicación: lanzar `web.yml` a mano y pasar el guion (§11).

### 5.6 Versiones

La web no va en el binario, así que la fase no sube la versión del escritorio por sí misma: lo que cambie (el `homepage` de `Cargo.toml` y las pruebas nuevas) sale con la siguiente. Si algo del binario o de `assets/` cambia, es una PATCH (1.6.2) con su línea. La app no cambia de número: la 0.2.0, que aún no ha salido, sale ya con el `applicationId` nuevo. Las dos novedades van en «Sin publicar» de su CHANGELOG.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 magi.erratia.com
 ├── /                    Portada ─────────────── ⇄ /en/
 │    ├─▶ Descargar 1.6.1 (botón principal)
 │    └─▶ Leer el manual
 ├── /descargas/          Descargas ───────────── ⇄ /en/download/
 │    ├─▶ «Qué se ha probado» (manual, estado) en las plataformas en pruebas o beta
 │    ├─▶ capítulo de instalación de cada canal
 │    └─▶ GitHub Releases (todas las versiones)
 ├── /versiones/          Versiones ───────────── ⇄ /en/releases/
 │    └─▶ cada versión, a su release en GitHub
 ├── /manual/             Manual (mdBook, solo en castellano; el menú inglés dice «Manual (Spanish)»)
 │    ├── 17 capítulos + «Qué se ha probado»
 │    └── búsqueda en la propia página
 └── /404.html            No encontrada (castellano e inglés)

 Cabecera común: MAGI · Inicio · Descargas · Versiones · Manual · EN (o ES)
 Pie común: licencia · privacidad · GitHub · seguridad · contribuir · «manual de la 1.6.1»
```

### 6.1 Portada (escritorio)

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ MAGI          Inicio   Descargas   Versiones   Manual                     EN  │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  MAGI                                                                         │
│  Gestor SSH de terminal, con estética de cabina y manejado con el teclado.    │
│  Linux y macOS · Termux · app Android. Software libre y sin telemetría.       │
│                                                                               │
│  [ Descargar 1.6.1 ]     [ Leer el manual ]                                   │
│                                                                               │
│  ▾ Animación de la demo                                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐      │
│  │ (magi.gif: Flota ─▶ sesión ─▶ archivos ─▶ deliberación MAGI)        │      │
│  └─────────────────────────────────────────────────────────────────────┘      │
│                                                                               │
│  ┌ FLOTA ───────────────────────────┐  ┌ SESIÓN CON PANELES ──────────────┐   │
│  │ captura 100×30 a color           │  │ captura                          │   │
│  └──────────────────────────────────┘  └──────────────────────────────────┘   │
│  Todos tus hosts y su estado,          Pestañas y paneles que siguen vivos    │
│  de un vistazo.                        al cerrar la ventana.                  │
│                                                                               │
│  … archivos · túneles · deliberación MAGI · servicios                         │
│                                                                               │
├───────────────────────────────────────────────────────────────────────────────┤
│ MIT OR Apache-2.0 · Sin cookies ni analítica (privacidad) · GitHub · Seguridad │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Descargas

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ MAGI          Inicio   Descargas   Versiones   Manual                     EN  │
├───────────────────────────────────────────────────────────────────────────────┤
│ DESCARGAS · escritorio 1.6.1, del 22 nov 2026                                 │
│ ┌ LINUX ────────────────────────────────────────────────────────────────────┐ │
│ │ x86_64  estable  magi-ssh-x86_64-unknown-linux-gnu.tar.xz        9,1 MB  │ │
│ │         SHA-256  3f2a9c…(los 64 caracteres, partidos si no caben)…c9e1   │ │
│ │ ARM64   …        magi-ssh-aarch64-unknown-linux-gnu.tar.xz       8,7 MB  │ │
│ └───────────────────────────────────────────────────────────────────────────┘ │
│ ┌ MACOS · en pruebas · qué se ha probado ───────────────────────────────────┐ │
│ │ Apple   magi-ssh-aarch64-apple-darwin.tar.xz · Intel  …x86_64-apple-darwin│ │
│ └───────────────────────────────────────────────────────────────────────────┘ │
│ ┌ TERMUX · en pruebas · 1.6.1 ──────────────────────────────────────────────┐ │
│ │ magi-ssh_1.6.1_aarch64.deb    apt install ./magi-ssh_1.6.1_aarch64.deb    │ │
│ └───────────────────────────────────────────────────────────────────────────┘ │
│ ┌ APP ANDROID · beta · 0.2.0 ───────────────────────────────────────────────┐ │
│ │ magi-android-v0.2.0.apk · Obtainium: etiquetas android-v, APK magi-android-│ │
│ │ Certificado de firma (SHA-256): AB:CD:…:EF                                │ │
│ └───────────────────────────────────────────────────────────────────────────┘ │
│ VERIFICAR                                                                     │
│   sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256                │
│   gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz \           │
│     --repo HectorEspejo/magi                                                  │
│ Todas las versiones ─▶ GitHub Releases                                        │
└───────────────────────────────────────────────────────────────────────────────┘
```

Homebrew y AUR añaden su bloque («brew install HectorEspejo/tap/magi-ssh», «yay -S magi-ssh-bin») solo cuando su variable está a `true`.

### 6.3 Versiones

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ MAGI          Inicio   Descargas   Versiones   Manual                     EN  │
├───────────────────────────────────────────────────────────────────────────────┤
│ VERSIONES       Escritorio · App                                              │
│ ┌ ESCRITORIO 1.6.1 · 22 nov 2026 ───────────────────────── en GitHub ─▶ ─┐    │
│ │ Resumen. Primera versión publicada de MAGI: …                          │    │
│ │ ▸ Detalle                                                              │    │
│ └────────────────────────────────────────────────────────────────────────┘    │
│ ┌ APP 0.2.0 · aún sin publicar ──────────────────────────────────────────┐    │
│ │ La primera release de la app sale después de esta fase.                │    │
│ └────────────────────────────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────────────────────────┘
```

En `/en/releases/` sale el `**Summary.**` y el detalle, que está en castellano, dentro de un «▸ Details (in Spanish)».

### 6.4 Manual

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ MAGI          Inicio   Descargas   Versiones   Manual                     EN  │
├──────────────────────────┬────────────────────────────────────────────────────┤
│ [ buscar en el manual ]  │ Solo en castellano · Spanish only                  │
│ Manual de MAGI           │                                                    │
│ Instalación, actual…     │ # Túneles                                          │
│ Primeros pasos y diag…   │ Un túnel local abre un puerto en tu equipo …       │
│ …                        │                                                    │
│ > Túneles                │ ## Túneles remotos                                 │
│ …                        │ …                                                  │
│ Qué se ha probado        │ ◀ Archivos                       Snippets ▶        │
├──────────────────────────┴────────────────────────────────────────────────────┤
│ MIT OR Apache-2.0 · Sin cookies ni analítica · GitHub · Manual de la 1.6.1     │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 6.5 Portada en el móvil (320 px)

```
┌──────────────────────────────┐
│ MAGI                     EN  │
│ Inicio · Descargas ·         │
│ Versiones · Manual           │
├──────────────────────────────┤
│ MAGI                         │
│ Gestor SSH de terminal…      │
│ [ Descargar 1.6.1 ]          │
│ [ Leer el manual ]           │
│ ▾ Animación de la demo       │
│ ┌──────────────────────────┐ │
│ │ (animación a lo ancho)   │ │
│ └──────────────────────────┘ │
│ ┌ FLOTA ───────────────────┐ │
│ │ captura escalada; si no  │ │
│ │ se lee, se desplaza      │ │
│ │ dentro de su marco       │ │
│ └──────────────────────────┘ │
└──────────────────────────────┘
```

### Notas de UX

- **Estética.** La de la TUI: JetBrains Mono, cajas con bordes de una línea y el acento ámbar de la paleta fija. Nada de logos, tipografías ni imágenes de terceros: la estética es la propia de MAGI.
- **Sin JavaScript propio.** Lo plegable va con `<details>`. El menú se parte en dos líneas en pantallas estrechas, sin botón de hamburguesa.
- **Tema.** Claro u oscuro según el sistema (`prefers-color-scheme`); el manual, igual, sin su selector de temas. Las capturas conservan su fondo de terminal en los dos.
- **Movimiento.** La animación se puede ocultar plegando su `<details>`, y con `prefers-reduced-motion: reduce` no se muestra: en su lugar sale la captura de Flota.
- **Estados.** «en pruebas» y «beta» van en texto junto a la descarga, nunca solo con color, y enlazan a «Qué se ha probado».
- **Órdenes.** Las de verificación e instalación van en bloques de código que se pueden seleccionar enteros; no hay botón de copiar (necesitaría JavaScript).
- **Sumas.** Siempre completas, partidas si no caben (`overflow-wrap: anywhere`).
- **Móvil.** Sin desplazamiento horizontal a 320 px; una captura que no cabe legible se desplaza dentro de su marco, nunca la página.
- **Idioma.** El enlace EN/ES lleva a la página equivalente. En la parte inglesa, el manual se anuncia como «Manual (Spanish)».

---

## 7. Lógica de Negocio

Los ejemplos van en Python por claridad; `magi-web` lo implementa en Rust.

### 7.1 La versión de cada canal

Sale la mayor por número (no la más reciente por fecha), sin borradores, prereleases ni candidatas. Termux comparte release con el escritorio, pero puede ir un paso por detrás mientras `termux-release.yml` sube su `.deb`.

```python
import re

ESCRITORIO = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
APP = re.compile(r"^android-v(\d+)\.(\d+)\.(\d+)$")

def publicadas(releases, patron):
    validas = [r for r in releases
               if not r["draft"] and not r["prerelease"] and patron.match(r["tag_name"])]
    return sorted(validas, reverse=True,
                  key=lambda r: tuple(int(n) for n in patron.match(r["tag_name"]).groups()))

def elegir(releases):
    escritorio = publicadas(releases, ESCRITORIO)
    if not escritorio:
        raise DatoRaro("no hay ninguna versión publicada del escritorio")
    app = publicadas(releases, APP)
    termux = next((r for r in escritorio
                   if any(a["name"] == f"magi-ssh_{r['tag_name'][1:]}_aarch64.deb"
                          for a in r["assets"])), None)
    return escritorio[0], termux, (app[0] if app else None)
```

### 7.2 Artefactos y sumas

Cada plataforma busca su artefacto por el nombre de `plataformas.toml` (con `{version}` sustituida) y su `.sha256`, que se baja y se valida. Falta uno de los dos en una versión elegida: la construcción falla (salvo Termux, §7.1).

```python
SUMA = re.compile(r"^([0-9a-f]{64}) [ *](\S+)$")

def suma(texto, nombre):
    lineas = texto.strip().splitlines()
    m = SUMA.match(lineas[0]) if len(lineas) == 1 else None
    if not m or m.group(2) != nombre:
        raise DatoRaro(f"{nombre}.sha256 no tiene la forma «<64 hexadecimales>  {nombre}»")
    return m.group(1)
```

### 7.3 Huella del certificado de la app

La huella que enseña la web es la constante del repositorio, revisada por PR. `android-release.yml` ya escribe la suya en las notas de cada release de la app; si no coinciden, algo ha cambiado el keystore y la web no se publica.

```python
HUELLA = re.compile(
    r"^- Certificado de firma \(SHA-256\): ((?:[0-9A-F]{2}:){31}[0-9A-F]{2})$", re.M)

def comprobar_huella(notas, constante):
    if not constante:
        raise DatoRaro("falta huella_certificado en web/datos/plataformas.toml")
    m = HUELLA.search(notas)
    if not m:
        raise DatoRaro("las notas de la release de la app no traen la huella")
    if m.group(1) != constante:
        raise DatoRaro(f"huella de la release {m.group(1)} ≠ la del repositorio {constante}")
```

Sin ninguna release de la app no se comprueba nada: el bloque dice «aún sin publicar».

### 7.4 De los CHANGELOG a versiones

Se leen de la etiqueta de la última versión publicada de cada canal. Fuera los comentarios de plantilla y la sección «Sin publicar», que no casa con el encabezado de una versión.

```python
ENCABEZADO = re.compile(r"^## \[(\d+\.\d+\.\d+)\] - (\d{4}-\d{2}-\d{2})$", re.M)

def entradas(texto):
    texto = re.sub(r"<!--.*?-->", "", texto, flags=re.S)
    trozos = ENCABEZADO.split(texto)[1:]        # [número, fecha, cuerpo, número, …]
    for numero, fecha, cuerpo in zip(*[iter(trozos)] * 3):
        es = re.search(r"^\*\*Resumen\.\*\* (.+?)(?:\n\n|\Z)", cuerpo, re.M | re.S)
        en = re.search(r"^\*\*Summary\.\*\* (.+?)(?:\n\n|\Z)", cuerpo, re.M | re.S)
        if not es or not en:
            raise DatoRaro(f"la versión {numero} no tiene Resumen y Summary")
        yield numero, fecha, es.group(1), en.group(1), cuerpo
```

El detalle se pasa a HTML con `pulldown-cmark`, cambiando todo HTML en bruto por texto escapado. La página en castellano lleva el resumen y el detalle; la inglesa, el `Summary` y el detalle dentro de «▸ Details (in Spanish)».

### 7.5 Preparar el manual

```python
INDICE = re.compile(r"^- \[(.+?)\]\(([a-z0-9-]+\.md)\)", re.M)

def summary(readme, ficheros):
    bloque = readme.split("## Capítulos", 1)[1].split("\n## ", 1)[0]
    capitulos = INDICE.findall(bloque)
    fuera = set(ficheros) - {"README.md", "estado.md"} - {f for _, f in capitulos}
    if fuera:
        raise DatoRaro(f"capítulo fuera del índice: {', '.join(sorted(fuera))}")
    lineas = ["[Manual de MAGI](README.md)", ""]
    lineas += [f"- [{t.replace('`', '')}]({f})" for t, f in capitulos]
    lineas.append("- [Qué se ha probado](estado.md)")
    return "\n".join(lineas) + "\n"

def enlace_de_fuera(destino, etiqueta):
    """destino: la ruta, resuelta desde docs/manual/, de un enlace que sale del manual."""
    if destino in ("README.md", "README.en.md"):
        return "../" if destino == "README.md" else "../en/"
    tipo = "tree" if destino.endswith("/") else "blob"
    return f"https://github.com/HectorEspejo/magi/{tipo}/{etiqueta}/{destino}"
```

Los enlaces que se quedan dentro del manual (`tuneles.md#…`) los convierte mdBook a `.html`. Las imágenes no hacen falta: el manual de la 1.6.1 no tiene ninguna; si un capítulo las trae, se copian con él.

### 7.6 Anclas como en GitHub

mdBook calcula los id de los títulos con su propio algoritmo, que no tiene por qué coincidir con el de GitHub (en la 0.4, por ejemplo, no pasaba a minúsculas las mayúsculas acentuadas), y los enlaces del manual usan los de GitHub. `preparar-manual` añade a cada título su `{#id}` con el algoritmo de GitHub, el mismo que ya usa `repositorio_publico` (`slug_base` y `slugs_de`). El algoritmo pasa a `magi-web` (`anclas.rs`) y la prueba lo usa desde allí (dependencia de desarrollo): una sola copia.

```python
def ancla(titulo, vistos):
    base = "".join(c for c in titulo.strip().lower() if c.isalnum() or c in " -_")
    base = base.replace(" ", "-")
    n = vistos.get(base, 0)
    vistos[base] = n + 1
    return base if n == 0 else f"{base}-{n}"
```

`titulo` es el texto del título ya sin marcas (el código sin comillas, los enlaces con su texto). mdBook respeta el `{#id}` de un título (atributos de encabezado), así que los enlaces `capitulo.md#ancla` funcionan igual que en GitHub; si la versión fijada no lo respetara, `magi-web` cambia los id en el HTML generado. `comprobar` lo verifica con todos los enlaces.

### 7.7 Capturas

La prueba ignorada `capturas_web`:

1. Crea un entorno aislado (`HOME` y `XDG_*`), con `tema = "fijo"`, e importa `assets/demo/inventario`. Nunca usa la semilla de las pruebas, cuyos alias pueden ser reales (R192).
2. Para cada vista (Flota, sesión con dos paneles, archivos, túneles, el diálogo de deliberación MAGI y Servicios) monta el estado con `App::de_prueba` y lo pinta con el bucle real sobre un `TestBackend` de 100×30. Las vistas que piden algo remoto (sesión, archivos, servicios) reciben datos fijos, como en las pruebas de pintado.
3. Exporta el `Buffer` a HTML: un `<pre>` por captura, con tramos de celdas del mismo estilo en `<span>` con clase; las clases van a `capturas.css`. Las celdas que continúan un carácter ancho no se emiten.
4. Comprueba que el inventario de la `App` es exactamente el de la demo (nombres `demo-*`), que ninguna captura contiene un nombre de la semilla de las pruebas y que cada carácter tiene glifo (§7.8).

Si una captura necesita más hosts para lucir, se añaden al inventario de la demo, siempre inventados (`demo-*`); no hace falta regrabar la animación por eso.

```python
def exportar(buffer, ancho=100, alto=30):
    clases, filas = {}, []
    for y in range(alto):
        tramos, estilo_actual, texto = [], None, ""
        for x in range(ancho):
            celda = buffer[x, y]
            if celda.es_continuacion:          # segunda mitad de un carácter ancho
                continue
            estilo = (celda.fg, celda.bg, celda.negrita, celda.subrayado, celda.invertido)
            if estilo != estilo_actual and texto:
                tramos.append((estilo_actual, texto))
                texto = ""
            estilo_actual, texto = estilo, texto + con_glifo(celda.simbolo)
        tramos.append((estilo_actual, texto))
        filas.append("".join(
            f'<span class="{clases.setdefault(e, f"c{len(clases)}")}">{escapar(t)}</span>'
            for e, t in tramos))
    return '<pre class="captura" aria-hidden="true">' + "\n".join(filas) + "</pre>", clases
```

En la portada, cada captura es un `<figure>` con su `<figcaption>` y una descripción para el lector de pantalla (`role="img"` y `aria-label`); el `<pre>` va con `aria-hidden`, para que nadie oiga los bordes de caja. La prueba normal `capturas_web` repite el pintado y compara con `web/capturas/`: si algo cambia, falla y dice «ejecuta `scripts/generar-capturas.sh`».

### 7.8 Glifos y fuentes

JetBrains Mono 2.305 (la de la app) no tiene diez de los glifos que usa la TUI en sus instantáneas: `↵ ↻ ⇄ ⇅ ▣ ◐ ◑ ★ ⚑ ⤴`. Sin ellos, el navegador usaría otra fuente con otro ancho y las cajas se descuadrarían.

- **Subconjunto de símbolos.** `magi-simbolos.woff2` lleva solo esos glifos, sacados de DejaVu Sans Mono (avance de 0,602 em frente a 0,600 de JetBrains Mono: un 0,34 % de diferencia), y se carga con `unicode-range`. Cubre nueve de los diez.
- **Degradación.** Lo que no está en ninguna de las dos fuentes (hoy, `⤴`) se cambia en la captura por su equivalente de `disposicion::texto_ascii`, el único degradador.
- **Prueba de cobertura.** Falla si una captura trae un carácter que no está en JetBrains Mono, ni en el subconjunto, ni en la tabla de degradación. `web/README.md` dice cómo rehacer el subconjunto (`pyftsubset` de fontTools, a mano, una vez).

Las dos fuentes van con su licencia (`OFL.txt` y `LICENSE-DejaVu.txt`). Los woff2 de JetBrains Mono salen de los TTF de la app, convertidos una vez.

### 7.9 Paletas

La oscura es la paleta fija de la TUI (`src/tema.rs`), más un gris secundario para el texto menos importante. La clara es su derivada. Una prueba de `magi-web` calcula los contrastes con la fórmula de WCAG y exige 4,5:1 o más en texto y 3:1 o más en bordes.

| Papel | Oscura | Contraste | Clara | Contraste |
|---|---|---|---|---|
| Fondo | `#0A0A0A` | — | `#F4F0E6` | — |
| Texto | `#EDE8DC` | 16,19:1 | `#0A0A0A` | 17,40:1 |
| Acento (enlaces, títulos) | `#FFA000` | 9,69:1 | `#8A5000` | 5,72:1 |
| Correcto | `#00FF6A` | 14,62:1 | `#006B2E` | 5,87:1 |
| Crítico | `#FF2A1A` | 5,28:1 | `#B3140A` | 6,11:1 |
| Secundario (texto) | `#9A968C` | 6,71:1 | `#5C5850` | 6,22:1 |
| Inactivo (solo bordes) | `#5A5A5A` | 2,87:1 | `#8C887E` | 3,11:1 |

El inactivo oscuro no llega a 3:1: en la web solo se usa en bordes decorativos, que no transmiten nada. Los botones principales van en acento con el texto del fondo: 9,69:1 en oscuro y 5,72:1 en claro. Las capturas usan siempre la paleta de la TUI sobre su propio fondo.

### 7.10 CSP

GitHub Pages no deja poner cabeceras, así que la CSP va en un `<meta http-equiv="Content-Security-Policy">`, al principio de `<head>` en cada página. La pone `magi-web construir` después de generar todo, también el manual.

```python
import base64, hashlib, re

def csp(html, es_manual):
    if not es_manual:
        guion = "'none'"                          # páginas propias: sin JavaScript
    else:
        en_linea = re.findall(r"<script>(.*?)</script>", html, flags=re.S)
        hashes = ["'sha256-" + base64.b64encode(hashlib.sha256(s.encode()).digest()).decode() + "'"
                  for s in en_linea]
        guion = " ".join(["'self'"] + hashes)
    return ("default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self'; "
            f"script-src {guion}; base-uri 'none'; form-action 'none'")
```

Si el tema de mdBook trae estilos en línea, van también por su hash; nunca `'unsafe-inline'`. `frame-ancestors` no funciona en un `<meta>`: se acepta, porque la web no tiene nada que proteger de un marco (§7.13). Cada página lleva además `<meta name="referrer" content="strict-origin-when-cross-origin">`.

### 7.11 Validaciones

- Etiquetas con su patrón exacto; versiones sin ceros a la izquierda ni sufijos.
- Artefactos con el nombre de `plataformas.toml`, tamaño mayor que 0 y URL bajo `https://github.com/HectorEspejo/magi/releases/download/<etiqueta>/`.
- `.sha256` de una línea con 64 hexadecimales en minúsculas y el nombre exacto del artefacto.
- `plataformas.toml`: ids únicos, canales y estados conocidos, un artefacto por plataforma, órdenes sin repetir y huella con 32 pares hexadecimales en mayúsculas (o vacía si no hay app publicada).
- Textos: las mismas claves en los dos ficheros y ninguna vacía.
- Manual: todo capítulo en el índice; todo enlace y ancla resuelve.
- HTML generado: sin atributos `style`, sin `<script>` en las páginas propias, enlaces externos solo de la lista permitida y por `https://`.
- Un fallo de cualquiera para la construcción con un mensaje que dice qué y dónde; la web publicada no cambia.

### 7.12 Casos especiales

| Caso | Qué hace la web |
|---|---|
| La app no tiene ninguna release (al estrenar la web) | Su bloque dice «aún sin publicar» y enlaza al capítulo `app-android` |
| La última release del escritorio aún no tiene el `.deb` | Termux enseña la anterior que sí lo tiene, con su número |
| Una release se borra | La siguiente construcción la deja de enseñar; para que sea ya, se lanza a mano |
| La API falla o limita | La construcción falla y la web anterior sigue; se relanza a mano |
| Homebrew o AUR se encienden | Su bloque aparece en la siguiente construcción con la variable a `true` |
| Antes de configurar el dominio | La web funciona en `hectorespejo.github.io/magi/` (enlaces relativos), salvo los enlaces absolutos del 404 |
| La etiqueta publicada es anterior a la 9b (sin `docs/manual/`) | No puede pasar (la primera publicada es la 1.6.1); si pasara, falla |
| Un capítulo nuevo en `main` | No sale hasta la siguiente versión (D443); la PR ya lo comprueba (§5.4) |

### 7.13 Limitaciones

- La búsqueda de mdBook no tiene raíces en castellano: encuentra palabras enteras, no variantes («túnel» no encuentra «túneles» si no se escribe así).
- Las capturas salen de `main` y pueden enseñar algo de «Sin publicar» unos días (R196).
- En inglés, el detalle de las versiones y el manual están en castellano.
- Sin `frame-ancestors`: otra web podría meter esta en un marco. No hay sesión, formularios ni acciones que proteger.

---

## 8. Requisitos No Funcionales

**Seguridad**

- Sin JavaScript propio; el del manual, el de mdBook, con CSP por hashes. Nada cargado de fuera y solo `https://`.
- Lo que llega de la API, de los `.sha256` y de los CHANGELOG es entrada no fiable: se valida y se escapa (§7.11).
- `web.yml` con los permisos justos (`contents: read`, `pages: write`, `id-token: write`), sin secretos, y despliegue solo desde `main`. La huella del APK es una constante del repositorio, comprobada contra la release.
- mdBook y las fuentes, fijados (versión y SHA-256 en un caso, ficheros versionados con licencia en el otro); las dependencias de `magi-web`, en `Cargo.lock` y bajo `cargo deny`.
- El dominio, verificado en GitHub antes de ponerlo en Pages; la retirada, primero en el DNS (R197).
- Sin formularios, sin cookies y sin sesión.

**Protección de datos (RGPD)**

- La web no recoge datos personales: no tiene formularios, cookies, analítica ni recursos de terceros.
- GitHub Pages registra los accesos (con la IP) como proveedor; lo cuenta la nota de privacidad del pie, con un enlace a la declaración de privacidad de GitHub.
- El manual guarda en el navegador (`localStorage`) el estado de su barra lateral; no sale del navegador ni identifica a nadie. Lo dice la misma nota.
- Sin aviso legal: es una web personal sin actividad económica. Si algún día la tuviera (donaciones, anuncios), habría que revisarlo (R203).

**Backups y recuperación**

- Nada que respaldar: todo sale del repositorio y de las releases.
- Pages conserva el último despliegue mientras una construcción falla.
- Volver atrás: relanzar desde Actions una ejecución anterior de `web.yml` o revertir en `main`.
- Retirar la web: quitar el CNAME del DNS, luego el dominio de Pages y, si se quiere, desactivar Pages (`docs/publicar-web.md`).

**Rendimiento**

- Portada por debajo de 1 MB sin contar descargas: la animación (536 KB), las fuentes (unos 200 KB), seis capturas (unos 150 KB) y la CSS.
- Primera visita en menos de 2 s con 4G; páginas propias sin peticiones a otros dominios.
- Construcción en CI por debajo de 5 min: `magi-web` compila solo sus dependencias, no las del escritorio.
- Volumen: unas 25 páginas y el índice de búsqueda del manual (unos 100 KB de texto).

**Accesibilidad (WCAG 2.2 AA)**

- Contraste comprobado por prueba (§7.9), foco visible, todo con teclado y un enlace para saltar al contenido.
- `lang` en cada página, `hreflang` en las alternativas y títulos de página únicos.
- Sin desplazamiento horizontal a 320 px; texto ampliable al 200 % sin perder nada.
- Animación que se puede ocultar (2.2.2) y que no sale con `prefers-reduced-motion`.
- Capturas como figuras con descripción; los estados, en texto y no solo con color.

**Disponibilidad**

- Depende de GitHub Pages. Si la web cae, las releases y el README siguen en GitHub.

---

## 9. Integraciones

| Integración | Uso | Autenticación |
|---|---|---|
| API REST de GitHub | Releases, artefactos y `.sha256` en cada construcción | `GITHUB_TOKEN` del workflow, `contents: read` |
| GitHub Pages | Alojamiento, con despliegue por Actions | OIDC (`id-token: write`), entorno `github-pages` |
| DNS de erratia.com | TXT de verificación y CNAME `magi` → `hectorespejo.github.io` | La cuenta del registrador; lo hace el desarrollador |
| mdBook | Manual | Binario fijado por versión y SHA-256 |
| Obtainium | Solo instrucciones en descargas (URL del repositorio, filtros `android-v` y `magi-android-`) | — |
| Homebrew y AUR | Solo texto en descargas, según `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` | Las variables se leen en `web.yml` (`vars.*`) |

**Formatos:** TOML (`plataformas.toml` y textos), JSON (`releases.json`), HTML y CSS, woff2. El `.sha256` sigue el formato de `sha256sum`.

---

## 10. Decisiones Técnicas (ADR-lite)

- **D439 — La web es la 9c** (decisión del analista). Sigue la línea de publicación de la 9 y la 9b y no renumera el roadmap: Accesos y certificados sigue siendo la F18. Descartado hacerla F18: habría movido cinco fases por una web.
- **D440 — GitHub Pages desde el mismo repositorio, con Actions** (Hector: «una siguiente fase para el GitHub Pages»). Sin secretos ni cuentas nuevas; la web se revisa en las PR como el resto y `repositorio_publico` la vigila. Descartados Cloudflare (otro token como secreto en el repositorio) y un repositorio aparte (dispararlo entre repositorios exige un token personal).
- **D441 — Dominio `magi.erratia.com`** (Hector, 1b). Subdominio del blog de Hector, con CNAME, dominio verificado en GitHub y HTTPS forzado. Los enlaces internos son relativos, para que la web funcione también en `hectorespejo.github.io/magi/`; solo el 404 usa absolutas. Descartado `github.io` como dirección principal.
- **D442 — mdBook para el manual y un generador propio para lo demás** (Hector, 6a). `magi-web` (Rust, en el workspace, biblioteca y binario, `publish = false` y `dist = false`) hace portada, descargas, versiones y 404 con plantillas `minijinja` en `web/plantillas/`, y pone la misma cabecera (por `header.hbs`) y el mismo pie (al generar) en el manual. Descartados Zola (reescribir los enlaces del manual) y todo propio (rehacer la búsqueda).
- **D443 — Qué sale de dónde** (Hector, 5a). El manual y los CHANGELOG, de la etiqueta de la última versión publicada de su canal: lo que se lee es lo que se baja. Plantillas, textos, datos, capturas y animación, de `main`. Versiones y artefactos, de la API en cada construcción. Descartado el manual de `main`, que se adelanta a lo publicado.
- **D444 — Capturas a color desde el código de pintado** (Hector, 4a). Una prueba ignorada las genera con `App::de_prueba`, `TestBackend` a 100×30, el tema fijo y el inventario de la demo; se versionan en `web/capturas/` y una prueba normal exige que estén al día. Nunca con la semilla de las pruebas (R192). Descartados los PNG con `vhs` (a mano y desfasables).
- **D445 — Fuentes servidas desde la web** (Hector, 3a). JetBrains Mono 2.305, la de la app, en woff2, más un subconjunto de DejaVu Sans Mono con los nueve glifos de la TUI que le faltan, por `unicode-range`. Lo que no esté en ninguna se degrada con `texto_ascii`, y una prueba vigila la cobertura.
- **D446 — Sin JavaScript propio** (Hector, 9a). Portada, descargas y versiones son HTML y CSS; lo plegable va con `<details>`. El único JavaScript es el de mdBook en el manual (búsqueda y barra lateral), con CSP por hashes calculada al generar. Descartado resaltar la plataforma del visitante.
- **D447 — Castellano en la raíz e inglés en `/en/`; el manual, solo en castellano** (Hector, 2a). Portada, descargas, versiones y 404 en los dos idiomas, con `hreflang` y enlace a la página equivalente. El manual avisa «Solo en castellano · Spanish only». Descartado traducir el manual (un sprint más y dos manuales que mantener en cada PR).
- **D448 — Descargas: la última versión publicada de cada canal** (Hector, 10a). Nunca una candidata. La suma sale del `.sha256` publicado, con verificación por `sha256sum`, `gh attestation` y, en la app, `apksigner`. El estado de cada plataforma vive en `plataformas.toml` y cambia en el mismo PR que `estado.md`. Homebrew y AUR solo salen con su variable a `true`. Termux puede ir un paso por detrás mientras sube su `.deb`.
- **D449 — La huella del APK es una constante del repositorio** (decisión del analista). La pone el desarrollador en `plataformas.toml` al crear el keystore, y la construcción falla si la release de la app dice otra. Descartado sacarla del propio APK publicado: no probaría nada si alguien lo cambiara.
- **D450 — `applicationId` `io.github.hectorespejo.magi` antes de la primera release de la app** (Hector, 12b). El namespace y el paquete Kotlin (`org.cuatrodetres.magi`, también el de UniFFI) se quedan: el usuario no los ve, renombrarlos es un diff enorme sin efecto visible, y se puede hacer más adelante sin romper instalaciones. Keystore nuevo si el que hay lleva «Cuatro de Tres». La app se publica después de esta fase (el ítem de la 9b espera). Descartado publicar con el identificador de 4d3.
- **D451 — Cuándo se regenera** (decisión del analista). Con `workflow_run` de «Release», «Release Termux» y «Release Android», solo si terminaron bien; con un push a `main` que toque la web; y a mano. Despliega solo desde `main`, con concurrencia `pages`. En las PR, el job `web` de `ci.yml` construye y comprueba sin desplegar, también el manual de la rama. Descartado `on: release`: una release que crea el `GITHUB_TOKEN` no dispara otros workflows.
- **D452 — Anclas y enlaces del manual como en GitHub** (decisión del analista). Cada título lleva su `{#id}` con el algoritmo de `repositorio_publico`, que pasa a `magi-web` para que solo haya uno. Los enlaces que salen del manual van a GitHub en la etiqueta publicada (`blob` o `tree`); los de los README, a la portada.
- **D453 — Privacidad** (Hector, 11a). Ni cookies, ni analítica, ni recursos de fuera; nota corta en el pie (GitHub registra los accesos; el manual guarda en el navegador su barra lateral). Sin aviso legal: web personal sin actividad económica.
- **D454 — La portada** (Hector, 4a y 7a). Textos propios en `web/textos/`, con las mismas claves en los dos idiomas. La animación del README va dentro de un `<details open>` que se pliega sin JavaScript (WCAG 2.2.2) y, con `prefers-reduced-motion`, se sustituye por la captura de Flota. Seis capturas: Flota, sesión con paneles, archivos, túneles, deliberación MAGI y servicios.
- **D455 — Versiones** (Hector, 8a). Los dos CHANGELOG, de su etiqueta, con `pulldown-cmark` y el HTML en bruto escapado. Cada versión lleva el resumen en el idioma de la página, el detalle en un `<details>` y el enlace a su release. Sin «Sin publicar».
- **D456 — Paletas** (Hector, 3a). La oscura es la paleta fija de `src/tema.rs`, más un secundario `#9A968C`; la clara es su derivada (§7.9). Contraste comprobado por prueba. El tema sigue al sistema, y las capturas conservan su fondo de terminal.
- **D457 — `homepage` de `Cargo.toml` es la web** (decisión del analista). La fórmula de Homebrew y los README enlazan `https://magi.erratia.com`; `repository` sigue siendo GitHub. El README sigue siendo la presentación en GitHub.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Semanas |
|---|---|---|
| S1 — Generador y descargas | Crate `magi-web` (biblioteca y binario, `dist = false`): datos, textos, plantillas, `web.css` con las paletas y su prueba de contraste, fuentes con el subconjunto de símbolos. Lectura y validación de releases y `.sha256`, elección por canal y huella del APK. Descargas y 404 en los dos idiomas. CSP, `comprobar` y `scripts/generar-web.sh`. `web.yml` y los jobs `cambios-web` y `web` de `ci.yml`. `homepage` | 1 |
| S2 — Portada, capturas y versiones | Prueba `capturas_web` (generar y comprobar), exportación a HTML con clases, nombres de la demo, cobertura de glifos y `scripts/generar-capturas.sh`. Portada en los dos idiomas con la animación ocultable y las seis capturas. Versiones desde los CHANGELOG de la etiqueta | 1 |
| S3 — Manual, app y cierre | `preparar-manual` (SUMMARY, enlaces de fuera, anclas con el algoritmo compartido con `repositorio_publico`) y el tema de mdBook (cabecera, pie, paletas, sin selector, aviso de idioma). `applicationId`, `android-release.yml` y `docs/publicar-android.md`. `docs/publicar-web.md`, `web/README.md`, `CLAUDE.md`, `docs/README.md`, `instalacion` y los CHANGELOG. Revisión adversarial de cierre e informe de implementación | 1 |
| Desarrollador | DNS y verificación, Pages, keystore y huella, primera publicación con el guion; después, la app (`android-v0.2.0-rc.1` y `android-v0.2.0`, ítem de la 9b) | ½ día |

**Total: 3 sprints, ~3 semanas.** Supuestos: un desarrollador con Claude Code y dedicación parcial, como en las fases anteriores; la 1.6.1 publicada antes de empezar; el DNS de erratia.com accesible; mdBook 0.5 con los parciales `head.hbs` y `header.hbs`.

**Pasos del desarrollador, en orden** (los repite `docs/publicar-web.md`):

1. Publicar la 1.6.1 (9b) y, antes de empezar la 9c, decidir lo pendiente de la 9b que toque a la web: los alias de las instantáneas (R192) no llegan a la web en ningún caso.
2. Verificar `erratia.com` en GitHub (TXT) y crear el CNAME `magi` → `hectorespejo.github.io` (§4.6).
3. Crear el keystore sin «Cuatro de Tres» si hace falta, sus cuatro secretos, y pasar al agente la huella para `plataformas.toml`.
4. Fusionar la 9c con la CI en verde.
5. Pages: origen «GitHub Actions», dominio `magi.erratia.com` y, con el certificado, «Enforce HTTPS».
6. Lanzar `web.yml` a mano y pasar el guion de abajo.
7. La app: `android-v0.2.0-rc.1`, el guion de la app de la 9b y `android-v0.2.0`. La web se regenera sola al terminar cada una.

**Guion de la web** (en el móvil y en el escritorio):

1. Portada en claro y en oscuro (cambiando el tema del sistema); plegar la animación; con «reducir movimiento», ver la captura de Flota en su lugar.
2. Descargas: bajar un tarball de Linux con su `.sha256` y pasar `sha256sum -c` y `gh attestation verify`.
3. Manual: buscar «túnel», seguir un enlace con ancla de un capítulo a otro y uno que sale a GitHub.
4. Pasar a inglés desde cada página y volver: se llega a la equivalente.
5. En el móvil, a 320 px: sin desplazamiento horizontal; las capturas se desplazan dentro de su marco.
6. Abrir una ruta que no existe: sale el 404 en los dos idiomas.
7. Con las herramientas del navegador: ninguna petición a otro dominio y ningún aviso de la CSP.

---

## 12. Conexiones con Otras Fases

**Dependencias**

- **F9 y 9b:** `dist` con sus `.sha256` y atestaciones, los nombres de los artefactos, la 1.6.1 publicada, el manual de `docs/manual/`, `estado.md`, los CHANGELOG con su resumen bilingüe, la demo y las variables `PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` (desviación 19 de la 9b).
- **F11 y F14:** la app y `android-release.yml`, con la huella en las notas de cada release.
- **F17:** el `.deb` de Termux y `termux-release.yml`.
- **F7:** `App::de_prueba`, el `TestBackend` y las pruebas de pintado, base de las capturas.
- **T151:** MAGI es un proyecto personal; la web no nombra a 4d3.

**Datos compartidos**

- Ninguna tabla. La web lee `docs/manual/`, los dos CHANGELOG, `assets/demo/` y `src/tema.rs` (la paleta, a través de las capturas), y escribe solo en `web/capturas/`.

**Lo que viene**

- **9b:** se cierra cuando salga la app, después de esta fase, con el `applicationId` nuevo.
- **F17:** su cierre en un prompt de reanudación, después de la 1.6.1; la web enseñará Termux «en pruebas» hasta que cambie su estado.
- **F18 (Accesos y certificados SSH)** y las siguientes, sin cambios.
- **Más adelante, si AUR sigue cerrado:** repositorios pacman y apt firmados servidos desde la misma web (la cuarta propuesta del 22 nov 2026). Y, si hace falta, traducir el manual.
