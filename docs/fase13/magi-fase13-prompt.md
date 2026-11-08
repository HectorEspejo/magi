# Prompt MAGI - Fase 13: Servicios, Contenedores y Logs

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3 (workspace `magi-ssh`, `crates/magi-nucleo` y `crates/magi-movil`), con una vista para **servicios, contenedores y logs** de cada host. Stack: **Rust** (tokio, russh 0.63, rusqlite, serde_json), script embebido `crates/magi-nucleo/scripts/servicios.sh` y, en los hosts, **systemd** 230+ (`systemctl`, `journalctl`), **Docker** o **Podman** con **Compose** v2, `sudo` y `timeout`. Tablas: (1) **COMPROBACIONES_SERVICIO** (migración 9) con `host_id`, `objetivo_tipo` (unidad o proyecto), `objetivo`, `comando`, `actualizado_en`, `uuid`, `version` y `vector`; viaja como elemento `comprobacion_servicio` y sube el `formato` a 3 solo si hay alguna; (2) **SONDEOS** gana `fallos_json`; **REGISTRO** gana `servicio_accion`; **DELIBERACIONES** guarda las acciones con `PlanDeliberado::Servicio`. Funcionalidades: vista `F9` (y `u` desde Flota, Hosts y la ficha) con hosts SSH, «Toda la flota» y secciones Servicios, Timers y Contenedores; vigilados y fallidos arriba; detalle con estado, dependencias y unidad; `A P R G H` (arrancar, parar, reiniciar, recargar, arranque) con espera al estado final, segunda mirada a los 2 s y últimas líneas del diario si falla; lanzar timers; contenedores por proyecto con consumo, logs, acciones y shell (`S`) en una pestaña con orden; deliberación para lo que deja algo parado (MELCHIOR-1 sin contar el objetivo) y confirmación para lo demás; sudo orden a orden (`sudo -n`, o contraseña comprobada con `ComprobarSudo`, por la entrada estándar y en memoria 5 min, nunca guardada); comprobación de configuración dentro de la acción (Nginx, Caddy, Apache, HAProxy, Compose o propia); logs en vivo con lotes y contrapresión, anillo de 10 000 líneas, filtros, pausa y `t` a pestaña; sondeo con fallos y `⚠N` en Flota; acciones custodiadas (`AccionesServicio{lista}`, `TrabajoServidor`); protocolo v8 (`peticion_id` desde 2^45) y `[servicios]` en `config.toml`. Interfaz: modo estrecho e instantáneas. La app Android lee el `formato` 3. Versión 1.3.0.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase13-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase13-implementacion.md`**
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
