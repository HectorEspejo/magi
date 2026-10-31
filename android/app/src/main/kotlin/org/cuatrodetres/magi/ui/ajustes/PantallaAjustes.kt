package org.cuatrodetres.magi.ui.ajustes

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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.navegacion.SeccionAjustes
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.textoFaseSincronizacion
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.huellaCorta
import org.cuatrodetres.magi.ui.modelos.textoVersion
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla Ajustes (informe §6 y checklist de interfaz): sincronización, clave
 * del móvil, terminal, batería, borrar datos y acerca de (con licencias).
 */
@Composable
fun PantallaAjustes(
    estado: EstadoMagi,
    alAbrirSeccion: (SeccionAjustes) -> Unit,
    modifier: Modifier = Modifier,
) {
    val clave = estado.claveMovil
    val detalleClave = if (clave == null) {
        stringResource(R.string.ajustes_clave_creando)
    } else {
        (huellaCorta(clave.huellaSha256) ?: stringResource(R.string.ajustes_clave_sin_huella))
    }
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraMagi(titulo = stringResource(R.string.ajustes_titulo))
        Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            FilaAjuste(
                glifo = "⇅",
                titulo = stringResource(R.string.ajustes_sincronizacion),
                detalle = textoFaseSincronizacion(estado.sincronizacion.fase),
                alPulsar = { alAbrirSeccion(SeccionAjustes.SINCRONIZACION) },
            )
            FilaAjuste(
                glifo = "⚿",
                titulo = stringResource(R.string.ajustes_clave_movil),
                detalle = detalleClave,
                alPulsar = { alAbrirSeccion(SeccionAjustes.CLAVE_MOVIL) },
            )
            FilaAjuste(
                glifo = "A",
                titulo = stringResource(R.string.ajustes_terminal),
                detalle = stringResource(R.string.ajustes_terminal_detalle, estado.tamanoLetra),
                alPulsar = { alAbrirSeccion(SeccionAjustes.TERMINAL) },
            )
            FilaAjuste(
                glifo = "⚡",
                titulo = stringResource(R.string.ajustes_bateria),
                detalle = stringResource(R.string.ajustes_bateria_detalle),
                alPulsar = { alAbrirSeccion(SeccionAjustes.BATERIA) },
            )
            Separador()
            FilaAjuste(
                glifo = "✕",
                titulo = stringResource(R.string.ajustes_borrar_datos),
                detalle = stringResource(R.string.ajustes_borrar_datos_detalle),
                colorTitulo = Paleta.ROJO,
                alPulsar = { alAbrirSeccion(SeccionAjustes.BORRAR_DATOS) },
            )
            Separador()
            FilaAjuste(
                glifo = "M",
                titulo = stringResource(R.string.ajustes_acerca_de),
                detalle = estado.version?.let {
                    stringResource(R.string.ajustes_acerca_de_detalle, textoVersion(it.app, it.codigo))
                } ?: stringResource(R.string.ajustes_acerca_de_detalle_sin_version),
                alPulsar = { alAbrirSeccion(SeccionAjustes.ACERCA_DE) },
            )
        }
    }
}

/** Fila de ajuste con glifo, título, detalle y flecha. */
@Composable
private fun FilaAjuste(
    glifo: String,
    titulo: String,
    detalle: String,
    alPulsar: () -> Unit,
    colorTitulo: Color = Paleta.TEXTO,
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
            Text(text = titulo, style = MaterialTheme.typography.titleSmall, color = colorTitulo)
            Text(text = detalle, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
        }
        Text(text = "›", style = MaterialTheme.typography.titleMedium, color = Paleta.GRIS)
    }
}
