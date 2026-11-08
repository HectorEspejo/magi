// Ratón y rueda sobre la superficie del escritorio. Las posiciones van en
// píxeles del escritorio remoto; la rueda se manda como desplazamiento suave en
// píxeles del escritorio, agrupada por fotograma.
import { cmd } from '../protocolo';
import type { App } from './app';

/** Botón del DOM a botón de X11: izquierdo, medio, derecho, atrás, adelante. */
const BOTONES = [1, 2, 3, 8, 9];

function botonDe(evento: PointerEvent): number {
	if (evento.pointerType === 'touch') return 1;
	return BOTONES[evento.button] ?? 1;
}

export interface Raton {
	soltarBotones(): void;
}

export function crearRaton(app: App): Raton {
	const { superficie } = app;
	let ultimaPosicion = '';
	let botonPulsado: number | null = null;
	let ruedaX = 0;
	let ruedaY = 0;
	let ruedaPendiente = false;

	function aEscritorio(punto: {
		clientX: number;
		clientY: number;
	}): [number, number] {
		const caja = superficie.getBoundingClientRect();
		if (!caja.width || !caja.height || !app.escritorio.ancho) return [0, 0];
		const x = Math.round(
			((punto.clientX - caja.left) * app.escritorio.ancho) / caja.width
		);
		const y = Math.round(
			((punto.clientY - caja.top) * app.escritorio.alto) / caja.height
		);
		return [
			Math.max(0, Math.min(app.escritorio.ancho - 1, x)),
			Math.max(0, Math.min(app.escritorio.alto - 1, y)),
		];
	}

	function mover(
		punto: { clientX: number; clientY: number },
		forzar = false
	): void {
		if (!app.hola || app.estado !== 'conectado') return;
		const [x, y] = aEscritorio(punto);
		const orden = cmd.raton(x, y);
		if (orden !== ultimaPosicion || forzar) {
			ultimaPosicion = orden;
			app.enviar(orden);
		}
	}

	function pulsar(boton: number, abajo: boolean): void {
		if (app.estado !== 'conectado') return;
		app.enviar(cmd.boton(boton, abajo));
	}

	function enviarRueda(): void {
		ruedaPendiente = false;
		if (Math.abs(ruedaX) < 0.5 && Math.abs(ruedaY) < 0.5) {
			ruedaX = 0;
			ruedaY = 0;
			return;
		}
		if (app.estado === 'conectado') app.enviar(cmd.rueda(ruedaX, ruedaY));
		ruedaX = 0;
		ruedaY = 0;
	}

	/** Píxeles de escritorio por píxel de pantalla. */
	function escritorioPorPixel(): number {
		const caja = superficie.getBoundingClientRect();
		return caja.width ? app.escritorio.ancho / caja.width : 1;
	}

	superficie.addEventListener('pointermove', (evento: PointerEvent) => {
		mover(evento);
	});
	superficie.addEventListener('pointerdown', (evento: PointerEvent) => {
		evento.preventDefault();
		app.teclado.focus({ preventScroll: true });
		if (!app.hola || app.estado !== 'conectado') return;
		mover(evento);
		try {
			superficie.setPointerCapture(evento.pointerId);
		} catch {
			// Sin captura: los eventos siguen llegando mientras el puntero esté dentro.
		}
		botonPulsado = botonDe(evento);
		pulsar(botonPulsado, true);
	});
	superficie.addEventListener('pointerup', (evento: PointerEvent) => {
		evento.preventDefault();
		if (app.hola) mover(evento);
		const boton = botonDe(evento);
		if (botonPulsado !== null && boton === botonPulsado) {
			pulsar(boton, false);
			botonPulsado = null;
		}
	});
	superficie.addEventListener('pointercancel', () => {
		soltarBotones();
		if (app.estado === 'conectado') app.enviar(cmd.soltarTodo);
	});
	superficie.addEventListener('contextmenu', (evento: MouseEvent) => {
		evento.preventDefault();
	});
	superficie.addEventListener(
		'wheel',
		(evento: WheelEvent) => {
			evento.preventDefault();
			if (!app.hola || app.estado !== 'conectado') return;
			mover(evento);
			// Unidades del evento a píxeles de pantalla (línea: 40, página: 800)
			// y de ahí a píxeles del escritorio.
			const factor =
				(evento.deltaMode === 1 ? 40 : evento.deltaMode === 2 ? 800 : 1) *
				escritorioPorPixel();
			ruedaX += evento.deltaX * factor;
			ruedaY += evento.deltaY * factor;
			if (ruedaPendiente) return;
			ruedaPendiente = true;
			requestAnimationFrame(enviarRueda);
		},
		{ passive: false }
	);

	function soltarBotones(): void {
		if (botonPulsado !== null) {
			pulsar(botonPulsado, false);
			botonPulsado = null;
		}
	}

	return { soltarBotones };
}
