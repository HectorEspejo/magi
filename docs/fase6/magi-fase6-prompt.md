# Prompt MAGI - Fase 6: Snippets y Deliberación MAGI

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, tokio, russh 0.63, russh-sftp 3.0.0, rusqlite 0.40; TUI `magi` + `magi --servidor` por socket Unix) con Fases 1-5 implementadas. Crate nuevo: **`shell-escape`**. **Primero la corrección 3b**: `conexion_para_canal` único con cerrojo por host para pestañas, SFTP, túneles, `Ejecutar` y ejecuciones; pestañas que reutilizan el pool con `multiplexar`; `Pool::guardar` nunca desplaza una conexión viva; pid del servidor antiguo (`SO_PEERCRED`) y `SIGTERM` en `servidor parar`; `SIGTERM` = `Parar`; `Hecho` tras confirmar la escritura en REGISTRO. Migración 5: **SNIPPETS** (nombre único, comando con `{{var}}`/`{{var:defecto}}`, descripcion, etiquetas, critico, timeout_seg, parar_al_fallo, usado_veces, ultimo_uso_en), **SNIPPET_DESTINOS** (snippet_id, etiqueta por nombre o host_id), **VERIFICACIONES_HOST** (host_id, salud, backup, backup_ruta, backup_patron, tests, tests_comando), **DELIBERACIONES** (fecha, snippet_id, accion, hosts_json, comprobaciones_json, resultado, bloqueada, motivo, usuario, ejecucion_resultado) y **HOSTS.snippet_al_conectar_id**; REGISTRO gana `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`. `VERSION_PROTOCOLO = 4`: `LanzarEjecucion`, `CancelarEjecucion`, `LimpiarEjecuciones`, `PedirSalida`/`Salida`, `Ejecuciones{lista}`, `AbrirSesion.comando_inicial`. Funcionalidades: el cliente resuelve destinos y variables (escapadas) y delibera; el servidor ejecuta el comando resuelto por `exec` en hasta 8 hosts, no interactivo, con timeout, parar al primer fallo, cancelación, salida 1 MiB por flujo en memoria 1 h (nunca en REGISTRO), anotando al terminar. Deliberación obligatoria si crítico, más de un host o host con verificaciones: MELCHIOR-1 salud (sondeo NOMINAL < 5 min), BALTHASAR-2 backup (`ListarDir`, fichero < 24 h), CASPER-3 tests (`sh -c` local, código 0), 2 s cada una en paralelo; unanimidad; `Ctrl+K` ejecuta (`↵` no); `f` fuerza con motivo ≥ 10. Interfaz: vista `F8` Snippets (`↵ a p n e x / t`), diálogo EJECUTAR, diálogo MAGI host × comprobación, Resultados (`Tab ↵ s r x C`), bloques de ficha «Snippet al conectar» y «Verificaciones previas», `!` en Hosts/Flota, `[flota.atajos]`, paleta, `magi snippets`. Cierra con revisión adversarial.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase6-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase6-implementacion.md`**
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
