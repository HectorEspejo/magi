package org.cuatrodetres.magi.ui.resultados

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.HostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.SalidaEjecucionVista
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/** Pestaña de la salida de un host (stdout o stderr). */
enum class PestanaSalida { STDOUT, STDERR }

/** Avisos que produce la pantalla de Resultados (sus textos viven en strings.xml). */
enum class AvisoResultados { LIMPIADAS, REPETIDA }

/** Hora corta local de un instante epoch, para la fila de la lista. */
internal fun horaCorta(instanteSegundos: Long): String {
    if (instanteSegundos <= 0) return "—"
    return DateTimeFormatter.ofPattern("HH:mm")
        .format(Instant.ofEpochSecond(instanteSegundos).atZone(ZoneId.systemDefault()))
}

/**
 * Estado de Resultados (informe §6.9): ejecuciones en curso y terminadas, la
 * ejecución y el host abiertos y su salida.
 */
data class EstadoResultadosUi(
    val ejecuciones: List<EjecucionVista> = emptyList(),
    val cargando: Boolean = false,
    val seleccionadaId: Long? = null,
    val hostSeleccionado: String? = null,
    val salida: SalidaEjecucionVista? = null,
    val cargandoSalida: Boolean = false,
    val pestana: PestanaSalida = PestanaSalida.STDOUT,
    /** Ejecución cuya cancelación está esperando confirmación. */
    val cancelacionPendiente: Long? = null,
    val planDeliberacion: PlanDeliberacionVista? = null,
    val aviso: AvisoResultados? = null,
    val error: String? = null,
) {
    /** Ejecución abierta (se relee del flujo completo). */
    val seleccionada: EjecucionVista?
        get() = seleccionadaId?.let { id -> ejecuciones.firstOrNull { it.id == id } }

    /** Host abierto dentro de la ejecución. */
    val host: HostEjecucionVista?
        get() = hostSeleccionado?.let { uuid -> seleccionada?.hosts?.firstOrNull { it.hostUuid == uuid } }

    /** Ejecuciones que siguen en curso. */
    val enCurso: Int get() = ejecuciones.count { it.enCurso }

    /** Texto del flujo que se está enseñando. */
    val textoSalida: String
        get() = when (pestana) {
            PestanaSalida.STDOUT -> salida?.stdout.orEmpty()
            PestanaSalida.STDERR -> salida?.stderr.orEmpty()
        }

    /** ¿La salida del flujo enseñado se truncó a 1 MiB? */
    val salidaTruncada: Boolean
        get() = when (pestana) {
            PestanaSalida.STDOUT -> salida?.stdoutTruncada == true
            PestanaSalida.STDERR -> salida?.stderrTruncada == true
        }
}

/**
 * ViewModel de Resultados (D269): recibe las ejecuciones por el flujo completo
 * del núcleo, pide la salida de un host a demanda, cancela con confirmación,
 * repite (volviendo a deliberar si hace falta) y limpia las terminadas.
 */
class VistaModeloResultados(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoResultadosUi(cargando = true))
    val estado: StateFlow<EstadoResultadosUi> = _estado.asStateFlow()

    private var iniciado = false

    init {
        alcance.launch {
            fuente.ejecuciones.collect { lista ->
                _estado.update { actual ->
                    actual.copy(
                        ejecuciones = lista,
                        cargando = false,
                        // Una ejecución que ya no está (limpiada o caducada) se cierra.
                        seleccionadaId = actual.seleccionadaId
                            ?.takeIf { id -> lista.any { it.id == id } },
                    )
                }
            }
        }
    }

    /** Carga la lista completa (el flujo la mantiene al día después). */
    fun iniciar() {
        if (iniciado) return
        iniciado = true
        _estado.update { it.copy(cargando = true) }
        alcance.launch {
            try {
                val lista = fuente.ejecucionesLista()
                _estado.update { it.copy(ejecuciones = lista, cargando = false) }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        cargando = false,
                        error = error.message ?: "no se pudieron leer las ejecuciones",
                    )
                }
            }
        }
    }

    fun recargar() {
        iniciado = false
        iniciar()
    }

    /** Abre una ejecución; se cierra el host y la salida anteriores. */
    fun seleccionar(id: Long) {
        _estado.update {
            it.copy(
                seleccionadaId = id,
                hostSeleccionado = null,
                salida = null,
                pestana = PestanaSalida.STDOUT,
                error = null,
            )
        }
    }

    /** Cierra el detalle y vuelve a la lista. */
    fun cerrarDetalle() {
        _estado.update {
            it.copy(
                seleccionadaId = null,
                hostSeleccionado = null,
                salida = null,
                cancelacionPendiente = null,
            )
        }
    }

    /** Pide al núcleo la salida de un host (ya sin secuencias de escape). */
    fun seleccionarHost(hostUuid: String) {
        val id = _estado.value.seleccionadaId ?: return
        _estado.update {
            it.copy(
                hostSeleccionado = hostUuid,
                salida = null,
                cargandoSalida = true,
                pestana = PestanaSalida.STDOUT,
                error = null,
            )
        }
        alcance.launch {
            try {
                when (val leido = fuente.salidaEjecucion(id, hostUuid)) {
                    is ResultadoNucleoVista.Listo -> _estado.update {
                        it.copy(salida = leido.valor, cargandoSalida = false)
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(cargandoSalida = false, error = leido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        cargandoSalida = false,
                        error = error.message ?: "no se pudo leer la salida",
                    )
                }
            }
        }
    }

    fun cambiarPestana(pestana: PestanaSalida) {
        _estado.update { it.copy(pestana = pestana) }
    }

    /** CANCELAR: primero se confirma (T6). */
    fun pedirCancelar(id: Long) {
        _estado.update { it.copy(cancelacionPendiente = id) }
    }

    fun cancelarConfirmacion() {
        _estado.update { it.copy(cancelacionPendiente = null) }
    }

    fun confirmarCancelar() {
        val id = _estado.value.cancelacionPendiente ?: return
        _estado.update { it.copy(cancelacionPendiente = null) }
        alcance.launch {
            try {
                when (val leido = fuente.cancelarEjecucion(id)) {
                    ResultadoAccion.Ok -> Unit
                    is ResultadoAccion.Error -> _estado.update { it.copy(error = leido.mensaje) }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(error = error.message ?: "no se pudo cancelar la ejecución")
                }
            }
        }
    }

    /** REPETIR: prepara otra vez; vuelve a deliberar si el núcleo lo pide. */
    fun repetir(id: Long) {
        alcance.launch {
            try {
                when (val leido = fuente.repetirEjecucion(id)) {
                    is ResultadoNucleoVista.Listo -> {
                        val preparacion = leido.valor
                        if (preparacion.delibera) {
                            _estado.update {
                                it.copy(planDeliberacion = PlanDeliberacionVista.Snippet(preparacion))
                            }
                        } else {
                            ejecutar(preparacion, null)
                        }
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(error = leido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(error = error.message ?: "no se pudo repetir la ejecución")
                }
            }
        }
    }

    /** Lanza la preparación repetida, con la deliberación que la autoriza si la hay. */
    fun ejecutar(preparacion: PreparacionEjecucionVista, deliberacion: Long?) {
        alcance.launch {
            try {
                when (val leido = fuente.ejecutarSnippet(
                    preparacion,
                    preparacion.pararAlFallo,
                    deliberacion,
                )) {
                    is ResultadoNucleoVista.Listo -> _estado.update {
                        it.copy(
                            seleccionadaId = leido.valor,
                            hostSeleccionado = null,
                            salida = null,
                            aviso = AvisoResultados.REPETIDA,
                        )
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(error = leido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(error = error.message ?: "no se pudo lanzar la ejecución")
                }
            }
        }
    }

    /** Ejecuta lo deliberado y aprobado o forzado, con su id de DELIBERACIONES. */
    fun ejecutarDeliberado(plan: PlanDeliberacionVista, deliberacion: Long) {
        _estado.update { it.copy(planDeliberacion = null) }
        if (plan is PlanDeliberacionVista.Snippet) {
            ejecutar(plan.preparacion, deliberacion)
        }
    }

    fun cancelarDeliberacion() {
        _estado.update { it.copy(planDeliberacion = null) }
    }

    /** LIMPIAR: olvida las ejecuciones terminadas. */
    fun limpiar() {
        alcance.launch {
            try {
                when (val leido = fuente.limpiarEjecuciones()) {
                    ResultadoAccion.Ok -> _estado.update {
                        it.copy(aviso = AvisoResultados.LIMPIADAS, seleccionadaId = null)
                    }
                    is ResultadoAccion.Error -> _estado.update { it.copy(error = leido.mensaje) }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(error = error.message ?: "no se pudieron limpiar las ejecuciones")
                }
            }
        }
    }

    fun descartarAviso() {
        _estado.update { it.copy(aviso = null) }
    }

    fun limpiarError() {
        _estado.update { it.copy(error = null) }
    }
}
