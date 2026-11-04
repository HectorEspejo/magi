package org.cuatrodetres.magi.ui.dialogos

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.plataforma.RecintoSeguro
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.modelos.AsistenteVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Asistente de unión a la sincronización (informe §4.1): elegir la carpeta
 * compartida con el selector del sistema y unir con la clave (QR o pegada).
 */
@Composable
fun PantallaAsistente(
    asistente: AsistenteVista,
    alCerrar: () -> Unit,
    alElegirCarpeta: () -> Unit,
    alUnirConClave: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var escaneando by remember { mutableStateOf(false) }
    var clave by remember { mutableStateOf("") }
    // La clave de sincronización es un secreto: ni capturas ni vista previa en
    // recientes mientras se pega o escanea (informe §8).
    RecintoSeguro(activo = asistente.paso == PasoAsistente.UNIR_CLAVE) {
        Column(modifier = modifier.fillMaxSize().background(Paleta.FONDO)) {
        CabeceraVolver(
            titulo = stringResource(R.string.asistente_titulo),
            alVolver = alCerrar,
        )
        Column(
            modifier = Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (asistente.error != null) {
                BarraAviso(texto = asistente.error, tipo = TipoBarra.ERROR)
            }
            when (asistente.paso) {
                PasoAsistente.ELEGIR_CARPETA -> {
                    Text(
                        text = stringResource(R.string.asistente_carpeta_titulo),
                        style = MaterialTheme.typography.titleMedium,
                        color = Paleta.TEXTO,
                    )
                    Text(
                        text = stringResource(R.string.asistente_carpeta_texto),
                        style = MaterialTheme.typography.bodyMedium,
                        color = Paleta.GRIS,
                    )
                    if (asistente.ocupado) {
                        IndicadorCarga(texto = stringResource(R.string.asistente_comprobando))
                    }
                    BotonCabina(
                        texto = stringResource(R.string.asistente_elegir_carpeta),
                        alPulsar = alElegirCarpeta,
                        primario = true,
                        habilitado = !asistente.ocupado,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                PasoAsistente.UNIR_CLAVE -> {
                    Text(
                        text = stringResource(R.string.asistente_clave_titulo),
                        style = MaterialTheme.typography.titleMedium,
                        color = Paleta.TEXTO,
                    )
                    Text(
                        text = stringResource(R.string.asistente_clave_texto),
                        style = MaterialTheme.typography.bodyMedium,
                        color = Paleta.GRIS,
                    )
                    if (asistente.carpeta != null) {
                        Text(
                            text = stringResource(R.string.asistente_carpeta_elegida, asistente.carpeta),
                            style = MaterialTheme.typography.labelMedium,
                            color = Paleta.AMBAR,
                        )
                    }
                    if (asistente.ocupado) {
                        IndicadorCarga(texto = stringResource(R.string.asistente_unindo))
                    }
                    BotonCabina(
                        texto = stringResource(R.string.asistente_escanear),
                        alPulsar = { escaneando = true },
                        modifier = Modifier.fillMaxWidth(),
                        habilitado = !asistente.ocupado,
                    )
                    CampoTextoMagi(
                        valor = clave,
                        alCambiar = { clave = it },
                        etiqueta = stringResource(R.string.asistente_pegar_etiqueta),
                        habilitado = !asistente.ocupado,
                        unaLinea = false,
                    )
                    BotonCabina(
                        texto = stringResource(R.string.asistente_unir),
                        alPulsar = {
                            if (clave.isNotBlank()) alUnirConClave(clave.trim())
                        },
                        primario = true,
                        habilitado = clave.isNotBlank() && !asistente.ocupado,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        text = stringResource(R.string.asistente_pegado_aviso),
                        style = MaterialTheme.typography.labelSmall,
                        color = Paleta.GRIS,
                    )
                }
            }
        }
        }
    }
    if (escaneando) {
        Box(modifier = Modifier.fillMaxSize()) {
            EscanerQr(
                alLeido = { texto ->
                    escaneando = false
                    clave = texto.trim()
                },
                alCerrar = { escaneando = false },
            )
        }
    }
}
