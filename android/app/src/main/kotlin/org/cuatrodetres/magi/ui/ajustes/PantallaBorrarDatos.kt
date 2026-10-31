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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Ajustes > Borrar todos los datos de MAGI.
 *
 * La confirmación es escribir «borrar» (checklist de interfaz): se borran el
 * inventario, la clave de sincronización, las contraseñas, las huellas y la
 * clave del móvil. No se puede deshacer y se avisa en rojo.
 */
@Composable
fun PantallaBorrarDatos(
    alVolver: () -> Unit,
    alBorrar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var texto by remember { mutableStateOf("") }
    val confirmado = texto.trim().lowercase() == "borrar"
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.ajustes_borrar_datos),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BarraAviso(
                texto = stringResource(R.string.borrar_aviso),
                tipo = TipoBarra.ERROR,
            )
            Text(
                text = stringResource(R.string.borrar_lista),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.TEXTO,
            )
            Text(
                text = stringResource(R.string.borrar_confirmacion),
                style = MaterialTheme.typography.labelLarge,
                color = Paleta.ROJO,
            )
            CampoTextoMagi(
                valor = texto,
                alCambiar = { texto = it },
                etiqueta = stringResource(R.string.borrar_etiqueta),
            )
            BotonCabina(
                texto = stringResource(R.string.borrar_accion),
                alPulsar = {
                    if (confirmado) {
                        alBorrar()
                        texto = ""
                    }
                },
                habilitado = confirmado,
                destructivo = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
