package org.cuatrodetres.magi.ui.modelos

/**
 * MAGI · app Android (Fase 11) — modelos de vista compartidos.
 *
 * La interfaz no habla directamente con los bindings UniFFI de `magi-movil`:
 * habla con [org.cuatrodetres.magi.ui.fuente.FuenteNucleo], que entrega estos
 * DTO propios de la capa de presentación. Así las pantallas y los ViewModels se
 * pueden probar en la JVM sin el núcleo nativo.
 */

/** Resultado de una acción de la interfaz contra el núcleo. */
sealed interface ResultadoAccion {
    /** La acción se completó. */
    data object Ok : ResultadoAccion

    /** La acción falló; [mensaje] es legible y lleva instrucción cuando aplica. */
    data class Error(val mensaje: String) : ResultadoAccion
}

/** Aviso efímero para la barra inferior. */
sealed interface AvisoVista {
    /** Cambia en cada aviso para que la interfaz lo vuelva a mostrar. */
    val identificador: Long

    /** Error o mensaje que ya llega en castellano desde el núcleo. */
    data class DeNucleo(
        override val identificador: Long,
        val mensaje: String,
        val esError: Boolean,
    ) : AvisoVista

    /** Aviso generado por la interfaz; el texto se resuelve en strings.xml. */
    data class DeInterfaz(
        override val identificador: Long,
        val tipo: TipoAviso,
        val detalle: String? = null,
    ) : AvisoVista
}

/** Avisos que produce la interfaz (los textos viven en res/values/strings.xml). */
enum class TipoAviso {
    SESION_CERRADA,
    SINCRONIZACION_AL_DIA,
    SINCRONIZACION_DEJADA,
    CLAVE_INSTALADA_YA_ESTABA,
    CLAVE_INSTALADA_ACEPTADA,
    CLAVE_INSTALADA_NO_ACEPTADA,
    CONTRASENA_OLVIDADA,
    CONFLICTO_HUELLA_ACEPTADO,
    CONFLICTO_HUELLA_MANTENIDO,
    DATOS_BORRADOS,
    TUNEL_ACTIVADO,
    TUNEL_PARADO,
    HOST_DESAPARECIDO,
    ERROR_INESPERADO,
    TEXTO_COPIADO,
    PORTAPAPELES_VACIO,
    SELECCION_VACIA,
    CLAVE_MOVIL_CREADA,
    CLAVE_MOVIL_NO_CREADA,
    TUNEL_DESCARTADO,
    ESCRITORIO_EN_EL_ORDENADOR,
}

/** Estado del bloqueo de la app (diagrama de §3 y regla de §7.8). */
enum class EstadoBloqueoVista {
    /** Hay que desbloquear con huella o credencial del teléfono. */
    BLOQUEADA,

    /** La interfaz se puede usar. */
    DESBLOQUEADA,

    /** El teléfono no tiene PIN, patrón ni contraseña: MAGI no arranca (§4.1). */
    SIN_BLOQUEO_DE_PANTALLA,
}

/**
 * Lo que la interfaz necesita del bloqueo del teléfono.
 *
 * La implementación real vive en `plataforma/Bloqueo.kt` (BiometricPrompt con
 * BIOMETRIC_STRONG | DEVICE_CREDENTIAL) y se adapta a esta interfaz al cablear
 * MainActivity. La UI solo pinta la pantalla de bloqueo y pregunta.
 */
interface ControlBloqueo {
    /** Si el teléfono tiene PIN, patrón o contraseña configurados. */
    fun hayBloqueoDePantalla(): Boolean = true

    /** Si al arrancar en frío hay que empezar bloqueado. */
    fun debeBloquearAlArrancar(): Boolean = true

    /** Pide el desbloqueo (biometría o credencial); true si el usuario se validó. */
    suspend fun solicitarDesbloqueo(): Boolean

    /** La app pasa a segundo plano: se anota la hora de salida. */
    fun alSalirDeLaApp() {}

    /** La app vuelve a primer plano: true si toca bloquear (margen de 5 min). */
    fun alVolverALaApp(): Boolean = true
}

/** Control de bloqueo por defecto: desarrollo, pruebas y previsualizaciones. */
object ControlBloqueoPermitido : ControlBloqueo {
    override fun debeBloquearAlArrancar(): Boolean = false

    override suspend fun solicitarDesbloqueo(): Boolean = true

    override fun alVolverALaApp(): Boolean = false
}
