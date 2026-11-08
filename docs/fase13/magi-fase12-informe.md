# MAGI - Fase 12: Escritorio Remoto

## Especificación Funcional

**Versión:** 1.2
**Fecha:** 4 de noviembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

**Changelog**
- 8 nov 2026 — v1.2: «Android, segunda vuelta» pasa a ser la F14, porque la F13 es Servicios, Contenedores y Logs (orden de las fases 13 a 21 aceptado por Hector). Se actualizan las referencias; sin cambios de alcance.
- 5 nov 2026 — v1.1: ajustada al informe de implementación (128/128). IronRDP 0.17 con `ironrdp-egfx` 0.3 y un `DvcProcessor` propio para pasar el AVC420 sin decodificar; `ironrdp-connector` e `ironrdp-acceptor` vendorizados con `sspi` 0.23.1 y `picky` 7.0.0-rc.27 para compartir `ecdsa` con russh 0.63. Protocolo del visor ampliado (`GET /`, `rec`, `latencia`; el canje devuelve el nombre legible). `CerrarEscritorio` durante «abriendo» cancela. Los diálogos que el servidor inicia desde el visor usan `[2^47, 2^48)`. Un hilo con runtime propio por sesión. Limitación: con H.264 no se pide la imagen entera al volver de la pestaña oculta (R117). D228-D233.

> Base: Fases 1-11 con informe procesado. La F11 está al 117/117 con la corrección 11b validada en el Honor; le quedan el resto del guion del plegable, la CI de GitHub, la primera release con keystore y los trámites de cierre de su prompt de reanudación (R103, R104 y R107), que no bloquean esta fase. La F10 sigue sin validación manual y la corrección 10b, pendiente. La F9 sigue pendiente de su primera publicación. Esta fase ocupa el puesto de «Android, segunda vuelta», que pasa a ser la F13 (desde el 8 nov 2026, la F14).
>
> Estado (5 nov 2026): implementación reportada, 128/128, con todas las pruebas automáticas en verde y la revisión adversarial hecha (`docs/fase12/magi-fase12-revision-adversarial.md`). Quedan la validación manual (§11), el AVC420 con Windows real, la prueba instrumentada de Android y la limitación de R117.

---

## 1. Visión General

La Fase 12 añade a MAGI **escritorios remotos**: abrir el escritorio gráfico de un Windows, de una máquina virtual o de un Linux desde el mismo inventario en el que ya están los hosts SSH. Habla los dos protocolos de siempre, **RDP** y **VNC**, y además sabe arrancar **voxl** en un host Linux por SSH.

MAGI es una TUI y Alacritty no pinta imágenes, así que el escritorio se ve en **una ventana del navegador** (Chromium o Brave en modo app, sin pestañas ni barra). La página la sirve el propio `magi --servidor`, solo en `127.0.0.1`, y se abre con un enlace de un solo uso. El visor es el **cliente web de voxl adaptado**: un programa pequeño en TypeScript (licencia MIT) que pinta con WebCodecs y un canvas en un worker, con teclado, ratón, portapapeles y cambio de tamaño ya resueltos.

**Qué es voxl.** voxl ([tembo/voxl](https://github.com/tembo/voxl), versión 0.3.3) no es un cliente de RDP ni de VNC: es un servidor en Rust que se instala en la máquina remota (Linux con X11 o Xvfb, Mac o Windows 10 en adelante), captura su pantalla y la manda como vídeo H.264 por un WebSocket a una pestaña del navegador. Llega a 60 fps donde noVNC no pasa de 8. Su código es MIT, pero enlaza x264, que es GPL. MAGI aprovecha dos cosas de voxl: **su visor**, copiado y adaptado a `visor/`, y **el propio voxl** en los hosts Linux que lo tengan instalado, arrancado por SSH y visto a través de un túnel. MAGI no enlaza voxl ni x264.

Por dentro, MAGI habla RDP con **IronRDP** (Rust, MIT/Apache) y VNC con un **cliente RFB propio**. Pasa la imagen al visor **sin volver a comprimirla**: los trozos de pantalla que cambian van en bruto por `127.0.0.1` y, si Windows ya manda vídeo H.264, se pasa tal cual. Siempre que se puede, la conexión va **por SSH**, por un canal del pool de un host del inventario. Las sesiones de escritorio las custodia el servidor de sesiones, como las de terminal.

**Objetivos principales**

1. Hosts de tipo RDP, VNC y voxl en el inventario, con grupo, etiquetas, búsqueda y sincronización, y «a través de» un host SSH.
2. Visor web servido por `magi --servidor` en `127.0.0.1` con enlaces de un solo uso: imagen, teclado, ratón, portapapeles de texto, resolución que se ajusta a la ventana y pantalla completa.
3. Cliente RDP con IronRDP: TLS, NLA y huella del certificado con la misma política que las huellas SSH; el H.264 de Windows llega al navegador sin recomprimir.
4. Cliente VNC propio para máquinas virtuales (QEMU, libvirt, Proxmox) y Linux (TigerVNC, x11vnc).
5. voxl por SSH en hosts Linux con X11: MAGI lo arranca en `127.0.0.1`, lo abre por un túnel y lo para al cerrar.
6. Sesiones de escritorio custodiadas por el servidor, en pestañas de Sesión, con un minuto de gracia sin visor; todo sin dependencias GPL.

**Contexto.** Producto propio. Hector trabaja en Omarchy (Hyprland, Alacritty y Chromium). Los escritorios que más usa son máquinas virtuales de QEMU (también Windows XP y 2000, que se ven por el VNC de QEMU), Windows en red y Linux. Llega a sus hosts por Tailscale y por saltos SSH.

Respuestas de Hector (4 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | RDP y VNC con el visor de voxl, y voxl por SSH en hosts Linux con X11 (a) |
| 2 | Visor en una ventana del navegador, servido por MAGI en `127.0.0.1` con una clave aleatoria (a) |
| 3 | Sin recomprimir: rectángulos y H.264 de Windows tal cual; sin x264 (a) |
| 4 | Android en una fase posterior; el móvil ve los escritorios sin abrirlos (a) |
| 5 | Windows por RDP, máquinas virtuales por VNC y Linux por RDP o VNC (a) |
| 6 | Por SSH siempre que se pueda; directo solo RDP, y VNC con aviso (a) |
| 7 | Hosts de otro tipo con «a través de» un host SSH (a) |
| 8 | Imagen, teclado, ratón, portapapeles de texto, resolución que se ajusta y pantalla completa (a) |
| 9 | Sesión custodiada por el servidor, con un minuto de gracia sin visor (a) |
| 10 | Es la F12; Android, segunda vuelta, pasa a F13 (a) |

**Fuera de alcance**

- Sonido, micrófono, imágenes en el portapapeles y ficheros (redirección de unidades en RDP, ficheros de voxl).
- Varias pantallas, impresoras, tarjetas inteligentes y USB.
- RDP antiguo sin TLS (Windows XP y 2003 directos), RD Gateway, Kerberos y tarjeta inteligente.
- Macs con Compartir pantalla (autenticación de Apple) y VNC con TLS (VeNCrypt).
- Grabar, modo «solo ver» y varios visores a la vez en la misma sesión.
- Instalar voxl en los hosts, voxl ya arrancado por el usuario y voxl con Wayland.
- Abrir escritorios desde el móvil (F14 o más adelante).

**Versión prevista:** escritorio **1.2.0** (MINOR: funcionalidad nueva, migración 8, protocolo v7 y `formato` 2 de sincronización, T68). La app Android solo cambia lo justo para leer el `formato` 2 (línea en su CHANGELOG; la versión la decide el desarrollador al publicar).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| RDP | IronRDP 0.17 (`ironrdp-connector`, `-session`, `-graphics`, `-cliprdr`, `-displaycontrol`, `-tokio`) y `ironrdp-egfx` `=0.3.0`, fijados con `=`; `ironrdp-connector` e `ironrdp-acceptor` vendorizados en `vendor/` con `sspi` 0.23.1 y `picky` 7.0.0-rc.27 (D228) | MIT/Apache; TLS con rustls y la misma cripto que russh (`aws-lc-rs`), sin OpenSSL; NLA con `sspi` (NTLM); el AVC420 sale por un `DvcProcessor` propio (D229) |
| VNC | Cliente RFB propio (RFC 6143) en `magi-nucleo` | `des` (RustCrypto) para la autenticación VNC y `flate2` para ZRLE |
| Servidor del visor | `axum` 0.8 (HTTP y WebSocket) sobre tokio, dentro de `magi --servidor` | Solo `127.0.0.1`; ficheros del visor incluidos en el binario |
| Visor | TypeScript adaptado del cliente web de voxl 0.3.3 (MIT) | WebCodecs (H.264), `OffscreenCanvas` en un worker; generado con esbuild fijado y versionado en `visor/dist/` |
| voxl (opcional) | voxl-server 0.3.x, instalado por el usuario en el host | MAGI lo arranca con un script embebido; no lo enlaza ni lo distribuye |
| Navegador | Chromium, Brave o Chrome con `--app=`; si no hay, `xdg-open` u `open` | WebCodecs con H.264: Chromium 94+, Firefox 130+ (Firefox no tiene modo app) |
| Sincronización | `formato` 2 cuando hay hosts que no son SSH (F10) | La app Android (F11) lo lee |
| Licencias | Sin dependencias GPL; `cargo deny` | El visor conserva el aviso MIT de voxl |

### Estructura de carpetas (novedades)

```
crates/magi-nucleo/
├── src/escritorio/
│   ├── mod.rs          tipos, opciones, eventos, órdenes, diálogos, errores y el hilo por sesión
│   │                   (lanzar_en_hilo_con)
│   ├── rdp/            mod, conexion, tls (verificador que captura el certificado), certificado,
│   │                   transporte, sesion, egfx (DvcProcessor del AVC420), cliprdr, entrada
│   ├── rfb/            mod, protocolo, seguridad (None y VNC), hextile, zrle
│   ├── teclas.rs       código físico del navegador → scancode RDP; keysym → Unicode
│   ├── puente.rs       búfer del escritorio, zonas sucias, rectángulos, copias, vídeo, cursor, entrada
│   ├── visor_http.rs   servidor HTTP + WebSocket: enlaces, Host/Origin, CSP, ficheros del visor
│   └── voxl.rs         arranque remoto, espera de /healthz, túnel de la sesión, lanzadera
├── scripts/escritorio_voxl.sh
└── tests/              escritorio_rfb.rs, escritorio_rdp.rs, escritorio_visor.rs, escritorio_voxl.rs,
                        escritorio_e2e.rs, sincronizacion_escritorios.rs
src/servidor/escritorios.rs   custodia, pool (UsoCanal::Escritorio), diálogos, gracia, Escritorios{lista}
src/app/escritorios.rs · src/app/lanzar.rs   pestaña ▣, diálogos y abrir el navegador sin shell
src/ui/escritorios.rs y el resto de src/ui/   formulario y ficha por tipo, Hosts, Sesiones, Flota
tests/escritorio_servidor.rs · tests/escritorio_ui.rs · tests/redimensionado/   servidor e instantáneas
visor/                        entrar.html, src/, dist/ (visor.js, trabajador.js, visor.css; versionado),
                              package.json, package-lock.json, LICENSE-voxl y README.md (origen y cambios)
scripts/generar-visor.sh      genera visor/dist con esbuild; --comprobar para la CI
vendor/ironrdp-connector · vendor/ironrdp-acceptor   IronRDP con sspi 0.23.1 y picky 7.0.0-rc.27 (D228)
android/                      leer el formato 2 y enseñar los escritorios sin abrirlos
docs/formato-sincronizacion.md   formato 2
docs/fase12/magi-fase12-revision-adversarial.md   revisión adversarial de cierre y estado de cada corrección
```

### Diagrama de arquitectura

```
 EQUIPO DEL USUARIO (Omarchy o macOS)
                                        ┌─ magi --servidor ───────────────────────────────────────────┐
 ┌────────────────────────┐             │                                                             │
 │ TUI magi (cliente)     │ socket Unix │  ┌───────────────┐  ┌───────────────┐  ┌─────────────────┐  │
 │ Hosts · pestañas ▣     │◀───────────▶│  │ escritorios   │  │ pool SSH      │  │ servidor del    │  │
 │ diálogos: certificado  │ protocolo v7│  │ custodia      │  │ conexion_     │  │ visor HTTP+WS   │  │
 │ y credenciales         │             │  │ gracia 60 s   │  │ para_canal    │  │ solo 127.0.0.1  │  │
 │ abre el navegador      │             │  │ lista         │  │ (T42)         │  │ enlaces 1 uso   │  │
 │ lee el llavero         │             │  └───────────────┘  └───────────────┘  └─────────────────┘  │
 └────────────────────────┘             │                                                 │           │
             │                          │                                                 │           │
             │ --app=…/entrar?t=…       │                                                 ▼           │
             ▼                          │  ┌───────────────┐  ┌───────────────┐  ┌─────────────────┐  │
 ┌────────────────────────┐             │  │ cliente RDP   │  │ cliente VNC   │  │ puente          │  │
 │ navegador (visor)      │ WebSocket   │  │ (IronRDP)     │  │ (RFB propio)  │  │ rectángulos     │  │
 │ adaptado de voxl       │◀───────────▶│  │ TLS · NLA     │  │ ZRLE · Hextile│─▶│ H.264 tal cual  │  │
 │ WebCodecs · canvas     │ 127.0.0.1:P │  │ huella        │  │ CopyRect      │  │ entrada · copia │  │
 │ worker                 │             │  └───────────────┘  └───────────────┘  └─────────────────┘  │
 └────────────────────────┘             │          │                  │                   ▲           │
                                        │          ├──────────────────┼───────────────────┘           │
                                        │          │                  │                               │
                                        └────┬─────┼──────────────────┼───────────────────────────────┘
                 SSH (pool)                  │     │                  │
              ╭──────────────────────────────┘     │ directo (RDP con TLS) o canal direct-tcpip del pool
              ▼                                    ▼                  ▼
 ┌────────────────────────┐             ┌─────────────────────────────────────────────────────────────┐
 │ host SSH               │────────────▶│ Windows · xrdp · GNOME/KDE (RDP 3389)                       │
 │ «a través de»          │ direct-tcpip│ QEMU · libvirt · Proxmox · TigerVNC (VNC 5900)              │
 └────────────────────────┘             └─────────────────────────────────────────────────────────────┘

```

La TUI nunca recibe píxeles: recibe la lista de escritorios y los diálogos, y abre el navegador con el enlace que le da el servidor. El servidor no dialoga ni toca el llavero (T25): pide la confirmación del certificado y las credenciales al cliente que abrió el escritorio. La decodificación de RDP y VNC corre en un hilo por sesión, fuera del runtime de red.

### voxl por SSH

```
 ┌──────────────────────┐           ┌─ magi --servidor ──────────┐          ┌─ host Linux (SSH) ──────┐
 │ TUI magi             │ v7        │                            │          │                         │
 │ abre el navegador    │──────────▶│ ┌────────────────────────┐ │exec (SSH)│ ┌─────────────────────┐ │
 │ en la lanzadera      │           │ │ escritorios: voxl      │─┼──────────┼▶│ escritorio_voxl.sh  │ │
 └──────────────────────┘           │ │ arranca, espera, para  │ │          │ │ contraseña por stdin│ │
            │                       │ └────────────────────────┘ │          │ └─────────────────────┘ │
            │ /entrar → canje       │                            │          │            ▼            │
            ▼                       │ ┌────────────────────────┐ │          │ ┌─────────────────────┐ │
 ┌──────────────────────┐           │ │ túnel de la sesión     │ │canal SSH │ │ voxl 0.3            │ │
 │ navegador            │ HTTP + WS │ │ 127.0.0.1:P2 → host    │─┼──────────┼▶│ 127.0.0.1:6901      │ │
 │ página de voxl       │◀──────────┼▶│ 127.0.0.1:6901         │ │          │ │ DISPLAY=:1 (X11)    │ │
 │ (su propio visor)    │           │ └────────────────────────┘ │          │ └─────────────────────┘ │
 └──────────────────────┘           └────────────────────────────┘          └─────────────────────────┘

```

Con voxl no hay puente: el navegador habla con el propio voxl a través del túnel de la sesión. MAGI arranca voxl, le da una contraseña aleatoria por la entrada estándar, espera a que responda y lo para al cerrar.

---

## 3. Modelo de Datos

No hay tablas nuevas. La **migración 8** añade cuatro columnas a HOSTS; las sesiones de escritorio viven en la memoria del servidor, como las de terminal (T22, T23).

### Diagrama E-R (cambios de la fase)

```
 ┌────────────────────────────────────────────┐             ┌────────────────────────────────┐
 │ HOSTS                                      │             │ REGISTRO (tipos nuevos)        │
 │ id uuid version vector       (F1, F10)     │  1:N        │ escritorio_abierto             │
 │ nombre grupo_id etiquetas    (F1)          │────────────<│ escritorio_cerrado             │
 │ tipo  ssh|rdp|vnc|voxl            (F12)    │             │ escritorio_fallido             │
 │ direccion puerto usuario     (F1)          │             │ huella_escritorio              │
 │ identidad_ref                (F1, F2)      │             │ host_id · detalle · resultado  │
 │ salto_host_id  «a través de» (F1)          │             │                                │
 │ escritorio_dominio                (F12)    │             └────────────────────────────────┘
 │ escritorio_opciones (JSON)        (F12)    │
 │ huella_escritorio (no viaja)      (F12)    │
 │ ultimo_estado ultima_conexion_en           │             ┌────────────────────────────────┐
 │ … campos SSH: multiplexar,                 │             │ SESIÓN DE ESCRITORIO           │
 │   keepalive_seg, opciones_extra,           │  1:N        │ (memoria del servidor,         │
 │   servicios, sftp_dir_*,                   │────────────<│ sin tabla)                     │
 │   snippet_al_conectar_id                   │             │ host_id tipo estado visor      │
 │   (sin uso en los escritorios)             │             │ enlaces clave tamaño           │
 │                                            │             │                                │
 └────────────────────────────────────────────┘             └────────────────────────────────┘
                        ▲                   │
                        │                   │
                        └───────────────────┘
                          salto_host_id → HOSTS (siempre tipo ssh)

```

### 3.1 HOSTS: columnas nuevas (migración 8, `user_version` 8)

| Campo | Tipo | Descripción |
|---|---|---|
| `tipo` | TEXT NOT NULL DEFAULT `'ssh'` | `ssh`, `rdp`, `vnc` o `voxl`, validado en código (T8). Se fija al crear el host y no cambia |
| `escritorio_dominio` | TEXT NULL | Dominio de Windows para RDP (opcional) |
| `escritorio_opciones` | TEXT NULL (JSON) | Opciones del escritorio según el tipo (§3.2) |
| `huella_escritorio` | TEXT NULL | `SHA256:<base64>` del certificado RDP aceptado. **Propia del equipo**: no viaja ni sube la versión |

Columnas que ya existían, en un escritorio:

| Columna | En un escritorio |
|---|---|
| `direccion`, `puerto` | Destino del servidor de escritorio, visto desde el host «a través de» si lo hay. En voxl, siempre `127.0.0.1` y el puerto en el que voxl escuchará en el host |
| `usuario` | Usuario de RDP; vacío en VNC y voxl |
| `identidad_ref` | `contrasena:llavero` (guardada en el llavero), `contrasena` (pedir siempre) o vacío (VNC sin contraseña) |
| `salto_host_id` | «A través de»: el host SSH por el que se llega. Opcional en RDP y VNC; obligatorio en voxl |
| `grupo_id`, etiquetas, `uuid`, `version`, `vector` | Igual que en los hosts SSH |
| `ultimo_estado`, `ultima_conexion_en` | Resultado de la última apertura (propios del equipo) |
| `multiplexar`, `keepalive_seg`, `opciones_extra`, `servicios`, `sftp_dir_*`, `snippet_al_conectar_id` | Sin uso: se guardan vacíos y se ignoran |

### 3.2 `escritorio_opciones` por tipo

```json
{"tamano": "ventana", "portapapeles": true, "teclado": "auto"}
{"tamano": "1920x1080", "portapapeles": false}
{"tamano": "ventana", "portapapeles": true, "pantalla": ":1", "programa": "voxl"}
```

| Clave | Tipos | Valores |
|---|---|---|
| `tamano` | todos | `ventana` (se ajusta a la ventana del visor) o `AxH` fijo, entre 640×480 y 8192×8192 |
| `portapapeles` | todos | `true` o `false`: texto en los dos sentidos |
| `teclado` | rdp | `auto` (sale de `[escritorio] distribucion_teclado`) o una distribución (`es-ES`, `en-US`, …) |
| `pantalla` | voxl | Pantalla X11: `:N` o `:N.M` |
| `programa` | voxl | `voxl` o una ruta absoluta, sin espacios ni caracteres de shell |
| `directo` | vnc | `true` si el usuario confirmó que va directo, sin cifrar; sin él, abrir un VNC directo a una dirección que no es de loopback pide confirmación |

Una clave desconocida se conserva y se ignora; un valor inválido se rechaza al guardar y, si llega de fuera (otro proceso o la sincronización), al leer (T38).

### 3.3 Sesión de escritorio (memoria del servidor, sin tabla)

| Campo | Descripción |
|---|---|
| `id` | Identificador de la sesión en el servidor |
| `host_id`, `tipo`, `nombre` | Copia del host al abrir |
| `estado`, `fase`, `motivo` | Ver el diagrama de estados; `fase` detalla `abriendo` |
| `camino` | Directo o a través de un host SSH (con su `ConexionTomada` del pool) |
| `visor` | Conectado o no, y `sin_visor_desde` para la gracia |
| `enlaces` | Enlaces de un solo uso pendientes (como mucho 60 s) |
| `clave_visor` | Clave vigente del visor (`Zeroizing`) |
| `tamano`, `imagen`, `fps` | Tamaño actual; `h264`, `rectangulos` o `voxl`; marcos por segundo |
| `voxl` | Canal `exec`, contraseña (`Zeroizing`), puerto remoto y puerto del túnel |
| `desde` | Hora de apertura, para la duración en REGISTRO |

### 3.4 REGISTRO: tipos nuevos

| Tipo | Cuándo | Detalle |
|---|---|---|
| `escritorio_abierto` | Al quedar abierta la sesión | Tipo, camino e imagen |
| `escritorio_cerrado` | Al cerrarse | Duración y motivo (cerrado por el usuario, sin visor, servidor parado) |
| `escritorio_fallido` | Al fallar la apertura | Motivo legible |
| `huella_escritorio` | Al aceptar o sustituir un certificado | `aceptada` o `sustituida`, con la huella nueva |

### 3.5 Relaciones

- `HOSTS.salto_host_id → HOSTS.id` (autorreferencia de la F1): en un escritorio, el host apuntado tiene que ser de tipo `ssh`.
- Un host SSH puede ser «a través de» de varios escritorios. Si se borra, sus escritorios se quedan sin «a través de» (`SET NULL`, como los saltos): un voxl no se abre hasta elegir otro y un VNC directo pide confirmación al abrirlo (`directo`).
- TUNELES, SNIPPET_DESTINOS, VERIFICACIONES_HOST y SINCRONIZACIONES_DIR solo admiten hosts SSH.
- REGISTRO guarda los eventos de escritorio con su `host_id`.

### 3.6 Diagramas de estados

**Sesión de escritorio**

```
  (inicio) ─ AbrirEscritorio ─▶ abriendo
     abriendo: conectando ─▶ verificando_certificado (RDP) ─▶ autenticando ─▶ arrancando (voxl)
     abriendo ─ error · cancelado · plazo ─▶ (fin) · REGISTRO escritorio_fallido
     abriendo ─ CerrarEscritorio ─▶ cancelado (fin) · escritorio_fallido · Error a la petición de apertura
     abriendo ─ listo ─▶ abierto · sin visor ─ el visor conecta ─▶ abierto · con visor
     abierto · con visor ─ el visor se va ─▶ abierto · sin visor (empieza la gracia)
     abierto · sin visor ─ 60 s sin visor ─▶ cerrando ─▶ (fin) · REGISTRO escritorio_cerrado
     abierto ─ conexión caída (o la del host «a través de») ─▶ caido ─ reconectar ─▶ abriendo
     abierto · caido ─ cerrar (confirmación) ─▶ cerrando ─▶ (fin)
  Inválidas: caido → abierto sin reconectar · cerrando → abierto · dos visores a la vez
             (el nuevo sustituye al anterior) · un caido no cuenta la gracia
```

**Enlace de un solo uso**

```
  emitido ─ canje válido (≤ 60 s, Host y Origin correctos) ─▶ canjeado (fin)
  emitido ─ 60 s sin canje ─▶ caducado (fin)
  emitido ─ la sesión se cierra ─▶ anulado (fin)
  Inválidas: canjeado → canjeado (un segundo uso da 403) · caducado → canjeado
```

**Huella del certificado RDP (`HOSTS.huella_escritorio`)**

```
  sin_huella ─ [CERTIFICADO NUEVO] aceptar ─▶ guardada · REGISTRO huella_escritorio (aceptada)
  sin_huella ─ cancelar ─▶ sin_huella (no se conecta ni se mandan credenciales)
  guardada ─ el mismo certificado ─▶ guardada
  guardada ─ otro certificado ─▶ [CERTIFICADO CAMBIADO]
        ├─ escribir el nombre del host ─▶ guardada (la nueva) · REGISTRO huella_escritorio (sustituida)
        └─ cancelar ─▶ guardada (la anterior; no se conecta)
  guardada ─ «olvidar huella» (confirmación) ─▶ sin_huella
  Inválidas: sustituir sin escribir el nombre · aceptarla sin diálogo · que viaje a otro equipo
```

**voxl en el host**

```
  parado ─ abrir ─▶ comprobando (voxl instalado y versión, pantalla, puerto) ─▶ arrancando
  arrancando ─ /healthz responde 200 ─▶ listo
  comprobando · arrancando ─ sin voxl · pantalla · puerto ocupado · 10 s ─▶ fallido (fin)
  listo ─ cerrar · gracia · conexión SSH caída ─▶ parando (fin de stdin → TERM; KILL a los 5 s) ─▶ parado
  Inválidas: listo sin túnel de sesión · dos voxl en el mismo puerto (el segundo falla con «puerto ocupado»)
```

---

## 4. Flujos de Trabajo

### 4.1 Abrir un escritorio RDP o VNC

```
  ↵ en un escritorio (Hosts, Flota, paleta o magi conectar)
      ▼ ¿ya tiene sesión? ─sí─▶ ir a su pestaña y pedir un enlace nuevo (§4.4)
      ▼ no
  AbrirEscritorio ─▶ servidor: validar la fila (T38) ─ inválida ─▶ «edita el host: <motivo>» (fin)
      ▼
  ¿a través de un host SSH? ─sí─▶ conexion_para_canal(host) ─▶ canal direct-tcpip al destino
      │                              ├─ huella SSH, frase o contraseña ─▶ diálogos de siempre (F1-F3)
      │                              └─ error o 10 s ─▶ escritorio_fallido «no se llega a <host>» (fin)
      └─no─▶ TCP al destino con plazo de 10 s ─ error ─▶ escritorio_fallido «no responde» (fin)
      ▼
  RDP: X.224 + TLS ─▶ huella del certificado (§4.2) ─ cancelada ─▶ fin sin mandar credenciales
  VNC: saludo RFB ─▶ tipo de seguridad ─ no admitido ─▶ «el servidor pide la seguridad N: no admitida» (fin)
      ▼
  ¿hace falta contraseña? ─sí─▶ PideCredenciales al cliente ─▶ llavero o diálogo (3 intentos)
      │                         └─ rechazada ─▶ diálogo otra vez · tras 3 ─▶ escritorio_fallido (fin)
      ▼
  sesión lista ─▶ abierto · sin visor ─▶ REGISTRO escritorio_abierto ─▶ EscritorioAbierto(enlace)
      ▼
  el cliente abre el navegador ─ no hay navegador ─▶ la pestaña enseña el enlace para copiarlo
      ▼
  canje y WebSocket (§4.4) ─▶ abierto · con visor
```

### 4.2 Certificado RDP

```
  TLS establecido ─▶ huella = SHA256 del certificado del servidor
      ├─ igual a huella_escritorio ─▶ sigue
      ├─ no hay ─▶ PideHuellaEscritorio (nuevo): sujeto, caducidad, huella
      │      ├─ aceptar ─▶ el cliente guarda huella_escritorio · REGISTRO ─▶ sigue
      │      └─ cancelar ─▶ cerrar TLS · escritorio_fallido «certificado no aceptado»
      └─ distinta ─▶ PideHuellaEscritorio (cambiado): anterior y nueva, en rojo
             ├─ escribir el nombre del host ─▶ el cliente sustituye la huella · REGISTRO ─▶ sigue
             └─ cancelar ─▶ cerrar · escritorio_fallido «certificado cambiado»
  Nunca se mandan credenciales antes de este paso.
```

### 4.3 Abrir voxl

```
  ↵ en un host voxl ─▶ servidor: conexion_para_canal(host SSH)
      ▼
  exec: sh -c "<escritorio_voxl.sh>" magi-voxl <pantalla> <puerto> <programa>
      ▼ script: valida los argumentos ─ inválidos ─▶ «pantalla o programa inválidos» (fin)
      ▼ script: command -v programa ─ falta ─▶ «voxl no está instalado en <host>» (fin)
  MAGI escribe la contraseña aleatoria en stdin ─▶ script: VOXL <versión> · LISTO <pid>
      ├─ versión < 0.3 ─▶ «actualiza voxl a la 0.3 o posterior» (fin)
      └─ voxl sale enseguida ─▶ última línea de stderr: «la pantalla :1 no existe»,
                                «el puerto 6901 del host está ocupado» (fin)
      ▼
  GET /healthz por direct-tcpip a 127.0.0.1:<puerto> hasta 200 ─ 10 s ─▶ «voxl no respondió en 10 s» (fin)
      ▼
  túnel de la sesión 127.0.0.1:<libre> → 127.0.0.1:<puerto> del host
      ▼
  abierto · sin visor ─▶ EscritorioAbierto(enlace) ─▶ lanzadera: canje ─▶ página de voxl por el túnel
  gracia: cuenta las conexiones abiertas del túnel · cerrar: fin de stdin → el script para voxl
```

### 4.4 Visor: canje del enlace y conexión

```
  navegador abre http://127.0.0.1:P/entrar?t=<enlace>
      ▼ Host ≠ 127.0.0.1:P o localhost:P ─▶ 403
  página estática ─▶ POST /api/canjear {enlace} con Origin propio
      ├─ Origin ajeno o ausente ─▶ 403
      ├─ usado, caducado o anulado ─▶ «enlace usado o caducado: ábrelo otra vez desde MAGI»
      ├─ 10 fallos en un minuto ─▶ 429 durante un minuto
      ├─ voxl ─▶ {voxl: url} ─▶ location.replace(url) (página de voxl por el túnel)
      └─ RDP o VNC ─▶ {clave} ─▶ sessionStorage · history.replaceState("/")
      ▼
  WebSocket /api/ws ─▶ primer mensaje {"t":"hola","clave":…,"h264":…} en 5 s ─ no ─▶ cerrar
      ▼ ¿había otro visor? ─sí─▶ al anterior: «abierto en otra ventana» y se cierra
  hello (tamaño, tipo, imagen) ─▶ pantalla completa en el primer marco ─▶ marcos incrementales
  recargar la pestaña ─▶ la clave sigue en sessionStorage ─▶ vuelve a conectar sin enlace
```

### 4.5 Gracia y cierre

```
  cada segundo, por sesión abierta:
      ├─ con visor (en voxl: alguna conexión en el túnel) ─▶ sin_visor_desde = nada
      └─ sin visor ─▶ sin_visor_desde = ahora (si no estaba) ─▶ ¿≥ gracia_seg (60)?
             └─ sí ─▶ cerrando ─▶ RDP: desconectar (Windows conserva su sesión) · VNC: cerrar
                                  voxl: fin de stdin ─▶ el script para voxl
                    ─▶ soltar el canal del pool por identidad (T34) ─▶ REGISTRO escritorio_cerrado
  cerrar desde la TUI (^] x, confirmación) ─▶ igual, con motivo «cerrado por el usuario»
  conexión caída ─▶ caido con el motivo (sin gracia) ─▶ el visor enseña «Reconectar»
      └─ Reconectar en el visor ─▶ el servidor pide credenciales a una ventana de MAGI abierta
                                   └─ ninguna ─▶ «abre MAGI para reconectar»
```

### 4.6 Sincronización con escritorios

```
  exportar el fichero del equipo:
      ├─ todos los hosts son ssh ─▶ formato 1 (lo importa MAGI 1.1)
      └─ hay algún rdp, vnc o voxl ─▶ formato 2 (con tipo, escritorio_dominio y escritorio_opciones)
  importar el fichero de otro equipo:
      ├─ formato 1 ─▶ todos los hosts, tipo ssh
      ├─ formato 2 ─▶ tipo de cada host; huella_escritorio y contraseñas nunca vienen
      └─ formato > 2 ─▶ equipo formato_nuevo · aviso «actualiza MAGI» (T74)
  MAGI 1.1 ante un fichero formato 2 ─▶ formato_nuevo (no lo malinterpreta como hosts SSH)
  app Android (lector) ─▶ formato 2 ─▶ escritorios visibles, con «se abre en el ordenador»
```

### 4.7 Diagrama de secuencia: RDP a través de un host SSH

```
  TUI         navegador        magi --servidor                  bastion (SSH)               win-01 (RDP)
   │─AbrirEscritorio(win-01)─────────▶│ conexion_para_canal(bastion)  │                           │
   │              │                   │─direct-tcpip 192.0.2.30:3389─▶│                           │
   │              │                   │─ X.224 · TLS 1.3 (por el canal) ─────────────────────────▶│
   │              │                   │◀───────────────────────────────────────────── certificado │
   │              │                   │ huella desconocida            │                           │
   │◀─────────── PideHuellaEscritorio │                               │                           │
   │ acepta·guarda│                   │                               │                           │
   │─aceptada────────────────────────▶│                               │                           │
   │◀─────────────── PideCredenciales │                               │                           │
   │ llavero      │                   │                               │                           │
   │─Credenciales────────────────────▶│                               │                           │
   │              │                   │─ CredSSP (NTLM) · canales · EGFX AVC420 ─────────────────▶│
   │              │                   │◀─────────────────────────────────────────── sesión activa │
   │◀────── EscritorioAbierto(enlace) │                               │                           │
   │─abre (--app)▶│                   │                               │                           │
   │              │─POST /api/canjear▶│                               │                           │
   │              │◀─────────── clave │                               │                           │
   │              │─WS /api/ws · hola▶│                               │                           │
   │              │                   │◀────────────────────────────────────────── AVC420 (H.264) │
   │              │◀ VideoRdp (igual) │                               │                           │
   │              │─kc · m · b───────▶│─ scancodes · ratón ──────────────────────────────────────▶│
```

1. La TUI pide abrir `win-01`. El servidor toma la conexión de `bastion` del pool y abre un canal `direct-tcpip` a `192.0.2.30:3389`.
2. Por ese canal negocia TLS con Windows. Antes de cualquier credencial compara la huella del certificado; es nueva, así que pregunta al cliente, que la guarda al aceptarla.
3. Pide las credenciales al cliente, que las saca del llavero sin diálogo. NLA (CredSSP con NTLM) termina la autenticación y se abren los canales, también EGFX con AVC420.
4. El servidor emite el enlace de un solo uso. El cliente abre el navegador; la página canjea el enlace por la clave del visor y abre el WebSocket.
5. Windows manda vídeo H.264 y el puente lo pasa tal cual al visor, que lo decodifica con WebCodecs. El teclado vuelve como scancodes y el ratón como eventos de puntero.

### 4.8 Diagrama de secuencia: voxl

```
  TUI         navegador        magi --servidor                             web-01 (SSH + voxl)
   │─AbrirEscritorio(voxl)───────────▶│                                             │
   │              │                   │─ exec: escritorio_voxl.sh :1 6901 voxl ────▶│
   │              │                   │─ stdin: contraseña aleatoria ──────────────▶│
   │              │                   │◀──────────────────────── VOXL 0.3.3 · LISTO │
   │              │                   │─ direct-tcpip 127.0.0.1:6901 · GET /healthz▶│
   │              │                   │◀─────────────────────────────────────── 200 │
   │              │                   │ túnel 127.0.0.1:P2 → 6901                   │
   │◀────── EscritorioAbierto(enlace) │                                             │
   │─abre (--app)▶│                   │                                             │
   │              │─POST /api/canjear▶│                                             │
   │              │◀───── url de voxl │                                             │
   │              │─ página y WebSocket de voxl, por el túnel de la sesión ────────▶│
   │              │                   │ cerrar · gracia · SSH caída                 │
   │              │                   │─ fin de stdin → TERM ──────────────────────▶│
```

1. El servidor ejecuta el script embebido en el host con la pantalla, el puerto y el programa como argumentos validados.
2. Le pasa la contraseña aleatoria por la entrada estándar y espera la línea `LISTO`.
3. Comprueba `/healthz` por un canal `direct-tcpip`, abre el túnel de la sesión y emite el enlace.
4. La lanzadera canjea el enlace y lleva al navegador a la página de voxl por el túnel.
5. Al cerrar, el servidor cierra el canal: el script ve el fin de la entrada y para voxl.

---

## 5. Interfaz del Servidor del Visor y Protocolo

### 5.1 Endpoints HTTP del visor (solo `127.0.0.1`)

| Método | Ruta | Descripción | Protección |
|---|---|---|---|
| GET | `/entrar?t=<enlace>` | Página del visor (HTML estático). No gasta el enlace: lo gasta el canje | Host |
| GET | `/` | La misma página, para poder recargar después de `history.replaceState` (implementación, D232) | Host |
| GET | `/visor.js`, `/trabajador.js`, `/visor.css` | Ficheros del visor, incluidos en el binario | Host |
| POST | `/api/canjear` | Cuerpo `{"enlace": "…"}`. Devuelve `{"clave", "escritorio", "tipo"}` (`escritorio` es el nombre legible del host, que pinta la barra) o, en voxl, `{"voxl": "http://127.0.0.1:<p>/?token=…"}`. 403 si el enlace está usado, caducado o anulado; 429 tras 10 fallos en un minuto | Host, Origin y enlace |
| GET | `/api/ws` | WebSocket del visor; el primer mensaje es `{"t":"hola","clave":"…","h264":true}` en 5 s (`h264`: si el navegador decodifica H.264) | Host, Origin y clave |
| GET | `/api/salud` | 200 si el servidor del visor responde | Host |

Todas las respuestas llevan `Content-Security-Policy: default-src 'self'; script-src 'self'; worker-src 'self'; connect-src 'self'; img-src 'self' blob: data:; style-src 'self'; frame-ancestors 'none'; base-uri 'none'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` y `Cache-Control: no-store`.

### 5.2 Mensajes del visor (WebSocket)

Se parte del protocolo de voxl (`web/src/protocol.ts`): mensajes binarios con un byte de tipo, texto JSON del servidor y órdenes cortas separadas por comas del visor.

| Sentido | Mensaje | Contenido |
|---|---|---|
| servidor → visor | `0x03` Cursor | Como en voxl: RGBA, tamaño y punto caliente (≤ 256×256) |
| servidor → visor | `0x05` Rect | `x y w h` (u16) y píxeles RGBA |
| servidor → visor | `0x06` Copia | `ox oy dx dy w h` (u16): copiar una zona ya pintada (CopyRect de VNC) |
| servidor → visor | `0x07` VideoRdp | Número de marco (u32), regiones (`n` y `n × x y w h`) y H.264 Annex-B tal cual |
| servidor → visor | `0x08` Marco | Fin de un lote: el visor presenta lo pintado de una vez |
| servidor → visor | texto | `hello` (ancho, alto, tipo, imagen, si se puede cambiar el tamaño), `estado` (`conectando`, `caido`, `cerrado`, `otra_ventana` y motivo), `tam` (tamaño nuevo), `clip` (texto), `ping`, `close` y `latencia` (`{"t":"latencia","ms":N}` tras cada `po`) |
| visor → servidor | texto | `kc,<keysym>,<código>,<1\|0>` tecla · `m,x,y` · `b,botón,<1\|0>` · `sp,dx,dy` rueda · `r` soltar todo · `c,<texto>` portapapeles · `sz,w,h` tamaño · `v,<1\|0>` imagen sí o no · `kf` pantalla completa · `cad` Ctrl+Alt+Supr · `win` tecla Windows · `po,<t>` respuesta al ping · `rec` reconectar (botón del aviso de caído) |

El código es el `KeyboardEvent.code` del navegador (`KeyA`, `ArrowUp`, `AltRight`…), vacío si no lo hay.

### 5.3 Protocolo cliente ⇄ servidor (`VERSION_PROTOCOLO = 7`)

| Mensaje | Sentido | Contenido |
|---|---|---|
| `AbrirEscritorio` | cliente → servidor | `peticion_id`, `host_id` |
| `EscritorioAbierto` | servidor → solicitante | `peticion_id`, `escritorio_id`, `enlace` |
| `PedirEnlace` → `Enlace` | cliente ⇄ servidor | Enlace nuevo para reabrir el visor o copiarlo |
| `ReconectarEscritorio` | cliente → servidor | `peticion_id`, `escritorio_id` |
| `CerrarEscritorio` | cliente → servidor | `escritorio_id` |
| `Escritorios{lista}` | servidor → todos | Lista completa en cada cambio (T27): id, host, tipo, estado, fase, motivo, visor, `sin_visor_desde`, tamaño, imagen, fps, camino y desde |
| `PideHuellaEscritorio` → `RespuestaHuellaEscritorio` | servidor ⇄ solicitante | Nuevo o cambiado, sujeto, caducidad, huella y anterior; la respuesta va después de que el cliente guarde la huella |
| `PideCredenciales` → `Credenciales` | servidor ⇄ solicitante | Tipo, usuario, dominio e intento; la contraseña viaja en `Zeroizing` y nunca va al log |

`TrabajoServidor` gana `escritorios` (abiertos): un servidor con escritorios no se sustituye sin preguntar (T67). Los `peticion_id` de escritorios empiezan en 2^46. `Hola` y `VersionIncompatible` no cambian (T66).

Precisiones de la implementación (D233): los diálogos que el servidor inicia desde el visor (reconectar) usan `peticion_id` en `[2^47, 2^48)`; `Bienvenida` solo lleva la lista de escritorios si no está vacía; `CerrarEscritorio` durante «abriendo» cancela la apertura (`escritorio_fallido` y `Error` a la petición de apertura); reconectar desde el visor sin ninguna ventana de MAGI conectada no reconecta y el visor dice «abre MAGI para reconectar».

### 5.4 Configuración (`config.toml`)

| Clave | Por defecto | Uso |
|---|---|---|
| `[escritorio] navegador` | `"auto"` | `auto` o una orden con `{url}`, por ejemplo `"brave --app={url}"` |
| `[escritorio] gracia_seg` | `60` | Segundos sin visor antes de cerrar una sesión (10-3600) |
| `[escritorio] puerto_visor` | `0` | Puerto del servidor del visor (`0` = libre) |
| `[escritorio] distribucion_teclado` | `"auto"` | Distribución que se anuncia a Windows; `auto` sale de `LANG` (y si no, `en-US`) |
| `[escritorio] h264` | `true` | Ofrecer AVC420 a Windows; `false` fuerza los rectángulos |

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 HOSTS (F2) ─┬─ n nuevo ─▶ FORMULARIO: tipo SSH · RDP · VNC · voxl (fijo al guardar)
             ├─ ↵ en un host SSH ─▶ SESIÓN (terminal), como siempre
             ├─ ↵ en un escritorio ─▶ [CERTIFICADO] ─▶ [CREDENCIALES] ─▶ pestaña ▣ + VISOR
             └─ e editar ─▶ FICHA ─┬─ bloque ESCRITORIO (tipo, destino, a través de, opciones)
                                   ├─ olvidar huella del certificado [confirmación]
                                   └─ olvidar contraseña [confirmación]
 FLOTA (F1) ── escritorios sin sondeo: último estado
 SESIÓN (F3) ── pestañas ● terminal · ▣ escritorio ─┬─ ↵ abrir el visor
                                                    ├─ y copiar un enlace de un solo uso
                                                    ├─ ^] r reconectar
                                                    └─ ^] x cerrar [confirmación]
 SESIONES (lista) ── columna TIPO · r reconectar · x cerrar
 PALETA (Ctrl+P) ── escritorios como hosts
 REGISTRO (F7) ── escritorio_abierto · escritorio_cerrado · escritorio_fallido · huella_escritorio
 VISOR (navegador, 127.0.0.1) ── barra: pantalla completa · Ctrl+Alt+Supr · Win · cerrar visor
 Desde cualquier apertura: [CERTIFICADO NUEVO] · [CERTIFICADO CAMBIADO] · [CREDENCIALES]
```

### 6.1 Hosts con escritorios

```
┌ MAGI · HOSTS ───────────────────────── 8 hosts · 5 escritorios ──┐
│ /                                                                │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ▼ 4d3 · producción                                         (5)  │
│      ● web-01         192.0.2.10        usuario 22               │
│      ○ bastion        192.0.2.11        usuario 22               │
│    ▸ ● web-01 · vnc   ▣ VNC 127.0.0.1           5901  ⤴ web-01   │
│      ○ web-01 · voxl  ▣ voxl :1                 6901  ⤴ web-01   │
│      ○ db-01          192.0.2.12        usuario 22               │
│                                                                  │
│  ▼ 4d3 · oficina                                            (3)  │
│      ○ win-01         ▣ RDP 192.0.2.30  usuario 3389  ⤴ bastion  │
│      ✕ win-02         ▣ RDP 192.0.2.31  usuario 3389             │
│      ○ nas            ▣ VNC 192.0.2.40          5900  ⚠ directo  │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ ↵ conectar  e editar  n nuevo  g grupo  x borrar  ⇥ etiq.        │
└──────────────────────────────────────────────────────────────────┘
```

`▣` marca un escritorio, con su tipo; `⤴` indica «a través de» y `⚠ directo`, un VNC sin SSH. En modo ASCII, `▣` es `[D]` y el tipo va en texto (T5). El estado (`●` sesión abierta, `○`, `✕` último intento con error, `◐` abriendo) funciona igual que en los hosts SSH.

### 6.2 Formulario de un host RDP

```
┌ MAGI · HOST ─────────────────────────────── win-01 · nuevo ──┐
│                                                              │
│  TIPO        ( ) SSH   (•) RDP   ( ) VNC   ( ) voxl          │
│              (no se cambia después de crear el host)         │
│                                                              │
│  IDENTIFICACIÓN                                              │
│    Nombre      [ win-01                                  ]   │
│    Dirección   [ 192.0.2.30                              ]   │
│    Puerto      [ 3389  ]     Grupo  [ 4d3 · oficina     ▾ ]  │
│    Etiquetas   [ windows  oficina                        ]   │
│                                                              │
│  ACCESO                                                      │
│    A través de [ bastion (SSH)                         ▾ ]   │
│    Usuario     [ usuario                                 ]   │
│    Dominio     [ OFICINA                                 ]   │
│    Contraseña  (•) guardar en el llavero  ( ) pedir siempre  │
│                                                              │
│  ESCRITORIO                                                  │
│    Tamaño      (•) ajustar a la ventana  ( ) fijo [1920x1080]│
│    Teclado     [ auto (es-ES)                          ▾ ]   │
│    Portapapeles [x] texto en los dos sentidos                │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ ^s guardar   ^t probar conexión   esc descartar              │
└──────────────────────────────────────────────────────────────┘
```

En VNC desaparecen usuario, dominio y teclado, y la contraseña admite «sin contraseña». En voxl, «a través de» es obligatorio, la dirección no se pide (es `127.0.0.1` del host) y aparecen «Pantalla [ :1 ]», «Puerto de voxl [ 6901 ]» y «Programa [ voxl ]». Al guardar un VNC directo a una dirección que no es de loopback, MAGI pide confirmación: «la imagen y el teclado viajarán sin cifrar; mejor a través de un host SSH».

### 6.3 Pestaña de escritorio

```
┌ MAGI · SESIÓN ───────────────────────────────────────────────┐
│ ● web-01 │ ▸▣ win-01 │ ◐ web-01 · vnc │ + nueva              │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   ▣ ESCRITORIO · win-01 · RDP                                │
│                                                              │
│   Estado      ● abierto · visor abierto                      │
│   Destino     192.0.2.30:3389 a través de bastion            │
│   Usuario     OFICINA\usuario                                │
│   Tamaño      1920×1080 · se ajusta a la ventana             │
│   Imagen      H.264 de Windows · 58 fps                      │
│   Certificado SHA256:T2qR…9cQ (aceptado el 4 nov)            │
│   Desde       21:40 · 12 min                                 │
│                                                              │
│   ↵ abrir el visor   y copiar enlace   ^] x cerrar           │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ ▣ 2/3 · abierto · visor abierto · ^] ? ayuda                 │
└──────────────────────────────────────────────────────────────┘
```

Sin visor, la línea de estado dice «● abierto · sin visor · se cierra en 45 s». Una sesión caída enseña el motivo en rojo y `^] r reconectar`, como las pestañas de terminal.

### 6.4 Visor en el navegador

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ win-01 · MAGI                             (ventana de Chromium, modo app)    │
├──────────────────────────────────────────────────────────────────────────────┤
│ ▣ win-01 · RDP  ● 58 fps · 9 ms  [⛶] [Ctrl+Alt+Supr] [⊞ Win] [✕ cerrar visor]│
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│                                                                              │
│              escritorio remoto, escalado a la ventana sin deformar           │
│                                                                              │
│              (la barra de arriba se oculta a los 2 s y vuelve al             │
│               acercar el ratón al borde superior)                            │
│                                                                              │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

La barra se oculta a los 2 s y vuelve al acercar el ratón al borde superior. En pantalla completa, el visor pide el bloqueo de teclado del navegador (`navigator.keyboard.lock`) para que Esc, Alt+Tab y la tecla Windows lleguen al remoto (Hyprland puede seguir quedándose con sus atajos de Super). Los avisos (conectando, caído con «Reconectar», «abierto en otra ventana», enlace caducado) salen sobre la imagen. Con voxl, la ventana es la página del propio voxl.

### 6.5 Certificado cambiado

```
    ┌─ CERTIFICADO CAMBIADO · win-01 ──────────────────────────┐
    │                                                          │
    │  ✕ El certificado de win-01 ha cambiado.                 │
    │    Puede ser una reinstalación o alguien en medio.       │
    │                                                          │
    │    Anterior  SHA256:T2qR…9cQ (aceptado el 4 nov)         │
    │    Nuevo     SHA256:mZ7a…Lw0 · CN=WIN-01                 │
    │    Caduca    3 may 2027                                  │
    │                                                          │
    │    Para sustituirlo, escribe el nombre del host:         │
    │    [ win-01_                                ]            │
    │                                                          │
    │    ↵ sustituir y conectar          esc no conectar       │
    │                                                          │
    └──────────────────────────────────────────────────────────┘
```

«Certificado nuevo» es el mismo diálogo en ámbar, con sujeto, caducidad y huella, y se acepta con `a`. Las huellas se escriben como las de SSH: `SHA256:` y base64 (T101).

### Notas de UX

- **Glifos y texto (T5):** `▣` escritorio, `⤴` a través de, `⚠` VNC directo; en ASCII, `[D]`, `>` y `!`.
- **Ninguna acción destructiva con una pulsación (T6):** cerrar una sesión de escritorio, olvidar la huella y olvidar la contraseña piden confirmación. Cerrar el visor no cierra la sesión.
- **El tipo se ve siempre:** en Hosts, en la pestaña, en Sesiones y en la barra del visor.
- **Errores con instrucción:** «voxl no está instalado en web-01: instala voxl-server», «el servidor pide la seguridad 18 (TLS): no admitida; usa SSH», «este servidor solo habla RDP antiguo, sin TLS: no admitido».
- **Tamaños (T45):** Hosts, el formulario, la ficha, la pestaña y los diálogos tienen modo estrecho e instantáneas a 40×12, 80×24 y 200×60.
- **Visor:** paleta de MAGI (ámbar `#FFA000` sobre `#0A0A0A`), fuente del sistema, botones con texto y `aria-label`, foco visible y contraste AA.
- **Ejemplos:** los mockups usan direcciones de documentación (`192.0.2.x`), como en el resto del proyecto.

---

## 7. Lógica de Negocio

### 7.1 Tipos de host y validación

```python
TIPOS = ("ssh", "rdp", "vnc", "voxl")
PUERTO_POR_DEFECTO = {"ssh": 22, "rdp": 3389, "vnc": 5900, "voxl": 6901}
PANTALLA = re.compile(r"^:[0-9]{1,2}(\.[0-9])?$")
PROGRAMA = re.compile(r"^(voxl|/[A-Za-z0-9._/-]+)$")

def validar_host(h, hosts):
    if h.tipo not in TIPOS:
        raise Invalido("tipo de host desconocido")
    if h.id and h.tipo != hosts[h.id].tipo:
        raise Invalido("el tipo no se cambia: crea otro host")
    via = hosts.get(h.salto_host_id)
    if via and via.tipo != "ssh":
        raise Invalido("«a través de» tiene que ser un host SSH")
    if not 1 <= h.puerto <= 65535:
        raise Invalido("puerto fuera de rango")
    op = h.escritorio_opciones
    if h.tipo != "ssh":
        validar_tamano(op.get("tamano", "ventana"))        # ventana o AxH entre 640×480 y 8192×8192
    if h.tipo == "voxl":
        if not via:
            raise Invalido("voxl se arranca en un host SSH: elige «a través de»")
        if h.puerto < 1024:
            raise Invalido("voxl escucha como usuario normal: usa un puerto desde 1024")
        h.direccion = "127.0.0.1"                           # voxl escucha en el propio host SSH
        if not PANTALLA.match(op.get("pantalla", "")):
            raise Invalido("pantalla X11 inválida: usa :1 o :1.0")
        if not PROGRAMA.match(op.get("programa", "voxl")):
            raise Invalido("programa inválido: voxl o una ruta absoluta sin espacios")
    if h.tipo == "rdp" and "\\" in h.usuario:
        h.escritorio_dominio, h.usuario = h.usuario.split("\\", 1)   # DOMINIO\usuario
    if h.tipo == "vnc" and not via and not es_loopback(h.direccion) and not op.get("directo"):
        h.necesita_confirmacion = "la imagen y el teclado viajarán sin cifrar"   # al aceptar: directo = true
```

### 7.2 Camino hasta el escritorio

```python
async def abrir_camino(host, pool):
    if host.salto_host_id:                                  # «a través de»
        tomada = await pool.conexion_para_canal(host.salto_host_id, Uso.ESCRITORIO)   # T42
        try:
            canal = await plazo(10, tomada.direct_tcpip(host.direccion, host.puerto))
        except Exception:
            tomada.soltar()                                 # por identidad, en todas las ramas (T34)
            raise
        return Camino(canal, tomada)
    flujo = await plazo(10, tcp_connect(host.direccion, host.puerto))
    return Camino(flujo, None)
```

El canal de un escritorio cuenta para el pool como el de un túnel: mantiene viva la conexión del host SSH. Si esa conexión cae, sus escritorios pasan a `caido` con el motivo (como los túneles de la F5 y el vigilante de la T97).

### 7.3 Enlaces de un solo uso y clave del visor

```python
VIDA_ENLACE = 60                                            # segundos

def emitir_enlace(sesion):
    e = secrets.token_urlsafe(32)                           # 256 bits
    enlaces[e] = (sesion.id, monotonic() + VIDA_ENLACE)
    return f"http://127.0.0.1:{puerto_visor}/entrar?t={e}"

def canjear(e, cabeceras):
    comprobar_host_y_origen(cabeceras, origen_obligatorio=True)
    if fallos.en_el_ultimo_minuto() >= 10:
        raise Demasiados()                                  # 429 durante un minuto
    sesion_id, caduca = enlaces.pop(e, (None, 0))           # se gasta aunque falle lo siguiente
    s = sesiones.get(sesion_id)
    if s is None or monotonic() > caduca:
        fallos.anotar()
        raise Prohibido("enlace usado o caducado: ábrelo otra vez desde MAGI")
    if s.tipo == "voxl":
        return {"voxl": f"http://127.0.0.1:{s.puerto_tunel}/?token={s.clave_voxl}"}
    s.clave_visor = secrets.token_urlsafe(32)               # la nueva invalida la anterior
    s.echar_visor_anterior("otra_ventana")
    return {"clave": s.clave_visor, "escritorio": s.nombre, "tipo": s.tipo}   # nombre legible para la barra
```

Al cerrar una sesión se anulan sus enlaces pendientes y su clave. El visor guarda la clave en `sessionStorage` (por pestaña y origen) y limpia la URL con `history.replaceState`, así que el enlace gastado tampoco queda en el historial.

### 7.4 Host y Origin

```python
def comprobar_host_y_origen(cab, origen_obligatorio):
    validos = {f"127.0.0.1:{puerto_visor}", f"localhost:{puerto_visor}"}
    if cab.get("host") not in validos:
        raise Prohibido("host")                             # DNS rebinding
    origen = cab.get("origin")
    if origen is None and origen_obligatorio:
        raise Prohibido("origen")                           # /api/* solo desde la página del visor
    if origen is not None and origen.removeprefix("http://") not in validos:
        raise Prohibido("origen")                           # otra página no abre el WebSocket
```

### 7.5 Puente: rectángulos, coalescencia y contrapresión

```python
FPS_MAX = 60
COLA_MAX = 8 * 1024 * 1024                                  # bytes pendientes hacia el visor

async def bucle_puente(fuente, visor):
    sucio = Region()                                        # unión de zonas cambiadas
    while True:
        ev = await fuente.siguiente()                       # lo produce el hilo de decodificación
        if ev.tipo == "rect":
            sucio.unir(ev.zona)
        elif ev.tipo == "copia" and sucio.vacia():
            await visor.copia(ev)                           # CopyRect: sin píxeles
        elif ev.tipo == "copia":
            sucio.unir(ev.destino)                          # con pintura pendiente, mejor repintar
        elif ev.tipo == "video":
            await visor.video(ev.marco, ev.regiones, ev.h264)   # AVC420 tal cual (D211)
        if toca_marco(FPS_MAX) and visor.pendiente() < COLA_MAX and not sucio.vacia():
            for zona in sucio.simplificar(max_zonas=64):
                await visor.rect(zona, fuente.pixeles(zona))    # RGBA desde el búfer del escritorio
            await visor.fin_de_marco()
            sucio.vaciar()
        # si el visor va lento, las zonas se acumulan y salen una sola vez con su estado final
```

La decodificación (ZRLE, Hextile, mapas de bits y RemoteFX de IronRDP) va en un hilo propio por sesión, que escribe en el búfer del escritorio y avisa al puente; el runtime de red solo mueve bytes (D225). Con `v,0` (pestaña oculta) el puente deja de mandar y, con `v,1` o `kf`, manda el escritorio entero. Con H.264 no hay búfer de píxeles en MAGI: la imagen entera tendría que pedirla el cliente RDP a Windows, y la implementación aún no lo hace (limitación, R117). El hilo de cada sesión lleva su propio runtime, porque el `ActiveStage` de IronRDP no es `Send` (D231).

### 7.6 Teclado

```python
def tecla(sesion, keysym, codigo, abajo):
    if sesion.tipo == "vnc":
        if sesion.teclas_qemu and codigo in SCANCODES:
            sesion.qemu_tecla(abajo, keysym, SCANCODES[codigo])   # QEMU Extended Key Event
        else:
            sesion.key_event(abajo, keysym)                 # RFB usa keysyms de X11
    elif sesion.tipo == "rdp":
        if codigo in SCANCODES:
            sc, extendida = SCANCODES[codigo]               # "KeyA" → (0x1E, False); "ArrowUp" → (0x48, True)
            sesion.scancode(sc, extendida, abajo)           # los caracteres los pone la distribución de Windows
        elif (c := keysym_a_unicode(keysym)) is not None:
            sesion.unicode(c, abajo)                        # teclados en pantalla y métodos de entrada

def ctrl_alt_supr(sesion):
    for codigo in ("ControlLeft", "AltLeft", "Delete"):
        tecla(sesion, 0, codigo, True)
    for codigo in ("Delete", "AltLeft", "ControlLeft"):
        tecla(sesion, 0, codigo, False)
```

La tabla `SCANCODES` (código del navegador → scancode del conjunto 1 y si es extendida) vive en `escritorio/teclas.rs` y tiene pruebas. A Windows se le anuncia la distribución del host (`teclado`) o la de `[escritorio] distribucion_teclado` (`auto` sale de `LANG`: `es_ES` → `es-ES`). El visor suelta todas las teclas (`r`) al perder el foco, para no dejar Ctrl pulsado en el remoto.

### 7.7 VNC: saludo, seguridad y límites

```python
def respuesta_vnc(reto: bytes, contrasena: str) -> bytes:
    clave = contrasena.encode("latin-1", "replace")[:8].ljust(8, b"\0")
    clave = bytes(int(f"{b:08b}"[::-1], 2) for b in clave)  # RFB invierte los bits de cada byte
    return DES(clave).cifrar_ecb(reto)                      # reto de 16 bytes

CODIFICACIONES = [ZRLE, HEXTILE, COPYRECT, RAW,             # 16, 5, 1, 0
                  DESKTOP_SIZE, EXTENDED_DESKTOP_SIZE,      # -223, -308
                  CURSOR, QEMU_EXTENDED_KEY,                # -239, -258
                  PORTAPAPELES_EXTENDIDO]                   # 0xC0A1E5CE
```

- Versiones 3.3, 3.7 y 3.8; seguridad `None` (1) y `VNC Authentication` (2). Cualquier otra se rechaza con su número.
- `ClientInit` con `shared-flag = 1`: abrir MAGI no echa a otros visores.
- Formato de píxel fijo: 32 bits, true color, RGBX en little endian.
- Peticiones de actualización incrementales: una nueva cuando el puente ha recogido la anterior, como mucho 60 por segundo.
- `SetDesktopSize` si el servidor anuncia ExtendedDesktopSize; si no, el visor escala.
- Límites contra un servidor malicioso: escritorio de hasta 8192×8192; cada rectángulo dentro del escritorio; descompresión ZRLE acotada al tamaño esperado; cursor de hasta 256×256; texto de hasta 1 MB. Un dato fuera de límites cierra la sesión con «datos inválidos del servidor», sin pánico.

### 7.8 RDP: certificado, credenciales y H.264

```python
async def abrir_rdp(sesion, camino, cliente):
    con = Conector(tamano=sesion.tamano_inicial, teclado=sesion.distribucion,
                   protocolos=[HYBRID, SSL],                # NLA si el servidor la pide; TLS sin NLA si no
                   egfx_avc420=config.h264)
    tls = await con.negociar_x224_y_tls(camino)            # TLS 1.2/1.3 con rustls
    huella = "SHA256:" + b64(sha256(tls.certificado_der)).rstrip("=")
    if sesion.host.huella_escritorio != huella:
        tipo = "nuevo" if sesion.host.huella_escritorio is None else "cambiado"
        if not await cliente.pide_huella(tipo, huella, sujeto(tls), caduca(tls)):
            raise Cancelado("certificado no aceptado")      # nunca se mandan credenciales antes
    cred = await cliente.pide_credenciales(sesion.host)     # llavero o diálogo (T25)
    await con.credssp(cred, tls.clave_publica)              # NLA liga las credenciales a este TLS
    return await con.sesion_activa()                        # canales: CLIPRDR, Display Control, EGFX
```

- Si el servidor solo admite la seguridad RDP clásica (sin TLS), falla con «este servidor solo habla RDP antiguo, sin TLS: no admitido».
- Con EGFX y AVC420, cada `WireToSurface1` con H.264 se manda al visor como `VideoRdp`, con sus regiones; MAGI no lo decodifica ni lo recomprime (D211). Si el visor dice en su `hola` que no decodifica H.264, MAGI reconecta sin AVC420 (Windows conserva la sesión) y manda rectángulos.
- Sin EGFX o sin AVC420, los mapas de bits, RemoteFX y lo demás que decodifica IronRDP llegan al puente como rectángulos.
- Display Control: cambio de tamaño en vivo, agrupado 500 ms. Sin ese canal, o con tamaño fijo, el visor escala.
- Portapapeles CLIPRDR: solo `CF_UNICODETEXT`, hasta 1 MB, si el host lo tiene activado.
- Implementación (D229, D230): el TLS se hace a mano con rustls y `aws-lc-rs`, con un verificador que captura el certificado DER, sin reanudación de sesión (CredSSP no la admite) y pasando a CredSSP los bits de la clave del `SubjectPublicKeyInfo`, no el DER entero. El AVC420 lo saca un `DvcProcessor` propio sobre `ironrdp-egfx` 0.3: descomprime ZGFX, lee `WireToSurface1` con sus regiones, confirma cada `EndFrame` con `FrameAcknowledge` y limita a 16 MiB por fotograma y 32 MiB descomprimido. Si las credenciales fallan, cada reintento (hasta 3) reabre el camino.

### 7.9 voxl: script embebido y arranque

```sh
# escritorio_voxl.sh — MAGI lo ejecuta con:
#   sh -c "$SCRIPT" magi-voxl <pantalla> <puerto> <programa> <portapapeles>
# La contraseña llega en la primera línea de stdin; el resto de stdin solo sirve para
# saber cuándo cierra MAGI el canal.
set -u
pantalla=$1 puerto=$2 programa=$3 portapapeles=$4
case $pantalla in :[0-9]|:[0-9][0-9]|:[0-9].[0-9]|:[0-9][0-9].[0-9]) ;; *) echo "ERROR pantalla" >&2; exit 64 ;; esac
case $puerto in ''|*[!0-9]*) echo "ERROR puerto" >&2; exit 64 ;; esac
[ "$puerto" -ge 1024 ] && [ "$puerto" -le 65535 ] || { echo "ERROR puerto" >&2; exit 64; }
case $programa in voxl|/*) ;; *) echo "ERROR programa" >&2; exit 64 ;; esac
case $programa in *[!A-Za-z0-9._/-]*) echo "ERROR programa" >&2; exit 64 ;; esac
case $portapapeles in true|false) ;; *) echo "ERROR portapapeles" >&2; exit 64 ;; esac
command -v "$programa" >/dev/null 2>&1 || { echo "ERROR sin_voxl" >&2; exit 69; }
echo "VOXL $("$programa" --version 2>/dev/null | head -n 1)"
IFS= read -r VOXL_PASSWORD || { echo "ERROR sin_clave" >&2; exit 65; }
export VOXL_PASSWORD
DISPLAY=$pantalla "$programa" --addr=127.0.0.1 --port="$puerto" --enable-resize=true \
  --file-transfers=none --audio-enabled=false --microphone-enabled=false \
  --enable-clipboard="$portapapeles" </dev/null &
pid=$!
unset VOXL_PASSWORD
echo "LISTO $pid"
cat >/dev/null                      # espera a que MAGI cierre el canal (o a que caiga SSH)
kill -TERM "$pid" 2>/dev/null
i=0
while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 5 ]; do sleep 1; i=$((i + 1)); done
kill -KILL "$pid" 2>/dev/null
```

```python
async def abrir_voxl(sesion, pool):
    tomada = await pool.conexion_para_canal(sesion.host.salto_host_id, Uso.ESCRITORIO)
    op = sesion.host.escritorio_opciones
    canal = await tomada.exec_con_argumentos("sh", "-c", SCRIPT_VOXL, "magi-voxl",
                                             op["pantalla"], str(sesion.host.puerto),
                                             op["programa"], str(op["portapapeles"]).lower())
    sesion.clave_voxl = Zeroizing(secrets.token_hex(16))
    await canal.escribir(sesion.clave_voxl + "\n")          # nunca en la línea de órdenes
    version = await plazo(10, canal.linea_que_empieza("VOXL "))
    if version_menor(version, "0.3"):
        raise Fallo("actualiza voxl a la 0.3 o posterior")
    await plazo(10, canal.linea_que_empieza("LISTO "))      # si el script sale antes: su stderr
    await plazo(10, esperar_salud(tomada, "127.0.0.1", sesion.host.puerto))   # GET /healthz → 200
    sesion.puerto_tunel = await tunel_local(tomada, "127.0.0.1", sesion.host.puerto)  # 127.0.0.1:0
    sesion.canal_voxl = canal                               # cerrarlo = fin de stdin = parar voxl
```

Los argumentos se validan en Rust antes de mandarlos y otra vez en el script (T30 y la regla de scripts embebidos). Los errores del script (`ERROR pantalla`, `ERROR sin_voxl`…) y la última línea de stderr de voxl se traducen a los mensajes de §6. El túnel de la sesión no se guarda en TUNELES ni sale en la vista Túneles.

### 7.10 Gracia

```python
def revisar_gracia(s, ahora):                               # cada segundo, en el servidor
    if s.estado != "abierto":
        return
    visto = s.visor_conectado or (s.tipo == "voxl" and s.tunel.conexiones_abiertas > 0)
    if visto:
        s.sin_visor_desde = None
        return
    s.sin_visor_desde = s.sin_visor_desde or ahora
    if ahora - s.sin_visor_desde >= config.gracia_seg:      # 60 por defecto
        cerrar(s, motivo="sin visor durante %d s" % config.gracia_seg)
```

### 7.11 Navegador

```python
CHROMIUM = ("chromium", "brave", "brave-browser", "google-chrome-stable",
            "google-chrome", "microsoft-edge-stable")

def abrir_navegador(url):                                   # lo hace el cliente, nunca el servidor
    if config.navegador != "auto":
        args = [a.replace("{url}", url) for a in shlex.split(config.navegador)]
    elif macos():
        app = primera_app(("Google Chrome", "Brave Browser", "Chromium"))
        args = ["open", "-na", app, "--args", f"--app={url}"] if app else ["open", url]
    elif (b := primero_en_path(CHROMIUM)):
        args = [b, f"--app={url}"]
    else:
        args = ["xdg-open", url]
    lanzar_sin_shell(args, stdin=None, stdout=None, stderr=None)   # sin heredar la terminal
```

Si no se puede lanzar nada, la pestaña enseña el enlace (válido 60 s) y `y` copia otro nuevo.

### 7.12 Sincronización: formato 2

```python
FORMATO_MAXIMO = 2

def formato_a_escribir(hosts):
    return 2 if any(h.tipo != "ssh" for h in hosts) else 1  # así MAGI 1.1 sigue leyendo si no hay escritorios

def leer_hosts(fichero):
    if fichero.formato > FORMATO_MAXIMO:
        return Estado.FORMATO_NUEVO                          # T74
    for e in fichero.hosts:
        e.datos.setdefault("tipo", "ssh")                    # formato 1: todos son SSH
        e.datos.pop("huella_escritorio", None)               # nunca viaja, aunque venga
```

Viajan `tipo`, `escritorio_dominio` y `escritorio_opciones`, que suben la versión del host como cualquier campo del inventario (T70). `huella_escritorio` y las contraseñas no viajan. Un host recibido con un tipo que no se puede validar se guarda como conflicto de elemento, no se aplica.

### 7.13 Validaciones

- Nombre único, como siempre; `tipo` de la lista y fijo; puerto 1-65535 (voxl, desde 1024).
- «A través de» de tipo `ssh`, sin ciclos (como los saltos) y obligatorio en voxl.
- Dirección: nombre o IP; en voxl, siempre `127.0.0.1`.
- Usuario RDP sin `\`: `DOMINIO\usuario` se separa en dominio y usuario al guardar.
- `escritorio_opciones` según §3.2; las claves desconocidas se conservan.
- VNC: una contraseña de más de 8 caracteres avisa «VNC solo usa los 8 primeros».
- Entrada del visor: coordenadas dentro del escritorio, códigos de tecla de la tabla, tamaños entre 200×200 y 8192×8192 y texto de hasta 1 MB; lo demás se descarta.
- `[escritorio]`: `gracia_seg` entre 10 y 3600, `puerto_visor` 0 o entre 1024 y 65535, `navegador` `auto` o una orden con `{url}`.

### 7.14 Casos especiales

- **Cambiar de tipo:** no se puede; se crea otro host y se borra el viejo.
- **Host SSH borrado** (aquí o en otro equipo): sus escritorios quedan sin «a través de». Un RDP sigue directo (va cifrado), un VNC directo pide confirmación al abrirlo y un voxl no se abre hasta elegir otro host.
- **Dos ventanas de MAGI:** las dos ven la sesión; cualquiera abre el visor; el certificado y las credenciales los pide la que abrió el escritorio (el solicitante).
- **Recargar el visor:** sigue con la misma clave. **Otro navegador u otra pestaña:** enlace nuevo con `y` o `↵`; el visor anterior pasa a «abierto en otra ventana».
- **Navegador sin H.264:** el `hola` del visor lo dice y MAGI reconecta sin AVC420.
- **Windows sin Display Control, VNC sin ExtendedDesktopSize o tamaño fijo:** el visor escala sin deformar; si el servidor cambia de tamaño (DesktopSize), el visor se adapta.
- **Pantalla de bloqueo de Windows:** se ve como cualquier pantalla; Ctrl+Alt+Supr está en la barra.
- **Suspender el portátil:** las conexiones caen y las sesiones pasan a `caido`; al volver, «Reconectar».
- **`magi servidor parar` con escritorios:** cuentan como trabajo (T67) y se pide confirmación.
- **Portapapeles de más de 1 MB:** se descarta con aviso en la barra del visor.
- **Puerto de voxl ocupado** (otra sesión o un voxl del usuario): «el puerto 6901 del host está ocupado: cambia el puerto de voxl en la ficha».
- **Escritorios Wayland:** voxl solo captura X11; GNOME y KDE en Wayland se ven por su propio servidor RDP.

### 7.15 Limitaciones heredadas

- R83-R85 de la F10 (sin la corrección 10b) siguen igual; el `formato` 2 no las cambia.
- Las F2-F6, F8, F10, F11 y F12 siguen sin validación manual completa (R15).
- Con H.264, al volver de la pestaña oculta no se pide la imagen entera (R117).
- `ironrdp-connector` e `ironrdp-acceptor` vendorizados: hay que revisarlos al subir russh o IronRDP (R118).

---

## 8. Requisitos No Funcionales

**Seguridad**

- **Servidor del visor:** solo `127.0.0.1`, en un puerto libre, y solo mientras hay sesiones de escritorio (T35, T98). Comprueba `Host` en todo y `Origin` en `/api/*`, con CSP estricta y sin recursos externos (T10).
- **Acceso al visor:** enlaces de un solo uso (256 bits, 60 s) y una clave por sesión; un solo visor a la vez; 429 tras 10 canjes fallidos en un minuto.
- **Entrada no fiable:** todo lo que llega del visor se valida (§7.13); lo que mandan los servidores RFB pasa por los límites de §7.7; un dato inválido cierra la sesión sin pánico.
- **Certificados RDP:** la misma política que las huellas SSH (T4, T101): se comprueban antes de mandar credenciales, un cambio bloquea en rojo y sustituirlo exige escribir el nombre del host. NLA liga las credenciales a ese TLS.
- **VNC:** por SSH por defecto; directo, con confirmación al guardar y `⚠` en la ficha. La autenticación VNC es débil (DES, 8 caracteres): por eso se recomienda el VNC en `127.0.0.1` del servidor y el acceso por SSH.
- **Credenciales:** solo en el llavero (T19), pedidas por el servidor al cliente (T25), por el socket en `Zeroizing` y nunca en el log.
- **voxl:** contraseña aleatoria por sesión, por la entrada estándar; voxl en `127.0.0.1` del host, sin ficheros, sin sonido ni micrófono; se para al cerrar. Su enlace lleva la contraseña de esa sesión y queda en el historial del navegador, pero deja de valer al cerrar la sesión (R111).
- **Equipos compartidos:** otros usuarios del mismo equipo pueden conectar a `127.0.0.1`; los enlaces y la clave protegen el visor, pero MAGI está pensado para equipos de un solo usuario.
- **Licencias:** sin dependencias GPL (T99); el aviso MIT del visor de voxl se conserva (T102).

**Protección de datos (RGPD)**

- La imagen, el teclado y el portapapeles de una sesión no se guardan: ni en disco, ni en el log, ni en REGISTRO (T44).
- REGISTRO guarda solo metadatos (host, tipo, duración y resultado), con la retención de 90 días de la F2.
- Las credenciales viven en el llavero y la huella en `magi.db`, en el equipo del usuario. No hay datos de terceros salvo lo que se ve en la pantalla remota, que no se trata.

**Backups y recuperación**

- La migración 8 solo añade columnas con valor por defecto. Una `magi.db` en la versión 8 no la abre un binario 1.1 (T74).
- Las sesiones de escritorio no se guardan: si el servidor se para, se cierran. Windows conserva su sesión y se reanuda al reconectar.

**Rendimiento**

- Abrir un RDP o un VNC en la red local en menos de 3 s sin diálogos; voxl en menos de 5 s.
- El puente añade menos de 5 ms. El H.264 de Windows llega a 60 fps sin gastar CPU de MAGI en vídeo.
- Rectángulos: al menos 30 fps a 1920×1080 con cambios parciales; con vídeo a pantalla completa por VNC baja (R110).
- Menos de 100 MB de memoria por sesión y menos de un núcleo para el puente a 1080p.
- Con la pestaña oculta, el visor pausa la imagen.

**Accesibilidad**

- TUI: glifo y texto en todos los estados (T5), modo ASCII y sin depender del color.
- Visor: botones con texto y `aria-label`, foco visible, contraste AA y atajos documentados; la barra no tapa el escritorio.

---

## 9. Integraciones

| Servidor | Protocolo | Qué usa MAGI | Notas |
|---|---|---|---|
| Windows 10/11 Pro y Enterprise, Windows Server 2016 o posterior | RDP 10 | TLS 1.2+, NLA, EGFX con AVC420, Display Control, CLIPRDR | Las ediciones Home no sirven RDP |
| xrdp 0.10 o posterior (Linux) | RDP | TLS, NLA según su configuración, mapas de bits | Normalmente sin H.264: rectángulos |
| GNOME Remote Desktop y KDE KRdp | RDP | TLS y NLA; H.264 si lo ofrecen | Es como se comparten los escritorios Wayland de GNOME y KDE |
| QEMU, libvirt y Proxmox | VNC (RFB 3.8) | None o VNC, ZRLE, ExtendedDesktopSize, teclas de QEMU | Mejor con `-vnc 127.0.0.1:N` y a través de SSH; así se ven también las VM de Windows XP y 2000 |
| TigerVNC (Xvnc) y x11vnc | VNC | VNC, ZRLE o Hextile, SetDesktopSize en TigerVNC | x11vnc no cambia de tamaño |
| voxl 0.3.x | HTTP y WebSocket propios | `--addr`, `--port`, `VOXL_PASSWORD`, `/healthz`, `--enable-resize`, `--file-transfers=none` | Solo X11 en Linux; el usuario lo instala (`cargo install voxl-server`) |
| Navegadores | WebCodecs | Chromium 94+, Brave, Chrome y Edge en modo app; Firefox 130+ en una pestaña | El H.264 lo decodifica el navegador |

**Especificaciones:** RFB (RFC 6143); MS-RDPBCGR, MS-RDPEGFX (AVC420), MS-RDPECLIP y MS-RDPEDISP; protocolo del visor de voxl (`web/src/protocol.ts` de la 0.3.3), ampliado en §5.2.

**Licencias:** IronRDP y `sspi` (MIT/Apache), `des` y `flate2` (MIT/Apache), `axum` (MIT) y el visor de voxl (MIT, con su aviso en `visor/LICENSE-voxl`). Nada de x264, FFmpeg ni OpenH264.

---

## 10. Decisiones Técnicas (ADR-lite)

- **D210 — El escritorio se ve en el navegador, con el visor de voxl adaptado y servido por `magi --servidor` en `127.0.0.1`** (Hector, 2a). Una TUI en Alacritty no pinta imágenes, y el navegador ya trae WebCodecs, pantalla completa y portapapeles. Descartados: una ventana nativa (más código y sin aprovechar voxl), lanzar FreeRDP o TigerVNC (cada uno con su interfaz) y pintar en el terminal con el protocolo de Kitty (Alacritty no lo admite y va a pocos fps).
- **D211 — Sin recomprimir ni x264** (Hector, 3a). Entre MAGI y el navegador sobra ancho de banda, así que los cambios van como rectángulos en bruto, y el H.264 que ya manda Windows (AVC420) pasa tal cual. MAGI sigue siendo MIT/Apache. Descartados: x264 como voxl (los binarios pasarían a GPL) y OpenH264 (patentes al distribuir).
- **D212 — RDP con IronRDP, fijado con `=`.** Es Rust, MIT/Apache, con TLS, NLA, CLIPRDR, Display Control y EGFX, y lo mantiene Devolutions. Solo RDP moderno: un servidor sin TLS falla con su motivo (Hector, 5a). Descartados: FreeRDP por FFI (C, otra cadena de compilación) y un cliente RDP propio (demasiado grande).
- **D213 — Cliente VNC propio (RFC 6143) en el núcleo.** RFB es sencillo y lo que hay en Rust no está maduro (`rfbclient` 0.1, inestable). Raw, CopyRect, Hextile y ZRLE cubren QEMU, TigerVNC y x11vnc; Tight queda fuera.
- **D214 — Los escritorios son hosts con `tipo`** (Hector, 7a). Así tienen grupos, etiquetas, búsqueda y sincronización sin tablas nuevas. «A través de» reutiliza `salto_host_id`. El tipo no cambia después de crear el host, y los hosts que no son SSH no salen en `magi_config` ni en las funciones de SSH (terminal, SFTP, túneles, snippets, salto). Descartado: una tabla ESCRITORIOS colgada de los hosts (dos listas en el inventario y los escritorios sin SSH, sueltos).
- **D215 — Por SSH siempre que se pueda** (Hector, 6a): un canal `direct-tcpip` de la conexión del pool, sin abrir puertos en el equipo. Directo solo RDP (va cifrado) y VNC con confirmación.
- **D216 — Las sesiones las custodia el servidor, con un minuto de gracia sin visor y un visor por sesión** (Hector, 9a). Cerrar el navegador o la TUI no corta la sesión; si nadie la mira en `gracia_seg`, se cierra. Un segundo visor sustituye al primero para no mezclar entradas.
- **D217 — Enlace de un solo uso y clave del visor en `sessionStorage`.** El enlace sale en la línea de órdenes del navegador (visible con `ps`) y en el historial, así que tiene que caducar al usarse. La clave vive solo en la pestaña; `Host` y `Origin` cortan el DNS rebinding y otras páginas. Descartados: cookies (en `127.0.0.1` se mandan a cualquier puerto) y un token fijo en la URL.
- **D218 — La huella del certificado RDP sigue la política de las huellas SSH y la guarda el cliente** en `HOSTS.huella_escritorio`, propia del equipo. El servidor no escribe esa columna (solo REGISTRO y estados) y la huella no viaja, para que una importación nunca la sustituya (T73). Descartado: que viaje, lo que obligaría a otra clase de conflicto.
- **D219 — voxl se arranca por SSH con un script embebido** (Hector, 1a). La contraseña va por la entrada estándar, nunca en la línea de órdenes; voxl escucha en `127.0.0.1` del host; el fin de la entrada estándar lo para. MAGI no instala voxl: si falta, lo dice. Al navegador se llega por una lanzadera para que el enlace de la orden sea de un solo uso.
- **D220 — `formato` 2 de sincronización solo cuando hay escritorios** (Hector, 4a). Un MAGI 1.1 leería un host RDP como si fuera SSH, así que con escritorios el fichero sube de formato y los equipos viejos lo marcan `formato_nuevo`. Sin escritorios, sigue en formato 1. La app Android se actualiza en esta fase para leer el formato 2 y enseñar los escritorios sin abrirlos. Versión 1.2.0 (MINOR, T68).
- **D221 — El visor se genera en `visor/dist/` y se versiona.** `scripts/generar-visor.sh` usa esbuild fijado; compilar MAGI no necesita Node, y la CI comprueba que `dist` está al día, como `assets/` en la F9. Descartado: esbuild en `build.rs`, como voxl (Node en cada compilación y en `dist`).
- **D222 — Teclado: el visor manda el keysym y el código físico.** VNC usa keysyms (y la extensión de QEMU si existe); RDP usa scancodes y deja que la distribución de Windows ponga los caracteres (ñ, AltGr, teclas muertas), con eventos Unicode de reserva. Descartado: traducir keysyms a scancodes con una distribución supuesta.
- **D223 — Funciones de la fase: imagen, teclado, ratón, portapapeles de texto, resolución que se ajusta y pantalla completa** (Hector, 8a). Sonido, imágenes en el portapapeles y ficheros quedan fuera.
- **D224 — Esta es la F12; «Android, segunda vuelta» pasa a ser la F13** (Hector, 10a). Abrir escritorios en el móvil irá allí o más adelante, con el mismo visor en Chrome de Android. Desde el 8 nov 2026 es la F14: la F13 es Servicios, Contenedores y Logs.
- **D225 — La decodificación va en un hilo por sesión y el puente agrupa a 60 fps con contrapresión.** Así un ZRLE grande no frena la red de las demás sesiones y un visor lento no acumula memoria: las zonas pendientes salen una vez con su estado final.
- **D226 — El navegador lo abre el cliente, sin shell:** `auto` busca Chromium, Brave, Chrome o Edge para el modo app y, si no, `xdg-open` u `open`; `[escritorio] navegador` admite una orden con `{url}`. El servidor no lanza procesos en la sesión gráfica del usuario.
- **D227 — Las credenciales se piden después de aceptar el certificado,** al cliente solicitante (T25): del llavero sin diálogo y, si fallan o no hay, con diálogo y 3 intentos, como en SSH.
- **D228 — (implementación) `ironrdp-connector` e `ironrdp-acceptor` vendorizados con `sspi` 0.23.1 y `picky` 7.0.0-rc.27.** IronRDP 0.17 fija `sspi` 0.21 y `picky` 7.0.0-rc.25, que arrastran `ecdsa` 0.17.0-rc.22, y russh 0.63 exige el `ecdsa` 0.17 estable: las dos generaciones no caben en el mismo grafo. Se copian dos crates sin tocar su código salvo dos líneas de adaptación a `sspi` 0.23. Descartados: duplicar RustCrypto (imposible), bajar russh y bifurcar IronRDP entero. Se quita en cuanto las versiones coincidan (R118).
- **D229 — (implementación) AVC420 con un `DvcProcessor` propio sobre `ironrdp-egfx` 0.3.** El meta-crate 0.17 no trae EGFX y la API de alto nivel descarta las regiones y exige decodificar; el procesador propio pasa el H.264 tal cual. La 0.5 de `ironrdp-egfx` no compila con la 0.17.
- **D230 — (implementación) TLS de RDP a mano con rustls.** Verificador que captura el certificado (para la huella antes de las credenciales), sin reanudación y con la clave del `SubjectPublicKeyInfo` para CredSSP.
- **D231 — (implementación) Un hilo con runtime propio por sesión de escritorio** (`lanzar_en_hilo_con`): el `ActiveStage` de IronRDP no es `Send`. La fábrica de la sesión corre dentro del hilo y el resto del servidor solo ve canales. Precisa D225.
- **D232 — (implementación) Protocolo del visor ampliado:** `GET /` para recargar, la orden `rec` para reconectar desde el aviso, el mensaje `latencia` tras `po` y el nombre legible en el canje.
- **D233 — (implementación) Precisiones del protocolo v7:** diálogos del servidor desde el visor en `[2^47, 2^48)`, `Bienvenida` sin la lista si está vacía y `CerrarEscritorio` durante «abriendo» como cancelación.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Inventario, modelo y protocolo | Migración 8; tipos de host y validaciones; formulario y ficha por tipo; Hosts, paleta, Flota y Sesiones con escritorios; fuera de `magi_config`, SFTP, túneles, snippets y saltos; sincronización con `formato` 2 y la app Android; protocolo v7 y contrato compilable antes de las pistas (T63) | 1,5 semanas |
| S2 — Visor, servidor del visor y VNC | Visor adaptado (rectángulos, copias, cursor, teclado, ratón, portapapeles, tamaño, pantalla completa, barra); `generar-visor.sh` y CI; servidor del visor (enlaces, clave, `Host`/`Origin`, CSP, límites); abrir el navegador; sesiones custodiadas, pestaña ▣ y gracia; cliente RFB con sus pruebas | 1,5 semanas |
| S3 — RDP | IronRDP: TLS, huella, credenciales, NLA, mapas de bits y RemoteFX, AVC420 tal cual, teclado por scancodes, CLIPRDR, Display Control, Ctrl+Alt+Supr; pruebas con `ironrdp-server` | 1,5 semanas |
| S4 — voxl, diagnóstico y cierre | voxl por SSH (script, versión, `/healthz`, túnel, lanzadera, parada); `magi diagnostico`; README, CHANGELOG y 1.2.0; prueba de extremo a extremo; revisión adversarial (T31, T54) | 1,5 semanas |

**Total: 4 sprints, ~6 semanas** (128 funcionalidades).

**Real (5 nov 2026):** implementada en Claude Code (en el árbol de trabajo, pendiente de subir). Revisión adversarial de cierre con su propio informe (`docs/fase12/magi-fase12-revision-adversarial.md`) y correcciones con pruebas de regresión. Verificación en local:

- `cargo test --workspace` en verde: 459 pruebas de la biblioteca del escritorio, 292 de redimensionado, 27 de `escritorio_visor`, 24 de `escritorio_rfb`, 19 de `escritorio_ui`, 14 de `escritorio_servidor`, 12 de `escritorio_rdp`, 9 de `escritorio_voxl`, 7 de `sincronizacion_escritorios` y la de `escritorio_e2e`.
- `fmt`, `clippy -D warnings`, `cargo deny`, `actionlint`, `desktop-file-validate`, `shellcheck`, `dist plan --tag v1.2.0`, `npm --prefix visor run check` y `generar-visor.sh --comprobar` en verde.
- Android: `assembleDebug`, `lintDebug` y las pruebas JVM; la instrumentada no se pudo ejecutar (el AVD se queda en la pantalla de bloqueo).

Pendiente: el guion de validación manual, el AVC420 con Windows 11 real, la prueba instrumentada de Android y la limitación de R117 (va en el prompt de reanudación).

**Supuestos**

- IronRDP compila con la cripto de russh y sin OpenSSL. Si su API no deja sacar el AVC420 sin decodificar, se queda en rectángulos y se anota como desviación (R108).
- El agente tiene en Omarchy Chromium para probar el visor, QEMU para una VM de prueba y `ironrdp-server` para las pruebas de RDP. Windows real lo valida Hector.
- voxl 0.3.x mantiene `--addr`, `--port`, `VOXL_PASSWORD` y `/healthz`.
- Node solo hace falta para regenerar el visor (desarrollo y CI).

**Guion de validación manual**

1. Windows 11 Pro en la red local por RDP directo: certificado nuevo, NLA con la contraseña guardada, «H.264» en la barra, redimensionar la ventana, pantalla completa, Ctrl+Alt+Supr, portapapeles en los dos sentidos y ñ, @ y € con AltGr.
2. Cerrar el visor y volver a abrirlo antes de 60 s desde la pestaña; después, dejarlo cerrado y comprobar que la sesión se cierra sola.
3. El mismo Windows a través de un host SSH (bastion).
4. Una VM de QEMU con `-vnc 127.0.0.1:0` en un host Linux, a través de ese host: imagen, cursor, cambio de resolución dentro de la VM y teclado español.
5. Una VM de Windows XP o 2000 en QEMU por VNC.
6. Linux con xrdp (RDP sin H.264) y GNOME con «Escritorio remoto» (RDP).
7. TigerVNC (Xvnc) con contraseña: el escritorio cambia de tamaño con la ventana.
8. voxl en una VM con `Xvfb :1` y un gestor de ventanas: arrancar, 60 fps, cerrar y comprobar con `pgrep -f voxl` que ya no corre.
9. Certificado cambiado (regenerar el de Windows): bloqueo en rojo y sustitución escribiendo el nombre.
10. Cortar la red con el visor abierto: estado caído y «Reconectar» desde el visor.
11. Dos ventanas de MAGI: las dos ven la sesión; abrir el visor desde la otra sustituye al primero.
12. Sincronización: crear un escritorio en un equipo y verlo en otro con MAGI 1.2 y en el móvil, sin poder abrirlo; un equipo con 1.1 marca «formato nuevo».
13. `magi diagnostico` con y sin navegador, y `[escritorio] navegador = "brave --app={url}"`.

---

## 12. Conexiones con Otras Fases

- **F1:** inventario, formulario, ficha, paleta y huellas (misma política, T4); `magi_config` excluye los escritorios.
- **F2:** llavero para las contraseñas de RDP y VNC; REGISTRO con tipos nuevos; Flota enseña los escritorios sin sondearlos.
- **F3:** servidor de sesiones, pestañas, Sesiones, protocolo (v7) y `TrabajoServidor`.
- **F5:** la mecánica del túnel local sirve para el túnel de la sesión de voxl; todo en loopback (T35).
- **F6:** snippets y deliberación ignoran los escritorios.
- **F7:** modo estrecho e instantáneas de las vistas nuevas (T45).
- **F9:** versión 1.2.0, CHANGELOG, `cargo deny`, `magi diagnostico` y `generar-recursos`.
- **F10:** `formato` 2; qué viaja y qué no (`huella_escritorio`, contraseñas).
- **F11:** el núcleo compartido tiene el cliente RFB, el adaptador de IronRDP y el puente; la app lee el `formato` 2 y enseña los escritorios.
- **F14 (Android, segunda vuelta; antes F13):** abrir escritorios en el móvil con el mismo visor en Chrome para Android y el puente en `magi-movil`.
