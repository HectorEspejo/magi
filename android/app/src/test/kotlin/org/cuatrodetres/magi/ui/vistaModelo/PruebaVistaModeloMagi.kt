package org.cuatrodetres.magi.ui.vistaModelo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.navegacion.DetalleVista
import org.cuatrodetres.magi.navegacion.SeccionAjustes
import org.cuatrodetres.magi.navegacion.SeccionMas
import org.cuatrodetres.magi.ui.dobles.AvisosPlataformaFalso
import org.cuatrodetres.magi.ui.dobles.ControlBloqueoPrueba
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.DialogoVista
import org.cuatrodetres.magi.ui.modelos.EstadoBloqueoVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.GrupoVista
import org.cuatrodetres.magi.ui.modelos.HostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.HostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.PasoAsistente
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.modelos.TipoConfirmacion
import org.cuatrodetres.magi.ui.modelos.TipoHostVista
import org.cuatrodetres.magi.ui.modelos.TrabajoPerdidoVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel de la app con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloMagi {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun host(uuid: String = "h1", nombre: String = "web-01", grupo: String? = "g1") = HostVista(
        uuid = uuid,
        nombre = nombre,
        direccion = "192.0.2.10",
        puerto = 22,
        usuario = "usuario",
        grupoUuid = grupo,
        estado = EstadoHostVista.SIN_SESION,
    )

    private fun sesion(id: Long = 1, estado: EstadoSesionVista = EstadoSesionVista.ABIERTA) =
        SesionVista(id = id, hostUuid = "h1", hostNombre = "web-01", estado = estado)

    private fun inventarioDePrueba() = InventarioVista(
        grupos = listOf(GrupoVista("g1", "producción", 1)),
        hosts = listOf(host()),
    )

    @Test
    fun alIniciarCargaInventarioSincronizacionYVersiones() {
        val fuente = FuenteNucleoFalsa().apply {
            inventarioRespuesta = inventarioDePrueba()
            emitirSincronizacion(
                EstadoSincronizacionVista(fase = FaseSincronizacionVista.ACTIVA, conflictosHuella = 1),
            )
            conflictosRespuesta = listOf(conflicto())
            claveRespuesta = null
        }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        assertTrue(vm.estado.value.iniciado)
        assertEquals(1, vm.estado.value.inventario.hosts.size)
        assertEquals(FaseSincronizacionVista.ACTIVA, vm.estado.value.sincronizacion.fase)
        assertEquals(1, vm.estado.value.conflictos.size)
        assertNotNull(vm.estado.value.version)
        assertEquals(1, fuente.vecesIniciado)
    }

    @Test
    fun elFiltroSePideAlNucleo() {
        val fuente = FuenteNucleoFalsa().apply { inventarioRespuesta = inventarioDePrueba() }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.cambiarFiltro("web")
        assertEquals("web", fuente.ultimoFiltro)
        assertEquals("web", vm.estado.value.filtro)
    }

    @Test
    fun plegarUnGrupoEsOptimistaYSeGuarda() {
        val fuente = FuenteNucleoFalsa().apply { inventarioRespuesta = inventarioDePrueba() }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.plegarGrupo("g1")
        assertTrue(vm.estado.value.inventario.grupos[0].plegado)
        assertTrue(fuente.llamadas.contains("plegar:g1:true"))
    }

    @Test
    fun abrirFichaCargaLaFichaYSusTuneles() {
        val fuente = FuenteNucleoFalsa().apply {
            inventarioRespuesta = inventarioDePrueba()
            fichas["h1"] = FichaHostVista(
                uuid = "h1",
                nombre = "web-01",
                direccion = "192.0.2.10",
                puerto = 22,
                usuario = "usuario",
                tieneContrasena = true,
            )
            tunelesRespuesta = listOf(tunel())
        }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.abrirFicha("h1")
        assertTrue(vm.estado.value.detalle is DetalleVista.Ficha)
        assertEquals("web-01", vm.estado.value.ficha?.nombre)
        assertEquals(1, vm.estado.value.tunelesFicha.size)
    }

    @Test
    fun abrirSesionAbreElTerminal() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.abrirSesion("h1")
        assertEquals(DetalleVista.Terminal(1L), vm.estado.value.detalle)
        assertEquals(1L, vm.estado.value.sesionVisible)
    }

    @Test
    fun unEscritorioNoSeAbreDesdeElMovil() {
        val fuente = FuenteNucleoFalsa().apply {
            inventarioRespuesta = InventarioVista(
                grupos = emptyList(),
                hosts = listOf(
                    host(uuid = "h1", nombre = "web-01"),
                    host(uuid = "w1", nombre = "win-01").copy(tipo = TipoHostVista.RDP),
                ),
            )
        }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.abrirSesion("w1")
        // No se llama al núcleo: el escritorio solo se abre en el ordenador.
        assertTrue(fuente.llamadas.none { it.startsWith("abrirSesion:") })
        assertNull(vm.estado.value.detalle)
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is AvisoVista.DeInterfaz)
        assertEquals(
            TipoAviso.ESCRITORIO_EN_EL_ORDENADOR,
            (aviso as AvisoVista.DeInterfaz).tipo,
        )
    }

    @Test
    fun unEscritorioNoInstalaLaClaveDelMovil() {
        val fuente = FuenteNucleoFalsa().apply {
            inventarioRespuesta = InventarioVista(
                hosts = listOf(host(uuid = "v1", nombre = "vm-01").copy(tipo = TipoHostVista.VNC)),
            )
        }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.instalarClave("v1", "vm-01")
        // Ni diálogo de confirmación ni llamada al núcleo.
        assertNull(vm.estado.value.dialogoActual)
        assertTrue(fuente.llamadas.none { it.startsWith("instalarClave") })
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is AvisoVista.DeInterfaz)
        assertEquals(
            TipoAviso.ESCRITORIO_EN_EL_ORDENADOR,
            (aviso as AvisoVista.DeInterfaz).tipo,
        )
    }

    @Test
    fun lasSesionesDelNucleoLleganAlEstadoYSeNumeran() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        fuente.emitirSesiones(listOf(sesion(1), sesion(2)))
        assertEquals(2, vm.estado.value.sesiones.size)
        assertEquals("web-01 (2)", vm.estado.value.sesiones[1].titulo)
    }

    @Test
    fun responderUnaHuellaQuitaElDialogoYRespondeAlNucleo() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        fuente.emitirPeticion(
            PeticionDialogoVista.HuellaNueva(
                peticionId = 7,
                hostUuid = "h1",
                hostNombre = "web-01",
                tipo = "ed25519",
                sha256 = "SHA256:abc",
            ),
        )
        val dialogo = vm.estado.value.dialogoActual
        assertTrue(dialogo is DialogoVista.DeNucleo)
        vm.responderDialogo(dialogo as DialogoVista.DeNucleo, RespuestaDialogoVista.AceptarHuella)
        assertNull(vm.estado.value.dialogoActual)
        assertEquals(7L, fuente.respuestasDialogo[0].first)
    }

    @Test
    fun cerrarSesionPideConfirmacionYLaCierra() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        fuente.emitirSesiones(listOf(sesion()))
        vm.abrirTerminal(1L)
        vm.cerrarSesion(1L)
        val dialogo = vm.estado.value.dialogoActual as DialogoVista.Confirmacion
        assertEquals(TipoConfirmacion.CERRAR_SESION, dialogo.tipo)
        vm.confirmar(dialogo)
        assertTrue(fuente.llamadas.contains("cerrarSesion:1"))
        assertNull(vm.estado.value.detalle)
    }

    @Test
    fun elBloqueoDecideSiSeEntraYDesbloquearArranca() {
        val fuente = FuenteNucleoFalsa()
        val control = ControlBloqueoPrueba(bloquearAlArrancar = true)
        val vm = VistaModeloMagi(fuente, control, ambito())
        assertEquals(EstadoBloqueoVista.BLOQUEADA, vm.estado.value.bloqueo)
        vm.desbloquear()
        assertEquals(EstadoBloqueoVista.DESBLOQUEADA, vm.estado.value.bloqueo)
        assertEquals(1, fuente.vecesIniciado)
    }

    @Test
    fun sinBloqueoDePantallaNoSeArranca() {
        val vm = VistaModeloMagi(
            FuenteNucleoFalsa(),
            ControlBloqueoPrueba(hayBloqueo = false),
            ambito(),
        )
        assertEquals(EstadoBloqueoVista.SIN_BLOQUEO_DE_PANTALLA, vm.estado.value.bloqueo)
    }

    @Test
    fun elAsistenteEligeCarpetaYUneConClave() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.abrirAsistente()
        assertNotNull(vm.estado.value.asistente)
        vm.carpetaElegida("content://carpeta/magi", "magi")
        assertEquals("content://carpeta/magi", fuente.carpetaElegida)
        assertEquals(PasoAsistente.UNIR_CLAVE, vm.estado.value.asistente?.paso)
        vm.unirConClave("AGE-SECRET-KEY-1")
        assertEquals("AGE-SECRET-KEY-1", fuente.claveUnida)
        assertNull(vm.estado.value.asistente)
    }

    @Test
    fun instalarClavePideConfirmacionYUsaElNucleo() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.instalarClave("h1", "web-01")
        val dialogo = vm.estado.value.dialogoActual as DialogoVista.Confirmacion
        assertEquals(TipoConfirmacion.INSTALAR_CLAVE, dialogo.tipo)
        vm.confirmar(dialogo)
        assertTrue(fuente.llamadas.contains("instalarClave:h1"))
    }

    @Test
    fun borrarTodoLimpiaElEstado() {
        val fuente = FuenteNucleoFalsa().apply { inventarioRespuesta = inventarioDePrueba() }
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.borrarTodo()
        assertTrue(fuente.llamadas.contains("borrarDatos:true"))
        assertTrue(vm.estado.value.inventario.vacio)
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is org.cuatrodetres.magi.ui.modelos.AvisoVista.DeInterfaz)
        assertEquals(TipoAviso.DATOS_BORRADOS, (aviso as org.cuatrodetres.magi.ui.modelos.AvisoVista.DeInterfaz).tipo)
    }

    @Test
    fun avisaDeLasSesionesQueAndroidDejoAbiertas() {
        val fuente = FuenteNucleoFalsa()
        val avisos = org.cuatrodetres.magi.ui.dobles.AvisosPlataformaFalso(sesionesPerdidas = 2)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        assertEquals(2, vm.estado.value.sesionesPerdidas)
        vm.descartarSesionesPerdidas()
        assertNull(vm.estado.value.sesionesPerdidas)
    }

    @Test
    fun noAvisaDeSesionesPerdidasSiElNucleoTieneSesionesVivas() {
        val fuente = FuenteNucleoFalsa()
        // Las sesiones llegan antes de arrancar el núcleo (p. ej. al volver sin
        // que Android haya matado el proceso): no hay nada perdido que avisar.
        fuente.emitirSesiones(listOf(sesion()))
        val avisos = org.cuatrodetres.magi.ui.dobles.AvisosPlataformaFalso(sesionesPerdidas = 3)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        assertNull(vm.estado.value.sesionesPerdidas)
    }

    @Test
    fun ofreceLaGuiaDeBateriaConLaPrimeraSesionYLaMarca() {
        val fuente = FuenteNucleoFalsa()
        val avisos = org.cuatrodetres.magi.ui.dobles.AvisosPlataformaFalso(ofrecerGuiaBateria = true)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        fuente.emitirSesiones(listOf(sesion()))
        assertEquals(DialogoVista.GuiaBateria, vm.estado.value.dialogoActual)
        vm.abrirGuiaBateria()
        assertTrue(avisos.guiaMarcada)
        assertNull(vm.estado.value.dialogoActual)
        assertTrue(vm.estado.value.detalle is DetalleVista.Ajustes)
    }

    @Test
    fun ahoraNoTambienMarcaLaGuiaComoVista() {
        val fuente = FuenteNucleoFalsa()
        val avisos = org.cuatrodetres.magi.ui.dobles.AvisosPlataformaFalso(ofrecerGuiaBateria = true)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        fuente.emitirSesiones(listOf(sesion()))
        vm.descartarGuiaBateria()
        assertTrue(avisos.guiaMarcada)
        assertNull(vm.estado.value.dialogoActual)
    }

    @Test
    fun descartarUnTunelCaidoLlamaAPararTunel() {
        val fuente = FuenteNucleoFalsa()
        val caido = tunel().copy(estado = org.cuatrodetres.magi.ui.modelos.EstadoTunelVista.CAIDO)
        fuente.tunelesRespuesta = listOf(caido)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.descartarTunel(caido)
        assertTrue(fuente.llamadas.contains("pararTunel:t1"))
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is AvisoVista.DeInterfaz)
        assertEquals(TipoAviso.TUNEL_DESCARTADO, (aviso as AvisoVista.DeInterfaz).tipo)
    }

    @Test
    fun lasPeticionesApiladasNoSeConflatan() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        fuente.emitirPeticion(
            PeticionDialogoVista.HuellaNueva(1, "h1", "web-01", "ed25519", "SHA256:a"),
        )
        fuente.emitirPeticion(
            PeticionDialogoVista.HuellaNueva(2, "h2", "db-01", "ed25519", "SHA256:b"),
        )
        assertEquals(
            2,
            vm.estado.value.dialogos.count { it is DialogoVista.DeNucleo },
        )
    }

    @Test
    fun crearClaveMovilAvisaYRefrescaLaClaveInvalidada() {
        val fuente = FuenteNucleoFalsa()
        fuente.claveRespuesta = ClaveMovilVista(
            lineaOpenssh = "ecdsa-sha2-nistp256 AAAA vieja",
            huellaSha256 = "SHA256:vieja",
            invalidada = true,
        )
        fuente.claveTrasCrear = ClaveMovilVista(
            lineaOpenssh = "ecdsa-sha2-nistp256 BBBB nueva",
            huellaSha256 = "SHA256:nueva",
            invalidada = false,
        )
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        assertEquals(true, vm.estado.value.claveMovil?.invalidada)
        vm.crearClaveMovil()
        assertEquals(1, fuente.vecesCrearClave)
        assertEquals(false, vm.estado.value.claveMovil?.invalidada)
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is AvisoVista.DeInterfaz)
        assertEquals(TipoAviso.CLAVE_MOVIL_CREADA, (aviso as AvisoVista.DeInterfaz).tipo)
        assertFalse(vm.estado.value.creandoClave)
    }

    @Test
    fun crearClaveMovilAvisaSiNoSePuedeCrear() {
        val fuente = FuenteNucleoFalsa()
        fuente.claveRespuesta = ClaveMovilVista(
            lineaOpenssh = "ecdsa-sha2-nistp256 AAAA vieja",
            huellaSha256 = "SHA256:vieja",
            invalidada = true,
        )
        fuente.claveCreada = false
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.crearClaveMovil()
        assertEquals(1, fuente.vecesCrearClave)
        val aviso = vm.estado.value.aviso
        assertTrue(aviso is AvisoVista.DeInterfaz)
        assertEquals(TipoAviso.CLAVE_MOVIL_NO_CREADA, (aviso as AvisoVista.DeInterfaz).tipo)
    }

    // -----------------------------------------------------------------------
    // Fase 14: ajustes, bloqueo y trabajo que Android cerró con MAGI
    // -----------------------------------------------------------------------

    @Test
    fun alArrancarAplicaLosAjustesGuardados() {
        val fuente = FuenteNucleoFalsa()
        val guardados = AjustesMovilVista(flotaRefrescoSeg = 60, umbralMemoriaPct = 70.0)
        val avisos = AvisosPlataformaFalso(ajustesGuardados = guardados)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        assertEquals(guardados, fuente.ajustesAplicados)
        assertEquals(guardados, vm.estado.value.ajustesMovil)
        assertEquals(1, avisos.vecesAjustesLeidos)
    }

    @Test
    fun guardarAjustesLosAcotaLosPersisteYLosAplicaLosNucleo() {
        val fuente = FuenteNucleoFalsa()
        val avisos = AvisosPlataformaFalso()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        vm.guardarAjustesMovil(
            AjustesMovilVista(
                umbralCargaPorNucleo = 99.0,
                serviciosContenedores = listOf("rkt"),
            ),
        )
        assertEquals(16.0, avisos.ajustesGuardados.umbralCargaPorNucleo, 0.0001)
        assertEquals(emptyList<String>(), avisos.ajustesGuardados.serviciosContenedores)
        assertEquals(avisos.ajustesGuardados, fuente.ajustesAplicados)
        assertTrue(vm.estado.value.avisosAjustes.isNotEmpty())
    }

    @Test
    fun alVolverSeVuelvenAAplicarLosAjustesGuardados() {
        val fuente = FuenteNucleoFalsa()
        val avisos = AvisosPlataformaFalso()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        val nuevos = AjustesMovilVista(sudoMemoriaMin = 10)
        avisos.ajustesGuardados = nuevos
        fuente.ajustesAplicados = null
        vm.alVolverALaApp()
        assertEquals(nuevos, fuente.ajustesAplicados)
        assertEquals(nuevos, vm.estado.value.ajustesMovil)
    }

    @Test
    fun alBloquearseMagiOlvidaElSudoEnMemoria() {
        val fuente = FuenteNucleoFalsa()
        val control = ControlBloqueoPrueba(bloquearAlVolver = true)
        val vm = VistaModeloMagi(fuente, control, ambito())
        vm.iniciar()
        vm.alVolverALaApp()
        assertEquals(EstadoBloqueoVista.BLOQUEADA, vm.estado.value.bloqueo)
        assertTrue(fuente.llamadas.contains("olvidarSudoAlBloquear"))
    }

    @Test
    fun avisaDelTrabajoQueAndroidCerroConMagi() {
        val fuente = FuenteNucleoFalsa()
        val avisos = AvisosPlataformaFalso(accionesPerdidas = 1, ejecucionesPerdidas = 2)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        assertEquals(TrabajoPerdidoVista(acciones = 1, ejecuciones = 2), vm.estado.value.trabajoPerdido)
        vm.descartarTrabajoPerdido()
        assertNull(vm.estado.value.trabajoPerdido)
    }

    @Test
    fun noAvisaDeTrabajoSiElNucleoTieneAccionesVivas() {
        val fuente = FuenteNucleoFalsa()
        fuente.emitirAccionesServicio(listOf(AccionEnCursoVista(hostNombre = "web-01")))
        val avisos = AvisosPlataformaFalso(accionesPerdidas = 1)
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        assertNull(vm.estado.value.trabajoPerdido)
    }

    @Test
    fun persisteElTrabajoVivoParaElProximoArranque() {
        val fuente = FuenteNucleoFalsa()
        val avisos = AvisosPlataformaFalso()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito(), avisos)
        vm.iniciar()
        // Una ejecución con un host en cola: la lista conserva 1 h las
        // terminadas, pero solo las vivas cuentan para el aviso de §7.12.
        fuente.emitirEjecuciones(
            listOf(
                EjecucionVista(
                    id = 1,
                    hosts = listOf(
                        HostEjecucionVista(hostUuid = "h1", hostNombre = "web-01"),
                    ),
                ),
            ),
        )
        assertEquals(Triple(0, 0, 1), avisos.trabajoGuardado)
    }

    @Test
    fun losServiciosYSnippetsSeAbrenDesdeLaFichaDeFlota() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloMagi(fuente, ControlBloqueoPrueba(), ambito())
        vm.iniciar()
        vm.abrirServiciosDeHost("h1")
        assertEquals(Destino.SERVICIOS, vm.estado.value.destino)
        assertEquals("h1", vm.estado.value.hostServiciosInicial)
        vm.abrirSnippetsDeHost("h1")
        assertEquals(Destino.MAS, vm.estado.value.destino)
        assertEquals(SeccionMas.SNIPPETS, vm.estado.value.seccionMas)
        assertEquals("h1", vm.estado.value.hostSnippetsInicial)
        assertNull(vm.estado.value.hostServiciosInicial)
        vm.volver()
        assertNull(vm.estado.value.hostSnippetsInicial)
    }

    private fun conflicto() = ConflictoHuellaVista(
        id = 3,
        hostUuid = "h1",
        hostNombre = "web-01",
        direccion = "192.0.2.10:22",
        huellaMia = "SHA256:aaa",
        huellaOtra = "SHA256:bbb",
    )

    private fun tunel() = TunelVista(
        uuid = "t1",
        hostUuid = "h1",
        hostNombre = "web-01",
        nombre = "grafana",
        tipo = org.cuatrodetres.magi.ui.modelos.TipoTunelVista.LOCAL,
        escucha = "127.0.0.1:3000",
        destino = "127.0.0.1:3000",
    )
}
