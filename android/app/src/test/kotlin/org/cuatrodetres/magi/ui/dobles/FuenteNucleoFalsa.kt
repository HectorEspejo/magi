package org.cuatrodetres.magi.ui.dobles

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAperturaVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.VersionVista
import org.cuatrodetres.magi.ui.modelos.ZOOM_POR_DEFECTO

/**
 * Doble de [FuenteNucleo] para las pruebas de la JVM: datos en memoria y
 * llamadas registradas. Nunca toca red, Keystore, SAF ni el núcleo nativo.
 */
class FuenteNucleoFalsa : FuenteNucleo by FuenteNucleoVacia {

    private val _sesiones = MutableStateFlow<List<SesionVista>>(emptyList())
    override val sesiones: StateFlow<List<SesionVista>> = _sesiones.asStateFlow()

    private val _tuneles = MutableStateFlow<List<TunelVista>>(emptyList())
    override val tuneles: StateFlow<List<TunelVista>> = _tuneles.asStateFlow()

    // Con buffer: una pantalla perdida descuadraría el terminal y una petición
    // perdida dejaría una conexión esperando; nunca se conflan (M2).
    private val _pantallas = MutableSharedFlow<PantallaVista>(extraBufferCapacity = 64)
    override val pantallas: Flow<PantallaVista> = _pantallas.asSharedFlow()

    private val _peticiones = MutableSharedFlow<PeticionDialogoVista>(extraBufferCapacity = 16)
    override val peticiones: Flow<PeticionDialogoVista> = _peticiones.asSharedFlow()

    private val _sincronizacion = MutableStateFlow(EstadoSincronizacionVista())
    override val sincronizacion: StateFlow<EstadoSincronizacionVista> = _sincronizacion.asStateFlow()

    private val _tamanoLetra = MutableStateFlow(ZOOM_POR_DEFECTO)
    override val tamanoLetra: StateFlow<Int> = _tamanoLetra.asStateFlow()

    // Datos que devuelven las llamadas.
    var inventarioRespuesta: InventarioVista = InventarioVista()
    var fichas: MutableMap<String, FichaHostVista> = mutableMapOf()
    var tunelesRespuesta: List<TunelVista> = emptyList()
    var conflictosRespuesta: List<ConflictoHuellaVista> = emptyList()
    var claveRespuesta: ClaveMovilVista? = null
    var versionRespuesta: VersionVista = VersionVista("0.1.0", 1009, "0.1.0", 1, 7)
    var abrirSesionResultado: ResultadoAperturaVista = ResultadoAperturaVista.Abierta(1L)
    var instalacionResultado: ResultadoInstalacionVista = ResultadoInstalacionVista.InstaladaAceptada
    var errorRespuesta: ResultadoAccion.Error? = null
    var pegadoEntreCorchetesActivo: Boolean = false

    // Llamadas registradas.
    val llamadas: MutableList<String> = mutableListOf()
    /** Eventos de entrada en el orden en que llegaron al núcleo (M7). */
    val eventos: MutableList<String> = mutableListOf()
    val escrituras: MutableList<String> = mutableListOf()
    val teclas: MutableList<Pair<TeclaVista, ModificadoresVista>> = mutableListOf()
    val pegados: MutableList<String> = mutableListOf()
    val respuestasDialogo: MutableList<Pair<Long, RespuestaDialogoVista>> = mutableListOf()
    val redimensiones: MutableList<Triple<Long, Int, Int>> = mutableListOf()
    val desplazamientos: MutableList<Pair<Long, Int>> = mutableListOf()
    var tamanoLetraGuardado: Int? = null
    var ultimoFiltro: String? = null
    var claveUnida: String? = null
    var carpetaElegida: String? = null
    var vecesIniciado: Int = 0

    fun emitirSesiones(lista: List<SesionVista>) {
        _sesiones.value = lista
    }

    fun emitirTuneles(lista: List<TunelVista>) {
        _tuneles.value = lista
    }

    fun emitirPantalla(pantalla: PantallaVista) {
        _pantallas.tryEmit(pantalla)
    }

    fun emitirPeticion(peticion: PeticionDialogoVista) {
        _peticiones.tryEmit(peticion)
    }

    fun emitirSincronizacion(estado: EstadoSincronizacionVista) {
        _sincronizacion.value = estado
    }

    fun emitirTamanoLetra(tamano: Int) {
        _tamanoLetra.value = tamano
    }

    private fun respuesta(): ResultadoAccion = errorRespuesta ?: ResultadoAccion.Ok

    override suspend fun iniciar(): ResultadoAccion {
        vecesIniciado++
        return respuesta()
    }

    override suspend fun version(): VersionVista = versionRespuesta

    override suspend fun inventario(filtro: String?): InventarioVista {
        ultimoFiltro = filtro
        return inventarioRespuesta
    }

    override suspend fun fichaHost(uuid: String): FichaHostVista? = fichas[uuid]

    override suspend fun plegarGrupo(uuid: String, plegado: Boolean): ResultadoAccion {
        llamadas.add("plegar:" + uuid + ":" + plegado)
        return respuesta()
    }

    override suspend fun sincronizacionEstado(): EstadoSincronizacionVista = _sincronizacion.value

    override suspend fun sincronizarAhora(): ResultadoAccion {
        llamadas.add("sincronizar")
        return respuesta()
    }

    override suspend fun elegirCarpeta(uri: String): ResultadoAccion {
        carpetaElegida = uri
        return respuesta()
    }

    override suspend fun unirConClave(texto: String): ResultadoAccion {
        claveUnida = texto
        return respuesta()
    }

    override suspend fun dejarDeSincronizar(): ResultadoAccion {
        llamadas.add("dejarDeSincronizar")
        return respuesta()
    }

    override suspend fun conflictosHuella(): List<ConflictoHuellaVista> = conflictosRespuesta

    override suspend fun aceptarHuellaDeOtroEquipo(id: Long): ResultadoAccion {
        llamadas.add("aceptarHuella:" + id)
        return respuesta()
    }

    override suspend fun mantenerMiHuella(id: Long): ResultadoAccion {
        llamadas.add("mantenerHuella:" + id)
        return respuesta()
    }

    override suspend fun abrirSesion(hostUuid: String): ResultadoAperturaVista {
        llamadas.add("abrirSesion:" + hostUuid)
        return abrirSesionResultado
    }

    override suspend fun escribir(sesion: Long, bytes: ByteArray): ResultadoAccion {
        val texto = String(bytes, Charsets.UTF_8)
        escrituras.add(texto)
        eventos.add("escribir:" + texto)
        return respuesta()
    }

    override suspend fun tecla(
        sesion: Long,
        tecla: TeclaVista,
        modificadores: ModificadoresVista,
    ): ResultadoAccion {
        teclas.add(tecla to modificadores)
        eventos.add("tecla:" + tecla)
        return respuesta()
    }

    override suspend fun pegar(sesion: Long, texto: String): ResultadoAccion {
        pegados.add(texto)
        eventos.add("pegar:" + texto)
        return respuesta()
    }

    override fun pegadoEntreCorchetes(sesion: Long): Boolean = pegadoEntreCorchetesActivo

    override suspend fun redimensionar(sesion: Long, columnas: Int, filas: Int): ResultadoAccion {
        redimensiones.add(Triple(sesion, columnas, filas))
        return respuesta()
    }

    override suspend fun desplazar(sesion: Long, filas: Int): ResultadoAccion {
        desplazamientos.add(sesion to filas)
        return respuesta()
    }

    override suspend fun textoSeleccion(
        sesion: Long,
        desde: PosicionVista,
        hasta: PosicionVista,
    ): String? = "seleccion"

    override suspend fun reconectar(sesion: Long): ResultadoAccion {
        llamadas.add("reconectar:" + sesion)
        return respuesta()
    }

    override suspend fun cerrarSesion(sesion: Long): ResultadoAccion {
        llamadas.add("cerrarSesion:" + sesion)
        return respuesta()
    }

    override suspend fun tuneles(hostUuid: String?): List<TunelVista> =
        if (hostUuid == null) tunelesRespuesta else tunelesRespuesta.filter { it.hostUuid == hostUuid }

    override suspend fun activarTunel(uuid: String): ResultadoAccion {
        llamadas.add("activarTunel:" + uuid)
        return respuesta()
    }

    override suspend fun pararTunel(uuid: String): ResultadoAccion {
        llamadas.add("pararTunel:" + uuid)
        return respuesta()
    }

    override suspend fun relanzarTunel(uuid: String): ResultadoAccion {
        llamadas.add("relanzarTunel:" + uuid)
        return respuesta()
    }

    override suspend fun activarEnPuertoLibre(uuid: String): ResultadoAccion {
        llamadas.add("puertoLibre:" + uuid)
        return respuesta()
    }

    override suspend fun responderDialogo(
        peticionId: Long,
        respuesta: RespuestaDialogoVista,
    ): ResultadoAccion {
        respuestasDialogo.add(peticionId to respuesta)
        return respuesta()
    }

    override suspend fun clavePublicaMovil(): ClaveMovilVista? = claveRespuesta

    /** Si la creación de la clave del móvil sale bien y qué clave queda. */
    var claveCreada: Boolean = true
    var claveTrasCrear: ClaveMovilVista? = null
    var vecesCrearClave: Int = 0

    override suspend fun crearClaveMovil(): Boolean {
        vecesCrearClave++
        if (claveCreada && claveTrasCrear != null) claveRespuesta = claveTrasCrear
        return claveCreada
    }

    override suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionVista {
        llamadas.add("instalarClave:" + hostUuid)
        return instalacionResultado
    }

    override suspend fun olvidarContrasena(hostUuid: String): ResultadoAccion {
        llamadas.add("olvidarContrasena:" + hostUuid)
        return respuesta()
    }

    override suspend fun borrarDatos(todo: Boolean): ResultadoAccion {
        llamadas.add("borrarDatos:" + todo)
        return respuesta()
    }

    override suspend fun cambiarTamanoLetra(sp: Int): ResultadoAccion {
        tamanoLetraGuardado = sp
        return respuesta()
    }
}
