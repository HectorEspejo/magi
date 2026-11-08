// Avisos sobre la imagen: conectando, caído (con Reconectar), cerrado, abierto
// en otra ventana y enlace usado o caducado.
import type { App } from './app';
import { elemento } from './app';

/** Texto exacto del aviso de enlace gastado (checklist de la Fase 12). */
export const ENLACE_USADO =
	'enlace usado o caducado: ábrelo otra vez desde MAGI';

export interface AccionesAvisos {
	reconectar(): void;
}

export interface Avisos {
	pantalla(titulo: string, detalle: string | null, reconectar?: boolean): void;
	conectando(detalle?: string): void;
	caido(motivo?: string): void;
	cerrado(motivo?: string): void;
	otraVentana(): void;
	enlaceUsado(): void;
	errorCanje(titulo: string, detalle: string): void;
	visorCerrado(): void;
	ocultar(): void;
}

export function crearAvisos(app: App, acciones: AccionesAvisos): Avisos {
	const caja = elemento('aviso');
	const titulo = elemento('aviso-titulo');
	const detalle = elemento('aviso-detalle');
	const botonReconectar = elemento<HTMLButtonElement>('boton-reconectar');

	botonReconectar.addEventListener('click', () => {
		botonReconectar.hidden = true;
		acciones.reconectar();
	});

	function pantalla(
		textoTitulo: string,
		textoDetalle: string | null,
		reconectar = false
	): void {
		titulo.textContent = textoTitulo;
		if (textoDetalle === null || textoDetalle === '') {
			detalle.hidden = true;
			detalle.textContent = '';
		} else {
			detalle.hidden = false;
			detalle.textContent = textoDetalle;
		}
		botonReconectar.hidden = !reconectar;
		caja.classList.add('visible');
		if (reconectar) botonReconectar.focus({ preventScroll: true });
	}

	function ocultar(): void {
		const teniaFoco =
			document.activeElement !== null && caja.contains(document.activeElement);
		caja.classList.remove('visible');
		botonReconectar.hidden = true;
		detalle.hidden = true;
		detalle.textContent = '';
		if (teniaFoco) app.teclado.focus({ preventScroll: true });
	}

	return {
		pantalla,
		conectando(textoDetalle) {
			pantalla('Conectando…', textoDetalle ?? null);
		},
		caido(motivo) {
			pantalla(
				'Conexión caída',
				motivo && motivo !== ''
					? motivo
					: 'el escritorio remoto ha perdido la conexión.',
				true
			);
		},
		cerrado(motivo) {
			pantalla(
				'Sesión cerrada',
				motivo && motivo !== '' ? motivo : 'el escritorio remoto se ha cerrado.'
			);
		},
		otraVentana() {
			pantalla(
				'Abierto en otra ventana',
				'el visor de este escritorio se ha abierto en otra ventana.'
			);
		},
		enlaceUsado() {
			pantalla('No se puede abrir el visor', ENLACE_USADO);
		},
		errorCanje(textoTitulo, textoDetalle) {
			pantalla(textoTitulo, textoDetalle);
		},
		visorCerrado() {
			pantalla(
				'Visor cerrado',
				'puedes cerrar esta pestaña. La sesión se cerrará sola si no hay otro visor abierto.'
			);
		},
		ocultar,
	};
}
