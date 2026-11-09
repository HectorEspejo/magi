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
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
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

    // -----------------------------------------------------------------------
    // Fase 14: ajustes de Flota, Servicios y Deliberación (informe §5.4)
    // -----------------------------------------------------------------------

    @Test
    fun losAjustesMovilesPorDefectoSonLosDelEscritorio() = runBlocking {
        val ajustes = PreferenciasMagi(almacenNuevo()).primeraLectura().ajustesMovil
        assertEquals(30, ajustes.flotaRefrescoSeg)
        assertEquals(1.0, ajustes.umbralCargaPorNucleo, 0.0001)
        assertEquals(90.0, ajustes.umbralMemoriaPct, 0.0001)
        assertEquals(90.0, ajustes.umbralDiscoPct, 0.0001)
        assertEquals(10, ajustes.serviciosRefrescoSeg)
        assertEquals(30, ajustes.serviciosEsperaSeg)
        assertEquals(listOf("docker", "podman"), ajustes.serviciosContenedores)
        assertEquals(5, ajustes.sudoMemoriaMin)
        assertEquals(24, ajustes.deliberacionBackupHoras)
        assertEquals(5, ajustes.deliberacionSaludMaxMin)
    }

    @Test
    fun guardaYLeeLosAjustesMovilesAcotados() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        val nuevos = AjustesMovilVista(
            flotaRefrescoSeg = 60,
            umbralCargaPorNucleo = 2.5,
            umbralMemoriaPct = 80.0,
            umbralDiscoPct = 85.0,
            serviciosRefrescoSeg = 20,
            serviciosEsperaSeg = 45,
            serviciosContenedores = listOf("podman"),
            sudoMemoriaMin = 10,
            deliberacionBackupHoras = 48,
            deliberacionSaludMaxMin = 30,
        )
        assertTrue(preferencias.guardarAjustesMovil(nuevos).isEmpty())
        assertEquals(nuevos, preferencias.primeraLectura().ajustesMovil)
    }

    @Test
    fun guardarAjustesMovilesAcotaYDevuelveLosAvisos() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        val fueraDeRango = AjustesMovilVista(
            flotaRefrescoSeg = 20,
            umbralCargaPorNucleo = 99.0,
            umbralMemoriaPct = -5.0,
            serviciosEsperaSeg = 1,
            serviciosContenedores = listOf("docker", "docker", "podman", "otro"),
            sudoMemoriaMin = 99,
            deliberacionBackupHoras = 0,
            deliberacionSaludMaxMin = 999_999,
        )
        val avisos = preferencias.guardarAjustesMovil(fueraDeRango)
        assertTrue(avisos.isNotEmpty())
        val leidos = preferencias.primeraLectura().ajustesMovil
        assertEquals(20, leidos.flotaRefrescoSeg)
        assertEquals(16.0, leidos.umbralCargaPorNucleo, 0.0001)
        assertEquals(0.0, leidos.umbralMemoriaPct, 0.0001)
        assertEquals(5, leidos.serviciosEsperaSeg)
        assertEquals(listOf("docker", "podman"), leidos.serviciosContenedores)
        assertEquals(15, leidos.sudoMemoriaMin)
        assertEquals(1, leidos.deliberacionBackupHoras)
        assertEquals(1_440, leidos.deliberacionSaludMaxMin)
    }

    @Test
    fun aAjustesMovilSaneaLoQueLlegueDeDisco() {
        val crudas = preferencesOf(
            ReglasPreferencias.CLAVE_FLOTA_REFRESCO_SEG to 20,
            ReglasPreferencias.CLAVE_UMBRAL_CARGA_POR_NUCLEO to 99.0,
            ReglasPreferencias.CLAVE_SERVICIOS_CONTENEDORES to "podman, podman, rkt",
            ReglasPreferencias.CLAVE_SUDO_MEMORIA_MIN to -4,
        )
        val ajustes = ReglasPreferencias.aAjustesMovil(crudas)
        assertEquals(20, ajustes.flotaRefrescoSeg)
        assertEquals(16.0, ajustes.umbralCargaPorNucleo, 0.0001)
        assertEquals(listOf("podman"), ajustes.serviciosContenedores)
        assertEquals(0, ajustes.sudoMemoriaMin)
    }

    // -----------------------------------------------------------------------
    // Fase 14: trabajo que Android cerró con MAGI (informe §7.12)
    // -----------------------------------------------------------------------

    @Test
    fun guardaYConsumeElTrabajoAbierto() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        preferencias.guardarTrabajoAbierto(sesiones = 2, acciones = 1, ejecuciones = 3)
        assertEquals(2, preferencias.consumirSesionesAbiertas())
        assertEquals(1, preferencias.consumirAccionesAbiertas())
        assertEquals(3, preferencias.consumirEjecucionesAbiertas())
        // Ya consumido: no se avisa dos veces.
        assertEquals(0, preferencias.consumirSesionesAbiertas())
        assertEquals(0, preferencias.consumirAccionesAbiertas())
        assertEquals(0, preferencias.consumirEjecucionesAbiertas())
    }

    @Test
    fun elTrabajoAbiertoSaneaNegativosYDesmedidos() = runBlocking {
        val preferencias = PreferenciasMagi(almacenNuevo())
        preferencias.guardarTrabajoAbierto(
            sesiones = -3,
            acciones = ReglasPreferencias.TRABAJO_ABIERTO_MAX + 50,
            ejecuciones = -1,
        )
        assertEquals(0, preferencias.consumirSesionesAbiertas())
        assertEquals(
            ReglasPreferencias.TRABAJO_ABIERTO_MAX,
            preferencias.consumirAccionesAbiertas(),
        )
        assertEquals(0, preferencias.consumirEjecucionesAbiertas())
    }
}
