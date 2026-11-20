# Contribuir a MAGI

[English](CONTRIBUTING.en.md) · **Castellano**

Gracias por interesarte en MAGI. Este documento cuenta cómo compilarlo, cómo
probarlo y cómo proponer cambios. Si solo quieres usarlo, empieza por el
[README](README.md) y el [manual](docs/manual/README.md).

## Antes de empezar

- **Un fallo:** abre una issue con la plantilla «Fallo». Pide la versión
  (`magi --version`), tu sistema y terminal, los pasos y la salida de
  `magi diagnostico`. No pegues IP, nombres de host, claves ni contraseñas
  reales.
- **Una idea o un cambio de comportamiento:** abre antes una issue con la
  plantilla «Propuesta», y espera respuesta antes de escribir código. Los
  cambios grandes se especifican como una fase del proyecto (el archivo de
  diseño está en [`docs/`](docs/README.md)) y no entran como una PR suelta.
- **Una vulnerabilidad:** no abras una issue pública. Usa el aviso privado de
  GitHub, como explica [`SECURITY.md`](SECURITY.md).
- Los cambios pequeños (un fallo evidente, una errata, una prueba que falta)
  pueden ir directamente en una PR.

## Compilar y probar

Hace falta Rust stable (MSRV 1.86). El paquete es `magi-ssh`, pero el comando
y la biblioteca se llaman `magi`. El repositorio es un workspace de Cargo:
`magi-ssh` (la raíz: TUI, servidor y CLI), `crates/magi-nucleo` (lo compartido
con la app) y `crates/magi-movil` (la puerta hacia Kotlin); la app Android
vive en `android/`.

Antes de abrir una PR, en verde:

```sh
TZ=UTC cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo deny check
```

Las pruebas de pintado necesitan `TZ=UTC`; las de SFTP, `sftp-server` (o la
variable `MAGI_SFTP_SERVER`); las de sesiones persistentes, `tmux` y el
`script` de util-linux. Si tocas workflows, pasa también `actionlint` y
`dist plan --tag vX.Y.Z`; si tocas scripts, `shellcheck`. Para pruebas aisladas,
fija `HOME` y `XDG_*`: MAGI no debe leer ni escribir tus ficheros reales. Las
pruebas nunca usan la red externa ni tu llavero.

## Desarrollo

```sh
cargo run                       # abre la TUI en la vista Flota
cargo run -- importar [ruta]    # importa ~/.ssh/config (o la ruta indicada)
cargo run -- exportar           # regenera ~/.ssh/magi_config
cargo run -- sondear [host…]    # sondea la flota sin TUI e imprime la tabla
cargo run -- registro exportar <ruta> [--json] [--desde AAAA-MM-DD]
cargo run -- diagnostico        # comprueba lo que MAGI necesita, sin escribir nada
cargo run -- autocompletado bash # escribe el autocompletado en la salida estándar
cargo run -- man                # escribe la página man en la salida estándar
cargo run -- servidor estado    # pid, versión, sesiones y clientes del servidor
cargo run -- servidor parar     # cierra las sesiones y apaga el servidor
cargo run -- tuneles            # lista los túneles definidos y activos
cargo run -- tunel activar <host> <nombre>   # levanta ese túnel
cargo run -- tunel parar <host> <nombre>     # lo para
cargo run -- conectar <host>    # abre la TUI con una sesión nueva a ese host
cargo run -- sincronizaciones   # lista las sincronizaciones guardadas
scripts/generar-recursos.sh     # regenera assets/ tras cambiar la CLI
TZ=UTC cargo test               # suite completa (las pruebas de pintado necesitan TZ=UTC)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Tests

```sh
TZ=UTC cargo test
```

Cubren el almacén (CRUD, cascadas, migración desde vacío), la ida y vuelta
`importar(exportar(hosts))` (salto, multiplexado, keepalive y opciones
extra), el protocolo (ida y vuelta de todos los mensajes, bytes en base64,
secretos ocultos en el depurado, mensajes desconocidos) y el servidor con
socket temporal (arranque, saludo versionado, apagado por inactividad, lock
duplicado y desconexión por mensaje mal formado).

El pool de conexiones (`tests/pool.rs`) comprueba que pestañas, SFTP y
túneles comparten una única conexión con `multiplexar`, que una pestaña no
tumba un túnel, el `Ejecutar` con plazo y el `SIGTERM` al binario real. Las
ejecuciones (`tests/ejecuciones.rs`) corren contra un servidor SSH en proceso
que atiende `exec`: varios hosts, código ≠ 0, timeout con el pool vivo, parar
al primer fallo, cancelación, tope de 1 MiB, anotaciones sin salida y
deliberaciones. La deliberación tiene pruebas de las tres comprobaciones
(CASPER-3 con `true`, `false` y `sleep 3` reales) y del diálogo.

El redimensionado (`tests/redimensionado.rs`) reproduce los dos fallos de la
Fase 7 contra el servidor SSH de pruebas, que registra cada `window-change`.
Prueba la tubería de tamaño con un `TestBackend` que cuenta las limpiezas y la
App real contra el servidor en proceso, con una y con dos ventanas. Además
compara instantáneas `insta` de todas las vistas y diálogos a 40×12, 80×24 y
200×60 (en `tests/snapshots/`), recorre una rejilla de tamaños sin pánico y
comprueba que las secuencias de cambio conservan el estado. Tras un cambio
visual deliberado, se regeneran con `INSTA_UPDATE=always cargo test --test
redimensionado` y se revisan con `git diff tests/snapshots` (o con `cargo insta
review`).

Las pruebas de archivos (`tests/sftp.rs`) levantan un servidor SSH en proceso
que sirve el subsistema `sftp` con el **`sftp-server` real de OpenSSH**: listar,
subir y bajar ficheros y directorios, conflicto con `omitir`, cancelación en
curso, `mtime` conservado, temporales en 600, borrado recursivo y la cola que
ve una segunda ventana. Desde la Fase 8 también sobrescribir un fichero
existente (posix-rename y los tres pasos), los temporales de edición, `StatRemoto`,
`CambiarPermisos` con sus alcances, `ListarArbol` por bloques con exclusiones y
`.magiignore`, los nombres de propietario, los permisos tras sobrescribir, el
borrado al terminar solo con éxito y las anotaciones de sincronización y
deliberación. Si el sistema no trae `sftp-server`, esas pruebas se saltan con
un aviso en lugar de fallar. El resto (marcas, avisos de sensibles,
paneles, orden y fechas) son pruebas unitarias puras.

Las pruebas de servicios no usan red externa: el servidor SSH de prueba con
`systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos, y
salidas grabadas de Debian 12, Ubuntu 24.04, Rocky 9 y Arch con Docker 27 y
Podman 5. Cubren el núcleo y el script (`servicios_lectura`,
`servicios_script`), la sincronización de las comprobaciones
(`sincronizacion_comprobaciones`), el servidor (`servicios_servidor`) y la
interfaz con sus instantáneas (`servicios_ui`, `redimensionado`).

Las de las sesiones persistentes tampoco usan red externa: el núcleo
(`sesiones_persistentes`, `tmux_script` con un tmux falso, `sincronizacion_sesiones`),
el servidor y su vigilante (`sesiones_persistentes_servidor`), la interfaz
(`sesiones_persistentes_ui`), la CLI (`sesiones_persistentes_cli`) y
`sesiones_persistentes_e2e`, que con **tmux de verdad** crea, engancha, suelta
desde otro cliente, atiende la campana y termina. `tmux_real` ejercita el script
de tmux contra el tmux del sistema. Las que necesitan tmux se saltan con un
aviso si no está (en CI se instala con `apt`).

## Convenciones

- **Idioma.** El código, los comentarios, los mensajes de commit, los textos de
  la interfaz y la documentación van en castellano; solo se mantienen en
  inglés los términos técnicos estándar (API, CRUD, endpoint, commit…). La
  puerta de entrada del repositorio va también en inglés y nada más:
  `README.en.md`, `CONTRIBUTING.en.md`, el resumen en inglés de cada versión del
  CHANGELOG, el párrafo en inglés de `SECURITY.md` y el inglés de las
  plantillas de `.github/`. Se aceptan issues y PR en inglés.
- **Nombres.** Identificadores de Rust en castellano (`snake_case`; tipos en
  `CamelCase`); tablas SQL en `MAYUSCULAS_SNAKE_CASE` y campos en
  `minusculas_snake_case`.
- **Documentación.** Todo cambio visible para el usuario añade su línea en la
  sección «Sin publicar» de `CHANGELOG.md` y actualiza el capítulo que toque de
  [`docs/manual/`](docs/manual/README.md). `README.md` y `README.en.md` dicen
  lo mismo, y este fichero y `CONTRIBUTING.en.md` también.
- **Datos de ejemplo.** Nada real en el repositorio: direcciones de
  documentación (`192.0.2.x`), el dominio `ejemplo.org`, el usuario `usuario` y
  `/home/usuario`. La prueba `repositorio_publico` lo comprueba en cada PR.
- **Dependencias fijadas.** `russh`, `ratatui`, `crossterm`, `tui-term`,
  `vt100`, `rusqlite`, IronRDP, `age` y UniFFI van fijadas y son compatibles
  entre sí: no se suben por separado, y no se aceptan cambios de una sola de
  ellas.
- **Sin telemetría.** MAGI no abre conexiones salientes que no sean a los
  hosts del usuario ni comprueba actualizaciones; no se aceptan cambios que lo
  hagan.

## Commits y pull requests

- Mensajes de commit en castellano, en imperativo y concisos.
- Una PR por cambio, con su prueba. Usa la plantilla: pruebas, línea en «Sin
  publicar», manual al día y sin datos reales.
- La CI tiene que estar en verde: los jobs de `ci.yml` (`formato`, `clippy`,
  `tests-linux`, `macos`, `visor`, `deny` y `aur-plantilla`) son obligatorios
  para fusionar en `main`. La revisión la hace el mantenedor.
- Las versiones y las releases las prepara el mantenedor; no hace falta que
  toques `version` ni etiquetes nada.

## Licencia de lo que aportas

No hay acuerdo de cesión de derechos. Lo que contribuyes entra bajo la misma
licencia doble del proyecto, **MIT OR Apache-2.0**, como indica el
[README](README.md#licencia).

## Agentes de código

El proyecto se desarrolla con agentes de código. [`CLAUDE.md`](CLAUDE.md) y
[`AGENTS.md`](AGENTS.md) (el mismo contenido) recogen las reglas de trabajo y las
decisiones de diseño que importan al tocar el código: léelos si vas a cambiar
algo más que una errata. Si usas un agente, esas reglas se aplican igual a lo
que produce, y tú respondes de la PR.
