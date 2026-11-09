package org.cuatrodetres.magi

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.net.InetSocketAddress
import java.net.Socket
import java.security.KeyPairGenerator
import java.security.Signature
import kotlinx.coroutines.delay
import org.cuatrodetres.magi.nucleo.AccionServicioMovil
import org.cuatrodetres.magi.nucleo.AjustesMovil
import org.cuatrodetres.magi.nucleo.CarpetaMovil
import org.cuatrodetres.magi.nucleo.EjecucionMovil
import org.cuatrodetres.magi.nucleo.EntradaRegistro
import org.cuatrodetres.magi.nucleo.Equipo
import org.cuatrodetres.magi.nucleo.EstadoLogs
import org.cuatrodetres.magi.nucleo.EstadoSincronizacion
import org.cuatrodetres.magi.nucleo.FicheroCarpeta
import org.cuatrodetres.magi.nucleo.FiltroRegistro
import org.cuatrodetres.magi.nucleo.FirmanteMovil
import org.cuatrodetres.magi.nucleo.HostFlota
import org.cuatrodetres.magi.nucleo.LlaveroMovil
import org.cuatrodetres.magi.nucleo.Nucleo
import org.cuatrodetres.magi.nucleo.OyenteNucleo
import org.cuatrodetres.magi.nucleo.Pantalla
import org.cuatrodetres.magi.nucleo.PeticionDialogo
import org.cuatrodetres.magi.nucleo.ResultadoServicioMovil
import org.cuatrodetres.magi.nucleo.Retrollamadas
import org.cuatrodetres.magi.nucleo.Rutas
import org.cuatrodetres.magi.nucleo.Sesion
import org.cuatrodetres.magi.nucleo.TipoRegistroFiltro
import org.cuatrodetres.magi.nucleo.TunelVivo
import org.junit.Assert.fail
import org.junit.Assume

/**
 * Apoyo común de las pruebas instrumentadas de la Fase 14 (Flota, Servicios y
 * Snippets) contra el servidor de pruebas del anfitrión:
 *
 *   cargo run -p magi-movil --example servidor_pruebas
 *
 * El emulador llega al anfitrión por 10.0.2.2 (su loopback) y el servidor solo
 * escucha en 127.0.0.1, como manda R107. La fixture es la de Flota
 * (generar_fixture_flota_android.rs) y la clave de host es la fija del
 * servidor de pruebas.
 */

/** Dirección del anfitrión vista desde el emulador. */
const val SERVIDOR_SSH = "10.0.2.2"

/** Puerto SSH del servidor de pruebas (ejemplo servidor_pruebas). */
const val PUERTO_SSH_SERVIDOR = 12222

/** Canal de control del servidor de pruebas (orden «tirar»). */
const val PUERTO_CONTROL_SERVIDOR = 12223

/** Clave pública ed25519 del host de pruebas (la de las fixtures). */
const val CLAVE_HOST_SERVIDOR =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMDUg4hwJY3WuFnQXmPzlo2c28c7JgaJ2zkxyf8OGHJs"

/** Clave de la fixture de Flota (la del generador del núcleo). */
const val CLAVE_FIXTURE_FLOTA =
    "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS"

/** Fichero de la fixture de Flota en androidTest/assets/flota/. */
const val FICHERO_FIXTURE_FLOTA = "magi-cccccccc-dddd-4eee-8fff-000000000000.age"

// UUIDs de sincronización de la fixture de Flota
// (crates/magi-nucleo/tests/generar_fixture_flota_android.rs).
const val UUID_WEB = "22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee"
const val UUID_DB = "33333333-cccc-4ddd-8eee-ffffffffffff"
const val UUID_CAIDO = "44444444-dddd-4eee-8fff-000000000000"
const val UUID_SNIPPET_ESTADO = "55555555-eeee-4fff-8aaa-111111111111"
const val UUID_SNIPPET_NGINX = "66666666-ffff-4aaa-8bbb-222222222222"
const val UUID_SNIPPET_JOURNALD = "77777777-aaaa-4bbb-8ccc-333333333333"

/**
 * Comprueba que el servidor de pruebas del anfitrión está levantado. Sin él,
 * las pruebas instrumentadas de extremo a extremo se omiten solas (como en el
 * E2E del túnel de la Fase 11).
 */
fun comprobarServidorDePruebas() {
    val disponible = try {
        Socket().use { socket ->
            socket.connect(InetSocketAddress(SERVIDOR_SSH, PUERTO_SSH_SERVIDOR), 1_000)
        }
        true
    } catch (_: Exception) {
        false
    }
    Assume.assumeTrue(
        "servidor de pruebas no disponible; arranca " +
            "cargo run -p magi-movil --example servidor_pruebas",
        disponible,
    )
}

/** Espera (con plazo) a que se cumpla la condición. Nunca duerme a ciegas. */
suspend fun esperarHasta(
    descripcion: String,
    plazoMs: Long = 30_000L,
    intervaloMs: Long = 100L,
    condicion: () -> Boolean,
) {
    val limite = System.currentTimeMillis() + plazoMs
    while (System.currentTimeMillis() < limite) {
        if (condicion()) return
        delay(intervaloMs)
    }
    fail("no se cumplió en " + plazoMs + " ms: " + descripcion)
}

/**
 * Ajustes del móvil para las pruebas: sin refresco automático y con umbrales
 * de carga, memoria y disco holgados a propósito, para que el estado de Flota
 * lo decidan las unidades vigiladas y no la carga del anfitrión de CI.
 */
fun ajustesDePrueba(): AjustesMovil = AjustesMovil(
    flotaRefrescoSeg = 0uL,
    umbralCargaPorNucleo = 16.0,
    umbralMemoriaPct = 100.0,
    umbralDiscoPct = 100.0,
    serviciosRefrescoSeg = 0uL,
    serviciosEsperaSeg = 30uL,
    serviciosContenedores = listOf("docker"),
    sudoMemoriaMin = 5uL,
    deliberacionBackupHoras = 24uL,
    deliberacionSaludMaxMin = 5uL,
)

/** Conexiones que el móvil abrió con ese host (evento conexion_abierta). */
fun contarConexionesAbiertas(nucleo: Nucleo, hostUuid: String): Int =
    entradasDelRegistro(nucleo, hostUuid, TipoRegistroFiltro.CONEXIONES)
        .count { it.tipo == "conexion_abierta" }

/** Entradas del registro de un host y un grupo de tipos (la página más nueva). */
fun entradasDelRegistro(
    nucleo: Nucleo,
    hostUuid: String?,
    tipo: TipoRegistroFiltro,
): List<EntradaRegistro> = nucleo.registro(
    FiltroRegistro(hostUuid = hostUuid, tipos = listOf(tipo), texto = ""),
    null,
    50u,
)

/**
 * Entorno aislado de una prueba: raíz privada con sus rutas, known_hosts con la
 * clave del servidor de pruebas, dobles de plataforma y una instancia del núcleo.
 */
class EntornoDePrueba(nombre: String, private val recurso: String?) {
    val contexto = InstrumentationRegistry.getInstrumentation().targetContext
    val raiz = File(contexto.filesDir, "magi-prueba-" + nombre)
    val llavero = LlaveroEnMemoria()
    val oyente = OyenteDePrueba()
    var nucleo: Nucleo? = null
        private set

    /** Deja la raíz limpia y el known_hosts con la clave del servidor. */
    fun preparar() {
        raiz.deleteRecursively()
        raiz.mkdirs()
        llavero.limpiar()
        val ssh = File(raiz, "hogar/.ssh").apply { mkdirs() }
        File(ssh, "known_hosts").writeText(
            "[" + SERVIDOR_SSH + "]:" + PUERTO_SSH_SERVIDOR + " " + CLAVE_HOST_SERVIDOR +
                System.lineSeparator(),
        )
    }

    /** Arranca el núcleo con los dobles de plataforma de la prueba. */
    fun iniciar(): Nucleo {
        val creado = Nucleo.iniciar(
            rutas = rutasPrivadas(),
            equipo = Equipo(nombre = "telefono-prueba", plataforma = "android"),
            retrollamadas = Retrollamadas(
                carpeta = CarpetaDePrueba(recurso),
                llavero = llavero,
                firmante = FirmanteClaveBlanda(),
                oyente = oyente,
            ),
        )
        nucleo = creado
        return creado
    }

    /** Importa la fixture .age de androidTest/assets con su clave. */
    suspend fun importarFixture(clave: String = CLAVE_FIXTURE_FLOTA) {
        val actual = nucleo ?: error("el núcleo no está iniciado")
        actual.elegirCarpeta("content://prueba/flota")
        actual.unirConClave(clave)
    }

    /** Suelta el núcleo y borra la raíz privada. */
    fun cerrar() {
        runCatching { nucleo?.destroy() }
        nucleo = null
        raiz.deleteRecursively()
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
}

/** Carpeta SAF doble: un único fichero .age empaquetado en los assets. */
class CarpetaDePrueba(private val recurso: String?) : CarpetaMovil {

    private fun contenido(): ByteArray {
        val ruta = recurso ?: error("esta prueba no usa fixture")
        return InstrumentationRegistry.getInstrumentation().context.assets
            .open(ruta)
            .use { it.readBytes() }
    }

    override fun listar(): List<FicheroCarpeta> {
        if (recurso == null) return emptyList()
        val datos = contenido()
        return listOf(
            FicheroCarpeta(
                nombre = recurso.substringAfterLast('/'),
                tamano = datos.size.toULong(),
                modificadoEn = 1_700_000_000L,
            ),
        )
    }

    override fun leer(nombre: String, maximo: ULong): ByteArray {
        check(nombre == recurso?.substringAfterLast('/')) { "nombre inesperado: " + nombre }
        val datos = contenido()
        check(datos.size.toULong() <= maximo) { "la fixture supera el máximo" }
        return datos
    }
}

/** Llavero del móvil en memoria: la prueba no toca el Keystore. */
class LlaveroEnMemoria : LlaveroMovil {

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

    @Synchronized
    fun cuentas(): Set<String> = datos.keys.toSet()
}

/** Clave EC P-256 en software: el servidor de pruebas no verifica la firma. */
class FirmanteClaveBlanda : FirmanteMovil {

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

/** Oyente del núcleo que guarda las listas y avisos que miran las pruebas. */
class OyenteDePrueba : OyenteNucleo {

    @Volatile
    var avisosFlota: Int = 0
        private set

    @Volatile
    var ultimaFlota: List<HostFlota> = emptyList()
        private set

    @Volatile
    var ultimasAcciones: List<AccionServicioMovil> = emptyList()
        private set

    @Volatile
    var ultimasEjecuciones: List<EjecucionMovil> = emptyList()
        private set

    val accionesHechas: MutableList<ResultadoServicioMovil> =
        java.util.Collections.synchronizedList(mutableListOf())

    val avisosLogs: MutableList<EstadoLogs> =
        java.util.Collections.synchronizedList(mutableListOf())

    val dialogos: MutableList<PeticionDialogo> =
        java.util.Collections.synchronizedList(mutableListOf())

    @Synchronized
    override fun pantalla(pantalla: Pantalla) = Unit

    @Synchronized
    override fun sesiones(lista: List<Sesion>) = Unit

    @Synchronized
    override fun tuneles(lista: List<TunelVivo>) = Unit

    @Synchronized
    override fun dialogo(peticion: PeticionDialogo) {
        dialogos.add(peticion)
    }

    @Synchronized
    override fun sincronizacion(estado: EstadoSincronizacion) = Unit

    @Synchronized
    override fun flota(lista: List<HostFlota>) {
        avisosFlota += 1
        ultimaFlota = lista
    }

    @Synchronized
    override fun accionesServicio(lista: List<AccionServicioMovil>) {
        ultimasAcciones = lista
    }

    @Synchronized
    override fun accionServicioHecha(resultado: ResultadoServicioMovil) {
        accionesHechas.add(resultado)
    }

    @Synchronized
    override fun logs(estado: EstadoLogs) {
        avisosLogs.add(estado)
    }

    @Synchronized
    override fun ejecuciones(lista: List<EjecucionMovil>) {
        ultimasEjecuciones = lista
    }
}
