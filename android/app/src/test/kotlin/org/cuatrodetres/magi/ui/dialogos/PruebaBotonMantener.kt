package org.cuatrodetres.magi.ui.dialogos

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pruebas de la máquina de «mantener pulsado» (informe §7.8) con un reloj
 * falso: soltar antes, completar a 1 s, una sola vez y TalkBack.
 */
class PruebaBotonMantener {

    private var ahora: Long = 0L

    private fun maquina() = MaquinaBotonMantener { ahora }

    @Test
    fun soltarAntesDeTiempoNoEjecutaYLoDice() {
        val maquina = maquina()
        assertEquals(EventoMantener.EMPEZADO, maquina.alPulsar())
        assertTrue(maquina.pulsando)
        ahora = 500
        assertEquals(EventoMantener.NADA, maquina.alFotograma())
        assertEquals(0.5f, maquina.progreso, 0.001f)
        assertEquals(EventoMantener.SOLTADO_ANTES, maquina.alSoltar())
        assertFalse(maquina.pulsando)
        assertFalse(maquina.disparado)
        assertEquals(0f, maquina.progreso, 0.001f)
        // Soltar sin estar pulsando no hace nada.
        assertEquals(EventoMantener.NADA, maquina.alSoltar())
    }

    @Test
    fun mantenerUnSegundoCompletaYNoSeRepite() {
        val maquina = maquina()
        assertEquals(EventoMantener.EMPEZADO, maquina.alPulsar())
        ahora = 999
        assertEquals(EventoMantener.NADA, maquina.alFotograma())
        assertFalse(maquina.disparado)
        ahora = 1_000
        assertEquals(EventoMantener.CUMPLIDO, maquina.alFotograma())
        assertTrue(maquina.disparado)
        assertEquals(1f, maquina.progreso, 0.001f)
        // Una sola vez: ni más fotogramas, ni volver a pulsar, ni soltar.
        ahora = 2_000
        assertEquals(EventoMantener.NADA, maquina.alFotograma())
        assertEquals(EventoMantener.NADA, maquina.alPulsar())
        assertEquals(EventoMantener.NADA, maquina.alSoltar())
    }

    @Test
    fun soltarJustoAlCumplirseNoAvisaDeNada() {
        val maquina = maquina()
        assertEquals(EventoMantener.EMPEZADO, maquina.alPulsar())
        ahora = 1_000
        assertEquals(EventoMantener.CUMPLIDO, maquina.alFotograma())
        // El dedo se levanta después de cumplirse: no es «soltar antes».
        assertEquals(EventoMantener.NADA, maquina.alSoltar())
        assertTrue(maquina.disparado)
    }

    @Test
    fun talkbackEjecutaUnaSolaVezConLaAccionDePulsacionLarga() {
        val maquina = maquina()
        // TalkBack toca dos veces y mantiene: la máquina lo resuelve de una vez.
        assertEquals(EventoMantener.CUMPLIDO, maquina.porAccesibilidad())
        assertTrue(maquina.disparado)
        assertEquals(1f, maquina.progreso, 0.001f)
        assertEquals(EventoMantener.NADA, maquina.porAccesibilidad())
        assertEquals(EventoMantener.NADA, maquina.alPulsar())
    }

    @Test
    fun reiniciarDejaElBotonListoOtraVez() {
        val maquina = maquina()
        assertEquals(EventoMantener.EMPEZADO, maquina.alPulsar())
        ahora = 1_000
        assertEquals(EventoMantener.CUMPLIDO, maquina.alFotograma())
        maquina.reiniciar()
        assertFalse(maquina.disparado)
        assertEquals(0f, maquina.progreso, 0.001f)
        ahora = 1_100
        assertEquals(EventoMantener.EMPEZADO, maquina.alPulsar())
        ahora = 2_100
        assertEquals(EventoMantener.CUMPLIDO, maquina.alFotograma())
    }
}
