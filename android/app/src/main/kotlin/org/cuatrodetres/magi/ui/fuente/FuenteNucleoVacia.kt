package org.cuatrodetres.magi.ui.fuente

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import org.cuatrodetres.magi.BuildConfig
import org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConsumoVista
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.DetalleRegistroVista
import org.cuatrodetres.magi.ui.modelos.DetalleServiciosVista
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.EstadoLogsVista
import org.cuatrodetres.magi.ui.modelos.FiltroLogsVista
import org.cuatrodetres.magi.ui.modelos.FiltroRegistroVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.OrdenPestanaVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAjustesVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.SalidaEjecucionVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.TAMANO_PAGINA_REGISTRO
import org.cuatrodetres.magi.ui.modelos.TodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.VariableSnippetVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
import org.cuatrodetres.magi.ui.modelos.acotarAjustes
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

    // =======================================================================
    // Fase 14 · flujos nuevos (informe §5.2)
    // =======================================================================

    private val _flota = MutableStateFlow<List<HostFlotaVista>>(emptyList())
    override val flota: StateFlow<List<HostFlotaVista>> = _flota.asStateFlow()

    private val _accionesServicio = MutableStateFlow<List<AccionEnCursoVista>>(emptyList())
    override val accionesServicio: StateFlow<List<AccionEnCursoVista>> =
        _accionesServicio.asStateFlow()

    private val _ejecuciones = MutableStateFlow<List<EjecucionVista>>(emptyList())
    override val ejecuciones: StateFlow<List<EjecucionVista>> = _ejecuciones.asStateFlow()

    // Sin núcleo no hay suscripciones de logs ni acciones que contar.
    override val avisosLogs: Flow<EstadoLogsVista> = emptyFlow()

    override val accionesHechas: Flow<ResultadoServicioVista> = emptyFlow()

    // =======================================================================
    // Fase 14 · ajustes y flota (informe §5.1 y §5.4)
    // =======================================================================

    override suspend fun aplicarAjustes(ajustes: AjustesMovilVista): ResultadoAjustesVista =
        acotarAjustes(ajustes)

    override suspend fun flota(filtro: String?): List<HostFlotaVista> = emptyList()

    override suspend fun sondear(hosts: List<String>): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    // =======================================================================
    // Fase 14 · servicios (informe §5.1)
    // =======================================================================

    override suspend fun serviciosTodaLaFlota(): TodaLaFlotaVista = TodaLaFlotaVista()

    override suspend fun serviciosListar(
        hostUuid: String,
        refresco: Boolean,
    ): ResultadoNucleoVista<ListadoServiciosVista> = sinNucleo()

    override suspend fun serviciosConsumo(
        hostUuid: String,
    ): ResultadoNucleoVista<List<ConsumoVista>> = sinNucleo()

    override suspend fun serviciosDetalle(
        hostUuid: String,
        objetivo: ObjetivoVista,
    ): ResultadoNucleoVista<DetalleServiciosVista> = sinNucleo()

    override suspend fun prepararAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
    ): ResultadoNucleoVista<InfoAccionVista> = sinNucleo()

    override suspend fun servicioAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
        deliberacion: Long?,
    ): ResultadoNucleoVista<ResultadoServicioVista> = sinNucleo()

    override suspend fun sudoComprobar(
        hostUuid: String,
        contrasena: ByteArray,
    ): ResultadoSudoVista = ResultadoSudoVista.ERROR(SIN_NUCLEO)

    override suspend fun sudoOlvidar(hostUuid: String): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun olvidarSudoAlBloquear(): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    // =======================================================================
    // Fase 14 · logs y sesiones con orden (informe §5.1)
    // =======================================================================

    override suspend fun logsSeguir(
        hostUuid: String,
        objetivo: ObjetivoVista,
        lineas: Int,
    ): ResultadoNucleoVista<Long> = sinNucleo()

    override suspend fun logsVentana(
        suscripcion: Long,
        filtro: FiltroLogsVista,
    ): ResultadoNucleoVista<VentanaLogsVista> = sinNucleo()

    override suspend fun logsParar(suscripcion: Long): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun abrirSesionConOrden(
        hostUuid: String,
        orden: OrdenPestanaVista,
    ): ResultadoAperturaVista = ResultadoAperturaVista.Error(SIN_NUCLEO)

    // =======================================================================
    // Fase 14 · snippets y ejecuciones (informe §5.1)
    // =======================================================================

    override suspend fun snippets(filtro: String?): List<SnippetVista> = emptyList()

    override suspend fun snippetsDeHost(hostUuid: String): List<SnippetVista> = emptyList()

    override suspend fun snippet(uuid: String): SnippetVista? = null

    override suspend fun prepararEjecucion(
        snippetUuid: String,
        hosts: List<String>,
        variables: List<VariableSnippetVista>,
    ): ResultadoNucleoVista<PreparacionEjecucionVista> = sinNucleo()

    override suspend fun ejecutarSnippet(
        preparacion: PreparacionEjecucionVista,
        pararAlFallo: Boolean,
        deliberacion: Long?,
    ): ResultadoNucleoVista<Long> = sinNucleo()

    override suspend fun ejecucionesLista(): List<EjecucionVista> = emptyList()

    override suspend fun salidaEjecucion(
        ejecucion: Long,
        hostUuid: String,
    ): ResultadoNucleoVista<SalidaEjecucionVista> = sinNucleo()

    override suspend fun cancelarEjecucion(id: Long): ResultadoAccion =
        ResultadoAccion.Error(SIN_NUCLEO)

    override suspend fun repetirEjecucion(id: Long): ResultadoNucleoVista<PreparacionEjecucionVista> =
        sinNucleo()

    override suspend fun limpiarEjecuciones(): ResultadoAccion = ResultadoAccion.Error(SIN_NUCLEO)

    // =======================================================================
    // Fase 14 · deliberación y registro (informe §5.1)
    // =======================================================================

    override suspend fun deliberar(
        plan: PlanDeliberacionVista,
    ): ResultadoNucleoVista<TablaDeliberacionVista> = sinNucleo()

    override suspend fun resolverDeliberacion(
        id: Long,
        decision: DecisionDeliberacionVista,
    ): ResultadoNucleoVista<Long> = sinNucleo()

    override suspend fun registro(
        filtro: FiltroRegistroVista,
        desde: Long?,
        cuantas: Int,
    ): List<EntradaRegistroVista> = emptyList()

    override suspend fun registroDetalle(id: Long): DetalleRegistroVista? = null

    /** Lectura de la Fase 14 sin núcleo: error legible, nunca datos inventados. */
    private fun <T> sinNucleo(): ResultadoNucleoVista<T> =
        ResultadoNucleoVista.Error(SIN_NUCLEO)
}
