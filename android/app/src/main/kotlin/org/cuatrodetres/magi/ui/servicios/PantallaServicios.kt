package org.cuatrodetres.magi.ui.servicios

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
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
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.plataforma.RecintoSeguro
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.BotonGlifo
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.ChipEstado
import org.cuatrodetres.magi.ui.componentes.Chiptexto
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.logs.PantallaLogs
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.ContenedorVista
import org.cuatrodetres.magi.ui.modelos.EstadoContenedorVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.MotorNombreVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.OrdenPestanaVista
import org.cuatrodetres.magi.ui.modelos.PestanaDetalleVista
import org.cuatrodetres.magi.ui.modelos.ProyectoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccionServicioVista
import org.cuatrodetres.magi.ui.modelos.SeccionServiciosVista
import org.cuatrodetres.magi.ui.modelos.TimerVista
import org.cuatrodetres.magi.ui.modelos.UnidadVista
import org.cuatrodetres.magi.ui.modelos.ahoraSegundos
import org.cuatrodetres.magi.ui.modelos.formatoFecha
import org.cuatrodetres.magi.ui.modelos.tiempoRelativo
import org.cuatrodetres.magi.ui.tema.Paleta

/** Altura mínima táctil (48 dp) de todas las filas y botones. */
private val ALTURA_FILA = 48.dp

/**
 * Pantalla Servicios (informe §6.3, §6.4 y §6.5): «Toda la flota», los
 * servicios de un host con sus tres secciones y el detalle con acciones, sudo y
 * logs. Es autocontenida: gestiona lista y detalle con dos paneles y su propio
 * ViewModel; el terminal lo pinta AppMagi a pantalla completa.
 */
@Composable
fun PantallaServicios(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    hostInicial: String? = null,
    ajustes: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
    alAbrirSesionConOrden: (String, OrdenPestanaVista) -> Unit = { _, _ -> },
    alAbrirTerminal: (Long) -> Unit = {},
    /** Vuelve a Flota desde «Toda la flota» (lo cablea AppMagi). */
    alVolver: () -> Unit = {},
    /** Diálogo de deliberación MAGI (otra pista): plan y resolutor. */
    alDeliberar: ((org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista, (Long?) -> Unit) -> Unit)? = null,
    modifier: Modifier = Modifier,
    vistaModelo: VistaModeloServicios = viewModel(
        key = "magi-servicios",
        factory = viewModelFactory {
            initializer {
                VistaModeloServicios(
                    fuente = fuente,
                    config = {
                        ConfigServicios(
                            refrescoSeg = ajustes.serviciosRefrescoSeg,
                            esperaSeg = ajustes.serviciosEsperaSeg,
                            motores = ajustes.motores,
                            sudoMemoriaMin = ajustes.sudoMemoriaMin,
                        )
                    },
                )
            }
        },
    ),
) {
    val estado by vistaModelo.estado.collectAsStateWithLifecycle()
    val propietarioCiclo = LocalLifecycleOwner.current

    // Panel de logs abierto desde el detalle (dentro de esta pantalla).
    var logsObjetivo by remember { mutableStateOf<ObjetivoVista?>(null) }

    LaunchedEffect(Unit) {
        vistaModelo.iniciar()
        if (hostInicial != null) {
            val nombre = runCatching { fuente.fichaHost(hostInicial)?.nombre }.getOrNull()
                ?: hostInicial
            vistaModelo.abrirHost(hostInicial, nombre)
        }
    }

    // El refresco solo corre con la pantalla a la vista (§6.3).
    DisposableEffect(propietarioCiclo) {
        val observador = LifecycleEventObserver { _, evento ->
            when (evento) {
                Lifecycle.Event.ON_PAUSE -> vistaModelo.alPerderFoco()
                Lifecycle.Event.ON_RESUME -> vistaModelo.alVolverALaVista()
                else -> Unit
            }
        }
        propietarioCiclo.lifecycle.addObserver(observador)
        onDispose { propietarioCiclo.lifecycle.removeObserver(observador) }
    }

    val objetivoLogs = logsObjetivo
    if (objetivoLogs != null) {
        PantallaLogs(
            fuente = fuente,
            hostUuid = estado.hostUuid.orEmpty(),
            hostNombre = estado.hostNombre,
            objetivo = objetivoLogs,
            motor = estado.listado?.motor?.motorUtilizable?.nombre,
            alAbrirSesionConOrden = alAbrirSesionConOrden,
            alAbrirTerminal = alAbrirTerminal,
            // El diálogo de sudo se pinta en esta pantalla, no en el panel de
            // logs: al pedirlo desde logs, se cierra el panel y se abre aquí.
            alPedirSudo = {
                logsObjetivo = null
                vistaModelo.pedirSudo()
            },
            alVolver = { logsObjetivo = null },
            modifier = modifier,
        )
        return
    }

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val ancha = maxWidth >= 600.dp
        when {
            !estado.enHost -> ListaTodaLaFlota(
                estado = estado,
                alAbrirHost = { host -> vistaModelo.abrirHost(host.hostUuid, host.hostNombre) },
                alReintentar = vistaModelo::reintentar,
                alVolver = alVolver,
                modifier = Modifier.fillMaxSize(),
            )
            ancha -> Row(modifier = Modifier.fillMaxSize()) {
                ListaHost(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    alAbrirDetalle = vistaModelo::abrirDetalle,
                    modifier = Modifier.weight(0.42f),
                )
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .width(1.dp)
                        .background(Paleta.GRIS.copy(alpha = 0.4f)),
                )
                if (estado.enDetalle) {
                    DetalleServicio(
                        estado = estado,
                        alCambiarPestana = vistaModelo::cambiarPestana,
                        alPedirAccion = vistaModelo::pedirAccion,
                        alVolver = vistaModelo::volverALista,
                        alAbrirLogs = { objetivo -> logsObjetivo = objetivo },
                        alAbrirSesionConOrden = alAbrirSesionConOrden,
                        alOlvidarSudo = vistaModelo::olvidarSudo,
                alRefrescar = vistaModelo::refrescarDetalle,
                        modifier = Modifier.weight(0.58f),
                    )
                } else {
                    MensajeVacio(
                        titulo = stringResource(R.string.servicios_eligiendo_titulo),
                        detalle = stringResource(R.string.servicios_eligiendo_detalle),
                        modifier = Modifier.weight(0.58f),
                    )
                }
            }
            else -> if (estado.enDetalle) {
                DetalleServicio(
                    estado = estado,
                    alCambiarPestana = vistaModelo::cambiarPestana,
                    alPedirAccion = vistaModelo::pedirAccion,
                    alVolver = vistaModelo::volverALista,
                    alAbrirLogs = { objetivo -> logsObjetivo = objetivo },
                    alAbrirSesionConOrden = alAbrirSesionConOrden,
                    alOlvidarSudo = vistaModelo::olvidarSudo,
                alRefrescar = vistaModelo::refrescarDetalle,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                ListaHost(
                    estado = estado,
                    vistaModelo = vistaModelo,
                    alAbrirDetalle = vistaModelo::abrirDetalle,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }

    estado.aviso?.let { aviso ->
        AlertDialog(
            onDismissRequest = vistaModelo::descartarAviso,
            title = { Text(stringResource(R.string.destino_servicios)) },
            text = { Text(aviso) },
            confirmButton = {
                TextButton(onClick = vistaModelo::descartarAviso) {
                    Text(stringResource(R.string.accion_cerrar))
                }
            },
        )
    }

    estado.confirmacion?.let { confirmacion ->
        DialogoConfirmarAccion(
            confirmacion = confirmacion,
            hostNombre = estado.hostNombre,
            alCancelar = vistaModelo::cancelarConfirmacion,
            alConfirmar = {
                val plan = vistaModelo.planDeliberacion()
                if (confirmacion.info.delibera && plan != null && alDeliberar != null) {
                    // El diálogo llama al resolutor con el id de DELIBERACIONES
                    // (aprobada o forzada) o con null si se cancela.
                    alDeliberar(plan) { id ->
                        if (id == null) vistaModelo.cancelarConfirmacion()
                        else vistaModelo.confirmarAccion(id)
                    }
                } else {
                    // Punto aislado mientras no está el diálogo de deliberación:
                    // el núcleo rechaza la acción con instrucción.
                    vistaModelo.confirmarAccion(null)
                }
            },
        )
    }

    estado.resultado?.let { resultado ->
        DialogoResultado(
            resultado = resultado,
            alCerrar = vistaModelo::descartarResultado,
            alRefrescar = {
                vistaModelo.descartarResultado()
                vistaModelo.refrescarDetalle()
            },
        )
    }

    if (estado.sudo.visible) {
        DialogoSudo(
            sudo = estado.sudo,
            alCerrar = vistaModelo::cerrarSudo,
            alComprobar = vistaModelo::comprobarSudo,
        )
    }
}

// ------------------------------------------------------------ Toda la flota

@Composable
private fun ListaTodaLaFlota(
    estado: EstadoServiciosUi,
    alAbrirHost: (org.cuatrodetres.magi.ui.modelos.HostTodaLaFlotaVista) -> Unit,
    alReintentar: () -> Unit,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val flota = estado.todaLaFlota
    Column(modifier = modifier) {
        CabeceraVolver(
            titulo = stringResource(R.string.servicios_toda_la_flota),
            alVolver = alVolver,
        )
        Text(
            text = stringResource(R.string.servicios_toda_la_flota_detalle),
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
        if (estado.cargandoFlota) {
            MensajeVacio(
                titulo = stringResource(R.string.servicios_toda_la_flota),
                detalle = stringResource(R.string.servicios_cargando),
                modifier = Modifier.weight(1f),
            )
            return@Column
        }
        if (flota.vacia) {
            MensajeVacio(
                titulo = stringResource(R.string.servicios_vacia_titulo),
                detalle = stringResource(R.string.servicios_vacia_detalle),
                modifier = Modifier.weight(1f),
            )
            return@Column
        }
        LazyColumn(modifier = Modifier.weight(1f)) {
            if (flota.fallos.isNotEmpty()) {
                item { EncabezadoSeccion(stringResource(R.string.servicios_fallos)) }
                items(flota.fallos, key = { it.hostUuid + ":" + it.fallo.nombre + ":" + it.fallo.tipo.name }) { fila ->
                    FilaFallo(
                        nombre = fila.fallo.nombre,
                        estado = fila.fallo.estado,
                        host = fila.hostNombre,
                        hora = fila.sondeoEn,
                    )
                }
            }
            item { EncabezadoSeccion(stringResource(R.string.servicios_hosts)) }
            if (flota.hosts.isEmpty()) {
                item(key = "sin-hosts") {
                    Text(
                        stringResource(R.string.servicios_sin_hosts),
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.GRIS,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    )
                }
            }
            items(flota.hosts, key = { "host:" + it.hostUuid }) { host ->
                FilaHostFlota(host = host, alAbrir = { alAbrirHost(host) })
            }
        }
        if (estado.aviso != null) {
            BotonCabina(
                texto = stringResource(R.string.servicios_reintentar),
                alPulsar = alReintentar,
                modifier = Modifier.fillMaxWidth().padding(12.dp),
            )
        }
    }
}

@Composable
private fun FilaFallo(nombre: String, estado: String, host: String, hora: Long?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ALTURA_FILA)
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("✕", color = Paleta.ROJO)
        Column(modifier = Modifier.weight(1f)) {
            Text(nombre, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
            Text(
                host + " · " + estado,
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
            )
        }
        Text(
            textoHace(hora),
            style = MaterialTheme.typography.bodySmall,
            color = Paleta.GRIS,
        )
    }
}

@Composable
private fun FilaHostFlota(
    host: org.cuatrodetres.magi.ui.modelos.HostTodaLaFlotaVista,
    alAbrir: () -> Unit,
) {
    val color = colorEstadoFlota(host.estado)
    val avisoFallos = stringResource(R.string.servicios_fallos_aviso, host.fallos)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ALTURA_FILA)
            .clickable(onClickLabel = host.hostNombre, onClick = alAbrir)
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(host.estado.glifo, color = color)
        Column(modifier = Modifier.weight(1f)) {
            Text(host.hostNombre, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
            if (!host.motivo.isNullOrEmpty()) {
                Text(host.motivo, style = MaterialTheme.typography.bodySmall, color = color)
            }
        }
        if (host.fallos > 0) {
            Text(
                "⚠" + host.fallos,
                color = Paleta.AMBAR,
                modifier = Modifier.semantics {
                    contentDescription = avisoFallos
                },
            )
        }
        Text(host.estado.palabra, style = MaterialTheme.typography.bodySmall, color = color)
    }
}

// ---------------------------------------------------------- Servicios de host

@Composable
private fun ListaHost(
    estado: EstadoServiciosUi,
    vistaModelo: VistaModeloServicios,
    alAbrirDetalle: (ObjetivoVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        CabeceraVolver(
            titulo = estado.hostNombre,
            alVolver = vistaModelo::volverATodaLaFlota,
            acciones = {
                if (estado.refrescando) {
                    Text(stringResource(R.string.servicios_cargando), color = Paleta.GRIS)
                }
            },
        )
        estado.listado?.let { listado ->
            if (listado.avisos.isNotEmpty()) {
                Text(
                    listado.avisos.joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.AMBAR,
                    modifier = Modifier.padding(horizontal = 12.dp),
                )
            }
            if (!listado.systemd.utilizable) {
                Text(
                    stringResource(R.string.servicios_sin_systemd),
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.GRIS,
                    modifier = Modifier.padding(horizontal = 12.dp),
                )
            }
            if (estado.incompleto) {
                Text(
                    stringResource(R.string.servicios_listado_incompleto),
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.AMBAR,
                    modifier = Modifier.padding(horizontal = 12.dp),
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                for (seccion in listado.secciones) {
                    val elegida = seccion == estado.seccion
                    Chiptexto(
                        texto = seccion.etiqueta + " " + listado.cuantos(seccion),
                        modifier = Modifier
                            .heightIn(min = ALTURA_FILA)
                            .clickable(
                                onClickLabel = stringResource(R.string.accion_ver_seccion, seccion.etiqueta),
                                role = androidx.compose.ui.semantics.Role.Tab,
                            ) { vistaModelo.elegirSeccion(seccion) }
                            .background(
                                if (elegida) Paleta.AMBAR.copy(alpha = 0.25f) else Color.Transparent,
                            ),
                    )
                }
            }
            CampoTextoMagi(
                valor = estado.filtro,
                alCambiar = vistaModelo::cambiarFiltro,
                etiqueta = stringResource(R.string.servicios_filtro),
            )
            when (estado.seccion) {
                SeccionServiciosVista.SERVICIOS -> LazyColumn(modifier = Modifier.weight(1f)) {
                    items(estado.unidades, key = { "u:" + it.nombre }) { unidad ->
                        FilaUnidad(
                            unidad = unidad,
                            accion = estado.accionDe(
                                ObjetivoVista(
                                    org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.UNIDAD,
                                    unidad.nombre,
                                ),
                            ),
                            alAbrir = {
                                alAbrirDetalle(
                                    ObjetivoVista(
                                        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.UNIDAD,
                                        unidad.nombre,
                                    ),
                                )
                            },
                        )
                    }
                }
                SeccionServiciosVista.TIMERS -> LazyColumn(modifier = Modifier.weight(1f)) {
                    items(estado.timers, key = { "t:" + it.nombre }) { timer ->
                        FilaTimer(
                            timer = timer,
                            accion = estado.accionDe(
                                ObjetivoVista(
                                    org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.TIMER,
                                    timer.nombre,
                                ),
                            ),
                            alAbrir = {
                                alAbrirDetalle(
                                    ObjetivoVista(
                                        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.TIMER,
                                        timer.nombre,
                                    ),
                                )
                            },
                        )
                    }
                }
                SeccionServiciosVista.CONTENEDORES -> {
                    if (listado.motor.motorUtilizable == null) {
                        Text(
                            stringResource(R.string.servicios_sin_motor),
                            style = MaterialTheme.typography.bodyMedium,
                            color = Paleta.GRIS,
                            modifier = Modifier.padding(12.dp),
                        )
                    } else {
                        LazyColumn(modifier = Modifier.weight(1f)) {
                            for (grupo in estado.gruposContenedores()) {
                                item(key = "g:" + grupo.clave) {
                                    EncabezadoSeccion(
                                        grupo.proyecto?.nombre
                                            ?: stringResource(R.string.servicios_sin_proyecto),
                                    )
                                }
                                items(grupo.contenedores, key = { "c:" + grupo.clave + ":" + it.nombre }) { contenedor ->
                                    FilaContenedor(
                                        contenedor = contenedor,
                                        consumo = estado.consumo[contenedor.id],
                                        accion = estado.accionDe(
                                            ObjetivoVista(
                                                org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.CONTENEDOR,
                                                contenedor.nombre,
                                            ),
                                        ),
                                        alAbrir = {
                                            alAbrirDetalle(
                                                ObjetivoVista(
                                                    org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.CONTENEDOR,
                                                    contenedor.nombre,
                                                ),
                                            )
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
            }
            // Pie con recuentos y, si el host deja de responder, el aviso.
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = stringResource(
                        R.string.servicios_pie,
                        estado.cuantasVisibles(),
                        listado.textoPie,
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.GRIS,
                    modifier = Modifier.weight(1f),
                )
                if (estado.aviso != null) {
                    Text(
                        estado.aviso,
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.ROJO,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        } ?: Column(modifier = Modifier.fillMaxSize()) {
            if (estado.aviso != null) {
                Text(
                    estado.aviso,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.ROJO,
                    modifier = Modifier.padding(12.dp),
                )
                BotonCabina(
                    texto = stringResource(R.string.servicios_reintentar),
                    alPulsar = vistaModelo::reintentar,
                    modifier = Modifier.fillMaxWidth().padding(12.dp),
                )
            } else {
                MensajeVacio(
                    titulo = estado.hostNombre,
                    detalle = stringResource(R.string.servicios_cargando),
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
}

@Composable
private fun FilaUnidad(unidad: UnidadVista, accion: org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista?, alAbrir: () -> Unit) {
    val color = when {
        unidad.fallida -> Paleta.ROJO
        unidad.vigilada -> Paleta.AMBAR
        unidad.activa -> Paleta.VERDE
        else -> Paleta.GRIS
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ALTURA_FILA)
            .clickable(onClickLabel = unidad.nombre, onClick = alAbrir)
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(if (unidad.fallida) "✕" else if (unidad.vigilada) "★" else "·", color = color)
        Column(modifier = Modifier.weight(1f)) {
            Text(unidad.nombre, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
            Text(
                unidad.textoEstado + if (unidad.descripcion.isEmpty()) "" else " · " + unidad.descripcion,
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (accion != null) {
            Text(stringResource(R.string.servicio_en_curso, accion.texto), color = Paleta.AMBAR)
        }
    }
}

@Composable
private fun FilaTimer(timer: TimerVista, accion: org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista?, alAbrir: () -> Unit) {
    val fallido = timer.estadoActivo == "failed"
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ALTURA_FILA)
            .clickable(onClickLabel = timer.nombre, onClick = alAbrir)
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(if (fallido) "✕" else "◷", color = if (fallido) Paleta.ROJO else Paleta.AMBAR)
        Column(modifier = Modifier.weight(1f)) {
            Text(timer.nombre, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
            Text(
                timer.descripcion.ifEmpty { timer.estadoActivo.ifEmpty { stringResource(R.string.servicio_timer) } },
                style = MaterialTheme.typography.bodySmall,
                color = Paleta.GRIS,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (accion != null) {
            Text(stringResource(R.string.servicio_en_curso, accion.texto), color = Paleta.AMBAR)
        }
    }
}

@Composable
private fun FilaContenedor(
    contenedor: ContenedorVista,
    consumo: org.cuatrodetres.magi.ui.modelos.ConsumoVista?,
    accion: org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista?,
    alAbrir: () -> Unit,
) {
    val color = when {
        contenedor.caido -> Paleta.ROJO
        contenedor.estado == EstadoContenedorVista.EN_MARCHA -> Paleta.VERDE
        else -> Paleta.GRIS
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ALTURA_FILA)
            .clickable(onClickLabel = contenedor.nombre, onClick = alAbrir)
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(if (contenedor.caido) "✕" else "▣", color = color)
        Column(modifier = Modifier.weight(1f)) {
            Text(contenedor.nombre, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
            val segunda = listOfNotNull(
                contenedor.estado.texto,
                contenedor.salud.texto.ifEmpty { null },
                consumo?.texto?.ifEmpty { null },
            ).joinToString(" · ")
            Text(segunda, style = MaterialTheme.typography.bodySmall, color = Paleta.GRIS)
        }
        if (accion != null) {
            Text(stringResource(R.string.servicio_en_curso, accion.texto), color = Paleta.AMBAR)
        }
    }
}

// ------------------------------------------------------------------ detalle

@Composable
private fun DetalleServicio(
    estado: EstadoServiciosUi,
    alCambiarPestana: (PestanaDetalleVista) -> Unit,
    alPedirAccion: (ObjetivoVista, AccionServicioVista) -> Unit,
    alVolver: () -> Unit,
    alAbrirLogs: (ObjetivoVista) -> Unit,
    alAbrirSesionConOrden: (String, OrdenPestanaVista) -> Unit,
    alOlvidarSudo: () -> Unit,
    alRefrescar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val objetivo = estado.objetivo ?: return
    val detalle = estado.detalle
    val acciones = accionesPermitidas(estado, objetivo)
    val hostUuid = estado.hostUuid.orEmpty()
    Column(modifier = modifier) {
        CabeceraVolver(
            titulo = objetivo.nombre,
            alVolver = alVolver,
            acciones = {
                BotonGlifo(glifo = "↻", descripcion = stringResource(R.string.servicios_refrescar), alPulsar = alRefrescar)
            },
        )
        Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            for (pestana in PestanaDetalleVista.entries) {
                val elegida = pestana == estado.pestana
                Chiptexto(
                    texto = pestana.etiqueta,
                    modifier = Modifier
                        .heightIn(min = ALTURA_FILA)
                        .clickable(
                        onClickLabel = stringResource(R.string.accion_ver_pestana, pestana.etiqueta),
                        role = androidx.compose.ui.semantics.Role.Tab,
                    ) { alCambiarPestana(pestana) }
                        .background(
                            if (elegida) Paleta.AMBAR.copy(alpha = 0.25f) else Color.Transparent,
                        ),
                )
            }
        }
        if (estado.cargandoDetalle || detalle == null) {
            MensajeVacio(
                titulo = objetivo.textoCompleto,
                detalle = stringResource(R.string.servicio_detalle_cargando),
                modifier = Modifier.weight(1f),
            )
        } else {
            Column(
                modifier = Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 12.dp, vertical = 4.dp),
            ) {
                if (detalle.contenedor != null) {
                    FichaContenedor(detalle.contenedor)
                }
                when (estado.pestana) {
                    PestanaDetalleVista.ESTADO -> {
                        if (detalle.propiedades.isEmpty() && detalle.contenedor == null) {
                            Text(stringResource(R.string.comun_sin_datos), color = Paleta.GRIS)
                        }
                        for (propiedad in detalle.propiedades) {
                            FilaPropiedad(propiedad.clave, propiedad.valor)
                        }
                    }
                    PestanaDetalleVista.DEPENDENCIAS -> {
                        EncabezadoSeccion(stringResource(R.string.servicio_necesita))
                        if (detalle.necesita.isEmpty()) {
                            Text(stringResource(R.string.servicio_sin_dependencias), color = Paleta.GRIS)
                        }
                        for (nombre in detalle.necesita) Text(nombre, color = Paleta.TEXTO)
                        EncabezadoSeccion(stringResource(R.string.servicio_necesitada))
                        if (detalle.necesitada.isEmpty()) {
                            Text(stringResource(R.string.servicio_sin_dependencias), color = Paleta.GRIS)
                        }
                        for (nombre in detalle.necesitada) Text(nombre, color = Paleta.TEXTO)
                    }
                    PestanaDetalleVista.UNIDAD -> {
                        if (detalle.texto.isEmpty()) {
                            Text(stringResource(R.string.comun_sin_datos), color = Paleta.GRIS)
                        } else {
                            Text(detalle.texto, style = MaterialTheme.typography.bodySmall, color = Paleta.TEXTO)
                        }
                    }
                }
            }
            // Acciones permitidas por el núcleo: ninguna tecla ejecuta sola, y
            // las que cambian algo en el host van en mayúsculas.
            if (acciones.isNotEmpty()) {
                EncabezadoSeccion(stringResource(R.string.servicio_acciones))
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    for (accion in acciones) {
                        BotonCabina(
                            texto = accion.textoBoton,
                            alPulsar = { alPedirAccion(objetivo, accion) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                BotonCabina(
                    texto = stringResource(R.string.servicio_logs),
                    alPulsar = { alAbrirLogs(objetivo) },
                    modifier = Modifier.weight(1f),
                )
                if (estado.sudoEnMemoria) {
                    BotonCabina(
                        texto = stringResource(R.string.servicio_olvidar_sudo),
                        alPulsar = alOlvidarSudo,
                        modifier = Modifier.weight(1f),
                    )
                }
                if (objetivo.tipo == org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.CONTENEDOR) {
                    val motor = estado.listado?.motor?.motorUtilizable
                    if (motor != null) {
                        BotonCabina(
                            texto = stringResource(R.string.servicio_shell),
                            alPulsar = {
                                alAbrirSesionConOrden(hostUuid, OrdenPestanaVista.ShellContenedor(motor.nombre, objetivo.nombre))
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun FichaContenedor(contenedor: org.cuatrodetres.magi.ui.modelos.DetalleContenedorVista) {
    FilaPropiedad(stringResource(R.string.servicio_campo_imagen), contenedor.imagen)
    FilaPropiedad(stringResource(R.string.servicio_campo_estado), contenedor.estado.texto)
    contenedor.codigoSalida?.let {
        FilaPropiedad(stringResource(R.string.servicio_campo_salida), it.toString())
    }
    if (contenedor.reinicios > 0) {
        FilaPropiedad(
            stringResource(R.string.servicio_campo_reinicios),
            contenedor.reinicios.toString(),
        )
    }
    if (contenedor.salud != org.cuatrodetres.magi.ui.modelos.SaludVista.NINGUNA) {
        FilaPropiedad(stringResource(R.string.servicio_campo_salud), contenedor.salud.texto)
    }
    if (contenedor.puertos.isNotEmpty()) {
        FilaPropiedad(
            stringResource(R.string.servicio_campo_puertos),
            contenedor.puertos.joinToString(", "),
        )
    }
    if (contenedor.montajes.isNotEmpty()) {
        FilaPropiedad(
            stringResource(R.string.servicio_campo_montajes),
            contenedor.montajes.joinToString(", "),
        )
    }
    if (contenedor.redes.isNotEmpty()) {
        FilaPropiedad(
            stringResource(R.string.servicio_campo_redes),
            contenedor.redes.joinToString(", "),
        )
    }
    contenedor.proyecto?.let {
        FilaPropiedad(stringResource(R.string.servicio_campo_proyecto), it)
    }
    contenedor.orden?.let {
        FilaPropiedad(stringResource(R.string.servicio_campo_orden), it)
    }
}

@Composable
private fun FilaPropiedad(clave: String, valor: String) {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(clave, style = MaterialTheme.typography.bodySmall, color = Paleta.GRIS, modifier = Modifier.width(110.dp))
        Text(valor, style = MaterialTheme.typography.bodySmall, color = Paleta.TEXTO, modifier = Modifier.weight(1f))
    }
}

/** Acciones que el núcleo permite en el estado actual del objetivo. */
private fun accionesPermitidas(estado: EstadoServiciosUi, objetivo: ObjetivoVista): List<AccionServicioVista> {
    val listado = estado.listado ?: return emptyList()
    return when (objetivo.tipo) {
        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.UNIDAD ->
            listado.unidades.firstOrNull { it.nombre == objetivo.nombre }?.acciones ?: emptyList()
        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.TIMER ->
            listado.timers.firstOrNull { it.nombre == objetivo.nombre }?.acciones ?: emptyList()
        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.CONTENEDOR ->
            listado.contenedores.firstOrNull { it.nombre == objetivo.nombre }?.acciones ?: emptyList()
        org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista.PROYECTO ->
            listado.proyectos.firstOrNull { it.nombre == objetivo.nombre }?.acciones ?: emptyList()
    }
}

// --------------------------------------------------------------- diálogos

@Composable
private fun DialogoConfirmarAccion(
    confirmacion: ConfirmacionAccionUi,
    hostNombre: String,
    alCancelar: () -> Unit,
    alConfirmar: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = alCancelar,
        title = { Text(stringResource(R.string.servicio_confirmar_texto, confirmacion.accion.texto, confirmacion.objetivo.nombre, hostNombre)) },
        text = {
            Column {
                if (confirmacion.info.comprobacion != null) {
                    Text(stringResource(R.string.servicio_comprobacion, confirmacion.info.comprobacion))
                }
                if (confirmacion.info.avisaCritica) {
                    Text(
                        stringResource(
                            R.string.servicio_critica,
                            confirmacion.info.hostsTraves.joinToString(", "),
                        ),
                        color = Paleta.ROJO,
                        fontWeight = FontWeight.Bold,
                    )
                }
                if (confirmacion.info.delibera) {
                    Text(
                        stringResource(R.string.servicio_exige_deliberacion),
                        color = Paleta.AMBAR,
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = alConfirmar) { Text(stringResource(R.string.accion_confirmar)) }
        },
        dismissButton = {
            TextButton(onClick = alCancelar) { Text(stringResource(R.string.accion_cancelar)) }
        },
    )
}

@Composable
private fun DialogoResultado(
    resultado: org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista,
    alCerrar: () -> Unit,
    alRefrescar: () -> Unit,
) {
    val color = if (resultado.resultado.esOk) Paleta.VERDE else Paleta.ROJO
    AlertDialog(
        onDismissRequest = alCerrar,
        title = { Text(stringResource(R.string.servicio_resultado_titulo)) },
        text = {
            Column(modifier = Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
                Text(resultado.resultado.glifo + " " + resultado.resultado.texto, color = color, fontWeight = FontWeight.Bold)
                Text(resultado.textoAccion, style = MaterialTheme.typography.bodySmall, color = Paleta.GRIS)
                if (resultado.estado.isNotEmpty()) {
                    Text(
                        stringResource(R.string.servicio_estado, resultado.estado),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                if (resultado.mensaje.isNotEmpty()) Text(resultado.mensaje, color = color)
                if (resultado.lineasDiario.isNotEmpty()) {
                    EncabezadoSeccion(stringResource(R.string.servicio_lineas_diario))
                    for (linea in resultado.lineasDiario) Text(linea, style = MaterialTheme.typography.bodySmall, color = Paleta.TEXTO)
                }
                if (resultado.salidaValidador.isNotEmpty()) {
                    EncabezadoSeccion(stringResource(R.string.servicio_comprobacion_salida))
                    for (linea in resultado.salidaValidador) Text(linea, style = MaterialTheme.typography.bodySmall, color = Paleta.TEXTO)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = alCerrar) { Text(stringResource(R.string.accion_cerrar)) }
        },
        dismissButton = {
            TextButton(onClick = alRefrescar) { Text(stringResource(R.string.servicios_refrescar)) }
        },
    )
}

@Composable
private fun DialogoSudo(
    sudo: EstadoSudoUi,
    alCerrar: () -> Unit,
    alComprobar: (ByteArray) -> Unit,
) {
    var contrasena by remember { mutableStateOf("") }
    RecintoSeguro {
        AlertDialog(
            onDismissRequest = alCerrar,
            title = { Text(stringResource(R.string.servicio_sudo_titulo)) },
            text = {
                Column {
                    Text(
                        stringResource(R.string.servicio_sudo_detalle, sudo.intentosRestantes),
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.GRIS,
                    )
                    CampoTextoMagi(
                        valor = contrasena,
                        alCambiar = { contrasena = it },
                        etiqueta = stringResource(R.string.servicio_sudo_campo),
                        contrasena = true,
                        habilitado = !sudo.comprobando,
                    )
                    sudo.error?.let { Text(it, color = Paleta.ROJO) }
                }
            },
            confirmButton = {
                TextButton(
                    enabled = !sudo.comprobando && contrasena.isNotEmpty(),
                    onClick = {
                        val bytes = contrasena.toByteArray(Charsets.UTF_8)
                        contrasena = ""
                        alComprobar(bytes)
                    },
                ) { Text(stringResource(R.string.servicio_sudo_comprobar)) }
            },
            dismissButton = {
                TextButton(onClick = alCerrar) { Text(stringResource(R.string.accion_cancelar)) }
            },
        )
    }
}

// ------------------------------------------------------------------ ayudas

private fun colorEstadoFlota(estado: EstadoFlotaVista): Color = when (estado) {
    EstadoFlotaVista.NOMINAL, EstadoFlotaVista.ALCANZABLE -> Paleta.VERDE
    EstadoFlotaVista.CARGA, EstadoFlotaVista.SONDEANDO -> Paleta.AMBAR
    EstadoFlotaVista.CAIDA -> Paleta.ROJO
    else -> Paleta.GRIS
}

@Composable
private fun textoHace(instante: Long?): String {
    val relativo = tiempoRelativo(instante, ahoraSegundos())
    if (relativo == null) return stringResource(R.string.servicios_sin_hora)
    val relativoTexto = when (relativo.unidad) {
        org.cuatrodetres.magi.ui.modelos.UnidadTemporal.SEGUNDOS ->
            stringResource(R.string.servicio_hace_segundos, relativo.cantidad.toString())
        org.cuatrodetres.magi.ui.modelos.UnidadTemporal.MINUTOS ->
            stringResource(R.string.servicio_hace_minutos, relativo.cantidad.toString())
        org.cuatrodetres.magi.ui.modelos.UnidadTemporal.HORAS ->
            stringResource(R.string.servicio_hace_horas, relativo.cantidad.toString())
        org.cuatrodetres.magi.ui.modelos.UnidadTemporal.DIAS ->
            stringResource(R.string.servicio_hace_dias, relativo.cantidad.toString())
    }
    return stringResource(R.string.servicios_sondeo_hace, relativoTexto)
}
