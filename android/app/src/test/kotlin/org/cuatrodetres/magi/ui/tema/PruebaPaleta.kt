package org.cuatrodetres.magi.ui.tema

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Test

/** La paleta de la cabina es fija (informe §6.7): un cambio aquí es un error. */
class PruebaPaleta {

    @Test
    fun losColoresDeLaCabinaSonLosFijados() {
        assertEquals(Color(0xFF0A0A0A), Paleta.FONDO)
        assertEquals(Color(0xFFEDE8DC), Paleta.TEXTO)
        assertEquals(Color(0xFFFFA000), Paleta.AMBAR)
        assertEquals(Color(0xFF00FF6A), Paleta.VERDE)
        assertEquals(Color(0xFFFF2A1A), Paleta.ROJO)
        assertEquals(Color(0xFF5A5A5A), Paleta.GRIS)
    }

    @Test
    fun laPaletaDelTerminalTieneDieciseisColores() {
        assertEquals(16, Paleta.TERMINAL.size)
        assertEquals(Color(0xFF1C1C1C), Paleta.TERMINAL[0])
        assertEquals(Color(0xFFEDE8DC), Paleta.TERMINAL[7])
        assertEquals(Color(0xFF5A5A5A), Paleta.TERMINAL[8])
        assertEquals(Color(0xFFFFFFFF), Paleta.TERMINAL[15])
    }
}
