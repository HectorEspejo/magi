package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Cuándo vive el servicio en primer plano (informe §4.6). */
class PruebaReglasServicio {

    @Test
    fun elServicioViveMientrasHayaSesionesOTuneles() {
        assertFalse(ReglasServicio.debeEstarActivo(0, 0))
        assertTrue(ReglasServicio.debeEstarActivo(1, 0))
        assertTrue(ReglasServicio.debeEstarActivo(0, 1))
        assertTrue(ReglasServicio.debeEstarActivo(3, 2))
    }

    @Test
    fun losNumerosNegativosCuentanComoCero() {
        assertEquals(0, ReglasServicio.sanear(-1))
        assertEquals(0, ReglasServicio.sanear(-100))
        assertEquals(7, ReglasServicio.sanear(7))
        assertFalse(ReglasServicio.debeEstarActivo(-1, -1))
    }
}
