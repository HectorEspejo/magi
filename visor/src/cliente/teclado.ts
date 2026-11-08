// Teclado del visor: traduce cada evento a la orden "kc,<keysym>,<código>,<1|0>"
// con el código físico de la tecla SIEMPRE, aunque el keysym sea 0 (teclas
// muertas e IME). Al perder el foco suelta todas las teclas con "r" para no
// dejar ningún modificador pulsado en el remoto. El evento paste del navegador
// es la vía del portapapeles del equipo al remoto.
import { cmd } from '../protocolo';
import { keysymDeCaracter, keysymDeEvento } from '../teclas';
import type { App } from './app';
import type { Portapapeles } from './portapapeles';

const XK_BACKSPACE = 0xff08;
const XK_RETURN = 0xff0d;
const XK_DELETE = 0xffff;

export interface Teclado {
	soltarTodo(): void;
}

export function crearTeclado(app: App, portapapeles: Portapapeles): Teclado {
	const pulsadas = new Map<string, number>();
	let pegadoPendiente: {
		codigo: string;
		keysym: number;
		temporizador: number;
	} | null = null;
	const esMac = /Mac|iPhone|iPad/.test(
		navigator.platform || navigator.userAgent
	);

	function esDeLaBarra(objetivo: EventTarget | null): boolean {
		return objetivo instanceof Node && app.barra.contains(objetivo);
	}

	function escribir(keysym: number, codigo: string, abajo: boolean): void {
		if (app.estado !== 'conectado') return;
		app.enviar(cmd.tecla(keysym, codigo, abajo));
	}

	function volcarPegado(): void {
		if (pegadoPendiente === null) return;
		const pendiente = pegadoPendiente;
		pegadoPendiente = null;
		window.clearTimeout(pendiente.temporizador);
		pulsadas.set(pendiente.codigo, pendiente.keysym);
		escribir(pendiente.keysym, pendiente.codigo, true);
	}

	function soltarTodo(): void {
		if (pegadoPendiente !== null) {
			window.clearTimeout(pegadoPendiente.temporizador);
			pegadoPendiente = null;
		}
		pulsadas.clear();
		// Se manda siempre, aunque no haya teclas apuntadas: el servidor no
		// sabe qué ha llegado a pulsarse si el foco se pierde a destiempo.
		if (app.estado === 'conectado') app.enviar(cmd.soltarTodo);
	}

	function onKeyDown(evento: KeyboardEvent): void {
		// Con el foco en la barra, el teclado funciona con normalidad (Tab,
		// Enter, Espacio): así se puede manejar el visor sin ratón.
		if (esDeLaBarra(evento.target)) return;
		if (evento.isComposing && evento.keyCode === 229) return;
		const codigo = evento.code;
		const keysym = keysymDeEvento(evento);
		const acelerador = esMac ? evento.metaKey : evento.ctrlKey;
		if (
			acelerador &&
			!evento.altKey &&
			(evento.key === 'v' || evento.key === 'V') &&
			!evento.repeat
		) {
			// Primero tiene que llegar el texto pegado al portapapeles del
			// remoto; el atajo se manda un poco después.
			pegadoPendiente = {
				codigo,
				keysym,
				temporizador: window.setTimeout(volcarPegado, 150),
			};
			return;
		}
		// Las teclas muertas (keysym 0) se dejan al navegador para que componga
		// el carácter; el carácter compuesto se manda luego como keysym Unicode.
		if (keysym !== 0) evento.preventDefault();
		if (!codigo) return;
		if (evento.repeat) {
			escribir(keysym, codigo, true);
			return;
		}
		pulsadas.set(codigo, keysym);
		escribir(keysym, codigo, true);
	}

	function onKeyUp(evento: KeyboardEvent): void {
		if (esDeLaBarra(evento.target) && !pulsadas.has(evento.code)) return;
		if (pegadoPendiente !== null && pegadoPendiente.codigo === evento.code)
			volcarPegado();
		const keysym = pulsadas.get(evento.code);
		if (keysym === undefined) return;
		pulsadas.delete(evento.code);
		evento.preventDefault();
		escribir(keysym, evento.code, false);
	}

	function onPaste(evento: ClipboardEvent): void {
		evento.preventDefault();
		const texto = evento.clipboardData?.getData('text/plain') ?? '';
		if (texto !== '') portapapeles.enviarLocal(texto);
		volcarPegado();
	}

	/** Texto que llega por IME o teclado en pantalla: se escribe como keysyms. */
	function escribirTexto(texto: string): void {
		for (const caracter of texto) {
			if (caracter === '\n') {
				escribir(XK_RETURN, '', true);
				escribir(XK_RETURN, '', false);
				continue;
			}
			const punto = caracter.codePointAt(0);
			if (punto === undefined) continue;
			const keysym = keysymDeCaracter(punto);
			if (keysym === 0) continue;
			escribir(keysym, '', true);
			escribir(keysym, '', false);
		}
	}

	function tap(keysym: number): void {
		escribir(keysym, '', true);
		escribir(keysym, '', false);
	}

	window.addEventListener('keydown', onKeyDown);
	window.addEventListener('keyup', onKeyUp);
	window.addEventListener('blur', soltarTodo);
	document.addEventListener('paste', onPaste, true);

	app.teclado.addEventListener('compositionend', (evento: CompositionEvent) => {
		if (evento.data) escribirTexto(evento.data);
		app.teclado.value = '';
	});
	app.teclado.addEventListener('input', (evento: Event) => {
		const entrada = evento as InputEvent;
		if (entrada.isComposing) return;
		if (entrada.inputType === 'insertText' && entrada.data)
			escribirTexto(entrada.data);
		else if (entrada.inputType === 'deleteContentBackward') tap(XK_BACKSPACE);
		else if (entrada.inputType === 'deleteContentForward') tap(XK_DELETE);
		else if (
			entrada.inputType === 'insertLineBreak' ||
			entrada.inputType === 'insertParagraph'
		)
			tap(XK_RETURN);
		app.teclado.value = '';
	});

	app.superficie.addEventListener('focus', () => {
		app.teclado.focus({ preventScroll: true });
	});

	return { soltarTodo };
}
