# MAGI - Fase 17: MAGI en Termux — Informe de implementación

> Documento vivo: se actualiza en cada sesión, nunca se regenera. Estado: **Tramo A hecho; parado en la puerta de la prueba de concepto (PoC)**. No hay código de la fase todavía: lo único tocado es documentación y reglas. Versión objetivo 1.6.0 (el árbol sigue en 1.5.0).
> Rama `fase17`. Entrega y reglas en el commit `51dfb5c`.

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

---

## 3. Estructura de archivos creada/modificada

Solo documentación y reglas; ningún fichero de código.

- `CLAUDE.md` (aplicados los cambios de la entrega y restauradas tres reglas) y `AGENTS.md` (igual que `CLAUDE.md`).
- `docs/fase17/`: `CLAUDE.md`, `magi-fase17-informe.md`, `magi-fase17-checklist.md`, `magi-fase17-prompt.md`, `magi-maestro.md` (la entrega) y este informe.
- Fuera de git: `target/poc-termux/` con el binario y la sonda para la PoC.

---

## 4. Decisiones técnicas tomadas durante el desarrollo

Del diseño previo al Sprint 1 (aprobado con el plan):

- **El llavero se inyecta, sin global.** Trait `LlaveroCliente: LlaveroSecreto + LlaveroHost` con `estado()`; `App.llavero: Arc<dyn LlaveroCliente>` y barrido mecánico de las ~29 construcciones de `LlaveroDelSistema` y llamadas libres. El servidor sigue sin tocarlo.
- **«Bloqueado» sin romper `magi-movil`:** constante `BLOQUEADO` y `es_bloqueado()` en `llavero.rs` del núcleo; `reconexion::clasificar` trata «llavero bloqueado» como `Credenciales` (si no, un texto sin «contraseña/frase/autentic» se reintentaría para siempre); el cliente reescribe en un solo sitio el motivo «sin contraseña guardada» cuando el llavero está bloqueado.
- **scrypt nunca en el hilo de UI** (`spawn_blocking` con evento de vuelta). `Recipient`/`Identity` de `age` miden scrypt al crearse: se fija siempre el factor (`set_work_factor(17)`, `set_max_work_factor(20)`).
- **Entorno de Termux inyectado** (`EntornoTermux`), sin leer `std::env` dentro de la lógica, para que las pruebas del mismo proceso no se pisen; `MAGI_PLATAFORMA` solo en depuración y solo para pruebas a nivel de binario.
- **Wake lock** como máquina de estados pura (probada con reloj sintético) más un conductor tokio alimentado por `!estado.trabajo().vacio()` desde `revisar_inactividad`; **avisos** con una única tarea reconciliadora (Notify + tick) que compara el conjunto deseado con los ids publicados; marca nueva `Sesion.atencion_remota`, porque `Sesion.aviso` mezcla atención remota con avisos internos del servidor.
- **Vigilante F16 sin ventanas:** predicado único `debe_vigilar`; no mantiene vivo al servidor (la spec dice que vive mientras haya trabajo).

---

## 5. Funcionalidades del checklist completadas (texto exacto)

Ninguna todavía (0 / 71). Estado de la primera, que **no se marca**:

- [ ] El paquete `magi-ssh` compila para `aarch64-linux-android` (API 24) con `cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh`, sin cambiar las dependencias del escritorio ni la app — *compila; falta el AC de `readelf -d` (§2.1)*.

---

## 6. Pendientes y bloqueos

- **PoC en el plegable: sin resultado todavía.** Hector decidió empezar el Sprint 1 sin esperar a la PoC. Hasta que la pase, las tres constantes que salen de ella llevan el valor prudente de la especificación y la marca `PROVISIONAL-POC`: `FOCO_FIABLE = false` (las pestañas avisan aunque estén delante y el aviso se retira al escribir, D379), `TOPE_OSC52 = 6000` (lo que dice §7.4) y el plazo de arranque del servidor, 2 s (sin cambios). Si la PoC contradice alguna, se cambia una sola constante. **Si el binario no arranca en Termux (R167), se para y se replantea todo lo que sigue.**
- **`dpkg-deb` no está instalado en esta máquina** (necesario para `scripts/paquete-termux.sh` y la prueba `termux_paquete`, Sprint 3): `sudo pacman -S dpkg`. No bloquea la puerta.
- `ANDROID_HOME`, `ANDROID_SDK_ROOT` y `ANDROID_NDK_HOME` no están en la shell por defecto: hay que exportarlas para `cargo ndk` (el NDK r30 está en `$HOME/Android/Sdk/ndk/30.0.16248370`).
- `deny.toml` solo audita los cuatro targets de escritorio; se añadirán los de Android en el Sprint 1.

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

### Después de la puerta

Se ejecutan los tres sprints del plan (contrato compilable → pistas paralelas → commit por sprint). Al cerrar cada uno se actualizan este informe y el checklist. Las órdenes de verificación por sprint son las de `CLAUDE.md` (`TZ=UTC cargo test --workspace --locked`, `clippy`, `fmt`, `deny`, y `shellcheck`/`actionlint`/`dist plan` si se tocan scripts o workflows).
