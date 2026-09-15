# Prompt MAGI - Fase 1: Inventario y Conexión

Desarrolla **MAGI**, gestor SSH de terminal (TUI) con estética de cabina técnica para Linux (Omarchy) y macOS, sin cuenta ni suscripción. Stack: **Rust stable, ratatui + crossterm, tokio, russh + russh-keys, tui-term + vt100, rusqlite bundled en WAL con migraciones por `PRAGMA user_version`, clap, toml + serde, directories, nucleo-matcher, zeroize, tracing**. Tablas: (1) **GRUPOS** con id, nombre único, orden, plegado, creado_en; (2) **HOSTS** con id, nombre único, grupo_id, direccion, puerto, usuario, identidad_ref (`auto`/`agente:SHA256:…`/`fichero:ruta`), salto_host_id (autorreferencia, máx. 3 niveles), multiplexar, keepalive_seg, opciones_extra (ssh_config literal), origen, ultimo_estado, ultima_conexion_en, creado_en, actualizado_en; (3) **ETIQUETAS** con id, nombre único; (4) **HOST_ETIQUETAS** con host_id, etiqueta_id. Funcionalidades: inventario agrupado con filtro incremental por subcadena sobre nombre/dirección/usuario/grupo/etiquetas, grupos plegables persistentes, etiquetas con autocompletado, ficha de host en cuatro bloques con validación, prueba de conexión `Ctrl+T`, identidades leídas de `~/.ssh/*.pub` y del agente (`SSH_AUTH_SOCK`), importación sin pérdida de `~/.ssh/config` (solo `Host` sin comodines, resto a opciones_extra, diálogo de conflictos y resumen), exportación atómica a `~/.ssh/magi_config` con inserción opcional del `Include`, conexión embebida con russh (agente, clave de fichero con frase en diálogo, keepalive, salto en cadena), huellas estrictas contra `~/.ssh/known_hosts` (desconocida: aceptar; cambiada: bloqueo con escribir el nombre del host), una sola sesión a la vez con estado en_segundo_plano, paleta `Ctrl+P` difusa sobre hosts y acciones, CLI `magi importar` y `magi exportar`. Interfaz: vista Hosts (F2) con glifos `●○✕` + color, barra inferior de atajos, ficha, vista Sesión (F3) a pantalla completa para el PTY remoto con prefijo `Ctrl+]` configurable (`q` fondo, `x` cerrar), diálogos de huella/frase/confirmación, ayuda `?`, tema de `~/.config/omarchy/current/theme/alacritty.toml` con paleta fija de respaldo y modo ASCII degradado; UI sin bloqueos (tareas tokio + canal mpsc); terminal restaurado ante panic. Ficheros 600 con escritura atómica; sin telemetría; frases con zeroize; sin `CHECK` en enumeraciones. Tests de almacén e ida y vuelta importar/exportar; CI en GitHub Actions.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase1-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase1-implementacion.md`**
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
