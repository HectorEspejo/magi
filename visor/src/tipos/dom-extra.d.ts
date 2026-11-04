// Tipos del DOM que la biblioteca de TypeScript aún no declara.

// Keyboard Lock (Chromium): en pantalla completa captura las combinaciones
// reservadas del navegador, para que vayan al escritorio remoto.
interface Keyboard {
	lock(codigos?: string[]): Promise<void>;
	unlock(): void;
}

interface Navigator {
	readonly keyboard?: Keyboard;
}
