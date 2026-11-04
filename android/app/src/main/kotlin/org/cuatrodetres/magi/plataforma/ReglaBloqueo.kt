package org.cuatrodetres.magi.plataforma

/**
 * Regla del bloqueo de la app (informe §7.8), sin Android para probarla en la
 * JVM: el arranque en frío siempre está bloqueado y al volver manda el margen
 * de cinco minutos.
 */
object ReglaBloqueo {

    /** Cinco minutos fuera de la app: a partir de aquí se pide desbloqueo. */
    const val MARGEN_SEGUNDOS: Long = 5L * 60L

    enum class Estado { BLOQUEADA, DESBLOQUEADA }

    fun alVolver(ahoraSegundos: Long, salioEn: Long?, arranqueEnFrio: Boolean): Estado {
        if (arranqueEnFrio || salioEn == null) return Estado.BLOQUEADA
        return if (ahoraSegundos - salioEn < MARGEN_SEGUNDOS) {
            Estado.DESBLOQUEADA
        } else {
            Estado.BLOQUEADA
        }
    }
}
