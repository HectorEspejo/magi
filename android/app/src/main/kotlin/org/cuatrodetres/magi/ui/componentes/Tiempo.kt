package org.cuatrodetres.magi.ui.componentes

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.modelos.UnidadTemporal
import org.cuatrodetres.magi.ui.modelos.ahoraSegundos
import org.cuatrodetres.magi.ui.modelos.formatoFecha
import org.cuatrodetres.magi.ui.modelos.tiempoRelativo
import org.cuatrodetres.magi.ui.tema.Paleta

/** Texto relativo: «hace 2 min», «hace 1 h» o «ahora» (sincronización y sesiones). */
@Composable
fun TextoHace(
    instanteSegundos: Long?,
    modifier: Modifier = Modifier,
    color: Color = Paleta.GRIS,
) {
    val relativo = tiempoRelativo(instanteSegundos, ahoraSegundos())
    val texto = when {
        relativo == null -> stringResource(R.string.tiempo_desconocido)
        relativo.unidad == UnidadTemporal.SEGUNDOS && relativo.cantidad <= 5 ->
            stringResource(R.string.tiempo_ahora)
        relativo.unidad == UnidadTemporal.SEGUNDOS ->
            stringResource(R.string.tiempo_hace_segundos, relativo.cantidad)
        relativo.unidad == UnidadTemporal.MINUTOS ->
            stringResource(R.string.tiempo_hace_minutos, relativo.cantidad)
        relativo.unidad == UnidadTemporal.HORAS ->
            stringResource(R.string.tiempo_hace_horas, relativo.cantidad)
        else -> stringResource(R.string.tiempo_hace_dias, relativo.cantidad)
    }
    Text(
        text = texto,
        style = MaterialTheme.typography.labelMedium,
        color = color,
        modifier = modifier,
    )
}

/** Fecha y hora local de un instante epoch, o «desconocido». */
@Composable
fun TextoFecha(
    instanteSegundos: Long?,
    modifier: Modifier = Modifier,
    color: Color = Paleta.GRIS,
) {
    val texto = formatoFecha(instanteSegundos) ?: stringResource(R.string.tiempo_desconocido)
    Text(
        text = texto,
        style = MaterialTheme.typography.labelMedium,
        color = color,
        modifier = modifier,
    )
}
