# MAGI - Fase 14: Android, Segunda Vuelta

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 9 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-13 con informe procesado. Esta fase da por hechos los dos retoques del prompt de reanudación de la F13: el sondeo recibe `[servicios] contenedores` como argumento `<motores>` (R125) y un host sin systemd abre en Contenedores. Siguen abiertos el resto de cierres:
>
> - F13: las pruebas instrumentadas de Android.
> - F12: la prueba instrumentada de Android y R117.
> - F11: el guion del plegable, la CI, la primera release (0.1.0) y los trámites R103, R104 y R107.
>
> Conviene pasar esos tres prompts de reanudación y subir sus ramas antes de empezar, porque esta fase mueve el sondeo al núcleo y se apoya en ese código. La F10 sigue sin la corrección 10b y la F9, pendiente de su primera publicación.

---

## 1. Visión General

La Fase 14 convierte la app Android en el **móvil de guardia**. Cuando algo se cae y no hay ordenador a mano, sirve para ver qué host está mal y por qué, y para arreglarlo desde el teléfono. Hasta ahora el móvil enseñaba el inventario y abría terminales y túneles. Para saber si un host estaba caído, o para reiniciar un servicio, había que abrir una terminal y escribir las órdenes a mano.

La app gana cuatro cosas que ya tiene el escritorio, con la misma lógica del núcleo:

- **Flota** con sondeo, que sustituye a Hosts y es la pantalla de inicio.
- **Servicios**: servicios, timers y contenedores, con acciones, sudo y logs en vivo, como en la F13.
- **Snippets** con **deliberación MAGI** (F6).
- La vista de **Registro**.

El móvil sigue siendo **lector** de la sincronización: no edita el inventario ni escribe fichero. Por eso la corrección 10b sigue sin hacer falta.

En el móvil no hay `magi --servidor`. `magi-movil` hace su papel dentro de la app, como ya hacía con las sesiones y los túneles (F11): ejecuta las acciones, los snippets y los logs con el código del núcleo. Lo que se acumula (el anillo de logs, las acciones en curso y las ejecuciones) vive en Rust, y la app solo pinta.

**Objetivos principales**

1. Flota como pantalla de inicio, con el sondeo del escritorio movido al núcleo: estados, métricas, vigilados y fallos. Sondea al abrir, al deslizar y cada 30 s con Flota a la vista.
2. Servicios en el móvil con la lógica de la F13: «Toda la flota» y, por host, servicios, timers y contenedores con su detalle, acciones con espera al resultado, sudo en memoria y logs en vivo.
3. Snippets sincronizados que se ejecutan en uno o varios hosts, con variables y resultados por host, y el snippet al conectar en las sesiones.
4. Deliberación MAGI en el móvil:
   - MELCHIOR-1 y BALTHASAR-2 como en el escritorio;
   - CASPER-3 rechaza con «solo en el escritorio»;
   - lo deliberado se ejecuta manteniendo pulsado 1 s.
5. Registro del móvil con filtros y el detalle de cada deliberación.
6. Pruebas de extremo a extremo (T95) en Rust, en el emulador y en CI, y app 0.2.0.

**Contexto.** Producto propio. Hector atiende sus servidores (systemd detrás de Nginx, algunos con Compose) y llega a ellos por Tailscale desde un plegable Honor con MagicOS. Los avisos ya los da Perseverus, la monitorización de 4d3. Por eso el móvil no vigila en segundo plano: se abre cuando llega el aviso.

Respuestas de Hector (9 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | El móvil de guardia: Flota con sondeo, Servicios, snippets con deliberación y registro. Sigue siendo lector; editar el inventario y SFTP quedan para más adelante (a) |
| 2 | Los escritorios remotos siguen sin abrirse en el móvil (a) |
| 3 | Flota sustituye a Hosts y es la pantalla de inicio; barra FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS (a) |
| 4 | Sondeo al abrir Flota, al deslizar y cada 30 s con Flota a la vista (se puede apagar); nada en segundo plano (a) |
| 5 | Servicios completo, como en el escritorio (a) |
| 6 | Contraseña de sudo pedida, comprobada y en memoria 5 min; nunca se guarda (a) |
| 7 | Ejecutar los snippets sincronizados en uno o varios hosts, con variables, salida y deliberación, y el snippet al conectar (a) |
| 8 | MELCHIOR-1 y BALTHASAR-2 como en el escritorio; CASPER-3 cuenta como no superada y hay que forzar con motivo (a) |
| 9 | Confirmación con la acción y el objetivo; lo deliberado se ejecuta manteniendo pulsado 1 s, y forzar pide un motivo de 10 caracteres (a) |
| 10 | Registro del propio móvil, con filtros y el detalle de cada deliberación (a) |
| 11 | App 0.2.0 y el servidor de pruebas en el job `humo` de CI (R106) (a) |

**Fuera de alcance**

- Editar el inventario desde el móvil y escribir su fichero en la carpeta: el móvil sigue lector (T87). Por eso tampoco se crean ni se editan snippets, comprobaciones propias ni verificaciones. Irá con la F18 (Equipo), después de la corrección 10b, o más adelante.
- SFTP en el móvil, salvo el listado de un directorio que hace BALTHASAR-2.
- Abrir escritorios remotos (RDP, VNC, voxl) en el móvil.
- Avisos y sondeo con la app en segundo plano o cerrada (para eso está Perseverus).
- Abrir un snippet en una sesión («en pestaña» de la F6) y los atajos de Flota.
- CASPER-3 en el móvil: su comando de tests es del ordenador.
- Exportar el registro o ver el de otros equipos: REGISTRO no viaja (D133).
- Acciones de servicio en varios hosts a la vez (para eso están los snippets).
- mosh, reenvío del agente, claves FIDO, iOS, Google Play y F-Droid (como en la F11).
- La corrección 10b (R83-R85), que sigue pendiente.

**Versión prevista:** app **0.2.0** (`android-v0.2.0`; MINOR, por funcionalidad nueva, T68), después de publicar la 0.1.0 con el cierre de la F11. El escritorio no cambia de versión, porque mover el sondeo y las comprobaciones al núcleo no se ve desde fuera. No hay migración (sigue `user_version` 9) ni cambios de protocolo o de `formato`.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Núcleo | `crates/magi-nucleo` | Gana `flota/` y `scripts/sondeo.sh` (movidos del escritorio), `deliberacion/` (plan, comprobaciones con proveedores y consenso), `sftp_lectura.rs` (listar un directorio) y los destinos y estados de una ejecución en `snippets/`. `servicios/` (F13) no cambia |
| Puerta al móvil | `crates/magi-movil` (UniFFI `=0.32.2`) | Conexiones con gracia, flota, servicios (acciones, sudo y logs), ejecuciones, deliberación, registro y ajustes; ejemplo `servidor_pruebas` |
| App | Kotlin, Jetpack Compose y Material 3 (los de la F11) | Pantallas nuevas en `ui/flota`, `ui/servicios`, `ui/logs`, `ui/snippets`, `ui/resultados` y `ui/registro`. Lo que solo corre a la vista sigue el ciclo de vida (`ProcessLifecycleOwner` y `repeatOnLifecycle`) |
| Respuesta háptica | `LocalHapticFeedback` de Compose | Al empezar y al cumplirse «mantener pulsado» |
| Host remoto | `sh` POSIX, `timeout`, systemd 230+, journald, Docker 20.10+ o Podman 4+, Compose v2, `sudo` y el subsistema SFTP | Lo que ya piden la F2 (sondeo), la F6 (BALTHASAR-2 por SFTP) y la F13 |
| CI | `android.yml` | El job `humo` arranca `servidor_pruebas` en el anfitrión antes del emulador (R106) |

No entra ninguna dependencia nueva en el workspace: `magi-nucleo` pasa a usar `russh-sftp` (`=3.0.0`) y `globset`, que ya usa el escritorio. Los dos son Rust puro y compilan para Android con `cargo-ndk`.

### Estructura de carpetas (novedades)

```
crates/magi-nucleo/
├── scripts/sondeo.sh            movido de src/flota/, con <motores> (R125)
├── src/flota/                   movido de src/flota/: mod.rs (Sondeo, EstadoFlota, Culpable), lectura.rs
│                                (antes parser.rs), estado.rs (umbrales, culpables, tasa de red) y fallos
├── src/deliberacion/            deliberacion.rs pasa a carpeta: plan.rs (PlanDeliberado), comprobaciones.rs
│                                (MELCHIOR-1, BALTHASAR-2, CASPER-3 con proveedores), consenso.rs
├── src/sftp_lectura.rs          listar un directorio por SFTP (BALTHASAR-2 en el móvil)
├── src/snippets/                + destinos.rs (resolver) y ejecucion.rs (estados por host y resultado)
└── tests/                       flota_lectura.rs, deliberacion_comprobaciones.rs, snippets_ejecucion.rs;
                                 fijos/ con las salidas grabadas y los binarios falsos de la F13 (compartidos)
crates/magi-movil/
├── src/conexiones.rs            antes dentro de gestor.rs: una por host, usos, gracia de 60 s, vigilante,
│                                apertura interactiva o automática
├── src/flota.rs                 sondeo (semáforo 8, 5 s), SONDEOS y lista completa
├── src/servicios.rs             toda la flota, listado, detalle, consumo, sudo, acciones y sesiones con orden
├── src/logs.rs                  suscripciones, anillo (10 000 líneas · 8 MiB) y ventanas
├── src/ejecuciones.rs           snippets en varios hosts, salida, cancelar, repetir y limpiar
├── src/deliberacion.rs          proveedores del móvil (sondeo, SFTP, «solo en el escritorio») y DELIBERACIONES
├── src/registro.rs · src/ajustes.rs   consultas con filtros · AjustesMovil
├── examples/servidor_pruebas.rs antes servidor_pruebas_tunel: + hosts con systemctl, journalctl, docker,
│                                sudo y nginx falsos, y un directorio de backups
└── tests/                       flota_e2e.rs, servicios_movil_e2e.rs, snippets_movil_e2e.rs,
                                 deliberacion_movil_e2e.rs (y tunel_local_e2e.rs de la 11b)
src/flota/ · src/deliberacion/   escritorio: orquestación, diálogo y proveedores; usan el núcleo
android/app/src/main/kotlin/org/cuatrodetres/magi/
├── ui/flota/ (sustituye a ui/hosts/) · ui/servicios/ · ui/logs/ · ui/snippets/ · ui/resultados/ · ui/registro/
├── ui/dialogos/                 + Deliberacion, ContrasenaSudo, Confirmacion y BotonMantener
├── ui/ficha/                    + bloque FLOTA, verificaciones, snippet al conectar, SERVICIOS y SNIPPETS
├── ui/ajustes/                  + Flota, Servicios y Deliberación
├── navegacion/                  barra de cinco, riel y dos paneles en Flota, Servicios, Snippets y Registro
└── plataforma/                  Visibilidad (ciclo de vida) · ServicioSesiones cuenta acciones y ejecuciones
android/app/src/androidTest/     PruebaFlota, PruebaServicios y PruebaSnippets (con el servidor de pruebas)
.github/workflows/android.yml    job humo: arranca servidor_pruebas antes del emulador (R106)
docs/fase14/                     informe, prompt, checklist, implementación y revisión adversarial
```

### Diagrama de arquitectura: repositorio

```
 repositorio 4d3-org/magi (workspace de Cargo): lo que cambia en la F14

 ┌───────────────────────────────┐       ┌──────────────────────────────────────┐      ┌────────────────────────────┐
 │ magi-ssh (raíz)               │ usa   │ crates/magi-nucleo                   │ usa  │ crates/magi-movil (cdylib) │
 │ src/flota/: orquesta el       │──────▶│ flota/ · scripts/sondeo.sh     (F14) │◀─────│ UniFFI 0.32.2 · tokio      │
 │   sondeo (Ejecutar o conexión │       │ deliberacion/: plan, consenso y      │      │ conexiones con gracia      │
 │   efímera) con el núcleo      │       │   comprobaciones (proveedores) (F14) │      │ flota · servicios · logs   │
 │ src/deliberacion/: diálogo    │       │ sftp_lectura.rs: listar        (F14) │      │ ejecuciones · deliberación │
 │   y proveedores propios       │       │ snippets/: destinos y estados  (F14) │      │ registro · ajustes         │
 │ servidor y protocolo: igual   │       │ servicios/ · servicios.sh      (F13) │      │ examples/servidor_pruebas  │
 │ sin cambios visibles (T85)    │       │ almacén · sincronización · SSH       │      │ tests/*_e2e (T95)          │
 │                               │       │ túneles · terminal · registro        │      │                            │
 └───────────────────────────────┘       └──────────────────────────────────────┘      └─────────────┬──────────────┘
                                                                                                     │
                                                                                                     ▼ bindings + .so
                                            android/: ui/flota · servicios · logs · snippets · resultados · registro
```

### Diagrama de arquitectura: el móvil en marcha

```
 móvil (Android 11+)
 ┌──────────────────────────────────────────────────────────────────────┐
 │ app Kotlin + Compose                                                 │
 │  FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS                        │
 │  MÁS: snippets · resultados · registro · ajustes                     │
 │  diálogos: sudo · confirmación · deliberación MAGI                   │
 │  plataforma: Visibilidad · ServicioSesiones · BotonMantener          │
 └──────┬───────────────────────────────────────────────▲───────────────┘
        │ llamadas suspend (UniFFI + JNA)               │ OyenteNucleo: listas completas (flota, sesiones,
        │                                               │ túneles, acciones, ejecuciones), avisos de logs,
        ▼                                               │ pantallas y diálogos
 ┌──────────────────────────────────────────────────────┴───────────────┐
 │ libmagi_movil.so                                                     │
 │  magi-movil: conexiones (una por host, gracia de 60 s, vigilante)    │  exec de sondeo.sh y servicios.sh,
 │   flota (sondeo, semáforo 8) · servicios (acciones, sudo,            ├───────────────────────────▶ hosts
 │   anillo de logs) · ejecuciones · deliberación · registro            │  snippets, SFTP (listar) y PTY
 │  magi-nucleo: sondeo · servicios · snippets · deliberación · SFTP    │
 │  magi.db (user_version 9): SONDEOS, DELIBERACIONES, REGISTRO         │
 │  known_hosts · LlaveroMovil · clave del móvil en el Keystore (F11)   │
 └──────────────────────────────────────────────────────────────────────┘

 Servicio en primer plano mientras hay sesiones, túneles, acciones o ejecuciones; sondeo,
 refrescos y logs solo con su pantalla a la vista (D266, D279).
```

**Reparto**

- **El núcleo decide; las plataformas ejecutan.** Qué acción se permite, qué exige deliberación, cómo se lee la salida de un host, los estados de Flota, las variables de un snippet y el consenso viven en `magi-nucleo`. Kotlin no repite nada de eso (T85, T114).
- **`magi-movil` hace en el móvil lo que el servidor de sesiones hace en el escritorio:** ejecuta las acciones, los snippets y los logs, guarda lo que está en curso y anota en REGISTRO por su hilo escritor (T26).
- **La app recibe listas completas y ventanas, nunca bytes del host** (T13, T27, T89). Los logs no cruzan enteros la frontera: la app pide la ventana que pinta (T117).
- **El escritorio solo cambia por dentro.** `src/flota/` y `src/deliberacion/` usan el sondeo y las comprobaciones del núcleo con sus propios proveedores (el `Ejecutar` del servidor, su SFTP y `sh -c` en local), y sus pruebas e instantáneas no cambian.

---

## 3. Modelo de Datos

**Sin tablas nuevas ni migración 10.** El móvil ya tiene el esquema completo (`user_version` 9, F11-F13). Lo nuevo es lo que escribe: sus propios sondeos, deliberaciones y entradas de REGISTRO, y dos campos propios de SNIPPETS. Nada de eso viaja (D133).

### Diagrama E-R (lo que usa el móvil)

```
 magi.db del móvil (user_version 9, sin migración nueva)
 ┌─ llega por la sincronización: solo lectura (la escribe la pasada) ─────────────────────────────────────────┐
 │ HOSTS (servicios vigilados, snippet_al_conectar_id) ──1:1── VERIFICACIONES_HOST (salud · backup · tests)   │
 │   ├──< COMPROBACIONES_SERVICIO (objetivo_tipo, objetivo, comando): antes de recargar o reiniciar (F13)     │
 │   └──< SNIPPET_DESTINOS >── SNIPPETS (nombre, comando, crítico, timeout_seg, parar_al_fallo)               │
 │ El móvil las usa en Flota, Servicios, Snippets y la deliberación; no las edita (T87).                      │
 └──────┬─────────────────────────────────────────────────────────────────────────────────────────────────────┘
        │ lo escribe el móvil (no viaja, D133)
        ▼
 ┌────────────────────────┐  ┌──────────────────────────┐  ┌───────────────────────────┐  ┌──────────────────────┐
 │ SONDEOS (F2, F13)      │  │ DELIBERACIONES (F6)      │  │ REGISTRO: tipos que       │  │ SNIPPETS (campos     │
 │ los del propio móvil   │  │ snippet o acción · hosts │  │ ya tenía el escritorio    │  │ propios, F10)        │
 │ 20 por host · métricas │  │ comprobaciones_json      │  │ sondeo_fallido            │  │ usado_veces          │
 │ servicios_json         │  │ usuario «móvil (…)»      │  │ snippet_ejecutado         │  │ ultimo_uso_en        │
 │ fallos_json            │  │ ejecucion_resultado      │  │ deliberacion_*            │  │ sin subir versión    │
 │                        │  │                          │  │ servicio_accion           │  │                      │
 └────────────────────────┘  └──────────────────────────┘  └───────────────────────────┘  └──────────────────────┘
 En memoria de magi-movil (sin tabla): conexiones con gracia · acciones en curso · suscripciones de log
 (anillo) · ejecuciones (1 h) · deliberaciones en curso · contraseña de sudo por host (Zeroizing, 5 min).
```

### Qué lee y qué escribe el móvil

| Dato | Dónde | En el móvil |
|---|---|---|
| `HOSTS.servicios`, `HOSTS.snippet_al_conectar_id`, VERIFICACIONES_HOST, SNIPPETS, SNIPPET_DESTINOS, COMPROBACIONES_SERVICIO | `magi.db` | Solo lectura; los escribe la pasada en modo lector (F11) |
| SONDEOS | `magi.db` | Los sondeos del propio móvil: 20 por host, con `fallos_json`. Un sondeo nunca cambia `HOSTS.ultimo_estado` (F2) |
| DELIBERACIONES | `magi.db` | Las del móvil, con `usuario` = «móvil (<nombre del equipo>)» (D282); `ejecucion_resultado` al terminar |
| REGISTRO | `magi.db` | Además de los de la F11, tipos que ya tenía el escritorio: `sondeo_fallido`, `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada` y `servicio_accion`. Nunca la salida de una orden (T44) |
| `SNIPPETS.usado_veces` y `ultimo_uso_en` | `magi.db` | Campos propios del equipo (F10): no suben versión ni viajan |
| Ajustes (refrescos, umbrales, sudo, contenedores, deliberación) | DataStore | La app (§5.4); nada secreto |
| Contraseña de sudo | memoria de `magi-movil` | `Zeroizing`, por host, 5 min sin uso como mucho; nunca en disco (T105) |
| Listados, anillo de logs, acciones, ejecuciones, deliberaciones en curso y conexiones | memoria de `magi-movil` | Se pierden si Android cierra MAGI |

### 3.1 Estructuras en memoria de `magi-movil`

| Estructura | Campos | Notas |
|---|---|---|
| Conexión | `host_uuid`, `usos`, `gracia_hasta`, vigilante, `interactiva` | Una por host para todo (D277). Sin usos, queda en gracia 60 s y se cierra |
| Acción de servicio | `id`, `host_uuid`, `objetivo`, `accion`, `fase` (`comprobando`, `ejecutando`, `esperando`), `deliberacion_id`, `desde` | Se difunde en `OyenteNucleo.acciones_servicio(lista)`. Una a la vez por objetivo |
| Suscripción de log | `id`, `host_uuid`, `objetivo`, canal, anillo, `omitidas`, `estado` | Una por objetivo; el anillo tiene 10 000 líneas y 8 MiB como mucho (D278) |
| Ejecución | `id`, snippet (uuid y nombre), comando resuelto, hosts (estado, código, duración, stdout, stderr, `truncada`), `timeout_seg`, `parar_al_fallo`, deliberación, `creada_en`, `terminada_en` | Se difunde en `OyenteNucleo.ejecuciones(lista)` sin la salida. Se guarda 1 h tras terminar |
| Deliberación en curso | `id`, `plan`, resultado por host y comprobación (veredicto, dato, ms), consenso, estado | Se resuelve en DELIBERACIONES. Una deliberación autoriza una sola acción (T62) |
| Contraseña de sudo | `host_uuid` → `Zeroizing<String>`, último uso | Se olvida a los 5 min sin uso, al bloquearse MAGI, si el host la rechaza o a mano (D268) |

### 3.2 Relaciones

- `SONDEOS.host_id → HOSTS.id` (CASCADE): un host borrado por la pasada se lleva sus sondeos del móvil.
- `DELIBERACIONES.snippet_id → SNIPPETS.id` (SET NULL), como en la F6. Las acciones de servicio van con `snippet_id` NULL, la acción en `accion` y el host en `hosts_json` (F13).
- `REGISTRO.host_id → HOSTS.id` (SET NULL): las entradas se quedan aunque el host desaparezca.
- VERIFICACIONES_HOST y COMPROBACIONES_SERVICIO llegan con su host por la sincronización. El móvil las lee al deliberar y al actuar, y nunca las escribe.
- Un host que la pasada quiere borrar con trabajo vivo en el móvil se aplaza (F11, §7.4). Desde esta fase, cuentan como trabajo vivo sus acciones en curso y sus ejecuciones.

### 3.3 Diagramas de estados

**Host en Flota (móvil)**

```
   [FRÍA] ○ ── sin sondeo guardado en el móvil
      │ abrir Flota · volver a ella · deslizar · cada 30 s a la vista (si el host se ve en la lista)
      ▼
  sondeando ◐ … ── 5 s · DNS/TCP · huella sin aceptar · sin clave del móvil ni contraseña guardada ──▶ [CAÍDA] ✕
      │                            (SONDEOS error; REGISTRO sondeo_fallido solo al pasar a CAÍDA)
      │ responde sin /proc/loadavg o sin systemctl ──▶ [ALCANZABLE] ● «sin métricas»
      ▼
  leído ── ¿umbral superado o vigilado no activo? ──sí──▶ [CARGA] ◐ con sus culpables
      │ no
      ▼
   [NOMINAL] ●                (⚠N va aparte: los fallos del sondeo no cambian el estado, F13)
  Hosts que no son ssh: [sin sondeo] con su último estado; nunca pasan a sondeando.
  Inválidas: FRÍA → NOMINAL sin sondear · sondear con la app en segundo plano · cambiar HOSTS.ultimo_estado.
```

**Conexión del móvil (gestor de `magi-movil`)**

```
   [sin conexión] ── algo la pide ──▶ abriendo
        abriendo ─ interactiva (conectar, activar un túnel, abrir los servicios de un host) ─▶ diálogos de huella
                   y contraseña (F11)
        abriendo ─ automática (sondeo, refresco, comprobación, ejecución) ─ haría falta un diálogo ─▶
                   [sin conexión] y error con instrucción
        abriendo ─ red, salto caído o 10 s ─▶ [sin conexión] y error con motivo
        abriendo ─▶ viva (usos = N ≥ 1)
   viva ─ termina el último uso ─▶ en_gracia (60 s)
   en_gracia ─ alguien la pide ─▶ viva (sin autenticar otra vez)
   en_gracia ─ pasan 60 s ─▶ cerrada ─▶ [sin conexión]
   viva · en_gracia ─ el host la cierra o la red cae (vigilante, 500 ms, T97) ─▶ caída: sesiones caídas,
        túneles caídos, acciones en error, logs «se cortó la conexión», hosts de ejecución en error ─▶ [sin conexión]
  Inválidas: dos conexiones vivas al mismo host · una apertura automática que abre un diálogo · reutilizar
             una conexión caída.
```

**Acción de servicio (móvil)**

```
  (app) botón de una acción en el detalle
      ¿permitida en ese estado (F13 §7.2)? ─no─▶ no hay botón
      ¿sudo con contraseña y no está en memoria? ─sí─▶ [SUDO] ─ cancelar o 3 rechazos ─▶ (fin, sin acción)
      ¿deja algo parado? ─sí─▶ [DELIBERACIÓN MAGI] ─ cancelar ─▶ (fin) · mantener 1 s ─▶ sigue
                         └no─▶ [CONFIRMACIÓN] ─ cancelar ─▶ (fin) · confirmar ─▶ sigue
  (magi-movil) servicio_accion ─ inválida, deliberación que no vale u otra acción en curso ─▶ error (fin)
      ─▶ comprobando (si el objetivo tiene comprobación) ─ falla ─▶ rechazada_configuracion (fin)
      ─▶ ejecutando ─ la orden falla ─▶ fallido (fin)
      ─▶ esperando ─ estado esperado ─▶ hecho (fin)
                   ─ failed, o el contenedor no queda en marcha ─▶ fallido (fin, con las últimas líneas)
                   ─ espera_seg sin estado final ─▶ sin_confirmar (fin; la orden sigue en el host)
      cualquier fase ─ conexión caída o plazo de magi-movil ─▶ error (fin)
      Android cierra MAGI ─▶ se pierde (la orden puede seguir en el host; aviso al volver)
  Inválidas: ejecutar sin deliberación cuando hace falta · dos acciones a la vez sobre el mismo objetivo ·
             salir de un estado final
```

Los estados de una unidad de systemd y de un contenedor en el host, con las acciones que MAGI ofrece en cada uno, son los de la F13 (§3.8 de su informe).

**Suscripción de log (móvil)**

```
  (inicio) ─ LOGS ─▶ abriendo ─▶ siguiendo
  siguiendo ─ desplazarse hacia arriba · PAUSA ─▶ en_pausa (las líneas siguen entrando en el anillo)
  en_pausa ─ AL FINAL · PAUSA ─▶ siguiendo
  siguiendo · en_pausa ─ salir de la pantalla · MAGI pasa a segundo plano ─▶ parada (las líneas se quedan)
  parada ─ SEGUIR OTRA VEZ ─▶ abriendo (anillo vacío y otras 200 líneas)
  abriendo · siguiendo · en_pausa ─ fin del proceso · conexión caída · sin permiso ─▶ terminada (con motivo)
  Inválidas: seguir con la pantalla fuera de la vista · dos suscripciones al mismo objetivo (la nueva
             sustituye a la anterior) · guardar líneas en disco
```

**Contraseña de sudo (móvil, por host)**

```
  ninguna ─ una acción la necesita · «Dar contraseña de sudo» ─▶ pedida
  pedida ─ servicios.sh sudo: ok ─▶ en_memoria
  pedida ─ rechazada (hasta 3 intentos) ─▶ pedida · 3 rechazos o cancelar ─▶ ninguna
  pedida ─ sin_sudo ─▶ ninguna («usuario no puede usar sudo en web-01»)
  en_memoria ─ una acción la usa ─▶ en_memoria (vuelven a contar los 5 min; los refrescos no cuentan)
  en_memoria ─ 5 min sin uso · MAGI se bloquea · el host la rechaza · «Olvidar» ─▶ ninguna (se borra)
  Inválidas: escribirla en LlaveroMovil, DataStore, disco, logcat o REGISTRO · mandarla a otro host
```

**Deliberación (móvil)**

```
  acción o ejecución que la requiere
       ▼
   comprobando ◐ (todas en paralelo, 2 s cada una)
       ├─ todas aprueban (o ninguna activa) ─▶ aprobada ─ mantener 1 s ─▶ lanzada [DELIBERACIONES aprobada]
       │                                         └─ cancelar ─▶ cancelada
       └─ alguna rechaza (también CASPER-3 «solo en el escritorio») ─▶ bloqueada
              bloqueada ─ FORZAR + motivo ≥ 10 ─▶ forzada ─ mantener 1 s ─▶ lanzada [forzada]
              bloqueada · forzada ─ cancelar ─▶ cancelada [DELIBERACIONES cancelada · deliberacion_cancelada]
   lanzada ─▶ al terminar la acción o la ejecución: ejecucion_resultado y deliberacion_aprobada | _forzada
  Inválidas: bloqueada → lanzada sin forzar · lanzar con un toque · forzar con un motivo de menos de
             10 caracteres · usar una deliberación dos veces (repetir delibera de nuevo)
```

**Ejecución de un snippet y cada host (móvil)**

```
 Ejecución:  en_curso ─ todos los hosts en estado final ─▶ terminada · ─ cancelar ─▶ cancelada
             terminada · cancelada ─ 1 h o LIMPIAR ─▶ (se olvida)
 Host:  en_cola ─ turno (semáforo 8) ─▶ conectando ─ conexión automática ─▶ ejecutando
           │                             └─ error (huella, sin clave ni contraseña, red) ─▶ error
           └─ parar al fallo · cancelar ─▶ omitido
        ejecutando ─ código 0 ─▶ ok · código ≠ 0 ─▶ fallo (con parar al fallo, los que esperan ─▶ omitido)
                   ─ timeout ─▶ error «tiempo agotado» · cancelar ─▶ cancelado
        ok · fallo · error · cancelado · omitido son finales; REGISTRO snippet_ejecutado en los cuatro primeros
  Inválidas: salir de un estado final · ejecutar dos veces el mismo host en una ejecución
```

**Servicio en primer plano (ampliado)**

```
   parado ─ primera sesión, túnel, acción o ejecución ─▶ en_marcha («MAGI · 2 sesiones · 1 acción»)
   en_marcha ─ cambia algún número ─▶ en_marcha (actualiza la notificación)
   en_marcha ─ termina lo último (sesiones, túneles, acciones y ejecuciones) ─▶ parado
   en_marcha ─ Android cierra MAGI ─▶ parado (al volver: aviso, también de acciones y ejecuciones)
  Inválidas: en_marcha solo por Flota, Servicios, logs o conexiones en gracia · acciones en la notificación (T6)
```

---

## 4. Flujos de Trabajo

### 4.1 Abrir Flota y sondear

```
  abrir MAGI · volver a Flota · deslizar hacia abajo · cada 30 s con Flota a la vista
      ▼ ¿la app está a la vista y en Flota? ─no─▶ nada (no se sondea en segundo plano)
  hosts a sondear: los ssh de la lista con el filtro aplicado; en el refresco automático, solo los de
  grupos desplegados
      ▼ por host (semáforo 8; si ese host ya se está sondeando, se salta)
  ¿conexión viva o en gracia? ─sí─▶ canal exec sobre ella
      ▼ no
  conexión automática (sin diálogos): huella conocida · clave del móvil · contraseña guardada
      ├─ huella desconocida o cambiada ─▶ CAÍDA «huella sin aceptar: conéctate una vez desde la ficha»
      ├─ ni la clave del móvil ni una contraseña guardada valen ─▶ CAÍDA «sin credenciales: conéctate una vez»
      └─ red, salto o 5 s ─▶ CAÍDA «no responde en 5 s»
      ▼
  exec sondeo.sh <motores> <vigilados…> ─▶ lectura del núcleo ─▶ SONDEOS (20 por host, con fallos_json)
      ├─ faltan LOADAVG o UPTIME ─▶ ALCANZABLE «sin métricas»
      └─ umbrales y vigilados ─▶ NOMINAL o CARGA (culpables)
      ▼ ¿pasa a CAÍDA? ─sí─▶ REGISTRO sondeo_fallido
  OyenteNucleo.flota(lista completa) ─▶ la fila cambia · la conexión queda en gracia 60 s
  deslizar, además ─▶ pasada de sincronización (F11)
```

### 4.2 De «Toda la flota» al servicio que falla

```
  SERVICIOS ─▶ TODA LA FLOTA (último sondeo del móvil de cada host; entrar no abre conexiones)
      ├─ host caído o sin sondeo ─▶ su línea («nas: CAÍDA hace 3 min» · «db-02: sin sondeo»)
      └─ fallo «✕ web-01 copia.service» ─▶ servicios de web-01 con copia.service abierto
  abrir el host ─▶ ¿conexión? ─no─▶ interactiva, porque lo pide el usuario (huella o contraseña con diálogo)
      │                            ─ error o 10 s ─▶ «no se llega a web-01» · REINTENTAR
      ▼
  servicios.sh listar (10 s) ─▶ secciones, vigilados y fallidos arriba
      ├─ sin systemd ─▶ abre en Contenedores; las secciones vacías no se pueden elegir
      ├─ sin permiso para el diario o para Docker ─▶ aviso con «Dar contraseña de sudo»
      └─ 10 s sin respuesta ─▶ se queda el listado anterior, con su hora
      ▼
  DETALLE de copia.service ─▶ botones de lo permitido: LOGS · ARRANCAR · REINICIAR
  refresco cada 10 s con la pantalla a la vista ─ «no responde» o «sin conexión» ─▶ se para · REINTENTAR
```

### 4.3 Acción sobre un servicio desde el móvil

```
  REINICIAR en app.service (web-01)
      ▼ preparar_accion ─▶ permitida · deja parado · sudo con contraseña · comprobación propia · ¿crítica?
  ¿sudo con contraseña y no está en memoria? ─sí─▶ [SUDO] ─▶ servicios.sh sudo
      ├─ rechazada ─▶ otra vez (3 intentos) ─▶ fin sin acción
      └─ sin_sudo ─▶ «usuario no puede usar sudo en web-01» (fin)
      ▼
  ¿deja algo parado? ─sí─▶ [DELIBERACIÓN MAGI] (§4.6) ─ cancelar ─▶ fin
      │                     └─ mantener 1 s (aprobada o forzada) ─▶ DELIBERACIONES
      └─no─▶ [CONFIRMACIÓN] «¿Recargar nginx.service en web-01? Antes: nginx -t» ─ cancelar ─▶ fin
      ▼
  servicio_accion ─▶ magi-movil: ¿deliberación válida y sin usar? ¿otra acción en curso? ─no─▶ error
      ▼ acciones_servicio(lista) con «◐ reiniciando» (antes del efecto, T40)
  servicios.sh accion … ─▶ comprobando ─ falla ─▶ rechazada_configuracion (con la salida del validador)
      ▼ ejecutando ─ código ≠ 0 ─▶ fallido (mensaje y últimas 20 líneas) · timeout ─▶ sin_confirmar
      ▼ esperando: a los 2 s, otra mirada ─▶ hecho ✓ · fallido ✕
  REGISTRO servicio_accion · ejecucion_resultado y deliberacion_* (hilo escritor) ─▶ [RESULTADO] ─▶ refresco
  la app pasa a segundo plano ─▶ la acción sigue (servicio en primer plano); al volver, el resultado espera
```

### 4.4 Logs en vivo

```
  LOGS en app.service (o en un timer, que sigue a su servicio, o en un contenedor)
      ▼ logs_seguir(200) ─▶ servicios.sh logs unidad app.service 200 (o contenedor <id> 200)
  líneas ─▶ anillo en magi-movil (10 000 líneas · 8 MiB; fuera las más viejas, con recuento)
      ▼ como mucho cada 100 ms ─▶ OyenteNucleo.logs(total, nuevas, omitidas, estado)
  la app pide logs_ventana(texto, nivel, al final, 60) ─▶ las líneas que pinta, ya filtradas
      ├─ desplazarse hacia arriba ─▶ en pausa: la vista no se mueve · «37 nuevas» · AL FINAL
      ├─ diario sin permiso ─▶ terminada sin_permiso: «Dar contraseña de sudo»
      ├─ A SESIÓN ─▶ sesión con orden (journalctl -fu o docker logs -f) y el panel sigue
      └─ conexión caída ─▶ terminada «se cortó la conexión» (las líneas se quedan)
  salir de la pantalla o MAGI a segundo plano ─▶ logs_parar ─▶ fin de stdin ─▶ el script corta journalctl
      ─▶ «seguimiento parado · SEGUIR OTRA VEZ»
```

### 4.5 Ejecutar un snippet

```
  MÁS ─▶ SNIPPETS ─▶ un snippet (o FICHA ─▶ SNIPPETS: solo en ese host)
      ▼ destinos resueltos (hosts con alguna etiqueta destino ∪ hosts sueltos, sin repetir)
      │ 0 hosts ─▶ «el snippet no apunta a ningún host» (fin)
  [EJECUTAR] casillas de hosts · variables · parar al primer fallo ─ cancelar ─▶ fin
      ▼ preparar_ejecucion (núcleo): variables escapadas
      │ vacía sin valor por defecto, o dentro de comillas o de un heredoc ─▶ error en su campo
  ¿crítico, más de un host o algún host con verificaciones? ─sí─▶ [DELIBERACIÓN MAGI] ─ cancelar ─▶ fin
      │                                                          └─ mantener 1 s ─▶ DELIBERACIONES
      └─no─▶ CONTINUAR en EJECUTAR lanza (dos toques, T6)
      ▼
  ejecutar_snippet ─▶ magi-movil: ejecución en_curso · usado_veces +1 · RESULTADOS
      ▼ por host (semáforo 8): conexión automática ─ error ─▶ error con instrucción
  exec(comando) · stdout y stderr hasta 1 MiB cada uno · timeout del snippet
      ─▶ ok · fallo (con parar al fallo, los que esperan ─▶ omitido) · error
      ▼ REGISTRO snippet_ejecutado por host
  todos en estado final ─▶ terminada · si hubo deliberación: deliberacion_aprobada | _forzada y ejecucion_resultado
  un host ─▶ SALIDA (stdout · stderr, sin secuencias de escape) · COPIAR · REPETIR (delibera otra vez) · LIMPIAR
```

### 4.6 Deliberación en el móvil

```
  hosts H y sus verificaciones activas (VERIFICACIONES_HOST, llegadas por la sincronización)
      ▼ todo en paralelo; cada comprobación con 2 s
  MELCHIOR-1 salud:  último sondeo del móvil ¿NOMINAL y de menos de 5 min? ─sí─▶ aprueba
                       ├─ viejo o sin sondeo ─▶ sondear (automático) ─▶ NOMINAL ─▶ aprueba · otro ─▶ rechaza
                       ├─ CARGA · ALCANZABLE · CAÍDA ─▶ rechaza con el dato
                       └─ en una acción de servicio, sin contar el propio objetivo (F13)
  BALTHASAR-2 backup: SFTP (sftp_lectura) ─▶ listar backup_ruta ─▶ el más reciente que casa con backup_patron
                       ├─ de menos de backup_horas (24) ─▶ aprueba «hace 3 h»
                       ├─ más viejo ─▶ rechaza «último backup hace 31 h»
                       └─ vacío · sin acceso · 2 s ─▶ rechaza con el motivo
  CASPER-3 tests:      no se puede hacer en el móvil ─▶ rechaza «solo en el escritorio»
      ▼
  consenso = unanimidad de las activas ─sí─▶ APROBADO: «mantén pulsado para ejecutar»
      └─no─▶ BLOQUEADO ─ FORZAR ─▶ motivo de 10 caracteres o más ─▶ FORZADO: «mantén pulsado para ejecutar»
  al resolverse ─▶ DELIBERACIONES (resultado, bloqueada, motivo, comprobaciones, usuario «móvil (…)»)
```

### 4.7 Snippet al conectar

```
  conectar o reconectar una sesión a web-01 (no las sesiones con orden)
      ▼ ¿HOSTS.snippet_al_conectar_id? ─no─▶ nada
      ▼ sí ─▶ ¿es crítico o tiene variables? ─sí─▶ aviso en la sesión «snippet al conectar no válido» (no se escribe)
      ▼ no
  tras arrancar el shell ─▶ se escribe «comando + salto de línea» en la sesión, sin deliberar (como en la F6)
```

### 4.8 Diagrama de secuencia: reiniciar un servicio con sudo y deliberación forzada

```
 Compose                     magi-movil                                         web-01 (SSH)
   │─preparar_accion(reiniciar)──▶│                                                   │
   │◀── delibera · sudo con clave │                                                   │
   │ diálogo sudo                 │                                                   │
   │─sudo_comprobar(…)───────────▶│─servicios.sh sudo (contraseña por stdin)─────────▶│
   │                              │◀──────────────────────────────────── MAGI_SUDO=ok │
   │◀────── ok · en memoria 5 min │                                                   │
   │─deliberar(Servicio)─────────▶│─sondeo.sh · SFTP: listar (≤ 2 s)─────────────────▶│
   │                              │◀───────────────────────────── métricas · ficheros │
   │◀─── 2/3 BLOQUEADO (CASPER-3) │                                                   │
   │ forzar · motivo              │                                                   │
   │ mantener 1 s                 │                                                   │
   │─resolver(forzar)────────────▶│ DELIBERACIONES (escritor)                         │
   │─servicio_accion(…)──────────▶│                                                   │
   │◀───────── acciones[en curso] │─servicios.sh accion … reiniciar──────────────────▶│
   │                              │◀──────────────────────────── MAGI_COMPROBACION=ok │
   │                              │◀───────────────── MAGI_RESULTADO=hecho active 2,3 │
   │                              │ REGISTRO servicio_accion · ejecucion_resultado    │
   │◀─── ResultadoServicio(hecho) │                                                   │
   │─servicios_listar────────────▶│─servicios.sh listar──────────────────────────────▶│
```

1. `REINICIAR` en `app.service`. `preparar_accion` dice que la acción delibera y que el usuario necesita sudo con contraseña. No hay ninguna en memoria, así que la app la pide y `magi-movil` la comprueba antes de nada.
2. Reiniciar deja el servicio parado un momento, así que hay deliberación. MELCHIOR-1 mira la salud del host sin contar `app.service`; BALTHASAR-2 lista el directorio de backups por SFTP. CASPER-3 está activo en ese host y en el móvil rechaza: queda BLOQUEADO.
3. Hector fuerza con un motivo y mantiene pulsado 1 s. `magi-movil` guarda la deliberación por su hilo escritor.
4. `magi-movil` valida la deliberación, difunde la acción en curso y ejecuta `servicios.sh accion` con la contraseña por la entrada estándar. El script pasa la comprobación propia, reinicia y, a los 2 s, vuelve a mirar: `active`.
5. `magi-movil` anota `servicio_accion` y `ejecucion_resultado`, devuelve el resultado y la app refresca el listado.

### 4.9 Diagrama de secuencia: logs en vivo con ventanas

```
 Compose (Logs)              magi-movil                                         web-01 (SSH)
   │─logs_seguir(app, 200)───────▶│─servicios.sh logs unidad app.service 200─────────▶│
   │                              │◀───────────────── 200 líneas (journalctl -o json) │
   │                              │ anillo: 10 000 líneas · 8 MiB                     │
   │◀───── logs(200 · nuevas 200) │                                                   │
   │─logs_ventana(final, 60)─────▶│                                                   │
   │◀──────── 60 líneas filtradas │                                                   │
   │                              │◀────────────────────────────── líneas nuevas (-f) │
   │◀────── logs(214 · nuevas 14) │                                                   │
   │ pausa: no pide el final      │                                                   │
   │ sale de la pantalla          │                                                   │
   │─logs_parar──────────────────▶│─cierra el canal: fin de stdin────────────────────▶│
   │                              │ el script corta journalctl en el host             │
   │◀─────── logs(estado: parado) │                                                   │
```

1. `LOGS` en `app.service` pide las últimas 200 líneas y el seguimiento.
2. Las líneas entran en el anillo de `magi-movil`. La app recibe un aviso (como mucho cada 100 ms) y pide solo la ventana que pinta, ya filtrada.
3. En pausa, la app deja de pedir el final: el anillo sigue llenándose y la cabecera cuenta las nuevas.
4. Al salir de la pantalla, `logs_parar` cierra el canal. El script ve el fin de su entrada estándar y corta `journalctl`.

### 4.10 Diagrama de secuencia: un snippet en dos hosts

```
 Compose                     magi-movil                                        web-01 · web-02
   │─preparar_ejecucion(2 hosts)─▶│                                                   │
   │◀── delibera (más de un host) │                                                   │
   │─deliberar(Snippet)──────────▶│─sondeo.sh ×2 · SFTP: listar ×2 (≤ 2 s)───────────▶│
   │                              │◀───────────────────────────── métricas · ficheros │
   │◀─────────────── 4/4 APROBADO │                                                   │
   │ mantener 1 s                 │                                                   │
   │─resolver(aprobar)───────────▶│ DELIBERACIONES (escritor)                         │
   │─ejecutar_snippet(…)─────────▶│                                                   │
   │◀────── ejecuciones[en curso] │─exec ×2 (semáforo 8)─────────────────────────────▶│
   │                              │◀─────────────────────────────── exit-status 0 · 1 │
   │                              │ snippet_ejecutado ×2 · deliberacion_aprobada      │
   │                              │ ejecucion_resultado = parcial                     │
   │◀── ejecuciones[ok · fallo 1] │                                                   │
   │─salida(web-02)──────────────▶│                                                   │
   │◀───────── salida sin escapes │                                                   │
```

1. «reiniciar nginx» va a dos hosts, así que delibera aunque no sea crítico (T41).
2. MELCHIOR-1 y BALTHASAR-2 pasan en los dos hosts y ninguno tiene CASPER-3: 4/4, APROBADO. Hector mantiene pulsado 1 s.
3. `magi-movil` ejecuta en los dos hosts a la vez, guarda la salida y anota `snippet_ejecutado` por host. Al terminar anota `deliberacion_aprobada` y `ejecucion_resultado = parcial`, porque uno falló.
4. Resultados enseña los dos hosts. La salida de `web-02` se pide aparte y llega sin secuencias de escape.

---

## 5. Interfaz del Núcleo, Workflows y Ajustes

En este proyecto, la «API» del móvil es la interfaz que `magi-movil` expone a Kotlin con UniFFI. Todo lo de la F11 sigue igual y lo nuevo se añade sin tocarlo. Las funciones de red son `suspend` en Kotlin. Los tipos de servicios (`Listado`, `Detalle`, `InfoAccion`, `ResultadoServicio`, `OrdenPestana`) y de deliberación (`PlanDeliberado`) son los del núcleo.

### 5.1 Funciones nuevas del núcleo (UniFFI)

| Recurso | Función | Descripción |
|---|---|---|
| Ajustes | `aplicar_ajustes(ajustes)` | Pasa al núcleo los ajustes del móvil (§5.4). Acota los valores fuera de rango y devuelve los avisos |
| Flota | `flota(filtro)` | Hosts con su estado de Flota (`fria`, `sondeando`, `caida`, `alcanzable`, `carga`, `nominal` o `sin_sondeo`), culpables, resumen de métricas, número de fallos, sesiones abiertas y hora del último sondeo. Mismo filtro que la F11 |
| Flota | `sondear(hosts)` | Sondea esos hosts (con la lista vacía, todos los `ssh`) con semáforo de 8 y 5 s por host. Un host que ya se está sondeando se salta. Los resultados llegan por `OyenteNucleo.flota(lista)` |
| Flota | `ficha_host(uuid)` (ampliada) | Lo de la F11, más `flota` (métricas completas, vigilados con su estado, fallos con nombres, hora y duración del sondeo), `verificaciones` y `snippet_al_conectar` |
| Servicios | `servicios_toda_la_flota()` | Fallos de todos los hosts según el último sondeo del móvil, con su hora, y los hosts caídos o sin sondeo. No abre conexiones |
| Servicios | `servicios_listar(host, refresco)` | `servicios.sh listar` (10 s) con los motores de los ajustes. Devuelve el `Listado` de la F13 y si hay contraseña de sudo en memoria. Con `refresco = false` (lo pide el usuario) puede abrir la conexión con diálogos; con `refresco = true` es automática y no dialoga. Si ya hay una petición en vuelo para ese host, espera a esa y devuelve su resultado |
| Servicios | `servicios_consumo(host)` | CPU y memoria de los contenedores en marcha (suborden `consumo`) |
| Servicios | `servicios_detalle(host, objetivo)` | Propiedades, dependencias y texto de la unidad o de `inspect` |
| Servicios | `preparar_accion(host, objetivo, accion)` | `InfoAccion` del núcleo: si se permite (y, si no, por qué), si delibera, si hace falta la contraseña de sudo, qué comprobación se aplicará y, si la unidad es crítica, los hosts que van a través de ese host |
| Servicios | `servicio_accion(host, objetivo, accion, deliberacion)` | Ejecuta la acción (§4.3). Devuelve el `ResultadoServicio` cuando ya está anotado y lo difunde además por `OyenteNucleo.accion_servicio_hecha` |
| Servicios | `sudo_comprobar(host, contrasena)` · `sudo_olvidar(host)` | `servicios.sh sudo`: `ok` (queda en memoria), `rechazada` o `sin_sudo`. Olvidar la borra |
| Logs | `logs_seguir(host, objetivo, lineas)` | Abre la suscripción (`servicios.sh logs`) y devuelve su id. Hay una por objetivo |
| Logs | `logs_ventana(suscripcion, filtro)` | Hasta 500 líneas ya filtradas (por texto, por nivel o solo stderr), desde un número de línea o desde el final, con el total, las omitidas y el estado |
| Logs | `logs_parar(suscripcion)` | Cierra el canal (fin de stdin). Las líneas se quedan en el anillo hasta que se suelta la pantalla |
| Sesiones | `abrir_sesion_con_orden(host, orden)` | Sesión con PTY para una `OrdenPestana` del núcleo (`ShellContenedor`, `SeguirDiario` o `SeguirContenedor`). `magi-movil` arma la orden desde el tipo (T108) |
| Snippets | `snippets(filtro)` · `snippets_de_host(host)` · `snippet(uuid)` | La lista; los que tienen ese host entre sus destinos; y el detalle: comando, descripción, destinos resueltos, variables con su valor por defecto, crítico, timeout, parar al fallo y uso |
| Snippets | `preparar_ejecucion(snippet, hosts, variables)` | El núcleo sustituye las variables con escape, valida y dice si delibera. Devuelve los errores por campo |
| Snippets | `ejecutar_snippet(preparacion, parar_al_fallo, deliberacion)` | Lanza la ejecución (§4.5) y devuelve su id |
| Resultados | `ejecuciones()` · `salida_ejecucion(ejecucion, host)` | La lista, sin la salida (también llega por `OyenteNucleo.ejecuciones`), y la salida de un host, sin secuencias de escape |
| Resultados | `cancelar_ejecucion(id)` · `repetir_ejecucion(id)` · `limpiar_ejecuciones()` | Repetir devuelve una preparación nueva, que vuelve a deliberar si hace falta |
| Deliberación | `deliberar(plan)` | Las comprobaciones de §4.6 en paralelo (2 s cada una). Devuelve la tabla, el consenso y el estado |
| Deliberación | `resolver_deliberacion(id, decision)` | `aprobar`, `forzar(motivo)` o `cancelar`. Guarda la fila de DELIBERACIONES y devuelve su id para la acción o la ejecución |
| Registro | `registro(filtro, desde, cuantas)` · `registro_detalle(id)` | Una página de entradas (filtro por host, tipos y texto) y una entrada con las comprobaciones de su deliberación |

### 5.2 Retrollamadas nuevas

`CarpetaMovil`, `LlaveroMovil` y `FirmanteMovil` no cambian. `OyenteNucleo` gana cinco métodos:

| Método | Cuándo llega |
|---|---|
| `flota(lista)` | Lista completa de hosts de Flota, al empezar y al terminar cada sondeo (T27) |
| `acciones_servicio(lista)` | Lista completa de acciones en curso, en cada cambio |
| `accion_servicio_hecha(resultado)` | Al terminar una acción, aunque la pantalla que la pidió ya no esté |
| `logs(estado)` | Por suscripción, como mucho cada 100 ms: total, nuevas, omitidas y estado |
| `ejecuciones(lista)` | Lista completa de ejecuciones (sin la salida), en cada cambio |

### 5.3 Workflows

| Workflow | Cambio |
|---|---|
| `android.yml` | El job `humo` arranca `cargo run -p magi-movil --example servidor_pruebas` en el anfitrión antes del emulador y espera a que diga que está listo. Escucha en `127.0.0.1` y el emulador llega por `10.0.2.2` (R107). Así las pruebas instrumentadas de extremo a extremo dejan de omitirse en CI (R106). Al acabar, para el servidor |
| `ci.yml` | Sin cambios: ya ejecuta las pruebas de Rust de `magi-movil`, también las nuevas de extremo a extremo |
| `android-release.yml` | Sin cambios; la etiqueta será `android-v0.2.0` |

### 5.4 Ajustes del móvil

`config.toml` no viaja (D133) y el móvil no tiene uno. Lo que el escritorio guarda allí y el móvil necesita va a DataStore, con los mismos valores por defecto (D280), y llega al núcleo con `aplicar_ajustes`:

| Clave (DataStore) | Por defecto | Rango | En el escritorio |
|---|---|---|---|
| `flota_refresco_seg` | 30 | 0 (apagado) o 15-300 | Auto-refresco de Flota (F2), que allí va apagado por defecto |
| `umbral_carga_por_nucleo` | 1,0 | El del escritorio | `[flota.umbrales] carga_por_nucleo` |
| `umbral_memoria_pct` | 90 | El del escritorio | `[flota.umbrales] memoria_pct` |
| `umbral_disco_pct` | 90 | El del escritorio | `[flota.umbrales] disco_pct` |
| `servicios_refresco_seg` | 10 | 0 o 5-300 | `[servicios] refresco_seg` |
| `servicios_espera_seg` | 30 | 5-300 | `[servicios] espera_seg` |
| `servicios_contenedores` | `docker,podman` | `docker` y `podman` sin repetir, o vacío (sin contenedores) | `[servicios] contenedores` |
| `sudo_memoria_min` | 5 | 0-15 (0 = pedirla siempre) | `[servicios] sudo_memoria_min` |
| `deliberacion_backup_horas` | 24 | El del escritorio | `[deliberacion] backup_horas` |
| `deliberacion_salud_max_min` | 5 | El del escritorio | `[deliberacion] salud_max_min` |

Quedan fijos, con los valores del escritorio:

- 10 000 líneas en el anillo de logs y 200 iniciales;
- 2 s de plazo por comprobación y 10 caracteres de motivo;
- la lista de unidades críticas por defecto (F13 §7.9).

Un valor fuera de rango no impide arrancar: se acota y Ajustes dice por qué (D261 de la F13). El sondeo pasa `servicios_contenedores` como `<motores>`: con la lista vacía, `-`.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 [arranque] ─▶ BLOQUEO ─▶ (sin sincronizar) ASISTENTE ─▶ FLOTA (inicio)
 Barra inferior (compacta) · riel (plegable abierto o pantalla ancha):
 ├── FLOTA ── buscar · deslizar = sondear + sincronizar · ⇅ · ◐ N/M · cada 30 s a la vista
 │     ├── ▶ en una fila ─▶ TERMINAL (con el snippet al conectar)
 │     └── fila ─▶ FICHA ─┬─ FLOTA: métricas · vigilados · fallos
 │                        ├─ CONECTAR · OTRA SESIÓN ─▶ TERMINAL
 │                        ├─ SERVICIOS ─▶ servicios de ese host
 │                        ├─ SNIPPETS ─▶ snippets de ese host ─▶ EJECUTAR
 │                        ├─ túneles · instalar mi clave · olvidar contraseña · conflicto de huella (F11)
 │                        └─ verificaciones y snippet al conectar (solo lectura)
 ├── SESIONES ── lista (● ◐ ✕; también «(shell)» y «(logs)») ─▶ TERMINAL
 ├── TÚNELES ── como en la F11
 ├── SERVICIOS ─┬─ TODA LA FLOTA (fallos del último sondeo) ─ fallo ─▶ DETALLE en ese host
 │              └─ HOSTS ─▶ SERVICIOS DEL HOST ─ Servicios · Timers · Contenedores · filtrar
 │                     ├─ elemento ─▶ DETALLE (Estado · Dependencias · Unidad)
 │                     │     ├─ LOGS ─▶ PANEL DE LOGS ─ A SESIÓN ─▶ TERMINAL (logs)
 │                     │     ├─ SHELL (contenedor) ─▶ TERMINAL (shell)
 │                     │     └─ acción ─▶ [SUDO] ─▶ [CONFIRMACIÓN] o [DELIBERACIÓN MAGI] ─▶ [RESULTADO]
 │                     └─ ⋮ dar u olvidar la contraseña de sudo · reintentar
 └── MÁS ─┬─ SNIPPETS ─ snippet ─▶ EJECUTAR ─▶ ([DELIBERACIÓN MAGI]) ─▶ RESULTADOS
          ├─ RESULTADOS ─ ejecución ─ host ─▶ SALIDA (stdout · stderr · copiar)
          ├─ REGISTRO ─ filtros ─ entrada ─▶ DETALLE (comprobaciones de su deliberación)
          └─ AJUSTES ─ Sincronización · Clave del móvil · Terminal · Flota · Servicios · Deliberación ·
                       Batería · Borrar datos · Acerca de
 Desde cualquier conexión interactiva: [HUELLA NUEVA] · [HUELLA CAMBIADA] · [CONTRASEÑA] (F11)
```

### 6.1 Flota (compacta)

```
┌──────────────────────────────────────┐
│ MAGI · FLOTA      ◐ 3/7  ⇅ al día  ⋮ │
│ ┌──────────────────────────────────┐ │
│ │ buscar…                          │ │
│ └──────────────────────────────────┘ │
│ ▾ 4D3 · PRODUCCIÓN                   │
│ ● web-01         NOMINAL  ⚠3     ▶   │
│   2 sesiones · carga 0,4 · mem 61 %  │
│ ◐ db-01          CARGA    ⚠1     ▶   │
│   memoria 94 % · exportar.service    │
│ ✕ cache          CAÍDA           ▶   │
│   no responde en 5 s · hace 40 s     │
│ ◐ web-02         …               ▶   │
│   sondeando                          │
│ ▸ LABORATORIO (3)                    │
│ ▾ SIN GRUPO                          │
│ ○ vps-salto      FRÍA            ▶   │
│   usuario@salto.ejemplo.org:22       │
│ ○ win-01         sin sondeo          │
│   RDP · se abre en el ordenador      │
├──────────────────────────────────────┤
│FLOTA SESIONES 2 TÚNELES SERVICIOS MÁS│
└──────────────────────────────────────┘
```

La fila tiene dos líneas. La primera lleva el glifo y la palabra del estado, `⚠N` si hay fallos y `▶` para conectar. La segunda depende del estado:

- NOMINAL: las sesiones abiertas y la carga, la memoria y el disco.
- CARGA: los culpables, en ámbar.
- CAÍDA: el motivo.
- FRÍA: la dirección.

Las sesiones ya no van en el glifo, como en la F11, porque el glifo ahora es el de Flota. La cabecera enseña `◐ 3/7` mientras sondea y el indicador `⇅` de la sincronización. Los hosts de escritorio salen con su último estado y «se abre en el ordenador», sin sondear (F12). Sin inventario, Flota invita a configurar la sincronización, como hacía Hosts.

### 6.2 Ficha de host

```
┌──────────────────────────────────────┐
│ ← web-01              NOMINAL  ⚠3  ⋮ │
│ CARGA ███░░░░░░░░░░░░░   0,4 / 4     │
│ MEM   ██████████░░░░░░   61 %        │
│ DISCO ████░░░░░░░░░░░░   28 %        │
│ RED   ↓ 2,4 MB/s · ↑ 840 kB/s        │
│ ACTIVO 14 d 02 h                     │
│ sondeo hace 12 s en 640 ms           │
│ ──────────────────────────────────── │
│ VIGILADOS ● nginx  ● app  ● postgres │
│ FALLOS    ✕ copia.service            │
│           ✕ web-worker-1 · ⚠ panel   │
│ ──────────────────────────────────── │
│ [ CONECTAR ]   [ OTRA SESIÓN ]       │
│ [ SERVICIOS ]  [ SNIPPETS ]          │
│ ──────────────────────────────────── │
│ dirección    192.0.2.10:22           │
│ salto        vps-salto               │
│ al conectar  ver-estado              │
│ verificac.   salud · backup          │
│              /var/backups/app *.gz   │
│ TÚNELES · 1 activo                 ▾ │
└──────────────────────────────────────┘
```

El bloque FLOTA va arriba, porque es lo que se mira en una guardia; si no hay sondeo, dice «sin sondeo: desliza en Flota». Debajo están las acciones y, después, los datos de la F11. VERIFICACIONES y AL CONECTAR son de solo lectura: se editan en el escritorio. SERVICIOS abre los servicios de ese host. SNIPPETS abre los snippets que lo tienen entre sus destinos y prepara la ejecución solo en él.

### 6.3 Servicios de un host

```
┌──────────────────────────────────────┐
│ ← web-01 · systemd 252 · hace 4 s  ⋮ │
│ ▸SERVICIOS 142   TIMERS 9   CONT. 6  │
│ ┌──────────────────────────────────┐ │
│ │ filtrar…                         │ │
│ └──────────────────────────────────┘ │
│ ✕   copia.service       fallido      │
│ ★ ● nginx.service       activo       │
│ ★ ◐ app.service         reiniciando… │
│ ★ ● postgresql.service  activo       │
│ ──────────────────────────────────── │
│   ● cron.service        activo       │
│   ● fail2ban.service    activo       │
│   ● ssh.service         activo       │
│   ○ tareas.service      inactivo     │
│   ● ufw.service         activo       │
│   …                                  │
│ 142 servicios · 1 fallido · 3 vigil. │
└──────────────────────────────────────┘
```

Arriba van los fallidos (primero) y los vigilados (`★`), separados del resto por una línea, como en la F13. Las secciones se eligen con tres pestañas. Con un host sin systemd, abre directamente en Contenedores y las otras dos no se pueden elegir. Una acción en curso pone `◐` y la acción en la fila («reiniciando…»), venga de esta pantalla o de otra. El pie da los recuentos y, si falta algo, el motivo: «sin permiso para el diario: Dar contraseña de sudo». `⋮` tiene «Dar contraseña de sudo», «Olvidar contraseña de sudo» y «Reintentar».

### 6.4 Detalle y acciones

```
┌──────────────────────────────────────┐
│ ← app.service · web-01             ⋮ │
│ ▸ESTADO   DEPENDENCIAS   UNIDAD      │
│ ● activo (running) desde hace 3 h    │
│ Aplicación de pedidos                │
│ arranque   activado                  │
│ PID        812 · 180 MB · 6 tareas   │
│ fichero    /etc/systemd/system/app.… │
│ recargar   no admite                 │
│ vigilado   ★ sí                      │
│ antes de reiniciar:                  │
│   comprobación propia                │
│   /srv/app/bin/comprobar             │
│ ──────────────────────────────────── │
│ [ LOGS ]   [ REINICIAR ]  [ PARAR ]  │
│ [ DESACTIVAR ARRANQUE ]              │
└──────────────────────────────────────┘
```

- **Botones.** Solo aparecen los de las acciones permitidas en ese estado (F13 §7.2): un servicio activo sin `CanReload` no tiene RECARGAR. Un contenedor en marcha tiene LOGS, SHELL, REINICIAR y PARAR. En la cabecera de un proyecto Compose, los botones actúan sobre el proyecto entero.
- **Confirmación.** Para lo que no deja nada parado, una hoja con la frase entera: «¿Recargar nginx.service en web-01? Antes: nginx -t», con CANCELAR y RECARGAR.
- **Unidad crítica.** La confirmación o la deliberación añaden, en rojo, «⚑ puede cortar tu conexión con web-01» y los hosts que van a través de él.
- **Resultado.** Una hoja con:
  - el estado: hecho ✓, fallido ✕, sin confirmar (en ámbar, con «la orden sigue en el host: refresca para ver el estado»), rechazada por la configuración o error;
  - el mensaje;
  - las últimas líneas del diario o la salida del validador, con LOGS.

  Si la comprobación propia falla, dice «edítala en el escritorio».

### 6.5 Logs

```
┌──────────────────────────────────────┐
│ ← LOGS · app.service   ● siguiendo ⋮ │
│ web-01 · nivel: aviso o más          │
│ 1 214 líneas · 14 omitidas           │
├──────────────────────────────────────┤
│21:38:47 ! Lentitud: GET /api/infor-  │
│         mes tardó 2,4 s              │
│21:40:02 ✕ Exception in ASGI          │
│         application                  │
│21:40:02 ✕ sqlite3.OperationalError:  │
│         database is locked           │
│21:41:15 ! Reintento 1/3 de la cola   │
│         de correo                    │
│         … 14 líneas omitidas         │
│21:44:02 ✕ Plazo agotado al llamar a  │
│         api.ejemplo.org/pagos        │
├──────────────────────────────────────┤
│ [PAUSA] [FILTRO] [NIVEL] [A SESIÓN]  │
└──────────────────────────────────────┘
```

`✕` marca error o peor (prioridades 0-3), `!` aviso (4) y `·` el resto. En los contenedores, `!` marca stderr. Al desplazarse hacia arriba, el panel se pausa solo y la cabecera pasa a «⏸ en pausa · 37 nuevas»; AL FINAL vuelve y sigue. FILTRO busca por texto y NIVEL elige entre todo, info, aviso y error (en contenedores: todo o solo stderr). A SESIÓN abre el seguimiento completo, con sus colores, en una sesión. Al salir de la pantalla, las líneas se quedan con «seguimiento parado · SEGUIR OTRA VEZ» hasta que se cierra.

### 6.6 Deliberación MAGI (una acción)

```
┌──────────────────────────────────────┐
│ DELIBERACIÓN MAGI                    │
│ reiniciar app.service → web-01       │
│ antes: comprobación propia           │
│ ──────────────────────────────────── │
│ MELCHIOR-1 · salud                   │
│   ✓ NOMINAL hace 40 s                │
│     (sin contar app.service)         │
│ BALTHASAR-2 · backup < 24 h          │
│   ✓ hace 3 h · /var/backups/app      │
│ CASPER-3 · tests                     │
│   ✕ solo en el escritorio            │
│ ──────────────────────────────────── │
│ CONSENSO 2/3 ████████████░░░░░░      │
│ ▸ FORZADO: quedará registrado        │
│ motivo (mín. 10 caracteres):         │
│ ┌──────────────────────────────────┐ │
│ │ tests en verde en CI esta mañana │ │
│ └──────────────────────────────────┘ │
│ [ CANCELAR ]                         │
│ [ ▓▓▓▓▓▓▓░░░ MANTÉN PARA REINICIAR ] │
└──────────────────────────────────────┘
```

Ocupa toda la pantalla. Cada comprobación lleva el nombre MAGI junto a lo que comprueba y el dato que justifica su veredicto (F6). Mientras comprueba, cada una enseña `◐`. Con consenso, sale «▸ APROBADO» y el botón «MANTÉN PARA REINICIAR». Bloqueada, salen FORZAR… y CANCELAR; FORZAR abre el campo del motivo y, con 10 caracteres o más, pasa a FORZADO con el botón de mantener. El botón se rellena durante 1 s y vibra al empezar y al cumplirse. Soltarlo antes no hace nada y dice «mantén pulsado para ejecutar». El texto del botón nombra la acción («REINICIAR», «PARAR», «EJECUTAR»).

### 6.7 Deliberación de un snippet en varios hosts

```
┌──────────────────────────────────────┐
│ DELIBERACIÓN MAGI                    │
│ reiniciar nginx → 3 hosts            │
│ ──────────────────────────────────── │
│ HOST       SALUD    BACKUP   TESTS   │
│ web-01     ✓ 0,2 s  ✓ 3 h    ✕ escr. │
│ web-02     ✓ 0,1 s  ✕ 31 h   —       │
│ vps-salto  ✓ 0,4 s  —        —       │
│ ──────────────────────────────────── │
│ ✕ escr.: CASPER-3 solo corre en el   │
│   escritorio                         │
│ CONSENSO 4/6 ████████████░░░░░░      │
│ ▸ BLOQUEADO: se requiere unanimidad  │
│ [ CANCELAR ]          [ FORZAR… ]    │
└──────────────────────────────────────┘
```

Con varios hosts, las comprobaciones van en una tabla: hosts en filas y comprobaciones en columnas. `—` es una comprobación que ese host no tiene activa. Si la tabla no cabe, se desplaza en horizontal. Tocar una celda enseña su dato completo («último backup hace 31 h en /var/backups/pg»).

### 6.8 Ejecutar un snippet

```
┌──────────────────────────────────────┐
│ ← EJECUTAR · limpiar journald        │
│ journalctl --vacuum-time={{dias}}d   │
│ ──────────────────────────────────── │
│ HOSTS (4 de 5)                       │
│ [x] web-01        [x] web-02         │
│ [x] db-01         [ ] backup-nas     │
│ [x] vps-salto                        │
│ VARIABLES                            │
│ dias                                 │
│ ┌──────────────────────────────────┐ │
│ │ 7                                │ │
│ └──────────────────────────────────┘ │
│ [ ] parar al primer fallo            │
│ timeout 60 s · delibera: 4 hosts     │
│ [ CANCELAR ]         [ CONTINUAR ]   │
└──────────────────────────────────────┘
```

Los hosts salen ya resueltos desde los destinos (etiquetas y hosts sueltos) y van todos marcados, salvo si se llega desde la ficha de un host: entonces solo va ese. Cada variable tiene su campo con el valor por defecto, y una vacía sin defecto no deja continuar. «Parar al primer fallo» viene con el valor del snippet. La última línea dice si va a deliberar y por qué. CONTINUAR lanza, o abre la deliberación si hace falta.

### 6.9 Resultados

```
┌──────────────────────────────────────┐
│ ← RESULTADOS           1 en curso  ⋮ │
│ 11:42 reiniciar nginx   terminada ⚑  │
│       3 hosts · ✓ 2 · ✕ 1            │
│ 11:40 limpiar journald  en curso     │
│       4 hosts · ✓ 3 · ◐ 1            │
│ ──────────────────────────────────── │
│ reiniciar nginx · 11:42 · forzada    │
│ ● web-01     ok     0  1,2 s  212 B  │
│ ✕ web-02     fallo  1  0,8 s  1,4 kB │
│ ● vps-salto  ok     0  1,9 s  208 B  │
│ ──────────────────────────────────── │
│ stderr · web-02                      │
│ Job for nginx.service failed because │
│ the control process exited with      │
│ error code.                          │
│ [ COPIAR ]  [ REPETIR ]  [ LIMPIAR ] │
└──────────────────────────────────────┘
```

`⚑` marca una ejecución forzada. Los estados de cada host son los de la F6:

- `●` ok;
- `✕` fallo o error;
- `◐` conectando o ejecutando;
- `○` en cola;
- `–` omitido o cancelado.

Tocar un host enseña su salida (stdout y stderr en pestañas), sin secuencias de escape y con «… truncada» si pasó de 1 MiB. COPIAR la copia al portapapeles marcada como sensible. REPETIR prepara la misma ejecución, que vuelve a deliberar si hace falta. LIMPIAR quita las terminadas; si no, se olvidan 1 h después. CANCELAR, en una en curso, pide confirmación.

### 6.10 Registro

```
┌──────────────────────────────────────┐
│ ← REGISTRO        214 · servicios  ⋮ │
│ ┌──────────────────────────────────┐ │
│ │ buscar…                          │ │
│ └──────────────────────────────────┘ │
│ todos · servicios · snippets · más ▾ │
│ ──────────────────────────────────── │
│ 9 nov 21:52  servicio_accion       ✕ │
│   web-01 · reiniciar app.service     │
│   fallido tras 4,1 s                 │
│ 9 nov 21:51  deliberacion_forzada  ⚑ │
│   web-01 · reiniciar app.service     │
│ 9 nov 21:40  sondeo_fallido        ✕ │
│   cache · no responde en 5 s         │
│ 9 nov 20:12  conexion_abierta      ✓ │
│   web-01                             │
└──────────────────────────────────────┘
```

Enseña las entradas del propio móvil, de la más nueva a la más vieja, por páginas de 50. Los filtros por tipo son fichas deslizables (§7.10); el host se elige en `⋮` y el texto se busca en el detalle. Tocar una entrada enseña su detalle completo; en una `deliberacion_*`, también la tabla de comprobaciones con su motivo, tal y como quedó en DELIBERACIONES.

### 6.11 Plegable abierto: Servicios en dos paneles

```
┌─────────┬────────────────────────────┬───────────────────────────────────────────────────────┐
│ □ Flota │ SERVICIOS                  │ ← web-01 · systemd 252 · hace 4 s                  ⋮  │
│         │ TODA LA FLOTA · 4 fallos   │ ▸SERVICIOS 142    TIMERS 9    CONTENEDORES 6          │
│ □ Sesión│ ✕ web-01 copia.service     │ ✕   copia.service        fallido      hace 2 h        │
│         │ ✕ web-01 web-worker-1      │ ★ ● nginx.service        activo       hace 14 d       │
│ □ Túnel │ ⚠ web-01 panel             │ ★ ● app.service          activo       hace 3 h        │
│         │ ✕ db-01  exportar.service  │ ★ ● postgresql.service   activo       hace 14 d       │
│ ▣ Serv. │ nas: CAÍDA hace 3 min      │ ───────────────────────────────────────────────────── │
│         │ ────────────────────────── │   ● cron.service         activo       hace 14 d       │
│ □ Más   │ HOSTS                      │   ● fail2ban.service     activo       hace 14 d       │
│         │ ▸ web-01  ● NOMINAL  ⚠3    │   ● ssh.service          activo       hace 14 d       │
│         │   db-01   ◐ CARGA    ⚠1    │   ○ tareas.service       inactivo     hace 6 d        │
│         │   nas     ✕ CAÍDA          │ 142 servicios · 1 fallido · 3 vigilados               │
│         │   web-02  ● NOMINAL        │                                                       │
└─────────┴────────────────────────────┴───────────────────────────────────────────────────────┘
```

Con el plegable abierto o en pantalla ancha, Flota, Servicios, Snippets y Registro van en dos paneles: la lista a la izquierda y el detalle a la derecha. En Servicios, a la izquierda están «Toda la flota» y los hosts, y a la derecha los servicios del host elegido. El detalle y los logs se abren en el panel derecho, con `←` para volver. El terminal sigue a pantalla completa (D209). Al plegar o desplegar no se pierde nada: la pantalla abierta sigue abierta.

### Notas de UX y diseño

- **Cabina de MAGI sobre Material 3** (F11): los colores y la fuente de siempre. Ámbar para CARGA y para lo que avisa, verde para lo nominal y lo hecho, rojo para lo caído, lo fallido y las unidades críticas.
- **Glifo, texto y color a la vez (T5):**
  - Flota: `●` NOMINAL o ALCANZABLE, `◐` CARGA o sondeando, `✕` CAÍDA, `○` FRÍA o sin sondeo.
  - Servicios: los glifos de la F13 (`●` `○` `✕` `◐` `⚠` `★` `⚑`).
  - Deliberación: `✓` `✕` `—` `◐`.
- **Ninguna acción con un toque (T6, T115):** lo que no delibera pide confirmación (dos toques) y lo deliberado se lanza manteniendo pulsado 1 s. Sin acciones en la notificación.
- **Lo que no vale no sale:** sin botón para lo que no se puede hacer en ese estado. El detalle dice por qué («no admite recargar»).
- **Errores con instrucción:**
  - «huella sin aceptar: conéctate una vez desde la ficha»;
  - «sin credenciales: instala tu clave o guarda la contraseña»;
  - «sin permiso para Docker: añade usuario al grupo docker o da la contraseña de sudo».
- **Solo se trabaja a la vista:** al pasar a segundo plano se paran el refresco de Flota, el de Servicios y los logs. Las acciones y las ejecuciones siguen, y la notificación las cuenta.
- **Ejemplos:** los mockups usan direcciones de documentación (`192.0.2.x`) y `ejemplo.org`, como en el resto del proyecto.

---

## 7. Lógica de Negocio

### 7.1 Qué sondea el móvil y cuándo

```python
def hosts_a_sondear(lista, filtro, motivo):
    visibles = [h for h in lista if h.tipo == "ssh" and casa(h, filtro)]
    if motivo == "automatico":                       # cada flota_refresco_seg con Flota a la vista
        visibles = [h for h in visibles if not h.grupo_plegado]
    return [h for h in visibles if not h.sondeando]  # sin solapes por host

async def refresco_de_flota(pantalla):              # Kotlin: repeatOnLifecycle(RESUMED)
    while pantalla.a_la_vista() and ajustes.flota_refresco_seg > 0:
        nucleo.sondear([h.uuid for h in hosts_a_sondear(pantalla.lista, pantalla.filtro, "automatico")])
        await esperar(ajustes.flota_refresco_seg)    # se corta al salir de Flota o al pasar a segundo plano
```

Al abrir Flota, al volver a ella y al deslizar se sondea con `motivo = "manual"` (también los grupos plegados). Al deslizar se lanza además la pasada de sincronización. El estado sale de la misma función del núcleo que usa el escritorio (F2 §7.2), con los umbrales de los ajustes del móvil y la tasa de red calculada con el sondeo anterior del móvil. Un sondeo que llega cuando el host ya no está en el inventario se descarta.

### 7.2 Conexiones del móvil

```python
GRACIA_SEG = 60

def conexion(host, uso, interactiva):
    c = gestor.viva_o_en_gracia(host)
    if c:
        c.usos += 1
        c.gracia_hasta = None
        return c                                     # sin autenticar otra vez
    politica = Interactiva(dialogos) if interactiva else Automatica()
    c = abrir(host, politica)                        # huella → clave del móvil → contraseña guardada → diálogo
    gestor.guardar(host, c, vigilante=True)          # T97: un vigilante por conexión
    return c

def soltar(c):                                       # por identidad, en todas las ramas (T34)
    c.usos -= 1
    if c.usos == 0:
        c.gracia_hasta = ahora() + GRACIA_SEG        # pasado ese tiempo, se cierra si nadie la pide
```

- **Interactivas** son las que pide el usuario: conectar, activar un túnel, abrir los servicios de un host, una acción y los logs. Pueden abrir los diálogos de la F11.
- **Automáticas** son el sondeo, los refrescos, el consumo, las comprobaciones de la deliberación y las ejecuciones. Nunca abren un diálogo (T15, T49), y fallan con una instrucción:
  - `HuellaSinAceptar`: «conéctate una vez desde la ficha»;
  - `SinCredenciales`: «instala tu clave o guarda la contraseña»;
  - `NoResponde`.
- **Plazos:**
  - DNS, TCP y saludo: los 10 s de la F11;
  - sondeo: 5 s en total;
  - listado: 10 s;
  - comprobaciones: 2 s;
  - acción: `espera_seg` + 30 s.

  Cubren la red y el canal, nunca el tiempo de un diálogo (T113).
- **Usos:** sesiones, túneles, sondeos, listados, acciones, suscripciones de log, ejecuciones y el SFTP de BALTHASAR-2 cuentan como usos y se sueltan por identidad. Un canal SFTP ocupa dos canales de sesión (F8).
- **Caída:** si el vigilante ve la conexión cerrada, todo lo que la usaba pasa a caído o a error con el motivo, y nada se queda «en curso».

### 7.3 Servicios en el móvil: lo mismo que el servidor del escritorio

El script, sus argumentos, la validación, la lectura de salidas, las acciones permitidas y los límites son los del núcleo (F13). Lo único que cambia es quién ejecuta: en el móvil, `magi-movil` sobre su conexión.

| En el escritorio (protocolo v8) | En el móvil | Igual que en la F13 |
|---|---|---|
| `ListarServicios` → `Servicios` | `servicios_listar` (`refresco` para los automáticos) | 10 s, límites de su §8 y «lista incompleta» |
| `PedirConsumo` → `Consumo` | `servicios_consumo` | Solo con Contenedores a la vista |
| `DetalleServicio` → `Detalle` | `servicios_detalle` | Texto de unidad o de `inspect` de hasta 256 KiB |
| `ComprobarSudo` → `SudoComprobado` | `sudo_comprobar` | `sudo -S -p '' -v` con `LC_ALL=C` |
| `AccionServicio` → `AccionServicioHecha` | `servicio_accion` | Comprobación dentro de la acción (20 s), `espera_seg` + 30 s en total, segunda mirada a los 2 s y últimas 20 líneas si falla |
| `AccionesServicio{lista}` | `OyenteNucleo.acciones_servicio` | Lista completa en cada cambio, antes del efecto (T40) |
| `SeguirLog` · `LineasLog` · `PararLog` · `FinLog` | `logs_seguir` · `OyenteNucleo.logs` y `logs_ventana` · `logs_parar` | Lotes de 100 ms como mucho, anillo de 10 000 líneas y 8 MiB, y TERM al proceso real |
| `AbrirSesion{orden}` | `abrir_sesion_con_orden` | La orden sale de un tipo cerrado (T108) |

`exige_deliberacion` y la MELCHIOR-1 que no cuenta el objetivo son las mismas funciones del núcleo (F13 §7.2). `servicio_accion` no confía en la app: vuelve a comprobar que la acción se permite, si exige deliberación y que la deliberación vale (T38, T62).

### 7.4 Logs: anillo y ventanas

```python
class Anillo:                                        # en magi-movil, uno por suscripción
    MAX_LINEAS, MAX_BYTES = 10_000, 8 * 1024 * 1024

    def meter(self, linea):                          # linea.texto ya recortado a 16 KiB (F13)
        self.siguiente += 1
        linea.n = self.siguiente                     # número que no se repite: la vista se ancla a él
        self.lineas.append(linea)
        self.bytes += len(linea.texto)
        while len(self.lineas) > self.MAX_LINEAS or self.bytes > self.MAX_BYTES:
            vieja = self.lineas.popleft()
            self.bytes -= len(vieja.texto)
            self.omitidas_aqui += 1                  # se marcan donde faltan: «… N líneas omitidas»

def ventana(anillo, texto, nivel, desde=None, cuantas=60):
    cuantas = min(cuantas, 500)
    visibles = [l for l in anillo.lineas if visible(l, texto, nivel)]   # visible(): F13 §7.6
    if desde is None:                                # al final: siguiendo
        return visibles[-cuantas:]
    i = primera_con_n_mayor_o_igual(visibles, desde) # en pausa: la vista no se mueve
    return visibles[i:i + cuantas]
```

El aviso `OyenteNucleo.logs` sale como mucho cada 100 ms, y la app pide una ventana solo si la pantalla está a la vista. Las omitidas que se enseñan suman las del anillo y las de la lectura del script. Al soltar la pantalla, el anillo se borra.

### 7.5 La contraseña de sudo en el móvil

```python
class SudoEnMemoria:                                 # en magi-movil, por host
    def __init__(self, minutos):                     # ajustes.sudo_memoria_min (0 = pedirla siempre)
        self.minutos, self.claves = minutos, {}      # host_uuid → (Zeroizing, ultimo_uso)

    def vigente(self, host):
        c = self.claves.get(host)
        if c and ahora() - c.ultimo_uso <= self.minutos * 60:
            return c
        self.olvidar(host)                           # zeroize
        return None

    def para_accion(self, host):                     # las acciones la renuevan (D260 de la F13)
        c = self.vigente(host)
        if c:
            c.ultimo_uso = ahora()
        return c.clave if c else None

    def para_refresco(self, host):                   # los listados la usan sin alargarla
        c = self.vigente(host)
        return c.clave if c else None

    def al_bloquearse_magi(self):                    # bloqueo de la app (F11 §7.8)
        for h in list(self.claves):
            self.olvidar(h)
```

La contraseña cruza la frontera de la FFI como bytes que se borran tras usarlos (F11). Llega al host por la primera línea de la entrada estándar del script (F13 §7.3). Si el host la rechaza en una acción, se olvida y la siguiente la vuelve a pedir. En el móvil no se olvida al cambiar de pantalla, porque se navega mucho entre servicios, logs y sesiones. A cambio, se olvida al bloquearse MAGI (D268).

### 7.6 Snippets: preparar y ejecutar

```python
def preparar(snippet, marcados, valores):
    destinos = resolver(snippet, inventario.hosts)               # F6 §7.1, ahora en el núcleo
    hosts = [h for h in destinos if h.uuid in marcados]          # solo hosts ssh (T100)
    comando, errores = sustituir(snippet.comando, valores)       # F6 §7.2: escape con shell-escape
    delibera = requiere_deliberacion(snippet, hosts, verificaciones)   # F6 §7.3
    return Preparacion(snippet, comando, hosts, delibera, errores)

def ejecutar(prep, parar_al_fallo, deliberacion):
    validar(deliberacion, Plan.Snippet(prep))                    # T62: existe, es de esto y no está usada
    e = Ejecucion(prep, parar_al_fallo, deliberacion)            # usado_veces +1 (campo propio)
    for h in prep.hosts:
        lanzar(ejecutar_host(e, h))                              # semáforo de 8
    return e.id

async def ejecutar_host(e, h):
    c = await conexion(h, uso="ejecucion", interactiva=False)   # sin diálogos (F6 §7.5)
    r = await exec_con_plazo(c, e.comando, e.timeout_seg, tope=1024 * 1024)   # por flujo
    e.host(h).terminar(r)                                        # ok · fallo · error · cancelado
    if r.estado == "fallo" and e.parar_al_fallo:
        e.omitir_los_que_esperan()
    anotar("snippet_ejecutado", h, codigo=r.codigo, duracion=r.duracion, bytes=r.bytes)
```

Al acabar todos los hosts, la ejecución pasa a `terminada`. Si hubo deliberación, se anotan `deliberacion_aprobada` o `deliberacion_forzada` y `ejecucion_resultado`: `ok` si todos fueron bien, `error` si ninguno, `parcial` en otro caso y `cancelada` si se canceló (F6 §7.7). La salida nunca va a REGISTRO ni a logcat (T44), y las secuencias de escape se quitan antes de pintar.

### 7.7 Deliberación en el móvil

```python
class ProveedoresMovil:                              # los pone magi-movil; el escritorio, los suyos
    def sondeo(self, host):
        return flota.sondear_ya(host, plazo=2)       # automático
    def listar(self, host, ruta):
        return sftp_lectura.listar(conexion(host, "comprobacion", interactiva=False), ruta, plazo=2)
    def tests(self, host, comando):
        return NoDisponible("solo en el escritorio") # no hay comando local que valga en el móvil

def comprobar(host, verif, plan, prov, ajustes):    # núcleo: deliberacion/comprobaciones.rs
    r = {}
    if verif.salud:
        s = ultimo_sondeo(host)
        if s is None or edad(s) > ajustes.salud_max_min * 60:
            s = prov.sondeo(host)
        r["MELCHIOR-1"] = melchior(s, sin_contar=plan.objetivo_de_servicio())
    if verif.backup:
        r["BALTHASAR-2"] = balthasar(prov.listar(host, verif.backup_ruta), verif.backup_patron,
                                     ajustes.backup_horas)
    if verif.tests:
        r["CASPER-3"] = como_veredicto(prov.tests(host, verif.tests_comando))   # NoDisponible → rechaza
    return r                                         # en paralelo con las de los demás hosts (T43)

def balthasar(ficheros, patron, horas):
    casan = [f for f in ficheros if f.es_fichero and (not patron or casa_glob(patron, f.nombre))]
    if not casan:
        return Rechaza("sin ficheros que casen")
    ultimo = max(casan, key=lambda f: f.mtime)
    h = (ahora() - ultimo.mtime) / 3600
    return Aprueba(f"hace {h:.0f} h") if h < horas else Rechaza(f"último backup hace {h:.0f} h")
```

- El consenso es la unanimidad de las comprobaciones activas (F6 §7.3). Sin ninguna activa, «sin comprobaciones configuradas», y se sigue pidiendo mantener pulsado.
- `NoDisponible` es un veredicto que **rechaza**: una comprobación activa que no puede hacerse en una plataforma nunca se omite en silencio (T116). Así lo que el escritorio bloquearía también se bloquea en el móvil, y quien fuerza deja un motivo.
- El listado de `sftp_lectura` no es recursivo, no sigue enlaces y se corta a 10 000 entradas.
- El escritorio usa el mismo `comprobar` con sus proveedores: el `Ejecutar` del servidor, su `AbrirSftp` con `ListarDir` y `sh -c` en local.

### 7.8 Mantener pulsado para ejecutar

```python
MANTENER_MS = 1000

class BotonMantener:                                 # Compose; probado en la JVM con un reloj falso
    def al_pulsar(self):
        if self.disparado:
            return
        self.desde = ahora_ms()
        vibrar("leve")
    def cada_fotograma(self):
        if self.desde is not None:
            self.progreso = min(1.0, (ahora_ms() - self.desde) / MANTENER_MS)
            if self.progreso == 1.0:
                self.desde, self.disparado = None, True
                vibrar("confirmación")
                self.al_cumplirse()                  # una sola vez: el botón queda desactivado
    def al_soltar(self):
        if self.desde is not None:
            avisar("mantén pulsado para ejecutar")
        self.desde, self.progreso = None, 0.0
```

Es el `Ctrl+K` del móvil (D271). Un toque corto no ejecuta: solo explica cómo se hace. Con TalkBack, el botón expone una acción de pulsación larga con su etiqueta («mantener para reiniciar app.service»), que se lanza tocando dos veces y manteniendo; el progreso se anuncia. Lo usan las deliberaciones aprobadas o forzadas, de acciones y de snippets. Las confirmaciones normales usan un botón de un toque, porque ya son el segundo.

### 7.9 Snippet al conectar

Al abrir o reconectar una sesión a un host con `snippet_al_conectar_id`, `magi-movil` escribe el comando del snippet en la sesión, con un salto de línea, en cuanto arranca el shell, como el `comando_inicial` del escritorio (F6). Nunca delibera. El escritorio solo deja elegir snippets no críticos y sin variables, pero lo que llega por la sincronización es entrada no fiable. Por eso el móvil lo vuelve a comprobar: si el snippet es crítico o tiene variables, no lo escribe y lo avisa en la sesión. Las sesiones con orden (shell de un contenedor, seguimiento de logs) no tienen snippet al conectar.

### 7.10 Registro: filtros por tipo

| Filtro | Tipos |
|---|---|
| Conexiones | `conexion_abierta`, `conexion_fallida`, `sesion_cerrada`, `sesion_reconectada` |
| Huellas | `huella_aceptada`, `huella_sustituida`, `huella_sincronizada` |
| Túneles | `tunel_abierto`, `tunel_cerrado`, `tunel_fallido` |
| Sondeos | `sondeo_fallido` |
| Snippets | `snippet_ejecutado` |
| Deliberaciones | `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada` |
| Servicios | `servicio_accion` |
| Claves | `clave_generada`, `clave_instalada` |
| Sincronización | `inventario_importado` |

Las entradas se leen por páginas de 50, de la más nueva a la más vieja, con el filtro aplicado en SQL. La purga de las de más de 90 días sigue como en la F11 (al arrancar). Una deliberación sin `ejecucion_resultado` dice «sin resultado: MAGI se cerró durante la ejecución». Solo se enseña así: no se escribe nada.

### 7.11 Validaciones

| Dato | Regla |
|---|---|
| Unidades, contenedores, proyectos y motor | Los de la F13 (§7.11), en el núcleo y otra vez en el script |
| Unidades vigiladas del sondeo | `[A-Za-z0-9@._-]+` (F2); las que no casan se ignoran con aviso en la ficha |
| Variables de un snippet | F6 §7.2: nombre `[a-z_][a-z0-9_]*`. Una vacía sin valor por defecto no deja seguir, y una dentro de comillas o de un heredoc se rechaza |
| Motivo del forzado | 10 caracteres o más, sin contar los espacios de los extremos |
| Contraseña de sudo | Una línea, sin NUL; 3 intentos |
| Ventana de logs | 500 líneas como mucho por petición |
| `backup_ruta` | Absoluta y sin NUL; se lista solo ese directorio |
| Snippet al conectar | No crítico y sin variables, comprobado otra vez en el móvil |
| Ajustes | Los rangos de §5.4; fuera de rango, se acotan con aviso |
| Lo que llega del host | Los límites de la F13 (§8) y 1 MiB por flujo de salida (F6); nunca pánico (T38, T109) |

### 7.12 Casos especiales

- **Host borrado en el escritorio con una acción o una ejecución en curso en el móvil:** se aplaza como cualquier trabajo vivo (F11 §7.4) y se borra al acabar.
- **Cambia la dirección de un host con su conexión en gracia:** la conexión sigue hasta cerrarse; la siguiente usa la nueva (F11).
- **El host deja de responder durante una acción:** el resultado es `error`, con «se cortó la conexión: puede que el reinicio se haya hecho; refresca para ver el estado».
- **Reiniciar `sshd` o `tailscaled` desde el móvil:** sale el aviso en rojo. Las sesiones suelen seguir (sshd no corta las abiertas); si la conexión cae, el vigilante lo dice.
- **Cambio de wifi a datos con Flota abierta:** algún sondeo puede fallar una vez. Con Tailscale las conexiones suelen seguir (F11).
- **Pantalla apagada con un reinicio en curso:** la acción sigue con el servicio en primer plano. Si MagicOS cierra MAGI, al volver lo avisa (R133).
- **El escritorio y el móvil actúan sobre el mismo servicio a la vez:** cada uno solo ve sus propias acciones en curso. El host decide el orden y cada resultado lo dice.
- **Ajustes distintos en el móvil y en el escritorio** (umbrales, horas del backup): la misma deliberación puede salir distinta. Cada celda enseña el dato (R131).
- **Hosts de escritorio** (`rdp`, `vnc`, `voxl`): no salen en Servicios ni son destino de un snippet (T100).
- **Un host sin verificaciones** en una deliberación obligatoria (snippet crítico o varios hosts): «sin comprobaciones configuradas».
- **Directorio de backups con miles de ficheros:** se cortan a 10 000 entradas con aviso, dentro de los 2 s.
- **La contraseña de sudo cambia en el host:** la siguiente acción falla con «sudo rechazó la contraseña»; se olvida y se pide otra vez (F13).
- **Android cierra MAGI con trabajo en curso:** al volver, «Android cerró MAGI con 1 acción en curso: puede que se haya hecho en el host», y la guía de batería. La deliberación queda sin `ejecucion_resultado` (§7.10).
- **Una ejecución con un host sin credenciales:** ese host acaba en `error` con instrucción y los demás siguen, salvo con «parar al primer fallo», que lo trata como un fallo (F6).

### 7.13 Limitaciones heredadas

- R85-R87 (sin la corrección 10b) siguen igual en el móvil (F11 §7.13).
- R125 y las secciones vacías de un host sin systemd se dan por arreglados con el cierre de la F13. Si no lo están, esta fase los hace al mover el sondeo.
- R126: las comprobaciones propias que necesitan root van con `sudo -- sh -c`.
- R100: lo que solo se ve en el móvil real (MagicOS, el plegado, la batería) va al guion de validación.
- R15: las fases anteriores siguen sin validación manual completa.

---

## 8. Requisitos No Funcionales

**Seguridad**

- **Contraseña de sudo:** sin secretos nuevos en disco. Solo vive en memoria (`Zeroizing`), su diálogo lleva `FLAG_SECURE` y nunca va a `LlaveroMovil`, DataStore, logcat ni REGISTRO (T19, T105). Se olvida también al bloquearse MAGI.
- **Lo automático no dialoga (T15, T49):** el sondeo, los refrescos, las comprobaciones y las ejecuciones solo usan huellas aceptadas, la clave del móvil y contraseñas ya guardadas.
- **Ninguna acción con un toque (T6, T41, T115):** lo deliberado se ejecuta manteniendo pulsado 1 s y lo demás pide confirmación. Una comprobación que no puede hacerse aquí rechaza (T116).
- **`magi-movil` no se fía de la app:** recomprueba la acción, si exige deliberación y que la deliberación vale (T38, T62).
- **Órdenes remotas:** solo con los scripts embebidos y argumentos validados (T30). La shell de un contenedor y los seguimientos salen de tipos cerrados (T108).
- **Entrada no fiable (T38, T109):** los límites de la F13 y de la F6, 500 líneas por ventana de logs y 10 000 entradas por listado SFTP.
- **Nada del host a logcat ni a REGISTRO (T44):** ni logs, ni salida de snippets, ni unidades, ni salidas de comprobaciones.
- **Copiar una salida** la marca como sensible en el portapapeles (`ClipDescription.EXTRA_IS_SENSITIVE`, Android 13+).

**Protección de datos (RGPD)**

- Los logs de un servicio y la salida de un snippet pueden llevar datos personales. MAGI solo los enseña: viven en memoria (el anillo, y las ejecuciones durante 1 h), nunca en disco, y se borran al soltar la pantalla o al limpiar.
- REGISTRO guarda solo metadatos, 90 días. DELIBERACIONES guarda el motivo que escribe el usuario y «móvil (<nombre del equipo>)».

**Backups y recuperación**

- No hay migración, así que no hay nada que deshacer.
- Lo que escribe el móvil (sondeos, deliberaciones y registro) es local y se pierde al borrar los datos o al reinstalar. El inventario se rehace uniéndose otra vez (F11).
- Las acciones y ejecuciones en curso no se guardan: si Android cierra MAGI, la orden puede seguir en el host sin quedar en REGISTRO (R133).

**Rendimiento** (en el móvil de validación)

| Métrica | Objetivo |
|---|---|
| Sondeo de 20 hosts con la conexión en gracia | < 2 s |
| Sondeo de 20 hosts sin conexión (red local o Tailscale) | < 5 s |
| Listado de un host con 300 unidades y 30 contenedores | < 2 s |
| Logs a 2 000 líneas por segundo | Sin tirones; ventana filtrada de 60 líneas sobre 10 000 en < 10 ms |
| Desplazar Flota con 200 hosts | 60 fps |
| Memoria con 5 sesiones y un panel de logs lleno | < 250 MB |
| Batería con Flota a la vista 10 min (20 hosts, refresco de 30 s) | Medirla en el guion de validación (R127) |
| En segundo plano sin sesiones, túneles, acciones ni ejecuciones | Ningún trabajo |

**Accesibilidad**

- Glifo y texto en todos los estados (T5). TalkBack lee la fila de Flota («web-01, nominal, 3 fallos, 2 sesiones»), cada celda de la deliberación y el estado de cada host de una ejecución.
- Mantener pulsado con TalkBack: una acción de pulsación larga con su etiqueta (tocar dos veces y mantener), y el progreso se anuncia.
- Objetivos táctiles de 48 dp; los botones de acción llevan texto, no solo un icono.
- Límite: los logs y la salida se leen línea a línea, sin más estructura.

**Compatibilidad.** Android 11 (API 30) a 17 (API 37), `arm64-v8a` (F11). En los hosts, lo que piden la F2, la F6 y la F13.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| systemd, journald, Docker, Podman, Compose y sudo | Los de la F13, por `servicios.sh`, y el sondeo de la F2 con `<motores>` |
| SFTP (`russh-sftp` `=3.0.0`) | Solo abrir y leer un directorio, con `stat`, para BALTHASAR-2. La apertura del subsistema lleva su propio plazo, porque `request_subsystem` de russh devuelve `Ok` aunque el host diga que no |
| `globset` | `backup_patron` de BALTHASAR-2, con la misma sintaxis que en el escritorio |
| Ciclo de vida de Android | `ProcessLifecycleOwner` y `repeatOnLifecycle`, para que el sondeo, los refrescos y los logs solo corran a la vista |
| Servicio en primer plano `specialUse` (F11) | Cuenta también las acciones y las ejecuciones |
| Respuesta háptica | `LocalHapticFeedback` al empezar y al cumplirse «mantener pulsado» |
| Portapapeles | `ClipboardManager`; copiar una salida la marca como sensible (`EXTRA_IS_SENSITIVE`) |
| TalkBack | Acción de pulsación larga con su etiqueta en el botón de mantener |
| GitHub Actions | `android.yml`: `servidor_pruebas` en el job `humo` (R106) |
| Perseverus | Sin integración: avisa por su lado y el móvil se abre al recibir el aviso |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D263 — Alcance: el móvil de guardia** (Hector, 1a): Flota con sondeo, Servicios, snippets con deliberación y Registro. Es lo que se usa cuando llega un aviso y no hay ordenador. El móvil sigue lector, así que no hace falta la corrección 10b ni hay riesgo para el inventario compartido. Descartados por ahora:
  - editar el inventario (el móvil pasaría a escribir su fichero; mejor con la F18 y después de la 10b);
  - SFTP (poco útil en una pantalla de teléfono).
- **D264 — Los escritorios remotos siguen sin abrirse en el móvil** (Hector, 2a). Siguen viéndose en Flota con «se abre en el ordenador» (F12). Abrir el visor de la F12 dentro de la app queda para más adelante.
- **D265 — Flota sustituye a Hosts y es la pantalla de inicio** (Hector, 3a). En un teléfono, dos listas de los mismos hosts sobran: Flota tiene los grupos, la búsqueda y `▶` de Hosts, más el estado. La barra queda en FLOTA · SESIONES · TÚNELES · SERVICIOS · MÁS: cinco destinos, el máximo de Material. Snippets, Resultados, Registro y Ajustes van en MÁS. Descartados: dos pantallas como en el escritorio, y Hosts de inicio con Flota aparte.
- **D266 — Sondeo al abrir Flota, al deslizar y cada 30 s con Flota a la vista; nada en segundo plano** (Hector, 4a). Los avisos ya los da Perseverus. Así no hace falta WorkManager, que MagicOS retrasa (D178), ni se gasta batería sin mirar. El refresco se puede apagar en Ajustes. Descartados: un sondeo en segundo plano cada 15 min con notificación, y solo a mano.
- **D267 — Servicios completo, con la lógica del núcleo** (Hector, 5a). El script, la lectura, las acciones permitidas, los validadores y los límites son los de la F13. `magi-movil` ejecuta lo que en el escritorio ejecuta el servidor. Descartados: solo ver, y solo lo que falla.
- **D268 — Contraseña de sudo en memoria 5 min, nunca guardada** (Hector, 6a), como en el escritorio (D241 y D260 de la F13). En el móvil se olvida además al bloquearse MAGI, y no al cambiar de pantalla: se navega mucho entre servicios, logs y sesiones, y el bloqueo de la app ya pone un límite. Descartado: guardarla en `LlaveroMovil` con huella, porque sería un secreto con permisos de root en el teléfono.
- **D269 — Snippets: ejecutar los sincronizados en uno o varios hosts** (Hector, 7a), con variables, resultados por host y el snippet al conectar. Sin crear, editar ni borrar (el móvil es lector), sin «en pestaña» y sin atajos de Flota. Descartados: un solo host cada vez, y crear y editar (exige el móvil escritor).
- **D270 — Deliberación en el móvil: MELCHIOR-1 y BALTHASAR-2 como en el escritorio, y CASPER-3 rechaza «solo en el escritorio»** (Hector, 8a). El comando de tests es local del ordenador y no tiene sentido en un teléfono. Contarlo como no superado mantiene la unanimidad (T43, T116): quien quiera seguir, fuerza y deja el motivo. Descartados: omitir CASPER-3 con aviso (en el móvil se pasaría lo que el escritorio bloquea) y no deliberar.
- **D271 — Lo deliberado se ejecuta manteniendo pulsado 1 s** (Hector, 9a). Es el `Ctrl+K` del móvil: un gesto que no sale por reflejo, con relleno y vibración. Lo que no delibera pide una confirmación que nombra la acción, el objetivo y el host, y se acepta con un toque (el segundo). Forzar pide un motivo de 10 caracteres o más (F6). Descartados: pedir también huella o PIN (el bloqueo de MAGI ya cubre el teléfono ajeno), y deslizar para ejecutar.
- **D272 — Registro del propio móvil, con filtros y el detalle de las deliberaciones; sin exportar** (Hector, 10a). REGISTRO no viaja (D133), así que el móvil enseña lo suyo. Descartados: no tener vista, y exportar a CSV.
- **D273 — App 0.2.0 y el servidor de pruebas en el job `humo`** (Hector, 11a). La 0.1.0 sale antes, con el cierre de la F11. Arrancar `servidor_pruebas` en el anfitrión de CI hace que las pruebas de extremo a extremo del emulador dejen de omitirse (cierra R106 cuando se implemente).
- **D274 — El sondeo pasa al núcleo sin cambiar el escritorio.** El script, la lectura de las líneas `MAGI_*`, los estados con sus culpables, los fallos y la tasa de red se mueven a `magi-nucleo` (T85), como se movió el almacén en la F11. Se mueven sin reescribir: las pruebas y las instantáneas del escritorio no cambian. El script recibe `<motores>` (R125). `src/flota/` se queda con lo que es solo del escritorio: pedir `Ejecutar` al servidor o abrir una conexión efímera.
- **D275 — Las comprobaciones de la deliberación pasan al núcleo, con proveedores por plataforma.** El núcleo tiene `PlanDeliberado`, MELCHIOR-1, BALTHASAR-2, CASPER-3, el plazo de 2 s y el consenso. Cada plataforma pone cómo sondear, cómo listar un directorio y cómo ejecutar un comando local: el escritorio, con sus mensajes de siempre; el móvil, con su conexión, `sftp_lectura` y «no disponible». Así las dos deliberan igual con el mismo código.
- **D276 — Un listado SFTP mínimo en el núcleo (`sftp_lectura.rs`) para BALTHASAR-2.** Solo lee un directorio, no es recursivo, no sigue enlaces y se corta a 10 000 entradas. El SFTP completo del escritorio (`src/servidor/sftp.rs`) no cambia: rehacerlo sobre el núcleo arriesga lo que ya funciona (F4, F8) para una sola lectura. Es una duplicación pequeña y aceptada.
- **D277 — Una conexión por host para todo, con gracia de 60 s.** Sesiones, túneles, sondeo, listados, acciones, logs, ejecuciones y el listado de BALTHASAR-2 comparten la conexión del host (amplía D180). Sin usos, queda viva 60 s: más que el refresco de Flota (30 s), para no reconectar en cada vuelta. Las aperturas automáticas nunca abren un diálogo (T15) y fallan con instrucción.
- **D278 — El anillo de logs vive en `magi-movil` y la app pide ventanas.** Cruzar 10 000 líneas por la FFI en cada aviso sería caro. Así, además, los límites (10 000 líneas, 8 MiB) se aplican en Rust, como manda la F13. Cada línea lleva un número que no se repite, para que la vista en pausa no se mueva cuando se descartan las viejas (T117).
- **D279 — Acciones y ejecuciones son trabajo del móvil; los logs y los refrescos no.** Una acción o una ejecución lanzada sigue con la app en segundo plano y mantiene el servicio en primer plano, como el trabajo custodiado del escritorio (D246). El sondeo, los refrescos y los logs solo corren con su pantalla a la vista (T118).
- **D280 — Ajustes del móvil para lo que en el escritorio está en `config.toml`.** Son los umbrales y el refresco de Flota, el refresco, la espera, los motores y los minutos de sudo de Servicios, y las horas del backup y los minutos de salud de la deliberación. Van en DataStore, con los valores por defecto y los rangos del escritorio. `config.toml` no viaja (D133), y sin estos ajustes la misma deliberación podría salir distinta sin poder corregirlo.
- **D281 — Sin migración ni tablas nuevas.** El móvil escribe SONDEOS, DELIBERACIONES y REGISTRO con los tipos que ya existían, y los campos propios `SNIPPETS.usado_veces` y `ultimo_uso_en`. Nada de eso viaja.
- **D282 — `DELIBERACIONES.usuario` en el móvil es «móvil (<nombre del equipo>)».** En Android no hay un usuario del sistema que diga quién fue (T50). El nombre del equipo es el que pone la app en EQUIPOS (F11).
- **D283 — Pruebas de extremo a extremo de todo lo nuevo (T95).** En Rust, `magi-movil` contra el servidor SSH de prueba con los binarios falsos de la F13, que pasan a `crates/magi-nucleo/tests/fijos/` para compartirlos. En el emulador, contra `servidor_pruebas` (antes `servidor_pruebas_tunel`), que gana hosts con esos binarios y un directorio de backups. Probar solo los ViewModels o el parser no basta (lo enseñó la 11b).
- **D284 — Deslizar en Flota sondea y sincroniza.** Hosts ya lanzaba la pasada al deslizar (F11). Ahora el mismo gesto pone al día las dos cosas.
- **D285 — El refresco automático no sondea los grupos plegados.** Lo que no se ve no se gasta. Al abrir Flota y al deslizar sí se sondean, con el filtro aplicado.
- **D286 — Las sesiones con orden las arma `magi-movil` desde `OrdenPestana`** (T108): la shell de un contenedor y los seguimientos de logs. No tienen snippet al conectar y se nombran «host · objetivo (shell)» o «(logs)».

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Núcleo, conexiones y contrato | Mover el sondeo (script, lectura, estados y fallos) y las comprobaciones de la deliberación al núcleo sin cambiar el escritorio (pruebas e instantáneas idénticas); `sftp_lectura`; destinos y estados de ejecución en `snippets/`; fijos compartidos. Gestor del móvil con gracia y aperturas automáticas; contrato UniFFI compilable antes de las pistas (T63); `aplicar_ajustes`. `servidor_pruebas` con los binarios falsos y el job `humo` (R106) | 1,5 semanas |
| S2 — Flota y Servicios | Flota (sondeo, refresco a la vista, fila y ficha); barra de cinco, riel y dos paneles; Toda la flota; servicios de un host (secciones, orden, filtro, detalle y consumo); sudo en memoria; acciones con confirmación y resultado; REGISTRO `servicio_accion` | 1,5 semanas |
| S3 — Deliberación, snippets y registro | Deliberación del móvil (proveedores, tabla, forzar y mantener pulsado) en acciones y snippets. Snippets: lista, ejecutar, variables, resultados, salida, cancelar y repetir; snippet al conectar. Registro con filtros y detalle | 1 semana |
| S4 — Logs, segundo plano y cierre | Logs (anillo, ventanas, pausa, parar al salir y a sesión), shell de contenedor y sesiones con orden; servicio en primer plano y aviso al volver; Ajustes nuevos; accesibilidad; pruebas de extremo a extremo en Rust y en el emulador; 0.2.0, CHANGELOG y README; revisión adversarial (T31, T54) | 1 semana |

**Total: 4 sprints, ~5 semanas** (98 funcionalidades).

**Supuestos**

- Los cierres de la F11, la F12 y la F13 están hechos, en particular R125 y las secciones vacías de un host sin systemd. Si no, el S1 los incluye al mover el sondeo.
- El agente tiene el SDK de Android, el NDK r30 y un emulador `x86_64` con KVM (F11).
- Las pruebas no usan hosts reales: el servidor de pruebas, con los binarios falsos de la F13.
- La 0.1.0 se publica con el cierre de la F11. La etiqueta y los secretos los pone el desarrollador (T69).
- Hector valida en el plegable lo que el emulador no cubre (R100): MagicOS, el plegado, la batería, Tailscale y TalkBack.

**Guion de validación manual (en el plegable)**

1. Instalar el APK de depuración o la `android-v0.2.0-rc.1`. Al abrir MAGI, Flota es la pantalla de inicio, con los grupos y el `⇅`.
2. Flota con 5-10 hosts reales por Tailscale:
   - estados, `⚠N` y la segunda línea de cada fila;
   - deslizar: sondea y sincroniza;
   - dejarla 10 min a la vista y anotar el gasto de batería (R127);
   - pasar MAGI a segundo plano y comprobar que no sondea.
3. Un host con la huella sin aceptar o sin credenciales: sale CAÍDA con su instrucción. Conectar una vez desde la ficha y volver a sondear.
4. Provocar un fallo (una unidad que falla al arrancar): verlo en Toda la flota, tocarlo y abrir su detalle y sus logs.
5. Reiniciar un servicio con un usuario que necesita contraseña de sudo: diálogo, deliberación con MELCHIOR-1 y BALTHASAR-2 reales, mantener pulsado, resultado y REGISTRO.
6. Un host con tests activos (CASPER-3): sale «solo en el escritorio». Forzar con motivo y ver la deliberación en Registro, con el motivo.
7. Romper la configuración de Nginx y recargar: no recarga y enseña la salida de `nginx -t`.
8. Logs en vivo:
   - filtro y nivel;
   - pausar desplazándose hacia arriba;
   - salir y volver (seguimiento parado);
   - A SESIÓN.
9. Contenedores: reiniciar un proyecto Compose (con `config -q`), abrir una shell en un contenedor y ver sus logs.
10. Un snippet con variables en dos hosts: deliberación, resultados, salida, copiar y repetir.
11. Un host con snippet al conectar: se escribe al conectar y al reconectar.
12. Un reinicio lento con la pantalla apagada y MAGI en segundo plano: al volver, el resultado espera. Sin la exclusión de batería, mirar si MagicOS cierra MAGI y que avisa al volver (R93, R133).
13. Plegar y desplegar con Servicios abierto (dos paneles) y con un panel de logs.
14. TalkBack: recorrer Flota y lanzar una acción deliberada manteniendo pulsado (tocar dos veces y mantener).
15. Con una contraseña de sudo en memoria, dejar MAGI fuera 5 min para que se bloquee: la siguiente acción la vuelve a pedir.

---

## 12. Conexiones con Otras Fases

- **F2:** sondeo (script, estados, umbrales, SONDEOS y `sondeo_fallido`), Flota y Registro. El sondeo pasa al núcleo (D274).
- **F3:** estados de sesión y reconexión. Las sesiones con orden siguen el modelo de las pestañas con orden de la F13; en el móvil sigue sin haber servidor de sesiones.
- **F6:**
  - snippets: destinos, variables, cuándo se delibera, estados de una ejecución y salida;
  - deliberación: MELCHIOR-1, BALTHASAR-2, CASPER-3, consenso y forzado;
  - el snippet al conectar.

  Las comprobaciones pasan al núcleo con proveedores (D275).
- **F8:** un canal SFTP ocupa dos canales de sesión, y el listado de BALTHASAR-2 lo respeta.
- **F10:** el móvil sigue siendo lector y nada de lo nuevo viaja (D133). VERIFICACIONES_HOST, SNIPPETS y COMPROBACIONES_SERVICIO llegan por la sincronización.
- **F11:** es la base del móvil:
  - el gestor y el vigilante (T97);
  - el servicio en primer plano y el bloqueo;
  - el plegable (D209);
  - las pruebas de extremo a extremo (T95) y la publicación.

  Flota sustituye a Hosts.
- **F12:** los escritorios se ven en Flota sin abrirse. Solo los hosts `ssh` tienen servicios y son destino de snippets (T100).
- **F13:** la misma lógica de servicios (script, lectura, acciones, sudo, comprobaciones, logs y unidades críticas); la vista llega al móvil.
- **Corrección 10b (pendiente):** no cambia con esta fase. Hará falta antes de que el móvil escriba.
- **F15 (Terminal, segunda vuelta):** lo que mejore el terminal y viva en el núcleo llegará también al móvil.
- **F16 (Despliegues y runbooks):** reutilizará en el móvil la ejecución de snippets y la deliberación.
- **F18 (Equipo):** candidata para que el móvil pase a escribir (editar el inventario), después de la 10b.
- **F19 (Caja negra):** el registro del móvil no viaja; la auditoría conjunta de los equipos irá allí.
