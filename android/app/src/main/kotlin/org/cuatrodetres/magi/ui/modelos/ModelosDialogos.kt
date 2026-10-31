package org.cuatrodetres.magi.ui.modelos

/**
 * Diálogos de la app (informe §6.6 y §7.1).
 *
 * Hay tres familias:
 * - peticiones del núcleo que esperan respuesta (huella nueva, huella cambiada
 *   y contraseña), con su id para correlacionar la respuesta;
 * - confirmaciones de la propia interfaz (ninguna acción destructiva con una
 *   sola pulsación, T6);
 * - el conflicto de huella que llega de otro equipo (aceptar o mantener).
 */

/** Petición del núcleo que la interfaz debe enseñar y responder. */
sealed interface PeticionDialogoVista {
    /** Correlaciona la respuesta con la operación que espera. */
    val peticionId: Long
    val hostUuid: String
    val hostNombre: String

    /** Huella desconocida: aceptar o cancelar. */
    data class HuellaNueva(
        override val peticionId: Long,
        override val hostUuid: String,
        override val hostNombre: String,
        val tipo: String,
        val sha256: String,
    ) : PeticionDialogoVista

    /** Huella cambiada: bloqueo; sustituir exige escribir el nombre del host. */
    data class HuellaCambiada(
        override val peticionId: Long,
        override val hostUuid: String,
        override val hostNombre: String,
        val tipo: String,
        val anterior: String,
        val nueva: String,
        val nombreAEscribir: String,
    ) : PeticionDialogoVista

    /** Contraseña de host: 3 intentos y «recordar en este móvil». */
    data class Contrasena(
        override val peticionId: Long,
        override val hostUuid: String,
        override val hostNombre: String,
        val usuario: String,
        val direccion: String,
        val intentosRestantes: Int,
        val recordarOfrecido: Boolean,
    ) : PeticionDialogoVista
}

/** Respuesta que la interfaz manda al núcleo (los bytes se borran tras usarlos). */
sealed interface RespuestaDialogoVista {
    data object AceptarHuella : RespuestaDialogoVista
    data object CancelarHuella : RespuestaDialogoVista
    data class SustituirHuella(val nombreEscrito: String) : RespuestaDialogoVista
    data class Contrasena(val bytes: ByteArray, val recordar: Boolean) : RespuestaDialogoVista
    data object CancelarContrasena : RespuestaDialogoVista
}

/** Tipos de confirmación de la interfaz (T6). */
enum class TipoConfirmacion {
    CERRAR_SESION,
    DEJAR_DE_SINCRONIZAR,
    OLVIDAR_CONTRASENA,
    INSTALAR_CLAVE,
    ACEPTAR_HUELLA_DE_EQUIPO,
    BORRAR_DATOS,
}

/** Diálogo que se está pintando en la app. */
sealed interface DialogoVista {
    /** Pregunta del núcleo; se responde por `responder_dialogo`. */
    data class DeNucleo(val peticion: PeticionDialogoVista) : DialogoVista

    /** Confirmación con una acción destructiva o delicada (T6). */
    data class Confirmacion(
        val tipo: TipoConfirmacion,
        /** Texto variable: nombre del host, sesión, etc. */
        val detalle: String? = null,
        val hostUuid: String? = null,
        val sesionId: Long? = null,
        /** Identificador del conflicto cuando aplica. */
        val conflictoId: Long? = null,
    ) : DialogoVista

    /** Conflicto de huella que llega de otro equipo; se resuelve en el móvil. */
    data class ConflictoHuella(val conflicto: ConflictoHuellaVista) : DialogoVista

    /** Oferta de la guía de batería al abrir la primera sesión. */
    data object GuiaBateria : DialogoVista
}

/** Pasos del asistente de unión a la sincronización (§4.1). */
enum class PasoAsistente {
    ELEGIR_CARPETA,
    UNIR_CLAVE,
}

/** Estado del asistente de sincronización que se pinta a pantalla completa. */
data class AsistenteVista(
    val paso: PasoAsistente = PasoAsistente.ELEGIR_CARPETA,
    /** Carpeta elegida en este intento (nombre legible), si ya la hay. */
    val carpeta: String? = null,
    val error: String? = null,
    val ocupado: Boolean = false,
)
