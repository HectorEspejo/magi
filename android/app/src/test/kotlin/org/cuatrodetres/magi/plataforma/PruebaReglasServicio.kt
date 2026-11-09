package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Cuándo vive el servicio en primer plano y qué cuenta (informe §3.3 y D279). */
class PruebaReglasServicio {

    @Test
    fun elServicioViveMientrasHayaSesionesOTuneles() {
        assertFalse(ReglasServicio.debeEstarActivo(0, 0))
        assertTrue(ReglasServicio.debeEstarActivo(1, 0))
        assertTrue(ReglasServicio.debeEstarActivo(0, 1))
        assertTrue(ReglasServicio.debeEstarActivo(3, 2))
    }

    @Test
    fun elServicioTambienViveConAccionesOEjecuciones() {
        // Fase 14: una acción o una ejecución sigue con la app en segundo plano.
        assertTrue(ReglasServicio.debeEstarActivo(0, 0, acciones = 1))
        assertTrue(ReglasServicio.debeEstarActivo(0, 0, ejecuciones = 1))
        assertTrue(ReglasServicio.debeEstarActivo(1, 0, acciones = 2, ejecuciones = 3))
        assertFalse(ReglasServicio.debeEstarActivo(0, 0, 0, 0))
    }

    @Test
    fun losNumerosNegativosCuentanComoCero() {
        assertEquals(0, ReglasServicio.sanear(-1))
        assertEquals(0, ReglasServicio.sanear(-100))
        assertEquals(7, ReglasServicio.sanear(7))
        assertFalse(ReglasServicio.debeEstarActivo(-1, -1))
        assertFalse(ReglasServicio.debeEstarActivo(-1, -1, -1, -1))
    }

    @Test
    fun laNotificacionCuentaSoloLoQueHayYEnSuOrden() {
        val cuentas = ReglasServicio.cuentasDeLaNotificacion(
            sesiones = 2,
            tuneles = 0,
            acciones = 1,
            ejecuciones = 0,
        )
        assertEquals(
            listOf(
                CuentaTrabajo(TipoTrabajoNotificacion.SESIONES, 2),
                CuentaTrabajo(TipoTrabajoNotificacion.ACCIONES, 1),
            ),
            cuentas,
        )
    }

    @Test
    fun laNotificacionSigueElOrdenDelInforme() {
        val cuentas = ReglasServicio.cuentasDeLaNotificacion(
            sesiones = 1,
            tuneles = 2,
            acciones = 3,
            ejecuciones = 4,
        )
        assertEquals(
            listOf(
                TipoTrabajoNotificacion.SESIONES,
                TipoTrabajoNotificacion.TUNELES,
                TipoTrabajoNotificacion.ACCIONES,
                TipoTrabajoNotificacion.EJECUCIONES,
            ),
            cuentas.map { it.tipo },
        )
        assertEquals(listOf(1, 2, 3, 4), cuentas.map { it.cantidad })
    }

    @Test
    fun sinTrabajoNoHayCuentasQueEnsenar() {
        assertTrue(ReglasServicio.cuentasDeLaNotificacion(0, 0, 0, 0).isEmpty())
    }
}
