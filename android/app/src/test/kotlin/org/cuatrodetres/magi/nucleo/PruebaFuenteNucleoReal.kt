package org.cuatrodetres.magi.nucleo

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.preferencias.PreferenciasMagi
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAperturaVista
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

/**
 * Fachada real del núcleo con un doble de NucleoInterface: sin .so, sin
 * Android y sin Keystore, para probar la conversión, el arranque y los flujos.
 */
class PruebaFuenteNucleoReal {

    // -----------------------------------------------------------------------
    // Doble del núcleo
    // -----------------------------------------------------------------------

    private class NucleoFalso : NucleoInterface {
        var falloAlAbrir: Throwable? = null
        var falloAlPlegar: Throwable? = null
        var versionLlamada: Int = 0
        var responder: Pair<ULong, RespuestaDialogo>? = null
        var pegandoEntreCorchetes: Boolean = true
        var hosts: List<HostInventario> = listOf(
            HostInventario(
                uuid = "h1",
                nombre = "web-01",
                direccion = "192.0.2.10",
                puerto = 22U,
                usuario = "usuario",
                grupoUuid = null,
                etiquetas = listOf("web"),
                estado = EstadoHost.SIN_SESION,
                sesionesAbiertas = 0U,
                ultimoError = null,
                ultimaConexionEn = null,
            ),
        )

        override suspend fun abrirSesion(hostUuid: String): ULong {
            falloAlAbrir?.let { throw it }
            return 7UL
        }

        override suspend fun aceptarHuellaDeOtroEquipo(id: ULong) = Unit

        override suspend fun activarEnPuertoLibre(uuid: String) = Unit

        override suspend fun activarTunel(uuid: String) = Unit

        override suspend fun borrarDatos(alcance: AlcanceBorrado) = Unit

        override fun cerrarSesion(sesion: ULong) = Unit

        override fun clavePublicaMovil(): ClavePublicaMovil = ClavePublicaMovil(
            lineaOpenssh = "ecdsa-sha2-nistp256 AAAA magi@móvil",
            huellaSha256 = "SHA256:abc",
        )

        override fun conflictosHuella(): List<ConflictoHuella> = emptyList()

        override suspend fun dejarDeSincronizar() = Unit

        override fun desplazar(sesion: ULong, filas: Int) = Unit

        override suspend fun elegirCarpeta(uri: String) = Unit

        override fun escribir(sesion: ULong, bytes: ByteArray) = Unit

        override fun fichaHost(uuid: String): FichaHost = throw ErrorNucleo.NoEncontrado("sin ficha")

        override suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionClave =
            ResultadoInstalacionClave.YA_ESTABA

        override fun inventarioGrupos(): List<GrupoInventario> = emptyList()

        override fun inventarioHosts(filtro: String?): List<HostInventario> = hosts

        override suspend fun mantenerMiHuella(id: ULong) = Unit

        override fun olvidarContrasena(hostUuid: String): Boolean = true

        override fun pararTunel(uuid: String) = Unit

        override fun pegadoEntreCorchetes(sesion: ULong): Boolean = pegandoEntreCorchetes

        override fun pegar(sesion: ULong, texto: String) = Unit

        override fun plegarGrupo(uuid: String, plegado: Boolean) {
            falloAlPlegar?.let { throw it }
        }

        override suspend fun reconectar(sesion: ULong) = Unit

        override fun redimensionar(sesion: ULong, columnas: UInt, filas: UInt) = Unit

        override suspend fun relanzarTunel(uuid: String) = Unit

        override fun responderDialogo(id: ULong, respuesta: RespuestaDialogo) {
            responder = id to respuesta
        }

        override fun sincronizacionEstado(): EstadoSincronizacion = EstadoSincronizacion(
            fase = FaseSincronizacion.ACTIVA,
            carpetaUri = "content://carpeta",
            carpetaNombre = "Sync/magi",
            huellaCortaClave = "7f3a·91c2",
            ultimaPasada = 1_700_000_000L,
            equipos = emptyList(),
            conflictosHuella = 0U,
            error = null,
        )

        override suspend fun sincronizarAhora() = Unit

        override fun tecla(sesion: ULong, tecla: Tecla, modificadores: Modificadores) = Unit

        override fun textoSeleccion(sesion: ULong, desde: Posicion, hasta: Posicion): String = ""

        override fun tuneles(hostUuid: String?): List<TunelVivo> = emptyList()

        override suspend fun unirConClave(texto: String) = Unit

        override fun version(): VersionNucleo {
            versionLlamada += 1
            return VersionNucleo(nucleo = "nucleo-x", formato = 1U, userVersion = 7U)
        }
    }

    // -----------------------------------------------------------------------
    // Ayudas
    // -----------------------------------------------------------------------

    private fun almacenNuevo(): DataStore<Preferences> {
        val directorio = Files.createTempDirectory("magi-nucleo").toFile()
        directorio.deleteOnExit()
        return PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(directorio, "magi_ajustes.preferences_pb") },
        )
    }

    private fun preferencias(): PreferenciasMagi = PreferenciasMagi(almacenNuevo())

    private fun fuente(
        nucleo: NucleoFalso,
        bloqueoDePantalla: Boolean = true,
        preferencias: PreferenciasMagi = preferencias(),
        infoClave: () -> InfoClavePlataforma? = { null },
        alIniciar: (OyenteNucleo) -> Unit = {},
        servicio: MutableList<Pair<Int, Int>> = mutableListOf(),
        vecesIniciado: () -> Unit = {},
    ): FuenteNucleoReal = FuenteNucleoReal(
        iniciarNucleo = { oyente ->
            vecesIniciado()
            alIniciar(oyente)
            nucleo
        },
        preferencias = preferencias,
        hayBloqueoDePantalla = { bloqueoDePantalla },
        infoClave = infoClave,
        actualizarServicio = { sesiones, tuneles -> servicio += sesiones to tuneles },
    )

    private fun sesion(id: Long, hostUuid: String, hostNombre: String, estado: EstadoSesion): Sesion =
        Sesion(
            id = id.toULong(),
            hostUuid = hostUuid,
            hostNombre = hostNombre,
            estado = estado,
            detalle = null,
            abiertaEn = null,
        )

    private fun tunel(estado: EstadoTunel): TunelVivo = TunelVivo(
        uuid = "t-" + estado.name,
        hostUuid = "h1",
        hostNombre = "web-01",
        nombre = "grafana",
        tipo = TipoTunel.LOCAL,
        escucha = "127.0.0.1:3000",
        escuchaAviso = false,
        destino = "127.0.0.1:3000",
        automatico = false,
        estado = estado,
        error = null,
        conexiones = 0U,
        bytesSubida = 0UL,
        bytesBajada = 0UL,
        puertoEfectivo = null,
    )

    // -----------------------------------------------------------------------
    // Pruebas
    // -----------------------------------------------------------------------

    @Test
    fun sinBloqueoDePantallaNoArranca() = runBlocking {
        var arrancado = false
        val fuente = fuente(NucleoFalso(), bloqueoDePantalla = false, alIniciar = { arrancado = true })
        val resultado = fuente.iniciar()
        assertTrue(resultado is ResultadoAccion.Error)
        assertEquals(
            FuenteNucleoReal.SIN_BLOQUEO_PANTALLA,
            (resultado as ResultadoAccion.Error).mensaje,
        )
        assertFalse(arrancado)
    }

    @Test
    fun iniciarEsIdempotenteYConectaLosFlujos() = runBlocking {
        var veces = 0
        var oyente: OyenteNucleo? = null
        val servicio = mutableListOf<Pair<Int, Int>>()
        val fuente = fuente(
            NucleoFalso(),
            alIniciar = { oyente = it },
            servicio = servicio,
            vecesIniciado = { veces += 1 },
        )
        assertEquals(ResultadoAccion.Ok, fuente.iniciar())
        assertEquals(ResultadoAccion.Ok, fuente.iniciar())
        assertEquals(1, veces)

        oyente!!.sesiones(
            listOf(
                sesion(1L, "h1", "web-01", EstadoSesion.ABIERTA),
                sesion(2L, "h1", "web-01", EstadoSesion.ABIERTA),
                sesion(3L, "h2", "db-01", EstadoSesion.ABRIENDO),
            ),
        )
        assertEquals(
            listOf("web-01", "web-01 (2)", "db-01"),
            fuente.sesiones.value.map { it.titulo },
        )
        assertEquals(3, servicio.last().first)
        assertEquals(0, servicio.last().second)

        oyente.tuneles(
            listOf(
                tunel(EstadoTunel.ACTIVO),
                tunel(EstadoTunel.INACTIVO),
                tunel(EstadoTunel.ACTIVANDO),
            ),
        )
        assertEquals(2, servicio.last().second)
        assertEquals(3, servicio.last().first)
        assertEquals(3, fuente.tuneles.value.size)

        oyente.sincronizacion(
            EstadoSincronizacion(
                fase = FaseSincronizacion.SINCRONIZANDO,
                carpetaUri = null,
                carpetaNombre = null,
                huellaCortaClave = null,
                ultimaPasada = null,
                equipos = emptyList(),
                conflictosHuella = 0U,
                error = null,
            ),
        )
        assertEquals(
            org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SINCRONIZANDO,
            fuente.sincronizacion.value.fase,
        )

        oyente.dialogo(
            PeticionDialogo.Contrasena(
                id = 9UL,
                hostUuid = "h1",
                hostNombre = "web-01",
                usuario = "usuario",
                direccion = "192.0.2.10:22",
                intentosRestantes = 3U,
                recordarOfrecido = true,
            ),
        )
        val peticion = fuente.peticiones.value
        assertEquals(9L, peticion?.peticionId)
        assertEquals(
            ResultadoAccion.Ok,
            fuente.responderDialogo(9L, RespuestaDialogoVista.CancelarContrasena),
        )
        assertNull(fuente.peticiones.value)
    }

    @Test
    fun inventarioYVersionLleganDeLaNucleo() = runBlocking {
        val nucleo = NucleoFalso()
        val fuente = fuente(nucleo)
        fuente.iniciar()
        val inventario = fuente.inventario(filtro = null)
        assertEquals(1, inventario.hosts.size)
        assertEquals("web-01", inventario.hosts.first().nombre)
        val version = fuente.version()
        assertEquals("nucleo-x", version?.nucleo)
        assertEquals(7, version?.userVersion)
        assertEquals(1, nucleo.versionLlamada)
    }

    @Test
    fun abrirSesionDevuelveElIdOMensaje() = runBlocking {
        val nucleo = NucleoFalso()
        val fuente = fuente(nucleo)
        fuente.iniciar()
        assertEquals(ResultadoAperturaVista.Abierta(7L), fuente.abrirSesion("h1"))
        nucleo.falloAlAbrir = ErrorNucleo.General("no se pudo conectar")
        assertEquals(ResultadoAperturaVista.Error("no se pudo conectar"), fuente.abrirSesion("h1"))
    }

    @Test
    fun cambiarElZoomSaneaYPersiste() = runBlocking {
        val preferencias = preferencias()
        val fuente = fuente(NucleoFalso(), preferencias = preferencias)
        fuente.iniciar()
        assertEquals(ResultadoAccion.Ok, fuente.cambiarTamanoLetra(99))
        assertEquals(24, fuente.tamanoLetra.value)
        assertEquals(24, preferencias.primeraLectura().tamanoLetra)
        assertEquals(ResultadoAccion.Ok, fuente.cambiarTamanoLetra(1))
        assertEquals(8, fuente.tamanoLetra.value)
    }

    @Test
    fun sinNucleoLasAccionesDevuelvenErrorYLasConsultasVacio() = runBlocking {
        val fuente = fuente(NucleoFalso(), bloqueoDePantalla = true)
        val resultado = fuente.plegarGrupo("g1", true)
        assertTrue(resultado is ResultadoAccion.Error)
        assertEquals(FuenteNucleoReal.SIN_NUCLEO, (resultado as ResultadoAccion.Error).mensaje)
        assertTrue(fuente.tuneles(null).isEmpty())
        assertTrue(fuente.conflictosHuella().isEmpty())
        assertNull(fuente.version())
        assertTrue(fuente.abrirSesion("h1") is ResultadoAperturaVista.Error)
        assertNull(fuente.clavePublicaMovil())
    }

    @Test
    fun losErroresDeLasAccionesSonLegibles() = runBlocking {
        val nucleo = NucleoFalso()
        val fuente = fuente(nucleo)
        fuente.iniciar()
        nucleo.falloAlPlegar = ErrorNucleo.General("grupo inexistente")
        val resultado = fuente.plegarGrupo("g1", true)
        assertEquals(ResultadoAccion.Error("grupo inexistente"), resultado)
    }

    @Test
    fun elPegadoEntreCorchetesYLaClaveDelMovilSeConsultan() = runBlocking {
        val nucleo = NucleoFalso()
        val fuente = fuente(
            nucleo,
            infoClave = {
                InfoClavePlataforma(
                    ubicacion = UbicacionClaveVista.STRONGBOX,
                    creadaEnMs = 1_700_000_000_000L,
                    invalidada = false,
                )
            },
        )
        fuente.iniciar()
        assertTrue(fuente.pegadoEntreCorchetes(1L))
        nucleo.pegandoEntreCorchetes = false
        assertFalse(fuente.pegadoEntreCorchetes(1L))
        val clave = fuente.clavePublicaMovil()
        assertEquals(UbicacionClaveVista.STRONGBOX, clave?.ubicacion)
        assertEquals(1_700_000_000L, clave?.creadaEn)
    }

    @Test
    fun unFalloAlIniciarSeCuentaComoError() = runBlocking {
        val fuente = FuenteNucleoReal(
            iniciarNucleo = { throw ErrorNucleo.General("no se pudo abrir magi.db") },
            preferencias = preferencias(),
            hayBloqueoDePantalla = { true },
            infoClave = { null },
            actualizarServicio = { _, _ -> },
        )
        val resultado = fuente.iniciar()
        assertEquals(ResultadoAccion.Error("no se pudo abrir magi.db"), resultado)
    }

    @Test
    fun lasSesionesSinNucleoNoTienenPantalla() {
        assertNull(fuente(NucleoFalso()).pantallas.value)
        assertTrue(fuente(NucleoFalso()).sesiones.value.isEmpty())
    }
}
