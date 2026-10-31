package org.cuatrodetres.magi.ui.vistaModelo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.cuatrodetres.magi.ui.dobles.FuenteNucleoFalsa
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TramoVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** ViewModel del terminal con el doble del núcleo (sin Compose ni Android). */
class PruebaVistaModeloTerminal {

    private fun ambito() = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun pantallaCompleta(texto: String = "hola mundo"): PantallaVista = PantallaVista(
        sesion = 1,
        columnas = 20,
        filas = 2,
        filasCambiadas = mapOf(0 to listOf(TramoVista(texto))),
        cursor = PosicionVista(0, 1),
        completa = true,
    )

    @Test
    fun acumulaSoloLasFilasCambiadasDeLaSesion() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        fuente.emitirPantalla(pantallaCompleta())
        fuente.emitirPantalla(
            PantallaVista(
                sesion = 1,
                columnas = 20,
                filas = 2,
                filasCambiadas = mapOf(1 to listOf(TramoVista("nueva"))),
                cursor = PosicionVista(2, 1),
            ),
        )
        val pantalla = vm.estado.value.pantallas[1]
        assertEquals(2, pantalla?.filasCambiadas?.size)
        assertEquals(PosicionVista(2, 1), pantalla?.cursor)
    }

    @Test
    fun escribeTextoYTeclasEnLaSesionVisible() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        vm.mostrarSesion(1)
        vm.alEscribir("ls\n")
        assertEquals(listOf("ls"), fuente.escrituras)
        assertEquals(TeclaVista.Intro, fuente.teclas[0].first)
    }

    @Test
    fun ctrlDeUnaPulsacionSeConsumeConLaTecla() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        vm.mostrarSesion(1)
        vm.alPulsarCtrl()
        vm.alPulsarTecla(TeclaVista.Escape)
        assertEquals(TeclaVista.Escape, fuente.teclas[0].first)
        assertEquals(true, fuente.teclas[0].second.ctrl)
        assertEquals(org.cuatrodetres.magi.ui.modelos.ModoModificador.NINGUNO, vm.estado.value.modificadores.ctrl)
    }

    @Test
    fun elZoomQuedaAcotadoEntreOchoYVeinticuatro() {
        val vm = VistaModeloTerminal(FuenteNucleoFalsa(), ambito())
        vm.fijarZoom(30)
        assertEquals(24, vm.estado.value.zoom)
        vm.alZoom(0.01f)
        assertEquals(8, vm.estado.value.zoom)
        vm.fijarZoom(13)
        assertEquals(13, vm.estado.value.zoom)
    }

    @Test
    fun laSeleccionEmpiezaEnLaPalabraYSeExtiende() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        fuente.emitirPantalla(pantallaCompleta())
        vm.mostrarSesion(1)
        vm.iniciarSeleccion(0, 7)
        assertEquals(PosicionVista(5, 0), vm.estado.value.seleccion?.desde)
        vm.extenderSeleccion(0, 9)
        assertEquals(PosicionVista(9, 0), vm.estado.value.seleccion?.hasta)
        vm.limpiarSeleccion()
        assertNull(vm.estado.value.seleccion)
    }

    @Test
    fun pegarVariasLineasSinModo2004PideConfirmacion() {
        val fuente = FuenteNucleoFalsa().apply { pegadoEntreCorchetesActivo = false }
        val vm = VistaModeloTerminal(fuente, ambito())
        vm.mostrarSesion(1)
        vm.solicitarPegado("uno\ndos")
        assertEquals("uno\ndos", vm.estado.value.pegadoPendiente)
        assertEquals(0, fuente.pegados.size)
        vm.confirmarPegado()
        assertEquals(1, fuente.pegados.size)
        assertNull(vm.estado.value.pegadoPendiente)
    }

    @Test
    fun conPegadoEntreCorchetesNoPideConfirmacion() {
        val fuente = FuenteNucleoFalsa().apply { pegadoEntreCorchetesActivo = true }
        val vm = VistaModeloTerminal(fuente, ambito())
        vm.mostrarSesion(1)
        vm.solicitarPegado("uno\ndos")
        assertNull(vm.estado.value.pegadoPendiente)
        assertEquals(1, fuente.pegados.size)
    }

    @Test
    fun redimensionarSoloEnviaCuandoCambia() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        vm.mostrarSesion(1)
        vm.alRedimensionar(80, 24)
        vm.alRedimensionar(80, 24)
        vm.alRedimensionar(0, 0)
        assertEquals(2, fuente.redimensiones.size)
        assertEquals(Triple(1L, 2, 1), fuente.redimensiones[1])
    }

    @Test
    fun alCerrarLaSesionSeSueltaSuPantalla() {
        val fuente = FuenteNucleoFalsa()
        val vm = VistaModeloTerminal(fuente, ambito())
        fuente.emitirPantalla(pantallaCompleta())
        vm.mostrarSesion(1)
        assertNotNull(vm.estado.value.pantalla)
        vm.limpiarSesion(1)
        assertNull(vm.estado.value.pantalla)
    }
}
