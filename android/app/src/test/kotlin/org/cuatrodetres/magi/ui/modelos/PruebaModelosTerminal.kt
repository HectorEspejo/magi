package org.cuatrodetres.magi.ui.modelos

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pantalla por tramos, teclas, modificadores, zoom y selección (§6.3 y §7.7). */
class PruebaModelosTerminal {

    @Test
    fun losCaracteresAnchosOcupanDosCeldas() {
        assertEquals(1, anchoDeCaracter('a'.code))
        assertEquals(2, anchoDeCaracter(0x6F22)) // 漢
        assertEquals(2, anchoDeCaracter(0x1F600)) // emoji
        assertEquals(0, anchoDeCaracter(0x0301)) // acento combinante
        assertEquals(0, anchoDeCaracter(0x200B)) // centinela
    }

    @Test
    fun lasCeldasRespetanElAnchoDeLosCaracteres() {
        val celdas = celdasDeFila(listOf(TramoVista("a漢b")))
        assertEquals(listOf("a", "漢", "", "b"), celdas)
    }

    @Test
    fun fusionaFilasCambiadasSinGuardarHistorial() {
        val completa = PantallaVista(
            sesion = 1,
            columnas = 10,
            filas = 3,
            filasCambiadas = mapOf(0 to listOf(TramoVista("hola")), 1 to listOf(TramoVista("mundo"))),
            cursor = PosicionVista(2, 1),
            completa = true,
        )
        val parcial = PantallaVista(
            sesion = 1,
            columnas = 10,
            filas = 3,
            filasCambiadas = mapOf(1 to listOf(TramoVista("adios"))),
            cursor = PosicionVista(3, 2),
            desplazamiento = 4,
        )
        val fusionada = aplicarPantalla(completa, parcial)
        assertEquals(listOf(TramoVista("hola")), fusionada.filasCambiadas[0])
        assertEquals(listOf(TramoVista("adios")), fusionada.filasCambiadas[1])
        assertEquals(3, fusionada.cursor?.columna)
        assertEquals(4, fusionada.desplazamiento)
        assertFalse(fusionada.completa)
        assertEquals(3, filasDePantalla(fusionada).size)
    }

    @Test
    fun unaPantallaCompletaSustituyeLaAnterior() {
        val anterior = PantallaVista(1, 10, 2, mapOf(0 to listOf(TramoVista("viejo"))), completa = true)
        val nueva = PantallaVista(1, 10, 2, mapOf(0 to listOf(TramoVista("nuevo"))), completa = true)
        val fusionada = aplicarPantalla(anterior, nueva)
        assertEquals(listOf(TramoVista("nuevo")), fusionada.filasCambiadas[0])
    }

    @Test
    fun numeraLasSesionesDeCadaHost() {
        val sesiones = listOf(
            SesionVista(1, "a", "web-01", EstadoSesionVista.ABIERTA),
            SesionVista(2, "b", "db-01", EstadoSesionVista.ABIERTA),
            SesionVista(3, "a", "web-01", EstadoSesionVista.ABIERTA),
        )
        val numeradas = numerarSesiones(sesiones)
        assertEquals("web-01", numeradas[0].titulo)
        assertEquals("db-01", numeradas[1].titulo)
        assertEquals("web-01 (2)", numeradas[2].titulo)
    }

    @Test
    fun interpretaElTextoDelTecladoEnBruto() {
        val entradas = interpretarEntrada("ab\ncd\t")
        assertEquals(4, entradas.size)
        assertEquals(EntradaVista.Texto("ab"), entradas[0])
        assertEquals(EntradaVista.Pulsacion(TeclaVista.Intro), entradas[1])
        assertEquals(EntradaVista.Texto("cd"), entradas[2])
        assertEquals(EntradaVista.Pulsacion(TeclaVista.Tab), entradas[3])
    }

    @Test
    fun ctrlYAltSeEnciendenConUnaPulsacionYSeFijanConDos() {
        var modos = ModificadoresFila()
        modos = modos.alPulsarCtrl()
        assertEquals(ModoModificador.PENDIENTE, modos.ctrl)
        modos = modos.alPulsarCtrl()
        assertEquals(ModoModificador.FIJO, modos.ctrl)
        modos = modos.alPulsarCtrl()
        assertEquals(ModoModificador.NINGUNO, modos.ctrl)
        modos = modos.alPulsarAlt().alPulsarAlt()
        assertTrue(modos.altActivo)
        assertEquals(ModificadoresVista(ctrl = false, alt = true), modos.aModificadores())
        assertEquals(ModoModificador.NINGUNO, modos.alPulsarCtrl().trasTecla().ctrl)
        assertEquals(ModoModificador.FIJO, modos.trasTecla().alt)
    }

    @Test
    fun elZoomOchoVeinticuatro() {
        assertEquals(20, ajustarZoom(13, 1.5f))
        assertEquals(8, ajustarZoom(8, 0.1f))
        assertEquals(24, ajustarZoom(20, 4f))
        assertEquals(8, acotarZoom(1))
        assertEquals(24, acotarZoom(99))
        assertEquals(ZOOM_POR_DEFECTO, acotarZoom(ZOOM_POR_DEFECTO))
    }

    @Test
    fun seleccionaLaPalabraBajoLaCelda() {
        val pantalla = PantallaVista(
            sesion = 1,
            columnas = 20,
            filas = 1,
            filasCambiadas = mapOf(0 to listOf(TramoVista("hola mundo"))),
            completa = true,
        )
        val palabra = palabraEnCelda(pantalla, 0, 7)
        assertEquals(PosicionVista(5, 0), palabra?.desde)
        assertEquals(PosicionVista(9, 0), palabra?.hasta)
        val espacio = palabraEnCelda(pantalla, 0, 4)
        assertEquals(PosicionVista(4, 0), espacio?.desde)
        assertEquals(true, palabra?.contiene(0, 6))
        assertFalse(palabra!!.contiene(0, 4))
        assertNull(palabraEnCelda(pantalla, 5, 0))
    }

    @Test
    fun normalizaLaSeleccionEnOrdenDeLectura() {
        val seleccion = normalizarSeleccion(PosicionVista(5, 1), PosicionVista(2, 0))
        assertEquals(PosicionVista(2, 0), seleccion.desde)
        assertEquals(PosicionVista(5, 1), seleccion.hasta)
        assertTrue(seleccion.contiene(1, 3))
        assertFalse(seleccion.vacia)
    }
}
