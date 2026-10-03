//! Permisos (Fase 8, §7.4), sin red ni interfaz: casillas rwx ⇄ octal (3 o 4
//! dígitos), estado mixto entre varios elementos, la máscara que se aplica
//! (`(viejo & !mascara) | (modo & mascara)`), el alcance recursivo y el aviso
//! de directorios que se quedan sin `x`.
