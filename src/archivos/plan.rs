//! Plan de sincronización (Fase 8, §7.1), sin red ni interfaz: cruza el árbol
//! del origen con el del destino (ya sin excluidos) y decide crear, actualizar
//! (tamaño o mtime ±2 s), borrar (solo con la casilla) u omitir con motivo.
//! Al crear, permisos del origen `& 0o777`; al actualizar, los del destino.
//! Las rutas a borrar quedan fijadas aquí; el servidor no recalcula nada (T33).
