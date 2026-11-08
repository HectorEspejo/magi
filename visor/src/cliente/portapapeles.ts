// Portapapeles de texto en los dos sentidos y hasta 1 MB. Del remoto al
// equipo se escribe con navigator.clipboard; si el navegador lo rechaza (suele
// pasar sin foco), se guarda pendiente y se reintenta al volver el foco o con el
// botón «Copiar» del aviso, que lo hace dentro del gesto del usuario. Del equipo
// al remoto se manda en el evento paste (lo captura teclado.ts).
import { PORTAPAPELES_MAXIMO, cmd } from '../protocolo';
import type { App } from './app';
import { elemento } from './app';

export interface Portapapeles {
	/** El hello del servidor dice si el host tiene portapapeles. */
	configurar(si: boolean): void;
	/** Texto nuevo del remoto: intenta llevarlo al portapapeles del equipo. */
	recibirTexto(texto: string): void;
	/** Texto que el usuario ha pegado aquí: se manda al remoto como "c,texto". */
	enviarLocal(texto: string): void;
	/** Reintenta la escritura pendiente (foco, gesto). */
	flush(): void;
	disponible(): boolean;
}

/** Recorta un texto a 1 MB de UTF-8 sin partir un carácter. */
export function recortarTexto(texto: string): string {
	const codificador = new TextEncoder();
	const bytes = codificador.encode(texto);
	if (bytes.byteLength <= PORTAPAPELES_MAXIMO) return texto;
	const decodificador = new TextDecoder('utf-8', { fatal: false });
	let corte = PORTAPAPELES_MAXIMO;
	let recortado = decodificador.decode(bytes.subarray(0, corte));
	// Si el corte ha partido un carácter, el decodificador deja un U+FFFD al
	// final: se quita recortando bytes hasta que desaparezca.
	while (recortado.endsWith('\uFFFD') && corte > 0) {
		corte--;
		recortado = decodificador.decode(bytes.subarray(0, corte));
	}
	return recortado;
}

export function crearPortapapeles(app: App): Portapapeles {
	let habilitado = false;
	let ultimoRemoto: string | null = null;
	let pendiente: string | null = null;
	const aviso = elemento('portapapeles-aviso');
	const botonCopiar = elemento<HTMLButtonElement>('boton-copiar');

	function mostrarAviso(): void {
		aviso.classList.add('visible');
	}

	function ocultarAviso(): void {
		aviso.classList.remove('visible');
	}

	async function escribir(texto: string): Promise<void> {
		if (
			!navigator.clipboard ||
			typeof navigator.clipboard.writeText !== 'function'
		) {
			pendiente = texto;
			mostrarAviso();
			return;
		}
		try {
			await navigator.clipboard.writeText(texto);
			if (pendiente === texto) pendiente = null;
			ocultarAviso();
		} catch {
			// Sin foco o sin permiso: se reintenta en el siguiente gesto.
			pendiente = texto;
			mostrarAviso();
		}
	}

	function flush(): void {
		if (!habilitado || pendiente === null) return;
		void escribir(pendiente);
	}

	botonCopiar.addEventListener('click', () => {
		const texto = pendiente ?? ultimoRemoto;
		if (texto !== null) void escribir(texto);
	});
	window.addEventListener('focus', flush);

	return {
		disponible: () => habilitado,
		configurar(si: boolean) {
			habilitado = si;
			if (!si) {
				pendiente = null;
				ultimoRemoto = null;
				ocultarAviso();
			}
		},
		recibirTexto(texto: string) {
			if (!habilitado || texto === '') return;
			if (texto === ultimoRemoto) return;
			ultimoRemoto = texto;
			pendiente = texto;
			void escribir(texto);
		},
		enviarLocal(texto: string) {
			if (!habilitado || texto === '') return;
			const recortado = recortarTexto(texto);
			if (recortado === '') return;
			// No se devuelve el eco: lo que el usuario pega ya está en su equipo.
			ultimoRemoto = recortado;
			app.enviar(cmd.portapapeles(recortado));
		},
		flush,
	};
}
