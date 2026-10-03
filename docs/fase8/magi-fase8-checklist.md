# MAGI - Checklist Fase 8: Archivos, Segunda Vuelta

## Almacén y protocolo
- [x] Migración 6 por `PRAGMA user_version`: tabla `SINCRONIZACIONES_DIR` (id, host_id FK CASCADE, nombre, ruta_local, ruta_remota, direccion, borrar, exclusiones, ultima_ejecucion_en, ultimo_resultado, creado_en, actualizado_en) con `UNIQUE(host_id, nombre)`
- [x] Sin `CHECK` sobre `direccion` ni `ultimo_resultado`; validación en código (nombre `[A-Za-z0-9._-]+`, rutas no vacías, dirección `subida | bajada`)
- [x] `VERSION_PROTOCOLO = 5`; mensajes `StatRemoto`/`Stat`, `CambiarPermisos`, `ListarArbol`/`Arbol`; `Hecho` gana `detalle` opcional; tests de ida y vuelta y rechazo de v4
- [x] `Transferir` gana `permisos?` por elemento, `borrar_al_terminar`, `deliberacion?`, `sincronizacion_id?` y `etiqueta?` (`edicion` | `sincronizacion`); `Transferencias{lista}` gana la fase `borrando` y el recuento de borrados
- [x] `DirListado` y `Arbol` incluyen `usuario` y `grupo` por nombre cuando el host los resuelve
- [x] Tipos nuevos en `REGISTRO`: `sincronizacion` (nombre o «ad hoc», dirección, creados, actualizados, borrados, omitidos, bytes, resultado) y `permisos_cambiados` (host, rutas, modo, alcance, afectados); el filtro `t` de Registro los incluye en «archivos»
- [x] Tests del almacén: migración 5→6, cascada por host, unicidad por host

## Servidor
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

## Edición remota
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

## Permisos y propietario
- [x] `p` abre el diálogo PERMISOS para los marcados (o la fila actual): rejilla 3×3 de casillas rwx y campo octal (3 o 4 dígitos) sincronizados
- [x] Con varios marcados de modos distintos, las casillas que no coinciden muestran `[~]` y el octal queda vacío hasta tocarlo
- [x] Las casillas solo tocan los 9 bits rwx; los bits especiales se conservan salvo que se escriban en el octal
- [x] Recursivo (solo con directorios marcados) con alcance todo / solo directorios / solo ficheros y confirmación con recuento
- [x] Aviso en ámbar si el modo quita `x` a directorios con alcance «todo»
- [x] Local con `std::fs::set_permissions`; remoto con `CambiarPermisos`; el panel se refresca al terminar
- [x] El detalle `i` muestra propietario y grupo por nombre con el número entre paréntesis (`www-data (33)`), o solo el número si el host no lo resuelve

## Sincronizar directorio
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

## Sincronizaciones guardadas
- [x] `L` en Archivos lista las sincronizaciones del host (nombre, dirección con `−` si borra, rutas, último resultado y fecha) con detalle inferior
- [x] `↵` planifica y abre la vista previa; `n` / `e` / `x` crean, editan y borran (confirmación) con validación
- [x] Al lanzar una guardada, el cliente escribe `ultima_ejecucion_en`; el servidor escribe `ultimo_resultado` al terminar
- [x] Entradas de paleta `sync · <host> · <nombre>`, `sincronizar directorio` y `editar fichero`
- [x] Una guardada cuyo host o rutas no existen falla al planificar con el motivo
- [x] `magi sincronizaciones` lista las guardadas con host, nombre, dirección, rutas, borrar y último resultado

## Interfaz y configuración
- [x] `config.toml`: `[archivos] editor`, `excluir = [".git/", "target/", "node_modules/", "__pycache__/", ".DS_Store"]` y `editar_max_mb = 10`
- [x] Teclas `E`, `p`, `S` y `L` en Archivos, en la barra inferior por prioridad y en la ayuda `?`
- [x] Teclado de los diálogos nuevos: `Tab` / `Shift+Tab` recorren campos, `Espacio` marca casillas, `↑` `↓` `←` `→` mueven por la rejilla de PERMISOS, `Ctrl+S` aplica y `Esc` cancela
- [x] Signos `+` `~` `−` `·` con color (verde, ámbar, rojo, gris) y en ASCII
- [x] Diálogos PERMISOS, SINCRONIZAR, conflicto de edición, vista previa y lista de guardadas con mínimo y modo estrecho declarados en `ui/disposicion.rs` e instantáneas a 40×12, 80×24 y 200×60 (T45)
- [x] Ninguna acción destructiva (sobrescribir tras conflicto, `chmod` recursivo, borrar en destino, descartar una edición) sin confirmación

## Calidad
- [x] Tests unitarios del plan (crear, actualizar, borrar, omitir, permisos, orden de borrado), de las exclusiones y del ciclo de edición (sin cambios, conflicto, error de subida conserva el temporal)
- [x] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [x] `cargo test` verde, incluidas las instantáneas nuevas
- [ ] Revisión adversarial del código nuevo (servidor y cliente) con verificación independiente de cada hallazgo, resultado en el informe de implementación
- [x] README: edición remota, permisos, sincronizar directorio, `.magiignore`, sincronizaciones guardadas

---

**Progreso Fase 8:** 63 / 64 funcionalidades (falta la revisión adversarial: Hector pidió saltarla; queda en pendientes del informe)

**Total MAGI (Fases 1-8):** 596 / 600 funcionalidades
