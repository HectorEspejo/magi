//! Ciclo de una edición remota (Fase 8, §3 y §7.3), sin red ni interfaz:
//! datos fijados al bajar el fichero (ruta, mtime, tamaño, permisos, uid y
//! SHA-256 del temporal), detección de cambios locales por hash, conflicto con
//! el remoto por `StatRemoto` (mtime o tamaño), aviso de propietario y nombre
//! de la copia local `<nombre>.magi-<AAAAMMDD-HHMMSS>`.
//!
//! Regla de oro (T55): el temporal solo se borra tras `hecha`, tras «sin
//! cambios» o tras «descartar» explícito.
