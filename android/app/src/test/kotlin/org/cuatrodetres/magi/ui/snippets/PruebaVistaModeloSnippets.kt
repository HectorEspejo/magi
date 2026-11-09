package org.cuatrodetres.magi.ui.snippets

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.modelos.VariableSnippetVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ViewModel de Snippets con el doble de la fuente del núcleo (sin Android). */
class PruebaVistaModeloSnippets {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun host(uuid: String, nombre: String) = HostDestinoVista(uuid = uuid, nombre = nombre)

    private fun snippet(
        uuid: String = "s1",
        nombre: String = "reiniciar nginx",
        critico: Boolean = false,
        hosts: List<HostDestinoVista> = listOf(host("h1", "web-01"), host("h2", "web-02")),
        variables: List<VariableSnippetVista> = emptyList(),
    ) = SnippetVista(
        uuid = uuid,
        nombre = nombre,
        comando = "systemctl restart nginx",
        descripcion = "",
        critico = critico,
        timeoutSeg = 60,
        pararAlFallo = false,
        usadoVeces = 2,
        ultimoUsoEn = 1_794_222_000,
        hosts = hosts,
        variables = variables,
    )

    private fun preparacion(
        delibera: Boolean = false,
        errores: List<org.cuatrodetres.magi.ui.modelos.ErrorCampoVista> = emptyList(),
    ) = PreparacionEjecucionVista(
        snippetUuid = "s1",
        snippetNombre = "reiniciar nginx",
        comando = "uptime",
        hosts = listOf(host("h1", "web-01")),
        errores = errores,
        delibera = delibera,
        motivos = if (delibera) listOf("2 hosts") else emptyList(),
        timeoutSeg = 60,
    )

    @Test
    fun iniciaCargaYFiltraLosSnippets() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(snippet(), snippet(uuid = "s2", nombre = "limpiar journald"))
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        assertFalse(vm.estado.value.cargando)
        assertEquals(2, vm.estado.value.todos.size)
        assertEquals(2, vm.estado.value.visibles.size)
        vm.cambiarFiltro("journald")
        assertEquals(1, vm.estado.value.visibles.size)
        assertEquals("s2", vm.estado.value.visibles[0].uuid)
        assertTrue(fuente.llamadas.any { it.startsWith("snippets:") })
    }

    @Test
    fun desdeLaFichaSoloTraeLosSnippetsDelHostYPreparaSoloEnEl() {
        val fuente = FuenteNucleoFalsa().apply { snippetsRespuesta = listOf(snippet()) }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar("h2")
        assertTrue(fuente.llamadas.contains("snippetsDeHost:h2"))
        vm.abrir("s1")
        // El ViewModel conserva todos los hosts resueltos del snippet; la
        // pantalla enseña solo el de la ficha. La marca sí es solo la suya.
        assertEquals(setOf("h2"), vm.estado.value.hostsMarcados)
        assertEquals(2, vm.estado.value.hosts.size)
        assertTrue(vm.estado.value.puedeContinuar)
    }

    @Test
    fun abrirMarcaTodosLosHostsYCargaVariablesConSuDefecto() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(
                snippet(
                    variables = listOf(
                        VariableSnippetVista(nombre = "dias", valorDefecto = "7"),
                        VariableSnippetVista(nombre = "unidad", valorDefecto = null),
                    ),
                ),
            )
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        vm.abrir("s1")
        assertEquals(setOf("h1", "h2"), vm.estado.value.hostsMarcados)
        assertEquals(2, vm.estado.value.variables.size)
        assertEquals("7", vm.estado.value.variables[0].valorDefecto)
        assertFalse(vm.estado.value.variables[0].faltaValor)
        assertTrue(vm.estado.value.variables[1].faltaValor)
        // Con una variable sin valor no se puede continuar.
        assertFalse(vm.estado.value.puedeContinuar)
        vm.cambiarVariable("unidad", "nginx.service")
        assertTrue(vm.estado.value.puedeContinuar)
        vm.alternarHost("h2")
        assertEquals(setOf("h1"), vm.estado.value.hostsMarcados)
        vm.alternarPararAlFallo()
        assertTrue(vm.estado.value.pararAlFallo)
    }

    @Test
    fun continuarSinDeliberacionLanzaLaEjecucion() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(snippet())
            preparacionRespuesta = preparacion()
            idRespuesta = 42
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        vm.abrir("s1")
        vm.continuar()
        assertTrue(fuente.llamadas.any { it.startsWith("prepararEjecucion:s1:") })
        assertTrue(fuente.llamadas.contains("ejecutarSnippet:s1"))
        assertEquals(42L, vm.estado.value.ejecucionLanzada)
        assertNull(vm.estado.value.planDeliberacion)
    }

    @Test
    fun continuarConDeliberacionAbreElDialogoYNoEjecutaTodavia() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(snippet(critico = true))
            preparacionRespuesta = preparacion(delibera = true)
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        vm.abrir("s1")
        vm.continuar()
        val plan = vm.estado.value.planDeliberacion
        assertNotNull(plan)
        assertEquals("reiniciar nginx → web-01", plan?.accion)
        assertFalse(fuente.llamadas.contains("ejecutarSnippet:s1"))
        vm.ejecutarDeliberado(plan!!, 9)
        assertTrue(fuente.llamadas.contains("ejecutarSnippet:s1"))
        assertNull(vm.estado.value.planDeliberacion)
    }

    @Test
    fun unErrorDelNucleoAlPrepararSeEnsenaYNoSeLanza() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(snippet())
            errorConsulta = "la variable está vacía"
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        vm.abrir("s1")
        vm.cambiarVariable("unidad", "")
        vm.continuar()
        assertEquals("la variable está vacía", vm.estado.value.error)
        assertFalse(fuente.llamadas.contains("ejecutarSnippet:s1"))
    }

    @Test
    fun losErroresPorCampoDeLaPreparacionSeVen() {
        val fuente = FuenteNucleoFalsa().apply {
            snippetsRespuesta = listOf(snippet())
            preparacionRespuesta = preparacion(
                errores = listOf(
                    org.cuatrodetres.magi.ui.modelos.ErrorCampoVista("hosts", "no es destino"),
                ),
            )
        }
        val vm = VistaModeloSnippets(fuente, ambito())
        vm.iniciar(null)
        vm.abrir("s1")
        vm.continuar()
        assertEquals("no es destino", vm.estado.value.erroresPorCampo["hosts"])
        assertFalse(fuente.llamadas.contains("ejecutarSnippet:s1"))
    }
}
