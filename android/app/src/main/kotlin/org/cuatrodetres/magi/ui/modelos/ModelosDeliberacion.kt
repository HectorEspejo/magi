package org.cuatrodetres.magi.ui.modelos

/**
 * Deliberación MAGI (Fase 14, informe §5.1, §6.6, §6.7 y §7.7): las
 * comprobaciones, su tabla host × comprobación, el consenso por unanimidad y
 * la decisión con la que se resuelve.
 *
 * Las comprobaciones corren en el núcleo (2 s cada una, en paralelo) y el
 * veredicto siempre llega con su dato: nunca un ✕ sin motivo. La app solo
 * pinta y pide mantener pulsado 1 s para ejecutar.
 */

/** Caracteres mínimos del motivo de un forzado (sin contar extremos). */
const val MOTIVO_DELIBERACION_MINIMO: Int = 10

/** Las tres comprobaciones, con su nombre MAGI y la palabra que comprueban. */
enum class ComprobacionVista(
    /** Nombre de la comprobación en el diálogo. */
    val nombreMagi: String,
    /** Qué comprueba, en una palabra. */
    val palabra: String,
) {
    SALUD("MELCHIOR-1", "salud"),
    BACKUP("BALTHASAR-2", "backup"),
    TESTS("CASPER-3", "tests"),
}

/**
 * Veredicto de una comprobación en un host. Siempre con el dato que lo
 * justifica; [NoActiva] es una comprobación que ese host no tiene activa.
 */
sealed interface VeredictoVista {
    /** La comprobación no está activa en ese host (—). */
    data object NoActiva : VeredictoVista

    /** Aún comprobando (◐). */
    data object Pendiente : VeredictoVista

    /** Aprueba con su dato y lo que tardó. */
    data class Aprueba(val detalle: String, val ms: Long = 0) : VeredictoVista

    /** Rechaza con su motivo y lo que tardó. */
    data class Rechaza(val detalle: String, val ms: Long = 0) : VeredictoVista
}

/** Glifo del veredicto (✓, ✕, — o ◐). */
val VeredictoVista.glifo: String
    get() = when (this) {
        is VeredictoVista.NoActiva -> "—"
        is VeredictoVista.Pendiente -> "◐"
        is VeredictoVista.Aprueba -> "✓"
        is VeredictoVista.Rechaza -> "✕"
    }

/** ¿Cuenta para el consenso? (las no activas no). */
val VeredictoVista.cuenta: Boolean get() = this !is VeredictoVista.NoActiva

/** ¿Aprobó? */
val VeredictoVista.aprueba: Boolean get() = this is VeredictoVista.Aprueba

/** ¿Rechazó? */
val VeredictoVista.rechaza: Boolean get() = this is VeredictoVista.Rechaza

/** El dato que justifica el veredicto; null en no activa y pendiente. */
val VeredictoVista.detalle: String?
    get() = when (this) {
        is VeredictoVista.Aprueba -> detalle
        is VeredictoVista.Rechaza -> detalle
        else -> null
    }

/** Milisegundos que tardó la comprobación; 0 si aún no ha llegado. */
val VeredictoVista.ms: Long
    get() = when (this) {
        is VeredictoVista.Aprueba -> ms
        is VeredictoVista.Rechaza -> ms
        else -> 0
    }

/** Una fila de la tabla de deliberación: un host y sus tres comprobaciones. */
data class FilaHostDeliberacionVista(
    val hostUuid: String? = null,
    val hostNombre: String = "",
    val salud: VeredictoVista = VeredictoVista.NoActiva,
    val backup: VeredictoVista = VeredictoVista.NoActiva,
    val tests: VeredictoVista = VeredictoVista.NoActiva,
) {
    /** Veredicto de una comprobación concreta. */
    fun veredicto(comprobacion: ComprobacionVista): VeredictoVista = when (comprobacion) {
        ComprobacionVista.SALUD -> salud
        ComprobacionVista.BACKUP -> backup
        ComprobacionVista.TESTS -> tests
    }

    /** Celdas de la fila en el orden de [ComprobacionVista]. */
    val veredictos: List<VeredictoVista>
        get() = listOf(salud, backup, tests)
}

/** Estado del diálogo MAGI (informe §6.6 y diagrama de §3.3). */
enum class EstadoDeliberacionVista(
    /** Glifo del estado. */
    val glifo: String,
    /** Texto del estado. */
    val texto: String,
) {
    /** Las comprobaciones están en curso: ◐. */
    COMPROBANDO("◐", "comprobando"),

    /** Todas las activas aprueban (o no hay ninguna): ▸. */
    APROBADA("▸", "APROBADO"),

    /** Alguna activa rechaza: ✕. */
    BLOQUEADA("✕", "BLOQUEADO"),

    /** Se forzó con motivo de 10 caracteres o más: ⚑. */
    FORZADA("⚑", "FORZADO"),

    /** Se canceló: —. */
    CANCELADA("—", "CANCELADA"),
    ;

    /** Ya no cambia. */
    val esFinal: Boolean
        get() = this == APROBADA || this == BLOQUEADA || this == FORZADA || this == CANCELADA

    /** Se puede ejecutar manteniendo pulsado. */
    val sePuedeEjecutar: Boolean get() = this == APROBADA || this == FORZADA
}

/**
 * Resultado de deliberar: la tabla host × comprobación, el consenso y el
 * estado con el id de la deliberación en curso (el que se pasa a resolver).
 */
data class TablaDeliberacionVista(
    /** Id de la deliberación en curso, para resolverla. */
    val id: Long = 0,
    val filas: List<FilaHostDeliberacionVista> = emptyList(),
    val aprobadas: Int = 0,
    val activas: Int = 0,
    val rechazos: Int = 0,
    val pendientes: Int = 0,
    /** Unanimidad de las comprobaciones activas. */
    val unanime: Boolean = false,
    val estado: EstadoDeliberacionVista = EstadoDeliberacionVista.COMPROBANDO,
) {
    /** Todas las activas aprueban (o no hay ninguna activa). */
    val consenso: Boolean get() = unanime

    /** Porcentaje de aprobadas sobre activas (0-100). */
    val porcentaje: Int
        get() = if (activas <= 0) 100 else (aprobadas * 100 / activas).coerceIn(0, 100)

    /** «2/3», para la barra de consenso. */
    val textoConsenso: String get() = aprobadas.toString() + "/" + activas

    /** Sin ninguna comprobación activa: se sigue pidiendo mantener pulsado. */
    val sinComprobaciones: Boolean get() = activas == 0

    /** Hay algo rechazado y por tanto no se puede aprobar sin forzar. */
    val bloqueada: Boolean get() = rechazos > 0
}

/**
 * Lo que se delibera. El núcleo fija el plan al abrir el diálogo (T33) y no lo
 * relee al confirmar: la app solo elige entre una ejecución preparada o una
 * acción de servicio.
 */
sealed interface PlanDeliberacionVista {
    /** Texto de la acción que se delibera, para la cabecera. */
    val accion: String

    /** Nombres de los hosts que se comprueban. */
    val hosts: List<String>

    /** Comprobación de configuración que se aplicará antes, si la hay. */
    val comprobacion: String?

    /** Una ejecución de snippet ya preparada. */
    data class Snippet(val preparacion: PreparacionEjecucionVista) : PlanDeliberacionVista {
        override val accion: String
            get() = preparacion.snippetNombre + " → " +
                preparacion.hosts.joinToString(", ") { it.nombre }

        override val hosts: List<String> get() = preparacion.hosts.map { it.nombre }

        override val comprobacion: String? get() = null
    }

    /** Una acción sobre un servicio, un timer, un contenedor o un proyecto. */
    data class Servicio(
        val hostUuid: String,
        val hostNombre: String,
        val objetivo: ObjetivoVista,
        val accionServicio: AccionServicioVista,
        override val comprobacion: String? = null,
    ) : PlanDeliberacionVista {
        override val accion: String
            get() = accionServicio.texto + " " + objetivo.nombre + " → " + hostNombre

        override val hosts: List<String> get() = listOf(hostNombre)
    }
}

/** Cómo se resuelve una deliberación (informe §5.1 y §6.6). */
sealed interface DecisionDeliberacionVista {
    /** Aprobar lo que el consenso ya permite. */
    data object Aprobar : DecisionDeliberacionVista

    /** Forzar una deliberación bloqueada con un motivo de 10 caracteres o más. */
    data class Forzar(val motivo: String) : DecisionDeliberacionVista

    /** Cancelar: no se ejecuta nada y queda anotada la cancelación. */
    data object Cancelar : DecisionDeliberacionVista
}

/** ¿El motivo de un forzado vale? (10 caracteres o más sin contar extremos). */
fun motivoForzadoValido(motivo: String): Boolean =
    motivo.trim().length >= MOTIVO_DELIBERACION_MINIMO

/**
 * Por qué una ejecución pasa por la deliberación (espejo del MotivoDeliberacion
 * del núcleo). La preparación ya trae los motivos legibles; este tipo sirve
 * para las pruebas y las previsualizaciones.
 */
sealed interface MotivoDeliberacionVista {
    /** Texto legible del motivo. */
    val texto: String

    /** El snippet es crítico. */
    data object Critico : MotivoDeliberacionVista {
        override val texto: String get() = "crítico"
    }

    /** Va a más de un host. */
    data class VariosHosts(val cuantos: Int) : MotivoDeliberacionVista {
        override val texto: String get() = cuantos.toString() + " hosts"
    }

    /** Hay hosts con alguna verificación activa. */
    data class Verificaciones(val hosts: List<String>) : MotivoDeliberacionVista {
        override val texto: String get() = "verificaciones en " + hosts.joinToString(", ")
    }
}

/**
 * Motivos para deliberar una ejecución: crítico, más de un host o algún host
 * con verificaciones activas. Vacío = se lanza sin deliberación.
 */
fun motivosDeDeliberacion(
    critico: Boolean,
    hosts: List<String>,
    hostsConVerificaciones: List<String> = emptyList(),
): List<MotivoDeliberacionVista> {
    val motivos = mutableListOf<MotivoDeliberacionVista>()
    if (critico) motivos += MotivoDeliberacionVista.Critico
    if (hosts.size > 1) motivos += MotivoDeliberacionVista.VariosHosts(hosts.size)
    if (hostsConVerificaciones.isNotEmpty()) {
        motivos += MotivoDeliberacionVista.Verificaciones(hostsConVerificaciones)
    }
    return motivos
}
