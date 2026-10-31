package org.cuatrodetres.magi.ui.hosts

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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.geometry.Offset
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonGlifo
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CampoBusqueda
import org.cuatrodetres.magi.ui.componentes.ChipEtiqueta
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.colorEstadoHost
import org.cuatrodetres.magi.ui.componentes.indicadorSincronizacion
import org.cuatrodetres.magi.ui.componentes.textoEstadoHost
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.BloqueInventarioVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.GrupoVista
import org.cuatrodetres.magi.ui.modelos.HostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.modelos.agruparInventario
import org.cuatrodetres.magi.ui.modelos.faseRequiereAccion
import org.cuatrodetres.magi.ui.modelos.glifoEstadoHost
import org.cuatrodetres.magi.ui.modelos.textoDireccion
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pantalla Hosts (informe §6.1): cabecera con indicador ⇅, búsqueda, grupos
 * plegables, filas con glifo + nombre + usuario@dirección:puerto + etiquetas,
 * botón ▶ de conectar, estado vacío y deslizar hacia abajo para sincronizar.
 */
@Composable
fun PantallaHosts(
    estado: EstadoMagi,
    inventario: InventarioVista,
    alCambiarFiltro: (String) -> Unit,
    alPlegarGrupo: (String) -> Unit,
    alAbrirFicha: (String) -> Unit,
    alConectar: (String) -> Unit,
    alSincronizar: () -> Unit,
    alAbrirAsistente: (PasoAsistente) -> Unit,
    alResolverConflicto: (ConflictoHuellaVista) -> Unit,
    alAbrirAjustes: () -> Unit,
    modifier: Modifier = Modifier,
    hostSeleccionado: String? = null,
) {
    var menuAbierto by remember { mutableStateOf(false) }
    val indicador = indicadorSincronizacion(estado.sincronizacion)

    // Deslizar hacia abajo en la lista (en el borde superior) = sincronizar.
    val umbralPx = with(LocalDensity.current) { 120.dp.toPx() }
    var arrastre by remember { mutableFloatStateOf(0f) }
    var yaLanzado by remember { mutableStateOf(false) }
    val conexionTirar = remember(umbralPx) {
        object : NestedScrollConnection {
            override fun onPostScroll(
                consumido: Offset,
                disponible: Offset,
                fuente: NestedScrollSource,
            ): Offset {
                if (disponible.y > 0f) {
                    arrastre += disponible.y
                    if (!yaLanzado && arrastre >= umbralPx) {
                        yaLanzado = true
                        alSincronizar()
                    }
                } else if (disponible.y < 0f) {
                    arrastre = 0f
                }
                return Offset.Zero
            }

            override suspend fun onPostFling(consumido: Velocity, disponible: Velocity): Velocity {
                arrastre = 0f
                yaLanzado = false
                return Velocity.Zero
            }
        }
    }

    Column(modifier = modifier.fillMaxSize().nestedScroll(conexionTirar)) {
        Box {
            CabeceraMagi(
                titulo = stringResource(R.string.hosts_titulo),
                indicador = indicador.glifo + " " + indicador.texto,
                colorIndicador = indicador.color,
                alAbrirMenu = { menuAbierto = true },
            )
            DropdownMenu(expanded = menuAbierto, onDismissRequest = { menuAbierto = false }) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.hosts_menu_sincronizar)) },
                    onClick = {
                        menuAbierto = false
                        alSincronizar()
                    },
                )
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.hosts_menu_asistente)) },
                    onClick = {
                        menuAbierto = false
                        alAbrirAsistente(PasoAsistente.ELEGIR_CARPETA)
                    },
                )
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.hosts_menu_ajustes)) },
                    onClick = {
                        menuAbierto = false
                        alAbrirAjustes()
                    },
                )
            }
        }

        if (estado.sincronizacion.fase == org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SINCRONIZANDO) {
            LinearProgressIndicator(
                modifier = Modifier.fillMaxWidth().heightIn(min = 2.dp),
                color = Paleta.AMBAR,
                trackColor = Paleta.SUPERFICIE,
            )
        }

        AvisosDeSincronizacion(
            estado = estado,
            alAbrirAsistente = alAbrirAsistente,
            alResolverConflicto = alResolverConflicto,
        )

        CampoBusqueda(texto = estado.filtro, alCambiar = alCambiarFiltro)

        if (inventario.vacio) {
            val fase = estado.sincronizacion.fase
            val (titulo, detalle, boton, paso) = when (fase) {
                org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SIN_CONFIGURAR -> Cuarteto(
                    stringResource(R.string.hosts_vacio_titulo),
                    stringResource(R.string.hosts_vacio_detalle),
                    stringResource(R.string.hosts_vacio_boton),
                    PasoAsistente.ELEGIR_CARPETA,
                )
                org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SIN_CLAVE -> Cuarteto(
                    stringResource(R.string.hosts_sin_clave_titulo),
                    stringResource(R.string.hosts_sin_clave_detalle),
                    stringResource(R.string.sync_accion_unir),
                    PasoAsistente.UNIR_CLAVE,
                )
                org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.ESPERANDO_CLAVE -> Cuarteto(
                    stringResource(R.string.sync_fase_esperando_clave),
                    stringResource(R.string.sync_aviso_esperando_clave),
                    stringResource(R.string.sync_accion_escanear),
                    PasoAsistente.UNIR_CLAVE,
                )
                org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.ERROR_CARPETA -> Cuarteto(
                    stringResource(R.string.sync_fase_error_carpeta),
                    stringResource(R.string.sync_aviso_error_carpeta),
                    stringResource(R.string.sync_accion_elegir_carpeta),
                    PasoAsistente.ELEGIR_CARPETA,
                )
                else -> Cuarteto(
                    stringResource(R.string.hosts_vacio_busqueda_titulo),
                    stringResource(R.string.hosts_vacio_busqueda_detalle),
                    null,
                    null,
                )
            }
            MensajeVacio(
                titulo = titulo,
                detalle = detalle,
                textoBoton = boton,
                alPulsar = paso?.let { { alAbrirAsistente(it) } },
                modifier = Modifier.weight(1f),
            )
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                agruparInventario(inventario).forEach { bloque ->
                    item(key = "grupo:" + bloque.clave) {
                        FilaGrupo(
                            bloque = bloque,
                            alPlegar = alPlegarGrupo,
                        )
                    }
                    if (bloque.grupo?.plegado != true) {
                        items(items = bloque.hosts, key = { "host:" + it.uuid }) { host ->
                            FilaHost(
                                host = host,
                                seleccionado = host.uuid == hostSeleccionado,
                                alAbrirFicha = alAbrirFicha,
                                alConectar = alConectar,
                            )
                        }
                    }
                }
                item(key = "pie") {
                    Text(
                        text = stringResource(R.string.hosts_pie_recuento, inventario.hosts.size),
                        style = MaterialTheme.typography.labelSmall,
                        color = Paleta.GRIS,
                        modifier = Modifier.padding(12.dp),
                    )
                }
            }
        }
    }
}

private data class Cuarteto(
    val titulo: String,
    val detalle: String,
    val boton: String?,
    val paso: PasoAsistente?,
)

/** Avisos de sincronización y conflictos de huella sobre la lista (con acción). */
@Composable
private fun AvisosDeSincronizacion(
    estado: EstadoMagi,
    alAbrirAsistente: (PasoAsistente) -> Unit,
    alResolverConflicto: (ConflictoHuellaVista) -> Unit,
) {
    val fase = estado.sincronizacion.fase
    if (faseRequiereAccion(fase)) {
        when (fase) {
            org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SIN_CONFIGURAR -> BarraAviso(
                texto = stringResource(R.string.sync_aviso_sin_configurar),
                tipo = TipoBarra.AVISO,
                textoAccion = stringResource(R.string.sync_accion_elegir_carpeta),
                alAccion = { alAbrirAsistente(PasoAsistente.ELEGIR_CARPETA) },
            )
            org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SIN_CLAVE -> BarraAviso(
                texto = stringResource(R.string.sync_aviso_sin_clave),
                tipo = TipoBarra.AVISO,
                textoAccion = stringResource(R.string.sync_accion_unir),
                alAccion = { alAbrirAsistente(PasoAsistente.UNIR_CLAVE) },
            )
            org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.ESPERANDO_CLAVE -> BarraAviso(
                texto = stringResource(R.string.sync_aviso_esperando_clave),
                tipo = TipoBarra.AVISO,
                textoAccion = stringResource(R.string.sync_accion_escanear),
                alAccion = { alAbrirAsistente(PasoAsistente.UNIR_CLAVE) },
            )
            org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.ERROR_CARPETA -> BarraAviso(
                texto = if (estado.sincronizacion.error != null) {
                    estado.sincronizacion.error
                } else {
                    stringResource(R.string.sync_aviso_error_carpeta)
                },
                tipo = TipoBarra.ERROR,
                textoAccion = stringResource(R.string.sync_accion_elegir_carpeta),
                alAccion = { alAbrirAsistente(PasoAsistente.ELEGIR_CARPETA) },
            )
            else -> Unit
        }
    }
    val conflicto = estado.conflictos.firstOrNull()
    if (conflicto != null) {
        BarraAviso(
            texto = stringResource(
                R.string.hosts_aviso_conflictos,
                estado.conflictos.size,
                conflicto.hostNombre,
            ),
            tipo = TipoBarra.AVISO,
            textoAccion = stringResource(R.string.accion_resolver),
            alAccion = { alResolverConflicto(conflicto) },
        )
    }
}

/** Cabecera de un grupo: glifo de plegado, nombre en mayúsculas y número de hosts. */
@Composable
private fun FilaGrupo(
    bloque: BloqueInventarioVista,
    alPlegar: (String) -> Unit,
) {
    val grupo = bloque.grupo
    val plegado = grupo?.plegado == true
    val nombre = grupo?.nombre?.uppercase() ?: stringResource(R.string.hosts_sin_grupo)
    val clickable = grupo != null
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .background(Paleta.SUPERFICIE)
            .clickable(enabled = clickable) { grupo?.let { alPlegar(it.uuid) } }
            .semantics(mergeDescendants = true) {
                if (clickable) {
                    contentDescription = nombre + ", " + bloque.hosts.size
                }
                heading()
            }
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = if (plegado) "▸" else "▾",
            style = MaterialTheme.typography.labelLarge,
            color = Paleta.GRIS,
            modifier = Modifier.widthIn(min = 12.dp),
        )
        Text(
            text = nombre,
            style = MaterialTheme.typography.labelLarge,
            color = Paleta.AMBAR,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = "(" + bloque.hosts.size + ")",
            style = MaterialTheme.typography.labelMedium,
            color = Paleta.GRIS,
        )
    }
}

/** Fila de host del mockup §6.1, con glifo de estado y botón ▶ de conectar. */
@Composable
private fun FilaHost(
    host: HostVista,
    seleccionado: Boolean,
    alAbrirFicha: (String) -> Unit,
    alConectar: (String) -> Unit,
) {
    val estadoTexto = textoEstadoHost(host.estado, host.sesionesAbiertas)
    val descripcion = stringResource(
        R.string.hosts_descripcion_host,
        host.nombre,
        estadoTexto,
        textoDireccion(host),
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(if (seleccionado) Paleta.SUPERFICIE else Paleta.FONDO)
            .clickable(onClickLabel = stringResource(R.string.accion_abrir_ficha)) { alAbrirFicha(host.uuid) }
            .semantics(mergeDescendants = true) { contentDescription = descripcion }
            .padding(start = 12.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = glifoEstadoHost(host.estado, host.sesionesAbiertas),
            style = MaterialTheme.typography.titleMedium,
            color = colorEstadoHost(host.estado),
            modifier = Modifier.widthIn(min = 22.dp),
        )
        Column(modifier = Modifier.weight(1f).padding(vertical = 8.dp)) {
            Text(
                text = host.nombre,
                style = MaterialTheme.typography.titleSmall,
                color = Paleta.TEXTO,
            )
            Text(
                text = textoDireccion(host),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.GRIS,
            )
            if (host.etiquetas.isNotEmpty()) {
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    host.etiquetas.take(4).forEach { etiqueta ->
                        ChipEtiqueta(texto = etiqueta)
                    }
                    if (host.etiquetas.size > 4) {
                        ChipEtiqueta(texto = "+" + (host.etiquetas.size - 4))
                    }
                }
            }
        }
        BotonGlifo(
            glifo = "▶",
            descripcion = stringResource(R.string.hosts_conectar_a, host.nombre),
            alPulsar = { alConectar(host.uuid) },
            color = Paleta.VERDE,
        )
    }
}

/** Fase vacía usada por la pantalla mientras no hay núcleo (sin datos falsos). */
val InventarioVacio: InventarioVista = InventarioVista()

/** Mensaje de la fuente vacía, reutilizado en documentación de la pantalla. */
val FuenteVaciaDocumentada: String = FuenteNucleoVacia.SIN_NUCLEO
