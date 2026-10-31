package org.cuatrodetres.magi.ui.ajustes

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.plataforma.RecintoSeguro
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.TextoFecha
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Ajustes > Clave del móvil: línea OpenSSH, huella SHA256, dónde vive
 * (StrongBox o TEE) y fecha de creación; copiar, compartir y mostrar el QR.
 */
@Composable
fun PantallaClaveMovil(
    estado: EstadoMagi,
    alVolver: () -> Unit,
    alCrearClaveNueva: () -> Unit,
    alAbrirSincronizacion: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val contexto = LocalContext.current
    val clave = estado.claveMovil
    val textoCompartir = stringResource(R.string.clave_compartir)
    var verQr by remember { mutableStateOf(false) }
    // La clave pública y su QR no son secretos, pero la pantalla enseña datos
    // de la identidad del móvil: sin capturas mientras se muestran (§8).
    RecintoSeguro {
        Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.clave_titulo),
            alVolver = alVolver,
        )
        if (clave == null) {
            IndicadorCarga(texto = stringResource(R.string.clave_creando))
            return@Column
        }
        if (clave.invalidada) {
            BarraAviso(
                texto = stringResource(R.string.clave_invalidada),
                tipo = TipoBarra.ERROR,
            )
            if (estado.creandoClave) {
                IndicadorCarga(texto = stringResource(R.string.clave_creando_nueva))
            }
            Column(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BotonCabina(
                    texto = stringResource(R.string.clave_crear_nueva),
                    alPulsar = alCrearClaveNueva,
                    primario = true,
                    habilitado = !estado.creandoClave,
                    modifier = Modifier.fillMaxWidth(),
                )
                BotonCabina(
                    texto = stringResource(R.string.clave_escanear_sincronizacion),
                    alPulsar = alAbrirSincronizacion,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        FilaDato(
            etiqueta = stringResource(R.string.clave_ubicacion),
            valor = when (clave.ubicacion) {
                UbicacionClaveVista.STRONGBOX -> stringResource(R.string.clave_strongbox)
                UbicacionClaveVista.TEE -> stringResource(R.string.clave_tee)
                UbicacionClaveVista.DESCONOCIDA -> stringResource(R.string.clave_ubicacion_desconocida)
            },
        )
        FilaDato(
            etiqueta = stringResource(R.string.clave_creada),
            valor = "",
        )
        TextoFecha(
            instanteSegundos = clave.creadaEn,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
        )
        FilaDato(etiqueta = stringResource(R.string.clave_huella), valor = clave.huellaSha256)
        Separador()
        Text(
            text = stringResource(R.string.clave_linea_openssh),
            style = MaterialTheme.typography.labelLarge,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
        SelectionContainer {
            Text(
                text = clave.lineaOpenssh,
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.TEXTO,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Paleta.SUPERFICIE)
                    .padding(12.dp),
            )
        }
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BotonCabina(
                texto = stringResource(R.string.clave_copiar),
                alPulsar = {
                    val gestor = contexto.getSystemService(android.content.ClipboardManager::class.java)
                    gestor?.setPrimaryClip(
                        android.content.ClipData.newPlainText("MAGI", clave.lineaOpenssh),
                    )
                },
                primario = true,
                modifier = Modifier.fillMaxWidth(),
            )
            BotonCabina(
                texto = stringResource(R.string.clave_compartir),
                alPulsar = {
                    val intent = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, clave.lineaOpenssh)
                    }
                    contexto.startActivity(Intent.createChooser(intent, textoCompartir))
                },
                modifier = Modifier.fillMaxWidth(),
            )
            BotonCabina(
                texto = stringResource(R.string.clave_ver_qr),
                alPulsar = { verQr = true },
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
        if (verQr && clave != null) {
            DialogoQrClave(clave = clave, alCerrar = { verQr = false })
        }
    }
}

/** Diálogo con el QR de la línea pública de la clave del móvil. */
@Composable
private fun DialogoQrClave(clave: ClaveMovilVista, alCerrar: () -> Unit) {
    Dialog(onDismissRequest = alCerrar) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(Paleta.FONDO)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.clave_qr_titulo),
                style = MaterialTheme.typography.titleMedium,
                color = Paleta.TEXTO,
            )
            LienzoQr(texto = clave.lineaOpenssh)
            Text(
                text = clave.huellaSha256,
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.GRIS,
            )
            BotonCabina(
                texto = stringResource(R.string.accion_cerrar),
                alPulsar = alCerrar,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
