package org.cuatrodetres.magi.ui.modelos

/**
 * Logs en vivo (Fase 14, informe §5.1, §6.5 y §7.4): el anillo vive en
 * magi-movil (10 000 líneas y 8 MiB) y la app pide ventanas ya filtradas de
 * hasta 500 líneas. Por la FFI no cruzan listas sin acotar.
 *
 * La app solo pinta lo que le llega: las líneas no se guardan en disco ni se
 * mandan al registro.
 */

/**
 * Estado de una suscripción de logs.
 *
 * Los estados [ABRIENDO] y [EN_PAUSA] son propios del móvil: el núcleo sigue
 * recibiendo líneas mientras la vista está en pausa (las guarda en el anillo).
 */
enum class EstadoSuscripcionLogVista(
    /** Glifo de la cabecera. */
    val glifo: String,
    /** Texto de la cabecera. */
    val texto: String,
) {
    /** Abriendo el canal con el host: ◐. */
    ABRIENDO("◐", "abriendo"),

    /** Llegan líneas: ●. */
    SIGUIENDO("●", "siguiendo"),

    /** La vista se desplazó hacia arriba: ⏸. */
    EN_PAUSA("⏸", "en pausa"),

    /** Parada por la app (fin de la entrada estándar); las líneas se quedan. */
    PARADA("■", "parado"),

    /** El proceso remoto terminó: ✕. */
    TERMINADA("✕", "terminado"),

    /** El usuario no tiene permiso para el diario del sistema. */
    SIN_PERMISO("✕", "sin permiso para el diario"),

    /** Se cayó la conexión con el host. */
    CONEXION_CAIDA("✕", "se cortó la conexión"),

    /** El objetivo ya no existe. */
    NO_ENCONTRADA("✕", "el objetivo ya no existe"),
    ;

    /** Ya no van a llegar más líneas. */
    val esFinal: Boolean
        get() = this == TERMINADA || this == PARADA || this == SIN_PERMISO ||
            this == CONEXION_CAIDA || this == NO_ENCONTRADA

    /** Se puede volver a seguir (la suscripción se paró, no terminó sola). */
    val sePuedeSeguir: Boolean get() = this == PARADA

    /** Hay que enseñar «Dar contraseña de sudo». */
    val sugiereSudo: Boolean get() = this == SIN_PERMISO
}

/** Motivo del fin de una suscripción de logs (espejo del núcleo). */
enum class MotivoFinLogVista(
    /** Texto del motivo. */
    val texto: String,
) {
    /** La app la paró. */
    CERRADO("cerrado"),

    /** El proceso remoto terminó solo. */
    TERMINADO("el proceso remoto terminó"),

    /** El usuario no tiene permiso para el diario. */
    SIN_PERMISO("sin permiso para el diario"),

    /** Se cayó la conexión con el host. */
    CONEXION_CAIDA("se cortó la conexión"),

    /** El objetivo ya no existe. */
    NO_ENCONTRADO("el objetivo ya no existe"),
    ;

    /** Estado de la suscripción que corresponde al motivo. */
    val estado: EstadoSuscripcionLogVista
        get() = when (this) {
            CERRADO -> EstadoSuscripcionLogVista.PARADA
            TERMINADO -> EstadoSuscripcionLogVista.TERMINADA
            SIN_PERMISO -> EstadoSuscripcionLogVista.SIN_PERMISO
            CONEXION_CAIDA -> EstadoSuscripcionLogVista.CONEXION_CAIDA
            NO_ENCONTRADO -> EstadoSuscripcionLogVista.NO_ENCONTRADA
        }
}

/** Filtro por nivel del panel de logs (informe §6.5). */
enum class NivelLogVista(
    /** Texto del filtro. */
    val texto: String,
) {
    TODO("todo"),
    INFO("info"),
    AVISO("aviso"),
    ERROR("error"),
    ;

    /** Siguiente nivel al pulsar el filtro (ciclo de la F13). */
    fun siguiente(): NivelLogVista = when (this) {
        TODO -> INFO
        INFO -> AVISO
        AVISO -> ERROR
        ERROR -> TODO
    }

    /** Prioridad máxima visible (0 = emerg; 7 = todo). */
    val prioridadMaxima: Int
        get() = when (this) {
            TODO -> 7
            INFO -> 6
            AVISO -> 4
            ERROR -> 3
        }
}

/** Nivel ya derivado de una línea, para el filtro y el glifo. */
enum class NivelLineaLogVista(
    /** Glifo de la línea en el panel. */
    val glifo: String,
) {
    /** Error o peor (prioridad 0-3): ✕. */
    ERROR("✕"),

    /** Aviso (prioridad 4) o stderr en un contenedor: !. */
    AVISO("!"),

    /** El resto: ·. */
    INFO("·"),
}

/**
 * Nivel de una línea a partir de la prioridad de journald y de si llegó por
 * stderr. En los contenedores stderr marca aviso aunque la prioridad sea 6.
 */
fun nivelLineaLog(prioridad: Int, stderr: Boolean): NivelLineaLogVista = when {
    prioridad <= 3 -> NivelLineaLogVista.ERROR
    prioridad <= 4 || stderr -> NivelLineaLogVista.AVISO
    else -> NivelLineaLogVista.INFO
}

/**
 * Una línea del anillo de logs (informe §7.4).
 *
 * [numero] no se repite y no vuelve a empezar: la vista en pausa se ancla a
 * él para no moverse cuando llegan líneas nuevas.
 */
data class LineaLogVista(
    /** Número que no se repite. */
    val numero: Long = 0,
    val texto: String = "",
    /** Nivel ya derivado de la prioridad del diario (o de stderr). */
    val nivel: NivelLineaLogVista = NivelLineaLogVista.INFO,
    /** Hora en la época del host (segundos); null si no la trae. */
    val hora: Long? = null,
    /** Llegó por la salida de errores (contenedores). */
    val stderr: Boolean = false,
) {
    /** Glifo de la línea, ya con el nivel y stderr. */
    val glifo: String
        get() = if (nivel == NivelLineaLogVista.ERROR) "✕"
        else if (stderr || nivel == NivelLineaLogVista.AVISO) "!"
        else "·"
}

/**
 * Filtro y ventana que pide la app para pintar los logs (informe §7.4).
 *
 * [desde] es el número de línea desde el que empezar cuando la vista está en
 * pausa; null = desde el final, siguiendo.
 */
data class FiltroLogsVista(
    /** Texto a buscar (vacío = sin filtro de texto). */
    val texto: String = "",
    val nivel: NivelLogVista = NivelLogVista.TODO,
    /** Solo líneas de la salida de errores (contenedores). */
    val soloStderr: Boolean = false,
    /** Número de línea desde el que empezar (en pausa); null = al final. */
    val desde: Long? = null,
    /** Líneas pedidas; el núcleo lo acota a 500. */
    val cuantas: Int = LINEAS_POR_VENTANA,
) {
    /** Líneas pedidas ya acotadas a lo que permite el núcleo. */
    val cuantasAcotadas: Int get() = cuantas.coerceIn(1, MAXIMO_LINEAS_POR_VENTANA)

    companion object {
        /** Líneas que se piden al abrir el panel. */
        const val LINEAS_INICIALES: Int = 200

        /** Líneas que se piden en cada ventana de refresco. */
        const val LINEAS_POR_VENTANA: Int = 60

        /** Tope duro que impone el núcleo. */
        const val MAXIMO_LINEAS_POR_VENTANA: Int = 500
    }
}

/**
 * Novedades de una suscripción que llegan por el flujo de avisos de logs
 * (informe §5.2), como mucho cada 100 ms.
 */
data class EstadoLogsVista(
    val suscripcion: Long = 0,
    /** Total de líneas en el anillo de esa suscripción. */
    val total: Long = 0,
    /** Líneas nuevas desde el aviso anterior. */
    val nuevas: Long = 0,
    /** Líneas descartadas (anillo o lectura del script) desde el anterior. */
    val omitidas: Long = 0,
    val estado: EstadoSuscripcionLogVista = EstadoSuscripcionLogVista.SIGUIENDO,
) {
    /** Hay líneas nuevas que pintar. */
    val hayNuevas: Boolean get() = nuevas > 0
}

/**
 * Ventana ya filtrada de hasta 500 líneas (informe §5.1 y §7.4), con el total,
 * las omitidas y el estado de la suscripción.
 */
data class VentanaLogsVista(
    val lineas: List<LineaLogVista> = emptyList(),
    /** Total de líneas que cumplen el filtro en el anillo. */
    val total: Long = 0,
    /** Líneas omitidas que hay que enseñar donde faltan. */
    val omitidas: Long = 0,
    val estado: EstadoSuscripcionLogVista = EstadoSuscripcionLogVista.SIGUIENDO,
    /** Motivo del fin, si la suscripción ya terminó. */
    val motivo: MotivoFinLogVista? = null,
) {
    /** No hay ninguna línea que pintar todavía. */
    val vacia: Boolean get() = lineas.isEmpty()

    /** Texto de la cabecera: «1 214 líneas · 14 omitidas». */
    val textoRecuento: String
        get() {
            val partes = mutableListOf(total.toString() + " líneas")
            if (omitidas > 0) partes += omitidas.toString() + " omitidas"
            return partes.joinToString(" · ")
        }
}
