# Termux

[← Índice del manual](README.md)

## MAGI en Termux

Desde la versión 1.6.1, MAGI corre también en Android dentro de
[Termux](https://termux.dev): es el mismo binario del escritorio compilado para
`aarch64-linux-android` (Android 7 o posterior), con la misma interfaz —Flota,
sesiones, túneles, SFTP, servicios, snippets y sesiones persistentes— y lo propio
de Termux aparte: un llavero con frase maestra, el portapapeles y los avisos de
Android, la fila de teclas y la carpeta de sincronización del almacenamiento
compartido. Se soporta el Termux de **F-Droid o de GitHub**; el de Google Play
arranca, pero `magi diagnostico` avisa de que no se soporta.

### Instalar

Descarga de [la release](https://github.com/HectorEspejo/magi/releases) el `.deb` y su
`.sha256`, compruébalo e instálalo (solo existe para `aarch64`; `dpkg` rechaza
otra arquitectura):

```sh
curl -LO https://github.com/HectorEspejo/magi/releases/download/v1.6.1/magi-ssh_1.6.1_aarch64.deb
curl -LO https://github.com/HectorEspejo/magi/releases/download/v1.6.1/magi-ssh_1.6.1_aarch64.deb.sha256
sha256sum -c magi-ssh_1.6.1_aarch64.deb.sha256
apt install ./magi-ssh_1.6.1_aarch64.deb
magi --version
```

Con `pkg install gh` puedes comprobar además la procedencia, como en el
escritorio: `gh attestation verify magi-ssh_1.6.1_aarch64.deb --repo HectorEspejo/magi`.
Si MAGI está en [TUR](https://github.com/termux-user-repository/tur) (la receta
vive en [`packaging/termux/`](../../packaging/termux/)), se instala y se actualiza con
`pkg`:

```sh
pkg install tur-repo
pkg install magi-ssh
```

El paquete instala `magi`, su página man y el autocompletado de bash, zsh y fish,
y no depende de nada: sugiere `termux-api` y `openssh`. Para desinstalarlo,
`apt remove magi-ssh` (tus datos, en `~/.local/share/magi`, no se tocan). La guía
para publicarlo está en [`docs/publicar-termux.md`](../publicar-termux.md).

### Termux:API y el almacenamiento

- **Termux:API.** Para copiar al portapapeles de Android y para los avisos,
  instala el paquete y la app, esta última **de la misma fuente que Termux**
  (F-Droid o GitHub): `pkg install termux-api`. Sin ella, MAGI copia por OSC 52
  solo unos 4 KiB y no publica avisos de Android; con ella copia hasta 512 KiB y
  avisa cuando una sesión persistente o una pestaña pide atención. `magi termux
  aviso` publica un aviso de prueba (y lo retira a los 10 s) para comprobar el
  permiso de notificaciones. Los enlaces y los escritorios remotos se abren en el
  navegador del móvil con `termux-open-url`, que ya trae Termux.
- **Almacenamiento compartido.** Para elegir como carpeta de sincronización una
  del almacenamiento del móvil (la que comparte Syncthing para Android u otra
  app), ejecuta antes `termux-setup-storage` y concede el permiso: la carpeta
  `~/storage/shared/…` se guarda ya resuelta (`/storage/emulated/0/…`). Termux se
  une como un equipo más (`termux-<modelo>`) y MAGI revisa la carpeta cada 30 s
  mientras está abierto.

### Fila de teclas

La fila de teclas que trae Termux no tiene las teclas de función ni el prefijo de
MAGI. MAGI propone una (`ESC TAB CTRL ALT ^] ← ↓ ↑ →` y `F1`-`F9`), generada con
su prefijo:

```sh
magi termux teclas              # enseña la fila propuesta y la actual, sin escribir
magi termux teclas --instalar   # la instala, con confirmación y copia del fichero
```

También se ofrece una vez en el primer arranque y desde la paleta (`Ctrl+P`).
Solo cambia la línea `extra-keys` de `termux.properties` (deja antes una copia
`termux.properties.magi-<fecha>`) y ejecuta `termux-reload-settings`. Sin instalar
nada, Termux da las teclas de función con **Volumen arriba + 1…9** (`F1`…`F9`) y
**Volumen arriba + 0** (`F10`).

### Llavero con frase maestra

En Termux no hay llavero del sistema: MAGI guarda las contraseñas recordadas y la
clave de sincronización en `llavero.age`, un fichero cifrado con una **frase
maestra** de al menos 12 caracteres que MAGI te pide la primera vez que necesita
guardar un secreto, y después al abrirse. **La frase no se recupera**: si la
olvidas, borra el llavero (`magi llavero borrar`), vuelve a unir el equipo a la
sincronización y escribe otra vez las contraseñas; el inventario sigue en tus
demás equipos. La frase vive solo en la memoria de la ventana de MAGI, nunca en
disco. `magi llavero estado` dice si hay llavero y `magi llavero cambiar` cambia la
frase; `Ctrl+P` ofrece desbloquearlo, bloquearlo, cambiar la frase o borrarlo.

### Segundo plano

Mientras haya sesiones, túneles o transferencias, el servidor de MAGI toma el
*wake lock* de Termux (y lo suelta 30 s después de quedarse sin trabajo); se
desactiva con `[termux] wake_lock = "nunca"` en `config.toml`. Aun así, **Android
12 o posterior limita los procesos en segundo plano** y puede cerrar Termux
aunque tenga el wake lock: desde Termux no se puede quitar ese límite. `magi
diagnostico` (bloque TERMUX) revisa lo que se puede comprobar y dice qué ajustar
del móvil; la primera vez que se toma el wake lock en Android 12 o posterior,
MAGI avisa y remite a ese diagnóstico.
