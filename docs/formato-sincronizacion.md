# Formato del fichero de sincronización de MAGI

**Formato:** 1
**Fase:** 10 (sincronización cifrada del inventario)
**Lectores previstos:** MAGI de escritorio (Linux y macOS) y app Android (Fase 11)
**Estado:** estable dentro del formato 1. Cualquier cambio de significado sube el número de formato.

Este documento describe el fichero que cada equipo de MAGI escribe en la carpeta
compartida que elige el usuario (Syncthing, Nextcloud, iCloud Drive, Dropbox, un
repositorio git…). MAGI no habla con ningún servicio: solo lee y escribe ficheros
en esa carpeta.

---

## 1. Resumen

- **Un fichero por equipo y por inventario:** se llama `magi-<uuid del equipo>.age`.
  Cada equipo escribe **solo el suyo**; de los demás solo lee.
- El fichero es una **instantánea completa** del inventario de ese equipo, no un
  diario de cambios. Se puede fusionar en cualquier orden y sin depender de
  ficheros anteriores.
- Todo el contenido va **cifrado con age (X25519)** y **firmado con HMAC-SHA256**
  dentro del propio fichero. Sin la clave no se puede leer ni fabricar un fichero
  válido.
- Los secretos (claves privadas SSH, contraseñas, la propia clave de
  sincronización) **nunca viajan**. La clave de sincronización vive solo en el
  llavero del sistema de cada equipo.
- El formato está pensado para ser leído y escrito por la app Android (F11) con
  una biblioteca de age y SHA-256/HMAC/HKDF estándar.

El fichero **no contiene** el historial (REGISTRO), los sondeos, las
deliberaciones, `config.toml`, contraseñas del llavero, claves privadas ni la
clave de sincronización.

---

## 2. Cifrado

- Algoritmo: **age** en su formato binario estándar (C2SP age v1), sin armor. El
  fichero empieza por la cabecera binaria de age (`age-encryption.org/v1`).
- Destinatario: la clave pública **X25519** de la sincronización,
  `age1…` (62 caracteres).
- Todos los equipos del mismo inventario comparten **la misma identidad** de
  sincronización: la clave privada `AGE-SECRET-KEY-1…` que se genera al crear la
  sincronización, se guarda en el llavero y se muestra como texto y QR para unir
  otros equipos.
- Fuera de MAGI, el fichero se abre con la herramienta `age`:

  ```text
  age -d -i clave.txt magi-<uuid>.age
  ```

  donde `clave.txt` contiene la línea `AGE-SECRET-KEY-1…` (permisos 600).

---

## 3. Firma

age cifra pero **no autentica al emisor**: con la clave pública cualquiera podría
fabricar un fichero. Por eso cada fichero lleva una firma HMAC-SHA256 con una
clave derivada del **secreto** de la identidad.

Derivación (HKDF-SHA256, RFC 5869):

```text
K = HKDF-SHA256(
        ikm  = bytes ASCII de la clave canónica de la identidad
               ("AGE-SECRET-KEY-1…" en mayúsculas, tal cual la produce age),
        salt = "magi",
        info = "magi-sincronizacion-firma-v1",
        L    = 32 bytes)
firma = HMAC-SHA256(K, JSON)      // 64 caracteres hexadecimales en minúscula
```

Detalles normativos:

- El **IKM** son los bytes UTF-8/ASCII de la representación canónica de la clave
  privada (`AGE-SECRET-KEY-1` seguido de los datos bech32, todo en mayúsculas).
  Es el texto que el usuario guarda; no hace falta decodificar el bech32 para la
  firma.
- El mensaje firmado es **exactamente** el JSON del documento, desde la llave de
  apertura `{` hasta la de cierre `}`, sin el salto de línea final.
- La firma se escribe en hexadecimal **minúscula** en la línea `firma: …`.
- La verificación es en tiempo constante. Un fichero descifrado con firma no
  válida **se rechaza entero**: no se aplica nada.
- La comprobación de la firma va **después** de descifrar y **antes** de parsear
  el JSON.

---

## 4. Contenido en claro

Una vez descifrado, el contenido es texto UTF-8 con esta estructura exacta:

```text
MAGI-SINCRONIZACION 1
firma: <64 hex en minúscula>
<JSON del documento, en una sola línea, sin salto de línea final>
```

- La primera línea es la cabecera: el literal `MAGI-SINCRONIZACION`, un espacio y
  el número de formato en decimal.
- La segunda línea es la firma: `firma: ` y la firma en hexadecimal.
- Todo lo que sigue hasta el final del contenido en claro es el JSON (antes del
  salto de línea final). Puede ser compacto; MAGI lo escribe compacto
  (`serde_json::to_string`).
- El contenido en claro termina con un salto de línea después del JSON.
- Codificación: UTF-8 sin BOM.

Orden de lectura: descifrar → comprobar la firma → comprobar que `equipo.uuid`
coincide con el nombre del fichero y que el equipo no está en `equipos_quitados`
→ comprobar `formato` → parsear.

---

## 5. Documento JSON

```json
{
  "formato": 1,
  "app": "1.1.0",
  "equipo": { "uuid": "...", "nombre": "omarchy-portatil", "plataforma": "linux" },
  "generado_en": "2026-11-05T09:12:44.120+01:00",
  "reloj_ms": 1793873564120,
  "elementos": [ ... ],
  "lapidas": [ ... ],
  "huellas": [ ... ],
  "equipos_quitados": [ ... ]
}
```

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `formato` | entero | sí | Número de formato. MAGI 1.1.0 conoce el 1. |
| `app` | texto | sí | Versión de MAGI que escribió el fichero. Informativo. |
| `equipo` | objeto | sí | Dueño del fichero: `uuid` (el del nombre del fichero), `nombre` y `plataforma` (`linux`, `macos`, `android`). |
| `generado_en` | texto | sí | Fecha ISO 8601 con milisegundos y zona del momento de la exportación. |
| `reloj_ms` | entero | sí | Reloj híbrido del equipo que exportó (ms desde la época). |
| `elementos` | array | sí | Elementos vivos. Puede ser vacío. |
| `lapidas` | array | sí | Lápidas (elementos borrados). Puede ser vacío. |
| `huellas` | array | sí | Huellas de `known_hosts` de los hosts del inventario. Puede ser vacío. |
| `equipos_quitados` | array | sí | Equipos que este equipo ha quitado. Puede ser vacío. |

Los arrays pueden ir en cualquier orden; los lectores **no** deben depender de él.
MAGI los escribe ordenados por tipo y uuid.

Los campos desconocidos se **ignoran** al parsear: un fichero del formato 1 puede
traer campos nuevos de una versión menor de MAGI y otro equipo los descarta sin
romper la fusión, siempre que el significado de los campos existentes no cambie.

---

## 6. Versiones y vectores

Cada elemento vivo y cada lápida llevan:

- `version`: texto `<ms>.<uuid del equipo>`, donde `<ms>` son los milisegundos
  del reloj híbrido con **13 cifras** (ceros a la izquierda) y `<uuid>` es el
  equipo que hizo esa edición.
- `vector`: objeto `{uuid de equipo: ms}`. Guarda la última edición vista de
  cada equipo para ese elemento.

Reglas:

1. **Reloj híbrido.** Cada edición usa `ms = max(ahora_ms, reloj_ms + 1)`, donde
   `reloj_ms` es el de `EQUIPOS` de este equipo, y actualiza `reloj_ms`. Al
   importar, `reloj_ms` sube a la mayor versión vista. Así un reloj adelantado o
   atrasado no rompe el orden de las ediciones propias y de las que vienen
   después de importar.
2. **Solo viajan los campos que viajan.** Conectarse a un host (cambia
   `ultima_conexion_en`), navegar por SFTP o plegar un grupo **no** cambian la
   versión ni se exportan.
3. **Vector de una edición:** se conserva el vector heredado y se sube (o añade)
   la entrada del equipo que edita con el `ms` de la edición.
4. **Dominancia.** El vector A domina a B si para todo equipo, `A[e] >= B[e]`
   (un equipo ausente cuenta como 0). Si A domina a B, A es posterior: gana A.
5. **Concurrentes.** Si ninguno domina al otro, son ediciones simultáneas: gana la
   versión más reciente (primero se compara `ms` como número; si empatan, el
   uuid del equipo en orden lexicográfico) y el **vector resultante** es el máximo
   componente a componente de todos los candidatos. El perdedor local se guarda
   como conflicto (ver §11).
6. `version` se compara por el número de milisegundos, nunca como texto (13
   cifras hace que el texto también ordene bien hasta 2286, pero el número es la
   fuente de verdad).

---

## 7. Elementos vivos

Cada elemento de `elementos` tiene esta forma:

```json
{
  "tipo": "host",
  "uuid": "0f1e…",
  "version": "1793873500001.6b1e…",
  "vector": { "6b1e…": 1793873500001 },
  "datos": { ... }
}
```

| Campo | Descripción |
|---|---|
| `tipo` | `grupo`, `host`, `identidad`, `tunel`, `snippet` o `sincronizacion_dir`. |
| `uuid` | Identificador global del elemento. En identidades es UUIDv5 de la huella (§7.3). |
| `version` | Versión de esta edición (§6). |
| `vector` | Vector de esta edición (§6). |
| `datos` | Objeto con los campos que viajan del tipo. |

Las referencias entre elementos se hacen **por uuid**. Si un uuid referenciado no
existe o está borrado, la referencia se resuelve como vacía al aplicar. Las
etiquetas de hosts y de destinos de snippets viajan **por nombre**, no por uuid.

### 7.1 Grupo

| Campo | Tipo | Descripción |
|---|---|---|
| `nombre` | texto | Nombre visible. Único entre grupos. |
| `orden` | entero | Orden en la lista. |

No viaja `plegado` (es propio de cada equipo).

### 7.2 Host

| Campo | Tipo | Descripción |
|---|---|---|
| `nombre` | texto | Nombre del host. Único entre hosts. |
| `direccion` | texto | IP o nombre. |
| `puerto` | entero | Puerto SSH. |
| `usuario` | texto o null | Usuario SSH. |
| `identidad_ref` | texto o null | Referencia de identidad: `auto`, `contrasena`, `contrasena:llavero`, `agente:<huella>` o `fichero:<ruta>`. La ruta bajo el home viaja como `~/…`. No es un secreto. |
| `salto` | uuid o null | Host que hace de salto. |
| `grupo` | uuid o null | Grupo del host. |
| `multiplexar` | booleano | Usar el pool de conexiones. |
| `keepalive_seg` | entero o null | `ServerAliveInterval`. |
| `opciones_extra` | texto | Directivas extra de ssh_config, una por línea. |
| `servicios` | texto | Unidades systemd, una por línea. |
| `origen` | texto | `manual` o `ssh_config`. |
| `etiquetas` | array de texto | Nombres de etiquetas. |
| `verificaciones` | objeto o null | Comprobaciones previas de la deliberación (§7.2.1). |
| `snippet_al_conectar` | uuid o null | Snippet que se escribe en cada pestaña. |

No viajan: `ultimo_estado`, `ultima_conexion_en`, `sftp_dir_local`,
`sftp_dir_remoto`, `actualizado_en`.

#### 7.2.1 Verificaciones

| Campo | Tipo |
|---|---|
| `salud` | booleano |
| `backup` | booleano |
| `backup_ruta` | texto o null |
| `backup_patron` | texto o null |
| `tests` | booleano |
| `tests_comando` | texto o null |

### 7.3 Identidad

| Campo | Tipo | Descripción |
|---|---|---|
| `huella` | texto | Huella SHA-256 de la clave, formato OpenSSH (`SHA256:…`). |
| `alias` | texto | Nombre visible. Único entre identidades. |
| `tipo` | texto | Tipo de clave. |
| `comentario` | texto o null | Comentario de la clave. |
| `revocada_en` | texto o null | Fecha de revocación. |

`uuid` = **UUIDv5** (SHA-1, RFC 4122) del namespace
`3f0b9c1e-6d24-4c8a-9e57-2a1d5b7f4c90` con el nombre = los bytes UTF-8 de
`huella`. Por eso la misma clave tiene el mismo `uuid` en todos los equipos y
en Android: no hay unión por alias para identidades.

No viajan: `origen`, `ruta`, `anadida_en`, `ultimo_uso_en`.

### 7.4 Túnel

| Campo | Tipo | Descripción |
|---|---|---|
| `host` | uuid | Host al que pertenece. |
| `nombre` | texto | Único dentro del host. |
| `tipo` | texto | `local`, `remoto` o `dinamico`. |
| `escucha` | texto | `dirección:puerto`. |
| `destino` | texto o null | `host:puerto`; null en dinámico. |
| `automatico` | booleano | Se levanta al conectar. |

No viajan: `creado_en`, `actualizado_en`. El estado en vivo no está en el fichero.

### 7.5 Snippet

| Campo | Tipo | Descripción |
|---|---|---|
| `nombre` | texto | Único entre snippets. |
| `comando` | texto | Comando con variables. |
| `descripcion` | texto | Puede ser vacía. |
| `etiquetas` | texto | Etiquetas del snippet, separadas por comas. |
| `critico` | booleano | Exige deliberación. |
| `timeout_seg` | entero | Plazo de ejecución. |
| `parar_al_fallo` | booleano | Detener la ejecución al primer error. |
| `destinos` | array | Destinos del snippet (§7.5.1). |

No viajan: `usado_veces`, `ultimo_uso_en`.

#### 7.5.1 Destino de snippet

Objeto con **exactamente uno** de estos dos campos:

```json
{ "etiqueta": "produccion" }
{ "host": "<uuid del host>" }
```

### 7.6 Sincronización de directorio

| Campo | Tipo | Descripción |
|---|---|---|
| `host` | uuid | Host al que pertenece. |
| `nombre` | texto | Único dentro del host. |
| `ruta_local` | texto | Ruta local; bajo el home viaja como `~/…`. |
| `ruta_remota` | texto | Ruta remota. |
| `direccion` | texto | `subida` o `bajada`. |
| `borrar` | booleano | Borrar en el destino lo que no está en el origen. |
| `exclusiones` | texto | Patrones gitignore, uno por línea. |

No viajan: `ultima_ejecucion_en`, `ultimo_resultado`.

---

## 8. Lápidas

Una lápida es un elemento borrado. Se conservan **siempre** (son pocas) para que
un equipo que vuelve tras meses no resucite lo borrado.

```json
{
  "tipo": "tunel",
  "uuid": "…",
  "nombre": "db-local",
  "version": "1793873500001.6b1e…",
  "vector": { "6b1e…": 1793873500001 },
  "borrado_en": "2026-11-05T09:12:44.120+01:00"
}
```

Una lápida es una versión más: gana a las ediciones anteriores y pierde ante una
edición posterior (que resucita el elemento). El `vector` de la lápida parte del
vector del elemento borrado y sube la entrada del equipo que borra.

Los borrados en cascada dejan lápida de los hijos: borrar un host deja lápida de
sus túneles y de sus sincronizaciones de directorio.

---

## 9. Huellas

```json
{
  "host": "<uuid del host>",
  "tipo": "ssh-ed25519",
  "clave": "AAAAC3NzaC1lZDI1NTE5AAAA…",
  "aceptada_en": "2026-11-01T10:00:00+01:00"
}
```

- `host` es el **uuid** del host del inventario, no su nombre ni su dirección.
- `tipo` es el algoritmo (`ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`…).
- `clave` es la clave pública en **base64**, sin el tipo ni el comentario.
- `aceptada_en` es la fecha en que el equipo que exporta aceptó la huella (la
  última anotación de su historial para ese host; si no hay, su última conexión).

Al importar: si este equipo no tiene **ninguna** huella para ese host, se añade a
`known_hosts` (con copia previa y una anotación en el historial). Si ya tiene
otra distinta, **nunca se sustituye sola**: se registra un conflicto de tipo
`huella` que el usuario resuelve aceptando la del otro equipo o manteniendo la
suya.

---

## 10. Equipos quitados

```json
{ "uuid": "…", "quitado_en": "2026-11-05T09:12:44.120+01:00" }
```

Un equipo que aparece aquí deja de sincronizar con ese inventario. Quitar un
equipo rota la clave de sincronización: el fichero del equipo quitado se borra de
la carpeta y su clave anterior solo sirve para leer durante un máximo de 30 días.
Un fichero de un equipo conocido que deja de abrirse con la clave de este equipo
detiene la sincronización hasta que el usuario introduzca la clave nueva.

---

## 11. Reglas de fusión

La fusión es **determinista**: el mismo conjunto de ficheros produce el mismo
resultado en cualquier equipo y en cualquier orden. Estos son los pasos que debe
seguir cualquier implementación (MAGI de escritorio y Android):

1. **Universo.** Para cada `uuid`, junta el elemento o lápida local y todos los de
   los ficheros legibles. Un elemento y una lápida del mismo `uuid` son dos
   versiones del mismo elemento; participan las dos.
2. **Ganador.**
   - Si un candidato domina a todos los demás (vector, §6.4), gana ese.
   - Si no, gana el de `version` más reciente entre los que no son dominados por
     otro (concurrentes). El resultado es el mismo venga el fichero antes o
     después.
   - El **vector resultante** del elemento es el máximo componente a componente
     de todos los candidatos (así «ve» todas las ediciones que ha fusionado).
3. **Contenido.** Gana el `datos` del candidato ganador. Una lápida ganadora
   significa que el elemento queda borrado.
4. **Conflictos.** Si la versión **local** pierde frente a una **concurrente** con
   contenido distinto, se guarda en la tabla local `CONFLICTOS` con el contenido
   completo de la versión local perdedora, para poder recuperarla con la acción
   «r». Un conflicto no viaja en el fichero: es local de cada equipo. Una lápida
   local que pierde ante una resurrección no genera contenido que recuperar.
5. **Lápidas.** Las del universo se conservan todas (también las de elementos
   resucitados). Un borrado gana a las ediciones anteriores y pierde ante las
   posteriores.
6. **Referencias.** Las referencias por uuid a elementos inexistentes o borrados
   se resuelven como vacías. Un túnel o una sincronización de directorio cuyo host
   no existe se descarta como conflicto `huerfano`.
7. **Nombres.** Los nombres únicos por tipo (host, grupo, snippet, alias de
   identidad; nombre de túnel o de sincronización dentro de su host) no pueden
   repetirse. Si dos elementos distintos acaban con el mismo nombre, el de
   `uuid` mayor se renombra a `<nombre> (2)` (o el primer número libre) como
   edición nueva y se registra un conflicto `nombre`. Todos los equipos eligen el
   mismo, porque el criterio es determinista.
8. **Trabajo vivo.** Una importación no cierra sesiones, túneles activos,
   transferencias ni ejecuciones en curso: el borrado de un host o de un túnel
   con trabajo vivo se aplaza y queda como conflicto `borrado`.
9. **Borrados masivos.** Antes de aplicar una importación que borra más de
   `umbral_borrado_hosts` hosts o al menos `umbral_borrado_pct` % de ellos, MAGI
   hace una copia de `magi.db` y pide confirmación. En Android debe existir una
   red de seguridad equivalente o no aplicarse.
10. **Aplicación.** Las versiones y los vectores se escriben **tal cual** vienen
    del fichero (nunca se regeneran al importar); el `reloj_ms` local sube a la
    mayor versión vista. Las ediciones locales posteriores ya serán más recientes.

---

## 12. Ficheros en la carpeta

- Nombre estricto: `magi-<uuid>.age`, con el uuid en minúsculas y con guiones.
- Solo se leen nombres que encajan exactamente con ese patrón. Se **ignoran**:
  temporales (`.magi-<uuid>.age.tmp`), copias en conflicto de las herramientas de
  sincronización (`magi-<uuid>.sync-conflict-….age`, «conflicted copy»),
  ficheros de otros programas y cualquier cosa que no sea un uuid válido.
- Cada equipo escribe **solo su fichero**, de forma atómica (temporal en el mismo
  directorio + rename) y con permisos 600. La escritura se hace únicamente cuando
  cambia el contenido, para no provocar sincronizaciones en vacío.
- Un fichero que no se descifra o no tiene firma válida **no se aplica**; el
  equipo queda marcado como ilegible o dañado y se reintenta en la siguiente
  pasada. Un fichero con `formato` mayor que el conocido se ignora con el aviso
  «el equipo X usa una versión más nueva de MAGI; actualiza».
- Un cerrojo (`flock`) garantiza que solo un proceso sincroniza a la vez en el
  mismo equipo.

---

## 13. Validación y errores

Orden obligatorio al leer un fichero ajeno:

1. Descifrar con la identidad actual (o con la anterior durante una rotación). Si
   falla y el equipo es conocido: `ilegible` y la sincronización se detiene
   (`esperando_clave`).
2. Comprobar la cabecera y la firma. Si falla: `danado`, se ignora y se reintenta
   en la siguiente pasada.
3. Comprobar que `equipo.uuid` coincide con el uuid del nombre del fichero y que
   el equipo no está en `equipos_quitados`.
4. Comprobar `formato`. Si es mayor: `formato_nuevo`, se ignora con aviso.
5. Parsear el JSON y validar los campos que se usan (tipos, uuids, versiones). Un
   fichero inválido se descarta entero: nunca se aplica a medias.

Estados de un equipo conocido: `al_dia`, `ilegible`, `danado`,
`formato_nuevo`, `quitado`. El equipo propio tiene estado `este`.

---

## 14. Compatibilidad y evolución

- El número de formato se sube **solo si cambia el significado** de algo ya
  definido (campos, reglas de fusión, derivación de la firma o del cifrado).
- Añadir campos opcionales nuevos o nuevos tipos de elemento no obliga a subir el
  formato, pero un lector antiguo los ignorará y un escritor nuevo no debe
  asumir que el otro los entiende.
- Un lector debe ignorar campos JSON desconocidos (no fallar).
- La app Android (F11) debe leer y escribir este formato 1; el QR de la clave
  contiene el texto `AGE-SECRET-KEY-1…`, que se guarda en el Android Keystore.

---

## 15. Ejemplo mínimo

Contenido en claro de un fichero de un equipo con un host, una huella y una
lápida (el JSON real va en una sola línea):

```text
MAGI-SINCRONIZACION 1
firma: 9c1d0b7a2e5f4c3b1a0918273645f6e7d8c9b0a1f2e3d4c5b6a7988071625344
{"formato":1,"app":"1.1.0","equipo":{"uuid":"6b1e5c2a-1111-4c3a-8a2b-0123456789ab","nombre":"omarchy-portatil","plataforma":"linux"},"generado_en":"2026-11-05T09:12:44.120+01:00","reloj_ms":1793873564120,"elementos":[{"tipo":"host","uuid":"0f1e2d3c-2222-4b3a-9c1d-abcdef012345","version":"1793873500001.6b1e5c2a-1111-4c3a-8a2b-0123456789ab","vector":{"6b1e5c2a-1111-4c3a-8a2b-0123456789ab":1793873500001},"datos":{"nombre":"web","direccion":"192.0.2.5","puerto":22,"usuario":"deploy","identidad_ref":"fichero:~/.ssh/id_ed25519","salto":null,"grupo":null,"multiplexar":false,"keepalive_seg":30,"opciones_extra":"","servicios":"","origen":"manual","etiquetas":["produccion"],"verificaciones":null,"snippet_al_conectar":null}}],"lapidas":[{"tipo":"tunel","uuid":"7a2b3c4d-3333-4e5f-8a9b-cdef01234567","nombre":"db-local","version":"1793873000001.6b1e5c2a-1111-4c3a-8a2b-0123456789ab","vector":{"6b1e5c2a-1111-4c3a-8a2b-0123456789ab":1793873000001},"borrado_en":"2026-11-05T08:43:20.000+01:00"}],"huellas":[{"host":"0f1e2d3c-2222-4b3a-9c1d-abcdef012345","tipo":"ssh-ed25519","clave":"AAAAC3NzaC1lZDI1NTE5AAAAIG…","aceptada_en":"2026-11-01T10:00:00+01:00"}],"equipos_quitados":[]}
```

---

## 16. Cómo abrir un fichero fuera de MAGI

```text
# 1. Guardar la clave (una sola línea) en un fichero con permisos 600
printf '%s\n' 'AGE-SECRET-KEY-1…' > clave.txt
chmod 600 clave.txt

# 2. Descifrar el fichero (el JSON es la última línea)
age -d -i clave.txt magi-<uuid>.age | tail -n 1 | python3 -m json.tool
```

La firma HMAC no la comprueba `age`; para validarla hace falta reconstruir la
clave de firma con HKDF-SHA256 y comprobar el HMAC-SHA256 del JSON, como se
describe en §3.
