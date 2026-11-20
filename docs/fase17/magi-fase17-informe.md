# MAGI - Fase 17: MAGI en Termux

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 17 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Base: Fases 1-16 con informe procesado. La F16 (65/67) tiene pendiente su prompt de cierre (el script de tmux por la entrada estándar, D357, la carrera de la reconexión y los retoques de su §11): esta fase parte de la F16 cerrada y fusionada. La F15 sigue a 95/96 y el prompt de cierre de la F14, sin informe; nada de eso bloquea esta fase.
>
> Estado (17 nov 2026): especificada; a la espera del informe de implementación.

---

## 1. Visión General

La Fase 17 lleva **el MAGI de escritorio entero a Android, dentro de Termux**. La app Android (F11, F14) es una compañera de guardia: lee el inventario, abre terminales y túneles, y enseña Flota, servicios y snippets, pero no edita nada y no tiene sesiones persistentes, SFTP, escritorios remotos ni la vista completa del terminal. En Termux, en cambio, corre el mismo binario que en Linux: TUI, servidor de sesiones y CLI, con todas las vistas.

Lo que cambia es lo que Linux da hecho y Android no: no hay llavero del sistema, el portapapeles y el navegador se piden a Termux, Android duerme y cierra procesos en segundo plano, el teclado del móvil no tiene teclas de función y la carpeta de sincronización vive en el almacenamiento compartido. Esta fase resuelve cada una de esas piezas y empaqueta MAGI para instalarlo con `apt`.

**Objetivos principales**

1. El mismo binario de escritorio compilado para `aarch64-linux-android`, con lo propio de Termux aislado en `src/termux/` y probado en Linux con dobles de las órdenes de Termux.
2. Instalación sencilla: un `.deb` en cada release de GitHub, hecho por CI, y una receta para el repositorio comunitario TUR.
3. Un llavero en fichero cifrado con una frase maestra, para guardar contraseñas y la clave de sincronización donde no hay secret-tool.
4. El móvil como un equipo más de la sincronización, que también escribe, con la carpeta de Syncthing en el almacenamiento compartido.
5. Lo del sistema, con Termux: portapapeles, enlaces y escritorios en el navegador del móvil, wake lock mientras haya trabajo y avisos de Android cuando una sesión pide atención.
6. Una fila de teclas de Termux pensada para MAGI, y `magi diagnostico` con lo que hay que saber de Termux y Android. Sin cambios de protocolo, migración ni `formato`; versión 1.6.0.

**Contexto.** Producto propio. Hector quiere tener MAGI completo en el móvil (el plegable), con sus sesiones persistentes, su Flota y sus escritorios, sin esperar a que la app Android llegue a todo. Termux ya trae un Linux de usuario con `apt`, OpenSSH y un terminal que entiende lo que MAGI usa (colores, ratón, pegado entre corchetes, OSC 52), así que el camino corto es compilar el binario de escritorio para Android y resolver las diferencias de la plataforma.

Respuestas de Hector (17 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | El MAGI de escritorio completo (TUI, servidor y CLI): el mismo binario compilado para Android, con lo propio de Termux aislado (a) |
| 2 | Un `.deb` en cada release de GitHub, hecho por CI, y la receta para TUR, el repositorio comunitario de Termux (a) |
| 3 | Solo aarch64, Android 7 o posterior y el Termux de F-Droid o de GitHub; el de Google Play no se soporta (a) |
| 4 | Un llavero en un fichero cifrado con una frase maestra, que se pide al abrir MAGI (a) |
| 5 | El móvil sincroniza como un equipo escritor más, con la carpeta en el almacenamiento compartido (la de Syncthing) (a) |
| 6 | Portapapeles con Termux:API si está y, si no, OSC 52; enlaces con `termux-open-url` (a) |
| 7 | Los escritorios remotos, en el navegador del móvil, con el mismo visor (a) |
| 8 | Wake lock automático mientras haya trabajo, con opción de apagarlo (a) |
| 9 | Avisos de Android cuando una sesión pide atención, con Termux:API (a) |
| 10 | MAGI propone su fila de teclas y la instala en `termux.properties`, con confirmación y copia (a) |
| 11 | Pruebas en CI (en Linux, con dobles de las órdenes de Termux) y un guion a mano en el plegable (a) |

**Fuera de alcance**

- Cambiar la app Android: sigue siendo lectora y no se toca. Termux es otro camino para tener MAGI en el móvil, no la sustituye.
- El Termux de Google Play (obsoleto y distinto), ARM de 32 bits y x86_64 publicados (x86_64 solo como artefacto de CI) y Android anterior a 7.
- Entrar en el repositorio oficial de Termux (termux-packages): solo TUR.
- Desbloquear el llavero con la huella: `termux-fingerprint` solo dice sí o no y no protege el fichero, y `termux-keystore` no cifra en las versiones publicadas.
- Abrir otra ventana de MAGI con `prefijo w`: Termux no tiene una orden para abrir sesiones; MAGI explica cómo hacerlo a mano.
- Leer el portapapeles de Android (como en el escritorio, nunca).
- Usar el llavero de fichero en Linux sin secret-tool (servidores sin escritorio): la pieza sirve, pero no se activa en esta fase.
- Termux:X11, Termux:Boot (arrancar al encender el móvil) y Termux:Widget.
- Avisos con el título, el texto del aviso o cualquier salida del remoto (T140).

**Versión prevista:** escritorio **1.6.0** (MINOR: plataforma nueva y funcionalidad nueva; sin migración —`user_version` sigue en 11—, protocolo v10 y `formato` 4). La app Android no cambia.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Móvil | Android 7 o posterior (API 24), aarch64; Termux de F-Droid o GitHub | Con el de Google Play, `magi diagnostico` avisa y no se soporta |
| Compilación | Rust stable, objetivo `aarch64-linux-android`; NDK r30 con `cargo-ndk`, que el repositorio ya usa para la app | `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh`; el binario solo enlaza con bionic |
| Plataforma | `src/termux/` (nuevo) | Rutas, órdenes de Termux, portapapeles, wake lock, avisos, fila de teclas y diagnóstico |
| Llavero | `LlaveroFichero` en `magi-nucleo` (nuevo), con `age` (scrypt) | `llavero.age` detrás de la interfaz `LlaveroSecreto` de siempre |
| Termux | termux-tools (`termux-open-url`, `termux-wake-lock`, `termux-wake-unlock`, `termux-reload-settings`); Termux:API, app y paquete `termux-api`, opcionales | `termux-clipboard-set`, `termux-notification` y `termux-notification-remove` |
| Empaquetado | `.deb` con `dpkg-deb -Zxz`; receta de TUR (`build.sh`) | `scripts/paquete-termux.sh` y `packaging/termux/` |
| CI | `termux-release.yml` (nuevo) y job `termux` en `ci.yml` | Solo `GITHUB_TOKEN`; `.sha256` y atestación de procedencia |
| Dependencias | Ninguna nueva | `age`, `nix` y `unicode-normalization` ya están; `libc`, ya en el árbol, pasa a directa solo para Android |

### Estructura de carpetas (novedades)

```
src/termux/                    (nuevo) solo se compila para Android y en las pruebas
├── mod.rs           es_termux() · versiones de Termux y de Android · primer arranque
├── rutas.rs         datos, configuración, estado y socket desde $HOME, XDG_* y $TMPDIR
├── ordenes.rs       termux-* sin shell y con plazo; estado de Termux:API por proceso
├── portapapeles.rs  termux-clipboard-set → OSC 52 → aviso
├── wake_lock.rs     tomar y soltar según TrabajoServidor (lo usa el servidor)
├── avisos.rs        publicar y retirar avisos de Android; ids guardados en el estado
├── teclas.rs        fila extra-keys y fusión de termux.properties
├── diagnostico.rs   bloque TERMUX de magi diagnostico
└── cli.rs           magi termux teclas · magi termux aviso
crates/magi-nucleo/src/llavero_fichero.rs   LlaveroFichero: age con frase, flock, NFC y topes
src/llavero.rs                 (F2) elige secret-tool, Keychain o, en Termux, LlaveroFichero
src/app/llavero.rs             (nuevo) diálogos de la frase y estado del llavero en el cliente
src/llavero_cli.rs             (nuevo) magi llavero estado · cambiar · borrar
packaging/termux/build.sh      receta para TUR
scripts/paquete-termux.sh      arma magi-ssh_<versión>_aarch64.deb
.github/workflows/termux-release.yml   el .deb en cada release; ci.yml gana el job termux
docs/publicar-termux.md        cómo sale el .deb y cómo se propone la receta a TUR
tests/fijos/termux/            dobles de termux-* (anotan argumentos y entrada; saben colgarse o fallar)
tests/termux_*.rs · crates/magi-nucleo/tests/llavero_fichero.rs
```

### Diagrama de arquitectura

```
 MÓVIL ANDROID · Termux de F-Droid o GitHub (aarch64, Android 7+)                            HOSTS SSH

 ┌──────────────────────────────────────┐      ┌───────────────────────────────────────┐      ┌──────────────────────┐
 │ magi (cliente, en Termux)            │      │ magi --servidor (setsid)              │      │ web-01 · dev-01 …    │
 │ vistas F1-F9 · paleta · ayuda        │      │ pestañas, pool, túneles y SFTP        │      │ tmux -L magi (F16)   │
 │ llavero de fichero:                  │ v10  │ sesiones persistentes y vigilante     │ SSH  │ systemd y docker     │
 │   frase maestra, solo en memoria     │◀────▶│   (en Termux, también sin ventanas    │◀────▶│ escritorios: RDP,    │
 │ portapapeles y enlaces               │      │   si hay avisos y trabajo)            │      │   VNC y voxl         │
 │ diálogos (huella, frase, contraseña) │      │ avisos de Android por sesión          │      │                      │
 │ sincronización (escritor)            │      │ wake lock mientras haya trabajo       │      │ nada cambia aquí     │
 │ magi llavero · magi termux teclas    │      │ visor de escritorios en 127.0.0.1     │      │                      │
 └──────────────────────────────────────┘      └───────────────────────────────────────┘      └──────────────────────┘
     │ termux-clipboard-set · termux-open-url      │ termux-notification · termux-wake-lock
     ▼                                             ▼
 ┌─────────────────────────────────────────────────────────────────────────────────────┐
 │ Termux · termux-tools: termux-open-url, termux-wake-lock, termux-reload-settings    │
 │          Termux:API (app y paquete, opcional): portapapeles y notificaciones        │
 │          ~/.termux/termux.properties: la fila de teclas extra de MAGI               │
 │          órdenes con plazo y sin shell; si Termux:API no responde, se apaga         │
 └─────────────────────────────────────────────────────────────────────────────────────┘
     ▼
 Android: portapapeles · notificaciones · navegador (visor de escritorios) · límite de procesos (12+)

 ~/.local/share/magi/: magi.db · llavero.age (age con frase maestra, 600; no viaja)
 ~/storage/shared/… (la carpeta de Syncthing) ◀── magi-<uuid>.age ──▶ tus escritorios y la app Android

 El mismo binario de escritorio, compilado para aarch64-linux-android; lo propio de Termux vive en src/termux/.
```

**Nada nuevo entre procesos.** El cliente y el servidor son los de siempre y hablan el protocolo v10 por su socket Unix, que en Termux vive en `$TMPDIR`. El cliente custodia el llavero desbloqueado y el portapapeles; el servidor, como hasta ahora, nunca toca el llavero (pide las contraseñas al cliente) y es quien toma el wake lock y publica los avisos, porque es el que sigue vivo con la ventana cerrada. Todo lo que se pide a Android pasa por una orden de Termux, siempre sin shell y con plazo: si Termux:API no está o no responde, esa función se apaga y MAGI sigue.

---

## 3. Modelo de Datos

Sin tablas nuevas ni migración (`user_version` sigue en 11) y sin `formato` nuevo de sincronización. Aparecen un fichero nuevo, el llavero de Termux, una sección de configuración y seis tipos de REGISTRO.

### Diagrama E-R (cambios de la fase)

```
 ┌────────────────────────────────────────┐           ┌──────────────────────────────────────────┐
 │ EQUIPOS (F10)            sin migración │           │ llavero.age (fichero nuevo)              │
 │ fila de este equipo:                   │           │ ~/.local/share/magi/ · 600               │
 │   plataforma android (como la app)     │           │ age con frase maestra (scrypt)           │
 │   nombre y etiqueta termux-<modelo>    │           │ entradas por referencia, como el         │
 │ escribe su fichero magi-<uuid>.age     │           │   llavero del escritorio:                │
 │ (es un escritor más)                   │           │   sincronizacion (clave age)             │
 └────────────────────────────────────────┘           │   contraseñas de los hosts               │
                                                      │ propio del equipo: no viaja              │
 ┌────────────────────────────────────────┐ referencia│ lo abre solo el cliente                  │
 │ HOSTS (F1)                             │── N:1 ───▶│ dañado: se aparta con fecha              │
 │ identidad_ref = contrasena:llavero     │           └──────────────────────────────────────────┘
 │   (la contraseña, en llavero.age;      │
 │   en magi.db solo la referencia)       │           ┌──────────────────────────────────────────┐
 └────────────────────────────────────────┘           │ config.toml [termux] (sección nueva)     │
                                                      │ wake_lock · avisos · api_plazo_seg       │
                                                      │ sincronizacion_sondeo_seg                │
 ┌────────────────────────────────────────┐           │ fuera de Termux se ignora                │
 │ REGISTRO (F2), tipos nuevos            │           └──────────────────────────────────────────┘
 │ llavero_creado                         │
 │ llavero_frase_cambiada                 │           ┌──────────────────────────────────────────┐
 │ llavero_borrado                        │           │ ~/.termux/termux.properties (Termux)     │
 │ llavero_desbloqueo_fallido             │           │ solo la línea extra-keys de MAGI         │
 │ llavero_apartado                       │           │ copia termux.properties.magi-<fecha>     │
 │ termux_teclas_instaladas               │           │ escribe: magi termux teclas              │
 │ (sin frases ni secretos)               │           └──────────────────────────────────────────┘
 └────────────────────────────────────────┘
```

### 3.1 `llavero.age` (fichero nuevo, no viaja)

Fichero `age` (versión 1) con una sola entrada de tipo scrypt, en el directorio de datos (`~/.local/share/magi/llavero.age`, permisos 600, escrito por `ficheros.rs`). Dentro, un JSON:

| Campo | Tipo | Descripción |
|---|---|---|
| `formato` | entero | `1` |
| `entradas` | objeto | Clave → secreto (texto). Las mismas claves que usa el llavero del escritorio por `LlaveroSecreto`: las contraseñas de los hosts con `contrasena:llavero` y la identidad de sincronización (actual y anterior) |

- **Factor de trabajo:** scrypt con 2^17 al crear o recifrar (unos 128 MiB y alrededor de un segundo en un móvil actual); al abrir se aceptan hasta 2^20, para que un fichero manipulado no deje MAGI calculando minutos.
- **Tope:** 1 MiB descifrado; lo que pase de ahí se trata como dañado.
- **Cerrojo:** `llavero.age.lock` con `flock` en el mismo directorio: quien escribe relee el fichero bajo el cerrojo, aplica su cambio y escribe de forma atómica.
- **Dañado:** si la cabecera no se puede leer o el contenido no se descifra bien con la frase correcta, se aparta como `llavero.age.danado-<fecha>` (nunca se borra solo).

### 3.2 EQUIPOS (sin migración)

La fila de este equipo en Termux:

| Campo | Valor en Termux |
|---|---|
| `nombre` | Por defecto, `termux-<modelo>` (el modelo de Android, saneado como la etiqueta de la F15); editable como en el escritorio |
| `plataforma` | `android`, el valor que el núcleo ya conoce por la app; nada en el núcleo decide por él |
| `app_version` | La versión de MAGI (`1.6.0`) |

El nombre del sistema en Termux es `localhost`, que no sirve: `termux-<modelo>` es también la etiqueta de las sesiones persistentes (F16) y el usuario de las deliberaciones.

### 3.3 REGISTRO: tipos nuevos

| Tipo | Cuándo | Detalle |
|---|---|---|
| `llavero_creado` | Se escribe por primera vez `llavero.age` | — |
| `llavero_frase_cambiada` | El fichero recifrado con la frase nueva ya está escrito | — |
| `llavero_borrado` | El fichero ya no existe | — |
| `llavero_desbloqueo_fallido` | Al quinto fallo seguido de una serie | Número de intentos |
| `llavero_apartado` | El fichero dañado ya está renombrado | Nombre de la copia |
| `termux_teclas_instaladas` | `termux.properties` ya está escrito | Nombre de la copia, o «no existía» |

Ninguno guarda frases, secretos ni el contenido del llavero.

### 3.4 Fuera de `magi.db`

- **`config.toml`**, sección `[termux]` (§5.3). Fuera de Termux se ignora.
- **`~/.termux/termux.properties`** (o `~/.config/termux/termux.properties`): MAGI solo toca la línea `extra-keys`, a petición, con copia `termux.properties.magi-<fecha>` (§4.8).
- **`~/.local/state/magi/termux/`**: `primer-arranque` (marca), `segundo-plano` (marca del aviso de Android 12+), `wake-lock` (MAGI lo tiene tomado) y `avisos` (ids de los avisos publicados, para retirarlos si el servidor muere).

### 3.5 Relaciones

- HOSTS N:1 `llavero.age`: un host con `identidad_ref = contrasena:llavero` tiene su contraseña en el llavero; en `magi.db`, solo la referencia (T19).
- El llavero es de cada equipo y no viaja; tampoco su frase. La clave de sincronización de este equipo vive en él.
- EQUIPOS: el de Termux escribe su `magi-<uuid>.age` como cualquier escritorio.

### 3.6 Diagramas de estados

**Llavero de fichero (en cada ventana de MAGI)**

```
  (no existe) ─ primer secreto que guardar (recordar una contraseña, crear o unir la sincronización)
              ─▶ [CREAR LA FRASE MAESTRA] ─ ↵ ─▶ abierto · REGISTRO llavero_creado
  (no existe) ─ Esc al crear ─▶ (no existe): la contraseña se usa sin guardar; la sincronización no se crea
  cerrado ─ frase correcta ─▶ abierto (frase y entradas en la memoria del cliente, Zeroizing)
  cerrado ─ frase incorrecta ─▶ cerrado · al 5.º fallo seguido: espera de 30 s y REGISTRO llavero_desbloqueo_fallido
  cerrado ─ Esc ─▶ cerrado («llavero bloqueado» en la barra cuando algo lo necesita)
  abierto ─ «Bloquear el llavero» · se cierra la ventana ─▶ cerrado (memoria borrada)
  abierto ─ cambiar la frase (actual y nueva dos veces) ─▶ abierto · recifrado · REGISTRO llavero_frase_cambiada
  cerrado · abierto ─ borrar (escribiendo «borrar») ─▶ (no existe) · REGISTRO llavero_borrado
  cerrado ─ cabecera o contenido ilegibles ─▶ dañado ─ apartar ─▶ (no existe) · llavero.age.danado-<fecha> ·
                                                       REGISTRO llavero_apartado
                                             └─ salir ─▶ cerrado (MAGI se cierra sin tocar nada)
  Inválidas: guardar un secreto con el llavero cerrado · pedir la frase en una operación automática ·
             escribir la frase en disco, en el log, en REGISTRO o mandarla al servidor · borrar un llavero dañado
```

**Wake lock (en el servidor)**

```
  suelto ─ TrabajoServidor deja de estar vacío (wake_lock = auto) ─▶ tomando ─ termux-wake-lock responde ─▶ tomado
  tomando ─ la orden falla o no responde en el plazo ─▶ apagado (hasta que el servidor reinicie; aviso una vez)
  tomado ─ TrabajoServidor vacío ─▶ esperando ─ vuelve el trabajo antes de 30 s ─▶ tomado
                                             └─ pasan 30 s ─▶ termux-wake-unlock ─▶ suelto
  tomado · esperando ─ el servidor termina (normal o SIGTERM) ─▶ termux-wake-unlock ─▶ suelto
  tomado ─ el servidor muere (SIGKILL) ─▶ sigue tomado en Termux ─ el siguiente servidor, sin trabajo ─▶ suelto
                                          (lo sabe por la marca wake-lock del estado)
  wake_lock = nunca ─▶ suelto siempre
  Inválidas: soltar un wake lock que MAGI no tomó · tomarlo fuera de Termux · tomarlo sin trabajo
```

**Aviso de Android (uno por sesión persistente o por pestaña)**

```
  sin_aviso ─ atención no → sí sin nadie mirando ─▶ publicado (termux-notification --id magi-<id>)
             (sesión persistente: campana de tmux, vigilante F16 · pestaña: BEL, OSC 9 u OSC 777, marca ! F15)
  publicado ─ otra atención de la misma sesión ─▶ publicado (mismo id: se sustituye sin volver a sonar)
  publicado ─ alguien la mira · escribes en la pestaña · se cierra la pestaña · se borra la ficha
            ─▶ retirado (termux-notification-remove magi-<id>) ─▶ sin_aviso
  publicado ─ el servidor termina ─▶ retirado (todos los suyos)
  publicado ─ el servidor muere (SIGKILL) ─▶ huérfano ─ arranca el siguiente servidor ─▶ retirado (ids del estado)
  Inválidas: publicar con avisos = false, fuera de Termux o con Termux:API apagada · poner en el aviso el título,
             el texto del aviso o la salida · dos avisos a la vez para la misma sesión
```

**Termux:API (en cada proceso: cliente y servidor)**

```
  sin_comprobar ─ primera orden que la usa responde en api_plazo_seg ─▶ disponible
  sin_comprobar ─ la orden no está en el PATH (falta el paquete termux-api) ─▶ ausente
  sin_comprobar · disponible ─ sin respuesta en el plazo o error ─▶ no_responde (falta la app o está parada)
  ausente · no_responde ─▶ apagada hasta reiniciar ese proceso: portapapeles por OSC 52 y sin avisos de Android
                           (la barra lo dice una vez)
  Inválidas: reintentar en bucle · esperar sin plazo · pasar a disponible sin una respuesta
```

**Fila de teclas de Termux (lo que dice `termux.properties`)**

```
  sin_fila · otra_fila ─ ↵ instalar (paleta o primer arranque) · magi termux teclas --instalar y «s»
      ─▶ copia termux.properties.magi-<fecha> (si existía) ─▶ escrita ─ termux-reload-settings ─▶ de_magi
  escrita ─ termux-reload-settings no responde ─▶ de_magi («reinicia Termux para verla»)
  de_magi ─ instalar otra vez ─▶ de_magi («ya está»: ni copia ni escritura)
  cualquiera ─ Esc · «n» ─▶ sin cambios
  Inválidas: escribir sin confirmación · tocar otra línea del fichero · escribir sin copia previa
```

---
## 4. Flujos de Trabajo

### 4.1 Instalar y primer arranque

```
  Termux de F-Droid o GitHub · pkg install curl (si falta)
      ▼ curl -LO …/releases/download/v1.6.0/magi-ssh_1.6.0_aarch64.deb (y su .sha256) · sha256sum -c
  apt install ./magi-ssh_1.6.0_aarch64.deb ─ otra arquitectura (arm, i686, x86_64) ─▶ dpkg lo rechaza
      │   (con TUR: pkg install tur-repo · pkg install magi-ssh)
      ▼
  magi ─ no arranca (biblioteca o API de Android) ─▶ error del enlazador · magi --version e incidencia (R167)
      ▼ modo Termux (compilado para Android) · rutas de $HOME, XDG_* y $TMPDIR · socket en $TMPDIR/magi-<uid>/
  ¿primera vez en Termux? ─ sí ─▶ [FILA DE TECLAS DE TERMUX] una sola vez (§4.8)
      │                            barra: «MAGI en Termux: mira magi diagnostico»
      ▼ ¿hay llavero.age? ─ sí ─▶ [LLAVERO DE MAGI] (§4.2)
  Flota (F1), en modo estrecho si la pantalla lo pide
```

1. El `.deb` no tiene dependencias obligatorias: sugiere `termux-api` y `openssh` (el agente SSH y `ssh-add`).
2. Termux de Google Play arranca MAGI igual, pero `magi diagnostico` avisa de que no se soporta (R171).
3. Lo que no cambia respecto al escritorio no se repite aquí: inventario, Flota, sesiones, túneles, SFTP, servicios, snippets y sesiones persistentes funcionan igual.

### 4.2 Llavero: crear, desbloquear y usar

```
  abrir MAGI con llavero.age ─▶ [LLAVERO DE MAGI]
      ├─ frase correcta ─▶ abierto: entradas en la memoria del cliente (Zeroizing)
      ├─ incorrecta ─▶ «frase incorrecta» · al 5.º fallo seguido: 30 s de espera y REGISTRO
      ├─ fichero dañado ─▶ [LLAVERO DAÑADO] ─ apartar ─▶ llavero.age.danado-<fecha> · se sigue sin llavero
      └─ Esc ─▶ cerrado
  conectar a un host con contrasena:llavero (interactivo)
      ▼ el servidor pide la contraseña al cliente (Solicitante, como en el escritorio)
  cliente ─ abierto y con entrada ─▶ Contrasena (Zeroizing) sin preguntar
          ─ abierto y sin entrada ─▶ diálogo de contraseña con «recordar» (F2) ─ recordar ─▶ al llavero
          ─ cerrado ─▶ [LLAVERO DE MAGI] ─ Esc ─▶ diálogo de contraseña sin «recordar»
          ─ no existe y «recordar» ─▶ [CREAR LA FRASE MAESTRA] ─ Esc ─▶ se usa sin guardar
  operación automática (reconexión, vigilante, túnel automático) con el llavero cerrado
      ─▶ error con instrucción: «llavero bloqueado: desbloquéalo (Ctrl+P)»; nunca un diálogo
  CLI que lo necesita (magi sincronizar…) ─ con TTY ─▶ pide la frase en el TTY ─ sin TTY ─▶ error con instrucción
```

1. Cada ventana de MAGI desbloquea por su cuenta: la frase no se comparte entre procesos ni pasa por el servidor.
2. Crear pide la frase dos veces, con al menos 12 caracteres, y dice que no se recupera (§6.2).
3. Cambiar la frase recifra el fichero con escritura atómica; borrar el llavero explica qué se pierde (las contraseñas guardadas y la clave de sincronización de este equipo) y pide escribir `borrar`.
4. Después de borrarlo, los hosts con `contrasena:llavero` vuelven a pedir la contraseña con «recordar» (F2), y la sincronización pide unir de nuevo este equipo.

### 4.3 Unir este equipo a la sincronización

```
  Sincronización (F10) · «Unir este equipo» ─ sin ~/storage ─▶ «ejecuta termux-setup-storage y vuelve»
      ▼ carpeta: ~/storage/shared/Sync/magi ─▶ realpath ─▶ /storage/emulated/0/Sync/magi (se guarda así)
      ├─ no existe o no se puede escribir ─▶ error en el campo
      ▼ pegar la clave (pegado entre corchetes de Termux) ─ no válida ─▶ error en el campo (F10)
  ¿llavero? ─ no existe ─▶ [CREAR LA FRASE MAESTRA] ─ Esc ─▶ «sin llavero no se puede guardar la clave» · cancelado
            ─ cerrado ─▶ [LLAVERO DE MAGI] ─ Esc ─▶ cancelado
      ▼ clave al llavero · equipo termux-<modelo>, plataforma android · unión por nombre (D135)
  pasada (F10): lee los magi-*.age · escribe magi-<uuid>.age (un chmod 600 que falla no es error)
  después: cada sincronizacion_sondeo_seg (30 s) con MAGI abierto, y al volver a la vista Sincronización
```

1. La carpeta la sincroniza otra app (Syncthing para Android o la que use el usuario) con sus escritorios; MAGI solo lee y escribe ficheros en ella, como en la F10.
2. En el almacenamiento compartido no se usa `notify`: Android no garantiza los avisos de cambios que hace otra app, así que se sondea.
3. La app Android, si está instalada en el mismo móvil, sigue siendo lectora y ve los cambios que haga Termux como los de cualquier escritorio.

### 4.4 Copiar al portapapeles

```
  modo historial (F15) · copiar la selección ─▶ texto (≤ 8 MiB, como en la F15)
  ¿Termux:API apagada? ─ no ─▶ termux-clipboard-set por la entrada estándar (≤ 512 KiB, con plazo)
      ├─ responde ─▶ «copiado (3,2 KiB)»
      ├─ no responde o falla ─▶ Termux:API apagada en este proceso (aviso una vez) ─▶ OSC 52
      └─ más de 512 KiB ─▶ «demasiado para el portapapeles (900 KiB): guárdalo en un fichero»
  OSC 52 ─ base64 ≤ 6 000 bytes ─▶ a Termux ─▶ «copiado»
         ─ más ─▶ «no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo»
```

El OSC 52 que manda un host (F15, solo con `portapapeles_remoto`) sigue el mismo camino. MAGI nunca lee el portapapeles: pegar es cosa de Termux (pulsación larga → Pegar), que llega como pegado entre corchetes.

### 4.5 Enlaces y escritorios en el navegador del móvil

```
  enlace (modo enlaces, F15) ─▶ [terminal] abrir_enlaces = termux-open-url <url> (sin shell, con plazo)
      ├─ localhost del host ─▶ túnel temporal (F15) ─▶ http://127.0.0.1:<puerto> en el navegador del móvil
      └─ falla ─▶ «no se pudo abrir el enlace» con la salida de la orden
  abrir un escritorio (F12) ─▶ el servidor sirve el visor en 127.0.0.1 con un enlace de un solo uso (T98)
      ▼ [escritorio] navegador = auto ─▶ termux-open-url <enlace> ─▶ navegador de Android
  con el navegador delante, Termux queda en segundo plano ─▶ wake lock tomado: hay trabajo (§4.6)
      └─ Android cierra los procesos de Termux ─▶ el visor pierde la conexión (R166, magi diagnostico)
```

El navegador del móvil tiene que hablar WebCodecs con H.264 (Chrome 94 o posterior). El teclado en pantalla dentro del visor depende del navegador: el guion lo comprueba (R173).

### 4.6 Wake lock

```
  servidor: TrabajoServidor pasa de vacío a no vacío (sesión, túnel, transferencia, escritorio)
      ▼ wake_lock = nunca ─▶ nada
  termux-wake-lock (con plazo) ─ falla ─▶ apagado en este servidor · «sin wake lock: Android puede dormir MAGI»
      ▼ tomado · marca wake-lock en el estado · Termux lo enseña en su notificación
  ¿Android 12 o posterior y primera vez? ─ sí ─▶ aviso único: «Android puede cerrar procesos de Termux:
      │                                           mira magi diagnostico»
      ▼
  TrabajoServidor vacío ─▶ 30 s ─ vuelve el trabajo ─▶ sigue tomado
                               └─ no ─▶ termux-wake-unlock · se quita la marca
  el servidor arranca con la marca puesta (el anterior murió) ─▶ en cuanto no tiene trabajo: termux-wake-unlock
```

El wake lock de Termux es uno para toda la app: si el usuario lo había tomado a mano, MAGI lo soltaría con el suyo. Quien lo gestione a mano pone `wake_lock = "nunca"` (R175).

### 4.7 Avisos de Android

```
  servidor en Termux con [termux] avisos = true (vive mientras haya trabajo, aunque no haya ventanas)
      ├─ vigilante (F16), cada vigilar_seg, también sin ventanas ─▶ una sesión persistente pasa a «pide atención»
      └─ pestaña (F15): BEL, OSC 9 u OSC 777 sin nadie mirando (ninguna ventana la tiene delante y con foco)
  ¿Termux:API apagada en el servidor? ─ sí ─▶ solo la barra (F15, F16)
      ▼ termux-notification --id magi-<id> --group magi --alert-once
                            -t «◆ agente-api pide atención» -c «dev-01 · MAGI» --action <fija>
      ├─ no responde ─▶ Termux:API apagada en el servidor · sin más avisos de Android
      └─ publicado ─▶ id anotado en ~/.local/state/magi/termux/avisos
  se quita la atención (alguien la mira, escribes en la pestaña) · se cierra la pestaña · se borra la ficha
      ─▶ termux-notification-remove magi-<id> · id fuera del estado
  el servidor termina ─▶ retira los suyos · el servidor arranca con ids en el estado ─▶ los retira
```

1. **Foco.** «Sin nadie mirando» usa el foco agregado de la F15, que llega de los eventos de foco del terminal (`EnableFocusChange`): con Termux en segundo plano, la pestaña que está delante cuenta como sin mirar. Si la prueba de concepto del primer sprint ve que Termux no manda esos eventos al cambiar de app, las pestañas avisan aunque estén delante y el aviso se retira en cuanto escribes en ellas (D379).
2. **Ids.** `magi-<id>`, con el `uuid` corto de la ficha (sesiones persistentes) o el id de la pestaña: nada del inventario ni del remoto.
3. **Tocar el aviso** trae Termux delante, donde estaba MAGI; no cambia de vista ni engancha nada.
4. Sin Termux:API, o con `avisos = false`, todo sigue como en el escritorio: la barra avisa mientras haya una ventana abierta.

### 4.8 Fila de teclas de Termux

```
  primer arranque en Termux (una vez) · paleta «Instalar la fila de teclas de Termux» · magi termux teclas --instalar
      ▼ fila propuesta: de la tabla de teclas de MAGI y del prefijo configurado
  fichero: ~/.termux/termux.properties o, si es el que existe, ~/.config/termux/termux.properties
      ├─ su extra-keys ya es la de MAGI ─▶ «ya está» (ni copia ni escritura)
      ▼ [FILA DE TECLAS DE TERMUX] · en la CLI, «¿instalar? [s/N]» ─ Esc · n ─▶ nada
  copia termux.properties.magi-<fecha> (si existía) ─ falla ─▶ error; no se escribe nada
      ▼ sustituir solo la línea lógica extra-keys (con sus continuaciones \) o añadirla al final con un comentario
  escritura atómica (con los permisos que tenía; 600 si es nuevo) ─▶ REGISTRO termux_teclas_instaladas
      ▼ termux-reload-settings (con plazo) ─ no responde ─▶ «reinicia Termux para ver la fila»
  la fila sale bajo el terminal: ESC TAB CTRL ALT ^] ← ↓ ↑ → / F1 … F9
```

### 4.9 Publicar una versión para Termux

```
  el desarrollador empuja el tag vX.Y.Z (nunca android-v*) ─▶ release.yml (dist, F9) y termux-release.yml a la vez
  termux-release.yml: cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh
      ▼ readelf -d ─ pide algo que no es de bionic ─▶ falla
      │             (libc++_shared.so: el paquete pasa a depender de libc++, con su línea en el informe)
  scripts/paquete-termux.sh X.Y.Z ─▶ magi-ssh_X.Y.Z_aarch64.deb (-Zxz) · comprobación de estructura · .sha256
      ▼ atestación de procedencia
  espera a la release del tag (cada 30 s, hasta 60 min) ─ no aparece ─▶ falla: «repítelo con workflow_dispatch»
      ▼ gh release upload vX.Y.Z (.deb y .sha256)
  TUR: la receta de packaging/termux/ la propone el desarrollador (PR a TUR) y se actualiza con cada versión
```

El agente prepara el workflow, el script, la receta y `docs/publicar-termux.md`; el tag, la subida y la PR a TUR son del desarrollador.

### 4.10 Diagrama de secuencia: desbloquear el llavero y conectar con contraseña

```
magi (cliente)              llavero.age                   magi --servidor                           web-01
   │ «Frase maestra» al abrir     │                               │                                     │
   │─descifrar (scrypt)──────────▶│                               │                                     │
   │◀─────────────────── entradas │                               │                                     │
   │ frase y entradas en memoria  │                               │                                     │
   │─AbrirSesion{host: web-01}───────────────────────────────────▶│                                     │
   │                              │                               │─SSH: autenticación─────────────────▶│
   │                              │                               │◀─────────────────── pide contraseña │
   │◀────────────────────────────── pide contraseña (Solicitante) │                                     │
   │ la lee del llavero           │                               │                                     │
   │─Contrasena (Zeroizing)──────────────────────────────────────▶│                                     │
   │                              │                               │─contraseña─────────────────────────▶│
   │                              │                               │◀───────────────────────────── shell │
   │◀────────────────────────────────── Abierta · Sesiones{lista} │                                     │
   │ si está bloqueado: diálogo   │                               │                                     │
   │   o error si es automático   │                               │                                     │
```

### 4.11 Diagrama de secuencia: aviso de atención con MAGI en segundo plano

```
magi --servidor                   dev-01 (tmux)                   Termux:API                  Android
   │ vigilante: cada vigilar_seg        │                               │                         │
   │─exec estado (plazo 10 s)──────────▶│                               │                         │
   │◀─────────── agente-api · campana 1 │                               │                         │
   │ atención: no → sí                  │                               │                         │
   │─termux-notification --id magi-7c1─────────────────────────────────▶│                         │
   │ «◆ agente-api pide atención»       │                               │                         │
   │   y el host: dev-01                │                               │                         │
   │                                    │                               │─notificación───────────▶│
   │                                    │                               │                         │ tocar: Termux delante
   │ el usuario se engancha (↵)         │                               │                         │
   │─exec estado (siguiente)───────────▶│                               │                         │
   │◀─────────── agente-api · campana 0 │                               │                         │
   │─termux-notification-remove magi-7c1───────────────────────────────▶│                         │
   │                                    │                               │─retira el aviso────────▶│
```

---

## 5. Paquete, Órdenes de Termux, Configuración, Teclas y CLI

### 5.1 El paquete `.deb`

| Ruta (bajo `/data/data/com.termux/files/usr/`) | Contenido |
|---|---|
| `bin/magi` | El binario (700, como los paquetes de Termux) |
| `share/man/man1/magi.1.gz` | La página man de `assets/` |
| `share/bash-completion/completions/magi` · `share/zsh/site-functions/_magi` · `share/fish/vendor_completions.d/magi.fish` | El autocompletado de `assets/` |
| `share/doc/magi-ssh/` | `LICENSE-MIT`, `LICENSE-APACHE` y `README.md` |

`control`: `Package: magi-ssh`, `Version: X.Y.Z`, `Architecture: aarch64`, `Maintainer: 4d3`, `Installed-Size`, `Suggests: termux-api, openssh`, `Homepage` y `Description`. Sin `Depends` mientras `readelf -d` solo pida bibliotecas de bionic. Se arma con `dpkg-deb -Zxz --root-owner-group` (xz, que el `dpkg` de Termux lee siempre).

La receta de TUR (`packaging/termux/build.sh`) compila desde el tarball del tag con `termux_setup_rust`, con `TERMUX_PKG_EXCLUDED_ARCHES="arm, i686"` (y x86_64 si TUR no lo compila bien), e instala lo mismo.

### 5.2 Órdenes de Termux

| Orden | Quién | Para qué | Si falla o no responde |
|---|---|---|---|
| `termux-clipboard-set` (texto por la entrada estándar) | Cliente | Copiar | OSC 52; Termux:API apagada en el cliente |
| `termux-open-url <url>` | Cliente | Enlaces y visor de escritorios | Mensaje con su salida |
| `termux-wake-lock` · `termux-wake-unlock` | Servidor | Wake lock | Apagado en el servidor, con aviso una vez |
| `termux-notification --id magi-<id> --group magi --alert-once -t <título> -c <texto> --action <fija>` | Servidor | Avisos de Android | Termux:API apagada en el servidor |
| `termux-notification-remove magi-<id>` | Servidor | Retirar avisos | Queda en el estado; lo retira el siguiente servidor |
| `termux-reload-settings` | Cliente o CLI | Ver la fila de teclas nueva | «Reinicia Termux para ver la fila» |
| `termux-battery-status` | `magi diagnostico` | Saber si Termux:API responde, sin efectos | Línea `!` de Termux:API |

Reglas comunes (T139): argumentos sueltos, nunca un shell; plazo `[termux] api_plazo_seg` (5 s); salida con tope de 64 KiB; código distinto de 0 es un fallo. El título y el texto de un aviso llevan solo el nombre de la sesión o de la pestaña y el del host (T140): «◆ agente-api pide atención» o «! web-01 pide atención», y «dev-01 · MAGI». La acción del aviso la ejecuta Termux:API con un shell, así que es una constante de MAGI (`am start -n com.termux/com.termux.app.TermuxActivity`), sin datos del inventario ni del remoto.

### 5.3 Configuración (`config.toml`)

| Clave | Por defecto | Uso |
|---|---|---|
| `[termux] wake_lock` | `"auto"` | `auto`: mientras haya trabajo y 30 s después; `nunca`: no se toca |
| `[termux] avisos` | `true` | Avisos de Android con Termux:API |
| `[termux] api_plazo_seg` | `5` | Plazo de cada orden de Termux (2-30) |
| `[termux] sincronizacion_sondeo_seg` | `30` | Cada cuánto se mira la carpeta de sincronización con MAGI abierto (10-600) |
| `[terminal] abrir_enlaces` | en Termux, `"termux-open-url"` | (F15) Programa que abre un enlace, sin shell |
| `[escritorio] navegador` | `"auto"` | (F12) En Termux, `auto` es `termux-open-url {url}` |

Un valor fuera de rango se acota donde se usa y MAGI avisa al arrancar (D261). Fuera de Termux, `[termux]` se ignora sin avisar.

### 5.4 Teclas

Fila propuesta, en una sola línea de `termux.properties` (con el prefijo por defecto):

```
extra-keys = [['ESC','TAB','CTRL','ALT',{macro: 'CTRL ]', display: '^]'},'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]
```

Con otro prefijo configurado, el macro y su texto cambian (`CTRL a` → `^A`). Las vistas F1-F9 son las de siempre; `F10` no está en la fila (MAGI no lo usa) y, como el resto, Termux lo da con Volumen arriba + 0.

| Dónde | Tecla | Acción |
|---|---|---|
| Paleta (solo en Termux) | «Desbloquear el llavero» · «Bloquear el llavero» | Pide la frase · borra de la memoria la frase y las entradas |
| | «Cambiar la frase maestra» · «Borrar el llavero» | Diálogos CAMBIAR LA FRASE MAESTRA y BORRAR EL LLAVERO |
| | «Instalar la fila de teclas de Termux» | Diálogo FILA DE TECLAS DE TERMUX |
| Diálogos de la frase | `↵` · `Tab` · `Esc` | Aceptar · siguiente campo · cancelar (o seguir sin llavero) |
| Termux, sin instalar nada | Volumen arriba + 1…9 · + 0 | F1…F9 · F10 |
| Sesión (F3) | `prefijo w` | En Termux: «abre otra sesión de Termux (desliza desde el borde izquierdo → NEW SESSION) y ejecuta magi» |

### 5.5 CLI

| Orden | Qué hace |
|---|---|
| `magi llavero estado` | Dice si hay llavero, su ruta, su tamaño y su fecha; nunca pide la frase ni enseña entradas |
| `magi llavero cambiar` | Pide en el TTY la frase actual y la nueva dos veces, y recifra |
| `magi llavero borrar` | Explica qué se pierde y pide escribir `borrar` |
| `magi termux teclas [--instalar]` | Enseña la fila propuesta y la actual; con `--instalar`, pregunta y la instala (§4.8) |
| `magi termux aviso` | Publica un aviso de prueba y lo retira a los 10 s (para comprobar el permiso de notificaciones de Termux:API) |
| `magi diagnostico` | (F9) Gana el bloque TERMUX (§6.5) |

Fuera de Termux, `magi llavero` dice «en este equipo el llavero es el del sistema» y `magi termux`, «solo en Termux», los dos con código 1. `assets/` se regenera (página man y autocompletado).

### 5.6 Protocolo: sin cambios

`VERSION_PROTOCOLO` sigue en 10. El servidor pide las contraseñas al cliente como siempre (Solicitante); el wake lock y los avisos son internos del servidor, y el portapapeles y el llavero, del cliente. `TrabajoServidor` no cambia.

### 5.7 Compilación y CI

- **Local:** `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh` con el NDK r30 del repositorio; `scripts/paquete-termux.sh <versión>` arma el `.deb` a partir de ese binario.
- **`ci.yml`, job `termux`:** en las PR que tocan `src/`, `crates/magi-nucleo/`, `Cargo.toml`, `Cargo.lock`, `packaging/termux/`, `scripts/paquete-termux.sh` o el propio workflow. Compila para aarch64 (y para x86_64, solo como artefacto), arma el `.deb` y pasa `termux_paquete`, que lo lee de `MAGI_DEB` (con `CI` puesta falla si no lo hay; fuera, se omite con un aviso).
- **`termux-release.yml`:** §4.9, con `contents: write`, `id-token: write` y `attestations: write`, y solo `GITHUB_TOKEN`. Las herramientas que se descarguen van fijadas por SHA-256, como en el resto de workflows.
- `actionlint` y `shellcheck packaging/termux/build.sh scripts/paquete-termux.sh tests/fijos/termux/*`.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 ARRANQUE EN TERMUX ── primera vez: [FILA DE TECLAS DE TERMUX] · con llavero.age: [LLAVERO DE MAGI]
 │   [LLAVERO DE MAGI] ─ ↵ ─▶ vista de inicio · Esc ─▶ vista de inicio (bloqueado) · dañado ─▶ [LLAVERO DAÑADO]
 FLOTA (F1) · HOSTS (F2) · SESIONES (F3) … SERVICIOS (F9) ── como en el escritorio, en modo estrecho si hace falta
 │   conectar a un host con contraseña del llavero ─▶ [LLAVERO DE MAGI] si está bloqueado
 │   «recordar» sin llavero ─▶ [CREAR LA FRASE MAESTRA]
 SINCRONIZACIÓN (F10) ── «Unir este equipo» ─▶ carpeta ─▶ clave ─▶ [CREAR LA FRASE MAESTRA] o [LLAVERO DE MAGI]
 PALETA (Ctrl+P) ── desbloquear · bloquear el llavero · [CAMBIAR LA FRASE MAESTRA] · [BORRAR EL LLAVERO] ·
 │                  [FILA DE TECLAS DE TERMUX]
 SESIÓN (F3) ── prefijo w ─▶ «abre otra sesión de Termux» · modo historial, copiar ─▶ portapapeles (§4.4)
 BARRA GLOBAL ── «llavero bloqueado» · «◆ agente-api pide atención · dev-01» (y, con Termux:API, el aviso de Android)
 ANDROID ── aviso ─ tocar ─▶ Termux delante, en la vista donde estaba MAGI
 CLI ── magi llavero estado|cambiar|borrar · magi termux teclas [--instalar] · magi termux aviso · magi diagnostico
```

Las pantallas siguientes están a 48 columnas, un móvil en vertical. En las que la llevan, las dos últimas líneas son la fila de teclas de MAGI, que pinta Termux y no forma parte de MAGI.

### 6.1 Llavero de MAGI

```
┌ MAGI · FLOTA ────────────────────────────────┐
│ ▸● web-01      NOMINAL                       │
│  ┌ LLAVERO DE MAGI ───────────────────────┐  │
│  │ Frase maestra                          │  │
│  │ ••••••••••••••_                        │  │
│  │                                        │  │
│  │ Desbloquea tus contraseñas y la clave  │  │
│  │ de sincronización de este equipo.      │  │
│  │                                        │  │
│  │ ↵ desbloquear · Esc seguir sin él      │  │
│  └────────────────────────────────────────┘  │
│                                              │
├──────────────────────────────────────────────┤
│ ↵ conectar · ? ayuda · ^] prefijo            │
└──────────────────────────────────────────────┘
 ESC │TAB │CTRL│ALT │ ^] │ ←  │ ↓  │ ↑  │ →  
  F1 │ F2 │ F3 │ F4 │ F5 │ F6 │ F7 │ F8 │ F9 
```

### 6.2 Crear la frase maestra

```
┌ MAGI · SINCRONIZACIÓN ───────────────────────┐
│ Unir este equipo · termux-abc-x10            │
│  ┌ CREAR LA FRASE MAESTRA ────────────────┐  │
│  │ Frase    ••••••••••••••••              │  │
│  │ Repetir  ••••••••••••••••_             │  │
│  │                                        │  │
│  │ Guarda tus contraseñas y la clave de   │  │
│  │ sincronización. Al menos 12            │  │
│  │ caracteres; mejor, varias palabras.    │  │
│  │ Si la olvidas no se recupera: habría   │  │
│  │ que borrar el llavero y volver a unir  │  │
│  │ este equipo.                           │  │
│  │ ↵ crear · Tab siguiente · Esc cancelar │  │
│  └────────────────────────────────────────┘  │
├──────────────────────────────────────────────┤
│ Esc cancelar                                 │
└──────────────────────────────────────────────┘
```

### 6.3 Flota en el móvil y aviso de Android

```
┌ MAGI · FLOTA ────────────────────────────────┐
│  HOST      ESTADO    CARGA  SESIONES         │
│ ▸web-01    NOMINAL   0,4    ● 2 ◆            │
│  dev-01    CARGA     3,1    ● 1 ◆ !          │
│  db-01     ALCANZ.   —                       │
│  nas       CAÍDA     —                       │
│                                              │
│                                              │
├──────────────────────────────────────────────┤
│ ◆ agente-api pide atención · dev-01          │
│ ↵ conectar · F2 hosts · ? más                │
└──────────────────────────────────────────────┘
 ESC │TAB │CTRL│ALT │ ^] │ ←  │ ↓  │ ↑  │ →  
  F1 │ F2 │ F3 │ F4 │ F5 │ F6 │ F7 │ F8 │ F9 
```

```
┌────────────────────────────────────────────┐
│ ▣ Termux:API · ahora                       │
│ ◆ agente-api pide atención                 │
│ dev-01 · MAGI                              │
└────────────────────────────────────────────┘
```

El aviso solo dice qué sesión y en qué host; tocarlo trae Termux delante. La barra de MAGI dice lo mismo mientras haya una ventana abierta.

### 6.4 Fila de teclas de Termux

```
┌ MAGI · HOSTS ────────────────────────────────┐
│ ▸web-01                                      │
│  ┌ FILA DE TECLAS DE TERMUX ──────────────┐  │
│  │ MAGI propone esta fila:                │  │
│  │  ESC TAB CTRL ALT ^]  ←  ↓  ↑  →       │  │
│  │  F1  F2  F3   F4  F5  F6 F7 F8 F9      │  │
│  │                                        │  │
│  │ Se escribe en ~/.termux/               │  │
│  │ termux.properties, solo la línea       │  │
│  │ extra-keys. Antes, una copia:          │  │
│  │ termux.properties.magi-2026-11-17      │  │
│  │                                        │  │
│  │ ↵ instalar · Esc ahora no              │  │
│  └────────────────────────────────────────┘  │
├──────────────────────────────────────────────┤
│ Esc volver                                   │
└──────────────────────────────────────────────┘
```

### 6.5 `magi diagnostico` en Termux

```
$ magi diagnostico
MAGI 1.6.0 · protocolo 10 · commit abc1234

  ✓ datos          ~/.local/share/magi (700) · magi.db versión 11
  ✓ configuración  ~/.config/magi/config.toml
  ✓ estado         ~/.local/state/magi (logs)
  ✓ runtime        $TMPDIR/magi-10234 (700) · socket 60 bytes
  ✓ servidor       en marcha · 1.6.0 · 1 sesión
  ✓ agente SSH     1 clave cargada
  ✓ llavero        fichero con frase maestra (creado el 17 nov 2026)
  ✓ portapapeles   termux-clipboard-set (Termux:API)
  ✓ editor         nano ($EDITOR)
  ✓ paginador      less
  · terminal       en Termux no aplica: abre otra sesión de Termux para otra ventana
  ✓ ssh_config     con «Include magi_config»

  TERMUX
  ✓ Termux         0.118.3 · Android 15 (API 35) · aarch64
  ✓ Termux:API     responde (0,4 s)
  ✓ almacenamiento ~/storage/shared → /storage/emulated/0
  ✓ teclas         la fila de MAGI (~/.termux/termux.properties)
  ✓ wake lock      auto · ahora sin trabajo
  ✓ avisos         activados
  · segundo plano  Android 12+ puede cerrar procesos de Termux. En Android 14 o posterior:
                   Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»
  · batería        quita la optimización de batería de Termux (y, según el fabricante, permite
                   su actividad en segundo plano)

0 avisos, 0 errores
```

Las líneas con `·` son lo que MAGI no puede comprobar desde Termux: no cuentan como avisos ni cambian el código de salida. En Android 12 y 13, la línea de segundo plano da las órdenes de `adb` que corresponden a la versión. Con el Termux de Google Play, la línea de Termux sale con `!` y recomienda el de F-Droid o GitHub. Nunca pide la frase maestra ni abre el llavero (F9).

### Marcas

| Marca | Texto en la barra | Significado |
|---|---|---|
| «llavero bloqueado» | llavero bloqueado | Algo necesita el llavero y está cerrado; Ctrl+P lo desbloquea |
| `◆` · `!` | pide atención | (F15, F16) Igual que en el escritorio; en Termux, además, aviso de Android |
| «Termux:API no responde» | — | Una vez por proceso: copia por OSC 52 y sin avisos de Android |

### Notas de UX

- **La frase maestra no se recupera,** y crearla lo dice sin rodeos: si se olvida, se borra el llavero y se vuelve a unir el equipo; el inventario sigue en los demás equipos.
- **Esc nunca deja a MAGI sin servir:** sin llavero se puede hacer todo menos lo que necesita secretos guardados, y la barra dice por qué.
- **Termux:API es opcional:** sin ella se copia por OSC 52 (hasta unos 4 KiB de texto) y los avisos se quedan en la barra.
- **Los avisos de Android no cuentan nada** de lo que pasa en la sesión: solo cuál y dónde (T140).
- **La fila de teclas nunca se instala sin preguntar,** y siempre deja una copia del fichero de Termux.
- **Pantalla pequeña:** el modo estrecho de la F7 hace el trabajo; las vistas principales y los diálogos nuevos tienen instantáneas a 48×20 además de las de 40×12, 80×24 y 200×60 (T45). Cuando sale el teclado en pantalla, Termux encoge el terminal y MAGI se redibuja.

---
## 7. Lógica de Negocio

### 7.1 Plataforma y rutas

```python
def es_termux(compilado_para_android, entorno, depuracion):
    """Se decide al compilar. Solo en compilaciones de depuración, MAGI_PLATAFORMA=termux lo fuerza para las pruebas."""
    if compilado_para_android:
        return True
    return depuracion and entorno.get("MAGI_PLATAFORMA") == "termux"

def rutas_termux(entorno, uid):
    """Siempre del entorno, nunca del uid: bionic no sabe dónde está el home de Termux."""
    home, prefijo = entorno["HOME"], entorno["PREFIX"]       # /data/data/com.termux/files/home y …/usr
    datos = entorno.get("XDG_DATA_HOME") or f"{home}/.local/share"
    config = entorno.get("XDG_CONFIG_HOME") or f"{home}/.config"
    estado = entorno.get("XDG_STATE_HOME") or f"{home}/.local/state"
    if entorno.get("XDG_RUNTIME_DIR"):
        runtime = f"{entorno['XDG_RUNTIME_DIR']}/magi"
    else:                                                     # Termux no la pone y no hay /tmp
        runtime = f"{entorno.get('TMPDIR') or prefijo + '/tmp'}/magi-{uid}"
    return {"datos": f"{datos}/magi", "config": f"{config}/magi", "estado": f"{estado}/magi",
            "socket": f"{runtime}/servidor.sock", "sh": f"{prefijo}/bin/sh"}
```

- El directorio del socket se crea con permisos 700 y la ruta tiene que quedar por debajo de 100 bytes, como en el escritorio (R41): con el `TMPDIR` de Termux son unos 60.
- Las propiedades de Android (modelo, versión y SDK) se leen con `__system_property_get` de bionic, sin lanzar `getprop`.
- Lo que el escritorio ejecuta con `sh -c` en local (las comprobaciones propias de la deliberación, F14) usa `rutas["sh"]`: en Termux no existe `/bin/sh`.

### 7.2 Órdenes de Termux

```python
DE_LA_API = {"termux-clipboard-set", "termux-notification", "termux-notification-remove", "termux-battery-status"}

def orden_termux(argumentos, api, plazo, entrada=None):
    """Sin shell, con plazo y con tope de salida. Si Termux:API está apagada, sus órdenes ni se intentan."""
    de_api = argumentos[0] in DE_LA_API
    if de_api and api.apagada:
        return Fallo("Termux:API apagada")
    if not buscar_en_path(argumentos[0]):
        if de_api:
            api.apagar("ausente")                               # falta el paquete termux-api
        return Fallo(f"{argumentos[0]} no está instalado")
    try:
        r = ejecutar(argumentos, entrada=entrada, plazo=plazo, tope_salida=64 * 1024)   # mata el grupo al vencer
    except PlazoAgotado:
        if de_api:
            api.apagar("no_responde")                           # falta la app Termux:API o está parada
        return Fallo("no responde")
    if r.codigo != 0:
        if de_api:
            api.apagar("no_responde")
        return Fallo(r.error[:200])
    if de_api:
        api.disponible = True
    return Exito(r.salida)
```

- `api.apagar` deja la función apagada hasta que ese proceso (cliente o servidor) reinicie y lo dice una vez en la barra.
- Ninguna orden se espera en el bucle de la interfaz: van en tareas de tokio, como el resto de E/S.

### 7.3 Llavero de fichero

```python
MIN_FRASE = 12
LOG_N_CREAR, LOG_N_MAX = 17, 20
TOPE = 1024 * 1024

def preparar_frase(frase, repetida=None):
    f = unicodedata.normalize("NFC", frase)                     # la misma frase con otro teclado da lo mismo
    if repetida is not None and f != unicodedata.normalize("NFC", repetida):
        raise Error("las frases no coinciden")
    if len(f) < MIN_FRASE or not f.strip():
        raise Error("al menos 12 caracteres")
    return Zeroizing(f)

def abrir(ruta, frase):
    datos = leer_con_tope(ruta, TOPE)
    cabecera = leer_cabecera_age(datos)                         # ilegible → Danado
    if cabecera.tipo != "scrypt" or cabecera.log_n > LOG_N_MAX:
        raise Danado
    try:
        claro = descifrar(datos, frase)
    except NingunaClaveCoincide:
        raise FraseIncorrecta                                   # el fichero no se toca
    except ContenidoCorrupto:
        raise Danado
    contenido = json.loads(claro)                               # error o formato ≠ 1 → Danado
    return Zeroizing(contenido["entradas"])

def guardar(ruta, frase, clave, secreto):
    with flock(ruta + ".lock"):
        entradas = abrir(ruta, frase) if existe(ruta) else {}   # relee: otra ventana pudo escribir
        entradas[clave] = secreto
        escribir_atomico(ruta, cifrar({"formato": 1, "entradas": entradas}, frase, LOG_N_CREAR), permisos=0o600)

class Intentos:
    """La espera no es la protección (lo es scrypt): evita repetir errores a ciegas."""
    def fallo(self, ahora):
        self.seguidos += 1
        if self.seguidos % 5 == 0:
            self.espera_hasta = ahora + 30
            anotar("llavero_desbloqueo_fallido", intentos=self.seguidos)

    def acierto(self):
        self.seguidos = 0
```

- La interfaz de siempre: `leer`, `guardar` y `borrar` sobre las mismas claves que el llavero del escritorio. Con el llavero cerrado, `leer` devuelve «bloqueado», que la conexión interactiva convierte en el diálogo de la frase y la automática, en error con instrucción.
- Cambiar la frase abre con la actual y escribe con la nueva bajo el mismo cerrojo. Apartar uno dañado lo renombra a `llavero.age.danado-<AAAA-MM-DD>` (con `-2`, `-3`… si ya existe).

### 7.4 Portapapeles

```python
TOPE_API = 512 * 1024
TOPE_OSC52 = 6000                    # bytes ya en base64: con más, Termux no copia nada

def copiar(texto, api, terminal, plazo):
    datos = texto.encode()
    if not api.apagada:
        if len(datos) > TOPE_API:
            return f"demasiado para el portapapeles ({kib(datos)}): guárdalo en un fichero"
        if orden_termux(["termux-clipboard-set"], api, plazo, entrada=datos).ok:
            return f"copiado ({kib(datos)})"
    codificado = base64.b64encode(datos)
    if len(codificado) <= TOPE_OSC52:
        terminal.escribir(b"\x1b]52;c;" + codificado + b"\x07")
        return "copiado"
    return f"no cabe en el portapapeles de Termux ({kib(datos)}): instala Termux:API o guárdalo"
```

### 7.5 Wake lock

```python
class WakeLock:
    def al_arrancar(self, ahora):
        if existe_marca("wake-lock"):                           # el servidor anterior murió con él tomado
            self.tomado, self.soltar_en = True, ahora + 30

    def trabajo_cambia(self, hay_trabajo, ahora):
        if self.modo == "nunca" or self.apagado:
            return
        if hay_trabajo:
            self.soltar_en = None
            if not self.tomado:
                if orden_termux(["termux-wake-lock"], self.api, self.plazo).ok:
                    self.tomado = True
                    poner_marca("wake-lock")
                    aviso_una_vez("segundo-plano", si=sdk_android() >= 31)
                else:
                    self.apagado = True
                    aviso("sin wake lock: Android puede dormir MAGI")
        elif self.tomado and self.soltar_en is None:
            self.soltar_en = ahora + 30

    def tic(self, ahora):
        if self.soltar_en is not None and ahora >= self.soltar_en:
            orden_termux(["termux-wake-unlock"], self.api, self.plazo)
            self.tomado, self.soltar_en = False, None
            quitar_marca("wake-lock")
```

Al terminar el servidor (apagado normal o `SIGTERM`), si lo tiene tomado, lo suelta antes de salir.

### 7.6 Avisos de Android

```python
ACCION = "am start -n com.termux/com.termux.app.TermuxActivity"   # constante: Termux:API la pasa por un shell

def id_aviso(origen):
    """Nada del inventario ni del remoto: el uuid corto de la ficha o el id de la pestaña."""
    return f"magi-{origen.ficha_uuid[:8]}" if origen.persistente else f"magi-p{origen.pestana_id}"

def textos(origen):
    if origen.persistente:
        titulo = f"◆ {origen.nombre_sesion} pide atención"          # nombre validado (F16)
    else:
        titulo = f"! {sin_controles(origen.nombre_pestana)[:40]} pide atención"
    return titulo[:80], f"{origen.host} · MAGI"

def atencion_cambia(origen, pide, mirando, cfg, api, publicados):
    if not cfg.avisos or api.apagada:
        return
    i = id_aviso(origen)
    if pide and not mirando and i not in publicados:
        titulo, texto = textos(origen)
        r = orden_termux(["termux-notification", "--id", i, "--group", "magi", "--alert-once",
                          "-t", titulo, "-c", texto, "--action", ACCION], api, cfg.api_plazo_seg)
        if r.ok:
            publicados.add(i)
            guardar_estado("avisos", publicados)
    elif (not pide or mirando) and i in publicados:
        orden_termux(["termux-notification-remove", i], api, cfg.api_plazo_seg)
        publicados.discard(i)
        guardar_estado("avisos", publicados)
```

- `mirando`: alguna ventana tiene la pestaña delante y con foco (foco agregado de la F15). En las sesiones persistentes que solo ve el vigilante, la atención de la F16 ya significa que nadie la mira.
- Al arrancar, el servidor retira los ids que encuentre en el estado; al terminar, los suyos.

### 7.7 Fila de teclas y `termux.properties`

```python
def fila(prefijo="CTRL ]", texto="^]"):
    arriba = ["'ESC'", "'TAB'", "'CTRL'", "'ALT'", f"{{macro: '{prefijo}', display: '{texto}'}}",
              "'LEFT'", "'DOWN'", "'UP'", "'RIGHT'"]
    abajo = [f"'F{n}'" for n in range(1, 10)]
    return f"extra-keys = [[{','.join(arriba)}],[{','.join(abajo)}]]"

ES_EXTRA_KEYS = re.compile(r"^\s*extra-keys\s*[=:\s]")

def continua(linea):
    """Como Java Properties: sigue en la siguiente si acaba en un número impar de barras invertidas."""
    l = linea.rstrip("\r\n")
    return (len(l) - len(l.rstrip("\\"))) % 2 == 1

def fusionar(texto, nueva):
    """Sustituye la última línea lógica extra-keys (la que usa Termux) o la añade al final. El resto, byte a byte."""
    lineas = texto.splitlines(keepends=True)
    i, ultima = 0, None
    while i < len(lineas):
        comentario = lineas[i].lstrip().startswith(("#", "!"))
        fin = i
        while not comentario and continua(lineas[fin]) and fin + 1 < len(lineas):
            fin += 1
        if not comentario and ES_EXTRA_KEYS.match(lineas[i]):
            ultima = (i, fin)
        i = fin + 1
    if ultima:
        i, fin = ultima
        salto = "\r\n" if lineas[fin].endswith("\r\n") else "\n"
        return "".join(lineas[:i]) + nueva + salto + "".join(lineas[fin + 1:])
    separador = "" if texto == "" or texto.endswith("\n") else "\n"
    return texto + separador + "# fila de teclas de MAGI (magi termux teclas)\n" + nueva + "\n"
```

- «Ya es la de MAGI» compara el valor de la línea efectiva sin espacios con el de `fila()`.
- El fichero se lee entero: hasta 256 KiB y en UTF-8; si no, no se toca («termux.properties no se puede leer: edítalo a mano»).

### 7.8 Nombre del equipo

```python
def nombre_equipo_termux(modelo):
    """ro.product.model con la regla de la etiqueta de la F15: «ABC-X10» → «termux-abc-x10» (20 caracteres como mucho)."""
    return etiqueta_equipo(f"termux {modelo}".strip())
```

Es el nombre por defecto del equipo en la sincronización, la etiqueta de las sesiones persistentes (`termux-abc-x10-1`) y el usuario de las deliberaciones; en la sincronización se puede cambiar como en el escritorio.

### 7.9 Diagnóstico en Termux

```python
def segundo_plano(sdk):
    if sdk < 31:
        return None                                         # antes de Android 12 no hay límite de procesos
    if sdk == 31:
        return ("adb shell device_config set_sync_disabled_for_tests persistent · "
                "adb shell device_config put activity_manager max_phantom_processes 2147483647")
    if sdk <= 33:
        return "adb shell settings put global settings_enable_monitor_phantom_procs false"
    return "Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»"

def linea_termux(version, release, sdk):
    marca = "!" if version.startswith("googleplay.") else "✓"
    consejo = " · no soportado: usa el Termux de F-Droid o GitHub" if marca == "!" else ""
    return marca, "Termux", f"{version or '?'} · Android {release} (API {sdk}) · aarch64{consejo}"
```

Las líneas con `·` (segundo plano y batería) son información: no suman avisos ni cambian el código de salida. Termux:API se comprueba con `termux-battery-status` y el plazo de siempre; el llavero, mirando solo si el fichero existe.

### 7.10 Validaciones

- **Frase maestra:** al menos 12 caracteres tras normalizar a NFC, no solo espacios, la misma dos veces al crearla o cambiarla, hasta 1 024 bytes.
- **Llavero:** una sola entrada scrypt con factor de hasta 2^20, hasta 1 MiB descifrado y JSON con `formato` 1; lo demás es «dañado».
- **Nombre del equipo:** la etiqueta de la F15.
- **Carpeta de sincronización:** absoluta tras `realpath`, existe y se puede escribir; siguen las reglas de la F10 dentro de ella (sin enlaces simbólicos, nombres de fichero con `uuid`).
- **Avisos:** ids `^magi-p?[0-9a-z]{1,16}$`; título y texto sin caracteres de control y de hasta 80 caracteres.
- **`termux.properties`:** hasta 256 KiB y UTF-8.
- **`[termux]`:** los rangos de §5.3.

### 7.11 Casos especiales

- **Dos ventanas de MAGI en Termux** (dos sesiones de Termux): comparten servidor; cada una pide la frase y las escrituras del llavero van con cerrojo.
- **Alguien borra `llavero.age` a mano:** es como si no existiera; la sincronización pide unir de nuevo este equipo.
- **Termux:API sin permiso de notificaciones** (Android 13 o posterior): `termux-notification` responde bien pero no se ve nada; `magi termux aviso` lo deja ver (R169).
- **La app Android y Termux en el mismo móvil:** dos instalaciones independientes; la app sigue siendo lectora.
- **Teclado físico** (Bluetooth, o el móvil conectado a una pantalla): las teclas de función llegan solas; la fila sobra pero no molesta.
- **Plegable:** al abrirlo o cerrarlo cambia el tamaño del terminal y MAGI se redibuja (F7).
- **Android cierra el servidor:** como un apagado brusco del equipo: las pestañas normales se pierden, las sesiones persistentes siguen en su host (F16) y el siguiente `magi` arranca otro servidor, que retira avisos huérfanos y suelta el wake lock.
- **`XDG_RUNTIME_DIR` puesto por el usuario:** se respeta, como en Linux.
- **La carpeta de sincronización dentro del home de Termux:** se puede, pero ninguna otra app la ve; MAGI no lo impide.

### 7.12 Limitaciones

- Android puede cerrar MAGI en segundo plano aunque tenga el wake lock (R166): `magi diagnostico` dice qué ajustar, pero no hay garantía.
- Sin Termux:API, solo se copian unos 4 KiB (OSC 52).
- Los cambios de la carpeta se ven con hasta `sincronizacion_sondeo_seg` de retraso (R172).
- Si Termux no manda eventos de foco, las pestañas a la vista también avisan (D379).
- El llavero no se abre con la huella.

---

## 8. Requisitos No Funcionales

**Seguridad**

- La frase maestra solo vive en la memoria del cliente (`Zeroizing`); nunca va a disco, al log, a REGISTRO ni al servidor. scrypt con 2^17 al crear y un tope de 2^20 al abrir.
- `llavero.age` está en el directorio privado de Termux (600): otras apps de Android no lo leen, pero cualquier programa que corra dentro de Termux sí; por eso va cifrado (T19).
- Las órdenes de Termux van sin shell y con plazo (T139); la acción de los avisos es una constante, porque Termux:API la ejecuta con un shell.
- Los avisos de Android llevan solo el nombre y el host (T140): salen en la pantalla de bloqueo.
- El visor de escritorios sigue solo en `127.0.0.1` con enlaces de un solo uso y clave por sesión (T98). En Android, cualquier app del móvil llega a `127.0.0.1`: lo que protege es eso, como en un equipo compartido (R111).
- Escribir en `termux.properties` pide confirmación, deja copia y toca solo su línea (T138).
- El `.deb` sale de CI a partir de un tag, con su SHA-256 y atestación de procedencia; nunca de un equipo personal.
- Lo que devuelven las órdenes de Termux es entrada no fiable: tope de 64 KiB y nunca pánico.

**Protección de datos (RGPD)**

- Nada nuevo sale del móvil: el llavero y la frase no viajan, y la sincronización es la de la F10.
- Los avisos de Android no llevan nada de lo que pasa en la sesión.
- REGISTRO no guarda frases, secretos ni contenido del llavero.

**Backups y recuperación**

- `llavero.age` no se copia a ningún sitio. Si se pierde el móvil, se borra Termux o se olvida la frase, se vuelve a unir el equipo y a escribir las contraseñas: el inventario está en los demás equipos.
- Antes de tocar `termux.properties` queda `termux.properties.magi-<fecha>`.
- Las copias previas a importar de la F10 siguen igual.

**Rendimiento**

- Desbloquear el llavero tarda alrededor de un segundo (scrypt) y usa 128 MiB durante ese segundo.
- El sondeo de la carpeta es un listado y un `stat` por fichero cada 30 s, solo con MAGI abierto.
- Cada orden de Termux tarda como mucho `api_plazo_seg` y nunca bloquea la interfaz.
- El binario y su consumo son los del escritorio; en reposo, el servidor no hace nada que no hiciera ya.

**Accesibilidad**

La fila de teclas lleva texto en cada tecla; el tamaño de letra lo controla Termux (pellizco). Lo demás, como en el escritorio: marcas con texto en la barra y respaldo ASCII.

---

## 9. Integraciones

| Con | Versión | Uso |
|---|---|---|
| Termux | 0.118 o posterior, de F-Droid o GitHub | Terminal, `$PREFIX`, `apt`, termux-tools (`termux-open-url`, `termux-wake-lock`, `termux-wake-unlock`, `termux-reload-settings`) |
| Termux:API | La app de la misma fuente que Termux y el paquete `termux-api` | Portapapeles, avisos y comprobación (`termux-battery-status`) |
| Android | 7 o posterior (API 24); validación en el plegable | Navegador para el visor (Chrome 94 o posterior), notificaciones, límite de procesos de 12 o posterior |
| TUR | — | Receta `packaging/termux/build.sh` |
| GitHub Releases | — | `.deb`, `.sha256` y atestación, junto a los binarios de `dist` |
| Syncthing para Android (o la app que use el usuario) | — | Sincroniza la carpeta del almacenamiento compartido con los escritorios |
| OpenSSH de Termux | — | Agente SSH (`ssh-agent`, `ssh-add`) para las identidades del agente |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D371 — El mismo binario de escritorio, compilado para `aarch64-linux-android`** (Hector, 1a; T137). Lo propio de Termux vive en `src/termux/` y se elige con `cfg(target_os = "android")`; en las pruebas, `MAGI_PLATAFORMA=termux` (solo en depuración) lo activa en Linux, con dobles de las órdenes de Termux. Descartados un binario aparte con menos funciones (dos productos que mantener) y llevarlo dentro de la app (es otra interfaz).
- **D372 — Un `.deb` en cada release, hecho por CI, y receta para TUR** (Hector, 2a). `termux-release.yml` con el mismo tag que la release de escritorio y solo `GITHUB_TOKEN`; la receta vive en `packaging/termux/` y la PR a TUR la abre el desarrollador. Descartado el repositorio oficial de Termux (pide proyectos conocidos y activos, y su ritmo no es el nuestro) y un `curl | sh` (la F9 ya lo descartó).
- **D373 — Solo aarch64, Android 7 o posterior y el Termux de F-Droid o GitHub** (Hector, 3a). Es lo que tienen los móviles de hoy y lo que Termux mantiene; x86_64 se compila en CI como artefacto, sin publicar. El Termux de Google Play arranca, pero no se soporta.
- **D374 — En Termux, el llavero es un fichero `age` con frase maestra** (`LlaveroFichero`, en el núcleo, detrás de `LlaveroSecreto`) (Hector, 4a). Se pide al abrir MAGI y vive en la memoria del cliente hasta cerrar la ventana; Esc sigue sin él. Precisa T19: el llavero sigue fuera de `magi.db`. Descartados `termux-keystore` (no cifra en las versiones publicadas), la huella (`termux-fingerprint` solo dice sí o no), guardar en `magi.db` (T19) y no tener llavero (sin él no hay sincronización ni contraseñas recordadas).
- **D375 — Termux sincroniza como un equipo escritor, con plataforma `android` y nombre `termux-<modelo>`** (Hector, 5a). Sin `formato` nuevo: `android` ya existe en el núcleo y nada decide por él. La carpeta, en el almacenamiento compartido y resuelta con `realpath`; se sondea cada 30 s porque Android no garantiza avisos de cambios de otra app; los permisos 600 que no admite no son error. Descartado un valor `termux` (los equipos 1.5.0 no lo leerían).
- **D376 — Portapapeles con Termux:API y, si no, OSC 52; enlaces con `termux-open-url`** (Hector, 6a). `termux-clipboard-set` por la entrada estándar hasta 512 KiB; OSC 52 hasta 6 000 bytes codificados, que es lo que copia Termux; con más, aviso. Nunca se lee el portapapeles.
- **D377 — Los escritorios, en el navegador del móvil** (Hector, 7a): el mismo visor de la F12, abierto con `termux-open-url`. Descartados Termux:X11 y un visor propio en la terminal.
- **D378 — Wake lock automático mientras haya trabajo** (Hector, 8a): lo toma el servidor cuando `TrabajoServidor` deja de estar vacío y lo suelta 30 s después de quedar vacío; solo suelta el que tomó; `wake_lock = "nunca"` lo apaga. El límite de procesos de Android 12 o posterior no se puede quitar desde Termux: se explica una vez y en `magi diagnostico`.
- **D379 — Avisos de Android con Termux:API** (Hector, 9a), publicados por el servidor cuando una sesión persistente o una pestaña pide atención sin nadie mirando, y retirados al mirarla. En Termux, el vigilante de la F16 corre también sin ventanas mientras viva el servidor (precisa D334). «Sin nadie mirando» usa el foco agregado de la F15; si la prueba de concepto ve que Termux no manda eventos de foco al cambiar de app, las pestañas avisan aunque estén delante y el aviso se retira al escribir en ellas.
- **D380 — MAGI propone e instala su fila de teclas** (Hector, 10a): generada de su tabla de teclas y del prefijo configurado, con confirmación, copia con fecha, solo la línea `extra-keys` y `termux-reload-settings` (T138). Descartados escribir el fichero entero (pisaría la configuración del usuario) y solo documentarla (la mayoría no la instalaría).
- **D381 — Ni protocolo, ni migración, ni `formato` nuevos; versión 1.6.0** (decisión del analista). Todo lo nuevo es local de cada proceso o de la plataforma. La app Android no cambia.
- **D382 — Pruebas en CI con dobles y guion a mano en el plegable** (Hector, 11a). Lo de Termux se prueba en Linux con `MAGI_PLATAFORMA=termux` y los dobles de `tests/fijos/termux/`; el paquete, con su estructura y sus bibliotecas. Descartado Termux en el emulador de CI (lento, frágil y sin Termux:API).
- **D383 — Rutas y entorno de Termux, siempre del entorno** (decisión del analista): `$HOME`, `XDG_*`, `$TMPDIR` y `$PREFIX`; socket en `$TMPDIR/magi-<uid>/` sin `XDG_RUNTIME_DIR`; `sh` es `$PREFIX/bin/sh`; propiedades de Android con `__system_property_get`. Nunca desde el uid.
- **D384 — Órdenes de Termux con plazo, sin shell y con apagado por proceso** (decisión del analista): Termux:API es otra app y, si falta, sus órdenes se quedan colgadas; MAGI no espera más de `api_plazo_seg` y apaga la función hasta reiniciar el proceso (T139).
- **D385 — `magi diagnostico` con bloque TERMUX y marca `·`** (decisión del analista) para lo que no se puede comprobar desde Termux (procesos en segundo plano y batería): informa sin contar como aviso. El primer arranque ofrece la fila de teclas una vez y remite al diagnóstico.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Semanas |
|---|---|---|
| S1 — Plataforma, compilación y llavero | Prueba de concepto primero, en el plegable: el binario arranca y conecta, eventos de foco de Termux al cambiar de app, tope de OSC 52, el macro `^]` y la acción del aviso. Compilación para Android (`readelf`), `src/termux/` (rutas, órdenes con plazo, propiedades), socket en `$TMPDIR`, `$PREFIX/bin/sh`, `LlaveroFichero` en el núcleo y su elección en Termux, diálogos de la frase, `magi llavero`, REGISTRO. Pruebas `llavero_fichero` y `termux_ordenes`, y dobles de `tests/fijos/termux/` | 1 |
| S2 — Sincronización y sistema | Equipo `termux-<modelo>` escritor con plataforma `android`, carpeta con `realpath` y sondeo; portapapeles, enlaces y navegador; wake lock; avisos de Android (vigilante sin ventanas, pestañas con foco, ids en el estado); `magi termux aviso`. Pruebas `termux_sincronizacion` y `termux_servidor` | 1 |
| S3 — Teclas, diagnóstico, paquete y cierre | Fila de teclas (paleta, primer arranque y `magi termux teclas`), bloque TERMUX de `magi diagnostico`, `[termux]`, ayuda, instantáneas a 48×20; `scripts/paquete-termux.sh`, `termux-release.yml`, job `termux`, receta de TUR, `docs/publicar-termux.md` y README. Pruebas `termux_teclas`, `termux_ui` y `termux_paquete`; revisión adversarial; 1.6.0 y guion | 1 |

**Total: 3 sprints, ~3 semanas.** Supuestos: un desarrollador con Claude Code y dedicación parcial; el NDK r30 y `cargo-ndk` que ya usa la app; el plegable con Termux de F-Droid y Termux:API para la prueba de concepto y el guion; la F16 cerrada con su prompt de cierre. Si la prueba de concepto encuentra que el binario no arranca en Termux, se para y se replantea antes de seguir (R167).

**Guion de validación manual** (lo hace el desarrollador en el plegable, con Termux y Termux:API de F-Droid):

1. Instalar el `.deb` de una release candidata (`apt install ./magi-ssh_1.6.0-rc.1_aarch64.deb`) y comprobar `magi --version` y `magi diagnostico`.
2. Primer arranque: aceptar la fila de teclas; F1-F9, `^]`, Ctrl+P y las flechas funcionan; Volumen arriba + 1 da F1.
3. Unir el móvil a la sincronización pegando la clave, con la carpeta de Syncthing en `~/storage/shared`: crear la frase maestra; un host editado en el móvil llega al escritorio y uno editado en el escritorio llega al móvil.
4. Conectar a un host con contraseña y «recordar»; cerrar MAGI y volver: pide la frase y conecta sin pedir la contraseña.
5. Esc en LLAVERO DE MAGI: la conexión pide la contraseña y la sincronización dice que el llavero está bloqueado.
6. Copiar del modo historial y pegarlo en otra app; parar Termux:API y repetirlo con un texto corto (OSC 52) y con uno largo (aviso).
7. Abrir un enlace de la salida remota y uno a `localhost` del host (túnel temporal) en el navegador.
8. Abrir un escritorio (voxl o RDP) en el navegador; escribir con el teclado en pantalla; apagar la pantalla 2 minutos y volver.
9. Una sesión persistente con Claude Code en un host; pasar a otra app; que pida permiso: llega el aviso de Android en menos de un minuto; tocarlo trae Termux delante y el aviso desaparece al mirarla.
10. Con Android 12 o posterior, seguir la indicación de `magi diagnostico`; dejar un túnel abierto 30 minutos con la pantalla apagada y comprobar que sigue.
11. `magi llavero cambiar`, `magi termux aviso` y, en un equipo de prueba, `magi llavero borrar` y volver a unir.
12. El escritorio 1.6.0 y la app Android leen el fichero de sincronización de Termux sin «formato nuevo».

---

## 12. Conexiones con Otras Fases

**Dependencias**

- **F16:** su prompt de cierre va antes; el vigilante corre aquí también sin ventanas.
- **F15:** portapapeles por OSC 52, enlaces y túneles temporales, foco agregado y avisos de las pestañas.
- **F12:** el visor de escritorios y `[escritorio] navegador`.
- **F10:** la sincronización, en la que Termux es un escritor más, con sus limitaciones R83-R88 (la corrección 10b sigue pendiente).
- **F9:** `assets/`, `magi diagnostico`, la política de versiones y las releases de `dist`.
- **F11:** el NDK r30 y `cargo-ndk` del repositorio.
- **F7:** el modo estrecho y las instantáneas.
- **F3 y F2:** el servidor y su socket, `TrabajoServidor`, y el llavero con las contraseñas que se piden al cliente.

**Datos compartidos**

- EQUIPOS: un escritor con plataforma `android` (sin migración ni `formato` nuevo).
- REGISTRO: seis tipos nuevos.
- `config.toml`: sección `[termux]`.

**Lo que viene**

- **F18 (Accesos y certificados SSH)** no depende de esta fase; sus claves en el móvil irán por el agente de Termux.
- **F19 (Equipo)** puede apoyarse en que el móvil ya escribe desde Termux; si la app debe escribir, sigue siendo cosa suya.
- **El llavero de fichero** podrá usarse en Linux sin secret-tool (servidores sin escritorio) con un cambio pequeño.
- **Una fase futura del móvil** puede llevar a la app lo que aquí ya funciona en Termux.
