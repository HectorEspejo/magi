package org.cuatrodetres.magi.ui.flota

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.delay
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BarraAvisoMagi
import org.cuatrodetres.magi.ui.componentes.BotonGlifo
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CampoBusqueda
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.indicadorSincronizacion
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.BloqueFlotaVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.textoSegundaLineaFlota
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla Flota (informe §6.1): sustituye a Hosts y es la pantalla de inicio.
 *
 * La lista se agrupa por grupos plegables; la ficha del host se abre en el
 * panel derecho (plegable abierto o pantalla ancha) o a pantalla completa en
 * compacta. Sondea al abrir y al volver, al deslizar (con la pasada de
 * sincronización) y cada flota_refresco_seg solo con la pantalla a la vista y
 * solo los hosts que se ven (D266, D285).
 */
@Composable
fun PantallaFlota(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    ajustes: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
    alConectar: (String) -> Unit,
    alAbrirServicios: (String) -> Unit,
    alAbrirSnippets: (String) -> Unit,
    alAbrirAsistente: (PasoAsistente) -> Unit,
    alAbrirAjustes: () -> Unit,
    modifier: Modifier = Modifier,
    vistaModelo: VistaModeloFlota = viewModel(
        key = "magi-flota",
        factory = viewModelFactory { initializer { VistaModeloFlota(fuente) } },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    val propietarioCiclo = LocalLifecycleOwner.current

    // Arranque: grupos, flota y primer sondeo manual.
    LaunchedEffect(Unit) { vistaModelo.iniciar() }

    // Al volver a la vista (RESUME) se sondea otra vez, menos la primera vez
    // (que ya lo hace iniciar()).
    var primeraVista by remember { mutableStateOf(true) }
    DisposableEffect(propietarioCiclo) {
        val observador = LifecycleEventObserver { _, evento ->
            if (evento == Lifecycle.Event.ON_RESUME) {
                if (primeraVista) primeraVista = false else vistaModelo.alVolverALaVista()
            }
        }
        propietarioCiclo.lifecycle.addObserver(observador)
        onDispose { propietarioCiclo.lifecycle.removeObserver(observador) }
    }

    // Refresco automático: solo con la pantalla a la vista y solo los hosts que
    // se ven. Se reinicia al cambiar el filtro o el plegado de los grupos.
    LaunchedEffect(propietarioCiclo, ajustes.flotaRefrescoSeg, estado.filtro, estado.grupos) {
        if (ajustes.flotaRefrescoSeg <= 0) return@LaunchedEffect
        propietarioCiclo.lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            while (true) {
                delay(ajustes.flotaRefrescoSeg * 1_000L)
                vistaModelo.refrescarAutomatico()
            }
        }
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val ancha = maxWidth >= 600.dp
        when {
            ancha -> Row(modifier = Modifier.fillMaxSize()) {
                ListaFlota(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    alConectar = alConectar,
                    alAbrirAsistente = alAbrirAsistente,
                    alAbrirAjustes = alAbrirAjustes,
                    modifier = Modifier.weight(0.42f),
                )
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(0.001f)
                        .background(Paleta.GRIS.copy(alpha = 0.4f)),
                )
                if (estado.hostSeleccionado != null) {
                    DetalleFlota(
                        estado = estado,
                        vistaModelo = vistaModelo,
                        alConectar = alConectar,
                        alAbrirServicios = alAbrirServicios,
                        alAbrirSnippets = alAbrirSnippets,
                        modifier = Modifier.weight(0.58f),
                    )
                } else {
                    MensajeVacio(
                        titulo = stringResource(R.string.flota_titulo),
                        detalle = stringResource(R.string.flota_elige_host),
                        modifier = Modifier.weight(0.58f),
                    )
                }
            }
            estado.hostSeleccionado != null -> DetalleFlota(
                estado = estado,
                vistaModelo = vistaModelo,
                alConectar = alConectar,
                alAbrirServicios = alAbrirServicios,
                alAbrirSnippets = alAbrirSnippets,
                modifier = Modifier.fillMaxSize(),
            )
            else -> ListaFlota(
                estado = estado,
                vistaModelo = vistaModelo,
                alConectar = alConectar,
                alAbrirAsistente = alAbrirAsistente,
                alAbrirAjustes = alAbrirAjustes,
                modifier = Modifier.fillMaxSize(),
            )
        }

        val aviso = estado.aviso
        if (aviso != null) {
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAvisoMagi(aviso = aviso, alCerrar = { vistaModelo.limpiarAviso(aviso.identificador) })
            }
        }
    }
}

/** Lista de Flota con su cabecera, búsqueda, grupos plegables y deslizar. */
@Composable
private fun ListaFlota(
    estado: EstadoFlotaUi,
    vistaModelo: VistaModeloFlota,
    alConectar: (String) -> Unit,
    alAbrirAsistente: (PasoAsistente) -> Unit,
    alAbrirAjustes: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var menuAbierto by remember { mutableStateOf(false) }
    val indicador = indicadorSincronizacion(estado.sincronizacion)
    val total = estado.hosts.count { it.esSondeable }

    // Deslizar hacia abajo desde arriba = sondear los filtrados y sincronizar.
    val umbral = with(LocalDensity.current) { 120.dp.toPx() }
    var arrastre by remember { mutableFloatStateOf(0f) }

    Column(
        modifier = modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                detectVerticalDragGestures(
                    onDragEnd = {
                        if (arrastre > umbral) vistaModelo.deslizar()
                        arrastre = 0f
                    },
                    onDragCancel = { arrastre = 0f },
                    onVerticalDrag = { _, delta ->
                        arrastre = (arrastre + delta).coerceAtLeast(0f)
                    },
                )
            },
    ) {
        Box {
            CabeceraMagi(
                titulo = stringResource(R.string.flota_titulo),
                indicador = if (estado.sondeando) {
                    stringResource(R.string.flota_sondeando, estado.leidos, total)
                } else {
                    indicador.glifo + " " + indicador.texto
                },
                colorIndicador = if (estado.sondeando) Paleta.AMBAR else indicador.color,
                alAbrirMenu = { menuAbierto = true },
            )
            DropdownMenu(expanded = menuAbierto, onDismissRequest = { menuAbierto = false }) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.flota_menu_sincronizar)) },
                    onClick = {
                        menuAbierto = false
                        vistaModelo.deslizar()
                    },
                )
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.flota_menu_asistente)) },
                    onClick = {
                        menuAbierto = false
                        alAbrirAsistente(PasoAsistente.UNIR_CLAVE)
                    },
                )
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.flota_menu_ajustes)) },
                    onClick = {
                        menuAbierto = false
                        alAbrirAjustes()
                    },
                )
            }
        }

        CampoBusqueda(texto = estado.filtro, alCambiar = vistaModelo::cambiarFiltro)

        if (estado.sincronizando) {
            BarraAviso(texto = indicador.texto, tipo = TipoBarra.AVISO)
        }

        when {
            estado.hosts.isEmpty() && !estado.cargando -> MensajeVacio(
                titulo = stringResource(R.string.hosts_vacio_titulo),
                detalle = stringResource(R.string.hosts_vacio_detalle),
                glifo = "▣",
                textoBoton = stringResource(R.string.hosts_vacio_boton),
                alPulsar = { alAbrirAsistente(PasoAsistente.UNIR_CLAVE) },
                modifier = Modifier.weight(1f),
            )
            estado.visibles.isEmpty() -> MensajeVacio(
                titulo = stringResource(R.string.hosts_vacio_busqueda_titulo),
                detalle = stringResource(R.string.hosts_vacio_busqueda_detalle),
                glifo = "⌕",
                modifier = Modifier.weight(1f),
            )
            else -> LazyColumn(modifier = Modifier.weight(1f)) {
                items(items = estado.bloques, key = { bloque -> bloque.clave }) { bloque ->
                    BloqueFlota(
                        bloque = bloque,
                        alPlegar = vistaModelo::plegarGrupo,
                        alAbrirFicha = vistaModelo::abrirFicha,
                        alConectar = alConectar,
                    )
                }
            }
        }
    }
}

/** Cabecera de grupo y sus filas (o solo la cabecera si está plegado). */
@Composable
private fun BloqueFlota(
    bloque: BloqueFlotaVista,
    alPlegar: (String) -> Unit,
    alAbrirFicha: (String) -> Unit,
    alConectar: (String) -> Unit,
) {
    val titulo = bloque.grupoNombre ?: stringResource(R.string.flota_sin_grupo)
    if (bloque.grupoUuid != null) {
        val descripcion = if (bloque.plegado) {
            stringResource(R.string.flota_plegar_grupo, titulo, bloque.numHosts)
        } else {
            stringResource(R.string.flota_desplegar_grupo, titulo, bloque.numHosts)
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 40.dp)
                .background(Paleta.SUPERFICIE)
                .clickable(onClickLabel = descripcion) { alPlegar(bloque.grupoUuid) }
                .semantics(mergeDescendants = true) { contentDescription = descripcion }
                .padding(horizontal = 12.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (bloque.plegado) "▸ " else "▾ ",
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
            Text(
                text = titulo.uppercase(),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.AMBAR,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = " (" + bloque.numHosts + ")",
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
        }
    }
    if (!bloque.plegado) {
        bloque.hosts.forEach { host ->
            FilaFlota(
                host = host,
                alAbrirFicha = alAbrirFicha,
                alConectar = alConectar,
            )
        }
    }
}

/** Fila de dos líneas de un host en Flota (mockup §6.1). */
@Composable
private fun FilaFlota(
    host: HostFlotaVista,
    alAbrirFicha: (String) -> Unit,
    alConectar: (String) -> Unit,
) {
    val color = colorEstadoFlota(host.estado)
    val descripcion = if (host.numFallos > 0) {
        stringResource(
            R.string.flota_descripcion,
            host.nombre,
            host.estado.palabra.lowercase(),
            host.numFallos,
            host.sesiones,
        )
    } else {
        stringResource(
            R.string.flota_descripcion_sin_fallos,
            host.nombre,
            host.estado.palabra.lowercase(),
            host.sesiones,
        )
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(onClickLabel = stringResource(R.string.flota_abrir_ficha, host.nombre)) {
                alAbrirFicha(host.uuid)
            }
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(text = host.estado.glifo, style = MaterialTheme.typography.titleMedium, color = color)
                Text(
                    text = host.nombre,
                    style = MaterialTheme.typography.titleSmall,
                    color = Paleta.TEXTO,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (host.numFallos > 0) {
                    Text(
                        text = "⚠" + host.numFallos,
                        style = MaterialTheme.typography.labelMedium,
                        color = Paleta.AMBAR,
                    )
                }
                Text(
                    text = host.estado.palabra,
                    style = MaterialTheme.typography.labelMedium,
                    color = color,
                )
            }
            Text(
                text = textoSegundaLineaFlota(host),
                style = MaterialTheme.typography.bodyMedium,
                color = colorSegundaLinea(host),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (host.esSondeable) {
            BotonGlifo(
                glifo = "▶",
                descripcion = stringResource(R.string.flota_conectar_a, host.nombre),
                alPulsar = { alConectar(host.uuid) },
            )
        }
    }
    Separador()
}

/** Detalle de Flota: la ficha del host seleccionado. */
@Composable
private fun DetalleFlota(
    estado: EstadoFlotaUi,
    vistaModelo: VistaModeloFlota,
    alConectar: (String) -> Unit,
    alAbrirServicios: (String) -> Unit,
    alAbrirSnippets: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ficha = estado.ficha
    if (ficha == null) {
        MensajeVacio(
            titulo = stringResource(R.string.ficha_cargando_titulo),
            detalle = stringResource(R.string.ficha_cargando_detalle),
            modifier = modifier,
        )
        return
    }
    val host = estado.hosts.firstOrNull { it.uuid == ficha.uuid }
    PantallaFichaFlota(
        ficha = ficha,
        host = host,
        tuneles = estado.tunelesFicha,
        conflicto = estado.conflicto,
        alVolver = vistaModelo::cerrarFicha,
        alConectar = { alConectar(ficha.uuid) },
        alAbrirServicios = { alAbrirServicios(ficha.uuid) },
        alAbrirSnippets = { alAbrirSnippets(ficha.uuid) },
        alAlternarTunel = { tunel: TunelVista ->
            // La ficha de la F11 gestiona los túneles a través del ViewModel
            // principal; la Fase 14 deja el botón en la ficha del host y el
            // cableado lo pone AppMagi (android-estructura).
            Unit
        },
        modifier = modifier,
    )
}

/** Color del estado de Flota (glifo y palabra, siempre juntos; T5). */
fun colorEstadoFlota(estado: EstadoFlotaVista): Color = when (estado) {
    EstadoFlotaVista.NOMINAL, EstadoFlotaVista.ALCANZABLE -> Paleta.VERDE
    EstadoFlotaVista.CARGA, EstadoFlotaVista.SONDEANDO -> Paleta.AMBAR
    EstadoFlotaVista.CAIDA -> Paleta.ROJO
    EstadoFlotaVista.FRIA, EstadoFlotaVista.SIN_SONDEO -> Paleta.GRIS
}

/** Color de la segunda línea: ámbar para los culpables, rojo para la caída. */
private fun colorSegundaLinea(host: HostFlotaVista): Color = when (host.estado) {
    EstadoFlotaVista.CARGA -> Paleta.AMBAR
    EstadoFlotaVista.CAIDA -> Paleta.ROJO
    EstadoFlotaVista.SONDEANDO -> Paleta.AMBAR
    else -> Paleta.GRIS
}
