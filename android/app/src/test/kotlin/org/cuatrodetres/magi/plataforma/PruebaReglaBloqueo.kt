package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Test

/** Regla de los cinco minutos del bloqueo de la app (informe §7.8). */
class PruebaReglaBloqueo {

    private fun bloquear(ahora: Long, salio: Long?, frio: Boolean = false) =
        ReglaBloqueo.alVolver(ahora, salio, frio)

    @Test
    fun elArranqueEnFrioSiempreEstaBloqueado() {
        assertEquals(
            ReglaBloqueo.Estado.BLOQUEADA,
            bloquear(ahora = 1_000L, salio = 990L, frio = true),
        )
    }

    @Test
    fun sinSalidaAnotadaEstaBloqueada() {
        assertEquals(
            ReglaBloqueo.Estado.BLOQUEADA,
            bloquear(ahora = 1_000L, salio = null),
        )
    }

    @Test
    fun cuatroMinutosFueraSigueDesbloqueada() {
        val salida = 10_000L
        assertEquals(
            ReglaBloqueo.Estado.DESBLOQUEADA,
            bloquear(ahora = salida + 4L * 60L, salio = salida),
        )
    }

    @Test
    fun seisMinutosFueraPideDesbloqueo() {
        val salida = 10_000L
        assertEquals(
            ReglaBloqueo.Estado.BLOQUEADA,
            bloquear(ahora = salida + 6L * 60L, salio = salida),
        )
    }

    @Test
    fun justoEnElMargenPideDesbloqueo() {
        val salida = 10_000L
        assertEquals(
            ReglaBloqueo.Estado.BLOQUEADA,
            bloquear(ahora = salida + ReglaBloqueo.MARGEN_SEGUNDOS, salio = salida),
        )
    }

    @Test
    fun unSegundoAntesDelMargenNoPideNada() {
        val salida = 10_000L
        assertEquals(
            ReglaBloqueo.Estado.DESBLOQUEADA,
            bloquear(ahora = salida + ReglaBloqueo.MARGEN_SEGUNDOS - 1L, salio = salida),
        )
    }

    @Test
    fun elMargenEsDeCincoMinutos() {
        assertEquals(300L, ReglaBloqueo.MARGEN_SEGUNDOS)
    }
}
