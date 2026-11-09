package org.cuatrodetres.magi.ui.dialogos

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.EstadoDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel del diálogo MAGI con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloDeliberacion {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun plan() = PlanDeliberacionVista.Snippet(
        PreparacionEjecucionVista(
            snippetUuid = "s1",
            snippetNombre = "reiniciar nginx",
            comando = "systemctl restart nginx",
            hosts = listOf(HostDestinoVista("h1", "web-01")),
        ),
    )

    private fun tabla() = TablaDeliberacionVista(
        id = 9,
        aprobadas = 2,
        activas = 3,
        rechazos = 1,
        estado = EstadoDeliberacionVista.BLOQUEADA,
    )

    @Test
    fun abrirPideLaTablaAlNucleo() {
        val fuente = FuenteNucleoFalsa().apply {
            tablaDeliberacionRespuesta = tabla()
        }
        val vm = VistaModeloDeliberacion(fuente, ambito())
        vm.abrir(plan())
        assertFalse(vm.estado.value.cargando)
        assertEquals(9L, vm.estado.value.tabla?.id)
        assertEquals(EstadoDeliberacionVista.BLOQUEADA, vm.estado.value.tabla?.estado)
        assertEquals(1, fuente.deliberacionesPedidas.size)
        assertEquals("reiniciar nginx → web-01", fuente.deliberacionesPedidas[0].accion)
    }

    @Test
    fun resolverEntregaElIdDeLaDeliberacion() {
        val fuente = FuenteNucleoFalsa().apply {
            tablaDeliberacionRespuesta = tabla()
            idRespuesta = 33
        }
        val vm = VistaModeloDeliberacion(fuente, ambito())
        vm.abrir(plan())
        var resuelto: Long? = null
        vm.resolver(DecisionDeliberacionVista.Aprobar) { resuelto = it }
        assertEquals(33L, resuelto)
        assertTrue(
            fuente.decisionesDeliberacion.contains(9L to DecisionDeliberacionVista.Aprobar),
        )
    }

    @Test
    fun forzarLlevaElMotivoAlNucleo() {
        val fuente = FuenteNucleoFalsa().apply {
            tablaDeliberacionRespuesta = tabla()
            idRespuesta = 44
        }
        val vm = VistaModeloDeliberacion(fuente, ambito())
        vm.abrir(plan())
        val decision = DecisionDeliberacionVista.Forzar("tests en verde en CI esta mañana")
        vm.resolver(decision) { }
        assertTrue(fuente.decisionesDeliberacion.contains(9L to decision))
    }

    @Test
    fun cancelarAnotaLaCancelacionYCierra() {
        val fuente = FuenteNucleoFalsa().apply {
            tablaDeliberacionRespuesta = tabla()
        }
        val vm = VistaModeloDeliberacion(fuente, ambito())
        vm.abrir(plan())
        var cancelado = false
        vm.cancelar { cancelado = true }
        assertTrue(cancelado)
        assertTrue(
            fuente.decisionesDeliberacion.contains(9L to DecisionDeliberacionVista.Cancelar),
        )
        assertNull(vm.estado.value.plan)
        assertNull(vm.estado.value.tabla)
    }

    @Test
    fun sinTablaNoSePuedeResolverPeroSiCancelar() {
        val fuente = FuenteNucleoFalsa().apply { errorConsulta = "no se pudo deliberar" }
        val vm = VistaModeloDeliberacion(fuente, ambito())
        vm.abrir(plan())
        assertNotNull(vm.estado.value.error)
        var resuelto: Long? = -1
        vm.resolver(DecisionDeliberacionVista.Aprobar) { resuelto = it }
        assertNull(resuelto)
        var cancelado = false
        vm.cancelar { cancelado = true }
        assertTrue(cancelado)
    }
}
