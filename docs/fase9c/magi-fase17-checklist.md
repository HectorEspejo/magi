# MAGI - Checklist Fase 17: MAGI en Termux

## Compilación y plataforma
- [x] El paquete `magi-ssh` compila para `aarch64-linux-android` (API 24) con `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh`, sin cambiar las dependencias del escritorio ni la app
  - AC: Dado el workspace, cuando se compila para Android, entonces sale `target/aarch64-linux-android/release/magi` y `readelf -d` solo pide bibliotecas de bionic (`libc`, `libm`, `libdl` y, si alguna dependencia lo usa, `liblog`)
  - **Precisión (19 nov 2026, D403):** `readelf -d` pide también `libz.so`, que Android trae siempre; la lista aceptada es `libc`, `libm`, `libdl`, `liblog` y `libz` (decisión de Hector, 17 nov)
- [x] Modo Termux elegido al compilar (`cfg(target_os = "android")`) y, solo en compilaciones de depuración, con `MAGI_PLATAFORMA=termux` para las pruebas
  - AC: Dado un binario de release para Linux con `MAGI_PLATAFORMA=termux`, cuando arranca, entonces se porta como en Linux
- [x] Lo propio de Termux en `src/termux/` (`mod`, `rutas`, `ordenes`, `portapapeles`, `wake_lock`, `avisos`, `teclas`, `diagnostico` y `cli`); fuera de ahí, solo llamadas a su interfaz
- [x] Rutas en Termux desde `$HOME`, `XDG_*`, `$TMPDIR` y `$PREFIX`, nunca desde el uid: datos, configuración y estado como en Linux, y socket en `$TMPDIR/magi-<uid>/servidor.sock` (directorio 700) cuando no hay `XDG_RUNTIME_DIR`
  - AC: Dado `XDG_RUNTIME_DIR` sin poner y el `TMPDIR` de Termux, cuando arranca el servidor, entonces el socket queda en `$TMPDIR/magi-<uid>/` con una ruta de menos de 100 bytes
- [x] Lo local que va con shell (las comprobaciones propias de la deliberación, F14) usa `$PREFIX/bin/sh` en Termux; ninguna ruta fija a `/bin/sh` ni a `/usr/bin` en el código local del escritorio
- [x] Modelo, versión y SDK de Android con `__system_property_get` de bionic, sin lanzar `getprop`
- [x] El núcleo no decide nada por `EQUIPOS.plataforma`: con `android` en un equipo escritor, la deliberación, el sondeo y la sincronización se portan como en el escritorio
  - AC: Dadas las pruebas de deliberación, sondeo y sincronización con la plataforma `android` inyectada en el escritorio, cuando corren, entonces dan lo mismo que con `linux`

## Instalación y publicación
- [ ] `scripts/paquete-termux.sh <versión>` arma `magi-ssh_<versión>_aarch64.deb` con `dpkg-deb -Zxz --root-owner-group`: el binario (700), la página man, el autocompletado y las licencias bajo `/data/data/com.termux/files/usr/`
  - AC: Dado el binario para Android, cuando se ejecuta el script, entonces `dpkg-deb --contents` lista exactamente esas rutas y `control` lleva `Architecture: aarch64`, `Suggests: termux-api, openssh` y ningún `Depends`
- [ ] `termux-release.yml`: con un tag `vX.Y.Z` compila, arma y comprueba el `.deb`, añade su `.sha256` y la atestación de procedencia, y lo sube a la release del tag cuando exista (espera hasta 60 min), solo con `GITHUB_TOKEN`; con `workflow_dispatch` repite la subida para un tag existente
  - AC: Dado un tag `android-v0.3.0`, cuando se empuja, entonces `termux-release.yml` no se dispara
- [ ] Job `termux` en `ci.yml`, en las PR que tocan `src/`, `crates/magi-nucleo/`, `Cargo.toml`, `Cargo.lock`, el empaquetado de Termux o el propio workflow: compila para aarch64 (y x86_64 como artefacto, sin publicar), arma el `.deb` y pasa `termux_paquete`
- [x] Receta de TUR en `packaging/termux/build.sh` (`termux_setup_rust`, `TERMUX_PKG_EXCLUDED_ARCHES="arm, i686"`, fuente del tag con su SHA-256, man y autocompletado), limpia con `shellcheck`
- [x] `docs/publicar-termux.md`: cómo sale el `.deb`, cómo se comprueba en el móvil y cómo se propone y se actualiza la receta en TUR (la PR la abre el desarrollador)
- [x] README: sección «MAGI en Termux» con la instalación (`apt install ./magi-ssh_<versión>_aarch64.deb` o TUR), Termux:API, `termux-setup-storage`, la fila de teclas y el límite de procesos de Android 12 o posterior
  - **Precisión (18 nov 2026):** con la 9b en `main`, el README solo da la instalación y enlaza; lo demás va en el manual, en `docs/manual/termux.md`

## Llavero con frase maestra
- [x] `LlaveroFichero` en `crates/magi-nucleo/src/llavero_fichero.rs`, detrás de `LlaveroSecreto`: `llavero.age` en el directorio de datos (600, por `ficheros.rs`), age con una entrada scrypt (2^17 al crear, hasta 2^20 al abrir), JSON con `formato` 1 y tope de 1 MiB
  - AC: Dado un llavero creado con una frase, cuando se abre con otra, entonces da «frase incorrecta» y el fichero no cambia
- [x] En Termux, el llavero de MAGI es `LlaveroFichero`; secret-tool, Keychain y `LlaveroMovil` no cambian
- [x] La frase maestra vive solo en la memoria del cliente (`Zeroizing`, normalizada a NFC) hasta que se cierra la ventana o se bloquea el llavero; nunca va a disco, al log, a REGISTRO ni al servidor
- [x] Diálogo CREAR LA FRASE MAESTRA cuando hace falta guardar el primer secreto (recordar una contraseña, crear o unir la sincronización): dos campos, al menos 12 caracteres y el aviso de que no se recupera
  - AC: Dado que no hay `llavero.age`, cuando se marca «recordar» al conectar y se pulsa Esc en CREAR LA FRASE MAESTRA, entonces la contraseña se usa sin guardarse y no se crea ningún fichero
- [x] Diálogo LLAVERO DE MAGI al abrir MAGI con `llavero.age`; Esc sigue sin él y la barra dice «llavero bloqueado» cuando algo lo necesita
- [x] Con el llavero bloqueado, una conexión interactiva que lo necesita pide antes la frase (y, con Esc, la contraseña sin «recordar»); una operación automática da error con instrucción, nunca un diálogo
  - AC: Dado el llavero bloqueado y la reconexión automática de un host con `contrasena:llavero`, cuando el servidor pide la contraseña, entonces la pestaña dice «llavero bloqueado: desbloquéalo (Ctrl+P)» y no se abre ningún diálogo
- [x] Cinco frases incorrectas seguidas imponen 30 s de espera y anotan `llavero_desbloqueo_fallido` (una línea por serie, con el número de intentos)
- [x] Diálogo CAMBIAR LA FRASE MAESTRA: la actual y la nueva dos veces; recifra con escritura atómica bajo el cerrojo y anota `llavero_frase_cambiada`
- [x] Diálogo BORRAR EL LLAVERO: explica qué se pierde (las contraseñas guardadas y la clave de sincronización de este equipo), pide escribir `borrar` y anota `llavero_borrado`
- [x] Diálogo LLAVERO DAÑADO (cabecera o contenido ilegibles): apartarlo como `llavero.age.danado-<fecha>` y seguir sin llavero, o salir sin tocar nada; nunca se borra solo; anota `llavero_apartado`
- [x] Dos ventanas de MAGI en el mismo Termux: cada una pide la frase; las escrituras van con `flock` y releen el fichero antes de escribir
  - AC: Dadas dos ventanas desbloqueadas que guardan a la vez las contraseñas de dos hosts distintos, cuando terminan, entonces `llavero.age` tiene las dos
- [x] La CLI que necesita el llavero (`magi sincronizar` y las demás) pide la frase en el TTY; sin TTY, error con instrucción
- [x] «Bloquear el llavero» desde la paleta borra de la memoria la frase y las entradas

## Sincronización
- [x] En Termux, el equipo se llama por defecto `termux-<modelo>` (`ro.product.model` con la regla de la etiqueta de la F15, 20 caracteres como mucho) y su plataforma es `android`, sin `formato` nuevo
  - AC: Dado un móvil con el modelo «ABC-X10», cuando se crea o se une la sincronización, entonces el equipo se llama `termux-abc-x10`
- [x] Termux sincroniza como un equipo escritor: escribe su `magi-<uuid>.age` y lee los de los demás, como el escritorio
  - AC: Dado el fichero escrito por Termux, cuando lo importan un escritorio 1.5.0 y la app Android, entonces lo leen sin «formato nuevo»
- [x] La carpeta de sincronización se guarda resuelta con `realpath` (`~/storage/shared/…` → `/storage/emulated/0/…`); sin `~/storage`, la instrucción de ejecutar `termux-setup-storage`
- [x] La carpeta se sondea cada `[termux] sincronizacion_sondeo_seg` (30 s) con MAGI abierto, en vez de usar `notify`, y también al volver a la vista Sincronización
- [x] Los permisos 600 que el almacenamiento compartido no admite no son un error (una línea en el log por sesión)
- [x] Unir este equipo pegando la clave en el diálogo (pegado entre corchetes de Termux); la clave va al llavero de fichero, que se crea si no existe

## Portapapeles, enlaces y escritorios
- [x] Copiar en Termux: `termux-clipboard-set` por la entrada estándar (hasta 512 KiB) si Termux:API responde; si no, OSC 52 hasta 6 000 bytes codificados; si no cabe, aviso con el tamaño y la alternativa
  - AC: Dado Termux:API parada y una selección de 20 KiB, cuando se copia, entonces la barra dice «no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo» y no se manda OSC 52
- [x] `[terminal] abrir_enlaces` vale `termux-open-url` por defecto en Termux, con la URL como único argumento y sin shell
- [x] `[escritorio] navegador = "auto"` abre el visor con `termux-open-url {url}` en Termux; el visor sigue solo en `127.0.0.1` (T98)
- [x] `prefijo w` en Termux explica cómo abrir otra sesión de Termux con `magi` en vez de lanzar un terminal

## Segundo plano
- [x] `[termux] wake_lock = "auto"`: el servidor toma el wake lock cuando `TrabajoServidor` deja de estar vacío y lo suelta 30 s después de quedar vacío; con `nunca`, no lo toca
  - AC: Dado un túnel que se abre y se cierra, cuando pasan 30 s sin trabajo, entonces el servidor ha ejecutado `termux-wake-lock` y `termux-wake-unlock` una vez cada una
- [x] Solo se suelta el wake lock que tomó MAGI; si el servidor muere sin soltarlo, el siguiente lo suelta en cuanto no tiene trabajo (marca `wake-lock` en el estado)
- [x] En Android 12 o posterior, un aviso único sobre el límite de procesos en segundo plano que remite a `magi diagnostico`

## Avisos de Android
- [x] Con `[termux] avisos = true` y Termux:API, el servidor publica un aviso de Android cuando una sesión persistente (vigilante, F16) o una pestaña (marca `!`, F15) pide atención sin nadie mirando, con id `magi-<id>`, grupo `magi` y `--alert-once`
  - AC: Dada una sesión persistente que toca la campana con la ventana de MAGI cerrada y su pestaña viva en el servidor, cuando el vigilante lo ve, entonces hay un aviso «◆ agente-api pide atención» con «dev-01 · MAGI» y ningún otro texto
- [x] El aviso lleva solo el nombre y el host (T140) y una acción fija que trae Termux delante; nunca el título, el texto del aviso ni la salida
- [x] El aviso se retira cuando se quita la atención, se cierra la pestaña o se borra la ficha; al terminar el servidor, todos los suyos; los de un servidor muerto, al arrancar el siguiente (ids en el estado)
- [x] En Termux con avisos, el vigilante corre también sin ventanas conectadas mientras viva el servidor (precisa D334)
- [x] Una pestaña delante pero sin foco (Termux en segundo plano) cuenta como sin mirar; si Termux no manda eventos de foco (prueba de concepto), el aviso sale igual y se retira al escribir en la pestaña

## Teclado
- [x] Fila de teclas generada de la tabla de MAGI: `ESC TAB CTRL ALT ^] ← ↓ ↑ →` y `F1`-`F9`, con el prefijo configurado como macro (`{macro: 'CTRL ]', display: '^]'}` por defecto)
- [x] Instalar la fila (paleta, primer arranque o `magi termux teclas --instalar`): confirmación, copia `termux.properties.magi-<fecha>`, escritura atómica de solo la línea lógica `extra-keys` y `termux-reload-settings`; anota `termux_teclas_instaladas`
  - AC: Dado un `termux.properties` con comentarios, otras claves y un `extra-keys` partido en tres líneas con `\`, cuando se instala la fila, entonces solo cambia esa línea lógica y el resto queda byte a byte
- [x] Sin fichero, se crea `~/.termux/termux.properties`; si el que existe es `~/.config/termux/termux.properties`, se usa ese; si la fila ya es la de MAGI, «ya está», sin copia ni escritura
  - **Precisión (19 nov 2026, D415):** si existen los dos ficheros, manda `~/.termux/termux.properties` (el orden en que Termux los busca)
- [x] La ayuda (`?`) en Termux recuerda Volumen arriba + 1-9 (F1-F9) y + 0 (F10)

## Interfaz en el móvil
- [x] Primer arranque en Termux: ofrece la fila de teclas una vez (marca en el estado) y la barra remite a `magi diagnostico`
- [x] Instantáneas a 48×20 de Flota, Hosts, Sesión y los diálogos nuevos (LLAVERO DE MAGI, CREAR LA FRASE MAESTRA, CAMBIAR LA FRASE MAESTRA, BORRAR EL LLAVERO, LLAVERO DAÑADO y FILA DE TECLAS DE TERMUX), además de las de 40×12, 80×24 y 200×60 (T45)
- [x] Paleta en Termux: «Desbloquear el llavero», «Bloquear el llavero», «Cambiar la frase maestra», «Borrar el llavero» e «Instalar la fila de teclas de Termux»; fuera de Termux no salen

## CLI, configuración y diagnóstico
- [x] `magi llavero` y `magi termux` en la CLI, la página man y el autocompletado (`assets/` regenerado); fuera de Termux dicen por qué no aplican, con código 1
- [x] `[termux]` en `config.toml` (`wake_lock`, `avisos`, `api_plazo_seg` de 2 a 30 y `sincronizacion_sondeo_seg` de 10 a 600): fuera de rango se acota con aviso; fuera de Termux se ignora
- [x] `magi diagnostico` en Termux: bloque TERMUX (versiones de Termux y Android, Termux:API, almacenamiento, fila de teclas, wake lock y avisos), las líneas generales adaptadas (runtime, llavero sin pedir la frase, portapapeles, terminal) y lo que no se puede comprobar (procesos en segundo plano, batería) con `·`, sin contar como aviso
  - AC: Dado el Termux de Google Play, cuando se ejecuta `magi diagnostico`, entonces la línea de Termux sale con `!` y recomienda el de F-Droid o GitHub
- [x] Órdenes de Termux con el plazo `[termux] api_plazo_seg` y sin shell; si Termux:API no responde, la función se apaga en ese proceso con un aviso y MAGI nunca se queda esperando
  - AC: Dado un `termux-clipboard-set` que no responde, cuando se copia, entonces a los 5 s se copia por OSC 52 y la barra lo dice una sola vez

## REGISTRO
- [x] Tipos nuevos `llavero_creado`, `llavero_frase_cambiada`, `llavero_borrado`, `llavero_desbloqueo_fallido`, `llavero_apartado` y `termux_teclas_instaladas`, sin frases ni secretos, con su etiqueta en la vista Registro

## API: CLI y órdenes de Termux
- [x] `magi llavero estado`: si hay llavero, ruta, tamaño y fecha; nunca pide la frase ni enseña entradas
- [x] `magi llavero cambiar` y `magi llavero borrar`, con la frase y la confirmación en el TTY
- [x] `magi termux teclas [--instalar]`: sin `--instalar`, enseña la fila propuesta y la actual sin escribir
- [x] `magi termux aviso`: publica un aviso de prueba y lo retira a los 10 s
- [x] `termux-clipboard-set` (por la entrada estándar) y `termux-open-url` desde el cliente
- [x] `termux-wake-lock` y `termux-wake-unlock` desde el servidor
- [x] `termux-notification` y `termux-notification-remove` desde el servidor, con ids `magi-<id>`
- [x] `termux-reload-settings` al instalar la fila y `termux-battery-status` en `magi diagnostico`

## Calidad
- [ ] Prueba de concepto en el plegable al empezar: el binario arranca y conecta, eventos de foco al cambiar de app, tope de OSC 52, el macro `^]` y la acción del aviso; el resultado, en el informe de implementación
- [x] Pruebas `llavero_fichero` del núcleo: crear, abrir, frase incorrecta, dañado, factor de trabajo acotado, NFC y escrituras concurrentes con `flock`
- [x] Dobles de `tests/fijos/termux/` (anotan argumentos y entrada, y saben colgarse o fallar) y pruebas `termux_ordenes`, `termux_ui`, `termux_servidor`, `termux_sincronizacion` y `termux_teclas` con `MAGI_PLATAFORMA=termux`
- [ ] Prueba `termux_paquete`: rutas de `dpkg-deb --contents`, el `control` y `readelf -d` del `.deb` armado (en CI falla si no hay `.deb`; fuera, se omite con aviso)
  - **Precisión (19 nov 2026, D405):** el `.deb` lo exige `MAGI_EXIGIR_DEB=1`, solo en el job `termux`, y no `CI` (GitHub la pone en todos los jobs)
- [x] El escritorio y la app no cambian: todas las pruebas anteriores en verde, las instantáneas de redimensionado sin cambios y `android/` sin tocar
- [x] Revisión adversarial de cierre (T31, T54) del llavero, las órdenes de Termux, la escritura de `termux.properties`, los avisos y el empaquetado, con su informe en `docs/fase17/magi-fase17-revision-adversarial.md`
- [x] Versión 1.6.0: `CHANGELOG.md` («Sin publicar»), `FECHA_DE_VERSION`, la DA2 de `tui_prueba` y las instantáneas `servidor_anterior_*`, y el guion de validación en el plegable en el informe de implementación

---

**Progreso Fase 17:** 66 / 71 funcionalidades

**Total MAGI (Fases 1-17, 9b y 9c):** 1432 / 1520 funcionalidades
