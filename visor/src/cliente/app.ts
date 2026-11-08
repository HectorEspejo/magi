// Lo que comparten los módulos de la página del visor: el trabajador, los
// elementos del DOM y el estado de la sesión.
import type { HaciaTrabajador, MensajeHola } from '../protocolo';

/** Estado del visor que se enseña en la barra y en los avisos. */
export type EstadoVisor =
	| 'conectando'
	| 'conectado'
	| 'caido'
	| 'cerrado'
	| 'otra_ventana';

export interface App {
	trabajador: Worker | null;
	/** Manda un mensaje al trabajador (si ya existe). */
	post(mensaje: HaciaTrabajador, transferir?: Transferable[]): void;
	/** Manda una orden corta al servidor del visor. */
	enviar(orden: string): void;
	escena: HTMLElement;
	superficie: HTMLElement;
	lienzo: HTMLCanvasElement;
	teclado: HTMLTextAreaElement;
	barra: HTMLElement;
	/** Nombre del escritorio (lo dice el canje; hello no lo trae). */
	nombre: string;
	/** Tipo de escritorio: rdp, vnc o voxl. */
	tipo: string;
	/** Datos del hello del servidor; null hasta que llega. */
	hola: MensajeHola | null;
	estado: EstadoVisor;
	/** El visor ha terminado (cerrado o abierto en otra ventana): no reconectar. */
	terminal: boolean;
	portapapeles: boolean;
	redimensionable: boolean;
	/** Píxeles CSS por píxel de escritorio. */
	escala: number;
	escritorio: { ancho: number; alto: number };
	/** Ajusta el lienzo a la ventana sin deformarlo. */
	colocar(): void;
}

export function elemento<T extends HTMLElement = HTMLElement>(id: string): T {
	const encontrado = document.getElementById(id);
	if (!encontrado)
		throw new Error('el visor necesita el elemento #' + id + ' en entrar.html');
	return encontrado as T;
}
