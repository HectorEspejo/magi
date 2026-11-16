# Prompt MAGI - Fase 16: Sesiones Persistentes

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3 (workspace `magi-ssh` + `crates/magi-nucleo` + `crates/magi-movil`), con sesiones que sobreviven a MAGI y al equipo, partiendo de la Fase 15 cerrada con su prompt de reanudación. Stack: **Rust, ratatui 0.30 + crossterm 0.29, vt100 0.16, russh 0.63, rusqlite 0.40 y tmux 3.0 o posterior en el host**, sin dependencias nuevas. Tablas: (1) **SESIONES_PERSISTENTES** (migración 11, viaja) con `uuid`, `version`, `vector`, `host_id` (CASCADE), `nombre` único por host, `carpeta`, `comando`, `comando_reanudar`, `nota`, `origen`, `creada_por` y fechas; (2) **HOSTS** gana `sesiones_persistentes`, que viaja y sustituye a `tmux_persistente` (su valor se copia subiendo la versión); (3) **REGISTRO** gana `sesion_persistente_creada`, `sesion_persistente_reabierta`, `sesion_persistente_enganchada` y `sesion_persistente_borrada`. Funcionalidades: marcar al abrir (nombre, carpeta, comando y comando de reanudación) o todo un host; tmux propio `tmux -L magi -f /dev/null` con la configuración de MAGI (sin barra ni prefijo, ratón, teclas extendidas, portapapeles, título y campana); `terminal_tmux.sh` con `crear`, `enganchar` (suelta a los demás salvo al compartir y sale con 100 al ser soltado), `estado`, `vista`, `historial`, `renombrar` y `terminar`, siempre con `=<nombre>`; reconexión que nunca quita la sesión a otro equipo; vigilante en el servidor cada 60 s, sin diálogos, con `SesionesPersistentes{lista}` y atención por la campana; modo historial con `capture-pane`; vista previa; `modifyOtherKeys`; fichas en el `formato` 4, unidas por host y nombre; la app Android solo aprende a leer el formato 4. Interfaz: bloque «Persistentes» en la vista Sesiones, diálogo de nueva sesión, pestaña soltada, bloque SESIONES en la ficha del host, aviso en la barra global, `magi sesion <nombre>` y `magi sesiones`. Protocolo v10, `[persistentes]` en `config.toml` y versión 1.5.0. Empieza por la prueba de concepto de Mayús+Intro con tmux 3.4 y 3.5 (D338).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase16-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase16-implementacion.md`**
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
