# MAGI - Informe Maestro

**Última actualización:** 30 de octubre de 2026 (Fase 9 «Empaquetado y Releases» y Fase 10 «Sincronización Cifrada» especificadas; la F10 sobre la F9 sin confirmar)

## 1. Visión Global

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy / Hyprland / Alacritty) y compatible con macOS, con una app Android prevista al final del roadmap. Cubre el terreno funcional de Termius —inventario de hosts, identidades, sesiones, SFTP, túneles y sincronización entre dispositivos— sin cuenta en un servicio de terceros ni suscripción: la sincronización se hará con un fichero cifrado sobre el transporte que el usuario ya tenga, y las claves privadas nunca salen del agente, el llavero o el token físico.

El diseño de interfaz (once pantallas, sistema visual, modelo de teclado) viene definido en el informe ejecutivo de diseño v0.1 de 14 de septiembre de 2026 y las fases lo implementan sin reinterpretarlo. La propuesta de valor es que el gestor viva en la terminal, se maneje sin ratón y siga estando en el móvil cuando algo se cae a las once de la noche.

**Cliente y contexto.** Producto propio de 4d3 para uso diario de Hector; sin cliente externo. Repositorio `4d3-org/magi` en GitHub. Implementación con Claude Code.

**Distribución (desde la F9).** Software libre (MIT OR Apache-2.0), publicado como paquete `magi-ssh` (el comando sigue siendo `magi`): `magi-ssh-bin` en AUR para Omarchy/Arch, fórmula `magi-ssh` en el tap `4d3-org/homebrew-tap` para macOS y tarballs verificables en cada GitHub Release. Primera versión pública: 1.0.0.

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
  │ F3  PESTAÑAS Y SERVIDOR DE SESIONES  (implementada; pool corregido F6)│
  │  magi --servidor · protocolo · pool de conexiones · pestañas ·        │
  │  varias ventanas · reconexión                                         │
  └──────┬───────────────────────┬───────────────────────┬────────────────┘
         ▼                       ▼                       ▼
  ┌──────────────────┐  ┌──────────────────┐  ┌────────────────────────┐
  │ F4 ARCHIVOS      │  │ F5 TÚNELES       │  │ F6 SNIPPETS Y          │
  │ SFTP panel doble │  │ local · remoto · │  │    DELIBERACIÓN MAGI   │
  │ sobre el pool    │  │ dinámico · auto  │  │ (+ corrección 3b pool) │
  │                  │  │ en el servidor   │  │ tecla F8 · 3 compr.    │
  └────────┬─────────┘  └────────┬─────────┘  └───────────┬────────────┘
           └───────────────────┬─┴─────────────────────────┘
                               ▼
                    ┌────────────────────────┐
                    │ F7 REDIMENSIONADO      │  (completada) tubería única · modo estrecho · mínimos · pruebas de pintado
                    │    ADAPTABLE (fix)     │  (regresión de F1/F3, todas las vistas F1-F6)
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F8 ARCHIVOS, 2ª VUELTA │  (implementada; sin revisión adversarial ni validación) editar remoto · chmod ·
                    │                        │  propietarios · sincronizar directorio · corrige 3 defectos de F4
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F9 EMPAQUETADO Y       │  magi-ssh 1.0.0 · dist · AUR magi-ssh-bin · tap Homebrew ·
                    │    RELEASES            │  CI · protocolo v6 con saludo estable · magi diagnostico
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F10 SINCRONIZACIÓN     │  un fichero age firmado por equipo en una carpeta elegida ·
                    │     CIFRADA            │  fusión por vectores · conflictos · magi.db en claro
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F11 ANDROID            │  app nativa, mismo modelo, sin servidor
                    └────────────────────────┘

  F4, F5 y F6 dependen de F3 (usan el servidor y el pool) y pueden reordenarse entre sí.
  F6 usa REGISTRO y el sondeo de F2. F7 revisa todas las vistas. F8 amplía F4 y usa la deliberación de F6.
  F9 distribuye todo lo anterior y cierra el pendiente de CI de F1. F10 requiere el modelo completo y
  se publica como MINOR. F11 requiere F10.
```

## 3. Tabla Resumen de Fases

| Fase | Módulo | Funcionalidades | Sprints | Semanas | Estado |
|------|--------|-----------------|---------|---------|--------|
| 1 | Inventario y Conexión | 128 | 4 | 5 | Completada (126/128; validada por Hector en Omarchy el 15 sep 2026; quedan CI y macOS como pendientes menores) |
| 2 | Flota, Identidades y Registro | 84 (79 + anexo 5) | 3 | 3,5 | Implementación reportada (84/84; pendiente validación manual de Hector; R12/R13 cerrados en F3) |
| 3 | Pestañas y Servidor de Sesiones | 79 | 3 | 4 | Implementación reportada (79/79 tras la corrección 3b de la F6; pendiente validación completa) |
| 4 | Archivos (SFTP en panel doble) | 65 | 3 | 3,5 | Implementación reportada (65/65; 146 tests; pendiente validación manual de Hector) |
| 5 | Túneles | 54 | 3 | 3 | Implementación reportada (54/54; 194 tests; túnel remoto y fallo de bind validados por Hector contra host real; resto pendiente) |
| 6 | Snippets y Deliberación MAGI (+ corrección 3b) | 80 | 4 | 3,5 | Implementación reportada (79/80; falta la revisión adversarial de S1-S3, R38; pendiente validación manual) |
| 7 | Redimensionado Adaptable (fix) | 46 | 3 | 2 | Completada (46/46; validada por Hector el 3 oct 2026) |
| 8 | Archivos, Segunda Vuelta | 64 | 3 | 3,5 | Implementación reportada (63/64; revisión adversarial no hecha por decisión de Hector, R50; pendiente validación manual) |
| 9 | Empaquetado y Releases | 54 | 2 | 2 | Especificada |
| 10 | Sincronización Cifrada | 63 | 3 | 3 | Especificada (sobre la F9 sin confirmar, R68) |
| 11 | App Android | ~80 (estimado) | — | — | Pendiente de especificar |
| **Total** | | **717 especificadas (~797 estimadas)** | **31** | **~33 semanas (F1-F10)** | |

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
 │ id host_id nombre    │             │  UNIQUE(host_id, nombre)
 │ tipo escucha destino │             │  local | remoto | dinamico
 │ automatico creado_en │             │
 │ actualizado_en       │             │
 └──────────────────────┘             │
 ══ F4 ═══════════════════════════════│═════════════════════════════════════
 HOSTS + sftp_dir_local, sftp_dir_remoto (F4); cola de transferencias y canales SFTP en memoria del servidor
 ══ F6 ═══════════════════════════════│═════════════════════════════════════
 HOSTS + snippet_al_conectar_id (FK SNIPPETS, SET NULL)
 ┌──────────────────────┐   ┌──────────┴───────────┐   ┌──────────────────────────┐
 │ SNIPPETS             │   │ VERIFICACIONES_HOST  │   │ DELIBERACIONES           │
 │ id nombre comando    │   │ host_id (PK, 1:1)    │   │ id fecha snippet_id      │
 │ descripcion etiquetas│   │ salud backup         │   │ accion hosts_json        │
 │ critico timeout_seg  │   │ backup_ruta          │   │ comprobaciones_json      │
 │ parar_al_fallo       │   │ backup_patron        │   │ resultado bloqueada      │
 │ usado_veces          │   │ tests tests_comando  │   │ motivo usuario           │
 │ ultimo_uso_en        │   └──────────────────────┘   │ ejecucion_resultado      │
 └──────────┬───────────┘                              └──────────────────────────┘
            │ 1:N CASCADE
 ┌──────────▼───────────┐
 │ SNIPPET_DESTINOS     │  etiqueta (por nombre) | host_id (FK HOSTS CASCADE)
 └──────────────────────┘
 Ejecuciones y su salida: en memoria del servidor (1 h)
 ══ F7 ═════════════════════════════════════════════════════════════════════
 (sin tablas: redimensionado adaptable; Geometria y Disposicion en memoria del cliente)
 ══ F8 ═════════════════════════════════════════════════════════════════════
 ┌──────────────────────────────┐
 │ SINCRONIZACIONES_DIR         │  host_id (FK HOSTS CASCADE) · nombre (UNIQUE por host)
 │ ruta_local ruta_remota       │  direccion subida|bajada · borrar · exclusiones
 │ ultima_ejecucion_en          │  ultimo_resultado (lo escribe el servidor)
 └──────────────────────────────┘
 ══ F9 ═════════════════════════════════════════════════════════════════════
 (sin tablas: empaquetado y releases; user_version sigue en 6; una magi.db más nueva que el binario no se abre)
 ══ F10 ════════════════════════════════════════════════════════════════════
 GRUPOS, HOSTS, IDENTIDADES (uuid v5 de la huella), TUNELES, SNIPPETS y SINCRONIZACIONES_DIR
 + uuid (UNIQUE) · version (<ms>.<uuid equipo>) · vector (JSON por equipo)   (migración 7)
 ┌──────────────────────────────┐ ┌──────────────────────────────┐ ┌──────────────────────────────┐
 │ EQUIPOS                      │ │ LAPIDAS                      │ │ CONFLICTOS                   │
 │ uuid nombre plataforma       │ │ uuid (PK) tipo nombre        │ │ tipo elemento_tipo uuid      │
 │ es_este app_version estado   │ │ version vector borrado_en    │ │ nombre version_perdedora_json│
 │ ultimo_fichero_en            │ └──────────────────────────────┘ │ version_ganadora             │
 │ ultima_importacion_en        │                                  │ equipo_ganador detectado_en  │
 │ hash_ultimo_fichero          │                                  │ resuelto_en resolucion       │
 │ quitado_en reloj_ms          │                                  └──────────────────────────────┘
 └──────────────────────────────┘
 Propias de cada equipo: no viajan. Fichero por equipo magi-<uuid>.age en la carpeta elegida (no es tabla).
 ══ F11 ════════════════════════════════════════════════════════════════════
 Android reutiliza el esquema F1-F10 y el formato de docs/formato-sincronizacion.md (previsión).
```

Las tablas de F11 son previsiones y se confirman al especificar la fase. La previsión `SINCRONIZACION_INVENTARIO` de antes quedó concretada en la F10 como EQUIPOS, LAPIDAS y CONFLICTOS.

## 5. Glosario de Dominio

| Término de negocio | Entidad/Tabla | Fase | Notas |
|---|---|---|---|
| Host | HOSTS | 1 | Equipo remoto; `nombre` es el alias `Host` de ssh_config |
| Grupo | GRUPOS | 1 | Carpeta plegable del inventario; un host tiene como mucho un grupo |
| Etiqueta | ETIQUETAS / HOST_ETIQUETAS | 1 | Clasificación libre N:N de hosts; destino de snippets (F6) por nombre |
| Salto | HOSTS.salto_host_id | 1 | `ProxyJump`; autorreferencia |
| Identidad | HOSTS.identidad_ref (F1) / IDENTIDADES (F2) | 1 / 2 | Referencia a clave del agente, fichero o token, o marca `contrasena` / `contrasena:llavero`; nunca el secreto. Revocar = baja lógica de la referencia |
| Llavero | `src/llavero.rs` (secret-tool / Keychain) | 2 | Único lugar donde vive una contraseña de host; `magi.db` solo guarda la referencia |
| Sesión | en memoria (F1) → servidor (F3) | 1 / 3 | Conexión SSH interactiva con su pty y su pantalla; desde F3 vive en `magi --servidor` y puede estar adjunta a varias ventanas |
| Pestaña | vista Sesión (F3) | 3 | Representación de una sesión en una ventana; nombre `<host>` o `<host> (n)` |
| Servidor | `magi --servidor` (F3) | 3 | Proceso local que custodia sesiones y el pool de conexiones; socket en `$XDG_RUNTIME_DIR/magi/` |
| Cliente / ventana | TUI `magi` (F3) | 3 | Cada terminal con `magi` abierto; adjunta pestañas y hace todo lo que no es sesión |
| Solicitante | servidor (F3) | 3 | Cliente que pidió abrir o reconectar una sesión; único que recibe sus diálogos |
| Pool de conexiones | servidor (F3, corregido en F6) | 3 / 6 | `host_id → Arc<Handle>`; única vía `conexion_para_canal` con cerrojo por host; SFTP, túneles, `Ejecutar` y ejecuciones lo usan siempre, las pestañas solo con `multiplexar`; nunca se desplaza una entrada viva |
| Archivos | vista F4 (F4) | 4 | Panel doble local ⇄ remoto sobre SFTP; protocolo `VERSION_PROTOCOLO = 2` |
| Transferencia | servidor, en memoria (F4) | 4 | Subida o bajada encolada; una en curso por host; sobrevive a la ventana |
| Marca | `archivos/marcas.rs` (F4) | 4 | `≠` difiere / `✕` no existe al otro lado; por nombre, tipo, tamaño y mtime |
| Sensibles | `[archivos] avisar` (F4) | 4 | Globs que piden confirmación antes de subir |
| Huella | `~/.ssh/known_hosts` | 1 | Sin tabla propia; se comparte con el ssh del sistema |
| Opciones extra | HOSTS.opciones_extra | 1 | Directivas ssh_config literales no modeladas |
| magi_config | `~/.ssh/magi_config` | 1 | Exportación del inventario incluida por `Include` |
| Flota | vista F1 sobre HOSTS + SONDEOS | 2 | Panel de estado; vista de arranque |
| Sondeo | SONDEOS | 2 | Resultado de ejecutar `sondeo.sh` en un host; 20 por host |
| Servicios | HOSTS.servicios | 2 | Unidades systemd que vigila el sondeo |
| Estado de flota | derivado de SONDEOS | 2 | FRÍA / CAÍDA / ALCANZABLE / CARGA / NOMINAL según umbrales de `config.toml` |
| Registro | REGISTRO | 2 | Historial de conexiones, huellas, importación/exportación, claves, sondeos, (F3) sesiones y servidor, (F4) `transferencia` y `borrado_remoto`, (F5) `tunel_abierto`, `tunel_cerrado`, `tunel_fallido`, (F6) `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`, (F8) `sincronizacion`, `permisos_cambiados` |
| Túnel | TUNELES | 5 | Local (`-L`), remoto (`-R`) o dinámico (`-D`, SOCKS5); definido en SQLite, ejecutado en el servidor sobre el pool (`VERSION_PROTOCOLO = 3`); automático = ligado a la primera/última pestaña o SFTP del host |
| Reenvío | `sshconfig/tuneles.rs` | 5 | `LocalForward`/`RemoteForward`/`DynamicForward` de ssh_config, convertidos a túneles y devueltos al exportar |
| Snippet | SNIPPETS + SNIPPET_DESTINOS | 6 | Comando guardado con variables `{{var}}`, dirigido a etiquetas de host (por nombre) y hosts sueltos; «crítico» fuerza deliberación |
| Ejecución | servidor, en memoria (F6) | 6 | Lanzamiento de un snippet ya resuelto en N hosts; salida por host con tope, 1 h; `VERSION_PROTOCOLO = 4` |
| Snippet al conectar | HOSTS.snippet_al_conectar_id | 6 | Snippet no crítico y sin variables escrito en la pestaña al abrirla |
| Verificaciones previas | VERIFICACIONES_HOST | 6 | Qué comprobaciones exige cada host: salud, backup (ruta, patrón), tests (comando local) |
| Deliberación MAGI | DELIBERACIONES | 6 / 8 | Pausa antes de ejecutar: MELCHIOR-1 salud, BALTHASAR-2 backup, CASPER-3 tests; unanimidad, `Ctrl+K`, forzado con motivo |
| Modo estrecho | `ui/disposicion.rs` (F7) | 7 | Disposición de una vista con ancho < 100: un panel con `Tab`, columnas secundarias ocultas |
| Tamaño mínimo | `ui/disposicion.rs` (F7) | 7 | Por vista (global 40×12); por debajo se pinta un aviso en lugar de la vista |
| Edición remota | `archivos/edicion.rs` · `app/edicion.rs` (F8) | 8 | `E`: temporal en `…/magi/ediciones` + `$EDITOR` + detección por SHA-256 + conflicto por `StatRemoto`; el temporal nunca se borra sin subir, descartar o guardar una copia local comprobada |
| Sincronización de directorio | SINCRONIZACIONES_DIR (F8) | 8 | Plan recursivo local ⇄ remoto (crear, actualizar, borrar opcional), con exclusiones y `.magiignore`; protocolo `VERSION_PROTOCOLO = 5` (`StatRemoto`, `CambiarPermisos`, `ListarArbol`, `borrar_al_terminar`, `SincronizacionLanzada`); no confundir con la sincronización cifrada del inventario (F10) |
| `.magiignore` | raíz del directorio origen (F8) | 8 | Exclusiones con sintaxis de gitignore; lo excluido nunca se borra en destino. Está en `[archivos] excluir` por defecto, así que no se sincroniza salvo que se quite de la lista |
| Renombrador | `servidor/sftp.rs` (F8) | 8 | Sustituye un fichero remoto con `posix-rename@openssh.com` por un segundo canal SFTP raw o, si el host no lo anuncia, en tres pasos con deshacer (`.magi-viejo`); corrige el «sobrescribir» de la F4 |
| Forzado | DELIBERACIONES.motivo + REGISTRO `deliberacion_forzada` | 6 | Ejecutar pese a un rechazo; motivo ≥ 10 caracteres, usuario y hora |
| Paquete `magi-ssh` | `Cargo.toml` (F9) | 9 | Nombre del paquete en AUR (`magi-ssh-bin`), Homebrew y los tarballs; el comando, la biblioteca y las rutas de datos siguen llamándose `magi` |
| Release | GitHub Release `vX.Y.Z` (F9) | 9 | Tarballs de 4 objetivos con `.sha256` y atestaciones, notas del CHANGELOG; la crea `dist` desde un tag, nunca a mano; no es una tabla |
| Servidor anterior | `VersionIncompatible` (F9) | 9 | Servidor de sesiones de otra versión al abrir `magi`; se sustituye solo si no custodia nada (trabajo y clientes a cero); si no, decide el usuario |
| Trabajo del servidor | `VersionIncompatible.trabajo` (F9) | 9 | Sesiones, transferencias en cola o en curso, túneles activos y ejecuciones en curso |
| Diagnóstico | `magi diagnostico` (F9) | 9 | Comprobación de lo que MAGI necesita, sin escribir ni conectar; `✓ ! ✕` y código 0 o 1 |
| Sincronización del inventario | `src/sincronizacion/` (F10) | 10 | El mismo inventario en todos los equipos del usuario por ficheros cifrados en una carpeta; no confundir con la sincronización de directorio (F8) ni con `magi sincronizaciones` |
| Equipo | EQUIPOS (F10) | 10 | Cada instalación de MAGI que sincroniza; su fichero es `magi-<uuid>.age`; estados `este`, `al_dia`, `ilegible`, `danado`, `formato_nuevo`, `quitado` |
| Fichero de sincronización | `magi-<uuid>.age` (F10) | 10 | Instantánea completa del inventario de un equipo: cabecera, firma HMAC y JSON `formato` 1, cifrado con `age`; formato en `docs/formato-sincronizacion.md` |
| Clave de sincronización | llavero (`sincronizacion`) (F10) | 10 | Identidad `age` X25519 generada por MAGI; solo en el llavero; se muestra como texto y QR; quitar un equipo la cambia |
| Versión y vector | `version`, `vector` (F10) | 10 | Última edición (`<ms>.<uuid equipo>`) y edición vista de cada equipo; deciden si un cambio deriva de otro o es concurrente |
| Lápida | LAPIDAS (F10) | 10 | Borrado que viaja como una versión más; se conserva siempre |
| Conflicto de sincronización | CONFLICTOS (F10) | 10 | Versión local que perdió (`elemento`), huella distinta (`huella`), borrado aplazado por trabajo vivo (`borrado`), renombrado (`nombre`) o huérfano; recuperable 90 días |

## 6. Estado del Proyecto

| Fase | Progreso | Último informe de implementación procesado |
|---|---|---|
| 1 — Inventario y Conexión | 126 / 128 | 15 sep 2026 (implementada en una sesión de Claude Code; 22 tests verdes; pendientes: CI en GitHub Actions —descartado por Hector hasta tener remoto— y verificación en macOS —sin equipo—; validada manualmente por Hector con hosts reales el 15 sep 2026: «funcionando todo») |
| 2 — Flota, Identidades y Registro | 84 / 84 | 15 sep 2026 (una sesión de Claude Code; 56 tests verdes; incluye el anexo de contraseña aprobado por Hector; pendiente validación manual con hosts reales, en especial contraseña contra un `sshd` real, RSA 4096, portapapeles en Wayland y revocación) |
| 3 — Pestañas y Servidor de Sesiones | 79 / 79 | 15 sep 2026, más la corrección 3b procesada con la F6 el 27 sep 2026 (una sesión de Claude Code; 71 tests verdes; Hector validó parcialmente en la misma sesión y salieron 6 fallos —2 críticos: apertura colgada tras autenticar y pantalla en blanco por parsers duplicados— ya corregidos y cubiertos por tests; pendiente validación completa: pestaña compartida entre ventanas, `multiplexar`, caída de red, SIGKILL al servidor, apagado por inactividad) |
| 4 — Archivos (SFTP) | 65 / 65 | 15 sep 2026 (una sesión de Claude Code; 146 tests, 17 de ellos de extremo a extremo contra `sftp-server` real; dos revisiones adversariales del agente encontraron 13 defectos —uno de pérdida de datos: mover con `omitir` borraba lo no copiado— corregidos con test de regresión; pendiente validación manual de la vista) |
| 5 — Túneles | 54 / 54 | 16 sep 2026 (una sesión de Claude Code; 194 tests; tres revisiones adversariales con 25 hallazgos, 24 corregidos con regresión —3 altos: reenvío remoto registrado en puerto 0, parar no cortaba conexiones, importación perdía reenvíos, exportación escribía líneas que rompían `ssh`—; Hector verificó túnel remoto y fallo de bind contra `un host real de Hector`) |
| 6 — Snippets y Deliberación MAGI | 79 / 80 | 27 sep 2026 (una sesión de Claude Code, un commit por sprint en `fase6-snippets`; 445 tests; revisión adversarial solo de la 3b —18 hallazgos corregidos—; la de S1-S3 la saltó Hector; prueba manual solo con hosts inalcanzables; corregida de paso una pérdida de datos de la ficha anterior a la fase) |
| 7 — Redimensionado Adaptable | 46 / 46 | 27 sep 2026 (procesado el 28; una sesión de Claude Code, un commit por sprint en `fase7-fix_redimensionado_ventana`; causas de R36 localizadas —servidor: el mínimo nunca crecía (F3); cliente: la pausa del visor tiraba eventos (F4)—; 178 pruebas nuevas y 128 instantáneas; revisión adversarial con 5 revisores y 3 verificadores por hallazgo: 19 hallazgos, 14 corregidos —uno de pérdida de datos en la ficha— y 5 refutados) |
| 8 — Archivos, Segunda Vuelta | 63 / 64 | 3 oct 2026 (una sesión de Claude Code con seis pistas en paralelo sobre un contrato común; un commit por sprint en `fase8-inapp_editor`; 468 unitarios, 48 de SFTP contra `sftp-server` real y 250 de pintado; corregidos tres defectos heredados de la F4 —sobrescribir fallaba siempre en OpenSSH, `tmp` se vaciaba en cada descarga y `borrar_temporal` aceptaba `..`—; la revisión adversarial se lanzó y Hector la paró antes de que devolviera hallazgos, R50; sin validación manual) |
| 9 — Empaquetado y Releases | 0 / 54 | ninguno (sin confirmar) |
| 10 — Sincronización Cifrada | 0 / 63 | ninguno (sin confirmar; especificada sobre la F9 sin implementar) |

**Progreso total acumulado:** 596 / 717 funcionalidades (faltan CI y macOS de la F1, la revisión adversarial de S1-S3 de la F6, la revisión adversarial de la F8 y las F9 y F10 enteras). La línea de CI de la F1 se marcará al procesar el informe de la F9 si su `ci.yml` pasa. Al completarse la F6, su primera línea de 3b vuelve a marcar la funcionalidad desmarcada de la F3.

La Fase 1 se da por completada: las dos funcionalidades sin marcar (CI y macOS) siguen abiertas en su checklist y se cierran cuando exista el remoto y un mac; no bloquean la Fase 2.

## 7. Roadmap

Reordenado el 15 sep 2026 tras elegir Hector la propuesta «solo pestañas» (ampliada a servidor de sesiones) como Fase 3 y SFTP como Fase 4.

1. **Fase 3 — Pestañas y Servidor de Sesiones.** Implementada el 15 sep 2026 (78/79 tras desmarcar la reutilización del pool, R28); R12 y R13 de la Fase 2 incluidos.
2. **Fase 4 — Archivos (SFTP en panel doble).** Implementada el 15 sep 2026 (65/65); pendiente de validación manual. Sin `chmod`, sin papelera, sin sincronizar directorio (`chmod` y sincronizar llegan en la F8). La F8 corrigió tres defectos suyos (informe v1.4).
3. **Fase 5 — Túneles.** Implementada el 16 sep 2026 (54/54); pendiente de validación manual completa.
3b. **Corrección del pool de la F3:** hecha como primer bloque de la Fase 6 (27 sep 2026); R28-R31 cerrados.
4. **Fase 6 — Snippets y Deliberación MAGI.** Implementada el 27 sep 2026 (79/80, 9 de ellas la corrección 3b); la revisión adversarial de S1-S3 queda sin hacer por decisión de Hector (27 sep 2026: PR y paso a la F7); pendiente la validación manual. Vista `F8`. Fuera: deploy con subida de ficheros, snippet que exige túnel, ventana de mantenimiento, ejecutar solo en los aprobados, ejecución desde la CLI.
5. **Fase 7 — Redimensionado Adaptable (fix).** Implementada el 27 sep 2026 (46/46) y validada por Hector el 3 oct 2026.
6. **Fase 8 — Archivos, Segunda Vuelta.** Especificada e implementada el 3 oct 2026 (63/64): edición remota, `chmod`, propietarios con nombre, sincronizar directorio con sincronizaciones guardadas y deliberación. La revisión adversarial queda sin hacer por decisión de Hector (R50; el prompt de reanudación la deja preparada); pendiente la validación manual. Fuera: `chown`, papelera, sincronización por hash o continua, limpieza de temporales de edición huérfanos (R51).
7. **Fase 9 — Empaquetado y Releases.** Especificada el 30 oct 2026 (54 funcionalidades, 2 sprints), aceptando las opciones recomendadas salvo la versión: `1.0.0` (Hector). Paquete `magi-ssh`, repositorio público con MIT OR Apache-2.0, releases con `dist`, AUR `magi-ssh-bin`, tap de Homebrew sin notarizar, CI con macOS y `cargo deny`, protocolo v6 con saludo estable y sustitución segura del servidor anterior, `magi diagnostico`. Se especifica con la F8 sin revisión adversarial ni validación manual (R50, R59). Fuera: notarización, .deb/.rpm, crates.io, `curl | sh`, Windows.
8. **Fase 10 — Sincronización Cifrada** (antes F9). Especificada el 30 oct 2026 a petición de Hector sin esperar al informe de la F9 (63 funcionalidades, 3 sprints, todas las opciones recomendadas). `magi.db` **no** se cifra en el disco (cambia el «cifrado en reposo y desbloqueo al arrancar» previsto): se cifra lo que sale del equipo. Un fichero `age` firmado por equipo en una carpeta elegida, fusión por vectores con conflictos recuperables, lápidas, huellas sin sustitución automática, unión por nombre, clave en el llavero con QR, quitar equipo cambia la clave, copia previa y confirmación de borrados masivos, sincronización automática mientras MAGI está abierto. Se publicará como 1.1.0. Fuera: SFTP como transporte, historial, compartir con otras personas, sincronizar con MAGI cerrado.
9. **Fase 11 — App Android** (antes F10). Comparte el modelo de datos y lee y escribe el fichero de sincronización (`docs/formato-sincronizacion.md`), con la clave en Android Keystore añadida por QR; sin servidor (sesiones propias del dispositivo). Depende de F10. Su distribución (Play Store o F-Droid) se decide en su fase.

## 8. Decisiones Transversales y Riesgos Abiertos

### Decisiones transversales

- **T1 — Sin custodia de claves privadas.** MAGI solo guarda referencias (huella del agente o ruta de fichero). Aplica a todas las fases, Android incluido.
- **T2 — Cliente SSH embebido (russh) en todas las fases.** Elegido en F1 frente al `ssh` del sistema; SFTP, túneles, sondeo y Android se construyen sobre él. `ControlMaster` solo se exporta a `magi_config`; dentro de MAGI el multiplexado es de canales (F3).
- **T3 — Inventario en SQLite como fuente de verdad; `~/.ssh/config` se importa, `magi_config` se exporta.** Nunca se edita el `config` del usuario salvo para insertar el `Include`, con copia de seguridad.
- **T4 — Huellas en `~/.ssh/known_hosts` del sistema.** Sin tabla propia; toda aceptación o sustitución queda registrada (log en F1, REGISTRO desde F2).
- **T5 — Doble codificación glifo + color y modo ASCII degradado** en toda pantalla, también en Android (glifos y paleta).
- **T6 — Ninguna acción destructiva con una sola pulsación.** Confirmación siempre; las irreversibles en producción pasan por la deliberación (F6).
- **T7 — Tema de Omarchy con paleta fija de respaldo.** Compartido con mmmusic.
- **T8 — Sin `CHECK` sobre enumeraciones en SQLite; migraciones por `PRAGMA user_version`.** Convención 4d3 aplicada a Rust.
- **T9 — Un solo crate binario; Android decide en su fase (F11) si extrae núcleo.**
- **T10 — Sin telemetría.** Las únicas conexiones salientes son a los hosts del usuario y, en la sincronización cifrada (F10), a su destino de sincronización. Precisión (F9): MAGI tampoco comprueba si hay versiones nuevas; las actualizaciones llegan por el gestor de paquetes.
- **T11 — (F1 implementación) Versiones fijadas del stack:** `russh 0.63` (claves, agente y known_hosts en `russh::keys`; el crate `russh-keys` separado no se usa), `ratatui 0.30` + `crossterm 0.29`, `tui-term 0.3` + `vt100 0.16`, `rusqlite 0.40`. Cualquier fase que suba una de ellas debe subir el conjunto y repetir las pruebas manuales de Sesión.
- **T12 — (F1 implementación) Toda escritura en `~/.ssh` y en los ficheros de MAGI pasa por `src/ficheros.rs`** (atómica, 600, copia con fecha). SFTP (F4) y sincronización (F5) reutilizan este módulo para lo local.
- **T13 — (F1 implementación) La UI recibe estados y avisos, nunca bytes:** el parser vt100 vive en la tarea de conexión y notifica `Pantalla` coalescido a ~30 fps; los diálogos que esperan al usuario (huella, frase) no tienen timeout. Las pestañas (F3) replican este patrón por sesión.
- **T14 — (F1 implementación) Documentación en `docs/faseN/`** (una carpeta por fase, `magi-maestro.md` en `docs/`), como en mmmusic.
- **T15 — (F2) Las operaciones automáticas nunca dialogan.** El sondeo (y en el futuro los túneles automáticos, los snippets masivos y la sincronización) reportan huellas desconocidas o frases pendientes como error con instrucción; solo las conexiones interactivas aceptan huellas o piden frases.
- **T16 — (F2) `registro::anotar` es la única escritura en REGISTRO.** Toda fase que añada eventos define un `tipo` nuevo y llama a esa función; las entradas no se editan, solo se purgan.
- **T17 — (F2) MAGI genera claves pero no las destruye.** Revocar es baja lógica de la referencia; borrar ficheros de `~/.ssh` o quitar claves del agente es siempre manual. Complementa T1.
- **T18 — (F2 implementación, reformulada en F3, F6 y F8) Un único escritor de SQLite por proceso.** En cada proceso, solo su bucle principal escribe (las tareas de red emiten eventos); entre procesos, `busy_timeout` 5 s y WAL. El servidor escribe únicamente REGISTRO (eventos que produce), `HOSTS.ultimo_estado`/`ultima_conexion_en`, desde F6 `DELIBERACIONES.ejecucion_resultado` y desde F8 `SINCRONIZACIONES_DIR.ultimo_resultado`, siempre por su hilo escritor.
- **T19 — (F2 anexo) Los secretos de autenticación que no son claves (contraseñas) viven solo en el llavero del sistema.** `magi.db` guarda referencias; ninguna fase puede guardar un secreto en la base de datos, tampoco cifrado (la F10 cifra lo que sale del equipo, no lo convierte en custodio).
- **T20 — (F2 implementación) `RegistroSesiones` es el registro único de conexiones vivas** (`host_id → Arc<Handle>`); toda función que necesite una conexión (sondeo, túneles, SFTP, snippets) la pide ahí antes de abrir una efímera.
- **T21 — (F2 implementación) Los eventos del registro se anotan cuando el efecto se ha producido**, no cuando se decide (`HuellaRegistrada` tras escribir `known_hosts`). Aplica a la deliberación de F6 (forzado anotado tras ejecutar).
- **T22 — (F3) Multiplexado propio: cierra la decisión abierta D1 del informe de diseño.** Las sesiones (y desde F4/F5 las transferencias y túneles) viven en `magi --servidor`, mismo binario, autolanzado, apagado por inactividad, socket Unix en directorio 700. Sin tmux ni zellij.
- **T23 — (F3) El servidor solo custodia lo que debe sobrevivir a la ventana** (sesiones, pool de conexiones, y después transferencias y túneles); todo lo demás sigue en el cliente. Nada que necesite un diálogo se decide en el servidor: se reenvía al solicitante (extiende T15).
- **T24 — (F3, precisada en F4) Protocolo JSON por líneas versionado.** `VERSION_PROTOCOLO` se incrementa cuando cambia la semántica de un mensaje existente **o cuando el cliente depende de mensajes que un servidor anterior no tiene** (F4 → v2); cliente y servidor de versiones distintas no cooperan y nunca se mata un servidor con sesiones sin orden explícita. Precisión (F9): `Hola` y `VersionIncompatible` son contrato estable (T66).
- **T25 — (F3 implementación) El servidor no toca el llavero ni ningún secreto almacenado:** `FuenteContrasena` decide si la contraseña sale del llavero (cliente, conexiones efímeras) o del solicitante (sesiones del servidor). Extiende T19 al modelo de dos procesos.
- **T26 — (F3 implementación) En el servidor, las escrituras de SQLite van por un hilo propio con canal de órdenes** (`Connection` de rusqlite no es `Send`); lecturas por una segunda conexión de solo lectura. Túneles (F5) y transferencias (F4) que anoten en REGISTRO usan ese hilo.
- **T27 — (F3 implementación) La difusión de la lista completa es el único mecanismo de sincronización cliente-servidor:** el cliente reconstruye su estado a partir de `Sesiones{lista}` conservando lo local (parsers, orden, activa). F4 y F5 harán lo mismo con `Transferencias{lista}` y `Tuneles{lista}`, sin mensajes de sincronización finos.
- **T28 — (F3 implementación) Todo tamaño de terminal que llega de fuera se sanea** (mín. 2×1) y el alto del PTY se calcula con un único helper que resta las filas fijas de la vista; `vt100` no tolera un 0.
- **T29 — (F4) Lo que dialoga se decide en el cliente antes de encolar; el servidor recomprueba y aplica una política.** Conflictos y avisos de sensibles se resuelven antes de `Transferir`; una cola desatendida nunca pregunta (T15). Los snippets masivos y el deploy (F6) siguen el mismo esquema.
- **T30 — (F4) Ninguna ruta pasa por un shell.** Todo lo remoto va por SFTP o por `Ejecutar` con argumentos validados; lo local por `std::fs`.
- **T31 — (F4 implementación, método) Revisión adversarial del código nuevo antes de cerrar una fase.** En F4 dos pasadas encontraron 13 defectos reales, uno de pérdida de datos, que 146 tests no cazaban; en F3 la validación manual de Hector había encontrado 6. Desde ahora es un paso obligatorio del cierre de fase (en `CLAUDE.md`).
- **T32 — (F4 implementación) Toda espera de red en el servidor tiene plazo:** 10 s para DNS/TCP y saludos de subsistema (`request_subsystem` devuelve `Ok` aunque el host diga no), 60 s por bloque de transferencia; solo los diálogos con el usuario (5 min) escapan a esa regla.
- **T33 — (F4 implementación) Un diálogo lleva fijados los datos sobre los que actúa** (rutas, ids) y no relee el estado al confirmar; y una respuesta a una petición que ya no está en vuelo se descarta.
- **T34 — (F4 implementación) Todo recurso del pool se libera por identidad y en todas las ramas de error**, incluidas las de apertura fallida; el servidor no se apaga mientras haya trabajo encolado (sesiones, transferencias y, en F5, túneles).
- **T35 — (F5) Todo lo que escucha en la máquina del usuario escucha en loopback por defecto**; exponerlo (`0.0.0.0`/`::`) exige aviso explícito. Aplica a túneles y a cualquier puerto futuro (modo remoto, sincronización).
- **T36 — (F5) Las directivas de ssh_config que MAGI modela se retiran de `opciones_extra`** y pasan a la lista de gestionadas; hay una sola fuente de verdad por directiva y la exportación la devuelve al fichero. Precisión (F5 implementación): solo las formas que MAGI representa; lo que `ssh` admite y MAGI no, sigue en `opciones_extra`, y lo que no se puede convertir al importar vuelve ahí con aviso, nunca se pierde.
- **T37 — (F5 implementación) `magi_config` nunca contiene una línea que `ssh` no sepa leer** (se sustituye por comentario) y el fichero generado se valida en los tests con el `ssh` real: una línea inválida deja sin `ssh` a todos los hosts porque el fichero se incluye desde `~/.ssh/config`.
- **T38 — (F5 implementación) El servidor valida toda fila que lee de SQLite antes de ejecutarla.** La tabla la escribe otro proceso; una fila corrupta o de otra versión no puede convertirse en un comportamiento distinto (un reenvío remoto en un listener local).
- **T39 — (F5 implementación, precisada en F6 y F8) Los `peticion_id` viven en un espacio único por proceso** con rangos disjuntos: Archivos (panel) desde 0, túneles 2^40, operaciones de Archivos en la `App` 2^42 (T61), ejecuciones 2^44, peticiones esperables del cliente 2^48. Toda petición con respuesta (también `Ejecutar`) lleva su `peticion_id`.
- **T40 — (F5 implementación) Un cambio de estado se difunde antes de ejecutar su efecto lento** (`parando` antes de cerrar) y **un reenvío o listener se retira en todas las ramas de salida** (caída, descarte, cancelación), con plazo.
- **T41 — (F6) Toda ejecución en más de un host, todo snippet crítico y todo host que lo exija pasan por la deliberación MAGI;** `Ctrl+K` es su única confirmación y el forzado exige motivo registrado. Cualquier acción masiva futura (sincronización, Android) hereda la regla.
- **T42 — (F6, corrección 3b) `conexion_para_canal` con cerrojo por host es la única forma de obtener una conexión** para cualquier subsistema; el pool nunca desplaza una entrada viva ni en gracia. Precisión (implementación): `usa_pool(host, uso)` decide; SFTP, túneles, `Ejecutar` y ejecuciones usan siempre el pool, y `multiplexar` solo afecta a las pestañas. Se toma con `ConexionTomada` y se suelta por identidad.
- **T43 — (F6) Las comprobaciones previas tienen un plazo duro de 2 s y se ejecutan en paralelo;** una comprobación lenta es un rechazo, no una espera.
- **T44 — (F6) La salida de comandos remotos nunca se escribe en REGISTRO ni en el log;** vive en memoria del servidor con tope y retención, y solo se guarda a disco por orden explícita (`ficheros.rs`, 600).
- **T45 — (F7) Toda vista deriva su disposición del área en cada pintado, declara su tamaño mínimo y su modo estrecho en `ui/disposicion.rs`, y tiene instantáneas de pintado a 40×12, 80×24 y 200×60.** Ninguna guarda tamaños entre pintados. Aplica a F8 en adelante y a cualquier vista futura.
- **T46 — (F7) Un solo punto aplica los cambios de tamaño** (agrupados 50 ms) y es el único que envía `Redimensionar`; cualquier suspensión de la TUI o readjuntado pasa por él.
- **T47 — (F6 implementación) Un `Hecho` que implica filas de REGISTRO se envía tras `OrdenBd::Barrera`,** y el hilo escritor se vacía con `OrdenBd::Terminar` al apagar; el apagado anota antes de tocar la red. Hace cumplir T21 entre procesos.
- **T48 — (F6 implementación) Las preguntas del servidor se apilan sobre el diálogo abierto, nunca lo pisan;** la deliberación vive fuera de la pila. Un formulario a medio escribir no se pierde por una huella o una contraseña.
- **T49 — (F6 implementación) Una operación masiva o automática nunca puede abrir un diálogo por error:** la contraseña de una ejecución se pide con `PideLlavero`, distinto de `PideContrasena`, y solo se resuelve desde el llavero.
- **T50 — (F6 implementación) Los registros de auditoría toman el usuario del uid del proceso,** no de `$USER`.
- **T51 — (F7 implementación) Todo cambio que afecta a cómo se interpretan los datos de una sesión se aplica en la tarea de lectura, en orden con esos datos** (`Redimensionada` antes de los `Datos` que ya vienen a ese tamaño); nunca en el bucle de la UI por detrás.
- **T52 — (F7 implementación) Una pausa del hilo de teclas nunca lee del tty** y se confirma con acuse antes de ceder la terminal a otro programa; `Resize` pasa siempre.
- **T53 — (F7 implementación) La App se construye en dos pasos** (estado y efectos): `App::de_prueba` y el bucle genérico sobre `Backend` permiten probar la interfaz real sin hilos, red ni servidor. Superficie de pruebas con `#[doc(hidden)] pub`.
- **T54 — (F7 implementación, método) La revisión adversarial verifica cada hallazgo con varios revisores independientes** (en F7, tres enfoques y mayoría de 2/3) y anota también los refutados; precisa T31.
- **T55 — (F8) Un trabajo del usuario hecho fuera de MAGI (una edición) nunca se borra mientras no esté a salvo:** el temporal solo desaparece tras subirse, tras «sin cambios», tras «descartar» explícito o (precisión de la implementación) tras guardar una copia local cuyo hash se ha comprobado. Vive en un directorio que nadie vacía (T59).
- **T56 — (F8) Sobrescribir un fichero existente conserva sus permisos** (se reaplican tras el `rename` del parcial) y avisa si cambia el propietario; los enlaces duros se rompen (R45). Precisión (implementación): el parcial nace ya con esos bits para no exponer nunca un 0600, y en una edición se toman del `StatRemoto` de justo antes de subir.
- **T57 — (F8) Un borrado masivo (sincronizar con «borrar») solo se ejecuta tras el éxito completo de la parte que escribe, sobre rutas fijadas en el plan, nunca sobre excluidas, y siempre con deliberación MAGI.** Precisión (implementación): tampoco borra nada que cuelgue de una ruta omitida ni de un enlace bajo la raíz; un directorio que no queda vacío se conserva sin error; y la cola para en el primer fallo.
- **T58 — (F8 implementación, Hector) Sobrescribir en remoto usa `posix-rename@openssh.com` por un segundo canal SFTP raw y, si el host no lo anuncia, tres pasos con deshacer (`.magi-viejo`);** nunca sustituye un directorio. El `rename` de SFTP v3 no sirve: en OpenSSH da EEXIST si el destino existe. Cada canal SFTP ocupa dos canales de sesión (R53).
- **T59 — (F8 implementación) Un temporal tiene nombre único y no se borra en bloque.** Los del visor en `tmp`, los de edición en `ediciones` (700, que nadie vacía); `BorrarTemporal` solo admite ficheros cuyo padre sea exactamente uno de esos dos directorios.
- **T60 — (F8 implementación) El servidor valida toda ruta de una transferencia al encolar:** absoluta, sin `.` ni `..`, colgando estrictamente de la raíz; nada que cuelgue de un enlace bajo la raíz se borra; una ruta no puede estar a la vez en destino y en borrado. Complementa T38 (validar lo que llega de otro proceso).
- **T61 — (F8 implementación) Las operaciones que deben sobrevivir a un cambio de vista o de host viven en la `App`, no en el estado de la vista,** con su propio rango de `peticion_id` y un único encaminador de respuestas (en F8, edición, permisos y sincronización en 2^42).
- **T62 — (F8 implementación) Una deliberación autoriza una sola acción:** la validación es común a ejecuciones y transferencias y «ya usada» vale en los dos sentidos. La interfaz se generaliza con `PlanDeliberado` para cualquier acción futura (T41).
- **T63 — (F8 implementación, método) Antes de trabajar en pistas paralelas se fija un contrato compilable** (tipos, mensajes, módulos vacíos y enganches); en F8, seis pistas con un solo conflicto al integrar.
- **T64 — (F9) El paquete se llama `magi-ssh`, pero el comando, la biblioteca, las rutas de datos y el socket siguen llamándose `magi`.** Ninguna ruta se deriva del nombre del paquete: cambiar de canal o de nombre de paquete nunca mueve los datos del usuario.
- **T65 — (F9) Los binarios publicados salen solo de CI a partir de un tag** (`dist`, `--locked`), con SHA-256 y atestaciones de procedencia; un tag publicado no se mueve ni se reutiliza.
- **T66 — (F9) `Hola` y `VersionIncompatible` son contrato estable:** nunca se quitan ni se renombran sus campos, solo se añaden opcionales, y el cliente lee la primera respuesta como JSON genérico. Así cualquier versión reconoce a otra, pasada o futura.
- **T67 — (F9) Un servidor de otra versión solo se para sin preguntar si no custodia nada** (ni sesiones, ni transferencias, ni túneles, ni ejecuciones, ni otras ventanas), y nunca desde un binario más antiguo que él. Un tipo de trabajo desconocido cuenta como trabajo. Precisa T24.
- **T68 — (F9) Versiones SemVer orientadas al usuario:** MAJOR si cambia algo que el usuario ve como incompatible (config, CLI, formatos exportados); MINOR para funcionalidades, migraciones y cambios de protocolo; PATCH para correcciones. No hay vuelta atrás: una `magi.db` más nueva que el binario no se abre.
- **T69 — (F9) Lo que se publica pasa antes una auditoría** (secretos en el historial, datos reales en documentos y tests). El agente prepara la publicación, pero hacer público el repositorio, crear secretos o cuentas y etiquetar versiones lo hace el desarrollador.
- **T70 — (F10) Todo lo que viaja entre equipos lleva `uuid`, `version` y `vector`.** Solo los campos que viajan suben la versión, con el reloj híbrido. Una tabla o un campo nuevo que deba viajar se añade a `sincronizacion/elementos.rs` y a `docs/formato-sincronizacion.md`, y si cambia el significado sube `formato`.
- **T71 — (F10) Lo que sale del equipo va cifrado (`age`) y firmado (HMAC con clave derivada); nada se importa sin firma válida.** Claves privadas, contraseñas y la clave de sincronización nunca viajan ni salen del llavero. Precisa T1 y T19.
- **T72 — (F10) Fusión determinista por vectores:** gana la versión dominante; entre concurrentes, la más reciente. La versión local que pierde nunca se pierde (CONFLICTOS) y los borrados son lápidas permanentes.
- **T73 — (F10) Una importación nunca cierra trabajo vivo ni sustituye una huella sin que el usuario lo acepte;** los borrados masivos se confirman y siempre hay copia previa de `magi.db`.
- **T74 — (F10) Un fichero o una base de datos de formato más nuevo que el binario se ignora o se rechaza con aviso, nunca se interpreta a medias** (amplía T68).
- **T75 — (F10) Cada equipo escribe solo su propio fichero;** el de otro solo se borra al quitarlo.

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
| R12 | ~~Ruido en REGISTRO por `sondeo_fallido` en cada refresco~~ | Cerrado 15 sep 2026 (F3) | Solo transiciones: `sondeo_fallido` al caer, `sondeo_recuperado` al volver |
| R13 | ~~macOS: contraseña por argumento a `security`~~ | Cerrado 15 sep 2026 (F3) | `security-framework` (Keychain por API); ejecución en macOS aún sin validar (R17) |
| R14 | `zeroize` incompleto: `ssh-key 0.7` no borra `PrivateKey` y russh copia la contraseña a `String` | Bajo | Vida mínima de los secretos en memoria; revisar al subir `ssh-key`/russh |
| R15 | Las Fases 2-6 y 8 tienen implementación reportada sin validación manual completa (override de Hector); la 3b cambió el pool que usan todas | Medio-alto | Validar F2-F6 en Omarchy con hosts reales, repitiendo Sesión, Archivos y Túneles (pestañas con y sin `multiplexar`, túnel activo al abrir pestaña, SFTP y túnel a la vez), y la F8 con su guion de 12 pasos; guiones al final de cada informe de implementación |
| R16 | Un demonio con protocolo y recuperación es la pieza más compleja del proyecto | Alto → Medio | Implementado; la validación parcial de Hector destapó 6 fallos (2 críticos) que los 71 tests no cubrían y que ahora sí cubren. Sigue abierto hasta la validación completa (pestaña compartida, caída de red, SIGKILL, inactividad) |
| R19 | `directories` resuelve el home por uid, no por `$HOME`: los tests aislados deben fijar también `XDG_*` y `XDG_RUNTIME_DIR` | Bajo | Documentado en `CLAUDE.md`; revisar si se cambia de crate de rutas |
| R20 | Rendimiento del servidor (20 sesiones × 3 ventanas, eco < 20 ms) sin medir | Bajo | Medir en la validación; nada en el diseño lo hace sospechar |
| R21 | ~~`russh-sftp` sin versión compatible~~ | Cerrado 15 sep 2026 | `russh-sftp = "=3.0.0"` funciona con russh 0.63.3; no subir uno sin el otro |
| R23 | Las peticiones SFTP en vuelo las negocia `russh-sftp` (no hay ventana configurable sin bajar a la capa raw); el objetivo de ≥ 20 MB/s en LAN no se ha medido | Bajo | Medir en la validación; si hace falta afinar, bajar a la capa raw de la biblioteca |
| R24 | ~~Propietario remoto solo numérico (`uid:gid`)~~ | Cerrado 3 oct 2026 (F8) | Nombres por `/etc/passwd` y `/etc/group` leídos por SFTP, sin ejecutar comandos (D99); su límite queda en R46 |
| R25 | `magi tunel activar` desde fuera de la TUI no puede dialogar: sin conexión viva y sin credenciales en agente/llavero falla | Bajo | Mensaje con instrucción; es el mismo límite del sondeo (T15) |
| R26 | El SOCKS5 sin autenticación expuesto en `0.0.0.0` es un proxy abierto en la red local | Medio si el usuario lo expone | Loopback por defecto y aviso (T35); sin opción de autenticación en esta fase |
| R27 | ~~Sin tecla de función para Snippets~~ | Cerrado 27 sep 2026 | `F8` (F10 reservada por Dicta en Omarchy) |
| R28 | ~~Las pestañas no reutilizaban el pool con `multiplexar` y desplazaban la conexión viva (defecto de la F3 descubierto en la F5)~~ | Cerrado 27 sep 2026 | Corrección 3b de la F6 |
| R29 | ~~Carrera «mirar el pool / conectar / guardar» entre pestañas, SFTP y túneles~~ | Cerrado 27 sep 2026 | Cerrojo por host en `conexion_para_canal` |
| R30 | ~~`magi servidor parar` no podía parar un servidor de versión anterior~~ | Cerrado 27 sep 2026 | pid por `peer_cred()` y `SIGTERM` con confirmación |
| R31 | ~~Test intermitente de la F4 por leer `REGISTRO` antes de que el hilo escritor confirmara~~ | Cerrado 27 sep 2026 | `OrdenBd::Barrera` |
| R32 | La ejecución masiva es la función más peligrosa del producto (un comando erróneo en 12 hosts) | Alto | Deliberación obligatoria en más de un host (T41), unanimidad, forzado con motivo, «parar al primer fallo», cancelación |
| R33 | CASPER-3 con `gh run list` u otra consulta por red puede superar los 2 s y rechazar en falso | Medio | Plazo configurable (`limite_seg`); el rechazo dice «plazo vencido»; el usuario puede desactivar la comprobación en ese host |
| R34 | `tests_comando` ejecuta en local un comando arbitrario del usuario con `sh -c` (excepción a T30) | Bajo (lo escribe el propio usuario) | Sin interpolación de datos del inventario ni de variables; plazo y `kill` |
| R35 | Memoria del servidor con muchas ejecuciones grandes (2 MiB por host × hosts × ejecuciones) | Bajo | Tope por flujo, retención 1 h, `C` limpia |
| R36 | ~~Regresión de redimensionado (repintado y tamaño del remoto)~~ | Cerrado 3 oct 2026 | Corregida en la F7 y validada por Hector |
| R37 | ~~La Fase 7 se especificó sobre la Fase 6 sin informe~~ | Cerrado 27 sep 2026 | Informe de la F6 procesado; la F7 pasa a v1.1 sin cambios de alcance |
| R38 | **La revisión adversarial de S1-S3 de la Fase 6 no se hizo** (Hector la saltó para cerrar): servidor de ejecuciones, lanzamiento, Resultados, deliberación y ficha sin la búsqueda de pérdida de datos, fugas, esperas sin plazo y estado leído a destiempo que en F4 y F5 encontró 13 y 25 defectos | Alto | Aceptado por Hector el 27 sep 2026: no se hace; el prompt de reanudación de la F6 queda disponible si aparecen fallos en ese código |
| R39 | BALTHASAR-2 abandonado por el cliente a los 2 s sigue en el servidor con el cerrojo del host hasta su plazo (10 s de TCP); una ejecución forzada justo después espera ese cerrojo (15,5 s en vez de 10 s en la prueba) | Bajo | Cancelar la apertura de comprobación cuando el cliente la abandona (reanudación F6) |
| R40 | Observaciones sin corregir de la F6: `aperturas_pendientes` no se limpia si el servidor rechaza un `AbrirSesion`; `Bienvenida` la vacía; `p` en varios hosts deja activa la última pestaña; una ejecución deliberada en curso cuando cae el servidor deja su fila sin `ejecucion_resultado`; EJECUTAR sin ayuda `?`; vector paralelo en el desplegable de snippet | Bajo | Reanudación F6 o candidatas de una fase posterior |
| R41 | Con `XDG_RUNTIME_DIR` largo el socket supera `SUN_LEN` (~108 bytes) y el servidor no arranca (visto en pruebas aisladas) | Bajo | Aviso claro al arrancar si la ruta es demasiado larga; documentado en `CLAUDE.md` |
| R42 | `tests/pool.rs::sigterm_para_el_servidor_limpio` falló con la máquina muy cargada (varios worktrees compilando): una vez en la F6 y otra en la F8; pasó al repetir | Bajo | Se ha repetido: dar más plazo al test o aislarlo |
| R43 | Las instantáneas solo recogen símbolos, no colores: resaltado de selección y colores del tema se comprueban a mano | Bajo | Validación manual; valorar instantáneas con estilo si aparece una regresión de color |
| R44 | Menores de interfaz fuera de checklist: pista de Sesión «‹ ›» nombra marcas y no teclas (F3); la barra del modo prefijo no enseña `f archivos` (F4); en ASCII Túneles usa `o` para activando e inactivo; un desplegable encogido no marca opciones por encima; título «nuevo · nuevo» en un host nuevo; código muerto `transferencias::confirmar`/`atajos` | Bajo | Candidatos para una fase de pulido |
| R45 | Sobrescribir (edición o sincronización) crea un inodo nuevo: el propietario pasa a ser el usuario de conexión y los enlaces duros se rompen | Medio | Se reaplican permisos y se avisa si cambia el propietario (T56); escribir en el sitio queda descartado (D98) |
| R46 | Los nombres de propietario salen de `/etc/passwd` y `/etc/group`: usuarios de LDAP/SSSD quedan con número | Bajo | Aceptado; el número sigue visible |
| R47 | Sincronizar árboles muy grandes (> 50 000 entradas) tarda y consume memoria en cliente y servidor | Medio | Aviso previo, `Arbol` por bloques, medir en la validación |
| R48 | Diferencias sutiles entre `.magiignore` y el gitignore real (negaciones, rutas ancladas) pueden excluir o incluir de más | Bajo | Mismo motor que git (`ignore`); la vista previa enseña los excluidos |
| R49 | El temporal de edición vive en `$XDG_RUNTIME_DIR` (tmpfs): un reinicio del equipo durante la edición la pierde | Bajo | Aceptado; la copia local en conflicto o error lo mitiga |
| R50 | **La revisión adversarial de la Fase 8 no se hizo** (Hector la paró el 3 oct 2026 antes de que devolviera hallazgos): unas 20 000 líneas sin revisar, con borrado al terminar, renombrador en tres pasos, temporales de edición, plan de sincronización y `chmod` recursivo, todo con riesgo de pérdida de datos | Alto | Aceptado por Hector. El workflow (siete focos, verificación 2/3) está guardado y el prompt de reanudación de la F8 lo deja listo; en F4 y F5 la revisión encontró 13 y 25 defectos que los tests no cazaban |
| R51 | Temporales de edición huérfanos en `…/magi/ediciones` (una ventana que muere o un fallo cerrado con `esc`): nadie los borra, a propósito (T55) | Bajo | Limpieza explícita en una fase futura; el tmpfs se vacía al reiniciar (R49) |
| R52 | Abrir un canal SFTP tarda más: nombres de `passwd`/`group` hasta 2 s y canal raw del renombrador hasta 10 s por paso si el host se cuelga, también en la apertura de comprobación; BALTHASAR-2 (plazo de 2 s) puede vencer más a menudo con hosts lentos sin canal abierto | Medio | Medir en la validación; si molesta, abrir el canal raw y los nombres en segundo plano |
| R53 | Cada canal SFTP ocupa dos canales de sesión de la conexión (principal + raw), que cuentan para `MaxSessions` del host (10 por defecto en OpenSSH) junto con pestañas multiplexadas, túneles y ejecuciones | Medio | Medir en la validación con muchas pestañas; si se alcanza, abrir el canal raw solo al sobrescribir |
| R54 | `ListarArbol` trata FIFOs, sockets y dispositivos como ficheros (igual que el listado de la F4): una bajada de sincronización intentaría leerlos | Medio | Saltarlos en el servidor como ya hace el recorrido local |
| R55 | TOCTOU en `CambiarPermisos`: entre el `lstat` y el `setstat` una ruta podría cambiarse por un enlace, y `setstat` lo seguiría (SFTP no tiene `fsetstat` para directorios) | Bajo | Aceptado; requiere que otro proceso cambie el árbol durante el `chmod` |
| R56 | Recuento de PERMISOS con el servidor caído: si estaba bajo una pregunta del servidor en la pila, reaparece tras SERVIDOR CAÍDO y espera hasta `Esc` | Bajo | Enganche de servidor caído para PERMISOS en una fase de pulido |
| R57 | Rendimiento de la F8 sin medir: plan de 10 000 entradas en < 3 s en LAN y repintado de la vista previa, que recalcula notas y anchos de todo el plan en cada pintado | Medio | Medir en la validación junto con R47 |
| R58 | `servidor::arrancar` (F3) pone la umask del proceso en 077 un instante para crear el socket: las pruebas que arrancan el servidor en el mismo proceso pueden crear ficheros 0600 (causa de una prueba intermitente de la F8) | Bajo (solo pruebas) | Los hogares de prueba fijan los modos de forma explícita; en producción el servidor es un proceso aparte |
| R59 | **Se publica la 1.0.0 con las Fases 2-8 sin validación manual completa y la F6 y la F8 sin revisión adversarial** (R15, R38, R50): los primeros usuarios externos pueden encontrar defectos que pierdan sesiones o ficheros | Alto | Aceptado por Hector (1.0.0 en vez de 0.9.0). Mitigación: validar al menos la F8 antes de etiquetar `v1.0.0`; parches 1.0.x rápidos desde el mismo flujo de releases |
| R60 | El nombre `magi` parece ocupado en AUR (sin confirmar) y hay crates `magi-*`; `magi-ssh-bin` o `magi-ssh` podrían estarlo también, y otro paquete podría instalar `/usr/bin/magi` | Bajo | Comprobación antes de la primera publicación; `provides`/`conflicts`; si choca, se consulta otro nombre |
| R61 | macOS sin notarizar: un tarball bajado con el navegador queda en cuarentena | Bajo | Homebrew no la pone; el README explica `xattr` |
| R62 | macOS sigue sin probarse en un equipo real: CI solo compila y pasa los tests unitarios (continúa R9 y R17) | Medio (macOS) | Instalar con Homebrew en el mac mini y seguir el guion de validación |
| R63 | Robo de los secretos de publicación (`HOMEBREW_TAP_TOKEN`, `AUR_SSH_PRIVATE_KEY`) permitiría publicar binarios maliciosos en los canales | Medio | Token de grano fino solo al tap, clave SSH dedicada a AUR, permisos mínimos en los workflows, rotación documentada; las atestaciones permiten comprobar que un binario salió de CI |
| R64 | `cargo deny` puede bloquear la CI por un aviso nuevo de RustSec en una dependencia fijada (russh-sftp `=3.0.0` atado a russh 0.63) | Bajo | Excepción en `deny.toml` con motivo y fecha; revisar al subir el conjunto |
| R65 | La entrada de escritorio con `Terminal=true` depende de cómo resuelva la terminal el lanzador de Omarchy | Bajo | Validación manual; alternativa `Exec=xdg-terminal-exec magi` como desviación |
| R66 | Tests intermitentes (R42 y los que dependen de tiempos) en runners compartidos de CI | Medio | Reintento manual; anotar y arreglar cada test intermitente en vez de desactivarlo |
| R67 | Los binarios de Linux exigen glibc ≥ 2.35: distros anteriores a 2022 no los ejecutan | Bajo (el objetivo es Omarchy) | Documentado en el README; compilar desde fuente sigue siendo posible |
| R68 | La F10 se especificó sobre la F9 sin implementar (y con la F8 sin revisión adversarial ni validación): si la F9 se desvía (nombre, protocolo, diagnóstico, CHANGELOG), hay que revisar la F10 | Medio | Procesar el informe de la F9 antes de lanzar la F10 y versionar su informe si hace falta |
| R69 | Relojes muy desfasados entre equipos: el reloj híbrido mantiene el orden de lo que deriva de otra cosa, pero entre dos ediciones concurrentes puede ganar la que en realidad fue anterior | Bajo | NTP en los equipos; la versión perdedora queda en CONFLICTOS |
| R70 | Perder un equipo: hasta quitarlo y cambiar la clave, quien lo tenga lee el inventario y puede escribir ficheros válidos en la carpeta | Alto si ocurre | Quitar y cambiar la clave en cuanto se sepa; los demás equipos se detienen al no abrir el fichero nuevo; las huellas nunca se sustituyen solas |
| R71 | Perder la clave en todos los equipos impide añadir equipos nuevos | Bajo (los datos siguen en cada `magi.db`) | Confirmar «la he guardado»; `k` la muestra de nuevo; se puede crear otra sincronización |
| R72 | La carpeta compartida enseña metadatos al servicio que la replica (número de equipos, tamaño, frecuencia de cambios) | Bajo | Aceptado; el contenido va cifrado y firmado |
| R73 | Las herramientas de sincronización dejan ficheros a medias, copias en conflicto o retrasos | Bajo | Un escritor por fichero, firma y reintento, copias en conflicto ignoradas, aviso en `magi diagnostico` |
| R74 | Dos ventanas del mismo equipo no ven los cambios locales de la otra hasta recargar (desde la F3); con la sincronización se nota más | Bajo | Candidato a pulido: difusión «inventario cambiado» por el servidor de sesiones |
| R75 | Nombres parecidos: `magi sincronizar` (inventario) frente a `magi sincronizaciones` (directorio, F8), y REGISTRO `sincronizacion` (F8) frente a `inventario_*` (F10) | Bajo | Ayuda de la CLI explícita y glosario |
| R76 | Quien tiene la clave tiene confianza total: puede cambiar direcciones y añadir huellas para hosts sin huella, y así redirigir conexiones sin aviso de huella | Medio | Clave solo en el llavero; quitar y cambiar clave; huellas añadidas en REGISTRO y nunca sustituidas |
| R77 | Las dependencias nuevas (`age`, `notify`, `qrcode`…) pueden tener licencias o avisos que `cargo deny` (F9) rechace | Bajo | Excepción con motivo o alternativa equivalente |
| R22 | El servidor escribe bajadas y temporales en el disco local: un cliente malicioso del mismo usuario podría dirigirlas a cualquier ruta | Bajo (mismo usuario, socket 700) | Modelo de amenaza igual al de `ssh-agent`; temporales en directorio propio 700. Desde la F8, `BorrarTemporal` ya no acepta `..` y las rutas de transferencia se validan al encolar (T59, T60) |
| R17 | `setsid`/autolanzado y el directorio de socket en macOS sin validar (sin equipo) | Medio (macOS) | Fallbacks de ruta especificados; validar en el mac mini cuando se pueda |
| R18 | Dos ventanas escribiendo en la misma pestaña sin arbitraje pueden mezclar entradas | Bajo | Comportamiento de tmux, conocido; la barra muestra cuántas ventanas hay adjuntas |
| R6 | La deliberación MAGI (F4) se vuelve decorativa si el forzado es gratis | Alto en F4 | Motivo obligatorio y registro con usuario, hora y acción |
| R7 | Las directivas globales de `~/.ssh/config` (antes del primer `Host`) se omiten al importar; un usuario con `ForwardAgent yes` global pierde ese comportamiento dentro de MAGI | Bajo | Aviso en la importación (F1). Modelar «opciones globales» en `config.toml` o en una fase posterior si aparece la necesidad |
| R8 | Sin CI: `cargo test`, `clippy` y `fmt` solo se ejecutan en local | Bajo | Crear el flujo de GitHub Actions cuando exista el remoto (queda en el checklist de F1) |
| R9 | macOS no verificado: solo hay garantía de que no existe código específico de plataforma | Bajo | Probar en el mac mini antes de la F2 o aceptar macOS como «mejor esfuerzo» |
