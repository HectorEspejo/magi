# MAGI - Fase 16: Sesiones Persistentes

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 16 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-15 con informe procesado. La F15 (95/96) tiene pendiente su prompt de reanudación, que comprueba y corrige que tmux apunte siempre al nombre exacto (R146): esta fase parte de la F15 cerrada y fusionada, y su script se reescribe manteniendo esa regla. El prompt de cierre de la F14 sigue sin informe.
>
> Estado (16 nov 2026): especificada; a la espera del informe de implementación.

---

## 1. Visión General

La Fase 16 hace que una sesión de terminal **sobreviva a MAGI y al equipo**. Hoy, sin tmux, una pestaña vive en el servidor de MAGI de tu equipo: aguanta que cierres la ventana, pero si el equipo se apaga, se duerme o pierde la red el tiempo suficiente, el host cierra el shell y lo que corría muere. La F15 añadió la sesión persistente con tmux, pero para todo un host, con nombres automáticos (`magi-portatil-3`) y visible solo desde la lista de ese host.

Esta fase la convierte en **sesiones persistentes con nombre**. Se marcan al abrirlas (o todas las de un host) y viven en un **tmux propio de MAGI** en el host. Sus **fichas viajan** con la sincronización a todos tus equipos, y una **lista común** enseña, sin engancharse, si siguen vivas, quién las tiene, cuándo hubo actividad y si el programa **pide atención**. Desde cualquier equipo de escritorio te enganchas (y se suelta el anterior), consultas el historial como en una pestaña normal y la reabres si el host se reinició.

El caso que la motiva: **agentes que siguen trabajando** (Claude Code) mientras cierras MAGI o cambias de equipo, y que quieres retomar después.

**Objetivos principales**

1. Marcar una sesión como persistente al abrirla (con nombre, carpeta, comando de arranque y comando de reanudación) o todas las de un host, con una marca que viaja.
2. Fichas que viajan con la sincronización y una lista común con el estado de cada sesión leído del host: viva, quién la tiene, actividad, atención, título y vista previa.
3. Engancharse desde cualquier equipo de escritorio, soltando al anterior o compartiendo, y reabrir las terminadas con su comando de reanudación.
4. Un tmux propio de MAGI en el host (`tmux -L magi`), ajustado para Claude Code y sin tocar el del usuario; el modo historial de MAGI lee el de tmux.
5. Avisos de atención en la barra de MAGI mientras esté abierto en algún equipo.
6. Lógica en `magi-nucleo`; protocolo v10, migración 11, `formato` 4 y versión 1.5.0. El móvil queda pendiente.

**Contexto.** Producto propio. Hector deja agentes corriendo en sus hosts y quiere poder cerrar MAGI, apagar el portátil o cambiar de equipo, y volver a la misma sesión más tarde. Con la F15 ya se puede, pero hay que activar tmux para todo el host, los nombres no dicen nada y, para encontrar una sesión, hay que saber en qué host buscar. Lo que corre en el propio portátil no se puede salvar: para que algo siga vivo sin tu equipo, tiene que correr en un host.

Respuestas de Hector (16 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | Marcar al abrir («nueva pestaña persistente», con nombre, carpeta y comando opcionales) y, además, marcar un host al crearlo o editarlo para que todas sus sesiones sean persistentes (a, con el añadido de Hector) |
| 2 | Viaja la ficha de cada sesión (host, nombre, carpeta, comando y nota); el estado y el contenido se leen del host; `formato` 4 solo si hay alguna (a) |
| 3 | La lista enseña, sin engancharse, si vive, quién la tiene, la última actividad, si pide atención, el título y, si se pide, una vista previa de 20 líneas; se refresca al abrirla, con `r` y cada 30 s a la vista (a) |
| 4 | Avisos en la barra mientras MAGI esté abierto en algún equipo y, además, en el móvil con la app cerrada (b) |
| 5 | Un tmux propio en el host (`tmux -L magi`) con la configuración de MAGI, sin tocar la del usuario; las sesiones de la F15 se siguen viendo y enganchando hasta que acaben (a) |
| 6 | El modo historial de MAGI le pide el historial a tmux y funciona como en una pestaña normal; la rueda la lleva tmux o el programa (a) |
| 7 | Engancharse suelta al anterior, que lo ve; con «compartir», a la vez, y el tamaño lo marca el último que escribe (a) |
| 8 | Al acabar el comando, un shell en su carpeta; la sesión muerta deja su ficha «terminada», que se reabre con su comando de reanudación; nunca se termina sola; la lista marca las de más de 7 días sin actividad (a) |
| 9 | El móvil, nada todavía: queda pendiente (c) |
| 10 | Bloque «Persistentes» en la vista Sesiones, aviso de atención en la barra y `magi sesion <nombre>` (a) |

Dos aclaraciones de Hector (16 nov 2026) precisan las respuestas:

- **Avisos en el móvil (4).** Quedan pendientes con el resto del móvil: entra la parte de escritorio de la respuesta 4, la del móvil no (D326).
- **Marca del host (1).** Viaja a tus otros equipos: `HOSTS.sesiones_persistentes` sustituye a `tmux_persistente` de la F15, que era propio de cada equipo (D327, precisa T125).

**Fuera de alcance**

- Todo lo de esta fase en el móvil: lista, engancharse y avisos con la app cerrada (Hector, 9c y aclaración). La app solo aprende a leer el `formato` 4.
- Instalar tmux en el host, y otros multiplexores (screen, zellij, shpool, dtach, abduco).
- Convertir en persistente una pestaña ya abierta: un proceso en marcha no se puede meter dentro de tmux.
- Sobrevivir a un reinicio del host (tmux-resurrect y parecidos): la sesión se reabre con su comando.
- Grabar lo que pasa en una sesión: va con la Caja negra (F19).
- Avisos del sistema operativo (notificaciones de escritorio) y vigilar con MAGI cerrado (D15 de la F2).
- Ventanas y divisiones de tmux gestionadas por MAGI: cada sesión tiene una ventana y los paneles siguen siendo los de MAGI (F15).
- Los avisos OSC 9 y OSC 777 desde dentro de tmux: tmux no los pasa, y la atención se lee de la campana (D335).

**Versión prevista:** escritorio **1.5.0** (MINOR: funcionalidad nueva, migración 11, protocolo v10 y `formato` 4 de sincronización solo cuando hay fichas o hosts marcados, T68). La app Android cambia lo justo para leer el `formato` 4 (línea en su CHANGELOG; la versión la decide el desarrollador al publicar).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Host remoto | tmux 3.0 o posterior y `sh` POSIX | Servidor propio `tmux -L magi -f /dev/null`; teclas extendidas desde 3.2, `csi-u` desde 3.5 y *passthrough* desde 3.3. `crates/magi-nucleo/scripts/terminal_tmux.sh`, reescrito y embebido con `include_str!` |
| Núcleo | `crates/magi-nucleo/src/sesiones_persistentes/` (nuevo), `terminal/tmux.rs` y `terminal/teclas.rs` (`modifyOtherKeys`) | Validadores, nombres automáticos, estado derivado, unión de fichas y lectura acotada de lo que devuelve el script |
| Datos | SQLite, migración 11 | SESIONES_PERSISTENTES y `HOSTS.sesiones_persistentes`; las dos viajan |
| Sincronización | `formato` 4 | Elemento `sesion_persistente` y campo nuevo del host |
| Servidor | `src/servidor/sesiones_persistentes.rs` (nuevo), `sesiones.rs` e `historial.rs` | Vigilante, pestañas persistentes, soltada, vista previa, renombrar; protocolo v10 |
| Cliente | `src/app/persistentes.rs` (nuevo), `src/ui/sesiones.rs`, `sesion.rs` y la ficha | Bloque Persistentes, diálogos, pestaña soltada, aviso de atención y bloque SESIONES |
| CLI | Subcomandos `sesion` y `sesiones`, junto a los de `tunel` | `assets/` regenerado (página man y autocompletado) |
| App Android | El núcleo nuevo | Lee el `formato` 4 y no enseña nada de esta fase |
| Dependencias | Ninguna nueva | |

### Estructura de carpetas (novedades)

```
crates/magi-nucleo/
├── src/sesiones_persistentes/
│   ├── mod.rs          tipos: FichaSesion, EstadoVisto, Origen
│   ├── validar.rs      nombre, carpeta, comando y nota; solo hosts ssh
│   ├── nombres.rs      nombre automático <etiqueta del equipo>-<n>
│   ├── estado.rs       estado derivado, atención y «antigua»; lectura acotada de «estado»
│   └── union.rs        dos fichas con el mismo host y nombre: se unen (sincronización)
├── src/terminal/tmux.rs      (F15) + servidor propio, su configuración y las subórdenes nuevas
├── src/terminal/teclas.rs    (F11, F15) + modifyOtherKeys
├── src/almacen/              sesiones_persistentes y hosts (columna nueva); migración 11
├── src/sincronizacion/elementos.rs   elemento sesion_persistente y formato 4
├── scripts/terminal_tmux.sh  comprobar · crear · enganchar · estado · vista · historial · renombrar · terminar
└── tests/                    sesiones_persistentes, tmux_script (ampliada), tmux_real, sincronizacion_sesiones
src/servidor/sesiones_persistentes.rs   vigilante, SesionesPersistentes{lista}, vista previa y renombrar
src/servidor/sesiones.rs      pestaña persistente: crear, enganchar, soltada y reconexión
src/servidor/historial.rs     historial de tmux en las pestañas persistentes
src/app/persistentes.rs       bloque Persistentes, diálogos y aviso de atención
src/ui/sesiones.rs · sesion.rs · ficha   lista, pestaña soltada, marca ◆ y bloque SESIONES
src/config.rs                 [persistentes] con sus rangos
tests/sesiones_persistentes_servidor.rs · sesiones_persistentes_ui.rs · sesiones_persistentes_e2e.rs
tests/redimensionado          instantáneas nuevas
tests/fijos/terminal/         tmux falso ampliado y tui_prueba con modifyOtherKeys
android/                      lee el formato 4 (fixture y línea en su CHANGELOG)
docs/fase16/magi-fase16-revision-adversarial.md
```

### Diagrama de arquitectura

```
 SOBREMESA (Omarchy o macOS)                                                                HOST SSH

 ┌──────────────────────────────────┐     ┌──────────────────────────────────────────┐      ┌───────────────────────┐
 │ magi (cliente)                   │     │ magi --servidor                          │      │ dev-01                │
 │ Sesiones: bloque Persistentes    │ v10 │ pestaña persistente: comprobar, crear y  │ SSH  │ tmux -L magi (MAGI)   │
 │   estado · atención · título     │◀───▶│   enganchar en el PTY (100 = soltada)    │◀────▶│   agente-api: claude  │
 │   vista previa · terminar        │     │ reconexión: nunca quita la sesión        │      │   refactor: shell     │
 │ pestañas persistentes ◆          │     │   a otro equipo                          │      │   sin tu ~/.tmux.conf │
 │   soltada: volver o compartir    │     │ vigilante: cada 60 s, sin diálogos,      │      │                       │
 │ modo historial (de tmux)         │     │   con alguna ventana conectada           │      │ tmux habitual (tuyo)  │
 │ aviso de atención en la barra    │     │ SesionesPersistentes{lista}              │      │   se ve; no se toca   │
 │ escribe las fichas (SQLite)      │     │ historial y vista previa de tmux         │      │                       │
 │ CLI: magi sesion · sesiones      │     │ lee SESIONES_PERSISTENTES y HOSTS        │      │ terminal_tmux.sh      │
 └──────────────────────────────────┘     └──────────────────────────────────────────┘      │   crear · enganchar   │
                                                                                            │   estado · vista      │
 carpeta de sincronización (formato 4):   ┌──────────────────────────────────────────┐      │   historial           │
 viajan las fichas y la marca del host;   │ PORTÁTIL: magi y su servidor             │ SSH  │   renombrar           │
 el estado y el contenido, nunca          │ las mismas fichas, por la sincronización │◀────▶│   terminar            │
 ────────────────────────────────────────▶│ engancharse suelta al anterior           │      │                       │
                                          └──────────────────────────────────────────┘      └───────────────────────┘

 magi-nucleo: sesiones_persistentes/ (validadores, nombres automáticos, estado y lectura de «estado»)
 y terminal/tmux.rs con scripts/terminal_tmux.sh (servidor propio, configuración y subórdenes).
 El móvil lee el formato 4 y no enseña las sesiones: queda pendiente (D326).
```

El **host es la fuente de verdad**: si el tmux de MAGI tiene la sesión, está viva. La ficha solo dice dónde mirar y cómo reabrirla. El servidor de MAGI de cada equipo engancha sus pestañas, vigila las fichas mientras haya ventanas conectadas y lee el historial y la vista previa del host; el cliente escribe las fichas en SQLite cuando el efecto se ha producido (la sesión ya existe en el host). Dos equipos que miran la misma sesión no hablan entre sí: cada uno pregunta al host.

---

## 3. Modelo de Datos

La migración 11 añade la tabla SESIONES_PERSISTENTES y la columna `HOSTS.sesiones_persistentes`. Las dos viajan con la sincronización; el estado de las sesiones no se guarda.

### Diagrama E-R (cambios de la fase)

```
 ┌──────────────────────────────────────┐           ┌────────────────────────────────────────────────┐
 │ HOSTS (F1)              migración 11 │ CASCADE   │ SESIONES_PERSISTENTES (viaja)    migración 11  │
 │ + sesiones_persistentes   0 = no     │─── 1:N ──<│ id · uuid · version · vector                   │
 │   viaja y sube la versión del host   │           │ host_id → HOSTS (solo hosts ssh)               │
 │ tmux_persistente (F15): obsoleto;    │           │ nombre: el de tmux, único por host             │
 │   su valor pasa a la columna nueva   │           │ carpeta · comando · comando_reanudar           │
 │ reconectar · portapapeles_remoto:    │           │ nota                                           │
 │   siguen propios del equipo          │           │ origen: manual | host | descubierta            │
 └──────────────────────────────────────┘           │ creada_por (equipo) · creada_en                │
            │ 1:N                                   │ actualizado_en                                 │
            ▼                                       │                                                │
 ┌──────────────────────────────────────┐           │ el estado no se guarda: se lee del host        │
 │ REGISTRO (F2), tipos nuevos          │           └────────────────────────────────────────────────┘
 │ sesion_persistente_creada            │
 │ sesion_persistente_reabierta         │           En la memoria del servidor (no se guarda):
 │ sesion_persistente_enganchada        │             estado visto por ficha: viva, clientes, equipo,
 │ sesion_persistente_borrada           │             actividad, atención y título; sesiones sin ficha
 │ (y tmux_terminada, de la F15)        │             por host; vistas previas, solo a quien las pide.
 └──────────────────────────────────────┘           En el host: tmux -L magi, con las sesiones.
 La sincronización pasa al formato 4 solo si hay alguna ficha o algún host marcado (T68).
```

### 3.1 SESIONES_PERSISTENTES (migración 11, viaja)

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| uuid · version · vector | TEXT | Sincronización (F10); siempre rellenos |
| host_id | INTEGER NOT NULL FK HOSTS ON DELETE CASCADE | Solo hosts `ssh` (T100) |
| nombre | TEXT NOT NULL | El de la sesión en el tmux de MAGI: `^[A-Za-z0-9_-]{1,64}$`. `UNIQUE(host_id, nombre)` |
| carpeta | TEXT NULL | Carpeta del host donde arranca: absoluta o `~/…`, hasta 1024 bytes, sin saltos de línea. Vacía: la carpeta personal |
| comando | TEXT NULL | Comando de arranque, texto libre como un snippet (hasta 4096 bytes). Vacío: el shell de acceso |
| comando_reanudar | TEXT NULL | Comando al reabrir una terminada (por ejemplo `claude --continue`). Vacío: `comando` |
| nota | TEXT NULL | Hasta 500 caracteres |
| origen | TEXT NOT NULL | `manual` (marcada al abrir), `host` (de un host marcado) o `descubierta` (añadida desde la lista); se valida en código |
| creada_por | TEXT NOT NULL | Nombre del equipo que la creó |
| creada_en · actualizado_en | TEXT NOT NULL | ISO 8601 |

El comando lo escribe el usuario y se ejecuta en el host, como un snippet: viaja como dato, llega al script como argumento y nunca va a REGISTRO.

### 3.2 HOSTS (migración 11)

| Campo | Tipo | Descripción |
|---|---|---|
| sesiones_persistentes | INTEGER NOT NULL DEFAULT 0 | 1 = toda pestaña nueva del host es persistente, con nombre automático. Viaja y sube la versión del host |
| tmux_persistente (F15) | — | Obsoleto. Al abrir por primera vez la 1.5.0, su valor pasa a `sesiones_persistentes` por el camino que sube la versión y se pone a 0; nadie lo vuelve a escribir |

`reconectar` y `portapapeles_remoto` siguen propios del equipo y sin viajar (T125). Solo tienen sentido en hosts de tipo `ssh`.

### 3.3 REGISTRO: tipos nuevos

| Tipo | Cuándo | Detalle |
|---|---|---|
| `sesion_persistente_creada` | La sesión ya existe en el host y la ficha está guardada | Host, nombre y origen |
| `sesion_persistente_reabierta` | Una ficha terminada se vuelve a crear | Host, nombre y si se usó el comando de reanudación |
| `sesion_persistente_enganchada` | El usuario se engancha desde la lista, el diálogo o la CLI (nunca en las reconexiones) | Host, nombre y «soltando a <equipo>» o «compartiendo» |
| `sesion_persistente_borrada` | Se borra una ficha | Host, nombre y «la sesión sigue en el host» si vivía |

`tmux_terminada` (F15) sirve también para estas sesiones. Nunca se anotan comandos, títulos, vistas previas ni contenido (T44).

### 3.4 DELIBERACIONES

Sin cambios: nada de esta fase delibera. Terminar una sesión pide confirmación y afecta a un solo host.

### 3.5 En la memoria del servidor (sin tabla)

| Qué | Contenido | Vida |
|---|---|---|
| Estado visto por ficha | Estado, clientes, equipo, actividad, atención, título, hora de la comprobación y error | Hasta la siguiente comprobación; se pierde al apagarse el servidor |
| Sesiones sin ficha por host | Las del tmux de MAGI que no están en la lista y las del tmux habitual | Igual |
| Vista previa | Las últimas líneas, en texto plano | Solo en el mensaje de respuesta: no se guarda |
| Pestaña soltada | Equipo que la tiene ahora y desde cuándo; la última pantalla | Hasta volver o cerrar la pestaña |

### 3.6 En el host

El servidor de tmux de MAGI (`tmux -L magi`), con sus sesiones, su historial (`history-limit`) y la opción de sesión `@magi_equipo`, la etiqueta del último equipo que se enganchó. MAGI no deja nada más en el host: ni ficheros ni configuración.

### 3.7 Relaciones

- HOSTS 1:N SESIONES_PERSISTENTES, con CASCADE: borrar un host borra sus fichas con lápida; la sincronización aplaza el borrado si hay trabajo vivo (F10).
- Una ficha corresponde, como mucho, a una sesión del tmux de MAGI en su host, por nombre exacto.
- Una pestaña persistente está enganchada a una ficha (por host y nombre), a una sesión sin ficha o a una del tmux habitual.

### 3.8 Diagramas de estados

**Ficha de sesión persistente**

```
  (no existe) ─ sesión creada en el host (a mano o en un host marcado) · a añadir ─▶ activa
  activa ─ e editar · , renombrar (primero en el host) ─▶ activa (versión nueva, viaja)
  activa ─ x borrar (confirmación) ─▶ borrada (lápida; si la sesión vive, sigue en el host sin ficha)
  activa ─ llega otra ficha con el mismo host y nombre ─▶ unida (queda la de uuid menor)
  Inválidas: guardar la ficha antes de que la sesión exista · terminar la sesión al borrar la ficha ·
             renombrar solo la ficha de una sesión viva
```

**Estado visto de una sesión (en cada equipo)**

```
  sin_comprobar ─ «estado» del host ─▶ viva_libre · viva_enganchada (clientes, equipo) · terminada
  sin_comprobar ─ sin respuesta en 10 s · pide huella, frase o contraseña ─▶ inalcanzable
  sin_comprobar ─ host sin tmux o con uno anterior a 3.0 ─▶ sin_tmux
  viva_libre ⇄ viva_enganchada                    (alguien se engancha o se suelta)
  viva_* ─ ya no sale en «estado» (exit, T terminar, reinicio del host) ─▶ terminada
  terminada ─ ↵ reabrir ─▶ viva_enganchada
  atención no ─ campana en una ventana que nadie mira ─▶ atención sí ─ alguien la mira ─▶ atención no
  Inválidas: pasar a terminada porque el host no responde · avisar de atención sin campana
```

**Pestaña persistente**

```
  creando ─ comprobar + crear (o la sesión ya existe) ─▶ enganchando ─ enganchar ─▶ enganchada
  enganchada ─ otro equipo se engancha sin compartir (el script sale con 100) ─▶ soltada
  soltada ─ ↵ volver (soltando a los demás) ─▶ enganchada · c compartir ─▶ enganchada · x ─▶ cerrada
  enganchada ─ red caída ─▶ reconectando (F15) ─┬─ la tiene este equipo o nadie ─▶ enganchada
                                                └─ la tiene otro equipo ─▶ soltada
  enganchada ─ exit del último shell · T terminar ─▶ terminada (la pestaña se cierra, F3)
  enganchada ─ prefijo x ─▶ cerrada (la sesión sigue viva en el host)
  Inválidas: quitar la sesión a otro equipo en una reconexión automática ·
             terminar la sesión al cerrar la pestaña sin preguntar
```

**Marca de sesiones persistentes del host**

```
  apagada ─ marcar en la ficha (al crear o al editar el host) ─▶ encendida (versión nueva, viaja)
  encendida ─ desmarcar ─▶ apagada (las sesiones vivas siguen; las pestañas nuevas ya no son persistentes)
  (F15) tmux_persistente = 1 ─ primera apertura de la 1.5.0 ─▶ encendida · tmux_persistente = 0
  Inválidas: encenderla en un host que no es ssh · cambiarla sin subir la versión del host
```

---

## 4. Flujos de Trabajo

### 4.1 Nueva sesión persistente

```
  n (Persistentes) · prefijo N · Hosts, Flota o paleta: «nueva sesión persistente»
      ▼ diálogo: host (solo ssh), nombre, carpeta, comando, reanudar y nota
  validar en el núcleo ─ nombre, carpeta o comando no válidos ─▶ error en el campo
      ▼ ¿ya hay una ficha con ese host y nombre? ─ sí ─▶ «ya existe · ↵ engancharse · cambiar el nombre»
  AbrirSesion{tmux: {magi, nombre, crear: {carpeta, comando}}}
      ▼ servidor: conexión del host (con diálogos: lo pidió el usuario, T15)
  comprobar ─ MAGI_TMUX=no ─▶ «instala tmux en el host» · ofrece una pestaña normal
            ─ versión anterior a 3.0 ─▶ «tmux demasiado antiguo (3.0 o posterior)»
            ─ MAGI_SESION=existe ─▶ «ya hay una sesión agente-api en dev-01 · ↵ engancharse · cambiar el nombre»
      ▼ crear ─ sin_carpeta ─▶ «la carpeta ~/proyectos/api no existe en dev-01» (sin ficha)
              ─ error ─▶ mensaje con su salida de error (sin ficha)
      ▼ creada ─▶ enganchar en el PTY de la pestaña
  cliente: guarda la ficha (versión nueva) · REGISTRO sesion_persistente_creada · viaja en la siguiente pasada
```

1. El diálogo propone como nombre el de la carpeta (`api`) o `sesion-<n>`, y la carpeta y el comando quedan vacíos (shell de acceso en la carpeta personal).
2. La validación es la misma en el núcleo y en el script (§7.7).
3. Si algo falla antes de `creada`, no queda ninguna ficha: MAGI anota cuando el efecto se ha producido.
4. El snippet al conectar y el comando inicial de la F6 solo se escriben en una sesión nueva sin comando propio (F15).

### 4.2 Pestaña en un host marcado

```
  ↵ en Hosts o Flota sobre un host con sesiones_persistentes = 1
      ▼ nombre automático: <etiqueta del equipo>-<n> (primer n libre en «estado» del host y en sus fichas)
  como 4.1, sin diálogo: shell de acceso en la carpeta personal · ficha con origen = host
      ├─ el host no tiene tmux ─▶ pestaña normal con aviso «dev-01 no tiene tmux: esta pestaña no es persistente»
      └─ creada ─▶ pestaña ◆ · ficha · REGISTRO sesion_persistente_creada
  prefijo x ─▶ «la sesión sobremesa-1 sigue viva en dev-01 · ↵ cerrar · T cerrar y terminarla · Esc» (F15)
```

### 4.3 Engancharse desde la lista y soltar al anterior

```
  ↵ sobre una ficha viva (c: compartiendo)
      ▼ ¿esta ventana ya tiene una pestaña enganchada a ella? ─ sí ─▶ ir a esa pestaña
  AbrirSesion{tmux: {magi, nombre, compartir}}
      ▼ PTY · enganchar magi <nombre> <soltar> <equipo>
  @magi_equipo = este equipo · tmux attach-session -t =<nombre> (con -d si no se comparte)
      ├─ no existía (75) ─▶ «la sesión ya no existe» · la ficha pasa a terminada · ↵ la reabre
      └─ enganchada ─▶ pestaña ◆ · REGISTRO sesion_persistente_enganchada («soltando a portátil» o «compartiendo»)
  en el equipo soltado: su script sale con 100 ─▶ estado ─▶ pestaña soltada («ahora la usa sobremesa»)
```

### 4.4 Reconexión de una pestaña persistente

```
  caída de red (F15) ─▶ reconectar sin diálogos ─▶ comprobar ─▶ estado de la sesión
      ├─ no existe ─▶ la pestaña se cierra como en la F15 · la ficha pasa a terminada
      ├─ sin clientes, o solo de este equipo (los de la conexión caída) ─▶ enganchar con -d
      └─ enganchada por otro equipo ─▶ pestaña soltada (no se la quita: ↵ volver lo hace a mano)
```

### 4.5 Vigilante y aviso de atención

```
  cada vigilar_seg (60) si hay alguna ventana conectada · enseguida, con ConsultarSesionesPersistentes
      ▼ hosts: los de las fichas y los marcados (filas validadas, solo ssh)
  por host (semáforo 8, plazo 10 s): conexión viva del pool o efímera, sin diálogos
      ├─ pide huella, frase o contraseña ─▶ inalcanzable: «sin comprobar: conéctate a mano»
      ├─ no responde ─▶ inalcanzable (sus fichas no pasan a terminada)
      └─ estado ─▶ por ficha: viva_libre · viva_enganchada · terminada; sin ficha; tmux habitual
  algo cambió ─▶ SesionesPersistentes{lista} a todas las ventanas
  atención no → sí ─▶ aviso en la barra de cada ventana · sí → no ─▶ el aviso se quita
  nada de esto va a REGISTRO ni al log
```

Con la lista a la vista, el cliente pide `ConsultarSesionesPersistentes` al abrirla, con `r` y cada `refresco_seg` (30 s); el servidor agrupa las peticiones que llegan seguidas y no repite un host comprobado hace menos de 5 s.

### 4.6 Historial y vista previa

```
  prefijo [ en una pestaña persistente ─▶ CapturarHistorial ─▶ historial <servidor> <nombre> <líneas>
      ▼ capture-pane -p -e -J -S -<líneas> ─▶ tramos (núcleo; topes de 8 MiB y 64 por fila) ─▶ HistorialLote
  modo historial de la F15: buscar, seleccionar, copiar y guardar (guardar, solo por orden del usuario)
      error o plazo (5 s) ─▶ «no se pudo leer el historial de tmux» y el modo historial no se abre
  v en la lista ─▶ PedirVistaPrevia ─▶ vista <servidor> <nombre> <20> ─▶ texto plano (≤ 64 KiB) ─▶ solo a esa ventana
      error ─▶ «no se pudo leer: <motivo>» (la lista sigue)
```

### 4.7 Terminar, borrar y reabrir

```
  T ─▶ «Terminar agente-api en web-01: se cerrará lo que corre dentro · ↵ terminar · Esc»
      ▼ TerminarTmux (=nombre)
      ├─ ok ─▶ REGISTRO tmux_terminada · la ficha queda terminada · sus pestañas se cierran (F3)
      └─ no_existe ─▶ «ya no existía» · ficha terminada
  x ─▶ confirmación (si vive: «seguirá viva en web-01, sin ficha») ─▶ lápida · sesion_persistente_borrada
  ↵ en una terminada ─▶ crear con comando_reanudar (o comando) en su carpeta ─▶ enganchar
      ▼ REGISTRO sesion_persistente_reabierta
  C ─▶ «borrar 3 fichas terminadas» ─▶ lápidas
```

### 4.8 Renombrar

```
  , ─▶ nombre nuevo (validado y libre entre las fichas del host) ─▶ RenombrarTmux
      ├─ ocupado ─▶ «ya hay otra sesión con ese nombre en dev-01»
      ├─ no_existe ─▶ se renombra solo la ficha (la sesión está terminada)
      └─ ok ─▶ el cliente cambia la ficha (versión nueva) · las pestañas enganchadas siguen
```

### 4.9 Sincronización de las fichas

```
  exportar: formato 4 si hay alguna ficha o algún host marcado; si no, el que tocaba (T68)
  importar: fichas por uuid (vectores, F10) y su host por uuid
      ├─ hay otra ficha con el mismo host y nombre ─▶ unión: queda la de uuid menor, con lo que le falte de la
      │      otra; la otra, lápida; conflicto «nombre» (todos los equipos acaban igual)
      ├─ lápida de una ficha con pestaña viva aquí ─▶ se borra la ficha; la pestaña sigue (es trabajo del servidor)
      └─ host borrado con fichas ─▶ como TUNELES (CASCADE; aplazado si hay trabajo vivo)
  móvil (lector): importa las fichas y la marca del host y no las usa
```

### 4.10 La marca de la F15 pasa a viajar

```
  primera apertura de la 1.5.0 (migración 11 aplicada)
      ▼ hosts con tmux_persistente = 1 y sesiones_persistentes = 0
  sesiones_persistentes = 1 por el camino que sube la versión · tmux_persistente = 0
      ▼ si otro equipo tenía otro valor, gana el último cambio (F10); la ficha del host lo enseña
  aviso una vez: «La marca de sesiones persistentes de 2 hosts viaja ahora a tus otros equipos»
```

### 4.11 Diagrama de secuencia: crear una sesión persistente y engancharse

```
magi (cliente)                  magi --servidor                                       dev-01 (tmux)
   │ diálogo validado: agente-api       │                                                   │
   │─AbrirSesion{tmux: crear}──────────▶│                                                   │
   │                                    │─exec terminal_tmux.sh comprobar magi agente-api──▶│
   │                                    │◀─────────────── MAGI_TMUX=3.5 · MAGI_SESION=nueva │
   │                                    │─exec crear agente-api ~/proyectos/api claude─────▶│
   │                                    │◀─────────────────────────── MAGI_RESULTADO=creada │
   │                                    │─PTY · enganchar magi agente-api 1 sobremesa──────▶│
   │                                    │◀───────────────── tmux pinta: Claude Code arranca │
   │◀── Hecho · Sesiones (◆ agente-api) │                                                   │
   │ guarda la ficha (versión nueva)    │                                                   │
   │ REGISTRO sesion_persistente_creada │                                                   │
   │ la sincronización la lleva a       │                                                   │
   │   tus otros equipos                │                                                   │
```

### 4.12 Diagrama de secuencia: engancharse desde otro equipo

```
sobremesa (magi --servidor)                dev-01 (tmux -L magi)                        portátil (magi --servidor)
   │ pestaña refactor enganchada                  │                                              │
   │                                              │◀─── PTY · enganchar magi refactor 1 portatil │
   │                                              │ attach -d: suelta a sobremesa                │
   │                                              │ @magi_equipo = portatil                      │
   │                                              │─tmux pinta refactor─────────────────────────▶│
   │◀───────────────── su cliente de tmux termina │                                              │
   │ la sesión sigue: el script sale con 100      │                                              │
   │─exec estado (quién la tiene)────────────────▶│                                              │
   │◀──────────── refactor · 1 cliente · portatil │                                              │
   │ pestaña soltada · Sesiones{lista}            │                                              │
   │ ↵ volver: enganchar con -d otra vez          │                                              │
```

### 4.13 Diagrama de secuencia: vigilante y aviso de atención

```
magi (cliente)                  magi --servidor                                         web-01
   │                                    │ cada 60 s, con alguna ventana conectada           │
   │                                    │─exec terminal_tmux.sh estado (plazo 10 s)────────▶│
   │                                    │◀───────────── agente-api · 0 clientes · campana 1 │
   │                                    │ atención: no → sí (no va a REGISTRO)              │
   │◀────── SesionesPersistentes{lista} │                                                   │
   │ barra: ◆ agente-api pide atención  │                                                   │
   │─AbrirSesion{tmux: agente-api}─────▶│                                                   │
   │                                    │─PTY · enganchar magi agente-api 1 sobremesa──────▶│
   │                                    │◀──────────────── tmux pinta; la campana se limpia │
   │                                    │─exec estado (siguiente vuelta)───────────────────▶│
   │                                    │◀────────────── agente-api · 1 cliente · campana 0 │
   │◀────── SesionesPersistentes{lista} │                                                   │
   │ el aviso de la barra se quita      │                                                   │
```

---

## 5. Script, Protocolo, Configuración, Teclas y CLI

### 5.1 `terminal_tmux.sh`: subórdenes

MAGI lo ejecuta con `sh -c "$SCRIPT" magi-tmux <suborden> <argumentos…>` por un canal `exec` del pool (T42), salvo `enganchar`, que va en el canal con PTY de la pestaña. `<servidor>` es `magi` (el propio: `tmux -L magi -f /dev/null`) o `habitual` (el tmux de siempre del usuario, solo para verlo, engancharse y terminar). Todo argumento se valida en Rust y en el script y va siempre como argumento, nunca dentro del texto del script. Toda orden con `-t` apunta a `=<nombre>` y la existencia se comprueba con la lista exacta (T126).

| Suborden | Argumentos | Qué hace y qué devuelve |
|---|---|---|
| `comprobar` | servidor, nombre | `MAGI_TMUX=<versión>\|no`, `MAGI_SESION=existe\|nueva` y `MAGI_TERM=tmux-256color\|screen-256color` |
| `crear` | nombre, carpeta, comando, líneas, columnas, filas | Aplica la configuración de MAGI y crea la sesión sin engancharse (`new-session -d`). `MAGI_RESULTADO=creada`, `existe` (73), `sin_carpeta` (72), `antiguo` (74) o `sin_tmux` (76) |
| `enganchar` | servidor, nombre, soltar (0/1), equipo | En el PTY: pone `@magi_equipo`, aplica la configuración (solo en el propio) y engancha (`attach-session -t =<nombre>`, con `-d` si soltar = 1). Al volver, sale con 0 si la sesión ya no existe, con 100 si sigue (te soltaron) y con 75 si no existía |
| `estado` | — | `MAGI_TMUX=…`, `MAGI_AHORA=<segundos>` y una línea `MAGI_SES` por sesión de los dos servidores, con los campos separados por 0x1F: servidor, nombre, clientes, equipo, actividad, creada, campana, ventanas y título |
| `vista` | servidor, nombre, líneas | Las últimas líneas visibles, en texto plano (`capture-pane -p -J`) |
| `historial` | servidor, nombre, líneas | El historial con colores (`capture-pane -p -e -J -S -<líneas>`) |
| `renombrar` | nombre, nuevo | `MAGI_RESULTADO=ok`, `no_existe` u `ocupado` (`rename-session -t =<nombre>`) |
| `terminar` | servidor, nombre | `MAGI_RESULTADO=ok` o `no_existe` (`kill-session -t =<nombre>`) |

`abrir` y `listar` de la F15 desaparecen: los sustituyen `crear`, `enganchar` y `estado`. El script cambia `~/` por `$HOME/` en la carpeta, sin `eval`. La actividad se compara con `MAGI_AHORA`, la hora del host, para no depender de que los relojes coincidan. Plazos (T32): 10 s para `comprobar`, `crear`, `estado`, `renombrar` y `terminar`; 5 s para `vista` e `historial`.

**Configuración del tmux de MAGI.** El script la aplica con `set` al crear y al enganchar en el servidor propio (es idempotente) y nunca carga `~/.tmux.conf` ni toca el tmux habitual:

| Opción | Valor | Para qué |
|---|---|---|
| `status` | `off` | Las pestañas las enseña MAGI |
| `prefix` y `prefix2` | `None` | tmux no se queda ninguna tecla: el multiplexor es MAGI |
| `mouse` | `on` | La rueda entra en el modo copia de tmux o llega al programa que pide el ratón |
| `extended-keys` (3.2+) y `extended-keys-format` (3.5+) | `on` y `csi-u` | Mayús+Intro y las combinaciones con modificadores llegan a Claude Code (D338) |
| `set-clipboard` | `on` | OSC 52 hacia MAGI, con el permiso por host de la F15 |
| `focus-events` | `on` | Foco (modo 1004) |
| `allow-passthrough` (3.3+) | `on` | Lo que un programa envuelve para el terminal exterior llega a MAGI |
| `set-titles` y `set-titles-string` | `on` y `#T` | El título del programa llega a la pestaña |
| `history-limit` | `[terminal] historial_lineas` | Antes de crear la sesión |
| `default-terminal` | `tmux-256color` (o `screen-256color` si el host no tiene su terminfo) | |
| `terminal-features` (3.2+) | `xterm*:RGB:clipboard:extkeys:focus:title` | Color completo y lo anterior; en 3.0 y 3.1, `terminal-overrides` con `Tc` |
| `escape-time` | `10` | Esc sin espera |
| `window-size` | `latest` | Al compartir, manda el tamaño del último que escribe |
| `monitor-bell` y `bell-action` | `on` y `any` | La campana marca la sesión: es la atención |

**Comando de arranque.** Con comando, la sesión ejecuta el envoltorio fijo del script (§7.2): el comando corre con el shell de acceso del usuario (para que tenga su PATH) y, al acabar, escribe «[MAGI] <comando> terminó con código N» y deja un shell en la carpeta. Sin comando, la sesión es el shell de acceso.

### 5.2 Protocolo cliente ⇄ servidor (`VERSION_PROTOCOLO = 10`)

| Mensaje | Sentido | Contenido |
|---|---|---|
| `AbrirSesion` con `tmux` | cliente → servidor | `tmux` pasa a ser `{servidor, nombre, crear?, compartir}`: `servidor` es `magi` o `habitual`; `crear`, `{carpeta?, comando?}` para una sesión nueva; `compartir`, no soltar a los demás. Cambia la semántica: protocolo v10 |
| `ListarTmux` → `SesionesTmux` | cliente ⇄ servidor | (F15) cada sesión gana `servidor`, `clientes`, `equipo`, `actividad`, `campana` y `titulo` |
| `TerminarTmux` → `TmuxTerminada` | cliente ⇄ servidor | (F15) gana `servidor`; siempre con `=<nombre>` |
| `ConsultarSesionesPersistentes` | cliente → servidor | `peticion_id`: comprobar ya los hosts con fichas o marcados; las peticiones seguidas se agrupan |
| `SesionesPersistentes{lista}` | servidor → todos | Estado visto por ficha (`uuid`) y, por host comprobado, sus sesiones sin ficha y las del tmux habitual: estado, clientes, equipo, actividad, atención, título, hora de la comprobación y error. Lista completa en cada cambio (T27) |
| `PedirVistaPrevia` → `VistaPrevia` | cliente ⇄ servidor | `peticion_id`, `host_id`, `servidor`, `nombre` → líneas en texto plano (≤ 64 KiB) y hora, solo a esa ventana |
| `RenombrarTmux` → `TmuxRenombrada` | cliente ⇄ servidor | `peticion_id`, `host_id`, `nombre`, `nuevo` → `ok`, `no_existe` u `ocupado` |
| `CapturarHistorial` → `HistorialLote` | cliente ⇄ servidor | (F15) en una pestaña persistente, el servidor lee el historial de tmux |
| `Sesiones{lista}` | servidor → todos | (F3, F15) `InfoSesion.tmux` gana `persistente{servidor, nombre}` y `soltada{por_equipo, desde}` |

`TrabajoServidor` no cambia: las sesiones persistentes viven en el host, no en el servidor, y una pestaña enganchada ya cuenta como sesión. Los `peticion_id` de estas peticiones usan el rango del terminal (2^49).

### 5.3 Configuración (`config.toml`)

| Clave | Por defecto | Uso |
|---|---|---|
| `[persistentes] vigilar_seg` | `60` | Cada cuánto comprueba el servidor las fichas, con alguna ventana conectada (30-600) |
| `[persistentes] refresco_seg` | `30` | Refresco con la lista a la vista (10-300) |
| `[persistentes] antigua_dias` | `7` | Días sin actividad para marcarla «antigua» (1-365) |
| `[persistentes] vista_previa_lineas` | `20` | Líneas de la vista previa (5-100) |

Un valor fuera de rango no impide arrancar: se acota donde se usa y MAGI avisa al arrancar (D261). `[terminal] historial_lineas` (F15) fija también el `history-limit` de las sesiones nuevas.

### 5.4 Teclas

| Dónde | Tecla | Acción |
|---|---|---|
| Sesiones · Persistentes | `↵` | Engancharse, soltando a los demás; en una terminada, reabrirla |
| | `c` | Engancharse compartiendo |
| | `v` | Vista previa |
| | `n` | Nueva sesión persistente |
| | `e` | Editar la ficha (carpeta, comando, reanudar y nota) |
| | `,` | Renombrar, en el host y en la ficha |
| | `T` | Terminar (confirmación) |
| | `x` | Borrar la ficha (confirmación) |
| | `a` | Añadir a la lista una sesión sin ficha |
| | `C` | Borrar las fichas terminadas (confirmación con recuento) |
| | `r` · `/` | Refrescar · filtrar |
| | `Tab` | Pasar a «Pestañas» y volver |
| Sesiones · Pestañas | `t` | (F15) Comprueba el host de la pestaña elegida: sus sesiones salen en «Persistentes» |
| Con pestañas | `prefijo N` | Nueva sesión persistente (diálogo) |
| Pestaña soltada | `↵` · `c` · `x` | Volver, soltando a los demás · compartir · cerrar |
| Vista previa | `↵` · `r` · `Esc` | Engancharse · refrescar · volver |

Las teclas del bloque «Persistentes» solo valen con el bloque enfocado; en «Pestañas» siguen las de la F3 y la F15.

### 5.5 CLI

| Orden | Qué hace |
|---|---|
| `magi sesion <nombre> [--host <host>]` | Abre MAGI enganchado a esa sesión, soltando a los demás. Si el nombre está en varios hosts, pide `--host` y dice cuáles; si no hay ficha, lo dice y sugiere las de nombre parecido |
| `magi sesiones [--json]` | Lista las fichas con su estado, que pregunta al servidor (lo arranca si hace falta, como `magi tunel`) |

### 5.6 Teclado: `modifyOtherKeys`

tmux pide las teclas extendidas a su terminal (MAGI) con `CSI > 4 ; 1 m` o `CSI > 4 ; 2 m` (`modifyOtherKeys` de xterm), no con el protocolo de kitty que trajo la F15. MAGI lo entiende en el emulador del núcleo, por pestaña, y mientras esté activo codifica las combinaciones con modificadores como xterm: Mayús+Intro → `CSI 27 ; 2 ; 13 ~`, Ctrl+Intro → `CSI 27 ; 5 ; 13 ~`. Con el modo 2, como xterm, también las combinaciones con Ctrl que tienen forma clásica. `CSI > 4 ; 0 m` o `CSI > 4 m` lo apagan, y se apaga solo al cerrarse la pestaña o al reiniciarse el terminal. tmux 3.5, con `extended-keys-format csi-u`, se las pasa a Claude Code como `CSI 13 ; 2 u`.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 SESIONES (prefijo l, F3) ── Tab: Persistentes ⇄ Pestañas
 │  PERSISTENTES: todas las fichas y, bajo cada host comprobado, sus sesiones sin ficha y las de tu tmux
 │    ├─ ↵ engancharse ─▶ SESIÓN (pestaña ◆) · c compartir ─▶ SESIÓN (pestaña ◆, compartida)
 │    ├─ v ─▶ [VISTA PREVIA] ─ ↵ ─▶ SESIÓN
 │    ├─ n ─▶ [NUEVA SESIÓN PERSISTENTE] ─ ↵ ─▶ SESIÓN
 │    ├─ e ─▶ [FICHA DE SESIÓN] · , ─▶ [RENOMBRAR]
 │    ├─ T ─▶ [CONFIRMAR TERMINAR] · x ─▶ [CONFIRMAR BORRAR] · C ─▶ [CONFIRMAR LIMPIAR]
 │    └─ a añadir (sin ficha) · r refrescar · / filtrar
 │  PESTAÑAS (F3, F15): ↵ entrar · t comprobar su host · , renombrar · r reconectar · x cerrar · S apagar
 HOSTS (F2) · FLOTA (F1) ── ↵ en un host marcado: pestaña persistente · paleta «nueva sesión persistente»
 FICHA DE HOST ── bloque SESIONES: «Todas las sesiones persistentes» (al crear y al editar)
 SESIÓN (F3) ── prefijo N ─▶ [NUEVA SESIÓN PERSISTENTE] · pestaña soltada: ↵ volver · c compartir · x cerrar
 BARRA GLOBAL ── «◆ agente-api pide atención · web-01» desde cualquier vista
 CLI ── magi sesion <nombre> ─▶ SESIÓN enganchada · magi sesiones
```

### 6.1 Sesiones con el bloque Persistentes

```
┌ MAGI · SESIONES ─────────────────────────────────────────────────────────────┐
│ PERSISTENTES · 5 · 2 sin ficha                       comprobadas hace 12 s   │
│       ESTADO      NOMBRE       HOST     ACTIVIDAD   TÍTULO                   │
│  ▸◆ ! libre       agente-api   web-01   hace 4 m    Claude Code: permiso     │
│   ◆ ● aquí        refactor     dev-01   hace 20 s   Claude Code              │
│   ◆ ● portátil    docs         dev-01   hace 1 m    Claude Code              │
│   ◆ ✕ terminada   informes     dev-01   ayer        ↵ reabrir                │
│   ◆   libre       pruebas      dev-01   hace 9 d    usuario@dev-01  antigua  │
│       sin ficha   portatil-3   web-01   hace 3 h    a añadir                 │
│       de tu tmux  trabajo      web-01   hace 1 d    ↵ engancharse            │
│  ──────────────────────────────────────────────────────────────────────────  │
│ PESTAÑAS · 2                                                                 │
│  ● ◆ refactor       dev-01   2 h    Claude Code                    1 ventana │
│  ●   db-01          db-01    18 m   usuario@db-01: ~               1 ventana │
├──────────────────────────────────────────────────────────────────────────────┤
│ ↵ engancharse · c compartir · v vista previa · n nueva · T terminar · ? más  │
├──────────────────────────────────────────────────────────────────────────────┤
│ ◆ agente-api pide atención · web-01                         ^] l  Sesiones   │
└──────────────────────────────────────────────────────────────────────────────┘
```

Arriba, las fichas de todos los hosts y, bajo cada host comprobado, las sesiones sin ficha («sin ficha», con `a` para añadirlas) y las de tu tmux de siempre («de tu tmux»: se enganchan y se terminan, sin ficha). La cabecera dice cuándo fue la última comprobación; un host que no se pudo comprobar enseña `?` en sus fichas y el motivo en la barra.

### 6.2 Nueva sesión persistente

```
┌ MAGI · SESIONES ─────────────────────────────────────────────────────────────┐
│ PERSISTENTES · 5 · 2 sin ficha                       comprobadas hace 12 s   │
│         ┌ NUEVA SESIÓN PERSISTENTE ────────────────────────────────┐         │
│         │ Host       dev-01                                        │         │
│         │ Nombre     agente-api_                                   │         │
│         │ Carpeta    ~/proyectos/api                               │         │
│         │ Comando    claude                                        │         │
│         │ Reanudar   claude --continue                             │         │
│         │ Nota       refactor del módulo de pagos                  │         │
│         │ ──────────────────────────────────────────────────────── │         │
│         │ Vive en dev-01 aunque cierres MAGI o apagues el equipo,  │         │
│         │ y la ves en todos tus equipos.                           │         │
│         │ ↵ crear y engancharse · Tab siguiente · Esc cancelar     │         │
│         └──────────────────────────────────────────────────────────┘         │
├──────────────────────────────────────────────────────────────────────────────┤
│ ↵ engancharse · c compartir · v vista previa · n nueva · T terminar · ? más  │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.3 Vista previa

```
┌ MAGI · SESIONES · VISTA PREVIA ──────────────────────────────────────────────┐
│ agente-api · web-01 · libre · actividad hace 4 m · ! pide atención           │
├──────────────────────────────────────────────────────────────────────────────┤
│   ● Cambios listos en src/pagos/cobro.py                                     │
│                                                                              │
│   ¿Ejecuto las pruebas?                                                      │
│   ❯ 1. Sí                                                                    │
│     2. Sí, y no vuelvas a preguntar                                          │
│     3. No, dime qué cambiar                                                  │
│                                                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ últimas 20 líneas · 14:31:07 · ↵ engancharse · r refrescar · Esc volver      │
└──────────────────────────────────────────────────────────────────────────────┘
```

Texto plano, sin colores ni escapes, solo para la ventana que la pide; no se guarda.

### 6.4 Pestaña soltada

```
┌ MAGI · SESIÓN ───────────────────────────────────────────────────────────────┐
│ ▸◆ refactor │ ● db-01 │ + nueva                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ (la última pantalla, en gris)                                                │
│                                                                              │
│         ┌ SOLTADA ─────────────────────────────────────────────────┐         │
│         │ Ahora la usa portátil (desde las 16:02).                 │         │
│         │ La sesión sigue viva en dev-01.                          │         │
│         │ ↵ volver (la suelta allí) · c compartir · x cerrar       │         │
│         └──────────────────────────────────────────────────────────┘         │
│                                                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ ◆ persistente · refactor · tmux 3.5 · soltada                                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.5 Ficha de host: bloques SESIONES y TERMINAL

```
 ┌─ SESIONES (viaja a tus equipos) ─────────────────────────┐
 │ [x] Todas las sesiones persistentes (tmux en el host)    │
 │     nombres automáticos: sobremesa-1, sobremesa-2…       │
 │ Espacio marca · Tab siguiente · Ctrl+S guardar           │
 └──────────────────────────────────────────────────────────┘

 ┌─ TERMINAL (solo en este equipo) ─────────────────────────┐
 │ [x] Reconectar solo si se cae la red                     │
 │ [ ] Permitir que el host copie al portapapeles           │
 │     (OSC 52; leerlo, nunca)                              │
 └──────────────────────────────────────────────────────────┘
```

El bloque SESIONES está en la ficha al crear y al editar el host, solo en hosts de tipo `ssh`, y viaja; si el host ya arranca tmux en su shell, advierte de tmux dentro de tmux (R143). El bloque TERMINAL de la F15 pierde la sesión persistente y sigue siendo propio del equipo.

### Marcas

| Marca | Texto en la barra | Significado |
|---|---|---|
| `◆` | persistente | Pestaña o ficha de una sesión persistente (en ASCII, `P`) |
| `!` | pide atención | La sesión tocó la campana y nadie la ha mirado (F15) |
| `●` | enganchada | Alguien la tiene: «aquí» o el nombre del equipo |
| `✕` | terminada | La sesión ya no está en el host |
| `?` | sin comprobar | No se pudo comprobar su host; el motivo, en la barra |
| «antigua» | antigua | Más de `antigua_dias` sin actividad |

### Notas de UX

- **Nada se termina solo.** Terminar siempre pide confirmación y dice qué se cierra; borrar una ficha no toca el host.
- **Soltar se ve siempre:** la pestaña soltada dice quién la tiene y desde cuándo, y conserva la última pantalla.
- **Un host que no responde no mata fichas:** sus sesiones salen «sin comprobar», nunca «terminadas».
- **Claude Code y la atención:** para que avise, su canal de avisos tiene que ser la campana del terminal (`preferredNotifChannel`); la ayuda lo dice. El título que pone se ve igual.
- **No pongas secretos en el comando:** la ficha viaja (cifrada) a tus equipos; la ayuda del diálogo lo recuerda.
- Todas las vistas, diálogos y modos nuevos tienen modo estrecho e instantáneas a 40×12, 80×24 y 200×60 (T45). En estrecho, el bloque Persistentes enseña marca, nombre y host.

---

## 7. Lógica de Negocio

### 7.1 Nombres

```python
import re

NOMBRE = r"^[A-Za-z0-9_-]{1,64}$"            # lo mismo en Rust y en el script (F15, §7.10)

def nombre_automatico(etiqueta_equipo, en_host, en_fichas):
    """En un host marcado: <etiqueta>-<n> con el primer n libre. Las fichas terminadas reservan su nombre."""
    usados = set(en_host) | set(en_fichas)
    n = 1
    while f"{etiqueta_equipo}-{n}" in usados:
        n += 1
    return f"{etiqueta_equipo}-{n}"

def nombre_propuesto(carpeta, en_fichas):
    """El diálogo propone el nombre de la carpeta («~/proyectos/api» → «api») o sesion-<n>."""
    base = re.sub(r"[^A-Za-z0-9_-]+", "-", carpeta.rstrip("/").split("/")[-1]).strip("-")[:64] if carpeta else ""
    return base if base and base not in en_fichas else nombre_automatico("sesion", [], en_fichas)
```

- La etiqueta del equipo es la de la F15 (`etiqueta_equipo`, hasta 20 caracteres en minúsculas y sin tildes).
- En el tmux propio los nombres ya no llevan el prefijo `magi-` (D340): el servidor entero es de MAGI.

### 7.2 Comando de arranque

```python
ENVOLTORIO = ('"${SHELL:-/bin/sh}" -l -i -c "$1"; c=$?; '
              'printf "\\n[MAGI] %s terminó con código %s\\n" "$1" "$c"; '
              'exec "${SHELL:-/bin/sh}" -l')

def argumentos_crear(nombre, carpeta, comando, lineas, columnas, filas):
    """Lo que el script pasa a tmux (3.0+: argumentos sueltos, sin shell en medio).
    El comando va como dato ($1), nunca dentro del texto del envoltorio."""
    orden = ["new-session", "-d", "-s", nombre, "-x", str(columnas), "-y", str(filas), "-c", carpeta]
    if comando:
        orden += ["--", "sh", "-c", ENVOLTORIO, "magi-sesion", comando]
    return orden          # history-limit = lineas se aplica antes, con set
```

- El comando corre con el shell de acceso del usuario en modo interactivo para que tenga el mismo PATH que una pestaña normal (por ejemplo, el `claude` instalado en `~/.local/bin`).
- Al acabar, el envoltorio deja un shell de acceso en la carpeta con la salida a la vista: la sesión no se cierra sola (D336).
- Reabrir una terminada usa `comando_reanudar` o, si está vacío, `comando`.

### 7.3 Estado derivado

```python
def estado_visto(ficha, host):
    """host: la última comprobación de su host, o None si aún no se ha comprobado."""
    if host is None:
        return "sin_comprobar"
    if host.error in ("sin_respuesta", "necesita_dialogo", "plazo"):
        return "inalcanzable"                 # nunca «terminada» por no poder mirar
    if host.tmux is None or host.tmux < (3, 0):
        return "sin_tmux"
    s = host.magi.get(ficha.nombre)           # sesiones del servidor propio, por nombre exacto
    if s is None:
        return "terminada"
    return "viva_enganchada" if s.clientes > 0 else "viva_libre"

def atencion(s):
    return s is not None and s.campana        # alguna ventana con la campana sin mirar

def antigua(s, ahora_host, dias):
    return s is not None and (ahora_host - s.actividad) >= dias * 86400
```

- La actividad es la de la ventana más reciente de la sesión (`window_activity`), comparada con `MAGI_AHORA`, la hora del host.
- tmux marca la campana de una ventana aunque nadie esté enganchado, y la quita cuando un cliente la mira: no hace falta guardar nada.

### 7.4 Engancharse y reconectar

```python
def al_engancharse(compartir):
    return {"soltar": 0 if compartir else 1}             # sin compartir: attach -d

def al_reconectar(s, este_equipo):
    """Reconexión automática (F15) de una pestaña persistente: nunca quita la sesión a otro equipo."""
    if s is None:
        return "cerrar"                                   # la sesión ya no existe
    if s.clientes > 0 and s.equipo != este_equipo:
        return "soltada"                                  # ↵ volver lo hace a mano
    return "enganchar_soltando"                           # -d: quita los clientes de la conexión caída

def al_volver_el_script(codigo):
    return {0: "terminada", 100: "soltada", 75: "no_existe"}.get(codigo, "error")
```

`@magi_equipo` guarda solo el último equipo que se enganchó: con dos equipos compartiendo, la lista dice el último.

### 7.5 Unión de fichas en la sincronización

```python
def unir(a, b):
    """Dos fichas del mismo host y nombre son la misma sesión: queda la de uuid menor (D337)."""
    gana, pierde = (a, b) if a.uuid < b.uuid else (b, a)
    for campo in ("carpeta", "comando", "comando_reanudar", "nota"):
        if not getattr(gana, campo) and getattr(pierde, campo):
            setattr(gana, campo, getattr(pierde, campo))  # edición nueva de la ganadora
    return gana, pierde                                    # pierde: lápida y conflicto «nombre»
```

Es determinista: cada equipo llega a la misma ficha aunque se unan en orden distinto, y la siguiente pasada iguala las versiones.

### 7.6 Formato de sincronización

```python
def formato_a_escribir(hay_no_ssh, hay_comprobaciones_propias, hay_fichas, hay_hosts_marcados):
    if hay_fichas or hay_hosts_marcados:
        return 4                              # F16
    if hay_comprobaciones_propias:
        return 3                              # F13
    return 2 if hay_no_ssh else 1             # F12 · F10
```

El `formato` 4 añade el elemento `sesion_persistente` y el campo `sesiones_persistentes` del host. Un lector que no lo conoce se queda en «formato nuevo» (F10) y no importa: por eso la app Android aprende a leerlo en esta fase (R153).

### 7.7 Validaciones

- **Nombre:** `NOMBRE`, y libre entre las fichas del host; en el host, `crear` comprueba que no existe.
- **Carpeta:** absoluta o `~/…`, hasta 1024 bytes, sin saltos de línea; el script comprueba que existe.
- **Comando y comando de reanudación:** hasta 4096 bytes, sin bytes nulos. Son texto libre, como un snippet: no se escapan, porque son el comando.
- **Nota:** hasta 500 caracteres.
- **Equipo:** la etiqueta `^[a-z0-9-]{1,20}$`; lo que no case en `@magi_equipo` se ve como «?».
- **Host:** solo de tipo `ssh`.
- **Lo que devuelve el script** es entrada no fiable (T109): 500 sesiones por host como mucho, campos separados por 0x1F, títulos de hasta 120 caracteres sin caracteres de control, vista previa de hasta 64 KiB e historial de hasta 8 MiB y 64 tramos por fila (D318); nunca pánico.

### 7.8 Casos especiales

- **Dos equipos crean a la vez el mismo nombre en el mismo host:** el segundo `crear` da `existe` y MAGI le ofrece engancharse.
- **`exit` dentro de la sesión:** la pestaña se cierra (F3) y la ficha pasa a terminada en la siguiente comprobación.
- **Host marcado sin tmux:** pestañas normales con aviso y sin fichas.
- **Otro equipo borra la ficha de una sesión enganchada aquí:** la pestaña sigue y la sesión pasa a «sin ficha».
- **El tmux habitual del usuario está abierto:** no se toca; son servidores distintos (D329).
- **El shell del host arranca tmux por su cuenta (R143):** dentro de la sesión de MAGI la variable `TMUX` ya está puesta, y la mayoría de configuraciones no arrancan otro; la ficha lo advierte.
- **Claude Code sin la campana como canal de avisos:** no habrá aviso de atención; el título sí se ve.
- **Varias ventanas de MAGI en el mismo equipo:** un solo vigilante, el del servidor, y una consulta por host.
- **Engancharse a una sesión del tmux habitual (F15):** como antes, sin configuración de MAGI ni ficha; el historial de MAGI se lee igual con `historial habitual`.

### 7.9 Limitaciones

- Un reinicio del host termina sus sesiones: se reabren con su comando de reanudación (R149).
- Los avisos OSC 9 y OSC 777 que el programa emite dentro de tmux no llegan a MAGI: la atención es la campana (D335).
- Con tmux anterior a 3.5, Mayús+Intro puede no llegar a Claude Code; la barra de la pestaña enseña la versión de tmux (R147).
- El móvil no ve las sesiones persistentes (D326).

---

## 8. Requisitos No Funcionales

**Seguridad**

- Nada nuevo queda expuesto: el tmux de MAGI usa el socket de tmux de siempre en el host (`/tmp/tmux-<uid>/magi`, en un directorio 700); solo el mismo usuario del host puede engancharse.
- Lo que llega del host es entrada no fiable, con los topes de §7.7, y nunca provoca un pánico (T109).
- El comando de arranque es texto del usuario, como un snippet: se ejecuta en el host, llega al script como argumento, nunca se mete en el texto del script y no va a REGISTRO.
- Terminar siempre pide confirmación; borrar una ficha no toca el host.
- La reconexión automática nunca quita una sesión a otro equipo; engancharse a mano sí, y el otro equipo lo ve.
- El vigilante es una operación automática: sin diálogos (T15, T49).

**Protección de datos (RGPD)**

- La salida de las sesiones, sus títulos, las vistas previas y el historial de tmux no se guardan: viven en memoria y solo se envían a la ventana que los pide; nunca van al log ni a REGISTRO (T44, T109). Guardar el historial en un fichero sigue siendo por orden del usuario (F15).
- Las fichas viajan cifradas (F10) con el host, el nombre, la carpeta, los comandos y la nota: la ayuda pide no poner secretos en el comando.
- REGISTRO guarda solo el host y el nombre.

**Backups y recuperación**

SESIONES_PERSISTENTES viaja como el resto del inventario y entra en las copias previas a importar (F10). En el host no hay nada que respaldar: si una sesión se pierde, se reabre desde su ficha.

**Rendimiento**

- El vigilante lanza una orden `estado` por host con fichas o marcado cada 60 s, con 8 a la vez como mucho: con 20 hosts, 20 órdenes por minuto.
- `estado` devuelve hasta 500 sesiones en una sola línea por sesión; la lista pinta sin esperar a la red.
- El historial de tmux respeta los topes de la F15 (8 MiB por captura) y se pide solo al abrir el modo historial.

**Accesibilidad**

Las marcas llevan texto en la barra (no solo color), tienen respaldo ASCII (`◆` → `P`) y las teclas están en la barra de cada vista y en `?`.

---

## 9. Integraciones

| Con | Versión | Uso |
|---|---|---|
| tmux en el host | 3.0 o posterior (3.5 para Mayús+Intro en Claude Code) | Servidor propio `tmux -L magi -f /dev/null`; subórdenes del script (§5.1) |
| Claude Code | — | Sin integración propia: se recomienda la campana del terminal como canal de avisos y `claude --continue` como comando de reanudación |
| Sincronización (F10) | `formato` 4 | Elemento `sesion_persistente` y campo `sesiones_persistentes` del host |
| App Android (F11, F14) | La que salga con la 1.5.0 | Solo lee el `formato` 4 |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D326 — Solo escritorio; el móvil queda pendiente** (Hector, 9c y aclaración de la 4). La app solo aprende a leer el `formato` 4. Descartado llevar ya los avisos al móvil: exigirían comprobar en segundo plano con la clave del móvil aunque MAGI esté bloqueado.
- **D327 — La marca del host viaja** (Hector, aclaración) y sustituye a `tmux_persistente` de la F15. Su valor se copia en la primera apertura por el camino que sube la versión; descartado copiarlo en la migración SQL: no subiría la versión y no viajaría (el fallo de R84).
- **D328 — Viajan las fichas, no el estado** (Hector, 2a). El host es la fuente de verdad. `formato` 4 solo con fichas o hosts marcados (T68).
- **D329 — Un tmux propio en el host (`tmux -L magi -f /dev/null`) con la configuración de MAGI** (Hector, 5a). Descartados el tmux habitual (sus opciones de servidor afectan a todas las sesiones del usuario), screen y zellij (menos frecuentes y sin las mismas lecturas) y dtach, abduco o shpool (no suelen estar instalados, y dtach no repinta al volver).
- **D330 — Sin prefijo de tmux:** MAGI es el multiplexor. La rueda va al modo copia de tmux o al programa, y el historial de MAGI se lee con `capture-pane` (Hector, 6a).
- **D331 — tmux 3.0 o posterior** para el servidor propio (órdenes como argumentos sueltos, sin shell en medio). Las opciones más nuevas se activan según la versión que da `comprobar`.
- **D332 — Engancharse suelta a los demás (`attach -d`), con «compartir»** (Hector, 7a). `@magi_equipo` dice quién la tiene. El script sale con 100 cuando lo sueltan, así el servidor lo distingue de una sesión terminada sin buscar texto en la salida.
- **D333 — La reconexión automática no quita la sesión a otro equipo:** si la tiene otro, la pestaña queda soltada. Solo el usuario suelta a otro equipo.
- **D334 — El vigilante vive en el servidor** y solo corre con alguna ventana conectada, cada 60 s y sin diálogos (Hector, 4b, parte de escritorio). Descartado en el cliente: cada ventana repetiría las consultas.
- **D335 — La atención es la campana que tmux marca en la ventana**, también sin nadie enganchado. Descartados OSC 9 y OSC 777: tmux no los pasa. Claude Code tiene que avisar con la campana.
- **D336 — Al acabar el comando, un shell en su carpeta con el código de salida; la ficha muerta queda «terminada»** y se reabre con `comando_reanudar` (Hector, 8a). Nunca se termina sola; «antigua» a los `antigua_dias`.
- **D337 — Dos fichas con el mismo host y nombre son la misma sesión y se unen** (queda la de uuid menor). Excepción a D136 (renombrar a `(2)`), que aquí dejaría una ficha apuntando a una sesión que no existe.
- **D338 — MAGI entiende `modifyOtherKeys`**, que es como tmux pide las teclas extendidas. Si con `extended-keys on` Claude Code no recibe Mayús+Intro, se usa `always`: se decide con la prueba de concepto del primer sprint.
- **D339 — Bloque «Persistentes» en la vista Sesiones, aviso en la barra global y `magi sesion` y `magi sesiones`** (Hector, 10a). Descartadas una vista propia en `F11` y la lista dentro de Flota.
- **D340 — Nombres sin el prefijo `magi-`** en el tmux propio; los automáticos son `<etiqueta del equipo>-<n>`.
- **D341 — El servidor lee SESIONES_PERSISTENTES y HOSTS (como TUNELES) y nunca escribe en ellas;** el cliente escribe la ficha cuando la sesión ya existe en el host.
- **D342 — Las sesiones de la F15, en el tmux habitual, se ven, se enganchan y se terminan, pero no se migran:** un proceso no se puede mover de un servidor de tmux a otro.
- **D343 — Protocolo v10** (cambia `AbrirSesion.tmux`), `peticion_id` en el rango del terminal (2^49) y versión 1.5.0. `TrabajoServidor` no cambia.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Semanas |
|---|---|---|
| S1 — Núcleo, script y sincronización | Prueba de concepto primero: Mayús+Intro con tmux 3.4 y 3.5 y Claude Code real (`extended-keys on` frente a `always`, D338). Migración 11 y almacén; copia de la marca de la F15; validadores, nombres y estado; `terminal_tmux.sh` con el servidor propio, su configuración y las subórdenes; `=<nombre>` en todo; `modifyOtherKeys`; `formato` 4, unión y lápidas; la app lee el `formato` 4. Pruebas `sesiones_persistentes`, `tmux_script`, `tmux_real` y `sincronizacion_sesiones` | 1 |
| S2 — Servidor y protocolo v10 | Pestaña persistente (crear, enganchar, soltada, reconexión), vigilante y `SesionesPersistentes{lista}`, historial con `capture-pane`, vista previa, renombrar y terminar. Prueba `sesiones_persistentes_servidor` | 1 |
| S3 — Cliente, CLI y cierre | Bloque Persistentes, diálogos, bloque SESIONES de la ficha, host marcado, pestaña soltada, aviso en la barra, ayuda y paleta; `magi sesion` y `magi sesiones`; `[persistentes]`; instantáneas; prueba de extremo a extremo; revisión adversarial; 1.5.0 y guion de validación | 1 |

**Total: 3 sprints, ~3 semanas.** Supuestos: un desarrollador con Claude Code y dedicación parcial; tmux 3.4 en las imágenes de CI (Ubuntu 24.04) para `tmux_real`, y 3.5 a mano para la prueba de concepto; la F15 cerrada con su prompt de reanudación.

**Guion de validación manual** (lo hace el desarrollador, con Claude Code real):

1. Nueva sesión persistente en un host con tmux 3.5: nombre `agente`, la carpeta de un proyecto y el comando `claude`; trabajar un poco.
2. Cerrar MAGI del todo (también `magi servidor parar`) y volver: Sesiones la enseña viva y libre, y `↵` vuelve a la misma conversación.
3. Desde el portátil, ya sincronizado: la ficha está; `↵` la engancha y en sobremesa queda «soltada»; `↵` en sobremesa la recupera.
4. `c` en los dos equipos y escribir en uno: se ve en los dos.
5. Con `preferredNotifChannel = "terminal_bell"`, pedirle algo que necesite permiso y soltar la sesión: la barra avisa en menos de un minuto.
6. `prefijo [` en la sesión: buscar en lo que escribió Claude Code y copiarlo.
7. Mayús+Intro dentro de la sesión con tmux 3.5 y con 3.4 (anotar si llega).
8. Reiniciar el host: la ficha pasa a terminada; `↵` la reabre con `claude --continue`.
9. Marcar un host con «Todas las sesiones persistentes»: abrir dos pestañas (nombres automáticos), cerrar una sin terminarla y verla en la lista.
10. Suspender el portátil 5 min con la sesión enganchada: al volver, la reconexión la recupera si nadie la tiene, o queda soltada si la tiene sobremesa.
11. `magi sesion agente` desde una terminal y `magi sesiones --json`.
12. El móvil sincroniza con el `formato` 4 sin quedarse en «formato nuevo».

---

## 12. Conexiones con Otras Fases

**Dependencias**

- **F15:** tmux, reconexión automática, historial, modos del terminal y la regla de nombres exactos (R146); su prompt de reanudación va antes.
- **F3:** servidor de sesiones, pestañas y vista Sesiones.
- **F10:** sincronización (`formato` 4, vectores, lápidas y conflictos).
- **F11 y F14:** la app Android, que solo aprende a leer el `formato` 4.
- **F7:** `Geometria` e instantáneas a 40×12, 80×24 y 200×60.
- **F6:** el snippet al conectar y el comando inicial, solo en sesiones nuevas sin comando propio.

**Datos compartidos**

- HOSTS: `sesiones_persistentes` (nueva, viaja) sustituye a `tmux_persistente` (F15).
- SESIONES_PERSISTENTES: nueva, viaja.
- REGISTRO: cuatro tipos nuevos y `tmux_terminada` (F15).
- `formato` 4 de sincronización.

**Lo que viene**

- **F17 (Accesos y certificados SSH)** no depende de esta fase.
- **F19 (Caja negra)** podrá grabar también las sesiones persistentes.
- **Una fase futura del móvil** podrá añadir la lista, engancharse y los avisos con la app cerrada, con la lógica ya en el núcleo (D326).
- **F22 (Despliegues y runbooks, aplazada)** podrá lanzar sus pasos largos dentro de una sesión persistente.
