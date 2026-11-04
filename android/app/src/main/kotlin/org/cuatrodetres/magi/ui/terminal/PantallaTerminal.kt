package org.cuatrodetres.magi.ui.terminal

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.BotonGlifo
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.colorEstadoSesion
import org.cuatrodetres.magi.ui.componentes.textoEstadoSesion
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.modelos.glifoSesion
import org.cuatrodetres.magi.ui.tema.Paleta
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloTerminal

/**
 * Pantalla del terminal (informe §6.3 y checklist de sesiones y terminal):
 * conmutador de sesiones, lienzo por celdas, acciones de portapapeles, entrada
 * en bruto, fila de teclas extra y menú con reconectar, pegar, zoom y cerrar.
 *
 * Atrás vuelve a la lista sin cerrar la sesión; cerrar pide confirmación.
 */
@Composable
fun PantallaTerminal(
    vistaModelo: VistaModeloTerminal,
    sesiones: List<SesionVista>,
    alVolver: () -> Unit,
    alSeleccionarSesion: (Long) -> Unit,
    alCerrarSesion: (Long) -> Unit,
    alReconectar: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    val contexto = LocalContext.current
    val foco = remember { FocusRequester() }
    val alcance = rememberCoroutineScope()
    var menuAbierto by remember { mutableStateOf(false) }
    val sesion = sesiones.firstOrNull { it.id == estado.sesionVisible }

    // Si la sesión visible desaparece, se suelta su pantalla; si no hay ninguna
    // elegida, se muestra la primera (la que se acaba de abrir).
    LaunchedEffect(sesiones, estado.sesionVisible) {
        val visible = estado.sesionVisible
        if (visible != null && sesiones.none { it.id == visible }) {
            vistaModelo.limpiarSesion(visible)
        }
        if (estado.sesionVisible == null) {
            sesiones.firstOrNull()?.let { vistaModelo.mostrarSesion(it.id) }
        }
    }
    LaunchedEffect(estado.sesionVisible) {
        runCatching { foco.requestFocus() }
    }

    fun copiar() {
        alcance.launch {
            val texto = vistaModelo.textoSeleccionado()
            if (texto.isNullOrEmpty()) {
                vistaModelo.avisar(TipoAviso.SELECCION_VACIA)
            } else {
                escribirPortapapeles(contexto, texto)
                vistaModelo.avisar(TipoAviso.TEXTO_COPIADO)
            }
        }
    }

    fun pegar() {
        val texto = leerPortapapeles(contexto)
        if (texto == null) {
            vistaModelo.avisar(TipoAviso.PORTAPAPELES_VACIO)
        } else {
            vistaModelo.solicitarPegado(texto)
        }
    }

    Column(modifier = modifier.fillMaxSize().background(Paleta.FONDO)) {
        CabeceraVolver(
            titulo = sesion?.titulo ?: stringResource(R.string.terminal_titulo),
            alVolver = alVolver,
            glifoEstado = sesion?.let { glifoSesion(it.estado) },
            textoEstado = sesion?.let { textoEstadoSesion(it.estado) },
            colorEstado = sesion?.let { colorEstadoSesion(it.estado) } ?: Paleta.GRIS,
            acciones = {
                Box {
                    BotonGlifo(
                        glifo = "⋮",
                        descripcion = stringResource(R.string.accion_abrir_menu),
                        alPulsar = { menuAbierto = true },
                    )
                    DropdownMenu(expanded = menuAbierto, onDismissRequest = { menuAbierto = false }) {
                        if (sesion?.estado == EstadoSesionVista.CAIDA) {
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.accion_reconectar)) },
                                onClick = {
                                    menuAbierto = false
                                    sesion?.let { alReconectar(it.id) }
                                },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.accion_pegar)) },
                            onClick = {
                                menuAbierto = false
                                pegar()
                            },
                        )
                        if (estado.seleccion != null) {
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.accion_copiar)) },
                                onClick = {
                                    menuAbierto = false
                                    copiar()
                                },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.terminal_menu_zoom_menos)) },
                            onClick = {
                                menuAbierto = false
                                vistaModelo.fijarZoom(estado.zoom - 1)
                            },
                        )
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.terminal_menu_zoom_mas)) },
                            onClick = {
                                menuAbierto = false
                                vistaModelo.fijarZoom(estado.zoom + 1)
                            },
                        )
                        if (sesion != null) {
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.accion_cerrar_sesion)) },
                                onClick = {
                                    menuAbierto = false
                                    sesion?.let { alCerrarSesion(it.id) }
                                },
                            )
                        }
                    }
                }
            },
        )

        FilaSesionesTerminal(
            sesiones = sesiones,
            sesionVisible = estado.sesionVisible,
            alElegir = alSeleccionarSesion,
        )

        if (sesion?.estado == EstadoSesionVista.ABRIENDO) {
            IndicadorCarga(texto = sesion.detalle ?: stringResource(R.string.terminal_abriendo))
        }
        if (sesion?.estado == EstadoSesionVista.CAIDA) {
            BarraAviso(
                texto = sesion.detalle ?: stringResource(R.string.terminal_caida),
                tipo = TipoBarra.ERROR,
                textoAccion = stringResource(R.string.accion_reconectar),
                alAccion = { alReconectar(sesion.id) },
            )
        }

        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (sesion == null) {
                MensajeVacio(
                    titulo = stringResource(R.string.terminal_sin_sesion_titulo),
                    detalle = stringResource(R.string.terminal_sin_sesion_detalle),
                    textoBoton = stringResource(R.string.accion_volver),
                    alPulsar = alVolver,
                    modifier = Modifier.align(Alignment.Center),
                )
            } else {
                LienzoTerminal(
                    pantalla = estado.pantalla,
                    zoom = estado.zoom,
                    seleccion = estado.seleccion,
                    alTocar = { runCatching { foco.requestFocus() } },
                    alRedimensionar = vistaModelo::alRedimensionar,
                    alDesplazar = vistaModelo::alDesplazar,
                    alZoom = vistaModelo::alZoom,
                    alIniciarSeleccion = vistaModelo::iniciarSeleccion,
                    alExtenderSeleccion = vistaModelo::extenderSeleccion,
                    alMoverTiradorInicio = vistaModelo::moverTiradorInicio,
                    alMoverTiradorFin = vistaModelo::moverTiradorFin,
                )
                Row(
                    modifier = Modifier.align(Alignment.BottomEnd).padding(8.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    BotonCabina(
                        texto = stringResource(R.string.accion_pegar),
                        alPulsar = ::pegar,
                    )
                    if (estado.seleccion != null) {
                        BotonCabina(
                            texto = stringResource(R.string.accion_copiar),
                            alPulsar = ::copiar,
                            primario = true,
                        )
                        BotonGlifo(
                            glifo = "✕",
                            descripcion = stringResource(R.string.accion_cancelar_seleccion),
                            alPulsar = vistaModelo::limpiarSeleccion,
                            color = Paleta.GRIS,
                        )
                    }
                }
            }
        }

        if (sesion != null) {
            EntradaTerminal(
                alEscribir = vistaModelo::alEscribir,
                alTecla = vistaModelo::alTeclaFisica,
                alCopiar = ::copiar,
                alPegar = ::pegar,
                foco = foco,
            )
            FilaTeclas(
                modificadores = estado.modificadores,
                alPulsarCtrl = vistaModelo::alPulsarCtrl,
                alPulsarAlt = vistaModelo::alPulsarAlt,
                alTecla = vistaModelo::alPulsarTecla,
            )
        }
    }
}

/** Conmutador de sesiones de la barra del terminal: host (n) con su glifo. */
@Composable
private fun FilaSesionesTerminal(
    sesiones: List<SesionVista>,
    sesionVisible: Long?,
    alElegir: (Long) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 6.dp, vertical = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        sesiones.forEach { sesion ->
            val activa = sesion.id == sesionVisible
            val descripcion = stringResource(R.string.terminal_descripcion_sesion, sesion.titulo)
            Row(
                modifier = Modifier
                    .heightIn(min = 40.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (activa) Paleta.SUPERFICIE else Paleta.FONDO)
                    .border(
                        BorderStroke(1.dp, if (activa) Paleta.AMBAR else Paleta.GRIS.copy(alpha = 0.4f)),
                        RoundedCornerShape(4.dp),
                    )
                    .clickable(onClickLabel = descripcion) { alElegir(sesion.id) }
                    .semantics { contentDescription = descripcion }
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    text = glifoSesion(sesion.estado),
                    style = MaterialTheme.typography.labelLarge,
                    color = colorEstadoSesion(sesion.estado),
                )
                Text(
                    text = sesion.titulo,
                    style = MaterialTheme.typography.labelMedium,
                    color = if (activa) Paleta.TEXTO else Paleta.GRIS,
                )
            }
        }
    }
}
