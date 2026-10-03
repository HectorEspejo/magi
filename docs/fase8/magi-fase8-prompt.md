# Prompt MAGI - Fase 8: Archivos, Segunda Vuelta

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, tokio, russh 0.63, russh-sftp 3.0.0, rusqlite 0.40; TUI `magi` + `magi --servidor` por socket Unix) con Fases 1-7 implementadas. Crates nuevos: **`ignore`** (gitignore) y **`sha2`**. Migración 6: tabla **SINCRONIZACIONES_DIR** (host_id FK cascada, nombre único por host, ruta_local, ruta_remota, direccion subida/bajada, borrar, exclusiones, ultima_ejecucion_en, ultimo_resultado, creado_en, actualizado_en); REGISTRO gana `sincronizacion` y `permisos_cambiados`. `VERSION_PROTOCOLO = 5`: `StatRemoto`/`Stat`, `CambiarPermisos` (alcance todo/directorios/ficheros), `ListarArbol`/`Arbol` en bloques de 1000 con `.magiignore`, `Hecho.detalle`; `Transferir` gana permisos por elemento, `borrar_al_terminar`, `deliberacion`, `sincronizacion_id` y `etiqueta`; `DirListado` y `Arbol` con usuario y grupo por nombre (el servidor lee `/etc/passwd` y `/etc/group` por SFTP). Funcionalidades: `E` edita un fichero remoto (temporal, `$EDITOR` sin shell con TUI suspendida, SHA-256 para detectar cambios, `StatRemoto` para conflicto con sobrescribir/copia local/descartar, aviso si cambia el propietario, subida con los permisos del original, el temporal nunca se borra si la subida no terminó); `p` cambia permisos con casillas rwx y octal sincronizados, estado mixto y recursivo con confirmación; `S` sincroniza el directorio del panel activo con el otro en cualquier dirección: plan recursivo en el cliente (crear, actualizar por tamaño/mtime ±2 s, borrar opcional, omitir enlaces a directorio), exclusiones por defecto + `.magiignore` + extras (nunca se borra lo excluido), vista previa, aviso de sensibles, deliberación MAGI si sube a host con verificaciones o borra, borrado solo tras éxito total; `L` gestiona sincronizaciones guardadas; paleta `sync · <host> · <nombre>`; `magi sincronizaciones`. Interfaz: diálogos PERMISOS, SINCRONIZAR, conflicto de edición, vista previa con `+ ~ − ·` y lista de guardadas, todos con mínimo, modo estrecho e instantáneas (T45). Cierra con revisión adversarial.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase8-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase8-implementacion.md`**
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
