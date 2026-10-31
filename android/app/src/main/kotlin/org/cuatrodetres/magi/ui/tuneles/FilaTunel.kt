package org.cuatrodetres.magi.ui.tuneles

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import org.cuatrodetres.magi.ui.componentes.ChipEstado
import org.cuatrodetres.magi.ui.componentes.colorEstadoTunel
import org.cuatrodetres.magi.ui.componentes.textoEstadoTunel
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.TipoTunelVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.contadoresTunel
import org.cuatrodetres.magi.ui.modelos.descripcionTunel
import org.cuatrodetres.magi.ui.modelos.glifoTunel
import org.cuatrodetres.magi.ui.modelos.urlNavegador
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Fila de un túnel (informe §6.2 y checklist de túneles): interruptor, tipo,
 * escucha → destino, automático, estado con glifo y texto, conexiones y bytes,
 * relanzar, puerto libre y abrir en el navegador.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FilaTunel(
    tunel: TunelVista,
    alAlternar: (TunelVista) -> Unit,
    alRelanzar: (TunelVista) -> Unit,
    alPuertoLibre: (TunelVista) -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alDescartar: (TunelVista) -> Unit,
    modifier: Modifier = Modifier,
    mostrarHost: Boolean = false,
) {
    val descripcion = stringResource(
        R.string.tuneles_descripcion_fila,
        tunel.nombre,
        tunel.hostNombre,
        descripcionTunel(tunel),
        stringResource(
            when (tunel.estado) {
                EstadoTunelVista.INACTIVO -> R.string.estado_tunel_inactivo
                EstadoTunelVista.ACTIVANDO -> R.string.estado_tunel_activando
                EstadoTunelVista.ACTIVO -> R.string.estado_tunel_activo
                EstadoTunelVista.CAIDO -> R.string.estado_tunel_caido
            },
        ),
    )
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .semantics(mergeDescendants = true) { contentDescription = descripcion },
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Switch(
                checked = tunel.estado == EstadoTunelVista.ACTIVO || tunel.estado == EstadoTunelVista.ACTIVANDO,
                onCheckedChange = { alAlternar(tunel) },
                enabled = tunel.estado != EstadoTunelVista.ACTIVANDO,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = Paleta.FONDO,
                    checkedTrackColor = Paleta.VERDE,
                    uncheckedThumbColor = Paleta.GRIS,
                    uncheckedTrackColor = Paleta.SUPERFICIE,
                    uncheckedBorderColor = Paleta.GRIS,
                ),
            )
            Column(modifier = Modifier.weight(1f)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        text = tunel.nombre,
                        style = MaterialTheme.typography.titleSmall,
                        color = Paleta.TEXTO,
                    )
                    if (tunel.automatico) {
                        Text(
                            text = stringResource(R.string.tunel_automatico),
                            style = MaterialTheme.typography.labelSmall,
                            color = Paleta.AMBAR,
                        )
                    }
                }
                Text(
                    text = if (mostrarHost) tunel.hostNombre + " · " + descripcionTunel(tunel) else descripcionTunel(tunel),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.GRIS,
                )
            }
            ChipEstado(
                glifo = glifoTunel(tunel.estado),
                texto = textoEstadoTunel(tunel.estado),
                color = colorEstadoTunel(tunel.estado),
            )
        }
        if (tunel.estado == EstadoTunelVista.ACTIVO) {
            Text(
                text = contadoresTunel(tunel),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
                modifier = Modifier.padding(start = 52.dp),
            )
        }
        if (tunel.escuchaAviso) {
            Text(
                text = stringResource(R.string.tunel_aviso_loopback),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.AMBAR,
                modifier = Modifier.padding(start = 52.dp),
            )
        }
        if (tunel.puertoEfectivo != null) {
            Text(
                text = stringResource(R.string.tunel_puerto_efectivo, tunel.puertoEfectivo),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.VERDE,
                modifier = Modifier.padding(start = 52.dp),
            )
        }
        if (tunel.error != null && tunel.estado == EstadoTunelVista.CAIDO) {
            Text(
                text = tunel.error,
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.ROJO,
                modifier = Modifier.padding(start = 52.dp),
            )
        }
        FlowRow(
            modifier = Modifier.fillMaxWidth().padding(start = 44.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            if (tunel.estado == EstadoTunelVista.CAIDO) {
                BotonCabina(
                    texto = stringResource(R.string.accion_relanzar),
                    alPulsar = { alRelanzar(tunel) },
                )
                BotonCabina(
                    texto = stringResource(R.string.accion_descartar),
                    alPulsar = { alDescartar(tunel) },
                )
                BotonCabina(
                    texto = stringResource(R.string.accion_puerto_libre),
                    alPulsar = { alPuertoLibre(tunel) },
                )
            }
            if (tunel.estado == EstadoTunelVista.ACTIVO && tunel.tipo == TipoTunelVista.LOCAL) {
                BotonCabina(
                    texto = stringResource(R.string.accion_abrir_navegador),
                    alPulsar = { alAbrirNavegador(tunel) },
                    habilitado = urlNavegador(tunel) != null,
                )
            }
        }
    }
}
