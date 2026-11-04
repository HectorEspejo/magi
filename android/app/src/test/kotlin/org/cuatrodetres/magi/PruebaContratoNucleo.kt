package org.cuatrodetres.magi

import org.cuatrodetres.magi.nucleo.NucleoRepositorio
import org.junit.Assert.assertEquals
import org.junit.Test

/** Contrato con Gradle y con el crate magi-movil (Fase 11). */
class PruebaContratoNucleo {

    @Test
    fun laBibliotecaNativaSeLlamaMagiMovil() {
        assertEquals("magi_movil", NucleoRepositorio.BIBLIOTECA)
    }

    @Test
    fun losBindingsVanAlPaqueteDelContrato() {
        assertEquals("org.cuatrodetres.magi.nucleo", NucleoRepositorio.PAQUETE_BINDINGS)
        assertEquals(NucleoRepositorio.PAQUETE_BINDINGS, NucleoRepositorio::class.java.packageName)
    }

    @Test
    fun laPlataformaDeclaradaEsAndroid() {
        assertEquals("android", NucleoRepositorio.PLATAFORMA)
    }

    @Test
    fun elNucleoEmpiezaSinIniciar() {
        assertEquals(NucleoRepositorio.Estado.SIN_INICIAR, NucleoRepositorio.estado)
    }
}
