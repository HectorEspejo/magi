package org.cuatrodetres.magi.ui.ajustes

import org.cuatrodetres.magi.ui.modelos.CARGA_POR_NUCLEO_MAX
import org.cuatrodetres.magi.ui.modelos.CARGA_POR_NUCLEO_MIN
import org.cuatrodetres.magi.ui.modelos.FLOTA_REFRESCO_SEG_MAX
import org.cuatrodetres.magi.ui.modelos.FLOTA_REFRESCO_SEG_MIN
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_ESPERA_SEG_MAX
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_ESPERA_SEG_MIN
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_REFRESCO_SEG_MAX
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_REFRESCO_SEG_MIN
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Reglas puras de las pantallas de Ajustes de la Fase 14: los deslizadores
 * solo ofrecen valores válidos (§5.4) y los motores se alternan sin repetir.
 */
class PruebaReglasAjustes {

    @Test
    fun elRefrescoDeFlotaOfreceApagadoYElRangoDelEscritorio() {
        val valores = valoresRefrescoFlota()
        assertEquals(0, valores.first())
        assertTrue(valores.contains(FLOTA_REFRESCO_SEG_MIN))
        assertTrue(valores.contains(FLOTA_REFRESCO_SEG_MAX))
        assertTrue(valores.drop(1).all { it in FLOTA_REFRESCO_SEG_MIN..FLOTA_REFRESCO_SEG_MAX })
        assertEquals(valores, valores.sorted())
    }

    @Test
    fun elRefrescoDeServiciosOfreceApagadoYElRangoDelEscritorio() {
        val valores = valoresRefrescoServicios()
        assertEquals(0, valores.first())
        assertTrue(valores.contains(SERVICIOS_REFRESCO_SEG_MIN))
        assertTrue(valores.contains(SERVICIOS_REFRESCO_SEG_MAX))
        assertTrue(
            valores.drop(1).all { it in SERVICIOS_REFRESCO_SEG_MIN..SERVICIOS_REFRESCO_SEG_MAX },
        )
    }

    @Test
    fun laEsperaDeServiciosNoOfreceApagado() {
        val valores = valoresEsperaServicios()
        assertFalse(valores.contains(0))
        assertEquals(SERVICIOS_ESPERA_SEG_MIN, valores.first())
        assertEquals(SERVICIOS_ESPERA_SEG_MAX, valores.last())
    }

    @Test
    fun losUmbralesQueSeEnsenanSonLosDelEscritorio() {
        assertEquals(0.1, CARGA_POR_NUCLEO_MIN, 0.0001)
        assertEquals(16.0, CARGA_POR_NUCLEO_MAX, 0.0001)
        assertEquals("1,0", unDecimal(1.0).replace('.', ','))
        assertEquals("90", sinDecimales(90.0))
    }

    @Test
    fun alternarMotoresLosDejaSinRepetirYEnSuOrden() {
        assertEquals(listOf("docker"), alternarMotor(emptyList(), "docker", true))
        assertEquals(
            listOf("docker", "podman"),
            alternarMotor(listOf("podman"), "docker", true),
        )
        assertEquals(listOf("docker"), alternarMotor(listOf("docker", "podman"), "podman", false))
        assertEquals(emptyList<String>(), alternarMotor(listOf("docker"), "docker", false))
        assertEquals(
            listOf("docker", "podman"),
            alternarMotor(listOf("docker", "docker", "podman"), "docker", true),
        )
    }
}
