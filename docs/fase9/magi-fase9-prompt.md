# Prompt MAGI - Fase 9: Empaquetado y Releases

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3 (TUI `magi` + `magi --servidor`), para convertirlo en un programa instalable y publicado como software libre. Stack: **Rust stable, ratatui 0.30, russh 0.63, rusqlite 0.40 y el resto fijado en `Cargo.toml`; nuevos `clap_complete`, `clap_mangen`, `dist`, `cargo-deny` y `gitleaks`**. Tablas: ninguna nueva (`magi.db` sigue en `user_version` 6). Funcionalidades: (1) paquete **`magi-ssh`** con `[lib] magi` y `[[bin]] magi`, versión **1.0.0**, licencia MIT OR Apache-2.0, `publish = false`, rutas de datos y socket siempre con «magi»; `magi --version` con protocolo y commit; LICENSE-MIT, LICENSE-APACHE, CHANGELOG (Keep a Changelog), SECURITY y `docs/publicar.md`. (2) **Protocolo v6**: `VersionIncompatible` gana `version_app`, `clientes` y `trabajo{sesiones, transferencias, tuneles, ejecuciones}`; `Hola` y `VersionIncompatible` son contrato estable; un servidor anterior sin trabajo ni otras ventanas se sustituye solo (SIGTERM, espera de 5 s, autolanzado); con trabajo o recuento desconocido, diálogo SERVIDOR DE SESIONES ANTERIOR (`p` con confirmación, `Esc`, `q`); nunca se para un servidor más nuevo. (3) Una `magi.db` de versión futura no se abre. (4) `magi diagnostico` (✓ ! ✕, ASCII, `NO_COLOR`, sin escribir ni conectar), `magi autocompletado <shell>`, `magi man`; `assets/` con autocompletado, man, `magi.desktop` e icono original, más un test de frescura. (5) `ci.yml`: fmt, clippy, tests Linux, macOS, `cargo deny` y plantilla AUR. (6) `dist`: Linux x86_64/aarch64 (Ubuntu 22.04) y macOS arm64/x86_64, SHA-256 y atestaciones, fórmula `magi-ssh` en `4d3-org/homebrew-tap`; las prereleases no tocan los canales. (7) AUR **`magi-ssh-bin`**: PKGBUILD, `.install` con aviso al actualizar y `publicar-aur.yml`. (8) Auditoría con gitleaks y datos reales antes de publicar. Hacer público el repositorio, crear secretos o cuentas y etiquetar versiones lo hace el desarrollador, no tú.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase9-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase9-implementacion.md`**
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
