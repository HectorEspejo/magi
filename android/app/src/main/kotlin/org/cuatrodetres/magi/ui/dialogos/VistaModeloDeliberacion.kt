package org.cuatrodetres.magi.ui.dialogos

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/**
 * Estado del diálogo MAGI (informe §6.6, §6.7 y §7.7): el plan que se delibera,
 * la tabla que devuelve el núcleo y el error si no se pudo deliberar.
 */
data class EstadoDeliberacionUi(
    val plan: PlanDeliberacionVista? = null,
    val tabla: TablaDeliberacionVista? = null,
    val cargando: Boolean = false,
    val resolviendo: Boolean = false,
    val error: String? = null,
)

/**
 * ViewModel del diálogo de deliberación (D270, D271): pide la tabla al núcleo
 * (las comprobaciones corren allí, en paralelo) y resuelve la deliberación
 * (aprobar, forzar o cancelar) cuando el usuario mantiene pulsado.
 *
 * No decide veredictos ni consenso: los pinta tal como llegan.
 */
class VistaModeloDeliberacion(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoDeliberacionUi())
    val estado: StateFlow<EstadoDeliberacionUi> = _estado.asStateFlow()

    private var planActual: PlanDeliberacionVista? = null

    /** Abre la deliberación de un plan: lanza las comprobaciones del núcleo. */
    fun abrir(plan: PlanDeliberacionVista) {
        planActual = plan
        _estado.value = EstadoDeliberacionUi(plan = plan, cargando = true)
        alcance.launch {
            try {
                when (val leido = fuente.deliberar(plan)) {
                    is ResultadoNucleoVista.Listo -> _estado.update {
                        it.copy(tabla = leido.valor, cargando = false, error = null)
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(cargando = false, error = leido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        cargando = false,
                        error = error.message ?: "no se pudo deliberar",
                    )
                }
            }
        }
    }

    /** Vuelve a lanzar las comprobaciones del mismo plan. */
    fun reintentar() {
        planActual?.let(::abrir)
    }

    /**
     * Resuelve la deliberación: aprobar, forzar con motivo o cancelar. Al
     * aprobar o forzar entrega en alResuelto el id de DELIBERACIONES que
     * autoriza la acción o la ejecución; null si no se pudo resolver.
     */
    fun resolver(
        decision: DecisionDeliberacionVista,
        alResuelto: (Long?) -> Unit = {},
    ) {
        val tabla = _estado.value.tabla ?: return alResuelto(null)
        if (_estado.value.resolviendo) return
        _estado.update { it.copy(resolviendo = true, error = null) }
        alcance.launch {
            try {
                when (val leido = fuente.resolverDeliberacion(tabla.id, decision)) {
                    is ResultadoNucleoVista.Listo -> {
                        _estado.update { it.copy(resolviendo = false) }
                        alResuelto(leido.valor)
                    }
                    is ResultadoNucleoVista.Error -> {
                        _estado.update { it.copy(resolviendo = false, error = leido.mensaje) }
                        alResuelto(null)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        resolviendo = false,
                        error = error.message ?: "no se pudo resolver la deliberación",
                    )
                }
                alResuelto(null)
            }
        }
    }

    /** Cancela la deliberación (el núcleo anota deliberacion_cancelada). */
    fun cancelar(alCancelar: () -> Unit) {
        val tabla = _estado.value.tabla
        if (tabla == null) {
            cerrar()
            alCancelar()
            return
        }
        _estado.update { it.copy(resolviendo = true) }
        alcance.launch {
            runCatching {
                fuente.resolverDeliberacion(tabla.id, DecisionDeliberacionVista.Cancelar)
            }
            cerrar()
            alCancelar()
        }
    }

    /** Cierra el diálogo y olvida la tabla. */
    fun cerrar() {
        planActual = null
        _estado.value = EstadoDeliberacionUi()
    }
}
