package org.cuatrodetres.magi.ui.modelos

import androidx.compose.ui.graphics.Color
import java.time.ZoneId
import org.cuatrodetres.magi.ui.terminal.colorDe
import org.cuatrodetres.magi.ui.terminal.colorIndice
import org.cuatrodetres.magi.ui.tema.Paleta
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Formatos de tiempo/versión y tabla de colores de xterm (§6.7). */
class PruebaFormatosYColores {

    @Test
    fun tiempoRelativoPorUnidades() {
        assertEquals(TiempoRelativo(3, UnidadTemporal.SEGUNDOS), tiempoRelativo(100, 103))
        assertEquals(TiempoRelativo(1, UnidadTemporal.MINUTOS), tiempoRelativo(0, 60))
        assertEquals(TiempoRelativo(2, UnidadTemporal.HORAS), tiempoRelativo(0, 7200))
        assertEquals(TiempoRelativo(1, UnidadTemporal.DIAS), tiempoRelativo(0, 90000))
        assertNull(tiempoRelativo(null, 0))
    }

    @Test
    fun fechaYHuellaCorta() {
        assertEquals("01/01/1970 00:00", formatoFecha(0, ZoneId.of("UTC")))
        assertNull(formatoFecha(null))
        assertEquals("abcd·ijkl", huellaCorta("SHA256:abcdefghijkl"))
        assertEquals("0.1.0 (1009)", textoVersion("0.1.0", 1009))
    }

    @Test
    fun laTablaDe256ColoresSigueXterm() {
        assertEquals(Paleta.TERMINAL[0], colorIndice(0))
        assertEquals(Paleta.TERMINAL[15], colorIndice(15))
        assertEquals(Color(0, 0, 0), colorIndice(16))
        assertEquals(Color(255, 255, 255), colorIndice(231))
        assertEquals(Color(8, 8, 8), colorIndice(232))
        assertEquals(Color(238, 238, 238), colorIndice(255))
    }

    @Test
    fun losColoresDeLosTramos() {
        assertEquals(Paleta.TEXTO, colorDe(ColorVista.Predeterminado))
        assertEquals(Color(0x12, 0x34, 0x56), colorDe(ColorVista.Rgb(0x12, 0x34, 0x56)))
        assertEquals(Paleta.TERMINAL[3], colorDe(ColorVista.Indice(3)))
    }
}
