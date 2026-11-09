package org.cuatrodetres.magi

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.DecisionDeliberacion
import org.cuatrodetres.magi.nucleo.EstadoDeliberacion
import org.cuatrodetres.magi.nucleo.EstadoHostEjecucionMovil
import org.cuatrodetres.magi.nucleo.PlanDeliberadoMovil
import org.cuatrodetres.magi.nucleo.ResultadoEjecucionMovil
import org.cuatrodetres.magi.nucleo.TipoRegistroFiltro
import org.cuatrodetres.magi.nucleo.VariableSnippetMovil
import org.cuatrodetres.magi.nucleo.VeredictoMovil
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Prueba instrumentada de Snippets (Fase 14, T95 y D283), sin interfaz: el
 * núcleo FFI de verdad contra el servidor de pruebas del anfitrión.
 *
 * Cubre lo que pide la tarea:
 * - snippets() y snippets_de_host() de la fixture (ver-estado, reiniciar nginx
 *   y limpiar journald con su variable);
 * - preparar_ejecucion con la variable dias y ejecutar_snippet en web-01, con
 *   su salida y su anotación en REGISTRO;
 * - preparar y ejecutar «reiniciar nginx» en dos hosts: CASPER-3 rechaza en
 *   db-01 («solo en el escritorio»), se fuerza con motivo y la ejecución
 *   termina bien en los dos.
 *
 * Sin servidor de pruebas, la prueba se omite sola. La fixture se regenera con:
 *   cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored
 */
@RunWith(AndroidJUnit4::class)
class PruebaSnippets {

    private lateinit var entorno: EntornoDePrueba

    @Before
    fun preparar() {
        comprobarServidorDePruebas()
        entorno = EntornoDePrueba("snippets", "flota/" + FICHERO_FIXTURE_FLOTA)
        entorno.preparar()
        entorno.iniciar()
        runBlocking {
            entorno.importarFixture()
            entorno.nucleo!!.aplicarAjustes(ajustesDePrueba())
        }
    }

    @After
    fun limpiar() {
        entorno.cerrar()
    }

    @Test
    fun preparaConVariableYEjectaEnDosHosts() = runBlocking {
        val nucleo = entorno.nucleo!!

        // 1. Inventario de snippets de la fixture (solo lectura en el móvil).
        val todos = nucleo.snippets(null)
        assertEquals("snippets de la fixture: " + todos.map { it.nombre }, 3, todos.size)
        assertTrue(
            "faltan snippets de la fixture: " + todos.map { it.nombre },
            todos.map { it.nombre }.containsAll(
                listOf("ver-estado", "reiniciar nginx", "limpiar journald"),
            ),
        )
        val deWeb = nucleo.snippetsDeHost(UUID_WEB).map { it.nombre }
        assertTrue("web-01 debería tener ver-estado: " + deWeb, deWeb.contains("ver-estado"))
        assertTrue("web-01 debería tener reiniciar nginx: " + deWeb, deWeb.contains("reiniciar nginx"))
        val journald = nucleo.snippet(UUID_SNIPPET_JOURNALD)
        assertEquals("limpiar journald", journald.nombre)
        val variable = journald.variables.firstOrNull { it.nombre == "dias" }
        assertNotNull("limpiar journald debería tener la variable dias: " + journald.variables, variable)
        // El comando de la fixture es journalctl --vacuum-time={{dias:7}}d: la
        // variable debe existir con su valor por defecto 7 (la prueba pasa luego
        // un valor explícito al preparar la ejecución).
        val dias = requireNotNull(variable) {
            "limpiar journald debería tener la variable dias: " + journald.variables
        }
        assertEquals("el valor por defecto de dias", "7", dias.valorDefecto)

        // 2. Preparar con la variable sustituida (y escapada) y ejecutar en web-01.
        val preparada = nucleo.prepararEjecucion(
            UUID_SNIPPET_JOURNALD,
            listOf(UUID_WEB),
            listOf(VariableSnippetMovil(nombre = "dias", valorDefecto = "7", valor = "3")),
        )
        assertEquals("no debería haber errores: " + preparada.errores, 0, preparada.errores.size)
        assertEquals(1, preparada.hosts.size)
        assertTrue(
            "el comando debía llevar la variable sustituida: " + preparada.comando,
            preparada.comando.contains("--vacuum-time=3d"),
        )
        assertTrue("con verificaciones activas debe deliberar: " + preparada.motivos, preparada.delibera)

        // En web-01 el vigilado copia.service está fallido: MELCHIOR-1 rechaza
        // y la deliberación queda BLOQUEADA; se fuerza con motivo.
        val tablaJournald = nucleo.deliberar(PlanDeliberadoMovil.Snippet(preparada))
        assertEquals(
            "la deliberación de journald debería bloquearse: " + tablaJournald.filas,
            EstadoDeliberacion.BLOQUEADO,
            tablaJournald.estado,
        )
        val filaJournaldWeb = tablaJournald.filas.first { it.hostUuid == UUID_WEB }
        assertTrue(
            "MELCHIOR-1 debía rechazar por copia.service: " + filaJournaldWeb.salud,
            filaJournaldWeb.salud is VeredictoMovil.Rechaza &&
                (filaJournaldWeb.salud as VeredictoMovil.Rechaza).detalle.contains("copia.service"),
        )
        val forzadaJournald = nucleo.resolverDeliberacion(
            tablaJournald.id,
            DecisionDeliberacion.Forzar(motivo = "prueba instrumentada de journald"),
        )
        val ejecucionJournald = nucleo.ejecutarSnippet(preparada, false, forzadaJournald)
        esperarHasta("la ejecución de limpiar journald termina") {
            nucleo.ejecuciones().any { it.id == ejecucionJournald && it.terminadaEn != null }
        }
        val filaJournald = nucleo.ejecuciones().first { it.id == ejecucionJournald }
        assertEquals(
            "limpiar journald debía terminar bien: " + filaJournald,
            ResultadoEjecucionMovil.OK,
            filaJournald.resultado,
        )
        assertEquals(1, filaJournald.hosts.count { it.estado == EstadoHostEjecucionMovil.OK })
        val salidaJournald = nucleo.salidaEjecucion(ejecucionJournald, UUID_WEB)
        assertEquals(UUID_WEB, salidaJournald.hostUuid)
        assertEquals("código de salida de journald", 0, salidaJournald.codigo)
        assertTrue(
            "el snippet ejecutado debe contar como uso: " + nucleo.snippet(UUID_SNIPPET_JOURNALD).usadoVeces,
            nucleo.snippet(UUID_SNIPPET_JOURNALD).usadoVeces >= 1u,
        )
        esperarHasta("el snippet ejecutado queda anotado en el registro") {
            entradasDelRegistro(nucleo, UUID_WEB, TipoRegistroFiltro.SNIPPETS)
                .any { it.tipo == "snippet_ejecutado" }
        }

        // 3. «reiniciar nginx» en dos hosts: crítico y en dos hosts -> delibera;
        // CASPER-3 rechaza en db-01 («solo en el escritorio»).
        val preparadaNginx = nucleo.prepararEjecucion(
            UUID_SNIPPET_NGINX,
            listOf(UUID_WEB, UUID_DB),
            emptyList(),
        )
        assertEquals("debería apuntar a dos hosts: " + preparadaNginx.hosts, 2, preparadaNginx.hosts.size)
        assertTrue("reiniciar nginx es crítico: " + preparadaNginx.motivos, preparadaNginx.delibera)
        assertTrue(
            "falta el motivo de crítico: " + preparadaNginx.motivos,
            preparadaNginx.motivos.any { it.contains("crítico", ignoreCase = true) },
        )
        assertTrue(
            "falta el motivo de dos hosts: " + preparadaNginx.motivos,
            preparadaNginx.motivos.any { it.contains("2 hosts") },
        )
        val tablaNginx = nucleo.deliberar(PlanDeliberadoMovil.Snippet(preparadaNginx))
        assertEquals(
            "CASPER-3 debe bloquear la deliberación: " + tablaNginx.filas,
            EstadoDeliberacion.BLOQUEADO,
            tablaNginx.estado,
        )
        assertFalse("una deliberación bloqueada no es unánime", tablaNginx.unanime)
        val filaDb = tablaNginx.filas.first { it.hostUuid == UUID_DB }
        assertTrue(
            "CASPER-3 en el móvil debe rechazar: " + filaDb.tests,
            filaDb.tests is VeredictoMovil.Rechaza,
        )
        assertTrue(
            "el rechazo debe decir «solo en el escritorio»: " + filaDb.tests,
            (filaDb.tests as VeredictoMovil.Rechaza).detalle.contains("solo en el escritorio"),
        )

        // 4. Forzar con motivo y ejecutar en los dos hosts.
        val motivo = "prueba instrumentada de extremo a extremo"
        val forzada = nucleo.resolverDeliberacion(
            tablaNginx.id,
            DecisionDeliberacion.Forzar(motivo = motivo),
        )
        val ejecucionNginx = nucleo.ejecutarSnippet(preparadaNginx, false, forzada)
        esperarHasta("la ejecución de reiniciar nginx termina") {
            nucleo.ejecuciones().any { it.id == ejecucionNginx && it.terminadaEn != null }
        }
        val filaNginx = nucleo.ejecuciones().first { it.id == ejecucionNginx }
        assertEquals(
            "las dos ejecuciones debían ir bien: " + filaNginx,
            ResultadoEjecucionMovil.OK,
            filaNginx.resultado,
        )
        assertEquals(
            "los dos hosts debían terminar OK: " + filaNginx.hosts,
            2,
            filaNginx.hosts.count { it.estado == EstadoHostEjecucionMovil.OK },
        )
        assertTrue("la ejecución debía quedar marcada como forzada: " + filaNginx, filaNginx.forzada)
        assertEquals(forzada, filaNginx.deliberacionId)
        val salidaWeb = nucleo.salidaEjecucion(ejecucionNginx, UUID_WEB)
        assertEquals(0, salidaWeb.codigo)
        val salidaDb = nucleo.salidaEjecucion(ejecucionNginx, UUID_DB)
        assertEquals(0, salidaDb.codigo)

        // 5. REGISTRO: la deliberación forzada queda anotada con su motivo (se
        // escribe al registrar el resultado de la ejecución).
        esperarHasta("la deliberación forzada queda anotada en el registro") {
            entradasDelRegistro(nucleo, null, TipoRegistroFiltro.DELIBERACIONES)
                .any { it.tipo == "deliberacion_forzada" && it.detalle.contains("prueba instrumentada") }
        }
        val forzadas = entradasDelRegistro(nucleo, null, TipoRegistroFiltro.DELIBERACIONES)
            .filter { it.tipo == "deliberacion_forzada" }
        assertTrue("no hay deliberacion_forzada en el registro", forzadas.isNotEmpty())
        assertTrue(
            "la anotación forzada no lleva el motivo: " + forzadas.map { it.detalle },
            forzadas.any { it.detalle.contains("prueba instrumentada") },
        )
        esperarHasta("el snippet de db-01 queda anotado en el registro") {
            entradasDelRegistro(nucleo, UUID_DB, TipoRegistroFiltro.SNIPPETS)
                .any { it.tipo == "snippet_ejecutado" }
        }
    }
}
