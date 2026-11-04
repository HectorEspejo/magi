package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Reglas puras de la carpeta SAF (T81), probadas en la JVM. */
class PruebaReglasCarpeta {

    private val uuid = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b"
    private val nombre = "magi-" + uuid + ".age"

    @Test
    fun aceptaElNombreCanonico() {
        assertTrue(ReglasCarpeta.nombreValido(nombre))
    }

    @Test
    fun rechazaElUuidEnMayusculas() {
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid.uppercase() + ".age"))
    }

    @Test
    fun rechazaElUuidSinGuiones() {
        val sinGuiones = uuid.replace("-", "")
        assertFalse(ReglasCarpeta.nombreValido("magi-" + sinGuiones + ".age"))
    }

    @Test
    fun rechazaCopiasEnConflictoDeSyncthing() {
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid + ".sync-conflict-20260101-000000.age"))
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid + ".age.tmp"))
        assertFalse(ReglasCarpeta.nombreValido(".magi-" + uuid + ".age.tmp"))
    }

    @Test
    fun rechazaNombresConRutasYSinExtension() {
        assertFalse(ReglasCarpeta.nombreValido("../magi-" + uuid + ".age"))
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid))
        assertFalse(ReglasCarpeta.nombreValido("otro-" + uuid + ".age"))
        assertFalse(ReglasCarpeta.nombreValido(""))
    }

    @Test
    fun rechazaSubcarpetasYOtrasExtensiones() {
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid))
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid + ".AGE"))
        assertFalse(ReglasCarpeta.nombreValido("magi-" + uuid + ".age/"))
    }

    @Test
    fun elTopeDeTamanoEsInclusivo() {
        assertTrue(ReglasCarpeta.tamanoValido(0L))
        assertTrue(ReglasCarpeta.tamanoValido(ReglasCarpeta.TAMANO_MAXIMO))
        assertFalse(ReglasCarpeta.tamanoValido(ReglasCarpeta.TAMANO_MAXIMO + 1L))
        assertFalse(ReglasCarpeta.tamanoValido(-1L))
    }

    @Test
    fun filtrarDescartaYOrdena() {
        val validoA = FicheroCarpeta("magi-11111111-1111-4111-8111-111111111111.age", 10L, 0L)
        val validoB = FicheroCarpeta("magi-22222222-2222-4222-8222-222222222222.age", 20L, 0L)
        val grande = FicheroCarpeta(
            "magi-33333333-3333-4333-8333-333333333333.age",
            ReglasCarpeta.TAMANO_MAXIMO + 1L,
            0L,
        )
        val conflicto = FicheroCarpeta(
            "magi-44444444-4444-4444-8444-444444444444.sync-conflict.age",
            10L,
            0L,
        )
        val sinTamano = FicheroCarpeta("magi-55555555-5555-4555-8555-555555555555.age", -1L, 0L)
        val filtrados = ReglasCarpeta.filtrar(listOf(validoB, grande, conflicto, validoA, sinTamano))
        assertEquals(listOf(validoA.nombre, validoB.nombre), filtrados.map { it.nombre })
    }

    @Test
    fun elTopeCoincideConElDelNucleo() {
        // carpeta::TAMANO_MAXIMO del núcleo (T81): 16 MiB.
        assertEquals(16L * 1024L * 1024L, ReglasCarpeta.TAMANO_MAXIMO)
    }
}
