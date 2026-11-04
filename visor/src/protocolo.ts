// Protocolo del visor de escritorio remoto (informe de la Fase 12, §5.2).
//
// Un solo WebSocket: los mensajes del servidor pueden ser binarios (un byte de
// tipo y enteros big-endian) o texto JSON; el visor contesta con órdenes cortas
// separadas por comas.

/** Tipo de los mensajes binarios que manda el servidor. */
export const Bin = {
	/** Cursor remoto: w, h, punto caliente y RGBA. */
	Cursor: 0x03,
	/** Rectángulo RGBA pintado en bruto. */
	Rect: 0x05,
	/** CopyRect: copiar una zona ya pintada. */
	Copia: 0x06,
	/** Vídeo H.264 Annex-B tal cual (AVC420 de RDP). */
	VideoRdp: 0x07,
	/** Fin de lote: el visor presenta lo pintado de una vez. */
	Marco: 0x08,
} as const;

export type TipoEscritorio = 'rdp' | 'vnc' | 'voxl';
export type TipoImagen = 'h264' | 'rectangulos' | 'voxl';
export type EstadoConexion =
	| 'conectando'
	| 'caido'
	| 'cerrado'
	| 'otra_ventana';

/** Texto JSON del servidor: hello. */
export interface MensajeHola {
	t: 'hello';
	w: number;
	h: number;
	tipo: TipoEscritorio;
	imagen: TipoImagen;
	redimensionable: boolean;
	portapapeles: boolean;
}

/** Texto JSON del servidor: estado de la sesión. */
export interface MensajeEstado {
	t: 'estado';
	estado: EstadoConexion;
	motivo?: string;
}

/** Texto JSON del servidor: el escritorio cambia de tamaño. */
export interface MensajeTamano {
	t: 'tam';
	w: number;
	h: number;
}

/** Texto JSON del servidor: el remoto tiene texto nuevo en el portapapeles. */
export interface MensajePortapapeles {
	t: 'clip';
	texto: string;
}

/** Texto JSON del servidor: sonda de latencia. */
export interface MensajePing {
	t: 'ping';
	s: number;
}

/**
 * Texto JSON del servidor: latencia medida, en milisegundos. Llega tras
 * contestar el ping. Es una desviación anotada: §5.2 no lo lista, pero la barra
 * del visor necesita una latencia real que el navegador no puede medir sola.
 */
export interface MensajeLatencia {
	t: 'latencia';
	ms: number;
}

/** Texto JSON del servidor: la sesión se ha cerrado. */
export interface MensajeCerrar {
	t: 'close';
}

/** Texto JSON del servidor. */
export type MensajeServidor =
	| MensajeHola
	| MensajeEstado
	| MensajeTamano
	| MensajePortapapeles
	| MensajePing
	| MensajeLatencia
	| MensajeCerrar;

/** Región de un marco de vídeo, en píxeles del escritorio. */
export interface Region {
	x: number;
	y: number;
	ancho: number;
	alto: number;
}

/**
 * Órdenes del visor al servidor (§5.2). El código de tecla es el
 * `KeyboardEvent.code` del navegador (vacío si no lo hay) y el keysym, el de
 * X11: el carácter Unicode si es ≤ 0xFF y 0x01000000 + punto de código si no.
 */
export const cmd = {
	tecla: (keysym: number, codigo: string, abajo: boolean) =>
		`kc,${keysym},${codigo},${abajo ? 1 : 0}`,
	raton: (x: number, y: number) => `m,${x},${y}`,
	boton: (boton: number, abajo: boolean) => `b,${boton},${abajo ? 1 : 0}`,
	rueda: (dx: number, dy: number) => `sp,${dx.toFixed(1)},${dy.toFixed(1)}`,
	soltarTodo: 'r',
	portapapeles: (texto: string) => `c,${texto}`,
	tamano: (ancho: number, alto: number) => `sz,${ancho},${alto}`,
	visible: (si: boolean) => `v,${si ? 1 : 0}`,
	pantallaCompleta: 'kf',
	ctrlAltSupr: 'cad',
	teclaWindows: 'win',
	pong: (s: number) => `po,${s}`,
	reconectar: 'rec',
} as const;

// ---- página <-> trabajador ----

/** Mensajes de la página al trabajador. */
export type HaciaTrabajador =
	| {
			t: 'iniciar';
			lienzo: OffscreenCanvas;
			url: string;
			clave: string;
	  }
	| { t: 'enviar'; d: string }
	| { t: 'visible'; activo: boolean }
	| { t: 'reconectar' }
	| { t: 'cerrar' };

/** Mensajes del trabajador a la página. */
export type DesdeTrabajador =
	| { t: 'abierto' }
	| { t: 'caido'; motivo: string }
	| { t: 'cerrado' }
	| { t: 'cursor'; buf: ArrayBuffer }
	| { t: 'estadisticas'; fps: number }
	| MensajeHola
	| MensajeEstado
	| MensajeTamano
	| MensajePortapapeles
	| MensajeLatencia;

/** Ancho y alto máximos que se aceptan del servidor (entrada no fiable). */
export const LADO_MAXIMO = 8192;

/** Límite del texto de portapapeles, en bytes UTF-8 (1 MB). */
export const PORTAPAPELES_MAXIMO = 1024 * 1024;
