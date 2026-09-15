# MAGI - Fase 4: Archivos (SFTP en panel doble)

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 15 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Especificada sobre las Fases 2 y 3 con implementación reportada pero **sin validación manual completa** (riesgo R15 en el maestro).

---

## 1. Visión General

La Fase 4 añade la vista **Archivos** (`F4`): un panel doble local ⇄ remoto sobre SFTP, con marcadores de diferencia que lo convierten en herramienta de comparación al desplegar, y una cola de transferencias que vive en el servidor de sesiones y sobrevive a cerrar la ventana. Cubre el terreno de Termius que más se echa de menos en terminal y reutiliza todo lo construido: el canal `sftp` se abre sobre la conexión del pool de la Fase 3, los diálogos van al solicitante, el estado se sincroniza por difusión de listas completas y las operaciones remotas quedan en REGISTRO.

**Objetivos principales**

1. Vista Archivos con dos paneles del mismo widget (local y remoto), navegación por teclado, ocultos, orden con directorios primero, filtro y selección múltiple.
2. Marcadores `≠` (existe al otro lado con tamaño o fecha distintos) y `✕` (no existe al otro lado), calculados sobre los directorios visibles.
3. Operaciones: copiar, mover, borrar, renombrar, crear directorio y ver fichero con `$PAGER`.
4. Cola de transferencias en el servidor: una a la vez por conexión, directorios recursivos, progreso por fichero y total, cancelable; panel inferior con la cola y vista ampliada.
5. Diálogo de destino existente (sobrescribir / omitir / todos) y aviso configurable antes de subir ficheros sensibles (`.env*`, `*.pem`, `*.key`, `id_*`).
6. Últimos directorios recordados por host (migración 3), REGISTRO con `transferencia` y `borrado_remoto`, `VERSION_PROTOCOLO = 2`.

**Contexto.** Sin riesgo técnico nuevo comparable al de la Fase 3: `russh-sftp` es un cliente maduro sobre canales russh y el servidor ya tiene el pool, el hilo escritor y la difusión. Lo delicado es el detalle de UX del panel doble (foco, marcas, conflictos), no la infraestructura.

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin cambios de crates principales. Novedades:

| Capa | Tecnología | Notas |
|---|---|---|
| SFTP | `russh-sftp` (versión compatible con russh 0.63.3, fijada) | `SftpSession` por host sobre un canal de la conexión del pool; `readdir`, `stat`, `open`/`read`/`write`, `rename`, `remove`, `rmdir`, `mkdir` |
| Patrones | `globset` | `[archivos] avisar` |
| Panel local | `std::fs` + `tokio::fs` en el cliente | Listado, metadatos, operaciones locales; recorrido recursivo para subidas |
| Temporales | `$XDG_RUNTIME_DIR/magi/tmp/` (700) | Descargas para «ver»; se borran al cerrar el visor |
| Visor | `$PAGER` (por defecto `less`) | La TUI se suspende y se restaura, como al conectar en F1 |
| Protocolo | `VERSION_PROTOCOLO = 2` | Mensajes SFTP nuevos (§5.2); un servidor v1 se rechaza en el saludo con instrucción |

### Estructura de carpetas (novedades)

```
src/
├── almacen/migraciones.rs   migración 3: HOSTS.sftp_dir_local, HOSTS.sftp_dir_remoto
├── protocolo.rs             VERSION_PROTOCOLO = 2 + mensajes SFTP y de transferencias
├── servidor/
│   ├── sftp.rs              SftpSession por host sobre el pool; listar, stat, rename, remove, mkdir; temporales
│   └── transferencias.rs    cola por host, una en curso por conexión, recursión, progreso, cancelación, Transferencias{lista}
├── archivos/
│   ├── mod.rs               modelo de panel: Entrada {nombre, tipo, tamano, mtime, permisos, propietario, marca}
│   ├── local.rs             listado y operaciones locales, recorrido recursivo
│   ├── marcas.rs            cálculo de ≠ / ✕ entre dos listados
│   └── sensibles.rs         patrones [archivos] avisar
├── config.rs                + [archivos] avisar, mostrar_ocultos, pager
└── ui/
    ├── archivos.rs          F4: dos paneles, panel de cola, filtro, marcas
    ├── transferencias.rs    vista ampliada de la cola
    └── dialogos.rs          conflicto, aviso sensible, renombrar, crear directorio, ir a ruta, borrar
docs/fase4/
├── magi-fase4-informe.md · magi-fase4-prompt.md · magi-fase4-checklist.md
└── magi-fase4-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura (novedades sobre la Fase 3)

```
 cliente magi (ventana)                            magi --servidor
 ┌──────────────────────────────┐                 ┌──────────────────────────────────┐
 │ ui/archivos.rs               │  ListarDir ───▶ │ servidor/sftp.rs                 │
 │  panel local ◀── archivos/   │ ◀── DirListado  │  host_id → SftpSession           │
 │  panel remoto ◀── protocolo  │  Transferir ──▶ │   (canal sftp sobre el pool)     │
 │  marcas.rs (≠ ✕)             │ ◀── Transferencias{lista}                          │
 │  sensibles.rs (aviso)        │  Cancelar ────▶ │ servidor/transferencias.rs       │
 │  cola (panel inferior)       │  BorrarRemoto…▶ │  cola por host · 1 en curso      │
 │                              │ ◀── RutaTemporal│  progreso · recursión · temporales│
 │ $PAGER (TUI suspendida)      │                 │  registro::anotar (transferencia,│
 └──────────────────────────────┘                 │   borrado_remoto) vía hilo BD    │
         │ std::fs (local)                        └───────────────┬──────────────────┘
         ▼                                                        │ sftp (subsistema)
   disco local                                              host remoto
```

**Reparto.** El servidor hace todo lo remoto (listar, transferir, borrar, renombrar, crear, descargar temporales) y guarda la cola; el cliente hace todo lo local, calcula las marcas y decide conflictos y avisos antes de encolar (T15: el servidor nunca dialoga). Al abrir Archivos para un host, el servidor abre (o reutiliza) el canal `sftp` sobre la conexión del pool; si no hay conexión, la abre con el flujo de la Fase 3 (huella, frase, contraseña al solicitante) y la mete en el pool respetando `multiplexar`.

---

## 3. Modelo de Datos

### HOSTS (migración 3)

| Campo | Tipo | Descripción |
|---|---|---|
| sftp_dir_local | TEXT NULL | Último directorio local visitado con este host |
| sftp_dir_remoto | TEXT NULL | Último directorio remoto visitado; NULL = directorio de inicio del usuario remoto |

Sin más tablas. El E-R de la Fase 2 no cambia. REGISTRO añade `transferencia` y `borrado_remoto`.

### Estructuras en memoria

```
 servidor                                     cliente
 ┌──────────────────────────────────┐         ┌──────────────────────────────────┐
 │ Transferencia                    │         │ Panel                            │
 │ id            u32 creciente      │         │ lado          local | remoto     │
 │ host_id                          │         │ ruta                             │
 │ direccion     subida | bajada    │         │ entradas      [Entrada]          │
 │ origen · destino (rutas)         │         │ seleccion     índice             │
 │ es_directorio bool               │         │ marcados      {nombre}           │
 │ ficheros      [(origen,destino,  │         │ filtro        String             │
 │                 bytes)] expandido│         │ ocultos       bool               │
 │ bytes_total · bytes_hechos       │         └──────────────────────────────────┘
 │ fichero_actual · fichero_bytes   │         ┌──────────────────────────────────┐
 │ estado  en_cola|en_curso|hecha   │         │ Entrada                          │
 │         |error|cancelada         │         │ nombre · tipo (dir|fichero|enlace)│
 │ error         Option<String>     │         │ tamano · mtime · permisos ·      │
 │ solicitante · creada_en ·        │         │ propietario · marca (≠ ✕ ninguna)│
 │ terminada_en                     │         └──────────────────────────────────┘
 └──────────────────────────────────┘
 ┌──────────────────────────────────┐
 │ SftpHost                         │
 │ host_id · SftpSession · canal    │
 │ dir_inicio · ultima_actividad    │
 │ en_curso  Option<transferencia>  │
 └──────────────────────────────────┘
```

### Diagrama de estados: transferencia

```
  Transferir (cliente, ya sin conflictos ni avisos pendientes)
        ▼
    en_cola ──Cancelar──▶ cancelada
        │ le toca (una en curso por host)
        ▼
    en_curso ──Cancelar (se comprueba por bloque de 64 KiB)──▶ cancelada (destino parcial borrado)
        │  │ fallo de E/S, canal cerrado, permiso denegado ──▶ error (detalle)
        ▼
      hecha
  hecha / error / cancelada ─▶ REGISTRO transferencia (resultado ok | error, detalle) ─▶ se conserva en la cola
  hasta «limpiar terminadas» (C) o 1 h ─▶ eliminada
  Inválidas: hecha → en_curso (no se reintenta en sitio: se encola otra); en_cola → hecha sin pasar por en_curso;
  cancelar una hecha.
```

### Diagrama de estados: canal SFTP por host

```
  [sin canal] ──AbrirSftp──▶ abriendo (conexión del pool o nueva con diálogos al solicitante)
                                │ error ─▶ [sin canal] + Error al solicitante
                                ▼
                             abierto ──sin actividad 10 min y sin transferencias──▶ cerrado ─▶ [sin canal]
                                │ conexión caída ─▶ caido: transferencias en curso → error «conexión caída»;
                                │                   el siguiente AbrirSftp vuelve a abriendo
  Un canal por host compartido por todas las ventanas; no cuenta como sesión (no aparece en pestañas).
```

---

## 4. Flujos de Trabajo

### 4.1 Abrir Archivos

```
  F4 (host de la pestaña activa o seleccionado) · s en Hosts/Flota · paleta «sftp · <host>»
      ▼
  cliente: panel local = HOSTS.sftp_dir_local o ~ ; lista local (std::fs)
  cliente: AbrirSftp{host_id} ─▶ servidor: ¿SftpSession abierta? ─no─▶ conexión del pool o nueva
                                                        (huella/frase/contraseña → solicitante; error → Error{motivo})
      ▼
  servidor: SftpAbierto{host_id, dir_inicio} ─▶ cliente: ruta remota = HOSTS.sftp_dir_remoto o dir_inicio
      ▼
  ListarDir{host_id, ruta} ─▶ DirListado{host_id, ruta, entradas[]} (o Error si no existe → se cae a dir_inicio)
      ▼
  marcas.rs compara los dos listados ─▶ pinta ambos paneles con ≠ / ✕ ─▶ guarda sftp_dir_* al cambiar de directorio
```

### 4.2 Copiar (transferir)

```
  c sobre marcados (o la fila actual) en el panel activo
      ▼
  destino = ruta del otro panel
  ¿subida y algún nombre coincide con [archivos] avisar? (se recorren los directorios locales)
      ─sí─▶ [diálogo AVISO: «vas a subir .env (2 kB) a /var/www/cooperapp» s/n] ─n─▶ fin
      ▼
  ¿existe ya en destino? (por el listado del otro panel; un directorio se trata como unidad)
      ─sí─▶ [diálogo: sobrescribir / omitir / sobrescribir todos / omitir todos, con tamaño y fecha de ambos]
              (para un directorio, la decisión aplica a todo su contenido)
      ▼
  Transferir{host_id, direccion, elementos[(origen, destino)], politica} ─▶ servidor: expande directorios
      (subida: el cliente ya envía la lista de ficheros; bajada: el servidor recorre con readdir), calcula bytes_total,
      encola ─▶ Transferencias{lista} a todos ─▶ panel de cola muestra «en cola 3 · 12,4 MB»
      ▼
  servidor: transfiere uno a uno (bloques de 64 KiB; mtime conservado) ─▶ Transferencias{lista} (≤ 4/s)
      │ error en un fichero ─▶ error en la transferencia, se continúa con la siguiente de la cola
      ▼
  hecha ─▶ REGISTRO transferencia ─▶ cliente: refresca el panel de destino y recalcula marcas
```

### 4.3 Ver un fichero

```
  ↵ sobre un fichero
      ├─ local ─▶ suspender TUI ─▶ $PAGER <ruta> ─▶ restaurar
      └─ remoto ─▶ ¿tamaño > 10 MB? ─sí─▶ confirmación
                   ▼
                   DescargarTemporal{host_id, ruta} ─▶ servidor: copia a $XDG_RUNTIME_DIR/magi/tmp/<id>-<nombre> (600)
                   ─▶ RutaTemporal{ruta} ─▶ suspender TUI ─▶ $PAGER ─▶ restaurar ─▶ BorrarTemporal{ruta}
```

### 4.4 Mover, borrar, renombrar, crear directorio

```
  m  mismo panel de origen y destino → no aplica; entre paneles → transferencia con politica + borrado del origen al terminar
     (borrado local en el cliente; remoto vía BorrarRemoto al recibir hecha)
  x  [diálogo: «borrar N elementos (K directorios) de <lado>» s/n] ─▶ local: fs::remove_*; remoto: BorrarRemoto{host_id, rutas}
     (recursivo en el servidor) ─▶ REGISTRO borrado_remoto ─▶ refresco + marcas
  r  [diálogo nombre] ─▶ local: fs::rename; remoto: RenombrarRemoto{host_id, de, a}
  d  [diálogo nombre] ─▶ local: fs::create_dir; remoto: CrearDirRemoto{host_id, ruta}
  Todas las remotas responden Hecho{peticion_id} o Error{peticion_id, motivo}; el cliente refresca el panel afectado.
```

### 4.5 Diagrama de secuencia: subida con conflicto

```
 ui/archivos   marcas/sensibles   servidor          host
    │──c────────▶│                    │               │
    │◀─aviso .env│ (diálogo s)        │               │
    │◀─conflicto─│ (diálogo sobrescr.)│               │
    │──Transferir{…, politica}───────▶│               │
    │◀─Transferencias[en_cola]────────│               │
    │            │                    │──sftp open/write──▶│
    │◀─Transferencias[en_curso 34 %]──│  (≤ 4/s)      │
    │◀─Transferencias[hecha]──────────│──setstat mtime────▶│
    │──ListarDir────────────────────▶│──readdir──────────▶│
    │◀─DirListado────────────────────│               │
    │  (marcas recalculadas: ≠ desaparece)           │
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

Ninguno nuevo. `magi servidor estado` añade la cola de transferencias (en curso y en cola por host).

### 5.2 Protocolo (mensajes nuevos, `VERSION_PROTOCOLO = 2`)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `AbrirSftp{host_id}` | `SftpAbierto{host_id, dir_inicio}` |
| `ListarDir{host_id, ruta, peticion_id}` | `DirListado{host_id, ruta, entradas, peticion_id}` |
| `Transferir{host_id, direccion, elementos, politica}` | `Transferencias{lista}` (difusión, ≤ 4/s) |
| `CancelarTransferencia{id}` / `LimpiarTransferencias` | `Hecho{peticion_id}` / `Error{peticion_id, mensaje}` |
| `BorrarRemoto{host_id, rutas, peticion_id}` / `RenombrarRemoto{host_id, de, a, peticion_id}` / `CrearDirRemoto{host_id, ruta, peticion_id}` | |
| `DescargarTemporal{host_id, ruta, peticion_id}` / `BorrarTemporal{ruta}` | `RutaTemporal{ruta, peticion_id}` |

`Bienvenida` añade `transferencias` (lista actual) para que una ventana nueva vea la cola.

### 5.3 Vista Archivos

| Tecla | Acción |
|---|---|
| `Tab` | Cambiar de panel (local ⇄ remoto) |
| `↑` `↓` / `j` `k` | Mover; `PgUp` `PgDn` página; `Home` `End` |
| `↵` | Directorio: entrar; fichero: ver con `$PAGER`; enlace: seguir |
| `Backspace` / `-` | Subir al directorio padre (también la entrada `..`) |
| `g` | Ir a ruta (diálogo con la ruta actual editable; `~` se expande) |
| `Espacio` | Marcar / desmarcar la fila y bajar una |
| `a` / `A` | Marcar todo / desmarcar todo |
| `c` | Copiar al otro panel |
| `m` | Mover al otro panel |
| `x` | Borrar (confirmación con recuento) |
| `r` | Renombrar |
| `d` | Crear directorio |
| `.` | Mostrar / ocultar ocultos |
| `/` | Filtrar por nombre en el panel activo |
| `R` | Refrescar ambos paneles |
| `h` | Cambiar de host (paleta filtrada a `sftp · <host>`) |
| `t` | Ir a la cola de transferencias (vista ampliada) |
| `i` | Detalle de la entrada (permisos, propietario, fecha completa, destino del enlace) |
| `q` / `Esc` | Volver (la cola sigue en el servidor); `Esc` primero limpia filtro o marcados |

### 5.4 Vista Transferencias (cola ampliada)

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover |
| `x` | Cancelar la seleccionada (en cola o en curso; confirmación si en curso) |
| `C` | Limpiar terminadas (hechas, error, canceladas) |
| `↵` | Detalle (rutas, bytes, error) |
| `q` / `Esc` | Volver a Archivos |

### 5.5 Otras vistas y paleta

| Dónde | Novedad |
|---|---|
| Hosts y Flota | `s` abre Archivos con el host seleccionado |
| Sesión | prefijo + `f` abre Archivos con el host de la pestaña |
| Paleta | `sftp · <host>`, `transferencias`, `cancelar transferencias` |
| `config.toml` | `[archivos] avisar = [".env*", "*.pem", "*.key", "id_*"]`, `mostrar_ocultos = false`, `pager` (por defecto `$PAGER` o `less`) |

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   F2 HOSTS ──s──┐   F1 FLOTA ──s──┐   F3 SESIÓN ──prefijo f──┐   Ctrl+P «sftp · host»
                 ▼                 ▼                          ▼
        ┌──────────────────── F4 ARCHIVOS ──────────────────────────┐
        │  panel local │ panel remoto  ·  cola (3 filas)            │
        │  ↵ ver/entrar · c m x r d · Espacio · / · . · g · h · i   │
        └────────────────────────┬──────────────────────────────────┘
                              t  │  ▲ q
                                 ▼  │
        ┌──────────── TRANSFERENCIAS (cola ampliada) ───────────────┐
        │  x cancelar · C limpiar · ↵ detalle                       │
        └───────────────────────────────────────────────────────────┘
        Diálogos: aviso sensible · conflicto · borrar · renombrar · crear · ir a ruta · > 10 MB
```

### 6.1 Archivos

```
┌ MAGI · ARCHIVOS ───────────── local ⇄ hetzner-01 ───────────┐
│  ~/proyectos/cooperapp        │  /var/www/cooperapp         │
│  ─────────────────────────    │  ─────────────────────────  │
│  ▸ ..                         │    ..                       │
│    app/            —   12 sep │    app/            —   12 sep│
│    static/         —   03 sep │    static/         —   03 sep│
│  * main.py         14 kB  hoy │    main.py         14 kB 12 sep ≠│
│    config.yaml      2 kB  hoy │    config.yaml      1 kB 12 sep ≠│
│    .env             1 kB  ayer ✕                             │
│                               │                             │
│  ─────────────────────────    │  ─────────────────────────  │
│  6 elementos · 1.2 MB · 1 marcado │ 5 elementos · 1.1 MB     │
├─────────────────────────────────────────────────────────────┤
│  ⇄ main.py → hetzner-01   ████████████████░░░░  78 %  1.1 MB │
│  en cola 2 · 240 kB                                         │
├─────────────────────────────────────────────────────────────┤
│ ⇥ panel  ↵ abrir  c copiar  m mover  x borrar  r d  . /  t  │
└─────────────────────────────────────────────────────────────┘
```

`*` es una fila marcada; `≠` y `✕` se pintan en ámbar y rojo al final de la fila del lado activo y del otro. Con filtro activo, la cabecera del panel muestra `/ texto`.

### 6.2 Diálogo de conflicto

```
        ┌─ YA EXISTE EN EL DESTINO ───────────────────────────┐
        │  config.yaml                                       │
        │    local    2 kB   15 sep 2026 14:02   (origen)    │
        │    remoto   1 kB   12 sep 2026 09:41   (destino)   │
        │                                                    │
        │  s sobrescribir   o omitir   S todos   O omitir todos│
        │  esc cancelar                                      │
        └────────────────────────────────────────────────────┘
```

### 6.3 Aviso de fichero sensible

```
        ┌─ AVISO ─────────────────────────────────────────────┐
        │  ✕ Vas a subir un fichero que coincide con «.env*»: │
        │    .env  (1 kB)  →  hetzner-01:/var/www/cooperapp   │
        │                                                     │
        │  Suele contener secretos. ¿Subirlo igualmente?      │
        │  s subir          esc cancelar (recomendado)        │
        └─────────────────────────────────────────────────────┘
```

### 6.4 Transferencias (cola ampliada)

```
┌ MAGI · TRANSFERENCIAS ─────────────────── 1 en curso · 2 en cola ┐
│  ESTADO   DIRECCIÓN  ORIGEN → DESTINO                   PROGRESO │
│  ────────────────────────────────────────────────────────────────│
│▸ ◐ curso  ↑ subida   main.py → hetzner-01:/var/www/…   78 % 1.1 MB│
│  ○ cola   ↑ subida   static/ (14 fich.) → hetzner-01   240 kB    │
│  ○ cola   ↓ bajada   hetzner-01:/var/log/nginx/… → ~/  3.2 MB    │
│  ● hecha  ↑ subida   config.yaml → hetzner-01          2 kB 0,3 s│
│  ✕ error  ↓ bajada   hetzner-02:/root/backup.tgz → ~/  permiso   │
│                                                                  │
│  ────────────────────────────────────────────────────────────────│
│  main.py → hetzner-01:/var/www/cooperapp/main.py                 │
│    1.1 MB de 1.4 MB · 3.8 MB/s · 0,1 s restantes · ventana 1     │
├──────────────────────────────────────────────────────────────────┤
│ x cancelar   C limpiar terminadas   ↵ detalle   q volver         │
└──────────────────────────────────────────────────────────────────┘
```

### 6.5 Detalle de entrada (`i`)

```
        ┌─ main.py ───────────────────────────────────────────┐
        │  Tipo        fichero        Tamaño   14 208 bytes   │
        │  Modificado  12 sep 2026 09:41:07                   │
        │  Permisos    -rw-r--r--     hector:hector           │
        │  Ruta        /var/www/cooperapp/main.py             │
        │  Otro lado   ~/proyectos/cooperapp/main.py  ≠ tamaño│
        └─────────────────────────────────────────────────────┘
```

### Notas de UX y diseño

- Un panel inactivo se pinta con el marco tenue; el activo lleva el nombre en ámbar. `Tab` no pierde la selección de ninguno.
- Las columnas son nombre (truncado con `…`), tamaño (`—` para directorios, unidades legibles) y fecha (`hoy`, `ayer`, `12 sep`, `2025`). Enlaces simbólicos con `@` tras el nombre.
- Marcas: se recalculan al cambiar de directorio o refrescar; comparan nombre, tipo, tamaño y mtime (tolerancia 2 s); un directorio se marca `✕` si no existe al otro lado y nunca `≠`.
- La cola ocupa 3 filas en Archivos (en curso + resumen) y se oculta si está vacía; `t` la amplía.
- Al salir de Archivos con transferencias en curso no se pregunta: siguen en el servidor y `magi servidor estado` las lista.
- Mientras un fichero se transfiere, el destino se escribe con sufijo `.magi-parcial` y se renombra al terminar; una cancelación o error borra el parcial.
- Modo ASCII: `*` marcado, `!=` y `x` como marcas, `#`/`.` en barras.
- El visor suspende la TUI como la conexión de la F1; si `$PAGER` no existe, mensaje y se ofrece `less`/`more` detectados.

---

## 7. Lógica de Negocio

### 7.1 Marcas

```python
def marcas(local, remoto):                       # dos dicts nombre → Entrada de los directorios visibles
    for nombre, e in local.items():
        otro = remoto.get(nombre)
        if otro is None:                         e.marca = "✕"
        elif e.tipo == "dir" or otro.tipo == "dir": e.marca = "" if e.tipo == otro.tipo else "≠"
        elif e.tamano != otro.tamano or abs(e.mtime - otro.mtime) > 2: e.marca = "≠"
        else:                                    e.marca = ""
    for nombre, e in remoto.items():
        if nombre not in local:                  e.marca = "✕"
        else:                                    e.marca = local[nombre].marca
```

Las marcas son informativas: no hay «sincronizar directorio» en esta fase (queda para snippets/deploy).

### 7.2 Aviso de sensibles

`[archivos] avisar` es una lista de globs (`globset`) que se comprueba, solo en subidas, contra el nombre de cada fichero que se va a transferir, incluidos los de dentro de directorios (el cliente recorre los directorios locales antes de encolar). Un aviso por transferencia listando hasta 5 coincidencias y «+N». Cancelar no encola nada.

### 7.3 Conflictos

El cliente detecta conflictos con el listado del otro panel para los elementos de primer nivel. La política resultante (`sobrescribir`, `omitir`, `preguntar` no existe para el servidor) viaja en `Transferir` por elemento; para un directorio aplica a todo su contenido. El servidor comprueba de nuevo la existencia justo antes de escribir (el listado puede estar desactualizado) y aplica la política; con `omitir` cuenta el fichero como omitido en el detalle.

### 7.4 Cola

Una transferencia en curso por host (una `SftpSession`, secuencial, sin paralelismo dentro del host); hosts distintos avanzan en paralelo. Orden FIFO por host. Progreso difundido con `Transferencias{lista}` a lo sumo 4 veces por segundo (coalescido) y siempre en cada cambio de estado. Velocidad y restante calculados en el cliente con los dos últimos `Transferencias`. Terminadas se conservan 1 h o hasta `C`.

### 7.5 Recursión

- Subida: el cliente recorre el directorio local, crea la lista `[(origen, destino, bytes)]` y la envía; el servidor crea los directorios remotos que falten (`mkdir` en cadena).
- Bajada: el servidor recorre con `readdir` y construye la lista, y escribe **localmente** los ficheros y directorios (es el mismo usuario y la misma máquina, D41); el cliente solo refresca al recibir `hecha`.
- Enlaces simbólicos: se copian como el fichero apuntado si es fichero; los enlaces a directorios no se siguen (se omiten con nota).
- mtime del origen se conserva en el destino (`setstat` remoto / `filetime` local).

### 7.6 Mover

Entre paneles: `Transferir` con `borrar_origen = true`; el servidor borra el origen remoto tras `hecha` (bajada) o el cliente borra el origen local tras `hecha` (subida) — nunca antes. En el mismo panel, `m` equivale a `r` (renombrar).

### 7.7 Casos especiales

- Directorio remoto sin permiso de lectura: `Error` y el panel se queda en el anterior con mensaje.
- Ruta remota guardada que ya no existe: se cae a `dir_inicio` con mensaje.
- Ficheros > 2 GiB: soportados (offsets de 64 bits); ver > 10 MB pide confirmación.
- Nombres con espacios o UTF-8 raro: se muestran tal cual; nunca se interpolan en un shell (todo va por SFTP).
- Cambiar de host con `h` conserva el panel local y cambia el remoto; la cola es global.
- Si la conexión del pool se cae con transferencias, quedan en `error «conexión caída»`; el siguiente `AbrirSftp` reabre el canal; no se reanudan solas.
- Borrar en local nunca pasa por la papelera: es `fs::remove_*` con confirmación previa explícita («N elementos, sin papelera»).

---

## 8. Requisitos No Funcionales

**Seguridad**

- El servidor solo escribe en disco local dentro de las rutas que el cliente le indica en `Transferir`/`DescargarTemporal`, y los temporales van a un directorio 700 con nombre único y se borran al cerrar el visor (o al apagar el servidor).
- Ninguna ruta se pasa por un shell; todo va por SFTP o `std::fs`.
- Aviso de sensibles configurable; no se puede desactivar dejando la lista vacía sin que la TUI lo indique en la barra al abrir Archivos.
- Los borrados remotos y todas las transferencias quedan en REGISTRO con rutas y bytes; nunca con contenido.

**Protección de datos (RGPD)**

Sin cambios: ficheros del propio usuario; REGISTRO guarda rutas (metadatos), no contenidos.

**Backups y recuperación**

Nada nuevo que persistir salvo dos columnas en HOSTS. Una transferencia interrumpida deja como mucho un `.magi-parcial`, que se borra.

**Rendimiento**

- Listado de un directorio de 5.000 entradas < 1 s (una llamada `readdir` paginada por `russh-sftp`).
- Transferencia con bloques de 64 KiB y hasta 16 peticiones en vuelo por fichero (`russh-sftp` lo permite): objetivo ≥ 20 MB/s en LAN.
- Marcas calculadas en el cliente en O(n) sobre los dos listados.

**Accesibilidad**

Marcas con glifo y color, y texto en el detalle (`≠ tamaño`); ASCII `!=` / `x`; todo por teclado.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Subsistema SFTP del host | `russh-sftp` versión 3 del protocolo; si el host no tiene subsistema `sftp` → `Error «el host no ofrece SFTP»` |
| `$PAGER` | Visor de ficheros; la TUI se suspende |
| Sistema de ficheros local | Panel local y escrituras de bajadas desde el servidor (mismo usuario) |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D40 — SFTP entero en el servidor (T23).** Listados por petición/respuesta y cola en el servidor: las transferencias sobreviven a la ventana y todas las ventanas ven la misma cola. Descartada la navegación en el cliente con conexión propia: dos conexiones por host y una cola que no se comparte.
- **D41 — El servidor escribe las bajadas directamente en el disco local.** Es el mismo usuario y máquina; enviar los bytes al cliente por el socket duplicaría el tráfico y ataría la bajada a la ventana.
- **D42 — Conflictos y avisos se deciden en el cliente antes de encolar; el servidor recomprueba y aplica la política.** T15: el servidor no dialoga. Para directorios la decisión aplica a todo el contenido: preguntar fichero a fichero dentro de una cola desatendida es imposible sin romper T15.
- **D43 — Una transferencia en curso por host, FIFO.** Una `SftpSession` por host; el paralelismo lo da `russh-sftp` con peticiones en vuelo dentro del fichero. Descartadas 3 en paralelo (Hector).
- **D44 — Marcas por nombre, tipo, tamaño y mtime con tolerancia de 2 s.** Suficiente para «¿qué cambió en el deploy?»; el hash es lento y necesitaría leer los ficheros remotos.
- **D45 — `VERSION_PROTOCOLO = 2` (precisa T24).** Se incrementa también cuando el cliente depende de mensajes que un servidor anterior no tiene; el saludo lo detecta y la TUI dice «servidor de una versión anterior: magi servidor parar».
- **D46 — Últimos directorios en HOSTS (migración 3), no en config.** Son por host y deben sincronizarse en la F7.
- **D47 — Sin papelera, sin `chmod`, sin sincronizar directorio.** Alcance acotado (Hector); `chmod` y sincronización son candidatos para fases posteriores.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Servidor SFTP | `russh-sftp` fijado, `servidor/sftp.rs` (canal sobre el pool, listar, stat, rename, remove, mkdir, temporales, cierre por inactividad), protocolo v2, migración 3, `servidor estado` con cola | 1 semana |
| S2 — Paneles | `archivos/` (modelo, local, marcas, sensibles), vista Archivos con dos paneles, navegación, filtro, ocultos, marcados, detalle, ir a ruta, ver con `$PAGER`, `s`/`prefijo f`/paleta, últimos directorios | 1,5 semanas |
| S3 — Transferencias | `servidor/transferencias.rs` (cola, recursión, progreso, cancelación, parciales, mtime), diálogos de conflicto y aviso, copiar/mover/borrar/renombrar/crear, panel de cola y vista ampliada, REGISTRO, tests | 1 semana |

**Total: 3 sprints, ~3,5 semanas.** Supuestos: `russh-sftp` compatible con russh 0.63.3 sin parches (si no, se fija la última compatible y se anota); tests con el `sshd` efímero de F2/F3 que ya ofrece subsistema `sftp`.

---

## 12. Conexiones con Otras Fases

- **Fase 3:** reutiliza pool, solicitante, hilo escritor, difusión de listas (T27) y `servidor estado`. `Bienvenida` gana `transferencias`.
- **Fase 5 (Túneles):** misma estructura: túneles en el servidor, `Tuneles{lista}`, protocolo v3 si añade mensajes.
- **Fase 6 (Snippets y deliberación):** «deploy» = snippet que puede encolar transferencias (subida de un directorio) antes de ejecutar; la sincronización de directorio se apoya en `marcas.rs`.
- **Fase 7 (Sincronización):** `sftp_dir_local`/`sftp_dir_remoto` viajan con HOSTS; la cola no se sincroniza.
- **Fase 8 (Android):** panel doble reimplementado en la app; la lógica de marcas es portable.
