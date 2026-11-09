package org.cuatrodetres.magi.ui.registro

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.DetalleDeliberacionRegistroVista
import org.cuatrodetres.magi.ui.modelos.DetalleRegistroVista
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.ResultadoRegistroVista
import org.cuatrodetres.magi.ui.modelos.TAMANO_PAGINA_REGISTRO
import org.cuatrodetres.magi.ui.modelos.TipoRegistroVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel de Registro con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloRegistro {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun entrada(id: Long, tipo: String = "conexion_abierta") = EntradaRegistroVista(
        id = id,
        fecha = 1_794_222_000,
        tipo = tipo,
        hostUuid = "h1",
        hostNombre = "web-01",
        resultado = ResultadoRegistroVista.OK,
        detalle = "sesión abierta",
    )

    private fun pagina(desde: Long, cuantas: Int) =
        (0 until cuantas).map { entrada(desde - it) }

    @Test
    fun iniciaConLaPrimeraPaginaYAvanzaConElCursorPorId() {
        val fuente = FuenteNucleoFalsa().apply {
            registroRespuesta = pagina(500, TAMANO_PAGINA_REGISTRO)
        }
        val vm = VistaModeloRegistro(fuente, ambito())
        vm.iniciar()
        assertEquals(TAMANO_PAGINA_REGISTRO, vm.estado.value.entradas.size)
        assertTrue(vm.estado.value.hayMas)
        val primera = fuente.paginasRegistro.first()
        assertNull(primera.second)
        assertEquals(TAMANO_PAGINA_REGISTRO, primera.third)
        // La segunda página arranca en el id de la última entrada de la primera.
        fuente.registroRespuesta = pagina(451, 2)
        vm.cargarMas()
        assertEquals(TAMANO_PAGINA_REGISTRO + 2, vm.estado.value.entradas.size)
        assertEquals(451L, fuente.paginasRegistro.last().second)
        assertFalse(vm.estado.value.hayMas)
    }

    @Test
    fun losFiltrosDeTipoYHostVanEnLaConsulta() {
        val fuente = FuenteNucleoFalsa().apply {
            registroRespuesta = pagina(10, 1)
        }
        val vm = VistaModeloRegistro(fuente, ambito())
        vm.iniciar()
        vm.alternarTipo(TipoRegistroVista.SERVICIOS)
        assertTrue(fuente.paginasRegistro.last().first.tipos.contains(TipoRegistroVista.SERVICIOS))
        vm.cambiarHost("h2")
        assertEquals("h2", fuente.paginasRegistro.last().first.hostUuid)
        vm.cambiarHost(null)
        assertNull(fuente.paginasRegistro.last().first.hostUuid)
        vm.limpiarTipos()
        assertTrue(fuente.paginasRegistro.last().first.tipos.isEmpty())
        vm.cambiarTexto("nginx")
        vm.aplicarFiltro()
        assertEquals("nginx", fuente.paginasRegistro.last().first.texto)
        assertTrue(vm.estado.value.filtrando)
    }

    @Test
    fun elDetalleDeUnaDeliberacionDiceSiNoHayResultado() {
        val fuente = FuenteNucleoFalsa().apply {
            registroRespuesta = listOf(entrada(9, tipo = "deliberacion_aprobada"))
            registroDetalleRespuesta = DetalleRegistroVista(
                entrada = entrada(9, tipo = "deliberacion_aprobada"),
                deliberacion = DetalleDeliberacionRegistroVista(
                    id = 3,
                    accion = "reiniciar nginx → web-01",
                    usuario = "móvil (prueba)",
                    sinResultado = true,
                    aprobadas = 1,
                    activas = 2,
                ),
            )
        }
        val vm = VistaModeloRegistro(fuente, ambito())
        vm.iniciar()
        vm.seleccionar(9)
        val detalle = vm.estado.value.detalle
        assertNotNull(detalle)
        assertFalse(vm.estado.value.cargandoDetalle)
        assertEquals("reiniciar nginx → web-01", detalle?.deliberacion?.accion)
        assertEquals(
            "sin resultado: MAGI se cerró durante la ejecución",
            detalle?.deliberacion?.textoResultado,
        )
        assertEquals("1/2", detalle?.deliberacion?.textoConsenso)
        vm.cerrarDetalle()
        assertNull(vm.estado.value.seleccionadaId)
    }
}
