package org.cuatrodetres.magi.ui.servicios

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.logs.VistaModeloLogs
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.EstadoLogsVista
import org.cuatrodetres.magi.ui.modelos.EstadoSuscripcionLogVista
import org.cuatrodetres.magi.ui.modelos.FalloFlotaVista
import org.cuatrodetres.magi.ui.modelos.FilaTodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.HostTodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.LineaLogVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.MotivoFinLogVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.PropiedadVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccionServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.SeccionServiciosVista
import org.cuatrodetres.magi.ui.modelos.SystemdVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.modelos.TodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.UnidadVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pruebas de la lógica de Servicios y Logs sin red ni Android (JVM). */
class PruebaServiciosLogs {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun objetivo(nombre: String = "app.service") =
        ObjetivoVista(TipoObjetivoVista.UNIDAD, nombre)

    private fun listadoDePrueba() = ListadoServiciosVista(
        systemd = SystemdVista.Disponible("252"),
        unidades = listOf(
            UnidadVista(
                nombre = "app.service",
                propiedades = listOf(PropiedadVista("ActiveState", "active")),
                vigilada = true,
                acciones = listOf(AccionServicioVista.REINICIAR, AccionServicioVista.PARAR),
            ),
            UnidadVista(
                nombre = "caida.service",
                propiedades = listOf(PropiedadVista("ActiveState", "failed")),
                acciones = listOf(AccionServicioVista.ARRANCAR),
            ),
        ),
    )

    @Test
    fun iniciarCargaTodaLaFlotaYAbriendoHostCargaElListado() {
        val fuente = FuenteNucleoFalsa().apply {
            todaLaFlotaRespuesta = TodaLaFlotaVista(
                fallos = listOf(
                    FilaTodaLaFlotaVista("h1", "web-01", FalloFlotaVista(nombre = "app.service")),
                ),
                hosts = listOf(
                    HostTodaLaFlotaVista("h1", "web-01", estado = EstadoFlotaVista.CAIDA, fallos = 1),
                ),
            )
            listadoRespuesta = listadoDePrueba()
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.iniciar()
        assertTrue(fuente.llamadas.contains("serviciosTodaLaFlota"))
        assertEquals(1, vm.estado.value.todaLaFlota.numFallos)

        vm.abrirHost("h1", "web-01")
        assertTrue(fuente.llamadas.contains("serviciosListar:h1:false"))
        assertEquals("web-01", vm.estado.value.hostNombre)
        assertNotNull(vm.estado.value.listado)
        // Fallida primero y vigilada después, como en la F13.
        assertEquals(listOf("caida.service", "app.service"), vm.estado.value.unidades.map { it.nombre })
        assertEquals(SeccionServiciosVista.SERVICIOS, vm.estado.value.seccion)
    }

    @Test
    fun elFiltroYLasSeccionesCuentanLoQueSeVe() {
        val estado = EstadoServiciosUi(
            hostUuid = "h1",
            hostNombre = "web-01",
            listado = listadoDePrueba(),
            seccion = SeccionServiciosVista.SERVICIOS,
            filtro = "caida",
        )
        assertEquals(1, estado.unidades.size)
        assertEquals("caida.service", estado.unidades.first().nombre)
        assertEquals(1, estado.cuantasVisibles())
        // Sin systemd solo se ofrece Contenedores.
        val sinSystemd = listadoDePrueba().copy(systemd = SystemdVista.Ausente)
        assertEquals(
            listOf(SeccionServiciosVista.CONTENEDORES),
            sinSystemd.secciones,
        )
    }

    @Test
    fun unaAccionPermitidaPasaPorConfirmacionYEjecuta() {
        val fuente = FuenteNucleoFalsa().apply {
            listadoRespuesta = listadoDePrueba()
            infoAccionRespuesta = InfoAccionVista(permitida = true, comprobacion = "nginx -t")
            resultadoServicioRespuesta = ResultadoServicioVista(
                hostUuid = "h1",
                hostNombre = "web-01",
                objetivo = objetivo(),
                accion = AccionServicioVista.REINICIAR,
                resultado = ResultadoAccionServicioVista.HECHO,
            )
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.abrirHost("h1", "web-01")
        vm.pedirAccion(objetivo(), AccionServicioVista.REINICIAR)
        assertNotNull(vm.estado.value.confirmacion)
        assertEquals("nginx -t", vm.estado.value.confirmacion?.info?.comprobacion)

        vm.confirmarAccion()
        assertNull(vm.estado.value.confirmacion)
        assertTrue(fuente.accionesSolicitadas.isNotEmpty())
        assertEquals(AccionServicioVista.REINICIAR, fuente.accionesSolicitadas.last().third)
        assertEquals(
            ResultadoAccionServicioVista.HECHO,
            vm.estado.value.resultado?.resultado,
        )
    }

    @Test
    fun unaAccionNoPermitidaNoSeConfirmaYEnseñaElMotivo() {
        val fuente = FuenteNucleoFalsa().apply {
            listadoRespuesta = listadoDePrueba()
            infoAccionRespuesta = InfoAccionVista(
                permitida = false,
                motivoNoPermitida = "la unidad ya está parada",
            )
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.abrirHost("h1", "web-01")
        vm.pedirAccion(objetivo(), AccionServicioVista.PARAR)
        assertNull(vm.estado.value.confirmacion)
        assertEquals("la unidad ya está parada", vm.estado.value.aviso)
    }

    @Test
    fun unaAccionConSudoPideLaContrasenaAntesDeEjecutar() {
        val fuente = FuenteNucleoFalsa().apply {
            listadoRespuesta = listadoDePrueba()
            infoAccionRespuesta = InfoAccionVista(permitida = true, sudoConClave = true)
            resultadoSudoRespuesta = ResultadoSudoVista.OK
            resultadoServicioRespuesta = ResultadoServicioVista(
                hostUuid = "h1",
                hostNombre = "web-01",
                objetivo = objetivo(),
                accion = AccionServicioVista.REINICIAR,
                resultado = ResultadoAccionServicioVista.HECHO,
            )
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.abrirHost("h1", "web-01")
        vm.pedirAccion(objetivo(), AccionServicioVista.REINICIAR)
        vm.confirmarAccion()
        assertTrue(vm.estado.value.sudo.visible)
        vm.comprobarSudo("secreta".toByteArray())
        assertTrue(fuente.contrasenasSudo.isNotEmpty())
        assertTrue(fuente.accionesSolicitadas.isNotEmpty())
        assertTrue(vm.estado.value.listado?.sudoEnMemoria == true)
    }

    @Test
    fun unaCaidaDeConexionDejaHostsEnErrorYAnotaLaEjecucion() {
        // Prueba pura del estado de la pantalla: la acción en curso se ve en la
        // fila y el resultado que llega del oyente se enseña.
        val estado = EstadoServiciosUi(
            hostUuid = "h1",
            listado = listadoDePrueba(),
        )
        assertNull(estado.accionDe(objetivo()))
        assertEquals(0, estado.acciones.size)
    }

    @Test
    fun losLogsAbrenLaSuscripcionYPidenLaPrimeraVentana() {
        val fuente = FuenteNucleoFalsa().apply {
            idRespuesta = 7L
            ventanaLogsRespuesta = VentanaLogsVista(
                lineas = listOf(
                    LineaLogVista(1, "primera"),
                    LineaLogVista(2, "segunda"),
                ),
                total = 2,
                estado = EstadoSuscripcionLogVista.SIGUIENDO,
            )
        }
        val vm = VistaModeloLogs(fuente, ambito())
        vm.iniciar("h1", "web-01", objetivo())
        assertEquals(7L, vm.estado.value.suscripcion)
        assertEquals(2, vm.estado.value.lineas.size)
        assertEquals("primera", vm.estado.value.lineas.first().texto)
        assertEquals(2L, vm.estado.value.total)
        assertTrue(fuente.ventanasPedidas.isNotEmpty())
        assertNull(fuente.ventanasPedidas.first().second.desde)
    }

    @Test
    fun enPausaLasNuevasSeCuentanYAlFinalSeVuelveAlFinal() {
        val fuente = FuenteNucleoFalsa().apply {
            idRespuesta = 3L
            ventanaLogsRespuesta = VentanaLogsVista(
                lineas = listOf(LineaLogVista(1, "una")),
                total = 1,
            )
        }
        val vm = VistaModeloLogs(fuente, ambito())
        vm.iniciar("h1", "web-01", objetivo())
        vm.alternarPausa()
        assertTrue(vm.estado.value.pausada)
        fuente.emitirAvisoLogs(
            EstadoLogsVista(
                suscripcion = 3L,
                total = 4,
                nuevas = 3,
                estado = EstadoSuscripcionLogVista.SIGUIENDO,
            ),
        )
        assertEquals(3, vm.estado.value.nuevas)
        assertEquals(4, vm.estado.value.total)
        vm.alFinal()
        assertEquals(0, vm.estado.value.nuevas)
        assertTrue(!vm.estado.value.pausada)
    }

    @Test
    fun pararLosLogsCierraElCanalYDejaElMotivo() {
        val fuente = FuenteNucleoFalsa().apply { idRespuesta = 9L }
        val vm = VistaModeloLogs(fuente, ambito())
        vm.iniciar("h1", "web-01", objetivo())
        vm.parar()
        assertTrue(fuente.llamadas.contains("logsParar:9"))
        assertEquals(EstadoSuscripcionLogVista.PARADA, vm.estado.value.estado)
        assertEquals(MotivoFinLogVista.CERRADO, vm.estado.value.motivo)
    }
}
