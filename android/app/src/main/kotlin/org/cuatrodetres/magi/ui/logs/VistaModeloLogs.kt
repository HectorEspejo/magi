package org.cuatrodetres.magi.ui.logs

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.EstadoLogsVista
import org.cuatrodetres.magi.ui.modelos.EstadoSuscripcionLogVista
import org.cuatrodetres.magi.ui.modelos.FiltroLogsVista
import org.cuatrodetres.magi.ui.modelos.LineaLogVista
import org.cuatrodetres.magi.ui.modelos.MotivoFinLogVista
import org.cuatrodetres.magi.ui.modelos.NivelLogVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/** Estado del panel de logs (informe §6.5 y §7.4). */
data class EstadoLogsUi(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val objetivo: ObjetivoVista = ObjetivoVista(),
    val suscripcion: Long? = null,
    val lineas: List<LineaLogVista> = emptyList(),
    val total: Long = 0,
    val omitidas: Long = 0,
    val estado: EstadoSuscripcionLogVista = EstadoSuscripcionLogVista.ABRIENDO,
    val motivo: MotivoFinLogVista? = null,
    val filtroTexto: String = "",
    val nivel: NivelLogVista = NivelLogVista.TODO,
    val soloStderr: Boolean = false,
    val pausada: Boolean = false,
    val nuevas: Long = 0,
    val cargando: Boolean = true,
    val aviso: String? = null,
) {
    /** Llegan líneas nuevas y se pintan al final. */
    val siguiendo: Boolean get() = !pausada && !estado.esFinal

    /** Se puede parar la suscripción: hay canal abierto. */
    val sePuedeParar: Boolean get() = suscripcion != null && !estado.esFinal

    /** Cabecera: «siguiendo · 1 214 líneas · 14 omitidas». */
    val cabecera: String
        get() {
            val partes = mutableListOf(estado.texto)
            if (total > 0) partes += total.toString() + " líneas"
            if (omitidas > 0) partes += omitidas.toString() + " omitidas"
            return partes.joinToString(" · ")
        }
}

private const val MAXIMO_LINEAS_EN_VISTA = 2_000

/**
 * ViewModel de Logs: abre la suscripción, pide ventanas de hasta 500 líneas ya
 * filtradas y las pinta. La vista en pausa se ancla al último número recibido,
 * así las líneas nuevas no la mueven; al volver al final se pide otra ventana.
 * El anillo y los límites viven en el núcleo.
 */
class VistaModeloLogs(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoLogsUi())
    val estado: StateFlow<EstadoLogsUi> = _estado.asStateFlow()

    private var trabajoAvisos: Job? = null
    private var trabajoVentana: Job? = null
    private var iniciado = false

    /** Abre la suscripción de logs del objetivo. Idempotente. */
    fun iniciar(hostUuid: String, hostNombre: String, objetivo: ObjetivoVista) {
        if (iniciado) return
        iniciado = true
        _estado.value = EstadoLogsUi(hostUuid = hostUuid, hostNombre = hostNombre, objetivo = objetivo)
        alcance.launch {
            val resultado = runCatching {
                fuente.logsSeguir(hostUuid, objetivo, FiltroLogsVista.LINEAS_INICIALES)
            }
            when (val leido = resultado.getOrNull()) {
                is ResultadoNucleoVista.Listo -> {
                    _estado.update {
                        it.copy(suscripcion = leido.valor, estado = EstadoSuscripcionLogVista.SIGUIENDO)
                    }
                    cargarVentana(desde = null)
                    escucharAvisos()
                }
                is ResultadoNucleoVista.Error -> _estado.update {
                    it.copy(cargando = false, estado = EstadoSuscripcionLogVista.CONEXION_CAIDA, aviso = leido.mensaje)
                }
                null -> _estado.update {
                    it.copy(
                        cargando = false,
                        estado = EstadoSuscripcionLogVista.CONEXION_CAIDA,
                        aviso = resultado.exceptionOrNull()?.message ?: "no se pudo abrir la suscripción",
                    )
                }
            }
        }
    }

    private fun escucharAvisos() {
        trabajoAvisos?.cancel()
        trabajoAvisos = alcance.launch {
            fuente.avisosLogs.collect { aviso: EstadoLogsVista ->
                val suscripcion = _estado.value.suscripcion ?: return@collect
                if (aviso.suscripcion != suscripcion) return@collect
                when {
                    aviso.estado.esFinal -> _estado.update {
                        it.copy(estado = aviso.estado, cargando = false)
                    }
                    _estado.value.pausada -> _estado.update {
                        it.copy(nuevas = it.nuevas + aviso.nuevas, total = aviso.total, omitidas = aviso.omitidas)
                    }
                    aviso.hayNuevas -> cargarVentana(desde = ultimaLinea())
                    else -> _estado.update { it.copy(total = aviso.total, omitidas = aviso.omitidas) }
                }
            }
        }
    }

    private fun ultimaLinea(): Long? = _estado.value.lineas.lastOrNull()?.numero

    private fun cargarVentana(desde: Long?) {
        val suscripcion = _estado.value.suscripcion ?: return
        if (trabajoVentana?.isActive == true) return
        trabajoVentana = alcance.launch {
            val filtro = FiltroLogsVista(
                texto = _estado.value.filtroTexto,
                nivel = _estado.value.nivel,
                soloStderr = _estado.value.soloStderr,
                desde = desde,
                cuantas = if (desde == null) 200 else 200,
            )
            val resultado = runCatching { fuente.logsVentana(suscripcion, filtro) }
            when (val leido = resultado.getOrNull()) {
                is ResultadoNucleoVista.Listo -> aplicarVentana(leido.valor, desde)
                is ResultadoNucleoVista.Error -> _estado.update {
                    it.copy(cargando = false, aviso = leido.mensaje)
                }
                null -> _estado.update {
                    it.copy(
                        cargando = false,
                        aviso = resultado.exceptionOrNull()?.message ?: "no se pudo leer la ventana de logs",
                    )
                }
            }
        }
    }

    private fun aplicarVentana(ventana: VentanaLogsVista, desde: Long?) {
        _estado.update { actual ->
            val lineas: List<LineaLogVista> = if (desde == null) {
                ventana.lineas
            } else {
                val ancla = actual.lineas.lastOrNull()?.numero ?: desde
                val nuevas = ventana.lineas.filter { it.numero > ancla }
                (actual.lineas + nuevas).takeLast(MAXIMO_LINEAS_EN_VISTA)
            }
            actual.copy(
                lineas = lineas,
                total = ventana.total,
                omitidas = ventana.omitidas,
                estado = ventana.estado,
                motivo = ventana.motivo,
                cargando = false,
                nuevas = 0,
                aviso = null,
            )
        }
    }

    fun cambiarFiltro(texto: String) {
        _estado.update { it.copy(filtroTexto = texto) }
        recargar()
    }

    fun cambiarNivel() {
        _estado.update { it.copy(nivel = it.nivel.siguiente()) }
        recargar()
    }

    fun alternarSoloStderr() {
        _estado.update { it.copy(soloStderr = !it.soloStderr) }
        recargar()
    }

    private fun recargar() {
        _estado.update { it.copy(lineas = emptyList(), cargando = true) }
        cargarVentana(desde = null)
    }

    /** Pausa al desplazarse hacia arriba; al reanudar, se va al final. */
    fun alternarPausa() {
        if (_estado.value.pausada) {
            alFinal()
        } else {
            _estado.update { it.copy(pausada = true, estado = EstadoSuscripcionLogVista.EN_PAUSA) }
        }
    }

    fun alFinal() {
        _estado.update {
            it.copy(pausada = false, nuevas = 0, estado = EstadoSuscripcionLogVista.SIGUIENDO)
        }
        cargarVentana(desde = null)
    }

    /** Cierra el canal (fin de la entrada estándar); las líneas se quedan. */
    fun parar() {
        val suscripcion = _estado.value.suscripcion ?: return
        alcance.launch {
            val resultado = runCatching { fuente.logsParar(suscripcion) }
            val aviso = (resultado.getOrNull() as? ResultadoAccion.Error)?.mensaje
            _estado.update {
                it.copy(
                    estado = EstadoSuscripcionLogVista.PARADA,
                    motivo = MotivoFinLogVista.CERRADO,
                    pausada = false,
                    aviso = aviso,
                )
            }
        }
    }

    /** Para la suscripción y suelta sus tareas al salir de la pantalla. */
    fun cerrar() {
        if (_estado.value.sePuedeParar) parar()
        trabajoAvisos?.cancel()
        trabajoVentana?.cancel()
    }

    fun descartarAviso() {
        _estado.update { it.copy(aviso = null) }
    }
}
