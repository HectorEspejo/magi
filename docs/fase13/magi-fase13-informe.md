# MAGI - Fase 13: Servicios, Contenedores y Logs

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 8 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-12 con informe procesado. A la F12 (128/128) le quedan el guion de validación manual, el AVC420 con Windows real, la prueba instrumentada de Android, la limitación R117 y subir la rama, todo en su prompt de reanudación. La F11 sigue con el guion del plegable, la CI, la primera release y sus trámites de cierre; la F10, sin validación manual ni corrección 10b; la F9, pendiente de su primera publicación. Nada de eso bloquea esta fase. El 8 nov 2026 Hector aceptó el orden de las fases 13 a 21 (maestro, §7): esta fase ocupa el puesto de «Android, segunda vuelta», que pasa a ser la F14.

---

## 1. Visión General

La Fase 13 añade una vista para **manejar lo que corre en cada host**: los servicios y timers de systemd, sus logs y los contenedores de Docker o Podman con sus proyectos Compose. Hasta ahora MAGI sabía si un servicio vigilado estaba caído (el sondeo de la F2), pero para saber por qué o para reiniciarlo había que abrir una terminal y escribir `systemctl` y `journalctl` a mano, o preparar un snippet.

La vista nueva está en `F9`: hosts a la izquierda y, a la derecha, los servicios, timers y contenedores del host elegido. Desde ahí se ve el estado, la unidad y las dependencias; se arranca, para, reinicia y recarga; se activa o desactiva el arranque; se siguen los logs en vivo con filtros, y se entra en un contenedor con una shell. Lo que deja algo parado pasa por la **deliberación MAGI** (F6), y recargar o reiniciar Nginx, Caddy, Apache, HAProxy o un proyecto Compose **comprueba antes la configuración**.

Todo lo remoto lo hace el servidor de sesiones sobre el pool, con un script embebido (`servicios.sh`) que recibe argumentos validados, como el sondeo y voxl. La lógica (órdenes, lectura de las salidas, acciones permitidas y validadores) vive en `magi-nucleo`, para que la F14 la lleve al móvil sin rehacerla.

**Objetivos principales**

1. Vista Servicios (`F9`, y `u` desde Flota, Hosts y la ficha) con servicios, timers y contenedores del host; vigilados y fallidos arriba, y «Toda la flota» con los fallos de todos los hosts.
2. Acciones sobre servicios, timers, contenedores y proyectos Compose, con espera al resultado y las últimas líneas del diario si algo no vuelve.
3. Logs en vivo en un panel propio (filtro por texto y por nivel, pausa) y en una pestaña con el seguimiento completo.
4. Seguridad: `sudo` orden a orden, con `sudo -n` o con una contraseña pedida en un diálogo que llega al host por la entrada estándar y no se guarda; deliberación para lo que deja algo parado; comprobación de configuración dentro de la propia acción.
5. Flota marca los servicios fallidos y los contenedores caídos de cada host a partir del sondeo.
6. Lógica en `magi-nucleo`, lista para el móvil (F14); protocolo v8 y versión 1.3.0.

**Contexto.** Producto propio. Hector despliega sus aplicaciones en servidores con systemd detrás de Nginx, y algunos hosts usan Docker con Compose. La vista sustituye la secuencia de cada incidencia: `ssh host`, `systemctl status`, `journalctl -u`, `sudo systemctl restart` y volver a mirar.

Respuestas de Hector (8 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | Servicios y timers de systemd del sistema, journalctl y contenedores de Docker y Podman con sus proyectos Compose (a) |
| 2 | Estado, unidad y dependencias; arrancar, parar, reiniciar, recargar, y activar o desactivar el arranque; espera a `active` y, si no vuelve, las últimas líneas del log; timers con próxima y última ejecución y lanzar ahora (a) |
| 3 | Contenedores: lista con estado, imagen, puertos y consumo; logs; arrancar, parar y reiniciar; shell en una pestaña nueva; Compose por proyecto; actualizar queda para Despliegues (a) |
| 4 | Vista nueva en `F9`, con `u` desde Flota, Hosts y la ficha (a) |
| 5 | Panel de logs propio en vivo, con filtro por texto y prioridad, pausa y salto al final, 10 000 líneas, y `t` para el seguimiento completo en una pestaña (a) |
| 6 | Arriba los vigilados de la ficha y los fallidos; debajo el resto, con filtro (a) |
| 7 | El sondeo cuenta servicios fallidos y contenedores caídos; Flota los marca y un filtro junta los de toda la flota (a) |
| 8 | `sudo -n`; si pide contraseña, diálogo y entrada estándar, sin guardarla (a) |
| 9 | Deliberación para parar, reiniciar, desactivar y lo que deje parado un contenedor o un proyecto, con las comprobaciones del host; el resto, confirmación (a) |
| 10 | Validar la configuración de Nginx, Caddy, Apache, HAProxy y Compose antes de recargar o reiniciar, y comprobación propia por servicio; si falla, no se ejecuta (a) |
| 11 | Al entrar, con `r` y cada 10 s mientras la vista está abierta; nada en marcha al salir (a) |
| 12 | Lógica en `magi-nucleo` y vista solo en el escritorio; el móvil la recibe en la F14 (a) |

**Fuera de alcance**

- Unidades de usuario (`systemctl --user`), sockets, montajes y el resto de tipos de unidad.
- Editar unidades, `daemon-reload`, `mask` y `unmask` (una unidad se edita con Archivos, F8).
- Kubernetes, LXC/LXD, Incus y máquinas virtuales.
- Borrar contenedores, imágenes o volúmenes, `prune`, y actualizar (`pull`, `up`, `build`): actualizar va con los despliegues (F16).
- Acciones en varios hosts a la vez (para eso están los snippets, F6, y la F16).
- Avisos de escritorio y vigilar con MAGI cerrado (D15 de la F2).
- Hosts sin systemd (OpenRC, runit, BSD, macOS): se ven como «sin systemd», con sus contenedores si los hay.
- La vista en el móvil (F14) y órdenes nuevas de CLI.

**Versión prevista:** escritorio **1.3.0** (MINOR: funcionalidad nueva, migración 9, protocolo v8 y `formato` 3 de sincronización solo cuando hay comprobaciones propias, T68). La app Android solo cambia lo justo para leer el `formato` 3 (línea en su CHANGELOG; la versión la decide el desarrollador al publicar).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Host remoto | `sh` POSIX, systemd 230 o posterior (`systemctl`, `journalctl`), Docker 20.10+ o Podman 4+, Compose v2 (`docker compose` o `podman compose`), `sudo` y `timeout` | Nada que instalar: MAGI usa lo que el host ya tiene |
| Script | `crates/magi-nucleo/scripts/servicios.sh`, embebido con `include_str!` | Argumentos validados en Rust y en el script; salida en líneas `MAGI_*` |
| Núcleo | `crates/magi-nucleo/src/servicios/` | Tipos, órdenes, lectura de salidas, acciones permitidas, validadores y límites; sin dependencias nuevas |
| Servidor | `src/servidor/servicios.rs` sobre el pool (`UsoCanal::Servicios`, T42) | Refrescos, acciones custodiadas, logs en vivo y `AccionesServicio{lista}` |
| Cliente | `src/app/servicios.rs` y `src/ui/servicios.rs` | Vista `F9`, detalle, panel de logs con su anillo, diálogos (sudo, confirmación, resultado) y deliberación (F6) |
| Datos | SQLite, migración 9 | Tabla COMPROBACIONES_SERVICIO (viaja) y columna `SONDEOS.fallos_json` |
| Protocolo | `VERSION_PROTOCOLO = 8` | Mensajes de §5.2 |
| Sincronización | `formato` 3 solo si hay comprobaciones propias (F10) | La app Android (F11) lo lee |

No hay dependencias nuevas de Rust: `serde_json` lee los JSON de Docker, Podman y `journalctl -o json`.

### Estructura de carpetas (novedades)

```
crates/magi-nucleo/
├── src/servicios/
│   ├── mod.rs          tipos: Objetivo (unidad, timer, contenedor, proyecto), Accion, Listado, Privilegio,
│   │                   Resultado, OrdenPestana
│   ├── orden.rs        argumentos de servicios.sh, nombres válidos y modo de privilegio
│   ├── lectura.rs      líneas MAGI_*, bloques de `systemctl show` y JSON de docker, podman y journalctl
│   ├── acciones.rs     acciones permitidas por tipo y estado, deliberación exigida y unidades críticas
│   ├── validadores.rs  Nginx, Caddy, Apache, HAProxy, Compose y comprobaciones propias
│   └── limites.rs      topes de la entrada no fiable (§8)
├── scripts/servicios.sh
└── tests/              servicios_lectura.rs, servicios_script.rs, sincronizacion_comprobaciones.rs
src/flota/sondeo.sh            + bloque de fallos (unidades fallidas y contenedores caídos)
src/servidor/servicios.rs      refrescos, acciones, logs, AccionesServicio{lista} y TrabajoServidor
src/app/servicios.rs           estado de la vista, anillo de logs y contraseña de sudo en memoria
src/ui/servicios.rs            vista F9, detalle, panel de logs y diálogos; marcas en Flota; bloque de la ficha
tests/servicios_servidor.rs · tests/servicios_ui.rs · tests/redimensionado/   servidor e instantáneas
tests/fijos/servicios/         salidas grabadas de systemd, Docker y Podman de varias distribuciones
docs/formato-sincronizacion.md   formato 3 y el elemento comprobacion_servicio
android/                       leer el formato 3
```

### Diagrama de arquitectura

```
 EQUIPO DEL USUARIO (Omarchy o macOS)                                        HOST SSH CON SYSTEMD
                                 ┌─ magi --servidor ────────────────────┐    ┌─ web-01 ───────────────────────┐
 ┌──────────────────────────┐    │ ┌──────────────────────────────────┐ │    │ ┌────────────────────────────┐ │
 │ TUI magi (cliente)       │ v8 │ │ servicios                        │ │    │ │ servicios.sh               │ │
 │ vista F9 · ⚠N en Flota   │◀───┼▶│ refrescos · acciones · logs      │ │exec│ │ privilegio · listar        │ │
 │ panel de logs (anillo)   │    │ │ AccionesServicio{lista}          │ │ ╭──┼▶│ detalle · accion · logs    │ │
 │ sudo en memoria (5 min)  │    │ │ TrabajoServidor                  │ │ │  │ │ sudo -n o sudo -S          │ │
 │ deliberación (F6)        │    │ └──────────────────────────────────┘ │ │  │ └────────────────────────────┘ │
 │ pestañas con orden       │    │                            ▼         │ │  │               ▼                │
 │ comprobaciones (ficha)   │    │ ┌───────────────┐  ┌───────────────┐ │ │  │  systemctl · journalctl        │
 └──────────────────────────┘    │ │ hilo escritor │  │ pool SSH      │─┼─╯  │  docker · podman · compose     │
                                 │ │ REGISTRO      │  │ UsoCanal::    │ │    │  nginx -t · caddy validate     │
                                 │ │ DELIBERACIONES│  │ Servicios     │ │    │  apachectl · haproxy -c        │
                                 │ └───────────────┘  └───────────────┘ │    │                                │
                                 └──────────────────────────────────────┘    └────────────────────────────────┘

 magi-nucleo/src/servicios: órdenes, lectura de salidas, acciones permitidas, validadores y límites.
 Lo usan el cliente y el servidor; en la F14, también la app Android.
```

La TUI nunca habla con el host: pide listados, acciones y logs al servidor, que ejecuta `servicios.sh` por un canal `exec` de la conexión del pool. La contraseña de sudo la pide el cliente y viaja dentro de cada petición; el servidor no la guarda ni toca el llavero (T25). El servidor escribe en SQLite solo REGISTRO y el resultado de la deliberación, por su hilo escritor (T26); COMPROBACIONES_SERVICIO la escribe el cliente.

---

## 3. Modelo de Datos

La **migración 9** (`user_version` 9) añade una tabla y una columna. Las acciones en curso y las suscripciones de log viven en la memoria del servidor, y los listados, el anillo de logs y la contraseña de sudo, en la del cliente (T23).

### Diagrama E-R (cambios de la fase)

```
 ┌──────────────────────────────────────┐         ┌────────────────────────────────────────────┐
 │ HOSTS (sin cambios)                  │  1:N    │ COMPROBACIONES_SERVICIO              (F13) │
 │ id uuid tipo nombre …   (F1-F12)     │────────<│ id  host_id (FK HOSTS ON DELETE CASCADE)   │
 │ servicios: los vigilados    (F2)     │         │ objetivo_tipo   unidad | proyecto          │
 │ solo hosts de tipo ssh    (T100)     │         │ objetivo        nginx.service · web        │
 │                                      │         │ comando         orden de comprobación      │
 └──────────────────────────────────────┘         │ actualizado_en                             │
            │                       │             │ uuid version vector   (viaja, T70)         │
            │ 1:N                   │ 1:N         └────────────────────────────────────────────┘
            ▼                       ▼              UNIQUE(host_id, objetivo_tipo, objetivo)
 ┌────────────────────────────┐  ┌───────────────────────────────┐
 │ SONDEOS (F2)               │  │ REGISTRO (tipo nuevo)         │  ┌─────────────────────────────┐
 │ id host_id fecha resultado │  │ servicio_accion               │  │ DELIBERACIONES (F6)         │
 │ métricas · servicios_json  │  │ host_id · detalle · resultado │  │ accion: «reiniciar          │
 │ fallos_json          (F13) │  │ nunca la salida (T44)         │  │ app.service → web-01»       │
 │ 20 por host · no viaja     │  │                               │  │ sin columnas nuevas         │
 └────────────────────────────┘  └───────────────────────────────┘  └─────────────────────────────┘

 En memoria del servidor (sin tabla): acciones en curso y suscripciones de log.
 En el cliente: listados, anillo de logs y contraseña de sudo por host (Zeroizing, 5 min sin uso).
```

### 3.1 COMPROBACIONES_SERVICIO (nueva, migración 9)

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | INTEGER PK | |
| `host_id` | INTEGER NOT NULL, FK HOSTS ON DELETE CASCADE | Solo hosts de tipo `ssh` (T100) |
| `objetivo_tipo` | TEXT NOT NULL | `unidad` o `proyecto` (Compose), validado en código (T8) |
| `objetivo` | TEXT NOT NULL | Unidad (`nginx.service`) o proyecto Compose (`web`), con los nombres de §7.11 |
| `comando` | TEXT NOT NULL | Orden que se ejecuta en el host antes de recargar o reiniciar ese objetivo, en lugar de la comprobación conocida; una línea de 1 a 1 024 caracteres |
| `actualizado_en` | TEXT NOT NULL | ISO 8601 |
| `uuid`, `version`, `vector` | TEXT | Como toda tabla que viaja (T70) |

`UNIQUE(host_id, objetivo_tipo, objetivo)`. Sin `CHECK` (T8). Viaja como elemento propio, `comprobacion_servicio`, y no dentro del host: así un equipo con MAGI 1.2 que edita el host no la borra sin saberlo (D249).

### 3.2 SONDEOS: columna nueva (migración 9)

| Campo | Tipo | Descripción |
|---|---|---|
| `fallos_json` | TEXT NULL (JSON) | Fallos del host en ese sondeo. NULL en los sondeos anteriores a la F13 o si el host no respondió |

```json
{"unidades": {"total": 1, "nombres": ["copia.service"]},
 "contenedores": {"motor": "docker", "total": 2, "nombres": ["web-worker-1", "panel"]}}
```

`"contenedores": null` significa que no hay motor o que el usuario SSH no tiene permiso: el sondeo nunca usa sudo (T15). Cada lista guarda como mucho 20 nombres; `total` es el recuento real. SONDEOS no viaja (D133).

### 3.3 Acción de servicio (memoria del servidor, sin tabla)

| Campo | Descripción |
|---|---|
| `id` | El `peticion_id` de la `AccionServicio` del solicitante |
| `host_id`, `objetivo`, `accion` | Qué se hace y dónde (objetivo: tipo y nombre) |
| `fase` | `comprobando`, `ejecutando` o `esperando` (diagrama de §3.8) |
| `deliberacion_id` | La deliberación que la autoriza, si hace falta |
| `solicitante` | Cliente que la pidió; recibe `AccionServicioHecha` |
| `conexion` | `ConexionTomada` del pool, que se suelta por identidad (T34) |
| `desde` | Hora de inicio, para la duración en REGISTRO |

Se difunde en `AccionesServicio{lista}` (T27) y cuenta en `TrabajoServidor.acciones_servicio`.

### 3.4 Suscripción de log (memoria del servidor, sin tabla)

| Campo | Descripción |
|---|---|
| `id` | El `peticion_id` de `SeguirLog` |
| `host_id`, `objetivo` | Servicio (o el servicio de un timer) o contenedor |
| `canal` | Canal `exec` con `servicios.sh logs`; cerrarlo para el seguimiento en el host |
| `lote`, `cola_bytes`, `omitidas` | Líneas pendientes de mandar, tamaño de la cola y líneas descartadas (§7.6) |
| `solicitante` | Único cliente que recibe sus `LineasLog` |

No es trabajo del servidor: se cierra con `PararLog`, al desconectarse el solicitante o al caer la conexión.

### 3.5 Estado de la vista (memoria del cliente)

| Dato | Descripción |
|---|---|
| Listado por host | Último `Servicios` recibido con su hora; se conserva si el host deja de responder |
| Anillo de logs | Hasta `[servicios] lineas_log` (10 000) líneas del panel abierto; se vacía al cerrarlo |
| Contraseña de sudo | Por host, en `Zeroizing`, hasta `[servicios] sudo_memoria_min` (5) minutos sin uso o hasta salir de la vista |
| Filtros | Texto por sección y nivel del panel de logs |

### 3.6 REGISTRO: tipo nuevo

| Tipo | Cuándo | Detalle |
|---|---|---|
| `servicio_accion` | Al terminar una acción, también si falla o la para la comprobación | Objetivo (tipo y nombre), acción, resultado (`hecho`, `fallido`, `sin_confirmar`, `rechazada_configuracion` o `error`), duración y deliberación (id) si la hubo. Nunca la salida de las órdenes (T44). `resultado` de REGISTRO es `ok` solo con `hecho` |

Abrir una shell en un contenedor o seguir unos logs no se anota: son lecturas o pestañas, como abrir una sesión, que ya anota la F3.

### 3.7 Relaciones

- `COMPROBACIONES_SERVICIO.host_id → HOSTS.id` (CASCADE): solo hosts `ssh`. Borrar el host borra sus comprobaciones y deja sus lápidas, igual que con los túneles (F10).
- `SONDEOS.host_id → HOSTS.id` (F2), sin cambios; `fallos_json` alimenta Flota y «Toda la flota».
- DELIBERACIONES (F6) guarda las de acciones de servicio con `snippet_id` NULL, la acción en `accion` y el host en `hosts_json`, como hizo la F8 con las sincronizaciones.
- REGISTRO guarda `servicio_accion` con su `host_id`.
- `HOSTS.servicios` (F2) dice qué unidades son vigiladas: salen arriba con `★`.

### 3.8 Diagramas de estados

**Acción de servicio**

```
  (cliente) A · P · R · G · H, o A en un timer
      ¿permitida en ese estado (§7.2)? ─no─▶ aviso «nginx.service ya está parado» (fin, sin acción)
      ¿hace falta sudo con contraseña y no está en memoria? ─sí─▶ diálogo ─ Esc o 3 rechazos ─▶ (fin, sin acción)
      ¿deja algo parado? ─sí─▶ deliberación ─ Esc ─▶ (fin, sin acción)
                         └no─▶ confirmación ─ Esc ─▶ (fin, sin acción)
  (servidor) AccionServicio ─ inválida o sin deliberación válida ─▶ Error (fin)
      ─▶ comprobando (si el objetivo tiene comprobación) ─ falla ─▶ rechazada_configuracion (fin)
      ─▶ ejecutando ─ la orden falla ─▶ fallido (fin)
      ─▶ esperando ─ estado esperado ─▶ hecho (fin)
                   ─ failed, o el contenedor no queda en marcha ─▶ fallido (fin, con las últimas líneas)
                   ─ espera_seg sin estado final ─▶ sin_confirmar (fin; la orden sigue en el host)
      cualquier fase ─ conexión caída o plazo del servidor ─▶ error (fin)
  Inválidas: ejecutar sin deliberación cuando hace falta · dos acciones a la vez sobre el mismo
             objetivo (la segunda se rechaza: «hay otra acción en curso») · salir de un estado final
```

**Unidad de servicio (estado del host) y acciones que MAGI ofrece**

```
  inactive ○ ─ arrancar ─▶ activating ◐ ─▶ active ● | failed ✕
  failed ✕   ─ arrancar · reiniciar ─▶ activating ◐ ─▶ active ● | failed ✕
  active ●   ─ parar ─▶ deactivating ◐ ─▶ inactive ○
  active ●   ─ reiniciar ─▶ deactivating ─▶ activating ◐ ─▶ active ● | failed ✕
  active ●   ─ recargar (solo con CanReload=yes) ─▶ reloading ◐ ─▶ active ●
  arranque:  disabled ─ activar ─▶ enabled · enabled ─ desactivar ─▶ disabled
             static, indirect, generated, transient y alias: sin activar ni desactivar
  masked:    se ve, sin ninguna acción · activating, deactivating y reloading: sin acciones (esperar)
  Inválidas (MAGI no las ofrece): parar lo inactivo · recargar sin CanReload · arrancar lo enmascarado
```

**Contenedor**

```
  created ○ · exited ○ ─ arrancar ─▶ running ●
  running ● ─ parar ─▶ exited ○ (código 0)
  running ● · restarting ◐ · exited ─ reiniciar ─▶ running ●
  running ● ─ el proceso sale con código ≠ 0 ─▶ exited ✕ (caído)
  running ● ─ su política de reinicio lo relanza una y otra vez ─▶ restarting ◐ (caído)
  running ● ─ el healthcheck falla ─▶ running ⚠ unhealthy (caído)
  paused · dead ─▶ sin acciones (logs y detalle sí)
  Inválidas: arrancar lo que corre · parar lo parado · shell en un contenedor que no corre
```

**Suscripción de log**

```
  (inicio) ─ l ─▶ abriendo ─▶ siguiendo
  siguiendo ─ Espacio o desplazarse hacia arriba ─▶ en pausa (las líneas siguen entrando en el anillo)
  en pausa ─ Espacio o Fin ─▶ siguiendo
  siguiendo · en pausa ─ Esc · otro host · salir de la vista ─▶ PararLog ─▶ cerrada (fin)
  abriendo · siguiendo ─ fin del proceso remoto · conexión caída · sin permiso ─▶ terminada
                         (fin, con el motivo; las líneas se quedan en el panel)
  Inválidas: dos suscripciones del mismo cliente al mismo objetivo (la nueva sustituye a la anterior)
```

**Contraseña de sudo (memoria del cliente, por host)**

```
  ninguna ─ una acción la necesita · ! ─▶ pedida ─ ComprobarSudo: ok ─▶ en memoria
  pedida ─ rechazada (hasta 3 intentos) ─▶ pedida · 3 rechazos o Esc ─▶ ninguna
  en memoria ─ se usa ─▶ en memoria (vuelven a contar los 5 min)
  en memoria ─ 5 min sin usarla · salir de la vista · el host la rechaza ─▶ ninguna (se borra)
  Inválidas: escribirla en disco, en el llavero, en el log o en REGISTRO · mandarla a otro host
```

Las comprobaciones propias no tienen ciclo de vida propio: se crean, se editan y se borran (con lápida) como el resto del inventario (F10).

---

## 4. Flujos de Trabajo

### 4.1 Abrir la vista y refrescar

```
  F9, o u en Flota, Hosts o la ficha (con ese host elegido)
      ▼
  hosts de tipo ssh a la izquierda, con el estado y ⚠N de su último sondeo; arriba, «Toda la flota» (§4.6)
      ▼ un host
  ¿conexión en el pool? ─no─▶ conexion_para_canal(host, Servicios): huella, frase o contraseña con diálogos,
      │                       porque lo ha pedido el usuario ─ error o 10 s ─▶ «no se llega a web-01» · r (fin)
      ▼
  ListarServicios ─▶ servidor: exec servicios.sh listar (plazo de 10 s)
      ├─ sin systemd o systemd < 230 ─▶ «sin systemd» o «systemd demasiado antiguo»: solo contenedores
      ├─ sin Docker ni Podman ─▶ la sección Contenedores dice «no hay Docker ni Podman»
      ├─ sin permiso (diario o Docker) ─▶ ¿contraseña en memoria o sudo -n? ─sí─▶ esa parte otra vez, con sudo
      │                                   └─no─▶ aviso «sin permiso para Docker: ! contraseña de sudo»
      ├─ salida por encima de los límites ─▶ se corta y la vista dice «lista incompleta» (T38)
      └─ 10 s sin respuesta ─▶ «web-01 no respondió en 10 s» (se queda el listado anterior, con su hora)
      ▼
  Servicios(listado) ─▶ el cliente ordena (vigilados y fallidos arriba, §7.1) y pinta
      ▼
  sección Contenedores visible ─▶ PedirConsumo (docker stats --no-stream) ─▶ CPU y memoria, sin frenar lo demás
      ▼
  cada refresco_seg (10) mientras la vista está abierta en ese host: otro ListarServicios
      (sin solapar refrescos y sin diálogos, T15; si la conexión cayó y no se puede abrir sin diálogo:
       «desconectado · r reconectar»)
  salir de la vista o pasar a otra ─▶ no se pide nada más; se sueltan la conexión tomada y los logs
```

### 4.2 Acción sobre un servicio

```
  R en app.service (web-01)
      ▼ ¿permitida en su estado (§7.2)? ─no─▶ la barra no la ofrece y la tecla avisa por qué (fin)
      ▼
  ¿hace falta sudo con contraseña (privilegio sudo_con_clave) y no está en memoria?
      └─ sí ─▶ [CONTRASEÑA DE SUDO] ─▶ ComprobarSudo
              ├─ rechazada ─▶ otra vez (3 intentos) ─▶ fin sin acción
              └─ sin_sudo ─▶ «usuario no puede usar sudo en web-01» (fin)
      ▼
  ¿deja algo parado (parar, reiniciar, desactivar, parar o reiniciar un contenedor o un proyecto)?
      ├─ sí ─▶ [DELIBERACIÓN MAGI] con las comprobaciones activas del host; MELCHIOR-1 no cuenta el
      │        estado del propio objetivo (§7.2) ─ Esc ─▶ fin
      │        └─ Ctrl+K (aprobada, o forzada con motivo) ─▶ el cliente guarda DELIBERACIONES
      └─ no ─▶ [CONFIRMACIÓN] «¿recargar nginx.service en web-01?» ─ Esc ─▶ fin
      ▼
  AccionServicio{host, objetivo, acción, deliberación?, sudo?}
      ▼ servidor: nombres, acción y deliberación (que exista, sea de esta acción y no esté usada, T62)
      │           ─ inválida ─▶ Error (fin) · otra acción sobre ese objetivo ─▶ «hay otra acción en curso»
      ▼ AccionesServicio{lista} con la acción en curso (antes del efecto, T40)
  comprobando ─▶ ¿Nginx, Caddy, Apache, HAProxy, Compose o comprobación propia? (§4.3)
      │          └─ falla ─▶ rechazada_configuracion, con la salida del validador (fin)
      ▼
  ejecutando ─▶ timeout espera_seg + sudo systemctl restart app.service (bloquea hasta acabar el trabajo)
      ├─ código ≠ 0 ─▶ fallido con el mensaje de systemctl y las últimas 20 líneas del diario
      └─ timeout ─▶ sin_confirmar «la orden sigue en el host: r para ver el estado»
      ▼
  esperando: a los 2 s, otra mirada al estado (pilla lo que muere nada más arrancar)
      ├─ active, o un oneshot terminado con éxito ─▶ hecho ✓
      └─ failed ─▶ fallido ✕ con las últimas 20 líneas del diario
      ▼
  REGISTRO servicio_accion · DELIBERACIONES.ejecucion_resultado y deliberacion_* · Barrera (T47)
      ▼
  AccionServicioHecha al solicitante ─▶ [RESULTADO] ─▶ refresco inmediato del listado
```

### 4.3 Comprobación de configuración

```
  recargar o reiniciar un objetivo
      ▼
  ¿tiene comprobación propia (COMPROBACIONES_SERVICIO)? ─sí─▶ esa orden, con sh -c
      └─no─▶ ¿la reconoce MAGI?
              nginx.service ─▶ nginx -t
              caddy.service ─▶ caddy validate --config <c> --adapter <a> (de su ExecStart; si no,
                               /etc/caddy/Caddyfile y caddyfile)
              apache2.service o httpd.service ─▶ apachectl configtest
              haproxy.service ─▶ haproxy -c -f <f> (de su ExecStart; si no, /etc/haproxy/haproxy.cfg)
              proyecto Compose ─▶ docker compose -p <p> -f <ficheros> config -q (o podman compose)
              otra ─▶ sin comprobación: se ejecuta tal cual
      ▼ en el mismo canal, con los mismos privilegios y justo antes de la acción (sin hueco entre las dos)
  código 0 ─▶ sigue con la acción
  código ≠ 0, también 127 (no encontrada), o 20 s sin terminar ─▶ rechazada_configuracion: no se ejecuta
      y el diálogo de resultado enseña la salida (hasta 16 KiB); «c» edita la comprobación propia
```

### 4.4 Logs en vivo

```
  l en un servicio, en un timer (su servicio) o en un contenedor
      ▼
  SeguirLog{host, objetivo, lineas_iniciales = 200, sudo?} ─▶ servidor: exec servicios.sh logs …
      ├─ unidad ─▶ journalctl -u <unidad> -f -n 200 -o json --no-pager
      └─ contenedor ─▶ docker|podman logs -f --tail 200 --timestamps <id> (stdout y stderr aparte)
      ▼ el servidor lee línea a línea, recorta a 16 KiB y manda lotes cada 100 ms (hasta 500 líneas)
  LineasLog{lineas, omitidas} ─▶ cliente: anillo de 10 000 líneas, filtro por texto y nivel
      ├─ el cliente no da abasto y la cola del servidor pasa de 4 MiB ─▶ se descartan las más viejas
      │  y el panel enseña «… N líneas omitidas» donde faltan
      ├─ diario sin permiso ─▶ FinLog sin_permiso: «sin permiso para el diario: ! contraseña de sudo»
      ├─ Espacio ─▶ en pausa: la vista no se mueve, pero las líneas siguen entrando en el anillo
      └─ t ─▶ pestaña con orden: journalctl -fu o docker logs -f (§4.5); el panel sigue abierto
      ▼
  Esc, otro host o salir de la vista ─▶ PararLog ─▶ el servidor cierra el canal ─▶ fin de la entrada
      estándar ─▶ el script mata el seguimiento en el host
  el proceso remoto termina o la conexión cae ─▶ FinLog{motivo} ─▶ el panel lo dice; las líneas se quedan
```

### 4.5 Contenedores: shell y pestañas con orden

```
  S en un contenedor en marcha
      ▼
  AbrirSesion{host, orden: ShellContenedor{motor, id}} (la pestaña de la F3, con PTY)
      ▼ servidor: arma la orden desde el tipo, nunca desde texto libre (D248)
  [sudo] docker exec -it <id> sh -c 'command -v bash >/dev/null 2>&1 && exec bash || exec sh'
      ├─ Docker necesita sudo con contraseña ─▶ sudo la pide dentro de la propia pestaña (es una terminal)
      ├─ el contenedor ya no corre o no tiene sh ─▶ la pestaña enseña el error y se cierra con una tecla
      └─ abierta ─▶ pestaña «web-01 · web-app-1 (shell)» en Sesión
      ▼
  exit ─▶ la pestaña se cierra, como al salir de una shell (F3)
  t en el panel de logs ─▶ lo mismo con SeguirDiario{unidad} o SeguirContenedor{motor, id}:
      pestaña «web-01 · app.service (logs)» con journalctl -fu o docker logs -f, con sus colores
```

### 4.6 Sondeo ampliado, Flota y «Toda la flota»

```
  sondeo.sh (F2) + bloque de fallos, sin sudo y sin diálogos (T15):
      systemctl list-units --type=service --state=failed ─▶ total y hasta 20 nombres
      docker o podman ps -a ─▶ caídos (salieron con código ≠ 0, se reinician una y otra vez o están
                                unhealthy) ─▶ total y hasta 20 nombres
      sin motor o sin permiso ─▶ "contenedores": null (sin datos)
      ▼
  SONDEOS.fallos_json ─▶ el estado del host no cambia por estos fallos (solo por los vigilados, como en la F2)
      ▼
  Flota: ⚠N en la fila del host y bloque FALLOS en su detalle
  F9 · Toda la flota: los fallos de todos los hosts juntos, del último sondeo de cada uno y con su hora
      (entrar no abre conexiones)
      ├─ ↵ ─▶ ese host, con el elemento seleccionado en su sección
      ├─ l · A · R ─▶ los logs o la acción en ese host, como en §4.4 y §4.2
      ├─ host caído o sin sondeo ─▶ una línea «nas: CAÍDA en el último sondeo · sin datos de servicios»
      └─ r ─▶ sondear todos (como R en Flota)
```

### 4.7 Sincronización con comprobaciones propias

```
  exportar el fichero del equipo:
      ├─ ninguna comprobación propia ─▶ formato 1 o 2, como en la F12 (lo leen MAGI 1.1 y 1.2)
      └─ alguna ─▶ formato 3, con elementos comprobacion_servicio (host por uuid, objetivo y orden)
  importar el fichero de otro equipo:
      ├─ formato 1 o 2 ─▶ como en la F12; las comprobaciones de este equipo no cambian
      ├─ formato 3 ─▶ fusión por vectores, como cualquier elemento (T72); un borrado llega como lápida
      └─ formato > 3 ─▶ equipo formato_nuevo · aviso «actualiza MAGI» (T74)
  MAGI 1.2 ante un fichero formato 3 ─▶ formato_nuevo: no lo lee a medias
  guardar la primera comprobación propia ─▶ aviso: «los equipos con MAGI 1.2 dejarán de leer este equipo
      hasta que se actualicen»
  app Android ─▶ lee el formato 3 y guarda las comprobaciones en su copia, sin usarlas (F14)
  comprobación de un host que ya no existe ─▶ huérfana: conflicto recuperable, no se aplica (F10)
```

### 4.8 Diagrama de secuencia: reiniciar un servicio con sudo y deliberación

```
  TUI          deliberación (F6)       magi --servidor                            web-01 (SSH)
   │ R app.service    │                       │                                         │
   │ diálogo sudo     │                       │                                         │
   │─ComprobarSudo───────────────────────────▶│─ servicios.sh sudo (stdin) ────────────▶│
   │                  │                       │◀────────────────────────── MAGI_SUDO=ok │
   │◀───────────────────── SudoComprobado(ok) │                                         │
   │─deliberar───────▶│                       │                                         │
   │                  │─sondeo · backup──────▶│─ sondeo.sh · SFTP (≤ 2 s) ─────────────▶│
   │                  │◀────────── resultados │                                         │
   │◀─── 2/2 APROBADA │                       │                                         │
   │─Ctrl+K──────────▶│ DELIBERACIONES        │                                         │
   │─AccionServicio(reiniciar)───────────────▶│ valida deliberación                     │
   │◀───────────── AccionesServicio[en curso] │                                         │
   │                  │                       │─ servicios.sh accion … reiniciar ──────▶│
   │                  │                       │◀────────────────── MAGI_COMPROBACION=ok │
   │                  │                       │◀─────────────────── MAGI_FASE=esperando │
   │                  │                       │◀─────── MAGI_RESULTADO=hecho active 2,3 │
   │                  │                       │ REGISTRO servicio_accion · Barrera      │
   │◀───────────── AccionServicioHecha(hecho) │                                         │
   │─ListarServicios─────────────────────────▶│─ servicios.sh listar ──────────────────▶│
```

1. Pulsar `R` en `app.service`. El último listado dice que el usuario necesita sudo con contraseña y no hay ninguna en memoria, así que el cliente la pide y la comprueba con `ComprobarSudo` antes de nada.
2. Reiniciar deja el servicio parado un momento, así que hay deliberación: MELCHIOR-1 mira la salud del host sin contar `app.service` y BALTHASAR-2, el backup; CASPER-3 no está activo en ese host. Con `Ctrl+K` el cliente guarda la deliberación.
3. El servidor valida la deliberación, difunde la acción en curso y ejecuta `servicios.sh accion` con la contraseña por la entrada estándar.
4. El script pasa la comprobación propia, reinicia con `timeout` y `sudo systemctl restart` y, a los 2 s, vuelve a mirar el estado: `active`.
5. El servidor anota `servicio_accion`, pasa la barrera del hilo escritor y contesta `AccionServicioHecha`. El cliente enseña el resultado y refresca el listado.

### 4.9 Diagrama de secuencia: logs en vivo

```
 TUI (panel)                     magi --servidor                                web-01 (SSH)
   │─SeguirLog(app.service, 200)───────▶│─ servicios.sh logs … 200 ──────────────────▶│
   │                                    │◀─────────── 200 líneas (journalctl -o json) │
   │◀─────────────────── LineasLog[200] │                                             │
   │                                    │◀──────────────────────── líneas nuevas (-f) │
   │◀────────── LineasLog (cada 100 ms) │                                             │
   │ espacio: pausa                     │                                             │
   │◀──────────── LineasLog (al anillo) │                                             │
   │─PararLog──────────────────────────▶│─ cierra el canal: fin de stdin ────────────▶│
   │                                    │ el script corta journalctl                  │
   │◀────────────────── FinLog(cerrado) │                                             │
```

1. `l` en `app.service` pide las últimas 200 líneas y el seguimiento.
2. El script lanza `journalctl -u app.service -f -n 200 -o json`; el servidor lee línea a línea y manda lotes cada 100 ms.
3. Con la pausa, el panel deja de moverse pero las líneas siguen entrando en el anillo.
4. Al cerrar el panel, `PararLog`: el servidor cierra el canal, el script ve el fin de su entrada estándar y mata `journalctl`.

---

## 5. Script, Protocolo y Configuración

### 5.1 `servicios.sh`: subórdenes

MAGI lo ejecuta con `sh -c "$SCRIPT" magi-servicios <suborden> <privilegio> <argumentos…>`. La primera línea de la entrada estándar es siempre la contraseña de sudo (vacía si no hay); en `logs`, el fin de la entrada estándar dice que MAGI cerró el canal. Toda la salida va en líneas con prefijo `MAGI_` y termina con `MAGI_FIN`, salvo en `logs`.

| Suborden | Argumentos | Qué devuelve |
|---|---|---|
| `privilegio` | — | `MAGI_UID`, `MAGI_PRIV=root\|sudo_sin_clave\|sudo_con_clave\|sin_sudo` (con `LC_ALL=C`) |
| `sudo` | — (contraseña por stdin) | `MAGI_SUDO=ok\|rechazada\|sin_sudo` tras `sudo -S -p '' -v` |
| `listar` | `<motores>` (`docker,podman`, `docker`, `podman` o `-`) | `MAGI_SYSTEMD=<versión>\|no`, `MAGI_PRIV`, bloques de `systemctl show` de servicios y timers, `MAGI_DIARIO=todo\|propio`, `MAGI_MOTOR=<motor> <versión>\|no\|sin_permiso`, `MAGI_CONTENEDOR <json>`, `MAGI_PROYECTO <json>`, `MAGI_RELOJ=<epoch del host>` |
| `consumo` | `<motor>` | `MAGI_CONSUMO <json>` por contenedor en marcha (`stats --no-stream`) |
| `detalle` | `unidad <nombre>` o `contenedor <motor> <id>` | `MAGI_PROP clave=valor`, `MAGI_DEP necesita\|necesitada <nombre>` y `MAGI_TEXTO` seguido del texto de `systemctl cat` o de `inspect` (hasta 256 KiB) |
| `accion` | `<tipo> <objetivo> <acción> <espera_seg> <comprobación>` [`<orden propia>`] | `MAGI_FASE=comprobando\|ejecutando\|esperando`, `MAGI_COMPROBACION=ok\|fallo <código>`, `MAGI_RESULTADO=<resultado> <estado> <segundos>` y `MAGI_LINEA` con las últimas líneas o la salida de la comprobación |
| `logs` | `<tipo> <objetivo> <n>` [`<motor>`] | Líneas de `journalctl -o json`, o `MAGI_LOG <flujo> <hora> <texto>` de Docker y Podman, hasta que se cierra la entrada estándar |

`<comprobación>` es `conocida`, `propia` o `ninguna`; con `propia`, la orden va como último argumento y el script la ejecuta con `sh -c`. Los nombres van como argumentos separados y validados (§7.11), nunca dentro del texto del script.

### 5.2 Protocolo cliente ⇄ servidor (`VERSION_PROTOCOLO = 8`)

| Mensaje | Sentido | Contenido |
|---|---|---|
| `ListarServicios` → `Servicios` | cliente ⇄ servidor | `peticion_id`, `host_id`, `sudo?` → listado: systemd, privilegio, diario, unidades (servicios y timers con sus propiedades), motor, contenedores, proyectos, reloj del host, avisos e `incompleto` |
| `PedirConsumo` → `Consumo` | cliente ⇄ servidor | `peticion_id`, `host_id`, `sudo?` → CPU y memoria de cada contenedor en marcha |
| `DetalleServicio` → `Detalle` | cliente ⇄ servidor | `peticion_id`, `host_id`, `objetivo`, `sudo?` → propiedades, dependencias (necesita y la necesitan) y texto de la unidad o de `inspect` |
| `ComprobarSudo` → `SudoComprobado` | cliente ⇄ servidor | `peticion_id`, `host_id`, contraseña (`Zeroizing`) → `ok`, `rechazada` o `sin_sudo` |
| `AccionServicio` → `AccionServicioHecha` | cliente ⇄ servidor | `peticion_id`, `host_id`, `objetivo`, `accion`, `deliberacion_id?`, `sudo?` → resultado, estado final, segundos, mensaje y líneas (las últimas del diario o la salida de la comprobación) |
| `AccionesServicio{lista}` | servidor → todos | Lista completa de acciones en curso en cada cambio (T27): id, host, objetivo, acción, fase y desde |
| `SeguirLog` | cliente → servidor | `peticion_id`, `host_id`, `objetivo`, `lineas_iniciales`, `sudo?` |
| `LineasLog` | servidor → solicitante | `peticion_id`, líneas (hora, prioridad o flujo, texto) y `omitidas` |
| `PararLog` | cliente → servidor | `peticion_id` |
| `FinLog` | servidor → solicitante | `peticion_id` y motivo: `cerrado`, `terminado`, `sin_permiso` o `conexion_caida` |
| `AbrirSesion` con `orden` | cliente → servidor | Campo opcional nuevo de la F3: `ShellContenedor{motor, id}`, `SeguirDiario{unidad}` o `SeguirContenedor{motor, id}` |

La contraseña de sudo viaja en `Zeroizing` dentro de la petición que la necesita y nunca va al log. `TrabajoServidor` gana `acciones_servicio` (en curso): un servidor con acciones en curso no se sustituye sin preguntar (T67). Los `peticion_id` de servicios empiezan en 2^45, entre los de ejecuciones (2^44) y los de escritorios (2^46). `Hola` y `VersionIncompatible` no cambian (T66). Un `Servicios`, un `Detalle` o un `LineasLog` de una petición que ya no está en vuelo se descarta (T33).

Plazos en el servidor (T32): 10 s para `ListarServicios`, `PedirConsumo`, `DetalleServicio` y `ComprobarSudo`; 20 s para la comprobación de configuración; `espera_seg` + 30 s para toda una acción. Los logs no tienen plazo: duran lo que la suscripción.

### 5.3 Configuración (`config.toml`)

| Clave | Por defecto | Uso |
|---|---|---|
| `[servicios] refresco_seg` | `10` | Refresco de la vista abierta (5-300; `0` = solo al entrar y con `r`) |
| `[servicios] espera_seg` | `30` | Espera máxima al estado final de una acción (5-300) |
| `[servicios] lineas_log` | `10000` | Tamaño del anillo del panel de logs (1 000-100 000) |
| `[servicios] lineas_iniciales` | `200` | Líneas anteriores al abrir los logs (0-5 000) |
| `[servicios] contenedores` | `["docker", "podman"]` | Motores, por orden de preferencia; `[]` desactiva los contenedores |
| `[servicios] criticas` | §7.9 | Unidades que pueden cortar la conexión con el host |
| `[servicios] sudo_memoria_min` | `5` | Minutos sin uso tras los que se olvida la contraseña de sudo (0-15; `0` = pedirla siempre) |

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 F9 SERVICIOS ◀── F9 desde cualquier vista ◀── u en Flota, Hosts o la ficha (con ese host elegido)
 │  hosts (solo ssh) ─┬─ ▲ Toda la flota ── fallos de todos los hosts ─ ↵ ─▶ ese host y elemento
 │                    │                                                └─ r sondear todos
 │                    └─ un host ─▶ 1 Servicios · 2 Timers · 3 Contenedores   (Tab: hosts ⇄ lista)
 │                         ├─ ↵ DETALLE (Tab: Estado · Dependencias · Unidad o inspect)
 │                         ├─ l LOGS (panel) ─ t ─▶ pestaña con orden en SESIÓN (F3)
 │                         ├─ A P R G H ─▶ [CONTRASEÑA DE SUDO] ─▶ [CONFIRMACIÓN] o [DELIBERACIÓN MAGI]
 │                         │               ─▶ [RESULTADO]
 │                         ├─ S (contenedor) ─▶ pestaña con orden: shell en SESIÓN (F3)
 │                         ├─ c ─▶ [COMPROBACIÓN PROPIA] del servicio o del proyecto
 │                         ├─ ! ─▶ [CONTRASEÑA DE SUDO] para este host
 │                         └─ / filtrar · r refrescar · Esc volver
 FLOTA (F1) ── ⚠N en la fila · bloque FALLOS en el detalle · u ─▶ F9 en ese host
 HOSTS (F2) ── u ─▶ F9 en ese host
 FICHA ── bloque «Comprobaciones de servicio»: n nueva · e editar · x borrar [confirmación] · u ─▶ F9
 REGISTRO (F7) ── servicio_accion
 Barra de Sesión ── «reiniciando app.service en web-01» mientras hay acciones en curso (de cualquier ventana)
```

### 6.1 Servicios de un host

```
┌ MAGI · SERVICIOS ──┬────────────────────────────────────────── web-01 · systemd 252 · hace 4 s ──┐
│ HOSTS              │ ▸1 Servicios (142)    2 Timers (9)    3 Contenedores (6)                    │
│ ────────────────── │ ─────────────────────────────────────────────────────────────────────────── │
│ ▲ Toda la flota ⚠4 │    UNIDAD                      ESTADO           ARRANQUE      DESDE         │
│ ▸ web-01     ● ⚠3  │    copia.service               ✕ fallido        activado      hace 2 h      │
│   web-02     ●     │  ★ nginx.service               ● activo         activado      hace 14 d     │
│   db-01      ◐ ⚠1  │  ★ app.service                 ● activo         activado      hace 3 h      │
│   bastion    ●     │  ★ postgresql.service          ● activo         activado      hace 14 d     │
│   nas        ✕     │    ──────────────────────────────────────────────────────────────────       │
│                    │    cron.service                ● activo         activado      hace 14 d     │
│                    │    fail2ban.service            ● activo         activado      hace 14 d     │
│                    │    ssh.service                 ● activo         activado      hace 14 d     │
│                    │    tareas.service              ○ inactivo       desactivado   hace 6 d      │
│                    │    ufw.service                 ● activo         activado      hace 14 d     │
│                    │    …                                                                        │
│                    │ 142 servicios · 1 fallido · 3 vigilados                                     │
├────────────────────┴─────────────────────────────────────────────────────────────────────────────┤
│ ↵ detalle  l logs  A arrancar  P parar  R reiniciar  G recargar  H arranque  r refrescar  ? ayuda│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Arriba van los fallidos y los vigilados de la ficha (`★`), separados del resto por una línea. Con 120 columnas o más aparece la descripción de cada unidad y, con menos de 100, el modo estrecho enseña un solo panel (hosts o lista) y `Tab` cambia de uno a otro (T45). «Desde» y «hace» se calculan con el reloj del host, que viene en el listado. Un host sin permiso para algo lo dice en la última línea: «sin permiso para el diario del sistema: ! contraseña de sudo».

**Detalle (`↵`).** Ocupa el panel de la derecha y tiene tres partes que cambian con `Tab`:

- **Estado:** descripción, estado y subestado desde cuándo, arranque, PID principal, memoria, tareas, fichero de la unidad, si admite recargar, si es vigilada o crítica, y qué comprobación de configuración se aplicará.
- **Dependencias:** «necesita» y «la necesitan», en su primer nivel.
- **Unidad:** el texto de `systemctl cat`, de solo lectura.

En un contenedor, el detalle enseña id, imagen, creado, estado y código de salida, reinicios, salud con su última comprobación, puertos, montajes, redes, proyecto Compose y orden, y el texto de `inspect`. En un timer, la próxima y la última ejecución, el servicio que activa con su estado y la unidad.

### 6.2 Contenedores

```
┌ MAGI · SERVICIOS ──┬────────────────────────────────────────── web-01 · docker 27.3 · hace 4 s ──┐
│ HOSTS              │  1 Servicios (142)    2 Timers (9)   ▸3 Contenedores (6)                    │
│ ────────────────── │ ─────────────────────────────────────────────────────────────────────────── │
│ ▲ Toda la flota ⚠4 │    NOMBRE             ESTADO           IMAGEN                   CPU  MEM    │
│ ▸ web-01     ● ⚠3  │  ▾ web · compose · 3                                                        │
│   web-02     ●     │    web-app-1          ● en marcha      ghcr.io/ejemplo/app:1.4  2 %  180 MB │
│   db-01      ◐ ⚠1  │    web-worker-1       ✕ salió (1)      ghcr.io/ejemplo/app:1.4  —    —      │
│   bastion    ●     │    web-redis-1        ● en marcha      redis:7.4                0 %  12 MB  │
│   nas        ✕     │  ▾ copias · compose · 1                                                     │
│                    │    copias-restic-1    ○ parado (0)     restic/restic:0.17       —    —      │
│                    │  ▾ sin proyecto · 2                                                         │
│                    │    panel              ⚠ unhealthy      metabase/metabase:v0.50  9 %  740 MB │
│                    │    prueba             ○ creado         alpine:3.20              —    —      │
│                    │                                                                             │
│                    │ 6 contenedores · 3 en marcha · 2 caídos                                     │
├────────────────────┴─────────────────────────────────────────────────────────────────────────────┤
│ ↵ detalle  l logs  S shell  A arrancar  P parar  R reiniciar  c comprobación  r refrescar        │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Los contenedores van por proyecto Compose y, al final, los que no tienen proyecto. Sobre la cabecera de un proyecto (`▾ web`), `A`, `P` y `R` actúan sobre el proyecto entero y `c` abre su comprobación propia. `S` abre una shell en el contenedor elegido. CPU y memoria se piden aparte y solo con esta sección a la vista (`—` en lo que no corre). Si no se saben los ficheros de un proyecto, su cabecera dice «sin ficheros de compose conocidos: solo acciones por contenedor».

### 6.3 Panel de logs

```
┌ MAGI · SERVICIOS · LOGS ─────────────────────────────────── app.service · web-01 · ● siguiendo ──┐
│ / sin filtro          nivel: aviso o más                          1 214 líneas · 14 omitidas     │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 21:38:47  !  WARNING  Lentitud: GET /api/informes tardó 2,4 s                                    │
│ 21:40:02  ✕  ERROR    Exception in ASGI application                                              │
│ 21:40:02  ✕  Traceback (most recent call last):                                                  │
│ 21:40:02  ✕    File "/srv/app/pedidos.py", line 88, in crear                                     │
│ 21:40:02  ✕  sqlite3.OperationalError: database is locked                                        │
│ 21:41:15  !  WARNING  Reintento 1/3 de la cola de correo                                         │
│ 21:43:30  !  WARNING  Lentitud: POST /api/pedidos tardó 3,1 s                                    │
│           … 14 líneas omitidas (el cliente iba lento)                                            │
│ 21:44:02  ✕  ERROR    Plazo agotado al llamar a https://api.ejemplo.org/pagos                    │
│ 21:44:09  !  WARNING  Reintento 2/3 de la cola de correo                                         │
│                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ / texto  n nivel  espacio pausa  fin al final  ↑↓ desplazar  t pestaña  esc cerrar               │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

`✕` marca error o peor (prioridades 0-3), `!` aviso (4) y `·` el resto; en contenedores, `!` marca stderr. Al desplazarse hacia arriba el panel se pausa solo y la cabecera pasa a «⏸ en pausa · 37 nuevas»; `Fin` vuelve al final y sigue. Las marcas «… N líneas omitidas» están donde faltan líneas.

### 6.4 Toda la flota

```
┌ MAGI · SERVICIOS ──┬────────────────── toda la flota · 4 fallos · sondeos de hace 40 s a 3 min ──┐
│ HOSTS              │ FALLOS DE LA FLOTA (del último sondeo de cada host)                         │
│ ────────────────── │ ─────────────────────────────────────────────────────────────────────────── │
│ ▸ Toda la flota ⚠4 │   HOST       TIPO         NOMBRE              ESTADO           SONDEO       │
│   web-01     ● ⚠3  │ ▸ web-01     servicio     copia.service       ✕ fallido        hace 40 s    │
│   web-02     ●     │   web-01     contenedor   web-worker-1        ✕ salió (1)      hace 40 s    │
│   db-01      ◐ ⚠1  │   web-01     contenedor   panel               ⚠ unhealthy      hace 40 s    │
│   bastion    ●     │   db-01      servicio     exportar.service    ✕ fallido        hace 1 min   │
│   nas        ✕     │   ──────────────────────────────────────────────────────────────────────    │
│                    │   nas: CAÍDA en el último sondeo (hace 3 min) · sin datos de servicios      │
│                    │   web-02 y bastion: sin fallos                                              │
├────────────────────┴─────────────────────────────────────────────────────────────────────────────┤
│ ↵ ir al host  l logs  A arrancar  R reiniciar  r sondear todos  esc volver                       │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Salen los fallos del último sondeo de cada host, sin abrir conexiones; `r` sondea todos como `R` en Flota. Un host caído o sin sondeo tiene su propia línea. En Flota, la fila del host añade `⚠N` y su detalle, un bloque FALLOS con los nombres; `u` abre esta vista en ese host.

### 6.5 Deliberación y resultado

```
        ┌─ DELIBERACIÓN MAGI ──────────────────────────────────────────────────┐
        │  ACCIÓN: reiniciar app.service → web-01                              │
        │  Antes de reiniciar: comprobación propia (/srv/app/bin/comprobar)    │
        │  ──────────────────────────────────────────────────────────────────  │
        │                 MELCHIOR-1    BALTHASAR-2    CASPER-3                │
        │                 salud         backup<24 h    tests                   │
        │  web-01         ✓ 0,2 s       ✓ hace 3 h     —                       │
        │                 (sin contar app.service)                             │
        │  ──────────────────────────────────────────────────────────────────  │
        │  CONSENSO  2 / 2     ████████████████████                            │
        │  ▸ APROBADO — ctrl+k reiniciar · esc cancelar                        │
        └──────────────────────────────────────────────────────────────────────┘
```

```
        ┌─ RESULTADO · reiniciar app.service · web-01 ─────────────────────────┐
        │  ✕ FALLIDO — no volvió a active (failed tras 4,1 s)                  │
        │  ──────────────────────────────────────────────────────────────────  │
        │  Últimas líneas del diario:                                          │
        │  21:52:10 app[4121]: Traceback (most recent call last):              │
        │  21:52:10 app[4121]:   File "/srv/app/main.py", line 12, in <module> │
        │  21:52:10 app[4121]: KeyError: 'DATABASE_URL'                        │
        │  21:52:10 systemd[1]: app.service: Main process exited, status=1     │
        │  21:52:10 systemd[1]: app.service: Failed with result 'exit-code'.   │
        │                                                                      │
        │  l logs        ↵ detalle        esc cerrar                           │
        └──────────────────────────────────────────────────────────────────────┘
```

La deliberación es la de la F6 con un solo host y la acción en la cabecera; MELCHIOR-1 dice «sin contar app.service». Con una unidad crítica sale además, en rojo, «⚑ puede cortar tu conexión con web-01 y la de los hosts que van a través de él». Si la comprobación de configuración falla, el resultado dice «✕ RECHAZADA POR LA CONFIGURACIÓN — no se ha reiniciado» y enseña la salida del validador, con `c` para editar la comprobación propia. «Sin confirmar» sale en ámbar con «la orden sigue en el host: r para ver el estado».

La **confirmación** es una línea con la acción y lo que se comprobará antes: «¿Recargar nginx.service en web-01? Antes: nginx -t · ↵ recargar · esc cancelar». La **contraseña de sudo** pide la de `usuario@web-01` en un campo enmascarado, dice «se recuerda 5 min sin uso; nunca se guarda» y da 3 intentos.

### Teclas de la vista

| Tecla | Acción |
|---|---|
| `F9` · `u` | Entrar en la vista (`u`, con el host elegido en Flota, Hosts o la ficha, fuera de un campo) |
| `Tab` | Cambiar entre hosts y lista (en el detalle: Estado, Dependencias, Unidad) |
| `1` `2` `3` | Servicios · Timers · Contenedores |
| `↑` `↓` `RePág` `AvPág` `Inicio` `Fin` | Moverse |
| `/` | Filtrar por nombre en la sección visible |
| `↵` | Detalle (en «Toda la flota»: ir al host) |
| `l` | Panel de logs |
| `A` · `P` · `R` · `G` · `H` | Arrancar (en un timer: lanzar ahora) · parar · reiniciar · recargar · activar o desactivar el arranque |
| `S` | Shell en el contenedor |
| `c` | Comprobación propia del servicio o del proyecto |
| `!` | Contraseña de sudo para este host |
| `r` | Refrescar (en «Toda la flota»: sondear todos) |
| `Esc` | Cerrar el detalle, el panel o el diálogo |

En el panel de logs: `/` texto, `n` nivel (todo, info, aviso, error; en contenedores, todo o solo stderr), `Espacio` pausa, `Fin` al final, `↑` `↓` desplazar, `t` pestaña y `Esc` cerrar.

### Notas de UX

- **Glifos y texto (T5):** `●` activo o en marcha, `○` inactivo o parado, `✕` fallido o caído, `◐` cambiando, `⚠` unhealthy y fallos, `★` vigilado y `⚑` crítica; en modo ASCII, los de T5 más `+` para `★`, `!` para `⚠` y `^` para `⚑`. El estado siempre va también en texto.
- **Ninguna acción con una pulsación (T6):** todas piden confirmación o deliberación, y las teclas que cambian algo en el host van en mayúsculas (D250).
- **La barra dice solo lo que vale:** una acción que no se puede hacer en ese estado no sale en la barra, y su tecla explica por qué («nginx.service no admite recargar»).
- **Errores con instrucción:** «sin permiso para Docker: añade usuario al grupo docker o pulsa ! para usar sudo», «systemd demasiado antiguo (219): solo se ven los contenedores», «nginx -t falló: la salida está abajo».
- **Acciones de otras ventanas:** `AccionesServicio{lista}` pone «◐ reiniciando» en la fila del objetivo y en la barra de Sesión, la empezara quien la empezara.
- **Tamaños (T45):** la vista, el detalle, el panel de logs y los diálogos tienen modo estrecho, mínimo declarado e instantáneas a 40×12, 80×24 y 200×60. El aviso de tamaño mínimo deja activas `F1`-`F9`.
- **Ejemplos:** los mockups usan nombres y dominios de ejemplo (`ejemplo.org`), como el resto del proyecto.

---

## 7. Lógica de Negocio

### 7.1 Qué se lista y en qué orden

```python
def unidades_de(salida):
    cargadas = {u.id: u for u in salida.bloques_show("service")}      # list-units --all + show
    for nombre, arranque in salida.unit_files("service"):             # instaladas sin cargar
        if nombre not in cargadas:
            cargadas[nombre] = Unidad(nombre, activo="inactive", sub="dead", arranque=arranque)
    return [u for u in cargadas.values() if not u.id.endswith("@.service")]   # plantillas no

def ordenar(unidades, vigiladas):
    arriba = [u for u in unidades if u.activo == "failed" or u.id in vigiladas]
    resto = sorted((u for u in unidades if u not in arriba), key=lambda u: u.id)
    arriba.sort(key=lambda u: (u.activo != "failed", u.id))           # fallidas primero
    return arriba, resto                                              # la vista pinta una línea entre las dos
```

Los timers se listan igual (cargados e instalados) con su próxima y su última ejecución. Los contenedores salen de `ps -a` (todos, también los parados) y se agrupan por la etiqueta `com.docker.compose.project`; los ficheros de un proyecto, de `com.docker.compose.project.config_files`.

### 7.2 Acciones permitidas y deliberación

| Objetivo | Acción | Se ofrece si | Pide | Comprueba la configuración |
|---|---|---|---|---|
| Servicio | Arrancar | `inactive` o `failed`, sin `masked` | Confirmación | No |
| Servicio | Parar | `active`, `activating` o `reloading` | Deliberación | No |
| Servicio | Reiniciar | `active`, `inactive` o `failed`, sin `masked` | Deliberación | Sí |
| Servicio | Recargar | `active` y `CanReload=yes` | Confirmación | Sí |
| Servicio | Activar el arranque | `UnitFileState=disabled` | Confirmación | No |
| Servicio | Desactivar el arranque | `UnitFileState=enabled` | Deliberación | No |
| Timer | Lanzar ahora | Su servicio no está activo | Confirmación | No |
| Contenedor | Arrancar | `created` o `exited` | Confirmación | No |
| Contenedor | Parar | `running` o `restarting` | Deliberación | No |
| Contenedor | Reiniciar | `running`, `restarting` o `exited` | Deliberación | No |
| Contenedor | Shell | `running` | Nada (abre una pestaña) | No |
| Proyecto | Arrancar | Algún contenedor parado | Confirmación | No |
| Proyecto | Parar | Algún contenedor en marcha | Deliberación | No |
| Proyecto | Reiniciar | Siempre | Deliberación | Sí (`config -q`) |

```python
DEJA_PARADO = {("unidad", "parar"), ("unidad", "reiniciar"), ("unidad", "desactivar"),
               ("contenedor", "parar"), ("contenedor", "reiniciar"),
               ("proyecto", "parar"), ("proyecto", "reiniciar")}

def exige_deliberacion(objetivo, accion):            # lo usa el cliente y lo recomprueba el servidor (T38)
    return (objetivo.tipo, accion) in DEJA_PARADO

def melchior_para_servicio(host, objetivo, ahora):   # MELCHIOR-1 dentro de PlanDeliberado::Servicio
    s = ultimo_sondeo(host)
    if s is None or ahora - s.fecha > 5 * 60:
        s = sondear(host)                            # como en la F6, dentro de su plazo de 2 s
    vigilados = {u: e for u, e in s.servicios.items() if u != objetivo.nombre}
    return estado_flota(s.metricas, vigilados) == "NOMINAL"   # estados de la F2, sin contar el objetivo
```

Reiniciar un servicio vigilado que ha fallado es justo lo que se quiere hacer, así que su propio estado no bloquea la salud del host; lo demás cuenta como en la F6. La deliberación se guarda con `PlanDeliberado::Servicio{host, objetivo, accion}`: una deliberación autoriza una sola acción (T62).

### 7.3 Privilegios y sudo

```python
def privilegio(uid, sudo_n):                   # suborden «privilegio», con LC_ALL=C
    if uid == 0:
        return "root"
    if sudo_n.codigo == 0:                     # sudo -n true
        return "sudo_sin_clave"
    if "password is required" in sudo_n.stderr:
        return "sudo_con_clave"
    return "sin_sudo"                          # sin sudo, o el usuario no está en sudoers

def hay_que_pedirla(accion, listado, memoria):  # en el cliente, antes de confirmar o deliberar
    return (accion.privilegiada and listado.privilegio == "sudo_con_clave"
            and memoria.contrasena(listado.host_id) is None)
```

En el host, cada orden privilegiada va con su propio sudo, nunca `sudo sh`:

- sin contraseña: `timeout 30 sudo -n -- systemctl restart app.service`;
- con contraseña: `printf '%s\n' "$CLAVE" | timeout 30 sudo -S -p '' -- systemctl restart app.service`.

`printf` es interno del shell, así que la contraseña no sale en la lista de procesos. El `timeout` va por fuera de sudo para que sudo solo vea la orden real: así basta un sudoers que permita `systemctl`, `journalctl`, `docker` y los validadores, y el registro del host enseña lo que se hizo. Van con privilegios las órdenes que cambian algo, los validadores y la comprobación propia; `journalctl` y `docker`, solo si sin ellos no hay permiso. Con Podman nunca se usa sudo: sus contenedores sin root son los del usuario SSH. La contraseña llega al servidor dentro de cada petición, se escribe en la primera línea de la entrada estándar del script y no se guarda en ningún sitio (T25).

### 7.4 Ejecutar una acción y esperar el resultado

```python
def ejecutar(accion, espera=config.espera_seg):      # lo hace servicios.sh; en Python para leerlo mejor
    r = con_privilegios(espera, orden_de(accion))    # systemctl, docker o docker compose: bloquean
    if r.codigo == 124:
        return "sin_confirmar"                       # timeout: el trabajo sigue en el host
    if r.codigo != 0:
        return "fallido"                             # con su mensaje y las últimas 20 líneas
    dormir(2)                                        # pilla lo que muere nada más arrancar
    p = estado(accion.objetivo)
    while accion.objetivo.tipo == "contenedor" and p.salud == "starting" and quedan(espera):
        dormir(1)
        p = estado(accion.objetivo)
    return "hecho" if exito(accion, p) else "fallido"

def exito(accion, p):
    o, a = accion.objetivo, accion.accion
    if o.tipo == "unidad":
        if a in ("arrancar", "reiniciar"):
            return p.ActiveState == "active" or (
                p.Type == "oneshot" and p.ActiveState == "inactive" and p.Result == "success")
        if a == "parar":
            return p.ActiveState == "inactive"
        if a == "recargar":
            return p.ActiveState == "active"
        return p.UnitFileState == ("enabled" if a == "activar" else "disabled")
    if o.tipo == "contenedor":
        return p.estado == "exited" if a == "parar" else p.estado == "running" and p.salud != "unhealthy"
    en_marcha = [c.estado == "running" for c in p.contenedores]          # proyecto
    return not any(en_marcha) if a == "parar" else all(en_marcha)
```

Las últimas líneas salen de `journalctl -u <unidad> -n 20 --no-pager -o short-iso` o de `docker logs --tail 20`. El servidor da a toda la acción `espera_seg` + 30 s; si pasa, contesta `error` («sin respuesta de web-01») y suelta la conexión.

### 7.5 Comprobaciones de configuración

| Objetivo | Se reconoce por | Orden | Notas |
|---|---|---|---|
| Nginx | `nginx.service` | `nginx -t` | Con privilegios |
| Caddy | `caddy.service` | `caddy validate --config <c> --adapter <a>` | `<c>` y `<a>` salen del `ExecStart` de la unidad; si no están, `/etc/caddy/Caddyfile` y `caddyfile` |
| Apache | `apache2.service` o `httpd.service` | `apachectl configtest` | En Debian, `apache2ctl` si no hay `apachectl` |
| HAProxy | `haproxy.service` | `haproxy -c -f <f>` | `<f>` del `-f` de su `ExecStart`; si no, `/etc/haproxy/haproxy.cfg` |
| Proyecto Compose | el proyecto | `docker compose -p <p> -f <ficheros> config -q` (o `podman compose`) | Ficheros de las etiquetas de sus contenedores |
| Propia | COMPROBACIONES_SERVICIO | La orden guardada, con `sh -c` | Sustituye a la conocida |

Solo se comprueba antes de **recargar** y de **reiniciar** (arrancar algo con la configuración rota no rompe nada que funcione). El plazo es de 20 s y la salida se recorta a 16 KiB.

### 7.6 Logs: lotes, contrapresión y anillo

```python
class Suscripcion:                                   # en el servidor, una por SeguirLog
    COLA_MAX = 4 * 1024 * 1024

    def linea(self, texto, prioridad, flujo):
        texto = recortar(texto, 16 * 1024)
        self.cola.append(Linea(hora_del_host(), prioridad, flujo, texto))
        self.cola_bytes += len(texto)
        while self.cola_bytes > self.COLA_MAX:       # el cliente no da abasto: fuera las más viejas
            vieja = self.cola.popleft()
            self.cola_bytes -= len(vieja.texto)
            self.omitidas += 1

    def cada_100_ms(self):                           # solo si el socket del solicitante admite más
        if self.cola and self.solicitante.puede_escribir():
            lote = [self.cola.popleft() for _ in range(min(500, len(self.cola)))]
            self.cola_bytes -= sum(len(l.texto) for l in lote)
            self.solicitante.enviar(LineasLog(self.id, lote, self.omitidas))
            self.omitidas = 0

anillo = deque(maxlen=config.lineas_log)             # en el cliente: 10 000 líneas

NIVELES = {"todo": 7, "info": 6, "aviso": 4, "error": 3}   # prioridades de journald (0 = emerg)

def visible(l, texto, nivel):
    return l.prioridad <= NIVELES[nivel] and texto.casefold() in l.texto.casefold()
```

`MESSAGE` de `journalctl -o json` puede llegar como lista de bytes: se decodifica como UTF-8 con sustitución. Muchas aplicaciones escriben todo por stderr y journald lo guarda como `info` (6); para ellas el filtro útil es el de texto. En los contenedores no hay prioridad: `n` alterna entre todo y solo stderr.

### 7.7 `servicios.sh`: arranque del script

```sh
# servicios.sh — MAGI lo ejecuta con:
#   sh -c "$SCRIPT" magi-servicios <suborden> <privilegio> <argumentos…>
# 1.ª línea de stdin: contraseña de sudo (vacía si no hay). En «logs», el fin de stdin
# dice que MAGI ha cerrado el canal.
set -u
LC_ALL=C; export LC_ALL
orden=$1 priv=$2; shift 2
IFS= read -r CLAVE || CLAVE=
case $priv in root|sudo_sin_clave|sudo_con_clave|sin_sudo) ;; *) echo "ERROR privilegio" >&2; exit 64 ;; esac
es_unidad() {
  case $1 in ''|*[!A-Za-z0-9@._-]*) return 1 ;; *.service|*.timer) return 0 ;; *) return 1 ;; esac
}
con_priv() {                    # con_priv <plazo> <orden…>: sudo orden a orden, nunca «sudo sh»
  plazo=$1; shift
  case $priv in
    root)           timeout "$plazo" "$@" ;;
    sudo_sin_clave) timeout "$plazo" sudo -n -- "$@" ;;
    sudo_con_clave) printf '%s\n' "$CLAVE" | timeout "$plazo" sudo -S -p '' -- "$@" ;;
    *)              echo "ERROR sin_privilegio" >&2; return 77 ;;
  esac
}
case $orden in
  logs)
    tipo=$1 objetivo=$2 n=$3
    case $n in ''|*[!0-9]*) echo "ERROR lineas" >&2; exit 64 ;; esac
    [ "$tipo" = unidad ] && { es_unidad "$objetivo" || { echo "ERROR objetivo" >&2; exit 64; }; }
    journalctl -u "$objetivo" -f -n "$n" -o json --no-pager </dev/null &   # o docker|podman logs -f
    pid=$!
    cat >/dev/null              # espera a que MAGI cierre el canal (o a que caiga SSH)
    kill -TERM "$pid" 2>/dev/null
    ;;
  # privilegio, sudo, listar, consumo, detalle y accion: §5.1, §7.3 y §7.4
esac
```

Los argumentos se validan en Rust antes de mandarlos y otra vez en el script (T30 y la regla de los scripts embebidos). Los errores del script (`ERROR objetivo`, `ERROR sin_privilegio`…) se traducen a los mensajes de §6.

### 7.8 Sondeo ampliado

```sh
# Nuevo en src/flota/sondeo.sh (F2). MAGI lo ejecuta con «sh -c '<script>' magi <motores> <unidades…>».
motores=$1; shift                         # «docker,podman», «docker», «podman» o «-»
# … métricas y MAGI_SVC de las unidades vigiladas, igual que en la F2 …
if command -v systemctl >/dev/null 2>&1; then
  systemctl list-units --type=service --state=failed --plain --no-legend --no-pager 2>/dev/null \
    | awk '{ n++; if (n <= 20) print "MAGI_FALLO_UNIDAD=" $1 } END { print "MAGI_FALLOS_UNIDADES=" n+0 }'
fi
for m in $(printf '%s' "$motores" | tr ',' ' '); do
  command -v "$m" >/dev/null 2>&1 || continue
  if salida=$("$m" ps -a --format '{{.Names}}\t{{.State}}\t{{.Status}}' 2>/dev/null); then
    echo "MAGI_MOTOR=$m"
    printf '%s\n' "$salida" | awk -F'\t' '
      $2 == "restarting" || $2 == "dead" || ($2 == "exited" && $3 !~ /^Exited \(0\)/) || $3 ~ /unhealthy/ {
        n++; if (n <= 20) print "MAGI_FALLO_CONTENEDOR=" $1 }
      END { print "MAGI_FALLOS_CONTENEDORES=" n+0 }'
  else
    echo "MAGI_MOTOR=$m sin_permiso"
  fi
  break
done
```

Sin sudo (T15): un usuario sin permiso para Docker da `"contenedores": null`, no un fallo. El estado del host sigue saliendo de las métricas y de los vigilados (F2); `fallos_json` solo alimenta las marcas.

### 7.9 Unidades críticas

`[servicios] criticas` trae por defecto `sshd.service`, `ssh.service`, `networking.service`, `NetworkManager.service`, `systemd-networkd.service`, `systemd-resolved.service`, `firewalld.service`, `ufw.service`, `nftables.service`, `iptables.service`, `tailscaled.service`, `wg-quick@*.service`, `docker.service`, `containerd.service` y `podman.service` (admite patrones con `*`). Parar, reiniciar, recargar o desactivar una de ellas añade a la confirmación o a la deliberación, en rojo, «⚑ puede cortar tu conexión con web-01» y, si los hay, los hosts que van a través de él (`salto_host_id`). No cambia lo que pide cada acción: solo avisa.

### 7.10 Sincronización: `formato` 3

```python
FORMATO_MAXIMO = 3

def formato_a_escribir(inventario):
    if inventario.comprobaciones_servicio:
        return 3                                     # MAGI 1.2 lo verá como formato_nuevo (T74)
    return 2 if any(h.tipo != "ssh" for h in inventario.hosts) else 1

def leer_comprobacion(e):                            # elemento «comprobacion_servicio»
    validar_objetivo(e.datos["objetivo_tipo"], e.datos["objetivo"])   # §7.11; si no vale: conflicto
    validar_orden(e.datos["comando"])
    host = host_por_uuid(e.datos["host"])
    if host is None or host.tipo != "ssh":
        return conflicto(e, "huérfana")              # recuperable, como los demás (F10)
    return e
```

Viajan `host` (uuid), `objetivo_tipo`, `objetivo` y `comando`, con su `uuid`, `version` y `vector` (T70); borrar deja lápida. `SONDEOS.fallos_json` no viaja (D133).

### 7.11 Validaciones

- **Unidades:** `[A-Za-z0-9@._-]+`, hasta 256 caracteres, acabadas en `.service` o `.timer` (como el sondeo de la F2). Las que tienen otros caracteres (`\x2d`) se ven, pero sin acciones: «nombre no admitido para acciones».
- **Contenedores:** id hexadecimal de 12 a 64 caracteres o nombre `[A-Za-z0-9][A-Za-z0-9_.-]*` de hasta 128. **Proyectos:** `[a-z0-9][a-z0-9_-]*` de hasta 64. **Motor:** `docker` o `podman`.
- **Rutas sacadas de `ExecStart`** (Caddy, HAProxy): absolutas y con `[A-Za-z0-9._/-]`; si no, la de siempre.
- **Comprobación propia:** una línea de 1 a 1 024 caracteres, sin NUL ni saltos de línea. Si empieza por `sudo`, aviso: «no hace falta: se ejecuta con los privilegios de la acción».
- **`[servicios]`:** los rangos de §5.3; `contenedores` solo con `docker` y `podman`, sin repetir; `criticas` con nombres válidos o patrones con `*`.
- **Entrada no fiable (T38):** los límites de §8, comprobados en el servidor al leer del host y en el cliente al recibir `Servicios`, `Detalle` y `LineasLog`.
- **Estado:** el cliente solo ofrece lo permitido (§7.2) y el servidor no vuelve a mirar el estado; si ya no vale (otro lo paró antes), `systemctl` lo dice y sale en el resultado.

### 7.12 Casos especiales

- **Host sin systemd o con systemd anterior a 230:** solo contenedores, con «sin systemd» o «systemd demasiado antiguo (219)».
- **Unidad enmascarada:** se ve con «enmascarada» y sin acciones. **Plantillas** (`app@.service`): no se listan; sus instancias (`app@1.service`), sí.
- **`oneshot`:** cuenta como hecho si termina con éxito, aunque quede `inactive`.
- **Reiniciar sshd:** la sesión de MAGI sigue (sshd no corta las conexiones abiertas); parar sshd, la red o el cortafuegos avisa en rojo (§7.9).
- **Reiniciar `docker.service`:** los contenedores se paran y vuelven según su política de reinicio; el resultado lo dice.
- **Cerrar la ventana con una acción en curso:** el servidor la termina y la anota; otra ventana la ve en `AccionesServicio{lista}` y su resultado queda en REGISTRO.
- **Dos ventanas:** las dos ven las acciones en curso; una segunda acción sobre el mismo objetivo se rechaza con «hay otra acción en curso».
- **Se corta la conexión durante una acción:** resultado `error`, «se cortó la conexión: puede que el reinicio se haya hecho; r para ver el estado».
- **Podman sin root:** se ven los contenedores del usuario SSH; los de root, solo si el usuario SSH es root.
- **Docker y Podman a la vez:** se usa el primero de `[servicios] contenedores`, y la cabecera dice cuál.
- **Proyecto sin ficheros conocidos:** sin acciones de proyecto, con el motivo.
- **Contenedor con TTY:** sus logs llegan todos por stdout.
- **Más de 2 000 unidades o 500 contenedores:** lista incompleta con aviso.
- **«Toda la flota» con un host sin sondeo:** «sin sondeo: r para sondear».
- **Suspender el portátil con el panel de logs abierto:** `FinLog` con «se cortó la conexión»; al volver, `l` lo abre otra vez.
- **Reloj del host distinto del local:** «desde» y «hace» se calculan con el reloj del host (`MAGI_RELOJ`).
- **La contraseña de sudo cambia en el host:** la siguiente acción falla con «sudo rechazó la contraseña»; MAGI la olvida y la pide otra vez.

### 7.13 Limitaciones heredadas

- R83-R85 de la F10 (sin la corrección 10b) siguen igual; el `formato` 3 no las cambia.
- Las F2-F6, F8, F10, F11 y F12 siguen sin validación manual completa (R15).
- Con H.264, al volver de la pestaña oculta no se pide la imagen entera (R117).
- `ironrdp-connector` e `ironrdp-acceptor` vendorizados: hay que revisarlos al subir russh o IronRDP (R118).

---

## 8. Requisitos No Funcionales

**Seguridad**

- **Órdenes remotas:** todo va con `servicios.sh` y argumentos validados en Rust y en el script; nada del inventario se mete en el texto de una orden. Las únicas órdenes libres son las comprobaciones propias, escritas por el usuario como sus snippets (F6).
- **sudo orden a orden:** nunca `sudo sh`. Vale con un sudoers limitado, y el registro del host (`journalctl _COMM=sudo` o `auth.log`) muestra la orden real.
- **Contraseña de sudo:** la pide el cliente y la comprueba con `ComprobarSudo`. Vive en memoria (`Zeroizing`) como mucho `sudo_memoria_min` sin uso, viaja por el socket solo dentro de la petición que la usa y llega al host por la entrada estándar, nunca en la línea de órdenes. Nunca va a disco, al llavero, al log ni a REGISTRO (T19, T25).
- **Deliberación y confirmación:** lo que deja algo parado delibera (T41) y nada se hace con una pulsación (T6); el servidor recomprueba qué exige deliberación y valida la deliberación (T62).
- **Comprobar antes de actuar,** en el mismo canal y justo antes, sin hueco entre las dos cosas (D243).
- **Entrada no fiable (T38):** 2 MiB por listado, 2 000 unidades, 500 contenedores, 100 proyectos, nombres de hasta 256 caracteres, texto de unidad o de `inspect` de hasta 256 KiB, líneas de log de hasta 16 KiB, lotes de hasta 500 líneas, cola de 4 MiB por suscripción y salida de comprobación de hasta 16 KiB. Un dato que no cuadra se descarta, nunca hay pánico.
- **Nada del host al log:** logs, unidades, `inspect` y salida de las órdenes nunca van al log de MAGI ni a REGISTRO (T44).
- **Unidades críticas** con aviso en rojo (§7.9).

**Protección de datos (RGPD)**

- Los logs de un servicio pueden llevar datos personales (direcciones IP, correos, usuarios). MAGI solo los enseña: viven en el anillo en memoria y se borran al cerrar el panel. Nunca se guardan.
- REGISTRO guarda solo metadatos (host, objetivo, acción, resultado y duración), con la retención de 90 días de la F2.
- La contraseña de sudo no se guarda en ningún sitio.

**Backups y recuperación**

- La migración 9 añade una tabla vacía y una columna nula. Una `magi.db` en la versión 9 no la abre un binario 1.2 (T74).
- Las acciones en curso no se guardan: si el servidor se para en medio de una, la orden sigue en el host pero REGISTRO no la anota (se ve al refrescar). `magi servidor parar` con acciones en curso pregunta antes (T67).

**Rendimiento**

- Listado de un host con 300 unidades y 30 contenedores en menos de 1,5 s en la red local: una sola ejecución del script por refresco.
- Con la vista cerrada, ningún coste: no hay refrescos ni logs en marcha.
- `docker stats --no-stream` tarda 1-2 s: va aparte y solo con la sección Contenedores a la vista.
- Logs: hasta 2 000 líneas por segundo sin frenar la interfaz; por encima, se omiten con recuento.
- Pintar 2 000 unidades con filtro en menos de 16 ms (solo se pinta lo visible).

**Accesibilidad**

- Glifo y texto en todos los estados (T5), modo ASCII y nada que dependa solo del color; los niveles del log llevan glifo además de color.
- Todo se maneja con el teclado y las teclas salen en la barra de cada vista.

---

## 9. Integraciones

| Herramienta | Versión | Qué usa MAGI | Notas |
|---|---|---|---|
| systemd | 230 o posterior | `systemctl show`, `list-units`, `list-unit-files`, `list-dependencies`, `cat`, `start`, `stop`, `restart`, `reload`, `enable` y `disable` | Debian 9+, Ubuntu 16.04+, RHEL y Rocky 8+, Arch |
| journald | el de systemd | `journalctl -u … -f -n … -o json --no-pager` | Ver el diario del sistema exige los grupos `systemd-journal`, `adm` o `wheel`, o sudo |
| Docker Engine | 20.10+ | `ps -a --format '{{json .}}'`, `inspect`, `logs -f --timestamps`, `start`, `stop`, `restart`, `exec -it`, `stats --no-stream` | El socket exige el grupo `docker` o sudo |
| Podman | 4+ | Lo mismo, con `--format json` | Sin root: los contenedores del usuario SSH |
| Compose | v2 (`docker compose`) o `podman compose` | `config -q`, `start`, `stop` y `restart` con `-p` y `-f` | Ficheros según las etiquetas de los contenedores |
| Nginx, Caddy, Apache, HAProxy | los de la distribución | `nginx -t`, `caddy validate`, `apachectl configtest`, `haproxy -c` | Con privilegios |
| sudo | 1.8+ | `-n`, `-S -p ''`, `-v` | Con `LC_ALL=C` para leer sus mensajes |
| coreutils | — | `timeout` | Plazo de cada orden en el host |

**Formatos:** `systemctl show` (clave=valor, estable entre versiones), JSON de Docker y Podman, y `journalctl -o json`.

**Licencias:** sin dependencias nuevas.

---

## 10. Decisiones Técnicas (ADR-lite)

- **D234 — Vista Servicios en `F9`, con `u` desde Flota, Hosts y la ficha** (Hector, 4a). `F1`-`F8` están ocupadas y `F10` la usa Dicta. En Hosts y Flota, `s` ya abre Archivos, así que se usa `u` (de unidades). Descartados: una subvista de Flota (Flota enseña estado, no acciones) y una pestaña de Sesión.
- **D235 — Alcance: servicios y timers de systemd del sistema, journalctl, y Docker y Podman con Compose** (Hector, 1a). Cubre cómo despliega 4d3: systemd detrás de Nginx y Compose en algunos hosts. Quedan fuera las unidades de usuario, los demás tipos de unidad y Kubernetes.
- **D236 — Acciones con espera al resultado** (Hector, 2a). La orden de `systemctl` bloquea hasta acabar su trabajo y va con `timeout`; a los 2 s se vuelve a mirar el estado para pillar lo que muere nada más arrancar. Un `oneshot` que termina bien cuenta como hecho. Sin editar unidades ni `daemon-reload`: una unidad se edita con Archivos (F8).
- **D237 — Contenedores sin borrar ni actualizar** (Hector, 3a): arrancar, parar, reiniciar, logs, shell y Compose por proyecto. Actualizar (`pull` y `up`) va con los despliegues (F16), que tendrán pasos, vuelta atrás y comprobaciones.
- **D238 — Logs en un panel propio y en una pestaña** (Hector, 5a). El panel filtra y se pausa sin cortar la llegada de líneas, y el anillo de 10 000 acota la memoria. `t` da el seguimiento completo, con colores, en una terminal de verdad.
- **D239 — Vigilados y fallidos arriba, fallidos primero** (Hector, 6a). La lista incluye los servicios instalados sin cargar, para poder arrancarlos; las plantillas no, porque sin instancia no se arrancan.
- **D240 — Los fallos salen del sondeo y no cambian el estado del host** (Hector, 7a). El sondeo cuenta unidades fallidas y contenedores caídos, sin sudo ni diálogos (T15). Solo los vigilados mueven el estado, como en la F2, para no cambiar lo que aprueba MELCHIOR-1. «Toda la flota» usa los últimos sondeos, así que entrar no abre conexiones. Sin avisos de escritorio (D15 de la F2).
- **D241 — sudo orden a orden, con la contraseña pedida por el cliente** (Hector, 8a). Se usa `sudo -n` si vale; si no, diálogo y `ComprobarSudo` antes de confirmar, y la contraseña llega al host por la entrada estándar. Nunca `sudo sh`: así basta un sudoers limitado y el registro del host muestra la orden real. Precisión de «sin guardarla»: se recuerda en la memoria del cliente hasta 5 min sin uso o hasta salir de la vista. Por SSH sin terminal, sudo no recuerda nada entre órdenes, y sin esa memoria habría que escribirla en cada acción.
- **D242 — Deliberación para lo que deja algo parado** (Hector, 9a), con `PlanDeliberado::Servicio` y las comprobaciones del host. MELCHIOR-1 no cuenta el estado del propio objetivo: reiniciar lo que falla es justo lo que se quiere hacer. Arrancar, recargar, activar el arranque y lanzar un timer, con confirmación.
- **D243 — La comprobación de configuración va dentro de la acción, no en la deliberación** (Hector, 10a). Va en el mismo canal, con los mismos privilegios y justo antes de ejecutar, así que no queda hueco entre comprobar y actuar. Hay comprobaciones conocidas (Nginx, Caddy, Apache, HAProxy, Compose) y una propia por servicio; si la comprobación falla o no se encuentra, no se ejecuta nada. Descartado: enseñarla en la deliberación, que tiene un plazo de 2 s y sería otra ejecución distinta de la real.
- **D244 — Refresco cada 10 s solo con la vista abierta** (Hector, 11a), sin solapes ni diálogos. El consumo de los contenedores se pide aparte (`docker stats` tarda 1-2 s) y solo con su sección a la vista.
- **D245 — La lógica va en `magi-nucleo` y la vista solo en el escritorio** (Hector, 12a). Órdenes, lectura, acciones permitidas, validadores y límites se comparten; la F14 añadirá la vista del móvil sin rehacerlos.
- **D246 — Todo lo remoto en el servidor, sobre el pool** (`UsoCanal::Servicios`, T42). Las acciones son trabajo custodiado (`TrabajoServidor.acciones_servicio`, `AccionesServicio{lista}`): si se cierra la ventana, terminan y se anotan. Los logs no son trabajo: se cortan con el panel o con el cliente.
- **D247 — Protocolo v8** con los mensajes de §5.2 y `peticion_id` de servicios desde 2^45, entre los de ejecuciones y los de escritorios.
- **D248 — Pestañas con orden.** `AbrirSesion` gana `orden`, un tipo cerrado que el servidor convierte en la orden real (shell del contenedor, seguimiento del diario o del contenedor). Se aprovecha la pestaña de la F3 (PTY, colores y sudo interactivo), y el cliente nunca manda texto para ejecutar. Descartado: un terminal propio dentro de la vista.
- **D249 — Las comprobaciones propias viajan como elemento propio y suben el `formato` a 3 solo si hay alguna.** Al ser un elemento con su propio `uuid`, un equipo con 1.2 que edite el host no las borra sin saberlo. Y, como pasó con el `formato` 2 (D220), el formato nuevo evita que un equipo viejo lo lea a medias. Precisa D132 de la F10 (las tablas hijas sin identidad propia viajan dentro del padre).
- **D250 — Las acciones van en mayúsculas** (`A`, `P`, `R`, `G`, `H`, `S`) y `r` refresca, como en Flota. Una pulsación perdida no cambia nada en el host, y además todas piden confirmación o deliberación (T6).
- **D251 — Lectura robusta y acotada:** las propiedades salen de `systemctl show` (clave=valor, estable entre versiones) y no de las columnas de `list-units`, de donde solo se toma el nombre; JSON en Docker, Podman y `journalctl`, y los topes de §8. systemd 230 o posterior, como pide ya el sondeo de la F2.
- **D252 — Docker antes que Podman,** con autodetección y sin campo por host (`[servicios] contenedores`). Con Podman nunca hay sudo: sus contenedores sin root son los del usuario SSH.
- **D253 — Unidades críticas configurables con aviso en rojo:** sshd, la red, el cortafuegos, Tailscale y Docker pueden cortar la conexión con el host y con los hosts que van a través de él. Solo avisan; no cambian lo que pide cada acción.
- **D254 — Versión 1.3.0** (MINOR, T68): funcionalidad nueva, migración 9, protocolo v8 y `formato` 3. Sin órdenes nuevas de CLI.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Núcleo, listado y vista | `magi-nucleo/src/servicios` (tipos, nombres, lectura, límites) y `servicios.sh` (`privilegio`, `listar`, `consumo`, `detalle`) con salidas grabadas de varias distribuciones; protocolo v8 y contrato compilable antes de las pistas (T63); refrescos en el servidor sobre el pool; vista `F9` con hosts, secciones, orden, filtro y detalle; `u` desde Flota, Hosts y la ficha; migración 9, sondeo ampliado, marcas en Flota y «Toda la flota»; modo estrecho e instantáneas | 1,5 semanas |
| S2 — Acciones | Acciones permitidas, confirmación y deliberación (`PlanDeliberado::Servicio`, MELCHIOR-1 sin el objetivo); privilegios, `ComprobarSudo` y la contraseña en memoria; `accion` con espera y segunda mirada; comprobaciones conocidas y propias (ficha, `c`, sincronización con `formato` 3 y la app Android); unidades críticas; `AccionesServicio{lista}`, `TrabajoServidor` y REGISTRO | 1 semana |
| S3 — Logs, pestañas y cierre | Logs en vivo (lotes, contrapresión, anillo, filtros, pausa); pestañas con orden (shell y seguimiento); README, CHANGELOG y 1.3.0; prueba de extremo a extremo; revisión adversarial (T31, T54) | 1 semana |

**Total: 3 sprints, ~3,5 semanas** (95 funcionalidades).

**Supuestos**

- Las pruebas no usan hosts reales: el servidor SSH de prueba con `systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos en el `PATH`, como el `voxl` falso de la F12, y salidas grabadas de Debian 12, Ubuntu 24.04, Rocky 9 y Arch, con Docker 27 y Podman 5.
- Los hosts tienen `sh` POSIX, `timeout` (coreutils) y, para las acciones, sudo o el usuario root.
- El agente no necesita Docker ni systemd reales; los valida Hector con sus hosts (guion de abajo).

**Guion de validación manual**

1. Un host con Nginx y una aplicación con systemd: `F9`, vigilados arriba, detalle con dependencias y unidad.
2. Romper la configuración de Nginx y recargar: MAGI no recarga y enseña la salida de `nginx -t`; arreglarla y recargar.
3. Reiniciar la aplicación con un usuario que necesita contraseña de sudo: diálogo, tres intentos, deliberación y memoria de 5 min.
4. Un servicio que falla al arrancar: resultado «fallido» con sus últimas líneas.
5. Logs en vivo de la aplicación: filtro de texto, nivel «aviso», pausa y `t` a una pestaña.
6. Docker con un proyecto Compose: reiniciar el proyecto (con `config -q`), shell en un contenedor y sus logs.
7. Un usuario sin el grupo `docker`: aviso «sin permiso» y `!` para dar la contraseña.
8. Podman sin root en otro host.
9. Provocar un fallo en dos hosts, sondear y verlos juntos en «Toda la flota»; `⚠N` en Flota.
10. Una comprobación propia en un servicio y sincronizar: otro equipo con 1.3 la recibe, uno con 1.2 marca «formato nuevo» y el móvil la lee.
11. Cerrar la ventana durante un reinicio lento, abrirla otra vez y ver el resultado en REGISTRO.
12. Reiniciar `ssh.service`: aviso en rojo y la sesión sigue.

---

## 12. Conexiones con Otras Fases

- **F2:** sondeo (bloque de fallos y `SONDEOS.fallos_json`), servicios vigilados (`HOSTS.servicios`) y REGISTRO con un tipo nuevo.
- **F3:** servidor de sesiones, pool, protocolo (v8), pestañas (ahora también con orden) y `TrabajoServidor`.
- **F6:** deliberación (`PlanDeliberado::Servicio`, VERIFICACIONES_HOST, MELCHIOR-1 sin el objetivo) y DELIBERACIONES.
- **F7:** modo estrecho e instantáneas de las vistas nuevas (T45).
- **F8:** las unidades se editan con Archivos; la deliberación generalizada se amplía.
- **F9:** versión 1.3.0, CHANGELOG, `assets/` (sin cambios de CLI) y `shellcheck` de los scripts.
- **F10:** elemento `comprobacion_servicio` y `formato` 3.
- **F11:** la lógica va en el núcleo compartido; la app lee el `formato` 3.
- **F12:** solo los hosts `ssh` tienen servicios (T100).
- **F14 (Android, segunda vuelta):** la vista de servicios en el móvil, con la lógica de `magi-nucleo`.
- **F16 (Despliegues):** reutiliza las acciones, la espera al resultado y los validadores, y añade actualizar contenedores.
- **F17 (Accesos y certificados):** reutiliza el manejo de sudo (T105).
