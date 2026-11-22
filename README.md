# MAGI

[English](README.en.md) · **Castellano**

Gestor SSH de terminal, operado por teclado y con estética de cabina técnica:
inventario de hosts, pestañas que sobreviven a la ventana, SFTP, túneles,
snippets con deliberación, sincronización cifrada entre equipos, escritorios
remotos (RDP, VNC), servicios y contenedores, y sesiones persistentes con tmux.
Software libre bajo MIT OR Apache-2.0, sin cuenta, sin suscripción y sin
telemetría.

![MAGI en una terminal: la flota con tres hosts de ejemplo, Archivos, Servicios y la paleta de órdenes.](assets/demo/magi.gif)

*Animación grabada contra el servidor de pruebas del repositorio, con hosts
inventados (`assets/demo/`).*

## Estado

| Plataforma | Estado |
|---|---|
| Linux (Omarchy, Arch y otras) | Uso diario |
| macOS | En pruebas |
| App Android | Beta |
| Termux (MAGI dentro de Android) | En pruebas |

«En pruebas» y «beta» quiere decir que lo cubren las pruebas automáticas y la
integración continua, pero que no se ha probado a mano con el mismo detalle que
Linux (en macOS y en Termux, todavía no). Qué se ha probado y qué no, por
plataforma y por fase, está en [`docs/manual/estado.md`](docs/manual/estado.md).

## Instalación

| Plataforma | Cómo |
|---|---|
| Linux y macOS | tarball de [la última release](https://github.com/HectorEspejo/magi/releases/latest) |
| Arch y Omarchy (AUR), macOS (Homebrew) | todavía no publicados |
| Android | [Obtainium](https://github.com/ImranR98/Obtainium) con la URL de este repositorio y las etiquetas `android-v` |
| Termux | el `.deb` de la release (ver [Termux](docs/manual/termux.md)) |

Cada release publica un `.sha256` por fichero y atestaciones de procedencia de
GitHub. Comprueba ambos antes de instalar un tarball:

```sh
sha256sum -c magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify magi-ssh-x86_64-unknown-linux-gnu.tar.xz --repo HectorEspejo/magi
```

El APK de la app se comprueba además con `apksigner verify --print-certs`. Los
detalles, los requisitos, compilar desde el código, actualizar y desinstalar
están en [`docs/manual/instalacion.md`](docs/manual/instalacion.md).

## Primeros pasos

1. Ejecuta `magi diagnostico`: comprueba rutas, socket, llavero y
   `~/.ssh/config`, y cada aviso trae su instrucción de arreglo.
2. Arranca `magi` y, en la vista Hosts (`F2`), pulsa `I` para importar
   `~/.ssh/config`.
3. Conecta con `↵`. Cerrar la ventana no cierra las sesiones.
4. Explora con `Ctrl+P` (paleta) y `?` (ayuda).

La guía completa está en [`docs/manual/primeros-pasos.md`](docs/manual/primeros-pasos.md).

## Manual

El [manual](docs/manual/README.md) cuenta cada parte con detalle:

- [Primeros pasos y diagnóstico](docs/manual/primeros-pasos.md) y el
  [servidor de sesiones](docs/manual/servidor-de-sesiones.md)
- [Pestañas y terminal](docs/manual/pestanas-y-terminal.md) y
  [sesiones persistentes](docs/manual/sesiones-persistentes.md)
- [Archivos](docs/manual/archivos.md) y [túneles](docs/manual/tuneles.md)
- [Snippets y deliberación](docs/manual/snippets-y-deliberacion.md) y
  [flota, identidades y registro](docs/manual/flota-identidades-y-registro.md)
- [Sincronización del inventario](docs/manual/sincronizacion.md)
- [Escritorio remoto](docs/manual/escritorio-remoto.md) y
  [servicios, contenedores y logs](docs/manual/servicios.md)
- [App Android](docs/manual/app-android.md) y [Termux](docs/manual/termux.md)
- [Atajos](docs/manual/atajos.md), [`Include` de `~/.ssh/config`](docs/manual/ssh-config.md)
  y [seguridad](docs/manual/seguridad.md)

## Contribuir

Las contribuciones son bienvenidas: lee [`CONTRIBUTING.md`](CONTRIBUTING.md)
(también [en inglés](CONTRIBUTING.en.md)). Los cambios grandes empiezan por una
propuesta (plantilla «Propuesta» de las issues). El código, los comentarios y
los textos de la interfaz están en castellano; se aceptan issues y PR en inglés.
El historial de diseño de cada fase está en [`docs/`](docs/README.md).

## Seguridad

Para avisar de una vulnerabilidad, usa el aviso privado de GitHub (Security →
Report a vulnerability) y no una issue pública: [`SECURITY.md`](SECURITY.md).

## Licencia

MAGI es software libre bajo doble licencia **MIT OR Apache-2.0**, a elección de
quien lo use: [LICENSE-MIT](LICENSE-MIT) y [LICENSE-APACHE](LICENSE-APACHE).
Salvo que se indique lo contrario, cualquier contribución enviada al proyecto se
licencia del mismo modo, siguiendo la convención de Rust.
