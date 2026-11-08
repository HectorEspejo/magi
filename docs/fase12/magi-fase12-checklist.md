# MAGI - Checklist Fase 12: Escritorio Remoto

## Inventario y modelo
- [x] Migración 8: HOSTS gana `tipo` (`ssh` por defecto), `escritorio_dominio`, `escritorio_opciones` (JSON) y `huella_escritorio`; `user_version` 8
  - AC: Dado un `magi.db` en la versión 7, cuando arranca MAGI 1.2, entonces todos los hosts quedan de tipo `ssh` y no cambia nada más
- [x] Tipos de host `ssh`, `rdp`, `vnc` y `voxl`, validados en código (sin `CHECK`), con puerto por defecto 22, 3389, 5900 y 6901
- [x] El tipo se elige al crear el host y no se puede cambiar después
  - AC: Dado un host RDP, cuando se edita, entonces el tipo aparece fijo, y una fila con el tipo cambiado por fuera se rechaza al leerla (T38)
- [x] «A través de» (`salto_host_id`) solo admite hosts SSH: opcional en RDP y VNC, obligatorio en voxl y sin ciclos
- [x] En voxl la dirección es siempre `127.0.0.1` del host SSH, el puerto empieza en 1024 y se piden la pantalla X11 (`:N` o `:N.M`) y el programa (`voxl` o una ruta absoluta)
  - AC: Dado «:1» y el puerto 6901, cuando se guarda, entonces se acepta; con «:1; rm» o un programa con espacios, se rechaza con el motivo
- [x] `escritorio_opciones` por tipo (tamaño, portapapeles, teclado en RDP, pantalla y programa en voxl, `directo` en VNC), validado al guardar y al leer; las claves desconocidas se conservan
- [x] Contraseña de RDP y VNC en el llavero por `uuid` del host (`contrasena:llavero`), pedida siempre (`contrasena`) o ninguna (VNC sin contraseña); nunca en `magi.db` (T19)
- [x] Usuario RDP escrito como `DOMINIO\usuario` se separa en dominio y usuario al guardar
- [x] `huella_escritorio` es propia del equipo: no viaja ni sube la versión del host
- [x] Los hosts que no son SSH no se exportan a `magi_config`
  - AC: Dado un inventario con 3 hosts SSH y 2 escritorios, cuando se exporta, entonces `magi_config` tiene 3 entradas `Host` y el `ssh` real lo lee sin errores
- [x] Los hosts que no son SSH no pueden ser destino de terminal, SFTP, túneles, snippets ni salto: no salen en esos selectores y un snippet por etiqueta los omite y lo dice
- [x] Un VNC directo a una dirección que no es de loopback pide confirmación al guardarlo o al abrirlo si no la tiene (`directo`), y la ficha muestra `⚠`
  - AC: Dado un VNC que iba a través de un host SSH borrado en otro equipo, cuando se abre, entonces MAGI avisa «la imagen y el teclado viajarán sin cifrar» y solo sigue si se acepta

## Vistas de la TUI
- [x] Formulario de host con selector de tipo (SSH, RDP, VNC, voxl) que enseña solo los campos de ese tipo
- [x] Ficha de un escritorio con el bloque «Escritorio»: tipo, destino, a través de, usuario y dominio, contraseña guardada (sí o no), tamaño, teclado, portapapeles, huella del certificado y última conexión
- [x] «Olvidar huella del certificado» y «Olvidar contraseña» en la ficha, con confirmación
- [x] Hosts: los escritorios llevan `▣` y su tipo además del estado, `⤴` para «a través de» y `⚠ directo`, con texto en modo ASCII (T5)
- [x] `↵` en un escritorio lo abre; si ya tiene sesión, va a su pestaña y abre el visor con un enlace nuevo
- [x] Paleta (`Ctrl+P`) y búsqueda incluyen los escritorios por nombre, dirección, tipo y etiquetas
- [x] Flota lista los escritorios sin sondearlos, con su último estado y «sin sondeo»
- [x] `magi conectar <escritorio>` abre MAGI en la pestaña del escritorio y el visor
- [x] Pestaña `▣` en Sesión con la tarjeta del escritorio: estado, destino, a través de, usuario, tamaño, imagen (H.264, rectángulos o voxl), visor, certificado y tiempo
  - AC: Dado un escritorio abierto sin visor, cuando pasan 15 s, entonces la tarjeta dice «sin visor · se cierra en 45 s»
- [x] En la pestaña: `↵` abre el visor, `y` copia un enlace de un solo uso, `^] r` reconecta y `^] x` cierra con confirmación (T6)
- [x] Vista Sesiones con columna TIPO; las de escritorio se reconectan y se cierran como las de terminal
- [x] Diálogo «Certificado nuevo» (ámbar) con sujeto, caducidad y huella `SHA256:`; aceptarla la guarda y queda en REGISTRO
- [x] Diálogo «Certificado cambiado» en rojo con la huella anterior y la nueva; sustituirla exige escribir el nombre del host
- [x] Diálogo de credenciales: usuario y dominio (RDP), contraseña enmascarada, «recordar en el llavero» y 3 intentos
- [x] Instantáneas `insta` de Hosts con escritorios, formulario RDP, ficha, pestaña de escritorio y diálogos de certificado a 40×12, 80×24 y 200×60 (T45)
- [x] REGISTRO: `escritorio_abierto`, `escritorio_cerrado` (con duración y motivo), `escritorio_fallido` (con motivo) y `huella_escritorio` (aceptada o sustituida), siempre por `registro::anotar`

## Sesiones de escritorio (servidor)
- [x] Las sesiones de escritorio viven en `magi --servidor`: sobreviven a cerrar la TUI y se ven desde todas las ventanas
- [x] Difusión de la lista completa `Escritorios{lista}` en cada cambio de estado (T27)
- [x] Estados abriendo (conectando, verificando certificado, autenticando, arrancando voxl), abierto (con visor o sin visor), caído y cerrando
- [x] Camino por SSH con `conexion_para_canal` (uso `Escritorio`) y un canal `direct-tcpip` al destino; directo, con TCP y plazo de 10 s
  - AC: Dado un VNC en `127.0.0.1:5901` a través de web-01, cuando se abre, entonces no se abre en el equipo ningún puerto salvo el del visor
- [x] Un canal de escritorio mantiene viva la conexión del pool, como un túnel, y se suelta por identidad en todas las ramas (T34)
- [x] Plazos: 10 s para TCP, saludos TLS y RFB y apertura de canal; 5 min para los diálogos; ninguna espera sin plazo (T32)
- [x] Gracia: una sesión abierta sin visor durante `gracia_seg` (60 s) se cierra sola; abrir el visor antes la conserva
  - AC: Dado un escritorio con el visor cerrado, cuando pasan 60 s, entonces se cierra y REGISTRO anota `escritorio_cerrado` con «sin visor durante 60 s»
- [x] Un visor por sesión: abrir otro sustituye al anterior, que enseña «abierto en otra ventana»
- [x] Conexión caída (también la del host «a través de»): estado caído con el motivo, la pestaña se conserva y se reconecta con `r` o desde el visor si hay una ventana de MAGI abierta
- [x] El servidor no dialoga ni toca el llavero: el certificado y las credenciales se piden al cliente solicitante (T25), que es quien guarda la huella
- [x] `TrabajoServidor.escritorios`: un servidor con escritorios abiertos no se sustituye sin preguntar (T67) y `magi servidor parar` los cuenta
- [x] La decodificación va en un hilo por sesión, fuera del runtime de red, y la TUI nunca recibe píxeles

## Visor en el navegador
- [x] Visor en `visor/`, adaptado del cliente web de voxl 0.3.3, con el aviso MIT en `visor/LICENSE-voxl` y el origen y los cambios en `visor/README.md`
- [x] El visor pinta rectángulos en bruto y copias en un `OffscreenCanvas` dentro de un worker y presenta cada lote al recibir `Marco`
- [x] El visor decodifica H.264 con WebCodecs cuando llega `VideoRdp` y solo pinta las regiones de cada marco
- [x] Teclado: manda el keysym y el código físico de cada tecla y suelta todas las teclas al perder el foco
- [x] Ratón y rueda (con desplazamiento suave), y cursor remoto dibujado por el visor
- [x] Portapapeles de texto en los dos sentidos, hasta 1 MB, si el host lo tiene activado
- [x] Resolución que se ajusta a la ventana (agrupada 500 ms); si el servidor no la cambia o el host la tiene fija, se escala sin deformar
- [x] Pantalla completa con `navigator.keyboard.lock` donde exista, para que Esc, Alt+Tab y la tecla Windows lleguen al remoto
- [x] Barra superior que se oculta a los 2 s: nombre y tipo, estado, fps y latencia, pantalla completa, Ctrl+Alt+Supr, tecla Windows (RDP) y cerrar visor
- [x] Avisos sobre la imagen: conectando, caído (con «Reconectar»), cerrado, «abierto en otra ventana» y enlace usado o caducado
- [x] Con la pestaña oculta el visor pausa la imagen (`v,0`) y al volver pide el escritorio entero
- [x] Estética de cabina (paleta de MAGI, fuente del sistema, sin recursos externos) y botones con texto y `aria-label`
- [x] `visor/dist/` se genera con `scripts/generar-visor.sh` (esbuild fijado) y va dentro del binario: compilar MAGI no necesita Node
  - AC: Dado un cambio en `visor/src`, cuando la CI ejecuta `scripts/generar-visor.sh --comprobar`, entonces falla si `visor/dist` no está al día

## Servidor del visor y seguridad
- [x] Servidor HTTP y WebSocket del visor dentro de `magi --servidor`, solo en `127.0.0.1` (puerto libre o `[escritorio] puerto_visor`), que arranca con la primera sesión de escritorio y se para con la última
- [x] Enlaces de un solo uso: 256 bits, caducan a los 60 s, se gastan al canjearlos aunque falle algo y se anulan al cerrar la sesión
  - AC: Dado un enlace ya canjeado, cuando se abre otra vez, entonces el visor dice «enlace usado o caducado: ábrelo otra vez desde MAGI» y no conecta
- [x] Clave del visor por sesión, guardada en `sessionStorage` (sobrevive a recargar la pestaña); la URL se limpia con `history.replaceState`
- [x] El WebSocket exige como primer mensaje `{"t":"hola","clave":…,"h264":…}` en 5 s; si no, se cierra
- [x] `Host` comprobado en todas las rutas (127.0.0.1 o localhost con el puerto propio) y `Origin` obligatorio en `/api/*`
- [x] Cabeceras: CSP estricta sin scripts en línea, `nosniff`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` y `frame-ancestors 'none'`
- [x] Tras 10 canjes fallidos en un minuto, `/api/canjear` responde 429 durante un minuto
- [x] Todo lo que llega del visor se valida: coordenadas dentro del escritorio, códigos de tecla de la tabla, tamaños entre 200×200 y 8192×8192 y texto de hasta 1 MB
- [x] Nada de la sesión va al log ni a REGISTRO: ni píxeles, ni teclas, ni portapapeles, ni contraseñas (T44)
- [x] El cliente abre el visor sin shell: `[escritorio] navegador = "auto"` busca Chromium, Brave, Chrome o Edge y usa `--app=`; si no hay, `xdg-open` (Linux) u `open` (macOS)
  - AC: Dado `navegador = "brave --app={url}"`, cuando se abre un escritorio, entonces se lanza Brave con la URL en un solo argumento
- [x] Si no se puede lanzar el navegador, la pestaña enseña el enlace para copiarlo

## RDP
- [x] Cliente RDP con IronRDP fijado con `=` en `magi-nucleo`, con TLS de rustls y la misma cripto que russh, sin OpenSSL
- [x] TLS 1.2 o 1.3 con NLA (CredSSP con NTLM) y, si el servidor no la exige, TLS sin NLA
- [x] Huella `SHA256:` del certificado comprobada antes de mandar credenciales: nueva, diálogo; distinta, bloqueo; igual, sigue
  - AC: Dado un Windows con el certificado regenerado, cuando se abre, entonces sale el bloqueo en rojo y no se envía ninguna credencial
- [x] Credenciales del llavero sin diálogo; si fallan o no hay, diálogo con 3 intentos y el motivo «usuario o contraseña incorrectos»
- [x] Mapas de bits, RemoteFX y lo demás que decodifica IronRDP llegan al visor como rectángulos
- [ ] Con EGFX y AVC420, el H.264 de Windows llega al visor sin recomprimir; si el visor no decodifica H.264, MAGI reconecta sin AVC420
  - AC: Dado Windows 11 y el visor en Chromium, cuando se abre, entonces la barra dice «H.264» y MAGI no decodifica ni recomprime vídeo
- [x] Teclado por scancodes con la distribución del host o la de `[escritorio] distribucion_teclado` (`auto` sale de `LANG`) y eventos Unicode de reserva
  - AC: Dado un Windows en español, cuando se escriben ñ, @ y € (AltGr) en el visor, entonces aparecen igual en el remoto
- [x] Ctrl+Alt+Supr y tecla Windows desde la barra del visor
- [x] Portapapeles de texto con CLIPRDR (`CF_UNICODETEXT`)
- [x] Cambio de tamaño en vivo con Display Control; sin ese canal, tamaño inicial y escalado
- [x] Forma y posición del puntero remoto en el visor
- [x] Un servidor que solo admite RDP antiguo sin TLS falla con «este servidor solo habla RDP antiguo, sin TLS: no admitido»

## VNC
- [x] Cliente RFB propio en `magi-nucleo` (RFC 6143), versiones 3.3, 3.7 y 3.8
- [x] Seguridad `None` y `VNC Authentication` (DES con los bits de cada byte invertidos; aviso si la contraseña pasa de 8 caracteres); otro tipo falla con su número
- [x] Codificaciones Raw, CopyRect, Hextile y ZRLE, y seudocodificaciones DesktopSize, ExtendedDesktopSize y Cursor
- [x] Escritorio compartido (`shared-flag`): abrir MAGI no echa a otros visores
- [x] Peticiones de actualización incrementales al ritmo del visor, como mucho 60 por segundo
- [x] Teclado con keysyms y, si el servidor la anuncia, la extensión de teclas de QEMU con scancodes
- [x] Portapapeles de texto: Latin-1 (lo que no cabe se sustituye con aviso) y UTF-8 si el servidor anuncia el portapapeles extendido
- [x] Cambio de tamaño con `SetDesktopSize` si el servidor anuncia ExtendedDesktopSize; si no, escalado
- [x] Límites contra servidores maliciosos: escritorio de hasta 8192×8192, rectángulos dentro del escritorio, ZRLE acotado, cursor de hasta 256×256 y texto de hasta 1 MB
  - AC: Dado un servidor de prueba que manda un rectángulo fuera del escritorio, cuando llega, entonces la sesión cae con «datos inválidos del servidor», sin pánico

## voxl por SSH
- [x] voxl se arranca en el host SSH con el script embebido `escritorio_voxl.sh` (`include_str!`), con pantalla, puerto, programa y portapapeles como argumentos validados en Rust y en el script
- [x] La contraseña de voxl es aleatoria por sesión y le llega por la entrada estándar; nunca en la línea de órdenes ni en el log
- [x] voxl escucha en `127.0.0.1` del host, con `--file-transfers=none`, sin sonido ni micrófono, con redimensionado y con el portapapeles según el host
- [x] MAGI exige voxl 0.3 o posterior y espera a `/healthz` (plazo de 10 s) antes de dar la sesión por abierta
- [x] Túnel de la sesión en `127.0.0.1` (puerto libre) hasta el voxl del host; no se guarda en TUNELES ni sale en la vista Túneles
- [x] La lanzadera canjea el enlace y lleva el navegador a la página de voxl a través del túnel
- [x] Al cerrar la sesión, por gracia o si cae la conexión SSH, el script recibe el fin de la entrada y para voxl (TERM y, a los 5 s, KILL)
  - AC: Dado un voxl abierto, cuando se cierra su pestaña en MAGI, entonces a los 6 s `pgrep -f "voxl --addr=127.0.0.1"` en el host no devuelve nada
- [x] Errores legibles: «voxl no está instalado en <host>», «la pantalla :1 no existe», «el puerto 6901 del host está ocupado», «actualiza voxl a la 0.3 o posterior» y «voxl no respondió en 10 s»
- [x] La gracia de voxl cuenta las conexiones abiertas del túnel: sin ninguna durante 60 s, la sesión se cierra

## Sincronización y app Android
- [x] `tipo`, `escritorio_dominio` y `escritorio_opciones` viajan con el host; `huella_escritorio` y las contraseñas no
- [x] El fichero sube a `formato` 2 solo si el inventario tiene algún host que no es SSH; si no, sigue en `formato` 1
  - AC: Dado un inventario solo con hosts SSH, cuando se exporta, entonces el fichero es `formato` 1 y un equipo con MAGI 1.1 lo importa
- [x] MAGI 1.2 lee el `formato` 1 (todos los hosts, SSH) y el 2; un equipo con 1.1 ve el de 1.2 como `formato_nuevo`
- [x] `docs/formato-sincronizacion.md` documenta el `formato` 2 y los campos nuevos
- [x] La app Android lee el `formato` 2 y enseña los escritorios en Hosts con su tipo y «se abre en el ordenador», sin conectar ni crear túneles en ellos
- [x] `android/CHANGELOG.md` recoge el cambio en «Sin publicar»

## Protocolo, configuración y diagnóstico
- [x] `VERSION_PROTOCOLO = 7` con los mensajes de escritorio; `Hola` y `VersionIncompatible` sin cambios (T66)
- [x] `peticion_id` de escritorios en su propio rango, desde 2^46
- [x] `config.toml` `[escritorio]`: `navegador`, `gracia_seg`, `puerto_visor`, `distribucion_teclado` y `h264`, validados con error claro
- [x] `magi diagnostico` dice qué navegador abrirá el visor y avisa si no encuentra ninguno

## API: servidor del visor y protocolo
- [x] GET /entrar
- [x] GET /visor.js · /trabajador.js · /visor.css
- [x] POST /api/canjear
- [x] GET /api/ws
- [x] GET /api/salud
- [x] Mensajes del servidor al visor: Cursor (`0x03`), Rect (`0x05`), Copia (`0x06`), VideoRdp (`0x07`), Marco (`0x08`) y los de texto `hello`, `estado`, `tam`, `clip`, `ping` y `close`
- [x] Órdenes del visor: `kc`, `m`, `b`, `sp`, `r`, `c`, `sz`, `v`, `kf`, `cad`, `win` y `po`
- [x] `AbrirEscritorio` → `EscritorioAbierto`
- [x] `PedirEnlace` → `Enlace`
- [x] `ReconectarEscritorio` y `CerrarEscritorio`
- [x] `Escritorios{lista}`
- [x] `PideHuellaEscritorio` → `RespuestaHuellaEscritorio`
- [x] `PideCredenciales` → `Credenciales` (contraseña en `Zeroizing`)

## Calidad, pruebas y publicación
- [x] Pruebas VNC con un servidor RFB de prueba en proceso: saludo, DES con un vector conocido, codificaciones (píxeles exactos), entrada, portapapeles, tamaño y límites
- [x] Pruebas RDP con `ironrdp-server`: TLS, huella nueva, igual y cambiada, credenciales, mapas de bits, AVC420 pasado tal cual, teclado, portapapeles y tamaño
- [x] Pruebas del servidor del visor: enlaces (uso único, caducidad, anulación), `Host`, `Origin`, cabeceras, clave, sustitución de visor y 429
- [x] Prueba de extremo a extremo sin red externa: servidor VNC de prueba → puente → cliente WebSocket de prueba, con píxeles y entrada de ida y vuelta (T95)
- [x] Prueba de voxl con el servidor SSH de prueba y un `voxl` falso: argumentos, contraseña por la entrada estándar, `/healthz`, túnel y parada al cerrar
- [x] Pruebas de sincronización: formato 1 sin escritorios, formato 2 con ellos, lectura de los dos y `formato_nuevo`
- [x] `npm --prefix visor run check` (TypeScript, formato y lint) y `scripts/generar-visor.sh --comprobar` en CI, con las dependencias de npm fijadas por integridad
- [x] `cargo deny check` en verde sin licencias GPL; IronRDP, `sspi`, `des` y `axum` en la lista de licencias
- [x] Versión 1.2.0: `Cargo.toml`, fecha de la página man, `assets/` regenerados y `CHANGELOG.md`
- [x] README con la sección «Escritorio remoto»: tipos, a través de SSH, navegador, voxl en el host y seguridad
- [x] Revisión adversarial de cierre (T31, T54) del servidor de escritorios, el servidor del visor, el puente y el cliente RFB, anotada en el informe de implementación

---

**Progreso Fase 12:** 127 / 128 funcionalidades

**Total MAGI (Fases 1-12):** 957 / 962 funcionalidades
