package org.cuatrodetres.magi.ui.fuente

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import org.cuatrodetres.magi.BuildConfig
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
 * Fuente sin núcleo: la app arranca y se puede revisar la interfaz mientras
 * `crates/magi-movil` y el adaptador real no estén conectados.
 *
 * No inventa datos: el inventario está vacío, no hay sesiones ni túneles y
 * cualquier acción que necesite el núcleo devuelve un error legible con
 * instrucción. Los dobles de prueba de la JVM heredan de aquí y sobrescriben lo
 * que necesiten.
 */
object FuenteNucleoVacia : FuenteNucleo {

    /** Mensaje único para las acciones que necesitan el núcleo. */
    const val SIN_NUCLEO: String = "El núcleo de MAGI aún no está conectado en esta compilación."

    private val _sesiones = MutableStateFlow<List<SesionVista>>(emptyList())
    override val sesiones: StateFlow<List<SesionVista>> = _sesiones.asStateFlow()

    private val _tuneles = MutableStateFlow<List<TunelVista>>(emptyList())
    override val tuneles: StateFlow<List<TunelVista>> = _tuneles.asStateFlow()

    override val pantallas: Flow<PantallaVista> = emptyFlow()

    override val peticiones: Flow<PeticionDialogoVista> = emptyFlow()

    private val _sincronizacion = MutableStateFlow(EstadoSincronizacionVista())
    override val sincronizacion: StateFlow<EstadoSincronizacionVista> = _sincronizacion.asStateFlow()

    private val _tamanoLetra = MutableStateFlow(ZOOM_POR_DEFECTO)
    override val tamanoLetra: StateFlow<Int> = _tamanoLetra.asStateFlow()

    override suspend fun iniciar(): ResultadoAccion = ResultadoAccion.Ok

    override suspend fun version(): VersionVista = VersionVista(
        app = BuildConfig.VERSION_NAME,
        codigo = BuildConfig.VERSION_CODE,
        nucleo = "—",
        formato = 0,
        userVersion = 0,
    )

    override suspend fun inventario(filtro: String?): InventarioVista = InventarioVista()

    override suspend fun fichaHost(uuid: String): FichaHostVista? = null

    override suspend fun plegarGrupo(uuid: String, plegado: Boolean): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun sincronizacionEstado(): EstadoSincronizacionVista = _sincronizacion.value

    override suspend fun sincronizarAhora(): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun elegirCarpeta(uri: String): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun unirConClave(texto: String): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun dejarDeSincronizar(): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun conflictosHuella(): List<ConflictoHuellaVista> = emptyList()

    override suspend fun aceptarHuellaDeOtroEquipo(id: Long): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun mantenerMiHuella(id: Long): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun abrirSesion(hostUuid: String): ResultadoAperturaVista =
        ResultadoAperturaVista.Error(SIN_NUCLEO)

    override suspend fun escribir(sesion: Long, bytes: ByteArray): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun tecla(
        sesion: Long,
        tecla: TeclaVista,
        modificadores: ModificadoresVista,
    ): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun pegar(sesion: Long, texto: String): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override fun pegadoEntreCorchetes(sesion: Long): Boolean = false

    override suspend fun redimensionar(sesion: Long, columnas: Int, filas: Int): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun desplazar(sesion: Long, filas: Int): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun textoSeleccion(
        sesion: Long,
        desde: PosicionVista,
        hasta: PosicionVista,
    ): String? = null

    override suspend fun reconectar(sesion: Long): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun cerrarSesion(sesion: Long): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun tuneles(hostUuid: String?): List<TunelVista> = emptyList()

    override suspend fun activarTunel(uuid: String): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun pararTunel(uuid: String): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun relanzarTunel(uuid: String): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun activarEnPuertoLibre(uuid: String): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun responderDialogo(
        peticionId: Long,
        respuesta: RespuestaDialogoVista,
    ): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun clavePublicaMovil(): ClaveMovilVista? = null

    override suspend fun crearClaveMovil(): Boolean = false

    override suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionVista =
        ResultadoInstalacionVista.Error(SIN_NUCLEO)

    override suspend fun olvidarContrasena(hostUuid: String): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun borrarDatos(todo: Boolean): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun cambiarTamanoLetra(sp: Int): ResultadoAccion = ResultadoAccion.Ok
}
