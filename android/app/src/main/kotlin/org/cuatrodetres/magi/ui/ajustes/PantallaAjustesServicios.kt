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
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_ESPERA_SEG_MAX
import org.cuatrodetres.magi.ui.modelos.SERVICIOS_ESPERA_SEG_MIN
import org.cuatrodetres.magi.ui.modelos.SUDO_MEMORIA_MIN_MAX

/**
 * Ajustes > Servicios (informe §5.4): refresco del listado, espera al estado
 * final de una acción, motores de contenedores y minutos de sudo en memoria.
 *
 * Son los mismos valores que el escritorio tiene en config.toml.
 */
@Composable
fun PantallaAjustesServicios(
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
            titulo = stringResource(R.string.ajustes_servicios),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            AvisosDeAjustes(avisos)
            DeslizadorEnteroAjuste(
                titulo = stringResource(R.string.ajustes_servicios_refresco),
                valores = valoresRefrescoServicios(),
                valor = borrador.serviciosRefrescoSeg,
                etiquetaValor = etiquetaRefresco(borrador.serviciosRefrescoSeg),
                ayuda = stringResource(R.string.ajustes_servicios_refresco_ayuda),
                alCambiar = { borrador = borrador.copy(serviciosRefrescoSeg = it) },
            )
            DeslizadorEnteroAjuste(
                titulo = stringResource(R.string.ajustes_servicios_espera),
                valores = valoresEsperaServicios(),
                valor = borrador.serviciosEsperaSeg.coerceIn(
                    SERVICIOS_ESPERA_SEG_MIN,
                    SERVICIOS_ESPERA_SEG_MAX,
                ),
                etiquetaValor = stringResource(
                    R.string.ajustes_segundos,
                    borrador.serviciosEsperaSeg,
                ),
                ayuda = stringResource(R.string.ajustes_servicios_espera_ayuda),
                alCambiar = { borrador = borrador.copy(serviciosEsperaSeg = it) },
            )
            EncabezadoSeccion(stringResource(R.string.ajustes_servicios_motores))
            TextoAyuda(stringResource(R.string.ajustes_servicios_motores_ayuda))
            CasillaAjuste(
                texto = stringResource(R.string.ajustes_servicios_motor_docker),
                marcada = "docker" in borrador.serviciosContenedores,
                alCambiar = { marcada ->
                    borrador = borrador.copy(
                        serviciosContenedores = alternarMotor(
                            borrador.serviciosContenedores,
                            "docker",
                            marcada,
                        ),
                    )
                },
            )
            CasillaAjuste(
                texto = stringResource(R.string.ajustes_servicios_motor_podman),
                marcada = "podman" in borrador.serviciosContenedores,
                alCambiar = { marcada ->
                    borrador = borrador.copy(
                        serviciosContenedores = alternarMotor(
                            borrador.serviciosContenedores,
                            "podman",
                            marcada,
                        ),
                    )
                },
            )
            DeslizadorEnteroAjuste(
                titulo = stringResource(R.string.ajustes_servicios_sudo),
                valores = (0..SUDO_MEMORIA_MIN_MAX).toList(),
                valor = borrador.sudoMemoriaMin.coerceIn(0, SUDO_MEMORIA_MIN_MAX),
                etiquetaValor = if (borrador.sudoMemoriaMin <= 0) {
                    stringResource(R.string.ajustes_servicios_sudo_siempre)
                } else {
                    stringResource(R.string.ajustes_minutos, borrador.sudoMemoriaMin)
                },
                ayuda = stringResource(R.string.ajustes_servicios_sudo_ayuda),
                alCambiar = { borrador = borrador.copy(sudoMemoriaMin = it) },
            )
            PieGuardarAjustes(
                ayuda = stringResource(R.string.ajustes_servicios_nota),
                hayCambios = hayCambios,
                alGuardar = { alGuardar(borrador) },
            )
        }
    }
}

/** Añade o quita un motor conservando el orden docker · podman (§5.4). */
fun alternarMotor(actuales: List<String>, motor: String, marcada: Boolean): List<String> {
    val limpios = actuales.filter { it == "docker" || it == "podman" }.distinct()
    if (marcada) {
        return (limpios + motor).distinct().sortedBy { if (it == "docker") 0 else 1 }
    }
    return limpios.filter { it != motor }
}
