package org.cuatrodetres.magi.ui.servicios

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.ConsumoVista
import org.cuatrodetres.magi.ui.modelos.DetalleServiciosVista
import org.cuatrodetres.magi.ui.modelos.FiltroLogsVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.MotorNombreVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.PestanaDetalleVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAccionServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.SeccionServiciosVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.modelos.TodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.deObjetivo
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/** Configuración de Servicios que la app pasa desde los ajustes (§5.4). */
data class ConfigServicios(
    /** Refresco automático del listado: 0 = solo al entrar. */
    val refrescoSeg: Int = 10,
    /** Espera máxima al estado final de una acción. */
    val esperaSeg: Int = 30,
    /** Motores de contenedores que el núcleo puede usar. */
    val motores: List<MotorNombreVista> = listOf(MotorNombreVista.DOCKER, MotorNombreVista.PODMAN),
    /** Minutos sin uso tras los que se olvida la contraseña de sudo. */
    val sudoMemoriaMin: Int = 5,
)

/** Diálogo de contraseña de sudo (enmascarado y nunca guardado). */
data class EstadoSudoUi(
    val visible: Boolean = false,
    val hostUuid: String = "",
    val hostNombre: String = "",
    val intentosRestantes: Int = 3,
    val comprobando: Boolean = false,
    val error: String? = null,
)

/** Confirmación de una acción: la acción, el objetivo y lo que avisa el núcleo. */
data class ConfirmacionAccionUi(
    val objetivo: ObjetivoVista,
    val accion: AccionServicioVista,
    val info: InfoAccionVista,
)

/** Estado completo de la pantalla Servicios (lista de la flota y de un host). */
data class EstadoServiciosUi(
    /** «Toda la flota»: fallos del último sondeo y hosts con problemas. */
    val todaLaFlota: TodaLaFlotaVista = TodaLaFlotaVista(),
    val hostUuid: String? = null,
    val hostNombre: String = "",
    val listado: ListadoServiciosVista? = null,
    val detalle: DetalleServiciosVista? = null,
    /** Objetivo cuyo detalle se está viendo (null = la lista del host). */
    val objetivo: ObjetivoVista? = null,
    val pestana: PestanaDetalleVista = PestanaDetalleVista.ESTADO,
    val seccion: SeccionServiciosVista = SeccionServiciosVista.SERVICIOS,
    val filtro: String = "",
    /** Objetivo que hay que abrir en cuanto llegue el listado (toque de un fallo). */
    val objetivoInicial: ObjetivoVista? = null,
    val consumo: Map<String, ConsumoVista> = emptyMap(),
    val acciones: List<AccionEnCursoVista> = emptyList(),
    val confirmacion: ConfirmacionAccionUi? = null,
    /**
     * Deliberación con la que se confirmó una acción antes de pedir la
     * contraseña de sudo: sin guardarla, al volver del sudo se ejecutaría sin
     * autorización y el núcleo la rechazaría.
     */
    val deliberacionPendiente: Long? = null,
    val resultado: ResultadoServicioVista? = null,
    val sudo: EstadoSudoUi = EstadoSudoUi(),
    val cargando: Boolean = false,
    val cargandoFlota: Boolean = true,
    val cargandoDetalle: Boolean = false,
    val refrescando: Boolean = false,
    val incompleto: Boolean = false,
    val aviso: String? = null,
) {
    /** ¿Se está viendo un host (y no «Toda la flota»)? */
    val enHost: Boolean get() = hostUuid != null

    /** ¿Se está viendo el detalle de un objetivo? */
    val enDetalle: Boolean get() = objetivo != null

    /** ¿Hay contraseña de sudo en memoria en el host abierto? */
    val sudoEnMemoria: Boolean get() = listado?.sudoEnMemoria == true

    /** ¿Está en marcha una acción sobre ese objetivo? */
    fun accionDe(objetivo: ObjetivoVista): AccionEnCursoVista? =
        hostUuid?.let { acciones.deObjetivo(it, objetivo) }

    /** ¿Hay acciones en curso en el host abierto? */
    val hayAcciones: Boolean
        get() = hostUuid != null && acciones.any { it.hostUuid == hostUuid }

    /** Unidades del listado que pasan el filtro, en el orden de la F13. */
    val unidades: List<org.cuatrodetres.magi.ui.modelos.UnidadVista>
        get() {
            val lista = listado?.unidadesOrdenadas ?: emptyList()
            return filtrarPorNombre(lista) { it.nombre }
        }

    /** Timers del listado que pasan el filtro. */
    val timers: List<org.cuatrodetres.magi.ui.modelos.TimerVista>
        get() {
            val lista = listado?.timersOrdenados ?: emptyList()
            return filtrarPorNombre(lista) { it.nombre }
        }

    /** Contenedores agrupados por proyecto y filtrados. */
    fun gruposContenedores(): List<GrupoContenedoresUi> {
        val grupos = listado?.contenedoresPorProyecto() ?: emptyList()
        val texto = filtro.trim().lowercase()
        return grupos.mapNotNull { grupo ->
            val contenedores = if (texto.isEmpty()) grupo.contenedores
            else grupo.contenedores.filter {
                it.nombre.lowercase().contains(texto) || it.imagen.lowercase().contains(texto)
            }
            if (contenedores.isEmpty() && grupo.proyecto == null) null
            else GrupoContenedoresUi(grupo.proyecto, contenedores)
        }
    }

    /** Número de filas de la sección elegida (para el pie). */
    fun cuantasVisibles(): Int = when (seccion) {
        SeccionServiciosVista.SERVICIOS -> unidades.size
        SeccionServiciosVista.TIMERS -> timers.size
        SeccionServiciosVista.CONTENEDORES -> gruposContenedores().sumOf { it.contenedores.size }
    }

    private fun <T> filtrarPorNombre(lista: List<T>, nombre: (T) -> String): List<T> {
        val texto = filtro.trim()
        if (texto.isEmpty()) return lista
        return lista.filter { nombre(it).lowercase().contains(texto.lowercase()) }
    }
}

/** Un bloque de contenedores de la vista (proyecto Compose o «sin proyecto»). */
data class GrupoContenedoresUi(
    val proyecto: org.cuatrodetres.magi.ui.modelos.ProyectoVista?,
    val contenedores: List<org.cuatrodetres.magi.ui.modelos.ContenedorVista>,
) {
    /** Clave estable del grupo; el texto lo pone la interfaz con strings. */
    val clave: String get() = proyecto?.nombre ?: "sin-proyecto"
}

/**
 * ViewModel de Servicios (informe §4.2, §6.3 y §6.4). Todo lo que decide qué se
 * puede hacer sale del núcleo (prepararAccion): aquí solo se orquesta la
 * lectura, el refresco a la vista, la confirmación, el sudo y la deliberación.
 */
class VistaModeloServicios(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    private val config: () -> ConfigServicios = { ConfigServicios() },
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoServiciosUi())
    val estado: StateFlow<EstadoServiciosUi> = _estado.asStateFlow()

    private var iniciado = false
    private var trabajoRefresco: Job? = null
    private var trabajoListado: Job? = null
    private var trabajoDetalle: Job? = null
    private var trabajoConsumo: Job? = null

    init {
        alcance.launch {
            fuente.accionesServicio.collect { acciones ->
                _estado.update { it.copy(acciones = acciones) }
            }
        }
        alcance.launch {
            fuente.accionesHechas.collect { resultado ->
                // Una acción terminada se enseña aunque la pantalla que la pidió
                // ya no esté (informe §5.2).
                _estado.update { actual ->
                    if (actual.hostUuid != null && actual.hostUuid == resultado.hostUuid) {
                        actual.copy(resultado = resultado)
                    } else {
                        actual
                    }
                }
                refrescarTrasAccion(resultado.hostUuid)
            }
        }
    }

    /** Abre la pantalla: carga «Toda la flota». Idempotente. */
    fun iniciar() {
        if (iniciado) return
        iniciado = true
        cargarTodaLaFlota()
    }

    /** Vuelve a «Toda la flota», parando el refresco del host. */
    fun volverATodaLaFlota() {
        trabajoRefresco?.cancel()
        trabajoListado?.cancel()
        trabajoDetalle?.cancel()
        trabajoConsumo?.cancel()
        _estado.update {
            it.copy(hostUuid = null, hostNombre = "", listado = null, detalle = null, objetivo = null, filtro = "", aviso = null)
        }
        cargarTodaLaFlota()
    }

    /** Carga los fallos de todos los hosts (no abre conexiones). */
    fun cargarTodaLaFlota() {
        _estado.update { it.copy(cargandoFlota = true) }
        alcance.launch {
            val resultado = runCatching { fuente.serviciosTodaLaFlota() }
            _estado.update { actual ->
                resultado.fold(
                    onSuccess = { flota -> actual.copy(todaLaFlota = flota, cargandoFlota = false) },
                    onFailure = {
                        actual.copy(cargandoFlota = false, aviso = it.message ?: "no se pudo leer la flota")
                    },
                )
            }
        }
    }

    /**
     * Abre los servicios de un host (lo pide el usuario: puede dialogar). Con
     * [objetivoInicial], al llegar el listado se abre el detalle de ese
     * elemento: es lo que hace tocar un fallo de «Toda la flota».
     */
    fun abrirHost(hostUuid: String, hostNombre: String, objetivoInicial: ObjetivoVista? = null) {
        trabajoRefresco?.cancel()
        trabajoListado?.cancel()
        trabajoDetalle?.cancel()
        trabajoConsumo?.cancel()
        _estado.update {
            it.copy(
                hostUuid = hostUuid,
                hostNombre = hostNombre,
                listado = null,
                detalle = null,
                objetivo = null,
                objetivoInicial = objetivoInicial,
                filtro = "",
                aviso = null,
                consumo = emptyMap(),
            )
        }
        cargarListado(manual = true)
        programarRefresco()
    }

    /**
     * Abre el detalle del objetivo pendiente en cuanto hay listado (toque de un
     * fallo en «Toda la flota»). Si el elemento ya no está, lo dice sin ruido.
     */
    private fun abrirObjetivoPendiente() {
        val actual = _estado.value
        val pendiente = actual.objetivoInicial ?: return
        val listado = actual.listado ?: return
        val objetivo = objetivoDelListado(listado, pendiente)
        if (objetivo == null) {
            _estado.update {
                it.copy(objetivoInicial = null, aviso = "el elemento ya no está en el host")
            }
            return
        }
        val seccion = when (objetivo.tipo) {
            TipoObjetivoVista.UNIDAD -> SeccionServiciosVista.SERVICIOS
            TipoObjetivoVista.TIMER -> SeccionServiciosVista.TIMERS
            TipoObjetivoVista.CONTENEDOR,
            TipoObjetivoVista.PROYECTO,
            -> SeccionServiciosVista.CONTENEDORES
        }
        _estado.update { it.copy(objetivoInicial = null, seccion = seccion) }
        abrirDetalle(objetivo)
        if (seccion == SeccionServiciosVista.CONTENEDORES) consumir()
    }

    /** El objetivo del listado que corresponde al pedido (nombre canónico). */
    private fun objetivoDelListado(
        listado: ListadoServiciosVista,
        objetivo: ObjetivoVista,
    ): ObjetivoVista? = when (objetivo.tipo) {
        TipoObjetivoVista.UNIDAD -> listado.unidades
            .firstOrNull { it.nombre == objetivo.nombre }
            ?.let { ObjetivoVista(TipoObjetivoVista.UNIDAD, it.nombre) }
        TipoObjetivoVista.TIMER -> listado.timers
            .firstOrNull { it.nombre == objetivo.nombre }
            ?.let { ObjetivoVista(TipoObjetivoVista.TIMER, it.nombre) }
        TipoObjetivoVista.CONTENEDOR -> listado.contenedores
            .firstOrNull { it.nombre == objetivo.nombre || it.id == objetivo.nombre }
            ?.let { ObjetivoVista(TipoObjetivoVista.CONTENEDOR, it.nombre) }
        TipoObjetivoVista.PROYECTO -> listado.proyectos
            .firstOrNull { it.nombre == objetivo.nombre }
            ?.let { ObjetivoVista(TipoObjetivoVista.PROYECTO, it.nombre) }
    }

    /** El botón REINTENTAR del aviso de «no responde» o «sin conexión». */
    fun reintentar() {
        if (_estado.value.enHost) {
            cargarListado(manual = true)
            programarRefresco()
        } else {
            cargarTodaLaFlota()
        }
    }

    private fun cargarListado(manual: Boolean) {
        val hostUuid = _estado.value.hostUuid ?: return
        if (trabajoListado?.isActive == true) return
        _estado.update { it.copy(refrescando = !manual, cargando = manual && it.listado == null) }
        trabajoListado = alcance.launch {
            val resultado = runCatching { fuente.serviciosListar(hostUuid, refresco = !manual) }
            _estado.update { actual ->
                when {
                    resultado.isFailure -> actual.copy(
                        cargando = false,
                        refrescando = false,
                        aviso = resultado.exceptionOrNull()?.message ?: "no se pudo leer el host",
                    )
                    else -> when (val leido = resultado.getOrThrow()) {
                        is ResultadoNucleoVista.Listo -> {
                            val seccion = if (actual.listado == null) leido.valor.seccionInicial else actual.seccion
                            actual.copy(
                                listado = leido.valor,
                                cargando = false,
                                refrescando = false,
                                aviso = null,
                                incompleto = leido.valor.incompleto,
                                seccion = seccion,
                            )
                        }
                        is ResultadoNucleoVista.Error -> actual.copy(
                            cargando = false,
                            refrescando = false,
                            aviso = leido.mensaje,
                        )
                    }
                }
            }
            if (resultado.isFailure || resultado.getOrNull() is ResultadoNucleoVista.Error) {
                pararRefresco()
            } else {
                abrirObjetivoPendiente()
            }
            if (_estado.value.seccion == SeccionServiciosVista.CONTENEDORES) {
                consumir()
            }
        }
    }

    /** Refresco automático solo con la pantalla a la vista y sin solapes. */
    private fun programarRefresco() {
        trabajoRefresco?.cancel()
        val segundos = config().refrescoSeg
        if (segundos <= 0) return
        trabajoRefresco = alcance.launch {
            while (isActive && _estado.value.enHost) {
                delay(segundos * 1000L)
                if (_estado.value.enHost && trabajoListado?.isActive != true) {
                    cargarListado(manual = false)
                }
            }
        }
    }

    private fun pararRefresco() {
        trabajoRefresco?.cancel()
        trabajoRefresco = null
    }

    /** Se para el refresco automático al salir de la pantalla o perder foco. */
    fun alPerderFoco() {
        pararRefresco()
    }

    /** Al volver a la vista (RESUME): relee y reanuda el refresco. */
    fun alVolverALaVista() {
        if (!_estado.value.enHost) {
            cargarTodaLaFlota()
            return
        }
        cargarListado(manual = true)
        programarRefresco()
    }

    fun elegirSeccion(seccion: SeccionServiciosVista) {
        _estado.update { it.copy(seccion = seccion, filtro = "") }
        if (seccion == SeccionServiciosVista.CONTENEDORES) consumir()
    }

    fun cambiarFiltro(texto: String) {
        _estado.update { it.copy(filtro = texto) }
    }

    /** Pide CPU y memoria de los contenedores (solo con Contenedores a la vista). */
    fun consumir() {
        val hostUuid = _estado.value.hostUuid ?: return
        if (trabajoConsumo?.isActive == true) return
        trabajoConsumo = alcance.launch {
            val resultado = runCatching { fuente.serviciosConsumo(hostUuid) }
            _estado.update { actual ->
                val listo = (resultado.getOrNull() as? ResultadoNucleoVista.Listo)?.valor
                if (listo != null) {
                    actual.copy(consumo = listo.associateBy { it.id })
                } else {
                    actual
                }
            }
        }
    }

    /** Abre el detalle de un objetivo (pestaña ESTADO). */
    fun abrirDetalle(objetivo: ObjetivoVista) {
        _estado.update {
            it.copy(objetivo = objetivo, detalle = null, pestana = PestanaDetalleVista.ESTADO, cargandoDetalle = true)
        }
        cargarDetalle()
    }

    fun cambiarPestana(pestana: PestanaDetalleVista) {
        _estado.update { it.copy(pestana = pestana) }
    }

    fun volverALista() {
        trabajoDetalle?.cancel()
        _estado.update { it.copy(objetivo = null, detalle = null) }
    }

    private fun cargarDetalle() {
        val hostUuid = _estado.value.hostUuid ?: return
        val objetivo = _estado.value.objetivo ?: return
        trabajoDetalle?.cancel()
        trabajoDetalle = alcance.launch {
            _estado.update { it.copy(cargandoDetalle = true) }
            val resultado = runCatching { fuente.serviciosDetalle(hostUuid, objetivo) }
            _estado.update { actual ->
                when {
                    resultado.isFailure -> actual.copy(
                        cargandoDetalle = false,
                        aviso = resultado.exceptionOrNull()?.message ?: "no se pudo leer el detalle",
                    )
                    else -> when (val leido = resultado.getOrThrow()) {
                        is ResultadoNucleoVista.Listo -> actual.copy(detalle = leido.valor, cargandoDetalle = false)
                        is ResultadoNucleoVista.Error -> actual.copy(cargandoDetalle = false, aviso = leido.mensaje)
                    }
                }
            }
        }
    }

    /** Refresco manual del detalle abierto (botón REFRESCAR de la hoja). */
    fun refrescarDetalle() {
        cargarDetalle()
        if (_estado.value.enHost) {
            alcance.launch {
                delay(300)
                cargarListado(manual = true)
            }
        }
    }

    /**
     * Prepara una acción con el núcleo. Si no se permite o pide deliberar, se
     * resuelve aquí; nunca decide la app.
     */
    fun pedirAccion(objetivo: ObjetivoVista, accion: AccionServicioVista) {
        val hostUuid = _estado.value.hostUuid ?: return
        if (_estado.value.accionDe(objetivo) != null) {
            _estado.update { it.copy(aviso = "ya hay una acción en curso sobre este objetivo") }
            return
        }
        alcance.launch {
            val resultado = runCatching { fuente.prepararAccion(hostUuid, objetivo, accion) }
            when (val leido = resultado.getOrNull()) {
                is ResultadoNucleoVista.Listo -> {
                    val info = leido.valor
                    when {
                        !info.permitida -> _estado.update {
                            it.copy(aviso = info.motivoNoPermitida ?: "la acción no se permite en este estado")
                        }
                        else -> _estado.update {
                            it.copy(confirmacion = ConfirmacionAccionUi(objetivo, accion, info), aviso = null)
                        }
                    }
                }
                is ResultadoNucleoVista.Error -> _estado.update { it.copy(aviso = leido.mensaje) }
                null -> _estado.update {
                    it.copy(aviso = resultado.exceptionOrNull()?.message ?: "no se pudo preparar la acción")
                }
            }
        }
    }

    /** Plan que hay que deliberar, si la acción lo exige. */
    fun planDeliberacion(): PlanDeliberacionVista? {
        val confirmacion = _estado.value.confirmacion ?: return null
        val hostUuid = _estado.value.hostUuid ?: return null
        if (!confirmacion.info.delibera) return null
        return PlanDeliberacionVista.Servicio(
            hostUuid = hostUuid,
            hostNombre = _estado.value.hostNombre,
            objetivo = confirmacion.objetivo,
            accionServicio = confirmacion.accion,
            comprobacion = confirmacion.info.comprobacion,
        )
    }

    /** Confirma la acción pendiente, con la deliberación que la autoriza si la hay. */
    fun confirmarAccion(deliberacion: Long? = null) {
        val confirmacion = _estado.value.confirmacion ?: return
        // Lo deliberado nunca se ejecuta sin su id: el núcleo lo rechazaría y
        // se perdería la deliberación. La pantalla abre antes el diálogo MAGI.
        if (confirmacion.info.delibera && deliberacion == null) {
            _estado.update { it.copy(aviso = "esta acción necesita deliberación MAGI") }
            return
        }
        if (confirmacion.info.sudoConClave && !_estado.value.sudoEnMemoria) {
            _estado.update {
                it.copy(
                    sudo = EstadoSudoUi(
                        visible = true,
                        hostUuid = it.hostUuid.orEmpty(),
                        hostNombre = it.hostNombre,
                    ),
                    deliberacionPendiente = deliberacion,
                )
            }
            return
        }
        val hostUuid = _estado.value.hostUuid ?: return
        _estado.update { it.copy(confirmacion = null, deliberacionPendiente = null) }
        alcance.launch {
            val resultado = runCatching {
                fuente.servicioAccion(hostUuid, confirmacion.objetivo, confirmacion.accion, deliberacion)
            }
            val hoja = when (val leido = resultado.getOrNull()) {
                is ResultadoNucleoVista.Listo -> leido.valor
                is ResultadoNucleoVista.Error -> ResultadoServicioVista(
                    hostUuid = hostUuid,
                    hostNombre = _estado.value.hostNombre,
                    objetivo = confirmacion.objetivo,
                    accion = confirmacion.accion,
                    resultado = ResultadoAccionServicioVista.ERROR,
                    mensaje = leido.mensaje,
                    comprobacion = confirmacion.info.comprobacion,
                )
                null -> ResultadoServicioVista(
                    hostUuid = hostUuid,
                    hostNombre = _estado.value.hostNombre,
                    objetivo = confirmacion.objetivo,
                    accion = confirmacion.accion,
                    resultado = ResultadoAccionServicioVista.ERROR,
                    mensaje = resultado.exceptionOrNull()?.message ?: "no se pudo ejecutar la acción",
                    comprobacion = confirmacion.info.comprobacion,
                )
            }
            _estado.update { it.copy(resultado = hoja) }
            refrescarTrasAccion(hostUuid)
        }
    }

    fun cancelarConfirmacion() {
        _estado.update { it.copy(confirmacion = null, deliberacionPendiente = null) }
    }

    fun descartarResultado() {
        _estado.update { it.copy(resultado = null) }
    }

    private fun refrescarTrasAccion(hostUuid: String) {
        if (_estado.value.hostUuid != hostUuid) return
        alcance.launch {
            delay(300)
            // Si el usuario cambió de host mientras se esperaba, no se refresca
            // el host nuevo con el resultado de la acción del anterior.
            if (_estado.value.hostUuid != hostUuid) return@launch
            if (_estado.value.enHost) cargarListado(manual = true)
            if (_estado.value.enDetalle) cargarDetalle()
        }
    }

    // ------------------------------------------------------------- sudo

    fun pedirSudo() {
        val hostUuid = _estado.value.hostUuid ?: return
        _estado.update {
            it.copy(sudo = EstadoSudoUi(visible = true, hostUuid = hostUuid, hostNombre = it.hostNombre))
        }
    }

    fun cerrarSudo() {
        _estado.update { it.copy(sudo = it.sudo.copy(visible = false, error = null)) }
    }

    /** Comprueba la contraseña de sudo; 3 intentos y nunca se guarda. */
    fun comprobarSudo(contrasena: ByteArray) {
        var bytes = contrasena
        val hostUuid = _estado.value.sudo.hostUuid
        if (hostUuid.isEmpty()) return
        _estado.update { it.copy(sudo = it.sudo.copy(comprobando = true, error = null)) }
        alcance.launch {
            val resultado = runCatching { fuente.sudoComprobar(hostUuid, bytes) }
            bytes.fill(0)
            // Si mientras se comprobaba se cambió de host, el resultado no vale.
            if (_estado.value.hostUuid != hostUuid) return@launch
            val estadoSudo = _estado.value.sudo
            when (val leido = resultado.getOrNull()) {
                ResultadoSudoVista.OK -> {
                    _estado.update {
                        it.copy(
                            sudo = EstadoSudoUi(),
                            listado = it.listado?.copy(sudoEnMemoria = true),
                        )
                    }
                    if (_estado.value.confirmacion != null) {
                        confirmarAccion(_estado.value.deliberacionPendiente)
                    }
                }
                ResultadoSudoVista.RECHAZADA -> {
                    val intentos = (estadoSudo.intentosRestantes - 1).coerceAtLeast(0)
                    _estado.update {
                        if (intentos <= 0) {
                            // Agotados los 3 intentos: se cierra y se olvida.
                            it.copy(
                                sudo = EstadoSudoUi(),
                                aviso = "la contraseña de sudo se rechazó 3 veces",
                            )
                        } else {
                            it.copy(
                                sudo = estadoSudo.copy(
                                    comprobando = false,
                                    intentosRestantes = intentos,
                                    error = "contraseña rechazada",
                                ),
                            )
                        }
                    }
                }
                ResultadoSudoVista.SIN_SUDO -> _estado.update {
                    it.copy(
                        sudo = EstadoSudoUi(),
                        aviso = "el usuario no puede usar sudo en ese host",
                    )
                }
                is ResultadoSudoVista.ERROR -> _estado.update {
                    it.copy(sudo = estadoSudo.copy(comprobando = false, error = leido.mensaje))
                }
                null -> _estado.update {
                    it.copy(
                        sudo = estadoSudo.copy(
                            comprobando = false,
                            error = resultado.exceptionOrNull()?.message ?: "no se pudo comprobar la contraseña",
                        ),
                    )
                }
            }
        }
    }

    fun olvidarSudo() {
        val hostUuid = _estado.value.hostUuid ?: return
        alcance.launch {
            val resultado = runCatching { fuente.sudoOlvidar(hostUuid) }
            _estado.update {
                it.copy(
                    listado = it.listado?.copy(sudoEnMemoria = false),
                    aviso = if (resultado.getOrNull() is ResultadoAccion.Error) {
                        (resultado.getOrNull() as ResultadoAccion.Error).mensaje
                    } else {
                        "contraseña de sudo olvidada"
                    },
                )
            }
        }
    }

    // -------------------------------------------------- sesiones y logs

    /** Filtro inicial del panel de logs de un objetivo. */
    fun filtroLogs(): FiltroLogsVista = FiltroLogsVista()

    /** El aviso de la barra se descarta al pintarlo. */
    fun descartarAviso() {
        _estado.update { it.copy(aviso = null) }
    }
}
