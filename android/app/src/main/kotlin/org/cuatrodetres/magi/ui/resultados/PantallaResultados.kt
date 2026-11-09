package org.cuatrodetres.magi.ui.resultados

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.ChipEstado
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.dialogos.AsistenteDeliberacion
import org.cuatrodetres.magi.ui.dialogos.DialogoConfirmacionSimple
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.HostEjecucionVista
import org.cuatrodetres.magi.ui.tema.Paleta

/** Color de un estado de host de una ejecución (siempre con su glifo). */
private fun colorEstadoHost(estado: EstadoHostEjecucionVista): Color = when (estado) {
    EstadoHostEjecucionVista.OK -> Paleta.VERDE
    EstadoHostEjecucionVista.FALLO,
    EstadoHostEjecucionVista.ERROR,
    -> Paleta.ROJO
    EstadoHostEjecucionVista.CONECTANDO,
    EstadoHostEjecucionVista.EJECUTANDO,
    -> Paleta.AMBAR
    EstadoHostEjecucionVista.EN_COLA,
    EstadoHostEjecucionVista.CANCELADO,
    EstadoHostEjecucionVista.OMITIDO,
    -> Paleta.GRIS
}

/**
 * Pantalla Resultados (informe §6.9): ejecuciones en curso y terminadas, el
 * estado de cada host y su salida (stdout y stderr, ya sin secuencias de
 * escape), con COPIAR sensible, CANCELAR con confirmación, REPETIR y LIMPIAR.
 *
 * Es autocontenida: recibe las ejecuciones por el flujo del núcleo y pide la
 * salida de cada host a demanda. Repetir vuelve a deliberar si el núcleo lo
 * pide (diálogo MAGI).
 */
@Composable
fun PantallaResultados(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    modifier: Modifier = Modifier,
    vistaModelo: VistaModeloResultados = viewModel(
        key = "magi-resultados",
        factory = viewModelFactory { initializer { VistaModeloResultados(fuente) } },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    var avisoLocal by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) { vistaModelo.iniciar() }
    LaunchedEffect(avisoLocal) {
        if (avisoLocal != null) {
            kotlinx.coroutines.delay(3_000)
            avisoLocal = null
        }
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val ancha = maxWidth >= 600.dp
        when {
            ancha -> Row(modifier = Modifier.fillMaxSize()) {
                ListaResultados(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    modifier = Modifier.weight(0.42f),
                )
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(0.001f)
                        .background(Paleta.GRIS.copy(alpha = 0.4f)),
                )
                if (estado.seleccionada != null) {
                    DetalleResultado(
                        estado = estado,
                        vistaModelo = vistaModelo,
                        alAvisar = { avisoLocal = it },
                        modifier = Modifier.weight(0.58f),
                    )
                } else {
                    MensajeVacio(
                        titulo = stringResource(R.string.res_titulo),
                        detalle = stringResource(R.string.res_elegir),
                        modifier = Modifier.weight(0.58f),
                    )
                }
            }
            estado.seleccionada != null -> DetalleResultado(
                estado = estado,
                vistaModelo = vistaModelo,
                alAvisar = { avisoLocal = it },
                modifier = Modifier.fillMaxSize(),
            )
            else -> ListaResultados(
                estado = estado,
                vistaModelo = vistaModelo,
                modifier = Modifier.fillMaxSize(),
            )
        }

        estado.error?.let { error ->
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAviso(
                    texto = error,
                    tipo = TipoBarra.ERROR,
                    alCerrar = vistaModelo::limpiarError,
                )
            }
        }
        estado.aviso?.let { aviso ->
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAviso(
                    texto = when (aviso) {
                        AvisoResultados.LIMPIADAS -> stringResource(R.string.res_limpiadas)
                        AvisoResultados.REPETIDA -> stringResource(R.string.res_repetida)
                    },
                    alCerrar = vistaModelo::descartarAviso,
                )
            }
        }
        avisoLocal?.let { mensaje ->
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAviso(texto = mensaje, alCerrar = { avisoLocal = null })
            }
        }

        val aCancelar = estado.cancelacionPendiente
        if (aCancelar != null) {
            DialogoConfirmacionSimple(
                titulo = stringResource(R.string.res_confirmar_cancelar_titulo),
                texto = stringResource(R.string.res_confirmar_cancelar_texto),
                alConfirmar = vistaModelo::confirmarCancelar,
                alCancelar = vistaModelo::cancelarConfirmacion,
                textoConfirmar = stringResource(R.string.res_cancelar),
                destructivo = true,
            )
        }

        AsistenteDeliberacion(
            fuente = fuente,
            plan = estado.planDeliberacion,
            textoMantener = stringResource(R.string.snip_mantener_ejecutar),
            alCancelar = vistaModelo::cancelarDeliberacion,
            alEjecutar = vistaModelo::ejecutarDeliberado,
            clave = "magi-deliberacion-resultados",
        )
    }
}

// ------------------------------------------------------------------- lista

@Composable
private fun ListaResultados(
    estado: EstadoResultadosUi,
    vistaModelo: VistaModeloResultados,
    modifier: Modifier = Modifier,
) {
    var menuAbierto by remember { mutableStateOf(false) }
    Column(modifier = modifier.fillMaxSize()) {
        Box {
            CabeceraMagi(
                titulo = stringResource(R.string.res_titulo),
                indicador = if (estado.enCurso > 0) {
                    stringResource(R.string.res_en_curso, estado.enCurso)
                } else {
                    null
                },
                colorIndicador = Paleta.AMBAR,
                alAbrirMenu = { menuAbierto = true },
            )
            DropdownMenu(expanded = menuAbierto, onDismissRequest = { menuAbierto = false }) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.res_limpiar)) },
                    onClick = {
                        menuAbierto = false
                        vistaModelo.limpiar()
                    },
                )
            }
        }
        when {
            estado.cargando -> IndicadorCarga(
                texto = stringResource(R.string.res_cargando),
                modifier = Modifier.weight(1f),
            )
            estado.ejecuciones.isEmpty() -> MensajeVacio(
                titulo = stringResource(R.string.res_vacio_titulo),
                detalle = stringResource(R.string.res_vacio_detalle),
                glifo = "▤",
                modifier = Modifier.weight(1f),
            )
            else -> LazyColumn(modifier = Modifier.weight(1f)) {
                items(items = estado.ejecuciones, key = { it.id }) { ejecucion ->
                    FilaEjecucion(
                        ejecucion = ejecucion,
                        seleccionada = ejecucion.id == estado.seleccionadaId,
                        alAbrir = { vistaModelo.seleccionar(ejecucion.id) },
                    )
                }
            }
        }
    }
}

/** Fila de una ejecución: hora, nombre, estado, ⚑ forzada y recuentos. */
@Composable
private fun FilaEjecucion(
    ejecucion: EjecucionVista,
    seleccionada: Boolean,
    alAbrir: () -> Unit,
) {
    val estado = if (ejecucion.terminada) {
        ejecucion.resultado?.texto ?: stringResource(R.string.res_terminada)
    } else {
        stringResource(R.string.res_en_curso_fila)
    }
    val descripcion = ejecucion.snippetNombre + ", " + estado + ", " + ejecucion.textoRecuentos
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .background(if (seleccionada) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = ejecucion.snippetNombre, onClick = alAbrir)
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = horaCorta(ejecucion.creadaEn),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
            Text(
                text = "  " + ejecucion.snippetNombre,
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.TEXTO,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (ejecucion.forzada) {
                Text(
                    text = stringResource(R.string.res_forzada),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.AMBAR,
                )
            }
        }
        Text(
            text = estado,
            style = MaterialTheme.typography.bodySmall,
            color = if (ejecucion.terminada) Paleta.GRIS else Paleta.AMBAR,
        )
        Text(
            text = ejecucion.textoRecuentos,
            style = MaterialTheme.typography.labelSmall,
            color = Paleta.GRIS,
        )
    }
}

// ------------------------------------------------------------------ detalle

@Composable
private fun DetalleResultado(
    estado: EstadoResultadosUi,
    vistaModelo: VistaModeloResultados,
    alAvisar: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ejecucion = estado.seleccionada ?: return
    val contexto = LocalContext.current
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = ejecucion.snippetNombre + " · " + horaCorta(ejecucion.creadaEn),
            alVolver = vistaModelo::cerrarDetalle,
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (ejecucion.terminada) {
                    stringResource(R.string.res_ejecucion, ejecucion.id)
                } else {
                    stringResource(R.string.res_en_curso, estado.enCurso)
                },
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
                modifier = Modifier.weight(1f),
            )
            if (ejecucion.forzada) {
                Text(
                    text = stringResource(R.string.res_forzada),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.AMBAR,
                )
            }
        }
        EncabezadoSeccion(stringResource(R.string.res_host))
        if (ejecucion.hosts.isEmpty()) {
            Text(
                text = stringResource(R.string.res_vacio_detalle),
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
        } else {
            Column(modifier = Modifier.fillMaxWidth().heightIn(max = 220.dp).verticalScroll(rememberScrollState())) {
                for (host in ejecucion.hosts) {
                    FilaHostEjecucion(
                        host = host,
                        seleccionado = host.hostUuid == estado.hostSeleccionado,
                        alAbrir = { vistaModelo.seleccionarHost(host.hostUuid) },
                    )
                }
            }
        }
        if (estado.hostSeleccionado != null) {
            Salida(estado = estado, vistaModelo = vistaModelo, modifier = Modifier.weight(1f))
        } else {
            Text(
                text = stringResource(R.string.res_elegir_host),
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BotonCabina(
                texto = stringResource(R.string.res_copiar),
                alPulsar = {
                    val copiada = copiarSensible(contexto, estado.textoSalida)
                    alAvisar(
                        if (copiada) {
                            contexto.getString(R.string.res_copiado)
                        } else {
                            contexto.getString(R.string.res_copiado_error)
                        },
                    )
                },
                habilitado = estado.textoSalida.isNotEmpty(),
                modifier = Modifier.weight(1f),
            )
            BotonCabina(
                texto = stringResource(R.string.res_repetir),
                alPulsar = { vistaModelo.repetir(ejecucion.id) },
                modifier = Modifier.weight(1f),
            )
            BotonCabina(
                texto = stringResource(R.string.res_limpiar),
                alPulsar = vistaModelo::limpiar,
            )
        }
        if (ejecucion.sePuedeCancelar) {
            BotonCabina(
                texto = stringResource(R.string.res_cancelar),
                alPulsar = { vistaModelo.pedirCancelar(ejecucion.id) },
                destructivo = true,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            )
        }
    }
}

/** Fila de un host: glifo, nombre, estado, código, duración y bytes. */
@Composable
private fun FilaHostEjecucion(
    host: HostEjecucionVista,
    seleccionado: Boolean,
    alAbrir: () -> Unit,
) {
    val descripcion = host.hostNombre + ", " + host.estado.texto + ", " + host.textoResumen
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .background(if (seleccionado) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = host.hostNombre, onClick = alAbrir)
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ChipEstado(
            glifo = host.estado.glifo,
            texto = host.estado.texto,
            color = colorEstadoHost(host.estado),
        )
        Text(
            text = host.hostNombre,
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.TEXTO,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = host.textoResumen,
            style = MaterialTheme.typography.labelSmall,
            color = Paleta.GRIS,
        )
    }
    if (host.error != null) {
        Text(
            text = host.error,
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.ROJO,
            modifier = Modifier.padding(horizontal = 12.dp),
        )
    }
}

/**
 * Salida del host abierto: pestañas stdout/stderr y las líneas ya saneadas por
 * el núcleo. Es una lista perezosa: una salida de 1 MiB no se compone entera.
 */
@Composable
private fun Salida(
    estado: EstadoResultadosUi,
    vistaModelo: VistaModeloResultados,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize()) {
        EncabezadoSeccion(
            stringResource(R.string.res_salida) + " · " + (estado.host?.hostNombre ?: ""),
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Pestana(
                texto = stringResource(R.string.res_stdout),
                activa = estado.pestana == PestanaSalida.STDOUT,
                alPulsar = { vistaModelo.cambiarPestana(PestanaSalida.STDOUT) },
            )
            Pestana(
                texto = stringResource(R.string.res_stderr),
                activa = estado.pestana == PestanaSalida.STDERR,
                alPulsar = { vistaModelo.cambiarPestana(PestanaSalida.STDERR) },
            )
        }
        if (estado.salidaTruncada) {
            Text(
                text = stringResource(R.string.res_truncada),
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.AMBAR,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
            )
        }
        when {
            estado.cargandoSalida -> IndicadorCarga(
                texto = stringResource(R.string.res_cargando_salida),
                modifier = Modifier.weight(1f),
            )
            estado.textoSalida.isEmpty() -> Text(
                text = stringResource(R.string.res_sin_salida),
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            )
            else -> LazyColumn(modifier = Modifier.weight(1f)) {
                val lineas = estado.textoSalida.lines()
                itemsIndexed(items = lineas) { indice, linea ->
                    Text(
                        text = linea,
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.TEXTO,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                    )
                }
            }
        }
    }
}

/** Pestaña de la salida (stdout o stderr), con glifo y texto siempre. */
@Composable
private fun Pestana(texto: String, activa: Boolean, alPulsar: () -> Unit) {
    Row(
        modifier = Modifier
            .heightIn(min = 48.dp)
            .background(if (activa) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = texto, onClick = alPulsar)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = (if (activa) "▣ " else "□ ") + texto,
            style = MaterialTheme.typography.labelLarge,
            color = if (activa) Paleta.AMBAR else Paleta.GRIS,
        )
    }
}
