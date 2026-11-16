//! Lectura acotada de lo que devuelve `terminal_tmux.sh estado` (§7.7):
//! campos separados por 0x1F, títulos saneados de hasta 120 caracteres y 500
//! sesiones por host como mucho. Entrada no fiable: nunca pánico. Pista A del
//! Sprint 1.
