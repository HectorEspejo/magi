# CLAUDE.md — MAGI

## Contexto

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy) y macOS, con app Android prevista. Cliente: 4d3 (producto propio).
Stack: Rust stable, ratatui + crossterm, tokio, russh + russh-keys, tui-term + vt100, rusqlite (bundled, WAL), clap, toml + serde, directories, nucleo-matcher, zeroize, tracing.

## Documentación del proyecto

La documentación funcional vive en `docs/`:

- `magi-maestro.md` — visión global, mapa de fases y estado del proyecto
- `magi-fase1-informe.md` — especificación funcional completa de la fase
- `magi-fase1-checklist.md` — alcance verificable de la fase
- `magi-fase1-prompt.md` — prompt de arranque de la fase
- `magi-fase1-implementacion.md` — informe de implementación (lo escribes tú)

El informe de fase manda sobre tu criterio: si algo te parece incorrecto o
incompleto, no lo cambies por tu cuenta — impleméntalo como está o párate y
coméntalo con el desarrollador.

## Reglas de trabajo

- Escribe siempre en castellano: respuestas, comentarios de código, mensajes de
  commit, documentación y textos de interfaz. Solo se mantienen en inglés los
  términos técnicos estándar (API, CRUD, endpoint, commit...).
- Tu effort por defecto es `ultracode`: una vez aprobado el plan que presentes,
  utiliza los workflows y todo lo disponible en ese effort.
- No añadas funcionalidades que no estén en el checklist de la fase. Si detectas
  algo necesario que falta, propónlo antes de implementarlo y déjalo anotado
  como desviación.
- Nomenclatura: tablas SQL en `MAYUSCULAS_SNAKE_CASE`, campos en
  `minusculas_snake_case`. Identificadores de Rust (módulos, funciones,
  variables) en castellano y `snake_case`; tipos en `CamelCase` en castellano.
- Sin `CHECK` sobre enumeraciones en SQLite: los valores se validan en código.
- Migraciones en `src/almacen/migraciones.rs`, numeradas y aplicadas por
  `PRAGMA user_version`; nunca modifiques una migración ya aplicada, añade otra.
- Nunca guardes claves privadas ni frases en disco ni en el log; en memoria
  usa `zeroize`.
- Toda operación de red va en una tarea tokio y comunica por el canal de
  eventos; la UI no bloquea nunca.
- Fija las versiones de `ratatui`, `tui-term` y `vt100` compatibles entre sí
  en `Cargo.toml` y anótalo en el informe de implementación.
- Comandos: `cargo run` arranca la TUI; `cargo run -- importar [ruta]` y
  `cargo run -- exportar` son los subcomandos; `cargo test` ejecuta los tests;
  `cargo clippy -- -D warnings` y `cargo fmt` antes de cada commit.
- Ficheros que MAGI escribe (`magi.db`, `magi_config`, `known_hosts`) se crean
  con permisos 600 y de forma atómica; antes de tocar `~/.ssh/config` o
  `known_hosts`, copia de seguridad con sufijo de fecha.

## Al terminar una fase (o al pausar la sesión)

1. Busca el archivo markdown que tenga `checklist` en el nombre correspondiente
   a esa fase y marca `[x]` todo lo que se haya realizado. Actualiza los
   contadores de progreso del final del archivo. No reescribas el texto de las
   funcionalidades: es la clave de trazabilidad con el resto de documentos.
2. Genera o actualiza `magi-fase[N]-implementacion.md` con exactamente
   esta estructura:
   - Resumen de lo implementado
   - Desviaciones respecto a la especificación (qué y por qué)
   - Estructura de archivos creada/modificada
   - Decisiones técnicas tomadas durante el desarrollo
   - Funcionalidades del checklist completadas (copiando su texto exacto)
   - Pendientes y bloqueos
   - Ejecución y pruebas (cómo arrancar, migrar y testear)

   Es un documento vivo: actualízalo de forma incremental en cada sesión, nunca
   lo regeneres desde cero. Sé honesto en las desviaciones y en los pendientes;
   ese informe es lo único que el analista funcional verá de lo que realmente
   pasó, y con él se actualiza la especificación.
3. Recuérdale al desarrollador que debe entregar el informe de implementación al
   analista funcional antes de especificar la siguiente fase.
4. Invita al desarrollador a hacer PR. Si te dice que lo hagas, usa el comando
   `gh`.

## Commits

- NUNCA firmes ni te atribuyas los commits: la autoría es del usuario.
- Sin `Co-Authored-By`, sin líneas de "Generated with", sin menciones a Claude
  en el mensaje.
- Mensajes en castellano, en imperativo y concisos.
