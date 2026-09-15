# Prompt MAGI - Fase 2: Flota, Identidades y Registro

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust cuya Fase 1 (inventario, ficha, ssh_config, conexión embebida, paleta) está validada. Stack sin cambios: **Rust stable, ratatui 0.30, crossterm 0.29, tokio, russh 0.63 (`russh::keys`, `ssh-key` para generar claves, `AgentClient::add_identity`), rusqlite 0.40, serde_json, csv**. Migración 2: columna **HOSTS.servicios** (unidades systemd, una por línea); tabla **SONDEOS** con host_id, fecha, resultado (ok/sin_metricas/error), error, nucleos, carga_1m/5m/15m, mem_total_kb, mem_disponible_kb, disco_total_kb, disco_usado_kb, red_rx_bytes, red_tx_bytes, uptime_seg, servicios_json, duracion_ms (20 últimos por host); tabla **IDENTIDADES** con alias único, tipo, huella única, origen (agente/fichero/token), ruta, comentario, anadida_en, ultimo_uso_en, revocada_en; tabla **REGISTRO** con fecha, tipo, host_id, identidad_id, detalle, resultado. Funcionalidades: vista Flota (F1, vista de arranque) con sondeo bajo demanda mediante un único script `sh` embebido ejecutado por canal `exec` (sesión viva si existe, si no conexión efímera), semáforo de 8 y timeout de 5 s, sin diálogos (huella o frase pendientes = error), estados FRÍA/CAÍDA/ALCANZABLE/CARGA/NOMINAL con umbrales globales en `config.toml`, tasa de red por diferencia con el sondeo anterior, auto-refresco opcional; campo servicios en la ficha; `registro::anotar` como única puerta al registro, con los eventos de Fase 1 (conexiones, huellas, importación/exportación) reconectados; vista Registro (F7) con filtro, tipo, detalle, purga > 90 días y exportación CSV/JSON; vista Identidades (F5) con sincronización por huella desde el escaneo, generar ed25519/rsa 4096 con frase y alta en el agente, importar, copiar pública (`wl-copy`/`pbcopy` con fallback), alias, revocar/reactivar (baja lógica: pone hosts en auto, nunca borra ficheros ni agente), último uso desde la autenticación; CLI `magi sondear [host…]` y `magi registro exportar <ruta> [--json] [--desde]`; paleta con entradas nuevas. Interfaz: Flota con lista glifo+palabra y panel de barras `█░` (ASCII `#.`) más servicios; Identidades y Registro con tabla+detalle; diálogo de generación; barra de Sesión con la carga reciente. Unidades validadas con `[A-Za-z0-9@._-]+` y pasadas como argumentos; sondeo nunca toca `ultimo_estado`; sin `CHECK` en enumeraciones.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase2-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase2-implementacion.md`**
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
