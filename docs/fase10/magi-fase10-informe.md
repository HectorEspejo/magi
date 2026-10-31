# MAGI - Fase 10: Sincronización Cifrada

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 30 de octubre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> **Base sin confirmar.** Hector pidió especificar esta fase antes de recibir el informe de implementación de la Fase 9, que sigue especificada y sin implementar. La Fase 8 está al 63/64, sin revisión adversarial (R50) ni validación manual. Si la F9 cambia algo de lo que esta fase da por hecho (paquete, protocolo v6, `magi diagnostico`, CHANGELOG), este informe se revisa al procesar aquel (R68).

---

## 1. Visión General

MAGI guarda el inventario en `magi.db`, en cada equipo por separado. La Fase 10 hace que **el mismo inventario esté en todos tus equipos**: hosts, grupos, identidades, túneles, snippets y sincronizaciones de directorio, junto con las huellas de los hosts. Los cambios se mueven en ficheros cifrados dentro de una carpeta que tú eliges (Syncthing, Nextcloud, iCloud Drive, Dropbox, un repositorio git…). MAGI no habla con ningún servicio, no tiene cuenta y no conoce a nadie más que a tus equipos.

Cada equipo escribe su propio fichero, `magi-<uuid>.age`, cifrado con `age` y firmado, y lee los de los demás. La fusión es elemento a elemento. Cada elemento lleva una versión y un vector por equipo, así MAGI distingue un cambio que viene después de otro (se aplica sin más) de dos cambios hechos a la vez en dos equipos (gana el más reciente y el perdedor se guarda para recuperarlo). Las claves privadas, las contraseñas y la propia clave de sincronización nunca viajan.

**Objetivos principales**

1. Migración 7: identificador global, versión y vector en todo lo que viaja; tablas EQUIPOS, LAPIDAS y CONFLICTOS.
2. Un fichero por equipo, cifrado con `age` (X25519) y firmado con HMAC, con un formato documentado que leerá la app Android (F11).
3. Fusión determinista con lápidas, conflictos recuperables, unión por nombre al añadir un equipo y huellas que nunca se sustituyen sin preguntar.
4. Sincronización automática mientras MAGI está abierto (al abrir, al cambiar otro equipo, tras cada cambio propio), además de `magi sincronizar`.
5. Clave generada por MAGI, mostrada como texto y QR y guardada en el llavero; quitar un equipo cambia la clave.
6. Red de seguridad: copia de `magi.db` antes de cada importación que cambia algo y confirmación de borrados masivos.

**Contexto.** Decisión de Hector: `magi.db` **no** se cifra en el disco (D125). No guarda claves ni contraseñas, tiene permisos 600 y el disco suele ir cifrado (LUKS, FileVault). Esto cambia lo que decía el roadmap («cifrado en reposo y desbloqueo al arrancar»): se cifra lo que sale del equipo. Fuera de alcance: transporte por SFTP a un host, servidor propio, sincronizar el historial, compartir inventario con otras personas, sincronización con MAGI cerrado y la app Android. Versión prevista: **1.1.0** (MINOR por la migración, T68).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Cifrado | `age` (crate de rage), identidad X25519 | Un destinatario; el fichero se abre con `age -d -i` fuera de MAGI (D126) |
| Firma | `hmac` + `hkdf` + `sha2` | HMAC-SHA256 con clave derivada de la identidad; `age` no autentica al emisor (D127) |
| Identificadores | `uuid` (v4; v5 para identidades) | Global por elemento |
| Vigilancia | `notify` | Cambios en la carpeta; sondeo de 30 s como respaldo |
| QR | `qrcode` | Pintado con medios bloques en la TUI |
| Clave | Llavero del sistema por `src/llavero.rs` | `secret-tool` (Linux), Keychain (macOS); nunca en `magi.db` ni en la carpeta |
| Copias | `VACUUM INTO` de SQLite | `~/.local/share/magi/copias/`, 600 |
| Cerrojo | `flock` (nix) | Un solo proceso sincroniza a la vez |

### Estructura de carpetas (novedades)

```
src/
├── sincronizacion/
│   ├── mod.rs                pasada completa: leer, fusionar, confirmar, aplicar, exportar
│   ├── formato.rs            cabecera, firma, JSON, formato 1 (tipos y campos)
│   ├── cifrado.rs            age X25519, HKDF + HMAC, generar/validar identidad
│   ├── fusion.rs             vectores, lápidas, conflictos, nombres, huérfanos (puro)
│   ├── elementos.rs          tabla ↔ elemento (campos que viajan / propios del equipo)
│   ├── huellas.rs            exportar/importar huellas de known_hosts
│   ├── reloj.rs              reloj híbrido y versiones
│   ├── carpeta.rs            lectura de magi-*.age, escritura atómica, vigilancia
│   └── equipos.rs            equipos, quitar, rotar clave
├── almacen/
│   ├── migraciones.rs        migración 7
│   ├── versiones.rs          sube version/vector en cada escritura que viaja
│   ├── equipos.rs · lapidas.rs · conflictos.rs
│   └── copias.rs             VACUUM INTO y rotación de copias
├── llavero.rs                + identidad de sincronización (actual y anterior)
├── app/sincronizacion.rs     vista, asistentes, indicador, diálogos
├── ui/sincronizacion.rs      SINCRONIZACIÓN, CONFLICTOS, CLAVE, CREAR/UNIR, IMPORTACIÓN CON BORRADOS
├── diagnostico.rs            + comprobación de sincronización
└── main.rs                   magi sincronizar [estado]
tests/
├── sincronizacion/{fusion,formato,huellas,carpeta,clave}.rs
└── redimensionado/vistas_m.rs
docs/formato-sincronizacion.md
docs/fase10/
├── magi-fase10-informe.md · magi-fase10-prompt.md · magi-fase10-checklist.md
└── magi-fase10-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 equipo A (omarchy-portatil)                 carpeta elegida                     equipo B (mac-mini)
 ┌──────────────────────────────┐   escribe  ┌──────────────────────────┐  lee   ┌──────────────────────────────┐
 │ magi (cliente)               │───────────▶│ magi-<uuid A>.age        │───────▶│ magi (cliente)               │
 │  almacen/versiones ─┐        │            │ magi-<uuid B>.age        │        │  sincronizacion/             │
 │  sincronizacion/    │        │◀───────────│ (Syncthing, Nextcloud,   │◀───────│   fusion · formato · cifrado │
 │   carpeta · fusion ─┘        │    lee     │  iCloud, Dropbox, git…)  │ escribe│  llavero (clave)             │
 │  llavero (clave)             │            └──────────────────────────┘        │  magi.db (en claro, 600)     │
 │  magi.db (en claro, 600)     │                       ▲                         └──────────────────────────────┘
 │  ~/.ssh/known_hosts ◀ huellas│              MAGI no habla con
 │  ~/.ssh/magi_config (regen.) │              ningún servicio
 └──────────────┬───────────────┘
                │ socket (sin cambios de protocolo)
 ┌──────────────▼───────────────┐
 │ magi --servidor              │  lee TUNELES y HOSTS al activar; la importación nunca cierra
 │                              │  sesiones, túneles, transferencias ni ejecuciones (borrado aplazado)
 └──────────────────────────────┘
```

**Reparto.** La sincronización vive entera en el cliente, que es quien escribe el inventario. El servidor de sesiones no cambia y el protocolo sigue en v6. Cada proceso cliente puede sincronizar, pero un cerrojo garantiza que solo lo haga uno a la vez, y la fusión es idempotente. Sin MAGI abierto no se sincroniza nada (salvo `magi sincronizar`, que se puede lanzar a mano o desde un temporizador).

---

## 3. Modelo de Datos

### Diagrama E-R (migración 7)

```
 tablas que viajan (+ uuid, version, vector)              tablas nuevas (propias de cada equipo)
 ┌──────────────────────┐                                 ┌───────────────────────────────────┐
 │ GRUPOS     + uuid …  │                                 │ EQUIPOS                           │
 │ HOSTS      + uuid …  │──┐ etiquetas (por nombre) y     │ id · uuid (UNIQUE) · nombre       │
 │ IDENTIDADES+ uuid v5 │  │ verificaciones viajan dentro │ plataforma · es_este (0/1)        │
 │ TUNELES    + uuid …  │  │ del host; destinos dentro    │ app_version · ultimo_fichero_en   │
 │ SNIPPETS   + uuid …  │  │ del snippet                  │ ultima_importacion_en             │
 │ SINCRONIZACIONES_DIR │  │                              │ hash_ultimo_fichero · estado      │
 │            + uuid …  │  │                              │ quitado_en · reloj_ms (este)      │
 └──────────────────────┘  │                              └───────────────────────────────────┘
                           │ borrado                      ┌───────────────────────────────────┐
                           └────────────────────────────▶ │ LAPIDAS                           │
                                                          │ uuid (PK) · tipo · nombre         │
                           pierde una versión local ────▶ │ version · vector · borrado_en     │
                                                          └───────────────────────────────────┘
                                                          ┌───────────────────────────────────┐
                                                          │ CONFLICTOS                        │
                                                          │ id · tipo · elemento_tipo · uuid  │
                                                          │ nombre · version_perdedora_json   │
                                                          │ version_ganadora · equipo_ganador │
                                                          │ detectado_en · resuelto_en        │
                                                          │ resolucion                        │
                                                          └───────────────────────────────────┘
```

### Columnas nuevas en las tablas que viajan

| Campo | Tipo | Descripción |
|---|---|---|
| uuid | TEXT UNIQUE NOT NULL | Identificador global. En IDENTIDADES, UUIDv5 de la huella (el mismo en todos los equipos) |
| version | TEXT NOT NULL | `<ms con 13 cifras>.<uuid del equipo>`: última edición; ordena las versiones concurrentes |
| vector | TEXT NOT NULL | JSON `{uuid de equipo: ms}`: la última edición vista de cada equipo; decide si una versión deriva de otra |

### Tablas nuevas

**EQUIPOS** (este equipo y los que aparecen en la carpeta)

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| uuid | TEXT UNIQUE NOT NULL | Da nombre a su fichero |
| nombre | TEXT NOT NULL | Por defecto el hostname; editable en este equipo |
| plataforma | TEXT NOT NULL | `linux` · `macos` · `android` (F11) |
| es_este | INTEGER NOT NULL | 1 solo en una fila |
| app_version | TEXT NULL | Versión de MAGI del último fichero leído |
| ultimo_fichero_en | TEXT NULL | `generado_en` del último fichero leído |
| ultima_importacion_en | TEXT NULL | Cuándo se importó por última vez |
| hash_ultimo_fichero | TEXT NULL | SHA-256 del fichero cifrado; si no cambia, no se vuelve a leer |
| estado | TEXT NOT NULL | `este` · `al_dia` · `ilegible` · `danado` · `formato_nuevo` · `quitado` |
| quitado_en | TEXT NULL | |
| reloj_ms | INTEGER NULL | Solo en la fila de este equipo: reloj híbrido |

**LAPIDAS**: uuid (PK), tipo (`grupo` · `host` · `identidad` · `tunel` · `snippet` · `sincronizacion_dir`), nombre (para mostrar), version, vector y borrado_en. Se conservan siempre: son pocas y ocupan poco (D131).

**CONFLICTOS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| tipo | TEXT NOT NULL | `elemento` · `nombre` · `huella` · `borrado` · `huerfano` |
| elemento_tipo | TEXT NOT NULL | Tipo del elemento afectado |
| uuid | TEXT NOT NULL | Elemento afectado |
| nombre | TEXT NOT NULL | Para mostrar |
| version_perdedora_json | TEXT NULL | Contenido completo de la versión local que perdió (o de la huella propia) |
| version_ganadora | TEXT NULL | Versión que ganó |
| equipo_ganador | TEXT NULL | uuid del equipo autor |
| detectado_en | TEXT NOT NULL | |
| resuelto_en | TEXT NULL | Los resueltos se purgan a los 90 días |
| resolucion | TEXT NULL | `recuperado` · `descartado` · `aceptado` |

Sin `CHECK` en las enumeraciones (T8).

### Qué viaja y qué no

| Elemento | Viaja | Se queda en cada equipo |
|---|---|---|
| grupo | nombre, orden | plegado |
| host | todo lo demás, incluidos servicios, opciones extra, salto (uuid), grupo (uuid), snippet al conectar (uuid), etiquetas por nombre y verificaciones | ultimo_estado, ultima_conexion_en, sftp_dir_local, sftp_dir_remoto |
| identidad | huella, alias, tipo, comentario, revocada_en | origen, ruta, anadida_en, ultimo_uso_en |
| túnel | host (uuid), nombre, tipo, escucha, destino, automático | creado_en, actualizado_en |
| snippet | nombre, comando, descripción, etiquetas, crítico, timeout, parar al fallo, destinos (etiqueta o host uuid) | usado_veces, ultimo_uso_en |
| sincronización de directorio | host (uuid), nombre, ruta_local (`~/…`), ruta_remota, dirección, borrar, exclusiones | ultima_ejecucion_en, ultimo_resultado |
| — | huellas de `known_hosts` de los hosts del inventario | REGISTRO, SONDEOS, DELIBERACIONES, `config.toml`, contraseñas, claves privadas, la clave de sincronización |

### Diagrama de estados: sincronización en este equipo

```
   desactivada (sin carpeta)
        │ n configurar carpeta
        ├─ carpeta sin ficheros de MAGI ─▶ creando ─ clave mostrada · «la he guardado» ─▶ activa
        └─ carpeta con ficheros ────────▶ uniendo ─ clave válida ─▶ activa
                                                    └ clave inválida ─▶ uniendo (reintentar) · Esc ─▶ desactivada
   activa ⇄ sincronizando (pasada)
        sincronizando ─ borrados por encima del umbral ─▶ en_pausa (diálogo) ─ a / m ─▶ activa
        sincronizando ─ fichero de equipo conocido no se abre con la clave ─▶ esperando_clave
        esperando_clave ─ clave nueva válida ─▶ activa (vuelve a cifrar su fichero)
        sincronizando ─ este equipo aparece en equipos_quitados ─▶ quitado ─ unirse de nuevo ─▶ uniendo (uuid nuevo)
        sincronizando ─ error de carpeta (no existe, sin permiso) ─▶ error ─ reintento en la siguiente pasada
   activa ─ d desactivar aquí ─▶ desactivada (borra su fichero y la clave; magi.db intacta)
  Inválidas: exportar en en_pausa o esperando_clave; importar sin firma válida; escribir el fichero de otro
  equipo (salvo borrar el de un equipo quitado); guardar la clave fuera del llavero.
```

### Diagrama de estados: conflicto

```
   detectado ─▶ pendiente
        ├─ r recuperar (elemento, borrado aplazado) ─▶ recuperado (edición nueva con la versión perdedora)
        ├─ a aceptar (huella del otro equipo; borrado aplazado sin trabajo vivo) ─▶ aceptado
        └─ d descartar ─▶ descartado
   recuperado · aceptado · descartado ─ 90 días ─▶ purgado
  Inválidas: aceptar un borrado aplazado mientras el host tiene trabajo vivo; recuperar un huérfano sin su host;
  sustituir una huella sin pasar por «aceptar».
```

### Diagrama de estados: otro equipo (visto desde este)

```
   (aparece su fichero) ─▶ al_dia ⇄ (fichero nuevo, se importa)
        al_dia ─ no se descifra con la clave ─▶ ilegible ─ (clave nueva aquí) ─▶ al_dia
        al_dia ─ se descifra pero la firma falla o el fichero está cortado ─▶ danado ─ fichero bueno ─▶ al_dia
        al_dia ─ formato mayor que el conocido ─▶ formato_nuevo ─ (actualizar MAGI) ─▶ al_dia
        cualquiera ─ x quitar (desde este o desde otro equipo) ─▶ quitado (su fichero se ignora)
  Inválidas: importar de un equipo quitado, ilegible, dañado o de formato nuevo.
```

---

## 4. Flujos de Trabajo

### 4.1 Configurar la sincronización (crear o unir)

```
  paleta «sincronización» ─▶ vista SINCRONIZACIÓN (desactivada) ─ n ─▶ elegir carpeta
      ├─ no existe o no se puede escribir ─▶ error ─▶ elegir otra
      ├─ sin ficheros magi-*.age ─▶ CREAR
      │     generar identidad age ─▶ llavero ─▶ diálogo CLAVE (texto + QR) ─ s «la he guardado» ─▶
      │     guardar carpeta en config.toml ─▶ exportar magi-<uuid>.age ─▶ activa
      └─ con ficheros ─▶ UNIR
            pegar clave ─▶ descifrar un fichero y comprobar su firma
              ├─ no abre o firma mala ─▶ «la clave no abre los ficheros de esta carpeta» ─▶ reintentar
              └─ válida ─▶ llavero ─▶ unión por nombre (§7.4) ─▶ pasada completa ─▶ activa
```

### 4.2 Una pasada de sincronización

```
  disparo: abrir MAGI · cambio en la carpeta (2 s) · cambio local (retraso_seg) · s · magi sincronizar
      ▼ cerrojo (otro proceso sincronizando ─▶ esperar ≤ 10 s o dejarlo para la siguiente)
  leer magi-*.age ajenos con hash distinto del último importado
      ├─ no se descifra (equipo conocido) ─▶ esperando_clave ─▶ fin (no se exporta)
      ├─ firma mala o cortado ─▶ equipo danado · se ignora · reintento en la siguiente pasada
      ├─ formato nuevo ─▶ equipo formato_nuevo · se ignora
      └─ válido ─▶ estado del equipo (incluido equipos_quitados)
  ¿este equipo en equipos_quitados? ─▶ quitado ─▶ fin
  fusionar estado local + ficheros (§7.2) ─▶ plan (escribir, borrar, conflictos, huellas)
      ▼ plan vacío ─▶ exportar si cambió algo propio ─▶ fin
  copia de magi.db (VACUUM INTO) · rotar copias
  ¿borrados por encima del umbral? ─▶ diálogo IMPORTACIÓN CON BORRADOS (CLI: código 2, nada aplicado)
      ├─ a ─▶ seguir
      ├─ m ─▶ editar de nuevo esos hosts (versión nueva) ─▶ seguir sin esos borrados
      └─ Esc ─▶ en_pausa ─▶ fin
  ¿borrados de hosts o túneles con trabajo vivo? ─▶ conflicto «borrado aplazado» (no se aplican)
  aplicar el plan en una transacción (túneles activos: el cambio vale en la próxima activación)
      ▼ huellas: añadir si no hay ninguna · conflicto si hay otra
      ▼ regenerar ~/.ssh/magi_config · REGISTRO inventario_importado · conflictos
  exportar el fichero propio si cambió su contenido (escritura atómica)
```

### 4.3 Quitar un equipo y cambiar la clave

```
  vista SINCRONIZACIÓN ─ x sobre «mac-mini» ─▶ confirmación «quitará mac-mini y cambiará la clave»
      ▼ sí
  añadir mac-mini a equipos_quitados ─▶ borrar magi-<uuid mac-mini>.age de la carpeta
  generar identidad nueva ─▶ llavero (la anterior queda como «anterior», solo para leer, 30 días)
  diálogo CLAVE (texto + QR) ─ s ─▶ exportar el fichero propio con la clave nueva
      ▼
  en los demás equipos: el fichero de este ya no se abre con su clave ─▶ esperando_clave
      ─▶ el usuario pega la clave nueva ─▶ vuelven a cifrar su fichero ─▶ activa
  cuando todos los equipos activos leen con la clave nueva (o pasan 30 días) ─▶ borrar la anterior
```

### 4.4 Huellas

```
  exportar: para cada host del inventario, sus entradas de ~/.ssh/known_hosts (también las hasheadas)
  importar, por host y huella:
      ├─ este equipo no tiene ninguna huella para ese host ─▶ añadir (ficheros.rs, copia) ─▶ REGISTRO huella_sincronizada
      ├─ tiene exactamente esa ─▶ nada
      └─ tiene otra ─▶ conflicto «huella» (aceptar la del otro equipo / mantener la mía); known_hosts no se toca
```

### 4.5 Diagrama de secuencia: un cambio que viaja de A a B

```
 A (cliente)             almacén A          carpeta (Syncthing)          B (cliente)            almacén B
   │─editar host web────────▶│ version = 1793…001.A                          │                      │
   │                         │ vector {A: 1793…001}                          │                      │
   │ (5 s) exportar ─────────────────────────▶│ magi-A.age (age + firma)     │                      │
   │                                           │──Syncthing──▶ réplica en B   │                      │
   │                                           │                notify ─────▶│ cerrojo · leer A     │
   │                                           │                             │ descifrar · firma ok │
   │                                           │                             │ fusionar: vector A domina
   │                                           │                             │──escribir web───────▶│
   │                                           │                             │ magi_config · REGISTRO
   │                                           │◀──── exportar magi-B.age ───│ (vector actualizado) │
```

### 4.6 Diagrama de secuencia: cambios concurrentes

```
 A                                   carpeta                                   B
   │ edita web: 192.0.2.5 (t=100, {A:100})                                     │ edita web: 192.0.2.7 (t=120, {B:120})
   │──magi-A.age──────────────────────▶│◀─────────────────────────magi-B.age────│
   │ lee B: {B:120} vs {A:100} concurrentes                                      │ lee A: concurrentes
   │ gana t=120 (B) · la suya (192.0.2.5) pierde → CONFLICTO elemento           │ gana t=120 (la suya) · sin conflicto
   │ vector {A:100, B:120}                                                       │ vector {A:100, B:120}
   │──magi-A.age──────────────────────▶│──────────────────────────▶ lee: iguales │
   │ r recuperar → edición nueva t=130 {A:130, B:120} → gana en todos            │
```

---

## 5. Acciones, Formato y CLI

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi sincronizar` | Una pasada sin TUI. Código 0 si va bien, 1 con error, 2 si hace falta decidir (borrados por encima del umbral, clave nueva) sin aplicar nada |
| `magi sincronizar estado` | Carpeta, equipos con estado y último fichero, conflictos pendientes; no descifra nada |
| `magi diagnostico` | (F9) añade «sincronización»: carpeta accesible y escribible, estado de los equipos; no lee la clave |

`magi sincronizar` (inventario) no es `magi sincronizaciones` (las sincronizaciones de directorio guardadas de la F8); la ayuda de los dos lo dice (R75).

### 5.2 Formato del fichero (`formato` 1)

```
MAGI-SINCRONIZACION 1
firma: 9c1d…(64 hex: HMAC-SHA256 de las líneas siguientes)
{"formato":1,"app":"1.1.0",
 "equipo":{"uuid":"6b1e…","nombre":"omarchy-portatil","plataforma":"linux"},
 "generado_en":"2026-11-05T09:12:44.120Z","reloj_ms":1793873564120,
 "elementos":[{"tipo":"host","uuid":"…","version":"1793873500001.6b1e…",
               "vector":{"6b1e…":1793873500001},"datos":{"nombre":"web","direccion":"192.0.2.5",…}}],
 "lapidas":[{"tipo":"tunel","uuid":"…","nombre":"db-local","version":"…","vector":{…}}],
 "huellas":[{"host":"<uuid>","tipo":"ssh-ed25519","clave":"AAAAC3Nza…","aceptada_en":"…"}],
 "equipos_quitados":[{"uuid":"…","quitado_en":"…"}]}
```

Todo el texto se cifra con `age` a la clave X25519 de la sincronización. La firma usa `K = HKDF-SHA256(ikm = secreto de la identidad, salt = "magi", info = "magi-sincronizacion-firma-v1")`. El fichero es una **instantánea completa** del inventario de ese equipo, no un diario. `docs/formato-sincronizacion.md` documenta cada tipo y cada campo para la F11.

### 5.3 Vista SINCRONIZACIÓN

| Tecla | Acción |
|---|---|
| `s` | Sincronizar ahora |
| `c` | Vista CONFLICTOS |
| `k` | Mostrar clave (confirmación) |
| `x` | Quitar el equipo seleccionado (confirmación; cambia la clave) |
| `r` | Cambiar la clave sin quitar ningún equipo |
| `d` | Desactivar en este equipo (confirmación) |
| `n` | (sin configurar) Configurar carpeta: crea o une según lo que encuentre |
| `↑` `↓` | Recorrer equipos |
| `q` / `Esc` | Volver |

### 5.4 Vista CONFLICTOS

| Tecla | Acción |
|---|---|
| `↑` `↓` · `↵` | Recorrer · ver el detalle de campos |
| `r` | Recuperar mi versión (elemento, borrado aplazado) |
| `a` | Aceptar (huella del otro equipo; borrado aplazado sin trabajo vivo) |
| `d` | Descartar |
| `q` / `Esc` | Volver |

### 5.5 Configuración y paleta

| Dónde | Novedad |
|---|---|
| `config.toml` | `[sincronizacion] carpeta`, `retraso_seg = 5`, `copias = 10`, `umbral_borrado_hosts = 5`, `umbral_borrado_pct = 20` |
| Paleta | `sincronización`, `sincronizar ahora`, `conflictos de sincronización` |
| Barra inferior | Indicador `⇅` (§6.6) |

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   Ctrl+P «sincronización» ─▶ SINCRONIZACIÓN
   │   ├── (desactivada) n ─▶ elegir carpeta ─▶ [CREAR + CLAVE] o [UNIR]
   │   ├── s ─▶ pasada ─▶ (si hace falta) [IMPORTACIÓN CON BORRADOS]
   │   ├── c ─▶ CONFLICTOS ─ ↵ detalle · r · a · d
   │   ├── k ─▶ [CLAVE] (confirmación)
   │   ├── x ─▶ [confirmación] ─▶ [CLAVE nueva]
   │   ├── r ─▶ [confirmación] ─▶ [CLAVE nueva]
   │   └── d ─▶ [confirmación] ─▶ desactivada
   Ctrl+P «conflictos de sincronización» ─▶ CONFLICTOS
   cualquier vista: barra «⇅ …»; esperando_clave ─▶ [UNIR con la clave nueva] al abrir la vista
```

### 6.1 Vista SINCRONIZACIÓN

```
┌ MAGI · SINCRONIZACIÓN ──────────────────────────────────────── ⇅ al día · hace 2 min ┐
│ carpeta  ~/Sync/magi                          clave  7f3a·91c2  (en el llavero)      │
│ este equipo  omarchy-portatil · linux                                                │
│ ──────────────────────────────────────────────────────────────────────────────────── │
│   EQUIPO                PLATAFORMA   MAGI     ÚLTIMO FICHERO   ESTADO                │
│ ▸ omarchy-portatil      linux        1.1.0    hace 2 min       este equipo           │
│   mac-mini              macos        1.1.0    hace 1 h         ● al día              │
│   framework-ostrava     linux        1.1.0    hace 9 días      ✕ no abre con la clave│
│ ──────────────────────────────────────────────────────────────────────────────────── │
│   2 conflictos pendientes · c para verlos                                            │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ s sincronizar  c conflictos  k clave  x quitar equipo  r cambiar clave  d desactivar │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

La «clave» que se muestra es una huella corta (SHA-256 de la clave pública), para comparar a simple vista que dos equipos comparten clave; no permite cifrar ni descifrar.

### 6.2 Diálogo CLAVE DE SINCRONIZACIÓN

```
        ┌─ CLAVE DE SINCRONIZACIÓN ────────────────────────────┐
        │  Guárdala en tu gestor de contraseñas. La necesitas  │
        │  para añadir equipos. Si la pierdes, tus datos       │
        │  siguen en cada equipo: crearías una clave nueva.    │
        │                                                      │
        │  AGE-SECRET-KEY-1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX…  │
        │                                                      │
        │  ▄▄▄▄▄▄▄ ▄▄ ▄ ▄▄▄▄▄▄▄                                │
        │  █ ▄▄▄ █ ▀█▄█ █ ▄▄▄ █   QR para el móvil (F11)       │
        │  █ ███ █ ▄▀▀▄ █ ███ █                                │
        │  …                                                   │
        │  s la he guardado                       esc cancelar │
        └──────────────────────────────────────────────────────┘
```

Por debajo del tamaño del QR se muestra solo el texto con «amplía la terminal para ver el QR». La clave no se escribe en el log, ni en REGISTRO, ni en ningún fichero.

### 6.3 Diálogo UNIR ESTE EQUIPO

```
        ┌─ UNIR ESTE EQUIPO ───────────────────────────────────┐
        │  ~/Sync/magi tiene ficheros de 2 equipos:            │
        │  omarchy-portatil · mac-mini                         │
        │                                                      │
        │  clave  [ •••••••••••••••••••••••••••••••••••••• ]   │
        │         Tab mostrar                                  │
        │                                                      │
        │  Los hosts con el mismo nombre se tomarán como el    │
        │  mismo (gana la versión más reciente).               │
        │  ↵ unir                                 esc cancelar │
        └──────────────────────────────────────────────────────┘
```

### 6.4 Vista CONFLICTOS

```
┌ MAGI · CONFLICTOS DE SINCRONIZACIÓN ──────────────────────────────────── 3 ┐
│   TIPO        ELEMENTO       GANÓ                         CUÁNDO           │
│ ▸ elemento    web            mac-mini · 5 nov 09:12       hace 1 h         │
│   huella      db-01          mac-mini aceptó otra         hace 1 h         │
│   borrado     hetzner-01     aplazado: 2 pestañas         hace 3 h         │
│ ─────────────────────────────────────────────────────────────────────────  │
│   web · campo «direccion»: tu versión 192.0.2.5 · ganó 192.0.2.7           │
├────────────────────────────────────────────────────────────────────────────┤
│ ↵ detalle   r recuperar mi versión   a aceptar   d descartar   q volver    │
└────────────────────────────────────────────────────────────────────────────┘
```

### 6.5 Diálogo IMPORTACIÓN CON BORRADOS

```
        ┌─ IMPORTACIÓN CON BORRADOS ───────────────────────────┐
        │  mac-mini ha borrado 7 de tus 23 hosts:              │
        │  web-1 · web-2 · web-3 · db-1 · db-2 · cache · lb    │
        │                                                      │
        │  Copia de magi.db guardada antes de aplicar nada:    │
        │  copias/magi-20261105-101200.db                      │
        │                                                      │
        │  a aplicar los borrados                              │
        │  m mantener los míos (vuelven a todos los equipos)   │
        │  esc decidir luego (sincronización en pausa)         │
        └──────────────────────────────────────────────────────┘
```

### 6.6 Indicador de la barra inferior

```
 … Flota · F2 ─────────────────────────────── ⇅ al día      (verde)
                                              ⇅ …           (sincronizando)
                                              ⇅ 2 conflictos (ámbar)
                                              ⇅ pausa       (ámbar)
                                              ⇅ clave nueva (rojo)
                                              ⇅ error       (rojo)
 ASCII: «sync ok», «sync …», «sync 2!», «sync pausa», «sync clave», «sync error»
```

### Notas de UX y diseño

- La sincronización no interrumpe: solo abre un diálogo para un borrado masivo o una clave nueva. Lo demás va a la barra y a la vista.
- Sin la sincronización activada no aparece nada nuevo en las vistas de siempre.
- Las direcciones de ejemplo usan el rango de documentación (`192.0.2.x`), como pide la auditoría de la F9.
- Todas las vistas y diálogos nuevos declaran mínimo y modo estrecho y tienen instantáneas (T45).

---

## 7. Lógica de Negocio

### 7.1 Reloj híbrido y versiones

```python
def nueva_version(este, ahora_ms):
    ms = max(ahora_ms, este.reloj_ms + 1)
    este.reloj_ms = ms
    return f"{ms:013d}.{este.uuid}", ms

def editar(elemento, este, ahora_ms):          # solo si cambia un campo que viaja
    elemento.version, ms = nueva_version(este, ahora_ms)
    elemento.vector = {**elemento.vector, este.uuid: ms}

def al_importar(este, versiones_vistas):
    este.reloj_ms = max(este.reloj_ms, *(int(v.split(".")[0]) for v in versiones_vistas))
```

Un reloj adelantado o atrasado en un equipo no rompe el orden de sus propias ediciones ni el de las que vienen después de importar las de otros. Solo afecta a cuál gana entre dos ediciones concurrentes (R69).

### 7.2 Fusión de un elemento

```python
def domina(a, b):            # a ha visto todo lo que ha visto b
    return all(a.get(e, 0) >= ms for e, ms in b.items())

def fusionar(local, entrante):
    """local, entrante: Version(vivo|lapida, version, vector, datos) o None."""
    if entrante is None: return local, None
    if local is None:    return entrante, None
    if domina(local.vector, entrante.vector): return local, None
    if domina(entrante.vector, local.vector): return entrante, None
    # concurrentes: gana la versión más reciente; el vector resultante ve las dos
    ganadora = max(local, entrante, key=lambda v: v.version)
    vector = {e: max(local.vector.get(e, 0), entrante.vector.get(e, 0))
              for e in local.vector.keys() | entrante.vector.keys()}
    resultado = ganadora.con_vector(vector)
    conflicto = None
    if ganadora is entrante and local.contenido() != entrante.contenido():
        conflicto = Conflicto("elemento", perdedora=local)   # solo se guarda donde se pierde
    return resultado, conflicto
```

- Se aplica a cada `uuid` presente en el estado local o en cualquier fichero legible, combinando los ficheros de uno en uno; el resultado no depende del orden (test de propiedades).
- Una lápida es una versión más: gana a una edición anterior y pierde ante una posterior (resucitar).
- Un elemento que gana con un vector nuevo (aunque su contenido no cambie) hace que este equipo exporte, para que los demás vean que ya está fusionado.

### 7.3 Después de fusionar

- **Referencias**: grupo, salto y snippet al conectar que apuntan a un `uuid` inexistente o con lápida quedan vacíos.
- **Huérfanos**: un túnel o una sincronización de directorio cuyo host no existe se descarta y queda como conflicto `huerfano`.
- **Nombres**: si dos elementos distintos acaban con el mismo nombre único (host, grupo, snippet, alias de identidad; túnel o sincronización dentro del mismo host), el de `uuid` mayor pasa a `<nombre> (2)` (o el primer número libre) como edición nueva, y se registra un conflicto `nombre`. Todos los equipos eligen el mismo, y una edición idéntica hecha en dos equipos no genera conflicto.
- **Trabajo vivo**: el borrado de un host con sesiones, túneles activos, transferencias o ejecuciones en curso, o el de un túnel activo, no se aplica y queda como conflicto `borrado`. Un cambio en un túnel activo se aplica a la tabla y vale en su próxima activación.
- **Etiquetas**: la tabla ETIQUETAS se reconstruye con los nombres que usan los hosts y los destinos de snippets.

### 7.4 Unir un equipo con inventario propio

```python
def unir(local, entrantes):
    for e in entrantes:
        propio = local.por_nombre(e.tipo, e.clave_natural())   # nombre (y host en túneles y sincr.)
        if propio and propio.uuid != e.uuid:
            local.renombrar_uuid(propio, e.uuid)               # adopta el uuid del fichero
    # después, fusión normal: vectores sin relación ⇒ concurrentes ⇒ gana la más reciente,
    # la versión local que pierda queda como conflicto recuperable
```

Las identidades no necesitan este paso: su `uuid` sale de la huella y coincide en todos los equipos.

### 7.5 Umbral de borrados masivos

```python
def requiere_confirmacion(borrados_hosts, total_hosts, umbral_n=5, umbral_pct=20):
    if borrados_hosts == 0: return False
    return borrados_hosts > umbral_n or borrados_hosts * 100 >= umbral_pct * total_hosts
```

Solo cuentan los borrados de hosts (los demás tipos se aplican sin preguntar, porque la copia previa ya los cubre). «Mantener los míos» hace una edición nueva de cada uno de esos hosts: su versión pasa a ser más reciente que la lápida y vuelven a todos los equipos.

### 7.6 Clave, firma y lectura de ficheros

- La identidad (`AGE-SECRET-KEY-1…`) vive solo en el llavero: entrada `magi` / `sincronizacion` y, durante una rotación, `sincronizacion-anterior`.
- La clave pública (`age1…`) tampoco se escribe en la carpeta. La firma HMAC impide fabricar un fichero válido aunque alguien la conociera.
- Orden de lectura: descifrar → comprobar la firma → comprobar que `equipo.uuid` coincide con el nombre del fichero y no está quitado → comprobar `formato` → parsear.
- «No abre con la clave» en el fichero de un equipo conocido detiene la sincronización en este equipo (`esperando_clave`). En un fichero desconocido, solo genera un aviso.
- Repetir un fichero viejo del mismo equipo no hace nada: sus versiones pierden frente a las actuales.

### 7.7 Casos especiales

- **Dos ventanas en el mismo equipo** comparten `magi.db`: el cerrojo serializa las pasadas y la fusión es idempotente. Los cambios locales de una ventana siguen sin verse en la otra hasta recargar (como desde la F3, R74).
- **Copias en conflicto de la herramienta de sincronización** (`magi-<uuid>.sync-conflict-….age`, «conflicted copy»): se ignoran. `magi diagnostico` avisa de que existen.
- **Fichero a medio copiar**: falla el descifrado o la firma, el equipo queda `danado` y se reintenta en la siguiente pasada.
- **Perder la clave en todos los equipos**: los datos siguen en cada `magi.db`. Se desactiva y se crea una sincronización nueva con otra carpeta o tras vaciarla.
- **Rutas que no existen en otro equipo** (clave de identidad, `ruta_local` de una sincronización de directorio): se importan igual. La ficha y la F8 ya avisan al usarlas.
- **MAGI anterior a la 1.1.0** en otro equipo: no escribe fichero y no participa. La vista no lo ve.

---

## 8. Requisitos No Funcionales

**Seguridad**

- Lo que sale del equipo va cifrado (age X25519) y firmado (HMAC-SHA256 con clave derivada). Sin la clave no se puede leer ni fabricar un fichero válido.
- La clave vive solo en el llavero; se muestra únicamente al crearla, al rotarla o con `k` y confirmación. Nunca en logs, REGISTRO, `magi.db` ni la carpeta.
- Claves privadas, contraseñas y la clave de sincronización nunca viajan (T1, T19).
- Una huella recibida nunca sustituye a otra sin que el usuario lo acepte. Añadir una huella para un host sin ninguna queda en REGISTRO (T4).
- Quien tiene la clave tiene la confianza total: puede cambiar direcciones y añadir huellas para hosts sin huella (R76). Quitar un equipo cambia la clave, y un fichero que deja de abrirse detiene la sincronización.
- La carpeta compartida expone solo metadatos: número de equipos (ficheros), tamaño y frecuencia de cambios (R72).

**Protección de datos (RGPD).** Solo datos del propio usuario, que se mueven entre sus equipos por un transporte que él elige. MAGI no envía nada a terceros (T10).

**Backups y recuperación.** Copia de `magi.db` antes de cada importación que cambia algo (las 10 últimas). Conflictos recuperables 90 días. Lápidas permanentes. Si la carpeta se pierde, cada equipo conserva su `magi.db` completa.

**Rendimiento.** Inventario objetivo: hasta 1 000 hosts y 5 equipos. Una pasada sin cambios (hashes iguales) tarda menos de 100 ms; una con cambios, menos de 1 s. El fichero debería quedar por debajo de 1 MB. La vigilancia no consume CPU en reposo.

**Accesibilidad.** Indicador con signo y texto además del color; ASCII; QR con alternativa de texto; todo por teclado; T45.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Carpeta sincronizada | Cualquier herramienta que replique ficheros (Syncthing, Nextcloud, iCloud Drive, Dropbox, git). MAGI solo lee y escribe `magi-*.age` |
| `age` | Formato estándar; `age -d -i clave.txt magi-<uuid>.age` lo abre fuera de MAGI |
| Llavero | `secret-tool` (Linux) y Keychain (macOS) por `src/llavero.rs`; Android Keystore en la F11 |
| `~/.ssh/known_hosts` | Lectura de huellas (también hasheadas) y escritura por `src/ficheros.rs` con copia |
| `~/.ssh/magi_config` | Se regenera tras cada importación que cambia el inventario |
| App Android (F11) | Leerá y escribirá el mismo formato (`docs/formato-sincronizacion.md`) y escaneará el QR |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D125 — `magi.db` sin cifrar en el disco** (Hector). No guarda secretos, tiene permisos 600 y el disco suele ir cifrado. Se evita pedir una frase al arrancar y pasar la clave al servidor de sesiones. Descartados SQLCipher con clave en el llavero y con frase al arrancar (lo que decía el roadmap).
- **D126 — `age` con identidad X25519 generada por MAGI, mostrada como texto y QR y guardada en el llavero** (Hector). Formato estándar que se abre fuera de MAGI. La clave tiene toda la entropía y el QR prepara la F11. Descartadas una frase elegida por el usuario y un cifrado propio.
- **D127 — Firma HMAC dentro del fichero.** `age` cifra pero no autentica al emisor: con la clave pública cualquiera podría fabricar un fichero. La firma con clave derivada del secreto lo impide sin una segunda clave.
- **D128 — Transporte: una carpeta que elige el usuario** (Hector). Vale con cualquier herramienta y MAGI no habla con nadie. Descartados SFTP a un host (podría añadirse en otra fase) y un servidor propio.
- **D129 — Un fichero por equipo, como instantánea completa.** Cada fichero tiene un solo escritor, así las herramientas de sincronización nunca generan copias en conflicto, y una instantánea se fusiona sin depender del orden. Descartados un fichero común y un diario de cambios.
- **D130 — Vector por elemento con reloj híbrido; «gana el más reciente» solo entre concurrentes** (Hector: opción a de conflictos). El vector separa una edición que deriva de otra de dos ediciones a la vez, para no dar falsos conflictos. Descartado comparar solo fechas.
- **D131 — Lápidas permanentes.** Un borrado viaja como una versión más; conservarlas evita que un equipo que vuelve tras meses resucite lo borrado.
- **D132 — Granularidad: grupo, host (con etiquetas y verificaciones), identidad (por huella), túnel, snippet (con destinos) y sincronización de directorio.** Las tablas hijas sin identidad propia viajan dentro de su padre.
- **D133 — Lo que no viaja:** historial (REGISTRO, SONDEOS, DELIBERACIONES), `config.toml`, campos propios del equipo y cualquier secreto (Hector).
- **D134 — Huellas: se añaden solo si no hay ninguna; una distinta es un conflicto** (Hector).
- **D135 — Unión por nombre al añadir un equipo** (Hector). El caso típico es haber importado el mismo `~/.ssh/config` en dos equipos.
- **D136 — Colisión de nombres posterior: renombrar `(2)` el de `uuid` mayor y avisar.** Es determinista en todos los equipos. Descartado fusionar en silencio dos hosts distintos con el mismo nombre.
- **D137 — Sincronización en el cliente, automática mientras MAGI está abierto, con cerrojo y vigilancia de la carpeta** (Hector). Sin demonio nuevo; el servidor de sesiones no cambia.
- **D138 — Copias antes de importar y confirmación de borrados masivos** (Hector).
- **D139 — Quitar un equipo cambia la clave; un fichero conocido que deja de abrirse detiene la sincronización** (Hector). La clave anterior solo sirve para leer, 30 días como máximo.
- **D140 — Vista desde la paleta, indicador en la barra y CLI** (Hector). Sin tecla de función nueva.
- **D141 — Una importación nunca cierra trabajo vivo:** los borrados afectados se aplazan como conflicto.
- **D142 — Formato documentado y versionado (`formato`) para la F11.** Un formato mayor que el conocido se ignora con aviso, igual que una `magi.db` más nueva (T68).
- **D143 — «Mostrar clave» desde el llavero, con confirmación.** Hace falta para añadir equipos más adelante; matiza el «se muestra una vez» de la respuesta 2 de Hector.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Modelo, formato y fusión | Migración 7, reloj híbrido, versiones en el almacén, lápidas, `elementos.rs` (qué viaja), formato con cabecera, firma y `age`, fusión pura con conflictos, nombres y huérfanos, tests de propiedades | 1 semana |
| S2 — Carpeta y ciclo completo | Lectura y escritura de la carpeta, cerrojo, vigilancia, pasada completa, crear y unir (llavero, unión por nombre), huellas, copias y umbral, trabajo vivo, `magi_config`, `magi sincronizar [estado]`, diagnóstico, REGISTRO | 1 semana |
| S3 — Interfaz y cierre | Vistas SINCRONIZACIÓN y CONFLICTOS, diálogos (clave con QR, unir, borrados), indicador, paleta, quitar equipo y rotación, `docs/formato-sincronizacion.md`, README, CHANGELOG, instantáneas, revisión adversarial (T31, T54) | 1 semana |

**Total: 3 sprints, ~3 semanas.** Supuestos: la F9 está implementada como se especificó; `secret-tool` y Keychain guardan una cadena de unos 75 caracteres; los tests usan carpetas temporales como transporte (sin Syncthing real) y el guion manual prueba Syncthing entre Omarchy y el mac mini.

---

## 12. Conexiones con Otras Fases

- **F1 y F2:** HOSTS, GRUPOS, IDENTIDADES y `known_hosts` son lo principal que viaja; `ficheros.rs` y `llavero.rs` se reutilizan; `magi_config` se regenera.
- **F3:** el servidor de sesiones no cambia; su trabajo vivo aplaza borrados.
- **F5, F6 y F8:** TUNELES, SNIPPETS (con destinos y verificaciones) y SINCRONIZACIONES_DIR viajan; DELIBERACIONES no.
- **F7:** vistas y diálogos nuevos cumplen T45.
- **F9 (sin confirmar):** se publica como 1.1.0 (MINOR); `magi diagnostico` gana una comprobación; las licencias de las dependencias nuevas pasan `cargo deny`; las direcciones de ejemplo siguen la auditoría (R68).
- **F11 (Android):** leerá y escribirá el mismo formato, guardará la clave en Android Keystore y la añadirá escaneando el QR.
