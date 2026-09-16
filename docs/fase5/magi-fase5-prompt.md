# Prompt MAGI - Fase 5: Túneles

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, tokio, russh 0.63, russh-sftp 3.0.0, rusqlite 0.40; TUI `magi` + `magi --servidor` por socket Unix con JSON por líneas) con Fases 1-4 implementadas. Sin crates nuevos. Migración 4: tabla **TUNELES** (id, host_id FK cascada, nombre único por host, tipo local/remoto/dinamico, escucha `dirección:puerto`, destino nulo en dinámico, automatico, creado_en, actualizado_en); `REGISTRO` gana `tunel_abierto`, `tunel_cerrado`, `tunel_fallido`. `VERSION_PROTOCOLO = 3` con `ActivarTunel`, `PararTunel`, `RelanzarTunel`, `RecargarTuneles`, `Tuneles{lista}` (≤ 2/s) y túneles en `Bienvenida`. Funcionalidades: CRUD en el cliente (T18) y ejecución en el servidor sobre la conexión del pool (el túnel cuenta como canal, se libera por identidad); local con `TcpListener` + `direct-tcpip` + `copy_bidirectional` 64 KiB; dinámico con SOCKS5 propio (solo CONNECT, sin auth, dominio sin resolver en local); remoto con `tcpip_forward` y handler `forwarded-tcpip` que solo acepta claves registradas y conecta al destino local; estados activando/activo/parando/caido con `tunel_fallido` en bind ocupado o rechazo del host, caída de conexión a `caido` sin reintento automático, relanzar con `r`; automáticos ligados al primer y último canal de pestaña o SFTP del host (los manuales no se paran); contadores de bytes y conexiones; conversión `LocalForward`/`RemoteForward`/`DynamicForward` ⇄ TUNELES en importación, exportación y desde `opciones_extra` (que pasa a rechazarlas); validación de escucha con aviso en `0.0.0.0`/`::` y puerto < 1024 remoto; CLI `magi tunel activar|parar <host> <nombre>` y `magi tuneles`; `servidor estado` con túneles y sin apagado con túneles activos. Interfaz: vista `F6` con tabla estado · tipo · escucha → destino · host · auto, `Espacio` activar/parar/descartar, `n e x a r ↵ /`, diálogo de túnel, bloque «Túneles» en la ficha con `i importar a túneles`, `⇅` en Hosts y Flota, «túneles N» en la barra de Sesión, entradas de paleta. Cierra con revisión adversarial (T31).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase5-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase5-implementacion.md`**
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
