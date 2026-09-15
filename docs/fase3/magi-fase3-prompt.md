# Prompt MAGI - Fase 3: Pestañas y Servidor de Sesiones

Continúa desarrollando **MAGI**, gestor SSH de terminal en Rust (ratatui 0.30, crossterm 0.29, tokio, russh 0.63, tui-term 0.3, vt100 0.16, rusqlite 0.40) con Fases 1 y 2 implementadas. Stack nuevo: **serde_json por líneas (`LinesCodec`), base64, `nix` (`setsid`, `flock`), `security-framework` (macOS)**. Sin tablas nuevas; `REGISTRO` gana los tipos `sesion_cerrada`, `sesion_reconectada`, `servidor_arrancado`, `servidor_detenido`, `servidor_caido`, `sondeo_recuperado`. Funcionalidades: proceso servidor `magi --servidor` (mismo binario) autolanzado por el primer cliente con `setsid`, socket Unix en `$XDG_RUNTIME_DIR/magi/servidor.sock` (dir 700) y lock con `flock`, saludo versionado (`VERSION_PROTOCOLO = 1`), apagado solo cuando no quedan sesiones ni clientes tras 10 s de gracia; el servidor custodia `conexion/` (salto, huellas, autenticación), `RegistroSesiones` a N, un parser vt100 por sesión y un pool `host_id → Arc<Handle>` que reutiliza conexión si `multiplexar` (gracia 30 s sin canales); diálogos (huella, frase, contraseña) reenviados solo al cliente solicitante con timeout 5 min; varias sesiones al mismo host (`host (n)`); `exit` elimina la pestaña; caída conserva la sesión con `Reconectar`; pestaña compartida entre ventanas con tamaño mínimo de los adjuntos y teclas sin arbitraje; `Ejecutar` para el sondeo sobre sesión viva; un escritor SQLite por proceso con `busy_timeout` 5 s (servidor solo REGISTRO y `ultimo_estado`); cliente con parser por pestaña, adjuntar al entrar en Sesión y desadjuntar al salir, detección de servidor caído con diálogo de relanzado; CLI `magi servidor estado|parar`, `magi conectar <host>`; `[terminal] comando` para ventana nueva. Interfaz: barra de pestañas siempre visible (`●` `◐` `✕`, `+ nueva`, `‹ ›`), prefijo `Ctrl+]` + `1`-`9`/`n`/`p`/`l`/`c`/`x`/`r`/`w`/`q`; vista Sesiones (lista) cuando no hay pestaña activa con `↵ n r x S`; pestaña caída con última pantalla en gris; relleno `░` cuando el remoto es menor; `●N` en Hosts y Flota; `q` sale sin confirmar. Incluye R12 (anotar solo transiciones del sondeo) y R13 (Keychain por API).

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase3-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase3-implementacion.md`**
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
