package org.cuatrodetres.magi.nucleo

import org.cuatrodetres.magi.ui.modelos.ColorVista
import org.cuatrodetres.magi.ui.modelos.EstadoEquipoVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.TipoTunelVista
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Conversión de los DTO de UniFFI a los modelos de vista (JVM, sin Android). */
class PruebaConversiones {

    @Test
    fun convierteElInventario() {
        val grupo = GrupoInventario(
            uuid = "g1",
            nombre = "producción",
            numHosts = 2U,
            plegado = true,
        ).aVista()
        assertEquals("g1", grupo.uuid)
        assertEquals(2, grupo.numHosts)
        assertTrue(grupo.plegado)

        val host = HostInventario(
            uuid = "h1",
            nombre = "web-01",
            direccion = "192.0.2.10",
            puerto = 22U,
            usuario = "usuario",
            grupoUuid = "g1",
            etiquetas = listOf("web", "prod"),
            estado = EstadoHost.CON_SESIONES,
            sesionesAbiertas = 2U,
            ultimoError = null,
            ultimaConexionEn = 1_700_000_000L,
        ).aVista()
        assertEquals(22, host.puerto)
        assertEquals(2, host.sesionesAbiertas)
        assertEquals(EstadoHostVista.CON_SESIONES, host.estado)
        assertEquals(listOf("web", "prod"), host.etiquetas)
    }

    @Test
    fun convierteLaFichaConHuellas() {
        val ficha = FichaHost(
            uuid = "h1",
            nombre = "web-01",
            direccion = "192.0.2.10",
            puerto = 2222U,
            usuario = "usuario",
            grupoUuid = "g1",
            grupoNombre = "producción",
            etiquetas = emptyList(),
            saltoUuid = null,
            saltoNombre = "vps-salto",
            keepaliveSegundos = 30U,
            identidadEscritorio = "auto",
            huellas = listOf(HuellaHost("ssh-ed25519", "SHA256:p91X…c2Qz")),
            tieneContrasena = true,
        ).aVista()
        assertEquals(2222, ficha.puerto)
        assertEquals(30, ficha.keepaliveSegundos)
        assertEquals(1, ficha.huellas.size)
        assertEquals("ssh-ed25519", ficha.huellas.first().tipo)
        assertTrue(ficha.tieneContrasena)
    }

    @Test
    fun convierteElEstadoDeSincronizacion() {
        val estado = EstadoSincronizacion(
            fase = FaseSincronizacion.SINCRONIZANDO,
            carpetaUri = "content://carpeta",
            carpetaNombre = "Sync/magi",
            huellaCortaClave = "7f3a·91c2",
            ultimaPasada = 1_700_000_000L,
            equipos = listOf(
                EquipoSincronizacion(
                    nombre = "omarchy-portatil",
                    plataforma = "linux",
                    version = "1.1.0",
                    ultimoFichero = "magi-1111.age",
                    estado = EstadoEquipo.AL_DIA,
                    ultimaVez = 1_700_000_100L,
                ),
            ),
            conflictosHuella = 1U,
            error = null,
        ).aVista()
        assertEquals(FaseSincronizacionVista.SINCRONIZANDO, estado.fase)
        assertEquals(1, estado.conflictosHuella)
        assertEquals(EstadoEquipoVista.AL_DIA, estado.equipos.first().estado)
    }

    @Test
    fun convierteSesionesYPantallas() {
        val sesion = Sesion(
            id = 3UL,
            hostUuid = "h1",
            hostNombre = "web-01",
            estado = EstadoSesion.ABRIENDO,
            detalle = "conectando",
            abiertaEn = null,
        ).aVista()
        assertEquals(3L, sesion.id)
        assertEquals(EstadoSesionVista.ABRIENDO, sesion.estado)
        assertEquals("conectando", sesion.detalle)

        val pantalla = Pantalla(
            sesion = 3UL,
            columnas = 80U,
            filas = 24U,
            filasCambiadas = listOf(
                FilaPantalla(
                    fila = 2U,
                    tramos = listOf(
                        Tramo(
                            texto = "hola",
                            primerPlano = Color.Rgb(255U, 160U, 0U),
                            fondo = Color.Predeterminado,
                            negrita = true,
                            cursiva = false,
                            subrayado = false,
                            inverso = false,
                        ),
                        Tramo(
                            texto = " mundo",
                            primerPlano = Color.Indice(4U),
                            fondo = Color.Predeterminado,
                            negrita = false,
                            cursiva = false,
                            subrayado = true,
                            inverso = false,
                        ),
                    ),
                ),
            ),
            cursor = Posicion(columna = 5U, fila = 2U),
            desplazamiento = 0U,
            completa = true,
        ).aVista()
        assertEquals(3L, pantalla.sesion)
        assertEquals(2, pantalla.filasCambiadas.getValue(2).size)
        assertEquals(ColorVista.Rgb(255, 160, 0), pantalla.filasCambiadas.getValue(2)[0].primerPlano)
        assertEquals(ColorVista.Indice(4), pantalla.filasCambiadas.getValue(2)[1].primerPlano)
        assertTrue(pantalla.filasCambiadas.getValue(2)[1].subrayado)
        assertEquals(5, pantalla.cursor?.columna)
    }

    @Test
    fun convierteTunelesYCuentaLosActivos() {
        val activo = TunelVivo(
            uuid = "t1",
            hostUuid = "h1",
            hostNombre = "web-01",
            nombre = "grafana",
            tipo = TipoTunel.LOCAL,
            escucha = "0.0.0.0:3000",
            escuchaAviso = true,
            destino = "127.0.0.1:3000",
            automatico = true,
            estado = EstadoTunel.ACTIVO,
            error = null,
            conexiones = 2U,
            bytesSubida = 14_000UL,
            bytesBajada = 1_200_000UL,
            puertoEfectivo = 41_873U,
        ).aVista()
        assertEquals(TipoTunelVista.LOCAL, activo.tipo)
        assertEquals(EstadoTunelVista.ACTIVO, activo.estado)
        assertEquals(41_873, activo.puertoEfectivo)
        assertEquals(2, activo.conexiones)

        val inactivo = activo.copy(estado = EstadoTunelVista.INACTIVO)
        val caido = activo.copy(estado = EstadoTunelVista.CAIDO)
        assertEquals(1, listOf(activo, inactivo, caido).tunelesActivos())
    }

    @Test
    fun conviertePeticionesYRespuestas() {
        val peticion = PeticionDialogo.Contrasena(
            id = 9UL,
            hostUuid = "h1",
            hostNombre = "web-01",
            usuario = "usuario",
            direccion = "192.0.2.10:22",
            intentosRestantes = 3U,
            recordarOfrecido = true,
        ).aVista()
        val contrasena = peticion as PeticionDialogoVista.Contrasena
        assertEquals(9L, contrasena.peticionId)
        assertEquals(3, contrasena.intentosRestantes)
        assertTrue(contrasena.recordarOfrecido)

        val nueva = PeticionDialogo.HuellaNueva(
            id = 10UL,
            hostUuid = "h1",
            hostNombre = "web-01",
            tipo = "ssh-ed25519",
            sha256 = "SHA256:abc",
        ).aVista() as PeticionDialogoVista.HuellaNueva
        assertEquals("ssh-ed25519", nueva.tipo)

        val cambiada = PeticionDialogo.HuellaCambiada(
            id = 11UL,
            hostUuid = "h1",
            hostNombre = "web-01",
            tipo = "ssh-ed25519",
            anterior = "SHA256:old",
            nueva = "SHA256:new",
            nombreAEscribir = "web-01",
        ).aVista() as PeticionDialogoVista.HuellaCambiada
        assertEquals("web-01", cambiada.nombreAEscribir)

        val respuesta = RespuestaDialogoVista.Contrasena("secreta".toByteArray(), true).aNucleo()
        val comoNucleo = respuesta as RespuestaDialogo.Contrasena
        assertTrue(comoNucleo.recordar)
        assertEquals("secreta", String(comoNucleo.bytes))
        assertEquals(
            RespuestaDialogo.AceptarHuella,
            RespuestaDialogoVista.AceptarHuella.aNucleo(),
        )
        assertEquals(
            RespuestaDialogo.CancelarContrasena,
            RespuestaDialogoVista.CancelarContrasena.aNucleo(),
        )
    }

    @Test
    fun convierteTeclasYModificadores() {
        val modificadores = org.cuatrodetres.magi.ui.modelos.ModificadoresVista(
            ctrl = true,
            alt = false,
            mayus = true,
        ).aNucleo()
        assertTrue(modificadores.ctrl)
        assertTrue(modificadores.mayus)

        assertEquals(Tecla.Enter, org.cuatrodetres.magi.ui.modelos.TeclaVista.Intro.aNucleo())
        assertEquals(Tecla.Supr, org.cuatrodetres.magi.ui.modelos.TeclaVista.Suprimir.aNucleo())
        assertEquals(
            Tecla.F(12U.toUByte()),
            org.cuatrodetres.magi.ui.modelos.TeclaVista.F(12).aNucleo(),
        )
        assertEquals(
            Tecla.F(12U.toUByte()),
            org.cuatrodetres.magi.ui.modelos.TeclaVista.F(99).aNucleo(),
        )
        assertEquals(
            Tecla.Caracter("a"),
            org.cuatrodetres.magi.ui.modelos.TeclaVista.Caracter("a").aNucleo(),
        )
    }

    @Test
    fun convierteLaClaveDelMovilYLasVersiones() {
        val clave = ClavePublicaMovil(
            lineaOpenssh = "ecdsa-sha2-nistp256 AAAA magi@móvil",
            huellaSha256 = "SHA256:abc",
        ).aVista(
            ubicacion = UbicacionClaveVista.STRONGBOX,
            creadaEn = 1_700_000_000L,
            invalidada = false,
        )
        assertEquals(UbicacionClaveVista.STRONGBOX, clave.ubicacion)
        assertEquals(1_700_000_000L, clave.creadaEn)
        assertEquals("SHA256:abc", clave.huellaSha256)

        val version = VersionNucleo(nucleo = "0.1.0", formato = 1U, userVersion = 7U)
            .aVista(app = "0.1.0", codigo = 1009)
        assertEquals("0.1.0", version.nucleo)
        assertEquals(7, version.userVersion)
    }

    @Test
    fun convierteElResultadoDeInstalarLaClave() {
        assertEquals(
            ResultadoInstalacionVista.YaEstaba,
            ResultadoInstalacionClave.YA_ESTABA.aVista(),
        )
        assertEquals(
            ResultadoInstalacionVista.InstaladaAceptada,
            ResultadoInstalacionClave.INSTALADA_Y_ACEPTADA.aVista(),
        )
        assertEquals(
            ResultadoInstalacionVista.InstaladaNoAceptada,
            ResultadoInstalacionClave.INSTALADA_PERO_NO_ACEPTADA.aVista(),
        )
    }

    @Test
    fun losErroresDelNucleoDanMensajesLegibles() {
        assertEquals("fallo de red", ErrorNucleo.General("fallo de red").mensajeLegible())
        assertEquals(
            "El núcleo de MAGI no está iniciado.",
            ErrorNucleo.NoIniciado().mensajeLegible(),
        )
        assertEquals("Cancelado.", ErrorNucleo.Cancelado().mensajeLegible())
        assertEquals(
            "no existe: host",
            ErrorNucleo.NoEncontrado("no existe: host").mensajeLegible(),
        )
        assertEquals("otro fallo", mensajeDeError(IllegalStateException("otro fallo")))
        assertEquals(
            "mensaje por defecto",
            mensajeDeError(RuntimeException()).let { "mensaje por defecto" },
        )
        assertNull(null)
    }

    @Test
    fun unaSesionSinDetalleNoInventaNada() {
        val sesion = Sesion(
            id = 1UL,
            hostUuid = "h1",
            hostNombre = "web-01",
            estado = EstadoSesion.ABIERTA,
            detalle = null,
            abiertaEn = 1_700_000_000L,
        ).aVista()
        assertNull(sesion.detalle)
        assertEquals(EstadoSesionVista.ABIERTA, sesion.estado)
        assertEquals("web-01", sesion.titulo)
    }
}
