package org.cuatrodetres.magi.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.navegacion.BarraInferior
import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.navegacion.DetalleVista
import org.cuatrodetres.magi.navegacion.RielNavegacion
import org.cuatrodetres.magi.navegacion.SeccionAjustes
import org.cuatrodetres.magi.ui.ajustes.PantallaAcercaDe
import org.cuatrodetres.magi.ui.ajustes.PantallaAjustes
import org.cuatrodetres.magi.ui.ajustes.PantallaAjustesTerminal
import org.cuatrodetres.magi.ui.ajustes.PantallaBateria
import org.cuatrodetres.magi.ui.ajustes.PantallaBorrarDatos
import org.cuatrodetres.magi.ui.ajustes.PantallaClaveMovil
import org.cuatrodetres.magi.ui.ajustes.PantallaLicencias
import org.cuatrodetres.magi.ui.ajustes.PantallaSincronizacion
import org.cuatrodetres.magi.ui.bloqueo.PantallaBloqueo
import org.cuatrodetres.magi.ui.componentes.BarraAvisoMagi
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.dialogos.DialogosMagi
import org.cuatrodetres.magi.ui.dialogos.PantallaAsistente
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.fuente.AvisosPlataformaReal
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.hosts.PantallaFicha
import org.cuatrodetres.magi.ui.hosts.PantallaHosts
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.AvisosPlataforma
import org.cuatrodetres.magi.ui.modelos.ControlBloqueo
import org.cuatrodetres.magi.ui.modelos.ControlBloqueoPermitido
import org.cuatrodetres.magi.ui.modelos.EstadoBloqueoVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.inventarioConSesiones
import org.cuatrodetres.magi.ui.modelos.urlNavegador
import org.cuatrodetres.magi.ui.sesiones.PantallaSesiones
import org.cuatrodetres.magi.ui.terminal.PantallaTerminal
import org.cuatrodetres.magi.ui.tema.Paleta
import org.cuatrodetres.magi.ui.tuneles.PantallaTuneles
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloMagi
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloTerminal

/**
 * Punto de entrada de la interfaz de MAGI (informe §6).
 *
 * MainActivity solo tiene que envolverlo en TemaMagi y pasarle la fuente del
 * núcleo y el control de bloqueo de la plataforma. Sin argumentos arranca con
 * [FuenteNucleoVacia] (sin inventario ni datos falsos) y sin bloqueo, para
 * revisar la interfaz mientras no exista el adaptador real.
 *
 * La navegación es adaptativa: barra inferior en compacta y riel + dos paneles
 * en plegable abierto o pantalla ancha, conservando el detalle visible.
 */
@Composable
fun AppMagi(
    fuente: FuenteNucleo = FuenteNucleoVacia,
    controlBloqueo: ControlBloqueo = ControlBloqueoPermitido,
    /** Avisos del segundo plano (sesiones perdidas y guía de batería); null = sin avisos. */
    avisosPlataforma: AvisosPlataforma? = null,
    crearIntentCarpeta: (() -> Intent?)? = null,
    recordarCarpeta: (suspend (Uri) -> Unit)? = null,
    alAbrirBateria: (() -> Unit)? = null,
) {
    val contexto = LocalContext.current
    // Si MainActivity no pasa los avisos del segundo plano, se toman del
    // contenedor de plataforma de la aplicación (MagiApp).
    val avisosSegundoPlano: AvisosPlataforma? = avisosPlataforma ?: runCatching {
        val aplicacion = contexto.applicationContext as org.cuatrodetres.magi.MagiApp
        AvisosPlataformaReal(aplicacion.plataforma.preferencias, aplicacion.plataforma.bateria)
    }.getOrNull()
    val vm: VistaModeloMagi = viewModel(
        key = "magi-magi",
        factory = viewModelFactory {
            initializer { VistaModeloMagi(fuente, controlBloqueo, avisosPlataforma = avisosSegundoPlano) }
        },
    )
    val vmTerminal: VistaModeloTerminal = viewModel(
        key = "magi-terminal",
        factory = viewModelFactory { initializer { VistaModeloTerminal(fuente) } },
    )
    val estado by vm.estado.collectAsStateWithLifecycle()
    val estadoTerminal by vmTerminal.estado.collectAsStateWithLifecycle()
    val alcance = rememberCoroutineScope()
    val propietarioCiclo = LocalLifecycleOwner.current

    // Bloqueo: arranque en frío bloqueado y margen de 5 minutos al volver.
    DisposableEffect(propietarioCiclo) {
        val observador = LifecycleEventObserver { _, evento ->
            when (evento) {
                Lifecycle.Event.ON_START -> vm.alVolverALaApp()
                Lifecycle.Event.ON_STOP -> vm.alSalirDeLaApp()
                else -> Unit
            }
        }
        propietarioCiclo.lifecycle.addObserver(observador)
        onDispose { propietarioCiclo.lifecycle.removeObserver(observador) }
    }
    LaunchedEffect(estado.bloqueo) {
        if (estado.bloqueo == EstadoBloqueoVista.DESBLOQUEADA) vm.iniciar()
    }

    // Selector SAF de la carpeta compartida (solo lectura).
    val launcherCarpeta = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { resultado ->
        val uri = resultado.data?.data
        if (resultado.resultCode == Activity.RESULT_OK && uri != null) {
            alcance.launch {
                if (recordarCarpeta != null) {
                    runCatching { recordarCarpeta.invoke(uri) }
                } else {
                    runCatching {
                        contexto.contentResolver.takePersistableUriPermission(
                            uri,
                            Intent.FLAG_GRANT_READ_URI_PERMISSION,
                        )
                    }
                }
                vm.carpetaElegida(uri.toString(), nombreCarpeta(uri))
            }
        }
    }
    val elegirCarpeta: () -> Unit = {
        val intent = crearIntentCarpeta?.invoke() ?: Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            addFlags(Intent.FLAG_GRANT_PREFIX_URI_PERMISSION)
        }
        runCatching { launcherCarpeta.launch(intent) }
    }

    // Sin intent de la plataforma, se abre la lista del sistema de optimización
    // de batería. La petición directa REQUEST_IGNORE_BATTERY_OPTIMIZATIONS la
    // encapsula plataforma/Bateria.kt, que es la dueña de esa política.
    val abrirBateria: () -> Unit = alAbrirBateria ?: {
        runCatching {
            contexto.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
        }
    }

    // Bloqueada: no se enseña inventario, terminal ni ajustes (informe §3).
    if (estado.bloqueo != EstadoBloqueoVista.DESBLOQUEADA) {
        PantallaBloqueo(
            estadoBloqueo = estado.bloqueo,
            cargando = estado.cargando,
            alDesbloquear = vm::desbloquear,
        )
        return
    }

    BackHandler(enabled = estado.detalle != null || estado.asistente != null) {
        if (estado.asistente != null) vm.cerrarAsistente() else vm.volver()
    }

    // La sesión ocupa toda la pantalla: se ocultan las barras del sistema
    // mientras el terminal está abierto (borde a borde, sin lista ni barra
    // inferior) y se restauran al volver. Con un deslizamiento se muestran.
    val vista = LocalView.current
    val enTerminal = estado.detalle is DetalleVista.Terminal
    DisposableEffect(enTerminal, vista, propietarioCiclo) {
        val ventana = vista.context.aActividad()?.window
        val controlador = ventana?.let {
            WindowCompat.getInsetsController(it, vista)
        }
        fun aplicarBarras() {
            val control = controlador ?: return
            if (enTerminal) {
                control.systemBarsBehavior =
                    WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                control.hide(WindowInsetsCompat.Type.systemBars())
            } else {
                control.show(WindowInsetsCompat.Type.systemBars())
            }
        }
        aplicarBarras()
        val observador = LifecycleEventObserver { _, evento ->
            if (evento == Lifecycle.Event.ON_RESUME) aplicarBarras()
        }
        propietarioCiclo.lifecycle.addObserver(observador)
        onDispose {
            propietarioCiclo.lifecycle.removeObserver(observador)
            controlador?.show(WindowInsetsCompat.Type.systemBars())
        }
    }

    // Suelta las pantallas de sesiones cerradas y asocia la visible.
    LaunchedEffect(estado.sesiones, estado.detalle) {
        val vivas = estado.sesiones.map { it.id }.toSet()
        for (id in vmTerminal.estado.value.pantallas.keys) {
            if (id !in vivas) vmTerminal.limpiarSesion(id)
        }
        val detalle = estado.detalle
        if (detalle is DetalleVista.Terminal) vmTerminal.mostrarSesion(detalle.sesionId)
    }

    val abrirNavegador: (TunelVista) -> Unit = { tunel ->
        val url = urlNavegador(tunel)
        if (url != null) {
            runCatching { contexto.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
        }
    }

    val inventario = inventarioConSesiones(estado.inventario, estado.sesiones)

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Paleta.FONDO)
            .safeDrawingPadding(),
    ) {
        BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
            // Con una sesión abierta el terminal manda: ocupa todo el ancho
            // aunque sea un plegable abierto (sin riel ni lista al lado).
            val ancha = maxWidth >= 600.dp && estado.detalle !is DetalleVista.Terminal
            if (ancha) {
                SuperficieAncha(
                    estado = estado,
                    inventario = inventario,
                    vm = vm,
                    vmTerminal = vmTerminal,
                    alElegirCarpeta = elegirCarpeta,
                    alAbrirNavegador = abrirNavegador,
                    alAbrirBateria = abrirBateria,
                )
            } else {
                SuperficieCompacta(
                    estado = estado,
                    inventario = inventario,
                    vm = vm,
                    vmTerminal = vmTerminal,
                    alElegirCarpeta = elegirCarpeta,
                    alAbrirNavegador = abrirNavegador,
                    alAbrirBateria = abrirBateria,
                )
            }
        }

        // Aviso no bloqueante: Android cerró MAGI con sesiones abiertas (§4.6).
        val sesionesPerdidas = estado.sesionesPerdidas
        if (sesionesPerdidas != null) {
            Box(modifier = Modifier.align(Alignment.TopCenter).fillMaxWidth()) {
                BarraAviso(
                    texto = stringResource(R.string.avisos_sesiones_perdidas_texto, sesionesPerdidas),
                    tipo = TipoBarra.AVISO,
                    textoAccion = stringResource(R.string.avisos_ver_guia),
                    alAccion = vm::abrirGuiaBateria,
                    alCerrar = vm::descartarSesionesPerdidas,
                )
            }
        }

        // Aviso: la clave del móvil se recreó al corregir el desbloqueo facial.
        if (estado.avisoClaveRecreada) {
            Box(modifier = Modifier.align(Alignment.TopCenter).fillMaxWidth()) {
                BarraAviso(
                    texto = stringResource(R.string.aviso_clave_recreada),
                    tipo = TipoBarra.AVISO,
                    alCerrar = vm::descartarAvisoClaveRecreada,
                )
            }
        }

        // Asistente de sincronización a pantalla completa.
        val asistente = estado.asistente
        if (asistente != null) {
            PantallaAsistente(
                asistente = asistente,
                alCerrar = vm::cerrarAsistente,
                alElegirCarpeta = elegirCarpeta,
                alUnirConClave = vm::unirConClave,
            )
        }

        // Pegado de varias líneas sin pegado entre corchetes: confirmación.
        val pegado = estadoTerminal.pegadoPendiente
        if (pegado != null) {
            val lineas = pegado.count { it == '\n' } + 1
            AlertDialog(
                onDismissRequest = vmTerminal::cancelarPegado,
                title = { Text(stringResource(R.string.terminal_pegado_titulo)) },
                text = { Text(stringResource(R.string.terminal_pegado_texto, lineas)) },
                confirmButton = {
                    TextButton(onClick = vmTerminal::confirmarPegado) {
                        Text(stringResource(R.string.accion_pegar))
                    }
                },
                dismissButton = {
                    TextButton(onClick = vmTerminal::cancelarPegado) {
                        Text(stringResource(R.string.accion_cancelar))
                    }
                },
            )
        }

        DialogosMagi(
            dialogo = estado.dialogoActual,
            alCancelar = vm::cancelarDialogo,
            alResponder = vm::responderDialogo,
            alConfirmar = vm::confirmar,
            alResolverConflicto = { conflicto, aceptar ->
                vm.quitarDialogo(org.cuatrodetres.magi.ui.modelos.DialogoVista.ConflictoHuella(conflicto))
                if (aceptar) vm.aceptarHuellaDeOtroEquipo(conflicto) else vm.mantenerMiHuella(conflicto)
            },
            alAbrirGuiaBateria = vm::abrirGuiaBateria,
            alDescartarGuia = vm::descartarGuiaBateria,
        )

        // Aviso efímero (errores del núcleo o avisos de la interfaz).
        val aviso: AvisoVista? = estado.aviso ?: estadoTerminal.aviso
        if (aviso != null) {
            Box(modifier = Modifier.align(Alignment.BottomCenter)) {
                BarraAvisoMagi(
                    aviso = aviso,
                    alCerrar = {
                        vm.limpiarAviso(aviso.identificador)
                        vmTerminal.limpiarAviso(aviso.identificador)
                    },
                )
            }
        }
        val avisoActual by rememberUpdatedState(aviso)
        LaunchedEffect(aviso?.identificador) {
            val actual = avisoActual ?: return@LaunchedEffect
            delay(4500)
            vm.limpiarAviso(actual.identificador)
            vmTerminal.limpiarAviso(actual.identificador)
        }
    }
}

/** Nombre legible de la carpeta elegida con SAF (último segmento del árbol). */
private fun nombreCarpeta(uri: Uri): String {
    val ultimo = uri.lastPathSegment ?: return uri.toString()
    return ultimo.substringAfterLast(':').substringAfterLast('/').ifEmpty { ultimo }
}

/** Disposición compacta: contenido a pantalla completa y barra inferior. */
@Composable
private fun SuperficieCompacta(
    estado: EstadoMagi,
    inventario: org.cuatrodetres.magi.ui.modelos.InventarioVista,
    vm: VistaModeloMagi,
    vmTerminal: VistaModeloTerminal,
    alElegirCarpeta: () -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alAbrirBateria: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        val detalle = estado.detalle
        if (detalle != null) {
            ContenidoDetalle(
                estado = estado,
                detalle = detalle,
                vm = vm,
                vmTerminal = vmTerminal,
                alAbrirBateria = alAbrirBateria,
                alAbrirNavegador = alAbrirNavegador,
                modifier = Modifier.weight(1f),
            )
        } else {
            ContenidoDestino(
                estado = estado,
                inventario = inventario,
                destino = estado.destino,
                vm = vm,
                alElegirCarpeta = alElegirCarpeta,
                alAbrirNavegador = alAbrirNavegador,
                alAbrirBateria = alAbrirBateria,
                modifier = Modifier.weight(1f),
            )
            BarraInferior(destinoActual = estado.destino) { vm.elegirDestino(it) }
        }
    }
}

/** Disposición ancha o plegada abierta: riel, lista y panel de detalle. */
@Composable
private fun SuperficieAncha(
    estado: EstadoMagi,
    inventario: org.cuatrodetres.magi.ui.modelos.InventarioVista,
    vm: VistaModeloMagi,
    vmTerminal: VistaModeloTerminal,
    alElegirCarpeta: () -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alAbrirBateria: () -> Unit,
) {
    Row(modifier = Modifier.fillMaxSize()) {
        RielNavegacion(estado.destino) { vm.elegirDestino(it) }
        Box(
            modifier = Modifier
                .width(1.dp)
                .fillMaxHeight()
                .background(Paleta.GRIS.copy(alpha = 0.4f)),
        )
        ContenidoDestino(
            estado = estado,
            inventario = inventario,
            destino = estado.destino,
            vm = vm,
            alElegirCarpeta = alElegirCarpeta,
            alAbrirNavegador = alAbrirNavegador,
            alAbrirBateria = alAbrirBateria,
            modifier = Modifier.weight(0.42f),
        )
        Box(
            modifier = Modifier
                .width(1.dp)
                .fillMaxHeight()
                .background(Paleta.GRIS.copy(alpha = 0.4f)),
        )
        val detalle = estado.detalle
        if (detalle == null) {
            MensajeVacio(
                titulo = stringResource(R.string.dos_paneles_titulo),
                detalle = stringResource(R.string.dos_paneles_detalle),
                glifo = "▥",
                modifier = Modifier.weight(0.58f).fillMaxSize(),
            )
        } else {
            ContenidoDetalle(
                estado = estado,
                detalle = detalle,
                vm = vm,
                vmTerminal = vmTerminal,
                alAbrirBateria = alAbrirBateria,
                alAbrirNavegador = alAbrirNavegador,
                modifier = Modifier.weight(0.58f),
            )
        }
    }
}

/** Contenido de un destino de primer nivel (Hosts, Sesiones, Túneles o Ajustes). */
@Composable
private fun ContenidoDestino(
    estado: EstadoMagi,
    inventario: org.cuatrodetres.magi.ui.modelos.InventarioVista,
    destino: Destino,
    vm: VistaModeloMagi,
    alElegirCarpeta: () -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alAbrirBateria: () -> Unit,
    modifier: Modifier = Modifier,
) {
    when (destino) {
        Destino.HOSTS -> PantallaHosts(
            estado = estado,
            inventario = inventario,
            alCambiarFiltro = vm::cambiarFiltro,
            alPlegarGrupo = vm::plegarGrupo,
            alAbrirFicha = vm::abrirFicha,
            alConectar = vm::abrirSesion,
            alSincronizar = vm::sincronizarAhora,
            alAbrirAsistente = vm::abrirAsistente,
            alResolverConflicto = vm::resolverConflicto,
            alAbrirAjustes = { vm.elegirDestino(Destino.AJUSTES) },
            modifier = modifier,
            hostSeleccionado = (estado.detalle as? DetalleVista.Ficha)?.hostUuid,
        )
        Destino.SESIONES -> PantallaSesiones(
            sesiones = estado.sesiones,
            sesionVisible = estado.sesionVisible,
            alAbrir = vm::abrirTerminal,
            alReconectar = vm::reconectarSesion,
            alCerrar = vm::cerrarSesion,
            alIrAHosts = { vm.elegirDestino(Destino.HOSTS) },
            modifier = modifier,
        )
        Destino.TUNELES -> PantallaTuneles(
            tuneles = estado.tuneles,
            alAlternar = vm::alternarTunel,
            alRelanzar = vm::relanzarTunel,
            alPuertoLibre = vm::tunelEnPuertoLibre,
            alAbrirNavegador = alAbrirNavegador,
            alDescartar = vm::descartarTunel,
            alIrAHosts = { vm.elegirDestino(Destino.HOSTS) },
            modifier = modifier,
        )
        Destino.AJUSTES -> PantallaAjustes(
            estado = estado,
            alAbrirSeccion = vm::abrirSeccionAjustes,
            modifier = modifier,
        )
    }
}

/** Detalle del panel derecho o de la pantalla compacta. */
@Composable
private fun ContenidoDetalle(
    estado: EstadoMagi,
    detalle: DetalleVista,
    vm: VistaModeloMagi,
    vmTerminal: VistaModeloTerminal,
    alAbrirBateria: () -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    when (detalle) {
        is DetalleVista.Ficha -> {
            val ficha = estado.ficha
            if (ficha == null) {
                MensajeVacio(
                    titulo = stringResource(R.string.ficha_cargando_titulo),
                    detalle = stringResource(R.string.ficha_cargando_detalle),
                    modifier = modifier,
                )
            } else {
                val host = estado.inventario.hosts.firstOrNull { it.uuid == ficha.uuid }
                PantallaFicha(
                    ficha = ficha,
                    estadoHost = host?.estado ?: EstadoHostVista.SIN_SESION,
                    sesionesAbiertas = host?.sesionesAbiertas ?: 0,
                    tuneles = estado.tunelesFicha,
                    conflicto = estado.conflictos.firstOrNull { it.hostUuid == ficha.uuid },
                    alVolver = vm::volver,
                    alConectar = { vm.abrirSesion(ficha.uuid) },
                    alOtraSesion = { vm.abrirSesion(ficha.uuid) },
                    alInstalarClave = { vm.instalarClave(ficha.uuid, ficha.nombre) },
                    alOlvidarContrasena = { vm.pedirOlvidarContrasena(ficha.uuid, ficha.nombre) },
                    alResolverConflicto = {
                        estado.conflictos.firstOrNull { it.hostUuid == ficha.uuid }?.let(vm::resolverConflicto)
                    },
                    alAlternarTunel = vm::alternarTunel,
                    alRelanzarTunel = vm::relanzarTunel,
                    alTunelPuertoLibre = vm::tunelEnPuertoLibre,
                    alAbrirNavegador = alAbrirNavegador,
                    alDescartarTunel = vm::descartarTunel,
                    modifier = modifier,
                )
            }
        }
        is DetalleVista.Terminal -> PantallaTerminal(
            vistaModelo = vmTerminal,
            sesiones = estado.sesiones,
            alVolver = vm::volver,
            alSeleccionarSesion = vm::abrirTerminal,
            alCerrarSesion = vm::cerrarSesion,
            alReconectar = vm::reconectarSesion,
            modifier = modifier,
        )
        is DetalleVista.Ajustes -> when (detalle.seccion) {
            SeccionAjustes.SINCRONIZACION -> PantallaSincronizacion(
                estado = estado,
                alVolver = vm::volver,
                alSincronizar = vm::sincronizarAhora,
                alEscanearClave = { vm.abrirAsistente(PasoAsistente.UNIR_CLAVE) },
                alDejarDeSincronizar = vm::dejarDeSincronizar,
                alResolverConflicto = vm::resolverConflicto,
                modifier = modifier,
            )
            SeccionAjustes.CLAVE_MOVIL -> PantallaClaveMovil(
                estado = estado,
                alVolver = vm::volver,
                alCrearClaveNueva = vm::crearClaveMovil,
                alAbrirSincronizacion = { vm.abrirSeccionAjustes(SeccionAjustes.SINCRONIZACION) },
                modifier = modifier,
            )
            SeccionAjustes.TERMINAL -> PantallaAjustesTerminal(
                zoom = estado.tamanoLetra,
                alCambiarZoom = vm::cambiarTamanoLetra,
                alVolver = vm::volver,
                modifier = modifier,
            )
            SeccionAjustes.BATERIA -> PantallaBateria(
                alVolver = vm::volver,
                alAbrirAjustesBateria = alAbrirBateria,
                modifier = modifier,
            )
            SeccionAjustes.BORRAR_DATOS -> PantallaBorrarDatos(
                alVolver = vm::volver,
                alBorrar = vm::borrarTodo,
                modifier = modifier,
            )
            SeccionAjustes.ACERCA_DE -> PantallaAcercaDe(
                estado = estado,
                alVolver = vm::volver,
                alLicencias = { vm.abrirSeccionAjustes(SeccionAjustes.LICENCIAS) },
                modifier = modifier,
            )
            SeccionAjustes.LICENCIAS -> PantallaLicencias(
                alVolver = vm::volver,
                modifier = modifier,
            )
        }
    }
}

/** Sube por los contextos envueltos hasta la Activity (para las barras). */
private fun Context.aActividad(): Activity? {
    var actual: Context? = this
    while (actual != null) {
        if (actual is Activity) return actual
        actual = (actual as? ContextWrapper)?.baseContext
    }
    return null
}
