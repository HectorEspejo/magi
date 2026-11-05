# MAGI — Fase 12: Escritorio Remoto · Informe de implementación

> Documento vivo: se actualiza en cada sesión de trabajo, nunca se regenera
> desde cero. Refleja lo realmente implementado, las desviaciones y lo que
> queda pendiente. El desarrollador debe entregarlo al analista funcional
> antes de especificar la siguiente fase.

## Resumen de lo implementado

La Fase 12 está implementada y verificada en el árbol de trabajo (pendiente de
push): **escritorios remotos** RDP, VNC y voxl integrados
en el inventario de MAGI, vistos en una ventana del navegador servida por
`magi --servidor` solo en `127.0.0.1`, con sesiones custodiadas, gracia de un
minuto sin visor y sin recomprimir vídeo. Versión **1.2.0**, migración 8,
protocolo **7** y `formato` **2** de sincronización.

- **Modelo e inventario.** Migración 8 (`HOSTS.tipo` ssh/rdp/vnc/voxl fijo al
  crear, `escritorio_dominio`, `escritorio_opciones` en JSON y
  `huella_escritorio` propia del equipo), validación en código (sin `CHECK`),
  puertos por defecto, «a través de» solo hacia hosts SSH, tipos y opciones
  validados al leer (T38). Los escritorios no salen en `magi_config` (exportar
  filtra por tipo SSH) ni pueden ser destino de terminal, SFTP, túneles,
  snippets o salto (los snippets por etiqueta los omiten y lo dicen).
- **TUI.** Formulario con selector de tipo y campos por tipo; ficha con bloque
  ESCRITORIO y «olvidar huella/contraseña» con confirmación; Hosts con `▣`,
  tipo, `⤴` y `⚠` (y texto en ASCII); paleta, búsqueda y Flota (sin sondeo);
  pestaña de escritorio con tarjeta y teclas `↵`, `y`, `^] r`, `^] x`;
  Sesiones con columna TIPO; diálogos de certificado nuevo/cambiado y de
  credenciales con 3 intentos; instantáneas `insta` a 40×12, 80×24 y 200×60;
  `magi conectar <escritorio>` abre la pestaña y el visor; navegador abierto
  por el cliente sin shell (`[escritorio] navegador`, modo `--app`), con el
  enlace visible si no se puede lanzar; `magi diagnostico` dice qué navegador
  usará y avisa si no hay ninguno.
- **Servidor de sesiones.** `src/servidor/escritorios.rs`: custodia en memoria
  con estados y fases, camino por `conexion_para_canal(UsoCanal::Escritorio)` +
  `direct-tcpip` o TCP directo con plazo de 10 s, suelta de la conexión del
  pool por identidad en todas las ramas, reintentos de credenciales 1..3
  reabriendo el camino, diálogos al cliente solicitante (nunca toca el
  llavero), gracia de `[escritorio] gracia_seg` sin visor, un visor por
  sesión, caída/reconexión, `Escritorios{lista}` completa en cada cambio,
  `TrabajoServidor.escritorios`, REGISTRO de `escritorio_abierto`/`cerrado`/
  `fallido` y apagado limpio.
- **Servidor del visor y puente.** `visor_http.rs` (axum 0.8, solo
  `127.0.0.1`): `/entrar`, `/`, ficheros embebidos, `/api/canjear`,
  `/api/ws` y `/api/salud`; enlaces de un solo uso de 256 bits y 60 s,
  clave por sesión, `Host` siempre y `Origin` en `/api/*`, CSP estricta,
  `nosniff`, `no-referrer`, `no-store`, 429 tras 10 fallos, `hola` en 5 s.
  `puente.rs` agrupa zonas sucias a 60 fps con contrapresión de 8 MB, manda
  rectángulos RGBA, copias, cursor y el H.264 de Windows tal cual, mantiene el
  búfer del escritorio y valida toda la entrada del visor.
- **Visor web** (`visor/`, adaptado de voxl 0.3.3 MIT): worker con
  `OffscreenCanvas` y WebCodecs, rectángulos/copias/cursor, teclado por keysym
  y código físico, ratón y rueda suave, portapapeles de texto en los dos
  sentidos, resolución que se ajusta o escalado, pantalla completa con
  `navigator.keyboard.lock`, barra de 2 s, avisos y `sessionStorage`; `dist/`
  generado y versionado, `scripts/generar-visor.sh` con `--comprobar`.
- **RDP** (`escritorio/rdp/`): IronRDP 0.17, TLS 1.2/1.3 con rustls/aws-lc-rs
  (sin OpenSSL), NLA/CredSSP con NTLM, huella `SHA256:` comprobada antes de
  mandar credenciales (política de huellas SSH), mapas de bits y RemoteFX como
  rectángulos, EGFX/AVC420 crudo con un `DvcProcessor` propio (sin decodificar
  ni recomprimir), teclado por scancodes con respaldo Unicode,
  Ctrl+Alt+Supr/Win, CLIPRDR, Display Control y reactivación.
- **VNC** (`escritorio/rfb/`): RFB 3.3/3.7/3.8, None y VNC Authentication
  (DES con bits invertidos), Raw/CopyRect/Hextile/ZRLE, DesktopSize,
  ExtendedDesktopSize y Cursor, `shared-flag`, QEMU keys, portapapeles Latin-1
  y extendido UTF-8, y límites contra servidores maliciosos.
- **voxl por SSH** (`escritorio/voxl.rs` + `scripts/escritorio_voxl.sh`):
  script embebido con argumentos validados, contraseña aleatoria por stdin,
  `/healthz`, túnel de sesión en `127.0.0.1`, parada TERM/KILL y errores
  legibles.
- **Sincronización y Android.** `formato` 2 solo si hay hosts que no son SSH;
  viajan `tipo`, `escritorio_dominio` y `escritorio_opciones`;
  `huella_escritorio` y contraseñas no; `formato_nuevo` para >2; el formato 1
  se lee con todos los hosts SSH. La app Android lee el formato 2 y enseña los
  escritorios con su tipo y «se abre en el ordenador», sin conectarlos.
- **Calidad.** Revisión adversarial de cierre (T31/T54) con informe propio y
  correcciones aplicadas con pruebas de regresión; `cargo fmt`, `clippy -D
  warnings`, `cargo deny check`, `actionlint`, `desktop-file-validate`,
  `shellcheck`, `dist plan --tag v1.2.0`, `npm run check` y
  `generar-visor.sh --comprobar` en verde.

## Desviaciones respecto a la especificación (qué y por qué)

1. **`ironrdp-connector` y `ironrdp-acceptor` vendorizados con `sspi` 0.23.1 y
   `picky` 7.0.0-rc.27.** IronRDP 0.17 fija `sspi` 0.21 y `picky` 7.0.0-rc.25,
   que arrastran `ecdsa 0.17.0-rc.22`; russh 0.63 (fijado en el proyecto) exige
   `ecdsa 0.17` estable a través de `p256`/`ssh-key`. Las dos generaciones no
   pueden convivir en el mismo grafo. Se vendorizan dos crates de IronRDP sin
   tocar su código salvo dos líneas de adaptación a la API de `sspi` 0.23
   (`KerberosConfig.kdc_resolution` y `TsRequest::buffer_len`, que ahora
   devuelve `Result`), y solo cambian sus pin de `sspi`/`picky`. Revisar al
   subir russh o IronRDP; si sus versiones coinciden, quitar el vendor.
2. **EGFX no está en el meta-crate `ironrdp` 0.17.** Se añade
   `ironrdp-egfx = "=0.3.0"` (la generación compatible con `pdu` 0.9 y
   `dvc` 0.8; la 0.5 no compila con 0.17). Para que el AVC420 de Windows llegue
   al visor sin recomprimir se implementa un `DvcProcessor` propio que captura
   los `WireToSurface1` H.264 y sus regiones y los pasa tal cual; la API de alto
   nivel de IronRDP descarta las regiones y exige decodificar.
3. **Mensajes del visor no listados en §5.2** (necesarios para el checklist):
   la orden `rec` (Reconectar desde el aviso de caído), el mensaje de servidor
   `{"t":"latencia","ms":N}` tras `po`, los avisos internos `Visible` y
   `SinH264` y `renovar_fuente` (reconectar sin recrear visor ni clave), y
   `GET /` además de `/entrar` (para recargar tras `history.replaceState`).
4. **El canje devuelve `{"escritorio": "<nombre legible>"}`** en vez del id,
   porque la página del visor terminada pinta ese valor en la barra.
5. **`EventoEscritorio::Abierto`** se añade al contrato interno como señal de
   «sesión lista» (el servidor pasa a abierto y emite el enlace al recibirla;
   si un cliente no la emitiera, acepta el primer Imagen/Rect/Video).
6. **`UsoCanal::Escritorio`** se añade en `src/servidor/conexiones.rs` para
   identificar el canal del pool de un escritorio.
7. **`peticion_id` del servidor para diálogos iniciados desde el visor** en
   `[2^47, 2^48)`, un tramo libre entre los del cliente (2^46) y los
   esperables del cliente (2^48); evita reutilizar ids ya cerrados.
8. **La `Bienvenida` solo incluye la lista `Escritorios` si no está vacía**
   (compatibilidad con pruebas y clientes previos; la ventana arranca vacía y
   recibe la difusión en el primer cambio).
9. **Reconexión pedida desde el visor sin ventana de MAGI conectada:** no se
   reconecta y el visor muestra «abre MAGI para reconectar» (no hay cliente a
   quien pedir certificado ni credenciales).
10. **`CerrarEscritorio` durante «abriendo» se trata como cancelación:** aborta
    la apertura, anota `escritorio_fallido` y contesta `Error` con el
    `peticion_id` de la apertura.
11. **Con `v,0` (pestaña oculta) el puente deja de mandar imagen** y con
    `v,1`/pantalla completa repinta el escritorio entero. Con H.264 no hay
    búfer de píxeles: el keyframe al volver lo tendría que pedir la pista RDP;
    el cliente RDP actual ignora `Visible`, así que queda como limitación
    conocida (documentada en el puente y en la revisión).
12. **AVC420 se prueba a nivel unitario del `DvcProcessor`** (ZGFX +
    `WireToSurface1` + regiones): el `ironrdp-server` de desarrollo no trae la
    feature `egfx` para generarlo en proceso. La validación con Windows 11 real
    está en el guion manual.
13. **La prueba instrumentada de Android no se pudo ejecutar** (el AVD se queda
    en `FallbackHome`/pantalla de bloqueo); sí pasan `:app:assembleDebug`,
    `:app:lintDebug` y `:app:testDebugUnitTest`, que cubren la lectura del
    formato 2 y la presentación de escritorios.
14. **Se actualizó `cargo-dist` local a 0.33.1** (el `Cargo.toml` ya fijaba
    esa versión) para poder ejecutar `dist plan --tag v1.2.0`.

## Estructura de archivos creada/modificada

- **Núcleo** `crates/magi-nucleo/`:
  - `src/escritorio/mod.rs` (tipos, opciones, eventos, órdenes, diálogos,
    errores, hilo por sesión), `src/escritorio/teclas.rs`,
    `src/escritorio/puente.rs`, `src/escritorio/visor_http.rs`,
    `src/escritorio/voxl.rs`, `src/escritorio/rdp/` (mod, conexion, tls,
    certificado, transporte, sesion, egfx, cliprdr, entrada),
    `src/escritorio/rfb/` (mod, protocolo, seguridad, hextile, zrle),
    `scripts/escritorio_voxl.sh`.
  - `src/almacen/{migraciones,hosts,mod}.rs`, `src/modelo.rs`,
    `src/registro.rs`, `src/snippets/mod.rs`,
    `src/sincronizacion/{formato,elementos,mod,aplicacion}.rs`,
    `src/configuracion.rs` (si aplica), `Cargo.toml`.
  - Pruebas: `tests/escritorio_{rfb,rdp,visor,voxl,e2e}.rs`,
    `tests/sincronizacion_escritorios.rs`, `tests/sincronizacion_lector.rs`.
- **Escritorio** (raíz `magi-ssh`):
  - `src/protocolo.rs` (v7), `src/config.rs` (`[escritorio]`),
    `src/servidor/escritorios.rs` y `src/servidor/mod.rs`,
    `src/servidor/conexiones.rs`, `src/app/escritorios.rs`,
    `src/app/lanzar.rs`, `src/app.rs`, `src/ui/escritorios.rs` y el resto
    de `src/ui/**`, `src/main.rs`, `src/cliente/mod.rs`,
    `src/diagnostico.rs`, `src/sshconfig/exportar.rs`, `Cargo.toml`.
  - Pruebas: `tests/escritorio_servidor.rs`, `tests/escritorio_ui.rs`,
    `tests/redimensionado/vistas_n.rs` y semilla/arnés,
    `tests/protocolo.rs`, `tests/recursos.rs`.
- **Visor**: `visor/` (`entrar.html`, `src/**`, `dist/{visor.js,
  trabajador.js,visor.css}`, `package.json`, `package-lock.json`,
  `LICENSE-voxl`, `README.md`, configuración de tipos/formato/lint) y
  `scripts/generar-visor.sh`.
- **Android**: `crates/magi-movil/src/api.rs` (tipo en el contrato UniFFI),
  `android/app/src/main/kotlin/**` (tipo en modelos, Hosts y ficha),
  `android/app/src/main/res/values/strings.xml`, `android/CHANGELOG.md` y
  pruebas JVM.
- **Empaquetado y documentación**: `Cargo.toml`/`Cargo.lock`,
  `vendor/ironrdp-connector`, `vendor/ironrdp-acceptor`, `deny.toml`,
  `.github/workflows/ci.yml`, `assets/man/magi.1`, `CHANGELOG.md`,
  `README.md`, `docs/formato-sincronizacion.md`,
  `docs/fase12/magi-fase12-checklist.md`,
  `docs/fase12/magi-fase12-revision-adversarial.md` y este informe.

## Decisiones técnicas tomadas durante el desarrollo

- **Un hilo con runtime propio por sesión de escritorio**
  (`lanzar_en_hilo_con`), porque el `ActiveStage` de IronRDP no es `Send`:
  la fábrica se ejecuta dentro del hilo y la decodificación (ZRLE/Hextile,
  bitmaps, RemoteFX) no frena el runtime de red del servidor (D225).
- **Transporte genérico** (`Box<dyn AsyncRead + AsyncWrite + Unpin + Send>`):
  el mismo cliente RDP/VNC sirve para TCP directo y para un canal
  `direct-tcpip` del pool SSH (D215).
- **TLS de RDP manual con rustls/aws-lc-rs**: verificador que captura el
  certificado DER, reanudación desactivada (CredSSP no la soporta) y
  `connect_finalize` con los bits del `SubjectPublicKeyInfo` (no el DER), que
  es lo que espera CredSSP; huella comprobada antes de credenciales (D218).
- **AVC420 crudo con un `DvcProcessor` propio** sobre `ironrdp-egfx` 0.3:
  `zgfx::Decompressor` + `GfxPdu` + `Avc420BitmapStream`, `FrameAcknowledge`
  en `EndFrame`, topes de 16 MiB por fotograma y 32 MiB descomprimido; MAGI no
  decodifica ni recomprime vídeo (D211).
- **El servidor no dialoga ni toca el llavero** (T25): reenvía
  `PideHuellaEscritorio` y `PideCredenciales` al cliente solicitante y espera
  su respuesta; la huella la guarda el cliente que la acepta y el REGISTRO
  `huella_escritorio` lo anota el cliente.
- **Un solo árbol criptográfico**: se vendorizan dos crates de IronRDP antes que
  duplicar versiones incompatibles de RustCrypto (ver desviación 1).
- **Puente con búfer del escritorio y contrapresión**: las zonas sucias se
  fusionan (≤64 regiones) y se parten si un lote no cabe en la cola; el sucio
  solo se descarta cuando se ha mandado, y los textos también descuentan.
- **Seguridad del visor**: enlaces de un solo uso gastados siempre, clave por
  sesión en `sessionStorage`, `Host` en todas las rutas, `Origin` en
  `/api/*`, CSP estricta, sin cookies ni tokens fijos en la URL (D217).
- **Un visor por sesión** con sustitución del anterior (`otra_ventana`) y
  gracia que cuenta el visor conectado (o las conexiones del túnel en voxl)
  (D216).

## Funcionalidades del checklist completadas

Texto exacto del checklist `magi-fase12-checklist.md` (128/128 marcadas):

### Inventario y modelo
- Migración 8: HOSTS gana `tipo` (`ssh` por defecto), `escritorio_dominio`, `escritorio_opciones` (JSON) y `huella_escritorio`; `user_version` 8
  - AC: Dado un `magi.db` en la versión 7, cuando arranca MAGI 1.2, entonces todos los hosts quedan de tipo `ssh` y no cambia nada más
- Tipos de host `ssh`, `rdp`, `vnc` y `voxl`, validados en código (sin `CHECK`), con puerto por defecto 22, 3389, 5900 y 6901
- El tipo se elige al crear el host y no se puede cambiar después
  - AC: Dado un host RDP, cuando se edita, entonces el tipo aparece fijo, y una fila con el tipo cambiado por fuera se rechaza al leerla (T38)
- «A través de» (`salto_host_id`) solo admite hosts SSH: opcional en RDP y VNC, obligatorio en voxl y sin ciclos
- En voxl la dirección es siempre `127.0.0.1` del host SSH, el puerto empieza en 1024 y se piden la pantalla X11 (`:N` o `:N.M`) y el programa (`voxl` o una ruta absoluta)
  - AC: Dado «:1» y el puerto 6901, cuando se guarda, entonces se acepta; con «:1; rm» o un programa con espacios, se rechaza con el motivo
- `escritorio_opciones` por tipo (tamaño, portapapeles, teclado en RDP, pantalla y programa en voxl, `directo` en VNC), validado al guardar y al leer; las claves desconocidas se conservan
- Contraseña de RDP y VNC en el llavero por `uuid` del host (`contrasena:llavero`), pedida siempre (`contrasena`) o ninguna (VNC sin contraseña); nunca en `magi.db` (T19)
- Usuario RDP escrito como `DOMINIO\usuario` se separa en dominio y usuario al guardar
- `huella_escritorio` es propia del equipo: no viaja ni sube la versión del host
- Los hosts que no son SSH no se exportan a `magi_config`
  - AC: Dado un inventario con 3 hosts SSH y 2 escritorios, cuando se exporta, entonces `magi_config` tiene 3 entradas `Host` y el `ssh` real lo lee sin errores
- Los hosts que no son SSH no pueden ser destino de terminal, SFTP, túneles, snippets ni salto: no salen en esos selectores y un snippet por etiqueta los omite y lo dice
- Un VNC directo a una dirección que no es de loopback pide confirmación al guardarlo o al abrirlo si no la tiene (`directo`), y la ficha muestra `⚠`
  - AC: Dado un VNC que iba a través de un host SSH borrado en otro equipo, cuando se abre, entonces MAGI avisa «la imagen y el teclado viajarán sin cifrar» y solo sigue si se acepta

### Vistas de la TUI
- Formulario de host con selector de tipo (SSH, RDP, VNC, voxl) que enseña solo los campos de ese tipo
- Ficha de un escritorio con el bloque «Escritorio»: tipo, destino, a través de, usuario y dominio, contraseña guardada (sí o no), tamaño, teclado, portapapeles, huella del certificado y última conexión
- «Olvidar huella del certificado» y «Olvidar contraseña» en la ficha, con confirmación
- Hosts: los escritorios llevan `▣` y su tipo además del estado, `⤴` para «a través de» y `⚠ directo`, con texto en modo ASCII (T5)
- `↵` en un escritorio lo abre; si ya tiene sesión, va a su pestaña y abre el visor con un enlace nuevo
- Paleta (`Ctrl+P`) y búsqueda incluyen los escritorios por nombre, dirección, tipo y etiquetas
- Flota lista los escritorios sin sondearlos, con su último estado y «sin sondeo»
- `magi conectar <escritorio>` abre MAGI en la pestaña del escritorio y el visor
- Pestaña `▣` en Sesión con la tarjeta del escritorio: estado, destino, a través de, usuario, tamaño, imagen (H.264, rectángulos o voxl), visor, certificado y tiempo
  - AC: Dado un escritorio abierto sin visor, cuando pasan 15 s, entonces la tarjeta dice «sin visor · se cierra en 45 s»
- En la pestaña: `↵` abre el visor, `y` copia un enlace de un solo uso, `^] r` reconecta y `^] x` cierra con confirmación (T6)
- Vista Sesiones con columna TIPO; las de escritorio se reconectan y se cierran como las de terminal
- Diálogo «Certificado nuevo» (ámbar) con sujeto, caducidad y huella `SHA256:`; aceptarla la guarda y queda en REGISTRO
- Diálogo «Certificado cambiado» en rojo con la huella anterior y la nueva; sustituirla exige escribir el nombre del host
- Diálogo de credenciales: usuario y dominio (RDP), contraseña enmascarada, «recordar en el llavero» y 3 intentos
- Instantáneas `insta` de Hosts con escritorios, formulario RDP, ficha, pestaña de escritorio y diálogos de certificado a 40×12, 80×24 y 200×60 (T45)
- REGISTRO: `escritorio_abierto`, `escritorio_cerrado` (con duración y motivo), `escritorio_fallido` (con motivo) y `huella_escritorio` (aceptada o sustituida), siempre por `registro::anotar`

### Sesiones de escritorio (servidor)
- Las sesiones de escritorio viven en `magi --servidor`: sobreviven a cerrar la TUI y se ven desde todas las ventanas
- Difusión de la lista completa `Escritorios{lista}` en cada cambio de estado (T27)
- Estados abriendo (conectando, verificando certificado, autenticando, arrancando voxl), abierto (con visor o sin visor), caído y cerrando
- Camino por SSH con `conexion_para_canal` (uso `Escritorio`) y un canal `direct-tcpip` al destino; directo, con TCP y plazo de 10 s
  - AC: Dado un VNC en `127.0.0.1:5901` a través de web-01, cuando se abre, entonces no se abre en el equipo ningún puerto salvo el del visor
- Un canal de escritorio mantiene viva la conexión del pool, como un túnel, y se suelta por identidad en todas las ramas (T34)
- Plazos: 10 s para TCP, saludos TLS y RFB y apertura de canal; 5 min para los diálogos; ninguna espera sin plazo (T32)
- Gracia: una sesión abierta sin visor durante `gracia_seg` (60 s) se cierra sola; abrir el visor antes la conserva
  - AC: Dado un escritorio con el visor cerrado, cuando pasan 60 s, entonces se cierra y REGISTRO anota `escritorio_cerrado` con «sin visor durante 60 s»
- Un visor por sesión: abrir otro sustituye al anterior, que enseña «abierto en otra ventana»
- Conexión caída (también la del host «a través de»): estado caído con el motivo, la pestaña se conserva y se reconecta con `r` o desde el visor si hay una ventana de MAGI abierta
- El servidor no dialoga ni toca el llavero: el certificado y las credenciales se piden al cliente solicitante (T25), que es quien guarda la huella
- `TrabajoServidor.escritorios`: un servidor con escritorios abiertos no se sustituye sin preguntar (T67) y `magi servidor parar` los cuenta
- La decodificación va en un hilo por sesión, fuera del runtime de red, y la TUI nunca recibe píxeles

### Visor en el navegador
- Visor en `visor/`, adaptado del cliente web de voxl 0.3.3, con el aviso MIT en `visor/LICENSE-voxl` y el origen y los cambios en `visor/README.md`
- El visor pinta rectángulos en bruto y copias en un `OffscreenCanvas` dentro de un worker y presenta cada lote al recibir `Marco`
- El visor decodifica H.264 con WebCodecs cuando llega `VideoRdp` y solo pinta las regiones de cada marco
- Teclado: manda el keysym y el código físico de cada tecla y suelta todas las teclas al perder el foco
- Ratón y rueda (con desplazamiento suave), y cursor remoto dibujado por el visor
- Portapapeles de texto en los dos sentidos, hasta 1 MB, si el host lo tiene activado
- Resolución que se ajusta a la ventana (agrupada 500 ms); si el servidor no la cambia o el host la tiene fija, se escala sin deformar
- Pantalla completa con `navigator.keyboard.lock` donde exista, para que Esc, Alt+Tab y la tecla Windows lleguen al remoto
- Barra superior que se oculta a los 2 s: nombre y tipo, estado, fps y latencia, pantalla completa, Ctrl+Alt+Supr, tecla Windows (RDP) y cerrar visor
- Avisos sobre la imagen: conectando, caído (con «Reconectar»), cerrado, «abierto en otra ventana» y enlace usado o caducado
- Con la pestaña oculta el visor pausa la imagen (`v,0`) y al volver pide el escritorio entero
- Estética de cabina (paleta de MAGI, fuente del sistema, sin recursos externos) y botones con texto y `aria-label`
- `visor/dist/` se genera con `scripts/generar-visor.sh` (esbuild fijado) y va dentro del binario: compilar MAGI no necesita Node
  - AC: Dado un cambio en `visor/src`, cuando la CI ejecuta `scripts/generar-visor.sh --comprobar`, entonces falla si `visor/dist` no está al día

### Servidor del visor y seguridad
- Servidor HTTP y WebSocket del visor dentro de `magi --servidor`, solo en `127.0.0.1` (puerto libre o `[escritorio] puerto_visor`), que arranca con la primera sesión de escritorio y se para con la última
- Enlaces de un solo uso: 256 bits, caducan a los 60 s, se gastan al canjearlos aunque falle algo y se anulan al cerrar la sesión
  - AC: Dado un enlace ya canjeado, cuando se abre otra vez, entonces el visor dice «enlace usado o caducado: ábrelo otra vez desde MAGI» y no conecta
- Clave del visor por sesión, guardada en `sessionStorage` (sobrevive a recargar la pestaña); la URL se limpia con `history.replaceState`
- El WebSocket exige como primer mensaje `{"t":"hola","clave":…,"h264":…}` en 5 s; si no, se cierra
- `Host` comprobado en todas las rutas (127.0.0.1 o localhost con el puerto propio) y `Origin` obligatorio en `/api/*`
- Cabeceras: CSP estricta sin scripts en línea, `nosniff`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` y `frame-ancestors 'none'`
- Tras 10 canjes fallidos en un minuto, `/api/canjear` responde 429 durante un minuto
- Todo lo que llega del visor se valida: coordenadas dentro del escritorio, códigos de tecla de la tabla, tamaños entre 200×200 y 8192×8192 y texto de hasta 1 MB
- Nada de la sesión va al log ni a REGISTRO: ni píxeles, ni teclas, ni portapapeles, ni contraseñas (T44)
- El cliente abre el visor sin shell: `[escritorio] navegador = "auto"` busca Chromium, Brave, Chrome o Edge y usa `--app=`; si no hay, `xdg-open` (Linux) u `open` (macOS)
  - AC: Dado `navegador = "brave --app={url}"`, cuando se abre un escritorio, entonces se lanza Brave con la URL en un solo argumento
- Si no se puede lanzar el navegador, la pestaña enseña el enlace para copiarlo

### RDP
- Cliente RDP con IronRDP fijado con `=` en `magi-nucleo`, con TLS de rustls y la misma cripto que russh, sin OpenSSL
- TLS 1.2 o 1.3 con NLA (CredSSP con NTLM) y, si el servidor no la exige, TLS sin NLA
- Huella `SHA256:` del certificado comprobada antes de mandar credenciales: nueva, diálogo; distinta, bloqueo; igual, sigue
  - AC: Dado un Windows con el certificado regenerado, cuando se abre, entonces sale el bloqueo en rojo y no se envía ninguna credencial
- Credenciales del llavero sin diálogo; si fallan o no hay, diálogo con 3 intentos y el motivo «usuario o contraseña incorrectos»
- Mapas de bits, RemoteFX y lo demás que decodifica IronRDP llegan al visor como rectángulos
- Con EGFX y AVC420, el H.264 de Windows llega al visor sin recomprimir; si el visor no decodifica H.264, MAGI reconecta sin AVC420
  - AC: Dado Windows 11 y el visor en Chromium, cuando se abre, entonces la barra dice «H.264» y MAGI no decodifica ni recomprime vídeo
- Teclado por scancodes con la distribución del host o la de `[escritorio] distribucion_teclado` (`auto` sale de `LANG`) y eventos Unicode de reserva
  - AC: Dado un Windows en español, cuando se escriben ñ, @ y € (AltGr) en el visor, entonces aparecen igual en el remoto
- Ctrl+Alt+Supr y tecla Windows desde la barra del visor
- Portapapeles de texto con CLIPRDR (`CF_UNICODETEXT`)
- Cambio de tamaño en vivo con Display Control; sin ese canal, tamaño inicial y escalado
- Forma y posición del puntero remoto en el visor
- Un servidor que solo admite RDP antiguo sin TLS falla con «este servidor solo habla RDP antiguo, sin TLS: no admitido»

### VNC
- Cliente RFB propio en `magi-nucleo` (RFC 6143), versiones 3.3, 3.7 y 3.8
- Seguridad `None` y `VNC Authentication` (DES con los bits de cada byte invertidos; aviso si la contraseña pasa de 8 caracteres); otro tipo falla con su número
- Codificaciones Raw, CopyRect, Hextile y ZRLE, y seudocodificaciones DesktopSize, ExtendedDesktopSize y Cursor
- Escritorio compartido (`shared-flag`): abrir MAGI no echa a otros visores
- Peticiones de actualización incrementales al ritmo del visor, como mucho 60 por segundo
- Teclado con keysyms y, si el servidor la anuncia, la extensión de teclas de QEMU con scancodes
- Portapapeles de texto: Latin-1 (lo que no cabe se sustituye con aviso) y UTF-8 si el servidor anuncia el portapapeles extendido
- Cambio de tamaño con `SetDesktopSize` si el servidor anuncia ExtendedDesktopSize; si no, escalado
- Límites contra servidores maliciosos: escritorio de hasta 8192×8192, rectángulos dentro del escritorio, ZRLE acotado, cursor de hasta 256×256 y texto de hasta 1 MB
  - AC: Dado un servidor de prueba que manda un rectángulo fuera del escritorio, cuando llega, entonces la sesión cae con «datos inválidos del servidor», sin pánico

### voxl por SSH
- voxl se arranca en el host SSH con el script embebido `escritorio_voxl.sh` (`include_str!`), con pantalla, puerto, programa y portapapeles como argumentos validados en Rust y en el script
- La contraseña de voxl es aleatoria por sesión y le llega por la entrada estándar; nunca en la línea de órdenes ni en el log
- voxl escucha en `127.0.0.1` del host, con `--file-transfers=none`, sin sonido ni micrófono, con redimensionado y con el portapapeles según el host
- MAGI exige voxl 0.3 o posterior y espera a `/healthz` (plazo de 10 s) antes de dar la sesión por abierta
- Túnel de la sesión en `127.0.0.1` (puerto libre) hasta el voxl del host; no se guarda en TUNELES ni sale en la vista Túneles
- La lanzadera canjea el enlace y lleva el navegador a la página de voxl a través del túnel
- Al cerrar la sesión, por gracia o si cae la conexión SSH, el script recibe el fin de la entrada y para voxl (TERM y, a los 5 s, KILL)
  - AC: Dado un voxl abierto, cuando se cierra su pestaña en MAGI, entonces a los 6 s `pgrep -f "voxl --addr=127.0.0.1"` en el host no devuelve nada
- Errores legibles: «voxl no está instalado en <host>», «la pantalla :1 no existe», «el puerto 6901 del host está ocupado», «actualiza voxl a la 0.3 o posterior» y «voxl no respondió en 10 s»
- La gracia de voxl cuenta las conexiones abiertas del túnel: sin ninguna durante 60 s, la sesión se cierra

### Sincronización y app Android
- `tipo`, `escritorio_dominio` y `escritorio_opciones` viajan con el host; `huella_escritorio` y las contraseñas no
- El fichero sube a `formato` 2 solo si el inventario tiene algún host que no es SSH; si no, sigue en `formato` 1
  - AC: Dado un inventario solo con hosts SSH, cuando se exporta, entonces el fichero es `formato` 1 y un equipo con MAGI 1.1 lo importa
- MAGI 1.2 lee el `formato` 1 (todos los hosts, SSH) y el 2; un equipo con 1.1 ve el de 1.2 como `formato_nuevo`
- `docs/formato-sincronizacion.md` documenta el `formato` 2 y los campos nuevos
- La app Android lee el `formato` 2 y enseña los escritorios en Hosts con su tipo y «se abre en el ordenador», sin conectar ni crear túneles en ellos
- `android/CHANGELOG.md` recoge el cambio en «Sin publicar»

### Protocolo, configuración y diagnóstico
- `VERSION_PROTOCOLO = 7` con los mensajes de escritorio; `Hola` y `VersionIncompatible` sin cambios (T66)
- `peticion_id` de escritorios en su propio rango, desde 2^46
- `config.toml` `[escritorio]`: `navegador`, `gracia_seg`, `puerto_visor`, `distribucion_teclado` y `h264`, validados con error claro
- `magi diagnostico` dice qué navegador abrirá el visor y avisa si no encuentra ninguno

### API: servidor del visor y protocolo
- GET /entrar
- GET /visor.js · /trabajador.js · /visor.css
- POST /api/canjear
- GET /api/ws
- GET /api/salud
- Mensajes del servidor al visor: Cursor (`0x03`), Rect (`0x05`), Copia (`0x06`), VideoRdp (`0x07`), Marco (`0x08`) y los de texto `hello`, `estado`, `tam`, `clip`, `ping` y `close`
- Órdenes del visor: `kc`, `m`, `b`, `sp`, `r`, `c`, `sz`, `v`, `kf`, `cad`, `win` y `po`
- `AbrirEscritorio` → `EscritorioAbierto`
- `PedirEnlace` → `Enlace`
- `ReconectarEscritorio` y `CerrarEscritorio`
- `Escritorios{lista}`
- `PideHuellaEscritorio` → `RespuestaHuellaEscritorio`
- `PideCredenciales` → `Credenciales` (contraseña en `Zeroizing`)

### Calidad, pruebas y publicación
- Pruebas VNC con un servidor RFB de prueba en proceso: saludo, DES con un vector conocido, codificaciones (píxeles exactos), entrada, portapapeles, tamaño y límites
- Pruebas RDP con `ironrdp-server`: TLS, huella nueva, igual y cambiada, credenciales, mapas de bits, AVC420 pasado tal cual, teclado, portapapeles y tamaño
- Pruebas del servidor del visor: enlaces (uso único, caducidad, anulación), `Host`, `Origin`, cabeceras, clave, sustitución de visor y 429
- Prueba de extremo a extremo sin red externa: servidor VNC de prueba → puente → cliente WebSocket de prueba, con píxeles y entrada de ida y vuelta (T95)
- Prueba de voxl con el servidor SSH de prueba y un `voxl` falso: argumentos, contraseña por la entrada estándar, `/healthz`, túnel y parada al cerrar
- Pruebas de sincronización: formato 1 sin escritorios, formato 2 con ellos, lectura de los dos y `formato_nuevo`
- `npm --prefix visor run check` (TypeScript, formato y lint) y `scripts/generar-visor.sh --comprobar` en CI, con las dependencias de npm fijadas por integridad
- `cargo deny check` en verde sin licencias GPL; IronRDP, `sspi`, `des` y `axum` en la lista de licencias
- Versión 1.2.0: `Cargo.toml`, fecha de la página man, `assets/` regenerados y `CHANGELOG.md`
- README con la sección «Escritorio remoto»: tipos, a través de SSH, navegador, voxl en el host y seguridad
- Revisión adversarial de cierre (T31, T54) del servidor de escritorios, el servidor del visor, el puente y el cliente RFB, anotada en el informe de implementación

## Pendientes y bloqueos

No hay bloqueos de implementación. Queda pendiente lo que no se puede validar
en este entorno y debe hacer el desarrollador con el guion de §11 del informe
funcional:

- **Validación manual real** (guion de §11): Windows 11 Pro por RDP directo y a
  través de SSH (certificado nuevo/cambiado, NLA, «H.264» en la barra,
  redimensionado, pantalla completa, Ctrl+Alt+Supr, portapapeles y ñ/@/€ con
  AltGr), VM QEMU por VNC (imagen, cursor, resolución, teclado español), Windows
  XP/2000 por VNC, xrdp y GNOME Remote Desktop, TigerVNC con contraseña, voxl
  con Xvfb (60 fps y parada con `pgrep`), corte de red con «Reconectar», dos
  ventanas de MAGI, sincronización con el móvil y `magi diagnostico` con y sin
  navegador.
- **AVC420 con Windows real**: el passthrough está implementado y probado a
  nivel de `DvcProcessor`, pero el `ironrdp-server` de desarrollo no puede
  generarlo; falta verlo con Windows 11.
- **`connectedDebugAndroidTest`**: el AVD de este entorno se queda en la
  pantalla de bloqueo; las pruebas JVM, el lint y el APK sí pasan. La prueba de
  humo actualizada queda sin ejecutar en emulador.
- **Keyframe H.264 al volver de `v,0`**: el puente deja de mandar vídeo y el
  cliente RDP ignora aún `OrdenEscritorio::Visible`; al volver, el keyframe lo
  tendría que pedir la pista RDP. Es una limitación conocida y documentada.
- **Mantenimiento del vendor**: al subir russh o IronRDP, revisar si sus
  versiones de `sspi`/`picky`/`ecdsa` ya coinciden y quitar
  `vendor/ironrdp-{connector,acceptor}`.
- **Validación manual de la fase en general**: la F12 se ha verificado con
  pruebas automáticas; el guion de validación manual completo sigue pendiente,
  como en fases anteriores.

## Ejecución y pruebas (cómo arrancar, migrar y testear)


Resultados concretos en este árbol (2026-11-05):

- `TZ=UTC cargo test --workspace --locked`: todas las suites en verde (entre
  otras: 459 pruebas de la lib del escritorio, 292 de `redimensionado`, 19 de
  `escritorio_ui`, 14 de `escritorio_servidor`, 27 de `escritorio_visor`, 24
  de `escritorio_rfb`, 12 de `escritorio_rdp`, 9 de `escritorio_voxl`, 1 de
  `escritorio_e2e`, 7 de `sincronizacion_escritorios`).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked
  -- -D warnings`, `cargo deny check`, `actionlint`,
  `desktop-file-validate assets/magi.desktop`, `shellcheck scripts/*.sh
  crates/magi-nucleo/scripts/*.sh`, `dist plan --tag v1.2.0`,
  `npm --prefix visor run check` y `scripts/generar-visor.sh --comprobar`:
  todos en verde.
- `tests/recursos` en verde tras regenerar `assets/` (la página man queda con
  la fecha 2026-11-04 y la versión 1.2.0).
- La revisión adversarial está en `docs/fase12/magi-fase12-revision-adversarial.md`
  con el estado de cada corrección al final del documento.


- **Compilar y arrancar**: `cargo run` (TUI). Servidor en primer plano:
  `cargo run -- --servidor` y, en otra terminal, `cargo run`. La migración 8
  se aplica sola al abrir `magi.db`; una base de datos de la versión 8 no la
  abre un binario 1.1.
- **Visor web**: compilar MAGI no necesita Node. Tras tocar `visor/src`:
  `npm --prefix visor ci`, `npm --prefix visor run check` y
  `scripts/generar-visor.sh` (con `--comprobar` para CI).
- **Batería completa**: `TZ=UTC cargo test --workspace --locked` (en este
  árbol: todas las suites en verde; las de escritorio no usan red externa:
  servidor RFB de prueba, `ironrdp-server` y servidor SSH de prueba con un
  voxl falso).
- **Gates de calidad**: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo deny check`, `actionlint`, `desktop-file-validate
  assets/magi.desktop`, `shellcheck scripts/*.sh
  crates/magi-nucleo/scripts/*.sh`, `dist plan --tag v1.2.0`,
  `npm --prefix visor run check` y `scripts/generar-visor.sh --comprobar`:
  todos en verde en este árbol.
- **Diagnóstico**: `cargo run -- diagnostico` (dice qué navegador abrirá el
  visor y avisa si no encuentra ninguno).
- **Pruebas de la fase por pista**:
  - `cargo test -p magi-nucleo --test escritorio_rfb --test escritorio_visor
    --test escritorio_rdp --test escritorio_voxl --test escritorio_e2e`
  - `cargo test -p magi-ssh --test escritorio_servidor --test escritorio_ui
    --test redimensionado`
  - `cargo test -p magi-nucleo --test sincronizacion_escritorios`
  - `cargo test -p magi-ssh --test protocolo --test recursos`
- **Guion manual**: §11 del informe funcional (`magi-fase12-informe.md`).
