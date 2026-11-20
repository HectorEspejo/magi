# Prompt MAGI - Fase 17: MAGI en Termux

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3, con la Fase 17 (MAGI en Termux), en la rama `fase17`, que parte de la F16 cerrada. Antes de empezar, copia la entrega a `docs/` (maestro) y `docs/fase17/`, y aplica al `CLAUDE.md` de la raíz los cambios del que llega, sin borrar sus reglas (`AGENTS.md` igual). Stack: el de siempre, también para `aarch64-linux-android` (API 24) con `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh`, solo con bionic; lo propio de Termux, en `src/termux/` (rutas del entorno, socket en `$TMPDIR/magi-<uid>/`, `$PREFIX/bin/sh` y órdenes `termux-*` sin shell y con plazo). Sin migración (`user_version` 11), protocolo v10 ni `formato` nuevo; versión 1.6.0; la app Android no se toca. Tablas: (1) **EQUIPOS**, el de Termux escritor con plataforma `android` y nombre `termux-<modelo>`; (2) **HOSTS**, con `contrasena:llavero` resuelto en el llavero nuevo; (3) **REGISTRO** con cinco tipos `llavero_*` y `termux_teclas_instaladas`. Funcionalidades: `LlaveroFichero` en el núcleo (`llavero.age`, age con una frase maestra que solo vive en la memoria del cliente), sincronización con la carpeta del almacenamiento compartido (`realpath` y sondeo cada 30 s), portapapeles con `termux-clipboard-set` u OSC 52 (6 000 bytes), enlaces y visor con `termux-open-url`, wake lock mientras haya `TrabajoServidor`, avisos de Android con Termux:API (solo nombre y host; el vigilante, también sin ventanas), fila de teclas en `termux.properties` con confirmación y copia, `[termux]` en la configuración, bloque TERMUX en `magi diagnostico`, `magi llavero` y `magi termux`. Interfaz: los diálogos del llavero y de la fila de teclas, con instantáneas a 48×20. Empaquetado: `scripts/paquete-termux.sh`, `termux-release.yml` (tag `vX.Y.Z`, solo `GITHUB_TOKEN`), job `termux` en CI, receta de TUR en `packaging/termux/` y `docs/publicar-termux.md`. Empieza por la prueba de concepto en el plegable (informe, §11): si el binario no arranca, para. Pruebas en Linux con `MAGI_PLATAFORMA=termux` y los dobles de `tests/fijos/termux/`. Commits sin firmar (`CLAUDE.md`).

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
