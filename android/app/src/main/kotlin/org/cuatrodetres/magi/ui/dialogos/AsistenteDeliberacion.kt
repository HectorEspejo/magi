package org.cuatrodetres.magi.ui.dialogos

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.EstadoDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista

/**
 * Pegamento entre las pantallas y el diálogo MAGI: con un plan no nulo pide la
 * deliberación a su ViewModel y pinta DialogoDeliberacion.
 *
 * Cuando el usuario mantiene pulsado, resuelve la deliberación (aprobar o
 * forzar) y entrega en alEjecutar el plan y el id de DELIBERACIONES que lo
 * autoriza; la pantalla que la abrió es la que lanza la acción o la ejecución.
 * Cancelar anota deliberacion_cancelada y llama a alCancelar.
 */
@Composable
fun AsistenteDeliberacion(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    plan: PlanDeliberacionVista?,
    textoMantener: String,
    alCancelar: () -> Unit,
    alEjecutar: (PlanDeliberacionVista, Long) -> Unit,
    modifier: Modifier = Modifier,
    clave: String = "magi-deliberacion",
    vistaModelo: VistaModeloDeliberacion = viewModel(
        key = clave,
        factory = viewModelFactory { initializer { VistaModeloDeliberacion(fuente) } },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()

    // Cada plan nuevo (re)abre la deliberación; sin plan, se cierra.
    LaunchedEffect(plan) {
        if (plan != null) vistaModelo.abrir(plan) else vistaModelo.cerrar()
    }

    val actual = plan ?: return
    val tabla = estado.tabla ?: TablaDeliberacionVista(estado = EstadoDeliberacionVista.COMPROBANDO)
    DialogoDeliberacion(
        accion = actual.accion,
        tabla = tabla,
        textoMantener = textoMantener,
        alCancelar = { vistaModelo.cancelar(alCancelar) },
        alEjecutar = { decision ->
            vistaModelo.resolver(decision) { id ->
                if (id != null) alEjecutar(actual, id)
            }
        },
        modifier = modifier,
        comprobacion = actual.comprobacion,
        cargando = estado.cargando,
        error = estado.error,
    )
}

/**
 * Igual que [AsistenteDeliberacion] pero con el contrato de resolutor que usan
 * Servicios y las pantallas que ya tienen un callback pendiente: al aprobar o
 * forzar entrega el id de DELIBERACIONES y al cancelar entrega null, una sola
 * vez. Es la forma más corta de cablear el diálogo desde AppMagi.
 */
@Composable
fun AsistenteDeliberacionResolutor(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    plan: PlanDeliberacionVista?,
    textoMantener: String,
    alResolver: (Long?) -> Unit,
    modifier: Modifier = Modifier,
    clave: String = "magi-deliberacion",
) {
    AsistenteDeliberacion(
        fuente = fuente,
        plan = plan,
        textoMantener = textoMantener,
        alCancelar = { alResolver(null) },
        alEjecutar = { _, id -> alResolver(id) },
        modifier = modifier,
        clave = clave,
    )
}
