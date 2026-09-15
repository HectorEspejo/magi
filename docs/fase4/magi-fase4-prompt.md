# Prompt MAGI - Fase 4: Archivos (SFTP en panel doble)

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, tokio, russh 0.63, rusqlite 0.40; TUI `magi` + `magi --servidor` por socket Unix con JSON por líneas) con Fases 1-3 implementadas. Stack nuevo: **`russh-sftp` (fijado, compatible con russh 0.63.3), `globset`, `filetime`**. Migración 3: **HOSTS.sftp_dir_local** y **HOSTS.sftp_dir_remoto**; sin tablas nuevas; `REGISTRO` gana `transferencia` y `borrado_remoto`. `VERSION_PROTOCOLO = 2` con mensajes `AbrirSftp`/`SftpAbierto`, `ListarDir`/`DirListado`, `Transferir`/`Transferencias`, `CancelarTransferencia`, `LimpiarTransferencias`, `BorrarRemoto`, `RenombrarRemoto`, `CrearDirRemoto`, `DescargarTemporal`/`RutaTemporal`, `BorrarTemporal`, `Hecho`; `Bienvenida` incluye la cola. Funcionalidades: todo lo remoto en el servidor (una `SftpSession` por host sobre la conexión del pool, abriendo conexión con diálogos al solicitante si no existe; cierre tras 10 min inactivo), cola de transferencias en el servidor con una en curso por host (FIFO), directorios recursivos, bloques de 64 KiB con peticiones en vuelo, parciales `.magi-parcial`, mtime conservado, bajadas escritas directamente en disco local, cancelación por bloque, política de conflicto por elemento aplicada a todo un directorio, terminadas conservadas 1 h, difusión `Transferencias{lista}` ≤ 4/s; cliente con panel local (`std::fs`) y remoto del mismo widget, marcas `≠` (tipo/tamaño/mtime ±2 s) y `✕` (no existe al otro lado), aviso configurable `[archivos] avisar` antes de subir sensibles (recorriendo directorios), diálogo de conflicto sobrescribir/omitir/todos con tamaño y fecha, copiar, mover (borrar origen tras `hecha`), borrar sin papelera con confirmación, renombrar, crear directorio, ver con `$PAGER` (remoto vía temporal 600 borrado al cerrar; >10 MB confirma), últimos directorios por host, filtro, ocultos, marcados con `Espacio`/`a`/`A`, detalle `i`, cambiar host `h`. Interfaz: `F4` (también `s` en Hosts/Flota, prefijo `f` en Sesión, paleta `sftp · <host>`), dos paneles con `Tab`, cola de 3 filas al pie y vista Transferencias con `t` (`x` cancelar, `C` limpiar, `↵` detalle), `magi servidor estado` con la cola.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase4-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase4-implementacion.md`**
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
