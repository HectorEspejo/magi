package org.cuatrodetres.magi.ui.vistaModelo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.flota.VistaModeloFlota
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.FallosFlotaVista
import org.cuatrodetres.magi.ui.modelos.GrupoVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel de Flota con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloFlota {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun host(
        uuid: String,
        nombre: String,
        grupo: String? = "g1",
        estado: EstadoFlotaVista = EstadoFlotaVista.NOMINAL,
        sesiones: Int = 0,
        fallos: Int = 0,
        etiquetas: List<String> = emptyList(),
    ) = HostFlotaVista(
        uuid = uuid,
        nombre = nombre,
        direccion = "192.0.2.10",
        puerto = 22,
        grupoUuid = grupo,
        etiquetas = etiquetas,
        estado = estado,
        sesiones = sesiones,
        fallos = FallosFlotaVista(total = fallos),
    )

    private fun fuenteCon(hosts: List<HostFlotaVista>): FuenteNucleoFalsa {
        val fuente = FuenteNucleoFalsa()
        fuente.inventarioRespuesta = InventarioVista(
            grupos = listOf(GrupoVista("g1", "producción", 2)),
            hosts = emptyList(),
        )
        fuente.flotaRespuesta = hosts
        return fuente
    }

    @Test
    fun el_arranque_carga_grupos_y_sondea_los_sondeables() {
        val fuente = fuenteCon(
            listOf(host("h1", "web-01"), host("h2", "db-01", grupo = null)),
        )
        val vm = VistaModeloFlota(fuente, ambito())
        vm.iniciar()

        assertEquals(listOf(listOf("h1", "h2")), fuente.sondas)
        assertEquals(2, vm.estado.value.hosts.size)
        assertEquals(1, vm.estado.value.grupos.size)
        assertFalse(vm.estado.value.cargando)
        // Los resultados llegan por el flujo del núcleo.
        fuente.emitirFlota(listOf(host("h1", "web-01", estado = EstadoFlotaVista.CARGA, fallos = 3)))
        assertEquals(1, vm.estado.value.hosts.size)
        assertEquals(EstadoFlotaVista.CARGA, vm.estado.value.hosts.first().estado)
    }

    @Test
    fun el_filtro_y_el_plegado_deciden_lo_que_se_ve() {
        val fuente = fuenteCon(
            listOf(
                host("h1", "web-01", etiquetas = listOf("web")),
                host("h2", "db-01"),
            ),
        )
        val vm = VistaModeloFlota(fuente, ambito())
        vm.iniciar()

        vm.cambiarFiltro("web")
        assertEquals(listOf("h1"), vm.estado.value.visibles.map { it.uuid })
        assertEquals(listOf("h1"), vm.estado.value.visiblesDesplegados.map { it.uuid })

        vm.cambiarFiltro("")
        vm.plegarGrupo("g1")
        assertTrue(vm.estado.value.grupos.first().plegado)
        assertTrue(vm.estado.value.visiblesDesplegados.isEmpty())
        assertTrue(fuente.llamadas.contains("plegar:g1:true"))
    }

    @Test
    fun el_refresco_automatico_solo_sondea_los_hosts_visibles() {
        val fuente = fuenteCon(
            listOf(
                host("h1", "web-01", etiquetas = listOf("web")),
                host("h2", "db-01"),
            ),
        )
        val vm = VistaModeloFlota(fuente, ambito())
        vm.iniciar()
        fuente.sondas.clear()

        vm.cambiarFiltro("web")
        vm.refrescarAutomatico()
        assertEquals(listOf(listOf("h1")), fuente.sondas)

        // Con el grupo plegado no se refresca nada: lo que no se ve no se gasta.
        fuente.sondas.clear()
        vm.cambiarFiltro("")
        vm.plegarGrupo("g1")
        vm.refrescarAutomatico()
        assertTrue(fuente.sondas.isEmpty())
    }

    @Test
    fun deslizar_sondea_los_filtrados_y_lanza_la_sincronizacion() {
        val fuente = fuenteCon(
            listOf(
                host("h1", "web-01", etiquetas = listOf("web")),
                host("h2", "db-01"),
            ),
        )
        val vm = VistaModeloFlota(fuente, ambito())
        vm.iniciar()
        fuente.sondas.clear()

        vm.cambiarFiltro("db")
        vm.deslizar()

        assertEquals(listOf(listOf("h2")), fuente.sondas)
        assertTrue(fuente.llamadas.contains("sincronizar"))
    }

    @Test
    fun el_contador_de_sondeo_sale_de_los_estados() {
        val fuente = fuenteCon(
            listOf(
                host("h1", "web-01", estado = EstadoFlotaVista.SONDEANDO),
                host("h2", "db-01", estado = EstadoFlotaVista.NOMINAL),
                host("h3", "cache", estado = EstadoFlotaVista.FRIA),
            ),
        )
        val vm = VistaModeloFlota(fuente, ambito())
        vm.iniciar()
        fuente.emitirFlota(
            listOf(
                host("h1", "web-01", estado = EstadoFlotaVista.SONDEANDO),
                host("h2", "db-01", estado = EstadoFlotaVista.NOMINAL),
                host("h3", "cache", estado = EstadoFlotaVista.FRIA),
            ),
        )
        assertTrue(vm.estado.value.sondeando)
        assertEquals(1, vm.estado.value.leidos)

        fuente.emitirFlota(
            listOf(
                host("h1", "web-01", estado = EstadoFlotaVista.CAIDA),
                host("h2", "db-01", estado = EstadoFlotaVista.NOMINAL),
            ),
        )
        assertFalse(vm.estado.value.sondeando)
        assertEquals(2, vm.estado.value.leidos)
    }
}
