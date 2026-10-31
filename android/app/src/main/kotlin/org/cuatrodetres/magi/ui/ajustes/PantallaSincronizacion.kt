package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.ChipEstado
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.TextoHace
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.colorEstadoEquipo
import org.cuatrodetres.magi.ui.componentes.textoEstadoEquipo
import org.cuatrodetres.magi.ui.componentes.textoFaseSincronizacion
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EquipoVista
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.glifoSincronizacion
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Sección Sincronización de Ajustes (informe §6.5): estado ⇅, carpeta, clave,
 * equipos de la carpeta compartida, conflictos de huella y las tres acciones
 * (sincronizar ahora, volver a escanear la clave y dejar de sincronizar).
 */
@Composable
fun PantallaSincronizacion(
    estado: EstadoMagi,
    alVolver: () -> Unit,
    alSincronizar: () -> Unit,
    alEscanearClave: () -> Unit,
    alDejarDeSincronizar: () -> Unit,
    alResolverConflicto: (ConflictoHuellaVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    val sincronizacion = estado.sincronizacion
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.sync_titulo),
            alVolver = alVolver,
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = glifoSincronizacion(sincronizacion.fase),
                style = MaterialTheme.typography.titleMedium,
                color = colorDeFase(sincronizacion),
            )
            Text(
                text = textoFaseSincronizacion(sincronizacion.fase),
                style = MaterialTheme.typography.titleSmall,
                color = colorDeFase(sincronizacion),
            )
            TextoHace(instanteSegundos = sincronizacion.ultimaPasada)
        }
        if (sincronizacion.fase == FaseSincronizacionVista.SINCRONIZANDO) {
            IndicadorCarga(texto = stringResource(R.string.sync_fase_sincronizando))
        }
        if (sincronizacion.error != null) {
            BarraAviso(
                texto = sincronizacion.error,
                tipo = TipoBarra.ERROR,
            )
        }
        FilaDato(
            etiqueta = stringResource(R.string.sync_carpeta),
            valor = sincronizacion.carpetaNombre ?: sincronizacion.carpetaUri
                ?: stringResource(R.string.sync_sin_carpeta),
        )
        FilaDato(
            etiqueta = stringResource(R.string.sync_clave),
            valor = sincronizacion.huellaCortaClave ?: stringResource(R.string.sync_sin_clave),
        )
        FilaDato(
            etiqueta = stringResource(R.string.sync_este_movil),
            valor = stringResource(R.string.sync_este_movil_lector),
        )
        Separador()
        EncabezadoSeccion(stringResource(R.string.sync_equipos))
        if (sincronizacion.equipos.isEmpty()) {
            Text(
                text = stringResource(R.string.sync_sin_equipos),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
        } else {
            sincronizacion.equipos.forEach { equipo ->
                FilaEquipo(equipo)
            }
        }
        if (sincronizacion.conflictosHuella > 0) {
            Separador()
            val conflicto = estado.conflictos.firstOrNull()
            BarraAviso(
                texto = stringResource(
                    R.string.sync_conflictos,
                    sincronizacion.conflictosHuella,
                    conflicto?.hostNombre ?: "",
                ),
                tipo = TipoBarra.AVISO,
                textoAccion = stringResource(R.string.accion_resolver),
                alAccion = { conflicto?.let { alResolverConflicto(it) } },
            )
        }
        Separador()
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BotonCabina(
                texto = stringResource(R.string.sync_accion_sincronizar_ahora),
                alPulsar = alSincronizar,
                primario = true,
                modifier = Modifier.fillMaxWidth(),
                habilitado = sincronizacion.fase != FaseSincronizacionVista.SINCRONIZANDO,
            )
            BotonCabina(
                texto = stringResource(R.string.sync_accion_escanear),
                alPulsar = alEscanearClave,
                modifier = Modifier.fillMaxWidth(),
            )
            BotonCabina(
                texto = stringResource(R.string.sync_accion_dejar),
                alPulsar = alDejarDeSincronizar,
                destructivo = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun colorDeFase(estado: EstadoSincronizacionVista): androidx.compose.ui.graphics.Color = when (estado.fase) {
    FaseSincronizacionVista.ACTIVA -> Paleta.VERDE
    FaseSincronizacionVista.SINCRONIZANDO -> Paleta.AMBAR
    FaseSincronizacionVista.ESPERANDO_CLAVE -> Paleta.AMBAR
    FaseSincronizacionVista.ERROR_CARPETA -> Paleta.ROJO
    else -> Paleta.GRIS
}

/** Un equipo de la carpeta compartida con su estado y su última novedad. */
@Composable
private fun FilaEquipo(equipo: EquipoVista) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = equipo.nombre,
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.TEXTO,
            )
            Text(
                text = equipo.plataforma + " · " + equipo.version,
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
        }
        TextoHace(instanteSegundos = equipo.ultimaVez)
        ChipEstado(
            glifo = if (equipo.estado == org.cuatrodetres.magi.ui.modelos.EstadoEquipoVista.AL_DIA) "●" else "✕",
            texto = textoEstadoEquipo(equipo.estado),
            color = colorEstadoEquipo(equipo.estado),
        )
    }
}
