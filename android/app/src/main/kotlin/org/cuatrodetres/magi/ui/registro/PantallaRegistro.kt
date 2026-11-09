package org.cuatrodetres.magi.ui.registro

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.ui.res.pluralStringResource
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
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.IndicadorCarga
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.dialogos.TablaDeliberacion
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.ResultadoRegistroVista
import org.cuatrodetres.magi.ui.modelos.TipoRegistroVista
import org.cuatrodetres.magi.ui.tema.Paleta

/** Color del resultado de una entrada (siempre con su glifo). */
private fun colorResultado(resultado: ResultadoRegistroVista): Color = when (resultado) {
    ResultadoRegistroVista.OK -> Paleta.VERDE
    ResultadoRegistroVista.ERROR -> Paleta.ROJO
}

/**
 * Pantalla Registro (informe §6.10 y §7.10): las entradas del propio móvil por
 * páginas de 50, con filtros por host, tipo y texto y el detalle de cada
 * entrada. En las deliberacion_* también enseña la tabla de comprobaciones tal
 * y como quedó en DELIBERACIONES; sin ejecucion_resultado dice «sin resultado:
 * MAGI se cerró durante la ejecución».
 *
 * Es autocontenida: la app solo lee; nunca escribe en el registro.
 */
@Composable
fun PantallaRegistro(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    modifier: Modifier = Modifier,
    vistaModelo: VistaModeloRegistro = viewModel(
        key = "magi-registro",
        factory = viewModelFactory { initializer { VistaModeloRegistro(fuente) } },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    val textoAlEntrar = remember { estado.filtro.texto }

    LaunchedEffect(Unit) { vistaModelo.iniciar() }
    // El texto se aplica con un pequeño retardo para no consultar por cada tecla.
    LaunchedEffect(estado.filtro.texto) {
        if (estado.filtro.texto != textoAlEntrar) {
            delay(350)
            vistaModelo.aplicarFiltro()
        }
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val ancha = maxWidth >= 600.dp
        when {
            ancha -> Row(modifier = Modifier.fillMaxSize()) {
                ListaRegistro(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    modifier = Modifier.weight(0.46f),
                )
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(0.001f)
                        .background(Paleta.GRIS.copy(alpha = 0.4f)),
                )
                if (estado.seleccionada != null) {
                    DetalleRegistro(estado = estado, vistaModelo = vistaModelo, modifier = Modifier.weight(0.54f))
                } else {
                    MensajeVacio(
                        titulo = stringResource(R.string.reg_titulo),
                        detalle = stringResource(R.string.reg_elegir),
                        modifier = Modifier.weight(0.54f),
                    )
                }
            }
            estado.seleccionada != null -> DetalleRegistro(
                estado = estado,
                vistaModelo = vistaModelo,
                modifier = Modifier.fillMaxSize(),
            )
            else -> ListaRegistro(
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
    }
}

// ------------------------------------------------------------------- lista

@Composable
private fun ListaRegistro(
    estado: EstadoRegistroUi,
    vistaModelo: VistaModeloRegistro,
    modifier: Modifier = Modifier,
) {
    var menuHost by remember { mutableStateOf(false) }
    Column(modifier = modifier.fillMaxSize()) {
        Box {
            CabeceraMagi(
                titulo = stringResource(R.string.reg_titulo),
                indicador = pluralStringResource(
                    R.plurals.reg_entradas_cuenta,
                    estado.entradas.size,
                    estado.entradas.size,
                ),
                alAbrirMenu = { menuHost = true },
            )
            DropdownMenu(expanded = menuHost, onDismissRequest = { menuHost = false }) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.reg_todos_hosts)) },
                    onClick = {
                        menuHost = false
                        vistaModelo.cambiarHost(null)
                    },
                )
                for (host in estado.hosts) {
                    DropdownMenuItem(
                        text = { Text(host.nombre) },
                        onClick = {
                            menuHost = false
                            vistaModelo.cambiarHost(host.uuid)
                        },
                    )
                }
            }
        }
        CampoBusqueda(texto = estado.filtro.texto, alCambiar = vistaModelo::cambiarTexto)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ChipFiltro(
                texto = stringResource(R.string.reg_todos),
                activo = estado.filtro.tipos.isEmpty(),
                alPulsar = vistaModelo::limpiarTipos,
            )
            for (tipo in TipoRegistroVista.entries) {
                ChipFiltro(
                    texto = tipo.etiqueta,
                    activo = tipo in estado.filtro.tipos,
                    alPulsar = { vistaModelo.alternarTipo(tipo) },
                )
            }
        }
        if (estado.filtrando) {
            BotonCabina(
                texto = stringResource(R.string.reg_quitar_filtros),
                alPulsar = vistaModelo::limpiarFiltros,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            )
        }
        when {
            estado.cargando -> IndicadorCarga(
                texto = stringResource(R.string.reg_cargando),
                modifier = Modifier.weight(1f),
            )
            estado.entradas.isEmpty() -> MensajeVacio(
                titulo = if (estado.filtrando) {
                    stringResource(R.string.reg_vacio_busqueda_titulo)
                } else {
                    stringResource(R.string.reg_vacio_titulo)
                },
                detalle = if (estado.filtrando) {
                    stringResource(R.string.reg_vacio_busqueda_detalle)
                } else {
                    stringResource(R.string.reg_vacio_detalle)
                },
                glifo = "▦",
                modifier = Modifier.weight(1f),
            )
            else -> LazyColumn(modifier = Modifier.weight(1f)) {
                items(items = estado.entradas, key = { it.id }) { entrada ->
                    FilaEntrada(
                        entrada = entrada,
                        seleccionada = entrada.id == estado.seleccionadaId,
                        alAbrir = { vistaModelo.seleccionar(entrada.id) },
                    )
                }
                if (estado.hayMas) {
                    item(key = "cargar-mas") {
                        if (estado.cargandoMas) {
                            IndicadorCarga(texto = stringResource(R.string.reg_cargando_mas))
                        } else {
                            BotonCabina(
                                texto = stringResource(R.string.reg_cargar_mas),
                                alPulsar = vistaModelo::cargarMas,
                                modifier = Modifier.fillMaxWidth().padding(12.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** Ficha deslizable del filtro por tipo: glifo y texto, selección en ámbar. */
@Composable
private fun ChipFiltro(texto: String, activo: Boolean, alPulsar: () -> Unit) {
    Row(
        modifier = Modifier
            .heightIn(min = 48.dp)
            .background(if (activo) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = texto, onClick = alPulsar)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = (if (activo) "▣ " else "□ ") + texto,
            style = MaterialTheme.typography.labelSmall,
            color = if (activo) Paleta.AMBAR else Paleta.GRIS,
        )
    }
}

/** Fila de una entrada: fecha, tipo, resultado y host con su detalle. */
@Composable
private fun FilaEntrada(
    entrada: EntradaRegistroVista,
    seleccionada: Boolean,
    alAbrir: () -> Unit,
) {
    val descripcion = fechaRegistro(entrada.fecha) + ", " + entrada.tipo + ", " +
        entrada.resultado.texto + ", " + entrada.textoFila
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .background(if (seleccionada) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = entrada.tipo, onClick = alAbrir)
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = fechaRegistro(entrada.fecha),
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.GRIS,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = entrada.tipo,
                style = MaterialTheme.typography.labelSmall,
                color = Paleta.TEXTO,
            )
            Text(
                text = "  " + entrada.resultado.glifo,
                style = MaterialTheme.typography.labelLarge,
                color = colorResultado(entrada.resultado),
            )
        }
        Text(
            text = entrada.textoFila,
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.TEXTO,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// ------------------------------------------------------------------ detalle

@Composable
private fun DetalleRegistro(
    estado: EstadoRegistroUi,
    vistaModelo: VistaModeloRegistro,
    modifier: Modifier = Modifier,
) {
    val entrada = estado.seleccionada ?: return
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = stringResource(R.string.reg_titulo),
            alVolver = vistaModelo::cerrarDetalle,
        )
        if (estado.cargandoDetalle) {
            IndicadorCarga(texto = stringResource(R.string.reg_cargando_detalle))
            return@Column
        }
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            FilaDato(etiqueta = stringResource(R.string.reg_fecha), valor = fechaRegistro(entrada.fecha))
            FilaDato(etiqueta = stringResource(R.string.reg_tipo), valor = entrada.tipo)
            FilaDato(
                etiqueta = stringResource(R.string.reg_host_campo),
                valor = entrada.hostNombre ?: "—",
            )
            FilaDato(
                etiqueta = stringResource(R.string.reg_resultado),
                valor = entrada.resultado.glifo + " " + entrada.resultado.texto,
                colorValor = colorResultado(entrada.resultado),
            )
            FilaDato(
                etiqueta = stringResource(R.string.reg_detalle_campo),
                valor = entrada.detalle,
            )

            val deliberacion = estado.detalle?.deliberacion
            if (deliberacion != null) {
                EncabezadoSeccion(
                    stringResource(R.string.reg_deliberacion, deliberacion.id),
                )
                FilaDato(etiqueta = stringResource(R.string.reg_accion), valor = deliberacion.accion)
                FilaDato(
                    etiqueta = stringResource(R.string.reg_resultado),
                    valor = deliberacion.resultado.texto,
                )
                if (deliberacion.motivo != null) {
                    FilaDato(
                        etiqueta = stringResource(R.string.reg_motivo),
                        valor = deliberacion.motivo,
                        colorValor = Paleta.AMBAR,
                    )
                }
                FilaDato(
                    etiqueta = stringResource(R.string.reg_usuario),
                    valor = deliberacion.usuario,
                )
                FilaDato(
                    etiqueta = stringResource(R.string.reg_consenso),
                    valor = deliberacion.textoConsenso,
                )
                FilaDato(
                    etiqueta = stringResource(R.string.reg_ejecucion),
                    valor = deliberacion.textoResultado,
                    colorValor = if (deliberacion.sinResultado) Paleta.AMBAR else Paleta.TEXTO,
                )
                TablaDeliberacion(filas = deliberacion.filas)
            }
        }
    }
}
