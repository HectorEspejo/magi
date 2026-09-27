# Prompt MAGI - Fase 7: Redimensionado Adaptable

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, crossterm 0.29, tokio, russh 0.63, rusqlite 0.40; TUI `magi` + `magi --servidor` por socket Unix) con Fases 1-6 implementadas. Hector ha detectado dos fallos al cambiar el tamaño de la terminal: la interfaz no se repinta (o se queda con el tamaño viejo hasta pulsar una tecla) y en una pestaña el remoto no recibe el tamaño nuevo (`vim`/`htop` no se ajustan). Es una regresión de lo validado en la Fase 1. Stack nuevo solo en desarrollo: **`insta`** con `ratatui::backend::TestBackend`. Sin tablas ni mensajes de protocolo nuevos. Funcionalidades: primero reproducir ambos fallos con tests y documentar la causa; tubería única de redimensionado (`Event::Resize` nunca descartado, `Geometria` en `app.rs` con agrupación de 50 ms y tick ≤ 16 ms mientras hay pendiente, `aplicar_tamano` con `resize` + `clear` + recálculo + repintado, también al volver del visor y al readjuntar tras relanzar el servidor); en Sesión, `Redimensionar` de la pestaña adjunta con `alto_pty` del tamaño aplicado en cada cambio; regla de la F3 del mínimo entre ventanas recalculada en cada `Redimensionar`, `Adjuntar` y `Desadjuntar`; `ui/disposicion.rs` con puntos de corte (estrecho < 100 columnas, columnas mínimas < 80, bajo < 20 filas), mínimos por vista y aviso centrado por debajo del mínimo (global 40×12); ninguna vista guarda tamaños entre pintados; paneles dobles a uno con `Tab` en estrecho (Flota, Archivos), columnas por prioridad, detalles plegados en vistas bajas, barras compactas con `? más`, diálogos, paleta, ayuda y diálogo MAGI que se recolocan y encogen con desplazamiento; selección, filtro, marcados y texto conservados al cambiar de modo. Pruebas: `TestBackend` e instantáneas a 40×12, 80×24 y 200×60 en todas las vistas, secuencias de cambio, y `window_change` de extremo a extremo con una y dos ventanas contra el servidor SSH de pruebas. Cierra con revisión adversarial.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase7-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase7-implementacion.md`**
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
