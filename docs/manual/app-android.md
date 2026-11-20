# App Android

[← Índice del manual](README.md)

## App Android

La app compañera para Android 11 o superior vive en [`android/`](../../android/) y se
publica aparte del escritorio, con etiquetas `android-vX.Y.Z`: es **lectora**
de la sincronización (el inventario llega del escritorio; el móvil nunca escribe
en la carpeta) y permite conectar a los hosts, abrir el terminal y usar túneles.

Desde la Fase 14 es también el **móvil de guardia**: Flota es su pantalla de
inicio, con el sondeo del escritorio (estados, métricas, vigilados y fallos) y
refresco solo con la pantalla a la vista; Servicios maneja los servicios,
timers y contenedores del host como en el escritorio, con acciones, sudo en
memoria y logs en vivo; Snippets ejecuta los sincronizados en uno o varios
hosts con variables, resultados y deliberación MAGI (lo deliberado se ejecuta
manteniendo pulsado 1 s); y Registro enseña lo que ha hecho el móvil, con
filtros y el detalle de cada deliberación. Toda la lógica (sondeo, acciones,
lectura de salidas, snippets y deliberación) vive en el núcleo compartido; la
app solo pinta y no decide qué se permite. Las acciones y las ejecuciones
siguen en segundo plano con el servicio en primer plano; el sondeo, los
refrescos y los logs solo corren con su pantalla a la vista. Funciona sin
Google Play Services, sin cuenta y sin telemetría.

Se instala y actualiza con [Obtainium](https://github.com/ImranR98/Obtainium)
desde las [releases](https://github.com/HectorEspejo/magi/releases) del repositorio:
añade la app con la URL del repositorio y filtra las etiquetas por `android-v`
y los APK por `magi-android-`. Las releases de Android **nunca** se marcan como
«Latest» (esa marca es del escritorio), así que en la lista busca la etiqueta
`android-v…`; si quieres probar candidatas, activa las prereleases (`-rc.N`).

Antes de instalar o actualizar, comprueba el APK y su certificado:

```sh
sha256sum -c magi-android-v0.2.0.apk.sha256
apksigner verify --print-certs magi-android-v0.2.0.apk
gh attestation verify magi-android-v0.2.0.apk --repo HectorEspejo/magi
```

`apksigner` viene con las build-tools del SDK de Android y `gh` es el GitHub
CLI. La huella SHA-256 que imprime `apksigner` debe coincidir con la de las
notas de la release y con la de las versiones anteriores de la app: si cambia,
no instales el APK y avisa. La guía completa (keystore, secretos, etiquetas y
qué hace el CI) está en
[`docs/publicar-android.md`](../publicar-android.md).

En el móvil, la carpeta de sincronización debe compartirse con Syncthing-Fork en
modo **«solo recibir»**: así el teléfono nunca escribe en la carpeta y no puede
pisar los datos de los escritorios.

Si pierdes el móvil, o lo reinstalas: cambia la clave de sincronización en el
escritorio (`r` en la vista SINCRONIZACIÓN), quita el teléfono de Syncthing y
borra de `~/.ssh/authorized_keys` la línea de la clave del móvil (identifícala
con la huella que mostraba Ajustes → Clave del móvil). Las contraseñas y la
clave del móvil estaban cifradas con el Keystore y protegidas por el bloqueo del
teléfono; si hay sospecha, cambia también las contraseñas de los hosts. Al
instalar de nuevo la app hay que crear otra clave del móvil, volver a unirse a
la sincronización y reaceptar las huellas.
