# Prompt MAGI - Fase 17: MAGI en Termux (reanudación)

Continúa desarrollando **MAGI**, el gestor SSH de terminal de Hector, en su Fase 17 (MAGI en Termux), en una rama nueva desde `main`, cuando la 9b esté fusionada y la candidata `v1.6.1-rc.1` probada. Estado: checklist en **66/71**; total del proyecto, 1432/1447. La F17 se fusionó antes que la 9b (PR #18, 1.6.0), y la 9b hizo la integración (`docs/manual/termux.md`, las referencias y los checks) y la publica en pruebas en la 1.6.1 (D425 y D426 de la 9b; D424 superada). Sin marcar: el script del `.deb`, `termux-release.yml`, el job `termux`, `termux_paquete` con un `.deb` real y la prueba de concepto en el plegable. Antes de empezar, copia a `docs/fase17/` el informe v1.3, el checklist y este prompt (el maestro y el `CLAUDE.md` llegaron con la 9b). Orden: (1) con los resultados de la prueba de concepto (P0-P9) y del guion de 13 pasos que te pase el desarrollador, fija las constantes `PROVISIONAL-POC` y quita la marca; si el binario no arranca en Termux, para; (2) comprueba qué hay ya en `main` de D418-D422 (el sondeo con el llavero cerrado, `refrescar()` en las operaciones de equipos, Termux:API en el servidor, los textos y los retoques) y haz lo que falte; (3) marca los cuatro ítems del paquete y la CI con lo que dieron la CI de la PR de la 9b y la candidata (el `.deb` de `dpkg-deb`, subido con su `.sha256` y su atestación, y el job `termux` en verde con `MAGI_EXIGIR_DEB=1`); lo que no se viera, pruébalo o déjalo pendiente; (4) si cambia código, versión 1.6.2 (PATCH: `FECHA_DE_VERSION`, la DA2 `10602` y las instantáneas `servidor_anterior_*`), con su línea en «Sin publicar» y lo que cambie para el usuario en `docs/manual/termux.md`; (5) el informe de implementación al día. Etiquetas y releases son del desarrollador. Commits sin firmar (`CLAUDE.md`).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase17-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase17-implementacion.md`**
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
