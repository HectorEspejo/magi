// Barra superior del visor: se oculta a los 2 s y vuelve al acercar el ratón al
// borde superior. Lleva el nombre y el tipo del escritorio, el estado, los fps y
// la latencia, y los botones de pantalla completa, Ctrl+Alt+Supr, tecla Windows
// y cerrar visor.
import type { App, EstadoVisor } from './app';
import { elemento } from './app';

const OCULTAR_MS = 2000;
const REVELAR_MS = 4000;
const BORDE_SUPERIOR = 64;

export interface AccionesBarra {
	pantallaCompleta(): void;
	ctrlAltSupr(): void;
	teclaWindows(): void;
	cerrarVisor(): void;
}

export interface Barra {
	mostrar(ms?: number): void;
	ocultar(): void;
	identidad(nombre: string, tipo: string): void;
	estado(estado: EstadoVisor): void;
	fps(fps: number): void;
	latencia(ms: number | null): void;
	pantallaCompletaActiva(si: boolean): void;
}

const ETIQUETAS_ESTADO: Record<EstadoVisor, string> = {
	conectando: 'conectando',
	conectado: 'conectado',
	caido: 'caído',
	cerrado: 'cerrado',
	otra_ventana: 'otra ventana',
};

export function crearBarra(app: App, acciones: AccionesBarra): Barra {
	const barraEl = app.barra;
	const titulo = elemento('barra-titulo');
	const tipoEl = elemento('barra-tipo');
	const conexion = elemento('barra-conexion');
	const fpsEl = elemento('barra-fps');
	const latenciaEl = elemento('barra-latencia');
	const botonPantalla = elemento<HTMLButtonElement>('boton-pantalla-completa');
	const botonCtrlAltSupr = elemento<HTMLButtonElement>('boton-ctrl-alt-supr');
	const botonWin = elemento<HTMLButtonElement>('boton-win');
	const botonCerrar = elemento<HTMLButtonElement>('boton-cerrar');
	let temporizador: number | undefined;

	function mostrar(ms = OCULTAR_MS): void {
		window.clearTimeout(temporizador);
		barraEl.classList.remove('oculta');
		if (ms > 0) temporizador = window.setTimeout(ocultar, ms);
	}

	function ocultar(): void {
		window.clearTimeout(temporizador);
		if (
			document.activeElement !== null &&
			barraEl.contains(document.activeElement)
		)
			return;
		barraEl.classList.add('oculta');
	}

	// Ratón: la barra vuelve al acercarse al borde superior.
	document.addEventListener('pointermove', (evento: PointerEvent) => {
		if (evento.pointerType === 'mouse' && evento.clientY <= BORDE_SUPERIOR)
			mostrar();
	});
	// Teclado: F10 la enseña y lleva el foco al primer botón, para que también
	// se pueda usar sin ratón. Es la única tecla que no llega al remoto.
	document.addEventListener('keydown', (evento: KeyboardEvent) => {
		if (evento.key !== 'F10' || evento.repeat) return;
		evento.preventDefault();
		// F10 no llega al remoto: es la tecla de la barra.
		evento.stopPropagation();
		mostrar(REVELAR_MS);
		botonPantalla.focus({ preventScroll: true });
	});

	barraEl.addEventListener('focusin', () => window.clearTimeout(temporizador));
	barraEl.addEventListener('focusout', () => {
		window.clearTimeout(temporizador);
		temporizador = window.setTimeout(ocultar, OCULTAR_MS);
	});

	function boton(elementoBoton: HTMLButtonElement, accion: () => void): void {
		elementoBoton.addEventListener('click', (evento: MouseEvent) => {
			accion();
			// Tras un clic con ratón, el teclado vuelve al remoto; si se activó
			// con Enter o Espacio, el foco se queda en la barra.
			if (evento.detail > 0) app.teclado.focus({ preventScroll: true });
			mostrar();
		});
	}

	boton(botonPantalla, acciones.pantallaCompleta);
	boton(botonCtrlAltSupr, acciones.ctrlAltSupr);
	boton(botonWin, acciones.teclaWindows);
	boton(botonCerrar, acciones.cerrarVisor);

	function identidad(nombre: string, tipo: string): void {
		const nombreLimpio = nombre.trim();
		titulo.textContent =
			nombreLimpio === '' ? 'Escritorio remoto' : nombreLimpio;
		const tipoLimpio = tipo.trim().toUpperCase();
		tipoEl.textContent = tipoLimpio;
		tipoEl.hidden = tipoLimpio === '';
		// La tecla Windows solo tiene sentido en RDP: en VNC no hay tecla
		// Windows que enviar (checklist: «Ctrl+Alt+Supr, tecla Windows (RDP)»).
		botonWin.hidden = tipo.trim().toLowerCase() !== 'rdp';
		document.title =
			nombreLimpio === ''
				? 'MAGI · escritorio remoto'
				: nombreLimpio + ' · MAGI · escritorio remoto';
	}

	function estado(nuevo: EstadoVisor): void {
		conexion.textContent = ETIQUETAS_ESTADO[nuevo];
		conexion.classList.remove('conectado', 'caido', 'cerrado');
		if (nuevo === 'conectado') conexion.classList.add('conectado');
		if (nuevo === 'caido' || nuevo === 'cerrado') conexion.classList.add(nuevo);
	}

	function fps(valor: number): void {
		const redondeado = Math.max(0, Math.round(valor));
		fpsEl.textContent = redondeado + ' fps';
	}

	function latencia(ms: number | null): void {
		if (ms === null || !Number.isFinite(ms)) {
			latenciaEl.hidden = true;
			latenciaEl.textContent = '— ms';
			return;
		}
		latenciaEl.hidden = false;
		latenciaEl.textContent = Math.max(0, Math.round(ms)) + ' ms';
	}

	function pantallaCompletaActiva(si: boolean): void {
		botonPantalla.setAttribute('aria-pressed', si ? 'true' : 'false');
	}

	// La barra empieza visible y se esconde a los 2 s.
	mostrar();
	estado('conectando');
	latencia(null);
	pantallaCompletaActiva(false);

	return {
		mostrar,
		ocultar,
		identidad,
		estado,
		fps,
		latencia,
		pantallaCompletaActiva,
	};
}
