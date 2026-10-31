package org.cuatrodetres.magi.ui.sesiones

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.BotonGlifo
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.ChipEstado
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.colorEstadoSesion
import org.cuatrodetres.magi.ui.componentes.textoEstadoSesion
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.glifoSesion
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla Sesiones (informe §6 y mapa de navegación): lista con los estados de
 * la F3, «Reconectar» en las caídas, cerrar con confirmación y abrir el
 * terminal sin cerrar nada.
 */
@Composable
fun PantallaSesiones(
    sesiones: List<SesionVista>,
    sesionVisible: Long?,
    alAbrir: (Long) -> Unit,
    alReconectar: (Long) -> Unit,
    alCerrar: (Long) -> Unit,
    alIrAHosts: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraMagi(
            titulo = stringResource(R.string.sesiones_titulo, sesiones.size),
        )
        if (sesiones.isEmpty()) {
            MensajeVacio(
                titulo = stringResource(R.string.sesiones_vacio_titulo),
                detalle = stringResource(R.string.sesiones_vacio_detalle),
                textoBoton = stringResource(R.string.accion_ir_a_hosts),
                alPulsar = alIrAHosts,
                modifier = Modifier.weight(1f),
            )
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                items(items = sesiones, key = { it.id }) { sesion ->
                    FilaSesion(
                        sesion = sesion,
                        visible = sesion.id == sesionVisible,
                        alAbrir = alAbrir,
                        alReconectar = alReconectar,
                        alCerrar = alCerrar,
                    )
                }
            }
        }
    }
}

/** Fila de sesión con su estado, detalle y acciones. */
@Composable
private fun FilaSesion(
    sesion: SesionVista,
    visible: Boolean,
    alAbrir: (Long) -> Unit,
    alReconectar: (Long) -> Unit,
    alCerrar: (Long) -> Unit,
) {
    val estadoTexto = textoEstadoSesion(sesion.estado)
    val descripcion = stringResource(R.string.sesiones_descripcion_fila, sesion.titulo, estadoTexto)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(if (visible) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = stringResource(R.string.accion_abrir_terminal)) { alAbrir(sesion.id) }
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(start = 12.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = glifoSesion(sesion.estado),
            style = MaterialTheme.typography.titleMedium,
            color = colorEstadoSesion(sesion.estado),
            modifier = Modifier.widthIn(min = 20.dp),
        )
        Column(modifier = Modifier.weight(1f).padding(vertical = 8.dp)) {
            Text(
                text = sesion.titulo,
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.TEXTO,
            )
            if (!sesion.detalle.isNullOrBlank()) {
                Text(
                    text = sesion.detalle,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (sesion.estado == EstadoSesionVista.CAIDA) Paleta.ROJO else Paleta.GRIS,
                )
            }
            ChipEstado(
                glifo = glifoSesion(sesion.estado),
                texto = estadoTexto,
                color = colorEstadoSesion(sesion.estado),
            )
        }
        if (sesion.estado == EstadoSesionVista.CAIDA) {
            BotonCabina(
                texto = stringResource(R.string.accion_reconectar),
                alPulsar = { alReconectar(sesion.id) },
            )
        }
        BotonGlifo(
            glifo = "✕",
            descripcion = stringResource(R.string.sesiones_cerrar_a, sesion.titulo),
            alPulsar = { alCerrar(sesion.id) },
            color = Paleta.ROJO,
        )
    }
}
