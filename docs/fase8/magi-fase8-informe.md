# MAGI - Fase 8: Archivos, Segunda Vuelta

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 3 de octubre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-7 con informe procesado; la Fase 7 validada por Hector. Las Fases 2-6 siguen sin validación manual completa (R15) y el código de S1-S3 de la Fase 6 sin revisión adversarial (R38, aceptado).

---

## 1. Visión General

La Fase 4 dejó un panel doble para mirar, copiar y borrar. La Fase 8 lo convierte en la herramienta con la que se despliega a mano: **editar un fichero remoto con tu editor**, **cambiar permisos**, ver **propietarios con nombre** y **sincronizar un directorio** en cualquier dirección con vista previa, exclusiones y sincronizaciones guardadas para repetirlas con una tecla. Una sincronización hacia un host que exige verificaciones, o que va a borrar ficheros, es un despliegue y pasa por la deliberación MAGI de la Fase 6.

Todo se apoya en lo existente: la cola de transferencias del servidor (F4), el temporal privado del visor (F4), las marcas `≠`/`✕` (F4) llevadas a todo el árbol, el aviso de sensibles (F4), la deliberación (F6) y la disposición adaptable (F7).

**Objetivos principales**

1. Edición remota con `E`: bajar a temporal, abrir `$EDITOR`, detectar cambios, avisar si el remoto cambió entretanto y subir conservando permisos. El temporal nunca se borra si la subida no terminó.
2. `chmod` con casillas rwx y octal sincronizados, sobre marcados, con recursivo opcional (todo, solo directorios o solo ficheros).
3. Propietario y grupo con nombre, leyendo `/etc/passwd` y `/etc/group` del host por SFTP.
4. Sincronizar directorio local ⇄ remoto con plan recursivo, vista previa, borrado opcional de lo que sobra, exclusiones por defecto y `.magiignore`.
5. Sincronizaciones guardadas por host (tabla nueva) ejecutables desde Archivos y la paleta, con deliberación cuando corresponde.
6. Protocolo v5 y REGISTRO con `sincronizacion` y `permisos_cambiados`.

**Contexto.** Fase media. La sincronización de directorio se hace aquí y no en Snippets: un futuro snippet de deploy podrá invocarla. Fuera de alcance: `chown`, papelera, sincronización por hash, sincronización continua (vigilar cambios) y edición simultánea de varios ficheros con fusión.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Exclusiones | `ignore` (`gitignore::GitignoreBuilder`) | Semántica de gitignore para los patrones por defecto, `.magiignore` y extras |
| Detección de cambios | `sha2` (SHA-256 del temporal antes y después de editar) | Ya está en el árbol de dependencias de russh |
| Editor | `[archivos] editor` → `$VISUAL` → `$EDITOR` → `nvim` → `vi` | Programa + argumentos, sin shell (T30); suspensión de la TUI con pausa de teclado con acuse (T52) |
| Permisos | SFTP `setstat` (remoto), `std::fs::set_permissions` (local) | |
| Nombres de propietario | Lectura de `/etc/passwd` y `/etc/group` por SFTP en el servidor | Tope 1 MiB por fichero, plazo 2 s; sin ejecutar comandos |
| Protocolo | `VERSION_PROTOCOLO = 5` | Mensajes nuevos y ampliaciones (§5.2) |

### Estructura de carpetas (novedades)

```
src/
├── almacen/
│   ├── migraciones.rs        migración 6: SINCRONIZACIONES_DIR
│   └── sincronizaciones.rs   CRUD (cliente)
├── archivos/
│   ├── edicion.rs            ciclo de una edición: temporal, hash, conflicto, copia local
│   ├── permisos.rs           modo ⇄ casillas, octal, alcance recursivo
│   ├── plan.rs               plan de sincronización recursivo (crear / actualizar / borrar / omitir)
│   └── exclusiones.rs        patrones por defecto + .magiignore + extras con `ignore`
├── servidor/
│   ├── sftp.rs               + StatRemoto, CambiarPermisos (recursivo), ListarArbol por bloques,
│   │                           mapa uid/gid → nombre por host
│   └── transferencias.rs     + permisos por elemento, borrar_al_terminar, deliberación y sincronización
└── ui/
    ├── archivos.rs           E · p · S · L
    ├── permisos.rs           diálogo chmod
    ├── sincronizar.rs        diálogo SINCRONIZAR y vista previa del plan
    └── sincronizaciones.rs   lista de sincronizaciones guardadas del host
docs/fase8/
├── magi-fase8-informe.md · magi-fase8-prompt.md · magi-fase8-checklist.md
└── magi-fase8-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 cliente magi                                                        magi --servidor
 ┌──────────────────────────────────────────────┐                   ┌────────────────────────────────────┐
 │ ui/archivos.rs                               │ DescargarTemporal │ servidor/sftp.rs                   │
 │  E ─▶ archivos/edicion.rs ── $EDITOR (local) │──────────────────▶│  temporal 600 en runtime/magi/tmp  │
 │       hash antes/después · StatRemoto ───────┼──────────────────▶│  StatRemoto · CambiarPermisos      │
 │  p ─▶ ui/permisos.rs ─── CambiarPermisos ────┼──────────────────▶│  ListarArbol (bloques de 1000,     │
 │  S ─▶ ui/sincronizar.rs                      │                   │    .magiignore del origen remoto)  │
 │       archivos/plan.rs ◀── árbol local (fs)  │  ListarArbol      │  mapa uid/gid → nombre (passwd,    │
 │                        ◀── Arbol{…, fin} ────┼◀──────────────────│    group) por host                 │
 │       archivos/exclusiones.rs (ignore)       │                   │ servidor/transferencias.rs         │
 │       aviso sensibles (F4) · deliberación (F6)│  Transferir{…,   │  permisos por elemento tras rename │
 │  L ─▶ ui/sincronizaciones.rs                 │   permisos,       │  borrar_al_terminar (si todo ok)   │
 │       almacen/sincronizaciones.rs (SQLite)   │   borrar_al_term.,│  anota sincronizacion /            │
 └───────────────────────┬──────────────────────┘   deliberacion,   │  deliberacion_* · ultimo_resultado │
                         │                          sincr._id}────▶│  (hilo escritor)                   │
                         ▼                                          └──────────────┬─────────────────────┘
               ~/.local/share/magi/magi.db                                         │ SFTP
                                                                               host remoto
```

**Reparto.** El cliente decide (qué editar, el modo de `chmod`, el plan de sincronización, la deliberación) y el servidor ejecuta con rutas fijadas (T29, T33). El plan lo calcula el cliente cruzando el árbol local (lo recorre él) con el remoto (`ListarArbol`). La ejecución es una transferencia de la cola de la F4 con dos ampliaciones: permisos por elemento y lista de rutas a borrar al terminar, que el servidor solo ejecuta si la transferencia acabó sin errores. Así una sincronización sobrevive a cerrar la ventana.

---

## 3. Modelo de Datos

### Diagrama E-R (migración 6)

```
 ┌──────────────────────┐          ┌────────────────────────────────┐
 │ HOSTS (F1)           │─────────<│ SINCRONIZACIONES_DIR           │
 └──────────────────────┘ CASCADE  │ id                 PK          │
                                   │ host_id            FK          │
                                   │ nombre             TEXT        │  UNIQUE(host_id, nombre)
                                   │ ruta_local         TEXT        │
                                   │ ruta_remota        TEXT        │
                                   │ direccion          TEXT        │  subida | bajada
                                   │ borrar             0/1         │  borrar en destino lo que no está en origen
                                   │ exclusiones        TEXT        │  patrones extra, uno por línea
                                   │ ultima_ejecucion_en TEXT NULL  │  la escribe el cliente al lanzar
                                   │ ultimo_resultado   TEXT NULL   │  ok | parcial | error | cancelada (servidor)
                                   │ creado_en · actualizado_en     │
                                   └────────────────────────────────┘
```

Sin `CHECK` sobre `direccion` ni `ultimo_resultado` (T8). REGISTRO añade `sincronizacion` (nombre o «ad hoc», dirección, creados, actualizados, borrados, omitidos, bytes, resultado) y `permisos_cambiados` (host, rutas, modo, alcance, afectados). Una edición remota se anota como `transferencia` con «edición» en el detalle. Las deliberaciones de sincronización usan DELIBERACIONES (F6) con `snippet_id` NULL.

### Diagrama de estados: edición remota (cliente)

```
   E sobre un fichero remoto ── > 10 MB ─▶ [confirmación] ── no ─▶ fin
        ▼
   descargando (DescargarTemporal) ── error ─▶ mensaje ─▶ fin
        │ parece binario (bytes nulos en los primeros 8 KiB) ─▶ [confirmación] ── no ─▶ BorrarTemporal ─▶ fin
        ▼
   editando ($EDITOR, TUI suspendida)
        ▼
   comparando (SHA-256) ── igual ─▶ BorrarTemporal · «sin cambios» ─▶ fin
        ▼ distinto
   [¿subir cambios?] ── no ─▶ [descartar / guardar copia local] ─▶ fin
        ▼ sí
   verificando (StatRemoto) ── el remoto cambió (fecha o tamaño) ─▶ conflicto
        │                        [sobrescribir / guardar copia local / descartar]
        │ propietario distinto del usuario de conexión ─▶ [aviso «pasará a ser de <usuario>»]
        ▼
   subiendo (Transferir con permisos del original) ── hecha ─▶ BorrarTemporal ─▶ fin
        └── error / cancelada ─▶ temporal conservado ─▶ mensaje con su ruta y [reintentar / guardar copia local]
  Inválidas: borrar el temporal en «subiendo» o tras un error de subida; subir sin pasar por «verificando».
```

### Diagrama de estados: sincronización

```
   S (o una sincronización guardada) ─▶ [diálogo SINCRONIZAR] ── Esc ─▶ fin
        ▼
   planificando (árbol local + ListarArbol, exclusiones) ── error ─▶ mensaje ─▶ fin
        ▼
   vista_previa ── plan vacío ─▶ «todo al día» ─▶ fin
        │ Esc ─▶ fin
        ▼ ↵
   avisando sensibles (subida) ── cancelar ─▶ fin
        ▼
   ¿deliberación? (subida a host con verificaciones activas · borrar marcado)
        ├─ sí ─▶ deliberación F6 ── cancelada ─▶ fin
        └─ no
        ▼ (Ctrl+K o ↵)
   en_cola ─▶ transfiriendo (cola F4) ── error en algún fichero ─▶ terminada «parcial» (no se borra nada)
        │ cancelar ─▶ cancelada (no se borra nada)
        ▼ todo ok
   borrando (borrar_al_terminar: ficheros y después directorios vacíos) ── error ─▶ «parcial»
        ▼
   hecha ─▶ REGISTRO sincronizacion · ultimo_resultado · deliberacion_* si hubo
  Inválidas: borrar antes de terminar la transferencia; borrar una ruta excluida o fuera del plan fijado.
```

---

## 4. Flujos de Trabajo

### 4.1 Editar un fichero remoto

```
  E sobre un fichero (panel remoto)
      ▼
  tamaño (del listado) > 10 MB ─▶ confirmación
      ▼
  DescargarTemporal{host, ruta} ─▶ RutaTemporal ─▶ guardar (ruta, mtime, tamaño, permisos, uid, SHA-256)
      ▼ ¿bytes nulos en los primeros 8 KiB? ─▶ confirmación «parece binario»
  pausa de teclado con acuse · suspender TUI · editor <temporal> · restaurar · aplicar tamaño (T46)
      ▼
  SHA-256 igual ─▶ BorrarTemporal · «sin cambios»
      ▼ distinto
  [¿subir cambios a host:ruta?] ─ no ─▶ [descartar / guardar copia local]
      ▼ sí
  StatRemoto{ruta} ─▶ ¿mtime o tamaño ≠ guardados? ─▶ [conflicto: sobrescribir / copia local / descartar]
      ▼
  ¿uid del remoto ≠ uid de conexión? ─▶ aviso «tras subirlo pertenecerá a <usuario>» (s/n)
      ▼
  Transferir{subida, [(temporal, ruta, permisos = los del original)], politica = sobrescribir, etiqueta = edición}
      ▼ hecha ─▶ BorrarTemporal · refrescar panel
      ▼ error ─▶ conservar temporal · mensaje con la ruta · [reintentar / guardar copia local]
```

En un fichero local, `E` solo suspende la TUI y abre el editor; al volver se refresca el panel.

### 4.2 Cambiar permisos

```
  p sobre marcados (o la fila actual)
      ▼
  [diálogo PERMISOS] casillas rwx (usuario, grupo, otros) ⇄ octal (3 o 4 dígitos)
      valor inicial: el de la fila actual; con varios marcados de modos distintos, casillas en estado «mixto»
      [ ] recursivo (solo si hay directorios): (•) todo  ( ) solo directorios  ( ) solo ficheros
      ▼ recursivo ─▶ confirmación con recuento estimado («aplicará 0644 a 1 342 ficheros»)
  local ─▶ std::fs::set_permissions (recorriendo si recursivo)
  remoto ─▶ CambiarPermisos{host, rutas, modo, alcance} ─▶ servidor: setstat (recorriendo) ─▶ Hecho{afectados}
           ─▶ REGISTRO permisos_cambiados ─▶ refrescar panel
      error en una ruta ─▶ sigue con el resto; Hecho con afectados y errores en el detalle
```

### 4.3 Sincronizar un directorio

```
  S en Archivos ─▶ origen = directorio del panel activo · destino = el del otro panel
      dirección: activo local → subida; activo remoto → bajada
  [diálogo SINCRONIZAR] [ ] borrar en destino lo que no está en origen · exclusiones extra
                        [ ] guardar como sincronización «nombre»
      ▼ ↵
  planificar:
     árbol local (recorrido en el cliente) ─┐
     ListarArbol{host, ruta remota} ───────┤─▶ exclusiones (por defecto + .magiignore del origen + extras)
                                           ▼
     plan = crear (✕ en destino) · actualizar (≠ tamaño o mtime ±2 s) · borrar (solo si casilla) · omitidos
      ▼
  [VISTA PREVIA] + 12 crear · ~ 3 actualizar · − 2 borrar · 4,2 MB · 37 excluidos · 1 omitido (enlace a directorio)
      plan vacío ─▶ «todo al día»
      ▼ ↵
  subida: aviso de sensibles sobre los ficheros a crear o actualizar
      ▼
  ¿subida a host con verificaciones activas o borrar marcado? ─▶ deliberación MAGI (acción «sync <nombre> → host: 15 ficheros, 2 borrados»)
      ▼
  Transferir{dirección, elementos con permisos, politica = sobrescribir, borrar_al_terminar, deliberacion?, sincronizacion_id?}
      ▼ (cliente escribe ultima_ejecucion_en si es guardada)
  servidor: cola F4 ─▶ ok ─▶ borrar_al_terminar ─▶ REGISTRO sincronizacion · ultimo_resultado · deliberacion_*
```

### 4.4 Diagrama de secuencia: sincronización guardada con borrado y deliberación

```
 ui/archivos     plan/exclusiones    deliberacion (F6)     servidor               host
    │──L ↵ «web-prod»──▶│                    │                  │                    │
    │                   │──ListarArbol(/var/www/app)────────────▶│──readdir recursivo─▶│
    │                   │◀──Arbol{1000…} · Arbol{…, fin}─────────│                    │
    │                   │ árbol local + exclusiones → plan       │                    │
    │◀──VISTA PREVIA────│ (+12 ~3 −2)                            │                    │
    │──↵──────────────▶│──requiere: borrar ─▶│ comprobaciones  │                    │
    │◀──────── MAGI APROBADO ───────────────│                  │                    │
    │──Ctrl+K · Transferir{…, borrar_al_terminar[2], deliberacion, sincronizacion_id}─▶│
    │◀──Transferencias[en_curso]──────────────────────────────────│──open/write ×15───▶│
    │                                                             │──remove ×2────────▶│
    │◀──Transferencias[hecha]─────────────────────────────────────│ REGISTRO sincronizacion
    │                                                             │ deliberacion_aprobada · ultimo_resultado = ok
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi sincronizaciones` | Lista las sincronizaciones guardadas (host, nombre, dirección, rutas, borrar, último resultado) |

La ejecución desde la CLI queda fuera: el plan y la deliberación necesitan la TUI (como D72).

### 5.2 Protocolo (`VERSION_PROTOCOLO = 5`)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `StatRemoto{peticion_id, host_id, ruta}` | `Stat{peticion_id, entrada?}` (sin entrada si no existe) |
| `CambiarPermisos{peticion_id, host_id, rutas, modo, alcance?}` (`todo` · `directorios` · `ficheros`) | `Hecho{peticion_id, detalle?}` (`detalle`: afectados y errores) |
| `ListarArbol{peticion_id, host_id, ruta, exclusiones, usar_magiignore}` | `Arbol{peticion_id, entradas, magiignore?, fin}` en bloques de 1000 entradas (ruta relativa, tipo, tamaño, mtime, permisos, uid) |
| `Transferir{…}` gana `permisos?` por elemento, `borrar_al_terminar`, `deliberacion?`, `sincronizacion_id?`, `etiqueta?` (`edicion` · `sincronizacion`) | `Transferencias{lista}` gana fase `borrando` y recuento de borrados |
| — | `DirListado` y `Arbol`: entradas con `usuario` y `grupo` por nombre cuando el host los resuelve |

### 5.3 Vista Archivos (novedades)

| Tecla | Acción |
|---|---|
| `E` | Editar el fichero de la fila: remoto con temporal y subida; local solo abre el editor |
| `p` | Permisos de los marcados (o la fila actual) |
| `S` | Sincronizar el directorio del panel activo con el del otro panel |
| `L` | Sincronizaciones guardadas del host |

### 5.4 Diálogo SINCRONIZAR y vista previa

| Tecla | Acción |
|---|---|
| `Tab` / `Shift+Tab` | Recorrer campos (borrar, exclusiones extra, guardar como) |
| `Espacio` | Marcar casillas |
| `↵` | Planificar (en el diálogo) · ejecutar (en la vista previa, sin deliberación) |
| `↑` `↓` `PgUp` `PgDn` | Recorrer la lista del plan |
| `f` | En la vista previa, filtrar por tipo de cambio (todos → crear → actualizar → borrar → omitidos) |
| `Esc` | Cancelar |

### 5.5 Lista de sincronizaciones guardadas (`L`)

| Tecla | Acción |
|---|---|
| `↵` | Ejecutar (planificar y vista previa) |
| `n` / `e` / `x` | Nueva / editar / borrar (confirmación) |
| `q` / `Esc` | Volver a Archivos |

### 5.6 Diálogo PERMISOS

| Tecla | Acción |
|---|---|
| `↑` `↓` `←` `→` | Moverse por la rejilla 3×3 de casillas |
| `Espacio` | Marcar o desmarcar la casilla |
| `Tab` | Ir al campo octal / al alcance recursivo |
| `Ctrl+S` | Aplicar |
| `Esc` | Cancelar |

### 5.7 Otras vistas y configuración

| Dónde | Novedad |
|---|---|
| Paleta | `sync · <host> · <nombre>`, `sincronizar directorio`, `editar fichero` |
| Detalle `i` (F4) | Propietario y grupo por nombre (y número entre paréntesis) |
| `config.toml` | `[archivos] editor`, `excluir = [".git/", "target/", "node_modules/", "__pycache__/", ".DS_Store"]`, `editar_max_mb = 10` |

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   F4 ARCHIVOS
   ├── E ─▶ (remoto) descarga ─▶ $EDITOR ─▶ [¿subir?] ─▶ [conflicto] ─▶ cola
   ├── p ─▶ [PERMISOS]
   ├── S ─▶ [SINCRONIZAR] ─▶ VISTA PREVIA ─▶ (aviso sensibles) ─▶ (DELIBERACIÓN MAGI) ─▶ cola ─▶ t TRANSFERENCIAS
   └── L ─▶ SINCRONIZACIONES del host ── ↵ ─▶ VISTA PREVIA … · n e x
   Ctrl+P «sync · host · nombre» ─▶ VISTA PREVIA
```

### 6.1 Diálogo PERMISOS

```
        ┌─ PERMISOS · 3 elementos (2 ficheros, 1 directorio) ─┐
        │              leer   escribir   ejecutar           │
        │  usuario     [x]    [x]        [ ]                │
        │  grupo       [x]    [ ]        [ ]                │
        │  otros       [x]    [ ]        [ ]                │
        │  octal       [ 0644 ]                             │
        │  [x] recursivo   (•) todo ( ) solo dirs ( ) solo ficheros │
        │  ⚠ 0644 en directorios impide entrar en ellos       │
        │  ^s aplicar                         esc cancelar    │
        └──────────────────────────────────────────────────────┘
```

Con modos distintos entre los marcados, la casilla que no coincide muestra `[~]` y el octal queda vacío hasta que se toque. El aviso en ámbar aparece cuando el modo quita `x` a directorios con alcance «todo».

### 6.2 Diálogo SINCRONIZAR

```
        ┌─ SINCRONIZAR · subida ───────────────────────────────┐
        │  origen   ~/proyectos/cooperapp                       │
        │  destino  hetzner-01:/var/www/cooperapp               │
        │  [ ] borrar en destino lo que no está en origen       │
        │  excluir (además de .git/ target/ node_modules/ …)    │
        │  [ *.log                                            ] │
        │  .magiignore del origen: 4 patrones                   │
        │  [x] guardar como  [ web-prod                       ] │
        │  ↵ planificar                         esc cancelar    │
        └───────────────────────────────────────────────────────┘
```

### 6.3 Vista previa del plan

```
┌ MAGI · SINCRONIZAR · web-prod ── ~/proyectos/cooperapp → hetzner-01:/var/www/cooperapp ┐
│  + 12 crear   ~ 3 actualizar   − 2 borrar   4,2 MB   37 excluidos   1 omitido           │
│  ───────────────────────────────────────────────────────────────────────────────────  │
│  + static/img/logo.svg                     14 kB                                       │
│  + static/js/app.js                        88 kB                                       │
│  ~ main.py                                 14 kB  (era 12 kB, hace 2 días)             │
│  ~ config.yaml                              2 kB  (era 1 kB)                           │
│  − static/old.css                           3 kB                                       │
│  − tmp/                                     (directorio vacío tras borrar)             │
│  · static/compartido → /srv/comun          enlace a directorio: omitido               │
│  ───────────────────────────────────────────────────────────────────────────────────  │
│  ⚠ borrará 2 elementos en el destino · requerirá deliberación MAGI                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ↵ continuar   f filtrar   ↑↓ PgUp PgDn   esc cancelar                                   │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.4 Conflicto de edición

```
        ┌─ EL FICHERO CAMBIÓ EN EL HOST ───────────────────────┐
        │  hetzner-01:/etc/nginx/sites-available/cooperapp     │
        │    al abrirlo   2 041 bytes   03 oct 14:31           │
        │    ahora        2 112 bytes   03 oct 14:36           │
        │                                                      │
        │  s sobrescribir   c guardar mi versión como copia local │
        │  d descartar mis cambios                             │
        └──────────────────────────────────────────────────────┘
```

La copia local se guarda en el directorio del panel local como `<nombre>.magi-<AAAAMMDD-HHMMSS>`.

### 6.5 Sincronizaciones guardadas

```
┌ MAGI · SINCRONIZACIONES · hetzner-01 ──────────────── 3 ─┐
│  NOMBRE      DIR.   LOCAL → REMOTO                 ÚLTIMA │
│▸ web-prod    ↑      ~/proyectos/cooperapp → /var…  ✓ ayer │
│  logs        ↓      ~/logs/hetz01 ← /var/log/nginx ✓ hoy  │
│  estaticos   ↑ −    ~/web/static → /srv/static     ✕ 2 sep│
│  ───────────────────────────────────────────────────────  │
│  web-prod · subida · sin borrar · excluye *.log           │
├─────────────────────────────────────────────────────────┤
│ ↵ ejecutar   n nueva   e editar   x borrar   q volver     │
└─────────────────────────────────────────────────────────┘
```

`−` tras la dirección indica «borrar» activo. Todas las pantallas y diálogos nuevos declaran mínimo y modo estrecho (T45) y tienen instantáneas a 40×12, 80×24 y 200×60.

### Notas de UX y diseño

- La edición no bloquea MAGI más allá del editor: mientras editas, el servidor sigue con sesiones, túneles y transferencias.
- La vista previa es la confirmación de la sincronización; no hay preguntas fichero a fichero.
- Los nombres de propietario se muestran en el detalle (`i`) como `www-data (33)`; si el host no los resuelve, solo el número.
- Iconos de cambio: `+` crear (verde), `~` actualizar (ámbar), `−` borrar (rojo), `·` omitido (gris); ASCII igual.

---

## 7. Lógica de Negocio

### 7.1 Plan de sincronización

```python
TOL = 2  # s

def planificar(origen, destino, borrar, excluido):
    # origen, destino: dict ruta_relativa → Entrada (recursivo), ya sin excluidos
    plan = []
    for ruta, e in sorted(origen.items()):
        if e.es_enlace_a_directorio: plan.append(("omitir", ruta, "enlace a directorio")); continue
        d = destino.get(ruta)
        if e.tipo == "dir":
            if d is None: plan.append(("crear_dir", ruta))
            continue
        if d is None:                                   plan.append(("crear", ruta, e.tamano, e.permisos & 0o777))
        elif d.tipo == "dir":                           plan.append(("omitir", ruta, "en destino es un directorio"))
        elif e.tamano != d.tamano or abs(e.mtime - d.mtime) > TOL:
                                                        plan.append(("actualizar", ruta, e.tamano, d.permisos))
    if borrar:
        sobran = [r for r in destino if r not in origen and not excluido(r)]
        # ficheros primero; directorios después, de más profundo a menos
        plan += [("borrar", r) for r in sorted(sobran, key=lambda r: (destino[r].tipo == "dir", -r.count("/")))]
    return plan
```

- Un fichero que en origen es fichero y en destino directorio (o al revés) se omite con motivo; nunca se borra un directorio para crear un fichero.
- Los permisos: al **crear**, los del origen (`& 0o777`); al **actualizar**, se conservan los del destino.
- Las rutas a borrar se fijan en el plan; el servidor no recalcula nada (T33).

### 7.2 Exclusiones

Orden: patrones por defecto de `[archivos] excluir` → `.magiignore` en la raíz del origen (si existe; en bajadas lo lee el servidor y lo devuelve en el primer `Arbol`) → exclusiones extra del diálogo o de la sincronización guardada. Sintaxis de gitignore (`ignore`). Una ruta excluida no se crea, no se actualiza y **nunca se borra** en destino. El `.magiignore` mismo no se sincroniza salvo que se quite de la lista por defecto.

### 7.3 Edición: detección de cambios y conflicto

- Cambio local: SHA-256 del temporal antes y después del editor.
- Cambio remoto: `StatRemoto` justo antes de subir; conflicto si mtime o tamaño difieren de los guardados al descargar.
- Permisos: la subida aplica los permisos del original tras el `rename` del parcial (T56).
- Propietario: el `rename` deja el fichero como del usuario de conexión; si el original era de otro uid, aviso explícito antes de subir. Los enlaces duros al original se rompen (R45).
- El temporal solo se borra tras `hecha`, tras «sin cambios» o tras «descartar» explícito. Ante error, cancelación o servidor caído se conserva y se dice su ruta (T55).
- Dos ediciones del mismo fichero en la misma ventana: la segunda avisa «ya lo estás editando». Entre ventanas protege el conflicto por `StatRemoto`.

### 7.4 Permisos

| Alcance | Aplica a |
|---|---|
| sin recursivo | Las rutas marcadas |
| todo | Las rutas marcadas y todo lo que contienen |
| solo directorios | Directorios marcados y subdirectorios |
| solo ficheros | Ficheros marcados y ficheros contenidos |

Octal de 3 o 4 dígitos (el cuarto: setuid/setgid/sticky). Las casillas solo tocan los 9 bits rwx; los bits especiales se conservan salvo que se escriban en el octal. Aviso en ámbar si el modo quita `x` a directorios con alcance «todo». El recorrido recursivo remoto lo hace el servidor; los enlaces simbólicos no se siguen.

### 7.5 Nombres de propietario

Al abrir el canal SFTP de un host, el servidor lee `/etc/passwd` y `/etc/group` (tope 1 MiB cada uno, plazo 2 s), extrae `nombre:x:id` y guarda el mapa mientras el canal vive. `DirListado` y `Arbol` añaden `usuario`/`grupo` cuando el id está en el mapa. Usuarios de LDAP/SSSD que no están en esos ficheros quedan con el número (R46). Nunca se ejecuta `getent` ni ningún comando.

### 7.6 Casos especiales

- Sincronizar con el otro panel apuntando dentro del origen (o al revés): se rechaza («destino dentro del origen»).
- Árbol de más de 50 000 entradas: se avisa antes de planificar y se sigue si se confirma.
- Una sincronización guardada cuyo host o rutas ya no existen: error al planificar con el motivo.
- Edición de un fichero sin permiso de escritura en el host: la subida falla, se conserva el temporal y se ofrece copia local.
- `E` sobre un directorio o enlace a directorio: no hace nada y lo dice.
- Servidor caído durante una sincronización: la transferencia se pierde con las demás (F3) y no se borra nada; `ultimo_resultado` queda sin escribir (R40, como en F6).

---

## 8. Requisitos No Funcionales

**Seguridad**

- Ninguna ruta pasa por un shell (T30); el editor se lanza como programa + argumentos.
- Temporales en el directorio 700 del servidor, ficheros 600; nunca se borran con cambios sin subir.
- Borrado en destino solo de rutas fijadas en el plan, solo si la transferencia terminó sin errores, nunca de excluidas; siempre con deliberación.
- Aviso de sensibles (F4) en subidas de sincronización y al subir una edición de un fichero que casa con los patrones.
- `chmod` recursivo con confirmación y recuento; aviso si deja directorios sin `x`.

**Protección de datos (RGPD)** — sin cambios; REGISTRO guarda rutas y recuentos, nunca contenidos.

**Backups y recuperación** — `SINCRONIZACIONES_DIR` viaja con `magi.db`. La copia local de una edición en conflicto evita perder trabajo.

**Rendimiento**

- Plan de 10 000 entradas en < 3 s en LAN (árbol remoto por bloques de 1000, cálculo O(n) en el cliente).
- Sincronización: misma cola y velocidad que las transferencias de la F4 (R23).
- Lectura de `passwd`/`group` una vez por canal SFTP.

**Accesibilidad** — signos `+ ~ − ·` además del color; todo por teclado; diálogos adaptables (T45).

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Editor del usuario | `$VISUAL` / `$EDITOR` / `[archivos] editor`; cualquier editor de terminal (nvim, helix, micro) |
| Host remoto | SFTP: `stat`, `setstat`, `readdir` recursivo, `/etc/passwd`, `/etc/group` |
| `.magiignore` | Sintaxis de gitignore en la raíz del directorio origen |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D93 — Sincronizar directorio vive en Archivos.** Un futuro snippet de deploy la invocará. Descartado hacerla en Snippets (Hector).
- **D94 — El cliente planifica, el servidor ejecuta una transferencia ampliada.** Plan con el árbol local del cliente y el remoto de `ListarArbol`; ejecución por la cola de la F4 con `permisos` por elemento y `borrar_al_terminar`, para que sobreviva a la ventana. Descartado un «trabajo de sincronización» nuevo en el servidor.
- **D95 — Borrado solo tras éxito total.** Si un solo fichero falla, no se borra nada y la sincronización queda «parcial»: borrar con un destino a medio actualizar puede dejar el despliegue roto.
- **D96 — Comparación por tamaño y mtime ±2 s, sin hash.** Igual que las marcas de la F4; el hash obligaría a leer todo el árbol remoto.
- **D97 — Edición con temporal + SHA-256 + `StatRemoto`.** Detecta cambios locales sin depender del mtime del editor y cambios remotos sin bloquear el fichero.
- **D98 — Sobrescribir conserva permisos, no propietario.** El `rename` del parcial (F4) cambia de inodo; se reaplican permisos y se avisa si cambia el propietario. Descartado escribir en el sitio (un fallo dejaría el fichero a medias).
- **D99 — Nombres de propietario por `/etc/passwd` leído por SFTP.** Sin ejecutar comandos (T30); suficiente para servidores sin directorio externo.
- **D100 — Sin `chown`.** Sin root casi nunca funciona y SFTP lo hace con números (Hector).
- **D101 — `SINCRONIZACIONES_DIR.ultimo_resultado` lo escribe el servidor** al terminar, por su hilo escritor (amplía la excepción de T18 hecha para DELIBERACIONES).
- **D102 — Protocolo v5** (mensajes nuevos y `Transferir`/`DirListado` ampliados; T24).

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Servidor y protocolo | v5; `StatRemoto`; `CambiarPermisos` recursivo; `ListarArbol` por bloques con `.magiignore`; mapa de nombres; `Transferir` con permisos, `borrar_al_terminar`, deliberación y sincronización; anotaciones; migración 6; tests contra el `sftp-server` real | 1 semana |
| S2 — Edición y permisos | `archivos/edicion.rs` con el ciclo completo y la copia local; diálogo PERMISOS; propietario con nombre en el detalle; instantáneas | 1 semana |
| S3 — Sincronización | `archivos/plan.rs` y `exclusiones.rs`; diálogo SINCRONIZAR; vista previa; aviso de sensibles; deliberación; sincronizaciones guardadas (`L`, paleta, `magi sincronizaciones`); instantáneas; revisión adversarial (T31, T54) | 1,5 semanas |

**Total: 3 sprints, ~3,5 semanas.** Supuesto: el `sftp-server` de los tests permite `setstat` y lectura de `/etc/passwd` del contenedor.

---

## 12. Conexiones con Otras Fases

- **F4:** cola, temporales, marcas, aviso de sensibles y `ListarDir`; `Transferir` y `DirListado` se amplían.
- **F6:** la deliberación se reutiliza con `snippet_id` NULL; un snippet de deploy podrá lanzar una sincronización guardada en una fase futura.
- **F7:** todo diálogo nuevo cumple T45 (mínimo, modo estrecho, instantáneas) y la vuelta del editor pasa por T46.
- **F9 (Sincronización cifrada, antes F8):** `SINCRONIZACIONES_DIR` viaja con el inventario (las rutas locales pueden no existir en otra máquina: se marcan al planificar).
- **F10 (Android, antes F9):** edición remota y sincronización probablemente fuera del móvil.
