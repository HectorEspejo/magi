package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.modelos.ZOOM_MAXIMO
import org.cuatrodetres.magi.ui.modelos.ZOOM_MINIMO
import org.cuatrodetres.magi.ui.tema.FamiliaJetBrainsMono
import org.cuatrodetres.magi.ui.tema.Paleta

/** Ajustes > Terminal: tamaño de letra por defecto del terminal (8-24 sp). */
@Composable
fun PantallaAjustesTerminal(
    zoom: Int,
    alCambiarZoom: (Int) -> Unit,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = stringResource(R.string.ajustes_terminal),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = stringResource(R.string.terminal_zoom_actual, zoom),
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.AMBAR,
            )
            Slider(
                value = zoom.toFloat(),
                onValueChange = { alCambiarZoom(it.toInt()) },
                valueRange = ZOOM_MINIMO.toFloat()..ZOOM_MAXIMO.toFloat(),
                steps = ZOOM_MAXIMO - ZOOM_MINIMO - 1,
                colors = SliderDefaults.colors(
                    thumbColor = Paleta.AMBAR,
                    activeTrackColor = Paleta.AMBAR,
                    inactiveTrackColor = Paleta.GRIS,
                ),
            )
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text(
                    text = stringResource(R.string.terminal_zoom_minimo),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.GRIS,
                )
                Text(
                    text = stringResource(R.string.terminal_zoom_maximo),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.GRIS,
                )
            }
            Text(
                text = stringResource(R.string.terminal_zoom_ejemplo),
                style = MaterialTheme.typography.bodyMedium.copy(
                    fontFamily = FamiliaJetBrainsMono,
                    fontSize = zoom.sp,
                    fontWeight = FontWeight.Normal,
                ),
                color = Paleta.TEXTO,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Paleta.SUPERFICIE)
                    .padding(12.dp),
            )
            Text(
                text = stringResource(R.string.terminal_zoom_nota),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.GRIS,
            )
        }
    }
}
