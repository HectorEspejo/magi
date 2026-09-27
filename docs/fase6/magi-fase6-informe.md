# MAGI - Fase 6: Snippets y Deliberación MAGI

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 27 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Especificada sobre las Fases 2 a 5 con implementación reportada pero **sin validación manual completa** (R15). Incluye como primer bloque la **corrección 3b** del pool de conexiones de la Fase 3 (R28-R31), porque los snippets abren muchas conexiones a la vez y no pueden apoyarse en un pool que desplaza conexiones vivas.

---

## 1. Visión General

La Fase 6 añade los **snippets**: comandos guardados que se dirigen a etiquetas de host (y a hosts sueltos), con variables que se piden al ejecutar, y que el servidor de sesiones ejecuta en paralelo sin terminal, capturando la salida de cada host. Y añade la **deliberación MAGI**, la pausa obligatoria antes de una acción arriesgada: tres comprobaciones automáticas por host —MELCHIOR-1 salud, BALTHASAR-2 backup reciente, CASPER-3 tests en verde—, unanimidad para ejecutar y forzado solo con motivo escrito que queda registrado. Es donde la metáfora del producto deja de ser decorativa y pasa a ser una política.

Antes de todo eso, la fase corrige el defecto de la Fase 3 descubierto en la 5: las pestañas no reutilizaban el pool y cada apertura desplazaba la conexión viva del host, tumbando túneles y otras pestañas.

**Objetivos principales**

1. **Corrección 3b**: un único punto de apertura por host con cerrojo, pestañas que reutilizan el pool con `multiplexar`, ninguna conexión viva desplazada; pid del servidor antiguo en el aviso de versión y parada por señal; `Hecho` tras la escritura del registro.
2. Tablas SNIPPETS y SNIPPET_DESTINOS (migración 5) y vista **Snippets** (`F8`) con alta, edición, borrado, filtro y vista previa de destinos.
3. Ejecución en el servidor: varios hosts en paralelo (8), timeout por snippet, «parar al primer fallo», salida capturada por host, cancelable, que sobrevive a la ventana; vista **Resultados**; opción «abrir en pestaña» para lo interactivo.
4. **Deliberación MAGI** con tres comprobaciones configurables por host (tabla VERIFICACIONES_HOST), límite de 2 s por comprobación, unanimidad, `Ctrl+K` para ejecutar y `f` para forzar con motivo; tabla DELIBERACIONES y anotación en REGISTRO tras ejecutar.
5. «Snippet al conectar» en la ficha y atajos de Flota asignables a snippets.
6. Protocolo v4.

**Contexto.** Fase grande. La ejecución masiva es la función más peligrosa del producto: por eso la deliberación se diseña para que sea rápida (si tarda, se desactivará) y difícil de saltar sin dejar rastro.

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin crates nuevos salvo `shell-escape` (entrecomillado POSIX de variables). `VERSION_PROTOCOLO = 4`.

| Capa | Tecnología | Notas |
|---|---|---|
| Ejecución remota | Canal `exec` de russh sobre `conexion_para_canal` (3b) | stdout y stderr separados, código de salida, timeout, cancelación |
| Comprobación «tests» | `tokio::process::Command::new("sh").arg("-c")` en local | Comando escrito por el usuario en la ficha; plazo de 2 s y `kill` al vencer |
| Comprobación «backup» | `ListarDir` (F4) por SFTP | Sin ejecutar nada en el host |
| Comprobación «salud» | Último sondeo (F2) o sondeo nuevo | Reutiliza `Ejecutar` sobre la conexión viva |
| Variables | `shell-escape` | Valor siempre escapado (comillas cuando hacen falta) |
| Credencial del peer | `SO_PEERCRED` con `nix::sys::socket::getsockopt` | pid del servidor antiguo (3b) |

### Estructura de carpetas (novedades)

```
src/
├── almacen/
│   ├── migraciones.rs   migración 5: SNIPPETS, SNIPPET_DESTINOS, VERIFICACIONES_HOST, DELIBERACIONES, HOSTS.snippet_al_conectar_id
│   ├── snippets.rs      CRUD de snippets y destinos
│   ├── verificaciones.rs
│   └── deliberaciones.rs
├── snippets/
│   ├── mod.rs           resolución de destinos (etiquetas + hosts) y validación
│   └── variables.rs     {{nombre}} / {{nombre:defecto}}, sustitución entrecomillada
├── deliberacion/
│   ├── mod.rs           orquestación: comprobaciones por host en paralelo, plazo 2 s, consenso
│   ├── salud.rs         MELCHIOR-1
│   ├── backup.rs        BALTHASAR-2
│   └── tests.rs         CASPER-3
├── servidor/
│   ├── conexiones.rs    3b: conexion_para_canal único con cerrojo por host; guardar sin desplazar
│   ├── sesiones.rs      3b: abrir_y_servir sobre conexion_para_canal; comando_inicial
│   └── ejecuciones.rs   Ejecucion, EjecucionHost, semáforo 8, salida con tope, Ejecuciones{lista}
├── cliente/mod.rs       3b: SO_PEERCRED en el aviso de versión
├── main.rs              3b: servidor parar a versión distinta por señal · magi snippets
└── ui/
    ├── snippets.rs      F8
    ├── resultados.rs    subvista de F8
    ├── deliberacion.rs  diálogo MAGI
    └── ficha.rs         snippet al conectar · verificaciones previas
docs/fase6/
├── magi-fase6-informe.md · magi-fase6-prompt.md · magi-fase6-checklist.md
└── magi-fase6-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 cliente magi                                                       magi --servidor
 ┌──────────────────────────────────────────┐                      ┌───────────────────────────────────┐
 │ ui/snippets.rs (F8) ─ resuelve destinos  │                      │ servidor/ejecuciones.rs           │
 │   y variables (snippets/)                │  LanzarEjecucion     │  Ejecucion · semáforo 8 ·         │
 │ ui/deliberacion.rs ◀── deliberacion/ ────┼─────────────────────▶│  timeout · parar al fallo ·       │
 │   MELCHIOR-1 salud ── sondeo (F2) ───────┼── Ejecutar ─────────▶│  salida con tope 1 MiB            │
 │   BALTHASAR-2 backup ─ ListarDir (F4) ───┼── AbrirSftp/Listar ─▶│  registro::anotar (hilo BD):      │
 │   CASPER-3 tests ── sh -c (local, 2 s)   │                      │   snippet_ejecutado ·             │
 │ ui/resultados.rs ◀── Ejecuciones{lista} ─┼──────────────────────│   deliberacion_aprobada/forzada · │
 │                  ◀── Salida (a demanda) ─┼── PedirSalida ──────▶│   DELIBERACIONES.ejecucion_result.│
 │ almacen/ (SNIPPETS, DESTINOS,            │                      │ servidor/conexiones.rs (3b)       │
 │   VERIFICACIONES_HOST, DELIBERACIONES)   │                      │  conexion_para_canal · cerrojo    │
 └───────────────┬──────────────────────────┘                      │  por host · sin desplazar         │
                 │ SQLite (escritor del cliente)                   └──────┬────────────┬───────────────┘
                 ▼                                                        │ exec       │ sesiones, SFTP,
        ~/.local/share/magi/magi.db ◀──── solo lectura + hilo escritor ───┘            │ túneles (mismo
                                                                                        ▼ pool)
                                                                                    hosts remotos
```

**Reparto.** El cliente resuelve todo lo que dialoga o decide (destinos, variables, deliberación) y envía al servidor una ejecución ya resuelta: comando final y lista de hosts (T29). El servidor la ejecuta sin diálogos (T15), guarda la salida en memoria y anota en REGISTRO cuando el efecto se ha producido (T21). DELIBERACIONES la escribe el cliente al decidir; el servidor solo rellena `ejecucion_resultado` al terminar, por su hilo escritor (excepción documentada a T18).

---

## 3. Modelo de Datos

### Diagrama E-R (migración 5)

```
 ┌──────────────────────┐          ┌────────────────────────────┐
 │ SNIPPETS             │─────────<│ SNIPPET_DESTINOS           │
 │ id              PK   │          │ id              PK         │
 │ nombre          UQ   │          │ snippet_id      FK CASCADE │
 │ comando              │          │ etiqueta        TEXT NULL  │ ─ por nombre (ETIQUETAS.nombre)
 │ descripcion          │          │ host_id         FK NULL ───┼──▶ HOSTS (CASCADE)
 │ etiquetas            │          └────────────────────────────┘   exactamente uno de los dos
 │ critico              │
 │ timeout_seg          │     ┌──────────────────────────┐        ┌──────────────────────────────┐
 │ parar_al_fallo       │     │ HOSTS (F1)               │───────│ VERIFICACIONES_HOST          │
 │ usado_veces          │◀────│ + snippet_al_conectar_id │  1:1   │ host_id      PK FK CASCADE   │
 │ ultimo_uso_en        │ SET │   FK SNIPPETS NULL       │        │ salud        0/1             │
 │ creado_en            │ NULL└──────────────────────────┘        │ backup       0/1             │
 │ actualizado_en       │                                         │ backup_ruta  TEXT NULL       │
 └──────────┬───────────┘                                         │ backup_patron TEXT NULL      │
            │ SET NULL                                            │ tests        0/1             │
            ▼                                                     │ tests_comando TEXT NULL      │
 ┌──────────────────────────────────┐                             │ actualizado_en               │
 │ DELIBERACIONES                   │                             └──────────────────────────────┘
 │ id                 PK            │
 │ fecha                            │
 │ snippet_id         FK NULL       │
 │ accion             TEXT          │  «reiniciar nginx → hetzner-01, hetzner-02»
 │ hosts_json                       │  [host_id, nombre]
 │ comprobaciones_json              │  por host: salud/backup/tests → aprueba|rechaza|n/a, detalle, ms
 │ resultado                        │  aprobada | forzada | cancelada
 │ bloqueada          0/1           │  hubo al menos un rechazo
 │ motivo             TEXT NULL     │  obligatorio si forzada (≥ 10 caracteres)
 │ usuario                          │  usuario local
 │ ejecucion_resultado TEXT NULL    │  ok | parcial | error | cancelada (lo escribe el servidor)
 └──────────────────────────────────┘
```

Sin `CHECK` sobre enumeraciones (T8). REGISTRO añade `snippet_ejecutado`, `deliberacion_aprobada`, `deliberacion_forzada`, `deliberacion_cancelada`.

### Tablas

**SNIPPETS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| nombre | TEXT UNIQUE NOT NULL | «reiniciar nginx» |
| comando | TEXT NOT NULL | Texto que ejecuta el shell del usuario remoto; puede ser multilínea y contener `{{variable}}` o `{{variable:defecto}}` |
| descripcion | TEXT NOT NULL DEFAULT '' | |
| etiquetas | TEXT NOT NULL DEFAULT '' | Clasificación propia del snippet (minúsculas, separadas por espacio); no son etiquetas de host |
| critico | INTEGER 0/1 | Siempre pasa por deliberación |
| timeout_seg | INTEGER NOT NULL DEFAULT 60 | 5-3600 |
| parar_al_fallo | INTEGER 0/1 DEFAULT 0 | Valor por defecto de la casilla al ejecutar |
| usado_veces | INTEGER DEFAULT 0 | Lo incrementa el cliente al lanzar |
| ultimo_uso_en | TEXT NULL | |
| creado_en · actualizado_en | TEXT | |

**SNIPPET_DESTINOS** — `etiqueta` por **nombre** (no por id) para que el snippet siga siendo válido cuando la flota cambia: una etiqueta sin hosts hoy resuelve a cero hosts, no rompe el snippet. `host_id` con cascada: un host borrado desaparece de los destinos.

**VERIFICACIONES_HOST** — una fila por host (se crea al activar la primera comprobación). `backup_ruta` directorio remoto; `backup_patron` glob opcional (`*.sql.gz`). `tests_comando` se ejecuta en local con `sh -c`.

**HOSTS (migración 5)** — `snippet_al_conectar_id` FK SNIPPETS `ON DELETE SET NULL`; solo admite snippets no críticos y sin variables.

### Estructuras en memoria del servidor

```
 Ejecucion                                  EjecucionHost
 │ id · snippet_id · nombre · comando       │ host_id · nombre
 │ hosts [EjecucionHost]                    │ estado  (ver diagrama)
 │ timeout_seg · parar_al_fallo             │ codigo · inicio · fin
 │ deliberacion {id, forzada, motivo}?      │ stdout · stderr  (tope 1 MiB cada uno; «… truncada»)
 │ solicitante · creada_en · terminada_en   │ error
 │ estado  en_curso | terminada | cancelada │
```

### Diagrama de estados: deliberación (cliente)

```
  acción que la requiere (§7.3)
        ▼
   comprobando ── (todas las comprobaciones de todos los hosts en paralelo, 2 s cada una)
        │
        ├── todas aprueban (o no hay ninguna activa) ──▶ aprobada ── Ctrl+K ─▶ lanzada ─▶ [DELIBERACIONES resultado=aprobada]
        │                                                   └── Esc ────────▶ cancelada
        └── alguna rechaza ──▶ bloqueada ── f + motivo ≥ 10 ─▶ forzada ── Ctrl+K ─▶ lanzada ─▶ [resultado=forzada]
                                   │                               └── Esc ─▶ bloqueada
                                   └── Esc ─▶ cancelada ─▶ [resultado=cancelada, bloqueada=1] · REGISTRO deliberacion_cancelada
   lanzada ─▶ (servidor, al terminar la ejecución) REGISTRO deliberacion_aprobada | deliberacion_forzada
                                                   + DELIBERACIONES.ejecucion_resultado
  Inválidas: bloqueada → lanzada sin forzar; forzada con motivo < 10 caracteres; ↵ como confirmación (solo Ctrl+K);
  relanzar una deliberación ya resuelta (repetir una ejecución abre una deliberación nueva).
```

### Diagrama de estados: ejecución y host (servidor)

```
 Ejecucion:   en_curso ──todos los hosts en estado final──▶ terminada
                 └──CancelarEjecucion──▶ cancelada (hosts en cola → omitido; en marcha → canal cerrado → cancelado)

 EjecucionHost:
   en_cola ──turno (semáforo 8)──▶ conectando ──pool o conexión nueva no interactiva──▶ ejecutando
      │                               │ error (huella, frase, contraseña sin llavero, red) ─▶ error
      │ parar al fallo / cancelar     ▼
      └──────────────────────────▶ omitido          ejecutando ── código 0 ─▶ ok
                                                         ├── código ≠ 0 ─▶ fallo  (si parar_al_fallo: el resto en_cola → omitido)
                                                         ├── timeout ───▶ error «tiempo agotado» (canal cerrado)
                                                         └── cancelar ──▶ cancelado
   ok / fallo / error / cancelado / omitido son finales. REGISTRO snippet_ejecutado al llegar a ok, fallo, error o cancelado.
```

### Diagrama de estados: conexión del pool (corrección 3b)

```
   (sin entrada) ──conexion_para_canal(host) con el cerrojo del host tomado──▶ abriendo
        ▲                                                                        │ error ─▶ (sin entrada), cerrojo liberado
        │                                                                        ▼
        │                                              viva (canales = N ≥ 1) ◀──┘  otra apertura del mismo host espera al
        │                                                 │  │                      cerrojo y, con multiplexar, toma canal aquí
        │                                  canales → 0    │  │ caída de red
        │                                                 ▼  ▼
        └──── cerrada ◀── gracia 30 s sin canales ── en_gracia      caida ─▶ (sin entrada): sesiones → caida, túneles → caido,
                                                                              SFTP → error; nada se «desplaza»
  Inválidas: sustituir una entrada viva o en gracia por otra conexión (guardar solo si no hay entrada); dos aperturas
  simultáneas del mismo host fuera del cerrojo; liberar un canal de otra conexión (por identidad, T34).
```

---

## 4. Flujos de Trabajo

### 4.1 Ejecutar un snippet

```
  F8 ↵ (o a, !, paleta «snippet · <nombre> [· <host>]», atajo de Flota)
      ▼
  resolver destinos: hosts con alguna etiqueta destino ∪ hosts sueltos (sin duplicados, orden por grupo y nombre)
      │ 0 hosts ─▶ mensaje «el snippet no apunta a ningún host» ─▶ fin
      ▼
  ↵: [diálogo EJECUTAR] casillas por host (todas marcadas) · variables · [ ] parar al primer fallo (valor del snippet)
  a: ejecutar en todos sin diálogo de hosts (solo variables si las hay)
      │ Esc ─▶ fin
      ▼
  sustituir variables (escapadas para el shell) ─ variable vacía sin defecto ─▶ se vuelve a pedir
      ▼
  ¿requiere deliberación? (crítico · más de un host · algún host con verificaciones activas)
      ├─ no ─▶ LanzarEjecucion
      └─ sí ─▶ deliberación (§4.2) ─ cancelada ─▶ fin
                                    ─ aprobada / forzada + Ctrl+K ─▶ LanzarEjecucion{…, deliberacion}
      ▼
  cliente: usado_veces +1, ultimo_uso_en · vista Resultados con la ejecución nueva
  servidor: Ejecucion en_curso · Ejecuciones{lista} a todos
```

### 4.2 Deliberación

```
  hosts H, comprobaciones activas por host (VERIFICACIONES_HOST)
      ▼ (todo en paralelo; cada comprobación con plazo de 2 s)
  MELCHIOR-1 salud:   último sondeo del host ¿NOMINAL y < 5 min?
                        ├─ sí ─▶ aprueba
                        ├─ viejo o inexistente ─▶ sondear (Ejecutar sobre conexión viva o efímera no interactiva)
                        │        ─▶ NOMINAL ─▶ aprueba · CARGA/ALCANZABLE/CAÍDA ─▶ rechaza (culpables) · > 2 s ─▶ rechaza «sin datos recientes»
                        └─ CARGA / ALCANZABLE / CAÍDA ─▶ rechaza con detalle
  BALTHASAR-2 backup: AbrirSftp (no interactivo) ─▶ ListarDir(backup_ruta) ─▶ fichero más reciente que casa con backup_patron
                        ├─ mtime < backup_horas (24) ─▶ aprueba «hace 3 h»
                        ├─ más viejo ─▶ rechaza «último backup hace 31 h»
                        └─ sin ficheros / sin acceso / > 2 s ─▶ rechaza con motivo
  CASPER-3 tests:     sh -c tests_comando (local) ─▶ código 0 ─▶ aprueba · ≠ 0 ─▶ rechaza (última línea de stderr) · > 2 s ─▶ kill, rechaza
      ▼
  consenso = todas las comprobaciones activas de todos los hosts aprueban
      ├─ sí ─▶ APROBADA: barra verde · «Ctrl+K ejecutar · Esc cancelar»
      └─ no ─▶ BLOQUEADA: «se requiere unanimidad» · «f forzar (registra motivo) · Esc cancelar»
                f ─▶ [campo motivo] ─ < 10 caracteres ─▶ no se acepta ─▶ ≥ 10 ─▶ FORZADA: «Ctrl+K ejecutar»
      ▼
  cliente: inserta DELIBERACIONES (resultado, bloqueada, motivo, comprobaciones_json, usuario) al resolver
```

### 4.3 Ejecución en el servidor

```
  LanzarEjecucion{peticion_id, snippet_id, nombre, comando, hosts, timeout_seg, parar_al_fallo, deliberacion?}
      ▼
  Ejecucion en_curso · Hecho{peticion_id} · Ejecuciones{lista}
      ▼ por host (semáforo 8):
  conexion_para_canal(host, no interactivo) ─error─▶ host error (motivo con instrucción) ─▶ REGISTRO snippet_ejecutado(error)
      ▼
  canal exec(comando) · leer stdout/stderr hasta 1 MiB cada uno · esperar exit-status con timeout_seg
      ├─ exit 0 ─▶ ok · ├─ exit ≠ 0 ─▶ fallo (si parar_al_fallo: pendientes → omitido) · └─ timeout ─▶ cerrar canal, error
      ▼
  REGISTRO snippet_ejecutado por host (snippet, host, código, duración, bytes; nunca la salida)
      ▼ todos finales
  terminada · si hay deliberación: REGISTRO deliberacion_aprobada|forzada (acción, hosts, comprobaciones, motivo, resultado)
            + DELIBERACIONES.ejecucion_resultado = ok | parcial | error | cancelada
  retención: 1 h tras terminar o hasta LimpiarEjecuciones; el servidor no se apaga con ejecuciones en curso (T34)
```

### 4.4 Abrir en pestaña (interactivo)

```
  p en F8 ─▶ mismos pasos 4.1 (destinos, variables, deliberación si procede) ─▶ > 5 hosts: confirmación
      ▼
  por host: AbrirSesion{host_id, cols, filas, comando_inicial = comando + "\n"} ─▶ el servidor lo escribe tras el shell
      ▼
  sin captura de salida (es una pestaña); REGISTRO conexion_abierta (F3) y snippet_ejecutado con «en pestaña»
```

### 4.5 Corrección 3b: abrir una pestaña a un host con otra conexión viva

```
  AbrirSesion(host)
      ▼
  conexion_para_canal(host):  tomar cerrojo del host
      ├─ ¿entrada viva o en gracia y host.multiplexar? ─sí─▶ canales +1 · soltar cerrojo · canal de sesión nuevo (sin auth)
      ├─ ¿entrada viva y sin multiplexar? ─▶ conexión propia de la pestaña, NO entra en el pool (no desplaza nada)
      └─ sin entrada ─▶ conectar (diálogos al solicitante si es interactivo) ─▶ guardar en el pool · soltar cerrojo
      ▼
  la pestaña, el SFTP, los túneles y las ejecuciones del host comparten la conexión del pool
  al cerrar el último canal ─▶ gracia 30 s ─▶ cerrar
```

### 4.6 Diagrama de secuencia: snippet crítico en dos hosts

```
 ui/snippets   deliberacion/        servidor              hetz-01 / hetz-02        gh (local)
    │──↵ ejecutar──▶│                   │                         │                    │
    │               │──Ejecutar(sondeo)─▶│──exec sondeo.sh (pool)─▶│                    │
    │               │──AbrirSftp/ListarDir(/backups)────────────▶│                    │
    │               │──sh -c «gh run list … »────────────────────────────────────────▶│
    │               │◀─ sondeos · listados · código 0 (≤ 2 s) ──────────────────────────│
    │◀─ MAGI: 6/6 ✓ APROBADA ─│         │                         │                    │
    │──Ctrl+K──────▶│ INSERT DELIBERACIONES (aprobada)            │                    │
    │──LanzarEjecucion{…, deliberacion_id}─▶│                     │                    │
    │◀─Hecho · Ejecuciones[en_curso]───────│──exec (x2, semáforo)─▶│                    │
    │◀─Ejecuciones[hetz-01 ok · hetz-02 fallo 1]──│◀─exit-status───│                    │
    │                                      │ REGISTRO snippet_ejecutado (x2)            │
    │                                      │ REGISTRO deliberacion_aprobada · UPDATE ejecucion_resultado=parcial
    │──PedirSalida{ej, hetz-02}───────────▶│                         │                    │
    │◀─Salida{stdout, stderr, truncada=false}│                        │                    │
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi snippets` | Lista los snippets con destinos resueltos, crítico y último uso |
| `magi servidor estado` | Añade las ejecuciones en curso (snippet, hosts, progreso) |
| `magi servidor parar` | 3b: ante un servidor de otra versión muestra su pid y ofrece enviarle `SIGTERM` (confirmación; `--si` la omite) |

La ejecución de snippets desde la CLI queda fuera: la deliberación necesita la TUI (D72).

### 5.2 Protocolo (`VERSION_PROTOCOLO = 4`)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `LanzarEjecucion{peticion_id, snippet_id, nombre, comando, host_ids, timeout_seg, parar_al_fallo, deliberacion: {id, forzada, motivo}?}` | `Hecho{peticion_id}` / `Error{peticion_id, mensaje}` |
| `CancelarEjecucion{id}` / `LimpiarEjecuciones` | `Ejecuciones{lista}` (metadatos por host, sin salida; en cada cambio de estado) |
| `PedirSalida{ejecucion_id, host_id}` | `Salida{ejecucion_id, host_id, stdout_b64, stderr_b64, truncada}` |
| `AbrirSesion{…, comando_inicial?}` (cambio) | `Bienvenida` añade `ejecuciones` |

`VersionIncompatible` (3b) lleva además el pid del servidor, obtenido por el cliente con `SO_PEERCRED` si el servidor es tan antiguo que no lo envía.

### 5.3 Vista Snippets (`F8`)

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover; el panel inferior muestra comando, destinos resueltos, confirmación y uso |
| `↵` | Ejecutar: diálogo con hosts (casillas), variables y «parar al primer fallo» |
| `a` | Ejecutar en todos los destinos (sin diálogo de hosts) |
| `p` | Abrir en pestaña (una por host; confirmación si > 5) |
| `n` / `e` / `x` | Nuevo / editar / borrar (confirmación; avisa si es snippet al conectar de algún host) |
| `/` | Filtrar por nombre, comando, etiquetas del snippet o etiqueta destino |
| `t` | Ir a Resultados |
| `q` / `Esc` | Volver |

### 5.4 Vista Resultados

| Tecla | Acción |
|---|---|
| `↑` `↓` | Mover entre ejecuciones (panel superior) |
| `Tab` | Cambiar al panel de hosts de la ejecución |
| `↵` (en un host) | Ver salida: stdout y stderr separados, desplazables; se refresca cada 1 s mientras el host está en marcha |
| `s` | Guardar la salida del host (o de todos) en un fichero (diálogo de ruta; `ficheros.rs`, 600) |
| `x` | Cancelar la ejecución (confirmación si está en curso) |
| `r` | Repetir (mismo snippet, hosts y variables; pasa otra vez por deliberación si procede) |
| `C` | Limpiar terminadas |
| `q` / `Esc` | Volver a Snippets |

### 5.5 Diálogo de deliberación

| Tecla | Acción |
|---|---|
| `Ctrl+K` | Ejecutar (solo con consenso o forzada) — `↵` no ejecuta |
| `f` | Forzar: abre el campo motivo (≥ 10 caracteres) |
| `↑` `↓` | Recorrer hosts si no caben |
| `Esc` | Cancelar (o salir del campo motivo) |

### 5.6 Otras vistas

| Dónde | Novedad |
|---|---|
| Hosts y Flota | `!` abre la paleta filtrada a los snippets que apuntan al host seleccionado |
| Flota | Atajos `[flota.atajos]` de `config.toml`: tecla → nombre de snippet, sobre el host seleccionado; teclas ya usadas por Flota se ignoran con aviso al arrancar; aparecen en la barra |
| Ficha de host | «Snippet al conectar» (desplegable: no críticos sin variables) en «Al conectar»; bloque «Verificaciones previas» |
| Paleta | `snippet · <nombre>`, `snippet · <nombre> · <host>`, `ir a snippets`, `ir a resultados`, `nuevo snippet` |
| Registro | Filtro `t` gana el grupo «snippets» (`snippet_ejecutado`, `deliberacion_*`); el detalle de una deliberación muestra sus comprobaciones desde DELIBERACIONES |
| `config.toml` | `[deliberacion] backup_horas = 24, salud_max_min = 5, limite_seg = 2, motivo_min = 10` · `[flota.atajos]` |

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   F8 SNIPPETS ◀── paleta «ir a snippets» ◀── Hosts/Flota «!» ◀── atajos de Flota
   │  ↵ ejecutar ─▶ [EJECUTAR: hosts · variables · parar al fallo] ─┐
   │  a todos ───────────────────────────────────────────────────────┤
   │  p en pestaña ─▶ (deliberación) ─▶ F3 pestañas                  ▼
   │                                          ¿requiere deliberación? ─sí─▶ [DELIBERACIÓN MAGI] ─Ctrl+K─┐
   │  n e x ─▶ [formulario de snippet]                          └─no────────────────────────────────────┤
   │  t ──────────────────────────────▶ RESULTADOS ◀──────────────────────────────────────────────────┘
   │                                     ejecuciones │ hosts ─↵─▶ salida (stdout/stderr) · s guardar · r repetir
   Ficha de host ── Al conectar: snippet ── Verificaciones previas: salud · backup · tests
```

### 6.1 Snippets

```
┌ MAGI · SNIPPETS ────────────────────────────────── 5 ───────┐
│ / nginx_                                                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ▸ reiniciar nginx               web → 3 hosts   CRÍTICO    │
│    ver logs nginx                web → 3 hosts              │
│    recargar configuración nginx  web → 3 hosts              │
│    backup postgres               db  → 1 host    CRÍTICO    │
│    limpiar journald              todos → 12 hosts           │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│    systemctl restart nginx && systemctl status nginx        │
│                                                             │
│    Destinos      etiqueta «web»: hetzner-01, hetzner-02,    │
│                  vps-openclaw                               │
│    Confirmación  requiere deliberación MAGI (crítico)       │
│    Timeout 60 s · usado 11 veces · última hace 2 días       │
├─────────────────────────────────────────────────────────────┤
│ ↵ ejecutar  a en todos  p en pestaña  n e x  / buscar  t    │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 Deliberación MAGI (varios hosts)

```
        ┌─ DELIBERACIÓN MAGI ──────────────────────────────────────────┐
        │  ACCIÓN: reiniciar nginx → 3 hosts                           │
        │  ────────────────────────────────────────────────────────── │
        │                 MELCHIOR-1   BALTHASAR-2   CASPER-3          │
        │                 salud        backup<24 h   tests             │
        │  hetzner-01     ✓ 0,2 s      ✓ hace 3 h    ✓ verde           │
        │  hetzner-02     ✓ 0,1 s      ✕ hace 31 h   ✓ verde           │
        │  vps-openclaw   ✓ 0,4 s      —             —                 │
        │  ────────────────────────────────────────────────────────── │
        │  CONSENSO  6 / 7     ██████████████████░░░                   │
        │  ▸ BLOQUEADO — se requiere unanimidad                         │
        │                                                              │
        │  f forzar (registra motivo)              esc cancelar        │
        └──────────────────────────────────────────────────────────────┘
```

Tras `f`:

```
        │  Motivo del forzado (mín. 10 caracteres):                    │
        │  [ backup de hetz-02 revisado a mano en R2 esta mañana_    ] │
        │  ▸ FORZADO — quedará registrado con usuario y hora           │
        │  ctrl+k ejecutar                        esc volver           │
```

Con consenso: `▸ APROBADO — ctrl+k ejecutar · esc cancelar` en verde. `—` = comprobación no activa en ese host. Mientras comprueba, cada celda muestra `◐`.

### 6.3 Diálogo de ejecución

```
        ┌─ EJECUTAR · limpiar journald ───────────────────────┐
        │  Hosts (12)                                         │
        │   [x] hetzner-01   [x] hetzner-02   [x] vps-openclaw│
        │   [ ] backup-nas   [x] ci-runner    …               │
        │  Variables                                          │
        │   dias      [ 7                                   ] │
        │  [ ] parar al primer fallo       timeout 60 s       │
        │                                                     │
        │  ↵ continuar        espacio marcar      esc cancelar│
        └─────────────────────────────────────────────────────┘
```

### 6.4 Resultados

```
┌ MAGI · RESULTADOS ────────────────────── 2 en curso · 5 hoy ┐
│  HORA   SNIPPET             HOSTS  ✓  ✕  …   ESTADO         │
│▸ 11:42  reiniciar nginx     3      2  1  0   terminada ⚑    │
│  11:40  limpiar journald    11     9  0  2   en curso       │
│  ─────────────────────────────────────────────────────────  │
│  hetzner-01     ● ok      0  1,2 s   212 B                  │
│  hetzner-02   ▸ ✕ fallo   1  0,8 s   1,4 kB                 │
│  vps-openclaw   ● ok      0  1,9 s   208 B                  │
│  ─────────────────────────────────────────────────────────  │
│  stderr · hetzner-02                                        │
│  Job for nginx.service failed because the control process   │
│  exited with error code. See "systemctl status nginx" …     │
├─────────────────────────────────────────────────────────────┤
│ ⇥ panel  ↵ salida  s guardar  r repetir  x cancelar  C      │
└─────────────────────────────────────────────────────────────┘
```

`⚑` marca una ejecución forzada. Estados de host: `●` ok, `✕` fallo/error, `◐` conectando/ejecutando, `○` en cola, `–` omitido/cancelado.

### 6.5 Ficha de host (bloques nuevos)

```
│  AL CONECTAR                                                │
│    Snippet     [ ninguno                               ▾ ]  │
│    Multiplexar [x] …                                        │
│  VERIFICACIONES PREVIAS (deliberación MAGI)                 │
│    [x] salud del host (último sondeo NOMINAL < 5 min)       │
│    [x] backup reciente   ruta [ /var/backups/pg      ]      │
│                          patrón [ *.sql.gz ]  (< 24 h)      │
│    [x] tests en verde    [ gh run list -R 4d3/cooperapp -L 1 --json conclusion -q '.[0].conclusion' | grep -qx success ] │
```

### Notas de UX y diseño

- El diálogo MAGI es el único sitio donde se usa la metáfora; los nombres MELCHIOR-1, BALTHASAR-2 y CASPER-3 acompañan siempre a la palabra que dice qué comprueban.
- `↵` nunca ejecuta desde la deliberación: la tecla distinta (`Ctrl+K`, la del informe de diseño) evita el «sí» por reflejo.
- Cada celda muestra el tiempo o el dato que justifica el veredicto («hace 31 h», «CARGA: memoria 94 %»); nunca un ✕ sin motivo.
- Una deliberación sin comprobaciones activas (snippet crítico en un host sin verificaciones) muestra «sin comprobaciones configuradas» y sigue pidiendo `Ctrl+K`.
- La salida se muestra tal cual en texto plano: las secuencias de escape se eliminan antes de pintar (no se interpretan).
- Glifos con color y palabra; ASCII `* x o -`.

---

## 7. Lógica de Negocio

### 7.1 Resolución de destinos

```python
def resolver(snippet, hosts):
    por_etiqueta = {h for h in hosts if set(h.etiquetas) & snippet.etiquetas_destino}
    sueltos = {h for h in hosts if h.id in snippet.hosts_destino}
    return sorted(por_etiqueta | sueltos, key=lambda h: (h.grupo_orden, h.nombre))
```

Un snippet sin destinos no se puede guardar; uno cuyos destinos resuelven a cero hosts se guarda y avisa.

### 7.2 Variables

- Sintaxis `{{nombre}}` o `{{nombre:defecto}}`; nombre `[a-z_][a-z0-9_]*`; se detectan al guardar y se listan en la ficha del snippet.
- Al ejecutar se piden todas (con el defecto precargado); una vacía sin defecto no deja continuar.
- Sustitución **siempre escapada** con `shell-escape` (entrecomilla cuando hace falta): `systemctl restart {{servicio}}` con `nginx` → `systemctl restart nginx` (sin comillas si no hacen falta) y con `a b; rm` → `'a b; rm'`.
- Un snippet con variables no puede ser «snippet al conectar».

### 7.3 Cuándo se delibera

```python
def requiere_deliberacion(snippet, hosts, verificaciones):
    return (snippet.critico
            or len(hosts) > 1
            or any(verificaciones[h.id].alguna_activa() for h in hosts))

def consenso(resultados):          # resultados[host][comprobacion] ∈ {"aprueba", "rechaza"} (solo activas)
    votos = [v for por_host in resultados.values() for v in por_host.values()]
    return all(v == "aprueba" for v in votos)   # vacío → True (se pide Ctrl+K igualmente)
```

### 7.4 Comprobaciones

| Comprobación | Aprueba si | Rechaza si |
|---|---|---|
| MELCHIOR-1 salud | Último sondeo NOMINAL de menos de `salud_max_min` (5), o sondeo nuevo NOMINAL | CARGA (con culpables), ALCANZABLE («sin métricas»), CAÍDA, sin sondeo en plazo |
| BALTHASAR-2 backup | El fichero más reciente de `backup_ruta` que casa con `backup_patron` tiene mtime < `backup_horas` (24) | Más antiguo, directorio vacío o sin coincidencias, sin acceso SFTP, plazo vencido |
| CASPER-3 tests | `sh -c tests_comando` termina con 0 en < 2 s | Código ≠ 0 (última línea de stderr), plazo vencido (proceso matado) |

Todas con `limite_seg` (2) medido desde el inicio de la deliberación; se ejecutan en paralelo entre sí y entre hosts. Ninguna dialoga: el SFTP del backup y el sondeo de salud usan política no interactiva.

### 7.5 Ejecución no interactiva

La conexión de cada host se toma con `conexion_para_canal` (3b). Si hay que abrirla, la política es la del sondeo (T15): huella desconocida o cambiada, clave con frase fuera del agente → error de ese host con instrucción («conéctate una vez con ↵»). Contraseña: el servidor la pide al solicitante, que solo la resuelve desde el llavero; sin entrada en el llavero → error del host. Nunca aparece un diálogo durante una ejecución.

### 7.6 Salida

Cada flujo (stdout, stderr) se guarda hasta 1 MiB; el resto se descarta y se marca `truncada`. La salida no se escribe nunca en REGISTRO ni en el log (T44). `Ejecuciones{lista}` lleva solo metadatos (estado, código, duración, bytes); la salida se pide con `PedirSalida`. Al pintar se eliminan secuencias de escape ANSI.

### 7.7 Anotaciones

| Momento | Quién | Qué |
|---|---|---|
| Deliberación resuelta (aprobada, forzada o cancelada) | Cliente | Fila en DELIBERACIONES; si cancelada, REGISTRO `deliberacion_cancelada` |
| Host en estado final (ok, fallo, error, cancelado) | Servidor | REGISTRO `snippet_ejecutado` (snippet, host, código, duración, bytes) |
| Ejecución terminada con deliberación | Servidor | REGISTRO `deliberacion_aprobada` o `deliberacion_forzada` (acción, hosts, comprobaciones, motivo, usuario, resultado) y `DELIBERACIONES.ejecucion_resultado` |

`ejecucion_resultado`: `ok` si todos ok; `error` si ninguno ok; `parcial` en otro caso; `cancelada` si se canceló.

### 7.8 Corrección 3b

- `conexion_para_canal(host, politica)` es la **única** forma de obtener una conexión para pestañas, SFTP, túneles, sondeo por `Ejecutar` y ejecuciones. Toma un cerrojo por host durante «mirar el pool / conectar / guardar».
- Con `multiplexar`, una entrada viva o en gracia se reutiliza (canales +1, sin autenticar). Sin `multiplexar`, la pestaña abre conexión propia que **no entra en el pool**; SFTP, túneles y ejecuciones de ese host siguen usando el pool.
- `Pool::guardar` solo inserta si no hay entrada; nunca desplaza una conexión viva ni en gracia.
- `magi servidor parar` contra un servidor de otra versión: muestra el pid (de `VersionIncompatible` o de `SO_PEERCRED`) y, con confirmación, le envía `SIGTERM`. El servidor nuevo trata `SIGTERM` como `Parar` (apagado limpio: sesiones, túneles y ejecuciones cerrados con sus anotaciones).
- Un `Hecho` de una operación que anota en REGISTRO se envía después de que el hilo escritor confirme la fila (T21); el test `borrar_remoto_es_recursivo_y_anota_el_borrado` deja de ser intermitente.
- README: la reutilización con `multiplexar` pasa a ser verdad.

### 7.9 Casos especiales

- Snippet borrado con una ejecución en curso: la ejecución sigue (el servidor tiene el comando resuelto); DELIBERACIONES conserva la acción en texto (`snippet_id` a NULL).
- Host borrado durante una ejecución: su fila en la ejecución sigue hasta terminar; `snippet_ejecutado` con `host_id` NULL y el nombre en el detalle.
- Snippet al conectar en una pestaña reconectada: se vuelve a escribir (es «al conectar»).
- Atajo de Flota a un snippet inexistente: aviso al arrancar y la tecla no hace nada.
- Ejecución en un host con salto: la conexión del pool ya lo resuelve.
- Un host que aparece dos veces (por etiqueta y suelto) se ejecuta una sola vez.

---

## 8. Requisitos No Funcionales

**Seguridad**

- La ejecución masiva siempre delibera (más de un host); un snippet crítico siempre delibera; los hosts marcan su propia exigencia. El forzado exige motivo y queda en DELIBERACIONES y REGISTRO con usuario y hora (T41).
- `Ctrl+K` es la única confirmación de una deliberación; `↵` no ejecuta.
- Variables siempre escapadas para el shell; la salida remota no se interpreta al pintar (sin secuencias de escape).
- `tests_comando` se ejecuta en local con el usuario de MAGI: lo escribe el propio usuario en la ficha; no admite variables del snippet ni datos del inventario interpolados.
- Ninguna salida de comando va a REGISTRO ni al log.

**Protección de datos (RGPD)** — sin cambios: datos propios, locales. DELIBERACIONES guarda el usuario local del sistema.

**Backups y recuperación** — las tablas nuevas viajan con `magi.db`. La salida de ejecuciones es efímera (memoria del servidor, 1 h) y se guarda a fichero con `s` si se quiere conservar.

**Rendimiento**

- Deliberación completa en ≤ 2 s (plazo por comprobación, todas en paralelo).
- 50 hosts × snippet corto: < 15 s con 8 concurrentes sobre conexiones del pool.
- Memoria acotada: 2 MiB por host y ejecución como máximo; retención 1 h.

**Accesibilidad** — veredictos con glifo, palabra y dato; todo por teclado.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Hosts remotos | Canal `exec` (shell del usuario remoto); SFTP para la comprobación de backup |
| Comando local de tests | Cualquier CLI del usuario (`gh`, `curl` a un CI, `test -f`) vía `sh -c` |
| Señales | `SIGTERM` al servidor de versión antigua (3b) |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D63 — Corrección 3b dentro de la fase.** Un único `conexion_para_canal` con cerrojo por host para todos los subsistemas; pestañas que reutilizan con `multiplexar`; `guardar` nunca desplaza. Descartado hacerla como fase aparte (Hector) o posponerla (los snippets multiplican aperturas simultáneas).
- **D64 — `F8` para Snippets.** Quedan F8-F12 libres en el terminal; F10 se evita porque Dicta la usa en Omarchy. Descartados solo paleta y `Shift+F`.
- **D65 — El cliente resuelve y delibera; el servidor ejecuta lo resuelto (T29).** El servidor recibe comando final y hosts; nunca lee SNIPPETS ni pregunta.
- **D66 — Variables siempre escapadas para el shell.** Evita que un valor rompa el comando; quien necesite texto crudo lo escribe en el comando.
- **D67 — Ejecuciones no interactivas (T15).** Una ejecución en 12 hosts no puede abrir 12 diálogos; la contraseña solo sale del llavero vía solicitante.
- **D68 — Salida en memoria del servidor con tope y a demanda.** 1 MiB por flujo, 1 h de retención, `PedirSalida`; nunca en REGISTRO. Descartado persistir salidas en SQLite (crecimiento sin control y datos sensibles en disco).
- **D69 — Deliberación en el cliente con plazo de 2 s y `Ctrl+K`.** Las comprobaciones son las del informe de diseño con los nombres MAGI; si tardan, se desactivan: por eso el plazo es duro.
- **D70 — Unanimidad y forzado con motivo ≥ 10.** DELIBERACIONES la escribe el cliente al decidir; el servidor anota en REGISTRO y rellena `ejecucion_resultado` al terminar (excepción a T18, por su hilo escritor).
- **D71 — Destinos por nombre de etiqueta.** El snippet sobrevive a que una etiqueta se quede sin hosts; `host_id` suelto con cascada.
- **D72 — Sin ejecución desde la CLI.** La deliberación necesita la TUI; una CLI que la saltara vaciaría el control.
- **D73 — Atajos de Flota solo en teclas libres.** `r` ya es «sondear» desde la F2; el mockup original (`r` reinicio) no se respeta literalmente.
- **D74 — Protocolo v4** (mensajes nuevos y `AbrirSesion` con campo nuevo; T24).
- **D75 — Fuera de alcance:** subida de ficheros antes del snippet (deploy con SFTP), snippet que exige un túnel activo, ventana de mantenimiento como cuarta comprobación, «ejecutar solo en los hosts aprobados».

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S0 — Corrección 3b | `conexion_para_canal` único con cerrojo, pestañas sobre el pool, `guardar` sin desplazar, `SIGTERM` y pid del servidor antiguo, `Hecho` tras la escritura, tests de regresión, README | 0,5 semanas |
| S1 — Snippets | Migración 5, CRUD, resolución de destinos, variables, vista F8, formulario, paleta, `!`, `magi snippets` | 1 semana |
| S2 — Ejecución | Protocolo v4, `servidor/ejecuciones.rs`, salida con tope, cancelación, vista Resultados, abrir en pestaña, snippet al conectar, atajos de Flota | 1 semana |
| S3 — Deliberación | VERIFICACIONES_HOST y bloque de la ficha, tres comprobaciones, diálogo MAGI, forzado, DELIBERACIONES, anotaciones, filtro de Registro, revisión adversarial (T31) | 1 semana |

**Total: 4 sprints, ~3,5 semanas.** Supuestos: el `sshd` en proceso de los tests admite `exec` con código de salida (ya lo hace el sondeo) y el SFTP de la F4; la comprobación de tests se prueba con comandos locales triviales (`true`, `false`, `sleep 3`).

---

## 12. Conexiones con Otras Fases

- **F2:** el sondeo alimenta MELCHIOR-1; REGISTRO gana cuatro tipos; Flota gana `!` y atajos.
- **F3:** la corrección 3b cierra la funcionalidad desmarcada de su checklist (reutilización del pool); `AbrirSesion` gana `comando_inicial`.
- **F4:** `ListarDir` alimenta BALTHASAR-2 sin ejecutar nada en el host.
- **F5:** los túneles dejan de caer al abrir una pestaña (3b). Un snippet que exija túnel queda como candidato.
- **F7 (Sincronización):** SNIPPETS, SNIPPET_DESTINOS y VERIFICACIONES_HOST viajan con el inventario; DELIBERACIONES y REGISTRO, como historial, a decidir.
- **F8 (Android):** snippets ejecutables desde el móvil con la misma deliberación (las comprobaciones de tests locales no aplican en el móvil).
