# Prompt MAGI - Fase 14: Android, Segunda Vuelta

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3: la app Android pasa a ser el móvil de guardia, con Flota, Servicios, snippets con deliberación y Registro, con la lógica del núcleo y siempre lectora de la sincronización. Parte de los cierres de F11-F13 (con R125). Stack: **Rust** (`magi-nucleo`; `magi-movil` con UniFFI `=0.32.2`; russh 0.63, `russh-sftp` `=3.0.0` y `globset`) y **Kotlin + Jetpack Compose** (Material 3, DataStore y `repeatOnLifecycle`). Tablas, sin migración (`user_version` 9): (1) **HOSTS** (`servicios`, `snippet_al_conectar_id`), **VERIFICACIONES_HOST**, **SNIPPETS** con **SNIPPET_DESTINOS** y **COMPROBACIONES_SERVICIO**, solo lectura; (2) **SONDEOS** con `fallos_json`; (3) **DELIBERACIONES** con `usuario` «móvil (<equipo>)», nombre que sale de **EQUIPOS**, y `ejecucion_resultado`; (4) **REGISTRO** con `sondeo_fallido`, `snippet_ejecutado`, `deliberacion_*` y `servicio_accion`, más los campos propios `usado_veces` y `ultimo_uso_en`. Funcionalidades: mover al núcleo el sondeo (`scripts/sondeo.sh` con `<motores>`, lectura, estados y fallos) y las comprobaciones de la deliberación con proveedores, más `sftp_lectura.rs`, sin cambiar el escritorio; una conexión por host con gracia de 60 s y aperturas automáticas sin diálogos; Flota como inicio (sondeo al abrir, al deslizar y cada 30 s a la vista); Servicios como en la F13, ejecutado por `magi-movil` (Toda la flota, detalle, acciones con confirmación o deliberación, sudo en memoria 5 min, logs con anillo en Rust y ventanas, y sesiones con orden); snippets sincronizados en uno o varios hosts, con variables, resultados y snippet al conectar; MELCHIOR-1 y BALTHASAR-2 por SFTP, CASPER-3 que rechaza «solo en el escritorio», ejecutar manteniendo pulsado 1 s y forzar con motivo; Registro con filtros. Interfaz: barra FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS, dos paneles en el plegable y ajustes nuevos en DataStore. Acciones y ejecuciones siguen en segundo plano; sondeo, refrescos y logs, solo a la vista. Pruebas de extremo a extremo en Rust y en el emulador, `servidor_pruebas` en el job `humo` (R106) y app 0.2.0.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase14-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase14-implementacion.md`**
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
