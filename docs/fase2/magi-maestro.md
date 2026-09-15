# MAGI - Informe Maestro

**Última actualización:** 15 de septiembre de 2026 (Fase 2 especificada)

## 1. Visión Global

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy / Hyprland / Alacritty) y compatible con macOS, con una app Android prevista al final del roadmap. Cubre el terreno funcional de Termius —inventario de hosts, identidades, sesiones, SFTP, túneles y sincronización entre dispositivos— sin cuenta en un servicio de terceros ni suscripción: la sincronización se hará con un fichero cifrado sobre el transporte que el usuario ya tenga, y las claves privadas nunca salen del agente, el llavero o el token físico.

El diseño de interfaz (once pantallas, sistema visual, modelo de teclado) viene definido en el informe ejecutivo de diseño v0.1 de 14 de septiembre de 2026 y las fases lo implementan sin reinterpretarlo. La propuesta de valor es que el gestor viva en la terminal, se maneje sin ratón y siga estando en el móvil cuando algo se cae a las once de la noche.

**Cliente y contexto.** Producto propio de 4d3 para uso diario de Hector; sin cliente externo. Repositorio `magi` en GitHub. Implementación con Claude Code.

## 2. Mapa de Módulos

```
  ┌───────────────────────────────────────────────────────────────────────┐
  │ F1  INVENTARIO Y CONEXIÓN                                             │
  │  hosts · grupos · etiquetas · ficha · ssh_config ⇄ magi_config        │
  │  cliente russh · huellas · sesión única · paleta                      │
  └──────────┬───────────────────────┬───────────────────────┬────────────┘
             │                       │                       │
             ▼                       ▼                       ▼
  ┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐
  │ F2 FLOTA           │  │ F3 SESIONES        │  │ F4 ARCHIVOS Y      │
  │ sondeo bajo        │  │ pestañas · túneles │  │    AUTOMATIZACIÓN  │
  │ demanda ·          │  │ multiplexado de    │  │ SFTP panel doble · │
  │ identidades (F5) · │  │ canales            │  │ snippets ·         │
  │ registro (F7)      │  │                    │  │ deliberación MAGI  │
  └─────────┬──────────┘  └─────────┬──────────┘  └─────────┬──────────┘
            │                       │                       │
            └───────────────────────┼───────────────────────┘
                                    ▼
                       ┌────────────────────────┐
                       │ F5 SINCRONIZACIÓN      │
                       │ cifrado en reposo ·    │
                       │ desbloqueo · fichero   │
                       │ cifrado exportable     │
                       └───────────┬────────────┘
                                   ▼
                       ┌────────────────────────┐
                       │ F6 ANDROID             │
                       │ app nativa, mismo      │
                       │ modelo de datos        │
                       └────────────────────────┘

  F2, F3 y F4 dependen solo de F1 y pueden reordenarse entre sí.
  F4 (deliberación) usa el registro de F2. F5 requiere el modelo completo. F6 requiere F5.
```

## 3. Tabla Resumen de Fases

| Fase | Módulo | Funcionalidades | Sprints | Semanas | Estado |
|------|--------|-----------------|---------|---------|--------|
| 1 | Inventario y Conexión | 128 | 4 | 5 | Completada (126/128; validada por Hector en Omarchy el 15 sep 2026; quedan CI y macOS como pendientes menores) |
| 2 | Flota, Identidades y Registro | 79 | 3 | 3,5 | Especificada |
| 3 | Sesiones con pestañas y túneles | ~60 (estimado) | — | — | Pendiente de especificar |
| 4 | SFTP, snippets y deliberación MAGI | ~90 (estimado) | — | — | Pendiente de especificar |
| 5 | Sincronización cifrada | ~40 (estimado) | — | — | Pendiente de especificar |
| 6 | App Android | ~80 (estimado) | — | — | Pendiente de especificar |
| **Total** | | **207 especificadas (~480 estimadas)** | **7** | **~8,5 semanas (F1-F2)** | |

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
 ┌──────────────────────┐             │
 │ TUNELES              │<────────────┤
 │ id host_id tipo      │             │
 │ local remoto         │             │
 │ automatico           │             │
 └──────────────────────┘             │
 ══ F4 ═══════════════════════════════│═════════════════════════════════════
 ┌──────────────────────┐  ┌──────────┴───────────┐  ┌──────────────────────┐
 │ SNIPPETS             │  │ VERIFICACIONES_HOST  │  │ DELIBERACIONES       │
 │ id nombre comando    │  │ host_id salud backup │  │ id accion host_id    │
 │ critico              │  │ tests ventana        │  │ resultado forzada    │
 └──────────┬───────────┘  └──────────────────────┘  │ motivo fecha         │
            │ SNIPPET_ETIQUETAS                       └──────────────────────┘
            └──────────────> ETIQUETAS
 ══ F5 ═════════════════════════════════════════════════════════════════════
 ┌──────────────────────┐
 │ SINCRONIZACION       │  (metadatos: dispositivo, versión, última exportación)
 └──────────────────────┘
 ══ F6 ═════════════════════════════════════════════════════════════════════
 Android reutiliza el esquema F1-F5 sin tablas nuevas.
```

Las tablas de F3 en adelante son previsiones y se confirman al especificar cada fase.

## 5. Glosario de Dominio

| Término de negocio | Entidad/Tabla | Fase | Notas |
|---|---|---|---|
| Host | HOSTS | 1 | Equipo remoto; `nombre` es el alias `Host` de ssh_config |
| Grupo | GRUPOS | 1 | Carpeta plegable del inventario; un host tiene como mucho un grupo |
| Etiqueta | ETIQUETAS / HOST_ETIQUETAS | 1 | Clasificación libre N:N; destino de snippets en F4 |
| Salto | HOSTS.salto_host_id | 1 | `ProxyJump`; autorreferencia |
| Identidad | HOSTS.identidad_ref (F1) / IDENTIDADES (F2) | 1 / 2 | Referencia a clave del agente, fichero o token; nunca la clave privada. Revocar = baja lógica de la referencia |
| Sesión | en memoria (F1) | 1 | Conexión SSH interactiva; máquina de estados en `conexion/` |
| Huella | `~/.ssh/known_hosts` | 1 | Sin tabla propia; se comparte con el ssh del sistema |
| Opciones extra | HOSTS.opciones_extra | 1 | Directivas ssh_config literales no modeladas |
| magi_config | `~/.ssh/magi_config` | 1 | Exportación del inventario incluida por `Include` |
| Flota | vista F1 sobre HOSTS + SONDEOS | 2 | Panel de estado; vista de arranque |
| Sondeo | SONDEOS | 2 | Resultado de ejecutar `sondeo.sh` en un host; 20 por host |
| Servicios | HOSTS.servicios | 2 | Unidades systemd que vigila el sondeo |
| Estado de flota | derivado de SONDEOS | 2 | FRÍA / CAÍDA / ALCANZABLE / CARGA / NOMINAL según umbrales de `config.toml` |
| Registro | REGISTRO | 2 | Historial de conexiones, huellas, importación/exportación, claves y sondeos fallidos; en F4 también forzados |
| Túnel | TUNELES | 3 | Local, remoto o dinámico |
| Snippet | SNIPPETS | 4 | Comando dirigido a etiquetas |
| Deliberación MAGI | DELIBERACIONES + VERIFICACIONES_HOST | 4 | Confirmación de acciones críticas con tres comprobaciones |

## 6. Estado del Proyecto

| Fase | Progreso | Último informe de implementación procesado |
|---|---|---|
| 1 — Inventario y Conexión | 126 / 128 | 15 sep 2026 (implementada en una sesión de Claude Code; 22 tests verdes; pendientes: CI en GitHub Actions —descartado por Hector hasta tener remoto— y verificación en macOS —sin equipo—; validada manualmente por Hector con hosts reales el 15 sep 2026: «funcionando todo») |

| 2 — Flota, Identidades y Registro | 0 / 79 | ninguno (sin confirmar) |

**Progreso total acumulado:** 126 / 207 funcionalidades.

La Fase 1 se da por completada: las dos funcionalidades sin marcar (CI y macOS) siguen abiertas en su checklist y se cierran cuando exista el remoto y un mac; no bloquean la Fase 2.

## 7. Roadmap

1. **Fase 2 — Flota, Identidades y Registro.** Especificada el 15 sep 2026 (79 funcionalidades). Pendiente de implementación.
2. **Fase 3 — Sesiones con pestañas y túneles.** Varias sesiones simultáneas, multiplexado de canales sobre una conexión, túneles local/remoto/dinámico con arranque automático. Depende de F1; es la fase de mayor riesgo técnico y conviene abordarla con F1 y F2 estabilizadas y usadas un tiempo.
3. **Fase 4 — SFTP, snippets y deliberación MAGI.** Panel doble con marcadores `≠`/`✕`, aviso sobre `.env`, snippets dirigidos a etiquetas con ejecución masiva (incluidos los `reiniciar`/`deploy`/`logs` del mockup de Flota), deliberación con tres comprobaciones apoyadas en el último sondeo y en REGISTRO, y forzado con motivo registrado. Depende de F1 y de F2.
4. **Fase 5 — Sincronización cifrada.** Cifrado del inventario en reposo, pantalla de arranque con desbloqueo, exportación/importación de un fichero cifrado con clave simétrica. Depende del modelo completo.
5. **Fase 6 — App Android.** App nativa con tarjetas y consola con barra de modificadores; comparte el modelo de datos y consume el fichero de sincronización. Depende de F5. La decisión de reimplementar o extraer un núcleo compartido se toma al especificarla (ver D1).

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

### Riesgos abiertos

| Id | Riesgo | Impacto | Mitigación |
|---|---|---|---|
| R1 | ~~Envolver el terminal remoto en la TUI se adelanta a F1 al elegir russh~~ | Cerrado 15 sep 2026 | Validado por Hector sobre Omarchy con hosts reales; la base de sesión única queda lista para las pestañas de F3 |
| R2 | ~~Compatibilidad de versiones entre `tui-term`, `ratatui` y `vt100`~~ | Cerrado 15 sep 2026 | Versiones fijadas y compatibles (T11) |
| R3 | Claves `sk` (YubiKey) y llaveros del sistema no se pueden usar como fichero con russh | Medio | Documentar que se usan a través del agente; F2 lo hace explícito en la pantalla de identidades |
| R4 | El público de un gestor SSH de terminal es pequeño y parte ya usa `~/.ssh/config` + tmux | Bajo (producto propio) | Entregar F1 y F2 y usarlas un mes antes de decidir F3 |
| R5 | Sondeo en tiempo real (F2) no escala más allá de unas decenas de hosts | Medio en F2 | Sondeo bajo demanda con semáforo de 8 y timeout de 5 s (D15); tiempo real fuera del roadmap |
| R10 | Generación rsa 4096 con `ssh-key`: la versión que trae russh 0.63 podría no tener habilitada la generación RSA | Bajo | Si no está, la F2 entrega solo ed25519 y rsa queda como desviación documentada |
| R11 | `wl-copy`/`pbcopy` ausentes o Alacritty sin acceso al portapapeles de Wayland | Bajo | Fallback siempre disponible: diálogo con la clave pública |
| R6 | La deliberación MAGI (F4) se vuelve decorativa si el forzado es gratis | Alto en F4 | Motivo obligatorio y registro con usuario, hora y acción |
| R7 | Las directivas globales de `~/.ssh/config` (antes del primer `Host`) se omiten al importar; un usuario con `ForwardAgent yes` global pierde ese comportamiento dentro de MAGI | Bajo | Aviso en la importación (F1). Modelar «opciones globales» en `config.toml` o en una fase posterior si aparece la necesidad |
| R8 | Sin CI: `cargo test`, `clippy` y `fmt` solo se ejecutan en local | Bajo | Crear el flujo de GitHub Actions cuando exista el remoto (queda en el checklist de F1) |
| R9 | macOS no verificado: solo hay garantía de que no existe código específico de plataforma | Bajo | Probar en el mac mini antes de la F2 o aceptar macOS como «mejor esfuerzo» |
