# MAGI - Informe Maestro

**Última actualización:** 14 de septiembre de 2026

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
| 1 | Inventario y Conexión | 128 | 4 | 5 | Especificada |
| 2 | Flota, identidades y registro | ~70 (estimado) | — | — | Pendiente de especificar |
| 3 | Sesiones con pestañas y túneles | ~60 (estimado) | — | — | Pendiente de especificar |
| 4 | SFTP, snippets y deliberación MAGI | ~90 (estimado) | — | — | Pendiente de especificar |
| 5 | Sincronización cifrada | ~40 (estimado) | — | — | Pendiente de especificar |
| 6 | App Android | ~80 (estimado) | — | — | Pendiente de especificar |
| **Total** | | **128 confirmadas (~470 estimadas)** | **4** | **~5 semanas (F1)** | |

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
 ┌──────────────┐            │        │        │      ┌─────────────────────┐
 │ IDENTIDADES  │◀ referencia│        │        │      │ REGISTRO            │
 │ id alias tipo│  por huella│        │        └─────<│ id host_id accion   │
 │ huella origen│  (F1 guarda│        │               │ detalle usuario     │
 │ anadida_en   │  solo ref.)│        │               │ fecha resultado     │
 └──────────────┘            │        │               └─────────────────────┘
 ┌──────────────┐            │        │
 │ SONDEOS      │<───────────┘        │
 │ id host_id   │                     │
 │ fecha cpu mem│                     │
 │ disco carga  │                     │
 │ servicios    │                     │
 └──────────────┘                     │
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

Las tablas de F2 en adelante son previsiones y se confirman al especificar cada fase.

## 5. Glosario de Dominio

| Término de negocio | Entidad/Tabla | Fase | Notas |
|---|---|---|---|
| Host | HOSTS | 1 | Equipo remoto; `nombre` es el alias `Host` de ssh_config |
| Grupo | GRUPOS | 1 | Carpeta plegable del inventario; un host tiene como mucho un grupo |
| Etiqueta | ETIQUETAS / HOST_ETIQUETAS | 1 | Clasificación libre N:N; destino de snippets en F4 |
| Salto | HOSTS.salto_host_id | 1 | `ProxyJump`; autorreferencia |
| Identidad | HOSTS.identidad_ref (F1) / IDENTIDADES (F2) | 1 / 2 | Referencia a clave del agente o fichero; nunca la clave privada |
| Sesión | en memoria (F1) | 1 | Conexión SSH interactiva; máquina de estados en `conexion/` |
| Huella | `~/.ssh/known_hosts` | 1 | Sin tabla propia; se comparte con el ssh del sistema |
| Opciones extra | HOSTS.opciones_extra | 1 | Directivas ssh_config literales no modeladas |
| magi_config | `~/.ssh/magi_config` | 1 | Exportación del inventario incluida por `Include` |
| Flota | vista F1 sobre HOSTS + SONDEOS | 2 | Panel de estado |
| Registro | REGISTRO | 2 | Acciones remotas y forzados |
| Túnel | TUNELES | 3 | Local, remoto o dinámico |
| Snippet | SNIPPETS | 4 | Comando dirigido a etiquetas |
| Deliberación MAGI | DELIBERACIONES + VERIFICACIONES_HOST | 4 | Confirmación de acciones críticas con tres comprobaciones |

## 6. Estado del Proyecto

| Fase | Progreso | Último informe de implementación procesado |
|---|---|---|
| 1 — Inventario y Conexión | 0 / 128 | ninguno (sin confirmar) |

**Progreso total acumulado:** 0 / 128 funcionalidades.

## 7. Roadmap

1. **Fase 2 — Flota, identidades y registro.** Vista Flota (F1) con sondeo bajo demanda (CPU, memoria, disco, uptime, servicios systemd) sobre el cliente russh; pantalla de identidades (F5) con huella, uso, generar, importar y revocar referencias; tabla REGISTRO y vista F7. Depende de F1.
2. **Fase 3 — Sesiones con pestañas y túneles.** Varias sesiones simultáneas, multiplexado de canales sobre una conexión, túneles local/remoto/dinámico con arranque automático. Depende de F1; es la fase de mayor riesgo técnico y conviene abordarla con F1 y F2 estabilizadas y usadas un tiempo.
3. **Fase 4 — SFTP, snippets y deliberación MAGI.** Panel doble con marcadores `≠`/`✕`, aviso sobre `.env`, snippets dirigidos a etiquetas con ejecución masiva, deliberación con tres comprobaciones y forzado con motivo registrado. Depende de F1 y del registro de F2.
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

### Riesgos abiertos

| Id | Riesgo | Impacto | Mitigación |
|---|---|---|---|
| R1 | Envolver el terminal remoto en la TUI (secuencias de escape, teclas capturadas, redimensionado) se adelanta a F1 al elegir russh | Alto: puede consumir el sprint 4 entero | Una sola sesión en F1; `tui-term` + `vt100` en lugar de emulador propio; prefijo de escape configurable; validar pronto con vim y htop remotos |
| R2 | Compatibilidad de versiones entre `tui-term`, `ratatui` y `vt100` | Medio | Fijar versiones en `Cargo.toml` al inicio del sprint 4; si `tui-term` está estancado, renderizar el buffer de `vt100` a mano |
| R3 | Claves `sk` (YubiKey) y llaveros del sistema no se pueden usar como fichero con russh | Medio | Documentar que se usan a través del agente; F2 lo hace explícito en la pantalla de identidades |
| R4 | El público de un gestor SSH de terminal es pequeño y parte ya usa `~/.ssh/config` + tmux | Bajo (producto propio) | Entregar F1 y F2 y usarlas un mes antes de decidir F3 |
| R5 | Sondeo en tiempo real (F2) no escala más allá de unas decenas de hosts | Medio en F2 | Sondeo bajo demanda en F2; tiempo real fuera del roadmap actual |
| R6 | La deliberación MAGI (F4) se vuelve decorativa si el forzado es gratis | Alto en F4 | Motivo obligatorio y registro con usuario, hora y acción |
