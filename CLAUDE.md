# CLAUDE.md — MAGI

## Contexto

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy) y macOS, con app Android prevista. Cliente: 4d3 (producto propio).
Stack: Rust stable, ratatui 0.30 + crossterm 0.29, tokio, russh 0.63 (`russh::keys`; no uses el crate `russh-keys`), tui-term 0.3 + vt100 0.16, rusqlite 0.40 (bundled, WAL), clap, toml + serde, directories, nucleo-matcher, zeroize, tracing + tracing-appender, chrono, unicode-normalization, anyhow + thiserror, serde_json, csv, ssh-key (fijado a la misma versión que trae russh), base64, nix (setsid/flock), security-framework (macOS), russh-sftp (`=3.0.0`, atado a russh 0.63), globset, filetime.
Desde la Fase 3 hay dos procesos: la TUI `magi` (cliente) y `magi --servidor` (custodia sesiones), comunicados por socket Unix con JSON por líneas.

## Documentación del proyecto

La documentación funcional vive en `docs/`, con una carpeta por fase:

- `docs/magi-maestro.md` — visión global, mapa de fases y estado del proyecto
- `docs/faseN/magi-faseN-informe.md` — especificación funcional completa de la fase
- `docs/faseN/magi-faseN-checklist.md` — alcance verificable de la fase
- `docs/faseN/magi-faseN-prompt.md` — prompt de arranque de la fase
- `docs/faseN/magi-faseN-prompt-continuacion.md` — prompt de reanudación (si la fase queda a medias)
- `docs/faseN/magi-faseN-implementacion.md` — informe de implementación (lo escribes tú)

El informe de fase manda sobre tu criterio: si algo te parece incorrecto o
incompleto, no lo cambies por tu cuenta — impleméntalo como está o párate y
coméntalo con el desarrollador.

## Reglas de trabajo

- Escribe siempre en castellano: respuestas, comentarios de código, mensajes de
  commit, documentación y textos de interfaz. Solo se mantienen en inglés los
  términos técnicos estándar (API, CRUD, endpoint, commit...).
- Tu effort por defecto es `ultracode`: una vez aprobado el plan que presentes,
  utiliza los workflows y todo lo disponible en ese effort.
- No añadas funcionalidades que no estén en el checklist de la fase. Si detectas
  algo necesario que falta, propónlo antes de implementarlo y déjalo anotado
  como desviación.
- Nomenclatura: tablas SQL en `MAYUSCULAS_SNAKE_CASE`, campos en
  `minusculas_snake_case`. Identificadores de Rust (módulos, funciones,
  variables) en castellano y `snake_case`; tipos en `CamelCase` en castellano.
- Sin `CHECK` sobre enumeraciones en SQLite: los valores se validan en código.
- Migraciones en `src/almacen/migraciones.rs`, numeradas y aplicadas por
  `PRAGMA user_version`; nunca modifiques una migración ya aplicada, añade otra.
- Nunca guardes claves privadas ni frases en disco ni en el log; en memoria
  usa `zeroize`.
- Toda operación de red va en una tarea tokio y comunica por el canal de
  eventos; la UI no bloquea nunca.
- Las versiones de `ratatui`, `crossterm`, `tui-term`, `vt100` y `russh` están
  fijadas en `Cargo.toml` y son compatibles entre sí: no subas una sin subir el
  conjunto y repetir las pruebas manuales de la vista Sesión.
- Comandos: `cargo run` arranca la TUI; `cargo run -- importar [ruta]` y
  `cargo run -- exportar` son los subcomandos; `cargo test` ejecuta los tests
  (el crate expone `src/lib.rs` para los de `tests/`);
  `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` antes de
  cada commit. Para pruebas aisladas usa `HOME` + `XDG_DATA_HOME` /
  `XDG_CONFIG_HOME` / `XDG_STATE_HOME`.
- Toda escritura en `~/.ssh` y en los ficheros de MAGI (`magi.db`,
  `magi_config`, `known_hosts`, `config.toml`) pasa por `src/ficheros.rs`:
  atómica, permisos 600 y copia con sufijo de fecha antes de tocar
  `~/.ssh/config` o `known_hosts`. No escribas en `~/.ssh` desde otro sitio.
- La UI nunca recibe bytes del remoto: el parser `vt100` vive en la tarea de
  conexión y notifica `Pantalla` coalescido a ~30 fps. Los diálogos que esperan
  al usuario (huella, frase) no tienen timeout; el de 10 s solo cubre DNS/TCP.
- Log: `~/.local/state/magi/logs/magi.log.<fecha>` (un fichero por día).
- Todo evento que deba quedar en el historial pasa por `registro::anotar`;
  nadie escribe en `REGISTRO` directamente y sus filas nunca se editan.
- Las operaciones automáticas (sondeo y las que vengan) nunca abren diálogos:
  reportan error con instrucción. Solo las conexiones interactivas aceptan
  huellas o piden frases.
- MAGI puede generar claves en `~/.ssh` (nunca sobrescribe) pero jamás borra
  ficheros de `~/.ssh` ni quita claves del agente.
- Todo lo que se ejecuta en un host remoto va en scripts embebidos con
  `include_str!` y recibe los datos del inventario como argumentos validados,
  nunca interpolados en el texto del comando.
- El bucle de UI es el único escritor de SQLite: las tareas de red emiten
  eventos y nunca abren la base de datos.
- Toda conexión viva está en `conexion::RegistroSesiones`; antes de abrir una
  conexión efímera, pide la existente ahí.
- Un secreto que no es una clave (contraseñas) solo vive en el llavero del
  sistema a través de `src/llavero.rs`; en `magi.db` van referencias, nunca
  secretos, tampoco cifrados.
- Anota en el registro cuando el efecto se ha producido (fichero escrito,
  comando ejecutado), no cuando se decide.
- Servidor de sesiones (`src/servidor/`): custodia solo sesiones, pool de
  conexiones y lo que deba sobrevivir a la ventana. No dialoga nunca: reenvía
  huellas, frases y contraseñas al cliente solicitante. Escribe en SQLite solo
  `REGISTRO` (eventos de sesión y servidor) y `HOSTS.ultimo_estado` /
  `ultima_conexion_en`; el resto lo escribe el cliente. Ambos abren la BD con
  `busy_timeout = 5000`.
- Protocolo en `src/protocolo.rs`, `VERSION_PROTOCOLO` explícita: si cambias
  la semántica de un mensaje existente, incrementa la versión. Un mensaje
  desconocido produce `Error` y desconexión, nunca un panic. Nunca mates un
  servidor con sesiones abiertas sin orden explícita del usuario.
- Secretos por el socket (`Frase`, `Contrasena`) van en `Zeroizing` y no se
  escriben en ningún log; `Datos`/`PantallaCompleta` solo a clientes adjuntos.
- Depuración del servidor: `cargo run -- --servidor` en primer plano en una
  terminal y `cargo run` en otra; log en `~/.local/state/magi/logs/servidor.log.<fecha>`.
- En el servidor las escrituras de SQLite van por el hilo escritor (`OrdenBd`);
  no abras conexiones de escritura desde tareas async. El servidor nunca toca
  el llavero: `FuenteContrasena::Solicitante`.
- El estado cliente-servidor se sincroniza solo por difusión de listas
  completas (`Sesiones{lista}`); no añadas mensajes de sincronización finos.
- Sanea todo tamaño de terminal que llegue de fuera (mín. 2×1) y calcula el
  alto del PTY con `alto_pty`; `vt100` entra en pánico con un 0.
- SFTP y transferencias (`src/servidor/sftp.rs`, `transferencias.rs`): todo lo
  remoto en el servidor sobre el pool; la cola es FIFO con una transferencia
  en curso por host; nunca preguntes desde el servidor: el cliente decide
  conflictos y avisos antes de `Transferir` y tú aplicas la política. Ninguna
  ruta pasa por un shell.
- Toda espera de red en el servidor lleva plazo (10 s DNS/TCP/saludos de
  subsistema, 60 s por bloque de transferencia); solo los diálogos con el
  usuario esperan 5 min. `request_subsystem` devuelve `Ok` aunque el host
  diga que no: envuélvelo en el plazo.
- Un diálogo lleva fijados los datos sobre los que actúa (rutas, ids) y no
  relee el estado al confirmar; descarta respuestas de peticiones que ya no
  están en vuelo. Libera cada recurso del pool por identidad y en todas las
  ramas de error.
- Antes de cerrar una fase, pasa una revisión adversarial sobre el código
  nuevo (servidor y cliente por separado) buscando pérdida de datos, fugas de
  recursos, esperas sin plazo y estado leído a destiempo; corrige lo que
  encuentres y añade test de regresión a lo que sea pérdida de datos. Anota
  el resultado en el informe de implementación.
- Túneles (`src/servidor/tuneles.rs`): el CRUD de `TUNELES` lo hace el cliente;
  el servidor solo lee la tabla y ejecuta. Un túnel activo cuenta como canal
  del pool pero no como pestaña ni SFTP para el ciclo automático. Todo lo que
  escuche en local va en `127.0.0.1` salvo aviso explícito; los canales
  `forwarded-tcpip` con clave no registrada se rechazan.
- Tests aislados: `directories` resuelve el home por uid, así que fija
  `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` y `XDG_RUNTIME_DIR`,
  no solo `HOME`.

## Al terminar una fase (o al pausar la sesión)

1. Busca el archivo markdown que tenga `checklist` en el nombre correspondiente
   a esa fase y marca `[x]` todo lo que se haya realizado. Actualiza los
   contadores de progreso del final del archivo. No reescribas el texto de las
   funcionalidades: es la clave de trazabilidad con el resto de documentos.
2. Genera o actualiza `magi-fase[N]-implementacion.md` con exactamente
   esta estructura:
   - Resumen de lo implementado
   - Desviaciones respecto a la especificación (qué y por qué)
   - Estructura de archivos creada/modificada
   - Decisiones técnicas tomadas durante el desarrollo
   - Funcionalidades del checklist completadas (copiando su texto exacto)
   - Pendientes y bloqueos
   - Ejecución y pruebas (cómo arrancar, migrar y testear)

   Es un documento vivo: actualízalo de forma incremental en cada sesión, nunca
   lo regeneres desde cero. Sé honesto en las desviaciones y en los pendientes;
   ese informe es lo único que el analista funcional verá de lo que realmente
   pasó, y con él se actualiza la especificación.
3. Recuérdale al desarrollador que debe entregar el informe de implementación al
   analista funcional antes de especificar la siguiente fase.
4. Invita al desarrollador a hacer PR. Si te dice que lo hagas, usa el comando
   `gh`.

## Commits

- NUNCA firmes ni te atribuyas los commits: la autoría es del usuario.
- Sin `Co-Authored-By`, sin líneas de "Generated with", sin menciones a Claude
  en el mensaje.
- Mensajes en castellano, en imperativo y concisos.
