package org.cuatrodetres.magi.ui.fuente

import kotlinx.coroutines.flow.StateFlow
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EntradaVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAperturaVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.VersionVista

/**
 * Puerta de la interfaz al núcleo (informe §5.1).
 *
 * Las pantallas y los ViewModels no hablan con UniFFI: hablan con esta
 * interfaz. El adaptador real (`nucleo/NucleoRepositorio`, fuera del alcance de
 * la UI) traduce las llamadas a [org.cuatrodetres.magi.nucleo.Nucleo] y las
 * retrollamadas del oyente a los flujos de aquí. Así la UI compila, se prueba
 * en la JVM con dobles y arranca sin el .so nativo.
 *
 * Reglas que respeta el adaptador:
 * - las listas de sesiones y túneles son completas (nunca mensajes finos);
 * - [pantallas] entrega pantallas por tramos, solo las filas cambiadas;
 * - los errores llegan ya en castellano y con instrucción cuando aplica.
 */
interface FuenteNucleo {

    // -----------------------------------------------------------------------
    // Flujos de estado (los alimenta el oyente del núcleo)
    // -----------------------------------------------------------------------

    /** Lista completa de sesiones abiertas, abriendo o caídas. */
    val sesiones: StateFlow<List<SesionVista>>

    /** Lista completa de túneles del inventario con su estado en vivo. */
    val tuneles: StateFlow<List<TunelVista>>

    /** Última pantalla recibida (con `sesion` para saber de cuál es). */
    val pantallas: StateFlow<PantallaVista?>

    /** Petición de diálogo pendiente (huella nueva, cambiada o contraseña). */
    val peticiones: StateFlow<PeticionDialogoVista?>

    /** Estado completo de la sincronización (indicador ⇅ y Ajustes). */
    val sincronizacion: StateFlow<EstadoSincronizacionVista>

    /** Tamaño de letra por defecto del terminal (8-24 sp), persistido. */
    val tamanoLetra: StateFlow<Int>

    // -----------------------------------------------------------------------
    // Arranque e inventario
    // -----------------------------------------------------------------------

    /** Abre o crea magi.db y arranca el núcleo; idempotente. */
    suspend fun iniciar(): ResultadoAccion

    /** Versiones de la app, del núcleo, formato y `user_version`; null si aún no hay núcleo. */
    suspend fun version(): VersionVista?

    /** Grupos y hosts; si `filtro` no es nulo, solo los hosts que lo contienen. */
    suspend fun inventario(filtro: String? = null): InventarioVista

    /** Ficha de solo lectura de un host; null si ya no existe. */
    suspend fun fichaHost(uuid: String): FichaHostVista?

    /** Guarda el plegado propio del grupo (no viaja). */
    suspend fun plegarGrupo(uuid: String, plegado: Boolean): ResultadoAccion

    // -----------------------------------------------------------------------
    // Sincronización
    // -----------------------------------------------------------------------

    /** Última instantánea del estado de sincronización. */
    suspend fun sincronizacionEstado(): EstadoSincronizacionVista

    /** Dispara una pasada en modo lector (si ya hay una en curso, el núcleo la ignora). */
    suspend fun sincronizarAhora(): ResultadoAccion

    /** Guarda el árbol SAF elegido (la app ya tiene el permiso) y comprueba que hay ficheros. */
    suspend fun elegirCarpeta(uri: String): ResultadoAccion

    /** Normaliza y valida la clave, la guarda en el llavero y lanza la primera pasada. */
    suspend fun unirConClave(texto: String): ResultadoAccion

    /** Deja de sincronizar; falla si hay sesiones o túneles abiertos. */
    suspend fun dejarDeSincronizar(): ResultadoAccion

    /** Conflictos de huella pendientes de resolver en el móvil. */
    suspend fun conflictosHuella(): List<ConflictoHuellaVista>

    /** Acepta la huella del otro equipo (sustituye la del known_hosts del móvil). */
    suspend fun aceptarHuellaDeOtroEquipo(id: Long): ResultadoAccion

    /** Mantiene la huella del móvil y descarta la del otro equipo. */
    suspend fun mantenerMiHuella(id: Long): ResultadoAccion

    // -----------------------------------------------------------------------
    // Sesiones y terminal
    // -----------------------------------------------------------------------

    /** Abre una sesión (§4.3); los diálogos llegan por [peticiones]. */
    suspend fun abrirSesion(hostUuid: String): ResultadoAperturaVista

    /** Escribe bytes tal cual en el PTY. */
    suspend fun escribir(sesion: Long, bytes: ByteArray): ResultadoAccion

    /** Codifica una tecla con modificadores en el núcleo (no dupliques §7.6 en Kotlin). */
    suspend fun tecla(sesion: Long, tecla: TeclaVista, modificadores: ModificadoresVista): ResultadoAccion

    /** Pega texto; el núcleo usa el pegado entre corchetes si el remoto lo activó. */
    suspend fun pegar(sesion: Long, texto: String): ResultadoAccion

    /** Si el remoto activó el pegado entre corchetes (modo 2004) en esa sesión. */
    fun pegadoEntreCorchetes(sesion: Long): Boolean

    /** Redimensiona el PTY (el núcleo sanea y agrupa 50 ms). */
    suspend fun redimensionar(sesion: Long, columnas: Int, filas: Int): ResultadoAccion

    /** Desplaza el historial (positivo = hacia arriba). */
    suspend fun desplazar(sesion: Long, filas: Int): ResultadoAccion

    /** Texto entre dos celdas de la pantalla, para copiar (null si no hay sesión). */
    suspend fun textoSeleccion(sesion: Long, desde: PosicionVista, hasta: PosicionVista): String?

    /** Reconecta una sesión caída con el mismo nombre. */
    suspend fun reconectar(sesion: Long): ResultadoAccion

    /** Cierra una sesión y libera sus recursos. */
    suspend fun cerrarSesion(sesion: Long): ResultadoAccion

    // -----------------------------------------------------------------------
    // Túneles
    // -----------------------------------------------------------------------

    /** Túneles del inventario (de un host o todos) con su estado vivo. */
    suspend fun tuneles(hostUuid: String? = null): List<TunelVista>

    suspend fun activarTunel(uuid: String): ResultadoAccion

    suspend fun pararTunel(uuid: String): ResultadoAccion

    suspend fun relanzarTunel(uuid: String): ResultadoAccion

    suspend fun activarEnPuertoLibre(uuid: String): ResultadoAccion

    // -----------------------------------------------------------------------
    // Diálogos, clave del móvil y datos
    // -----------------------------------------------------------------------

    /** Responde a una petición del núcleo. */
    suspend fun responderDialogo(peticionId: Long, respuesta: RespuestaDialogoVista): ResultadoAccion

    /** Línea OpenSSH y huella de la clave del móvil; null si aún no está creada. */
    suspend fun clavePublicaMovil(): ClaveMovilVista?

    /** Instala la clave del móvil en el host (§4.4 y §7.9). */
    suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionVista

    /** Olvida la contraseña guardada de un host. */
    suspend fun olvidarContrasena(hostUuid: String): ResultadoAccion

    /** Borra los datos del móvil: false = solo la sincronización, true = todo. */
    suspend fun borrarDatos(todo: Boolean): ResultadoAccion

    /** Guarda el tamaño de letra del terminal (8-24 sp). */
    suspend fun cambiarTamanoLetra(sp: Int): ResultadoAccion
}

/** Entrada de texto ya interpretada (`EntradaVista`) para el terminal. */
fun EntradaVista.aBytes(): ByteArray? = when (this) {
    is EntradaVista.Texto -> texto.toByteArray(Charsets.UTF_8)
    is EntradaVista.Pulsacion -> null
}
