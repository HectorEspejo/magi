// Punto de entrada de la página del visor: canjea el enlace de un solo uso,
// guarda la clave de la sesión, arranca el trabajador (WebSocket y
// OffscreenCanvas) y conecta la entrada y la barra. Es el único módulo que habla
// con /api/canjear.
import { LADO_MAXIMO, cmd } from '../protocolo';
import type { DesdeTrabajador, MensajeEstado } from '../protocolo';
import type { App } from './app';
import { elemento } from './app';
import { crearAvisos } from './avisos';
import { crearBarra } from './barra';
import { crearCursor } from './cursor';
import { crearPortapapeles } from './portapapeles';
import { crearRaton } from './raton';
import { crearTeclado } from './teclado';

const CLAVE_SESION = 'magi.visor.clave';
const CLAVE_NOMBRE = 'magi.visor.escritorio';
const CLAVE_TIPO = 'magi.visor.tipo';
/** Factor máximo de píxeles del dispositivo para pedir resolución. */
const ESCALA_PIXELES = 2;

interface Sesion {
	clave: string;
	nombre: string;
	tipo: string;
}

interface DatosCanje {
	clave?: unknown;
	escritorio?: unknown;
	tipo?: unknown;
	voxl?: unknown;
}

function leerSesion(): Sesion | null {
	try {
		const clave = sessionStorage.getItem(CLAVE_SESION);
		if (clave === null || clave === '') return null;
		return {
			clave,
			nombre: sessionStorage.getItem(CLAVE_NOMBRE) ?? '',
			tipo: sessionStorage.getItem(CLAVE_TIPO) ?? '',
		};
	} catch {
		return null;
	}
}

function guardarSesion(sesion: Sesion): void {
	try {
		sessionStorage.setItem(CLAVE_SESION, sesion.clave);
		sessionStorage.setItem(CLAVE_NOMBRE, sesion.nombre);
		sessionStorage.setItem(CLAVE_TIPO, sesion.tipo);
	} catch {
		// Sin sessionStorage el visor funciona, pero al recargar no reconecta.
	}
}

function borrarSesion(): void {
	try {
		sessionStorage.removeItem(CLAVE_SESION);
		sessionStorage.removeItem(CLAVE_NOMBRE);
		sessionStorage.removeItem(CLAVE_TIPO);
	} catch {
		// Nada que borrar.
	}
}

function textoDe(valor: unknown): string {
	return typeof valor === 'string' ? valor : '';
}

function urlWebsocket(): string {
	const url = new URL('/api/ws', location.href);
	url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
	return url.href;
}

function pantallaCompleta(): void {
	if (document.fullscreenElement !== null) {
		void document.exitFullscreen().catch(() => {});
		return;
	}
	void document.documentElement
		.requestFullscreen({ navigationUI: 'hide' })
		.catch(() => {});
}

function main(): void {
	const app = crearApp();
	const cursor = crearCursor(app);
	const portapapeles = crearPortapapeles(app);
	const avisos = crearAvisos(app, { reconectar });
	const teclado = crearTeclado(app, portapapeles);

	function colocar(): void {
		const { ancho, alto } = app.escritorio;
		if (!ancho || !alto) return;
		if (!window.innerWidth || !window.innerHeight) return;
		const escala = Math.min(
			window.innerWidth / ancho,
			window.innerHeight / alto
		);
		if (!Number.isFinite(escala) || escala <= 0) return;
		app.escala = escala;
		app.superficie.style.width = Math.max(1, Math.round(ancho * escala)) + 'px';
		app.superficie.style.height = Math.max(1, Math.round(alto * escala)) + 'px';
		cursor.recolocar();
	}
	app.colocar = colocar;

	const barra = crearBarra(app, {
		pantallaCompleta,
		ctrlAltSupr,
		teclaWindows,
		cerrarVisor,
	});
	crearRaton(app);

	let temporizadorTamano: number | undefined;

	function pedirTamano(forzar = false): void {
		if (!app.hola || !app.redimensionable) return;
		const factor = Math.min(
			Math.max(window.devicePixelRatio || 1, 1),
			ESCALA_PIXELES
		);
		const ancho = Math.min(LADO_MAXIMO, Math.round(window.innerWidth * factor));
		const alto = Math.min(LADO_MAXIMO, Math.round(window.innerHeight * factor));
		if (ancho < 1 || alto < 1) return;
		if (
			!forzar &&
			Math.abs(ancho - app.escritorio.ancho) <= 2 &&
			Math.abs(alto - app.escritorio.alto) <= 2
		)
			return;
		app.enviar(cmd.tamano(ancho, alto));
	}

	function actualizarVisibilidad(): void {
		if (app.trabajador === null) return;
		const activo =
			!document.hidden && window.innerWidth > 0 && window.innerHeight > 0;
		app.post({ t: 'visible', activo });
		if (!activo) teclado.soltarTodo();
	}

	function ctrlAltSupr(): void {
		if (app.estado === 'conectado') app.enviar(cmd.ctrlAltSupr);
	}

	function teclaWindows(): void {
		if (app.estado === 'conectado') app.enviar(cmd.teclaWindows);
	}

	function cerrarVisor(): void {
		app.terminal = true;
		teclado.soltarTodo();
		app.post({ t: 'cerrar' });
		borrarSesion();
		window.close();
		window.setTimeout(() => {
			if (!window.closed) avisos.visorCerrado();
		}, 250);
	}

	function reconectar(): void {
		if (app.terminal) return;
		app.estado = 'conectando';
		barra.estado('conectando');
		avisos.conectando('reconectando la sesión…');
		app.post({ t: 'reconectar' });
	}

	function onTrabajador(mensaje: DesdeTrabajador): void {
		switch (mensaje.t) {
			case 'abierto':
				if (app.terminal) return;
				barra.estado('conectando');
				return;
			case 'hello':
				app.hola = mensaje;
				app.escritorio = { ancho: mensaje.w, alto: mensaje.h };
				app.portapapeles = mensaje.portapapeles;
				app.redimensionable = mensaje.redimensionable;
				if (app.tipo === '') app.tipo = mensaje.tipo;
				app.estado = 'conectado';
				app.terminal = false;
				barra.identidad(app.nombre, app.tipo);
				barra.estado('conectado');
				avisos.ocultar();
				portapapeles.configurar(mensaje.portapapeles);
				colocar();
				pedirTamano(true);
				app.teclado.focus({ preventScroll: true });
				return;
			case 'tam':
				app.escritorio = { ancho: mensaje.w, alto: mensaje.h };
				colocar();
				return;
			case 'estado':
				onEstado(mensaje, avisos, barra, teclado, app);
				return;
			case 'clip':
				portapapeles.recibirTexto(mensaje.texto);
				return;
			case 'latencia':
				barra.latencia(mensaje.ms);
				return;
			case 'cursor':
				cursor.recibir(mensaje.buf);
				return;
			case 'estadisticas':
				barra.fps(mensaje.fps);
				return;
			case 'caido':
				if (app.terminal) return;
				app.estado = 'caido';
				barra.estado('caido');
				teclado.soltarTodo();
				avisos.caido(mensaje.motivo);
				return;
			case 'cerrado':
				if (app.terminal) return;
				app.terminal = true;
				app.estado = 'cerrado';
				barra.estado('cerrado');
				teclado.soltarTodo();
				portapapeles.configurar(false);
				avisos.cerrado();
				return;
		}
	}

	function arrancarTrabajador(sesion: Sesion): void {
		const trabajador = new Worker('/trabajador.js');
		app.trabajador = trabajador;
		trabajador.addEventListener(
			'message',
			(evento: MessageEvent<DesdeTrabajador>) => onTrabajador(evento.data)
		);
		const lienzo = app.lienzo;
		if (!('transferControlToOffscreen' in lienzo)) {
			app.terminal = true;
			avisos.pantalla(
				'Navegador no compatible',
				'este navegador no tiene OffscreenCanvas; abre el visor con Chromium.'
			);
			return;
		}
		const offscreen = lienzo.transferControlToOffscreen();
		app.post(
			{
				t: 'iniciar',
				lienzo: offscreen,
				url: urlWebsocket(),
				clave: sesion.clave,
			},
			[offscreen]
		);
		actualizarVisibilidad();
		app.teclado.focus({ preventScroll: true });
	}

	async function canjear(enlace: string): Promise<Sesion | null> {
		let respuesta: Response;
		try {
			respuesta = await fetch('/api/canjear', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ enlace }),
				credentials: 'omit',
				cache: 'no-store',
			});
		} catch {
			avisos.errorCanje(
				'No se puede abrir el visor',
				'no se ha podido hablar con el servidor del visor: comprueba que MAGI sigue abierto y ábrelo otra vez desde MAGI.'
			);
			return null;
		}
		if (respuesta.status === 429) {
			avisos.errorCanje(
				'Demasiados intentos',
				'ha habido demasiados intentos de canje en el último minuto; espera un minuto y ábrelo otra vez desde MAGI.'
			);
			return null;
		}
		if (!respuesta.ok) {
			avisos.enlaceUsado();
			return null;
		}
		let datos: DatosCanje;
		try {
			datos = (await respuesta.json()) as DatosCanje;
		} catch {
			avisos.enlaceUsado();
			return null;
		}
		if (typeof datos.voxl === 'string' && datos.voxl !== '') {
			let url: URL;
			try {
				url = new URL(datos.voxl);
			} catch {
				avisos.enlaceUsado();
				return null;
			}
			const local =
				url.hostname === '127.0.0.1' || url.hostname === 'localhost';
			if ((url.protocol !== 'http:' && url.protocol !== 'https:') || !local) {
				avisos.errorCanje(
					'No se puede abrir el visor',
					'el servidor del visor ha devuelto una dirección inesperada.'
				);
				return null;
			}
			location.replace(url.href);
			return null;
		}
		const clave = textoDe(datos.clave);
		if (clave === '') {
			avisos.enlaceUsado();
			return null;
		}
		const sesion: Sesion = {
			clave,
			nombre: textoDe(datos.escritorio),
			tipo: textoDe(datos.tipo),
		};
		guardarSesion(sesion);
		try {
			history.replaceState(null, '', '/');
		} catch {
			// Si el navegador no deja limpiar la URL, la clave ya está guardada.
		}
		return sesion;
	}

	async function iniciar(): Promise<void> {
		if (!window.isSecureContext) {
			app.terminal = true;
			avisos.pantalla(
				'Navegador no compatible',
				'el visor necesita abrirse en localhost: los navegadores solo permiten vídeo y portapapeles en páginas seguras.'
			);
			return;
		}
		const parametros = new URLSearchParams(location.search);
		const enlace = parametros.get('t');
		let sesion: Sesion | null;
		if (enlace !== null && enlace !== '') {
			sesion = await canjear(enlace);
		} else {
			sesion = leerSesion();
			if (sesion === null) avisos.enlaceUsado();
		}
		if (sesion === null) return;
		app.nombre = sesion.nombre;
		app.tipo = sesion.tipo;
		barra.identidad(app.nombre, app.tipo);
		arrancarTrabajador(sesion);
	}

	// ---- eventos de ventana y del navegador ----

	window.addEventListener('resize', () => {
		colocar();
		window.clearTimeout(temporizadorTamano);
		temporizadorTamano = window.setTimeout(() => pedirTamano(), 500);
	});
	document.addEventListener('visibilitychange', actualizarVisibilidad);
	document.addEventListener('fullscreenchange', () => {
		const activa = document.fullscreenElement !== null;
		barra.pantallaCompletaActiva(activa);
		if (activa) {
			if (app.estado === 'conectado') app.enviar(cmd.pantallaCompleta);
			void navigator.keyboard?.lock().catch(() => {});
		} else {
			navigator.keyboard?.unlock();
		}
		colocar();
	});
	window.addEventListener('beforeunload', () => {
		// Al descargar la página el WebSocket se cierra solo y el servidor cuenta
		// la gracia; solo se sueltan las teclas para no dejarlas pulsadas.
		teclado.soltarTodo();
	});

	void iniciar();
}

function onEstado(
	mensaje: MensajeEstado,
	avisos: ReturnType<typeof crearAvisos>,
	barra: ReturnType<typeof crearBarra>,
	teclado: ReturnType<typeof crearTeclado>,
	app: App
): void {
	switch (mensaje.estado) {
		case 'conectando':
			app.estado = 'conectando';
			barra.estado('conectando');
			avisos.conectando();
			return;
		case 'caido':
			if (app.terminal) return;
			app.estado = 'caido';
			barra.estado('caido');
			teclado.soltarTodo();
			avisos.caido(mensaje.motivo);
			return;
		case 'cerrado':
			if (app.terminal) return;
			app.terminal = true;
			app.estado = 'cerrado';
			barra.estado('cerrado');
			teclado.soltarTodo();
			avisos.cerrado(mensaje.motivo);
			return;
		case 'otra_ventana':
			if (app.terminal) return;
			app.terminal = true;
			app.estado = 'otra_ventana';
			barra.estado('otra_ventana');
			teclado.soltarTodo();
			avisos.otraVentana();
			return;
	}
}

function crearApp(): App {
	const app: App = {
		trabajador: null,
		post(mensaje, transferir = []) {
			app.trabajador?.postMessage(mensaje, transferir);
		},
		enviar(orden) {
			app.post({ t: 'enviar', d: orden });
		},
		escena: elemento('escena'),
		superficie: elemento('superficie'),
		lienzo: elemento<HTMLCanvasElement>('lienzo'),
		teclado: elemento<HTMLTextAreaElement>('teclado'),
		barra: elemento('barra'),
		nombre: '',
		tipo: '',
		hola: null,
		estado: 'conectando',
		terminal: false,
		portapapeles: false,
		redimensionable: false,
		escala: 1,
		escritorio: { ancho: 0, alto: 0 },
		colocar() {},
	};
	return app;
}

main();
