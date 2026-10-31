package org.cuatrodetres.magi.ui.modelos

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Túneles del móvil: loopback forzado, puerto libre y contadores (§7.5). */
class PruebaModelosTuneles {

    private fun tunel(
        estado: EstadoTunelVista = EstadoTunelVista.INACTIVO,
        tipo: TipoTunelVista = TipoTunelVista.LOCAL,
        escucha: String = "127.0.0.1:3000",
        puertoEfectivo: Int? = null,
    ) = TunelVista(
        uuid = "t1",
        hostUuid = "h1",
        hostNombre = "web-01",
        nombre = "grafana",
        tipo = tipo,
        escucha = escucha,
        destino = "127.0.0.1:3000",
        estado = estado,
        puertoEfectivo = puertoEfectivo,
        conexiones = 2,
        bytesSubida = 14336,
        bytesBajada = 1258291,
    )

    @Test
    fun separaLaEscuchaDeSuPuerto() {
        assertEquals("0.0.0.0" to 8080, separarPuerto("0.0.0.0:8080"))
        assertEquals("[::]" to 1080, separarPuerto("[::]:1080"))
        assertNull(separarPuerto("sin-puerto"))
        assertNull(separarPuerto("host:99999"))
    }

    @Test
    fun localYDinamicoEscuchanSiempreEnLoopback() {
        assertEquals("127.0.0.1:8080", escuchaMostrada(tunel(escucha = "0.0.0.0:8080")))
        assertEquals("127.0.0.1:1080", escuchaMostrada(tunel(tipo = TipoTunelVista.DINAMICO, escucha = "[::]:1080")))
        assertEquals("127.0.0.1:41873", escuchaMostrada(tunel(puertoEfectivo = 41873)))
        assertEquals("0.0.0.0:3000", escuchaMostrada(tunel(tipo = TipoTunelVista.REMOTO, escucha = "0.0.0.0:3000")))
    }

    @Test
    fun elNavegadorSoloParaUnLocalActivo() {
        assertEquals("http://127.0.0.1:3000", urlNavegador(tunel(estado = EstadoTunelVista.ACTIVO)))
        assertNull(urlNavegador(tunel()))
        assertNull(urlNavegador(tunel(estado = EstadoTunelVista.ACTIVO, tipo = TipoTunelVista.REMOTO)))
        assertEquals(
            "http://127.0.0.1:41873",
            urlNavegador(tunel(estado = EstadoTunelVista.ACTIVO, puertoEfectivo = 41873)),
        )
    }

    @Test
    fun formateaBytesConComaDecimal() {
        assertEquals("0 B", formatoBytes(0))
        assertEquals("512 B", formatoBytes(512))
        assertEquals("14 KB", formatoBytes(14336))
        assertEquals("1,2 MB", formatoBytes(1258291))
        assertEquals("5 MB", formatoBytes(5 * 1024 * 1024))
    }

    @Test
    fun agrupaTunelesPorHost() {
        val lista = listOf(
            tunel().copy(uuid = "t1", hostUuid = "h2", hostNombre = "zeta"),
            tunel().copy(uuid = "t2", hostUuid = "h1", hostNombre = "alfa"),
            tunel().copy(uuid = "t3", hostUuid = "h1", hostNombre = "alfa", nombre = "socks"),
        )
        val bloques = agruparTuneles(lista)
        assertEquals(listOf("alfa", "zeta"), bloques.map { it.hostNombre })
        assertEquals(2, bloques[0].tuneles.size)
        assertEquals("grafana", bloques[0].tuneles[0].nombre)
        assertEquals("L", TipoTunelVista.LOCAL.letra)
        assertEquals("◐", glifoTunel(EstadoTunelVista.ACTIVANDO))
    }
}
