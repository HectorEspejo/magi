# MAGI - Informe Maestro

**Última actualización:** 15 de septiembre de 2026 (Fase 3 especificada; roadmap reordenado)

## 1. Visión Global

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy / Hyprland / Alacritty) y compatible con macOS, con una app Android prevista al final del roadmap. Cubre el terreno funcional de Termius —inventario de hosts, identidades, sesiones, SFTP, túneles y sincronización entre dispositivos— sin cuenta en un servicio de terceros ni suscripción: la sincronización se hará con un fichero cifrado sobre el transporte que el usuario ya tenga, y las claves privadas nunca salen del agente, el llavero o el token físico.

El diseño de interfaz (once pantallas, sistema visual, modelo de teclado) viene definido en el informe ejecutivo de diseño v0.1 de 14 de septiembre de 2026 y las fases lo implementan sin reinterpretarlo. La propuesta de valor es que el gestor viva en la terminal, se maneje sin ratón y siga estando en el móvil cuando algo se cae a las once de la noche.

**Cliente y contexto.** Producto propio de 4d3 para uso diario de Hector; sin cliente externo. Repositorio `magi` en GitHub. Implementación con Claude Code.

## 2. Mapa de Módulos

```
  ┌───────────────────────────────────────────────────────────────────────┐
  │ F1  INVENTARIO Y CONEXIÓN  (completada)                                │
  │  hosts · grupos · etiquetas · ficha · ssh_config ⇄ magi_config        │
  │  cliente russh · huellas · sesión única · paleta                      │
  └──────────┬────────────────────────────────────────────────────────────┘
             ▼
  ┌───────────────────────────────────────────────────────────────────────┐
  │ F2  FLOTA, IDENTIDADES Y REGISTRO  (implementada, sin validar)        │
  │  sondeo bajo demanda · IDENTIDADES · REGISTRO · contraseña + llavero  │
  └──────────┬────────────────────────────────────────────────────────────┘
             ▼
  ┌───────────────────────────────────────────────────────────────────────┐
  │ F3  PESTAÑAS Y SERVIDOR DE SESIONES  (especificada)                   │
  │  magi --servidor · protocolo · pool de conexiones · pestañas ·        │
  │  varias ventanas · reconexión                                         │
  └──────┬───────────────────────┬───────────────────────┬────────────────┘
         ▼                       ▼                       ▼
  ┌──────────────────┐  ┌──────────────────┐  ┌────────────────────────┐
  │ F4 ARCHIVOS      │  │ F5 TÚNELES       │  │ F6 SNIPPETS Y          │
  │ SFTP panel doble │  │ local · remoto · │  │    DELIBERACIÓN MAGI   │
  │ sobre el pool    │  │ dinámico · auto  │  │ etiquetas · masivo ·   │
  │                  │  │ en el servidor   │  │ 3 comprobaciones       │
  └────────┬─────────┘  └────────┬─────────┘  └───────────┬────────────┘
           └───────────────────┬─┴─────────────────────────┘
                               ▼
                    ┌────────────────────────┐
                    │ F7 SINCRONIZACIÓN      │  cifrado en reposo · desbloqueo · fichero cifrado
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F8 ANDROID             │  app nativa, mismo modelo, sin servidor
                    └────────────────────────┘

  F4, F5 y F6 dependen de F3 (usan el servidor y el pool) y pueden reordenarse entre sí.
  F6 usa REGISTRO y el sondeo de F2. F7 requiere el modelo completo. F8 requiere F7.
```

## 3. Tabla Resumen de Fases

| Fase | Módulo | Funcionalidades | Sprints | Semanas | Estado |
|------|--------|-----------------|---------|---------|--------|
| 1 | Inventario y Conexión | 128 | 4 | 5 | Completada (126/128; validada por Hector en Omarchy el 15 sep 2026; quedan CI y macOS como pendientes menores) |
| 2 | Flota, Identidades y Registro | 84 (79 + anexo 5) | 3 | 3,5 | Implementación reportada (84/84; pendiente validación manual de Hector) |
| 3 | Pestañas y Servidor de Sesiones | 79 | 3 | 4 | Especificada (sobre F2 sin validar, R15) |
| 4 | SFTP en panel doble | ~60 (estimado) | — | — | Pendiente de especificar (decidida como siguiente) |
| 5 | Túneles | ~40 (estimado) | — | — | Pendiente de especificar |
| 6 | Snippets y deliberación MAGI | ~70 (estimado) | — | — | Pendiente de especificar |
| 7 | Sincronización cifrada | ~40 (estimado) | — | — | Pendiente de especificar |
| 8 | App Android | ~80 (estimado) | — | — | Pendiente de especificar |
| **Total** | | **291 especificadas (~580 estimadas)** | **10** | **~12,5 semanas (F1-F3)** | |

Las estimaciones de fases no especificadas son orientativas y se sustituirán por el recuento real de cada checklist.

## 4. Modelo de Datos Global

```
 ══ F1 ═══════════════════════════════════════════════════════════════════════
 ┌──────────────┐        ┌─────────────────────────┐        ┌──────────────┐
 │   GRUPOS     │───────<│         HOSTS           │>──────<│  ETIQUETAS   │
 │ id nombre    │        │ id nombre grupo_id      │  vía   │ id nombre    │
 │ orden plegado│   ┌───<│ direccion puerto usuario│ HOST_  └──────────────┘
 └──────────────┘   │    │ identidad_ref           │ ETIQUETAS
                    └────│ salto_host_id           │
                         │ multiplexar keepalive_seg│
                         │ opciones_extra origen   │
                         │ ultimo_estado           │
                         │ ultima_conexion_en      │
                         └───┬────────┬────────┬───┘
 ══ F2 ══════════════════════│════════│════════│═════════════════════════════
 HOSTS + servicios (F2)      │        │        │
 ┌──────────────────────┐    │        │        │      ┌────────────────────────┐
 │ IDENTIDADES          │◀ referencia│        └─────<│ REGISTRO               │
 │ id alias tipo huella │  por huella│               │ id fecha tipo host_id  │
 │ origen ruta          │  o ruta    │               │ identidad_id detalle   │
 │ comentario anadida_en│  (F1)      │               │ resultado              │
 │ ultimo_uso_en        │<───────────┼───────────────│ (identidad_id)         │
 │ revocada_en          │            │               └────────────────────────┘
 └──────────────────────┘            │
 ┌──────────────────────┐            │
 │ SONDEOS              │<───────────┘
 │ id host_id fecha     │
 │ resultado error      │
 │ nucleos carga_*      │
 │ mem_* disco_* red_*  │
 │ uptime_seg           │
 │ servicios_json       │
 │ duracion_ms          │
 └──────────────────────┘
 ══ F3 ═══════════════════════════════│═════════════════════════════════════
 (sin tablas: sesiones, pool y pestañas viven en memoria del servidor; REGISTRO gana tipos)
 ══ F5 ═══════════════════════════════│═════════════════════════════════════
 ┌──────────────────────┐             │
 │ TUNELES              │<────────────┤
 │ id host_id tipo      │             │
 │ local remoto         │             │
 │ automatico           │             │
 └──────────────────────┘             │
 ══ F4 (SFTP): sin tablas previstas; marcadores y transferencias en memoria ═══
 ══ F6 ═══════════════════════════════│═════════════════════════════════════
 ┌──────────────────────┐  ┌──────────┴───────────┐  ┌──────────────────────┐
 │ SNIPPETS             │  │ VERIFICACIONES_HOST  │  │ DELIBERACIONES       │
 │ id nombre comando    │  │ host_id salud backup │  │ id accion host_id    │
 │ critico              │  │ tests ventana        │  │ resultado forzada    │
 └──────────┬───────────┘  └──────────────────────┘  │ motivo fecha         │
            │ SNIPPET_ETIQUETAS                       └──────────────────────┘
            └──────────────> ETIQUETAS
 ══ F7 ═════════════════════════════════════════════════════════════════════
 ┌──────────────────────┐
 │ SINCRONIZACION       │  (metadatos: dispositivo, versión, última exportación)
 └──────────────────────┘
 ══ F8 ═════════════════════════════════════════════════════════════════════
 Android reutiliza el esquema F1-F7 sin tablas nuevas.
```

Las tablas de F5 en adelante son previsiones y se confirman al especificar cada fase.

## 5. Glosario de Dominio

| Término de negocio | Entidad/Tabla | Fase | Notas |
|---|---|---|---|
| Host | HOSTS | 1 | Equipo remoto; `nombre` es el alias `Host` de ssh_config |
| Grupo | GRUPOS | 1 | Carpeta plegable del inventario; un host tiene como mucho un grupo |
| Etiqueta | ETIQUETAS / HOST_ETIQUETAS | 1 | Clasificación libre N:N; destino de snippets en F4 |
| Salto | HOSTS.salto_host_id | 1 | `ProxyJump`; autorreferencia |
| Identidad | HOSTS.identidad_ref (F1) / IDENTIDADES (F2) | 1 / 2 | Referencia a clave del agente, fichero o token, o marca `contrasena` / `contrasena:llavero`; nunca el secreto. Revocar = baja lógica de la referencia |
| Llavero | `src/llavero.rs` (secret-tool / Keychain) | 2 | Único lugar donde vive una contraseña de host; `magi.db` solo guarda la referencia |
| Sesión | en memoria (F1) → servidor (F3) | 1 / 3 | Conexión SSH interactiva con su pty y su pantalla; desde F3 vive en `magi --servidor` y puede estar adjunta a varias ventanas |
| Pestaña | vista Sesión (F3) | 3 | Representación de una sesión en una ventana; nombre `<host>` o `<host> (n)` |
| Servidor | `magi --servidor` (F3) | 3 | Proceso local que custodia sesiones y el pool de conexiones; socket en `$XDG_RUNTIME_DIR/magi/` |
| Cliente / ventana | TUI `magi` (F3) | 3 | Cada terminal con `magi` abierto; adjunta pestañas y hace todo lo que no es sesión |
| Solicitante | servidor (F3) | 3 | Cliente que pidió abrir o reconectar una sesión; único que recibe sus diálogos |
| Pool de conexiones | servidor (F3) | 3 | `host_id → Arc<Handle>`; compartido cuando el host tiene `multiplexar` |
| Huella | `~/.ssh/known_hosts` | 1 | Sin tabla propia; se comparte con el ssh del sistema |
| Opciones extra | HOSTS.opciones_extra | 1 | Directivas ssh_config literales no modeladas |
| magi_config | `~/.ssh/magi_config` | 1 | Exportación del inventario incluida por `Include` |
| Flota | vista F1 sobre HOSTS + SONDEOS | 2 | Panel de estado; vista de arranque |
| Sondeo | SONDEOS | 2 | Resultado de ejecutar `sondeo.sh` en un host; 20 por host |
| Servicios | HOSTS.servicios | 2 | Unidades systemd que vigila el sondeo |
| Estado de flota | derivado de SONDEOS | 2 | FRÍA / CAÍDA / ALCANZABLE / CARGA / NOMINAL según umbrales de `config.toml` |
| Registro | REGISTRO | 2 | Historial de conexiones, huellas, importación/exportación, claves, sondeos y (F3) sesiones y servidor; en F6 también forzados |
| Túnel | TUNELES | 5 | Local, remoto o dinámico; vive en el servidor |
| Snippet | SNIPPETS | 6 | Comando dirigido a etiquetas |
| Deliberación MAGI | DELIBERACIONES + VERIFICACIONES_HOST | 6 | Confirmación de acciones críticas con tres comprobaciones |

## 6. Estado del Proyecto

| Fase | Progreso | Último informe de implementación procesado |
|---|---|---|
| 1 — Inventario y Conexión | 126 / 128 | 15 sep 2026 (implementada en una sesión de Claude Code; 22 tests verdes; pendientes: CI en GitHub Actions —descartado por Hector hasta tener remoto— y verificación en macOS —sin equipo—; validada manualmente por Hector con hosts reales el 15 sep 2026: «funcionando todo») |

| 2 — Flota, Identidades y Registro | 84 / 84 | 15 sep 2026 (una sesión de Claude Code; 56 tests verdes; incluye el anexo de contraseña aprobado por Hector; pendiente validación manual con hosts reales, en especial contraseña contra un `sshd` real, RSA 4096, portapapeles en Wayland y revocación) |

| 3 — Pestañas y Servidor de Sesiones | 0 / 79 | ninguno (sin confirmar) |

**Progreso total acumulado:** 210 / 291 funcionalidades.

La Fase 1 se da por completada: las dos funcionalidades sin marcar (CI y macOS) siguen abiertas en su checklist y se cierran cuando exista el remoto y un mac; no bloquean la Fase 2.

## 7. Roadmap

Reordenado el 15 sep 2026 tras elegir Hector la propuesta «solo pestañas» (ampliada a servidor de sesiones) como Fase 3 y SFTP como Fase 4.

1. **Fase 3 — Pestañas y Servidor de Sesiones.** Especificada el 15 sep 2026 (78 funcionalidades). Incluye los arreglos R12 y R13 de la Fase 2.
2. **Fase 4 — SFTP en panel doble.** Decidida como siguiente. Vista Archivos local ⇄ remoto con marcadores `≠`/`✕`, transferencias con progreso que sobreviven a la ventana (viven en el servidor), aviso sobre `.env`, `russh-sftp` sobre el pool. El protocolo gana `AbrirCanal`/mensajes de transferencia.
3. **Fase 5 — Túneles.** Local, remoto y dinámico, en el servidor, con arranque automático al conectar; `LocalForward` de `opciones_extra` migrable a TUNELES.
4. **Fase 6 — Snippets y deliberación MAGI.** Snippets a etiquetas, ejecución masiva vía `Ejecutar`, deliberación con tres comprobaciones (último sondeo, backup, tests) y forzado con motivo en REGISTRO; los `reiniciar`/`deploy`/`logs` del mockup de Flota.
5. **Fase 7 — Sincronización cifrada.** Cifrado del inventario en reposo, desbloqueo al arrancar, fichero cifrado exportable. Depende del modelo completo.
6. **Fase 8 — App Android.** Comparte el modelo de datos y consume el fichero de sincronización; sin servidor (sesiones propias del dispositivo). Depende de F7.

## 8. Decisiones Transversales y Riesgos Abiertos

### Decisiones transversales

- **T1 — Sin custodia de claves privadas.** MAGI solo guarda referencias (huella del agente o ruta de fichero). Aplica a todas las fases, Android incluido.
- **T2 — Cliente SSH embebido (russh) en todas las fases.** Elegido en F1 frente al `ssh` del sistema; SFTP, túneles, sondeo y Android se construyen sobre él. `ControlMaster` solo se exporta a `magi_config`; dentro de MAGI el multiplexado es de canales (F3).
- **T3 — Inventario en SQLite como fuente de verdad; `~/.ssh/config` se importa, `magi_config` se exporta.** Nunca se edita el `config` del usuario salvo para insertar el `Include`, con copia de seguridad.
- **T4 — Huellas en `~/.ssh/known_hosts` del sistema.** Sin tabla propia; toda aceptación o sustitución queda registrada (log en F1, REGISTRO desde F2).
- **T5 — Doble codificación glifo + color y modo ASCII degradado** en toda pantalla, también en Android (glifos y paleta).
- **T6 — Ninguna acción destructiva con una sola pulsación.** Confirmación siempre; las irreversibles en producción pasan por la deliberación (F4).
- **T7 — Tema de Omarchy con paleta fija de respaldo.** Compartido con mmmusic.
- **T8 — Sin `CHECK` sobre enumeraciones en SQLite; migraciones por `PRAGMA user_version`.** Convención 4d3 aplicada a Rust.
- **T9 — Un solo crate binario; Android decide en F6 si extrae núcleo.**
- **T10 — Sin telemetría.** Las únicas conexiones salientes son a los hosts del usuario y, en F5, a su destino de sincronización.
- **T11 — (F1 implementación) Versiones fijadas del stack:** `russh 0.63` (claves, agente y known_hosts en `russh::keys`; el crate `russh-keys` separado no se usa), `ratatui 0.30` + `crossterm 0.29`, `tui-term 0.3` + `vt100 0.16`, `rusqlite 0.40`. Cualquier fase que suba una de ellas debe subir el conjunto y repetir las pruebas manuales de Sesión.
- **T12 — (F1 implementación) Toda escritura en `~/.ssh` y en los ficheros de MAGI pasa por `src/ficheros.rs`** (atómica, 600, copia con fecha). SFTP (F4) y sincronización (F5) reutilizan este módulo para lo local.
- **T13 — (F1 implementación) La UI recibe estados y avisos, nunca bytes:** el parser vt100 vive en la tarea de conexión y notifica `Pantalla` coalescido a ~30 fps; los diálogos que esperan al usuario (huella, frase) no tienen timeout. Las pestañas (F3) replican este patrón por sesión.
- **T14 — (F1 implementación) Documentación en `docs/faseN/`** (una carpeta por fase, `magi-maestro.md` en `docs/`), como en mmmusic.
- **T15 — (F2) Las operaciones automáticas nunca dialogan.** El sondeo (y en el futuro los túneles automáticos, los snippets masivos y la sincronización) reportan huellas desconocidas o frases pendientes como error con instrucción; solo las conexiones interactivas aceptan huellas o piden frases.
- **T16 — (F2) `registro::anotar` es la única escritura en REGISTRO.** Toda fase que añada eventos define un `tipo` nuevo y llama a esa función; las entradas no se editan, solo se purgan.
- **T17 — (F2) MAGI genera claves pero no las destruye.** Revocar es baja lógica de la referencia; borrar ficheros de `~/.ssh` o quitar claves del agente es siempre manual. Complementa T1.
- **T18 — (F2 implementación, reformulada en F3) Un único escritor de SQLite por proceso.** En cada proceso, solo su bucle principal escribe (las tareas de red emiten eventos); entre procesos, `busy_timeout` 5 s y WAL. El servidor escribe únicamente REGISTRO de sesión/servidor y `ultimo_estado`.
- **T19 — (F2 anexo) Los secretos de autenticación que no son claves (contraseñas) viven solo en el llavero del sistema.** `magi.db` guarda referencias; ninguna fase puede guardar un secreto en la base de datos, tampoco cifrado (la F5 cifra el inventario, no lo convierte en custodio).
- **T20 — (F2 implementación) `RegistroSesiones` es el registro único de conexiones vivas** (`host_id → Arc<Handle>`); toda función que necesite una conexión (sondeo, túneles, SFTP, snippets) la pide ahí antes de abrir una efímera.
- **T21 — (F2 implementación) Los eventos del registro se anotan cuando el efecto se ha producido**, no cuando se decide (`HuellaRegistrada` tras escribir `known_hosts`). Aplica a la deliberación de F6 (forzado anotado tras ejecutar).
- **T22 — (F3) Multiplexado propio: cierra la decisión abierta D1 del informe de diseño.** Las sesiones (y desde F4/F5 las transferencias y túneles) viven en `magi --servidor`, mismo binario, autolanzado, apagado por inactividad, socket Unix en directorio 700. Sin tmux ni zellij.
- **T23 — (F3) El servidor solo custodia lo que debe sobrevivir a la ventana** (sesiones, pool de conexiones, y después transferencias y túneles); todo lo demás sigue en el cliente. Nada que necesite un diálogo se decide en el servidor: se reenvía al solicitante (extiende T15).
- **T24 — (F3) Protocolo JSON por líneas versionado.** Cada fase que añada mensajes incrementa `VERSION_PROTOCOLO` si cambia la semántica de los existentes; cliente y servidor de versiones distintas no cooperan y nunca se mata un servidor con sesiones sin orden explícita.

### Riesgos abiertos

| Id | Riesgo | Impacto | Mitigación |
|---|---|---|---|
| R1 | ~~Envolver el terminal remoto en la TUI se adelanta a F1 al elegir russh~~ | Cerrado 15 sep 2026 | Validado por Hector sobre Omarchy con hosts reales; la base de sesión única queda lista para las pestañas de F3 |
| R2 | ~~Compatibilidad de versiones entre `tui-term`, `ratatui` y `vt100`~~ | Cerrado 15 sep 2026 | Versiones fijadas y compatibles (T11) |
| R3 | Claves `sk` (YubiKey) y llaveros del sistema no se pueden usar como fichero con russh | Medio | Documentar que se usan a través del agente; F2 lo hace explícito en la pantalla de identidades |
| R4 | El público de un gestor SSH de terminal es pequeño y parte ya usa `~/.ssh/config` + tmux | Bajo (producto propio) | Entregar F1 y F2 y usarlas un mes antes de decidir F3 |
| R5 | Sondeo en tiempo real (F2) no escala más allá de unas decenas de hosts | Medio en F2 | Sondeo bajo demanda con semáforo de 8 y timeout de 5 s (D15); tiempo real fuera del roadmap |
| R10 | ~~Generación rsa 4096 con `ssh-key`~~ | Cerrado 15 sep 2026 | `ssh-key =0.7.0-rc.11` como dependencia directa con `rsa`; `DEFAULT_RSA_KEY_SIZE = 4096` |
| R11 | `wl-copy`/`pbcopy` ausentes o Alacritty sin acceso al portapapeles de Wayland | Bajo | Fallback validado en la implementación; `wl-copy` real pendiente de la validación manual |
| R12 | Ruido en REGISTRO: con auto-refresco y un host caído se anota `sondeo_fallido` en cada refresco (una entrada cada 15 s) | Medio | Propuesta para la siguiente fase: anotar solo la transición a CAÍDA (y la vuelta a NOMINAL) |
| R13 | macOS: `security add-generic-password -w` recibe la contraseña por argumento y puede verse un instante en `ps`; camino macOS sin validar | Medio (solo macOS) | Propuesta: sustituir el subproceso por el crate `security-framework` (Keychain por API, sin argv) cuando se valide macOS; Linux (`secret-tool` por stdin) no tiene el problema |
| R14 | `zeroize` incompleto: `ssh-key 0.7` no borra `PrivateKey` y russh copia la contraseña a `String` | Bajo | Vida mínima de los secretos en memoria; revisar al subir `ssh-key`/russh |
| R15 | La Fase 3 se especifica sobre una Fase 2 sin validación manual (override de Hector) | Medio | Validar F2 antes de arrancar la implementación de F3 o asumir que los arreglos de F2 se mezclan con F3 |
| R16 | Un demonio con protocolo y recuperación es la pieza más compleja del proyecto: ciclo de vida, lock, socket huérfano, versión, dos procesos sobre SQLite | Alto | Tests de servidor con socket temporal desde el S1; `magi --servidor` en primer plano para depurar; un escritor por proceso (T18) |
| R17 | `setsid`/autolanzado y el directorio de socket en macOS sin validar (sin equipo) | Medio (macOS) | Fallbacks de ruta especificados; validar en el mac mini cuando se pueda |
| R18 | Dos ventanas escribiendo en la misma pestaña sin arbitraje pueden mezclar entradas | Bajo | Comportamiento de tmux, conocido; la barra muestra cuántas ventanas hay adjuntas |
| R6 | La deliberación MAGI (F4) se vuelve decorativa si el forzado es gratis | Alto en F4 | Motivo obligatorio y registro con usuario, hora y acción |
| R7 | Las directivas globales de `~/.ssh/config` (antes del primer `Host`) se omiten al importar; un usuario con `ForwardAgent yes` global pierde ese comportamiento dentro de MAGI | Bajo | Aviso en la importación (F1). Modelar «opciones globales» en `config.toml` o en una fase posterior si aparece la necesidad |
| R8 | Sin CI: `cargo test`, `clippy` y `fmt` solo se ejecutan en local | Bajo | Crear el flujo de GitHub Actions cuando exista el remoto (queda en el checklist de F1) |
| R9 | macOS no verificado: solo hay garantía de que no existe código específico de plataforma | Bajo | Probar en el mac mini antes de la F2 o aceptar macOS como «mejor esfuerzo» |
