package org.cuatrodetres.magi.ui.servicios

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.flota.VistaModeloFlota
import org.cuatrodetres.magi.ui.logs.VistaModeloLogs
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.EstadoSuscripcionLogVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.LineaLogVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.SystemdVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Regresiones de la revisión adversarial de la Fase 14 (T31/T54) sobre la app
 * Android, sin red ni Android. Cubren los fallos corregidos:
 * - el id de deliberación no se pierde al pedir la contraseña de sudo;
 * - una acción que exige deliberación no se ejecuta sin su id;
 * - una ficha en vuelo no repuebla un panel ya cerrado;
 * - los logs se paran al pasar a segundo plano y SEGUIR OTRA VEZ los reabre.
 */
class PruebaRegresionRevisionAdversarial {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun objetivo() = ObjetivoVista(TipoObjetivoVista.UNIDAD, "app.service")

    private fun fuenteDeServicios() = FuenteNucleoFalsa().apply {
        listadoRespuesta = ListadoServiciosVista(systemd = SystemdVista.Disponible("252"))
    }

    @Test
    fun laDeliberacionNoSePierdeCuandoLaAccionTambienPideSudo() {
        val fuente = fuenteDeServicios().apply {
            infoAccionRespuesta = InfoAccionVista(
                permitida = true,
                delibera = true,
                sudoConClave = true,
            )
            resultadoSudoRespuesta = ResultadoSudoVista.OK
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.abrirHost("h1", "web-01")
        vm.pedirAccion(objetivo(), AccionServicioVista.REINICIAR)
        vm.confirmarAccion(42L)

        // La acción deliberada pide antes la contraseña de sudo.
        assertTrue(vm.estado.value.sudo.visible)
        assertTrue(fuente.accionesSolicitadas.isEmpty())

        vm.comprobarSudo("secreta".toByteArray())

        // Al volver del sudo se ejecuta con el id de deliberación guardado.
        assertEquals(listOf(42L), fuente.deliberacionesSolicitadas)
        assertEquals(1, fuente.accionesSolicitadas.size)
        assertNotNull(vm.estado.value.resultado)
    }

    @Test
    fun unaAccionQueExigeDeliberacionNoSeEjecutaSinSuId() {
        val fuente = fuenteDeServicios().apply {
            infoAccionRespuesta = InfoAccionVista(permitida = true, delibera = true)
        }
        val vm = VistaModeloServicios(fuente, ambito = ambito())
        vm.abrirHost("h1", "web-01")
        vm.pedirAccion(objetivo(), AccionServicioVista.PARAR)
        vm.confirmarAccion()

        assertTrue(fuente.accionesSolicitadas.isEmpty())
        assertNotNull(vm.estado.value.confirmacion)
        assertNotNull(vm.estado.value.aviso)
    }

    @Test
    fun unaFichaEnVueloNoResucitaLaSeleccionCerrada() {
        val fuente = FuenteNucleoFalsa().apply {
            fichas["h1"] = FichaHostVista(
                uuid = "h1",
                nombre = "web-01",
                direccion = "192.0.2.10",
                puerto = 22,
                usuario = "usuario",
            )
        }
        val puerta = CompletableDeferred<Unit>()
        fuente.puertaFichaHost = puerta
        val vm = VistaModeloFlota(fuente, ambito = ambito())

        vm.abrirFicha("h1")
        vm.cerrarFicha()
        puerta.complete(Unit)

        assertNull(vm.estado.value.hostSeleccionado)
        assertNull(vm.estado.value.ficha)
    }

    @Test
    fun losLogsSeParanAlPasarASegundoPlanoYSeReanudan() {
        val fuente = FuenteNucleoFalsa().apply {
            idRespuesta = 5L
            ventanaLogsRespuesta = VentanaLogsVista(
                lineas = listOf(LineaLogVista(1, "una")),
                total = 1,
                estado = EstadoSuscripcionLogVista.SIGUIENDO,
            )
        }
        val vm = VistaModeloLogs(fuente, ambito())
        vm.iniciar("h1", "web-01", objetivo())
        assertEquals(5L, vm.estado.value.suscripcion)

        vm.alPasarASegundoPlano()
        assertTrue(fuente.llamadas.contains("logsParar:5"))
        assertEquals(EstadoSuscripcionLogVista.PARADA, vm.estado.value.estado)
        // Las líneas se quedan mientras está parada.
        assertEquals(1, vm.estado.value.lineas.size)

        fuente.idRespuesta = 6L
        fuente.ventanaLogsRespuesta = VentanaLogsVista(
            lineas = listOf(LineaLogVista(10, "nueva")),
            total = 1,
            estado = EstadoSuscripcionLogVista.SIGUIENDO,
        )
        vm.reanudar()
        assertEquals(6L, vm.estado.value.suscripcion)
        assertEquals(listOf("nueva"), vm.estado.value.lineas.map { it.texto })
        assertFalse(vm.estado.value.pausada)
    }
}
