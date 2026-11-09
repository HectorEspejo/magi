package org.cuatrodetres.magi

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.CarpetaMovil
import org.cuatrodetres.magi.nucleo.Equipo
import org.cuatrodetres.magi.nucleo.ErrorNucleo
import org.cuatrodetres.magi.nucleo.EstadoEquipo
import org.cuatrodetres.magi.nucleo.EstadoSincronizacion
import org.cuatrodetres.magi.nucleo.FaseSincronizacion
import org.cuatrodetres.magi.nucleo.FicheroCarpeta
import org.cuatrodetres.magi.nucleo.FirmanteMovil
import org.cuatrodetres.magi.nucleo.LlaveroMovil
import org.cuatrodetres.magi.nucleo.Nucleo
import org.cuatrodetres.magi.nucleo.OyenteNucleo
import org.cuatrodetres.magi.nucleo.AccionServicioMovil
import org.cuatrodetres.magi.nucleo.EjecucionMovil
import org.cuatrodetres.magi.nucleo.EstadoLogs
import org.cuatrodetres.magi.nucleo.HostFlota
import org.cuatrodetres.magi.nucleo.ResultadoServicioMovil
import org.cuatrodetres.magi.nucleo.Pantalla
import org.cuatrodetres.magi.nucleo.PeticionDialogo
import org.cuatrodetres.magi.nucleo.Retrollamadas
import org.cuatrodetres.magi.nucleo.Rutas
import org.cuatrodetres.magi.nucleo.Sesion
import org.cuatrodetres.magi.nucleo.TunelVivo
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * La app Android lee el formato 3 de la sincronización (Fase 13): importa una
 * fixture con una comprobación de servicio, deja el equipo al día y la guarda
 * en su copia sin usarla (la vista de servicios del móvil llega en la Fase 14).
 *
 * Se ejecuta en el emulador, como la prueba de humo. La fixture se regenera con
 * cargo test -p magi-nucleo --test generar_fixture_android
 *   generar_fixture_android_formato3 -- --ignored --nocapture
 */
@RunWith(AndroidJUnit4::class)
class PruebaFormatoSincronizacionAndroid {

    companion object {
        private const val FICHERO = "magi-dddddddd-eeee-4fff-8aaa-bbbbbbbbbbbb.age"
        private const val RUTA_ASSETS = "sincronizacion/" + FICHERO
        private const val CLAVE =
            "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS"
        private const val CARPETA = "content://prueba/formato3"
        private const val CARPETA_PRUEBA = "magi-prueba-formato3"
    }

    private lateinit var contexto: Context
    private lateinit var raiz: File
    private val llavero = LlaveroEnMemoria()
    private var nucleo: Nucleo? = null

    @Before
    fun preparar() {
        contexto = InstrumentationRegistry.getInstrumentation().targetContext
        raiz = File(contexto.filesDir, CARPETA_PRUEBA)
        raiz.deleteRecursively()
        raiz.mkdirs()
        llavero.limpiar()
    }

    @After
    fun limpiar() {
        runCatching { nucleo?.destroy() }
        nucleo = null
        raiz.deleteRecursively()
    }

    @Test
    fun leeElFormatoTresYDejaElEquipoAlDia() = runBlocking {
        nucleo = Nucleo.iniciar(
            rutas = rutasPrivadas(),
            equipo = Equipo(nombre = "telefono-prueba", plataforma = "android"),
            retrollamadas = Retrollamadas(
                carpeta = CarpetaDeAssets(),
                llavero = llavero,
                firmante = FirmanteSinKeystore(),
                oyente = OyenteDePrueba(),
            ),
        )

        // El núcleo declara el formato máximo 3 y la migración 9 (Fase 13).
        val version = nucleo!!.version()
        assertEquals(3, version.formato.toInt())
        assertEquals(9, version.userVersion.toInt())

        // Importa la fixture del formato 3 (lleva host y comprobación propia).
        nucleo!!.elegirCarpeta(CARPETA)
        nucleo!!.unirConClave(CLAVE)

        val hosts = nucleo!!.inventarioHosts(null)
        assertTrue(
            "el host del fichero del formato 3 debe importarse: " + hosts,
            hosts.any { it.nombre == "web-01" },
        )
        val estado = nucleo!!.sincronizacionEstado()
        assertEquals(FaseSincronizacion.ACTIVA, estado.fase)
        assertTrue(
            "el formato 3 no puede quedar como formato nuevo: " + estado.equipos,
            estado.equipos.none { it.estado == EstadoEquipo.FORMATO_NUEVO },
        )
        assertTrue(
            "el otro equipo debe quedar al día: " + estado.equipos,
            estado.equipos.any { it.estado == EstadoEquipo.AL_DIA },
        )
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

    /** Lista y lee la fixture del formato 3 empaquetada en los assets de la prueba. */
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
        fun limpiar() {
            datos.clear()
        }
    }

    /** La prueba no firma: el núcleo tolera que aún no haya clave. */
    private class FirmanteSinKeystore : FirmanteMovil {

        private fun error(): ErrorNucleo =
            ErrorNucleo.Firmante("la prueba del formato 3 no usa la clave del móvil")

        override fun clavePublicaX509(): ByteArray = throw error()

        override fun firmar(datos: ByteArray): ByteArray = throw error()

        override fun olvidarClave() = Unit
    }

    /** El núcleo avisa del estado de sincronización; esta prueba no pinta nada. */
    private class OyenteDePrueba : OyenteNucleo {

        override fun pantalla(pantalla: Pantalla) = Unit

        override fun sesiones(lista: List<Sesion>) = Unit

        override fun tuneles(lista: List<TunelVivo>) = Unit

        override fun dialogo(peticion: PeticionDialogo) = Unit

        override fun flota(lista: List<HostFlota>) = Unit

        override fun accionesServicio(lista: List<AccionServicioMovil>) = Unit

        override fun accionServicioHecha(resultado: ResultadoServicioMovil) = Unit

        override fun logs(estado: EstadoLogs) = Unit

        override fun ejecuciones(lista: List<EjecucionMovil>) = Unit

        override fun sincronizacion(estado: EstadoSincronizacion) = Unit
    }
}
