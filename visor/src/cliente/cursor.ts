// Cursor remoto: el servidor manda su imagen RGBA y el visor la pinta con CSS,
// así que el puntero no tiene la latencia del vídeo. si el remoto lo oculta
// (w = h = 0), se oculta el cursor local.
import type { App } from './app';

const MINIMO_CSS = 16;
const MAXIMO_CSS = 128;
const TAMANO_CURSOR_MAXIMO = 256;

export interface Cursor {
	recibir(mensaje: ArrayBuffer): void;
	/** El tamaño del escritorio en pantalla ha cambiado: se reescala. */
	recolocar(): void;
}

export function crearCursor(app: App): Cursor {
	let mensaje: ArrayBuffer | null = null;
	const escalado = document.createElement('canvas');

	function aplicar(): void {
		if (mensaje === null) return;
		const vista = new DataView(mensaje);
		const ancho = vista.getUint16(1);
		const alto = vista.getUint16(3);
		const puntoX = vista.getUint16(5);
		const puntoY = vista.getUint16(7);
		if (!ancho || !alto) {
			app.superficie.style.cursor = 'none';
			return;
		}
		if (
			ancho > TAMANO_CURSOR_MAXIMO ||
			alto > TAMANO_CURSOR_MAXIMO ||
			mensaje.byteLength !== 9 + ancho * alto * 4
		)
			return;
		const origen = document.createElement('canvas');
		origen.width = ancho;
		origen.height = alto;
		const contextoOrigen = origen.getContext('2d');
		if (!contextoOrigen) return;
		const pixeles = new Uint8ClampedArray(mensaje, 9, ancho * alto * 4);
		contextoOrigen.putImageData(new ImageData(pixeles, ancho, alto), 0, 0);

		// El cursor está en píxeles del escritorio: se escala con la vista, pero
		// ni por debajo de un puntero normal ni por encima del límite del
		// navegador. Se pinta a la resolución del dispositivo para que no salga
		// borroso.
		const dpr = window.devicePixelRatio || 1;
		const mayor = Math.max(ancho, alto);
		const escalaVista = app.escala > 0 ? app.escala : 1;
		const escala = Math.min(
			Math.max(escalaVista, MINIMO_CSS / mayor),
			MAXIMO_CSS / mayor
		);
		const anchoCss = Math.max(1, Math.round(ancho * escala));
		const altoCss = Math.max(1, Math.round(alto * escala));
		const anchoDispositivo = Math.max(1, Math.round(anchoCss * dpr));
		const altoDispositivo = Math.max(1, Math.round(altoCss * dpr));
		escalado.width = anchoDispositivo;
		escalado.height = altoDispositivo;
		const contexto = escalado.getContext('2d');
		if (!contexto) return;
		contexto.imageSmoothingQuality = 'high';
		contexto.clearRect(0, 0, anchoDispositivo, altoDispositivo);
		contexto.drawImage(origen, 0, 0, anchoDispositivo, altoDispositivo);
		const url = escalado.toDataURL('image/png');
		const puntoCssX = Math.round(puntoX * escala);
		const puntoCssY = Math.round(puntoY * escala);
		app.superficie.style.cursor =
			'url("' + url + '") ' + puntoCssX + ' ' + puntoCssY + ', default';
	}

	return {
		recibir(nuevo: ArrayBuffer) {
			mensaje = nuevo;
			aplicar();
		},
		recolocar: aplicar,
	};
}
