package org.cuatrodetres.magi.ui.fuente

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.ConsumoVista
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.DetalleRegistroVista
import org.cuatrodetres.magi.ui.modelos.DetalleServiciosVista
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EstadoLogsVista
import org.cuatrodetres.magi.ui.modelos.FiltroLogsVista
import org.cuatrodetres.magi.ui.modelos.FiltroRegistroVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.OrdenPestanaVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAjustesVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.SalidaEjecucionVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.TAMANO_PAGINA_REGISTRO
import org.cuatrodetres.magi.ui.modelos.TodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.VariableSnippetVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
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
 * - [pantallas] y [peticiones] llegan por flujos con buffer (no se conflan);
 * - [pantallas] entrega tramos, solo las filas cambiadas;
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

    /**
     * Pantallas por tramos en orden; el adaptador las entrega con buffer y no
     * se puede perder ninguna (perder filas descuadraría el terminal).
     */
    val pantallas: Flow<PantallaVista>

    /**
     * Peticiones de diálogo (huella nueva, cambiada o contraseña), en orden y
     * sin conflar: el ViewModel las apila (informe §7.1).
     */
    val peticiones: Flow<PeticionDialogoVista>

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

    /**
     * Crea una clave del móvil nueva en el Keystore cuando Android invalidó la
     * anterior (CA 31). Devuelve true si la clave queda lista para firmar. La
     * implementa el adaptador real (FuenteNucleoReal); por defecto no se crea
     * nada, para que la interfaz compile sin el núcleo conectado.
     */
    suspend fun crearClaveMovil(): Boolean = false

    /** Instala la clave del móvil en el host (§4.4 y §7.9). */
    suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionVista

    /** Olvida la contraseña guardada de un host. */
    suspend fun olvidarContrasena(hostUuid: String): ResultadoAccion

    /** Borra los datos del móvil: false = solo la sincronización, true = todo. */
    suspend fun borrarDatos(todo: Boolean): ResultadoAccion

    /** Guarda el tamaño de letra del terminal (8-24 sp). */
    suspend fun cambiarTamanoLetra(sp: Int): ResultadoAccion

    // =======================================================================
    // Fase 14 · flujos nuevos (informe §5.2)
    // =======================================================================

    /** Lista completa de hosts de Flota, al empezar y al terminar cada sondeo. */
    val flota: StateFlow<List<HostFlotaVista>>

    /** Lista completa de acciones de servicio en curso, en cada cambio. */
    val accionesServicio: StateFlow<List<AccionEnCursoVista>>

    /** Lista completa de ejecuciones de snippets (sin la salida), en cada cambio. */
    val ejecuciones: StateFlow<List<EjecucionVista>>

    /** Novedades de las suscripciones de logs, como mucho cada 100 ms. */
    val avisosLogs: Flow<EstadoLogsVista>

    /** Acciones de servicio terminadas, aunque la pantalla que las pidió ya no esté. */
    val accionesHechas: Flow<ResultadoServicioVista>

    // =======================================================================
    // Fase 14 · ajustes y flota (informe §5.1 y §5.4)
    // =======================================================================

    /**
     * Pasa al núcleo los ajustes del móvil. El núcleo los acota y devuelve los
     * valores normalizados con un aviso por cada corrección.
     */
    suspend fun aplicarAjustes(ajustes: AjustesMovilVista): ResultadoAjustesVista

    /** Hosts de Flota con su estado; mismo filtro que el inventario de la Fase 11. */
    suspend fun flota(filtro: String? = null): List<HostFlotaVista>

    /**
     * Sondea esos hosts (con la lista vacía, todos los SSH) sin abrir diálogos.
     * Los resultados llegan por el flujo [flota].
     */
    suspend fun sondear(hosts: List<String>): ResultadoAccion

    // La ficha de host (fichaHost) gana el bloque flota, las verificaciones y el
    // snippet al conectar; los modelos FichaHostVista ya los incluyen.

    // =======================================================================
    // Fase 14 · servicios (informe §5.1)
    // =======================================================================

    /** Fallos de todos los hosts según el último sondeo del móvil; no abre conexiones. */
    suspend fun serviciosTodaLaFlota(): TodaLaFlotaVista

    /**
     * Listado de servicios de un host con los motores de los ajustes. Con
     * [refresco] = true es automático y no puede abrir diálogos; con false lo
     * pide el usuario y sí puede.
     */
    suspend fun serviciosListar(
        hostUuid: String,
        refresco: Boolean,
    ): ResultadoNucleoVista<ListadoServiciosVista>

    /** CPU y memoria de los contenedores en marcha de un host. */
    suspend fun serviciosConsumo(hostUuid: String): ResultadoNucleoVista<List<ConsumoVista>>

    /** Propiedades, dependencias y texto de la unidad o del inspect. */
    suspend fun serviciosDetalle(
        hostUuid: String,
        objetivo: ObjetivoVista,
    ): ResultadoNucleoVista<DetalleServiciosVista>

    /** Lo que la app necesita saber antes de confirmar o deliberar una acción. */
    suspend fun prepararAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
    ): ResultadoNucleoVista<InfoAccionVista>

    /**
     * Ejecuta la acción y devuelve su resultado. [deliberacion] es el id de la
     * deliberación que la autoriza, si la acción la exige.
     */
    suspend fun servicioAccion(
        hostUuid: String,
        objetivo: ObjetivoVista,
        accion: AccionServicioVista,
        deliberacion: Long? = null,
    ): ResultadoNucleoVista<ResultadoServicioVista>

    /**
     * Comprueba la contraseña de sudo con servicios.sh sudo. [contrasena] va en
     * bytes para poder borrarla tras usarla (nunca a disco, log ni REGISTRO).
     */
    suspend fun sudoComprobar(hostUuid: String, contrasena: ByteArray): ResultadoSudoVista

    /** Olvida la contraseña de sudo en memoria de un host. */
    suspend fun sudoOlvidar(hostUuid: String): ResultadoAccion

    /**
     * Olvida todas las contraseñas de sudo en memoria al bloquearse MAGI
     * (informe §7.5, D268): la siguiente acción la vuelve a pedir.
     */
    suspend fun olvidarSudoAlBloquear(): ResultadoAccion

    // =======================================================================
    // Fase 14 · logs y sesiones con orden (informe §5.1)
    // =======================================================================

    /**
     * Abre la suscripción de logs de un objetivo y devuelve su id. Hay una por
     * objetivo; [lineas] son las líneas iniciales que se piden.
     */
    suspend fun logsSeguir(
        hostUuid: String,
        objetivo: ObjetivoVista,
        lineas: Int = FiltroLogsVista.LINEAS_INICIALES,
    ): ResultadoNucleoVista<Long>

    /** Hasta 500 líneas ya filtradas de una suscripción (con su estado y omitidas). */
    suspend fun logsVentana(
        suscripcion: Long,
        filtro: FiltroLogsVista = FiltroLogsVista(),
    ): ResultadoNucleoVista<VentanaLogsVista>

    /** Cierra el canal de una suscripción; las líneas se quedan en el anillo. */
    suspend fun logsParar(suscripcion: Long): ResultadoAccion

    /** Abre una sesión con PTY para una orden de pestaña del núcleo (T108). */
    suspend fun abrirSesionConOrden(
        hostUuid: String,
        orden: OrdenPestanaVista,
    ): ResultadoAperturaVista

    // =======================================================================
    // Fase 14 · snippets y ejecuciones (informe §5.1)
    // =======================================================================

    /** Snippets del inventario (solo lectura en el móvil), con filtro por subcadena. */
    suspend fun snippets(filtro: String? = null): List<SnippetVista>

    /** Los snippets que tienen ese host entre sus destinos. */
    suspend fun snippetsDeHost(hostUuid: String): List<SnippetVista>

    /** Detalle de un snippet; null si ya no existe. */
    suspend fun snippet(uuid: String): SnippetVista?

    /**
     * Prepara una ejecución: el núcleo sustituye las variables con escape,
     * valida y dice si delibera. Los errores por campo vienen dentro.
     */
    suspend fun prepararEjecucion(
        snippetUuid: String,
        hosts: List<String>,
        variables: List<VariableSnippetVista> = emptyList(),
    ): ResultadoNucleoVista<PreparacionEjecucionVista>

    /**
     * Lanza una ejecución preparada y devuelve su id. [deliberacion] es el id
     * de la deliberación que la autoriza, si la ejecución la exige.
     */
    suspend fun ejecutarSnippet(
        preparacion: PreparacionEjecucionVista,
        pararAlFallo: Boolean,
        deliberacion: Long? = null,
    ): ResultadoNucleoVista<Long>

    /** Ejecuciones en curso y terminadas, sin la salida. */
    suspend fun ejecucionesLista(): List<EjecucionVista>

    /** Salida de un host de una ejecución, ya sin secuencias de escape. */
    suspend fun salidaEjecucion(
        ejecucion: Long,
        hostUuid: String,
    ): ResultadoNucleoVista<SalidaEjecucionVista>

    /** Cancela una ejecución en curso. */
    suspend fun cancelarEjecucion(id: Long): ResultadoAccion

    /** Prepara otra vez una ejecución terminada; volverá a deliberar si hace falta. */
    suspend fun repetirEjecucion(id: Long): ResultadoNucleoVista<PreparacionEjecucionVista>

    /** Olvida las ejecuciones terminadas. */
    suspend fun limpiarEjecuciones(): ResultadoAccion

    // =======================================================================
    // Fase 14 · deliberación y registro (informe §5.1)
    // =======================================================================

    /** Hace las comprobaciones en paralelo (2 s cada una) y devuelve la tabla. */
    suspend fun deliberar(plan: PlanDeliberacionVista): ResultadoNucleoVista<TablaDeliberacionVista>

    /**
     * Resuelve una deliberación (aprobar, forzar con motivo o cancelar), guarda
     * su fila y devuelve el id que autoriza la acción o la ejecución.
     */
    suspend fun resolverDeliberacion(
        id: Long,
        decision: DecisionDeliberacionVista,
    ): ResultadoNucleoVista<Long>

    /**
     * Una página del registro del móvil, de la más nueva a la más vieja.
     * [desde] es el id de la última entrada de la página anterior; null empieza
     * por la más nueva.
     */
    suspend fun registro(
        filtro: FiltroRegistroVista = FiltroRegistroVista(),
        desde: Long? = null,
        cuantas: Int = TAMANO_PAGINA_REGISTRO,
    ): List<EntradaRegistroVista>

    /** Detalle de una entrada del registro, con la deliberación si la hay. */
    suspend fun registroDetalle(id: Long): DetalleRegistroVista?
}

/** Entrada de texto ya interpretada (`EntradaVista`) para el terminal. */
fun EntradaVista.aBytes(): ByteArray? = when (this) {
    is EntradaVista.Texto -> texto.toByteArray(Charsets.UTF_8)
    is EntradaVista.Pulsacion -> null
}
