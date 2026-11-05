// Trabajador del visor: es el dueño del WebSocket, del OffscreenCanvas y del
// decodificador H.264. Pinta los rectángulos y las copias en un búfer y los
// presenta de golpe al recibir Marco, para que no se vean medios lotes; el
// vídeo se decodifica con WebCodecs y solo se pintan las regiones de cada
// marco. La página se queda con el teclado, el ratón, el portapapeles y la
// barra, y habla con este trabajador por mensajes.
import { Bin, cmd, LADO_MAXIMO, PORTAPAPELES_MAXIMO } from './protocolo';
import type {
	DesdeTrabajador,
	HaciaTrabajador,
	MensajeHola,
	MensajeServidor,
	Region,
	TipoImagen,
} from './protocolo';

declare const self: DedicatedWorkerGlobalScope;

let lienzo: OffscreenCanvas | null = null;
let contextoVisible: OffscreenCanvasRenderingContext2D | null = null;
/** Búfer donde se pinta el lote en curso; se vuelca al lienzo en Marco. */
let bufer: OffscreenCanvas | null = null;
let contextoBufer: OffscreenCanvasRenderingContext2D | null = null;
let ancho = 0;
let alto = 0;
let imagen: TipoImagen = 'rectangulos';
let visible = true;
let url = '';
let clave = '';
let ws: WebSocket | null = null;
/** El cierre lo pidió el servidor o la página: no es una caída. */
let cierrePedido = false;

let decodificador: VideoDecoder | null = null;
let codec = '';
let esperandoClave = true;
let contadorMarcas = 0;
const regionesPorMarca = new Map<number, Region[]>();

/** Perfiles H.264 que se prueban antes del alta (AVC420 de RDP es 4:2:0). */
const CODIFICACIONES_AVC = [
	'avc1.42E01E', // Baseline 3.0
	'avc1.42E01F', // Baseline 3.1
	'avc1.4D401E', // Main 3.0
	'avc1.640028', // High 4.0
];
/** Si el navegador tarda en contestar isConfigSupported, se anuncia que no. */
const ESPERA_SOPORTE_MS = 2000;
/** El navegador decodifica de verdad H.264 (comprobado antes del hola). */
let soporteH264 = false;
let promesaSoporte: Promise<boolean> | null = null;
/** Ya se ha reconectado una vez pidiendo rectángulos tras fallar el décodec. */
let reintentoSinH264 = false;

let fotogramas = 0;
let ultimoInforme = performance.now();

function publicar(m: DesdeTrabajador, transferir: Transferable[] = []): void {
	self.postMessage(m, transferir);
}

function enviar(orden: string): void {
	if (ws && ws.readyState === WebSocket.OPEN) ws.send(orden);
}

/** Valida un lado (ancho o alto) que llega del servidor. */
function lado(valor: unknown): number | null {
	if (
		typeof valor !== 'number' ||
		!Number.isInteger(valor) ||
		valor < 1 ||
		valor > LADO_MAXIMO
	)
		return null;
	return valor;
}

/** Prepara el lienzo visible y el búfer de lote al tamaño del escritorio. */
function prepararLienzo(nuevoAncho: number, nuevoAlto: number): void {
	if (!lienzo) return;
	// El escritorio puede cambiar de tamaño: el búfer anterior se libera antes
	// de reservar el nuevo.
	soltarLienzo(bufer);
	bufer = null;
	contextoBufer = null;
	ancho = nuevoAncho;
	alto = nuevoAlto;
	lienzo.width = nuevoAncho;
	lienzo.height = nuevoAlto;
	contextoVisible = lienzo.getContext('2d', {
		alpha: false,
		desynchronized: true,
	});
	bufer = new OffscreenCanvas(nuevoAncho, nuevoAlto);
	contextoBufer = bufer.getContext('2d', {
		alpha: false,
		desynchronized: true,
	});
	if (contextoVisible) {
		contextoVisible.fillStyle = '#000000';
		contextoVisible.fillRect(0, 0, nuevoAncho, nuevoAlto);
	}
	if (contextoBufer) {
		contextoBufer.fillStyle = '#000000';
		contextoBufer.fillRect(0, 0, nuevoAncho, nuevoAlto);
	}
}

/**
 * Suelta un OffscreenCanvas. Los navegadores no le dan close() (no está en la
 * especificación): reducirlo a 0×0 libera el búfer del tamaño del escritorio y
 * después se sueltan la referencia y su contexto.
 */
function soltarLienzo(canvas: OffscreenCanvas | null): void {
	if (!canvas) return;
	try {
		canvas.width = 0;
		canvas.height = 0;
	} catch {
		// Un lienzo ya inutilizable: solo queda soltar las referencias.
	}
}

/** Cierra el WebSocket si sigue abierto. */
function cerrarSocket(): void {
	try {
		ws?.close();
	} catch {
		// Ya estaba cerrado.
	}
	ws = null;
}

/**
 * Suelta el lienzo visible y el búfer del lote: son del tamaño del escritorio
 * (hasta 8192×8192) y no deben quedarse en memoria cuando el visor termina. Se
 * llama solo en las ramas definitivas (cerrar visor, close del servidor), no en
 * una caída que todavía puede reconectar.
 */
function liberarLienzos(): void {
	soltarLienzo(bufer);
	bufer = null;
	soltarLienzo(lienzo);
	lienzo = null;
	contextoBufer = null;
	contextoVisible = null;
	ancho = 0;
	alto = 0;
}

function reiniciarDecodificador(): void {
	if (decodificador) {
		try {
			decodificador.close();
		} catch {
			// Ya estaba cerrado.
		}
		decodificador = null;
	}
	codec = '';
	esperandoClave = true;
	regionesPorMarca.clear();
}

function conectarSocket(): void {
	let sock: WebSocket;
	try {
		sock = new WebSocket(url);
	} catch {
		publicar({
			t: 'caido',
			motivo: 'no se ha podido abrir el WebSocket del visor',
		});
		return;
	}
	ws = sock;
	sock.binaryType = 'arraybuffer';
	sock.addEventListener('open', () => {
		publicar({ t: 'abierto' });
		// Solo se anuncia H.264 si isConfigSupported ha dicho que se puede
		// decodificar de verdad; si no, el servidor reconecta RDP sin AVC420 y
		// manda rectángulos (D211).
		const h264 = soporteH264 && !reintentoSinH264;
		sock.send(JSON.stringify({ t: 'hola', clave, h264 }));
		if (!visible) enviar(cmd.visible(false));
	});
	sock.addEventListener(
		'message',
		(evento: MessageEvent<string | ArrayBuffer>) => {
			if (typeof evento.data === 'string') onTexto(evento.data);
			else onBinario(new Uint8Array(evento.data));
		}
	);
	sock.addEventListener('close', () => {
		if (ws !== sock) return;
		ws = null;
		reiniciarDecodificador();
		if (!cierrePedido)
			publicar({
				t: 'caido',
				motivo: 'se ha perdido la conexión con el servidor del visor',
			});
	});
}

// ---------------------------------------------------------------- texto del servidor

function onTexto(texto: string): void {
	let mensaje: MensajeServidor;
	try {
		mensaje = JSON.parse(texto) as MensajeServidor;
	} catch {
		return;
	}
	if (!mensaje || typeof mensaje !== 'object' || typeof mensaje.t !== 'string')
		return;
	switch (mensaje.t) {
		case 'ping':
			if (typeof mensaje.s === 'number') enviar(cmd.pong(mensaje.s));
			return;
		case 'close':
			cierrePedido = true;
			reiniciarDecodificador();
			publicar({ t: 'cerrado' });
			cerrarSocket();
			liberarLienzos();
			return;
		case 'hello':
			onHola(mensaje);
			return;
		case 'tam': {
			const nuevoAncho = lado(mensaje.w);
			const nuevoAlto = lado(mensaje.h);
			if (nuevoAncho === null || nuevoAlto === null) return;
			prepararLienzo(nuevoAncho, nuevoAlto);
			reiniciarDecodificador();
			publicar({
				t: 'tam',
				w: nuevoAncho,
				h: nuevoAlto,
			});
			return;
		}
		case 'clip':
			if (typeof mensaje.texto !== 'string') return;
			if (mensaje.texto.length > PORTAPAPELES_MAXIMO) return;
			publicar(mensaje);
			return;
		case 'latencia':
			if (typeof mensaje.ms !== 'number' || !Number.isFinite(mensaje.ms))
				return;
			publicar(mensaje);
			return;
		case 'estado':
			if (typeof mensaje.estado !== 'string') return;
			publicar(mensaje);
			return;
	}
}

function onHola(mensaje: MensajeHola): void {
	const nuevoAncho = lado(mensaje.w);
	const nuevoAlto = lado(mensaje.h);
	if (nuevoAncho === null || nuevoAlto === null) return;
	const imagenValida: TipoImagen[] = ['h264', 'rectangulos', 'voxl'];
	const tipoValido = ['rdp', 'vnc', 'voxl'].includes(mensaje.tipo);
	if (!tipoValido) return;
	imagen = imagenValida.includes(mensaje.imagen)
		? mensaje.imagen
		: 'rectangulos';
	prepararLienzo(nuevoAncho, nuevoAlto);
	reiniciarDecodificador();
	publicar({
		t: 'hello',
		w: nuevoAncho,
		h: nuevoAlto,
		tipo: mensaje.tipo,
		imagen,
		redimensionable: mensaje.redimensionable === true,
		portapapeles: mensaje.portapapeles === true,
	});
}

// ---------------------------------------------------------------- binarios del servidor

function onBinario(datos: Uint8Array): void {
	// Marco es un mensaje de un solo byte.
	if (datos.byteLength < 1) return;
	const vista = new DataView(datos.buffer, datos.byteOffset, datos.byteLength);
	switch (datos[0]) {
		case Bin.Cursor:
			onCursor(datos, vista);
			return;
		case Bin.Rect:
			onRect(datos, vista);
			return;
		case Bin.Copia:
			// Cabecera de 13 bytes: origen, destino y tamaño (u16 cada uno).
			if (datos.byteLength < 13) return;
			onCopia(vista);
			return;
		case Bin.VideoRdp:
			onVideo(datos, vista);
			return;
		case Bin.Marco:
			presentar();
			return;
	}
}

/** Cursor remoto: se valida y se manda a la página, que lo pinta con CSS. */
function onCursor(datos: Uint8Array, vista: DataView): void {
	if (datos.byteLength < 9) return;
	const cursorAncho = vista.getUint16(1);
	const cursorAlto = vista.getUint16(3);
	// visible = false llega como 0×0.
	const esperado = 9 + cursorAncho * cursorAlto * 4;
	if (datos.byteLength !== esperado) return;
	if (cursorAncho > 256 || cursorAlto > 256) return;
	const copia = new ArrayBuffer(datos.byteLength);
	new Uint8Array(copia).set(datos);
	publicar({ t: 'cursor', buf: copia });
}

/** Rectángulo RGBA: se pinta en el búfer del lote en curso. */
function onRect(datos: Uint8Array, vista: DataView): void {
	if (datos.byteLength < 9) return;
	const x = vista.getUint16(1);
	const y = vista.getUint16(3);
	const rectAncho = vista.getUint16(5);
	const rectAlto = vista.getUint16(7);
	if (!rectAncho || !rectAlto) return;
	if (x + rectAncho > ancho || y + rectAlto > alto) return;
	if (datos.byteLength !== 9 + rectAncho * rectAlto * 4) return;
	if (!contextoBufer) return;
	const pixeles = new Uint8ClampedArray(
		datos.buffer as ArrayBuffer,
		datos.byteOffset + 9,
		rectAncho * rectAlto * 4
	);
	contextoBufer.putImageData(new ImageData(pixeles, rectAncho, rectAlto), x, y);
}

/** CopyRect de VNC: se copia una zona ya pintada del búfer. */
function onCopia(vista: DataView): void {
	if (!contextoBufer || !bufer || !ancho || !alto) return;
	const origenX = vista.getUint16(1);
	const origenY = vista.getUint16(3);
	const destinoX = vista.getUint16(5);
	const destinoY = vista.getUint16(7);
	const copiaAncho = vista.getUint16(9);
	const copiaAlto = vista.getUint16(11);
	if (!copiaAncho || !copiaAlto) return;
	if (origenX + copiaAncho > ancho || origenY + copiaAlto > alto) return;
	if (destinoX + copiaAncho > ancho || destinoY + copiaAlto > alto) return;
	contextoBufer.drawImage(
		bufer,
		origenX,
		origenY,
		copiaAncho,
		copiaAlto,
		destinoX,
		destinoY,
		copiaAncho,
		copiaAlto
	);
}

/** Fin de lote: se presenta el búfer de una vez. */
function presentar(): void {
	if (imagen === 'h264') return;
	if (!contextoVisible || !bufer) return;
	contextoVisible.drawImage(bufer, 0, 0);
	fotogramas++;
}

// ---------------------------------------------------------------- soporte de H.264

/** Comprueba con WebCodecs que el navegador decodifica AVC de verdad. */
function comprobarSoporteH264(): Promise<boolean> {
	if (
		typeof VideoDecoder === 'undefined' ||
		typeof VideoDecoder.isConfigSupported !== 'function'
	)
		return Promise.resolve(false);
	const comprobacion = Promise.all(
		CODIFICACIONES_AVC.map(async (codecAvc): Promise<boolean> => {
			try {
				const soporte = await VideoDecoder.isConfigSupported({
					codec: codecAvc,
					avc: { format: 'annexb' },
				} as ConfiguracionAvc);
				return soporte.supported === true;
			} catch {
				// Este perfil no.
				return false;
			}
		})
	).then((resultados) => resultados.some(Boolean));
	const plazo = new Promise<boolean>((resolver) => {
		setTimeout(() => resolver(false), ESPERA_SOPORTE_MS);
	});
	return Promise.race([comprobacion, plazo]);
}

/** Espera a saber si hay H.264 real y solo entonces abre el WebSocket. */
async function prepararYConectar(): Promise<void> {
	if (promesaSoporte === null) promesaSoporte = comprobarSoporteH264();
	soporteH264 = await promesaSoporte;
	conectarSocket();
}

/**
 * El decodificador ha fallado después del alta (configure o un fotograma):
 * se reconecta una sola vez pidiendo rectángulos, que es lo que entiende el
 * servidor cuando el hola dice h264=false (reconecta RDP sin AVC420). No se
 * avisa a la página para que no salga el aviso de caído mientras dura.
 */
function falloDecodificador(): void {
	reiniciarDecodificador();
	if (!soporteH264 || reintentoSinH264) return;
	reintentoSinH264 = true;
	cierrePedido = true;
	try {
		ws?.close();
	} catch {
		// Ya estaba cerrado.
	}
	ws = null;
	// Un respiro para que el servidor vea el cierre antes que el alta nueva.
	setTimeout(() => {
		cierrePedido = false;
		conectarSocket();
	}, 200);
}

// ---------------------------------------------------------------- vídeo H.264

interface ConfiguracionAvc extends VideoDecoderConfig {
	/**
	 * Sin descripción (avcC) WebCodecs espera Annex-B, que es justo lo que manda
	 * el servidor. La opción existe en los navegadores aunque TypeScript aún no
	 * la declare.
	 */
	avc?: { format: 'annexb' };
}

function esFotogramaClave(datos: Uint8Array): boolean {
	let i = 0;
	while (i + 2 < datos.length) {
		if (datos[i] !== 0 || datos[i + 1] !== 0) {
			i++;
			continue;
		}
		let cabecera = -1;
		if (datos[i + 2] === 1) cabecera = i + 3;
		else if (i + 3 < datos.length && datos[i + 2] === 0 && datos[i + 3] === 1)
			cabecera = i + 4;
		if (cabecera < 0) {
			i++;
			continue;
		}
		if (cabecera >= datos.length) return false;
		const tipoNal = (datos[cabecera] ?? 0) & 0x1f;
		if (tipoNal === 5 || tipoNal === 7) return true;
		i = cabecera + 1;
	}
	return false;
}

const hex2 = (valor: number): string => valor.toString(16).padStart(2, '0');

/** avc1.PPCCLL a partir del SPS del primer fotograma clave. */
function codecDeSps(datos: Uint8Array): string | null {
	let i = 0;
	while (i + 2 < datos.length) {
		if (datos[i] !== 0 || datos[i + 1] !== 0) {
			i++;
			continue;
		}
		let cabecera = -1;
		if (datos[i + 2] === 1) cabecera = i + 3;
		else if (i + 3 < datos.length && datos[i + 2] === 0 && datos[i + 3] === 1)
			cabecera = i + 4;
		if (cabecera < 0) {
			i++;
			continue;
		}
		if (cabecera + 3 >= datos.length) return null;
		if (((datos[cabecera] ?? 0) & 0x1f) === 7) {
			return (
				'avc1.' +
				hex2(datos[cabecera + 1] ?? 0) +
				hex2(datos[cabecera + 2] ?? 0) +
				hex2(datos[cabecera + 3] ?? 0)
			);
		}
		i = cabecera + 1;
	}
	return null;
}

function crearDecodificador(nuevoCodec: string): boolean {
	if (typeof VideoDecoder === 'undefined') return false;
	let dec: VideoDecoder;
	try {
		dec = new VideoDecoder({
			output: pintarFotograma,
			error: () => falloDecodificador(),
		});
		const configuracion: ConfiguracionAvc = {
			codec: nuevoCodec,
			optimizeForLatency: true,
			hardwareAcceleration: 'no-preference',
			avc: { format: 'annexb' },
		};
		dec.configure(configuracion);
	} catch {
		falloDecodificador();
		return false;
	}
	decodificador = dec;
	codec = nuevoCodec;
	return true;
}

/** Pinta solo las regiones que el servidor dice que han cambiado. */
function pintarFotograma(fotograma: VideoFrame): void {
	const regiones = regionesPorMarca.get(fotograma.timestamp);
	regionesPorMarca.delete(fotograma.timestamp);
	try {
		if (!contextoVisible || !regiones) return;
		for (const region of regiones) {
			const x = Math.max(0, Math.min(region.x, fotograma.displayWidth - 1));
			const y = Math.max(0, Math.min(region.y, fotograma.displayHeight - 1));
			const regionAncho = Math.min(
				region.ancho,
				fotograma.displayWidth - x,
				ancho - x
			);
			const regionAlto = Math.min(
				region.alto,
				fotograma.displayHeight - y,
				alto - y
			);
			if (regionAncho <= 0 || regionAlto <= 0) continue;
			contextoVisible.drawImage(
				fotograma,
				x,
				y,
				regionAncho,
				regionAlto,
				x,
				y,
				regionAncho,
				regionAlto
			);
		}
		fotogramas++;
	} catch {
		// Un fotograma ilegible no puede tumbar al visor.
	} finally {
		fotograma.close();
	}
}

/** [0x07][marco:u32][n:u16] + n×[x,y,w,h] + H.264 Annex-B tal cual. */
function onVideo(datos: Uint8Array, vista: DataView): void {
	if (datos.byteLength < 7) return;
	const numeroRegiones = vista.getUint16(5);
	const inicioVideo = 7 + numeroRegiones * 8;
	if (datos.byteLength <= inicioVideo) return;
	const regiones: Region[] = [];
	for (let i = 0; i < numeroRegiones; i++) {
		const base = 7 + i * 8;
		regiones.push({
			x: vista.getUint16(base),
			y: vista.getUint16(base + 2),
			ancho: vista.getUint16(base + 4),
			alto: vista.getUint16(base + 6),
		});
	}
	const h264 = datos.subarray(inicioVideo);
	imagen = 'h264';
	if (!regiones.length) return;
	if (typeof VideoDecoder === 'undefined') return;

	const esClave = esFotogramaClave(h264);
	if (!decodificador) {
		if (!esClave) {
			esperandoClave = true;
			return;
		}
		const nuevoCodec = codecDeSps(h264) ?? codec;
		if (!nuevoCodec || !crearDecodificador(nuevoCodec)) return;
	} else if (esClave) {
		const nuevoCodec = codecDeSps(h264);
		if (nuevoCodec && nuevoCodec !== codec) {
			reiniciarDecodificador();
			if (!crearDecodificador(nuevoCodec)) return;
		}
	}
	if (esperandoClave && !esClave) return;
	if (!esClave && !codec) return;
	esperandoClave = false;

	const marca = ++contadorMarcas;
	regionesPorMarca.set(marca, regiones);
	const decodificadorActivo = decodificador;
	if (decodificadorActivo === null) return;
	// Si el decodificador va muy retrasado, este fotograma se decodifica igual
	// (para no romper la cadena de referencias) pero no se pinta: es viejo.
	const pintar = decodificadorActivo.decodeQueueSize <= 8;
	if (!pintar) regionesPorMarca.delete(marca);
	try {
		decodificadorActivo.decode(
			new EncodedVideoChunk({
				type: esClave ? 'key' : 'delta',
				timestamp: marca,
				data: h264,
			})
		);
	} catch {
		falloDecodificador();
	}
}

// ---------------------------------------------------------------- mensajes de la página

function onVisible(activo: boolean): void {
	visible = activo;
	if (activo) {
		// Al volver, el servidor manda el escritorio entero: el decodificador
		// arranca de cero para no mezclar fotogramas viejos.
		reiniciarDecodificador();
		enviar(cmd.visible(true));
	} else {
		enviar(cmd.visible(false));
	}
}

function onReconectar(): void {
	if (ws && ws.readyState === WebSocket.OPEN) {
		enviar(cmd.reconectar);
		return;
	}
	cierrePedido = false;
	conectarSocket();
}

function onCerrar(): void {
	cierrePedido = true;
	reiniciarDecodificador();
	cerrarSocket();
	liberarLienzos();
}

self.addEventListener('message', (evento: MessageEvent<HaciaTrabajador>) => {
	const mensaje = evento.data;
	switch (mensaje.t) {
		case 'iniciar':
			lienzo = mensaje.lienzo;
			url = mensaje.url;
			clave = mensaje.clave;
			// Antes del hola hay que saber si H.264 se decodifica de verdad.
			void prepararYConectar();
			return;
		case 'enviar':
			if (typeof mensaje.d === 'string') enviar(mensaje.d);
			return;
		case 'visible':
			onVisible(mensaje.activo === true);
			return;
		case 'reconectar':
			onReconectar();
			return;
		case 'cerrar':
			onCerrar();
			return;
	}
});

setInterval(() => {
	const ahora = performance.now();
	const transcurrido = (ahora - ultimoInforme) / 1000;
	const fps = transcurrido > 0 ? fotogramas / transcurrido : 0;
	fotogramas = 0;
	ultimoInforme = ahora;
	publicar({ t: 'estadisticas', fps });
}, 1000);
