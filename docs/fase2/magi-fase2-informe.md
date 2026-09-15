# MAGI - Fase 2: Flota, Identidades y Registro

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 15 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

---

## 1. Visión General

La Fase 1 dejó a MAGI como sustituto de `~/.ssh/config`: inventario, ficha, importación/exportación y conexión embebida. La Fase 2 lo convierte en panel de control. Se añaden las tres vistas de primer nivel que el informe de diseño reserva para ello: **Flota** (F1), la pantalla de entrada que responde a «¿está todo bien?»; **Identidades** (F5), la gestión de claves como pantalla propia; y **Registro** (F7), el historial consultable de lo que MAGI ha hecho.

Nada de esto toca la sesión embebida validada en la Fase 1. El sondeo reutiliza el cliente russh y abre conexiones efímeras bajo demanda, nunca persistentes. Las claves privadas siguen sin guardarse: MAGI las genera y las escribe en `~/.ssh` cuando se le pide, pero después solo conserva la referencia.

**Objetivos principales**

1. Vista Flota con sondeo bajo demanda (carga, memoria, disco, red, uptime y servicios systemd) mediante un único script por SSH, en paralelo y con umbrales globales que derivan los estados NOMINAL / CARGA / FRÍA / CAÍDA.
2. Campo «servicios» en la ficha de host para las unidades systemd a vigilar.
3. Tabla IDENTIDADES poblada por el escaneo de la Fase 1, con alias, huella, origen (agente, fichero, token), hosts que la usan y último uso.
4. Generar (ed25519 / rsa 4096), importar, copiar la pública y revocar referencias desde la vista Identidades.
5. Tabla REGISTRO con los eventos de conexión, huellas, importación/exportación, claves y sondeos fallidos; vista Registro con filtro, detalle, purga y exportación CSV/JSON.
6. MAGI arranca en Flota; `F1` y `F2` quedan ambos operativos.

**Contexto.** Esta fase es la de «usar MAGI todos los días» antes de abordar las pestañas y túneles de la Fase 3. No hay riesgo técnico nuevo: todo se apoya en el cliente y en la infraestructura de ficheros de la Fase 1.

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin cambios respecto a la Fase 1 (Rust stable, ratatui 0.30, crossterm 0.29, tokio, russh 0.63, tui-term 0.3, vt100 0.16, rusqlite 0.40, clap, toml + serde, directories, nucleo-matcher, zeroize, tracing, chrono, unicode-normalization, anyhow + thiserror). Novedades:

| Capa | Tecnología | Notas |
|---|---|---|
| Generación de claves | `ssh-key` (ya es dependencia de russh; `russh::keys`) con `rand_core::OsRng` | ed25519 y rsa 4096; cifrado OpenSSH con frase; escritura en formato OpenSSH. Sin `ssh-keygen` para no exponer la frase en argumentos |
| Alta en el agente | `russh::keys::agent::client::AgentClient::add_identity` | Añade la clave recién generada al agente sin pasar por `ssh-add` |
| Portapapeles | `wl-copy` (Wayland) / `pbcopy` (macOS) por `std::process::Command` con la clave por stdin | Fallback: diálogo con la línea de la clave pública |
| Sondeo | Script `sh` embebido (`include_str!("flota/sondeo.sh")`) ejecutado por canal `exec` de russh | Un solo viaje por host; parseado por marcadores |
| Serialización | `serde_json` | `servicios_json` en SONDEOS y exportación JSON del registro |
| CSV | `csv` crate | Exportación del registro |

### Estructura de carpetas (novedades)

```
src/
├── flota/
│   ├── mod.rs           orquestación del sondeo: semáforo de 8, timeout 5 s, reutilización de sesión viva
│   ├── sondeo.sh        script ejecutado en el host (sin dependencias fuera de coreutils + systemctl)
│   ├── parser.rs        marcadores → Sondeo; detección de «sin métricas»
│   └── estado.rs        umbrales → NOMINAL / CARGA / FRÍA / CAÍDA / ALCANZABLE
├── identidades.rs       (F1) + sincronización con la tabla IDENTIDADES, generar, importar, revocar
├── portapapeles.rs      wl-copy / pbcopy con fallback
├── registro.rs          escritura de eventos (API única usada por conexion/, sshconfig/, flota/, identidades)
├── almacen/
│   ├── migraciones.rs   migración 2: HOSTS.servicios, SONDEOS, IDENTIDADES, REGISTRO
│   ├── sondeos.rs
│   ├── identidades.rs
│   └── registro.rs
└── ui/
    ├── flota.rs         F1
    ├── identidades.rs   F5
    └── registro.rs      F7
docs/fase2/
├── magi-fase2-informe.md
├── magi-fase2-prompt.md
├── magi-fase2-checklist.md
└── magi-fase2-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura (novedades sobre la Fase 1)

```
┌──────────────────────────── proceso magi ─────────────────────────────────┐
│  hilo UI                              runtime tokio                        │
│  ┌──────────────┐  Sondear(hosts)  ┌──────────────────────────────────┐   │
│  │ ui/flota.rs  │─────────────────▶│ flota/mod.rs                     │   │
│  │              │◀────────────────│  semáforo(8) · timeout 5 s        │   │
│  └──────────────┘  Sondeo(host,r)  │  ┌───────────┐  ┌───────────┐    │   │
│         │                          │  │ tarea h1  │  │ tarea h2  │ …  │   │
│         │ lee último               │  └─────┬─────┘  └─────┬─────┘    │   │
│         ▼                          └────────┼──────────────┼──────────┘   │
│  ┌──────────────┐                  ┌────────▼──────────────▼─────────┐    │
│  │ almacen/     │◀── guarda ───────│ conexion/cliente.rs (F1)        │    │
│  │ sondeos.rs   │                  │  sesión viva → canal exec       │    │
│  │ registro.rs  │◀── evento ───┐   │  si no → conexión efímera       │    │
│  │ identidades  │              │   └────────┬────────────────────────┘    │
│  └──────────────┘              │            │ exec sondeo.sh              │
│  ┌──────────────┐              │            ▼                             │
│  │ registro.rs  │──────────────┘      host remoto                         │
│  │ (API única)  │◀── conexion/ · sshconfig/ · identidades · flota/        │
│  └──────────────┘                                                         │
│  ┌──────────────────┐   ssh-key       ┌──────────────┐  add_identity      │
│  │ identidades.rs   │───────────────▶│ ~/.ssh/<n>   │──────────▶ ssh-agent│
│  │ generar/importar │   wl-copy/      └──────────────┘                     │
│  │ copiar/revocar   │───pbcopy──────▶ portapapeles                         │
│  └──────────────────┘                                                     │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Modelo de Datos

### Diagrama E-R (novedades; F1 en gris)

```
 ┌──────────────┐        ┌───────────────────────┐        ┌──────────────┐
 │ GRUPOS (F1)  │───────<│ HOSTS (F1)            │>──────<│ ETIQUETAS(F1)│
 └──────────────┘        │ + servicios     (F2)  │        └──────────────┘
                         └──┬─────────┬──────────┘
                            │         │
              ┌─────────────┘         └──────────────┐
              ▼                                      ▼
 ┌────────────────────────┐              ┌────────────────────────────┐
 │ SONDEOS                │              │ REGISTRO                   │
 │ id              PK     │              │ id              PK         │
 │ host_id         FK     │              │ fecha                      │
 │ fecha                  │              │ tipo                       │
 │ resultado              │              │ host_id         FK NULL ───┼──▶ HOSTS
 │ error                  │              │ identidad_id    FK NULL ───┼──▶ IDENTIDADES
 │ nucleos                │              │ detalle                    │
 │ carga_1m/5m/15m        │              │ resultado                  │
 │ mem_total_kb           │              └────────────────────────────┘
 │ mem_disponible_kb      │
 │ disco_total_kb         │              ┌────────────────────────────┐
 │ disco_usado_kb         │              │ IDENTIDADES                │
 │ red_rx_bytes           │              │ id              PK         │
 │ red_tx_bytes           │   referencia │ alias           UQ         │
 │ uptime_seg             │   por huella │ tipo                       │
 │ servicios_json         │   o ruta     │ huella          UQ         │
 │ duracion_ms            │  ◀ ─ ─ ─ ─ ─ │ origen                     │
 └────────────────────────┘  HOSTS.      │ ruta            NULL       │
                             identidad_  │ comentario                 │
                             ref (F1)    │ anadida_en                 │
                                         │ ultimo_uso_en   NULL       │
                                         │ revocada_en     NULL       │
                                         └────────────────────────────┘
```

### Tablas

**HOSTS (modificación)**

| Campo | Tipo | Descripción |
|---|---|---|
| servicios | TEXT NOT NULL DEFAULT '' | Unidades systemd a vigilar, una por línea (sin `.service` obligatorio; se normaliza) |

**SONDEOS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| host_id | INTEGER FK HOSTS ON DELETE CASCADE | |
| fecha | TEXT NOT NULL | Momento del sondeo (local, ISO 8601) |
| resultado | TEXT NOT NULL | `ok`, `sin_metricas` (responde pero no es Linux/systemd), `error` |
| error | TEXT NULL | Motivo si `error` |
| nucleos | INTEGER NULL | `nproc` |
| carga_1m, carga_5m, carga_15m | REAL NULL | `/proc/loadavg` |
| mem_total_kb, mem_disponible_kb | INTEGER NULL | `/proc/meminfo` |
| disco_total_kb, disco_usado_kb | INTEGER NULL | `df -kP /` |
| red_rx_bytes, red_tx_bytes | INTEGER NULL | Suma de contadores de `/proc/net/dev` sin `lo`; la tasa se calcula con el sondeo anterior |
| uptime_seg | INTEGER NULL | `/proc/uptime` |
| servicios_json | TEXT NULL | `{"nginx":"active","penwork":"failed"}` |
| duracion_ms | INTEGER NOT NULL | Tiempo total del sondeo |

Se conservan los **20 últimos** sondeos por host; al insertar se purgan los anteriores.

**IDENTIDADES**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| alias | TEXT UNIQUE NOT NULL | Por defecto el comentario de la clave o el nombre del fichero; editable |
| tipo | TEXT NOT NULL | `ed25519`, `rsa`, `ecdsa`, `ed25519-sk`, `ecdsa-sk` |
| huella | TEXT UNIQUE NOT NULL | `SHA256:…` |
| origen | TEXT NOT NULL | `agente`, `fichero`, `token` (claves `-sk`, siempre vía agente) |
| ruta | TEXT NULL | Ruta del fichero privado si `origen = fichero` (o del `.pub` si solo existe ese) |
| comentario | TEXT NULL | Comentario de la clave pública |
| anadida_en | TEXT NOT NULL | Primera vez que MAGI la vio |
| ultimo_uso_en | TEXT NULL | Última autenticación con éxito |
| revocada_en | TEXT NULL | Revocada = ningún host la referencia y no se ofrece en el desplegable |

**REGISTRO**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| fecha | TEXT NOT NULL | |
| tipo | TEXT NOT NULL | `conexion_abierta`, `conexion_fallida`, `huella_aceptada`, `huella_sustituida`, `importacion`, `exportacion`, `clave_generada`, `clave_importada`, `referencia_revocada`, `sondeo_fallido` |
| host_id | INTEGER FK HOSTS ON DELETE SET NULL | |
| identidad_id | INTEGER FK IDENTIDADES ON DELETE SET NULL | |
| detalle | TEXT NOT NULL | Texto libre: motivo del fallo, huellas anterior y nueva, recuentos de importación… |
| resultado | TEXT NOT NULL | `ok` o `error` |

Sin `CHECK` sobre enumeraciones; los valores se validan en código. Las entradas de REGISTRO nunca se editan; solo se purgan por antigüedad.

### Diagrama de estados: host en Flota (derivado del último sondeo)

```
   [FRÍA] ○ ─── sin sondeo guardado
      │ entrar en Flota / r / R / auto-refresco
      ▼
  sondeando ◐ ─── timeout 5 s · DNS/TCP/auth fallan ──▶ [CAÍDA] ✕  (resultado=error, REGISTRO sondeo_fallido)
      │
      │ el host responde pero falta /proc/loadavg o systemctl ──▶ [ALCANZABLE] ● «sin métricas»
      ▼
  parseado ─── ¿algún umbral superado o servicio caído? ──sí──▶ [CARGA] ◐
      │ no
      ▼
   [NOMINAL] ●

  Cualquier estado ──nuevo sondeo──▶ sondeando. Un sondeo nunca modifica HOSTS.ultimo_estado
  (reservado a conexiones reales). Transición inválida: FRÍA → NOMINAL sin pasar por sondeando.
```

### Diagrama de estados: identidad

```
  (escaneo de ~/.ssh y agente)          n generar / i importar
            │                                     │
            ▼                                     ▼
       [detectada] ◀──────────────────── [creada] (fichero escrito, opcional add_identity)
            │
            │ un host la selecciona como identidad_ref
            ▼
        [en uso] ── último host deja de usarla ──▶ [detectada]
            │
            │ x revocar (confirmación: «N hosts pasan a auto»)
            ▼
       [revocada] ── vuelve a aparecer en el escaneo ──▶ sigue revocada (no se resucita sola);
                     e «reactivar» ──▶ [detectada]

  Inválidas: [revocada] → [en uso] directamente (hay que reactivar); borrar la fila (nunca se borra;
  la revocación es la baja lógica). Revocar nunca toca ficheros de ~/.ssh ni el agente.
```

---

## 4. Flujos de Trabajo

### 4.1 Sondeo de la flota

```
  entrar en Flota · r (host) · R (todos) · auto-refresco cada N s
      ▼
  lista de hosts a sondear (filtrados si hay filtro)  ─▶ marcar ◐ sondeando
      ▼
  semáforo de 8 tareas concurrentes; por host:
      ¿sesión viva (F1) con este host? ──sí──▶ canal exec sobre el Handle existente
              │ no
              ▼
      conexión efímera: DNS → TCP → huella (known_hosts; desconocida/cambiada = error, NO diálogo)
              → autenticación (misma resolución que F1; clave cifrada sin frase en memoria = error)
      ──error / timeout 5 s──▶ SONDEOS resultado=error · REGISTRO sondeo_fallido ──▶ [CAÍDA]
              ▼
      exec sondeo.sh (con la lista de servicios del host inyectada como argumentos)
              ▼
      parsear marcadores ── faltan LOADAVG/UPTIME ──▶ resultado=sin_metricas ──▶ [ALCANZABLE]
              ▼
      calcular tasa de red con el sondeo anterior (si existe y < 1 h)
      guardar SONDEOS · purgar > 20 · cerrar conexión efímera
              ▼
      evaluar umbrales ──▶ [NOMINAL] o [CARGA]  ─▶ evento Sondeo(host, resultado) a la UI
```

El sondeo nunca abre diálogos: una huella desconocida o una clave que pide frase se reportan como error con el motivo («huella desconocida: conéctate una vez con ↵»). Así el refresco de 30 hosts no interrumpe al usuario.

### 4.2 Generar una identidad

```
  n en Identidades
      ▼
  [diálogo] nombre de fichero (por defecto id_ed25519_<alias>) · tipo (ed25519 | rsa 4096)
            · comentario · frase (dos veces, opcional) · [x] añadir al agente
      ▼
  ¿existe ~/.ssh/<nombre> o <nombre>.pub? ──sí──▶ error «ya existe», no se sobrescribe nunca
      ▼
  generar con ssh-key (OsRng) · cifrar con la frase si la hay
      ▼
  escribir ~/.ssh/<nombre> (600) y <nombre>.pub (644) vía ficheros.rs (atómico)
      ▼
  [x] añadir al agente ──▶ AgentClient::add_identity (la clave descifrada está en memoria; zeroize después)
                            ── agente no disponible ──▶ aviso, la clave queda solo en fichero
      ▼
  fila IDENTIDADES (origen=fichero o agente si se añadió) · REGISTRO clave_generada
      ▼
  [diálogo] «clave pública copiada al portapapeles» o la línea para copiar a mano
```

### 4.3 Revocar una referencia

```
  x en Identidades sobre una identidad
      ▼
  contar hosts con identidad_ref = esta (por huella o ruta)
      ▼
  [diálogo] «N hosts pasan a identidad auto. ¿Revocar? s/n» (n → fin)
      ▼
  UPDATE HOSTS SET identidad_ref = NULL WHERE … · IDENTIDADES.revocada_en = ahora
      ▶ REGISTRO referencia_revocada · exportar magi_config si exportar_al_guardar
      ▼
  mensaje: «revocada; la clave sigue en ~/.ssh y en el agente: bórrala a mano si procede»
```

### 4.4 Diagrama de secuencia: refresco de Flota

```
 ui/flota   app.rs     flota/mod.rs      conexion/cliente   almacen     host
    │          │             │                  │              │          │
    │──R──────▶│──Sondear───▶│                  │              │          │
    │◀◐ todos──│             │──(x8) conectar──▶│──efímera────────────────▶│
    │          │             │                  │◀─────── exec sondeo.sh ─│
    │          │             │◀─salida──────────│              │          │
    │          │             │──parsear+umbrales│              │          │
    │          │             │──guardar─────────────────────▶│          │
    │          │             │──purgar>20───────────────────▶│          │
    │◀Sondeo(h)│◀────────────│                  │              │          │
    │ (repinta │             │  … siguiente host del semáforo …          │
    │  fila h) │             │                  │              │          │
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi sondear [host…]` | Sondea todos los hosts (o los indicados) sin TUI e imprime una tabla estado/carga/memoria/disco/servicios |
| `magi registro exportar <ruta> [--json] [--desde AAAA-MM-DD]` | Exporta el registro a CSV (por defecto) o JSON |

### 5.2 Atajos globales (novedades)

| Tecla | Acción |
|---|---|
| `F1` | Ir a Flota (vista de arranque desde esta fase) |
| `F5` | Ir a Identidades |
| `F7` | Ir a Registro |
| `F4`, `F6` | Siguen reservadas («vista no disponible en esta fase») |

### 5.3 Flota

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover selección; el panel derecho muestra el host seleccionado |
| `↵` | Conectar (flujo de la Fase 1) |
| `r` | Sondear el host seleccionado |
| `R` | Sondear todos los hosts visibles |
| `/` | Filtrar (mismo filtro que Hosts) |
| `e` | Editar la ficha del host |
| `a` | Activar / desactivar el auto-refresco durante esta ejecución |

### 5.4 Identidades

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover selección; el panel inferior muestra el detalle |
| `n` | Generar clave nueva |
| `i` | Importar: registrar un fichero de clave existente (ruta) |
| `c` | Copiar la clave pública al portapapeles |
| `e` | Editar alias |
| `x` | Revocar referencia (confirmación) o reactivar si está revocada |
| `s` | Volver a escanear `~/.ssh` y el agente |
| `v` | Mostrar / ocultar revocadas |

### 5.5 Registro

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover selección |
| `↵` | Ver detalle completo de la entrada |
| `/` | Filtrar por tipo, host o texto del detalle |
| `t` | Ciclar filtro rápido por tipo (todos → conexiones → huellas → claves → importación → sondeos) |
| `p` | Purgar entradas de más de 90 días (confirmación con recuento) |
| `x` | Exportar a CSV o JSON (diálogo: formato y ruta) |

### 5.6 Ficha de host (novedades)

| Campo | Descripción |
|---|---|
| Servicios | Área de texto en el bloque «Al conectar»: unidades systemd, una por línea |

### 5.7 Paleta de comandos (entradas nuevas)

`sondear · <host>`, `sondear todos`, `ir a flota`, `ir a identidades`, `ir a registro`, `generar clave`, `exportar registro`.

---

## 6. Interfaz de Usuario

### Mapa de navegación (estado tras la Fase 2)

```
                         ┌──────────────┐
                         │   ARRANQUE   │
                         └──────┬───────┘
                                ▼
              ┌──────────── F1  FLOTA ───────────┐  (vista de entrada)
              │      r/R sondear · ↵ conectar    │
              └──┬───────────────┬───────────────┘
                 │ e             │ ↵
     ┌───────────▼────┐ ┌────────▼───────┐
     │  F2  HOSTS     │ │  F3  SESIÓN    │
     │  (F1)          │ │  (F1)          │
     └────────┬───────┘ └────────────────┘
              │ e / n
     ┌────────▼───────┐
     │  ficha de host │ + campo servicios
     └────────────────┘
     ┌────────────────┐ ┌────────────────┐
     │  F5  IDENTIDAD.│ │  F7  REGISTRO  │
     │  n i c e x s v │ │  ↵ / t p x     │
     └────────────────┘ └────────────────┘
     Transversal: Ctrl+P paleta (con entradas nuevas) · ? ayuda
     Reservadas:  F4 Archivos (F4) · F6 Snippets (F4)
```

### 6.1 Flota

```
┌ MAGI · FLOTA ──────────────────────────────────── 15:42:07 ─┐
│                                                             │
│  HOSTS                   │  hetzner-01                      │
│  ──────────────────────  │  ──────────────────────────────  │
│  ● hetzner-01   NOMINAL  │  CARGA ███████░░░░░░░░░  2.7/8   │
│  ● hetzner-02   NOMINAL  │  MEM   ████████████░░░░   61 %   │
│  ◐ mac-mini-m1  CARGA    │  DSK   █████░░░░░░░░░░░   28 %   │
│  ● vps-openclaw NOMINAL  │  NET   ↓ 2.4 MB/s  ↑ 840 kB/s    │
│  ○ dgx-spark    FRÍA     │  UP    14 d 02 h                 │
│  ✕ rincon-dev   CAÍDA    │  ──────────────────────────────  │
│  ● router       ALCANZ.  │  SERVICIOS                       │
│                          │  ● nginx      ● postgresql       │
│  ──────────────────────  │  ● cooperapp  ✕ penwork          │
│  7 hosts · 4 nominal     │  ● tailscale  ● hebb             │
│  sondeo hace 12 s        │  ──────────────────────────────  │
│                          │  sondeado hace 12 s en 640 ms    │
├─────────────────────────────────────────────────────────────┤
│ ↵ ssh   r sondear   R todos   e editar   / buscar   a auto  │
└─────────────────────────────────────────────────────────────┘
```

Estado por glifo y palabra. `CARGA` en ámbar cuando un umbral se supera o un servicio no está `active`; el detalle muestra en ámbar la barra o el servicio culpable. `ALCANZ.` = responde pero sin métricas. Mientras un host se sondea, su glifo es `◐` y la palabra «…».

### 6.2 Identidades

```
┌ MAGI · IDENTIDADES ─────────────────────────── 4 claves ────┐
│                                                             │
│  ALIAS               TIPO        USADA EN   ORIGEN          │
│  ─────────────────────────────────────────────────────────  │
│▸ 4d3-ed25519         ed25519     7 hosts    agente          │
│  personal-ed25519    ed25519     4 hosts    agente          │
│  legacy-rsa          rsa 4096    1 host     fichero         │
│  yubikey-sk          ed25519-sk  2 hosts    token           │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  4d3-ed25519                                                │
│    Huella     SHA256:hq3K…8fMw                              │
│    Vista      12 mar 2026 · usada hace 4 min                │
│    Fichero    — (solo en el agente)                         │
│    Hosts      hetzner-01, hetzner-02, vps-openclaw, +4      │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ n generar  i importar  c copiar pública  e alias  x revocar │
└─────────────────────────────────────────────────────────────┘
```

### 6.3 Diálogo de generación de clave

```
        ┌─ GENERAR CLAVE ──────────────────────────────┐
        │  Fichero     [ id_ed25519_hetzner          ] │
        │  Tipo        (•) ed25519   ( ) rsa 4096      │
        │  Comentario  [ hector@omarchy              ] │
        │  Frase       [ ••••••••••••               ] │
        │  Repetir     [ ••••••••••••               ] │
        │  [x] añadir al agente                        │
        │  [x] copiar la pública al portapapeles       │
        │                                              │
        │  ^s generar          esc cancelar            │
        └──────────────────────────────────────────────┘
```

### 6.4 Registro

```
┌ MAGI · REGISTRO ───────────────────── 214 entradas · todos ─┐
│ / hetz_                                                     │
├─────────────────────────────────────────────────────────────┤
│  FECHA            TIPO                HOST          RES.    │
│  ─────────────────────────────────────────────────────────  │
│▸ 15 sep 15:41:02  conexion_abierta    hetzner-01    ok      │
│  15 sep 15:40:55  sondeo_fallido      rincon-dev    error   │
│  15 sep 12:03:10  huella_aceptada     hetzner-02    ok      │
│  14 sep 23:12:44  exportacion         —             ok      │
│  14 sep 23:11:07  importacion         —             ok      │
│  14 sep 20:02:31  clave_generada      —             ok      │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  sondeo_fallido · rincon-dev · 15 sep 15:40:55              │
│    timeout tras 5 s (TCP 10.0.0.40:22)                      │
├─────────────────────────────────────────────────────────────┤
│ ↵ detalle   t tipo   / buscar   p purgar >90 d   x exportar │
└─────────────────────────────────────────────────────────────┘
```

### 6.5 Ficha de host (bloque «Al conectar» ampliado)

```
│  AL CONECTAR                                                │
│    Multiplexar [x] ControlMaster auto al exportar           │
│    Mantener    [x] keepalive cada [ 30 ] s                  │
│    Servicios   ┌────────────────────────────────────────┐   │
│                │ nginx                                  │   │
│                │ postgresql                             │   │
│                │ cooperapp                              │   │
│                └────────────────────────────────────────┘   │
```

### Notas de UX y diseño

- Flota es la vista de arranque; si no hay ningún host, muestra «Sin hosts: pulsa n en Hosts o I para importar» y `F2` sigue operativo.
- Las barras de Flota usan `█` y `░` (ASCII: `#` y `.`); la tasa de red muestra «—» hasta el segundo sondeo.
- El refresco pinta cada host en cuanto llega su resultado, sin esperar a los demás; la cabecera muestra «sondeando 3/7» mientras dura.
- El auto-refresco se configura en `config.toml` (`[flota] auto_refresco_seg = 0`, 0 = desactivado; mínimo 15) y se pausa cuando la vista activa es Sesión.
- La barra de estado de la vista Sesión (F1) añade «carga X.X» si hay un sondeo del host de menos de 10 min.
- Identidades muestra las claves `-sk` con origen `token` y sin acciones `i`/`c` de fichero; `c` copia la pública si el agente la aporta.
- Registro se ordena de más reciente a más antiguo y carga por páginas de 200.

---

## 7. Lógica de Negocio

### 7.1 Script de sondeo

```sh
#!/bin/sh
# Ejecutado por MAGI vía exec; argumentos: unidades systemd a comprobar.
printf 'MAGI_NUCLEOS=%s\n' "$(nproc 2>/dev/null || echo 1)"
[ -r /proc/loadavg ] && printf 'MAGI_LOADAVG=%s\n' "$(cat /proc/loadavg)"
[ -r /proc/meminfo ] && grep -E '^(MemTotal|MemAvailable):' /proc/meminfo | sed 's/^/MAGI_MEM_/'
df -kP / 2>/dev/null | awk 'NR==2 {print "MAGI_DISCO=" $2 " " $3}'
[ -r /proc/net/dev ] && awk -F'[: ]+' 'NR>2 && $2!="lo" {rx+=$3; tx+=$11} END {print "MAGI_RED=" rx " " tx}' /proc/net/dev
[ -r /proc/uptime ] && printf 'MAGI_UPTIME=%s\n' "$(cut -d' ' -f1 /proc/uptime)"
for u in "$@"; do
  printf 'MAGI_SVC=%s=%s\n' "$u" "$(systemctl is-active "$u" 2>/dev/null || echo unknown)"
done
```

Sin `MAGI_LOADAVG` ni `MAGI_UPTIME` el resultado es `sin_metricas`. Los nombres de unidad se pasan entrecomillados y se validan antes (`[A-Za-z0-9@._-]+`) para que no puedan inyectar shell.

### 7.2 Estados y umbrales

```python
UMBRALES = {"carga_por_nucleo": 1.0, "memoria_pct": 90, "disco_pct": 90}   # config.toml [flota.umbrales]

def estado(sondeo, umbrales):
    if sondeo is None:                     return "FRIA"
    if sondeo.resultado == "error":        return "CAIDA"
    if sondeo.resultado == "sin_metricas": return "ALCANZABLE"
    culpables = []
    if sondeo.carga_1m > umbrales["carga_por_nucleo"] * sondeo.nucleos: culpables.append("carga")
    if 100 * (1 - sondeo.mem_disponible_kb / sondeo.mem_total_kb) > umbrales["memoria_pct"]: culpables.append("memoria")
    if 100 * sondeo.disco_usado_kb / sondeo.disco_total_kb > umbrales["disco_pct"]: culpables.append("disco")
    culpables += [u for u, s in sondeo.servicios.items() if s != "active"]
    return ("CARGA", culpables) if culpables else ("NOMINAL", [])
```

Tasa de red: `(rx_actual - rx_anterior) / segundos_entre_sondeos`, solo si el sondeo anterior es de menos de una hora y los contadores no han retrocedido (reinicio del host → «—»).

### 7.3 Sincronización de IDENTIDADES con el escaneo

Cada escaneo (arranque, `s`, abrir el desplegable de la ficha) hace *upsert* por huella: nueva → fila con `anadida_en`; existente → actualiza `tipo`, `comentario`, `ruta` y `origen` (una clave que estaba solo en fichero y ahora está en el agente pasa a `agente`); revocada → se conserva revocada. Las claves que desaparecen del sistema no se borran (siguen mostrando «no encontrada» en el detalle). `USADA EN` se calcula contando `HOSTS.identidad_ref` que coincidan por huella (`agente:`) o por ruta (`fichero:`).

`ultimo_uso_en` se actualiza desde `conexion/` cuando una autenticación por clave tiene éxito, buscando por huella de la clave usada.

### 7.4 Registro

`registro::anotar(tipo, host_id, identidad_id, detalle, resultado)` es la única puerta de entrada. Los puntos de la Fase 1 que hoy escriben en el log pasan a llamar también a `anotar`: apertura y fallo de conexión (motivo), aceptación y sustitución de huella (huellas anterior y nueva), importación (recuentos) y exportación (número de hosts). El log de `tracing` se mantiene para depuración.

Purga: `DELETE FROM REGISTRO WHERE fecha < ahora - 90 días`, previa confirmación con el recuento. Exportación CSV con cabecera `fecha,tipo,host,identidad,resultado,detalle` (host e identidad por nombre) y JSON como lista de objetos con los mismos campos; se escribe vía `ficheros.rs`.

### 7.5 Casos especiales

- Sondeo de un host cuyo salto está caído: error «salto <nombre>: <motivo>», el host queda CAÍDA y el salto también se marca CAÍDA si estaba en la lista.
- Sondeo con sesión viva en segundo plano: se abre un canal `exec` sobre el mismo `Handle`; si el canal falla se cae a conexión efímera.
- Un host con `servicios` vacío no muestra la sección SERVICIOS.
- Generar con un nombre que ya existe en `~/.ssh` se rechaza siempre; MAGI nunca sobrescribe claves.
- Importar (`i`) una ruta: si existe `<ruta>.pub` se lee la huella de ahí; si no, se intenta leer la privada (pidiendo frase si está cifrada) solo para obtener la huella, y se descarta de memoria.
- Revocar una identidad que usa el host de una sesión viva no afecta a la sesión.
- Un servicio listado en la ficha que no existe en el host aparece como `unknown` y cuenta como no activo (estado CARGA), con el texto «unidad no encontrada».

---

## 8. Requisitos No Funcionales

**Seguridad**

- Las claves generadas se escriben cifradas con la frase indicada (OpenSSH bcrypt-pbkdf) o en claro si el usuario deja la frase vacía, con aviso explícito en el diálogo. La clave descifrada solo vive en memoria para `add_identity` y se destruye con `zeroize`.
- El sondeo nunca acepta ni sustituye huellas; solo las conexiones interactivas lo hacen.
- Los nombres de unidad se validan contra `[A-Za-z0-9@._-]+` antes de pasarlos al script; el script no interpola nada más del inventario.
- El registro no contiene frases, claves ni contenido de sesiones: solo metadatos y motivos.
- `magi sondear` y `magi registro exportar` respetan los mismos permisos (600 en los ficheros exportados).

**Protección de datos (RGPD)**

Igual que la Fase 1: datos del propio usuario sobre su infraestructura, sin terceros. REGISTRO añade un historial de conexiones con marca de tiempo, local y purgable por el usuario.

**Backups y recuperación**

`magi.db` sigue siendo el único fichero a copiar. Los sondeos son desechables (se regeneran); el registro se puede exportar a CSV/JSON como copia legible.

**Rendimiento**

- 50 hosts sondeados en < 10 s con 8 concurrentes y timeout de 5 s; cada resultado repinta su fila sin bloquear.
- SONDEOS acotado a 20 filas por host (1.000 filas para 50 hosts). REGISTRO con índice por `fecha` y por `tipo`; paginación de 200.

**Accesibilidad**

Glifo + palabra + color en Flota; barras con caracteres ASCII en modo degradado; todas las acciones por teclado.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Hosts remotos | `sh` POSIX con coreutils (`nproc`, `df`, `awk`, `grep`, `sed`, `cut`) y `systemctl`; sin ellos → `sin_metricas` |
| `ssh-agent` | `add_identity` para las claves generadas; listado (F1) |
| Portapapeles | `wl-copy` (Wayland), `pbcopy` (macOS); detección por presencia del binario |
| `~/.ssh` | Escritura de pares de claves nuevos (600 / 644); nunca sobrescribe |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D14 — Sondeo con un único script embebido por canal `exec`.** Un viaje por host, sin agentes ni dependencias en el remoto. Descartado ejecutar N comandos separados (N viajes) y descartado instalar nada en el host.
- **D15 — Conexiones efímeras con semáforo de 8 y timeout de 5 s; reutilizar la sesión viva si existe.** Descartadas las conexiones persistentes (coste oculto del informe de diseño); el tiempo real queda fuera del roadmap.
- **D16 — El sondeo nunca dialoga.** Huella desconocida o frase pendiente = error con instrucción; evita que un refresco masivo pida decisiones. Consecuencia: la primera conexión a un host nuevo debe ser interactiva.
- **D17 — Generación de claves con `ssh-key` en proceso, alta en el agente con `add_identity`.** Descartado `ssh-keygen`/`ssh-add` por subproceso: la frase iría por argumentos o por un tty que la TUI ya ocupa.
- **D18 — Revocar es baja lógica de la referencia.** MAGI no borra ficheros de `~/.ssh` ni quita claves del agente: no custodia claves y por tanto no las destruye. La fila se conserva para el historial.
- **D19 — Umbrales globales en `config.toml`.** Por host llegaría en una fase posterior si hace falta; hoy sería complejidad sin uso.
- **D20 — `registro::anotar` como única puerta.** Un punto de entrada evita que cada módulo invente formatos; los eventos de la Fase 1 se reconectan a él. Preparado para las deliberaciones de la Fase 4.
- **D21 — SONDEOS acotado a 20 por host.** Suficiente para la tasa de red y para «hace N s»; el histórico de métricas no es objetivo del producto.
- **D22 — Flota como vista de arranque.** Es la pantalla de entrada del informe de diseño; con inventario vacío redirige a Hosts con un mensaje.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Almacén y registro | Migración 2 (servicios, SONDEOS, IDENTIDADES, REGISTRO), `registro::anotar` y reconexión de los eventos de F1, campo servicios en la ficha, `magi registro exportar`, vista Registro | 1 semana |
| S2 — Sondeo y Flota | Script, ejecución por exec con reutilización de sesión, parser, umbrales y estados, orquestación concurrente, vista Flota, auto-refresco, `magi sondear`, arranque en Flota, carga en la barra de Sesión | 1,5 semanas |
| S3 — Identidades | Sincronización con IDENTIDADES, vista, generar con ssh-key y add_identity, importar, copiar al portapapeles, alias, revocar/reactivar, último uso desde conexion/, entradas nuevas de la paleta | 1 semana |

**Total: 3 sprints, ~3,5 semanas.** Supuestos: un desarrollador con Claude Code, dedicación parcial; `ssh-key` con generación rsa habilitada en la versión que trae russh 0.63 (si no, rsa se pospone y se documenta como desviación). En la práctica el agente suele cerrar una fase así en una o dos sesiones más la validación manual.

---

## 12. Conexiones con Otras Fases

- **Fase 1:** reutiliza `conexion/cliente.rs` (efímeras y canal `exec`), `identidades.rs` (escaneo), `ficheros.rs`, la ficha y el filtro. Los eventos que iban solo al log pasan por `registro::anotar`.
- **Fase 3 (pestañas y túneles):** el sondeo reutilizará cualquiera de las conexiones abiertas; los túneles caídos podrán anotarse en REGISTRO.
- **Fase 4 (SFTP, snippets, deliberación):** las tres comprobaciones de la deliberación («salud del host», «backup reciente», «tests en verde») se apoyan en el último sondeo y en REGISTRO; los forzados se anotan en REGISTRO con tipo nuevo. Los snippets `reiniciar`, `deploy` y `logs` del mockup de Flota llegan con esa fase.
- **Fase 5 (sincronización):** IDENTIDADES y REGISTRO se cifran con el resto; SONDEOS no se sincroniza (es local y desechable).
- **Fase 6 (Android):** la vista compacta muestra el estado de Flota y el `%` de carga del último sondeo sincronizado o sondeado desde el móvil.
