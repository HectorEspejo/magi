package org.cuatrodetres.magi.ui.modelos

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Inventario, agrupación y sesiones vivas (informe §6.1 y §6.2). */
class PruebaModelosInventario {

    private fun host(
        uuid: String,
        nombre: String,
        grupo: String? = null,
        estado: EstadoHostVista = EstadoHostVista.SIN_SESION,
    ) = HostVista(
        uuid = uuid,
        nombre = nombre,
        direccion = "192.0.2.10",
        puerto = 22,
        usuario = "usuario",
        grupoUuid = grupo,
        estado = estado,
    )

    @Test
    fun agrupaPorNombreYSinGrupoAlFinal() {
        val inventario = InventarioVista(
            grupos = listOf(
                GrupoVista("g2", "laboratorio"),
                GrupoVista("g1", "4d3"),
            ),
            hosts = listOf(
                host("h2", "zeta", "g1"),
                host("h1", "alfa", "g1"),
                host("h3", "suelto", null),
                host("h4", "suelto2", null),
            ),
        )
        val bloques = agruparInventario(inventario)
        assertEquals(2, bloques.size)
        assertEquals("4d3", bloques[0].grupo?.nombre)
        assertEquals(listOf("alfa", "zeta"), bloques[0].hosts.map { it.nombre })
        assertEquals(null, bloques[1].grupo)
        assertEquals(2, bloques[1].hosts.size)
    }

    @Test
    fun losGruposVaciosNoSePintan() {
        val inventario = InventarioVista(
            grupos = listOf(GrupoVista("g1", "vacío"), GrupoVista("g2", "lleno")),
            hosts = listOf(host("h1", "host", "g2")),
        )
        val bloques = agruparInventario(inventario)
        assertEquals(1, bloques.size)
        assertEquals("lleno", bloques[0].grupo?.nombre)
    }

    @Test
    fun lasSesionesEnVivoCambianElGlifoDelHost() {
        val inventario = InventarioVista(hosts = listOf(host("h1", "web-01", estado = EstadoHostVista.ERROR)))
        val sesiones = listOf(
            SesionVista(1, "h1", "web-01", EstadoSesionVista.ABIERTA),
            SesionVista(2, "h1", "web-01", EstadoSesionVista.ABIERTA),
            SesionVista(3, "otro", "db-01", EstadoSesionVista.ABRIENDO),
        )
        val resultado = inventarioConSesiones(inventario, sesiones)
        assertEquals(EstadoHostVista.CON_SESIONES, resultado.hosts[0].estado)
        assertEquals(2, resultado.hosts[0].sesionesAbiertas)
    }

    @Test
    fun sinSesionesElInventarioNoCambia() {
        val inventario = InventarioVista(hosts = listOf(host("h1", "web-01")))
        assertEquals(inventario, inventarioConSesiones(inventario, emptyList()))
    }

    @Test
    fun losEscritoriosSeDistinguenDeLosHostsSsh() {
        val ssh = host("h1", "web-01")
        assertFalse(esEscritorio(ssh))
        assertEquals(TipoHostVista.SSH, ssh.tipo)
        assertFalse(ssh.tipo.esEscritorio)

        val escritorio = ssh.copy(tipo = TipoHostVista.RDP)
        assertTrue(esEscritorio(escritorio))
        assertTrue(escritorio.tipo.esEscritorio)
        assertEquals("RDP", escritorio.tipo.etiqueta)
        assertEquals("VNC", TipoHostVista.VNC.etiqueta)
        assertEquals("voxl", TipoHostVista.VOXL.etiqueta)
    }

    @Test
    fun direccionYGlifos() {
        val h = host("h1", "web-01")
        assertEquals("usuario@192.0.2.10:22", textoDireccion(h))
        assertEquals("○", glifoEstadoHost(EstadoHostVista.SIN_SESION, 0))
        assertEquals("◐", glifoEstadoHost(EstadoHostVista.CONECTANDO, 0))
        assertEquals("●2", glifoEstadoHost(EstadoHostVista.CON_SESIONES, 2))
        assertEquals("✕", glifoEstadoHost(EstadoHostVista.ERROR, 0))
        assertEquals(30, keepaliveEfectivo(null))
        assertTrue(keepaliveEsDelMovil(null))
        assertEquals(45, keepaliveEfectivo(45))
    }
}
