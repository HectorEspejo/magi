# MAGI — Fase 12: revisión adversarial (escritorio remoto)

> Revisión sobre el árbol de trabajo (sin commit) el 2026-11-05 a las 00:16
> CET, mientras varias pistas seguían editando. Los hallazgos se revalidaron
> contra los ficheros en ese momento; las líneas pueden desplazarse si el
> árbol sigue cambiando.
>
> Huellas principales: visor_http.rs 83e424a86d1a, puente.rs e4e03e3df49b,
> rfb/mod.rs 7be8637835c2, rfb/protocolo.rs ffddd404344e, rfb/zrle.rs
> 00e4949b85a9, servidor/escritorios.rs 954977fd1250, app/escritorios.rs
> 502c83bbbcf2, visor/src/trabajador.ts 9060d8874f29.
>
> El cliente RDP y la TUI estaban en curso (los tests de RDP aún fallan y la
> pista siguió editando después de esa hora). Lo que no se pudo revisar a
> fondo se indica en «Límites de la revisión».

## Resumen ejecutivo

Tres defectos críticos, todos en el camino que custodia el servidor y en el
cliente VNC:

1. Una sesión voxl que es la única abierta no llega a enlazarse: el bucle de
   revisión para el servidor del visor por mirar solo el registro del visor,
   donde voxl no se da de alta hasta que ya está arrancado. La petición no
   recibe ni enlace ni Error y la TUI se queda «abriendo».
2. El contador de bytes pendientes del visor nunca descuenta los mensajes de
   texto (ping, hello, clip…): tras acumular 8 MB de texto el puente deja de
   mandar imagen y el visor se congela hasta reabrirlo.
3. El cliente RFB acumula sin cota los rectángulos ya decodificados antes de
   entregarlos al puente; un servidor VNC hostil puede mandar en unos pocos
   KB cientos de rectángulos ZRLE de 8192×8192 y agotar la memoria de
   magi --servidor (y con él todas las sesiones).

El resto de hallazgos mayores: portapapeles extendido RFB >16 KB roto,
colisión de peticion_id entre ventanas al pedir certificado/credenciales,
diálogo de certificado RDP repetido en cada reintento de contraseña, carrera
arranque/parada del servidor del visor, envío WebSocket sin plazo, detección
de H.264 insuficiente en el visor, v,0 que no detiene el vídeo y varias ramas
de apertura que no contestan a la petición.

## Hallazgos críticos

### C1. El servidor del visor se para durante la apertura de voxl y el cliente no recibe nada

Severidad: crítico.

Dónde:
- src/servidor/escritorios.rs:1939: revisar() llama a
  registro.parar_si_vacio() en cada pasada (cada segundo), sin mirar
  estado.escritorios.sesiones.
- crates/magi-nucleo/src/escritorio/visor_http.rs:297:
  parar_si_vacio() solo mira self.sesiones, el mapa del registro del visor.
- src/servidor/escritorios.rs:512: en abrir() solo se da de alta en el
  registro del visor si el host no es voxl.
- src/servidor/escritorios.rs:1853: voxl se da de alta con alta_voxl al final
  de tarea_voxl, después de SSH, guion, LISTO y /healthz, normalmente más de
  un segundo.
- crates/magi-nucleo/src/escritorio/visor_http.rs:435: enlace() devuelve None
  si puerto() es None.
- src/servidor/escritorios.rs:1869: tarea_voxl solo responde
  EscritorioAbierto si el enlace es Some; si es None, no manda Error ni nada.

Escenario: con el servidor recién arrancado, el usuario abre un host voxl y
es la única sesión de escritorio. asegurar_visor() arranca el listener;
abrir() inserta la sesión en sesiones pero no en el registro del visor. Antes
de que termine el SSH, la siguiente pasada de revisar() ve el registro vacío
y ejecuta parar(): pone puerto_actual = 0 y aborta la tarea de axum. Cuando
voxl termina de arrancar, alta_voxl inserta la sesión, pero registro.enlace()
ya no tiene puerto; el cliente no recibe ni EscritorioAbierto ni Error. La
sesión queda Abierto en el servidor, la TUI sigue con la petición en vuelo y
el servidor del visor está parado hasta que otra apertura lo vuelva a
arrancar. PedirEnlace posterior también falla.

Cómo comprobarlo: prueba de integración con un voxl falso que tarde más de
1 s en contestar LISTO y /healthz (se puede reutilizar el servidor falso de
tests/escritorio_voxl.rs). Abrir el host voxl y esperar 2 s:
registro.puerto() pasa a None y no llega MensajeServidor::EscritorioAbierto.

Camino de corrección: parar_si_vacio debe decidir con las sesiones del
servidor (Escritorios::cuenta o sesiones), no solo con el registro del visor;
o dar de alta un marcador de voxl en el registro al iniciar la apertura. Si
el enlace no se puede emitir, hay que contestar Error a la petición, nunca
dejarla sin respuesta.

---

### C2. El puente nunca descuenta los bytes de los mensajes de texto: el visor se congela a los 8 MB

Severidad: crítico.

Dónde:
- crates/magi-nucleo/src/escritorio/puente.rs:170: SalidaVisor::enviar suma
  MensajeVisor::bytes() a pendientes para binarios y textos.
- crates/magi-nucleo/src/escritorio/puente.rs:940: lote() no manda nada si
  pendiente() >= COLA_MAX (8 MB).
- crates/magi-nucleo/src/escritorio/visor_http.rs:1041: solo se llama a
  salida.descontar(bytes) en la rama Binario.
- crates/magi-nucleo/src/escritorio/visor_http.rs:1049: la rama Texto envía
  al WebSocket pero no descuenta nada.

Escenario: el remoto manda texto al portapapeles (VNC ServerCutText, RDP
CLIPRDR) y el puente lo reenvía como {"t":"clip",...} hasta 1 MB. Ocho textos
de 1 MB en la misma sesión dejan pendientes en 8 MB; a partir de ahí lote()
no manda rectángulos y enviar rechaza mensajes nuevos. El visor no vuelve a
pintar hasta que se cierra y se reabre (el contador es por SalidaVisor, no se
reinicia). Con uso normal, los ping de cada segundo y los latencia también
suman; en una sesión de días el fallo aparece sin necesidad de portapapeles.

Agravante: en puente.rs:173 la condición exige antes > 0, así que el primer
mensaje de una sesión puede superar COLA_MAX sin límite: un rectángulo de
8192×8192 es 268 435 456 bytes y se acepta entero. La contrapresión de 8 MB
no es un tope duro.

Cómo comprobarlo: en tests/escritorio_visor.rs (ya hay entorno y cliente WS),
dar de alta una sesión VNC con portapapeles=true, canjear, conectar el WS y
enviar ocho EventoEscritorio::Portapapeles de 1 MB. Después enviar un
EventoEscritorio::Rect y comprobar que no llega ningún binario 0x05; con el
contador corregido, el 0x05 llega. Para el primer mensaje, enviar un Rect de
8192×8192 y medir pendiente() antes de que el WS lo consuma.

Camino de corrección: descontar también en la rama Texto (o contar solo
binarios y poner un tope aparte para textos), y rechazar cualquier mensaje
individual mayor que COLA_MAX, sin excepción para el primero; si un
rectángulo completo no cabe, trocearlo o esperar al siguiente lote.

---

### C3. El cliente RFB acumula sin cota rectángulos decodificados antes de entregarlos: OOM con un servidor VNC hostil

Severidad: crítico (entrada no fiable; afecta a todo el servidor).

Dónde:
- crates/magi-nucleo/src/escritorio/rfb/mod.rs:593:
  eventos_pendientes: Vec<EventoEscritorio>.
- crates/magi-nucleo/src/escritorio/rfb/mod.rs:721: avanzar() procesa todo
  lo que quepa en la entrada en un solo bucle.
- crates/magi-nucleo/src/escritorio/rfb/mod.rs:1068: cada rectángulo
  terminado se empuja al vector.
- crates/magi-nucleo/src/escritorio/rfb/mod.rs:693: el vector no se drena
  hasta que avanzar() termina (después de la lectura de 64 KB).
- crates/magi-nucleo/src/escritorio/rfb/zrle.rs:62: cada Zrle::nuevo reserva
  ancho*alto*4 bytes de una vez.
- crates/magi-nucleo/src/escritorio/rfb/mod.rs:927: Raw hace
  Vec::with_capacity(total * 4).
- crates/magi-nucleo/src/escritorio/rfb/hextile.rs:60: Hextile::nuevo también
  reserva el rectángulo entero.

Escenario: un servidor VNC (o un servidor legítimo comprometido) manda un
FramebufferUpdate con muchos rectángulos de 8192×8192 en ZRLE, cada uno de un
solo color. Cada rectángulo pesa 16 384 teselas × 4 bytes sin comprimir
(64 KB) y zlib lo reduce a unos cientos de bytes. En una lectura de 64 KB
caben cientos de rectángulos; cada Zrle::nuevo reserva 268 MB y el evento se
queda en eventos_pendientes. Con 50 rectángulos son ~13 GB; el proceso muere
por OOM. No hay ningún tope de eventos, de rectángulos por actualización ni
de memoria total. Lo mismo con Hextile sólido y con varios Raw grandes
seguidos.

Cómo comprobarlo: ampliar tests/escritorio_rfb.rs con un FramebufferUpdate
de, por ejemplo, 20 rectángulos 8192×8192 ZRLE de color sólido (el
CompresorZrle de la prueba ya sirve). Medir RSS del proceso de test; con el
código actual crece en pasos de 268 MB y no se libera hasta terminar
avanzar(). Debe fallar con datos inválidos o con un tope de memoria/eventos,
no agotar la RAM.

Camino de corrección: acotar el número de eventos pendientes y el total de
píxeles por actualización; entregar los eventos al puente de forma
incremental (drenar dentro de avanzar) o descartar la actualización completa
si excede un presupuesto; no reservar el rectángulo entero antes de tener una
cota de memoria.

---

## Hallazgos mayores

### M1. El portapapeles extendido de VNC falla con textos de más de 16 KB

Severidad: mayor (mata la sesión VNC al copiar texto grande).

Dónde: crates/magi-nucleo/src/escritorio/rfb/protocolo.rs:457. En el bucle de
descomprimir_acotado se pasa la misma rebanada datos completa a
Decompress::decompress en cada iteración (línea 464), pero flate2 reinicia
next_in al principio de la rebanada en cada llamada; no avanza el puntero
como sí hace ZRLE en zrle.rs:190. El bucle solo funciona si la salida cabe en
los 16 KB del búfer.

Evidencia empírica: con un texto de 200 000 bytes comprimido a 218 bytes, la
primera llamada produce 16 384 bytes (total_in=35, total_out=16384); la
segunda vuelve a empezar y devuelve incorrect data check; la sesión acaba en
ErrorEscritorio::Datos («datos inválidos del servidor: zlib corrupto»). Se
reprodujo con un programa independiente enlazado contra la misma flate2
1.1.10 del Cargo.lock.

Escenario: servidor VNC con portapapeles extendido (TigerVNC, QEMU VNC con la
extensión); el usuario copia 20 KB de texto en el remoto. El cliente anuncia
capacidades, recibe CORTE_PROVISION, falla la descompresión y rfb::abrir
termina con Datos; la sesión cae.

Cómo comprobarlo: prueba unitaria de descomprimir_acotado con un zlib de
200 KB (o prueba de integración con corte_servidor_extendido y
CORTE_PROVISION); actualmente devuelve error. Debe devolver el texto completo.

Camino de corrección: avanzar datos por los bytes consumidos (total_in delta)
igual que ZRLE, o usar flate2::read::ZlibDecoder con un Take acotado.

---

### M2. pendientes se indexa solo por peticion_id: dos ventanas pueden pisarse el diálogo y una puede cancelar el de la otra

Severidad: mayor (escenario normal de varias ventanas).

Dónde:
- src/servidor/escritorios.rs:159 y :193: resolver_huella y
  resolver_credenciales hacen remove(&peticion_id) antes de comprobar que el
  cliente es el solicitante; si el cliente no coincide, el diálogo del
  solicitante desaparece.
- src/servidor/escritorios.rs:2113 y :2168: pendientes.insert(peticion_id,
  ...) sin incluir el cliente.
- src/app.rs:2678: cada proceso magi empieza su contador de escritorios en
  RANGO_ESCRITORIOS (2^46); dos ventanas distintas generan el mismo primer
  peticion_id.

Escenario: ventana A abre RDP-1 y ventana B abre RDP-2 casi a la vez. Ambas
usan 2^46+1. El segundo insert sobrescribe el oneshot del primero; al caer el
receptor, la tarea RDP de A termina con Cancelado («la ventana que abrió el
escritorio se cerró»). Si A contesta antes, resolver_huella quita el
pendiente de B, ve que el solicitante no coincide y lo descarta: B también
falla. El usuario ve una apertura fallida sin haber hecho nada.

Cómo comprobarlo: prueba de servidor con dos clientes (dos Cliente sobre el
socket, o dos DialogosSolicitante) que usen el mismo peticion_id; comprobar
que ambos reciben su PideHuellaEscritorio y que responder uno no afecta al
otro. Hoy uno de los dos pendientes se pierde.

Camino de corrección: indexar por (cliente_id, peticion_id) (o asignar los
peticion_id de diálogo en el servidor, como ya se hace para las reconexiones
del visor) y no extraer el pendiente hasta validar el solicitante.

---

### M3. El certificado RDP se vuelve a pedir en cada reintento de credenciales

Severidad: mayor (seguridad/UX: el usuario acepta el mismo certificado hasta
tres veces; el servidor no incorpora la huella que el cliente acaba de
guardar).

Dónde:
- src/servidor/escritorios.rs:958: en cada vuelta de tarea_apertura se
  reconstruye config_escritorio(&datos.host, ...), y datos.host es el leído
  al abrir; no se actualiza tras aceptar la huella.
- crates/magi-nucleo/src/escritorio/rdp/conexion.rs:100: politica(config,
  &certificado) compara con config.huella_escritorio, que sigue siendo la
  vieja (o None).

Escenario: RDP con certificado nuevo y contraseña incorrecta en el primer
intento. El usuario acepta el certificado (el cliente lo guarda en
HOSTS.huella_escritorio y responde). El servidor reintenta con la misma
config; el certificado sigue siendo «nuevo» para él y vuelve a mandar
PideHuellaEscritorio. El usuario tiene que aceptar el mismo certificado en el
intento 2 y, si falla, en el 3; en un certificado cambiado tiene que volver a
escribir el nombre del host cada vez.

Cómo comprobarlo: prueba RDP con servidor de prueba que rechaza la primera
autenticación y acepta la segunda; contar los PideHuellaEscritorio. Debe
haber uno; hoy hay dos. El test huella_igual_no_vuelve_a_preguntar solo
cubre una conexión, no el reintento de la capa de sesión.

Camino de corrección: actualizar datos.host.huella_escritorio (o
config.huella_escritorio) cuando el cliente responde
RespuestaHuellaEscritorio { aceptada: true }, antes de reintentar;
alternativamente, releer la fila del host entre intentos.

---

### M4. Carrera entre arrancar() y parar() del servidor del visor

Severidad: mayor (el listener puede quedar parado con una sesión viva, o con
puerto_actual=0 y la tarea de axum viva).

Dónde: crates/magi-nucleo/src/escritorio/visor_http.rs:243 (arrancar), :282
(parar) y :297 (parar_si_vacio). arrancar toma self.arranque, pero parar no
lo toma; el orden de puerto_actual.store y del mutex servidor no está
serializado con la parada.

Escenario: una sesión RDP se cierra y baja() llama parar_si_vacio() justo
cuando otra ventana abre un escritorio. El arrancar de la apertura nueva
puede intercalarse entre el store(0) y el servidor.take() de la parada: se
aborta la tarea recién creada o queda puerto_actual=0 con la tarea viva. El
nuevo enlace() devuelve None y la petición no recibe respuesta (ver M5/C1).

Cómo comprobarlo: prueba de concurrencia que llame a arrancar(0) y
parar_si_vacio() en dos tareas con tokio::join! y repita; comprobar que
arrancar devuelve un puerto y salud responde 200 (o que ambos quedan en un
estado coherente), y que puerto() no es None con la tarea viva. Hoy la
combinación puede dejar inconsistente.

Camino de corrección: tomar el mismo arranque en parar (o un cerrojo común) y
decidir parar con el estado de sesiones del servidor; no publicar el puerto
hasta tener servidor instalado.

---

### M5. El WebSocket del visor no tiene plazo de escritura: un cliente que no lee bloquea la tarea y la gracia no empieza

Severidad: mayor (espera sin plazo; fuga de tarea y de estado «visor
conectado»).

Dónde: crates/magi-nucleo/src/escritorio/visor_http.rs:1041 y :1049:
socket.send(...).await sin tokio::time::timeout. El único plazo del camino es
esperar_hola (5 s); después no hay ninguno.

Escenario: una pestaña del navegador se congela o deja de leer (TCP recibe
lleno). La tarea atender_visor queda bloqueada en send; compartido.conectado
sigue a true y el servidor no inicia la gracia. El puente sigue encolando
hasta COLA_MAX y luego deja de pintar; la sesión queda «viva» sin nadie que
la vea. Reabrir el visor sustituye al anterior, pero la tarea bloqueada y su
socket no se liberan.

Cómo comprobarlo: con un cliente WS que hace hola y luego no lee, provocar un
repintado grande (>1 MB); comprobar con tokio::time/contadores que
atender_visor no termina en 5 s y que RegistroVisor::visor_conectado sigue a
true. Con un plazo de send, el socket debe cerrarse y VisorDesconectado debe
llegar.

Camino de corrección: envolver cada send en un plazo corto (por ejemplo
5-10 s) y, al vencer, cerrar el socket y llamar a desconectar_visor; no
esperar indefinidamente a un cliente local.

---

### M6. El visor anuncia H.264 solo por typeof VideoDecoder: pantalla negra si el navegador no decodifica AVC

Severidad: mayor (navegadores con WebCodecs pero sin códec H.264).

Dónde: visor/src/trabajador.ts:118: const h264 = typeof VideoDecoder !==
'undefined'. No se prueba VideoDecoder.isConfigSupported("avc1...") ni se
comprueba que configure funcione. trabajador.ts:382 devuelve false si new
VideoDecoder o configure fallan; trabajador.ts:473 ignora ese false y sigue
recibiendo 0x07.

Escenario: Chromium/Firefox construido sin códecs propietarios (o con H.264
deshabilitado) tiene VideoDecoder pero configure({codec:'avc1...'}) lanza. El
visor manda {"t":"hola","h264":true}; el puente manda AVC420; el trabajador
no decodifica y no hay ni un mensaje de error. La pantalla queda negra; el
servidor nunca recibe SinH264 y no reconecta sin AVC420.

Cómo comprobarlo: en un navegador afectado (o con un stub de VideoDecoder que
lance en configure) abrir un host RDP con h264=true; el canvas no se pinta y
el estado sigue «conectado». Debe mandarse h264:false (o un mensaje que
provoque la reconexión sin AVC420).

Camino de corrección: antes del hola, probar
VideoDecoder.isConfigSupported({codec:'avc1.42E01E', ...}) (o el perfil del
SPS cuando llegue el primer fotograma) y mandar h264:false si no se puede
configurar; si falla después, avisar al servidor para reconectar sin AVC420.

---

### M7. v,0 no detiene el vídeo y RDP ignora la visibilidad: al volver puede quedar negro

Severidad: mayor (ancho de banda y pérdida de imagen en RDP H.264).

Dónde:
- crates/magi-nucleo/src/escritorio/puente.rs:848: video() no comprueba
  self.visible; cursor() y mandar_copia() tampoco.
- crates/magi-nucleo/src/escritorio/rdp/sesion.rs:518:
  OrdenEscritorio::Visible(_) | Reconectar => {}: el cliente RDP no hace nada
  al ocultar/mostrar.
- visor/src/cliente/arranque.ts:140: al ocultarse manda v,0 y soltarTodo.
- visor/src/trabajador.ts:508: al volver reinicia el decodificador
  (esperandoClave=true) y manda v,1, pero nadie pide un keyframe al RDP.

Escenario: pestaña oculta con una sesión RDP H264. El puente sigue mandando
0x07 (ancho de banda y cola de decodificación). Al volver, el trabajador
cierra el decodificador; los siguientes marcos son deltas y se descartan
hasta que Windows mande un IDR, que puede tardar minutos. El usuario ve negro
al volver a la pestaña aunque la sesión esté viva. En VNC, v,0 sí detiene las
peticiones, pero el cursor y el portapapeles siguen saliendo.

Cómo comprobarlo: prueba de puente con EventoEscritorio::Video después de
ComandoPuente::Visible(false); debería no llegar 0x07 y hoy llega. En RDP, con
Visible(true), comprobar que se manda una petición de refresco/keyframe (no
implementada).

Camino de corrección: ignorar video/cursor/copia con visible=false; en RDP,
reaccionar a Visible(true) pidiendo un keyframe (y a Visible(false) pausando
el envío), y no reiniciar el decodificador sin un keyframe pendiente.

---

### M8. Varias ramas de apertura no contestan a AbrirEscritorio (la TUI se queda «abriendo»)

Severidad: mayor (petición en vuelo para siempre; no hay Error).

Dónde:
- src/servidor/escritorios.rs:503: si apagando cambia entre las dos
  comprobaciones, return sin responder_error.
- src/servidor/escritorios.rs:531: la apertura simultánea perdedora llama
  registro.baja(sesion_id) y return sin contestar.
- src/servidor/escritorios.rs:1486 (marcar_abierto) y :1875 (voxl): si
  enlace() es None, solo se registra un warn; no hay Error.

Escenario: doble ↵ o dos ventanas abren el mismo host; la segunda pierde la
carrera y nunca recibe Error; su aperturas_escritorio y su tarjeta quedan
«abriendo». Lo mismo si el listener se para (C1/M4) o el servidor empieza a
apagarse justo después de aceptar la petición.

Cómo comprobarlo: pruebas de servidor que fuercen cada rama y esperen
MensajeServidor::Error con el peticion_id; hoy la lectura se queda esperando.
La prueba existente de abrir simultáneo no cubre la rama perdedora.

Camino de corrección: en todas las salidas tempranas tras validar
peticion_id, llamar a responder_error (o a un helper que lo haga) con el
motivo; no dejar una petición sin respuesta.

---

### M9. La opción de teclado del host RDP se ignora: se usa siempre la distribución global

Severidad: mayor (el teclado de Windows no coincide con el de la ficha).

Dónde:
- src/servidor/escritorios.rs:494: al abrir y al reconectar se toma
  config.escritorio.distribucion_efectiva() (config global), no
  host.escritorio_opciones.distribucion_efectiva(...).
- src/servidor/escritorios.rs:958: config_escritorio(&datos.host, ...) recibe
  el global como distribucion_teclado.
- crates/magi-nucleo/src/escritorio/rdp/conexion.rs:253: keyboard_layout:
  klid(&config.distribucion_teclado).

Escenario: host RDP con escritorio_opciones.teclado = "es-ES" y
[escritorio] distribucion_teclado = "auto" que resuelve en-US (o al revés).
La ficha enseña «es-ES», pero el conector anuncia en-US y Windows interpreta
mal las teclas. La opción del host nunca tiene efecto.

Cómo comprobarlo: prueba de config_escritorio o integración RDP que lea el
keyboard_layout del ClientInfo; con el host en es-ES debe ser el KLID de
es-ES. Hoy es el global.

Camino de corrección: resolver
host.escritorio_opciones.distribucion_efectiva(&config.escritorio.
distribucion_efectiva()) al construir ConfigEscritorio y usar ese valor en
RDP.

---

## Hallazgos menores

### m1. CopyRect aislado no se presenta hasta el siguiente lote

Dónde: puente.rs:737 manda 0x06 sin 0x08; trabajador.ts:284 pinta la copia en
el búfer, y trabajador.ts:309 solo presenta al recibir 0x08. Un
FramebufferUpdate que solo traiga CopyRect no se ve hasta que llegue otro
lote con Marco. Comprobación: servidor VNC de prueba que mande solo CopyRect;
el canvas visible no cambia (el búfer sí). Corrección: mandar 0x08 tras la
copia o incluir la copia en el siguiente lote.

### m2. Los botones 8 y 9 del ratón se descartan en el puente

Dónde: puente.rs:1075 rechaza boton > 7; visor/src/cliente/raton.ts:8 mapea
los botones DOM 3/4 (atrás/adelante) a 8/9, y rdp/entrada.rs:138 sabe
traducirlos a MouseButton::X1/X2. Atrás/adelante no funciona; RFB solo admite
8 botones, pero el puente debería aceptar 9 y dejar que RFB descarte el 9.
Comprobación: enviar b,8,1 y comprobar que el puente devuelve None; con RDP,
que no llega ninguna operación.

### m3. La TUI abre el visor o copia un enlace con respuestas que ya no están en vuelo

Dónde: src/app/escritorios.rs:821: peticiones_escritorio.remove(&peticion_id)
puede devolver None y aun así se llama a abrir_visor_escritorio; en
src/app/escritorios.rs:833 un None copia el enlace. Una respuesta tardía de
una apertura cancelada abre una pestaña del navegador o escribe en el
portapapeles sin que el usuario lo haya pedido. Comprobación: enviar
EscritorioAbierto con un peticion_id desconocido y comprobar que no se lanza
el navegador (el test puede inyectar navegador); hoy se lanza.

### m4. El diálogo de credenciales busca el host por nombre, no por id

Dónde: src/app/escritorios.rs:940, :978 y :1340. El mensaje PideCredenciales
trae escritorio_id; la App tiene el host_id en escritorios. Si el host se
renombra con el diálogo en vuelo, o se reutiliza el nombre para otro host, se
leen usuario/dominio del host equivocado y se puede ofrecer una contraseña
del llavero que no corresponde. Comprobación: renombrar el host entre
PideCredenciales y la apertura del diálogo; comprobar que los campos y el
llavero no cambian.

### m5. Se guarda la contraseña en el llavero y se marca el host antes de saber si autentica

Dónde: src/app/escritorios.rs:1339: con recordar y contraseña no vacía,
guardar_contrasena_escritorio se llama al pulsar ↵, antes de que el servidor
confirme la autenticación. Una contraseña incorrecta queda guardada y el host
pasa a ContrasenaLlavero; la siguiente apertura la usa automáticamente y
vuelve a fallar. Comprobación: contraseña incorrecta + «recordar»; reabrir y
ver que se usa el llavero. Debería guardarse solo tras el éxito (o pedir
confirmación al cliente cuando el servidor confirme).

### m6. El trabajador del visor no libera los OffscreenCanvas al cerrar

Dónde: visor/src/trabajador.ts:529: onCerrar cierra el VideoDecoder y el
WebSocket, pero lienzo, bufer, contextoVisible y contextoBufer siguen
apuntando a canvas del tamaño del escritorio. Si window.close() no cierra la
pestaña (no fue abierta por script, o el navegador lo bloquea), la página se
queda con la memoria del escritorio. Comprobación: pulsar «Cerrar visor» en
una pestaña que no se cierre; bufer.width sigue siendo el ancho del
escritorio y la memoria no baja.

### m7. lote() vacía el sucio aunque no haya podido mandar ninguna zona

Dónde: puente.rs:958: si todas las recorte() fallan (por ejemplo, tras un
redimensionar a menor con regiones viejas unidas), enviadas == 0 y se hace
sucio.vaciar(); no se manda ningún Rect ni Marco y la zona válida se pierde.
RFB suele pedir una actualización completa tras DesktopSize, así que a menudo
se recupera, pero RDP/otros pueden no hacerlo. Comprobación: forzar un Tamano
menor con sucio previo y comprobar que no se manda imagen; con la corrección,
se descartan solo las regiones fuera del búfer y se repinta el resto.

### m8. fps no cuenta los marcos de vídeo H.264

Dónde: puente.rs:964 solo incrementa marcos al mandar Marco de rectángulos;
video() no lo hace. La lista Escritorios y la barra muestran 0 fps en una
sesión RDP H.264 aunque haya vídeo. Comprobación: sesión RDP con AVC420 y leer
fps en la difusión; debe ser >0.

### m9. estado_visor y desconectar_visor usan try_send y pueden perder el aviso

Dónde: visor_http.rs:627 y :722. Si el canal de comandos del puente está
lleno (flujo de teclas/ratón), VisorDesconectado se pierde y el servidor
sigue creyendo que hay visor; la gracia no empieza. estado_visor también
puede perder «caído/cerrado». Comprobación: llenar el canal con órdenes y
cerrar el WS; comprobar que compartido.conectado no baja. Debe usarse
send().await con plazo o un canal dedicado para los avisos de estado.

### m10. Lector de líneas de voxl sin cota de búfer

Dónde: crates/magi-nucleo/src/escritorio/voxl.rs:349 y :399: bufer_salida y
bufer_errores crecen con String::from_utf8_lossy sin límite hasta que aparece
un salto de línea; el script/voxl de un host comprometido puede mandar un
flujo sin saltos y agotar la memoria. Comprobación: voxl falso que escribe
100 MB sin salto; el proceso del servidor crece sin cota. Hay que
cortar/diagnosticar a partir de un máximo (por ejemplo 1 MB).

## Estado de verificación en la revisión

Comandos y resultados (con CARGO_TARGET_DIR=$HOME/.cache/magi-target-revision):

- cargo check --workspace --all-targets --locked: OK.
- cargo test -p magi-nucleo --test escritorio_rfb: 22/22.
- cargo test -p magi-nucleo --test escritorio_visor: 19/19.
- cargo test -p magi-nucleo --test sincronizacion_escritorios: 7/7.
- cargo test --test escritorio_servidor: 6/6 (un fallo de difusión
  aparecido a las 23:58 quedó corregido en la siguiente compilación).
- cargo test --test escritorio_ui: 16/16.
- cargo test -p magi-nucleo --test escritorio_rdp: 10/12; fallan
  tls_certificado_nuevo_y_rectangulos (espera 64×48 y recibe 1×1) y
  portapapeles_cliprdr_en_los_dos_sentidos (timeout).
- npm --prefix visor run check: OK (formato, oxlint y tsc).
- scripts/generar-visor.sh --comprobar: OK, visor/dist al día.
- cargo fmt --all --check: falla; 104 bloques de diff. Es puerta de CI aunque
  no sea un defecto funcional.

No se ejecutaron escritorio_e2e, escritorio_voxl ni las pruebas de
Android/Gradle; el entorno de esta revisión no tenía el SDK/NDK levantado.
RDP estaba en curso y sus dos fallos no se han depurado aquí.

## Límites de la revisión

- RDP: el cliente está en curso (ficheros creados entre las 23:37 y las
  00:06). Se revisaron por encima TLS/huella y el orden
  certificado-antes-de-credenciales, pero no la corrección de EGFX/AVC420,
  CLIPRDR ni Display Control; sus tests aún fallan (arriba). No se afirma que
  RDP esté terminado.
- Android: no se compiló; se revisó estáticamente que abrir_sesion,
  instalar_clave y los túneles del móvil rechazan TipoHost no SSH
  (gestor.rs:449, :1641, :1010) y que huella_escritorio no se expone en
  HostVista.
- Sincronización: los 7 tests de sincronizacion_escritorios pasan;
  huella_escritorio no viaja y el formato 2 solo se escribe con escritorios.
  No se revisó la fusión completa de todos los formatos.
- El árbol estaba siendo editado por otras pistas durante la revisión; los
  fallos de formato/UI que aparecieron a primera hora se corrigieron solos
  (por ejemplo, el diálogo de certificado ya solo acepta con ↵ y el test de
  UI pasa). Los hallazgos de arriba se revalidaron a las 00:16.

## Prioridad sugerida para el Lead

1. C1 (voxl no enlaza) y C2 (contador de texto) antes de dar la fase por
   cerrada: son fallos funcionales silenciosos y fáciles de reproducir.
2. C3 (OOM RFB) y M1 (portapapeles >16 KB): entrada no fiable; añadir tests
   de regresión.
3. M2/M3 (diálogos y certificado RDP): afectan a varias ventanas y a la
   política de certificados.
4. M4/M5/M6/M7/M8/M9 y los menores según el tiempo disponible.
5. No cerrar la fase con cargo fmt en rojo ni con escritorio_rdp fallando (o
   anotar RDP como pendiente explícito en el informe de implementación).


---

## Estado de las correcciones (Lead, 2026-11-05)

Todas las correcciones se han aplicado con prueba de regresión y verificadas
por sus pistas; el árbol compila y las suites de escritorio están en verde.

| Hallazgo | Estado | Dónde |
|---|---|---|
| C1 voxl como única sesión no enlazaba | Corregido | `sincronizar_visor(estado)` en `src/servidor/escritorios.rs`; enlace antes de `marcar_abierto`; prueba `voxl_lento_como_unica_sesion_abre_y_da_enlace` |
| C2 el contador de texto congelaba el visor | Corregido | `visor_http.rs` descuenta siempre los bytes; troceo de zonas > `COLA_MAX`; pruebas `los_textos_no_congelan_el_puente` y `un_rectangulo_mayor_que_la_cola_se_trocea` |
| C3 RFB acumulaba rectángulos sin cota (OOM) | Corregido | tope de 16 MB pendientes + presupuesto por `FramebufferUpdate`; pruebas de tandas y de rechazo sin reservar |
| M1 portapapeles extendido RFB > 16 KB | Corregido | `ZlibDecoder` sobre `Take`; pruebas con ~200 KB |
| M2 diálogos pisados entre ventanas | Corregido | `pendientes` por `(cliente_id, peticion_id)`; prueba de dos ventanas con el mismo id |
| M3 certificado RDP repetido en reintentos | Corregido | el servidor relee `HOSTS.huella_escritorio` en cada intento; prueba unitaria |
| M4 carrera arrancar/parar del visor | Corregido | cerrojo de ciclo de vida con generación; prueba con 30 carreras |
| M5 WebSocket sin plazo de escritura | Corregido | `PLAZO_ENVIO` de 10 s en todas las escrituras; prueba con visor que no lee |
| M6 detección de H.264 solo por `typeof` | Corregido | `VideoDecoder.isConfigSupported` con 4 perfiles antes del `hola` y reconexión única sin AVC si falla; prueba de navegador |
| M7 `v,0` no detenía el vídeo | Corregido en el puente | con pestaña oculta no se manda vídeo/cursor/copia y se avisa `Visible`; el cliente RDP ignora aún `Visible` (el keyframe al volver con H.264 queda como limitación anotada) |
| M8 ramas de apertura sin respuesta | Corregido | toda rama responde `Error` o anota `escritorio_fallido`; prueba de dos aperturas simultáneas |
| M9 teclado del host RDP ignorado | Corregido | `config_escritorio` usa `escritorio_opciones.distribucion_efectiva`; prueba unitaria |
| m1 CopyRect aislado sin presentar | Corregido | `Marco` inmediato tras copia aislada |
| m2 botones 8/9 del ratón | Corregido | el puente los acepta (RFB descarta el 9, RDP los usa) |
| m3 respuestas de peticiones canceladas en la TUI | Corregido | se descartan; prueba nueva |
| m4 diálogo de credenciales por nombre | Corregido | se resuelve el host por id de sesión |
| m5 contraseña guardada antes de autenticar | Corregido | se guarda solo cuando la difusión dice `abierto`; se descarta al cancelar/caer |
| m6 `OffscreenCanvas` no liberados | Corregido | lienzos a 0×0 y contextos sueltos al cerrar/reconectar; prueba de navegador |
| m7 `lote()` vaciaba el sucio sin mandar | Corregido | solo se quita del sucio lo mandado |
| m8 fps no contaba marcos H.264 | Corregido | los `VideoRdp` cuentan |
| m9 avisos con `try_send` | Corregido | canal de estados sin pérdida separado del de comandos |
| m10 lector de líneas de voxl sin cota | Corregido | tope de 1 MB por línea a medias; el canal se cierra y la sesión falla con motivo |

Límites de la revisión: el cliente RDP y la TUI estaban en curso durante la
revisión; sus tests ya están en verde y las correcciones de arriba se
verificaron después. La validación manual con Windows, QEMU y voxl reales
sigue siendo del guion de §11 del informe funcional.
