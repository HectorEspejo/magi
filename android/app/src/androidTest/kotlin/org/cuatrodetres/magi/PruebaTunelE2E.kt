package org.cuatrodetres.magi

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.CarpetaMovil
import org.cuatrodetres.magi.nucleo.Equipo
import org.cuatrodetres.magi.nucleo.ErrorNucleo
import org.cuatrodetres.magi.nucleo.EstadoSincronizacion
import org.cuatrodetres.magi.nucleo.EstadoTunel
import org.cuatrodetres.magi.nucleo.FicheroCarpeta
import org.cuatrodetres.magi.nucleo.FirmanteMovil
import org.cuatrodetres.magi.nucleo.LlaveroMovil
import org.cuatrodetres.magi.nucleo.Nucleo
import org.cuatrodetres.magi.nucleo.OyenteNucleo
import org.cuatrodetres.magi.nucleo.Pantalla
import org.cuatrodetres.magi.nucleo.PeticionDialogo
import org.cuatrodetres.magi.nucleo.RespuestaDialogo
import org.cuatrodetres.magi.nucleo.Retrollamadas
import org.cuatrodetres.magi.nucleo.Rutas
import org.cuatrodetres.magi.nucleo.Sesion
import org.cuatrodetres.magi.nucleo.TunelVivo
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.net.Socket
import java.security.KeyPairGenerator
import java.security.Signature

/**
 * E2E del túnel local en Android, sin red externa: arranca el núcleo nativo,
 * importa una fixture con un host 10.0.2.2:2222 y un túnel 127.0.0.1:9090 →
 * 127.0.0.1:9091, activa el túnel y comprueba que un socket local del propio
 * dispositivo recibe el eco. El servidor SSH y el servicio de eco los levanta
 * el anfitrión:
 *
 *   cargo run -p magi-movil --example servidor_pruebas_tunel
 *
 * La fixture se regenera con:
 *   cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored
 */
@RunWith(AndroidJUnit4::class)
class PruebaTunelE2E {

    companion object {
        private const val FICHERO = "magi-bbbbbbbb-cccc-4ddd-8eee-ffffffffffff.age"
        private const val RUTA_ASSETS = "tunel/" + FICHERO
        private const val CLAVE =
            "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS"
        private const val CARPETA = "content://prueba/tunel"
        private const val CARPETA_PRUEBA = "magi-prueba-tunel"
        private const val PUERTO_TUNEL = 9090
        private const val HOST_SSH = "10.0.2.2:12222"
        private const val CLAVE_HOST_SSH =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMDUg4hwJY3WuFnQXmPzlo2c28c7JgaJ2zkxyf8OGHJs"
    }

    private lateinit var contexto: Context
    private lateinit var raiz: File
    private val llavero = LlaveroEnMemoria()
    private val oyente = OyenteDePrueba()
    @Volatile
    private var nucleoParaDialogos: Nucleo? = null
    private var nucleo: Nucleo? = null

    @Before
    fun preparar() {
        contexto = InstrumentationRegistry.getInstrumentation().targetContext
        raiz = File(contexto.filesDir, CARPETA_PRUEBA)
        raiz.deleteRecursively()
        raiz.mkdirs()
        llavero.limpiar()
        oyente.limpiar()
        // La prueba conoce la clave del servidor de pruebas: sin esto el
        // núcleo pediría aceptar la huella (diálogo) y el saludo se agotaría.
        val rutaSsh = File(raiz, "hogar/.ssh").apply { mkdirs() }
        File(rutaSsh, "known_hosts").writeText(
            "[" + HOST_SSH + "] " + CLAVE_HOST_SSH + System.lineSeparator(),
        )
    }

    @After
    fun limpiar() {
        runCatching { nucleo?.destroy() }
        nucleo = null
        raiz.deleteRecursively()
    }

    @Test
    fun elTunelLocalReenviaEnAndroid() = runBlocking {
        // El E2E necesita el servidor de pruebas en el anfitrión. Sin él (CI
        // sin cuidadores), se omite en vez de fallar.
        val servidor = try {
            Socket().use {
                it.connect(java.net.InetSocketAddress("10.0.2.2", 12222), 1_000)
            }
            true
        } catch (e: Exception) {
            false
        }
        Assume.assumeTrue(
            "servidor de pruebas no disponible; arranca " +
                "cargo run -p magi-movil --example servidor_pruebas_tunel",
            servidor,
        )
        nucleo = Nucleo.iniciar(
            rutas = rutasPrivadas(),
            equipo = Equipo(nombre = "telefono-prueba", plataforma = "android"),
            retrollamadas = Retrollamadas(
                carpeta = CarpetaDeAssets(),
                llavero = llavero,
                firmante = FirmanteClaveBlanda(),
                oyente = oyente,
            ),
        )
        // Comprobación previa: el proceso de la app llega al SSH de pruebas.
        Socket("10.0.2.2", 12222).use { socket ->
            socket.soTimeout = 5_000
            val banner = socket.getInputStream().bufferedReader().readLine()
            assertTrue("saludo SSH inesperado: " + banner, banner.startsWith("SSH-2.0-"))
        }

        nucleoParaDialogos = nucleo
        nucleo!!.elegirCarpeta(CARPETA)
        nucleo!!.unirConClave(CLAVE)

        val inventario = nucleo!!.tuneles(null)
        val web = inventario.firstOrNull { it.nombre == "web" }
        assertNotNull(
            "el túnel web debe estar en el inventario; hay " +
                inventario.joinToString { it.nombre + ":" + it.estado },
            web,
        )
        assertEquals(EstadoTunel.INACTIVO, web!!.estado)

        nucleo!!.activarTunel(web.uuid)
        val activo = esperarTunel(web.uuid) { it.estado == EstadoTunel.ACTIVO }
        val puerto = activo.puertoEfectivo?.toInt() ?: PUERTO_TUNEL
        assertEquals(
            "el túnel debe escuchar en el puerto del inventario; fila=" + activo,
            PUERTO_TUNEL,
            puerto,
        )


        val eco = hablar("127.0.0.1", puerto, "hola por el tunel")
        assertEquals(
            "el agujero del túnel no devolvió el eco; fila=" + activo +
                " oyente=" + oyente.ultimosTuneles,
            "hola por el tunel",
            eco,
        )

        // Los contadores de la fila tienen que reflejar el tráfico (la
        // conexión ya se cerró: lo que importa son los bytes y las aceptadas).
        var fila = activo
        repeat(100) {
            fila = esperarTunel(web.uuid) { it.bytesSubida.toLong() > 0L }
            if (fila.bytesSubida.toLong() > 0L) return@repeat
            delay(30)
        }
        assertTrue(
            "la fila no cuenta el tráfico: " + fila,
            fila.bytesSubida.toLong() > 0L && fila.bytesBajada.toLong() > 0L,
        )
        val aceptadas = esperarTunel(web.uuid) { it.aceptadas.toInt() >= 1 }
        assertTrue(
            "la fila no cuenta las conexiones aceptadas: " + aceptadas,
            aceptadas.aceptadas.toInt() >= 1,
        )
    }

    private suspend fun esperarTunel(
        uuid: String,
        vale: (TunelVivo) -> Boolean,
    ): TunelVivo {
        var ultima: TunelVivo? = null
        repeat(100) {
            val lista = nucleo!!.tuneles(null)
            ultima = lista.firstOrNull { it.uuid == uuid }
            if (ultima != null && vale(ultima!!)) return ultima!!
            delay(30)
        }
        fail("el túnel no llegó al estado esperado: " + ultima + " lista=" + oyente.ultimosTuneles)
        error("sin túnel")
    }

    /** Conecta al agujero, escribe y devuelve el eco (con plazo). */
    private fun hablar(host: String, puerto: Int, texto: String): String {
        Socket(host, puerto).use { socket ->
            socket.soTimeout = 5_000
            socket.getOutputStream().write(texto.toByteArray())
            socket.getOutputStream().flush()
            val bufer = ByteArray(256)
            val leidos = socket.getInputStream().read(bufer)
            check(leidos > 0) { "el túnel cerró sin devolver nada" }
            return String(bufer, 0, leidos)
        }
    }

    private fun rutasPrivadas(): Rutas {
        fun directorio(nombre: String): String =
            File(raiz, nombre).apply { mkdirs() }.absolutePath
        val hogar = File(raiz, "hogar").apply { mkdirs() }
        File(hogar, ".ssh").mkdirs()
        return Rutas(
            datos = directorio("datos"),
            config = directorio("config"),
            estado = directorio("estado"),
            hogar = hogar.absolutePath,
            runtime = "",
        )
    }

    private class CarpetaDeAssets : CarpetaMovil {

        private fun fixture(): ByteArray =
            InstrumentationRegistry.getInstrumentation().context.assets
                .open(RUTA_ASSETS)
                .use { it.readBytes() }

        override fun listar(): List<FicheroCarpeta> {
            val contenido = fixture()
            return listOf(
                FicheroCarpeta(
                    nombre = FICHERO,
                    tamano = contenido.size.toULong(),
                    modificadoEn = 1_700_000_000L,
                ),
            )
        }

        override fun leer(nombre: String, maximo: ULong): ByteArray {
            check(nombre == FICHERO) { "nombre inesperado: " + nombre }
            val contenido = fixture()
            check(contenido.size.toULong() <= maximo) { "la fixture supera el máximo" }
            return contenido
        }
    }

    private class LlaveroEnMemoria : LlaveroMovil {
        private val datos = LinkedHashMap<String, ByteArray>()

        @Synchronized
        override fun leer(clave: String): ByteArray? = datos[clave]?.copyOf()

        @Synchronized
        override fun guardar(clave: String, secreto: ByteArray) {
            datos[clave] = secreto.copyOf()
        }

        @Synchronized
        override fun borrar(clave: String) {
            datos.remove(clave)
        }

        @Synchronized
        fun limpiar() {
            datos.clear()
        }
    }

    /** Clave EC P-256 en software (el servidor de pruebas no verifica la firma). */
    private class FirmanteClaveBlanda : FirmanteMovil {
        private val par = KeyPairGenerator.getInstance("EC").apply {
            initialize(256)
        }.generateKeyPair()

        override fun clavePublicaX509(): ByteArray = par.public.encoded

        override fun firmar(datos: ByteArray): ByteArray {
            val firma = Signature.getInstance("SHA256withECDSA")
            firma.initSign(par.private)
            firma.update(datos)
            return firma.sign()
        }

        override fun olvidarClave() = Unit
    }

    private inner class OyenteDePrueba : OyenteNucleo {
        @Volatile
        var ultimosTuneles: List<TunelVivo> = emptyList()
            private set

        @Synchronized
        override fun pantalla(pantalla: Pantalla) = Unit

        @Synchronized
        override fun sesiones(lista: List<Sesion>) = Unit

        @Synchronized
        override fun tuneles(lista: List<TunelVivo>) {
            ultimosTuneles = lista
        }

        @Synchronized
        override fun dialogo(peticion: PeticionDialogo) {
                // La prueba conoce la clave del servidor: acepta la huella si el
            // known_hosts no la hubiera leído, para no bloquear el saludo.
            runCatching {
                when (peticion) {
                    is PeticionDialogo.HuellaNueva ->
                        nucleoParaDialogos?.responderDialogo(
                            peticion.id,
                            RespuestaDialogo.AceptarHuella,
                        )
                    else -> Unit
                }
            }
        }

        @Synchronized
        override fun sincronizacion(estado: EstadoSincronizacion) = Unit

        @Synchronized
        fun limpiar() {
            ultimosTuneles = emptyList()
        }
    }
}
