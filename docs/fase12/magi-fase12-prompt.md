# Prompt MAGI - Fase 12: Escritorio Remoto

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3 (workspace `magi-ssh`, `crates/magi-nucleo` y `crates/magi-movil`), con **escritorios remotos**: RDP, VNC y voxl. Stack: **Rust** (tokio, russh 0.63, rusqlite), **IronRDP** fijado con `=` (TLS con rustls, NLA con NTLM), cliente **RFB propio** (RFC 6143, `des`, `flate2`), **axum** 0.8 para el servidor del visor y el **visor web de voxl 0.3.3 adaptado** en `visor/` (TypeScript MIT, WebCodecs, esbuild a `visor/dist/`). Tablas: ninguna nueva; **migración 8** en **HOSTS**: `tipo` (ssh, rdp, vnc o voxl; fijo), `escritorio_dominio`, `escritorio_opciones` (JSON) y `huella_escritorio` (propia del equipo); «a través de» es `salto_host_id`, solo hacia hosts SSH; **REGISTRO** gana `escritorio_abierto`, `escritorio_cerrado`, `escritorio_fallido` y `huella_escritorio`. Funcionalidades: sesiones custodiadas por `magi --servidor` con pestaña ▣, `Escritorios{lista}` y 60 s de gracia sin visor; camino por SSH (`conexion_para_canal` y `direct-tcpip`) o directo (RDP; VNC con confirmación); visor solo en `127.0.0.1` con enlaces de un solo uso, clave en `sessionStorage`, `Host`, `Origin` y CSP; sin recomprimir ni x264 (rectángulos en bruto y AVC420 de Windows tal cual); certificado RDP con la política de las huellas SSH antes de las credenciales, que salen del llavero vía el cliente; teclado por scancodes (RDP) o keysyms (VNC); portapapeles de texto, resolución que se ajusta y pantalla completa; voxl arrancado por SSH con `escritorio_voxl.sh` (contraseña por stdin, `127.0.0.1`, `/healthz`, túnel y parada al cerrar); `formato` 2 de sincronización solo con escritorios, y la app Android los enseña sin abrirlos; protocolo v7, `[escritorio]` en `config.toml` y `magi diagnostico`. Interfaz: Hosts con `▣`, formulario por tipo, ficha, diálogos de certificado y credenciales, Sesiones con TIPO y visor en Chromium en modo app. Versión 1.2.0.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase12-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase12-implementacion.md`**
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
