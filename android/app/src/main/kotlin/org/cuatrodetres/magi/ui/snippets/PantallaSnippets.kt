package org.cuatrodetres.magi.ui.snippets

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.material3.Checkbox
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.delay
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.CampoBusqueda
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.Chiptexto
import org.cuatrodetres.magi.ui.componentes.ChipEtiqueta
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.TextoHace
import org.cuatrodetres.magi.ui.dialogos.AsistenteDeliberacion
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla Snippets (informe §6.8 y §7.6): la lista de los snippets
 * sincronizados con búsqueda y, al abrir uno, la ejecución preparada (hosts con
 * casillas, variables, parar al primer fallo y timeout).
 *
 * Es autocontenida: su ViewModel lee del núcleo, prepara la ejecución y la
 * lanza; si el núcleo dice que hace falta deliberar, abre el diálogo MAGI.
 * Llegando desde la ficha de un host ([hostInicial]) solo enseña los snippets
 * de ese host y prepara la ejecución solo en él.
 */
@Composable
fun PantallaSnippets(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    hostInicial: String? = null,
    modifier: Modifier = Modifier,
    alVerResultados: (Long) -> Unit = {},
    alVolver: () -> Unit = {},
    vistaModelo: VistaModeloSnippets = viewModel(
        key = "magi-snippets",
        factory = viewModelFactory { initializer { VistaModeloSnippets(fuente) } },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    var lanzada by remember { mutableStateOf<Long?>(null) }

    LaunchedEffect(hostInicial) { vistaModelo.iniciar(hostInicial) }
    LaunchedEffect(estado.ejecucionLanzada) {
        val id = estado.ejecucionLanzada ?: return@LaunchedEffect
        lanzada = id
        alVerResultados(id)
        vistaModelo.descartarEjecucion()
    }
    LaunchedEffect(lanzada) {
        if (lanzada != null) {
            delay(4_000)
            lanzada = null
        }
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val ancha = maxWidth >= 600.dp
        when {
            ancha -> Row(modifier = Modifier.fillMaxSize()) {
                ListaSnippets(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    hostInicial = hostInicial,
                    alVolver = alVolver,
                    modifier = Modifier.weight(0.42f),
                )
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(0.001f)
                        .background(Paleta.GRIS.copy(alpha = 0.4f)),
                )
                if (estado.seleccionado != null) {
                    DetalleEjecucion(
                        estado = estado,
                        vistaModelo = vistaModelo,
                        hostInicial = hostInicial,
                        modifier = Modifier.weight(0.58f),
                    )
                } else {
                    MensajeVacio(
                        titulo = stringResource(R.string.snip_titulo),
                        detalle = stringResource(R.string.snip_elegir),
                        modifier = Modifier.weight(0.58f),
                    )
                }
            }
            estado.seleccionado != null -> DetalleEjecucion(
                estado = estado,
                vistaModelo = vistaModelo,
                hostInicial = hostInicial,
                modifier = Modifier.fillMaxSize(),
            )
            else -> ListaSnippets(
                estado = estado,
                vistaModelo = vistaModelo,
                hostInicial = hostInicial,
                alVolver = alVolver,
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
        if (lanzada != null) {
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAviso(texto = stringResource(R.string.snip_ejecucion_lanzada))
            }
        }

        // Diálogo MAGI a pantalla completa cuando el núcleo pide deliberar.
        AsistenteDeliberacion(
            fuente = fuente,
            plan = estado.planDeliberacion,
            textoMantener = stringResource(R.string.snip_mantener_ejecutar),
            alCancelar = vistaModelo::cancelarDeliberacion,
            alEjecutar = vistaModelo::ejecutarDeliberado,
            clave = "magi-deliberacion-snippets",
        )
    }
}

// ------------------------------------------------------------------- lista

@Composable
private fun ListaSnippets(
    estado: EstadoSnippetsUi,
    vistaModelo: VistaModeloSnippets,
    hostInicial: String?,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize()) {
        if (hostInicial != null) {
            CabeceraVolver(titulo = stringResource(R.string.snip_titulo), alVolver = alVolver)
        } else {
            CabeceraMagi(titulo = stringResource(R.string.snip_titulo))
        }
        CampoBusqueda(texto = estado.filtro, alCambiar = vistaModelo::cambiarFiltro)
        when {
            estado.cargando -> IndicadorCarga(
                texto = stringResource(R.string.snip_cargando),
                modifier = Modifier.weight(1f),
            )
            estado.todos.isEmpty() -> MensajeVacio(
                titulo = stringResource(R.string.snip_vacio_titulo),
                detalle = stringResource(R.string.snip_vacio_detalle),
                glifo = "⌘",
                modifier = Modifier.weight(1f),
            )
            estado.visibles.isEmpty() -> MensajeVacio(
                titulo = stringResource(R.string.snip_vacio_busqueda_titulo),
                detalle = stringResource(R.string.snip_vacio_busqueda_detalle),
                glifo = "⌕",
                modifier = Modifier.weight(1f),
            )
            else -> LazyColumn(modifier = Modifier.weight(1f)) {
                items(items = estado.visibles, key = { it.uuid }) { snippet ->
                    FilaSnippet(snippet = snippet, alAbrir = { vistaModelo.abrir(snippet.uuid) })
                }
            }
        }
    }
}

/** Fila de la lista: nombre, destinos, comando, timeout y último uso. */
@Composable
private fun FilaSnippet(snippet: SnippetVista, alAbrir: () -> Unit) {
    val hosts = snippet.hosts.size
    val destinos = if (hosts > 0) {
        stringResource(
            R.string.snip_destinos_resumen,
            snippet.resumenDestinos,
            hosts,
        )
    } else {
        stringResource(R.string.snip_sin_hosts)
    }
    val descripcion = snippet.nombre + ", " + destinos + ", " +
        stringResource(R.string.snip_timeout, snippet.timeoutSeg)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clickable(onClickLabel = snippet.nombre, onClick = alAbrir)
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = snippet.nombre,
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.TEXTO,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (snippet.critico) {
                Text(
                    text = stringResource(R.string.snip_critico),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.ROJO,
                )
            }
        }
        Text(
            text = destinos,
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.GRIS,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = snippet.comando.replace('\n', ' '),
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.TEXTO,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = stringResource(R.string.snip_timeout, snippet.timeoutSeg),
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.GRIS,
                modifier = Modifier.weight(1f),
            )
            if (snippet.ultimoUsoEn != null) {
                TextoHace(instanteSegundos = snippet.ultimoUsoEn)
            } else {
                Text(
                    text = stringResource(R.string.snip_sin_usar),
                    style = MaterialTheme.typography.labelSmall,
                    color = Paleta.GRIS,
                )
            }
        }
    }
}

// ------------------------------------------------------------------ detalle

@Composable
private fun DetalleEjecucion(
    estado: EstadoSnippetsUi,
    vistaModelo: VistaModeloSnippets,
    hostInicial: String?,
    modifier: Modifier = Modifier,
) {
    val snippet = estado.seleccionado ?: return
    val hosts = if (hostInicial.isNullOrEmpty()) {
        snippet.hosts
    } else {
        snippet.hosts.filter { it.uuid == hostInicial }
    }
    val errores = estado.erroresPorCampo
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = stringResource(R.string.snip_titulo_ejecutar, snippet.nombre),
            alVolver = vistaModelo::cerrarDetalle,
        )
        Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            EncabezadoSeccion(stringResource(R.string.snip_comando))
            Chiptexto(texto = snippet.comando, modifier = Modifier.padding(horizontal = 12.dp))
            if (snippet.descripcion.isNotEmpty()) {
                Text(
                    text = snippet.descripcion,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.GRIS,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                )
            }
            if (snippet.etiquetas.isNotEmpty()) {
                EncabezadoSeccion(stringResource(R.string.snip_etiquetas))
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    for (etiqueta in snippet.etiquetas) ChipEtiqueta(texto = etiqueta)
                }
            }
            if (snippet.escritoriosOmitidos > 0) {
                BarraAviso(
                    texto = stringResource(
                        R.string.snip_escritorios,
                        snippet.escritoriosOmitidos,
                    ),
                    tipo = TipoBarra.AVISO,
                )
            }

            EncabezadoSeccion(
                stringResource(R.string.snip_hosts, estado.hostsMarcados.size, hosts.size),
            )
            if (hosts.isEmpty()) {
                Text(
                    text = stringResource(R.string.snip_sin_hosts),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.ROJO,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                )
            } else {
                for (host in hosts) {
                    FilaHost(
                        host = host,
                        marcado = host.uuid in estado.hostsMarcados,
                        alAlternar = { vistaModelo.alternarHost(host.uuid) },
                    )
                }
            }
            errores["hosts"]?.let { TextoError(it) }

            if (estado.variables.isNotEmpty()) {
                EncabezadoSeccion(stringResource(R.string.snip_variables))
                for (variable in estado.variables) {
                    CampoTextoMagi(
                        valor = variable.valor.orEmpty(),
                        alCambiar = { vistaModelo.cambiarVariable(variable.nombre, it) },
                        etiqueta = variable.nombre,
                    )
                    val defecto = variable.valorDefecto
                    Text(
                        text = if (defecto != null) {
                            stringResource(R.string.snip_variable_defecto, defecto)
                        } else {
                            stringResource(R.string.snip_variable_sin_defecto)
                        },
                        style = MaterialTheme.typography.labelSmall,
                        color = Paleta.GRIS,
                        modifier = Modifier.padding(horizontal = 16.dp),
                    )
                    if (variable.faltaValor) {
                        TextoError(stringResource(R.string.snip_variable_vacia))
                    }
                    errores[variable.nombre]?.let { TextoError(it) }
                }
            }

            errores["snippet"]?.let { TextoError(it) }
            errores["comando"]?.let { TextoError(it) }

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .clickable(
                        onClickLabel = stringResource(R.string.snip_parar_al_fallo),
                        onClick = vistaModelo::alternarPararAlFallo,
                    )
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(checked = estado.pararAlFallo, onCheckedChange = {
                    vistaModelo.alternarPararAlFallo()
                })
                Text(
                    text = stringResource(R.string.snip_parar_al_fallo),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.TEXTO,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
            Text(
                text = estado.preparacion?.textoPie
                    ?: stringResource(R.string.snip_timeout, snippet.timeoutSeg),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BotonCabina(
                texto = stringResource(R.string.accion_cancelar),
                alPulsar = vistaModelo::cerrarDetalle,
            )
            BotonCabina(
                texto = if (estado.preparando) {
                    stringResource(R.string.snip_preparando)
                } else {
                    stringResource(R.string.snip_continuar)
                },
                alPulsar = vistaModelo::continuar,
                habilitado = estado.puedeContinuar && !estado.preparando,
                primario = true,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun FilaHost(
    host: HostDestinoVista,
    marcado: Boolean,
    alAlternar: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clickable(onClickLabel = host.nombre, onClick = alAlternar)
            .semantics(mergeDescendants = true) { contentDescription = host.nombre }
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = marcado, onCheckedChange = { alAlternar() })
        Text(
            text = host.nombre,
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.TEXTO,
            modifier = Modifier.padding(start = 8.dp),
        )
    }
}

@Composable
private fun TextoError(texto: String) {
    Text(
        text = texto,
        style = MaterialTheme.typography.bodySmall,
        color = Paleta.ROJO,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 2.dp),
    )
}
