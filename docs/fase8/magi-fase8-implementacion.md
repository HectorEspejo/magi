# MAGI - Fase 8: Archivos, Segunda Vuelta

## Informe de Implementación

**Última actualización:** 3 de octubre de 2026 (sesión 1: fase implementada, 63/64; la revisión adversarial de cierre no se hizo por decisión de Hector y queda pendiente; falta la validación manual)

---

## Resumen de lo implementado

La fase se desarrolla en la rama `fase8-inapp_editor`, con un commit por sprint:

| Sprint | Commit | Contenido |
|---|---|---|
| S1 | `b0519ad` | Protocolo v5, migración 6 y su CRUD, exclusiones, servidor (posix-rename, temporales de edición, nombres de propietario, `StatRemoto`, `CambiarPermisos`, `ListarArbol`, transferencias ampliadas, deliberación compartida) y esqueleto del cliente. Incluye la corrección de tres defectos heredados de la F4 |
| S2 | `08c5df1` | Edición remota con `E` y diálogo PERMISOS con `p`; detalle `i` con propietario por nombre |
| S3 | `ef9c1c1` | Sincronizar directorio (`S`), plan, vista previa, deliberación generalizada, fase `borrando`, sincronizaciones guardadas (`L`), paleta y `magi sincronizaciones` |
| Cierre | (este) | Test de rechazo de un cliente v4, arreglo de una prueba intermitente, README, checklist e informe |

**Método.** Se fijó primero un contrato compilable: tipos y mensajes del protocolo v5, migración, módulos vacíos del cliente y sus enganches en `app.rs`, diálogos, mínimos de disposición y ficheros de tests por pista. Sobre él trabajaron seis pistas en paralelo, cada una en un worktree:

| Pista | Contenido |
|---|---|
| A | Servidor SFTP: canal raw, temporales, nombres, `StatRemoto`, `CambiarPermisos`, `ListarArbol` |
| B | Transferencias y deliberación |
| C | Edición |
| D | Permisos |
| E | Sincronizar |
| F | Guardadas |

Se integraron con cherry-pick y se aplastaron en el commit de cada sprint. Hubo un solo conflicto: A y B hicieron el mismo cambio en `copiar_subida` para usar el renombrador. Se resolvió quedándose con la versión de B.

### Defectos heredados de la Fase 4 (corregidos con test en rojo primero)

1. **Sobrescribir un fichero remoto existente fallaba siempre en OpenSSH.**
   - `copiar_subida` hacía `rename(parcial, destino)` por SFTP v3, y `sftp-server` lo implementa con `link()`+`unlink()`: da EEXIST si el destino existe.
   - El test `subir_con_sobrescribir_sobre_un_fichero_existente` falló con «Failure: Failure» antes del arreglo.
   - Ningún test de la F4 subía sobre un fichero existente. La política «sobrescribir» del diálogo de conflicto de la F4 nunca funcionó contra OpenSSH.
2. **Cada `DescargarTemporal` vaciaba todo `…/magi/tmp`.**
   - Un temporal de edición habría muerto con cualquier «ver» de otra ventana.
   - Además, dos ventanas con el mismo `peticion_id` compartían nombre de temporal.
3. **`borrar_temporal` aceptaba rutas con `..`.** `starts_with` no canonicaliza, así que `…/tmp/../x` borraba fuera del directorio. El test borraba la víctima antes del arreglo.

### S1 — Servidor, protocolo y almacén

**Protocolo v5** (`VERSION_PROTOCOLO = 5`):
- Mensajes nuevos: `StatRemoto`/`Stat`, `CambiarPermisos{modo, mascara, alcance}` y `ListarArbol{exclusiones, usar_magiignore, exclusiones_extra}`/`Arbol{entradas, magiignore, excluidos, fin}`.
- `Hecho` gana `detalle`.
- `Transferir` gana `peticion_id`, `borrar_al_terminar`, `deliberacion`, `sincronizacion` y `etiqueta`; `ElementoTransferencia` gana `permisos`.
- `InfoTransferencia` gana `peticion_id`, `etiqueta`, `borrados` y `borrados_total`, y aparece el estado `Borrando`.
- `SftpAbierto` gana `usuario_conexion`/`uid_conexion` y `DescargarTemporal` gana `edicion`.
- `Entrada.propietario` pasa a ser `Propietario{uid, gid, usuario, grupo}`.
- Tests de ida y vuelta de todo lo nuevo, de lectura de un `Transferir` v4 con valores por defecto y del rechazo de un cliente v4.

**Almacén:**
- Migración 6: `SINCRONIZACIONES_DIR`, con FK en cascada, `UNIQUE(host_id, nombre)`, índice por host y sin `CHECK`.
- `almacen/sincronizaciones.rs` con la CRUD, `marcar_ejecucion` (cliente) y `fijar_resultado` (servidor, por el hilo escritor con `OrdenBd::ResultadoSincronizacion`).
- Validación en `modelo::validar_sincronizacion`, que reutiliza `validar_nombre`.
- Una fila con dirección desconocida se salta con aviso.
- REGISTRO gana `sincronizacion` y `permisos_cambiados`, los dos en el filtro «archivos».

**Exclusiones** (`archivos/exclusiones.rs`, crate `ignore`):
- Orden: por defecto → `.magiignore` → extras; gana el último patrón que casa.
- Las comparten el cliente y el servidor.

**Servidor SFTP** (`servidor/sftp.rs`):
- **Renombrador.**
  - Segundo canal SFTP raw por host para `posix-rename@openssh.com`, con plazos de 10 s.
  - Si el host no anuncia la extensión, tres pasos con deshacer: destino → `.magi-viejo`, parcial → destino, borrar el viejo. Si el segundo paso falla, el viejo vuelve a su sitio.
  - Nunca sustituye un directorio.
- **Temporales.**
  - Los de edición van a `…/magi/ediciones` (700), que no vacía nadie.
  - Nombres únicos con un contador global, `create_new` y modo 600.
  - `borrar_temporal` solo admite ficheros cuyo padre sea exactamente `tmp` o `ediciones`.
- **Nombres de propietario:** `/etc/passwd` y `/etc/group` leídos por SFTP al abrir el canal (tope de 1 MiB, plazo total de 2 s), aplicados a `DirListado`, `Stat` y `Arbol`.
- **`CambiarPermisos`.**
  - Recursivo sin seguir enlaces; los enlaces nunca se tocan.
  - Máscara: `(viejo & !mascara) | (modo & mascara)`.
  - Pre o postorden según el modo deje entrar o no.
  - Anota `permisos_cambiados`, pasa la barrera y responde `Hecho{detalle}`.
- **`ListarArbol`.**
  - Abre el canal con `asegurar` y poda lo excluido.
  - Marca los enlaces a directorio sin seguirlos.
  - Bloques de 1000: el `.magiignore` va en el primero; `fin` y `excluidos`, en el último.

**Transferencias** (`servidor/transferencias.rs`):
- Permisos por elemento tras el `rename`. El parcial nace con los bits del original para no exponer nunca un 0600.
- Lista plana en las sincronizaciones.
- Validación al encolar: rutas absolutas, sin `.` ni `..`, colgando estrictamente de la raíz de destino.
- `borrar_al_terminar` solo tras `Hecha`, en dos pasadas: primero lo que no es directorio; después los directorios con `remove_dir`, de más profundo a menos. Un directorio no vacío se conserva sin error. Nada que cuelgue de un enlace bajo la raíz se borra.
- Fase `Borrando` con difusión antes del efecto.
- Resultado `ok`, `parcial`, `error` o `cancelada`.
- Al cerrar anota `sincronizacion`, escribe `ultimo_resultado` y, si hubo deliberación, `deliberacion_aprobada` o `forzada` con su `ejecucion_resultado`.
- Una edición se anota como `transferencia` con «edición».
- Un cierre único cubre la caída de la conexión, el apagado y la cancelación en cola.
- La validación de la deliberación se comparte con `ejecuciones.rs`, y «ya usada» vale en los dos sentidos (ejecución ⇄ transferencia).

### S2 — Edición remota y permisos

**Edición (`E`).** La lógica pura está en `archivos/edicion.rs` y el flujo en `app/edicion.rs`.
- **Local:** abre el editor y refresca el panel al volver.
- **Remoto:**
  1. Confirmación por tamaño (`editar_max_mb`).
  2. `DescargarTemporal{edicion:true}`.
  3. Confirmación si parece binario.
  4. Editor sin shell, con la TUI suspendida, pausa de teclado con acuse y tamaño real al volver (T46).
  5. Comparación por SHA-256.
  6. «¿Subir?»; con «no», descartar (confirmado) o copia local.
  7. `StatRemoto`. Si cambió, el diálogo de conflicto: sobrescribir, copia local o descartar.
  8. Aviso si cambia el propietario y aviso de sensibles.
  9. `Transferir` con política sobrescribir y los permisos del original.
  10. El temporal se borra tras `Hecha`. Ante error, cancelación o servidor caído se conserva y el diálogo dice su ruta, con reintentar o copia local.
- **Enlaces:** un enlace a fichero se edita en su destino; uno a directorio no hace nada y lo dice.
- «Ya lo estás editando» si se repite sobre el mismo fichero.

**PERMISOS (`p`).** La lógica pura está en `archivos/permisos.rs`, el flujo en `app/permisos.rs` y la interfaz en `ui/permisos.rs`.
- Rejilla 3×3 ⇄ octal de 3 o 4 dígitos, con estado mixto `[~]`.
- La máscara conserva los bits mixtos y los especiales.
- Recursivo con alcance; antes, recuento y confirmación: en local con un hilo, en remoto con `ListarArbol`.
- Aviso ámbar si los directorios se quedan sin `x`.
- Local con `std::fs::set_permissions` en un hilo, sin tocar enlaces; remoto con `CambiarPermisos`.
- El detalle `i` enseña «Propietario www-data (33)» y «Grupo …».

### S3 — Sincronización y guardadas

**Sincronizar (`S`).** Plan puro en `archivos/plan.rs`; flujo en `app/sincronizar.rs`; interfaz en `ui/sincronizar.rs`.
- **Diálogo SINCRONIZAR:**
  - borrar, extras y «guardar como»;
  - recuento del `.magiignore` del origen (en bajada se lee con un temporal);
  - rechaza un destino anidado en el origen o al revés.
- **Plan:**
  - El árbol local se recorre en un hilo y el remoto con `ListarArbol`, con las exclusiones en el orden de §7.2 en los dos lados.
  - Crear, actualizar (tamaño o mtime ±2 s), borrar y omitir con motivo.
  - Permisos del origen al crear; los del destino se conservan al actualizar.
  - Confirmación por encima de 50 000 entradas.
- **Vista previa:** `+ ~ − ·` con color y ASCII, filtro `f`, lista desplazable y avisos de borrado y de deliberación. Con el plan vacío dice «todo al día».
- **Antes de encolar:** aviso de sensibles en subidas.
- **Deliberación:** se generalizó (`PlanDeliberado{Snippet, Sincronizacion}`, `MotivoDeliberacion::Borrar`) con la acción «sync <nombre> → host: N ficheros, M borrados». `Ctrl+K` ejecuta.
- **Ejecución:** `Transferir` plana con permisos y `borrar_al_terminar`. `ultima_ejecucion_en` se escribe al recibir el `Hecho` de la cola.
- Transferencias enseña `borrando N/M` y «N borrados».

**Guardadas (`L`).** En `app/guardadas.rs` y `ui/sincronizaciones.rs`.
- Lista del host con dirección (`−` si borra), rutas, último resultado y fecha, y detalle inferior.
- `↵` planifica; `n`/`e` abren el formulario «SINCRONIZACIÓN GUARDADA»; `x` borra con confirmación.
- Paleta: `sync · <host> · <nombre>`, `sincronizar directorio` y `editar fichero`.
- `magi sincronizaciones` usa `archivos/listado.rs`.

### Revisión adversarial

**No se hizo.** El workflow de revisión se lanzó tras S3:
- siete focos: servidor SFTP, transferencias, edición, sincronizar, permisos y guardadas, transversal y cobertura del checklist;
- con verificación 2/3 de cada hallazgo.

Hector pidió pararlo («sin revisión adversarial») antes de que ningún buscador devolviera resultados. Según `CLAUDE.md`, queda en «Pendientes y bloqueos» con lo que no se ha revisado. El ítem del checklist sigue sin marcar.

---

## Desviaciones respecto a la especificación (qué y por qué)

**Decisiones de Hector (3 oct 2026):**
1. **Sobrescribir con `posix-rename@openssh.com` por un segundo canal SFTP raw**, con respaldo en tres pasos. `russh-sftp` 3.0.0 no expone la extensión en `SftpSession`. Ahora cada canal SFTP ocupa dos canales de sesión de la conexión, y eso cuenta para `MaxSessions` (10 por defecto en OpenSSH).
2. **`.magiignore` entra en `[archivos] excluir` por defecto.** El informe se contradecía: §7.2 dice que no se sincroniza salvo que se quite de la lista, y el checklist da la lista sin él.

**Defectos de la F4 corregidos en esta fase** (no estaban en el checklist; eran necesarios): los tres de arriba.

**Protocolo v5, además de lo del informe:**
- `Transferir.peticion_id`, que se contesta con `Hecho`/`Error`. Con él, la ventana reconoce su fila por `(solicitante, peticion_id)` y cierra la deliberación si el servidor la rechaza.
- `InfoTransferencia.{peticion_id, etiqueta, borrados, borrados_total}`.
- `CambiarPermisos.mascara`. Sin ella no se pueden conservar `[~]` ni los bits especiales ruta a ruta.
- `ListarArbol.exclusiones_extra`. Sin él, en una bajada las extras no podrían ir después del `.magiignore` remoto (§7.2).
- `Arbol.excluidos`, para la vista previa de una bajada.
- `SincronizacionLanzada{id, nombre, recuentos, raiz_origen, raiz_destino}` en vez de un `sincronizacion_id` suelto: el servidor necesita el nombre para anotar y la raíz para validar los borrados.
- `DescargarTemporal.edicion`.
- `SftpAbierto.{usuario_conexion, uid_conexion}`, para el aviso de propietario. El uid sale del mapa de `passwd` o, si no está, del dueño de `dir_inicio`.

**Rango de `peticion_id` nuevo: 2^42 para las operaciones de Archivos que viven en la `App`** (edición, permisos y sincronización).
- El contador del panel (desde 0) se reinicia al cambiar de host con `h`. Una edición o un plan en vuelo cruzaría sus respuestas con el panel nuevo.
- `CLAUDE.md` enumera los rangos y no recoge este. El analista debería añadirlo.

**Edición:**
- `E` sobre un enlace a fichero edita su destino resuelto. Si no, la subida sustituiría el enlace por un fichero.
- La copia local se guarda con `create_new` y modo 600; si el nombre se repite en el mismo segundo, se añade `-2`. Tras comprobar su hash se borra el temporal: el trabajo ya está a salvo. T55 enumera «hecha, sin cambios, descartar»; esta es una cuarta salida segura.
- Permisos y dueño de la subida: los del `StatRemoto` de justo antes de subir, no los del listado al pulsar `E`. Un `chmod` durante la edición no cambia el mtime y se perdería.
- `esc` en el conflicto vuelve a «¿subir?». Solo `s` sobrescribe.
- Los títulos van sin signos de apertura («SUBIR CAMBIOS»): `texto_ascii` los quita.
- Si el temporal llega con el usuario fuera de Archivos o Transferencias, la edición se cancela y se borra el temporal, recién bajado y sin tocar. Es el mismo criterio que el visor de la F4.

**Permisos:**
- `chmod` nunca toca enlaces: `setstat` los seguiría. Se saltan y se cuentan.
- El aviso ámbar sale cuando, con alcance «todo», algún trío queda con `r` y sin `x`.
- El recursivo cambia en postorden si el modo deja al dueño sin `r` o `x` y en preorden si se las da: así `0644` llega al fondo y `0755` abre lo que estaba cerrado. Lo hacen igual el cliente (local) y el servidor.
- `permisos_cambiados` solo se anota para lo remoto (flujo §4.2).
- Ctrl+S se niega con todas las casillas mixtas o con un octal a medias.

**Sincronización:**
- La cola sigue parando en el primer fichero que falla, como en la F4. El resultado es `parcial` si se escribió algo y `error` si no.
- Un directorio que no queda vacío (contenido excluido) se conserva, se cuenta en el detalle y no es error.
- El plan no borra nada que cuelgue de una ruta omitida, en ninguno de los dos sentidos (choque fichero/directorio o enlace a directorio). El pseudocódigo de §7.1 sí borraría el contenido de un directorio que choca con un fichero.
- También se omite «en destino es un enlace a directorio», para no escribir a través de él.
- Defensa en profundidad:
  - el cliente vuelve a aplicar las exclusiones al árbol remoto;
  - el servidor no borra nada que cuelgue de un enlace por debajo de la raíz;
  - el servidor rechaza una ruta que esté a la vez en destino y en borrado.
- Una sincronización que solo borra (sin elementos) se admite.
- La deliberación se exige con «borrar» marcado aunque el plan no borre nada (lectura literal del informe).
- «Destino dentro del origen» compara por componentes y de forma estricta: dos rutas iguales en local y en remoto se admiten.
- En ASCII el signo de omitido es «.», para no confundirlo con el «-» de borrar.
- La vista previa se titula «SINCRONIZAR · <nombre> ── origen → destino», sin el «MAGI ·» de la maqueta: es un diálogo sobre la vista actual.
- `ultima_ejecucion_en` se escribe al recibir el `Hecho` del `Transferir` y no al enviarlo: una sincronización rechazada no cuenta como lanzada.
- El recorrido local salta ficheros especiales y falla entero si un directorio no se puede leer. Con «borrar», un subdirectorio ilegible del origen haría borrar su contenido en el destino.

**Guardadas:**
- `n`/`e` usan un formulario propio («SINCRONIZACIÓN GUARDADA», con el host fijo) en vez del diálogo SINCRONIZAR en modo edición que proponía el plan.
- La lista no entra en las pruebas comunes de `vistas_g`: su título en ASCII no casa con el buscador común. Las mismas comprobaciones están en `vistas_k`.

**Disposición:**
- Los diálogos nuevos declaran su mínimo (40×12) y su ancho de modo estrecho en `ui/disposicion.rs` (`MINIMO_*`, `ESTRECHO_*`, `minimo_dialogo`, `con_dialogo`).
- `texto_ascii` gana `⚠ → !` y `− → -`.

---

## Estructura de archivos creada/modificada

```
Cargo.toml                       + ignore 0.4, sha2 0.11
src/
├── protocolo.rs                 v5: mensajes y campos nuevos, Borrando, EntradaArbol, AlcancePermisos…
├── modelo.rs                    Sincronizacion, DatosSincronizacion, ResultadoSincronizacion, validar_sincronizacion
├── registro.rs                  sincronizacion y permisos_cambiados (filtro «archivos»)
├── config.rs                    [archivos] editor, excluir (EXCLUIR_POR_DEFECTO), editar_max_mb
├── visor.rs                     editor() con su orden de preferencia
├── main.rs                      magi sincronizaciones
├── almacen/
│   ├── migraciones.rs           migración 6: SINCRONIZACIONES_DIR
│   ├── sincronizaciones.rs      (nuevo) CRUD, marcar_ejecucion, fijar_resultado
│   └── mod.rs                   envoltorios de Almacen
├── archivos/
│   ├── exclusiones.rs           (nuevo) gitignore con `ignore`
│   ├── edicion.rs               (nuevo) ciclo puro de una edición
│   ├── permisos.rs              (nuevo) casillas ⇄ octal, máscara, alcance, chmod local
│   ├── plan.rs                  (nuevo) plan de sincronización
│   ├── listado.rs               (nuevo) listado de `magi sincronizaciones`
│   ├── marcas.rs                Propietario
│   ├── local.rs · panel.rs · mod.rs
├── servidor/
│   ├── sftp.rs                  Renombrador, temporales, MapaNombres, StatRemoto, CambiarPermisos, ListarArbol
│   ├── transferencias.rs        permisos, lista plana, borrar_al_terminar, cierres y anotaciones
│   ├── ejecuciones.rs           validación de deliberación compartida
│   └── mod.rs                   manejadores, OrdenBd::ResultadoSincronizacion
├── app.rs                       enganches: PeticionArchivos/RespuestaArchivos, diálogos, paleta, teclas E p S L
├── app/
│   ├── edicion.rs               (nuevo) E
│   ├── permisos.rs              (nuevo) p
│   ├── sincronizar.rs           (nuevo) S, plan, vista previa, ejecución
│   ├── guardadas.rs             (nuevo) L, paleta
│   ├── dialogo_magi.rs          PlanDeliberado
│   └── lanzar.rs                cerrar_deliberacion compartida
├── snippets/mod.rs              MotivoDeliberacion::Borrar
└── ui/
    ├── edicion.rs · permisos.rs · sincronizar.rs · sincronizaciones.rs   (nuevos)
    ├── dialogos.rs · disposicion.rs · mod.rs · barra.rs · ayuda.rs · pager.rs
    ├── transferencias.rs        fase borrando
    └── deliberacion.rs          plan generalizado
tests/
├── almacen.rs                   migración 5→6, cascada, unicidad, validación, resultado
├── servidor.rs                  un cliente v4 no coopera
├── ejecuciones.rs               deliberación usada por una transferencia
├── sftp.rs                      sobrescribir sobre existente (+ módulos de la fase)
├── sftp/{temporales,permisos,arbol,sincronizacion}.rs   (nuevos)
├── redimensionado/vistas_{h,i,j,k}.rs                  (nuevos) edición, permisos, sincronizar, guardadas
└── snapshots/                   27 nuevas; 13 cambiadas (barra de Archivos, «protocolo 5», paleta)
README.md · docs/fase8/magi-fase8-checklist.md · docs/fase8/magi-fase8-implementacion.md
```

---

## Decisiones técnicas tomadas durante el desarrollo

1. **Contrato antes de las pistas.** Tipos, mensajes, módulos vacíos y enganches en `app.rs` se fijaron en un commit provisional. Los tipos de cada módulo (`DialogoX`, `AccionX`, `PeticionX`, `EventoX`) eran enums vacíos que cada pista rellenaba; así seis pistas trabajaron a la vez con un solo conflicto al integrar.
2. **Operaciones de Archivos en la `App`, no en `EstadoArchivos`.** Ediciones, recuentos y planes sobreviven a salir de Archivos, a cambiar de host o a lanzarse desde la paleta en otra vista. Tienen rango propio de `peticion_id` (2^42) y un único encaminador (`respuesta_archivos`).
3. **Temporales de edición fuera de `tmp`.** `ediciones/` no la vacía ni el arranque ni el apagado del servidor: una edición sobrevive a un reinicio del servidor (T55). Los huérfanos se quedan (ver pendientes).
4. **El servidor no recalcula el plan (T33):** recibe rutas absolutas fijadas y solo las valida (raíz, `..`, enlaces).
5. **Deliberación compartida.** Una sola validación y una guarda de «ya usada» en los dos sentidos, para que una deliberación no autorice a la vez una ejecución y una transferencia. La interfaz de la deliberación se generalizó con `PlanDeliberado`.
6. **Tests contra el `sftp-server` real** para todo lo remoto. Para el renombrador, además, se lanza `sftp-server` como proceso hijo y se prueban los tres pasos y posix-rename por separado.
7. **Diálogos nuevos con mínimo declarado** (40×12, enteros a ese tamaño y limpios en ASCII) y ancho de modo estrecho propio. Entran en `minimo_de` con `con_dialogo`, como el diálogo MAGI.
8. **Un commit por sprint** (S1 · S2 · S3 · cierre), en castellano y sin firma.

---

## Funcionalidades del checklist completadas (copiando su texto exacto)

### Almacén y protocolo
- [x] Migración 6 por `PRAGMA user_version`: tabla `SINCRONIZACIONES_DIR` (id, host_id FK CASCADE, nombre, ruta_local, ruta_remota, direccion, borrar, exclusiones, ultima_ejecucion_en, ultimo_resultado, creado_en, actualizado_en) con `UNIQUE(host_id, nombre)`
- [x] Sin `CHECK` sobre `direccion` ni `ultimo_resultado`; validación en código (nombre `[A-Za-z0-9._-]+`, rutas no vacías, dirección `subida | bajada`)
- [x] `VERSION_PROTOCOLO = 5`; mensajes `StatRemoto`/`Stat`, `CambiarPermisos`, `ListarArbol`/`Arbol`; `Hecho` gana `detalle` opcional; tests de ida y vuelta y rechazo de v4
- [x] `Transferir` gana `permisos?` por elemento, `borrar_al_terminar`, `deliberacion?`, `sincronizacion_id?` y `etiqueta?` (`edicion` | `sincronizacion`); `Transferencias{lista}` gana la fase `borrando` y el recuento de borrados
- [x] `DirListado` y `Arbol` incluyen `usuario` y `grupo` por nombre cuando el host los resuelve
- [x] Tipos nuevos en `REGISTRO`: `sincronizacion` (nombre o «ad hoc», dirección, creados, actualizados, borrados, omitidos, bytes, resultado) y `permisos_cambiados` (host, rutas, modo, alcance, afectados); el filtro `t` de Registro los incluye en «archivos»
- [x] Tests del almacén: migración 5→6, cascada por host, unicidad por host

### Servidor
- [x] `StatRemoto` devuelve la entrada (tamaño, mtime, permisos, uid) o ninguna si no existe
- [x] `CambiarPermisos` aplica `setstat` a las rutas y, con alcance `todo`, `directorios` o `ficheros`, recorre sin seguir enlaces; un error en una ruta no detiene el resto; responde `Hecho` con afectados y errores en `detalle`; anota `permisos_cambiados` tras el efecto
- [x] `ListarArbol` recorre el directorio remoto, aplica las exclusiones recibidas y, con `usar_magiignore`, el `.magiignore` de la raíz; responde en bloques de 1000 entradas con `fin` y devuelve el texto del `.magiignore` en el primer bloque
- [x] Al abrir el canal SFTP de un host, el servidor lee `/etc/passwd` y `/etc/group` (tope 1 MiB, plazo 2 s), guarda el mapa id → nombre mientras el canal vive y, si falla, sigue con números
- [x] La cola aplica `permisos` de cada elemento tras el `rename` del parcial
- [x] `borrar_al_terminar` se ejecuta solo si la transferencia terminó sin errores: ficheros primero, después directorios vacíos de más profundo a menos; remoto por SFTP en subidas y local por el servidor en bajadas
  - AC: Dado una sincronización con 15 ficheros y 2 borrados en la que falla un fichero, cuando termina, entonces no se borra nada y el resultado es «parcial»
- [x] Al terminar una transferencia de sincronización, el servidor anota `sincronizacion`, escribe `SINCRONIZACIONES_DIR.ultimo_resultado` si tiene `sincronizacion_id` y, si hubo deliberación, anota `deliberacion_aprobada` o `deliberacion_forzada` y rellena `DELIBERACIONES.ejecucion_resultado`, todo por el hilo escritor
- [x] Una edición subida se anota como `transferencia` con «edición» en el detalle
- [x] Tests contra el `sftp-server` real: `StatRemoto`, `CambiarPermisos` con los tres alcances, `ListarArbol` con exclusiones y `.magiignore`, permisos tras sobrescribir, borrado al terminar solo con éxito, nombres de propietario

### Edición remota
- [x] `E` sobre un fichero remoto: confirmación por encima de `[archivos] editar_max_mb` (10), `DescargarTemporal`, y confirmación si parece binario (bytes nulos en los primeros 8 KiB)
- [x] Se guardan ruta, mtime, tamaño, permisos, uid y SHA-256 del temporal; el editor (`[archivos] editor` → `$VISUAL` → `$EDITOR` → `nvim` → `vi`) se lanza como programa + argumentos con la TUI suspendida y la pausa de teclado con acuse
- [x] Al volver se aplica el tamaño real de la terminal (T46) y se compara el SHA-256: sin cambios borra el temporal y lo dice
- [x] Con cambios, diálogo «¿subir cambios a host:ruta?»; con «no», elegir descartar o guardar copia local
- [x] Antes de subir, `StatRemoto`: si el mtime o el tamaño difieren de los guardados, diálogo de conflicto (sobrescribir / guardar mi versión como copia local / descartar)
  - AC: Dado un fichero que cambia en el host mientras se edita, cuando se elige subir, entonces aparece el conflicto con los dos tamaños y fechas y nada se sobrescribe sin elegir «sobrescribir»
- [x] Si el uid del original no es el del usuario de conexión, aviso «tras subirlo pertenecerá a <usuario>» con confirmación
- [x] La subida es una transferencia con política sobrescribir y los permisos del original; el aviso de sensibles (F4) se aplica si el nombre casa con los patrones
- [x] El temporal se borra solo tras `hecha`, «sin cambios» o «descartar»; ante error, cancelación o servidor caído se conserva, se muestra su ruta y se ofrece reintentar o guardar copia local
  - AC: Dado un host sin permiso de escritura en el fichero, cuando falla la subida, entonces el temporal sigue existiendo y el mensaje dice dónde está
- [x] La copia local se guarda en el directorio del panel local como `<nombre>.magi-<AAAAMMDD-HHMMSS>`
- [x] Editar el mismo fichero dos veces en la misma ventana avisa «ya lo estás editando»
- [x] `E` sobre un fichero local abre el editor y refresca el panel al volver; sobre un directorio o enlace a directorio no hace nada y lo dice

### Permisos y propietario
- [x] `p` abre el diálogo PERMISOS para los marcados (o la fila actual): rejilla 3×3 de casillas rwx y campo octal (3 o 4 dígitos) sincronizados
- [x] Con varios marcados de modos distintos, las casillas que no coinciden muestran `[~]` y el octal queda vacío hasta tocarlo
- [x] Las casillas solo tocan los 9 bits rwx; los bits especiales se conservan salvo que se escriban en el octal
- [x] Recursivo (solo con directorios marcados) con alcance todo / solo directorios / solo ficheros y confirmación con recuento
- [x] Aviso en ámbar si el modo quita `x` a directorios con alcance «todo»
- [x] Local con `std::fs::set_permissions`; remoto con `CambiarPermisos`; el panel se refresca al terminar
- [x] El detalle `i` muestra propietario y grupo por nombre con el número entre paréntesis (`www-data (33)`), o solo el número si el host no lo resuelve

### Sincronizar directorio
- [x] `S` abre el diálogo SINCRONIZAR con origen = directorio del panel activo, destino = el del otro panel y dirección según el panel activo (local → subida, remoto → bajada)
- [x] Destino dentro del origen (o al revés) se rechaza con mensaje
- [x] Opciones: borrar en destino lo que no está en origen (desmarcada), exclusiones extra, guardar como sincronización con nombre; muestra cuántos patrones trae el `.magiignore` del origen
- [x] Exclusiones en orden: `[archivos] excluir` por defecto (`.git/`, `target/`, `node_modules/`, `__pycache__/`, `.DS_Store`), `.magiignore` del origen y extras, con semántica de gitignore (`ignore`)
- [x] Una ruta excluida no se crea, no se actualiza y nunca se borra en destino
  - AC: Dado `.env` en el destino y `.env` en las exclusiones, cuando se sincroniza con «borrar» marcado, entonces `.env` sigue en el destino
- [x] Plan recursivo: crear lo que falta (`✕`), actualizar lo que difiere en tamaño o mtime (±2 s), borrar lo que sobra solo con la casilla, omitir enlaces a directorio y choques fichero/directorio con motivo
- [x] Permisos en el plan: al crear, los del origen (`& 0o777`); al actualizar, se conservan los del destino
- [x] Árboles de más de 50 000 entradas avisan antes de planificar
- [x] Vista previa con resumen (+ crear, ~ actualizar, − borrar, bytes, excluidos, omitidos), lista desplazable con `↑` `↓` `PgUp` `PgDn`, filtro por tipo con `f` y aviso si borrará o requerirá deliberación; plan vacío muestra «todo al día»
- [x] En subidas, aviso de sensibles (F4) sobre los ficheros a crear o actualizar antes de encolar
- [x] Deliberación MAGI (F6, `snippet_id` NULL, acción «sync <nombre> → host: N ficheros, M borrados») si es una subida a un host con verificaciones activas o si «borrar» está marcado; `Ctrl+K` ejecuta
- [x] Sin deliberación, `↵` en la vista previa ejecuta; la ejecución es un `Transferir` con los elementos, sus permisos, política sobrescribir y las rutas fijadas de `borrar_al_terminar`
- [x] El progreso se ve en Transferencias (F4) con la fase `borrando`; la sincronización sobrevive a cerrar la ventana

### Sincronizaciones guardadas
- [x] `L` en Archivos lista las sincronizaciones del host (nombre, dirección con `−` si borra, rutas, último resultado y fecha) con detalle inferior
- [x] `↵` planifica y abre la vista previa; `n` / `e` / `x` crean, editan y borran (confirmación) con validación
- [x] Al lanzar una guardada, el cliente escribe `ultima_ejecucion_en`; el servidor escribe `ultimo_resultado` al terminar
- [x] Entradas de paleta `sync · <host> · <nombre>`, `sincronizar directorio` y `editar fichero`
- [x] Una guardada cuyo host o rutas no existen falla al planificar con el motivo
- [x] `magi sincronizaciones` lista las guardadas con host, nombre, dirección, rutas, borrar y último resultado

### Interfaz y configuración
- [x] `config.toml`: `[archivos] editor`, `excluir = [".git/", "target/", "node_modules/", "__pycache__/", ".DS_Store"]` y `editar_max_mb = 10`
- [x] Teclas `E`, `p`, `S` y `L` en Archivos, en la barra inferior por prioridad y en la ayuda `?`
- [x] Teclado de los diálogos nuevos: `Tab` / `Shift+Tab` recorren campos, `Espacio` marca casillas, `↑` `↓` `←` `→` mueven por la rejilla de PERMISOS, `Ctrl+S` aplica y `Esc` cancela
- [x] Signos `+` `~` `−` `·` con color (verde, ámbar, rojo, gris) y en ASCII
- [x] Diálogos PERMISOS, SINCRONIZAR, conflicto de edición, vista previa y lista de guardadas con mínimo y modo estrecho declarados en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)
- [x] Ninguna acción destructiva (sobrescribir tras conflicto, `chmod` recursivo, borrar en destino, descartar una edición) sin confirmación

### Calidad
- [x] Tests unitarios del plan (crear, actualizar, borrar, omitir, permisos, orden de borrado), de las exclusiones y del ciclo de edición (sin cambios, conflicto, error de subida conserva el temporal)
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde, incluidas las instantáneas nuevas
- [ ] Revisión adversarial del código nuevo (servidor y cliente) con verificación independiente de cada hallazgo, resultado en el informe de implementación — **no hecha** (ver pendientes)
- [x] README: edición remota, permisos, sincronizar directorio, `.magiignore`, sincronizaciones guardadas

---

## Pendientes y bloqueos

1. **Revisión adversarial de cierre: no hecha.**
   - Hector pidió saltarla el 3 oct 2026. `CLAUDE.md` exige anotarlo con lo que queda sin revisar: **todo el código nuevo de la fase**, unas 20 000 líneas de código y tests.
   - Lo que más importa, por riesgo de pérdida de datos:
     - **Servidor, `servidor/transferencias.rs`:** borrado al terminar (dos pasadas, enlaces bajo la raíz, validación de rutas), cierre único con `abortar_host`/`abortar_todas`/`cerrar_sin_motor`, permisos del parcial.
     - **Servidor, `servidor/sftp.rs`:** renombrado en tres pasos con deshacer y ciclo de vida del canal raw, temporales de edición, `CambiarPermisos` recursivo en pre/postorden, `ListarArbol` con exclusiones y enlaces.
     - **Cliente, `app/edicion.rs`:** que el temporal no se borre nunca antes de tiempo (T55) con pila de diálogos, servidor caído y respuestas obsoletas.
     - **Cliente, `app/sincronizar.rs` y `archivos/plan.rs`:** que el plan no borre nada excluido, fuera de la raíz o dentro de un omitido; orden de exclusiones igual en los dos árboles; bloques `Arbol` tardíos.
     - **Cliente, `app/permisos.rs`:** chmod local recursivo.
     - **Cliente, deliberación generalizada:** regresiones en snippets.
   - El script del workflow de revisión (siete focos con verificación 2/3) está guardado en la sesión y puede relanzarse tal cual. Mientras tanto, el riesgo es el de R38 en la Fase 6: en F4 y F5 la revisión encontró 13 y 25 defectos que los tests no cazaban.
2. **Validación manual.** Nada de la fase se ha probado a mano en una terminal real con un host real. Guion abajo.
3. **Temporales de edición huérfanos.** Una ventana que muere, o un fallo cerrado con `esc`, deja su temporal en `…/magi/ediciones` para siempre: nadie lo vacía, a propósito (T55). Candidato a una limpieza explícita en una fase futura.
4. **Latencia al abrir un canal SFTP.**
   - El canal raw y la lectura de `passwd`/`group` se hacen dentro de `abrir`: los nombres hasta 2 s y el canal raw hasta 10 s por paso si el host se cuelga.
   - También pasa con `ModoSftp::Comprobacion`, así que BALTHASAR-2 (plazo duro de 2 s) puede vencer más a menudo sobre un host lento sin canal abierto. Hay que medirlo.
5. **Dos canales de sesión por canal SFTP** (principal + raw): cuentan para `MaxSessions` del host (10 por defecto en OpenSSH).
6. **Ficheros especiales.** `ListarArbol` trata FIFOs, sockets y dispositivos como ficheros, igual que `listar` de la F4. Una bajada de sincronización intentaría leerlos. El recorrido local sí los salta.
7. **TOCTOU en `CambiarPermisos`.** Entre el `lstat` y el `setstat` una ruta podría cambiarse por un enlace, y `setstat` lo seguiría. SFTP no tiene `fsetstat` para directorios.
8. **Recuento de PERMISOS con el servidor caído.** Si el diálogo de recuento estaba apartado en la pila bajo una pregunta del servidor, vuelve a verse tras SERVIDOR CAÍDO y espera hasta `Esc`. No hay enganche `permisos_servidor_caido`.
9. **Rendimiento sin medir.**
   - El plan de 10 000 entradas en menos de 3 s en LAN (§8).
   - La vista previa recalcula notas y anchos de todo el plan en cada pintado.
   - R47: árboles de más de 50 000 entradas.
10. **Rango de `peticion_id` 2^42** para Archivos en la `App`: añadirlo a la regla de rangos de `CLAUDE.md` (lo mantiene el analista).
11. **Prueba intermitente corregida en el cierre.**
    - Qué fallaba: `vistas_j::s_planifica_una_subida_y_la_lanza_sin_deliberar` fallaba alrededor de una vez de cada tres. El `README.md` del hogar de prueba salía con 0600.
    - Causa: `servidor::arrancar` pone la umask del proceso en 077 un instante para crear el socket. Es código de la F3. En el binario de pintado, otras pruebas arrancan el servidor en el mismo proceso.
    - Arreglo: `vistas_d::preparar_hogar` fija los modos explícitamente. Pasó 8 de 8 seguidas.
    - En producción no afecta: el servidor es un proceso aparte y la ventana solo cubre el `bind`.
12. **`tests/pool.rs::sigterm_para_el_servidor_limpio`** falló una vez en una pista con la máquina muy cargada (seis worktrees compilando) y pasó al repetir. Es R42, ya conocido.
13. **Heredados sin tocar:** R38 y R40 (Fase 6), validación manual de las Fases 2-6 (R15) y la de la Fase 7.

---

## Ejecución y pruebas (cómo arrancar, migrar y testear)

**Arrancar**
- `cargo run` abre la TUI. Para depurar el servidor: `cargo run -- --servidor` en otra terminal (log en `~/.local/state/magi/logs/servidor.log.<fecha>`).
- `cargo run -- sincronizaciones` lista las guardadas sin la TUI.
- **El servidor de sesiones de una versión anterior (protocolo 4) no coopera**: hay que pararlo antes (`magi servidor parar` con el binario viejo, o el aviso de versión de la TUI con su pid).

**Migrar**
- La migración 6 se aplica sola al abrir `magi.db` (`PRAGMA user_version` 5 → 6) y crea `SINCRONIZACIONES_DIR` vacía.
- Las claves nuevas de `[archivos]` (`editor`, `excluir`, `editar_max_mb`) toman su valor por defecto si faltan: un `config.toml` existente no hace falta tocarlo.

**Testear**
- `TZ=UTC cargo test` ejecuta todo:
  - 468 unitarios;
  - almacén 38;
  - SFTP 48 contra el `sftp-server` real;
  - servidor 12;
  - pintado 250, con 27 instantáneas nuevas;
  - el resto, como antes.
- Por bloques:
  - `cargo test --test sftp`: temporales, permisos, árbol, sincronización y sobrescribir.
  - `cargo test --test redimensionado vistas_h vistas_i vistas_j vistas_k`: edición, permisos, sincronizar y guardadas.
  - `cargo test --lib archivos::`: plan, exclusiones, permisos y edición puros.
- Instantáneas: `INSTA_UPDATE=always cargo test --test redimensionado` y revisar con `git diff tests/snapshots`.
- Antes de cada commit: `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`.

**Guion de prueba manual** (Hyprland/Alacritty, servidor en primer plano, un host real y uno `localhost`)
1. **Editar.**
   - `E` sobre un fichero remoto: se abre nvim; guardar sin cambios → «sin cambios» y el temporal desaparece de `$XDG_RUNTIME_DIR/magi/ediciones`.
   - Repetir cambiando algo → subir. Los permisos se conservan (`ls -l` en el host).
2. **Conflicto.** Editar; mientras, cambiar el fichero en el host desde otra pestaña; subir → conflicto con los dos tamaños y fechas. Probar `c`: la copia aparece en el panel local.
3. **Fallo de subida.** Fichero de root sin permiso de escritura (o directorio sin `w`) → la subida falla, el diálogo da la ruta del temporal y `r` reintenta.
4. **Propietario.** Editar un fichero de otro usuario con permiso de escritura → aviso «pertenecerá a <usuario>».
5. **Enlaces.** `E` sobre un enlace de `sites-enabled` → se edita el fichero de `sites-available` y el enlace sigue siendo enlace.
6. **Permisos.** `p` con varios ficheros de modos distintos (`[~]`), `0644` recursivo «todo» sobre un árbol (aviso ámbar, recuento, confirmación), solo directorios a `0755`; comprobar con `ls -lR`.
7. **Sincronizar en subida.** De un proyecto con `.git/`, `node_modules/` y un `.magiignore`: vista previa, filtro `f`; ejecutar y ver la cola.
8. **Sincronizar con «borrar».** Pasa por la deliberación (`Ctrl+K`); con un fichero ilegible en el origen → nada borrado y «parcial» en `L`.
9. **Sincronizar en bajada.** Probar el `.magiignore` remoto.
10. **Guardadas.** Guardar como «web-prod»; `L` → `↵`; `sync · host · web-prod` desde la paleta en Flota; `magi sincronizaciones`.
11. **Supervivencia.** Cerrar la ventana durante una sincronización larga → sigue en el servidor (`magi servidor estado`).
12. **Tamaños.** Redimensionar con cada diálogo nuevo abierto (40×12, estrecho, grande).
