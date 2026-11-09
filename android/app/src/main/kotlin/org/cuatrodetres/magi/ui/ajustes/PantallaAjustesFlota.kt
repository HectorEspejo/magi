package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.CARGA_POR_NUCLEO_MAX
import org.cuatrodetres.magi.ui.modelos.CARGA_POR_NUCLEO_MIN
import org.cuatrodetres.magi.ui.modelos.UMBRAL_PCT_MAX
import org.cuatrodetres.magi.ui.modelos.UMBRAL_PCT_MIN

/**
 * Ajustes > Flota (informe §5.4): auto-refresco y umbrales de CARGA.
 *
 * Son los mismos valores que el escritorio tiene en config.toml. Un valor que
 * llegue de DataStore fuera de rango se acota al guardar y se dice por qué.
 */
@Composable
fun PantallaAjustesFlota(
    ajustes: AjustesMovilVista,
    avisos: List<String>,
    alGuardar: (AjustesMovilVista) -> Unit,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var borrador by remember(ajustes) { mutableStateOf(ajustes) }
    val hayCambios = borrador != ajustes
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = stringResource(R.string.ajustes_flota),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            AvisosDeAjustes(avisos)
            DeslizadorEnteroAjuste(
                titulo = stringResource(R.string.ajustes_flota_refresco),
                valores = valoresRefrescoFlota(),
                valor = borrador.flotaRefrescoSeg,
                etiquetaValor = etiquetaRefresco(borrador.flotaRefrescoSeg),
                ayuda = stringResource(R.string.ajustes_flota_refresco_ayuda),
                alCambiar = { borrador = borrador.copy(flotaRefrescoSeg = it) },
            )
            DeslizadorDecimalAjuste(
                titulo = stringResource(R.string.ajustes_flota_carga),
                minimo = CARGA_POR_NUCLEO_MIN,
                maximo = CARGA_POR_NUCLEO_MAX,
                paso = 0.1,
                valor = borrador.umbralCargaPorNucleo,
                etiquetaValor = stringResource(
                    R.string.ajustes_flota_carga_valor,
                    unDecimal(borrador.umbralCargaPorNucleo),
                ),
                ayuda = stringResource(R.string.ajustes_flota_carga_ayuda),
                alCambiar = { borrador = borrador.copy(umbralCargaPorNucleo = it) },
            )
            DeslizadorDecimalAjuste(
                titulo = stringResource(R.string.ajustes_flota_memoria),
                minimo = UMBRAL_PCT_MIN,
                maximo = UMBRAL_PCT_MAX,
                paso = 1.0,
                valor = borrador.umbralMemoriaPct,
                etiquetaValor = stringResource(
                    R.string.ajustes_porcentaje_valor,
                    sinDecimales(borrador.umbralMemoriaPct),
                ),
                ayuda = stringResource(R.string.ajustes_flota_memoria_ayuda),
                alCambiar = { borrador = borrador.copy(umbralMemoriaPct = it) },
            )
            DeslizadorDecimalAjuste(
                titulo = stringResource(R.string.ajustes_flota_disco),
                minimo = UMBRAL_PCT_MIN,
                maximo = UMBRAL_PCT_MAX,
                paso = 1.0,
                valor = borrador.umbralDiscoPct,
                etiquetaValor = stringResource(
                    R.string.ajustes_porcentaje_valor,
                    sinDecimales(borrador.umbralDiscoPct),
                ),
                ayuda = stringResource(R.string.ajustes_flota_disco_ayuda),
                alCambiar = { borrador = borrador.copy(umbralDiscoPct = it) },
            )
            PieGuardarAjustes(
                ayuda = stringResource(R.string.ajustes_flota_nota),
                hayCambios = hayCambios,
                alGuardar = { alGuardar(borrador) },
            )
        }
    }
}

/** «30 s» o «apagado» según el valor del auto-refresco. */
@Composable
fun etiquetaRefresco(segundos: Int): String =
    if (segundos <= 0) {
        stringResource(R.string.ajustes_refresco_apagado)
    } else {
        stringResource(R.string.ajustes_segundos, segundos)
    }
