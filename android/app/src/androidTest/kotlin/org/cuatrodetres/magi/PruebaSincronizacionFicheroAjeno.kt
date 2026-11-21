package org.cuatrodetres.magi

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.FaseSincronizacion
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Un fichero de la carpeta que no se abre con la clave del móvil (de otra
 * clave, a medio descargar o que no es age) no puede tumbar la sincronización.
 * Desde la corrección 10b (R85) solo «ninguna identidad coincide» pide la clave:
 * un fichero que no es age (o está cortado) es «a medio copiar o dañado», con su
 * aviso y su reintento en la siguiente pasada.
 *
 * Regresión: con el núcleo compilado en debug, age leía sus textos de error de
 * la ruta donde se compiló, que en el móvil no existe, y entraba en pánico
 * (LanguageNotAvailable) al formatear el primer error. La unión y la pasada se
 * interrumpían. En la JVM no se ve: allí esa ruta existe.
 *
 * No necesita el servidor de pruebas: solo importa la fixture de Flota.
 */
@RunWith(AndroidJUnit4::class)
class PruebaSincronizacionFicheroAjeno {

    companion object {
        /** Se lista antes que la fixture: la unión lo prueba primero. */
        private const val FICHERO_AJENO = "magi-eeeeeeee-ffff-4aaa-8bbb-cccccccccccc.age"

        /**
         * Aviso no bloqueante del lector para un fichero que no es age, de un
         * equipo desconocido: dice el nombre del fichero y que se reintenta.
         */
        private const val AVISO =
            FICHERO_AJENO + ": a medio copiar o dañado; se reintenta en la siguiente pasada"
    }

    private lateinit var entorno: EntornoDePrueba

    @Before
    fun preparar() {
        entorno = EntornoDePrueba(
            "fichero-ajeno",
            "flota/" + FICHERO_FIXTURE_FLOTA,
            ajenos = mapOf(FICHERO_AJENO to "esto no es un fichero age".toByteArray()),
        )
        entorno.preparar()
    }

    @After
    fun limpiar() {
        entorno.cerrar()
    }

    @Test
    fun unFicheroQueNoSeAbreNoTumbaLaSincronizacion() = runBlocking {
        val nucleo = entorno.iniciar()
        entorno.importarFixture()

        // La pasada termina y el fichero ajeno queda como aviso, que el estado
        // enseña en `error` a falta de otro error.
        val estado = nucleo.sincronizacionEstado()
        assertEquals(FaseSincronizacion.ACTIVA, estado.fase)
        assertEquals(AVISO, estado.error)
        val hosts = nucleo.inventarioHosts(null)
        assertTrue(
            "la fixture se importa aunque haya un fichero ajeno: " + hosts,
            hosts.any { it.nombre == "web-01" },
        )

        // Una segunda pasada vuelve a encontrarse el fichero ajeno y tampoco falla.
        nucleo.sincronizarAhora()
        val otra = nucleo.sincronizacionEstado()
        assertEquals(FaseSincronizacion.ACTIVA, otra.fase)
        assertEquals(AVISO, otra.error)
    }
}
