package org.cuatrodetres.magi.ui.dialogos

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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.modelos.ComprobacionVista
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.EstadoDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.FilaHostDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.VeredictoVista
import org.cuatrodetres.magi.ui.modelos.detalle
import org.cuatrodetres.magi.ui.modelos.glifo
import org.cuatrodetres.magi.ui.modelos.motivoForzadoValido
import org.cuatrodetres.magi.ui.tema.Paleta

/** Color de un veredicto (nunca es la única señal: siempre va su glifo). */
fun colorVeredicto(veredicto: VeredictoVista): Color = when (veredicto) {
    is VeredictoVista.Aprueba -> Paleta.VERDE
    is VeredictoVista.Rechaza -> Paleta.ROJO
    is VeredictoVista.Pendiente -> Paleta.AMBAR
    VeredictoVista.NoActiva -> Paleta.GRIS
}

/** Texto del veredicto con su dato: «✓ NOMINAL hace 40 s». */
@Composable
fun textoVeredicto(veredicto: VeredictoVista): String {
    val glifo = veredicto.glifo
    val detalle = veredicto.detalle
    return when {
        detalle != null -> glifo + " " + detalle
        veredicto is VeredictoVista.Pendiente ->
            glifo + " " + stringResource(R.string.delib_veredicto_pendiente)
        else -> glifo + " " + stringResource(R.string.delib_veredicto_no_activa)
    }
}

/**
 * Tabla host × comprobación de una deliberación (informe §6.6 y §6.7). Con un
 * solo host se pintan las comprobaciones en líneas; con varios, en columnas.
 * Tocar una celda enseña el dato que la justifica.
 */
@Composable
fun TablaDeliberacion(
    filas: List<FilaHostDeliberacionVista>,
    modifier: Modifier = Modifier,
) {
    var detalleCelda by remember(filas) { mutableStateOf<String?>(null) }

    fun apuntar(host: String, comprobacion: ComprobacionVista, veredicto: VeredictoVista) {
        val texto = if (veredicto.detalle != null) {
            host + " · " + comprobacion.nombreMagi + " " + comprobacion.palabra + ": " +
                veredicto.glifo + " " + veredicto.detalle
        } else {
            host + " · " + comprobacion.nombreMagi + " " + comprobacion.palabra + ": " +
                veredicto.glifo
        }
        detalleCelda = texto
    }

    Column(modifier = modifier.fillMaxWidth()) {
        if (filas.size <= 1) {
            val fila = filas.firstOrNull()
            if (fila != null) {
                for (comprobacion in ComprobacionVista.entries) {
                    val veredicto = fila.veredicto(comprobacion)
                    val texto = textoVeredicto(veredicto)
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp)
                            .clickable(onClickLabel = stringResource(R.string.delib_detalle_celda)) {
                                apuntar(fila.hostNombre, comprobacion, veredicto)
                            }
                            .semantics(mergeDescendants = true) {
                                contentDescription = comprobacion.nombreMagi + ", " +
                                    comprobacion.palabra + ", " + texto
                            }
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            text = comprobacion.nombreMagi + " · " + comprobacion.palabra,
                            style = MaterialTheme.typography.labelMedium,
                            color = Paleta.GRIS,
                        )
                        Text(
                            text = texto,
                            style = MaterialTheme.typography.bodyMedium,
                            color = colorVeredicto(veredicto),
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Paleta.SUPERFICIE)
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = stringResource(R.string.delib_col_host),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.GRIS,
                    modifier = Modifier.weight(1.4f),
                )
                for (comprobacion in ComprobacionVista.entries) {
                    Text(
                        text = when (comprobacion) {
                            ComprobacionVista.SALUD -> stringResource(R.string.delib_col_salud)
                            ComprobacionVista.BACKUP -> stringResource(R.string.delib_col_backup)
                            ComprobacionVista.TESTS -> stringResource(R.string.delib_col_tests)
                        },
                        style = MaterialTheme.typography.labelSmall,
                        color = Paleta.GRIS,
                        modifier = Modifier.weight(0.8f),
                    )
                }
            }
            Column(modifier = Modifier.fillMaxWidth().heightIn(max = 260.dp).verticalScroll(rememberScrollState())) {
                for (fila in filas) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp)
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = fila.hostNombre,
                            style = MaterialTheme.typography.bodyMedium,
                            color = Paleta.TEXTO,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1.4f),
                        )
                        for (comprobacion in ComprobacionVista.entries) {
                            val veredicto = fila.veredicto(comprobacion)
                            val texto = textoVeredicto(veredicto)
                            Box(
                                modifier = Modifier
                                    .weight(0.8f)
                                    .clickable(onClickLabel = stringResource(R.string.delib_detalle_celda)) {
                                        apuntar(fila.hostNombre, comprobacion, veredicto)
                                    }
                                    .semantics {
                                        contentDescription = fila.hostNombre + ", " +
                                            comprobacion.nombreMagi + " " + comprobacion.palabra +
                                            ", " + texto
                                    }
                                    .padding(vertical = 8.dp),
                            ) {
                                Text(
                                    text = veredicto.glifo,
                                    style = MaterialTheme.typography.titleSmall,
                                    color = colorVeredicto(veredicto),
                                )
                            }
                        }
                    }
                }
            }
        }
        val detalle = detalleCelda
        if (detalle != null) {
            Text(
                text = detalle,
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.TEXTO,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
            )
        } else {
            Text(
                text = stringResource(R.string.delib_detalle_celda),
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            )
        }
    }
}

/**
 * Diálogo MAGI a pantalla completa (informe §6.6, §6.7 y §7.8).
 *
 * Recibe la tabla ya deliberada por el núcleo y avisa con [alEjecutar] de la
 * decisión cuando se mantiene pulsado 1 s. No decide nada por su cuenta: el
 * consenso y los veredictos vienen del núcleo. Se integra desde Snippets,
 * Resultados y Servicios.
 */
@Composable
fun DialogoDeliberacion(
    accion: String,
    tabla: TablaDeliberacionVista,
    textoMantener: String,
    alCancelar: () -> Unit,
    alEjecutar: (DecisionDeliberacionVista) -> Unit,
    modifier: Modifier = Modifier,
    comprobacion: String? = null,
    cargando: Boolean = false,
    error: String? = null,
) {
    var forzando by remember(tabla.id) { mutableStateOf(false) }
    var forzada by remember(tabla.id) { mutableStateOf(false) }
    var motivo by remember(tabla.id) { mutableStateOf("") }
    var avisoSoltar by remember(tabla.id) { mutableStateOf(false) }

    LaunchedEffect(avisoSoltar) {
        if (avisoSoltar) {
            delay(2_500)
            avisoSoltar = false
        }
    }

    val bloqueada = tabla.bloqueada && !forzada
    val sePuedeEjecutar = !cargando && (tabla.estado == EstadoDeliberacionVista.APROBADA || forzada)
    val decision = if (forzada) {
        DecisionDeliberacionVista.Forzar(motivo.trim())
    } else {
        DecisionDeliberacionVista.Aprobar
    }

    Surface(modifier = modifier.fillMaxSize().background(Paleta.FONDO)) {
        Column(modifier = Modifier.fillMaxSize().padding(vertical = 8.dp)) {
            CabeceraMagi(titulo = stringResource(R.string.delib_titulo))
            Column(
                modifier = Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 12.dp),
            ) {
                Text(
                    text = accion,
                    style = MaterialTheme.typography.titleSmall,
                    color = Paleta.TEXTO,
                )
                if (comprobacion != null) {
                    Text(
                        text = stringResource(R.string.delib_antes, comprobacion),
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.GRIS,
                    )
                }
                Separador()
                if (cargando) {
                    IndicadorCarga(texto = stringResource(R.string.delib_cargando))
                } else {
                    TablaDeliberacion(filas = tabla.filas)
                    Separador()
                    if (tabla.sinComprobaciones) {
                        Text(
                            text = stringResource(R.string.delib_sin_comprobaciones),
                            style = MaterialTheme.typography.bodyMedium,
                            color = Paleta.AMBAR,
                        )
                    }
                    Text(
                        text = stringResource(R.string.delib_consenso, tabla.textoConsenso),
                        style = MaterialTheme.typography.labelMedium,
                        color = Paleta.GRIS,
                    )
                    LinearProgressIndicator(
                        progress = { tabla.porcentaje / 100f },
                        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                        color = if (bloqueada) Paleta.ROJO else Paleta.VERDE,
                    )
                    val estado = when {
                        forzada -> EstadoDeliberacionVista.FORZADA
                        tabla.estado == EstadoDeliberacionVista.BLOQUEADA && tabla.bloqueada ->
                            EstadoDeliberacionVista.BLOQUEADA
                        else -> tabla.estado
                    }
                    Text(
                        text = textoEstadoDeliberacion(estado),
                        style = MaterialTheme.typography.labelLarge,
                        color = when (estado) {
                            EstadoDeliberacionVista.BLOQUEADA -> Paleta.ROJO
                            EstadoDeliberacionVista.FORZADA -> Paleta.AMBAR
                            EstadoDeliberacionVista.APROBADA -> Paleta.VERDE
                            EstadoDeliberacionVista.CANCELADA -> Paleta.GRIS
                            EstadoDeliberacionVista.COMPROBANDO -> Paleta.AMBAR
                        },
                    )
                    if (forzando && !forzada) {
                        EncabezadoSeccion(stringResource(R.string.delib_forzar))
                        CampoTextoMagi(
                            valor = motivo,
                            alCambiar = { motivo = it },
                            etiqueta = stringResource(R.string.delib_motivo),
                        )
                        if (motivo.isNotBlank() && !motivoForzadoValido(motivo)) {
                            Text(
                                text = stringResource(R.string.delib_motivo_corto),
                                style = MaterialTheme.typography.bodySmall,
                                color = Paleta.ROJO,
                            )
                        }
                        BotonCabina(
                            texto = stringResource(R.string.delib_forzar_confirmar),
                            alPulsar = { if (motivoForzadoValido(motivo)) forzada = true },
                            habilitado = motivoForzadoValido(motivo),
                            primario = true,
                            modifier = Modifier.padding(vertical = 8.dp),
                        )
                    }
                }
                if (error != null) {
                    Text(
                        text = error,
                        style = MaterialTheme.typography.bodyMedium,
                        color = Paleta.ROJO,
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
            }
            if (avisoSoltar) {
                Text(
                    text = stringResource(R.string.delib_mantener_para_ejecutar),
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.AMBAR,
                    modifier = Modifier.padding(horizontal = 12.dp),
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BotonCabina(texto = stringResource(R.string.accion_cancelar), alPulsar = alCancelar)
                if (bloqueada && !sePuedeEjecutar) {
                    if (!forzando) {
                        BotonCabina(
                            texto = stringResource(R.string.delib_forzar),
                            alPulsar = { forzando = true },
                            primario = true,
                        )
                    }
                } else if (sePuedeEjecutar) {
                    BotonMantener(
                        texto = stringResource(R.string.delib_mantener, textoMantener),
                        alCumplirse = { alEjecutar(decision) },
                        etiquetaAccesible = stringResource(
                            R.string.delib_mantener_accesible,
                            textoMantener,
                        ),
                        alSoltarAntes = { avisoSoltar = true },
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/** Texto del estado del diálogo con su glifo y su explicación (§6.6). */
@Composable
fun textoEstadoDeliberacion(estado: EstadoDeliberacionVista): String = when (estado) {
    EstadoDeliberacionVista.APROBADA -> stringResource(R.string.delib_aprobado)
    EstadoDeliberacionVista.BLOQUEADA -> stringResource(R.string.delib_bloqueado)
    EstadoDeliberacionVista.FORZADA -> stringResource(R.string.delib_forzado)
    EstadoDeliberacionVista.CANCELADA -> stringResource(R.string.delib_cancelado)
    EstadoDeliberacionVista.COMPROBANDO -> stringResource(R.string.delib_comprobando)
}
