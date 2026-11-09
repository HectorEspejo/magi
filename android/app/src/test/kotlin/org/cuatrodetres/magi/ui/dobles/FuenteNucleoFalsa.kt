package org.cuatrodetres.magi.ui.dobles

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
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

    /** Puerta de prueba: si no es null, fichaHost espera a que se complete. */
    var puertaFichaHost: CompletableDeferred<Unit>? = null

    override suspend fun fichaHost(uuid: String): FichaHostVista? {
        puertaFichaHost?.await()
        return fichas[uuid]
    }

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

    private val _avisosLogs = MutableSharedFlow<EstadoLogsVista>(extraBufferCapacity = 64)
    override val avisosLogs: Flow<EstadoLogsVista> = _avisosLogs.asSharedFlow()

    private val _accionesHechas = MutableSharedFlow<ResultadoServicioVista>(extraBufferCapacity = 16)
    override val accionesHechas: Flow<ResultadoServicioVista> = _accionesHechas.asSharedFlow()

    // Datos que devuelven las llamadas de la Fase 14.
    var flotaRespuesta: List<HostFlotaVista> = emptyList()
    var todaLaFlotaRespuesta: TodaLaFlotaVista = TodaLaFlotaVista()
    var listadoRespuesta: ListadoServiciosVista = ListadoServiciosVista()
    var consumosRespuesta: List<ConsumoVista> = emptyList()
    var detalleServicioRespuesta: DetalleServiciosVista = DetalleServiciosVista()
    var infoAccionRespuesta: InfoAccionVista = InfoAccionVista()
    var resultadoServicioRespuesta: ResultadoServicioVista = ResultadoServicioVista()
    var resultadoSudoRespuesta: ResultadoSudoVista = ResultadoSudoVista.OK
    var ventanaLogsRespuesta: VentanaLogsVista = VentanaLogsVista()
    var snippetsRespuesta: List<SnippetVista> = emptyList()
    var snippetRespuesta: SnippetVista? = null
    var preparacionRespuesta: PreparacionEjecucionVista = PreparacionEjecucionVista()
    var ejecucionesRespuesta: List<EjecucionVista> = emptyList()
    var salidaRespuesta: SalidaEjecucionVista = SalidaEjecucionVista()
    var tablaDeliberacionRespuesta: TablaDeliberacionVista = TablaDeliberacionVista()
    var registroRespuesta: List<EntradaRegistroVista> = emptyList()
    var registroDetalleRespuesta: DetalleRegistroVista? = null
    /** Id que devuelven logsSeguir, ejecutarSnippet y resolverDeliberacion. */
    var idRespuesta: Long = 1L
    /** Si no es null, las lecturas nuevas devuelven este error. */
    var errorConsulta: String? = null
    var ajustesAplicados: AjustesMovilVista? = null

    // Llamadas registradas de la Fase 14.
    val sondas: MutableList<List<String>> = mutableListOf()
    val accionesSolicitadas: MutableList<Triple<String, ObjetivoVista, AccionServicioVista>> =
        mutableListOf()

    /** Deliberación con la que se pidió cada acción (null si no hacía falta). */
    val deliberacionesSolicitadas: MutableList<Long?> = mutableListOf()
    val ventanasPedidas: MutableList<Pair<Long, FiltroLogsVista>> = mutableListOf()
    val deliberacionesPedidas: MutableList<PlanDeliberacionVista> = mutableListOf()
    val decisionesDeliberacion: MutableList<Pair<Long, DecisionDeliberacionVista>> =
        mutableListOf()
    val paginasRegistro: MutableList<Triple<FiltroRegistroVista, Long?, Int>> = mutableListOf()
    val contrasenasSudo: MutableList<Pair<String, Int>> = mutableListOf()

    fun emitirFlota(lista: List<HostFlotaVista>) {
        _flota.value = lista
    }

    fun emitirAccionesServicio(lista: List<AccionEnCursoVista>) {
        _accionesServicio.value = lista
    }

    fun emitirEjecuciones(lista: List<EjecucionVista>) {
        _ejecuciones.value = lista
    }

    fun emitirAvisoLogs(estado: EstadoLogsVista) {
        _avisosLogs.tryEmit(estado)
    }

    fun emitirAccionHecha(resultado: ResultadoServicioVista) {
        _accionesHechas.tryEmit(resultado)
    }

    private fun <T> lectura(valor: T): ResultadoNucleoVista<T> =
        errorConsulta?.let { ResultadoNucleoVista.Error(it) }
            ?: ResultadoNucleoVista.Listo(valor)

    // =======================================================================
    // Fase 14 · ajustes y flota (informe §5.1 y §5.4)
    // =======================================================================

    override suspend fun aplicarAjustes(ajustes: AjustesMovilVista): ResultadoAjustesVista {
        llamadas.add("aplicarAjustes")
        ajustesAplicados = ajustes
        return acotarAjustes(ajustes)
    }

    override suspend fun flota(filtro: String?): List<HostFlotaVista> {
        ultimoFiltro = filtro
        llamadas.add("flota:" + (filtro ?: ""))
        return flotaRespuesta
    }

    override suspend fun sondear(hosts: List<String>): ResultadoAccion {
        sondas.add(hosts)
        llamadas.add("sondear:" + hosts.joinToString(","))
        return respuesta()
    }

    // =======================================================================
    // Fase 14 · servicios (informe §5.1)
    // =======================================================================

    override suspend fun serviciosTodaLaFlota(): TodaLaFlotaVista {
        llamadas.add("serviciosTodaLaFlota")
        return todaLaFlotaRespuesta
    }

    override suspend fun serviciosListar(
        hostUuid: String,
        refresco: Boolean,
    ): ResultadoNucleoVista<ListadoServiciosVista> {
        llamadas.add("serviciosListar:" + hostUuid + ":" + refresco)
        return lectura(listadoRespuesta)
    }

    override suspend fun serviciosConsumo(
        hostUuid: String,
    ): ResultadoNucleoVista<List<ConsumoVista>> {
        llamadas.add("serviciosConsumo:" + hostUuid)
        return lectura(consumosRespuesta)
    }

    override suspend fun serviciosDetalle(
        hostUuid: String,
        objetivo: ObjetivoVista,
    ): ResultadoNucleoVista<DetalleServiciosVista> {
        llamadas.add("serviciosDetalle:" + hostUuid + ":" + objetivo.nombre)
        return lectura(detalleServicioRespuesta)
    }

    override suspend fun prepararAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
    ): ResultadoNucleoVista<InfoAccionVista> {
        llamadas.add("prepararAccion:" + hostUuid + ":" + objetivo.nombre + ":" + accion)
        return lectura(infoAccionRespuesta)
    }

    override suspend fun servicioAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
        deliberacion: Long?,
    ): ResultadoNucleoVista<ResultadoServicioVista> {
        accionesSolicitadas.add(Triple(hostUuid, objetivo, accion))
        deliberacionesSolicitadas.add(deliberacion)
        llamadas.add("servicioAccion:" + hostUuid + ":" + objetivo.nombre + ":" + accion)
        return lectura(resultadoServicioRespuesta)
    }

    override suspend fun sudoComprobar(
        hostUuid: String,
        contrasena: ByteArray,
    ): ResultadoSudoVista {
        contrasenasSudo.add(hostUuid to contrasena.size)
        llamadas.add("sudoComprobar:" + hostUuid)
        return resultadoSudoRespuesta
    }

    override suspend fun sudoOlvidar(hostUuid: String): ResultadoAccion {
        llamadas.add("sudoOlvidar:" + hostUuid)
        return respuesta()
    }

    override suspend fun olvidarSudoAlBloquear(): ResultadoAccion {
        llamadas.add("olvidarSudoAlBloquear")
        return respuesta()
    }

    // =======================================================================
    // Fase 14 · logs y sesiones con orden (informe §5.1)
    // =======================================================================

    override suspend fun logsSeguir(
        hostUuid: String,
        objetivo: ObjetivoVista,
        lineas: Int,
    ): ResultadoNucleoVista<Long> {
        llamadas.add("logsSeguir:" + hostUuid + ":" + objetivo.nombre + ":" + lineas)
        return lectura(idRespuesta)
    }

    override suspend fun logsVentana(
        suscripcion: Long,
        filtro: FiltroLogsVista,
    ): ResultadoNucleoVista<VentanaLogsVista> {
        ventanasPedidas.add(suscripcion to filtro)
        llamadas.add("logsVentana:" + suscripcion)
        return lectura(ventanaLogsRespuesta)
    }

    override suspend fun logsParar(suscripcion: Long): ResultadoAccion {
        llamadas.add("logsParar:" + suscripcion)
        return respuesta()
    }

    override suspend fun abrirSesionConOrden(
        hostUuid: String,
        orden: OrdenPestanaVista,
    ): ResultadoAperturaVista {
        llamadas.add("abrirSesionConOrden:" + hostUuid + ":" + orden)
        return abrirSesionResultado
    }

    // =======================================================================
    // Fase 14 · snippets y ejecuciones (informe §5.1)
    // =======================================================================

    override suspend fun snippets(filtro: String?): List<SnippetVista> {
        ultimoFiltro = filtro
        llamadas.add("snippets:" + (filtro ?: ""))
        return snippetsRespuesta
    }

    override suspend fun snippetsDeHost(hostUuid: String): List<SnippetVista> {
        llamadas.add("snippetsDeHost:" + hostUuid)
        return snippetsRespuesta
    }

    override suspend fun snippet(uuid: String): SnippetVista? {
        llamadas.add("snippet:" + uuid)
        return snippetRespuesta ?: snippetsRespuesta.firstOrNull { it.uuid == uuid }
    }

    override suspend fun prepararEjecucion(
        snippetUuid: String,
        hosts: List<String>,
        variables: List<VariableSnippetVista>,
    ): ResultadoNucleoVista<PreparacionEjecucionVista> {
        llamadas.add("prepararEjecucion:" + snippetUuid + ":" + hosts.joinToString(","))
        return lectura(preparacionRespuesta)
    }

    override suspend fun ejecutarSnippet(
        preparacion: PreparacionEjecucionVista,
        pararAlFallo: Boolean,
        deliberacion: Long?,
    ): ResultadoNucleoVista<Long> {
        llamadas.add("ejecutarSnippet:" + preparacion.snippetUuid)
        return lectura(idRespuesta)
    }

    override suspend fun ejecucionesLista(): List<EjecucionVista> {
        llamadas.add("ejecucionesLista")
        return ejecucionesRespuesta
    }

    override suspend fun salidaEjecucion(
        ejecucion: Long,
        hostUuid: String,
    ): ResultadoNucleoVista<SalidaEjecucionVista> {
        llamadas.add("salidaEjecucion:" + ejecucion + ":" + hostUuid)
        return lectura(salidaRespuesta)
    }

    override suspend fun cancelarEjecucion(id: Long): ResultadoAccion {
        llamadas.add("cancelarEjecucion:" + id)
        return respuesta()
    }

    override suspend fun repetirEjecucion(
        id: Long,
    ): ResultadoNucleoVista<PreparacionEjecucionVista> {
        llamadas.add("repetirEjecucion:" + id)
        return lectura(preparacionRespuesta)
    }

    override suspend fun limpiarEjecuciones(): ResultadoAccion {
        llamadas.add("limpiarEjecuciones")
        return respuesta()
    }

    // =======================================================================
    // Fase 14 · deliberación y registro (informe §5.1)
    // =======================================================================

    override suspend fun deliberar(
        plan: PlanDeliberacionVista,
    ): ResultadoNucleoVista<TablaDeliberacionVista> {
        deliberacionesPedidas.add(plan)
        llamadas.add("deliberar:" + plan.accion)
        return lectura(tablaDeliberacionRespuesta)
    }

    override suspend fun resolverDeliberacion(
        id: Long,
        decision: DecisionDeliberacionVista,
    ): ResultadoNucleoVista<Long> {
        decisionesDeliberacion.add(id to decision)
        llamadas.add("resolverDeliberacion:" + id + ":" + decision)
        return lectura(idRespuesta)
    }

    override suspend fun registro(
        filtro: FiltroRegistroVista,
        desde: Long?,
        cuantas: Int,
    ): List<EntradaRegistroVista> {
        paginasRegistro.add(Triple(filtro, desde, cuantas))
        llamadas.add("registro:desde=" + (desde ?: "") + ":cuantas=" + cuantas)
        return registroRespuesta
    }

    override suspend fun registroDetalle(id: Long): DetalleRegistroVista? {
        llamadas.add("registroDetalle:" + id)
        return registroDetalleRespuesta
    }
}
