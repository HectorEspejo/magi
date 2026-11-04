# MAGI — Fase 12: Escritorio Remoto · Informe de implementación

> Documento vivo: se actualiza en cada sesión de trabajo, nunca se regenera
> desde cero. Refleja lo realmente implementado, las desviaciones y lo que
> queda pendiente. El desarrollador debe entregarlo al analista funcional antes
> de especificar la siguiente fase.

## Resumen de lo implementado

Estado a esta altura de la Fase 12 (sesión en curso):

- **Contrato compilable fijado** (commit `Fija el contrato de la Fase 12`):
  - Migración 8 (`user_version` 8): `HOSTS` gana `tipo` (ssh por defecto),
    `escritorio_dominio`, `escritorio_opciones` (JSON) y `huella_escritorio`;
    el resto de hosts queda como `ssh` y no cambia nada más.
  - Tipos de host y opciones de escritorio en `magi_nucleo::escritorio` con
    validación en código (sin `CHECK`), puertos por defecto (22/3389/5900/6901)
    y tipo fijo al crear el host.
  - Lectura/escritura de las columnas nuevas en el almacén, rechazo al leer una
    fila con tipo u opciones inválidas (T38), `huella_escritorio` como campo
    propio del equipo que no sube la versión ni viaja.
  - Protocolo v7 con los mensajes de escritorio (`AbrirEscritorio`,
    `EscritorioAbierto`, `PedirEnlace`/`Enlace`,
    `ReconectarEscritorio`, `CerrarEscritorio`, `Escritorios{lista}`,
    `PideHuellaEscritorio`/`RespuestaHuellaEscritorio`,
    `PideCredenciales`/`Credenciales`), `peticion_id` de escritorios desde
    2^46 y `TrabajoServidor.escritorios`.
  - Sección `[escritorio]` de `config.toml` validada (navegador, gracia,
    puerto del visor, distribución de teclado, H.264).
  - Tipos comunes de sesión, eventos de fuente, órdenes del visor, diálogos y
    errores; tabla única de teclas (scancodes y keysyms) y prueba de hilo por
    sesión.
  - Tipos de `REGISTRO` nuevos (`escritorio_abierto`, `escritorio_cerrado`,
    `escritorio_fallido`, `huella_escritorio`).

- **Pistas en marcha** (cada una con su tarea compartida):
  - Visor web en `visor/` (TypeScript adaptado de voxl 0.3.3, MIT) y
    `scripts/generar-visor.sh`.
  - Cliente RFB propio (VNC) en `magi-nucleo` con servidor RFB de prueba.
  - Cliente RDP con IronRDP 0.17 (TLS/huella, NLA, EGFX/AVC420 crudo, CLIPRDR,
    Display Control) y pruebas con `ironrdp-server`.
  - Puente y servidor HTTP/WebSocket del visor en `magi-nucleo`.
  - Custodia de escritorios en el servidor de sesiones (gracia, enlaces,
    diálogos al solicitante, REGISTRO).
  - TUI: formulario por tipo, ficha, Hosts, pestaña de escritorio, diálogos y
    apertura del navegador.
  - voxl por SSH (script embebido, `/healthz`, túnel y parada).
  - Sincronización `formato` 2 y app Android (leer y enseñar escritorios sin
    abrirlos).

## Desviaciones respecto a la especificación (qué y por qué)

1. **`ironrdp-connector` y `ironrdp-acceptor` vendorizados con `sspi` 0.23.1 y
   `picky` 7.0.0-rc.27.** IronRDP 0.17 fija `sspi` 0.21 y `picky` 7.0.0-rc.25,
   que arrastran `ecdsa 0.17.0-rc.22`; russh 0.63 (fijado en el proyecto) exige
   `ecdsa 0.17` estable a través de `p256`/`ssh-key`. Las dos generaciones no
   pueden convivir en el mismo grafo. Se vendorizan dos crates de IronRDP sin
   tocar su código salvo dos líneas de adaptación a la API de `sspi` 0.23
   (`KerberosConfig.kdc_resolution` y `TsRequest::buffer_len` que ahora
   devuelve `Result`), y solo cambian sus pin de `sspi`/`picky`. Revisar al
   subir russh o IronRDP; si sus versiones coinciden, quitar el vendor.
2. **EGFX no está en el meta-crate `ironrdp` 0.17.** Se añade
   `ironrdp-egfx = "=0.3.0"` (la generación compatible con `pdu` 0.9 y
   `dvc` 0.8; la 0.5 no compila con 0.17). Para que el AVC420 de Windows llegue
   al visor sin recomprimir se implementa un `DvcProcessor` propio que captura
   los `WireToSurface1` H.264 y sus regiones y los pasa tal cual; la API de alto
   nivel de IronRDP descarta las regiones y exige decodificar. Si el procesador
   propio no llegara a buen término, el respaldo es rectángulos RGBA y se
   anotaría como desviación (R108).
3. **El visor añade dos mensajes no listados en §5.2** (necesarios para lo que
   el checklist pide): la orden `rec` del visor al servidor para «Reconectar»
   desde el aviso de caído, y el mensaje de texto del servidor
   `{"t":"latencia","ms":N}` tras recibir `po` para poder enseñar la latencia
   real en la barra (el ping lleva epoch ms).
4. **`UsoCanal::Escritorio`** se añade en `src/servidor/conexiones.rs` para
   que el canal del pool de un escritorio se identifique como tal (CLAUDE.md
   habla del uso `Escritorio`; la variante no existía).
5. **`EventoEscritorio::Abierto`** se añade al contrato interno como señal de
   «sesión lista» para que el servidor sepa cuándo pasar a `abierto`, anotar
   `escritorio_abierto` y emitir el enlace sin esperar al primer píxel.

## Estructura de archivos creada/modificada

(Se completa al cierre de la fase. Principales a esta altura:)

- `crates/magi-nucleo/src/escritorio/`: `mod.rs` (tipos, opciones, eventos,
  órdenes, diálogos, errores, hilo por sesión), `teclas.rs`, `rdp/`, `rfb/`,
  `puente.rs`, `visor_http.rs`, `voxl.rs`.
- `crates/magi-nucleo/src/escritorio/scripts/escritorio_voxl.sh` (pista voxl).
- `crates/magi-nucleo/src/almacen/{migraciones.rs,hosts.rs,mod.rs}`,
  `crates/magi-nucleo/src/modelo.rs`, `crates/magi-nucleo/src/registro.rs`.
- `src/protocolo.rs` (v7), `src/config.rs` (`[escritorio]`),
  `src/servidor/escritorios.rs` y `src/servidor/mod.rs`,
  `src/app.rs` y `src/app/escritorios.rs`, `src/ui/**`.
- `visor/`: TypeScript, `package.json`, `dist/` generado y versionado,
  `entrar.html`, `LICENSE-voxl`, `README.md`; `scripts/generar-visor.sh`.
- `vendor/ironrdp-connector`, `vendor/ironrdp-acceptor` (parches mínimos),
  `Cargo.toml`/`Cargo.lock`.
- Documentación: `README.md` (sección «Escritorio remoto»), `CHANGELOG.md`,
  `docs/formato-sincronizacion.md` (formato 2), este informe y el checklist.

## Decisiones técnicas tomadas durante el desarrollo

- **Un hilo con runtime propio por sesión de escritorio** (`lanzar_en_hilo`) para
  que la decodificación (ZRLE/Hextile, bitmaps, RemoteFX) no frene el runtime de
  red del servidor (D225). El puente agrupa zonas sucias a 60 fps con
  contrapresión de 8 MB.
- **El flujo de transporte es genérico** (`Box<dyn AsyncRead + AsyncWrite +
  Unpin + Send>`): el mismo cliente RDP/VNC sirve para TCP directo y para un
  canal `direct-tcpip` del pool SSH.
- **TLS de RDP manual con rustls/aws-lc-rs**: verificador que captura el
  certificado DER, reanudación desactivada (CredSSP no la soporta) y
  `connect_finalize` con los bits del `SubjectPublicKeyInfo` (no el DER), que
  es lo que espera CredSSP.
- **Sin OpenSSL ni dependencias GPL**: IronRDP con la cripto de russh
  (aws-lc-rs) y el H.264 de Windows tal cual, sin x264/FFmpeg/OpenH264.
- **El servidor no dialoga ni toca el llavero** (T25): reenvía
  `PideHuellaEscritorio` y `PideCredenciales` al cliente solicitante y espera
  su respuesta; la huella la guarda el cliente que la acepta (D218).
- **Un solo árbol criptográfico**: se vendorizan dos crates de IronRDP antes que
  duplicar versiones incompatibles de RustCrypto (ver desviación 1).

## Funcionalidades del checklist completadas

(Aún ninguna marcada en `magi-fase12-checklist.md`; se marcarán al final de la
fase, cuando cada funcionalidad tenga su prueba. Contrato y migración 8 en
verde.)

## Pendientes y bloqueos

- Terminar las pistas en marcha: visor web, VNC, RDP, puente/servidor del visor,
  servidor de escritorios, TUI, voxl y sincronización/Android.
- Revisión adversarial de cierre (T31, T54) del servidor de escritorios, el
  servidor del visor, el puente y el cliente RFB.
- `cargo deny check` con las licencias nuevas (IronRDP, `sspi`, `des`, `axum`)
  y revisión de avisos RUSTSEC.
- Versión 1.2.0, fecha de la página man, `assets/` regenerados y
  `CHANGELOG.md`; CI del visor (`npm --prefix visor run check`,
  `scripts/generar-visor.sh --comprobar`).
- Guion de validación manual con Windows real, QEMU y voxl (no reproducible en
  este entorno).

## Ejecución y pruebas (cómo arrancar, migrar y testear)

- Compilar y arrancar la TUI: `cargo run`. Servidor en primer plano:
  `cargo run -- --servidor`; en otra terminal, `cargo run`.
- La migración 8 se aplica sola al abrir `magi.db` (no hay que hacer nada); una
  base de datos de la versión 8 no la abre un binario 1.1.
- Visor web: tras tocar `visor/src`, `npm --prefix visor ci`,
  `npm --prefix visor run check` y `scripts/generar-visor.sh`; compilar MAGI
  no necesita Node.
- Pruebas: `TZ=UTC cargo test --workspace --locked`; las de escritorio no usan
  red externa (servidor RFB de prueba, `ironrdp-server` y servidor SSH de
  prueba con un voxl falso).
- Diagnóstico: `cargo run -- diagnostico` (dice qué navegador abrirá el visor).
- Guion manual de la fase: ver §11 del informe funcional (Windows 11 por RDP
  directo y por SSH, VM QEMU por VNC, xrdp, TigerVNC, voxl con Xvfb, certificado
  cambiado, corte de red, dos ventanas, sincronización con el móvil,
  diagnóstico).
