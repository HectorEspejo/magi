package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringArrayResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Ajustes > Batería: guía para sacar MAGI del ahorro de batería agresivo de
 * MagicOS (informe §4.6). Se ofrece al abrir la primera sesión y siempre aquí.
 */
@Composable
fun PantallaBateria(
    alVolver: () -> Unit,
    alAbrirAjustesBateria: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val pasos = stringArrayResource(R.array.bateria_pasos)
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.ajustes_bateria),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.bateria_explicacion),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.TEXTO,
            )
            pasos.forEachIndexed { indice, paso ->
                Text(
                    text = (indice + 1).toString() + ". " + paso,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.TEXTO,
                )
            }
            Text(
                text = stringResource(R.string.bateria_nota),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
            BotonCabina(
                texto = stringResource(R.string.bateria_abrir_ajustes),
                alPulsar = alAbrirAjustesBateria,
                primario = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
