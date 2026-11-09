# MAGI - Informe Maestro

**Última actualización:** 9 de noviembre de 2026 (especificada la Fase 14, Android, Segunda Vuelta: 98 funcionalidades y 4 sprints, a la espera del informe de implementación. El móvil pasa a ser el de guardia, con Flota, Servicios, snippets con deliberación y Registro, y sigue siendo lector. La F13 sigue con su prompt de reanudación, en el que va R125; la F12, con el suyo; la F11, con el guion del plegable, la CI, la primera release y sus trámites de cierre; la F10, sin validación manual; la F9, pendiente de su primera publicación)

## 1. Visión Global

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy / Hyprland / Alacritty) y compatible con macOS, con una app Android compañera (especificada en la F11 y, desde la F14, también móvil de guardia con Flota, servicios, snippets y registro). Cubre el terreno funcional de Termius —inventario de hosts, identidades, sesiones, SFTP, túneles, sincronización entre dispositivos y, desde la F12, escritorios remotos por RDP, VNC y voxl; desde la F13, los servicios, contenedores y logs de cada host— sin cuenta en un servicio de terceros ni suscripción: la sincronización se hace con un fichero cifrado sobre el transporte que el usuario ya tenga (F10), y las claves privadas nunca salen del agente, el llavero o el token físico.

El diseño de interfaz (once pantallas, sistema visual, modelo de teclado) viene definido en el informe ejecutivo de diseño v0.1 de 14 de septiembre de 2026 y las fases lo implementan sin reinterpretarlo. La propuesta de valor es que el gestor viva en la terminal, se maneje sin ratón y siga estando en el móvil cuando algo se cae a las once de la noche.

**Cliente y contexto.** Producto propio de 4d3 para uso diario de Hector; sin cliente externo. Repositorio `4d3-org/magi` en GitHub. Implementación con Claude Code.

**Distribución (desde la F9).** Software libre (MIT OR Apache-2.0), publicado como paquete `magi-ssh` (el comando sigue siendo `magi`): `magi-ssh-bin` en AUR para Omarchy/Arch, fórmula `magi-ssh` en el tap `4d3-org/homebrew-tap` para macOS y tarballs verificables en cada GitHub Release. Primera versión pública: 1.0.0. La app Android (F11) se publica aparte, como APK firmado en las GitHub Releases con etiquetas `android-vX.Y.Z`, instalable y actualizable con Obtainium; Google Play y F-Droid quedan fuera por ahora.

## 2. Mapa de Módulos

```
  ┌───────────────────────────────────────────────────────────────────────┐
  │ F1  INVENTARIO Y CONEXIÓN  (completada)                               │
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
                    │ F9 EMPAQUETADO Y       │  (implementada; pendiente la primera publicación) magi-ssh 1.0.0 · dist ·
                    │    RELEASES            │  AUR magi-ssh-bin · tap Homebrew · CI · protocolo v6 · magi diagnostico
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F10 SINCRONIZACIÓN     │  (implementada; validación manual pendiente) fichero age firmado por
                    │     CIFRADA            │  equipo en una carpeta elegida · fusión por vectores · conflictos
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F11 APP ANDROID        │  (implementada con la 11b; validación en el plegable pendiente) núcleo compartido ·
                    │                        │  lector del inventario · terminal · túneles · Keystore · APK en GitHub Releases
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F12 ESCRITORIO REMOTO  │  (implementada; validación manual pendiente) RDP (IronRDP) · VNC (RFB propio) · voxl ·
                    │                        │  visor web de voxl en 127.0.0.1 · sesiones en el servidor · formato 2
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F13 SERVICIOS,         │  (implementada; validación manual pendiente) vista F9 · systemd · journalctl · Docker y Podman ·
                    │     CONTENEDORES, LOGS │  Compose · sudo orden a orden · deliberación · validadores · formato 3
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F14 ANDROID, 2ª VUELTA │  (especificada) el móvil de guardia: Flota con sondeo · servicios ·
                    │                        │  snippets con deliberación · registro · sigue siendo lector
                    └───────────┬────────────┘
                                ▼
                    ┌────────────────────────┐
                    │ F15-F21 (propuestas)   │  F15 terminal, 2ª vuelta · F16 despliegues · F17 accesos y
                    │ orden del 8 nov 2026   │  certificados · F18 equipo · F19 caja negra · F20 bases de datos
                    │                        │  por túnel · F21 asistente con IA propia (§7)
                    └────────────────────────┘

  F4, F5 y F6 dependen de F3 (usan el servidor y el pool) y pueden reordenarse entre sí.
  F6 usa REGISTRO y el sondeo de F2. F7 revisa todas las vistas. F8 amplía F4 y usa la deliberación de F6.
  F9 distribuye todo lo anterior y cierra el pendiente de CI de F1. F10 requiere el modelo completo y
  se publica como MINOR. F11 requiere F10 y convierte el repositorio en un workspace con núcleo
  compartido. F12 requiere F11 (núcleo) y usa F3 (servidor y pool), F5 (túneles) y F10 (formato 2);
  F13 usa F2 (sondeo y vigilados), F3 (servidor y pool), F6 (deliberación) y F10 (formato 3); F14 requiere
  F11 y F13 y usa F2 (sondeo), F6 (snippets y deliberación) y F12 (escritorios visibles sin abrirlos).
  Las dependencias de F15-F21 están en el roadmap (§7).
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
| 9 | Empaquetado y Releases | 54 | 2 | 2 | Implementación reportada (54/54; revisión adversarial hecha, 25 hallazgos corregidos; pendiente la primera publicación del desarrollador y comprobar los canales con una release real, R81) |
| 10 | Sincronización Cifrada | 63 | 3 | 3 | Implementación reportada (63/63; revisión adversarial en dos pasadas, todo corregido; limitaciones R83-R88 con propuesta de corrección; pendiente validación manual) |
| 11 | App Android (lector, terminal y túneles) | 117 | 4 | 5 | Implementación reportada (117/117: corrección 11b hecha y validada en el Honor —el túnel local reenvía—; antes, revisión adversarial con 23 hallazgos —1 crítico: la clave del móvil no se creaba—, todos corregidos; 127 pruebas JVM y 8 instrumentadas en el emulador; pendientes el resto de la validación en el plegable, la CI de GitHub y la primera release con keystore) |
| 12 | Escritorio Remoto (RDP, VNC y voxl) | 128 | 4 | 6 | Implementación reportada (128/128; revisión adversarial con su propio informe y correcciones con regresión; IronRDP 0.17 con dos crates vendorizados por la cripto de russh; pendientes la validación manual, el AVC420 con Windows real y la prueba instrumentada de Android) |
| 13 | Servicios, Contenedores y Logs | 95 | 3 | 3,5 | Implementación reportada (95/95; revisión adversarial con su propio informe: 2 críticos, 2 altos, 5 medios y 8 bajos corregidos con regresión salvo B4, R125; pendientes la validación manual, las pruebas instrumentadas de Android y dos retoques del prompt de reanudación) |
| 14 | Android, Segunda Vuelta | 98 | 4 | 5 | Especificada (9 nov 2026; a la espera del informe de implementación) |
| 15 | Terminal, Segunda Vuelta | ~85 (estimado) | — | — | Propuesta (8 nov 2026) |
| 16 | Despliegues y Runbooks | ~70 (estimado) | — | — | Propuesta |
| 17 | Accesos y Certificados SSH | ~70 (estimado) | — | — | Propuesta |
| 18 | Equipo: Inventario Compartido | ~90 (estimado) | — | — | Propuesta (antes, la corrección 10b) |
| 19 | Caja Negra: Grabación y Auditoría | ~55 (estimado) | — | — | Propuesta |
| 20 | Bases de Datos por Túnel | ~60 (estimado) | — | — | Propuesta |
| 21 | Asistente con IA Propia | ~45 (estimado) | — | — | Propuesta |
| **Total** | | **1155 especificadas (~1630 estimadas)** | **46** | **~52,5 semanas (F1-F14)** | |

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
                         │ multiplexar             │
                         │ keepalive_seg           │
                         │ opciones_extra origen   │
                         │ ultimo_estado           │
                         │ ultima_conexion_en      │
                         └───┬────────┬────────┬───┘
 ══ F2 ══════════════════════│════════│════════│═════════════════════════════
 HOSTS + servicios (F2)      │        │        │
 ┌──────────────────────┐    │        │        │     ┌────────────────────────┐
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
 Sin tablas nuevas ni migración 8: el móvil crea magi.db con las migraciones 1-7 del núcleo y la usa como espejo
 de solo lectura (solo la escribe la pasada en modo lector), salvo GRUPOS.plegado y HOSTS.ultimo_estado /
 ultima_conexion_en, propios del móvil. CONFLICTOS solo de huella y borrado; REGISTRO local + clave_instalada.
 Fuera de magi.db: known_hosts privado, secretos cifrados con el Keystore y la clave del móvil en el Keystore.
 ══ F12 ════════════════════════════════════════════════════════════════════
 HOSTS + tipo (ssh | rdp | vnc | voxl, fijo al crear) · escritorio_dominio · escritorio_opciones (JSON)
       + huella_escritorio (certificado RDP; propia del equipo, no viaja)                (migración 8)
 «A través de» = salto_host_id, siempre hacia un host ssh. Sesiones de escritorio en memoria del servidor.
 REGISTRO + escritorio_abierto · escritorio_cerrado · escritorio_fallido · huella_escritorio.
 Sincronización: formato 2 solo si hay hosts que no son ssh (viajan tipo, escritorio_dominio y
 escritorio_opciones).
 ══ F13 ════════════════════════════════════════════════════════════════════
 ┌──────────────────────────────────────────┐
 │ COMPROBACIONES_SERVICIO                  │   SONDEOS + fallos_json: unidades fallidas y contenedores
 │ host_id (FK HOSTS CASCADE) objetivo_tipo │   caídos del sondeo (no viaja)              (migración 9)
 │ objetivo comando actualizado_en          │   REGISTRO + servicio_accion; DELIBERACIONES sin columnas
 │ uuid version vector (viaja)              │   nuevas (PlanDeliberado::Servicio). En memoria del servidor:
 └──────────────────────────────────────────┘   acciones en curso y suscripciones de log.
 UNIQUE(host_id, objetivo_tipo, objetivo). Sincronización: formato 3 solo si hay comprobaciones propias
 (elemento comprobacion_servicio). Solo hosts ssh.
 ══ F14 ════════════════════════════════════════════════════════════════════
 Sin tablas nuevas ni migración 10 (user_version 9). El móvil escribe sus SONDEOS (con fallos_json),
 DELIBERACIONES (usuario «móvil (<equipo>)») y REGISTRO (sondeo_fallido, snippet_ejecutado,
 deliberacion_*, servicio_accion), y los campos propios SNIPPETS.usado_veces y ultimo_uso_en; nada viaja.
 Lee VERIFICACIONES_HOST, SNIPPETS, SNIPPET_DESTINOS y COMPROBACIONES_SERVICIO. En memoria de magi-movil:
 conexiones con gracia, acciones, anillo de logs, ejecuciones, deliberaciones y la contraseña de sudo.
```

La F11 no añade tablas (confirmado al especificarla) y la F12 tampoco: amplía HOSTS (migración 8). La F13 añade COMPROBACIONES_SERVICIO y amplía SONDEOS (migración 9). La F14 no añade tablas ni migración: el móvil empieza a escribir sus propios SONDEOS, DELIBERACIONES y REGISTRO. La previsión `SINCRONIZACION_INVENTARIO` de antes quedó concretada en la F10 como EQUIPOS, LAPIDAS y CONFLICTOS.

## 5. Glosario de Dominio

| Término de negocio | Entidad/Tabla | Fase | Notas |
|---|---|---|---|
| Host | HOSTS | 1 | Equipo remoto; `nombre` es el alias `Host` de ssh_config |
| Grupo | GRUPOS | 1 | Carpeta plegable del inventario; un host tiene como mucho un grupo |
| Etiqueta | ETIQUETAS / HOST_ETIQUETAS | 1 | Clasificación libre N:N de hosts; destino de snippets (F6) por nombre |
| Salto | HOSTS.salto_host_id | 1 | `ProxyJump`; autorreferencia |
| Identidad | HOSTS.identidad_ref (F1) / IDENTIDADES (F2) | 1 / 2 | Referencia a clave del agente, fichero o token, o marca `contrasena` / `contrasena:llavero`; nunca el secreto. Revocar = baja lógica de la referencia |
| Llavero | `src/llavero.rs` (secret-tool / Keychain) | 2 | Único lugar donde vive una contraseña de host; `magi.db` solo guarda la referencia; en Android, `LlaveroMovil` (secretos cifrados con una clave del Keystore, F11) |
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
| Huella | `~/.ssh/known_hosts` | 1 | Sin tabla propia; se comparte con el ssh del sistema; en Android, un `known_hosts` privado de la app con la misma política (F11) |
| Opciones extra | HOSTS.opciones_extra | 1 | Directivas ssh_config literales no modeladas |
| magi_config | `~/.ssh/magi_config` | 1 | Exportación del inventario incluida por `Include` |
| Flota | vista F1 sobre HOSTS + SONDEOS | 2 / 14 | Panel de estado; vista de arranque. En el móvil (F14) sustituye a Hosts y es la pantalla de inicio |
| Sondeo | SONDEOS | 2 | Resultado de ejecutar `sondeo.sh` en un host; 20 por host; desde la F13, con sus fallos (`fallos_json`) |
| Servicios vigilados | HOSTS.servicios | 2 | Unidades systemd que vigila el sondeo; en la vista Servicios (F13) salen arriba con `★` |
| Estado de flota | derivado de SONDEOS | 2 | FRÍA / CAÍDA / ALCANZABLE / CARGA / NOMINAL según umbrales de `config.toml` |
| Registro | REGISTRO | 2 | Historial de conexiones, huellas, importación/exportación, claves, sondeos, (F3) sesiones y servidor, (F4) `transferencia` y `borrado_remoto`, (F5) `tunel_abierto`, `tunel_cerrado`, `tunel_fallido`, (F6) `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`, (F8) `sincronizacion`, `permisos_cambiados`, (F12) `escritorio_abierto`, `escritorio_cerrado`, `escritorio_fallido`, `huella_escritorio`, (F13) `servicio_accion` |
| Túnel | TUNELES | 5 | Local (`-L`), remoto (`-R`) o dinámico (`-D`, SOCKS5); definido en SQLite, ejecutado en el servidor sobre el pool (`VERSION_PROTOCOLO = 3`); automático = ligado a la primera/última pestaña o SFTP del host |
| Reenvío | `sshconfig/tuneles.rs` | 5 | `LocalForward`/`RemoteForward`/`DynamicForward` de ssh_config, convertidos a túneles y devueltos al exportar |
| Snippet | SNIPPETS + SNIPPET_DESTINOS | 6 | Comando guardado con variables `{{var}}`, dirigido a etiquetas de host (por nombre) y hosts sueltos; «crítico» fuerza deliberación |
| Ejecución | servidor, en memoria (F6) | 6 | Lanzamiento de un snippet ya resuelto en N hosts; salida por host con tope, 1 h; `VERSION_PROTOCOLO = 4` |
| Snippet al conectar | HOSTS.snippet_al_conectar_id | 6 | Snippet no crítico y sin variables escrito en la pestaña al abrirla |
| Verificaciones previas | VERIFICACIONES_HOST | 6 | Qué comprobaciones exige cada host: salud, backup (ruta, patrón), tests (comando local) |
| Deliberación MAGI | DELIBERACIONES | 6 / 8 / 13 | Pausa antes de ejecutar: MELCHIOR-1 salud, BALTHASAR-2 backup, CASPER-3 tests; unanimidad, `Ctrl+K`, forzado con motivo; desde la F13, también antes de las acciones de servicio que dejan algo parado (MELCHIOR-1 no cuenta el propio objetivo) |
| Modo estrecho | `ui/disposicion.rs` (F7) | 7 | Disposición de una vista con ancho < 100: un panel con `Tab`, columnas secundarias ocultas |
| Tamaño mínimo | `ui/disposicion.rs` (F7) | 7 | Por vista (global 40×12); por debajo se pinta un aviso en lugar de la vista |
| Edición remota | `archivos/edicion.rs` · `app/edicion.rs` (F8) | 8 | `E`: temporal en `…/magi/ediciones` + `$EDITOR` + detección por SHA-256 + conflicto por `StatRemoto`; el temporal nunca se borra sin subir, descartar o guardar una copia local comprobada |
| Sincronización de directorio | SINCRONIZACIONES_DIR (F8) | 8 | Plan recursivo local ⇄ remoto (crear, actualizar, borrar opcional), con exclusiones y `.magiignore`; protocolo `VERSION_PROTOCOLO = 5` (`StatRemoto`, `CambiarPermisos`, `ListarArbol`, `borrar_al_terminar`, `SincronizacionLanzada`); no confundir con la sincronización cifrada del inventario (F10) |
| `.magiignore` | raíz del directorio origen (F8) | 8 | Exclusiones con sintaxis de gitignore; lo excluido nunca se borra en destino. Está en `[archivos] excluir` por defecto, así que no se sincroniza salvo que se quite de la lista |
| Renombrador | `servidor/sftp.rs` (F8) | 8 | Sustituye un fichero remoto con `posix-rename@openssh.com` por un segundo canal SFTP raw o, si el host no lo anuncia, en tres pasos con deshacer (`.magi-viejo`); corrige el «sobrescribir» de la F4 |
| Forzado | DELIBERACIONES.motivo + REGISTRO `deliberacion_forzada` | 6 | Ejecutar pese a un rechazo; motivo ≥ 10 caracteres, usuario y hora |
| Paquete `magi-ssh` | `Cargo.toml` (F9) | 9 | Nombre del paquete en AUR (`magi-ssh-bin`), Homebrew y los tarballs; el comando, la biblioteca y las rutas de datos siguen llamándose `magi` |
| Release | GitHub Release `vX.Y.Z` (F9) | 9 | Tarballs de 4 objetivos con `.sha256` y atestaciones, notas del CHANGELOG; la crea `dist` desde un tag, nunca a mano; no es una tabla; las de la app Android usan `android-vX.Y.Z`, las crea `android-release.yml` y nunca son «Latest» (F11) |
| Servidor anterior | `VersionIncompatible` (F9) | 9 | Servidor de sesiones de otra versión al abrir `magi`; se sustituye solo si no custodia nada (trabajo y clientes a cero); si no, decide el usuario |
| Trabajo del servidor | `VersionIncompatible.trabajo` (F9) | 9 | Sesiones, transferencias en cola o en curso, túneles activos y ejecuciones en curso; desde la F12, escritorios abiertos, y desde la F13, acciones de servicio en curso |
| Diagnóstico | `magi diagnostico` (F9) | 9 | Comprobación de lo que MAGI necesita, sin escribir ni conectar; `✓ ! ✕` y código 0 o 1 |
| Sincronización del inventario | `src/sincronizacion/` (F10) | 10 | El mismo inventario en todos los equipos del usuario por ficheros cifrados en una carpeta; no confundir con la sincronización de directorio (F8) ni con `magi sincronizaciones` |
| Equipo | EQUIPOS (F10) | 10 | Cada instalación de MAGI que sincroniza; su fichero es `magi-<uuid>.age`; estados `este`, `al_dia`, `ilegible`, `danado`, `formato_nuevo`, `quitado`; la app Android (F11) lee pero no escribe fichero, así que no figura como equipo en los escritorios |
| Fichero de sincronización | `magi-<uuid>.age` (F10) | 10 | Instantánea completa del inventario de un equipo: cabecera, firma HMAC y JSON `formato` 1, cifrado con `age`; formato en `docs/formato-sincronizacion.md` |
| Clave de sincronización | llavero (`sincronizacion`) (F10) | 10 | Identidad `age` X25519 generada por MAGI; solo en el llavero; se muestra como texto y QR; quitar un equipo la cambia; la firma HMAC se deriva de su texto canónico en mayúsculas; la anterior solo lee, hasta `clave_anterior_hasta` |
| Versión y vector | `version`, `vector` (F10) | 10 | Última edición (`<ms>.<uuid equipo>`) y edición vista de cada equipo; deciden si un cambio deriva de otro o es concurrente |
| Lápida | LAPIDAS (F10) | 10 | Borrado que viaja como una versión más; se conserva siempre. Un borrado aplazado no deja lápida local |
| Conflicto de sincronización | CONFLICTOS (F10) | 10 | Versión local que perdió (`elemento`), huella distinta (`huella`), borrado aplazado por trabajo vivo (`borrado`), renombrado (`nombre`) o huérfano; recuperable 90 días |
| Núcleo | `crates/magi-nucleo` (F11) | 11 | Crate Rust compartido por el escritorio y el móvil: almacén, sincronización, conexión, túneles, parser y teclas del terminal, registro; no conoce la plataforma (rutas y servicios inyectados) |
| `magi-movil` | `crates/magi-movil` (F11) | 11 | Biblioteca nativa (cdylib) con UniFFI que expone el núcleo a Kotlin y hace en el móvil el papel del servidor de sesiones, dentro de la app |
| App Android | `android/` (F11) | 11 / 14 | App Kotlin + Compose (`org.cuatrodetres.magi`) con versión propia (`android-vX.Y.Z`); en la F11, lectora del inventario, terminal y túneles; desde la F14, también Flota, Servicios, snippets con deliberación y Registro |
| Modo lector | pasada de sincronización (F11) | 11 | Pasada que nunca exporta ni escribe en la carpeta, sin copia ni confirmación de borrados y solo con conflictos de huella y de borrado; la usa el móvil |
| Clave del móvil | Android Keystore, `magi-clave-movil` (F11) | 11 | Clave SSH ECDSA P-256 creada en el hardware del teléfono, no exportable; se instala en los hosts con «instalar mi clave»; desde el 1 nov 2026, sin `setUnlockedDeviceRequired`, y las instalaciones anteriores la recrearon una vez con otro alias (T96) |
| LlaveroMovil | retrollamada de Kotlin (F11) | 11 | Implementación de `LlaveroSecreto` en Android: ficheros cifrados con AES-GCM y una clave del Keystore |
| Autenticación en cadena | `magi-movil` (F11) | 11 | En el móvil: clave del móvil, luego contraseña guardada, luego diálogo; la identidad del escritorio no se usa |
| Puerto libre | túneles del móvil (F11) | 11 | Salida cuando la escucha de un túnel está ocupada o es menor que 1024: se abre en un puerto que elige el sistema, se muestra y no se guarda |
| Fila de teclas extra | terminal del móvil (F11) | 11 | Teclas que faltan en el teclado del móvil (Esc, Tab, Ctrl, Alt, flechas, símbolos, F1-F12), con Ctrl y Alt de una pulsación o fijos |
| Servicio de sesiones del móvil | servicio en primer plano `specialUse` (F11) | 11 | Mantiene vivas sesiones y túneles con la app en segundo plano; notificación sin acciones |
| Vigilante de conexión | gestor de `magi-movil` (F11, 11b) | 11 | Mira cada 500 ms si el host cerró la conexión; si es así, sus sesiones y túneles pasan a caídos con el motivo (T97) |
| Escritorio | HOSTS con `tipo` rdp, vnc o voxl (F12) | 12 | Host que se abre como escritorio gráfico en el visor; tiene grupo, etiquetas y sincronización como los demás |
| Tipo de host | HOSTS.tipo (F12) | 12 | `ssh`, `rdp`, `vnc` o `voxl`; se fija al crear el host; solo los `ssh` van a `magi_config` y a terminal, SFTP, túneles, snippets y saltos |
| A través de | HOSTS.salto_host_id (F12) | 12 | En un escritorio, el host SSH por cuya conexión se llega (canal `direct-tcpip`); obligatorio en voxl |
| Visor | `visor/` (F12) | 12 | Cliente web de voxl adaptado (MIT): pinta rectángulos y H.264 en el navegador, con teclado, ratón, portapapeles, tamaño y pantalla completa |
| Servidor del visor | `escritorio/visor_http.rs` en `magi --servidor` (F12) | 12 | HTTP y WebSocket solo en `127.0.0.1`, con `Host`, `Origin` y CSP; vive mientras hay sesiones de escritorio |
| Enlace de un solo uso | servidor del visor (F12) | 12 | URL `…/entrar?t=` que abre el visor; caduca a los 60 s o al canjearla por la clave del visor |
| Clave del visor | `sessionStorage` del visor (F12) | 12 | Secreto por sesión que autentica el WebSocket; uno nuevo echa al visor anterior |
| Puente | `escritorio/puente.rs` (F12) | 12 | Pasa al visor lo que entrega RDP o VNC: rectángulos en bruto, copias, H.264 de Windows tal cual y cursor; devuelve teclado y ratón |
| Huella del certificado | HOSTS.huella_escritorio (F12) | 12 | `SHA256:` del certificado RDP aceptado; misma política que las huellas SSH; propia del equipo |
| voxl | programa en el host (F12) | 12 | Servidor de escritorio para el navegador (H.264 por WebSocket) que MAGI arranca por SSH en `127.0.0.1` y para al cerrar; lo instala el usuario |
| Lanzadera | servidor del visor (F12) | 12 | En voxl, la página que canjea el enlace y lleva el navegador a voxl por el túnel de la sesión |
| Gracia | `[escritorio] gracia_seg` (F12) | 12 | Tiempo (60 s) que una sesión de escritorio sigue abierta sin visor antes de cerrarse |
| Vista Servicios | `F9` (F13) | 13 | Hosts SSH a la izquierda y, del elegido, servicios, timers y contenedores; también con `u` desde Flota, Hosts y la ficha |
| Unidad | systemd del host (F13) | 13 | Servicio o timer de systemd del sistema; no es una tabla: se lee en cada refresco con `systemctl show` |
| Timer | unidad `.timer` (F13) | 13 | Unidad que lanza un servicio a su hora; MAGI enseña la próxima y la última ejecución y lo lanza ahora |
| Contenedor | Docker o Podman del host (F13) | 13 | Se lista con `ps -a` y se agrupa por proyecto Compose; no es una tabla |
| Proyecto Compose | etiqueta `com.docker.compose.project` (F13) | 13 | Grupo de contenedores que se arranca, se para y se reinicia junto, con sus ficheros de compose |
| Motor de contenedores | `[servicios] contenedores` (F13) | 13 | Docker o Podman: se usa el primero de la lista que haya en el host |
| Acción de servicio | servidor, en memoria (F13) | 13 | Arrancar, parar, reiniciar, recargar o cambiar el arranque de un objetivo; custodiada como trabajo y con espera al resultado; REGISTRO `servicio_accion` |
| Comprobación de configuración | `servicios/validadores.rs` (F13) | 13 | Validación dentro de la acción de recargar o reiniciar (Nginx, Caddy, Apache, HAProxy, Compose); si falla, no se ejecuta nada |
| Comprobación propia | COMPROBACIONES_SERVICIO (F13) | 13 | Orden del usuario que sustituye a la comprobación conocida de un servicio o un proyecto; viaja como `comprobacion_servicio` |
| Unidad crítica | `[servicios] criticas` (F13) | 13 | Unidad que puede cortar la conexión con el host (sshd, red, cortafuegos, Tailscale, Docker); avisa en rojo |
| Fallos | SONDEOS.fallos_json (F13) | 13 | Unidades fallidas y contenedores caídos de un host en su último sondeo; `⚠N` en Flota; no cambian el estado del host |
| Toda la flota | vista Servicios (F13) | 13 / 14 | Los fallos de todos los hosts juntos, del último sondeo de cada uno, sin abrir conexiones; en el móvil, la primera pantalla de SERVICIOS (F14) |
| Panel de logs | vista Servicios (F13) | 13 | Logs en vivo de una unidad o un contenedor, con filtros y pausa, en un anillo en memoria de 10 000 líneas y 8 MiB |
| Pestaña con orden | `AbrirSesion.orden` (F13) | 13 | Pestaña de Sesión que ejecuta una orden armada por el servidor (shell de un contenedor o seguimiento de logs) y se cierra al acabar |
| Privilegio | listado de servicios (F13) | 13 | `root`, `sudo_sin_clave`, `sudo_con_clave` o `sin_sudo`: cómo hace MAGI lo que pide permisos en ese host |
| Contraseña de sudo | memoria del cliente (F13) / de `magi-movil` (F14) | 13 / 14 | Se pide en un diálogo, se comprueba con `ComprobarSudo` (en el móvil, `sudo_comprobar`) y se olvida a los 5 min sin uso; en el móvil, también al bloquearse MAGI; nunca se guarda |
| Móvil de guardia | app Android (F14) | 14 | El móvil con Flota, Servicios, snippets con deliberación y Registro; sigue siendo lector de la sincronización |
| Gracia (móvil) | conexión del gestor de `magi-movil` (F14) | 14 | 60 s que una conexión sin usos sigue abierta para que Flota, Servicios y las comprobaciones la reutilicen |
| Apertura automática | `Automatica` en `magi-movil` (F14) | 14 | Conexión que abre algo automático (sondeo, refresco, comprobación, ejecución): nunca dialoga y falla con instrucción |
| Ventana de logs | `logs_ventana` (F14) | 14 | Trozo ya filtrado del anillo de logs que pide la app para pintar; el anillo vive en Rust |
| Mantener pulsado | `BotonMantener` (F14) | 14 | El `Ctrl+K` del móvil: lo deliberado se ejecuta manteniendo pulsado el botón 1 s |
| Proveedor de comprobación | `deliberacion/comprobaciones.rs` (F14) | 14 | Lo que pone cada plataforma para deliberar (sondear, listar un directorio, orden local); en el móvil, CASPER-3 da «solo en el escritorio» |
| Ajustes del móvil | DataStore (F14) | 14 | Lo que en el escritorio está en `config.toml` (umbrales, refrescos, sudo, contenedores, backup y salud), con los mismos valores por defecto |
| Resultados | ejecuciones en memoria de `magi-movil` (F14) | 14 | Pantalla del móvil con las ejecuciones de snippets y la salida de cada host; se olvidan 1 h después de acabar |

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
| 9 — Empaquetado y Releases | 54 / 54 | 30 oct 2026 (procesado el 31; una sesión de Claude Code en `fase9-empaquetado`; 951 pruebas —485 unitarias y 466 de integración—; revisión adversarial en dos frentes, código y cadena de suministro, con 25 hallazgos verificados y corregidos: entre ellos la causa de R42 —el manejador de `SIGTERM` se instalaba después del socket—, un servidor más nuevo que se podía parar desde la CLI, el pid de un rechazo manipulado, una `magi.db` futura con WAL modificada al rechazarla, `servidor parar` sin contar transferencias, todos los secretos pasados al job de AUR y el choque con el paquete AUR `magi`; `gitleaks` limpio y datos de ejemplo saneados. Pendiente del desarrollador: hacer público el repositorio, tap, cuenta de AUR, secretos, ensayo `v1.0.0-rc.1` y `v1.0.0`; los workflows aún no se han ejecutado en GitHub) |
| 10 — Sincronización Cifrada | 63 / 63 | 31 oct 2026 (Claude Code, versión 1.1.0; revisión adversarial en dos pasadas —funcional y de seguridad, 16 reproducciones—: un cambio de clave no recifraba el fichero propio, borrados aplazados que vaciaban referencias o chocaban con nombres y atascaban la sincronización, enlaces simbólicos en la carpeta, equipo quitado importado en la misma pasada, la clave anterior usada para exportar…, todo corregido con test; `age` fijado en 0.11.5; limitaciones R83-R88; pendiente validación manual con llavero real, Syncthing entre dos equipos, QR y `age` real) |
| 11 — App Android | 117 / 117 | 4 nov 2026, procesado el mismo día (actualización con la corrección 11b); el primero, del 31 oct, se procesó el 1 nov (Claude Code con un Lead y pistas en paralelo; commits hasta `5495161`; revisión adversarial con 1 crítico —la clave del móvil no se creaba porque se usaba `KeyGenerator` en vez de `KeyPairGenerator`, y toda conexión fallaba—, 5 altos —conexiones caídas cacheadas, comandos perdidos durante «abriendo», esperas sin plazo, carpeta SAF olvidada al reiniciar y una pasada cancelada que dejaba la fase colgada—, 7 medios y 10 bajos, todo corregido; R91 cerrado; 126 pruebas JVM, 3/3 instrumentadas en API 30, `lint` con 0 errores y Rust en verde; pendientes la validación en el plegable, la CI de GitHub, la primera release con keystore y corregir `docs/formato-sincronizacion.md` §11.9). **Validación en el plegable (4 nov 2026):** un túnel local activo no reenviaba; se desmarcó la mecánica de túneles y se añadió la corrección 11b (7 funcionalidades). **Corrección 11b (4 nov 2026):** con solo túneles nadie vigilaba la conexión del host y, si se caía, el túnel seguía `activo`; ahora cada conexión tiene vigilante y la fila enseña abiertas · aceptadas, el último error con su hora y el puerto real; prueba de extremo a extremo en Rust (falla sin el vigilante) y en el emulador (con servidor de pruebas en el anfitrión, omitida en CI); 127 pruebas JVM y 8/8 instrumentadas; validada en el Honor. El informe recoge además las claves del Keystore sin `setUnlockedDeviceRequired` (1 nov, desbloqueo facial) y, a petición de Hector, el terminal a pantalla completa y sin panel vacío en el plegable. Pendientes: el resto del guion en el plegable, la CI, la release y los trámites de cierre (R103, R104 y R107) |
| 12 — Escritorio Remoto | 128 / 128 | 5 nov 2026, procesado el mismo día (Claude Code; en el árbol de trabajo, pendiente de subir; versión 1.2.0, migración 8, protocolo 7 y `formato` 2. Revisión adversarial en `docs/fase12/magi-fase12-revision-adversarial.md`, con las correcciones y sus pruebas. Pruebas en verde: 459 de la biblioteca del escritorio, 292 de redimensionado, 27 del visor, 24 de RFB, 19 de interfaz, 14 del servidor de escritorios, 12 de RDP, 9 de voxl, 7 de sincronización y la de extremo a extremo; `fmt`, `clippy`, `deny`, `actionlint`, `shellcheck`, `dist plan`, `npm run check` y `generar-visor --comprobar`. Desviaciones: `ironrdp-connector` e `ironrdp-acceptor` vendorizados con `sspi` 0.23.1 y `picky` 7.0.0-rc.27 (R118), `ironrdp-egfx` 0.3 con un `DvcProcessor` propio para el AVC420, protocolo del visor ampliado (`GET /`, `rec`, `latencia`). Pendientes: guion de validación manual, AVC420 con Windows 11 real (el `ironrdp-server` de pruebas no lo genera), prueba instrumentada de Android (AVD bloqueado) y la imagen entera al volver con H.264, R117) |
| 13 — Servicios, Contenedores y Logs | 95 / 95 | 8 nov 2026, procesado el mismo día (Claude Code, rama `fase13-servicios_contenedores_logs`; versión 1.3.0, migración 9, protocolo 8 y `formato` 3. Revisión adversarial en `docs/fase13/magi-fase13-revision-adversarial.md`: dos críticos —dos ventanas se pisaban las acciones y los logs al compartir `peticion_id`, y una suscripción de log perdía su conexión a los ~30 s—, dos altos y cinco medios, todo corregido con prueba de regresión salvo B4. 1 570 pruebas en verde (49 binarios); `clippy`, `fmt`, `deny`, `shellcheck` y `recursos`. Desviaciones: el sondeo detecta los motores sin `[servicios] contenedores` (R125), el script detecta el motor del contenedor, gramática ampliada de `servicios.sh`, topes de bytes en los logs (anillo de 8 MiB, lotes de 2 MiB), la contraseña de sudo solo se renueva con las acciones, la comprobación propia con root va con `sudo -- sh -c` (R126), `[servicios]` fuera de rango se acota con aviso y la excepción RUSTSEC-2026-0441 en `deny.toml`. Pendientes: guion manual, pruebas instrumentadas de Android, R125 y las secciones vacías de un host sin systemd) |
| 14 — Android, Segunda Vuelta | 0 / 98 (sin confirmar) | — (especificada el 9 nov 2026; a la espera del informe de implementación) |

**Progreso total acumulado:** 1053 / 1155 funcionalidades (la F14, especificada y sin informe de implementación, cuenta 0 / 98; de las anteriores faltan CI y macOS de la F1, la revisión adversarial de S1-S3 de la F6 y la revisión adversarial de la F8). La línea de CI de la F1 se marcará cuando `ci.yml` pase de verdad en GitHub (la F9 lo dejó listo, pero todavía no se ha ejecutado).

La Fase 1 se da por completada: las dos funcionalidades sin marcar (CI y macOS) siguen abiertas en su checklist y se cierran cuando exista el remoto y un mac; no bloquean la Fase 2.

## 7. Roadmap

Reordenado el 15 sep 2026 tras elegir Hector la propuesta «solo pestañas» (ampliada a servidor de sesiones) como Fase 3 y SFTP como Fase 4.

**8 nov 2026:** Hector aceptó el orden propuesto para las fases 13 a 21, ocho fases de funcionalidad nueva más la segunda vuelta de Android: F13 Servicios, F14 Android, F15 Terminal, F16 Despliegues, F17 Accesos y certificados, F18 Equipo, F19 Caja negra, F20 Bases de datos por túnel y F21 Asistente con IA propia. Se quedaron fuera la segunda vuelta del escritorio remoto (mejor después de validar la F12), más tipos de conexión (serie, telnet, shell local) e importar hosts de Hetzner o Tailscale.

**9 nov 2026:** especificada la F14 como móvil de guardia. Quedan para más adelante, sin fase, el SFTP y los escritorios en el móvil; el móvil escritor, con la F18.

1. **Fase 3 — Pestañas y Servidor de Sesiones.** Implementada el 15 sep 2026 (78/79 tras desmarcar la reutilización del pool, R28); R12 y R13 de la Fase 2 incluidos.
2. **Fase 4 — Archivos (SFTP en panel doble).** Implementada el 15 sep 2026 (65/65); pendiente de validación manual. Sin `chmod`, sin papelera, sin sincronizar directorio (`chmod` y sincronizar llegan en la F8). La F8 corrigió tres defectos suyos (informe v1.4).
3. **Fase 5 — Túneles.** Implementada el 16 sep 2026 (54/54); pendiente de validación manual completa.
3b. **Corrección del pool de la F3:** hecha como primer bloque de la Fase 6 (27 sep 2026); R28-R31 cerrados.
4. **Fase 6 — Snippets y Deliberación MAGI.** Implementada el 27 sep 2026 (79/80, 9 de ellas la corrección 3b); la revisión adversarial de S1-S3 queda sin hacer por decisión de Hector (27 sep 2026: PR y paso a la F7); pendiente la validación manual. Vista `F8`. Fuera: deploy con subida de ficheros, snippet que exige túnel, ventana de mantenimiento, ejecutar solo en los aprobados, ejecución desde la CLI.
5. **Fase 7 — Redimensionado Adaptable (fix).** Implementada el 27 sep 2026 (46/46) y validada por Hector el 3 oct 2026.
6. **Fase 8 — Archivos, Segunda Vuelta.** Especificada e implementada el 3 oct 2026 (63/64): edición remota, `chmod`, propietarios con nombre, sincronizar directorio con sincronizaciones guardadas y deliberación. La revisión adversarial queda sin hacer por decisión de Hector (R50; el prompt de reanudación la deja preparada); pendiente la validación manual. Fuera: `chown`, papelera, sincronización por hash o continua, limpieza de temporales de edición huérfanos (R51).
7. **Fase 9 — Empaquetado y Releases.** Especificada el 30 oct 2026 (54 funcionalidades, 2 sprints), aceptando las opciones recomendadas salvo la versión: `1.0.0` (Hector). Paquete `magi-ssh`, repositorio público con MIT OR Apache-2.0, releases con `dist`, AUR `magi-ssh-bin`, tap de Homebrew sin notarizar, CI con macOS y `cargo deny`, protocolo v6 con saludo estable y sustitución segura del servidor anterior, `magi diagnostico`. Se especifica con la F8 sin revisión adversarial ni validación manual (R50, R59). Fuera: notarización, .deb/.rpm, crates.io, `curl | sh`, Windows. **Implementada el 30 oct 2026 (54/54)**, con revisión adversarial (25 hallazgos corregidos); falta la primera publicación siguiendo `docs/publicar.md`: transferir y hacer público el repositorio, tap, cuenta de AUR, secretos, ensayo `v1.0.0-rc.1` y `v1.0.0`.
8. **Fase 10 — Sincronización Cifrada** (antes F9). Especificada el 30 oct 2026 a petición de Hector sin esperar al informe de la F9 (63 funcionalidades, 3 sprints, todas las opciones recomendadas); base confirmada el 31 oct al procesar ese informe, sin cambios de alcance (informe v1.1). `magi.db` **no** se cifra en el disco (cambia el «cifrado en reposo y desbloqueo al arrancar» previsto): se cifra lo que sale del equipo. Un fichero `age` firmado por equipo en una carpeta elegida, fusión por vectores con conflictos recuperables, lápidas, huellas sin sustitución automática, unión por nombre, clave en el llavero con QR, quitar equipo cambia la clave, copia previa y confirmación de borrados masivos, sincronización automática mientras MAGI está abierto. Se publicará como 1.1.0. Fuera: SFTP como transporte, historial, compartir con otras personas, sincronizar con MAGI cerrado. **Implementada el 31 oct 2026 (63/63).** Limitaciones conocidas con propuesta de corrección (R83-R88): sobre todo que `magi sincronizar` no consulta el trabajo vivo del servidor, la versión que se sube en dos pasos y el fichero truncado confundido con una clave distinta. Conviene corregirlas antes de publicar la 1.1.0.
9. **Fase 11 — App Android** (antes F10). **Especificada el 31 oct 2026** (110 funcionalidades, 4 sprints, ~5 semanas): Kotlin + Compose con núcleo Rust compartido por UniFFI en un workspace (`magi-nucleo`, `magi-movil`); el móvil es lector del inventario (no escribe en la carpeta), con terminal SSH y túneles; clave del móvil en el Keystore, bloqueo con huella o PIN y servicio en primer plano; Android 11+, arm64 y diseño para plegable; APK en GitHub Releases (`android-v0.1.0`) con Obtainium. Hector aceptó las opciones recomendadas salvo dos: sin corrección 10b (1b) e inventario de solo lectura y terminal, más túneles (2c). **Implementada el 31 oct 2026 (110/110).** **Validación en el plegable (4 nov 2026):** un túnel local activo no reenviaba; la corrección 11b se hizo y se validó en el Honor el mismo día (117/117). Faltan el resto del guion de validación (informe, §11), la CI de GitHub, la primera release con keystore (`android-v0.1.0-rc.1` y `android-v0.1.0`) y los trámites de cierre del prompt de reanudación (R103, R104 y R107).
10. **Fase 12 — Escritorio Remoto.** **Especificada el 4 nov 2026** (128 funcionalidades, 4 sprints, ~6 semanas), aceptando todas las opciones recomendadas: RDP con IronRDP, VNC con un cliente RFB propio y voxl arrancado por SSH en hosts Linux con X11; el visor es el cliente web de voxl adaptado, servido por `magi --servidor` solo en `127.0.0.1` con enlaces de un solo uso y abierto en Chromium en modo app; sin recomprimir ni x264 (MAGI sigue MIT/Apache); por SSH siempre que se pueda; escritorios como hosts con `tipo` y «a través de»; sesiones custodiadas con un minuto de gracia sin visor; `formato` 2 de sincronización solo con escritorios. Versión 1.2.0. Fuera: sonido, ficheros, imágenes en el portapapeles, varias pantallas, RDP antiguo, Macs y abrir escritorios en el móvil. Depende de F11 (núcleo) y usa F3, F5 y F10. **Implementada el 5 nov 2026 (128/128)**, pendiente de subir: IronRDP 0.17 con `ironrdp-egfx` 0.3 y dos crates vendorizados (D228-D229 de su informe). Faltan el guion de validación manual (informe, §11), el AVC420 con Windows real, la prueba instrumentada de Android y la imagen entera al volver con H.264 (R117), que va en el prompt de reanudación.
11. **Fase 13 — Servicios, Contenedores y Logs.** **Especificada el 8 nov 2026** (95 funcionalidades, 3 sprints, ~3,5 semanas), aceptando todas las opciones recomendadas: vista `F9` (y `u`) con servicios y timers de systemd, journalctl y contenedores de Docker o Podman con Compose; acciones con espera al resultado; logs en vivo en un panel y en una pestaña; sudo orden a orden con la contraseña en memoria 5 min; deliberación para lo que deja algo parado y comprobación de configuración dentro de la acción; fallos del sondeo en Flota y «Toda la flota»; `formato` 3 solo con comprobaciones propias. Versión 1.3.0. Fuera: unidades de usuario, Kubernetes, borrar o actualizar contenedores, acciones en varios hosts y avisos de escritorio. Depende de F2, F3, F6 y F10. **Implementada el 8 nov 2026 (95/95)**, con la revisión adversarial hecha (D255-D262 de su informe). Faltan el guion de validación manual (informe, §11), las pruebas instrumentadas de Android y dos retoques que van en el prompt de reanudación: que el sondeo respete `[servicios] contenedores` (R125) y las secciones vacías de un host sin systemd.
12. **Fase 14 — Android, Segunda Vuelta** (antes F12 y F13). **Especificada el 9 nov 2026** (98 funcionalidades, 4 sprints, ~5 semanas), aceptando todas las opciones recomendadas: el móvil de guardia, que sigue siendo lector. Flota sustituye a Hosts y es la pantalla de inicio (sondeo al abrir, al deslizar y cada 30 s a la vista; nada en segundo plano: los avisos los da Perseverus); Servicios como en la F13, ejecutado por `magi-movil`, con la contraseña de sudo en memoria 5 min; snippets sincronizados en uno o varios hosts con deliberación (MELCHIOR-1 y BALTHASAR-2; CASPER-3 rechaza «solo en el escritorio»), y lo deliberado se ejecuta manteniendo pulsado 1 s; Registro del móvil. El sondeo y las comprobaciones de la deliberación pasan al núcleo sin cambiar el escritorio; servidor de pruebas en el job `humo` (R106); app 0.2.0. Fuera: editar el inventario (el móvil escritor, con la F18 o más adelante, después de la 10b), SFTP, abrir escritorios en el móvil, avisos en segundo plano y CASPER-3 en el móvil. Depende de F11 y F13, con los cierres de F11-F13 hechos (R125 incluido), y usa F2 y F6.
13. **Fase 15 — Terminal, Segunda Vuelta** (propuesta): paneles en la vista Sesión, búsqueda en el historial, copiar con el teclado, abrir enlaces, escribir en varias pestañas a la vez con aviso, reconexión automática y `tmux` remoto si el host lo tiene. Depende de F3 y F7.
14. **Fase 16 — Despliegues y Runbooks** (propuesta): recetas de varios pasos (comando, subir ficheros, sincronización guardada, reiniciar un servicio, esperar a que responda) que paran al fallar y tienen vuelta atrás, con historial; lo que la F6 dejó fuera (D72, D75): ventana de mantenimiento, ejecutar solo en los aprobados y lanzar desde la CLI; actualizar contenedores (`pull` y `up`). Depende de F6, F8 y F13.
15. **Fase 17 — Accesos y Certificados SSH** (propuesta): mapa de qué claves entran en qué hosts, rotación de claves en la flota, certificados SSH con autoridad propia (de usuario y de host), claves FIDO2 y reenvío del agente con confirmación. Depende de F2 y F6; reutiliza el sudo de la F13 (T105).
16. **Fase 18 — Equipo: Inventario Compartido** (propuesta): compartir grupos de hosts, snippets y huellas verificadas con otras personas, con papeles de lector y editor comprobados por firma, invitación por QR y cambio de clave al quitar a alguien, sobre la carpeta de la F10. Necesita antes la corrección 10b; mejor después de la F17. Es también la candidata para que el móvil pase a escribir (editar el inventario), lo que la F14 dejó fuera (D263).
17. **Fase 19 — Caja Negra: Grabación y Auditoría** (propuesta): grabación opcional y cifrada de sesiones, reproductor con búsqueda, registro encadenado que delata manipulaciones e informes de actividad exportables. Cambia T44 solo para lo que se grabe. Depende de F2 y F3.
18. **Fase 20 — Bases de Datos por Túnel** (propuesta): fichas de PostgreSQL, MySQL o MariaDB, Redis y SQLite remoto con túnel automático, cliente en una pestaña o vista de consultas de solo lectura. Depende de F5.
19. **Fase 21 — Asistente con IA Propia** (propuesta, apagado por defecto): explicar errores, resumir salidas y proponer comandos con un modelo que elige el usuario; lo que propone pasa por la deliberación. Cambia T10 de forma explícita. Depende de F6.
10b. **Corrección 10b** (pendiente, sin fecha): R83-R85 de la F10 y la propuesta de R97 (la clave anterior solo abre ficheros de equipos conocidos). Hector decidió no meterla en la F11; conviene hacerla antes de publicar la 1.1.0.
11b. **Corrección 11b** (hecha y validada en el Honor el 4 nov 2026): vigilante por conexión, `caido` con motivo, último error y contadores abiertas · aceptadas en la fila, puerto real y arreglos de texto, con prueba de extremo a extremo en Rust y en el emulador (T95, T97). R105 cerrado.

## 8. Decisiones Transversales y Riesgos Abiertos

### Decisiones transversales

- **T1 — Sin custodia de claves privadas.** MAGI solo guarda referencias (huella del agente o ruta de fichero). Aplica a todas las fases, Android incluido. Precisión (F11): en Android, la clave del móvil se crea en el Keystore y no se puede exportar (T88).
- **T2 — Cliente SSH embebido (russh) en todas las fases.** Elegido en F1 frente al `ssh` del sistema; SFTP, túneles, sondeo y Android se construyen sobre él. `ControlMaster` solo se exporta a `magi_config`; dentro de MAGI el multiplexado es de canales (F3).
- **T3 — Inventario en SQLite como fuente de verdad; `~/.ssh/config` se importa, `magi_config` se exporta.** Nunca se edita el `config` del usuario salvo para insertar el `Include`, con copia de seguridad.
- **T4 — Huellas en `~/.ssh/known_hosts` del sistema.** Sin tabla propia; toda aceptación o sustitución queda registrada (log en F1, REGISTRO desde F2). Precisión (F11): en Android, un `known_hosts` privado de la app, con la misma política estricta.
- **T5 — Doble codificación glifo + color y modo ASCII degradado** en toda pantalla, también en Android (glifos y paleta).
- **T6 — Ninguna acción destructiva con una sola pulsación.** Confirmación siempre; las irreversibles en producción pasan por la deliberación (F6).
- **T7 — Tema de Omarchy con paleta fija de respaldo.** Compartido con mmmusic.
- **T8 — Sin `CHECK` sobre enumeraciones en SQLite; migraciones por `PRAGMA user_version`.** Convención 4d3 aplicada a Rust.
- **T9 — Un solo crate binario; Android decide en su fase (F11) si extrae núcleo.** **Resuelta en la F11:** se extrae `magi-nucleo` en un workspace (T85).
- **T10 — Sin telemetría.** Las únicas conexiones salientes son a los hosts del usuario y, en la sincronización cifrada (F10), a su destino de sincronización. Precisión (F9): MAGI tampoco comprueba si hay versiones nuevas; las actualizaciones llegan por el gestor de paquetes. Precisión (F11): la app Android no usa servicios de Google ni copia en la nube (T91).
- **T11 — (F1 implementación) Versiones fijadas del stack:** `russh 0.63` (claves, agente y known_hosts en `russh::keys`; el crate `russh-keys` separado no se usa), `ratatui 0.30` + `crossterm 0.29`, `tui-term 0.3` + `vt100 0.16`, `rusqlite 0.40`. Cualquier fase que suba una de ellas debe subir el conjunto y repetir las pruebas manuales de Sesión.
- **T12 — (F1 implementación) Toda escritura en `~/.ssh` y en los ficheros de MAGI pasa por `src/ficheros.rs`** (atómica, 600, copia con fecha). SFTP (F4) y sincronización (F5) reutilizan este módulo para lo local.
- **T13 — (F1 implementación) La UI recibe estados y avisos, nunca bytes:** el parser vt100 vive en la tarea de conexión y notifica `Pantalla` coalescido a ~30 fps; los diálogos que esperan al usuario (huella, frase) no tienen timeout. Las pestañas (F3) replican este patrón por sesión.
- **T14 — (F1 implementación) Documentación en `docs/faseN/`** (una carpeta por fase, `magi-maestro.md` en `docs/`), como en mmmusic.
- **T15 — (F2) Las operaciones automáticas nunca dialogan.** El sondeo (y en el futuro los túneles automáticos, los snippets masivos y la sincronización) reportan huellas desconocidas o frases pendientes como error con instrucción; solo las conexiones interactivas aceptan huellas o piden frases.
- **T16 — (F2) `registro::anotar` es la única escritura en REGISTRO.** Toda fase que añada eventos define un `tipo` nuevo y llama a esa función; las entradas no se editan, solo se purgan.
- **T17 — (F2) MAGI genera claves pero no las destruye.** Revocar es baja lógica de la referencia; borrar ficheros de `~/.ssh` o quitar claves del agente es siempre manual. Complementa T1.
- **T18 — (F2 implementación, reformulada en F3, F6 y F8) Un único escritor de SQLite por proceso.** En cada proceso, solo su bucle principal escribe (las tareas de red emiten eventos); entre procesos, `busy_timeout` 5 s y WAL. El servidor escribe únicamente REGISTRO (eventos que produce), `HOSTS.ultimo_estado`/`ultima_conexion_en`, desde F6 `DELIBERACIONES.ejecucion_resultado` y desde F8 `SINCRONIZACIONES_DIR.ultimo_resultado`, siempre por su hilo escritor.
- **T19 — (F2 anexo) Los secretos de autenticación que no son claves (contraseñas) viven solo en el llavero del sistema.** `magi.db` guarda referencias; ninguna fase puede guardar un secreto en la base de datos, tampoco cifrado (la F10 cifra lo que sale del equipo, no lo convierte en custodio). Precisión (F11): en Android, los secretos van cifrados con una clave del Keystore (`LlaveroMovil`) detrás de la misma interfaz `LlaveroSecreto`.
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
- **T32 — (F4 implementación) Toda espera de red en el servidor tiene plazo:** 10 s para DNS/TCP y saludos de subsistema (`request_subsystem` devuelve `Ok` aunque el host diga no), 60 s por bloque de transferencia; solo los diálogos con el usuario (5 min) escapan a esa regla. Precisión (F11): en el móvil también llevan plazo el saludo SSH, la apertura de canal, el PTY, `exec` y la lectura del script de instalación.
- **T33 — (F4 implementación) Un diálogo lleva fijados los datos sobre los que actúa** (rutas, ids) y no relee el estado al confirmar; y una respuesta a una petición que ya no está en vuelo se descarta.
- **T34 — (F4 implementación) Todo recurso del pool se libera por identidad y en todas las ramas de error**, incluidas las de apertura fallida; el servidor no se apaga mientras haya trabajo encolado (sesiones, transferencias y, en F5, túneles).
- **T35 — (F5) Todo lo que escucha en la máquina del usuario escucha en loopback por defecto**; exponerlo (`0.0.0.0`/`::`) exige aviso explícito. Aplica a túneles y a cualquier puerto futuro (modo remoto, sincronización). Precisión (F11): en el móvil, los túneles locales y dinámicos escuchan siempre en `127.0.0.1`. Precisión (F11, 11b): también los servidores de pruebas; el emulador llega al anfitrión por `10.0.2.2`, que es su loopback (R107).
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
- **T65 — (F9) Los binarios publicados salen solo de CI a partir de un tag**, con SHA-256 y atestaciones de procedencia; un tag publicado no se mueve ni se reutiliza. Precisión (implementación): `dist` no pasa `--locked`, así que el plan ejecuta `cargo metadata --locked` antes de compilar; cada workflow recibe solo su secreto y las herramientas se fijan por SHA-256.
- **T66 — (F9) `Hola` y `VersionIncompatible` son contrato estable:** nunca se quitan ni se renombran sus campos, solo se añaden opcionales, y el cliente lee la primera respuesta como JSON genérico. Así cualquier versión reconoce a otra, pasada o futura.
- **T67 — (F9) Un servidor de otra versión solo se para sin preguntar si no custodia nada** (ni sesiones, ni transferencias, ni túneles, ni ejecuciones, ni otras ventanas), y nunca desde un binario más antiguo que él. Un tipo de trabajo desconocido cuenta como trabajo. Precisa T24. Precisión (implementación): un campo desconocido en `trabajo` también cuenta como trabajo (`deny_unknown_fields`).
- **T68 — (F9) Versiones SemVer orientadas al usuario:** MAJOR si cambia algo que el usuario ve como incompatible (config, CLI, formatos exportados); MINOR para funcionalidades, migraciones y cambios de protocolo; PATCH para correcciones. No hay vuelta atrás: una `magi.db` más nueva que el binario no se abre.
- **T69 — (F9) Lo que se publica pasa antes una auditoría** (secretos en el historial, datos reales en documentos y tests). El agente prepara la publicación, pero hacer público el repositorio, crear secretos o cuentas y etiquetar versiones lo hace el desarrollador. Hecha en la F9: `gitleaks` sobre todo el historial sin hallazgos y datos de ejemplo saneados con un mapa único (`192.0.2.x`, `usuario`, `/home/usuario`).
- **T70 — (F10) Todo lo que viaja entre equipos lleva `uuid`, `version` y `vector`.** Solo los campos que viajan suben la versión, con el reloj híbrido. Una tabla o un campo nuevo que deba viajar se añade a `sincronizacion/elementos.rs` y a `docs/formato-sincronizacion.md`, y si cambia el significado sube `formato`. Precisión (implementación): los `SET NULL` de claves ajenas cuentan como edición de los hosts afectados.
- **T71 — (F10) Lo que sale del equipo va cifrado (`age`) y firmado (HMAC con clave derivada); nada se importa sin firma válida.** Claves privadas, contraseñas y la clave de sincronización nunca viajan ni salen del llavero. Precisa T1 y T19. Precisión (implementación): la clave de firma se deriva del texto canónico de la identidad en mayúsculas y la firma se comprueba antes de mirar `formato`; la clave anterior solo lee y todo cambio de clave obliga a recifrar el fichero propio.
- **T72 — (F10) Fusión determinista por vectores:** gana la versión dominante; entre concurrentes, la más reciente. La versión local que pierde nunca se pierde (CONFLICTOS) y los borrados son lápidas permanentes.
- **T73 — (F10) Una importación nunca cierra trabajo vivo ni sustituye una huella sin que el usuario lo acepte;** los borrados masivos se confirman y siempre hay copia previa de `magi.db`.
- **T74 — (F10) Un fichero o una base de datos de formato más nuevo que el binario se ignora o se rechaza con aviso, nunca se interpreta a medias** (amplía T68).
- **T75 — (F10) Cada equipo escribe solo su propio fichero;** el de otro solo se borra al quitarlo.
- **T76 — (F9 implementación) Una señal a otro proceso solo con un pid seguro:** el del kernel (`peer_cred`) o, si no lo hay, el del mensaje, validado (1 < pid ≤ i32::MAX y distinto del propio). Nunca se señala a un servidor más nuevo, ni desde la TUI ni desde `magi servidor parar` (ni con `--si`).
- **T77 — (F9 implementación) Orden de arranque del servidor:** manejador de `SIGTERM`, lock, abrir y validar `magi.db`, y socket. Ninguna señal lo mata sin ordenarlo y ningún fallo deja un socket huérfano.
- **T78 — (F9 implementación) Leer `magi.db` sin modificarla** (comprobar la versión, diagnosticar) nunca abre el original en lectura/escritura ni en WAL: se mira la cabecera o se lee una copia temporal con sus laterales.
- **T79 — (F9 implementación) Que un proceso ha terminado se decide por sus recursos** (lock libre, socket que no atiende), no por `kill(pid, 0)`, que da por vivo a un zombi.
- **T80 — (F9 implementación) Toda orden externa lleva plazo,** también en el diagnóstico (`ssh-add`, 2 s).
- **T81 — (F10 implementación) Una carpeta compartida es entrada no fiable:** no se siguen enlaces simbólicos (`O_NOFOLLOW`, temporales con `create_new` y nombre único), los nombres de fichero solo admiten el uuid canónico en minúsculas y hay un tamaño máximo. Aplica también a la app Android.
- **T82 — (F10 implementación) Las operaciones que cambian la sincronización (crear, unir, rotar, quitar, desactivar, reunir) toman el mismo cerrojo que la pasada.**
- **T83 — (F10 implementación) Un borrado aplazado no deja lápida local:** sus referencias se mantienen, el aplazamiento se propaga al padre si un hijo sigue vivo y un nombre que ocupa obliga a renombrar al nuevo.
- **T84 — (F10 implementación) Los secretos del llavero se inyectan** (`LlaveroSecreto`); las pruebas usan `LlaveroEnMemoria` y nunca el llavero real.
- **T85 — (F11) Núcleo compartido `magi-nucleo` en un workspace.** Lo que deben hacer igual el escritorio y el móvil (almacén, sincronización, conexión, túneles, parser y codificación de teclas, registro) vive en el núcleo y se mueve, no se duplica: Kotlin nunca lo reimplementa. `magi-ssh` sigue en la raíz y `magi-movil` es la única puerta UniFFI. Sustituye a T9. Precisión (implementación): el núcleo incluye también `configuracion`, `snippets` y `deliberacion`, porque los necesita el almacén; el escritorio conserva sus rutas de importación con reexportaciones.
- **T86 — (F11) El núcleo no conoce la plataforma:** recibe por inyección las rutas, el nombre y la plataforma del equipo y los servicios (carpeta, llavero, firmante, trabajo vivo); nunca lee el entorno ni calcula rutas, y no depende de la interfaz ni de Android.
- **T87 — (F11) El móvil es lector de la sincronización** mientras no edite el inventario: nunca escribe en la carpeta ni exporta; solo escribe campos propios, huellas, REGISTRO y conflictos de huella y de borrado. No figura como equipo en el escritorio: se le corta el acceso cambiando la clave.
- **T88 — (F11) Claves del móvil en el hardware:** la clave SSH del móvil se genera en el Android Keystore, no se puede exportar y solo firma con el teléfono desbloqueado; la app exige bloqueo de pantalla y se protege con huella o PIN. Extiende T1 y T19. Precisión (1 nov 2026): el Keystore ya no exige el teléfono desbloqueado (T96).
- **T89 — (F11) Entre el núcleo y la app Android rige lo mismo que entre servidor y cliente:** la app recibe pantallas ya interpretadas (tramos, 30 fps como máximo) y listas completas de sesiones y túneles, nunca bytes del remoto ni mensajes finos. Extiende T13 y T27.
- **T90 — (F11) Publicaciones separadas:** el escritorio con `vX.Y.Z` (dist) y la app con `android-vX.Y.Z` (`android-release.yml`). Una release de Android nunca es «Latest» y `release.yml` no se dispara con ella; cada workflow recibe solo sus secretos y el keystore lo crea el desarrollador. Precisa T65.
- **T91 — (F11) Sin servicios de Google en la app:** ni Play Services, ni Firebase, ni ML Kit, ni copia en la nube; dependencias de Gradle verificadas por SHA-256. Extiende T10 y deja abierta la puerta a F-Droid.
- **T92 — (F11 implementación) Una sola tabla de teclas:** el escritorio y el móvil codifican las teclas con la tabla del núcleo (Ctrl, Alt y Mayús + flecha con `CSI 1;m`); `src/teclas.rs` solo traduce los eventos de crossterm.
- **T93 — (F11 implementación) La versión de la app vive en `android/version.properties` y se comprueba dos veces:** Gradle la usa y el workflow la compara con la etiqueta y, con `aapt2`, con el APK. La firma de release solo existe con las variables `MAGI_KEYSTORE_*` del workflow (esquemas v2 y v3), y las herramientas que CI descarga van fijadas por SHA-256 (cargo-ndk). Precisa T90.
- **T94 — (F11 implementación, método) La integración núcleo-app se prueba con una fixture `.age` real** generada por el núcleo e importada en el emulador, y el Keystore con una prueba instrumentada real; el resto de la plataforma, con dobles en la JVM.
- **T95 — (F11 validación, método) Lo que el móvil hace en red se prueba de extremo a extremo antes de darlo por hecho:** túneles y conexiones (y lo que llegue en la F14) se prueban en Rust (`magi-movil` con un servidor SSH de prueba y un servicio de destino) y en el emulador, sin red externa; probar la escucha o el estado por separado no basta. La F11 marcó 110/110 con 126 pruebas JVM en verde y el primer túnel en el plegable no reenvió (D204, R105). Complementa T31 y T94.
- **T96 — (F11 implementación) Claves del Keystore sin `setUnlockedDeviceRequired`:** con desbloqueo facial (biometría no fuerte) Android las rechaza aunque el teléfono esté desbloqueado (comprobado en MagicOS). Siguen en hardware y no se pueden exportar; lo que impide usarlas sin permiso es el bloqueo de MAGI. Cambiar cómo se crea una clave del Keystore obliga a migrar las instalaciones: los secretos se vuelven a cifrar y, si cambia la clave del móvil, hay que reinstalarla en los hosts (la app lo avisa). Matiza T88.
- **T97 — (F11 corrección 11b) Toda conexión viva del móvil tiene su vigilante,** no el de una sesión: si el host la cierra, todo lo que la usa (sesiones, túneles y lo que llegue en la F14, como SFTP o ejecuciones) pasa a caído con el motivo. Ningún estado `activo` sobrevive a su conexión. Nace del túnel que seguía `activo` sin conexión (R105).
- **T98 — (F12) Lo que MAGI enseña en el navegador lo sirve solo en `127.0.0.1`,** con enlaces de un solo uso, una clave por sesión, `Host` comprobado siempre, `Origin` obligatorio en la API, CSP estricta y ningún recurso externo. Nunca en `0.0.0.0`. Extiende T35 y T10.
- **T99 — (F12) MAGI no lleva dependencias GPL:** el proyecto es MIT OR Apache-2.0 y lo comprueba `cargo deny`. Por eso no recomprime vídeo con x264 (D211 de la F12); si algún día hace falta un códec, se elige con su licencia y sus patentes a la vista.
- **T100 — (F12) Un host tiene tipo** (`ssh`, `rdp`, `vnc` o `voxl`), fijo al crearlo. Solo los `ssh` se exportan a `magi_config` y pueden ser destino de terminal, SFTP, túneles, snippets o salto; los demás llegan «a través de» un host SSH o directos.
- **T101 — (F12) Toda huella de servidor sigue la misma política,** sea la clave SSH o el certificado RDP: se acepta la primera vez con diálogo, un cambio bloquea en rojo y sustituirla exige escribir el nombre del host; se escribe como `SHA256:` y base64 y nunca se acepta en una operación automática. Extiende T4 y T15.
- **T102 — (F12) El código de terceros que se copia al repositorio conserva su licencia y su origen** (el visor de voxl, MIT: `visor/LICENSE-voxl` y el commit de origen en `visor/README.md`) y se mantiene como código propio: no se actualiza solo.
- **T103 — (F12 implementación) Un solo árbol criptográfico.** Si dos dependencias fijadas exigen generaciones incompatibles de RustCrypto (russh 0.63 e IronRDP 0.17 con `ecdsa`), se vendoriza lo mínimo con los cambios anotados, antes que duplicar versiones o subir todo el conjunto. Lo vendorizado se revisa en cada subida de russh o IronRDP y se quita en cuanto las versiones coinciden (R118).
- **T104 — (F12 implementación) Una biblioteca que no es `Send` corre en un hilo con runtime propio por sesión** (el `ActiveStage` de IronRDP); el resto del proceso solo ve canales. Vale también para el móvil (F14).
- **T105 — (F13) Lo que pide permisos en un host va con sudo orden a orden, nunca `sudo sh`:** `sudo -n` si vale y, si no, una contraseña que pide el cliente, comprueba antes de actuar, guarda solo en memoria con caducidad corta y manda al host por la entrada estándar. Nunca va a disco, al llavero, al log ni a REGISTRO. Vale para la F16 (Despliegues) y la F17 (Accesos).
- **T106 — (F13) Lo que se comprueba antes de actuar se comprueba dentro de la propia acción,** en el mismo canal, con los mismos privilegios y justo antes, sin hueco entre comprobar y ejecutar (validadores de configuración; la F16 los reutiliza).
- **T107 — (F13) En las listas, las teclas que cambian algo en un host remoto van en mayúsculas** y nunca actúan sin confirmación o deliberación (precisa T6).
- **T108 — (F13) Una pestaña que ejecuta una orden la arma el servidor desde un tipo cerrado** (`OrdenPestana`): el cliente nunca manda texto para ejecutar, salvo los snippets y las comprobaciones propias, que escribe el usuario.
- **T109 — (F13) Lo que se lee de un host para enseñarlo (listados, unidades, logs) es entrada no fiable con límites y no se guarda:** los logs viven solo en un anillo en memoria (precisa T38 y T44).
- **T110 — (F13 implementación) Lo que el servidor guarda por petición se indexa por `(cliente_id, peticion_id)`** (acciones, suscripciones de log y escritorios): dos ventanas pueden repetir ids. Los mensajes de cada ventana se atienden en orden y, antes de abrir algo para un cliente, se comprueba que sigue conectado.
- **T111 — (F13 implementación) Lo que mantiene un canal abierto conserva su `ConexionTomada` mientras dura** (túneles, escritorios, seguimientos de logs) y la suelta al terminar, en todas las ramas; si la suelta antes, el pool recicla la conexión al acabar su gracia. Precisa T34.
- **T112 — (F13 implementación) Ningún mensaje del protocolo pasa del tamaño máximo del códec:** los lotes se cortan por tamaño codificado, no solo por número, y lo que se acumula en memoria (anillos, colas) tiene tope de bytes además de tope de elementos.
- **T113 — (F13 implementación) El plazo de una operación cubre la red y el canal, nunca el tiempo de sus diálogos** (huella, frase, contraseña), que tienen su propia espera (precisa T32).
- **T114 — (F14) En el móvil, `magi-movil` hace el papel del servidor de sesiones con la lógica del núcleo:** ejecuta acciones, snippets, logs y deliberaciones, y anota por su hilo escritor. Kotlin pinta y pregunta: qué se permite, qué delibera y cómo se lee lo que llega de un host lo decide el núcleo (amplía T85 y T89).
- **T115 — (F14) En el móvil, lo deliberado se ejecuta manteniendo pulsado 1 s, el `Ctrl+K` del móvil:** un toque nunca ejecuta desde el diálogo MAGI, y lo demás pide una confirmación que nombra la acción, el objetivo y el host (amplía T6 y T41).
- **T116 — (F14) Una comprobación activa que no puede hacerse en una plataforma rechaza («solo en el escritorio») y obliga a forzar con motivo;** nunca se omite en silencio. Así lo que bloquea en un equipo también bloquea en otro (amplía T43).
- **T117 — (F14) Lo que se acumula para una vista vive en Rust y la app pide ventanas:** el anillo de logs nunca cruza entero la FFI y sus topes se aplican en el núcleo, como las pantallas en tramos (T13, T89).
- **T118 — (F14) El móvil solo trabaja en segundo plano en lo que lanzó el usuario:** sesiones, túneles, acciones y ejecuciones, que mantienen el servicio en primer plano. El sondeo, los refrescos y los logs solo corren con su pantalla a la vista.

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
| R15 | Las Fases 2-6, 8, 10, 11, 12 y 13 tienen implementación reportada sin validación manual completa (override de Hector); la 3b cambió el pool que usan todas | Medio-alto | Validar F2-F6 en Omarchy con hosts reales, repitiendo Sesión, Archivos y Túneles (pestañas con y sin `multiplexar`, túnel activo al abrir pestaña, SFTP y túnel a la vez), la F8 con su guion de 12 pasos, la F10 con llavero real, Syncthing entre dos equipos, QR y `age` real, la F11 en el plegable con el guion de su informe (§11), la F12 con Windows, VM de QEMU, xrdp, TigerVNC y voxl (guion de su informe, §11) y la F13 con Nginx, sudo, Docker Compose y Podman (guion de su informe, §11); guiones al final de cada informe de implementación |
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
| R42 | ~~`tests/pool.rs::sigterm_para_el_servidor_limpio` intermitente con la máquina cargada~~ | Cerrado 31 oct 2026 (F9) | Causa encontrada: el manejador de `SIGTERM` se instalaba después del socket; ahora va antes del lock y el test pasó cinco veces seguidas (T77) |
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
| R60 | Nombres en AUR: el paquete AUR `magi` (una TUI de Git) instala también `/usr/bin/magi` (confirmado en la F9); `magi-ssh-bin` y `magi-ssh` estaban libres pero pueden dejar de estarlo antes de publicar | Bajo | `conflicts=('magi-ssh' 'magi')`; el desarrollador vuelve a comprobar los nombres justo antes de la primera publicación |
| R61 | macOS sin notarizar: un tarball bajado con el navegador queda en cuarentena | Bajo | Homebrew no la pone; el README explica `xattr` |
| R62 | macOS sigue sin probarse en un equipo real: CI solo compila y pasa los tests unitarios (continúa R9 y R17) | Medio (macOS) | Instalar con Homebrew en el mac mini y seguir el guion de validación |
| R63 | Robo de los secretos de publicación (`HOMEBREW_TAP_TOKEN`, `AUR_SSH_PRIVATE_KEY`) permitiría publicar binarios maliciosos en los canales | Medio | Token de grano fino solo al tap, clave SSH dedicada a AUR, cada workflow con solo su secreto (sin `secrets: inherit`), la clave de AUR fuera del contenedor de Arch, huellas de AUR comprobadas antes de empujar, permisos mínimos y rotación documentada; las atestaciones permiten comprobar que un binario salió de CI |
| R64 | `cargo deny` puede bloquear la CI por un aviso nuevo de RustSec en una dependencia fijada (russh-sftp `=3.0.0` atado a russh 0.63). Hoy hay uno aceptado: RUSTSEC-2023-0071 (`rsa`, sin parche, transitiva de russh y ssh-key) | Bajo | Excepción con motivo y fecha límite (2027-10-30) que el job `deny` comprueba; MAGI solo firma con RSA como cliente; revisar al subir el conjunto russh |
| R65 | La entrada de escritorio con `Terminal=true` depende de cómo resuelva la terminal el lanzador de Omarchy | Bajo | Validación manual; alternativa `Exec=xdg-terminal-exec magi` como desviación |
| R66 | Tests intermitentes que dependen de tiempos en runners compartidos de CI (la causa de R42 ya está corregida) | Medio | Reintento manual; anotar y arreglar cada test intermitente en vez de desactivarlo |
| R67 | Los binarios de Linux exigen glibc ≥ 2.35: distros anteriores a 2022 no los ejecutan | Bajo (el objetivo es Omarchy) | Documentado en el README; compilar desde fuente sigue siendo posible |
| R68 | ~~La F10 se especificó sobre la F9 sin implementar~~ | Cerrado 31 oct 2026 | Informe de la F9 procesado: la F10 no necesita cambios de alcance (informe v1.1) |
| R69 | Relojes muy desfasados entre equipos: el reloj híbrido mantiene el orden de lo que deriva de otra cosa, pero entre dos ediciones concurrentes puede ganar la que en realidad fue anterior | Bajo | NTP en los equipos; la versión perdedora queda en CONFLICTOS |
| R70 | Perder un equipo: hasta quitarlo y cambiar la clave, quien lo tenga lee el inventario y puede escribir ficheros válidos en la carpeta | Alto si ocurre | Quitar y cambiar la clave en cuanto se sepa; los demás equipos se detienen al no abrir el fichero nuevo; las huellas nunca se sustituyen solas. En el móvil (F11), que no figura como equipo, se corta con «cambiar la clave» y quitándolo de Syncthing (R96) |
| R71 | Perder la clave en todos los equipos impide añadir equipos nuevos | Bajo (los datos siguen en cada `magi.db`) | Confirmar «la he guardado»; `k` la muestra de nuevo; se puede crear otra sincronización |
| R72 | La carpeta compartida enseña metadatos al servicio que la replica (número de equipos, tamaño, frecuencia de cambios) | Bajo | Aceptado; el contenido va cifrado y firmado |
| R73 | Las herramientas de sincronización dejan ficheros a medias, copias en conflicto o retrasos | Bajo | Un escritor por fichero, firma y reintento, copias en conflicto ignoradas, aviso en `magi diagnostico` |
| R74 | Dos ventanas del mismo equipo no ven los cambios locales de la otra hasta recargar (desde la F3); con la sincronización se nota más | Bajo | Candidato a pulido: difusión «inventario cambiado» por el servidor de sesiones |
| R75 | Nombres parecidos: `magi sincronizar` (inventario) frente a `magi sincronizaciones` (directorio, F8), y REGISTRO `sincronizacion` (F8) frente a `inventario_*` (F10) | Bajo | Ayuda de la CLI explícita y glosario |
| R76 | Quien tiene la clave tiene confianza total: puede cambiar direcciones y añadir huellas para hosts sin huella, y así redirigir conexiones sin aviso de huella | Medio | Clave solo en el llavero; quitar y cambiar clave; huellas añadidas en REGISTRO y nunca sustituidas |
| R77 | Dependencias de la F10 con avisos de RustSec: RUSTSEC-2026-0173 (`proc-macro-error2`) y RUSTSEC-2026-0374 (`basic-toml`), transitivos de `age` por su localización, sin parche ni versión de `age` sin ellos | Bajo (compilación o ruta que MAGI no usa) | Excepciones con motivo y fecha en `deny.toml`; revisar al actualizar `age` |
| R78 | Sustitución silenciosa del servidor anterior: entre la foto de trabajo del rechazo y el `SIGTERM` podría colarse una conexión en vuelo | Bajo (ventana mínima) | Solución futura: un `PararSiVacio` atómico en el protocolo |
| R79 | `Evento::Sustitucion(Ok)` podría pisar un `ServidorCaido` si el servidor nuevo muere justo después de la Bienvenida (no reproducido) | Bajo | Candidato a pulido |
| R80 | Riesgos aceptados de cadena de suministro: actions fijadas por etiqueta y no por SHA, toolchain por la rama stable, imagen rolling de Arch (sin secretos), MPL-2.0 en las licencias permitidas, sin `timeout-minutes` ni `concurrency`, `namcap` sobre el PKGBUILD y no sobre el paquete construido | Bajo-medio | Fijar actions por SHA y añadir límites de tiempo en una fase de pulido |
| R81 | Los workflows nunca se han ejecutado en GitHub (validados con `actionlint`, `dist plan` y `makepkg` en local): la primera release real puede fallar y las AC de release, `yay` y `brew` están sin comprobar. Desde la F11, lo mismo con `android.yml` y `android-release.yml`: KVM del runner, plataforma 37.0 y build-tools que descarga AGP, wrapper y cargo-ndk sin comprobar | Medio | Ensayo con `v1.0.0-rc.1` antes de `v1.0.0` (`docs/publicar.md`); un canal que falla tras la release se relanza. En Android, ensayo con `android-v0.1.0-rc.1` (`docs/publicar-android.md`) |
| R82 | `release.yml` se mantiene a mano (`allow-dirty = ["ci"]`): actualizar `dist` exige regenerarlo y reaplicar los ajustes | Bajo | Procedimiento en `docs/publicar.md`; no quitar `allow-dirty` |
| R83 | **`magi sincronizar` (CLI) no consulta el trabajo vivo del servidor:** un borrado por debajo del umbral se aplica aunque el host tenga sesiones o túneles abiertos, justo el caso de un temporizador con la TUI abierta. Rompe T73 fuera de la TUI | Medio | Consultar al servidor por el socket como `magi servidor estado` (sin servidor no hay trabajo vivo; con otra versión o sin respuesta, aplazar todos los borrados de hosts y túneles). Corregir antes de publicar la 1.1.0 |
| R84 | El almacén hace el `UPDATE` y la subida de versión en dos pasos: una caída entre ambos deja un cambio sin versión nueva, que no viaja hasta la siguiente edición | Bajo-medio | Misma transacción |
| R85 | Un fichero truncado y una clave distinta no se distinguen al descifrar: un fichero a medio copiar puede detener la sincronización pidiendo una clave nueva | Bajo-medio | Separar el error «ninguna identidad coincide» de `age` del resto; solo ese pide clave |
| R86 | Un elemento con firma válida pero datos no válidos hace fallar la pasada entera (nada a medias, pero la sincronización se atasca) | Bajo | Aceptado; en el futuro, descartar solo ese elemento como conflicto |
| R87 | Aceptar la huella de otro equipo sustituye todas las claves del host, no solo la del tipo en conflicto | Bajo | Sustituir solo la del tipo en conflicto |
| R88 | Con varios equipos, el autor de un conflicto de huella se muestra como «otro equipo» | Bajo (solo visual) | Pasar los documentos al importador de huellas |
| R89 | `age` fijado en 0.11.5: la 0.12 no compila con el `ml-kem` que arrastra russh 0.63 | Bajo | Mismo formato age v1; revisar al subir el conjunto russh |
| R90 | **La reorganización en workspace toca casi todo el código de las F1-F10**, con la F9 sin publicar y varias fases sin validar: puede meter regresiones en el escritorio | Medio-alto | Mover sin reescribir; todas las pruebas sin cambiar aserciones e instantáneas idénticas; sprint propio (S1); revisión adversarial (T31) |
| R91 | ~~Compilación cruzada para Android de `aws-lc-rs`, la cripto por defecto de russh 0.63~~ | Cerrado 1 nov 2026 | russh 0.63.3 con `aws-lc-rs` compila y enlaza para `arm64-v8a` y `x86_64` con el NDK r30; no hace falta `ring` |
| R92 | Entrada del teclado del sistema en una vista de terminal en Compose (composición, sugerencias, escritura deslizando) | Medio | Vista de entrada propia en bruto (`TYPE_NULL`, como Termux); probar con Gboard y con el teclado de Honor |
| R93 | MagicOS cierra apps en segundo plano aunque tengan servicio en primer plano, y con la pantalla apagada (Doze) la red puede cortarse | Medio | Guía de batería, aviso al volver si Android cerró MAGI y keepalive de 30 s; validar en el plegable |
| R94 | Verificación de desarrolladores de Google: desde el 30 sep 2026 en Brasil, Indonesia, Singapur y Tailandia, y en todo el mundo en 2027. Instalar un APK de un desarrollador no verificado exigirá el proceso avanzado (espera de 24 h) o ADB | Medio | Nombre de paquete y clave de firma estables desde la 0.1.0; decidir el registro (cuenta de 4d3 o de aficionado, hasta 20 dispositivos) antes de 2027 |
| R95 | Perder la clave de firma del APK impide publicar actualizaciones: Android no instala encima una app firmada con otra clave | Alto si ocurre | Copia cifrada fuera de GitHub; documentado en `docs/publicar-android.md` |
| R96 | Un móvil perdido lee el inventario hasta que se cambia la clave, y no aparece en la lista de equipos del escritorio porque no escribe fichero | Alto si ocurre | Bloqueo de la app y secretos en el Keystore; «cambiar la clave» (`r`) en el escritorio y quitarlo de Syncthing; guía en el README |
| R97 | Con la clave anterior (30 días tras una rotación), un escritorio acepta ficheros de equipos que no conocía: quien tenga un equipo perdido y acceso a la carpeta podría colar un fichero válido | Medio | Syncthing del móvil en «solo recibir» y quitar el dispositivo; propuesta para la 10b: la clave anterior solo abre ficheros de equipos ya conocidos |
| R98 | R85 sigue abierto (sin 10b) y llega al móvil: un fichero a medio copiar puede pasarlo a `esperando_clave` | Bajo | En el móvil es un aviso que vuelve solo; la corrección en el núcleo con la 10b sirve a los dos |
| R99 | La app usa JNA (UniFFI) y va sin minificar en la 0.1.0: APK más pesado (objetivo < 40 MB; el de depuración, con dos ABI, pesa ≈ 50 MB y la release está por medir) y, al activar R8, reglas de conservación que mantener | Bajo | Minificación desactivada en la 0.1.0; activarla con prueba de humo de la release |
| R100 | El agente prueba en el emulador y en el anfitrión: plegado, MagicOS, StrongBox, Syncthing-Fork, QR y teclado físico solo los comprueba Hector en el móvil | Medio | Guion de validación manual del informe de la F11 (§11); amplía R15. El 4 nov 2026 el plegable encontró un túnel que no reenviaba y que las pruebas no cazaban (R105, T95) |
| R101 | Túneles del inventario pensados para el escritorio pueden no servir en el móvil (puertos < 1024, `0.0.0.0`, destinos de la red de casa) | Bajo | Loopback forzado, puerto libre y error claro |
| R102 | Las pantallas del terminal cruzan la frontera FFI muchas veces por segundo y copiar los tramos tiene un coste | Bajo-medio | Solo filas cambiadas y 30 fps como máximo; medir con `yes` en el plegable |
| R103 | `docs/formato-sincronizacion.md` §11.9 pide una «red de seguridad» para los borrados masivos en Android y contradice el informe de la F11 (§7.3), que los aplica sin copia ni confirmación | Bajo | Corregir el documento para que diga lo que hace el móvil lector; lo implementado sigue al informe de la F11 |
| R104 | El informe de implementación de la F11 y su script de entorno usan la ruta real del SDK con el usuario del desarrollador: datos reales en un repositorio público (T69). Además, el script vive en `/tmp`, que se vacía al reiniciar | Bajo | Sustituir la ruta por `$HOME/Android/Sdk` antes de subir el informe y guardar el script fuera de `/tmp`. Sigue abierto el 4 nov 2026: el informe actualizado mantiene la ruta y el script; va en el prompt de reanudación |
| R105 | ~~Un túnel del móvil se ve `activo` sin reenviar: en el plegable, `ERR_SOCKET_NOT_CONNECTED` desde el navegador, ninguna conexión contada y ningún error en la fila~~ | Cerrado 4 nov 2026 | Corrección 11b: vigilante por conexión (T97), último error y contadores en la fila y prueba de extremo a extremo en Rust y en el emulador (T95); validada en el Honor |
| R106 | ~~La prueba de extremo a extremo de los túneles en el emulador no corre en CI: necesita el servidor de pruebas del anfitrión y se omite sola sin él. Una regresión en la parte Android de los túneles solo la caza quien la ejecute en local~~ | Cerrado 9 nov 2026 | El job `humo` de `android.yml` compila y arranca `servidor_pruebas` antes del emulador y espera su línea «listo:»; las pruebas instrumentadas dejan de omitirse (D273) |
| R107 | ~~El servidor de pruebas del túnel (`servidor_pruebas_tunel`) escucha su SSH en `0.0.0.0:12222`: mientras está arrancado, se puede alcanzar desde la red local del desarrollador (T35)~~ | Cerrado 9 nov 2026 | `servidor_pruebas` (antes `servidor_pruebas_tunel`) escucha SSH, eco y control solo en `127.0.0.1` (el emulador llega al anfitrión por `10.0.2.2`, que es su loopback) y su SSH solo reenvía `direct-tcpip` al eco de la prueba |
| R108 | IronRDP es 0.x y cambia rápido: su API puede no dejar sacar el AVC420 de EGFX sin decodificarlo, o tener lagunas en el portapapeles o en Display Control | Medio | Versión fijada con `=`, adaptador propio en `escritorio/rdp/`, pruebas contra `ironrdp-server`; si no se puede pasar el H.264, rectángulos y desviación anotada. Materializado en parte (5 nov 2026): EGFX no está en el meta-crate 0.17 y se usa `ironrdp-egfx` 0.3 con un `DvcProcessor` propio; el AVC420 solo está probado a nivel unitario y falta verlo con Windows 11 real |
| R109 | Teclado entre el navegador, Hyprland y Windows en español: AltGr, teclas muertas, Ctrl+Alt+Supr y atajos que se queda el compositor | Medio | Código físico y scancodes con la distribución de Windows (D222), botones de Ctrl+Alt+Supr y Win, bloqueo de teclado en pantalla completa y validación manual (paso 1 del guion) |
| R110 | Rectángulos en bruto con vídeo a pantalla completa (VNC, o RDP sin H.264): menos fps y más CPU en el navegador | Bajo-medio | Agrupar a 60 fps con contrapresión, pausar con la pestaña oculta y medir; un códec queda fuera mientras no haya uno sin GPL ni patentes (T99) |
| R111 | El enlace de voxl lleva la contraseña de esa sesión y queda en el historial del navegador; en un equipo compartido, otros usuarios pueden llegar a `127.0.0.1` | Bajo | Contraseña aleatoria por sesión, voxl parado al cerrar, lanzadera con enlace de un solo uso; MAGI está pensado para equipos de un solo usuario |
| R112 | Con escritorios, el fichero de sincronización pasa a `formato` 2: los equipos con MAGI 1.1 y la app 0.1 dejan de importarlo hasta actualizarse | Bajo (aún no hay versiones publicadas) | Formato 2 solo si hay escritorios, aviso `formato_nuevo` y nota en el CHANGELOG |
| R113 | El visor necesita npm (esbuild y TypeScript) para generarse: cadena de suministro | Bajo | Solo dependencias de desarrollo, fijadas con integridad en `package-lock.json`; `visor/dist` versionado y comprobado en CI; nada de npm en tiempo de ejecución |
| R114 | voxl es joven (0.3.x): su página, sus opciones o su protocolo pueden cambiar | Bajo | MAGI copia el visor (no depende de voxl para RDP y VNC); para voxl exige la 0.3 o posterior y usa solo `--addr`, `--port`, `VOXL_PASSWORD` y `/healthz` |
| R115 | VNC directo viaja sin cifrar y su autenticación es débil (DES, 8 caracteres) | Medio | Por SSH por defecto, confirmación al guardar o abrir un VNC directo, `⚠` en Hosts y en la ficha |
| R116 | voxl solo captura X11: los escritorios Wayland no se ven con voxl | Bajo | GNOME y KDE en Wayland se ven por su servidor RDP; voxl para máquinas virtuales con Xvfb |
| R117 | Con H.264 (RDP con AVC420), al volver de la pestaña oculta el cliente RDP no pide a Windows la imagen entera: puede verse la anterior hasta el siguiente cambio | Bajo | Pedir la imagen entera al volver (Refresh Rect o Suppress Output de RDP) con su prueba; va en el prompt de reanudación de la F12 |
| R118 | `ironrdp-connector` e `ironrdp-acceptor` vendorizados (con `sspi` 0.23.1 y `picky` 7.0.0-rc.27, una versión candidata): los avisos de seguridad de RustSec no cubren una copia local y las correcciones de IronRDP no llegan solas | Medio | Revisar las publicaciones de IronRDP, `sspi` y `picky` en cada subida y quitar el vendor cuando IronRDP use versiones compatibles con russh (T103); la regla está en `CLAUDE.md`. Desde la F13, `deny.toml` lleva además la excepción RUSTSEC-2026-0441 (`pin-utils` sin mantener, transitiva de `sspi`), que se revisa igual |
| R119 | Los hosts son muy distintos (versiones de systemd, Docker, Podman y Compose; distribuciones) y la lectura de sus salidas puede fallar en alguno | Medio | `systemctl show` clave=valor y JSON, salidas grabadas de varias distribuciones en las pruebas, «sin systemd» y «systemd demasiado antiguo»; el guion de la F13 prueba Debian o Ubuntu y Podman |
| R120 | La contraseña de sudo pasa unos minutos en la memoria del cliente | Bajo | `Zeroizing`, 5 min sin uso como mucho (`sudo_memoria_min`, 0 para no recordarla), se olvida al salir de la vista; nunca se guarda (T105) |
| R121 | Un servicio muy hablador satura el socket o la interfaz con sus logs | Bajo | Lotes cada 100 ms, cola de 4 MiB con descarte y recuento de omitidas, anillo acotado |
| R122 | Una acción sobre una unidad crítica corta la conexión con el host o con los hosts que van a través de él | Medio | Aviso en rojo con los hosts afectados y deliberación para parar, reiniciar y desactivar; el guion de la F13 lo prueba con `ssh.service` |
| R123 | El `formato` 3 deja fuera a los equipos con MAGI 1.2 en cuanto hay una comprobación propia | Bajo | Solo con comprobaciones propias, aviso al crear la primera y actualizar todos los equipos (también el móvil) |
| R124 | Una comprobación propia mal escrita bloquea todas las recargas y reinicios de ese servicio | Bajo | El resultado enseña su salida y `c` la edita al momento |
| R125 | El sondeo no recibe `[servicios] contenedores`: con `contenedores = []` sigue mirando los contenedores y marcando sus fallos (B4 de la revisión adversarial de la F13) | Bajo | Pasar la preferencia en `PeticionSondeo` y como argumento del script; va en el prompt de reanudación de la F13 |
| R126 | Las comprobaciones propias que necesitan root van con `sudo -- sh -c`: un sudoers limitado (solo `systemctl`, `journalctl` y `docker`) no las deja pasar | Bajo | Escribir la comprobación para que no necesite root o permitir `sh` en sudoers; el resultado enseña por qué falló |
| R127 | Sondear cada 30 s con Flota a la vista mantiene una conexión por host y gasta batería y datos | Bajo-medio | Solo a la vista y de los hosts que se ven, gracia de 60 s y se puede apagar (`flota_refresco_seg = 0`); medirlo en el guion de la F14 |
| R128 | El gesto de mantener pulsado puede ser incómodo o no accesible con TalkBack | Bajo | Acción de pulsación larga con etiqueta (tocar dos veces y mantener) y prueba con TalkBack en el guion de la F14 |
| R129 | Con mala red, una acción desde el móvil puede acabar en `sin_confirmar` o `error` con el trabajo hecho en el host | Medio | El resultado lo dice y pide refrescar; la acción sigue con el servicio en primer plano y REGISTRO anota lo que se supo |
| R130 | REGISTRO y DELIBERACIONES del móvil no viajan: el escritorio no ve lo que se hizo desde el teléfono, ni al revés | Medio | Registro en el móvil (F14); la auditoría conjunta irá con la Caja negra (F19) |
| R131 | Los ajustes del móvil (umbrales, horas del backup) pueden no coincidir con el `config.toml` del escritorio, y la misma deliberación salir distinta | Bajo | Mismos valores por defecto, cada celda enseña su dato y Ajustes deja cambiarlos (D280) |
| R132 | Mover el sondeo y las comprobaciones de la deliberación al núcleo puede cambiar el escritorio sin querer | Medio | Mover sin reescribir, con las pruebas y las instantáneas del escritorio idénticas (como en R90), y revisión adversarial (T31) |
| R133 | Si Android o MagicOS cierran MAGI durante una acción o una ejecución, la orden puede seguir en el host sin quedar en REGISTRO, y la deliberación se queda sin `ejecucion_resultado` | Medio | Servicio en primer plano mientras dure, guía de batería, aviso al volver y Registro que lo dice («sin resultado») |
| R22 | El servidor escribe bajadas y temporales en el disco local: un cliente malicioso del mismo usuario podría dirigirlas a cualquier ruta | Bajo (mismo usuario, socket 700) | Modelo de amenaza igual al de `ssh-agent`; temporales en directorio propio 700. Desde la F8, `BorrarTemporal` ya no acepta `..` y las rutas de transferencia se validan al encolar (T59, T60) |
| R17 | `setsid`/autolanzado y el directorio de socket en macOS sin validar (sin equipo) | Medio (macOS) | Fallbacks de ruta especificados; validar en el mac mini cuando se pueda |
| R18 | Dos ventanas escribiendo en la misma pestaña sin arbitraje pueden mezclar entradas | Bajo | Comportamiento de tmux, conocido; la barra muestra cuántas ventanas hay adjuntas |
| R6 | La deliberación MAGI (F4) se vuelve decorativa si el forzado es gratis | Alto en F4 | Motivo obligatorio y registro con usuario, hora y acción |
| R7 | Las directivas globales de `~/.ssh/config` (antes del primer `Host`) se omiten al importar; un usuario con `ForwardAgent yes` global pierde ese comportamiento dentro de MAGI | Bajo | Aviso en la importación (F1). Modelar «opciones globales» en `config.toml` o en una fase posterior si aparece la necesidad |
| R8 | Sin CI: `cargo test`, `clippy` y `fmt` solo se ejecutan en local | Bajo | Crear el flujo de GitHub Actions cuando exista el remoto (queda en el checklist de F1) |
| R9 | macOS no verificado: solo hay garantía de que no existe código específico de plataforma | Bajo | Probar en el mac mini antes de la F2 o aceptar macOS como «mejor esfuerzo» |
