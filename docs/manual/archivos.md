# Archivos

[← Índice del manual](README.md)

## Archivos

`F4` (o `s` en Hosts y Flota, o el prefijo `f` en Sesión, o `sftp · <host>` en
la paleta) abre la vista Archivos: un panel doble local ⇄ remoto. Todo lo
remoto lo hace el servidor sobre un canal SFTP por host, así que las
transferencias siguen cuando se cierra la ventana.

- **Paneles**: `Tab` cambia de panel; el activo lleva el nombre en ámbar. Las
  columnas son nombre, tamaño y fecha (`hoy`, `ayer`, `12 sep`, `2025`), con
  los directorios primero y una fila `..` para subir.
- **Marcas**: `≠` si el elemento existe al otro lado con otro tamaño o fecha
  (tolerancia de 2 s) y `✕` si no está. Los directorios solo llevan `✕`. Se
  comparan los dos listados visibles, así que sirven para ver qué cambió en un
  despliegue.
- **Copiar y mover**: `c` copia y `m` mueve lo marcado (o la fila actual) al
  directorio del otro panel. Si el destino ya existe, se pregunta sobrescribir
  u omitir, con el tamaño y la fecha de los dos lados; `S` y `O` aplican la
  decisión a todo lo que queda. Un «mover» borra el origen solo cuando la
  copia ha terminado bien.
- **Aviso de sensibles**: antes de subir, se avisa si algún fichero coincide
  con los patrones de `[archivos] avisar` (por defecto `.env*`, `*.pem`,
  `*.key`, `id_*`), recorriendo también los directorios. Con la lista vacía el
  aviso queda desactivado y la barra lo dice.
- **Cola**: al pie, tres filas con la transferencia en curso y lo que espera
  turno. `t` abre la vista ampliada (`x` cancelar, `C` limpiar terminadas, `↵`
  detalle), y `magi servidor estado` lista la cola desde fuera de la TUI. Una
  transferencia en curso se cancela por bloques de 64 KiB: el parcial se borra
  y nada queda a medias.
- **Ver ficheros**: `↵` sobre un fichero lo abre con `[archivos] pager`
  (`$PAGER` o `less`): la TUI se suspende, se ve el fichero y se restaura. Un
  remoto se trae antes a un temporal en `$XDG_RUNTIME_DIR/magi/tmp` (700, 600)
  que se borra al cerrar el visor; si pasa de 10 MB se pide confirmación.
- **Borrar, renombrar y crear**: `x` borra sin papelera tras confirmar con el
  recuento, `r` renombra y `d` crea un directorio. En remoto lo hace el
  servidor (el borrado, recursivo) y queda anotado en el historial.
- **Editar** (`E`): un fichero local se abre con tu editor y el panel se
  refresca al volver. Uno remoto se baja a un temporal privado
  (`$XDG_RUNTIME_DIR/magi/ediciones`, 700 y 600), se abre con el editor
  (`[archivos] editor`, `$VISUAL`, `$EDITOR`, `nvim` o `vi`, lanzado sin shell
  y con la TUI suspendida) y, si el SHA-256 cambió, se pregunta si subirlo.
  Antes de subir se comprueba el remoto: si cambió mientras editabas sale el
  conflicto (`s` sobrescribir, `c` guardar tu versión como copia local
  `<nombre>.magi-<AAAAMMDD-HHMMSS>` en el panel local, `d` descartar); si pasará
  a ser de otro usuario o es un fichero sensible, se avisa. La subida conserva
  los permisos del original. **El temporal no se borra hasta que la subida
  termina, dices «sin cambios», descartas o guardas la copia local**: si algo
  falla se conserva, se dice su ruta y se ofrece reintentar. Por encima de
  `editar_max_mb` (10) o si parece binario, se pide confirmación. Sobre un
  enlace a fichero se edita su destino; sobre un directorio, nada.
- **Permisos** (`p`): rejilla rwx (usuario, grupo, otros) y octal de 3 o 4
  dígitos sincronizados sobre lo marcado. Con modos distintos, las casillas que
  no coinciden salen `[~]` y se respetan en cada fichero; las casillas solo
  tocan los 9 bits rwx y los especiales solo cambian si se escriben en el
  octal. Con directorios, recursivo con alcance todo, solo directorios o solo
  ficheros, con confirmación y recuento; un aviso en ámbar dice si los
  directorios se quedarán sin `x`. Los enlaces nunca se tocan. `Ctrl+S` aplica.
  En remoto lo hace el servidor y queda anotado (`permisos_cambiados`).
- **Propietario**: el detalle `i` enseña propietario y grupo por nombre con el
  número entre paréntesis (`www-data (33)`). En remoto los nombres salen de
  `/etc/passwd` y `/etc/group` del host, leídos por SFTP al abrir el canal (sin
  ejecutar nada); usuarios de LDAP/SSSD se quedan con el número.
- **Sincronizar** (`S`): sincroniza el directorio del panel activo con el del
  otro, en cualquier dirección (activo local → subida, remoto → bajada). El
  diálogo SINCRONIZAR ofrece borrar en el destino lo que no está en el origen
  (desmarcado), exclusiones extra y guardar la sincronización con un nombre. El
  plan es recursivo: crear lo que falta, actualizar lo que difiere en tamaño o
  fecha (±2 s), borrar lo que sobra solo con la casilla y omitir, con su motivo,
  los enlaces a directorio y los choques fichero/directorio. Al crear se usan
  los permisos del origen; al actualizar se conservan los del destino. La vista
  previa es la confirmación: resumen `+ crear · ~ actualizar · − borrar`,
  bytes, excluidos y omitidos, lista con `↑` `↓` `PgUp` `PgDn`, filtro `f` y
  `↵` para seguir (si el plan está vacío, «todo al día»). En subidas se avisa de
  ficheros sensibles; si se sube a un host con verificaciones o se marca
  «borrar», pasa por la deliberación MAGI (`Ctrl+K` ejecuta). Se ejecuta en la
  cola del servidor (sobrevive a cerrar la ventana) y **solo borra al terminar,
  si todo se copió bien**: si falla un fichero, no se borra nada y queda
  «parcial». Transferencias enseña la fase `borrando`.
- **Exclusiones y `.magiignore`**: con la sintaxis de gitignore y en este
  orden: `[archivos] excluir` (por defecto `.git/`, `target/`, `node_modules/`,
  `__pycache__/`, `.DS_Store` y `.magiignore`), el `.magiignore` de la raíz del
  origen y las exclusiones extra. **Lo excluido no se crea, no se actualiza y
  nunca se borra** en el destino.
- **Sincronizaciones guardadas** (`L`): las del host, con dirección (`−` si
  borra), rutas y último resultado. `↵` planifica y abre la vista previa; `n`
  nueva, `e` editar, `x` borrar (con confirmación). También desde la paleta
  (`sync · <host> · <nombre>`) y con `magi sincronizaciones`.

Últimos directorios: cada host recuerda dónde se quedó cada panel
(`HOSTS.sftp_dir_local` y `HOSTS.sftp_dir_remoto`), así que al volver se abre
donde se estaba.

```sh
magi sincronizaciones   # host, nombre, dirección, rutas, borrar y último resultado
```
