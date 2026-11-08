# Visor web de MAGI

Cliente web del visor de escritorio remoto de la Fase 12. Se sirve desde
`magi --servidor` solo en `127.0.0.1` y compila MAGI sin Node: los ficheros de
`visor/dist/` van versionados y el binario los incrusta con
`include_bytes!`/`include_str!`.

## Origen y licencia

Está adaptado del cliente web de **voxl 0.3.3** (<https://github.com/tembo/voxl>),
concretamente de su carpeta `web/` (`src/protocol.ts`, `src/worker.ts`,
`src/client/*.ts`, `index.html` y sus estilos), publicado con licencia MIT:

- Aviso original en [LICENSE-voxl](LICENSE-voxl): `MIT License`,
  `Copyright (c) 2026 Tembo`.
- Se conserva el aviso MIT en ese fichero y se ha mantenido el espíritu y buena
  parte de la estructura del cliente: el WebSocket y el decodificador viven en
  un `worker` con `OffscreenCanvas`; la página se queda con la entrada; las
  órdenes al servidor son cadenas cortas separadas por comas y los mensajes
  binarios llevan un byte de tipo.

Los cambios respecto a voxl 0.3.3 son los de abajo. No hay recursos externos:
ni fuentes, ni iconos, ni imágenes remotas, ni CDN (la CSP del servidor es
estricta y usa `script-src 'self'` y `style-src 'self'`).

## Estructura

| Ruta                          | Qué es                                                                                                                                                                                                                             |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `entrar.html`                 | Página estática que sirve el servidor del visor. Carga `/visor.js` (diferido) y `/visor.css`, sin scripts ni estilos en línea. También es la página del cliente RDP/VNC; los enlaces de voxl redirigen a la página propia de voxl. |
| `src/cliente/arranque.ts`     | Punto de entrada: canje de `?t=`, `sessionStorage`, WebSocket, avisos y ganchos de ventana.                                                                                                                                        |
| `src/cliente/app.ts`          | Estado compartido y elementos del DOM.                                                                                                                                                                                             |
| `src/cliente/avisos.ts`       | Avisos sobre la imagen (conectando, caído con Reconectar, cerrado, abierto en otra ventana, enlace usado o caducado).                                                                                                              |
| `src/cliente/barra.ts`        | Barra superior que se oculta a los 2 s: nombre y tipo, estado, fps, latencia, pantalla completa, Ctrl+Alt+Supr, Win y cerrar visor.                                                                                                |
| `src/cliente/teclado.ts`      | Teclado: `kc,<keysym>,<código>,<1\|0>`, soltar todo al perder el foco, IME y pegado.                                                                                                                                               |
| `src/cliente/raton.ts`        | Ratón, botones y rueda suave.                                                                                                                                                                                                      |
| `src/cliente/portapapeles.ts` | Portapapeles de texto en los dos sentidos, hasta 1 MB.                                                                                                                                                                             |
| `src/cliente/cursor.ts`       | Cursor remoto pintado con CSS a partir del RGBA del servidor.                                                                                                                                                                      |
| `src/trabajador.ts`           | Worker: WebSocket, búfer de rectángulos, `OffscreenCanvas` y H.264 con WebCodecs.                                                                                                                                                  |
| `src/protocolo.ts`            | Mensajes y órdenes del protocolo (§5.2 del informe de la Fase 12).                                                                                                                                                                 |
| `src/teclas.ts`               | Tabla `KeyboardEvent.code` → keysym de X11 (espejo de `crates/magi-nucleo/src/escritorio/teclas.rs`).                                                                                                                              |
| `src/visor.css`               | Estilos del visor (fuente del sistema; se empaqueta a `dist/visor.css`).                                                                                                                                                           |
| `dist/`                       | **Generado y versionado**: `visor.js`, `trabajador.js` y `visor.css`. No se edita a mano.                                                                                                                                          |

## Cómo se genera

`scripts/generar-visor.sh` usa el `esbuild` fijado por versión exacta en
`package.json`/`package-lock.json` y soporta `--comprobar`:

```sh
npm --prefix visor ci          # dependencias solo de desarrollo, fijadas por integridad
npm --prefix visor run check   # formato (oxfmt), lint (oxlint) y tipos (tsc)
scripts/generar-visor.sh       # regenera visor/dist/
scripts/generar-visor.sh --comprobar   # falla si dist/ no coincide byte a byte
```

Compilar MAGI no necesita Node: la carpeta de dependencias
(`visor/node_modules/`) no se versiona y `dist/` sí.

## Protocolo (resumen)

El servidor Rust es la referencia; aquí solo se consume.

- WebSocket en `/api/ws`. El primer mensaje del visor es
  `{"t":"hola","clave":"…","h264":true|false}` y debe salir en menos de 5 s.
- Mensajes binarios del servidor con enteros big-endian:
  `0x03` cursor (RGBA y punto caliente), `0x05` rectángulo RGBA,
  `0x06` copia (CopyRect), `0x07` vídeo H.264 Annex-B con sus regiones y
  `0x08` fin de lote (el visor presenta lo pintado de una vez).
- Texto JSON del servidor: `hello`, `estado`, `tam`, `clip`, `ping`,
  `latencia` y `close`.
- Órdenes del visor: `kc`, `m`, `b`, `sp`, `r`, `c`, `sz`, `v`, `kf`,
  `cad`, `win`, `po` y `rec` (reconectar).

## Cambios respecto a voxl 0.3.3

### Protocolo y trabajador

- Se ha reescrito `src/protocolo.ts` para el protocolo de MAGI: fuera los
  mensajes de vídeo/audio/micrófono/portapapeles de imagen y los _stripes_ de
  voxl; dentro `Rect` (0x05), `Copia` (0x06), `VideoRdp` (0x07) y
  `Marco` (0x08), el cursor con cabecera de 9 bytes (voxl llevaba relleno), el
  alta con `hola`/`clave` y las órdenes `kc`/`m`/`b`/`sp`/`r`/`c`/
  `sz`/`v`/`kf`/`cad`/`win`/`po`/`rec`.
- `src/worker.ts` se sustituye por `src/trabajador.ts`:
  - Se quita el decodificador por _stripes_ y la presentación por
    `MediaStreamTrackGenerator`/<video>; en su lugar, H.264 Annex-B completo
    con un único `VideoDecoder` y pintado **solo de las regiones** que trae
    cada `VideoRdp`.
  - Los rectángulos y las copias se pintan en un búfer del lote y se vuelcan al
    lienzo visible al recibir `Marco`, para no enseñar medios lotes.
  - Se quitan el audio Opus, el micrófono, los contadores de _bits_ y de
    _pipeline_, la petición de fotograma clave (`k`/`a`/`kf` de voxl) y los
    ficheros; no existen en el protocolo de MAGI.
  - Se añade el alta del protocolo (`hola` con clave y soporte de H.264),
    la reconexión de socket, la visibilidad (`v,0`/`v,1`) y la latencia que
    manda el servidor.
  - El soporte de H.264 **no** se deduce de que exista `VideoDecoder`: antes
    del `hola` se comprueba con `VideoDecoder.isConfigSupported` una lista de
    perfiles AVC (baseline, main y high) y solo si alguno se puede decodificar
    se anuncia `h264:true`. Si el navegador no contesta en 2 s, se anuncia
    `false` (el servidor reconecta RDP sin AVC420 y manda rectángulos).
  - Si aun así el `configure` o el propio decodificador fallan después del
    alta, el trabajador cierra el WebSocket y reconecta una sola vez con
    `h264:false` en el `hola`, sin enseñar el aviso de caído; el servidor
    reconecta la sesión RDP sin AVC420 (`SinH264`) y manda el escritorio
    entero en rectángulos.
  - Al cerrar el visor (o si el servidor manda `close`) se liberan los
    `OffscreenCanvas`: el navegador no les da `close()`, así que el lienzo
    visible y el búfer del lote se reducen a 0×0 y se sueltan contextos y
    referencias, para no dejar en memoria el escritorio cuando la ventana no se
    llega a cerrar. Al reconectar (o al cambiar el tamaño del escritorio), el
    búfer anterior se libera antes de reservar el nuevo.
- Se ha simplificado el cursor: el trabajador valida la cabecera y manda el
  RGBA a la página, que lo pinta con CSS (igual que voxl, pero con la cabecera
  de MAGI y sin el campo de serie).

### Página y entrada

- `index.html` se convierte en `entrar.html`: mismo concepto de una sola
  página, pero sin estilos en línea (CSP `style-src 'self'`), sin la fuente
  Inter empaquetada, sin favicon propio, sin el panel de ficheros, sin el botón
  de teclado en pantalla, sin el audio y sin los _worklets_. Solo hay una
  interfaz en castellano.
- Se añade todo el flujo de entrada de MAGI, que voxl no tenía: canje de
  `?t=<enlace>` contra `POST /api/canjear`, `sessionStorage` para la clave y
  los datos del escritorio (nombre y tipo, que `hello` no manda), limpieza de
  la URL con `history.replaceState`, aviso de enlace usado o caducado y
  redirección a la dirección de voxl cuando el canje la devuelve.
- Se añaden los avisos sobre la imagen y el botón «Reconectar» (orden `rec`),
  que voxl no tenía.
- La barra superior es nueva: nombre y tipo, estado, fps y latencia, pantalla
  completa, Ctrl+Alt+Supr, tecla Windows (solo RDP) y cerrar visor. Se oculta a
  los 2 s y vuelve al acercar el ratón al borde superior.
- Estética de cabina de MAGI (ámbar `#FFA000` sobre `#0A0A0A`, fuente del
  sistema) en lugar de la paleta azul de voxl. Botones con texto visible y
  `aria-label`; foco visible; contraste AA.
- El pegado del portapapeles remoto funciona como en voxl (evento `paste` del
  navegador), pero se limita a 1 MB y, si el navegador rechaza escribirlo, sale
  un aviso con botón «Copiar» que lo hace dentro del gesto del usuario.
- La resolución se pide con `sz,w,h` (agrupada 500 ms) solo si `hello` dice
  que es redimensionable; si no, la imagen se escala sin deformar.
- Pantalla completa: se pide `navigator.keyboard.lock()` donde exista (para que
  Esc, Alt+Tab y la tecla Windows lleguen al remoto) y se manda `kf`.
- `F10` enseña la barra y lleva el foco a sus botones, para poder usarla sin
  ratón; es la única tecla que no se manda al remoto.

### Teclado

- Tabla de keysyms propia, espejo de la tabla única de Rust
  (`crates/magi-nucleo/src/escritorio/teclas.rs`), en lugar de la tabla de
  voxl con el caso especial de macOS.
- Se manda **siempre** el `KeyboardEvent.code` junto al keysym, aunque el
  keysym sea 0 (teclas muertas e IME); al perder el foco se suelta todo con
  `r`.
- Ya no hay lógica de _Command_/_Control_ de macOS ni de «teclado en pantalla»;
  el IME y los teclados en pantalla se traducen a keysyms Unicode.

### Herramientas

- voxl compilaba el cliente desde su `build.rs` con un `esbuild` del sistema
  (nix) y no versionaba el resultado. MAGI lo compila con `esbuild` fijado por
  npm y **versiona** `dist/`, porque compilar MAGI no debe necesitar Node.
- El formateo y el lint usan las mismas herramientas que voxl (oxfmt y oxlint),
  fijadas por versión exacta, más `tsc` y un `esbuild` también fijado.
- Se eliminan las pruebas de _bench_ de voxl (`bench/`) y los globales
  `window.fps`/`window.voxlStats`.

## Desviaciones anotadas

- No hay mensaje de petición de portapapeles en el protocolo: el texto que el
  usuario pega en el visor se manda con `c,<texto>` (≤ 1 MB) y el servidor lo
  guarda para cuando el protocolo remoto pida el portapapeles; del remoto al
  equipo solo llega `{"t":"clip","texto":…}`. Es el contrato acordado con el
  servidor.
- `{"t":"latencia","ms":N}` es un mensaje del servidor que §5.2 no lista: llega
  después de contestar `po,<s>` y es lo que pinta la latencia de la barra
  (desviación anotada en el informe de implementación).
- `rec` (reconectar) tampoco está en §5.2; lo usa el botón «Reconectar» y el
  servidor lo trata como orden.
- La reconexión por H.264 no utilizable tampoco añade mensajes nuevos: se
  reutiliza el `hola` con `h264:false` en una conexión WebSocket nueva, que es
  la señal que el servidor ya interpreta como `SinH264` para reconectar RDP
  sin AVC420.
- En `sessionStorage` se guardan `magi.visor.clave`, `magi.visor.escritorio`
  y `magi.visor.tipo`; la especificación solo pedía la clave, pero `hello` no
  trae el nombre y la barra lo necesita también tras recargar.
- `F10` para enseñar la barra es una concesión de accesibilidad: sin ratón no
  habría forma de llegar a sus botones.

## Verificación

Lo que se ejecuta en cada cambio del visor (y en CI):

```sh
npm --prefix visor ci
npm --prefix visor run check         # oxfmt --check, oxlint y los dos tsc
scripts/generar-visor.sh --comprobar # falla si dist/ no coincide byte a byte
```

Además se ha probado a mano con Chromium 152 y un servidor WebSocket de prueba
que habla el protocolo de §5.2: canje del enlace (válido, usado y de voxl),
alta de 5 s, hello y barra, rectángulos y CopyRect pintados de verdad (píxeles
comprobados en una captura), cursor por CSS, H.264 con WebCodecs, teclado
(keysym + `code`), ratón y rueda, portapapeles en los dos sentidos (también su
aviso con «Copiar»), `v,0`/`v,1`, `sz` agrupado, avisos de caído y «abierto
en otra ventana», reconexión con `rec`, recarga con `sessionStorage`, aviso de
enlace usado y ausencia de violaciones de la CSP y de recursos externos. En
total 53 comprobaciones en verde.

La prueba de navegador no vive en el repositorio: no añade dependencias de
ejecución y no hace falta para compilar MAGI.

## Navegador

Pensado para el Chromium que MAGI abre en modo aplicación. Necesita
`OffscreenCanvas` con `transferControlToOffscreen` (obligatorio en todos los
navegadores modernos); el H.264 se usa solo si WebCodecs confirma con
`isConfigSupported` que decodifica AVC de verdad y, si no, el visor dice
`h264:false` en el alta y el servidor manda rectángulos. La pantalla completa
con captura de teclas usa la Keyboard Lock API, solo disponible en Chromium;
fuera de ahí el botón sigue funcionando pero el navegador puede quedarse
algunas combinaciones.
