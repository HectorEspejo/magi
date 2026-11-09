package org.cuatrodetres.magi.ui.mas

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.navegacion.SeccionMas
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla MÁS (D265): agrupa Snippets, Resultados, Registro y Ajustes, que no
 * caben en la barra de cinco destinos. Con una sección elegida, el contenido lo
 * pinta su pantalla (o, en plegable, el panel derecho).
 */
@Composable
fun PantallaMas(
    seccion: SeccionMas?,
    alElegir: (SeccionMas) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraMagi(titulo = stringResource(R.string.mas_titulo))
        if (seccion == null) {
            Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState())) {
                FilaMas(
                    glifo = "⌘",
                    titulo = stringResource(R.string.mas_snippets),
                    detalle = stringResource(R.string.mas_snippets_detalle),
                    alPulsar = { alElegir(SeccionMas.SNIPPETS) },
                )
                FilaMas(
                    glifo = "▤",
                    titulo = stringResource(R.string.mas_resultados),
                    detalle = stringResource(R.string.mas_resultados_detalle),
                    alPulsar = { alElegir(SeccionMas.RESULTADOS) },
                )
                FilaMas(
                    glifo = "▦",
                    titulo = stringResource(R.string.mas_registro),
                    detalle = stringResource(R.string.mas_registro_detalle),
                    alPulsar = { alElegir(SeccionMas.REGISTRO) },
                )
                FilaMas(
                    glifo = "⚙",
                    titulo = stringResource(R.string.destino_ajustes),
                    detalle = stringResource(R.string.mas_ajustes_detalle),
                    alPulsar = { alElegir(SeccionMas.AJUSTES) },
                )
            }
        } else {
            MensajeVacio(
                titulo = stringResource(seccion.etiqueta),
                detalle = stringResource(R.string.en_construccion),
                modifier = Modifier.weight(1f),
            )
        }
    }
}

/** Etiqueta de la sección en la barra y en el panel. */
private val SeccionMas.etiqueta: Int
    get() = when (this) {
        SeccionMas.SNIPPETS -> R.string.mas_snippets
        SeccionMas.RESULTADOS -> R.string.mas_resultados
        SeccionMas.REGISTRO -> R.string.mas_registro
        SeccionMas.AJUSTES -> R.string.destino_ajustes
    }

/** Fila de MÁS con glifo, título, detalle y flecha. */
@Composable
private fun FilaMas(
    glifo: String,
    titulo: String,
    detalle: String,
    alPulsar: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .background(Paleta.FONDO)
            .clickable(onClickLabel = titulo, onClick = alPulsar)
            .semantics(mergeDescendants = true) { contentDescription = titulo + ", " + detalle }
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = glifo,
            style = MaterialTheme.typography.titleMedium,
            color = Paleta.GRIS,
            modifier = Modifier.widthIn(min = 32.dp),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(text = titulo, style = MaterialTheme.typography.titleSmall, color = Paleta.TEXTO)
            Text(text = detalle, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
        }
        Text(text = "›", style = MaterialTheme.typography.titleMedium, color = Paleta.GRIS)
    }
}
