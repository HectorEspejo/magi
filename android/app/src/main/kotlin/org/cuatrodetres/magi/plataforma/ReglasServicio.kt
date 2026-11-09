package org.cuatrodetres.magi.plataforma

/**
 * Reglas del servicio en primer plano (informe §3.3 y D279).
 *
 * Desde la Fase 14 el servicio cuenta también las acciones de servicio y las
 * ejecuciones de snippets: siguen con la app en segundo plano. El sondeo, los
 * refrescos y los logs no cuentan: solo corren con su pantalla a la vista.
 */
object ReglasServicio {

    /** Nunca se publica un número negativo en la notificación. */
    fun sanear(cantidad: Int): Int = cantidad.coerceAtLeast(0)

    /** El servicio vive mientras haya sesiones, túneles, acciones o ejecuciones. */
    fun debeEstarActivo(
        sesiones: Int,
        tuneles: Int,
        acciones: Int = 0,
        ejecuciones: Int = 0,
    ): Boolean = sanear(sesiones) > 0 ||
        sanear(tuneles) > 0 ||
        sanear(acciones) > 0 ||
        sanear(ejecuciones) > 0

    /**
     * Recuentos que salen en la notificación, en el orden del ejemplo del
     * informe («MAGI · 2 sesiones · 1 acción»). Los que van a cero no salen.
     */
    fun cuentasDeLaNotificacion(
        sesiones: Int,
        tuneles: Int,
        acciones: Int = 0,
        ejecuciones: Int = 0,
    ): List<CuentaTrabajo> = buildList {
        if (sanear(sesiones) > 0) add(CuentaTrabajo(TipoTrabajoNotificacion.SESIONES, sanear(sesiones)))
        if (sanear(tuneles) > 0) add(CuentaTrabajo(TipoTrabajoNotificacion.TUNELES, sanear(tuneles)))
        if (sanear(acciones) > 0) add(CuentaTrabajo(TipoTrabajoNotificacion.ACCIONES, sanear(acciones)))
        if (sanear(ejecuciones) > 0) {
            add(CuentaTrabajo(TipoTrabajoNotificacion.EJECUCIONES, sanear(ejecuciones)))
        }
    }
}

/** Tipo de trabajo que cuenta el servicio en primer plano. */
enum class TipoTrabajoNotificacion {
    SESIONES,
    TUNELES,
    ACCIONES,
    EJECUCIONES,
}

/** Un recuento de la notificación (tipo y cuántos hay). */
data class CuentaTrabajo(
    val tipo: TipoTrabajoNotificacion,
    val cantidad: Int,
)
