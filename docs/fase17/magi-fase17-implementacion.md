# MAGI - Fase 17: MAGI en Termux — Informe de implementación

> Documento vivo: se actualiza en cada sesión, nunca se regenera. Estado: **Sprints 1 y 2 hechos** (llavero, plataforma, sincronización, portapapeles, wake lock y avisos); **Sprint 3 por hacer** (teclado, diagnóstico, paquete, CI, TUR, README, versión 1.6.0 y revisión adversarial de cierre). La PoC en el plegable sigue sin pasar: se trabaja con las constantes que dependen de ella en su valor prudente (`PROVISIONAL-POC`, §6). Versión objetivo 1.6.0 (el árbol sigue en 1.5.0).
> Rama `fase17`. Entrega y reglas en `51dfb5c`; Sprint 1 en `73ed247` (contrato) y `b4b60a4`; Sprint 2 en `92e31bc` (contrato) y el commit del sprint.

---

## 1. Resumen de lo implementado

### Tramo A — Entrega, reglas y compilación para Android (previo al Sprint 1)

**Entrega y reglas (`51dfb5c`).** La entrega del analista se copió a `docs/fase17/` y se aplicó al `CLAUDE.md` de la raíz. La entrega trae el `CLAUDE.md` entero, no un parche, y **omitía tres reglas que siguen vigentes** en el código: la excepción RUSTSEC-2026-0495 (`deny.toml:85`), el `rust-embed` con `debug-embed` de `magi-nucleo` (`Cargo.toml` del núcleo, línea 40) y `flota::emitir_flota` del móvil (`crates/magi-movil/src/flota.rs`). Se restauraron en su sitio, sin tocar el resto del texto de la entrega, y `AGENTS.md` se dejó idéntico a `CLAUDE.md` (llevaba sin actualizarse desde antes de la Fase 16). No se crea `docs/magi-maestro.md`: por convención el maestro vive en `docs/faseN/`.

**Compilación para `aarch64-linux-android`.** Primera vez que se compila `magi-ssh` (TUI, servidor y CLI) para Android en este árbol; hasta ahora solo se había compilado `magi-movil`.

```
export ANDROID_HOME=$HOME/Android/Sdk ANDROID_SDK_ROOT=$HOME/Android/Sdk \
       ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/30.0.16248370
cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked
```

| Medida | Resultado |
|---|---|
| Compilación | Correcta **a la primera, sin cambiar una línea de código** y con `Cargo.lock` intacto (3 min 50 s en frío). Sin avisos del código propio; solo uno del `ironrdp-connector` vendorizado (`#[expect(single_use_lifetimes)]` no cumplido en `connection.rs:775`, que no se toca por la regla de `CLAUDE.md`) y el aviso de futura incompatibilidad de `proc-macro-error2`, de terceros |
| Binario | `target/aarch64-linux-android/release/magi`, ELF aarch64 PIE, intérprete `/system/bin/linker64`, 36,8 MB (el perfil `release` ya trae `strip = true`) |
| API mínima | 24 (nota `.note.android.ident`), NDK r30 |
| SHA-256 | `798a0bb6b64f809fe65dd37d911734fa6c39685e342e947917e793a8b3cf9080` |
| `readelf -d` (NEEDED) | `libdl.so`, **`libz.so`**, `libm.so`, `libc.so` |
| `getrandom` | Referencia débil sin resolver (no existe en API 24, llega en la 28): los crates caen a `/dev/urandom`. No impide enlazar |
| Símbolos propios de Android que usa | `getpwuid_r`/`getgrgid_r` (`LIBC_N`), `gethostname`, `flock`, `inotify_*` |

**Hallazgo: `libz.so`.** El binario pide `libz.so`, así que el criterio literal del checklist («`readelf -d` solo pide bibliotecas de bionic») **no se cumple**. Causa: `winscard` → `flate2` (feature `zlib`) → `libz-sys` 1.1.31, que en Android enlaza `libz` dinámico. `libz.so` es una biblioteca pública del NDK, presente en toda versión de Android, de modo que el binario funcionaría; pero no es lo que pide la spec. Ver §2, desviación 1.

**Sonda de scrypt (desechable, fuera del repositorio).** Para medir en el móvil lo que la spec da por supuesto (2^17 ≈ 128 MiB y «alrededor de un segundo») se compiló un programa de 60 líneas con el mismo `age` 0.11.5. Referencia en el PC de desarrollo: 2^15 → 90 ms y 36 MiB; 2^17 → 364 ms y 132 MiB; 2^18 → 733 ms y 260 MiB (2^20 serían ≈1 GiB).

### Sprint 1 — Plataforma, compilación y llavero (hecho)

**Método.** Contrato compilable en `73ed247` (trait `LlaveroCliente`, `App.llavero`, firmas de `LlaveroFichero`, `src/termux/*`, esqueleto de `magi llavero` y `magi termux`, 6 tipos de REGISTRO, `tests/fijos/termux/mod.rs`; ~29 sitios del escritorio barridos sin cambiar su comportamiento) → cuatro pistas en paralelo en worktrees aislados (núcleo, plataforma Termux, interfaz del llavero, CLI del llavero), cada una con una revisión adversarial de solo lectura → una ronda de correcciones (núcleo hecho aparte; CLI, interfaz y plataforma en tres pistas) → integración y verificación. Al cerrar el sprint no queda ninguna marca `CONTRATO-S1` ni `LEGADO-S1`.

**Núcleo — `LlaveroFichero` (`crates/magi-nucleo/src/llavero_fichero.rs`).** `llavero.age` (age con una sola entrada scrypt y dentro `{"formato":1,"entradas":{…}}`), 600, escrito siempre con `ficheros::escribir_atomico`. Se crea a 2^17 (`set_work_factor`); la cabecera se valida a mano antes de derivar nada (un `log_n` mayor que 20 es «dañado» sin gastar scrypt) y se abre con `set_max_work_factor(20)`; tope de 1 MiB; frase NFC de 12 a 1 024 bytes en `Zeroizing`. Cinco fallos seguidos imponen 30 s (reloj inyectable), `flock` en `llavero.age.lock` con plazo de 10 s y relectura (por SHA-256) antes de escribir, `apartar_danado` → `llavero.age.danado-<fecha>[-n]`, y «Bloquear» es definitivo aunque haya una operación de scrypt en vuelo. Dos cerrojos internos (`interno`, que se toma un instante, y `operacion`, para lo lento) hacen que `estado()` y `recuperar()` nunca esperen a scrypt. Alrededor: `ficheros::escribir_atomico` borra su temporal si falla (y existe `escribir_atomico_tolerante`, para el almacenamiento compartido del Sprint 2), `reconexion::clasificar` trata «llavero bloqueado» como `Credenciales` (no se reintenta), el núcleo expone `conexion::cliente::SIN_CONTRASENA_EN_LLAVERO` y `Cargo.toml` optimiza solo `scrypt` en depuración (`[profile.dev.package.scrypt] opt-level = 3`: crear pasa de 17 s a 0,4 s y la suite `llavero_fichero`, de 46 s a 10 s).

**Plataforma Termux (`src/termux/{mod,rutas,propiedades,ordenes}.rs`, `config.rs`, `conexion.rs`, `deliberacion`).** `EntornoTermux` inyectado (solo `del_proceso()` y `es_termux()` leen `std::env`; `MAGI_PLATAFORMA` solo vale en depuración), `rutas::calcular` (todo del entorno, nunca del uid; socket en `$TMPDIR/magi-<uid>/`, ~60 bytes con el `TMPDIR` de Termux; un `XDG_*` relativo se ignora), `propiedades` con `__system_property_get` vía `nix::libc` (solo `cfg(android)`), `Ordenes` sin shell (`PATH` inyectado, `process_group` + `killpg` al vencer el plazo, tope de 64 KiB por flujo, entrada en `Zeroizing`, `ApiProceso` apaga una función de Termux:API por proceso y devuelve «recién apagada» para avisar una sola vez), `termux::shell_local()` (`$PREFIX/bin/sh`), nombre del equipo `termux-<modelo>` y plataforma `android`. La deliberación local admite la shell como parámetro (`deliberacion::tests::ejecutar_con`), así que no hay copia del código del núcleo. `deny.toml` audita también `aarch64-linux-android` y `x86_64-linux-android`. Dobles de las órdenes de Termux: un único script, `tests/fijos/termux/falsos/doble`, instalado con el nombre de cada orden, con la API en `tests/fijos/termux/mod.rs`.

**Interfaz del llavero (`src/app/llavero.rs`, `src/ui/llavero.rs`, barra y paleta).** Diálogos LLAVERO DE MAGI, CREAR / CAMBIAR LA FRASE MAESTRA, BORRAR EL LLAVERO y LLAVERO DAÑADO con campos que se borran al caer; scrypt siempre en `spawn_blocking` con evento de vuelta (una respuesta sin petición en vuelo se descarta); continuaciones que se mueven (no se clonan) y una sola pregunta de frase aunque varias operaciones esperen el desbloqueo; marca «llavero bloqueado» en la barra (calculada en el tick, sin E/S por pintado); cuatro entradas de paleta solo con llavero de fichero (se activan por `de_fichero().is_some()`/`estado()`, nunca por plataforma); conexión interactiva (pestañas, escritorios, ficha de prueba) que pide antes la frase y, con Esc, la contraseña sin «recordar»; operaciones automáticas con error con instrucción y sin diálogo; crear, unir, mostrar clave, rotar, quitar y desactivar la sincronización con el llavero cerrado o sin crear; instantáneas a 48×20 de Flota, Hosts y Sesión y de los cinco diálogos a 40×12, 80×24, 200×60 y 48×20.

**CLI del llavero (`src/llavero_cli.rs`, `src/main.rs`).** `magi llavero estado|cambiar|borrar` (en Termux; fuera de él dicen por qué no aplican, código 1), frase en el TTY sin eco con modo raw restaurado por una guarda, `Consola` inyectable para probar sin terminal, y `magi sincronizar`/`magi sondear` que piden la frase con tres intentos si el llavero está cerrado (sin TTY, error con instrucción). `assets/` regenerado (man y autocompletado de bash, zsh y fish).

**Revisión adversarial del sprint (una por pista, solo lectura) y su resultado.** Sin hallazgos de severidad alta. **Corregidos** (con prueba de regresión): `magi sincronizar` pedía la frase dos veces; `magi llavero borrar` borraba un llavero dañado (§3.6 lo prohíbe; ahora lo rechaza también el núcleo); `magi sondear` abría el llavero aunque ningún host lo usara; la confirmación de `borrar` consumía lo tecleado por adelantado; la lectura de la frase giraba al 99 % de CPU si el terminal desaparecía; una frase vacía abortaba la orden; cualquier fallo de `Io` apagaba Termux:API (y la barra decía «no responde» sin ser verdad); `apartar_danado` renombraba un llavero sano que otra ventana acababa de restaurar y `borrar` borraba uno recién dañado; `usuario_local` cambiaba el comportamiento del escritorio; `tomar_cerrojo` confundía cualquier errno con «otra ventana lo usa»; «Bloquear» no era definitivo con scrypt en vuelo; una petición de contraseña de una pestaña con el llavero cerrado se perdía aunque el usuario desbloqueara dentro del plazo; credenciales de escritorios que ya no existían sobrevivían en memoria; la barra hacía E/S en cada pintado; la frase pegada quedaba en dos `String` sin borrar; tres pruebas de la interfaz no probaban lo que decían y una de plataforma pasaba en vacío en release; y una carrera preexistente (F14) en la prueba `snippets_movil_e2e` (esperaba `resultado` pero no `terminada_en`). **No corregidos, decididos o anotados** (§2 y §6): la lectura de `recuperar` entre ventanas, el sondeo con el llavero cerrado, las escrituras de equipos en el hilo de UI y los límites de crossterm y de las señales.

**Medidas.** `cargo ndk … build --release -p magi-ssh --locked` con el código del sprint: 3 min 25 s, 37 115 024 bytes, SHA-256 `00c49ac708924e1a8b9927c3e78ff51e0ac3bbd5e9c42bf60c49544ed5ba0b7f`, `readelf -d`: `libdl.so`, `libz.so`, `libm.so` y `libc.so` (la lista aceptada en §2.1). `git diff --stat android/` vacío.

### Sprint 2 — Sincronización y sistema (hecho)

**Método.** Contrato compilable en `92e31bc` (`[termux]` en `config.toml`, ficheros de estado de `src/termux/estado.rs` y `App.termux`; lo demás es interno de cada pista) → tres pistas en paralelo en worktrees aislados (servidor, sincronización, portapapeles y barra), cada una con su revisión adversarial de solo lectura → una ronda de correcciones en tres pistas y dos retoques de integración → verificación. Al cerrar el sprint no queda ninguna marca `CONTRATO-S2`.

**`[termux]` en `config.toml` (adelantado del Sprint 3).** `wake_lock`, `avisos`, `api_plazo_seg` y `sincronizacion_sondeo_seg` con sus rangos; se acota donde se usa y se avisa al arrancar solo en Termux. Fuera de Termux, con los valores de siempre, la sección no se escribe (y si alguien la escribió a mano —un `config.toml` compartido con su móvil—, se conserva al guardar). Se adelantó porque las tres pistas la consumen.

**Servidor (`src/termux/{wake_lock,avisos}.rs`, `src/servidor/android.rs`, `sesiones*.rs`).**
- **Wake lock:** `MaquinaWakeLock` pura (reloj sintético, sin tokio) y un conductor `WakeLock` alimentado cada segundo desde `revisar_inactividad` con `!trabajo().vacio()` (las ventanas no cuentan, y cuándo se apaga el servidor no cambia). Toma con `termux-wake-lock`, suelta 30 s después de quedar sin trabajo, marca `wake-lock` en el estado (antes de ejecutar la orden, para que un servidor que muera en medio deje rastro), `termux-wake-unlock` con tres reintentos, y suelta al terminar el servidor con plazo. Si no puede tomarlo: `AvisoServidor::SinWakeLock`; en Android 12 o posterior (SDK ≥ 31) y una sola vez en la vida del equipo (marca `segundo-plano`): `LimiteProcesos`.
- **Avisos de Android:** una tarea reconciliadora (Notify + tic de 3 s) compara lo deseado con lo publicado (estado `avisos`) y publica con `termux-notification --id magi-<id> --group magi --alert-once` o retira con `termux-notification-remove`; ids `magi-<uuid[:8]>` (fichas) y `magi-p<id>` (pestañas), validados; títulos «◆ nombre pide atención» / «! pestaña pide atención» y texto «host · MAGI», sin nada más; dedupe por (host, sesión de tmux); tope de 64; marca nueva `Sesion.atencion_remota` (solo BEL/OSC 9/OSC 777 del emulador, no los avisos internos), que se limpia al enfocar, al escribir y al quitar la sesión; `FOCO_FIABLE = false` (`PROVISIONAL-POC`: la PoC decide si Termux manda eventos de foco). Retira huérfanos al arrancar y todo al terminar, con plazo y a 4 órdenes a la vez como mucho.
- **Vigilante F16:** predicado único `debe_vigilar` (alguna ventana, o Termux con avisos, Termux:API no apagada y `termux-notification` en el `PATH`); no mantiene vivo al servidor.
- **Barra:** el servidor escribe `estado::escribir_servidor` (su `arranque` y los avisos nuevos) **antes** de crear el socket y lo quita al terminar. Inyección: `servidor::arrancar_con(rutas, config, Option<OpcionesTermux>)`; `arrancar` conserva su firma y en el escritorio el servidor es idéntico.
- **`magi termux aviso`** publica un aviso de prueba (`magi-test`) y lo retira a los 10 s.

**Sincronización (`src/app/sincronizacion.rs`, `src/sincronizacion/`, núcleo).** En Termux la carpeta se guarda resuelta con `realpath` (y un «permiso denegado» bajo `~/storage`, `/storage` o `/sdcard` remite a `termux-setup-storage`); no se usa `notify`: se sondea cada `[termux] sincronizacion_sondeo_seg` con una firma barata (nombre, tamaño y mtime en milisegundos de los ficheros de MAGI, listados con las reglas de la pasada y filtrando por nombre antes del `stat`), en un hilo con plazo de 10 s, y solo se pide pasada si la firma cambia, más una de seguridad cada 10 min y otra al volver a la vista Sincronización; mientras la carpeta no responda no se pide ninguna pasada (ni desde el sondeo ni desde `solicitar_pasada_con`). El fichero propio se escribe con `ficheros::escribir_atomico_tolerante` (un `chmod 600` que no se puede aplicar no es un error; una línea de log por sesión; fuera de Termux, estricto como siempre). Pruebas del checklist: con `android` inyectado, la deliberación, el sondeo y la sincronización dan lo mismo que con `linux` y una guarda de texto del código del núcleo y del móvil falla si alguien decide por la plataforma del equipo; el fichero de un equipo Termux lo leen el escritorio y el lector del móvil sin «formato nuevo»; «ABC-X10» → `termux-abc-x10`.

**Portapapeles, enlaces y barra (`src/termux/portapapeles.rs`, `src/app/{portapapeles,termux,escritorios,sesion}.rs`, `src/cliente/ventana.rs`).** Una sola vía de copia (`App::copiar`): en Termux, `termux-clipboard-set` por la entrada estándar (hasta 512 KiB, con plazo, en una tarea) → si no responde, la API se apaga en el cliente con un aviso y se sigue con OSC 52 (≤ 6 000 bytes codificados, escrito por el hilo de UI; `TOPE_OSC52`, `PROVISIONAL-POC`) → si no cabe, el mensaje de §4.4 con el tamaño y sin OSC 52; las copias van serializadas (una en vuelo y, como mucho, una pendiente: gana la última). En el escritorio las tres vías que se saltaban `copiar_texto` (clave pública, enlace del visor, clave generada) pasan por la misma `App::copiar` pero con su comportamiento de siempre (solo `wl-copy`/`pbcopy` y sus respaldos). `abrir_enlaces` vale `termux-open-url` en Termux (con las `Ordenes`, URL como único argumento, plazo y salida del fallo), `[escritorio] navegador = "auto"` abre el visor con `termux-open-url {url}`, `prefijo w` y la paleta explican cómo abrir otra sesión de Termux (como información, en los dos sitios). La barra dice, una vez por servidor y sin que la primera tecla la borre, cada aviso que el servidor deja en `termux/servidor`; el cliente lo lee cada 5 s, con plazo, solo en Termux y tras la `Bienvenida`.

**Revisión adversarial del sprint (una por pista, solo lectura) y su resultado.** Sin hallazgos de severidad alta. **Corregidos** (con prueba): wake lock que no se volvía a tomar con la marca de un servidor muerto (§7.5 tenía el mismo fallo); el vigilante sin ventanas se activaba sin el paquete `termux-api`; un aviso que vence el plazo perdía su id (y quedaba huérfano en Android); un ciclo completo del reconciliador por cada lote con campana; `leer_avisos` sin tope y retirada de decenas de avisos a la vez; el cliente leía el estado de un servidor muerto antes de que el nuevo lo reescribiera; la pasada de seguridad de 10 min congelaba la TUI con el almacenamiento compartido colgado; sondeo mudo si no se podía crear el hilo; `stat` de todo lo ajeno en una carpeta con miles de ficheros; **el escritorio dejaba de enseñar el diálogo de respaldo de la clave pública y el enlace visible** (OSC 52 colado en las tres vías unificadas); los avisos «una sola vez» desaparecían con la primera tecla; un FIFO en el sitio del estado colgaba la lectura; copias que se pisaban en el portapapeles; el diálogo de respaldo sustituía otro abierto; `prefijo w` salía rojo desde la paleta. **No corregidos, anotados** (§2 y §6): el coste de la pasada de F10 en el hilo de UI, el recorte de los mensajes a 48 columnas, la campana tocada al teclear con `FOCO_FIABLE = false`, y que `magi termux aviso` interrumpido deja su aviso hasta deslizarlo.

**Medidas.** `cargo ndk … build --release -p magi-ssh --locked` con el código de los dos sprints: 3 min 50 s, 37 745 120 bytes, SHA-256 `1ad049d38fcc5487ef2eb03fae1f411b7d4f4dad63b0c281c96ae593c4fe4004`, `readelf -d`: `libdl.so`, `libz.so`, `libm.so`, `libc.so`. `TZ=UTC cargo test --workspace --locked --no-fail-fast`: 81 binarios, 0 fallos. `git diff --stat android/` vacío; instantáneas existentes sin cambios (4 nuevas).

---

## 2. Desviaciones respecto a la especificación (qué y por qué)

Aprobadas con el plan (17 nov 2026); la 1 la decidió Hector después, al ver el `readelf`:

1. **`libz.so` en `readelf -d` (DECIDIDA: se acepta la alternativa a).** El plan preveía enlazar zlib estático solo en Android con la feature `static` de `libz-sys`. **No funciona**: en `libz-sys` 1.1.31 la rama de Android hace `return` con `rustc-link-lib=z` *antes* de mirar `static` (`build.rs`, líneas 25-30). Alternativas medidas:
   - **a)** Aceptar `libz.so` como biblioteca permitida (`libc libm libdl liblog libz`) y decirlo en el informe, como la spec hace con `libc++_shared.so`. No exige `Depends`: Android la trae siempre. Es la que menos toca.
   - **b)** Quitar la dependencia: `winscard` activa `flate2/zlib`; habría que parchear `winscard` (`[patch]`) o sustituir `sspi`. Invasivo, y la regla de `CLAUDE.md` sobre la copia vendorizada de IronRDP dice que no se toque.
   - **c)** Enlazar `libz.a` del NDK (existe en el sysroot) con un `build.rs` propio y con el orden de enlace adecuado; frágil y con efectos sobre las compilaciones de escritorio.
   - **d)** Feature `zlib-ng-no-cmake-experimental-community-maintained` de `libz-sys`: cambia la implementación de zlib y es «experimental».
   **Decisión de Hector (17 nov 2026): a).** El AC de `readelf -d` pasa a «solo bibliotecas del NDK estable (`libc`, `libm`, `libdl`, `liblog` y `libz`)»; el `.deb` sigue sin `Depends` porque `libz.so` viene en toda versión de Android. `termux-release.yml` y `termux_paquete` comprobarán esa lista, no la de «solo bionic» del texto del checklist. El texto del checklist no se reescribe (es la clave de trazabilidad): esta desviación es la que lo corrige.
2. **Aviso del servidor a la barra sin protocolo.** La spec pide que «la barra lo diga una vez» (Termux:API apagada en el servidor, sin wake lock, límite de procesos de Android 12+). El protocolo está congelado en v10 y `MensajeServidor::Error{peticion_id: None}` acaba en la vista Archivos. Se añade un quinto fichero de estado, `~/.local/state/magi/termux/servidor` (JSON mínimo), que el cliente mira cada ~5 s solo en Termux y que `magi diagnostico` también lee.
3. **`termux_paquete`.** GitHub pone `CI=true` en todos los jobs (también `tests-linux`, que nunca tendrá `.deb`), así que «con `CI` puesta falla si no hay `.deb`» rompería esa CI. Se usa `MAGI_EXIGIR_DEB=1`, solo en el job `termux`; sin ella y sin `MAGI_DEB`, la prueba se omite con aviso.
4. **Candidatas `-rc.N`.** El guion de validación las usa: `Version: 1.6.0~rc.1` en el `control` (con guion ordenaría *después* de `1.6.0`) y `termux-release.yml` con un segundo patrón de tag.
5. **`libc` no pasa a dependencia directa:** basta `nix::libc::__system_property_get` (la spec decía «dependencias: ninguna nueva»).
6. **Sondeo de la carpeta con firma barata** (nombre, tamaño, mtime) y **un único `App::copiar_texto`**: necesarios para «MAGI nunca se queda esperando» y para el tope de 6 000 bytes.
7. **Espera del socket de `conectar`** (2 s): se sube en Termux solo si la PoC mide un arranque en frío de más de ~1 s.

**Desviaciones nuevas del Sprint 1** (las pistas las anotaron; las que piden decisión del analista van marcadas **[analista]**):

8. **Dependencias y CI.** (a) `Cargo.lock`: `libz-sys` 1.1.31 → 1.1.33 por RUSTSEC-2026-0544 (desbordamiento de memoria en zlib; salió después de la Fase 16 y `cargo deny` fallaba en `main`), que además es la biblioteca que Android enlaza como `libz.so`; es un cambio de versión de parche también para el escritorio, no una dependencia nueva. (b) `deny.toml`: excepción RUSTSEC-2026-0543 para `chrono` (sin mantener, sin versión a la que subir, sin vulnerabilidad descrita; el aviso recomienda `jiff` o `time`, y cambiarlo toca núcleo, escritorio y móvil: se pospone a una fase de dependencias). Misma razón y misma forma que las de `pin-utils` y `rustls-pemfile`, sin fecha de caducidad en CI. **[analista]** confirmar esta excepción. (c) `Cargo.toml`: `[profile.dev.package.scrypt] opt-level = 3` (la spec no lo preveía; sin él, crear un llavero a 2^17 tarda 17 s en depuración y las pruebas, minutos). (d) El comando de `shellcheck` de `CLAUDE.md` y `AGENTS.md` pasa a `shellcheck packaging/termux/build.sh tests/fijos/termux/falsos/*`: el glob `tests/fijos/termux/*` de la spec incluye `mod.rs` y un directorio y no puede pasar. El informe de la spec (§11) conserva el glob antiguo.
9. **`LlaveroFichero.estado()` con la frase en memoria** devuelve `Abierto` aunque otra ventana haya reescrito el fichero (si la cabecera sigue siendo legible), en vez de `Cerrado`: con `Cerrado`, cada guardado de una ventana haría pedir la frase a todas las demás. Si la frase ya no vale se descubre en el siguiente `guardar`/`olvidar`, que no escribe, bloquea el objeto y lo dice. **Consecuencia que la revisión señaló y se mantiene** (la spec dice «`recuperar` solo de caché»): entre dos ventanas, una lectura puede ser vieja (otra ventana rotó la clave de sincronización y esta sigue exportando con la antigua hasta su siguiente escritura). **[analista]** valorar si `recuperar` debe comprobar la firma del fichero y pedir la frase de nuevo.
10. **`InvalidMac` con la frase buena es «dañado»**, no «frase incorrecta» (una frase equivocada falla antes, en la entrada scrypt). Una sal estropeada se ve como «frase incorrecta»: desde fuera no se distingue, el fichero no se toca ni se aparta solo, y el usuario puede apartarlo o borrarlo a mano.
11. **Un contenido ilegible con la frase buena** (MAC, flujo, JSON, `formato`) se recuerda por la firma del fichero y `estado()` lo da como `Danado`, para que la interfaz ofrezca apartarlo (con la cabecera sola no se ve). **`borrar()` no borra un llavero dañado** (`NoSeBorraDanado`) y **`apartar_danado()` no aparta uno que ya no lo está** (`NoEstaDanado`): ambos miran bajo el cerrojo el fichero que hay ahora y no el que vio un diálogo hace rato. (La spec lo pedía por el diagrama de estados de §3.6; el contrato no lo concretaba.)
12. **Detalles de `guardar`/`olvidar`.** Además de la firma (inodo, tamaño, fechas) se guarda un SHA-256 del fichero leído: decide si hay que releer, porque un inodo reutilizado o una fecha tosca podrían ocultar la escritura de otra ventana y perder una clave. `olvidar` sin fichero devuelve `Err(SIN_CREAR)` (contrato); la interfaz lo trata como «no había nada que olvidar» con un adaptador. Una frase corta (menos de 12) al abrir se prueba como cualquier otra (es «incorrecta», no «inválida»); vacía o de más de 1 024 bytes da «inválida» sin gastar scrypt ni contar como intento.
13. **Interfaz del llavero.** (a) Tras Esc en LLAVERO DE MAGI, el diálogo de contraseña conserva la casilla «recordar» visible pero desmarcada (la spec dice «sin recordar»; ocultarla cambia `Dialogo::ContrasenaServidor`, que usan muchas pruebas); si se marca y se pulsa ↵, vuelve a pedir la frase. (b) La reconexión automática de una pestaña con el llavero cerrado no se contesta con `Cerrar` (cerraría la pestaña que se reconecta): se deja vencer el plazo de 10 s del servidor y el cliente reescribe el motivo a «llavero bloqueado: desbloquéalo (Ctrl+P)» en un solo método; si el usuario desbloquea dentro del plazo, la petición recordada (con tope y vencimiento) se contesta. (c) Además de lo pedido: credenciales de escritorios (RDP/VNC) y prueba de ficha siguen el mismo patrón (frase antes, y con Esc se usa sin guardar); tras desbloquear se relanza la pasada de sincronización si había fallado por el llavero cerrado; «borrar» se acepta sin distinguir mayúsculas y con espacios (los teclados del móvil capitalizan). (d) **[analista]** con scrypt en marcha **Esc se ignora** y el pie dice «espera…» (antes Esc cerraba el diálogo pero el llavero se creaba o la frase cambiaba igualmente, contra «Esc al crear → no existe»); decidir si prefiere otra redacción. (e) La paleta no ofrece Cambiar ni Borrar con el llavero dañado o sin crear (borrar un dañado es inválido) ni «crear el llavero» explícitamente (solo se crea al guardar el primer secreto). (f) El detalle de `llavero_desbloqueo_fallido` es solo el número de intentos; el de `llavero_creado`, `llavero_frase_cambiada` y `llavero_borrado`, vacío.
14. **CLI.** `magi llavero estado` con el fichero dañado sale con 1; `borrar` sin llavero dice «no hay nada que borrar» y sale con 0 (idempotente); la confirmación es `borrar` en minúsculas con espacios alrededor; Esc no cancela la lectura (solo Ctrl+C y Ctrl+D); la fecha de `estado` va con desfase horario (Termux no siempre conoce su zona). Con tres intentos por proceso no se llega a la serie de cinco, así que la CLI no anota `llavero_desbloqueo_fallido`. `magi sondear` solo pide la frase si algún host sondeado (o de su cadena de saltos) usa `contrasena:llavero` (precisa «CLI que lo necesita», §4.2). La lectura oculta vigila el terminal con `poll(2)` sobre el fd 0 (crossterm 0.29 no devuelve error ni con EOF ni con EIO) y vacía lo tecleado por adelantado con `tcflush` y la cola de crossterm; usa `nix::libc` (con dos bloques `unsafe` acotados y comentados) porque `nix` no tiene la feature `term` en el escritorio.
15. **Plataforma.** (a) `ordenes::correr` reintenta hasta 5 veces con 20 ms si el programa está «ocupado» (ETXTBSY: actualización del paquete). (b) `Ordenes` añade `ejecutar_avisando` y `ejecutar_async_avisando`, que devuelven si esa llamada acaba de apagar Termux:API (con solo `ejecutar` no se puede dar el aviso único). (c) Un fallo de `Io` (no se puede lanzar, no se puede crear un hilo, un argumento con carácter nulo) **no** apaga Termux:API: la spec solo apaga por «no instalada», «plazo agotado» y «código distinto de 0». (d) `config::rutas_de_termux` y `usuario_local_con` existen para poder probar la rama de Termux sin el `OnceLock` de `es_termux()`. (e) `usuario_local` cae a `getpwuid_r` solo en Termux (consulta única, en caché) y allí una `USER` o `USERNAME` vacía cuenta como no puesta; en el escritorio queda exactamente como estaba (la primera variable puesta, aunque esté vacía; si no, `root`). La primera versión de la pista lo cambiaba también en el escritorio y la revisión lo señaló. (f) Un `ENOENT` al lanzar un guion con shebang a un intérprete que no existe se sigue viendo como «no instalada» y apaga con `Ausente`.
16. **Pruebas.** `tests/termux_plataforma.rs` (5 pruebas, `#![cfg(debug_assertions)]` porque `MAGI_PLATAFORMA` no vale en release) cubre la rama de Termux de `plataforma_actual`, `nombre_actual`, `descubrir_rutas` y el proveedor de CASPER-3; `es_termux()` es un `OnceLock` por proceso y no puede probarse junto a la rama de Linux. Las pruebas con pty usan el `script` de util-linux (sin él fallan con `CI` puesta y se omiten con aviso sin ella). `tests/termux_ui.rs` lleva un `#[global_allocator]` de todo su binario para comprobar que los búferes de la frase se ponen a cero al caer; está acotado (un búfer vigilado a la vez, con cerrojo). La prueba `snippets_movil_e2e` (F14, móvil) tenía una carrera preexistente (esperaba `resultado` y asertaba `terminada_en`, que se fija un instante después) y falló 1 de cada 3 veces; ahora espera las dos cosas.


**Desviaciones nuevas del Sprint 2** (**[analista]** = pide decisión):

17. **`[termux]` en `config.toml` pasa del Sprint 3 al 2** (las tres pistas la consumen). Se escribe siempre en Termux y, fuera de él, solo si no es la de siempre; los avisos de rango salen solo en Termux.
18. **El aviso del servidor a la barra va por un fichero** (`~/.local/state/magi/termux/servidor`, desviación 2 de §2): el servidor lo escribe antes de crear el socket (así un cliente nunca lee el de un servidor muerto) y lo quita al terminar; el cliente lo lee solo en Termux, tras la `Bienvenida`, cada 5 s y con plazo; se abre sin seguir enlaces y sin esperar (un FIFO no lo cuelga) y vale hasta 64 KiB. Un `AvisoServidor` desconocido invalida el fichero entero.
19. **Wake lock.** (a) **[analista]** Con la marca `wake-lock` de un servidor anterior y trabajo, el servidor **vuelve a ejecutar `termux-wake-lock`** (idempotente) en vez de darlo por tomado: tras un cierre de Android el wake lock real ya no existe y §7.5 lo dejaba sin tomar hasta el primer descanso. Sin trabajo se suelta como dice la spec. (b) La marca se pone antes de la orden y se quita si falla (en vez de después), y el id de un aviso se anota antes de publicarlo: si el servidor muere en medio queda rastro y el siguiente retira o suelta. (c) Con `wake_lock = "nunca"` y una marca previa, el servidor sí suelta ese wake lock (es de MAGI) pero nunca toma otro, como en el pseudocódigo; el brief decía «no se toca nada». (d) Un `termux-wake-unlock` que falla se reintenta cada gracia y, a los 3 fallos seguidos, se abandona dejando la marca para el siguiente servidor. (e) Los tiempos de prueba (gracia de 30 s y tic de 3 s) se inyectan en `OpcionesTermux`, no por una clave oculta de `config.toml`.
20. **Avisos de Android.** (a) La marca `atencion_remota` se pone si `!(FOCO_FIABLE && enfocada)`: con el valor prudente (`false`) avisa aunque una ventana la tenga enfocada y se retira al escribir (D379). (b) Un aviso ya publicado de una ficha de la que ahora no se sabe nada (host sin respuesta, sin comprobar, sin tmux) **se conserva**: no se retira una atención por no poder mirarla. **[analista]** (c) Tope de 64 avisos a la vez y de 256 ids al leer el estado. (d) Un aviso cuyo `termux-notification` vence el plazo **conserva su id** (pudo publicarse). (e) El vigilante sin ventanas exige que `termux-notification` esté en el `PATH` al arrancar el servidor: si se instala `termux-api` con el servidor ya en marcha, no se activa hasta el siguiente arranque. (f) `ApiApagada` en la barra se dice después de dejar el estado de `avisos` consistente.
21. **Sincronización.** (a) La firma barata reutiliza `CarpetaDisco::listar` del núcleo (mismas reglas que la pasada, mtime en milisegundos, ficheros de hasta 16 MiB); un cambio que deje igual nombre, tamaño y milisegundo solo lo recoge la pasada de seguridad. (b) La lectura de la firma va en un `std::thread` suelto con plazo de 10 s, no en `spawn_blocking` (uno colgado impediría cerrar el runtime). (c) El registro de lecturas colgadas es **estado global del proceso** (`LECTURAS_EN_VUELO`, por carpeta), porque pasar esa información a la `App` obligaba a cambiar el evento y `app.rs` fuera del territorio de la pista; el integrador añadió la consulta en `solicitar_pasada_con` y queda como mejora sustituirlo por estado en `EstadoSincronizacion`. (d) La tolerancia a `chmod` entra por funciones nuevas (`pasada_con_permisos`) sin cambiar `ResultadoPasada` ni `OpcionesPasada`; `ficheros::escribir_atomico_con` y `ajustar_permisos` son `#[doc(hidden)] pub` para poder inyectar el `chmod` en pruebas. (e) El error de «no se puede escribir en la carpeta» sale ya en el campo de la carpeta (como pide el diagrama de §4.3), no al pegar la clave, con una sonda `.magi-escritura-<uuid>` más. (f) Al volver a la vista no se pide pasada si hay borrados en pausa, el equipo está quitado o espera clave nueva (otra pasada no lo resolvería). (g) `EstadoSincronizacion` gana `pasadas_pedidas`, `sondeo_vigilancia` y `plazo_lectura_carpeta` (observación y pruebas).
22. **Portapapeles, enlaces y barra.** (a) El resolver del navegador está en `src/app/escritorios.rs`, no en `src/visor.rs` (que es el paginador de Archivos): ahí se añadió `Plataforma::Termux`. (b) En Termux los mensajes de éxito son los literales de §4.4 («copiado (3,2 KiB)») sin la etiqueta que lleva el escritorio, y el tope de copia de 1 MiB del historial no aplica (mandan 512 KiB y 6 000 bytes). (c) Un `abrir_enlaces` que sea una ruta (lleva `/`) se lanza como en el escritorio, porque `Ordenes` solo busca nombres. (d) **Añadidos sin estar en la spec:** «copiando…» mientras corre la orden, serialización de las copias (`COPIAS_A_LA_VEZ` pasa de 4 en paralelo a 1 en vuelo y 1 pendiente), el texto «sustituida por una copia más reciente» y un aviso «…; la línea saldrá al cerrar este diálogo» cuando el respaldo se apila. (e) `ventana::abrir` y `abrir_para` se quedan como envoltorios de `abrir_ventana`/`abrir_ventana_para` (enum `AperturaVentana`). (f) `ClaveGenerada.copiada` se conserva pero ahora siempre vale `false`: la `App` copia al terminar. (g) El aviso «Termux:API no responde» se usa igual si falta el paquete que si no responde, como dice la spec. **[analista]** valorar un texto aparte para «no instalada». (h) **[analista]** Los mensajes de §4.4 y la explicación de `prefijo w` miden 72-95 caracteres y a 48 columnas la barra los recorta con «…» (se pierde la alternativa): quedaron cuatro instantáneas a 48×20 que lo documentan; decidir variantes cortas o un mensaje de dos líneas. **[analista]** Una copia descartada de tipo clave pública abre el diálogo «CLAVE PÚBLICA» aunque la última salga bien; quizá suprimirlo si el texto de la sustituta es idéntico.
23. **Escritorio sin Termux.** Una sola vía `App::copiar` no cambia nada en el escritorio: las tres vías antiguas conservan sus respaldos (una primera versión de la pista las llevaba a OSC 52 y la revisión lo señaló).

---

## 3. Estructura de archivos creada/modificada

**Tramo A (solo documentación y reglas):**

- `CLAUDE.md` (aplicados los cambios de la entrega y restauradas tres reglas) y `AGENTS.md` (igual que `CLAUDE.md`).
- `docs/fase17/`: `CLAUDE.md`, `magi-fase17-informe.md`, `magi-fase17-checklist.md`, `magi-fase17-prompt.md`, `magi-maestro.md` (la entrega) y este informe.
- Fuera de git: `target/poc-termux/` con el binario y la sonda para la PoC.

**Sprint 1 (39 ficheros tocados; todo bajo `android/` queda sin tocar):**

- **Núcleo:** `crates/magi-nucleo/src/llavero_fichero.rs` (~1 200 líneas), `llavero.rs` (`BLOQUEADO`, `es_bloqueado`), `ficheros.rs` (temporal borrado al fallar, `escribir_atomico_tolerante`), `conexion/reconexion.rs`, `conexion/cliente.rs` (`SIN_CONTRASENA_EN_LLAVERO`), `registro.rs` (6 tipos, `TIPOS` 50 → 56), `deliberacion/tests.rs` (`ejecutar_con`); pruebas `tests/llavero_fichero.rs` (51), `tests/reconexion.rs` (+1) y `tests/sesiones_persistentes.rs` (contadores 50 → 56).
- **Plataforma:** `src/termux/{mod,rutas,propiedades,ordenes}.rs` (el resto de `src/termux/*` sigue en esqueleto hasta los Sprints 2 y 3), `src/config.rs`, `src/conexion.rs`, `src/deliberacion/mod.rs`, `deny.toml`, `Cargo.toml` y `Cargo.lock`; pruebas `tests/termux_ordenes.rs` (39), `tests/termux_plataforma.rs` (5) y `tests/fijos/termux/{mod.rs,falsos/doble}`.
- **Interfaz del llavero:** `src/llavero.rs` (`LlaveroCliente`, `EstadoLlavero`, `del_equipo`), `src/app.rs` (campo `llavero`), `src/app/llavero.rs` (nuevo, ~2 000 líneas), `src/app/{escritorios,sesion,sincronizacion}.rs`, `src/flota/mod.rs`, `src/ui/{llavero,barra,dialogos,disposicion,mod}.rs`; pruebas `tests/termux_ui.rs` (73) e instantáneas `tests/redimensionado/vistas_{g,p}.rs` + `tests/snapshots/llavero_*` y `sesion_48x20`, `flota_48x20`, `hosts_48x20`.
- **CLI:** `src/llavero_cli.rs` (nuevo), `src/main.rs`, `assets/` regenerado; pruebas `tests/llavero_cli.rs` (55).
- **Otros:** `CHANGELOG.md` («Sin publicar»), `crates/magi-movil/tests/snippets_movil_e2e.rs` (carrera preexistente), `CLAUDE.md` y `AGENTS.md` (comando de `shellcheck`).

**Sprint 2 (36 ficheros tocados; `android/` sin tocar):**

- **Configuración y estado:** `src/config.rs` (`SeccionTermux`), `src/termux/estado.rs` (nuevo: marcas, ids de avisos, fichero `servidor`), `src/app/termux.rs` (nuevo: `EstadoTermuxApp`, lectura del estado del servidor y cola de la barra).
- **Servidor:** `src/termux/wake_lock.rs` (máquina pura y conductor), `src/termux/avisos.rs` (ids, textos, planificación, `FOCO_FIABLE`), `src/termux/cli.rs` (solo `magi termux aviso`), `src/servidor/android.rs` (nuevo: `OpcionesTermux`, `BarraServidor`, `TermuxServidor`, reconciliador y cierre), `src/servidor/{mod,sesiones,sesiones_persistentes,escritura_varios}.rs`; pruebas `tests/termux_servidor.rs` (42) y `tests/comun/mod.rs` (`termux` opcional en el escenario).
- **Sincronización:** `src/app/sincronizacion.rs` (sondeo y registro de lecturas colgadas), `src/sincronizacion/{mod,almacenamiento,equipos}.rs`, `crates/magi-nucleo/src/sincronizacion/{carpeta,mod}.rs`, `crates/magi-nucleo/src/ficheros.rs`, `src/app.rs` (carpeta, vigilancia, pasada); pruebas `tests/termux_sincronizacion.rs` (45).
- **Portapapeles y enlaces:** `src/termux/portapapeles.rs`, `src/app/{portapapeles,escritorios,sesion}.rs`, `src/portapapeles.rs`, `src/identidades.rs`, `src/cliente/ventana.rs`, `src/app.rs` (paleta y teclas); pruebas `tests/termux_portapapeles.rs` (45) e instantáneas `barra_mensaje_*_48x20` (4).
- **Otros:** `README.md` (`abrir_enlaces` en Termux), `CLAUDE.md` y `AGENTS.md` (`--test termux_portapapeles`), `CHANGELOG.md`.

---

## 4. Decisiones técnicas tomadas durante el desarrollo

Del diseño previo al Sprint 1 (aprobado con el plan):

- **El llavero se inyecta, sin global.** Trait `LlaveroCliente: LlaveroSecreto + LlaveroHost` con `estado()`; `App.llavero: Arc<dyn LlaveroCliente>` y barrido mecánico de las ~29 construcciones de `LlaveroDelSistema` y llamadas libres. El servidor sigue sin tocarlo.
- **«Bloqueado» sin romper `magi-movil`:** constante `BLOQUEADO` y `es_bloqueado()` en `llavero.rs` del núcleo; `reconexion::clasificar` trata «llavero bloqueado» como `Credenciales` (si no, un texto sin «contraseña/frase/autentic» se reintentaría para siempre); el cliente reescribe en un solo sitio el motivo «sin contraseña guardada» cuando el llavero está bloqueado.
- **scrypt nunca en el hilo de UI** (`spawn_blocking` con evento de vuelta). `Recipient`/`Identity` de `age` miden scrypt al crearse: se fija siempre el factor (`set_work_factor(17)`, `set_max_work_factor(20)`).
- **Entorno de Termux inyectado** (`EntornoTermux`), sin leer `std::env` dentro de la lógica, para que las pruebas del mismo proceso no se pisen; `MAGI_PLATAFORMA` solo en depuración y solo para pruebas a nivel de binario.
- **Wake lock** como máquina de estados pura (probada con reloj sintético) más un conductor tokio alimentado por `!estado.trabajo().vacio()` desde `revisar_inactividad`; **avisos** con una única tarea reconciliadora (Notify + tick) que compara el conjunto deseado con los ids publicados; marca nueva `Sesion.atencion_remota`, porque `Sesion.aviso` mezcla atención remota con avisos internos del servidor.
- **Vigilante F16 sin ventanas:** predicado único `debe_vigilar`; no mantiene vivo al servidor (la spec dice que vive mientras haya trabajo).

Tomadas durante el Sprint 1:

- **El núcleo nunca espera a scrypt desde la interfaz:** `estado()`, `recuperar()` y `bloquear()` son baratos; `crear`, `desbloquear`, `cambiar_frase` y los `guardar`/`olvidar` que reescriben son de `spawn_blocking`. Excepciones que la spec acepta y siguen en el hilo de UI (~1 s con scrypt a 2^17 en un móvil): `equipos::{crear, unir, quitar, rotar, desactivar}`, guardar la contraseña de un escritorio desde la ficha y olvidar una contraseña; `rotar` y `quitar` hacen dos reescrituras seguidas y, si otra ventana tiene el `flock`, esperan hasta 10 s más. Se miden en el plegable (§6).
- **Un diálogo no se fía de lo que pintó:** al confirmar apartar o borrar, el núcleo recomprueba el fichero bajo el cerrojo (`NoEstaDanado`, `NoSeBorraDanado`) y la interfaz decide qué hacer con la respuesta (volver a evaluar `estado()`, sin renombrar nunca un llavero sano). Misma regla que las peticiones de escritorio: se comprueba que la sesión sigue viva antes de continuar.
- **El estado del llavero se activa por lo que es, no por dónde corre:** `de_fichero()`/`estado()`; ninguna rama de la interfaz pregunta por la plataforma (la paleta del llavero solo aparece con llavero de fichero, y el escritorio con `LlaveroDelSistema` es byte a byte el de antes: instantáneas existentes sin cambios).
- **Reescrito único del motivo «sin contraseña guardada» → «llavero bloqueado»** en `normalizar_motivos_llavero`, apoyado en la constante del núcleo (si alguien cambia el texto del núcleo, la prueba de la interfaz lo ve).
- **Procesos de Termux:API:** tres hilos auxiliares por orden (escritor, lector de stdout y de stderr, con tope de 64 KiB y el resto descartado); al vencer el plazo se mata el grupo entero con el líder aún sin recoger (su pid no se puede reutilizar) y se recoge esperando hasta 2 s; un nieto fuera de grupo (`setsid`) que retenga las tuberías no bloquea al llamante.
- **Lectura de la frase en la CLI:** modo raw antes de escribir la pregunta, guarda `Drop`, búfer `Zeroizing` de capacidad fija (nunca se reasigna) y comprobación con `poll(2)` de que el terminal sigue vivo.

Tomadas durante el Sprint 2:

- **Lo que habla con Android nunca espera con el cerrojo del estado en la mano:** ni en el servidor (el reconciliador copia lo que necesita, suelta el cerrojo y luego ejecuta; solo el cerrojo propio `publicados` se mantiene durante las órdenes y solo lo disputan el ciclo, la purga y el cierre) ni en el cliente (todo en tareas con evento de vuelta; el OSC 52 lo escribe el hilo de UI).
- **Máquinas puras con reloj inyectado** para el wake lock y la planificación de avisos (lo deseado frente a lo publicado): la lógica se prueba sin tokio ni órdenes y los tiempos reales se sustituyen por inyectados en las pruebas de integración.
- **El servidor no decide por lo que ve en `std::env`:** `arrancar_con` recibe `Option<OpcionesTermux>` (entorno, `Ordenes`, tiempos); `arrancar` pasa el del proceso solo en Termux.
- **Entrada no fiable también en los ficheros de estado de MAGI:** ids de aviso validados antes de pasarlos a una orden, tope de lectura, apertura sin seguir enlaces ni esperar, y un fichero ilegible es «sin estado», nunca un pánico.
- **Una firma barata de la carpeta no es la pasada:** el sondeo decide si pedirla; la pasada de F10 sigue siendo la que lee, hace SHA-256 e importa, en el bucle de UI (riesgo anotado en §6).
- **Un fallo de lectura nunca se parece a una ausencia** (regla de CLAUDE.md) aplicado a los avisos (se conservan los de lo que no se puede comprobar), a la carpeta colgada (no se pide pasada) y a Termux:API (un fallo local de `Io` no la apaga).

---

## 5. Funcionalidades del checklist completadas (texto exacto)

**46 / 71** tras el Sprint 2. Todas se han comprobado sin móvil (pruebas, compilación para Android o lectura del código); lo que solo se ve en el plegable queda en §6 y en el guion. Observaciones: la de compilación se da por cumplida con la lista de bibliotecas aceptada en §2.1 (`libdl`, `libz`, `libm`, `libc`); la de «Modelo, versión y SDK…» está implementada y compila para Android, pero su lectura real va al guion; la de «Rutas en Termux…» se comprueba con el servidor arrancado de verdad en `tests/termux_ordenes.rs`; la de «Modo Termux elegido al compilar» se comprobó además a mano con el binario `--release` de Linux; la de «Una pestaña delante pero sin foco…» está completa para `FOCO_FIABLE = false` y su rama `true` solo en la lógica pura (la PoC decide); «`[termux]` en `config.toml`» se hizo en el Sprint 2 en vez del 3; y la de «Órdenes de Termux con el plazo…» está completa en cliente y servidor (el diagnóstico las usa en el Sprint 3).

**Compilación y plataforma**

- [x] El paquete `magi-ssh` compila para `aarch64-linux-android` (API 24) con `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh`, sin cambiar las dependencias del escritorio ni la app
  - AC: Dado el workspace, cuando se compila para Android, entonces sale `target/aarch64-linux-android/release/magi` y `readelf -d` solo pide bibliotecas de bionic (`libc`, `libm`, `libdl` y, si alguna dependencia lo usa, `liblog`)
- [x] Modo Termux elegido al compilar (`cfg(target_os = "android")`) y, solo en compilaciones de depuración, con `MAGI_PLATAFORMA=termux` para las pruebas
  - AC: Dado un binario de release para Linux con `MAGI_PLATAFORMA=termux`, cuando arranca, entonces se porta como en Linux
- [x] Rutas en Termux desde `$HOME`, `XDG_*`, `$TMPDIR` y `$PREFIX`, nunca desde el uid: datos, configuración y estado como en Linux, y socket en `$TMPDIR/magi-<uid>/servidor.sock` (directorio 700) cuando no hay `XDG_RUNTIME_DIR`
  - AC: Dado `XDG_RUNTIME_DIR` sin poner y el `TMPDIR` de Termux, cuando arranca el servidor, entonces el socket queda en `$TMPDIR/magi-<uid>/` con una ruta de menos de 100 bytes
- [x] Lo local que va con shell (las comprobaciones propias de la deliberación, F14) usa `$PREFIX/bin/sh` en Termux; ninguna ruta fija a `/bin/sh` ni a `/usr/bin` en el código local del escritorio
- [x] Modelo, versión y SDK de Android con `__system_property_get` de bionic, sin lanzar `getprop`
- [x] El núcleo no decide nada por `EQUIPOS.plataforma`: con `android` en un equipo escritor, la deliberación, el sondeo y la sincronización se portan como en el escritorio
  - AC: Dadas las pruebas de deliberación, sondeo y sincronización con la plataforma `android` inyectada en el escritorio, cuando corren, entonces dan lo mismo que con `linux`

**Llavero con frase maestra**

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

**Sincronización**

- [x] En Termux, el equipo se llama por defecto `termux-<modelo>` (`ro.product.model` con la regla de la etiqueta de la F15, 20 caracteres como mucho) y su plataforma es `android`, sin `formato` nuevo
  - AC: Dado un móvil con el modelo «ABC-X10», cuando se crea o se une la sincronización, entonces el equipo se llama `termux-abc-x10`
- [x] Termux sincroniza como un equipo escritor: escribe su `magi-<uuid>.age` y lee los de los demás, como el escritorio
  - AC: Dado el fichero escrito por Termux, cuando lo importan un escritorio 1.5.0 y la app Android, entonces lo leen sin «formato nuevo»
- [x] La carpeta de sincronización se guarda resuelta con `realpath` (`~/storage/shared/…` → `/storage/emulated/0/…`); sin `~/storage`, la instrucción de ejecutar `termux-setup-storage`
- [x] La carpeta se sondea cada `[termux] sincronizacion_sondeo_seg` (30 s) con MAGI abierto, en vez de usar `notify`, y también al volver a la vista Sincronización
- [x] Los permisos 600 que el almacenamiento compartido no admite no son un error (una línea en el log por sesión)
- [x] Unir este equipo pegando la clave en el diálogo (pegado entre corchetes de Termux); la clave va al llavero de fichero, que se crea si no existe

**Portapapeles, enlaces y escritorios**

- [x] Copiar en Termux: `termux-clipboard-set` por la entrada estándar (hasta 512 KiB) si Termux:API responde; si no, OSC 52 hasta 6 000 bytes codificados; si no cabe, aviso con el tamaño y la alternativa
  - AC: Dado Termux:API parada y una selección de 20 KiB, cuando se copia, entonces la barra dice «no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo» y no se manda OSC 52
- [x] `[terminal] abrir_enlaces` vale `termux-open-url` por defecto en Termux, con la URL como único argumento y sin shell
- [x] `[escritorio] navegador = "auto"` abre el visor con `termux-open-url {url}` en Termux; el visor sigue solo en `127.0.0.1` (T98)
- [x] `prefijo w` en Termux explica cómo abrir otra sesión de Termux con `magi` en vez de lanzar un terminal

**Segundo plano**

- [x] `[termux] wake_lock = "auto"`: el servidor toma el wake lock cuando `TrabajoServidor` deja de estar vacío y lo suelta 30 s después de quedar vacío; con `nunca`, no lo toca
  - AC: Dado un túnel que se abre y se cierra, cuando pasan 30 s sin trabajo, entonces el servidor ha ejecutado `termux-wake-lock` y `termux-wake-unlock` una vez cada una
- [x] Solo se suelta el wake lock que tomó MAGI; si el servidor muere sin soltarlo, el siguiente lo suelta en cuanto no tiene trabajo (marca `wake-lock` en el estado)
- [x] En Android 12 o posterior, un aviso único sobre el límite de procesos en segundo plano que remite a `magi diagnostico`

**Avisos de Android**

- [x] Con `[termux] avisos = true` y Termux:API, el servidor publica un aviso de Android cuando una sesión persistente (vigilante, F16) o una pestaña (marca `!`, F15) pide atención sin nadie mirando, con id `magi-<id>`, grupo `magi` y `--alert-once`
  - AC: Dada una sesión persistente que toca la campana con la ventana de MAGI cerrada y su pestaña viva en el servidor, cuando el vigilante lo ve, entonces hay un aviso «◆ agente-api pide atención» con «dev-01 · MAGI» y ningún otro texto
- [x] El aviso lleva solo el nombre y el host (T140) y una acción fija que trae Termux delante; nunca el título, el texto del aviso ni la salida
- [x] El aviso se retira cuando se quita la atención, se cierra la pestaña o se borra la ficha; al terminar el servidor, todos los suyos; los de un servidor muerto, al arrancar el siguiente (ids en el estado)
- [x] En Termux con avisos, el vigilante corre también sin ventanas conectadas mientras viva el servidor (precisa D334)
- [x] Una pestaña delante pero sin foco (Termux en segundo plano) cuenta como sin mirar; si Termux no manda eventos de foco (prueba de concepto), el aviso sale igual y se retira al escribir en la pestaña

**CLI, configuración y diagnóstico**

- [x] `[termux]` en `config.toml` (`wake_lock`, `avisos`, `api_plazo_seg` de 2 a 30 y `sincronizacion_sondeo_seg` de 10 a 600): fuera de rango se acota con aviso; fuera de Termux se ignora
- [x] Órdenes de Termux con el plazo `[termux] api_plazo_seg` y sin shell; si Termux:API no responde, la función se apaga en ese proceso con un aviso y MAGI nunca se queda esperando
  - AC: Dado un `termux-clipboard-set` que no responde, cuando se copia, entonces a los 5 s se copia por OSC 52 y la barra lo dice una sola vez

**API: CLI y órdenes de Termux**

- [x] `magi llavero estado`: si hay llavero, ruta, tamaño y fecha; nunca pide la frase ni enseña entradas
- [x] `magi llavero cambiar` y `magi llavero borrar`, con la frase y la confirmación en el TTY
- [x] `magi termux aviso`: publica un aviso de prueba y lo retira a los 10 s
- [x] `termux-clipboard-set` (por la entrada estándar) y `termux-open-url` desde el cliente
- [x] `termux-wake-lock` y `termux-wake-unlock` desde el servidor
- [x] `termux-notification` y `termux-notification-remove` desde el servidor, con ids `magi-<id>`

**Calidad**

- [x] Pruebas `llavero_fichero` del núcleo: crear, abrir, frase incorrecta, dañado, factor de trabajo acotado, NFC y escrituras concurrentes con `flock`

Siguen sin marcar, a propósito, las del Sprint 3 y la PoC: `src/termux/` completo (teclas, diagnóstico y CLI de teclas), la fila de teclas y `termux.properties`, la ayuda y el primer arranque, las instantáneas y la paleta con la fila de teclas, `magi termux teclas`, `magi diagnostico`, `termux-reload-settings` y `termux-battery-status`, los tipos de REGISTRO con su etiqueta (`termux_teclas_instaladas` se emite en el Sprint 3), el paquete, la CI, TUR y el README, las pruebas que faltan (`termux_teclas`, `termux_paquete`), la revisión adversarial de cierre y la versión 1.6.0.

---

## 6. Pendientes y bloqueos

- **PoC en el plegable: sin resultado todavía.** Hector decidió empezar el Sprint 1 sin esperar a la PoC. Hasta que la pase, las tres constantes que salen de ella llevan el valor prudente de la especificación y la marca `PROVISIONAL-POC`: `FOCO_FIABLE = false` (las pestañas avisan aunque estén delante y el aviso se retira al escribir, D379), `TOPE_OSC52 = 6000` (lo que dice §7.4) y el plazo de arranque del servidor, 2 s (sin cambios). Si la PoC contradice alguna, se cambia una sola constante. **Si el binario no arranca en Termux (R167), se para y se replantea todo lo que sigue.**
- **`dpkg-deb` no está instalado en esta máquina** (necesario para `scripts/paquete-termux.sh` y la prueba `termux_paquete`, Sprint 3): `sudo pacman -S dpkg`. No bloquea la puerta.
- `ANDROID_HOME`, `ANDROID_SDK_ROOT` y `ANDROID_NDK_HOME` no están en la shell por defecto: hay que exportarlas para `cargo ndk` (el NDK r30 está en `$HOME/Android/Sdk/ndk/30.0.16248370`).
- ~~`deny.toml` solo audita los cuatro targets de escritorio~~: hecho en el Sprint 1 (también `aarch64-linux-android` y `x86_64-linux-android`); no añadieron fallos de licencias, bans ni fuentes.

**Pendientes y límites que deja el Sprint 1** (todo lo marcado en el checklist se ha probado sin móvil; lo de abajo solo se ve en el plegable y va al guion):

- **Nada del código `cfg(android)` se ha ejecutado:** `__system_property_get` (modelo, versión, SDK) y las llamadas `libc` de la CLI compilan para `aarch64-linux-android` (`cargo ndk build` y `check`) pero no se han corrido; `flock`, el `rename` atómico y los permisos 600 se asumen en el almacenamiento interno de Termux como en Linux; la semántica de `termux-*` real, el teclado táctil, el pegado entre corchetes de Termux, el diálogo a 48×20 real y la espera de 30 s con reloj real quedan para el guion. Los scripts doble se probaron con el `sh` de esta máquina (bash), no con `dash`.
- **Abrir un llavero creado con el factor de producción 2^20 (~1 GiB) no se ha probado** (solo que la cabecera con 2^20 se acepta sin derivar y que >20 se rechaza). En un móvil, medirlo en el guion.
- **Copias de la frase que no se pueden borrar:** los búferes internos de crossterm al leer un pegado (1 KiB en el lector y el `Vec` del parser), los temporales de `serde_json` con escapes y los iteradores de NFC dejan restos en memoria que el código no controla. Se reduce todo lo posible; no se garantiza.
- **SIGTERM o SIGHUP** mientras la CLI pide la frase matan el proceso con el terminal en modo raw (sin eco) hasta un `reset` o `stty sane`; no se añaden manejadores de señales. Queda además una carrera de microsegundos (el terminal desaparece justo entre nuestro `poll` y el de crossterm) en la que vuelve a girar.
- **Sondeo con el llavero cerrado [analista]:** un host de contraseña de llavero sale CAÍDA y anota `sondeo_fallido` (y `sondeo_recuperado` al desbloquear) aunque el host esté bien; no se relanza el sondeo al desbloquear. Propuesta: tratar el fallo por `BLOQUEADO` como «sin comprobar», sin REGISTRO, y relanzar tras desbloquear.
- **Otros [analista]:** con SERVIDOR CAÍDO un diálogo del llavero que solo existía para una pregunta del servidor queda apilado y vacío; una pestaña Caída vuelve a enseñar «no hay ninguna guardada…» en la siguiente difusión del servidor aunque el llavero ya esté abierto; `quitar` un equipo comprueba `estado()` antes de borrar el fichero del otro equipo y, si otra ventana cambió la frase, falla después de borrarlo (la pasada lo reescribe); la ficha de escritorio con el llavero cerrado muestra «sin contraseña guardada» y, al guardar, cierra y descarta lo tecleado (rehacer `guardar_ficha` con `ficha_guardada_a_medias`); el pegado de otros diálogos con frase o contraseña (huella, servidor…) sigue sin borrar la `String` del evento (anterior a la fase).
- **Las pruebas con pty** (`llavero_cli`) dependen de `script` de util-linux y de un margen de 1 s entre teclear y arrancar el binario: en un equipo muy cargado podrían dar un falso fallo, nunca un falso éxito. La de «no se puede lanzar» de `termux_ordenes` se omite con aviso si corre como root.

**Pendientes y límites que deja el Sprint 2** (todo lo marcado se ha probado sin móvil, con los dobles de `tests/fijos/termux/`; lo siguiente solo se ve en el plegable y va al guion):

- **Nada de Termux:API real se ha ejecutado:** la semántica de `termux-wake-lock` (¿es idempotente tras un cierre de Android?), `termux-notification` con `--action`, el permiso de notificaciones de Android 13+ (para eso es `magi termux aviso`), `termux-clipboard-set` y `termux-open-url` con latencias reales, el visor en Chrome de Android, la escritura real del OSC 52 en la terminal y que Android no cierre el servidor igualmente.
- **Almacenamiento compartido real:** `chmod` sobre `/storage/emulated/0` (FUSE/sdcardfs), `realpath` de `~/storage/shared`, la granularidad del mtime, el coste de `read_dir` y la creación de hilos bajo los límites de Android. Hay que probar una carpeta que no responde.
- **Foco (D379):** con `FOCO_FIABLE = false`, una campana que el remoto toca en respuesta a lo que se acaba de teclear (el autocompletado de bash) publica un aviso de Android hasta la siguiente pulsación, y con `--alert-once` vuelve a sonar tras cada retirada. La PoC decide si Termux manda eventos de foco; la rama `true` no tiene prueba de extremo a extremo.
- **La pasada de F10 sigue en el hilo de UI y sin plazo [analista]:** lee la carpeta y hace SHA-256 de todo; el sondeo evita lanzarla sin cambios y se aplaza mientras la carpeta no responde, pero una pasada ya empezada, la inicial, `exportar_al_salir` o una pedida a mano sobre un almacenamiento colgado pueden congelar la TUI. Candidata a una mejora: hash por metadatos antes de leer y pasada en `spawn_blocking`.
- **Una ficha sin pestaña no avisa con la ventana cerrada** cuando el servidor ya se apagó por inactividad (vive solo mientras haya trabajo): riesgo ya anotado en la spec. El vigilante sin ventanas solo corre mientras viva el servidor.
- **`magi termux aviso` interrumpido** (Ctrl+C o SIGHUP) durante los 10 s deja el aviso `magi-test` hasta que se deslice a mano; no se añaden manejadores de señales.
- **Los avisos de Termux:API no instalada y apagada comparten texto [analista].** Un `termux-api` instalado con el servidor en marcha no activa el vigilante sin ventanas hasta el siguiente arranque.
- **Las pruebas de tiempos** (`termux_servidor`, `termux_sincronizacion`, `termux_portapapeles`) usan tiempos cortos inyectados y márgenes de ≥ 0,8 s: solo pueden fallar en la dirección segura (nada emitido); la de control positivo de `notify` necesita inotify en el CI.

**Riesgos que se mantienen como dice la spec:** apertura de `llavero.age` con scrypt de hasta 2^20 (≈1 GiB; en un móvil el *low memory killer* podría cerrar Termux entero, servidor incluido, si alguien forja el fichero); escrituras de `equipos::crear/unir` con scrypt (~1 s) en el hilo de UI mientras la PoC mida ≤ 1,5 s; una ficha de sesión persistente sin pestaña no avisa con la ventana cerrada (el servidor muere a los 10 s sin trabajo).

---

## 7. Ejecución y pruebas

**Compilar para Termux:** el bloque de §1. **Comprobar bibliotecas:** `readelf -d target/aarch64-linux-android/release/magi | grep NEEDED`.

### Guion de la prueba de concepto (lo haces tú en el plegable)

Todo en Termux de F-Droid o GitHub (no el de Google Play). Pega la salida de cada bloque en la respuesta; no hace falta que el resultado sea bueno, hace falta saberlo.

**P0 · Pasar los ficheros.** En el PC, desde `target/poc-termux/` (tiene `magi`, `sonda_scrypt` y `SHA256SUMS`):

```
python -m http.server 8000          # en el PC
```
```
cd ~ && curl -O http://<IP-del-PC>:8000/magi -O http://<IP-del-PC>:8000/sonda_scrypt -O http://<IP-del-PC>:8000/SHA256SUMS
sha256sum -c SHA256SUMS && chmod 700 magi sonda_scrypt
```
(Alternativa con `adb`: `adb push magi sonda_scrypt SHA256SUMS /sdcard/Download/` y en Termux `cp ~/storage/downloads/{magi,sonda_scrypt,SHA256SUMS} ~/`. El binario tiene que ejecutarse desde `$HOME`, no desde `/sdcard`.)

**P1 · Entorno.**
```
echo "HOME=$HOME PREFIX=$PREFIX TMPDIR=$TMPDIR USER=${USER-<sin definir>} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR-<sin definir>} TERMUX_VERSION=${TERMUX_VERSION-?}"
getprop ro.product.model; getprop ro.build.version.release; getprop ro.build.version.sdk; uname -m
ls -ld /bin/sh /usr/bin /tmp 2>&1
```

**P2 · ¿Arranca y conecta?** (la puerta)
```
./magi --version
./magi diagnostico                       # sin XDG_RUNTIME_DIR: se espera un error por /tmp/magi-<uid>
export XDG_RUNTIME_DIR="$TMPDIR"
./magi diagnostico
./magi                                   # añade un host real con clave, conecta, escribe, sal con el prefijo
```

**P3 · Arranque en frío del servidor** (la espera de `conectar` es de 2 s).
```
./magi servidor parar; time ./magi sesiones
./magi servidor parar; time ./magi sesiones
```

**P4 · Eventos de foco.** `printf '\e[?1004h'; cat -v` y cambia a otra app y vuelve. Si salen `^[[O` (pierde foco) y `^[[I` (gana foco), Termux los manda. Sal con Ctrl+C y `printf '\e[?1004l'`.

**P5 · Tope real de OSC 52.** Pega cada vez en el propio Termux (pulsación larga → Pegar) y mira cuántos caracteres llegan:
```
for n in 3000 4500 6000 8000 12000; do
  printf '\e]52;c;%s\a' "$(head -c "$n" /dev/zero | tr '\0' x | base64 -w0)"
  echo "n=$n (base64=$((4*(n+2)/3))). Pega y pulsa Intro:"; read -r x; echo "llegaron ${#x} caracteres"
done
```

**P6 · Macro `^]`.** Copia de seguridad y fila de prueba:
```
mkdir -p ~/.termux && cp -n ~/.termux/termux.properties ~/.termux/termux.properties.antes-poc 2>/dev/null
cat >> ~/.termux/termux.properties <<'EOF'
extra-keys = [['ESC','TAB','CTRL','ALT',{macro: 'CTRL ]', display: '^]'},'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]
EOF
termux-reload-settings
cat -v
```
Pulsa `^]` de la fila (debe salir `^]`). Para deshacer: `mv ~/.termux/termux.properties.antes-poc ~/.termux/termux.properties` (o borra la última línea si no existía el fichero).

**P7 · Termux:API** (necesita la app Termux:API y `pkg install termux-api`).
```
time termux-battery-status
echo prueba | termux-clipboard-set
termux-notification --id magi-poc --group magi --alert-once -t "◆ agente-api pide atención" -c "dev-01 · MAGI" \
  --action 'am start -n com.termux/com.termux.app.TermuxActivity'
```
Pasa a otra app, toca el aviso y di si Termux vuelve delante. Luego `termux-notification-remove magi-poc`. Con Android 13 o posterior, comprueba que Termux:API tiene permiso de notificaciones.

**P8 · scrypt (tiempo y memoria).**
```
./sonda_scrypt 15 17 18
```
(No pidas 20: son ≈1 GiB y Android podría cerrar Termux.) Con esto se decide si las escrituras de `crear/unir` pueden ir en el hilo de UI (≤ 1,5 s) o van en dos fases.

**P9 · Almacenamiento compartido.**
```
termux-setup-storage          # una vez, acepta el permiso
D=~/storage/shared/magi-poc; mkdir -p "$D"; realpath "$D"
touch "$D/a"; chmod 600 "$D/a"; echo "chmod rc=$?"; ls -l "$D/a"
```
Opcional (`pkg install inotify-tools`): `inotifywait -m -e create,modify "$D" &` y, desde otra app o con `adb shell touch /sdcard/magi-poc/desde-otra-app`, di si llega el evento.

**Qué pega cada medida en el diseño:** P2 decide si seguimos; P3, el plazo de `conectar`; P4, `FOCO_FIABLE` (D379); P5, `TOPE_OSC52`; P6 y P7, la fila de teclas y la acción del aviso; P8, el reparto del trabajo de scrypt; P9, la tolerancia de `chmod` y el sondeo en vez de `notify`; P1, el nombre del equipo y el usuario (¿existe `$USER`?).

### Pruebas del Sprint 1 (sin móvil)

```
cargo test -p magi-nucleo --test llavero_fichero --test reconexion
TZ=UTC cargo test -p magi-ssh --test termux_ordenes --test termux_plataforma --test termux_ui \
  --test llavero_cli --test recursos --test redimensionado
TZ=UTC cargo test --workspace --locked --no-fail-fast        # todo, sin parar al primer fallo
cargo clippy --workspace --all-targets --locked -- -D warnings ; cargo fmt --all --check ; cargo deny check
```

Sprint 2: `TZ=UTC cargo test -p magi-ssh --test termux_servidor --test termux_sincronizacion --test termux_portapapeles` (42, 45 y 45 pruebas; las de tiempo reales de `termux_servidor` tardan unos 12 s) y `cargo test -p magi-ssh --lib termux:: servidor::`.

`termux_ordenes` y `termux_plataforma` también pasan con `MAGI_PLATAFORMA=termux` exportado (el checklist lo pide así). Para el binario de Termux: el bloque de compilación de §1 (con `ANDROID_HOME`, `ANDROID_SDK_ROOT` y `ANDROID_NDK_HOME` exportadas) y `readelf -d`.

### Después de la puerta

Se ejecutan los tres sprints del plan (contrato compilable → pistas paralelas → commit por sprint). Al cerrar cada uno se actualizan este informe y el checklist. Las órdenes de verificación por sprint son las de `CLAUDE.md` (`TZ=UTC cargo test --workspace --locked`, `clippy`, `fmt`, `deny`, y `shellcheck`/`actionlint`/`dist plan` si se tocan scripts o workflows).
