package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.textoVersion
import org.cuatrodetres.magi.ui.tema.Paleta
import androidx.compose.material3.MaterialTheme

/** Ajustes > Acerca de: versiones de la app y del núcleo, formato, user_version y licencias. */
@Composable
fun PantallaAcercaDe(
    estado: EstadoMagi,
    alVolver: () -> Unit,
    alLicencias: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val version = estado.version
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.acerca_titulo),
            alVolver = alVolver,
        )
        if (version == null) {
            Text(
                text = stringResource(R.string.acerca_sin_nucleo),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.GRIS,
                modifier = Modifier.padding(16.dp),
            )
        } else {
            FilaDato(
                etiqueta = stringResource(R.string.acerca_version_app),
                valor = textoVersion(version.app, version.codigo),
            )
            FilaDato(
                etiqueta = stringResource(R.string.acerca_version_nucleo),
                valor = version.nucleo,
            )
            FilaDato(
                etiqueta = stringResource(R.string.acerca_formato),
                valor = version.formato.toString(),
            )
            FilaDato(
                etiqueta = stringResource(R.string.acerca_user_version),
                valor = version.userVersion.toString(),
            )
        }
        FilaDato(
            etiqueta = stringResource(R.string.acerca_licencia),
            valor = stringResource(R.string.acerca_licencia_valor),
        )
        FilaDato(
            etiqueta = stringResource(R.string.acerca_movil),
            valor = stringResource(R.string.acerca_movil_lector),
        )
        Separador()
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BotonCabina(
                texto = stringResource(R.string.acerca_licencias),
                alPulsar = alLicencias,
                primario = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
