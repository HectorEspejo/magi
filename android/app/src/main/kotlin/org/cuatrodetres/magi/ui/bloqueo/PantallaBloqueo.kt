package org.cuatrodetres.magi.ui.bloqueo

import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.modelos.EstadoBloqueoVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla de bloqueo de MAGI (informe §3 y §7.8).
 *
 * Arranque en frío bloqueado y bloqueo al volver tras 5 minutos fuera. El
 * desbloqueo real (BiometricPrompt con huella o credencial del teléfono) lo
 * hace la plataforma a través de ControlBloqueo; aquí solo se pide y se pinta.
 * Sin PIN, patrón ni contraseña en el teléfono, MAGI no arranca.
 */
@Composable
fun PantallaBloqueo(
    estadoBloqueo: EstadoBloqueoVista,
    cargando: Boolean,
    alDesbloquear: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val contexto = LocalContext.current
    Column(
        modifier = modifier
            .fillMaxSize()
            .background(Paleta.FONDO)
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = stringResource(R.string.nombre_app),
            style = MaterialTheme.typography.displayMedium,
            color = Paleta.AMBAR,
        )
        Text(
            text = stringResource(R.string.bloqueo_cabina),
            style = MaterialTheme.typography.labelLarge,
            color = Paleta.GRIS,
        )
        if (estadoBloqueo == EstadoBloqueoVista.SIN_BLOQUEO_DE_PANTALLA) {
            Text(
                text = stringResource(R.string.bloqueo_sin_bloqueo),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.ROJO,
            )
            BotonCabina(
                texto = stringResource(R.string.bloqueo_abrir_seguridad),
                alPulsar = {
                    contexto.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS))
                },
                primario = true,
            )
        } else {
            Text(
                text = stringResource(R.string.bloqueo_texto),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.TEXTO,
            )
            if (cargando) {
                IndicadorCarga(texto = stringResource(R.string.bloqueo_pidiendo))
            }
            BotonCabina(
                texto = stringResource(R.string.bloqueo_desbloquear),
                alPulsar = alDesbloquear,
                primario = true,
                habilitado = !cargando,
            )
            Text(
                text = stringResource(R.string.bloqueo_nota),
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.GRIS,
            )
        }
    }
}
