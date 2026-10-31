package org.cuatrodetres.magi.preferencias

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.preferencesOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

/**
 * DataStore de los ajustes de MAGI, probado en la JVM con un fichero temporal
 * (el DataStore real de Android es el mismo código de androidx.datastore).
 */
class PruebaPreferencias {

    private fun almacenNuevo(): DataStore<Preferences> {
        val directorio = Files.createTempDirectory("magi-preferencias").toFile()
        directorio.deleteOnExit()
        return PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(directorio, "magi_ajustes.preferences_pb") },
        )
    }

    @Test
    fun losValoresPorDefecto() = runBlocking {
        val ajustes = PreferenciasMagi(almacenNuevo()).primeraLectura()
        assertNull(ajustes.carpetaUri)
        assertEquals(ReglasPreferencias.TAMANO_LETRA_DEFECTO, ajustes.tamanoLetra)
        assertNull(ajustes.claveAnteriorHasta)
        assertEquals(0, ajustes.sesionesAbiertas)
        assertFalse(ajustes.guiaBateriaVista)
    }

    @Test
    fun guardaYLeeLosAjustes() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        val uri = "content://com.android.externalstorage.documents/tree/primary%3ASync"
        preferencias.guardarCarpetaUri(uri)
        preferencias.guardarTamanoLetra(20)
        preferencias.guardarClaveAnteriorHasta(1_700_000_000L)
        preferencias.guardarSesionesAbiertas(3)
        preferencias.marcarGuiaBateriaVista()

        val ajustes = preferencias.ajustes.first()
        assertEquals(uri, ajustes.carpetaUri)
        assertEquals(20, ajustes.tamanoLetra)
        assertEquals(1_700_000_000L, ajustes.claveAnteriorHasta)
        assertEquals(3, ajustes.sesionesAbiertas)
        assertTrue(ajustes.guiaBateriaVista)
    }

    @Test
    fun losSettersSaneanYSePuedenBorrar() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        preferencias.guardarTamanoLetra(1)
        preferencias.guardarSesionesAbiertas(-5)
        preferencias.guardarCarpetaUri("  ")
        preferencias.guardarClaveAnteriorHasta(0L)
        preferencias.marcarGuiaBateriaVista(true)

        val ajustes = preferencias.ajustes.first()
        assertEquals(ReglasPreferencias.TAMANO_LETRA_MIN, ajustes.tamanoLetra)
        assertEquals(0, ajustes.sesionesAbiertas)
        assertNull(ajustes.carpetaUri)
        assertNull(ajustes.claveAnteriorHasta)
        assertTrue(ajustes.guiaBateriaVista)

        preferencias.guardarTamanoLetra(ReglasPreferencias.TAMANO_LETRA_MAX + 10)
        preferencias.guardarSesionesAbiertas(ReglasPreferencias.SESIONES_ABIERTAS_MAX + 10)
        val ajustes2 = preferencias.ajustes.first()
        assertEquals(ReglasPreferencias.TAMANO_LETRA_MAX, ajustes2.tamanoLetra)
        assertEquals(ReglasPreferencias.SESIONES_ABIERTAS_MAX, ajustes2.sesionesAbiertas)
    }

    @Test
    fun aAjustesSaneaLoQueLlegueDeDisco() {
        val crudas = preferencesOf(
            ReglasPreferencias.CLAVE_TAMANO_LETRA to 1,
            ReglasPreferencias.CLAVE_SESIONES_ABIERTAS to -3,
            ReglasPreferencias.CLAVE_CARPETA_URI to "   ",
            ReglasPreferencias.CLAVE_CLAVE_ANTERIOR_HASTA to 0L,
            ReglasPreferencias.CLAVE_GUIA_BATERIA_VISTA to true,
        )
        val ajustes = ReglasPreferencias.aAjustes(crudas)
        assertEquals(ReglasPreferencias.TAMANO_LETRA_MIN, ajustes.tamanoLetra)
        assertEquals(0, ajustes.sesionesAbiertas)
        assertNull(ajustes.carpetaUri)
        assertNull(ajustes.claveAnteriorHasta)
        assertTrue(ajustes.guiaBateriaVista)
    }

    @Test
    fun consumirSesionesAbiertasLasLeeYLasReinicia() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        preferencias.guardarSesionesAbiertas(2)
        assertEquals(2, preferencias.consumirSesionesAbiertas())
        assertEquals(0, preferencias.consumirSesionesAbiertas())
        assertEquals(0, preferencias.ajustes.first().sesionesAbiertas)
    }

    @Test
    fun elZoomPorDefectoEsTreceYElMargenEsDeOchoAVeinticuatro() {
        assertEquals(13, ReglasPreferencias.TAMANO_LETRA_DEFECTO)
        assertEquals(8, ReglasPreferencias.TAMANO_LETRA_MIN)
        assertEquals(24, ReglasPreferencias.TAMANO_LETRA_MAX)
        assertEquals(8, ReglasPreferencias.sanearTamanoLetra(0))
        assertEquals(24, ReglasPreferencias.sanearTamanoLetra(99))
    }
}
