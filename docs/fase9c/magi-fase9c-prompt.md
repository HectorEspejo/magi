# Prompt MAGI - Fase 9c: Web de MAGI

Desarrolla la Fase 9c de **MAGI**, el gestor SSH de terminal de Hector Espejo (proyecto personal): su web, `https://magi.erratia.com`, en GitHub Pages desde este repositorio, en una rama `fase9c` desde `main` con la 1.6.1 publicada. Antes, copia a `docs/fase9c/` el informe, el checklist y este prompt; el maestro, a `docs/magi-maestro.md` y a `docs/fase9c/`; a `docs/fase9b/`, el informe v1.2 y el checklist, y a `docs/fase17/`, su checklist y su prompt de reanudación. Aplica al `CLAUDE.md` de la raíz los cambios del que llega. Stack: **crate `magi-web`** (Rust, biblioteca y binario; `minijinja`, `pulldown-cmark`; `publish = false` y `dist = false`), **mdBook 0.5** fijado por SHA-256, **`web.yml`** con `deploy-pages` y sin secretos, HTML y CSS sin JavaScript propio, JetBrains Mono y un subconjunto de DejaVu Sans Mono. Sin tablas, migración, protocolo ni `formato`; datos en ficheros: (1) **`web/datos/plataformas.toml`** con plataforma, canal, estado, artefacto y huella del APK; (2) **`web/textos/es.toml`** y **`en.toml`**; (3) **`releases.json`**, de la API en cada construcción; (4) **`web/capturas/`**, generadas. Funcionalidades: descargas de la última versión publicada de cada canal (escritorio, Termux y app; nunca `-rc`) con SHA-256 y verificación, y Homebrew y AUR solo con su variable; portada con la animación plegable y seis capturas a color desde el pintado, con el inventario de la demo; versiones con los CHANGELOG de la etiqueta; el manual de la etiqueta, con las anclas de GitHub; CSP por hashes, comprobador de enlaces y `applicationId` `io.github.hectorespejo.magi`. Interfaz: estética de la TUI (paleta de `src/tema.rs` y su derivada clara), castellano en la raíz e inglés en `/en/` (el manual, solo en castellano), AA y 320 px. No tocas el DNS ni los ajustes de Pages. Commits sin firmar (`CLAUDE.md`).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase9c-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase9c-implementacion.md`**
   (informe de implementación) con exactamente esta estructura:
   - Resumen de lo implementado
   - Desviaciones respecto a la especificación (qué y por qué)
   - Estructura de archivos creada/modificada
   - Decisiones técnicas tomadas durante el desarrollo
   - Funcionalidades del checklist completadas (copiando su texto exacto)
   - Pendientes y bloqueos
   - Ejecución y pruebas (cómo arrancar, migrar y testear)
4. Actualiza el informe de implementación en cada sesión, no lo
   regeneres desde cero.
5. Cuando el informe esté listo, avisa al desarrollador de que debe
   entregárselo al analista funcional: hasta entonces la documentación de
   proyecto (checklist maestro, informe maestro) no refleja la realidad.
