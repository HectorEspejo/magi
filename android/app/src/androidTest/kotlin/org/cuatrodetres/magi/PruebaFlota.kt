package org.cuatrodetres.magi

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.EstadoFlotaMovil
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Prueba instrumentada de Flota (Fase 14, T95 y D283), sin interfaz: importa
 * la fixture de Flota con el núcleo FFI de verdad y sondea el servidor de
 * pruebas del anfitrión (10.0.2.2:12222).
 *
 * Comprueba lo que pide la tarea:
 * - web-01 queda en CARGA con copia.service como culpable (vigilado fallido);
 * - db-01 queda NOMINAL;
 * - caido queda CAÍDA con su motivo;
 * - los SONDEOS se guardan y el fallo deja anotación sondeo_fallido;
 * - un segundo sondeo reutiliza la conexión del host: no hay otra
 *   autenticación (conexion_abierta) dentro de la gracia de 60 s.
 *
 * Sin servidor de pruebas, la prueba se omite sola. La fixture se regenera con:
 *   cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored
 */
@RunWith(AndroidJUnit4::class)
class PruebaFlota {

    private lateinit var entorno: EntornoDePrueba

    @Before
    fun preparar() {
        comprobarServidorDePruebas()
        entorno = EntornoDePrueba("flota", "flota/" + FICHERO_FIXTURE_FLOTA)
        entorno.preparar()
        entorno.iniciar()
        runBlocking { entorno.importarFixture() }
    }

    @After
    fun limpiar() {
        entorno.cerrar()
    }

    @Test
    fun sondeaClasificaYReutilizaLaConexion() = runBlocking {
        val nucleo = entorno.nucleo!!
        val avisos = nucleo.aplicarAjustes(ajustesDePrueba())
        assertEquals("los ajustes de la prueba deberían ser válidos: " + avisos, 0, avisos.size)

        // Sondeo completo: los tres hosts de la fixture son SSH (el caído apunta
        // a un puerto cerrado del anfitrión).
        nucleo.sondear(listOf())
        esperarHasta("los tres hosts terminan su sondeo") {
            val filas = nucleo.flota(null)
            filas.size >= 3 && filas.none {
                it.estado == EstadoFlotaMovil.SONDEANDO || it.estado == EstadoFlotaMovil.FRIA
            }
        }

        val porUuid = nucleo.flota(null).associateBy { it.uuid }
        val web = porUuid.getValue(UUID_WEB)
        val db = porUuid.getValue(UUID_DB)
        val caido = porUuid.getValue(UUID_CAIDO)

        // web-01: la unidad vigilada copia.service está fallida -> CARGA.
        assertEquals("web-01 debería estar en CARGA: " + web, EstadoFlotaMovil.CARGA, web.estado)
        assertTrue(
            "copia.service debería ser culpable de web-01: " + web.culpables,
            web.culpables.any { it.contains("copia.service") },
        )
        assertTrue("web-01 sin fallos contados: " + web.fallos, web.fallos >= 1u)
        assertNotNull("web-01 sin resumen de métricas", web.resumen)
        assertNotNull("web-01 sin hora de sondeo", web.ultimoSondeoEn)

        // db-01: postgresql.service está activa y los umbrales son holgados.
        assertEquals("db-01 debería ser NOMINAL: " + db, EstadoFlotaMovil.NOMINAL, db.estado)
        assertNotNull("db-01 sin resumen de métricas", db.resumen)
        assertNotNull("db-01 sin hora de sondeo", db.ultimoSondeoEn)

        // caido: el puerto del anfitrión no escucha -> CAÍDA con motivo.
        assertEquals("caido debería estar CAÍDA: " + caido, EstadoFlotaMovil.CAIDA, caido.estado)
        assertTrue(
            "caido debería traer el motivo: " + caido.motivo,
            !caido.motivo.isNullOrBlank(),
        )
        assertNotNull("caido sin hora de sondeo", caido.ultimoSondeoEn)

        // SONDEOS guardados y fallo anotado en el registro del móvil.
        assertTrue(
            "no hay ninguna anotación sondeo_fallido del caido",
            entradasDelRegistro(nucleo, UUID_CAIDO, org.cuatrodetres.magi.nucleo.TipoRegistroFiltro.SONDEOS)
                .isNotEmpty(),
        )
        assertEquals("la lista de Flota por el oyente no llegó", true, entorno.oyente.ultimaFlota.isNotEmpty())

        // La primera conexión de cada host se abrió (una por host, D277).
        val conexionesWebAntes = contarConexionesAbiertas(nucleo, UUID_WEB)
        val conexionesDbAntes = contarConexionesAbiertas(nucleo, UUID_DB)
        assertTrue(
            "web-01 no abrió conexión en el primer sondeo: " + conexionesWebAntes,
            conexionesWebAntes >= 1,
        )
        assertTrue(
            "db-01 no abrió conexión en el primer sondeo: " + conexionesDbAntes,
            conexionesDbAntes >= 1,
        )

        // Segundo sondeo dentro de la gracia de 60 s: misma conexión, sin
        // reautenticar (no debe aparecer otro conexion_abierta).
        val avisosAntes = entorno.oyente.avisosFlota
        nucleo.sondear(listOf(UUID_WEB, UUID_DB))
        esperarHasta(
            "el segundo sondeo termina (dos avisos de Flota por host)",
        ) { entorno.oyente.avisosFlota >= avisosAntes + 4 }
        delay(300)
        assertEquals(
            "el segundo sondeo reautenticó web-01 dentro de la gracia",
            conexionesWebAntes,
            contarConexionesAbiertas(nucleo, UUID_WEB),
        )
        assertEquals(
            "el segundo sondeo reautenticó db-01 dentro de la gracia",
            conexionesDbAntes,
            contarConexionesAbiertas(nucleo, UUID_DB),
        )
        assertEquals(
            "web-01 debería seguir en CARGA",
            EstadoFlotaMovil.CARGA,
            nucleo.flota(null).first { it.uuid == UUID_WEB }.estado,
        )
    }
}
