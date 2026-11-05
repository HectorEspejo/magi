package org.cuatrodetres.magi.ui.vistaModelo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.navegacion.DetalleVista
import org.cuatrodetres.magi.navegacion.SeccionAjustes
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.ControlBloqueo
import org.cuatrodetres.magi.ui.modelos.ControlBloqueoPermitido
import org.cuatrodetres.magi.ui.modelos.DialogoVista
import org.cuatrodetres.magi.ui.modelos.EstadoBloqueoVista
import org.cuatrodetres.magi.ui.modelos.EstadoMagi
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAperturaVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.modelos.TipoConfirmacion
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.VersionVista
import org.cuatrodetres.magi.ui.modelos.ZOOM_POR_DEFECTO
import org.cuatrodetres.magi.ui.modelos.acotarZoom
import org.cuatrodetres.magi.ui.modelos.numerarSesiones

/**
 * ViewModel de la app: inventario, sincronización, sesiones, túneles, ajustes,
 * diálogos y navegación (informe §6).
 *
 * No habla con UniFFI: recibe una [FuenteNucleo]. En producción será el
 * adaptador real; en las pruebas de la JVM, un doble. Las acciones de red no
 * bloquean la interfaz: todo va por corrutinas y el estado se publica por un
 * [StateFlow].
 */
class VistaModeloMagi(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    private val bloqueo: ControlBloqueo = ControlBloqueoPermitido,
    ambito: CoroutineScope? = null,
    private val avisosPlataforma: org.cuatrodetres.magi.ui.modelos.AvisosPlataforma? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoMagi(bloqueo = bloqueoInicial()))
    val estado: StateFlow<EstadoMagi> = _estado.asStateFlow()

    private var iniciado = false
    private var solicitudInventario = 0L
    private var contadorAvisos = 0L
    private var avisosComprobados = false
    private var guiaBateriaComprobada = false

    init {
        observarFuente()
    }

    // -----------------------------------------------------------------------
    // Arranque y bloqueo
    // -----------------------------------------------------------------------

    private fun bloqueoInicial(): EstadoBloqueoVista = when {
        !bloqueo.hayBloqueoDePantalla() -> EstadoBloqueoVista.SIN_BLOQUEO_DE_PANTALLA
        bloqueo.debeBloquearAlArrancar() -> EstadoBloqueoVista.BLOQUEADA
        else -> EstadoBloqueoVista.DESBLOQUEADA
    }

    private fun observarFuente() {
        alcance.launch {
            fuente.sesiones.collect { lista -> aplicarSesiones(lista) }
        }
        alcance.launch {
            fuente.tuneles.collect { lista ->
                _estado.update { it.copy(tuneles = lista) }
                actualizarTunelesFicha()
            }
        }
        alcance.launch {
            fuente.sincronizacion.collect { sincronizacion ->
                _estado.update { it.copy(sincronizacion = sincronizacion) }
            }
        }
        alcance.launch {
            fuente.peticiones.collect { peticion ->
                agregarDialogo(DialogoVista.DeNucleo(peticion))
            }
        }
        alcance.launch {
            fuente.tamanoLetra.collect { tamano ->
                _estado.update { it.copy(tamanoLetra = acotarZoom(tamano)) }
            }
        }
    }

    private fun aplicarSesiones(lista: List<SesionVista>) {
        val numeradas = numerarSesiones(lista)
        val antesHabia = _estado.value.sesiones.isNotEmpty()
        _estado.update { actual ->
            val detalle = actual.detalle
            val sigueViva = when (detalle) {
                is DetalleVista.Terminal -> numeradas.any { it.id == detalle.sesionId }
                else -> true
            }
            actual.copy(
                sesiones = numeradas,
                detalle = if (sigueViva) detalle else null,
            )
        }
        // Primera sesión de esta ejecución: se ofrece la guía de batería una vez.
        if (!antesHabia && numeradas.isNotEmpty()) quizasOfrecerGuiaBateria()
    }

    /**
     * Al arrancar, avisa si Android cerró MAGI con sesiones abiertas y ofrece la
     * guía de batería (informe §4.6). Es un aviso no bloqueante: solo si el
     * núcleo no tiene sesiones vivas. Sin plataforma inyectada no se muestra.
     */
    fun comprobarAvisosDePlataforma() {
        val avisos = avisosPlataforma ?: return
        if (avisosComprobados) return
        avisosComprobados = true
        alcance.launch {
            val perdidas = try {
                avisos.sesionesQueQuedaronAbiertas()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                0
            }
            if (perdidas > 0 && _estado.value.sesiones.isEmpty()) {
                _estado.update { it.copy(sesionesPerdidas = perdidas) }
            }
            val recreada = try {
                avisos.claveMovilRecreada()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                false
            }
            if (recreada) {
                _estado.update { it.copy(avisoClaveRecreada = true) }
            }
        }
    }

    /** Cierra el aviso de clave del móvil recreada. */
    fun descartarAvisoClaveRecreada() {
        _estado.update { it.copy(avisoClaveRecreada = false) }
    }

    private fun quizasOfrecerGuiaBateria() {
        val avisos = avisosPlataforma ?: return
        if (guiaBateriaComprobada) return
        guiaBateriaComprobada = true
        alcance.launch {
            val ofrecer = try {
                avisos.debeOfrecerseGuiaBateria()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                false
            }
            if (ofrecer) agregarDialogo(DialogoVista.GuiaBateria)
        }
    }

    /** «ABRIR LA GUÍA»: marca la guía como vista y abre Ajustes > Batería. */
    fun abrirGuiaBateria() {
        _estado.update { it.copy(sesionesPerdidas = null) }
        quitarDialogoGuiaBateria()
        avisosPlataforma?.let { avisos ->
            alcance.launch { runCatching { avisos.marcarGuiaBateriaOfrecida() } }
        }
        abrirSeccionAjustes(SeccionAjustes.BATERIA)
    }

    /** «AHORA NO»: no se vuelve a ofrecer automáticamente. */
    fun descartarGuiaBateria() {
        quitarDialogoGuiaBateria()
        avisosPlataforma?.let { avisos ->
            alcance.launch { runCatching { avisos.marcarGuiaBateriaOfrecida() } }
        }
    }

    /** Cierra el aviso no bloqueante de sesiones perdidas. */
    fun descartarSesionesPerdidas() {
        _estado.update { it.copy(sesionesPerdidas = null) }
    }

    private fun quitarDialogoGuiaBateria() {
        _estado.update { actual ->
            actual.copy(dialogos = actual.dialogos.filter { it !is DialogoVista.GuiaBateria })
        }
    }

    /** Abre el núcleo y carga inventario, sincronización, clave y versiones. */
    fun iniciar() {
        if (iniciado) return
        iniciado = true
        alcance.launch {
            _estado.update { it.copy(cargando = true) }
            val resultado = try {
                fuente.iniciar()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            when (resultado) {
                ResultadoAccion.Ok -> {
                    _estado.update { it.copy(iniciado = true) }
                    comprobarAvisosDePlataforma()
                    refrescarInventario()
                    refrescarSincronizacion()
                    refrescarConflictos()
                    refrescarClaveMovil()
                    cargarVersion()
                }
                is ResultadoAccion.Error -> avisoNucleo(resultado.mensaje, true)
            }
            _estado.update { it.copy(cargando = false) }
        }
    }

    /** Pide el desbloqueo al sistema (huella o credencial) y arranca si va bien. */
    fun desbloquear() {
        if (_estado.value.bloqueo == EstadoBloqueoVista.SIN_BLOQUEO_DE_PANTALLA) return
        alcance.launch {
            _estado.update { it.copy(cargando = true) }
            val permitido = try {
                bloqueo.solicitarDesbloqueo()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                false
            }
            _estado.update {
                it.copy(
                    bloqueo = if (permitido) EstadoBloqueoVista.DESBLOQUEADA else EstadoBloqueoVista.BLOQUEADA,
                    cargando = false,
                )
            }
            if (permitido) iniciar()
        }
    }

    /** La app vuelve a primer plano: se bloquea si el margen de 5 min se agotó. */
    fun alVolverALaApp() {
        if (_estado.value.bloqueo != EstadoBloqueoVista.DESBLOQUEADA) return
        val hayQueBloquear = try {
            bloqueo.alVolverALaApp()
        } catch (e: Exception) {
            if (e is kotlinx.coroutines.CancellationException) throw e
            true
        }
        if (hayQueBloquear) {
            _estado.update { it.copy(bloqueo = EstadoBloqueoVista.BLOQUEADA) }
        }
    }

    /** La app pasa a segundo plano: el bloqueo anota la hora de salida. */
    fun alSalirDeLaApp() {
        try {
            bloqueo.alSalirDeLaApp()
        } catch (e: Exception) {
            if (e is kotlinx.coroutines.CancellationException) throw e
            // Sin bloqueo disponible no se pierde nada: la app queda como estaba.
        }
    }

    // -----------------------------------------------------------------------
    // Navegación
    // -----------------------------------------------------------------------

    fun elegirDestino(destino: Destino) {
        _estado.update { it.copy(destino = destino, detalle = null, ficha = null) }
        when (destino) {
            Destino.AJUSTES -> {
                refrescarSincronizacion()
                refrescarClaveMovil()
                cargarVersion()
            }
            Destino.TUNELES -> refrescarTuneles()
            Destino.SESIONES -> Unit
            Destino.HOSTS -> refrescarInventario()
        }
    }

    fun abrirSeccionAjustes(seccion: SeccionAjustes) {
        _estado.update { it.copy(detalle = DetalleVista.Ajustes(seccion)) }
        when (seccion) {
            SeccionAjustes.SINCRONIZACION -> refrescarSincronizacion()
            SeccionAjustes.CLAVE_MOVIL -> refrescarClaveMovil()
            SeccionAjustes.ACERCA_DE, SeccionAjustes.LICENCIAS -> cargarVersion()
            else -> Unit
        }
    }

    /** «Atrás» del detalle: vuelve a la lista sin cerrar sesiones ni túneles. */
    fun volver() {
        _estado.update { it.copy(detalle = null, ficha = null, tunelesFicha = emptyList()) }
    }

    fun abrirTerminal(sesionId: Long) {
        _estado.update { it.copy(detalle = DetalleVista.Terminal(sesionId)) }
    }

    // -----------------------------------------------------------------------
    // Inventario
    // -----------------------------------------------------------------------

    fun refrescarInventario() {
        lanzarInventario(_estado.value.filtro)
    }

    /** Deslizar hacia abajo en Hosts: sincronizar y refrescar. */
    fun refrescarTodo() {
        sincronizarAhora()
    }

    fun cambiarFiltro(texto: String) {
        _estado.update { it.copy(filtro = texto) }
        lanzarInventario(texto)
    }

    private fun lanzarInventario(filtro: String) {
        val solicitud = ++solicitudInventario
        alcance.launch {
            val inventario = try {
                fuente.inventario(filtro.ifBlank { null })
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty(), true)
                return@launch
            }
            if (solicitud != solicitudInventario) return@launch
            _estado.update { it.copy(inventario = inventario) }
        }
    }

    fun plegarGrupo(uuid: String) {
        val grupo = _estado.value.inventario.grupos.firstOrNull { it.uuid == uuid } ?: return
        val plegado = !grupo.plegado
        _estado.update { actual ->
            actual.copy(
                inventario = actual.inventario.copy(
                    grupos = actual.inventario.grupos.map { if (it.uuid == uuid) it.copy(plegado = plegado) else it },
                ),
            )
        }
        lanzarAccion(bloque = { fuente.plegarGrupo(uuid, plegado) })
    }

    fun abrirFicha(hostUuid: String) {
        alcance.launch {
            val ficha = try {
                fuente.fichaHost(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty(), true)
                null
            }
            if (ficha == null) {
                avisoInterfaz(TipoAviso.HOST_DESAPARECIDO)
                return@launch
            }
            _estado.update { it.copy(detalle = DetalleVista.Ficha(hostUuid), ficha = ficha) }
            val tuneles = try {
                fuente.tuneles(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                emptyList()
            }
            _estado.update { it.copy(tunelesFicha = tuneles) }
        }
    }

    fun abrirSesion(hostUuid: String) {
        // Un escritorio (RDP, VNC o voxl) no se abre desde el móvil (F12): se
        // enseña en Hosts y se abre en el ordenador. Tampoco crea sesiones ni
        // túneles.
        val host = _estado.value.inventario.hosts.firstOrNull { it.uuid == hostUuid }
        if (host != null && host.tipo.esEscritorio) {
            avisoInterfaz(TipoAviso.ESCRITORIO_EN_EL_ORDENADOR, host.nombre)
            return
        }
        alcance.launch {
            _estado.update { it.copy(cargando = true) }
            val resultado = try {
                fuente.abrirSesion(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAperturaVista.Error(e.message.orEmpty())
            }
            _estado.update { it.copy(cargando = false) }
            when (resultado) {
                is ResultadoAperturaVista.Abierta ->
                    _estado.update { it.copy(detalle = DetalleVista.Terminal(resultado.sesion)) }
                is ResultadoAperturaVista.Error -> avisoNucleo(resultado.mensaje, true)
            }
        }
    }

    fun cerrarSesion(sesionId: Long) {
        val sesion = _estado.value.sesiones.firstOrNull { it.id == sesionId }
        agregarDialogo(
            DialogoVista.Confirmacion(
                tipo = TipoConfirmacion.CERRAR_SESION,
                detalle = sesion?.titulo,
                sesionId = sesionId,
            ),
        )
    }

    fun reconectarSesion(sesionId: Long) {
        lanzarAccion({ fuente.reconectar(sesionId) })
    }

    // -----------------------------------------------------------------------
    // Sincronización
    // -----------------------------------------------------------------------

    fun refrescarSincronizacion() {
        alcance.launch {
            val sincronizacion = try {
                fuente.sincronizacionEstado()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                return@launch
            }
            _estado.update { it.copy(sincronizacion = sincronizacion) }
        }
    }

    fun refrescarConflictos() {
        alcance.launch {
            val conflictos = try {
                fuente.conflictosHuella()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                emptyList()
            }
            _estado.update { it.copy(conflictos = conflictos) }
        }
    }

    fun sincronizarAhora() {
        if (_estado.value.sincronizacion.fase == org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SINCRONIZANDO) return
        _estado.update {
            it.copy(sincronizacion = it.sincronizacion.copy(fase = org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista.SINCRONIZANDO))
        }
        alcance.launch {
            val resultado = try {
                fuente.sincronizarAhora()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            when (resultado) {
                ResultadoAccion.Ok -> avisoInterfaz(TipoAviso.SINCRONIZACION_AL_DIA)
                is ResultadoAccion.Error -> avisoNucleo(resultado.mensaje, true)
            }
            refrescarSincronizacion()
            refrescarInventario()
            refrescarConflictos()
        }
    }

    fun abrirAsistente(paso: PasoAsistente = PasoAsistente.ELEGIR_CARPETA) {
        val sincronizacion = _estado.value.sincronizacion
        val pasoReal = if (sincronizacion.tieneCarpeta) PasoAsistente.UNIR_CLAVE else paso
        _estado.update { it.copy(asistente = org.cuatrodetres.magi.ui.modelos.AsistenteVista(paso = pasoReal)) }
    }

    fun cerrarAsistente() {
        _estado.update { it.copy(asistente = null) }
    }

    /** Vuelta del selector SAF: persiste el permiso (en el adaptador) y valida la carpeta. */
    fun carpetaElegida(uri: String, nombre: String?) {
        val asistente = _estado.value.asistente ?: return
        _estado.update {
            it.copy(
                asistente = asistente.copy(
                    ocupado = true,
                    error = null,
                    carpeta = nombre ?: asistente.carpeta,
                ),
            )
        }
        alcance.launch {
            val resultado = try {
                fuente.elegirCarpeta(uri)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            when (resultado) {
                ResultadoAccion.Ok -> {
                    _estado.update { actual ->
                        actual.copy(asistente = actual.asistente?.copy(paso = PasoAsistente.UNIR_CLAVE, ocupado = false))
                    }
                    refrescarSincronizacion()
                }
                is ResultadoAccion.Error -> _estado.update { actual ->
                    actual.copy(asistente = actual.asistente?.copy(ocupado = false, error = resultado.mensaje))
                }
            }
        }
    }

    fun unirConClave(texto: String) {
        val asistente = _estado.value.asistente ?: return
        if (texto.isBlank()) return
        _estado.update { it.copy(asistente = asistente.copy(ocupado = true, error = null)) }
        alcance.launch {
            val resultado = try {
                fuente.unirConClave(texto)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            when (resultado) {
                ResultadoAccion.Ok -> {
                    _estado.update { it.copy(asistente = null) }
                    refrescarInventario()
                    refrescarSincronizacion()
                    refrescarConflictos()
                }
                is ResultadoAccion.Error -> _estado.update { actual ->
                    actual.copy(asistente = actual.asistente?.copy(ocupado = false, error = resultado.mensaje))
                }
            }
        }
    }

    fun dejarDeSincronizar() {
        agregarDialogo(DialogoVista.Confirmacion(tipo = TipoConfirmacion.DEJAR_DE_SINCRONIZAR))
    }

    fun resolverConflicto(conflicto: ConflictoHuellaVista) {
        agregarDialogo(DialogoVista.ConflictoHuella(conflicto))
    }

    // -----------------------------------------------------------------------
    // Clave del móvil, versiones y ajustes
    // -----------------------------------------------------------------------

    fun refrescarClaveMovil() {
        alcance.launch {
            val clave = try {
                fuente.clavePublicaMovil()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                null
            }
            _estado.update { it.copy(claveMovil = clave) }
        }
    }

    fun cargarVersion() {
        alcance.launch {
            val version = try {
                fuente.version()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                null
            }
            if (version != null) _estado.update { it.copy(version = version) }
        }
    }

    /**
     * Crea una clave nueva del móvil cuando Android invalidó la anterior
     * (CA 31). El adaptador real olvida la clave del Keystore, crea otra y
     * limpia la caché del núcleo; aquí solo se avisa y se refresca la ficha.
     */
    fun crearClaveMovil() {
        if (_estado.value.creandoClave) return
        _estado.update { it.copy(creandoClave = true) }
        alcance.launch {
            val creada = try {
                fuente.crearClaveMovil()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                false
            }
            _estado.update { it.copy(creandoClave = false) }
            if (creada) {
                refrescarClaveMovil()
                avisoInterfaz(TipoAviso.CLAVE_MOVIL_CREADA)
            } else {
                avisoInterfaz(TipoAviso.CLAVE_MOVIL_NO_CREADA)
            }
        }
    }

    fun cambiarTamanoLetra(sp: Int) {
        val acotado = acotarZoom(sp)
        _estado.update { it.copy(tamanoLetra = acotado) }
        lanzarAccion(bloque = { fuente.cambiarTamanoLetra(acotado) })
    }

    fun instalarClave(hostUuid: String, hostNombre: String) {
        // Instalar la clave SSH no tiene sentido en un escritorio.
        val host = _estado.value.inventario.hosts.firstOrNull { it.uuid == hostUuid }
        if (host != null && host.tipo.esEscritorio) {
            avisoInterfaz(TipoAviso.ESCRITORIO_EN_EL_ORDENADOR, host.nombre)
            return
        }
        agregarDialogo(
            DialogoVista.Confirmacion(
                tipo = TipoConfirmacion.INSTALAR_CLAVE,
                detalle = hostNombre,
                hostUuid = hostUuid,
            ),
        )
    }

    fun pedirOlvidarContrasena(hostUuid: String, hostNombre: String) {
        agregarDialogo(
            DialogoVista.Confirmacion(
                tipo = TipoConfirmacion.OLVIDAR_CONTRASENA,
                detalle = hostNombre,
                hostUuid = hostUuid,
            ),
        )
    }

    // -----------------------------------------------------------------------
    // Túneles
    // -----------------------------------------------------------------------

    fun alternarTunel(tunel: TunelVista) {
        when (tunel.estado) {
            org.cuatrodetres.magi.ui.modelos.EstadoTunelVista.INACTIVO -> lanzarAccion(
                { fuente.activarTunel(tunel.uuid) },
                exito = { avisoInterfaz(TipoAviso.TUNEL_ACTIVADO) },
            )
            org.cuatrodetres.magi.ui.modelos.EstadoTunelVista.CAIDO -> lanzarAccion(
                { fuente.relanzarTunel(tunel.uuid) },
                exito = { avisoInterfaz(TipoAviso.TUNEL_ACTIVADO) },
            )
            org.cuatrodetres.magi.ui.modelos.EstadoTunelVista.ACTIVO -> lanzarAccion(
                { fuente.pararTunel(tunel.uuid) },
                exito = { avisoInterfaz(TipoAviso.TUNEL_PARADO) },
            )
            org.cuatrodetres.magi.ui.modelos.EstadoTunelVista.ACTIVANDO -> Unit
        }
        refrescarTuneles()
    }

    fun relanzarTunel(tunel: TunelVista) {
        lanzarAccion(
            { fuente.relanzarTunel(tunel.uuid) },
            exito = { avisoInterfaz(TipoAviso.TUNEL_ACTIVADO) },
        )
        refrescarTuneles()
    }

    /**
     * «DESCARTAR» de un túnel caído: el núcleo lo quita llamando a
     * parar_tunel(uuid), así deja de bloquear «dejar de sincronizar» (M1).
     */
    fun descartarTunel(tunel: TunelVista) {
        lanzarAccion(
            bloque = { fuente.pararTunel(tunel.uuid) },
            exito = { avisoInterfaz(TipoAviso.TUNEL_DESCARTADO) },
        )
        refrescarTuneles()
    }

    fun tunelEnPuertoLibre(tunel: TunelVista) {
        lanzarAccion(
            { fuente.activarEnPuertoLibre(tunel.uuid) },
            exito = { avisoInterfaz(TipoAviso.TUNEL_ACTIVADO) },
        )
        refrescarTuneles()
    }

    fun refrescarTuneles() {
        alcance.launch {
            val lista = try {
                fuente.tuneles(null)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                return@launch
            }
            _estado.update { it.copy(tuneles = lista) }
            actualizarTunelesFicha()
        }
    }

    private fun actualizarTunelesFicha() {
        _estado.update { actual ->
            val ficha = actual.detalle as? DetalleVista.Ficha ?: return@update actual
            actual.copy(tunelesFicha = actual.tuneles.filter { it.hostUuid == ficha.hostUuid })
        }
    }

    // -----------------------------------------------------------------------
    // Diálogos
    // -----------------------------------------------------------------------

    fun agregarDialogo(dialogo: DialogoVista) {
        _estado.update { actual ->
            val repetido = dialogo is DialogoVista.DeNucleo &&
                actual.dialogos.any {
                    it is DialogoVista.DeNucleo && it.peticion.peticionId == dialogo.peticion.peticionId
                }
            if (repetido) actual else actual.copy(dialogos = actual.dialogos + dialogo)
        }
    }

    fun quitarDialogo(dialogo: DialogoVista) {
        _estado.update { actual -> actual.copy(dialogos = actual.dialogos - dialogo) }
    }

    /** Cancelar un diálogo: las peticiones del núcleo se responden como canceladas. */
    fun cancelarDialogo(dialogo: DialogoVista) {
        quitarDialogo(dialogo)
        if (dialogo !is DialogoVista.DeNucleo) return
        val respuesta: RespuestaDialogoVista = when (dialogo.peticion) {
            is org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista.HuellaNueva -> RespuestaDialogoVista.CancelarHuella
            is org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista.HuellaCambiada -> RespuestaDialogoVista.CancelarHuella
            is org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista.Contrasena -> RespuestaDialogoVista.CancelarContrasena
        }
        responderAlNucleo(dialogo.peticion.peticionId, respuesta)
    }

    /** Respuesta del usuario a una petición del núcleo. */
    fun responderDialogo(dialogo: DialogoVista.DeNucleo, respuesta: RespuestaDialogoVista) {
        quitarDialogo(dialogo)
        responderAlNucleo(dialogo.peticion.peticionId, respuesta)
    }

    private fun responderAlNucleo(peticionId: Long, respuesta: RespuestaDialogoVista) {
        alcance.launch {
            try {
                val resultado = fuente.responderDialogo(peticionId, respuesta)
                if (resultado is ResultadoAccion.Error) avisoNucleo(resultado.mensaje, true)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty(), true)
            } finally {
                // La contraseña no se queda en memoria más de lo necesario (§8).
                if (respuesta is RespuestaDialogoVista.Contrasena) respuesta.bytes.fill(0)
            }
        }
    }

    /** Confirmación de una acción destructiva o delicada (T6). */
    fun confirmar(dialogo: DialogoVista.Confirmacion) {
        quitarDialogo(dialogo)
        when (dialogo.tipo) {
            TipoConfirmacion.CERRAR_SESION -> {
                val sesionId = dialogo.sesionId ?: return
                lanzarAccion(
                    { fuente.cerrarSesion(sesionId) },
                    exito = {
                        avisoInterfaz(TipoAviso.SESION_CERRADA)
                        _estado.update { actual ->
                            actual.copy(detalle = if (actual.sesionVisible == sesionId) null else actual.detalle)
                        }
                    },
                )
            }
            TipoConfirmacion.DEJAR_DE_SINCRONIZAR -> {
                lanzarAccion(
                    { fuente.dejarDeSincronizar() },
                    exito = {
                        avisoInterfaz(TipoAviso.SINCRONIZACION_DEJADA)
                        refrescarInventario()
                        refrescarSincronizacion()
                    },
                )
            }
            TipoConfirmacion.OLVIDAR_CONTRASENA -> {
                val hostUuid = dialogo.hostUuid ?: return
                lanzarAccion(
                    { fuente.olvidarContrasena(hostUuid) },
                    exito = {
                        avisoInterfaz(TipoAviso.CONTRASENA_OLVIDADA)
                        refrescarFicha(hostUuid)
                    },
                )
            }
            TipoConfirmacion.INSTALAR_CLAVE -> {
                val hostUuid = dialogo.hostUuid ?: return
                alcance.launch {
                    val resultado = try {
                        fuente.instalarClaveEnHost(hostUuid)
                    } catch (e: Exception) {
                        if (e is kotlinx.coroutines.CancellationException) throw e
                        ResultadoInstalacionVista.Error(e.message.orEmpty())
                    }
                    when (resultado) {
                        ResultadoInstalacionVista.YaEstaba ->
                            avisoInterfaz(TipoAviso.CLAVE_INSTALADA_YA_ESTABA)
                        ResultadoInstalacionVista.InstaladaAceptada ->
                            avisoInterfaz(TipoAviso.CLAVE_INSTALADA_ACEPTADA)
                        ResultadoInstalacionVista.InstaladaNoAceptada ->
                            avisoInterfaz(TipoAviso.CLAVE_INSTALADA_NO_ACEPTADA)
                        is ResultadoInstalacionVista.Error -> avisoNucleo(resultado.mensaje, true)
                    }
                    refrescarFicha(hostUuid)
                }
            }
            TipoConfirmacion.ACEPTAR_HUELLA_DE_EQUIPO -> {
                val conflictoId = dialogo.conflictoId ?: return
                lanzarAccion(
                    { fuente.aceptarHuellaDeOtroEquipo(conflictoId) },
                    exito = {
                        avisoInterfaz(TipoAviso.CONFLICTO_HUELLA_ACEPTADO)
                        refrescarConflictos()
                    },
                )
            }
            TipoConfirmacion.BORRAR_DATOS -> borrarTodo()
        }
    }

    /**
     * Borra todos los datos de MAGI (checklist de interfaz). La pantalla de
     * Ajustes ya exige escribir «borrar»; aquí solo se ejecuta y se limpia el
     * estado de la interfaz.
     */
    fun borrarTodo() {
        lanzarAccion(
            bloque = { fuente.borrarDatos(true) },
            exito = {
                _estado.update {
                    it.copy(
                        inventario = InventarioVista(),
                        sincronizacion = EstadoSincronizacionVista(),
                        sesiones = emptyList(),
                        tuneles = emptyList(),
                        conflictos = emptyList(),
                        detalle = null,
                        ficha = null,
                        tunelesFicha = emptyList(),
                        claveMovil = null,
                        filtro = "",
                    )
                }
                avisoInterfaz(TipoAviso.DATOS_BORRADOS)
            },
        )
    }

    /** Mantener la huella del móvil y descartar la del otro equipo. */
    fun mantenerMiHuella(conflicto: ConflictoHuellaVista) {
        lanzarAccion(
            { fuente.mantenerMiHuella(conflicto.id) },
            exito = { avisoInterfaz(TipoAviso.CONFLICTO_HUELLA_MANTENIDO) },
        )
        refrescarConflictos()
    }

    /** Aceptar la huella del otro equipo pide confirmación (sustituye la del móvil). */
    fun aceptarHuellaDeOtroEquipo(conflicto: ConflictoHuellaVista) {
        agregarDialogo(
            DialogoVista.Confirmacion(
                tipo = TipoConfirmacion.ACEPTAR_HUELLA_DE_EQUIPO,
                detalle = conflicto.hostNombre,
                conflictoId = conflicto.id,
                hostUuid = conflicto.hostUuid,
            ),
        )
    }

    fun refrescarFicha(hostUuid: String) {
        val detalle = _estado.value.detalle as? DetalleVista.Ficha ?: return
        if (detalle.hostUuid != hostUuid) return
        alcance.launch {
            val ficha = try {
                fuente.fichaHost(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                null
            } ?: return@launch
            _estado.update { it.copy(ficha = ficha) }
        }
    }

    // -----------------------------------------------------------------------
    // Avisos
    // -----------------------------------------------------------------------

    fun limpiarAviso(identificador: Long) {
        _estado.update { actual ->
            if (actual.aviso?.identificador == identificador) actual.copy(aviso = null) else actual
        }
    }

    fun avisoNucleo(mensaje: String, esError: Boolean) {
        if (mensaje.isBlank()) {
            avisoInterfaz(TipoAviso.ERROR_INESPERADO)
            return
        }
        val aviso = AvisoVista.DeNucleo(++contadorAvisos, mensaje, esError)
        _estado.update { it.copy(aviso = aviso) }
    }

    fun avisoInterfaz(tipo: TipoAviso, detalle: String? = null) {
        val aviso = AvisoVista.DeInterfaz(++contadorAvisos, tipo, detalle)
        _estado.update { it.copy(aviso = aviso) }
    }

    // -----------------------------------------------------------------------
    // Utilidades
    // -----------------------------------------------------------------------

    private fun lanzarAccion(bloque: suspend () -> ResultadoAccion, exito: () -> Unit = {}) {
        alcance.launch {
            val resultado = try {
                bloque()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            when (resultado) {
                ResultadoAccion.Ok -> exito()
                is ResultadoAccion.Error -> avisoNucleo(resultado.mensaje, true)
            }
        }
    }
}
