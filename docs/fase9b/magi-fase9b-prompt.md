# Prompt MAGI - Fase 9b: Primera Publicación

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3, con la Fase 9b (Primera publicación), en la rama `fase9b` desde `main` (F16 fusionada, versión 1.5.0); la F17 avanza en paralelo y se fusionará después. Antes de empezar, copia la entrega a `docs/` (maestro) y `docs/fase9b/`, y aplica al `CLAUDE.md` de la raíz los cambios del que llega que son de la 9b (los de la F17 llegan con su rama), sin borrar sus reglas (`AGENTS.md` igual). Stack: el de siempre, sin dependencias, tablas ni migración nuevas (`user_version` 11, protocolo v10, `formato` 4). Tablas que toca la 10b: (1) **EQUIPOS**: con la clave anterior solo abren los conocidos y no quitados; (2) las que viajan (**HOSTS**, **TUNELES**…): cambio y versión en una transacción. S1, correcciones: el cierre de la F16 (§11 de su informe v1.2: script por la entrada estándar y `terminal_enganche.sh`, carrera de la reconexión, `no_existe` y `creada` opcionales en v10, D368, D370, D367, R159, R160, etiquetas y tmux real) y la 10b (R83: `magi sincronizar` pregunta al servidor antes de borrar; R84; R85: solo «ninguna identidad coincide» pide clave; R97). S2, publicación: referencias de `4d3-org` a `HectorEspejo` (tap `HectorEspejo/homebrew-tap`, `hola@4d3.org` en AUR), sin rutas personales, `gitleaks` una vez y la prueba `repositorio_publico` en CI; README corto y `README.en.md` con la animación de VHS (`assets/demo/`, contra `servidor_pruebas`), el manual movido sin reescribir a `docs/manual/` con `estado.md`, `docs/README.md`, `CONTRIBUTING` en los dos idiomas, plantillas y `SECURITY.md`; CHANGELOG con lo no publicado junto en `[1.5.1]` (app, `[0.2.0]`) y resumen bilingüe; versión 1.5.1; `docs/publicar.md` §4 y `docs/publicar-android.md` con los pasos del desarrollador, los ajustes del repositorio y los guiones cortos de macOS y de la app. No cambias ajustes, no creas secretos ni empujas etiquetas: tu informe lo deja listo. Revisión adversarial al cerrar. Commits sin firmar (`CLAUDE.md`).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase9b-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase9b-implementacion.md`**
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
