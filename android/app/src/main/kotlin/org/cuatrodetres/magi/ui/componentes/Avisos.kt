package org.cuatrodetres.magi.ui.componentes

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.tema.Paleta

/** Texto de un aviso: los del núcleo llegan hechos; los de la interfaz, de strings.xml. */
@Composable
fun textoDeAviso(aviso: AvisoVista): String = when (aviso) {
    is AvisoVista.DeNucleo -> aviso.mensaje
    is AvisoVista.DeInterfaz -> textoDeTipoAviso(aviso.tipo, aviso.detalle)
}

@Composable
private fun textoDeTipoAviso(tipo: TipoAviso, detalle: String?): String = when (tipo) {
    TipoAviso.SESION_CERRADA -> stringResource(R.string.aviso_sesion_cerrada)
    TipoAviso.SINCRONIZACION_AL_DIA -> stringResource(R.string.aviso_sincronizacion_al_dia)
    TipoAviso.SINCRONIZACION_DEJADA -> stringResource(R.string.aviso_sincronizacion_dejada)
    TipoAviso.CLAVE_INSTALADA_YA_ESTABA -> stringResource(R.string.aviso_clave_ya_estaba)
    TipoAviso.CLAVE_INSTALADA_ACEPTADA -> stringResource(R.string.aviso_clave_instalada)
    TipoAviso.CLAVE_INSTALADA_NO_ACEPTADA -> stringResource(R.string.aviso_clave_no_aceptada)
    TipoAviso.CONTRASENA_OLVIDADA -> stringResource(R.string.aviso_contrasena_olvidada)
    TipoAviso.CONFLICTO_HUELLA_ACEPTADO -> stringResource(R.string.aviso_conflicto_aceptado)
    TipoAviso.CONFLICTO_HUELLA_MANTENIDO -> stringResource(R.string.aviso_conflicto_mantenido)
    TipoAviso.DATOS_BORRADOS -> stringResource(R.string.aviso_datos_borrados)
    TipoAviso.TUNEL_ACTIVADO -> stringResource(R.string.aviso_tunel_activado, detalle ?: "")
    TipoAviso.TUNEL_PARADO -> stringResource(R.string.aviso_tunel_parado, detalle ?: "")
    TipoAviso.HOST_DESAPARECIDO -> stringResource(R.string.aviso_host_desaparecido)
    TipoAviso.ERROR_INESPERADO -> stringResource(R.string.aviso_error_inesperado)
    TipoAviso.TEXTO_COPIADO -> stringResource(R.string.aviso_texto_copiado)
    TipoAviso.PORTAPAPELES_VACIO -> stringResource(R.string.aviso_portapapeles_vacio)
    TipoAviso.SELECCION_VACIA -> stringResource(R.string.aviso_seleccion_vacia)
    TipoAviso.TUNEL_DESCARTADO -> stringResource(R.string.aviso_tunel_descartado)
    TipoAviso.CLAVE_MOVIL_CREADA -> stringResource(R.string.aviso_clave_movil_creada)
    TipoAviso.CLAVE_MOVIL_NO_CREADA -> stringResource(R.string.aviso_clave_movil_no_creada)
}

/** Snackbar de la cabina: barra inferior con el aviso y una ✕ para cerrarlo. */
@Composable
fun BarraAvisoMagi(
    aviso: AvisoVista,
    alCerrar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val error = aviso is AvisoVista.DeNucleo && aviso.esError
    val color = if (error) Paleta.ROJO else Paleta.AMBAR
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(Paleta.SUPERFICIE)
            .padding(start = 10.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier
                .heightIn(min = 36.dp)
                .padding(vertical = 2.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = if (error) "✕" else "!",
                style = MaterialTheme.typography.labelLarge,
                color = color,
            )
        }
        Text(
            text = textoDeAviso(aviso),
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.TEXTO,
            modifier = Modifier.weight(1f).padding(vertical = 10.dp),
        )
        BotonGlifo(
            glifo = "✕",
            descripcion = stringResource(R.string.accion_cerrar_aviso),
            alPulsar = alCerrar,
            color = Paleta.GRIS,
        )
    }
}
