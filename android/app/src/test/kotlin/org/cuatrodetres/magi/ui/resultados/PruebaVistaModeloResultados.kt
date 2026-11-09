package org.cuatrodetres.magi.ui.resultados

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.HostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoEjecucionVista
import org.cuatrodetres.magi.ui.modelos.SalidaEjecucionVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel de Resultados con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloResultados {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun ejecucion(id: Long, terminada: Boolean, forzada: Boolean = false) = EjecucionVista(
        id = id,
        snippetNombre = "reiniciar nginx",
        hosts = listOf(
            HostEjecucionVista(
                hostUuid = "h1",
                hostNombre = "web-01",
                estado = if (terminada) EstadoHostEjecucionVista.OK else EstadoHostEjecucionVista.EJECUTANDO,
                codigo = if (terminada) 0 else null,
                duracionMs = if (terminada) 1_200 else 0,
                bytesStdout = if (terminada) 212 else 0,
            ),
        ),
        resultado = if (terminada) ResultadoEjecucionVista.OK else null,
        creadaEn = 1_794_222_000,
        forzada = forzada,
    )

    @Test
    fun iniciaYSeMantieneAlDiaConElFlujo() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(1, terminada = true))
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        assertEquals(1, vm.estado.value.ejecuciones.size)
        assertFalse(vm.estado.value.cargando)
        fuente.emitirEjecuciones(listOf(ejecucion(1, true), ejecucion(2, false)))
        assertEquals(2, vm.estado.value.ejecuciones.size)
        assertEquals(1, vm.estado.value.enCurso)
    }

    @Test
    fun abrirUnHostPideSuSalidaYCambiaDePestana() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(1, terminada = true))
            salidaRespuesta = SalidaEjecucionVista(
                hostUuid = "h1",
                hostNombre = "web-01",
                codigo = 0,
                stdout = "hola",
                stderr = "aviso",
            )
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        vm.seleccionar(1)
        vm.seleccionarHost("h1")
        assertTrue(fuente.llamadas.contains("salidaEjecucion:1:h1"))
        assertEquals("hola", vm.estado.value.textoSalida)
        vm.cambiarPestana(PestanaSalida.STDERR)
        assertEquals("aviso", vm.estado.value.textoSalida)
        assertNotNull(vm.estado.value.salida)
    }

    @Test
    fun repetirSinDeliberacionLanzaLaEjecucion() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(7, terminada = true))
            preparacionRespuesta = PreparacionEjecucionVista(
                snippetUuid = "s1",
                snippetNombre = "reiniciar nginx",
                comando = "uptime",
                hosts = listOf(HostDestinoVista("h1", "web-01")),
                pararAlFallo = true,
            )
            idRespuesta = 21
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        vm.repetir(7)
        assertTrue(fuente.llamadas.contains("repetirEjecucion:7"))
        assertTrue(fuente.llamadas.contains("ejecutarSnippet:s1"))
        assertEquals(21L, vm.estado.value.seleccionadaId)
        assertEquals(AvisoResultados.REPETIDA, vm.estado.value.aviso)
    }

    @Test
    fun repetirConDeliberacionAbreElDialogoYEspera() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(7, terminada = true))
            preparacionRespuesta = PreparacionEjecucionVista(
                snippetUuid = "s1",
                snippetNombre = "reiniciar nginx",
                comando = "uptime",
                hosts = listOf(HostDestinoVista("h1", "web-01")),
                delibera = true,
            )
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        vm.repetir(7)
        assertNotNull(vm.estado.value.planDeliberacion)
        assertFalse(fuente.llamadas.contains("ejecutarSnippet:s1"))
        vm.ejecutarDeliberado(vm.estado.value.planDeliberacion!!, 5)
        assertTrue(fuente.llamadas.contains("ejecutarSnippet:s1"))
        assertNull(vm.estado.value.planDeliberacion)
    }

    @Test
    fun cancelarPideConfirmacionYAnotaLaCancelacion() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(3, terminada = false))
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        vm.pedirCancelar(3)
        assertEquals(3L, vm.estado.value.cancelacionPendiente)
        vm.cancelarConfirmacion()
        assertNull(vm.estado.value.cancelacionPendiente)
        vm.pedirCancelar(3)
        vm.confirmarCancelar()
        assertNull(vm.estado.value.cancelacionPendiente)
        assertTrue(fuente.llamadas.contains("cancelarEjecucion:3"))
    }

    @Test
    fun limpiarAvisaYOlvidaLaSeleccion() {
        val fuente = FuenteNucleoFalsa().apply {
            ejecucionesRespuesta = listOf(ejecucion(1, terminada = true))
        }
        val vm = VistaModeloResultados(fuente, ambito())
        vm.iniciar()
        vm.seleccionar(1)
        vm.limpiar()
        assertTrue(fuente.llamadas.contains("limpiarEjecuciones"))
        assertEquals(AvisoResultados.LIMPIADAS, vm.estado.value.aviso)
    }

    @Test
    fun laHoraCortaDeUnaEjecucionSinFechaEsUnGuion() {
        assertEquals("—", horaCorta(0))
        assertTrue(horaCorta(1_794_222_000).contains(":"))
    }
}
