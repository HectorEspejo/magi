package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Nombres de clave del llavero: nada de rutas ni nombres raros. */
class PruebaReglasLlavero {

    @Test
    fun aceptaLasClavesQueUsaMagi() {
        assertTrue(ReglasLlavero.claveValida("sincronizacion"))
        assertTrue(ReglasLlavero.claveValida("sincronizacion_anterior"))
        assertTrue(ReglasLlavero.claveValida("2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b"))
        assertTrue(ReglasLlavero.claveValida("host:2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b"))
    }

    @Test
    fun rechazaRutasYNombresOcultos() {
        assertFalse(ReglasLlavero.claveValida(""))
        assertFalse(ReglasLlavero.claveValida(".."))
        assertFalse(ReglasLlavero.claveValida(".oculto"))
        assertFalse(ReglasLlavero.claveValida("../fuera"))
        assertFalse(ReglasLlavero.claveValida("/etc/passwd"))
        assertFalse(ReglasLlavero.claveValida("con espacio"))
        assertFalse(ReglasLlavero.claveValida("con\barra"))
        assertFalse(ReglasLlavero.claveValida("ñandú"))
    }

    @Test
    fun rechazaNombresDemasiadoLargos() {
        assertFalse(ReglasLlavero.claveValida("a".repeat(129)))
        assertTrue(ReglasLlavero.claveValida("a".repeat(128)))
    }

    @Test
    fun elNombreDelFicheroEsLaClaveConExtension() {
        assertEquals("sincronizacion.enc", ReglasLlavero.nombreDeFichero("sincronizacion"))
        assertNull(ReglasLlavero.nombreDeFichero("../fuera"))
    }
}
