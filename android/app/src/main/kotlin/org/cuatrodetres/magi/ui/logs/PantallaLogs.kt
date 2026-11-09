package org.cuatrodetres.magi.ui.logs

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.flow.distinctUntilChanged
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.Chiptexto
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.EstadoSuscripcionLogVista
import org.cuatrodetres.magi.ui.modelos.LineaLogVista
import org.cuatrodetres.magi.ui.modelos.MotorNombreVista
import org.cuatrodetres.magi.ui.modelos.NivelLineaLogVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.OrdenPestanaVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.tema.Paleta
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val HORA_LOG: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm:ss")

/**
 * Panel de logs en vivo (informe §6.5 y §7.4): sigue una suscripción del
 * núcleo, pinta sus ventanas filtradas y permite pausar, volver al final,
 * filtrar por texto o nivel y abrir una sesión con el diario o una shell.
 * El anillo y los límites (10 000 líneas y 8 MiB) viven en magi-movil.
 */
@Composable
fun PantallaLogs(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    hostUuid: String,
    hostNombre: String,
    objetivo: ObjetivoVista,
    motor: MotorNombreVista? = null,
    alAbrirSesionConOrden: (String, OrdenPestanaVista) -> Unit = { _, _ -> },
    alAbrirTerminal: (Long) -> Unit = {},
    alPedirSudo: () -> Unit = {},
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
    vistaModelo: VistaModeloLogs = viewModel(
        key = "magi-logs-" + hostUuid + "-" + objetivo.tipo.name + "-" + objetivo.nombre,
        factory = viewModelFactory {
            initializer { VistaModeloLogs(fuente) }
        },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    val lista = rememberLazyListState()

    LaunchedEffect(Unit) { vistaModelo.iniciar(hostUuid, hostNombre, objetivo) }
    DisposableEffect(Unit) {
        onDispose { vistaModelo.cerrar() }
    }

    // Al llegar líneas nuevas con el panel pegado al final, se baja solo.
    LaunchedEffect(estado.lineas.size, estado.pausada) {
        if (!estado.pausada && estado.lineas.isNotEmpty()) {
            lista.animateScrollToItem(estado.lineas.lastIndex)
        }
    }

    // Pausa al desplazarse hacia arriba; «AL FINAL» o el botón reanudan.
    LaunchedEffect(lista, estado.pausada, estado.lineas.size) {
        if (estado.pausada) return@LaunchedEffect
        snapshotFlow { lista.firstVisibleItemIndex }
            .distinctUntilChanged()
            .collect { indice ->
                if (estado.lineas.size > 20 && indice < estado.lineas.lastIndex - 2) {
                    vistaModelo.alternarPausa()
                }
            }
    }

    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = hostNombre + " · " + objetivo.nombre,
            alVolver = alVolver,
            glifoEstado = estado.estado.glifo,
            textoEstado = estado.cabecera,
            colorEstado = colorEstadoLog(estado.estado),
        )
        if (estado.estado == EstadoSuscripcionLogVista.SIN_PERMISO) {
            Text(
                stringResource(R.string.logs_sin_permiso),
                color = Paleta.AMBAR,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
            BotonCabina(
                texto = stringResource(R.string.logs_dar_sudo),
                alPulsar = alPedirSudo,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
            )
        }
        if (estado.motivo != null) {
            Text(
                stringResource(R.string.logs_motivo, estado.motivo!!.texto),
                color = Paleta.ROJO,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
        }
        CampoTextoMagi(
            valor = estado.filtroTexto,
            alCambiar = vistaModelo::cambiarFiltro,
            etiqueta = stringResource(R.string.logs_filtro),
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Chiptexto(
                texto = stringResource(R.string.logs_nivel, estado.nivel.texto),
                modifier = Modifier.heightIn(min = 48.dp).clickable(
                    onClickLabel = stringResource(R.string.logs_accion_nivel),
                    role = androidx.compose.ui.semantics.Role.Button,
                ) { vistaModelo.cambiarNivel() },
            )
            Chiptexto(
                texto = stringResource(R.string.logs_stderr),
                modifier = Modifier
                    .heightIn(min = 48.dp)
                    .clickable(
                        onClickLabel = stringResource(R.string.logs_accion_stderr),
                        role = androidx.compose.ui.semantics.Role.Button,
                    ) { vistaModelo.alternarSoloStderr() }
                    .background(
                        if (estado.soloStderr) Paleta.AMBAR.copy(alpha = 0.25f) else Paleta.SUPERFICIE,
                    ),
            )
            Chiptexto(
                texto = if (estado.pausada) stringResource(R.string.logs_seguir) else stringResource(R.string.logs_pausa),
                modifier = Modifier.heightIn(min = 48.dp).clickable(
                    onClickLabel = stringResource(R.string.logs_accion_pausa),
                    role = androidx.compose.ui.semantics.Role.Button,
                ) { vistaModelo.alternarPausa() },
            )
            Chiptexto(
                texto = if (estado.nuevas > 0) {
                    stringResource(R.string.logs_al_final_nuevas, estado.nuevas)
                } else {
                    stringResource(R.string.logs_al_final)
                },
                modifier = Modifier.heightIn(min = 48.dp).clickable(
                    onClickLabel = stringResource(R.string.logs_accion_al_final),
                    role = androidx.compose.ui.semantics.Role.Button,
                ) { vistaModelo.alFinal() },
            )
        }
        if (estado.cargando) {
            MensajeVacio(
                titulo = estado.objetivo.nombre,
                detalle = stringResource(R.string.logs_cargando),
                modifier = Modifier.weight(1f),
            )
        } else if (estado.lineas.isEmpty()) {
            MensajeVacio(
                titulo = estado.objetivo.nombre,
                detalle = if (estado.filtroTexto.isEmpty() && estado.nivel == org.cuatrodetres.magi.ui.modelos.NivelLogVista.TODO) {
                    stringResource(R.string.logs_sin_lineas)
                } else {
                    stringResource(R.string.logs_sin_coincidencias)
                },
                modifier = Modifier.weight(1f),
            )
        } else {
            LazyColumn(state = lista, modifier = Modifier.weight(1f)) {
                if (estado.omitidas > 0) {
                    item(key = "omitidas") {
                        Text(
                            stringResource(R.string.logs_omitidas, estado.omitidas),
                            color = Paleta.AMBAR,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                        )
                    }
                }
                items(estado.lineas, key = { it.numero }) { linea ->
                    FilaLineaLog(linea)
                }
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                stringResource(R.string.logs_recuento, estado.total, estado.omitidas),
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                modifier = Modifier.weight(1f),
            )
            if (estado.sePuedeParar) {
                BotonCabina(
                    texto = stringResource(R.string.logs_parar),
                    alPulsar = vistaModelo::parar,
                    destructivo = true,
                )
            }
            when (objetivo.tipo) {
                TipoObjetivoVista.CONTENEDOR -> {
                    val motorActual = motor
                    if (motorActual != null) {
                        BotonCabina(
                            texto = stringResource(R.string.servicio_sesion),
                            alPulsar = {
                                alAbrirSesionConOrden(
                                    hostUuid,
                                    OrdenPestanaVista.SeguirContenedor(motorActual, objetivo.nombre),
                                )
                            },
                        )
                        BotonCabina(
                            texto = stringResource(R.string.servicio_shell),
                            alPulsar = {
                                alAbrirSesionConOrden(
                                    hostUuid,
                                    OrdenPestanaVista.ShellContenedor(motorActual, objetivo.nombre),
                                )
                            },
                        )
                    }
                }
                else -> BotonCabina(
                    texto = stringResource(R.string.servicio_sesion),
                    alPulsar = {
                        alAbrirSesionConOrden(
                            hostUuid,
                            OrdenPestanaVista.SeguirDiario(objetivo.nombre),
                        )
                    },
                )
            }
        }
    }

    estado.aviso?.let { aviso ->
        AlertDialog(
            onDismissRequest = vistaModelo::descartarAviso,
            title = { Text(estado.objetivo.nombre) },
            text = { Text(aviso) },
            confirmButton = {
                TextButton(onClick = vistaModelo::descartarAviso) {
                    Text(stringResource(R.string.accion_cerrar))
                }
            },
        )
    }
}

@Composable
private fun FilaLineaLog(linea: LineaLogVista) {
    val color = when (linea.nivel) {
        NivelLineaLogVista.ERROR -> Paleta.ROJO
        NivelLineaLogVista.AVISO -> Paleta.AMBAR
        NivelLineaLogVista.INFO -> Paleta.TEXTO
    }
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 1.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Text(linea.glifo, color = color, style = MaterialTheme.typography.bodySmall)
        if (linea.hora != null) {
            Text(
                HORA_LOG.format(
                    Instant.ofEpochSecond(linea.hora).atZone(ZoneId.systemDefault()),
                ),
                color = Paleta.GRIS,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Text(
            linea.texto,
            color = color,
            style = MaterialTheme.typography.bodySmall,
            maxLines = 4,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

private fun colorEstadoLog(estado: EstadoSuscripcionLogVista) = when {
    estado.esFinal -> Paleta.ROJO
    estado == EstadoSuscripcionLogVista.EN_PAUSA -> Paleta.AMBAR
    else -> Paleta.VERDE
}
