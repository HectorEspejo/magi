package org.cuatrodetres.magi

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.CarpetaMovil
import org.cuatrodetres.magi.nucleo.Equipo
import org.cuatrodetres.magi.nucleo.ErrorNucleo
import org.cuatrodetres.magi.nucleo.EstadoSincronizacion
import org.cuatrodetres.magi.nucleo.FicheroCarpeta
import org.cuatrodetres.magi.nucleo.FirmanteMovil
import org.cuatrodetres.magi.nucleo.LlaveroMovil
import org.cuatrodetres.magi.nucleo.Nucleo
import org.cuatrodetres.magi.nucleo.NucleoRepositorio
import org.cuatrodetres.magi.nucleo.OyenteNucleo
import org.cuatrodetres.magi.nucleo.Pantalla
import org.cuatrodetres.magi.nucleo.PeticionDialogo
import org.cuatrodetres.magi.nucleo.Retrollamadas
import org.cuatrodetres.magi.nucleo.Rutas
import org.cuatrodetres.magi.nucleo.Sesion
import org.cuatrodetres.magi.nucleo.TipoHostMovil
import org.cuatrodetres.magi.nucleo.TunelVivo
import org.cuatrodetres.magi.plataforma.ClaveMovil
import org.cuatrodetres.magi.plataforma.ContenedorPlataforma
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assume
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Prueba de humo instrumentada con el núcleo nativo de verdad (checklist Fase
 * 11): carga `libmagi_movil.so`, inicia el núcleo con retrollamadas falsas
 * (sin red, SAF, Keystore ni biometría), importa la fixture de sincronización
 * de `androidTest/assets` y comprueba que el inventario tiene el grupo
 * «produccion» con sus hosts web-01 y db-01.
 *
 * La fixture se regenera con:
 *   cargo test -p magi-nucleo --test generar_fixture_android -- --ignored
 */
@RunWith(AndroidJUnit4::class)
class PruebaHumoNucleo {

    companion object {
        private const val FICHERO = "magi-aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee.age"
        private const val RUTA_ASSETS = "sincronizacion/" + FICHERO
        private const val CLAVE =
            "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS"
        private const val CARPETA = "content://prueba/carpeta"
        private const val CARPETA_PRUEBA = "magi-prueba-humo"
    }

    private lateinit var contexto: Context
    private lateinit var raiz: File
    private val llavero = LlaveroEnMemoria()
    private val oyente = OyenteDePrueba()
    private var nucleo: Nucleo? = null

    @Before
    fun preparar() {
        contexto = InstrumentationRegistry.getInstrumentation().targetContext
        raiz = File(contexto.filesDir, CARPETA_PRUEBA)
        raiz.deleteRecursively()
        raiz.mkdirs()
        llavero.limpiar()
        oyente.limpiar()
    }

    @After
    fun limpiar() {
        runCatching { nucleo?.destroy() }
        nucleo = null
        raiz.deleteRecursively()
    }

    @Test
    fun cargaElNucleoImportaLaFixtureYPintaSusHosts() = runBlocking {
        nucleo = Nucleo.iniciar(
            rutas = rutasPrivadas(),
            equipo = Equipo(nombre = "telefono-prueba", plataforma = "android"),
            retrollamadas = Retrollamadas(
                carpeta = CarpetaDeAssets(),
                llavero = llavero,
                firmante = FirmanteSinKeystore(),
                oyente = oyente,
            ),
        )

        // El núcleo nativo está cargado y sabe qué formato y base de datos usa.
        // El formato es el máximo que entiende este binario (2 desde la F12);
        // la fixture es del formato 1, que también se lee.
        val version = nucleo!!.version()
        assertEquals(2, version.formato.toInt())
        assertEquals(8, version.userVersion.toInt())
        assertTrue(version.nucleo.isNotBlank())

        // Elegir la carpeta de prueba (la fixture) y unir con su clave.
        nucleo!!.elegirCarpeta(CARPETA)
        nucleo!!.unirConClave(CLAVE)
        assertTrue(
            "la clave de sincronización debe quedar en el llavero",
            llavero.cuentas().contains("sincronizacion"),
        )

        // Inventario: grupo «produccion» con dos hosts.
        val grupos = nucleo!!.inventarioGrupos()
        val hosts = nucleo!!.inventarioHosts(null)
        val diagnostico = "grupos=" + grupos.joinToString { it.nombre + ":" + it.numHosts } +
            " hosts=" + hosts.joinToString { it.nombre + "@" + it.direccion + " grupo=" + it.grupoUuid } +
            " sincro=" + oyente.ultimaSincronizacion
        val produccion = grupos.firstOrNull { it.nombre == "produccion" }
        assertNotNull("el grupo produccion debe existir; " + diagnostico, produccion)
        assertEquals("numHosts del grupo; " + diagnostico, 2, produccion!!.numHosts.toInt())
        assertEquals("hosts del inventario; " + diagnostico, 2, hosts.size)
        val web = hosts.first { it.nombre == "web-01" }
        val db = hosts.first { it.nombre == "db-01" }
        assertEquals("192.0.2.10", web.direccion)
        assertEquals("192.0.2.11", db.direccion)
        assertEquals(22, web.puerto.toInt())
        assertEquals("usuario", web.usuario)
        assertEquals(produccion.uuid, web.grupoUuid)
        // Un fichero del formato 1 no trae tipo: todos sus hosts son SSH.
        assertEquals(TipoHostMovil.SSH, web.tipo)
        assertEquals(TipoHostMovil.SSH, db.tipo)

        // La ficha de un host se puede abrir y trae los datos del documento.
        val ficha = nucleo!!.fichaHost(web.uuid)
        assertEquals("web-01", ficha.nombre)
        assertEquals("192.0.2.10", ficha.direccion)
        assertEquals("produccion", ficha.grupoNombre)
        assertEquals("usuario", ficha.usuario)

        // El oyente recibió el estado de sincronización de la pasada.
        assertNotNull(
            "el oyente debe recibir el estado de la sincronización",
            oyente.ultimaSincronizacion,
        )
        assertEquals(0, oyente.ultimaSincronizacion!!.conflictosHuella.toInt())
    }

    /**
     * C1 de la revisión adversarial: al arrancar el núcleo la clave del móvil
     * tiene que existir. Se prueba con el Keystore real del emulador; si el AVD
     * no tiene PIN/patrón, no se puede crear y la prueba se omite.
     */
    @Test
    fun elArranqueCreaLaClaveDelMovilYQuedaLista() = runBlocking {
        val plataforma = ContenedorPlataforma(contexto)
        Assume.assumeTrue(
            "el emulador no tiene PIN, patrón ni contraseña: no se puede crear la clave",
            plataforma.bloqueo.elTelefonoTieneBloqueo(),
        )
        try {
            plataforma.iniciar()
            NucleoRepositorio.iniciar(contexto, plataforma, OyenteDePrueba())
            assertEquals(ClaveMovil.Estado.LISTA, plataforma.firmante.estado())
        } finally {
            runCatching { NucleoRepositorio.cerrar() }
            runCatching { plataforma.firmante.olvidarClave() }
            File(contexto.filesDir, NucleoRepositorio.DIRECTORIO_RAIZ).deleteRecursively()
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

    // -----------------------------------------------------------------------
    // Retrollamadas falsas: sin red, SAF real, Keystore ni biometría.
    // -----------------------------------------------------------------------

    /** Lista y lee la fixture empaquetada en los assets de la prueba. */
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

    /** Llavero en memoria: la prueba no toca el Keystore. */
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
        fun cuentas(): Set<String> = datos.keys.toSet()

        @Synchronized
        fun limpiar() {
            datos.clear()
        }
    }

    /** La prueba de humo no firma: el núcleo tolera que aún no haya clave. */
    private class FirmanteSinKeystore : FirmanteMovil {

        private fun error(): ErrorNucleo =
            ErrorNucleo.Firmante("la prueba de humo no usa la clave del móvil")

        override fun clavePublicaX509(): ByteArray = throw error()

        override fun firmar(datos: ByteArray): ByteArray = throw error()

        override fun olvidarClave() = Unit
    }

    /** Apunta las listas completas y el estado de sincronización. */
    private class OyenteDePrueba : OyenteNucleo {

        @Volatile
        var ultimaSincronizacion: EstadoSincronizacion? = null
            private set

        @Synchronized
        override fun pantalla(pantalla: Pantalla) = Unit

        @Synchronized
        override fun sesiones(lista: List<Sesion>) = Unit

        @Synchronized
        override fun tuneles(lista: List<TunelVivo>) = Unit

        @Synchronized
        override fun dialogo(peticion: PeticionDialogo) = Unit

        @Synchronized
        override fun sincronizacion(estado: EstadoSincronizacion) {
            ultimaSincronizacion = estado
        }

        @Synchronized
        fun limpiar() {
            ultimaSincronizacion = null
        }
    }
}
