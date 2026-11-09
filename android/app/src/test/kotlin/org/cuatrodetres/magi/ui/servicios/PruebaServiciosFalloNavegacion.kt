package org.cuatrodetres.magi.ui.servicios

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.SeccionServiciosVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.modelos.UnidadVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** «Toda la flota»: tocar un fallo abre su host con ese elemento (§6.3). */
class PruebaServiciosFalloNavegacion {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @Test
    fun tocar_un_fallo_abre_el_host_con_ese_elemento() {
        val fuente = FuenteNucleoFalsa()
        fuente.listadoRespuesta = ListadoServiciosVista(
            unidades = listOf(UnidadVista(nombre = "copia.service")),
        )
        val vm = VistaModeloServicios(fuente, ambito = ambito())

        vm.abrirHost(
            "h1",
            "web-01",
            ObjetivoVista(TipoObjetivoVista.UNIDAD, "copia.service"),
        )

        assertEquals("h1", vm.estado.value.hostUuid)
        assertEquals("copia.service", vm.estado.value.objetivo?.nombre)
        assertEquals(SeccionServiciosVista.SERVICIOS, vm.estado.value.seccion)
    }

    @Test
    fun un_fallo_de_contenedor_abre_contenedores_y_su_detalle() {
        val fuente = FuenteNucleoFalsa()
        fuente.listadoRespuesta = ListadoServiciosVista()
        val vm = VistaModeloServicios(fuente, ambito = ambito())

        vm.abrirHost(
            "h1",
            "web-01",
            ObjetivoVista(TipoObjetivoVista.CONTENEDOR, "panel"),
        )

        assertEquals(SeccionServiciosVista.CONTENEDORES, vm.estado.value.seccion)
        assertTrue(vm.estado.value.aviso != null)
    }
}
