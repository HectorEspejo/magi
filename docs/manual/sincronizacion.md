# Sincronización del inventario

[← Índice del manual](README.md)

## Sincronización del inventario

Desde la 1.1.0 el mismo inventario puede estar en todos tus equipos sin cuenta
ni servicio: MAGI escribe un fichero cifrado por equipo en una carpeta que tú
eliges (Syncthing, Nextcloud, iCloud Drive, Dropbox, un repositorio git…) y lee
los de los demás. `magi.db` no se cifra (no guarda secretos y tiene permisos
600); lo que sale del equipo va cifrado con age (X25519) y firmado con
HMAC-SHA256, y la clave vive solo en el llavero del sistema.

### Activar la sincronización

1. Añade la carpeta a `config.toml` (o hazlo desde la vista SINCRONIZACIÓN):

   ```toml
   [sincronizacion]
   carpeta = "~/Sync/magi"   # sin carpeta, la sincronización está desactivada
   retraso_seg = 5           # espera tras el último cambio local antes de exportar
   copias = 10               # copias de magi.db que se conservan antes de importar
   umbral_borrado_hosts = 5  # más de N hosts borrados piden confirmación
   umbral_borrado_pct = 20   # o al menos el N % de los hosts
   ```

2. Abre MAGI, `Ctrl+P` → «sincronización» y pulsa `n` para elegir la carpeta.
3. Si la carpeta no tiene ficheros de MAGI, MAGI crea la clave y la muestra como
   texto y QR: guárdala en tu gestor de contraseñas y pulsa `s` para confirmar.
   Si ya tiene ficheros de otros equipos, pega la clave compartida (diálogo
   UNIR).
4. Repite en los demás equipos con la misma clave (o escanea el QR desde la app
   Android, F11).

### Añadir un equipo

En el equipo nuevo: vista SINCRONIZACIÓN → `n` → elegir la misma carpeta →
UNIR → pegar la clave. Los elementos con el mismo nombre se unen (gana la
versión más reciente y la otra queda como conflicto recuperable); el resto se
suma. La primera pasada importa todo.

### La clave

- Se muestra con `k` (texto y QR) y se puede rotar con `r`.
- Al quitar un equipo con `x`, MAGI borra su fichero y **cambia la clave**; los
  demás equipos pedirán la nueva la próxima vez.
- Solo si el fichero de un equipo conocido está cifrado para otra clave, la
  sincronización se detiene hasta que introduzcas la clave nueva; no se pierde
  nada. Un fichero cortado, vacío o a medio copiar (lo normal con Syncthing)
  sale como «a medio copiar o dañado»: se avisa con el nombre del fichero, se
  importan los demás y se reintenta en la siguiente pasada, sin pedir nada.
- Durante los 30 días siguientes a rotar la clave, la anterior solo lee ficheros
  de equipos que este ya conocía y no están quitados; el de un equipo
  desconocido se rechaza con un aviso (y una línea en el log, sin su contenido).
- Sin la clave no se puede leer ni fabricar un fichero, pero tus datos siguen en
  el `magi.db` de cada equipo: puedes desactivar (`d`) y crear otra
  sincronización.

### Cuándo sincroniza

- Al abrir MAGI, al cambiar la carpeta, unos segundos después de cada cambio
  local y al pulsar `s`; también con `magi sincronizar` (código 0 bien, también
  con borrados aplazados; 1 error; 2 si hace falta decidir) y `magi sincronizar
  estado`, que enseña el estado y los borrados aplazados con su motivo sin
  descifrar nada.
- Antes de borrar un host o un túnel que llega por la sincronización,
  `magi sincronizar` pregunta al servidor de sesiones, sin arrancarlo nunca y
  con un plazo de 5 s: sin servidor aplica el borrado; si el host tiene trabajo
  vivo (pestañas, túneles, transferencias, ejecuciones o escritorios) aplaza
  ese borrado, y con un servidor de otra versión o que no responde aplaza todos
  los de hosts y túneles. Un aplazado no es un error: la salida dice por qué y
  se resuelve desde CONFLICTOS (`a` acepta, `d` descarta).

  ```
  $ magi sincronizar
  Sincronización · carpeta ~/Sync/magi
    importados: portatil · sobremesa (al día)
    cambios: 2 creados · 1 actualizados · 0 borrados · 0 conflictos · 0 huellas
    ! magi-3f0a….age: a medio copiar o dañado; se reintenta en la siguiente pasada
    aplazado: borrar web-01 (tiene trabajo vivo en el servidor de sesiones: 1 sesión(es))
    exportado: magi-6b1e….age
  1 aviso · 1 borrado aplazado
  ```
- Solo un proceso sincroniza a la vez (cerrojo en
  `$XDG_RUNTIME_DIR/magi/sincronizacion.lock`).
- Antes de importar algo que cambia el inventario guarda una copia de `magi.db`
  en `~/.local/share/magi/copias/` y, si va a borrar muchos hosts, pide
  confirmación.
- Los conflictos (dos equipos editaron lo mismo a la vez, dos nombres iguales,
  huellas distintas, borrados con sesiones abiertas) se ven en la vista
  CONFLICTOS: `r` recupera tu versión, `a` acepta la del otro, `d` descarta. El
  detalle nombra el tipo del elemento con palabras («túnel», «sesión
  persistente», «sincronización de directorio», «comprobación de servicio») y el
  tipo de conflicto «huérfano» lleva tilde.
- El indicador `⇅` de la barra inferior resume el estado: al día, conflictos,
  pausa, clave nueva, error o sincronizando (en ASCII, «sync ok», «sync N!»,
  «sync pausa», «sync clave», «sync error», «sync …»).

### Abrir un fichero fuera de MAGI

El fichero `magi-<uuid>.age` es un age estándar. Guarda la clave en un fichero
con permisos 600 y:

```sh
age -d -i clave.txt magi-<uuid>.age
```

La cabecera, la firma y el JSON están documentados en
[`docs/formato-sincronizacion.md`](../formato-sincronizacion.md) para la app
Android.
