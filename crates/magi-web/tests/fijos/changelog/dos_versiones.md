# Registro de cambios

Todos los cambios visibles de DEMO se anotan en este fichero. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).

<!-- Plantilla de una versión nueva (este comentario ocupa varias líneas y no
sale en la web):

## [X.Y.Z] - AAAA-MM-DD

**Resumen.** Una o dos líneas.

**Summary.** One or two lines.
-->

## [Sin publicar]

### Añadido

- Algo que todavía no ha salido y no debe aparecer en la web.

## [1.2.0] - 2026-11-22

<!-- comentario de una sola línea -->
**Resumen.** Segunda versión de DEMO: añade los túneles y mejora la
conexión (un resumen en dos líneas).

**Summary.** Second DEMO release: adds tunnels and improves
the connection (a two-line summary).

Texto de introducción con `código`, **negrita** y un
[enlace externo](https://github.com/HectorEspejo/magi/releases). La versión
anterior está en [la 1.1.0][1.1.0].

### Añadido

- **Túneles.** Locales y remotos, con `magi tunel`.
  - Un detalle anidado.
  - Otro detalle, con un nombre `magi-<uuid>.age` en código.
- La documentación del formato: [formato](docs/formato-sincronizacion.md) y el
  [manual](docs/manual/).

### Corregido

- Un fallo con <uuid> suelto en los nombres de fichero.

## [1.1.0] - 2026-10-01

**Resumen.** Primera versión de DEMO.

**Summary.** First DEMO release.

### Añadido

- La base: inventario y conexión.

[Sin publicar]: https://github.com/HectorEspejo/magi/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/HectorEspejo/magi/releases/tag/v1.2.0
[1.1.0]: https://github.com/HectorEspejo/magi/releases/tag/v1.1.0
