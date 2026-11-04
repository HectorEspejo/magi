// Tabla de teclas del visor: código físico del navegador (KeyboardEvent.code)
// a keysym de X11. Es espejo de la tabla única de MAGI
// (crates/magi-nucleo/src/escritorio/teclas.rs): para los caracteres se manda
// el keysym Unicode y para el resto, el keysym X11 de la tecla. El visor manda
// siempre el código físico, además del keysym, en la orden kc.

/** Keysym de las teclas que no son caracteres. */
const KEYSYMS: Record<string, number> = {
	Backspace: 0xff08,
	Tab: 0xff09,
	Enter: 0xff0d,
	NumpadEnter: 0xff0d,
	Escape: 0xff1b,
	Delete: 0xffff,
	Insert: 0xff63,
	Home: 0xff50,
	End: 0xff57,
	PageUp: 0xff55,
	PageDown: 0xff56,
	ArrowLeft: 0xff51,
	ArrowUp: 0xff52,
	ArrowRight: 0xff53,
	ArrowDown: 0xff54,
	ShiftLeft: 0xffe1,
	ShiftRight: 0xffe2,
	ControlLeft: 0xffe3,
	ControlRight: 0xffe4,
	AltLeft: 0xffe9,
	AltRight: 0xffea,
	MetaLeft: 0xffeb,
	MetaRight: 0xffec,
	CapsLock: 0xffe5,
	NumLock: 0xff7f,
	ScrollLock: 0xff14,
	PrintScreen: 0xff61,
	Pause: 0xff13,
	ContextMenu: 0xff67,
	Space: 0x20,
	F1: 0xffbe,
	F2: 0xffbf,
	F3: 0xffc0,
	F4: 0xffc1,
	F5: 0xffc2,
	F6: 0xffc3,
	F7: 0xffc4,
	F8: 0xffc5,
	F9: 0xffc6,
	F10: 0xffc7,
	F11: 0xffc8,
	F12: 0xffc9,
	Numpad0: 0xffb0,
	Numpad1: 0xffb1,
	Numpad2: 0xffb2,
	Numpad3: 0xffb3,
	Numpad4: 0xffb4,
	Numpad5: 0xffb5,
	Numpad6: 0xffb6,
	Numpad7: 0xffb7,
	Numpad8: 0xffb8,
	Numpad9: 0xffb9,
	NumpadDecimal: 0xffae,
	NumpadAdd: 0xffab,
	NumpadSubtract: 0xffad,
	NumpadMultiply: 0xffaa,
	NumpadDivide: 0xffaf,
};

/** Keysym X11 de un carácter: Unicode si es ≤ 0xFF; si no, 0x01000000 + punto. */
export function keysymDeCaracter(punto: number): number {
	if (!Number.isInteger(punto) || punto < 0 || punto > 0x10ffff) return 0;
	return punto <= 0xff ? punto : 0x01000000 + punto;
}

/**
 * Keysym de un evento de teclado: primero la tabla de teclas (para que un
 * Numpad1 mande KP_1 aunque el navegador diga key "1"), luego el carácter que
 * ha producido el evento y, si no hay ninguno, 0 (teclas muertas, IME), que se
 * manda igualmente con el código físico.
 */
export function keysymDeEvento(evento: KeyboardEvent): number {
	const porCodigo = KEYSYMS[evento.code];
	if (porCodigo !== undefined) return porCodigo;
	const clave = evento.key;
	if (!clave || clave === 'Dead' || clave === 'Unidentified') return 0;
	if ([...clave].length !== 1) return 0;
	return keysymDeCaracter(clave.codePointAt(0) ?? 0);
}
