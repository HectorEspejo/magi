package org.cuatrodetres.magi

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.cuatrodetres.magi.nucleo.AccionMovil
import org.cuatrodetres.magi.nucleo.DecisionDeliberacion
import org.cuatrodetres.magi.nucleo.EstadoContenedorMovil
import org.cuatrodetres.magi.nucleo.EstadoDeliberacion
import org.cuatrodetres.magi.nucleo.EstadoSuscripcionLog
import org.cuatrodetres.magi.nucleo.FiltroLogs
import org.cuatrodetres.magi.nucleo.MotorEstadoMovil
import org.cuatrodetres.magi.nucleo.MotorNombreMovil
import org.cuatrodetres.magi.nucleo.NivelLogMovil
import org.cuatrodetres.magi.nucleo.ObjetivoMovil
import org.cuatrodetres.magi.nucleo.PlanDeliberadoMovil
import org.cuatrodetres.magi.nucleo.PrivilegioMovil
import org.cuatrodetres.magi.nucleo.ResultadoAccionMovil
import org.cuatrodetres.magi.nucleo.ResultadoSudo
import org.cuatrodetres.magi.nucleo.SystemdMovil
import org.cuatrodetres.magi.nucleo.TipoObjetivoMovil
import org.cuatrodetres.magi.nucleo.VentanaLogs
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
 * Prueba instrumentada de Servicios (Fase 14, T95 y D283), sin interfaz: el
 * núcleo FFI de verdad contra el servidor de pruebas del anfitrión, con sus
 * binarios falsos (systemd, Docker, sudo y nginx).
 *
 * Cubre lo que pide la tarea:
 * - servicios_listar de web-01: systemd, unidades, timer, contenedores y
 *   proyecto Compose, con las unidades fallidas primero;
 * - detalle de nginx.service y consumo de los contenedores;
 * - preparar_accion y servicio_accion de reiniciar nginx, con sudo «secreta» y
 *   deliberación aprobada;
 * - logs_seguir, logs_ventana y logs_parar sobre el diario de app.service.
 *
 * Sin servidor de pruebas, la prueba se omite sola. La fixture se regenera con:
 *   cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored
 */
@RunWith(AndroidJUnit4::class)
class PruebaServicios {

    private lateinit var entorno: EntornoDePrueba

    @Before
    fun preparar() {
        comprobarServidorDePruebas()
        entorno = EntornoDePrueba("servicios", "flota/" + FICHERO_FIXTURE_FLOTA)
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
    fun listaDetallaYReiniciaNginxConLogs() = runBlocking {
        val nucleo = entorno.nucleo!!
        val web = UUID_WEB
        val nginx = ObjetivoMovil(TipoObjetivoMovil.UNIDAD, "nginx.service")

        // 1. Listado: systemd, unidades, timer, contenedores y proyecto.
        val listado = nucleo.serviciosListar(web, false)
        assertTrue(
            "systemd no disponible: " + listado.systemd,
            listado.systemd is SystemdMovil.Disponible,
        )
        assertEquals(
            "versión de systemd: " + listado.systemd,
            "252",
            (listado.systemd as SystemdMovil.Disponible).v1,
        )
        assertEquals(PrivilegioMovil.SUDO_SIN_CLAVE, listado.privilegio)
        assertTrue("sin motor de contenedores: " + listado.motor, listado.motor is MotorEstadoMovil.Disponible)
        assertEquals(
            MotorNombreMovil.DOCKER,
            (listado.motor as MotorEstadoMovil.Disponible).v1.nombre,
        )
        val nombres = listado.unidades.map { it.nombre }
        listOf("app.service", "copia.service", "nginx.service", "postgresql.service").forEach {
            assertTrue("falta " + it + " en " + nombres, nombres.contains(it))
        }
        assertEquals(
            "las unidades fallidas van primero: " + nombres,
            "copia.service",
            listado.unidades.first().nombre,
        )
        val copia = listado.unidades.first { it.nombre == "copia.service" }
        assertEquals(
            "copia.service debería estar fallida: " + copia.props,
            "failed",
            copia.props.firstOrNull { it.clave == "ActiveState" }?.valor,
        )
        assertTrue("falta app.timer", listado.timers.any { it.nombre == "app.timer" })
        assertEquals("debería haber dos contenedores: " + listado.contenedores, 2, listado.contenedores.size)
        val webApp = listado.contenedores.first { it.nombre == "web-app-1" }
        assertEquals(EstadoContenedorMovil.EN_MARCHA, webApp.estado)
        val panel = listado.contenedores.first { it.nombre == "panel" }
        assertEquals(EstadoContenedorMovil.PARADO, panel.estado)
        assertEquals("panel debía salir con código 1: " + panel, 1, panel.codigoSalida)
        assertTrue(
            "falta el proyecto web: " + listado.proyectos,
            listado.proyectos.any { it.nombre == "web" },
        )
        assertTrue("sin contraseña de sudo en memoria al empezar", !listado.sudoEnMemoria)

        // 2. Detalle de una unidad.
        val detalle = nucleo.serviciosDetalle(web, nginx)
        assertEquals(nginx, detalle.objetivo)
        assertEquals(
            "nginx.service debería estar activa: " + detalle.props,
            "active",
            detalle.props.firstOrNull { it.clave == "ActiveState" }?.valor,
        )

        // 3. Consumo: el servidor de pruebas no da estadísticas de contenedores.
        val consumo = nucleo.serviciosConsumo(web)
        assertTrue("el consumo debía venir vacío: " + consumo, consumo.isEmpty())

        // 4. La contraseña de sudo «secreta» que acepta el servidor de pruebas.
        assertEquals(ResultadoSudo.OK, nucleo.sudoComprobar(web, "secreta".toByteArray()))

        // 5. Preparar la acción: reiniciar nginx delibera y comprueba nginx -t.
        val info = nucleo.prepararAccion(web, nginx, AccionMovil.REINICIAR)
        assertTrue("reiniciar nginx no se permite: " + info.motivo, info.permitida)
        assertTrue("reiniciar nginx debería deliberar: " + info, info.delibera)
        assertFalse("con sudo sin clave no debe pedir contraseña: " + info, info.pideSudo)
        val comprobacion = info.comprobacion
        assertNotNull("reiniciar nginx debería comprobar la configuración: " + info, comprobacion)
        assertTrue(
            "la comprobación debía ser nginx -t o la propia: " + comprobacion,
            comprobacion!!.contains("nginx") || comprobacion.contains("propia"),
        )

        // 6. Deliberación: en web-01 el vigilado copia.service está fallido, así
        // que MELCHIOR-1 rechaza con CARGA; CASPER-3 no está activo y
        // BALTHASAR-2 aprueba el backup reciente. Queda BLOQUEADA y se fuerza
        // con motivo, como en el guion manual.
        val tabla = nucleo.deliberar(
            PlanDeliberadoMovil.Servicio(web, nginx, AccionMovil.REINICIAR, info.comprobacion),
        )
        assertEquals("debería estar BLOQUEADA por copia.service: " + tabla.filas, EstadoDeliberacion.BLOQUEADO, tabla.estado)
        assertFalse("una deliberación bloqueada no es unánime", tabla.unanime)
        val fila = tabla.filas.first { it.hostUuid == web }
        assertTrue("MELCHIOR-1 debía rechazar: " + fila.salud, fila.salud is VeredictoMovil.Rechaza)
        assertTrue(
            "el rechazo de MELCHIOR-1 debía nombrar copia.service: " + fila.salud,
            (fila.salud as VeredictoMovil.Rechaza).detalle.contains("copia.service"),
        )
        assertTrue("BALTHASAR-2 debía aprobar: " + fila.backup, fila.backup is VeredictoMovil.Aprueba)
        assertEquals("CASPER-3 no está activo en web-01", VeredictoMovil.NoActiva, fila.tests)
        val deliberacion = nucleo.resolverDeliberacion(
            tabla.id,
            DecisionDeliberacion.Forzar(motivo = "prueba instrumentada de servicios"),
        )

        // 7. La acción: reiniciar nginx con la comprobación y el resultado hecho.
        val resultado = nucleo.servicioAccion(web, nginx, AccionMovil.REINICIAR, deliberacion)
        assertEquals(
            "la acción debía terminar bien: " + resultado,
            ResultadoAccionMovil.HECHO,
            resultado.resultado,
        )
        assertEquals("active", resultado.estado)
        assertEquals(web, resultado.hostUuid)
        assertNotNull("la acción debía llevar la comprobación", resultado.comprobacion)
        assertTrue(
            "la acción no avisó al oyente: " + entorno.oyente.accionesHechas,
            entorno.oyente.accionesHechas.any {
                it.hostUuid == web && it.accion == AccionMovil.REINICIAR &&
                    it.resultado == ResultadoAccionMovil.HECHO
            },
        )
        assertTrue(
            "no hay anotación servicio_accion en el registro",
            entradasDelRegistro(nucleo, web, org.cuatrodetres.magi.nucleo.TipoRegistroFiltro.SERVICIOS)
                .any { it.tipo == "servicio_accion" },
        )

        // 8. Logs en vivo del diario de app.service: dos líneas iniciales, la
        // ventana filtrada y la parada de la suscripción.
        val suscripcion = nucleo.logsSeguir(
            web,
            ObjetivoMovil(TipoObjetivoMovil.UNIDAD, "app.service"),
            2u,
        )
        val filtro = FiltroLogs(
            texto = "",
            nivel = NivelLogMovil.TODO,
            soloStderr = false,
            desde = null,
            cuantas = 50u,
        )
        var ventana: VentanaLogs = nucleo.logsVentana(suscripcion, filtro)
        esperarHasta("llegan las dos líneas iniciales del diario") {
            ventana = nucleo.logsVentana(suscripcion, filtro)
            ventana.lineas.size >= 2
        }
        val textos = ventana.lineas.map { it.texto }
        assertTrue("no llegó «arrancando app»: " + textos, textos.any { it.contains("arrancando app") })
        assertTrue("no llegó «app en marcha»: " + textos, textos.any { it.contains("app en marcha") })
        nucleo.logsParar(suscripcion)
        esperarHasta("la suscripción de logs queda parada") {
            nucleo.logsVentana(suscripcion, filtro).estado == EstadoSuscripcionLog.PARADA
        }
        assertTrue(
            "el oyente no recibió avisos de logs",
            entorno.oyente.avisosLogs.isNotEmpty(),
        )
    }
}
