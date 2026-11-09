# Prompt MAGI - Fase 15: Terminal, Segunda Vuelta

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3 (workspace `magi-ssh` + `crates/magi-nucleo` + `crates/magi-movil`), con la segunda vuelta del terminal del escritorio, partiendo de la rama `fase14` ya cerrada y fusionada. Stack: **Rust, ratatui 0.30 + crossterm 0.29, tui-term 0.3 + vt100 0.16 con sus `Callbacks`, russh 0.63, rusqlite 0.40 y la dependencia nueva `regex`**. Tablas: (1) **HOSTS** gana `reconectar` (1), `tmux_persistente` (0) y `portapapeles_remoto` (0) en la migración 10, propios del equipo y sin viajar; (2) **REGISTRO** gana `reconexion_agotada`, `escritura_varios_activada`, `escritura_varios_terminada`, `historial_guardado` y `tmux_terminada`; (3) **DELIBERACIONES** guarda la escritura en varios sin columnas nuevas. Funcionalidades, con Claude Code primero: emulador en `magi-nucleo/src/terminal/` con respuestas a consultas solo desde el servidor, pegado entre corchetes de punta a punta, teclado de kitty con la bandera 1, salida sincronizada, ED 3, foco, avisos, título y ratón para el remoto que lo pide; historial de 10 000 líneas en el servidor con tope `historial_mib` y modo historial (búsqueda, selección, copia por `wl-copy`/`pbcopy` u OSC 52, guardar); portapapeles del remoto con permiso por host (`prefijo P`); enlaces con letras y túnel temporal a `localhost`; cinco disposiciones de paneles; escritura en varios con deliberación y concesión del servidor; reconexión automática sin diálogos; tmux con `terminal_tmux.sh`; renombrar y mover pestañas; aviso al pegar varias líneas. Interfaz: vista Sesión con paneles, marcas `↻`, `!`, `▸` y `»`, Sesiones con título y tmux, y el bloque TERMINAL en la ficha. Protocolo v9, sección `[terminal]` y versión 1.4.0; el móvil no cambia. Sigue el informe de la Fase 15 (consultas en §5.5, lógica en §7) y deja su guion de validación con Claude Code real para el desarrollador.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase15-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase15-implementacion.md`**
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
