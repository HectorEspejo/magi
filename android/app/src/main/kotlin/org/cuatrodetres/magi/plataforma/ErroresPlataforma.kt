package org.cuatrodetres.magi.plataforma

/**
 * Error de la capa de plataforma. Espeja las variantes de `ErrorNucleo` del
 * contrato UniFFI para que el adaptador lo traduzca sin perder el motivo: el
 * mensaje ya viene en castellano y listo para enseñar.
 */
sealed class ErrorPlataforma(
    mensaje: String,
    causa: Throwable? = null,
) : Exception(mensaje, causa) {

    /** La carpeta compartida no se puede usar (sin elegir, permiso retirado…). */
    class Carpeta(mensaje: String, causa: Throwable? = null) : ErrorPlataforma(mensaje, causa)

    /** Falla del llavero cifrado (Keystore, fichero dañado…). */
    class Llavero(mensaje: String, causa: Throwable? = null) : ErrorPlataforma(mensaje, causa)

    /** Falla de la clave del móvil (sin crear, invalidada, teléfono bloqueado). */
    class Firmante(mensaje: String, causa: Throwable? = null) : ErrorPlataforma(mensaje, causa)

    /** Cualquier otro error de plataforma con su motivo legible. */
    class General(mensaje: String, causa: Throwable? = null) : ErrorPlataforma(mensaje, causa)
}
