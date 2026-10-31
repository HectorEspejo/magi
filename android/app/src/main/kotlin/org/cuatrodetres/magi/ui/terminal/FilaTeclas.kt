package org.cuatrodetres.magi.ui.terminal

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.modelos.ModoModificador
import org.cuatrodetres.magi.ui.modelos.ModificadoresFila
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Fila deslizable de teclas extra sobre el teclado (informe §6.3 y D183).
 *
 * Ctrl y Alt se encienden con una pulsación para la tecla siguiente y con dos
 * se fijan hasta volver a tocarlos; el estado se ve siempre (ámbar fijo, gris
 * pendiente, apagado inactivo).
 */
@Composable
fun FilaTeclas(
    modificadores: ModificadoresFila,
    alPulsarCtrl: () -> Unit,
    alPulsarAlt: () -> Unit,
    alTecla: (TeclaVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(Paleta.SUPERFICIE)
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 6.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        BotonTecla(
            texto = stringResource(R.string.tecla_ctrl),
            descripcion = stringResource(
                when (modificadores.ctrl) {
                    ModoModificador.NINGUNO -> R.string.tecla_ctrl_apagado
                    ModoModificador.PENDIENTE -> R.string.tecla_ctrl_pendiente
                    ModoModificador.FIJO -> R.string.tecla_ctrl_fijo
                },
            ),
            activa = modificadores.ctrlActivo,
            alPulsar = alPulsarCtrl,
        )
        BotonTecla(
            texto = stringResource(R.string.tecla_alt),
            descripcion = stringResource(
                when (modificadores.alt) {
                    ModoModificador.NINGUNO -> R.string.tecla_alt_apagado
                    ModoModificador.PENDIENTE -> R.string.tecla_alt_pendiente
                    ModoModificador.FIJO -> R.string.tecla_alt_fijo
                },
            ),
            activa = modificadores.altActivo,
            alPulsar = alPulsarAlt,
        )
        TeclasBasicas.forEach { basica ->
            val etiqueta = stringResource(basica.etiqueta)
            BotonTecla(
                texto = etiqueta,
                descripcion = etiqueta,
                alPulsar = { alTecla(basica.tecla) },
            )
        }
        (1..12).forEach { numero ->
            BotonTecla(
                texto = "F" + numero,
                descripcion = "F" + numero,
                alPulsar = { alTecla(TeclaVista.F(numero)) },
            )
        }
    }
}

/** Tecla fija de la fila con su etiqueta en strings.xml. */
private data class TeclaBasica(
    @param:StringRes val etiqueta: Int,
    val tecla: TeclaVista,
)

/** Teclas fijas de la fila, en el orden del mockup (§6.3). */
private val TeclasBasicas: List<TeclaBasica> = listOf(
    TeclaBasica(R.string.tecla_escape, TeclaVista.Escape),
    TeclaBasica(R.string.tecla_tab, TeclaVista.Tab),
    TeclaBasica(R.string.tecla_izquierda, TeclaVista.Izquierda),
    TeclaBasica(R.string.tecla_arriba, TeclaVista.Arriba),
    TeclaBasica(R.string.tecla_abajo, TeclaVista.Abajo),
    TeclaBasica(R.string.tecla_derecha, TeclaVista.Derecha),
    TeclaBasica(R.string.tecla_barra_vertical, TeclaVista.Caracter("|")),
    TeclaBasica(R.string.tecla_virgulilla, TeclaVista.Caracter("~")),
    TeclaBasica(R.string.tecla_barra, TeclaVista.Caracter("/")),
    TeclaBasica(R.string.tecla_guion, TeclaVista.Caracter("-")),
    TeclaBasica(R.string.tecla_inicio, TeclaVista.Inicio),
    TeclaBasica(R.string.tecla_fin, TeclaVista.Fin),
    TeclaBasica(R.string.tecla_repag, TeclaVista.RePag),
    TeclaBasica(R.string.tecla_avpag, TeclaVista.AvPag),
)

@Composable
private fun BotonTecla(
    texto: String,
    descripcion: String,
    alPulsar: () -> Unit,
    activa: Boolean = false,
) {
    Row(
        modifier = Modifier
            .heightIn(min = 44.dp)
            .sizeIn(minWidth = 44.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(if (activa) Paleta.AMBAR else Paleta.FONDO)
            .clickable(onClickLabel = descripcion, onClick = alPulsar)
            .semantics { contentDescription = descripcion }
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            text = texto,
            style = MaterialTheme.typography.labelLarge,
            color = if (activa) Paleta.FONDO else Paleta.TEXTO,
        )
    }
}
