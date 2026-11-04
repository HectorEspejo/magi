package org.cuatrodetres.magi.nucleo

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.cuatrodetres.magi.BuildConfig
import org.cuatrodetres.magi.preferencias.PreferenciasMagi
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.InventarioVista
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAperturaVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista
import org.cuatrodetres.magi.ui.modelos.VersionVista
import org.cuatrodetres.magi.ui.modelos.ZOOM_POR_DEFECTO
import org.cuatrodetres.magi.ui.modelos.acotarZoom

/**
 * Datos de la clave del móvil que solo conoce la capa de plataforma (el
 * Keystore). El núcleo aporta la línea OpenSSH y la huella.
 */
data class InfoClavePlataforma(
    val ubicacion: UbicacionClaveVista = UbicacionClaveVista.DESCONOCIDA,
    /** Instante de creación en milisegundos (se pasa a segundos al pintarlo). */
    val creadaEnMs: Long? = null,
    val invalidada: Boolean = false,
)

/**
 * Implementación real de [FuenteNucleo]: la interfaz habla con el núcleo Rust a
 * través de los bindings generados por UniFFI y recibe sus notificaciones por
 * [AdaptadorOyenteNucleo].
 *
 * No depende de Android para poder probarse en la JVM: el arranque del núcleo,
 * la información del Keystore y la actualización del servicio llegan como
 * lambdas; los ajustes, por DataStore.
 *
 * Reglas:
 * - las listas de sesiones y túneles son completas (nunca mensajes finos);
 * - las pantallas llegan por tramos y se guarda la última;
 * - las llamadas síncronas del núcleo se hacen en un hilo de E/S;
 * - los errores de [ErrorNucleo] se convierten en mensajes legibles.
 */
class FuenteNucleoReal(
    private val iniciarNucleo: suspend (OyenteNucleo) -> NucleoInterface,
    private val preferencias: PreferenciasMagi,
    private val hayBloqueoDePantalla: () -> Boolean,
    private val infoClave: () -> InfoClavePlataforma?,
    private val actualizarServicio: (sesiones: Int, tuneles: Int) -> Unit,
    /**
     * Se llama al usar una clave de sincronización pegada o escaneada; la app
     * lo aprovecha para vaciar el portapapeles si aún la contiene (§4.1).
     */
    private val alUsarClave: (texto: String) -> Unit = {},
    /** Borra del Keystore la clave del móvil (recrearla cuando Android la invalida). */
    private val olvidarClaveMovil: () -> Unit = {},
    /** Crea la clave del móvil en el Keystore; true si queda en estado LISTA. */
    private val asegurarClaveMovil: () -> Boolean = { false },
    /**
     * Suelta el permiso SAF y olvida la carpeta recordada; se llama al dejar de
     * sincronizar o al borrar los datos (M5 de la revisión).
     */
    private val olvidarCarpeta: suspend () -> Unit = {},
) : FuenteNucleo {

    companion object {
        /** Mismo mensaje que usa la fuente vacía cuando el núcleo no está. */
        const val SIN_NUCLEO: String = "El núcleo de MAGI aún no está iniciado."

        /** El teléfono no tiene PIN, patrón ni contraseña: MAGI no arranca (§4.1). */
        const val SIN_BLOQUEO_PANTALLA: String =
            "MAGI necesita un PIN, un patrón o una contraseña en el teléfono para arrancar."
    }

    private val cerrojo = Mutex()

    @Volatile
    private var nucleo: NucleoInterface? = null

    private val _sesiones = MutableStateFlow<List<SesionVista>>(emptyList())
    override val sesiones: StateFlow<List<SesionVista>> = _sesiones.asStateFlow()

    private val _tuneles = MutableStateFlow<List<TunelVista>>(emptyList())
    override val tuneles: StateFlow<List<TunelVista>> = _tuneles.asStateFlow()

    // M2 de la revisión: ni los tramos de pantalla ni las peticiones de diálogo
    // se pueden conflar. Canales sin límite: el oyente del núcleo hace trySend
    // desde su hilo sin bloquearse y un consumidor recibe todo en orden.
    private val canalPantallas = Channel<PantallaVista>(Channel.UNLIMITED)
    override val pantallas: Flow<PantallaVista> = canalPantallas.receiveAsFlow()

    private val canalPeticiones = Channel<PeticionDialogoVista>(Channel.UNLIMITED)
    override val peticiones: Flow<PeticionDialogoVista> = canalPeticiones.receiveAsFlow()

    private val _sincronizacion = MutableStateFlow(EstadoSincronizacionVista())
    override val sincronizacion: StateFlow<EstadoSincronizacionVista> =
        _sincronizacion.asStateFlow()

    private val _tamanoLetra = MutableStateFlow(ZOOM_POR_DEFECTO)
    override val tamanoLetra: StateFlow<Int> = _tamanoLetra.asStateFlow()

    /**
     * Arranca el núcleo una sola vez y engancha el oyente. Se llama cuando la
     * interfaz ya está desbloqueada; si el teléfono no tiene bloqueo de
     * pantalla, no arranca y lo explica.
     */
    override suspend fun iniciar(): ResultadoAccion {
        if (nucleo != null) return ResultadoAccion.Ok
        return cerrojo.withLock {
            if (nucleo != null) return@withLock ResultadoAccion.Ok
            if (!hayBloqueoDePantalla()) {
                return@withLock ResultadoAccion.Error(SIN_BLOQUEO_PANTALLA)
            }
            try {
                val oyente = AdaptadorOyenteNucleo(
                    alRecibirSesiones = { _sesiones.value = it },
                    alRecibirTuneles = { _tuneles.value = it },
                    alRecibirPantalla = { canalPantallas.trySend(it) },
                    alRecibirPeticion = { canalPeticiones.trySend(it) },
                    alRecibirSincronizacion = { _sincronizacion.value = it },
                    alCambiarTrabajoVivo = actualizarServicio,
                )
                val instancia = iniciarNucleo(oyente)
                nucleo = instancia
                withContext(Dispatchers.IO) {
                    // A4: si DataStore recuerda la carpeta y SAF la ha restaurado,
                    // el núcleo la necesita antes de la primera pasada; si no, en
                    // frío se queda «sin configurar» y la pasada es un no-op.
                    val carpetaRecordada = preferencias.primeraLectura().carpetaUri
                    if (carpetaRecordada != null) {
                        try {
                            instancia.elegirCarpeta(carpetaRecordada)
                        } catch (e: CancellationException) {
                            throw e
                        } catch (_: Throwable) {
                            // Permiso retirado o carpeta movida: la UI pedirá otra.
                        }
                    }
                    // Primera pasada en modo lector al arrancar el núcleo (§4.2):
                    // si no hay carpeta o clave, el núcleo no hace nada.
                    try {
                        instancia.sincronizarAhora()
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        // La pasada es best-effort; su error llega por el oyente.
                    }
                    try {
                        _sincronizacion.value = instancia.sincronizacionEstado().aVista()
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Throwable) {
                        // Sin estado todavía: se queda el último conocido.
                    }
                    _tamanoLetra.value = preferencias.primeraLectura().tamanoLetra
                }
                ResultadoAccion.Ok
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                ResultadoAccion.Error(mensajeDeError(e))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Inventario
    // -----------------------------------------------------------------------

    override suspend fun version(): VersionVista? = leerONulo {
        it.version().aVista(BuildConfig.VERSION_NAME, BuildConfig.VERSION_CODE)
    }

    override suspend fun inventario(filtro: String?): InventarioVista = leer(
        porDefecto = InventarioVista(),
    ) { nucleoReal ->
        InventarioVista(
            grupos = nucleoReal.inventarioGrupos().map { it.aVista() },
            hosts = nucleoReal.inventarioHosts(filtro).map { it.aVista() },
        )
    }

    override suspend fun fichaHost(uuid: String): FichaHostVista? = leerONulo {
        it.fichaHost(uuid).aVista()
    }

    override suspend fun plegarGrupo(uuid: String, plegado: Boolean): ResultadoAccion =
        accion { it.plegarGrupo(uuid, plegado) }

    // -----------------------------------------------------------------------
    // Sincronización
    // -----------------------------------------------------------------------

    override suspend fun sincronizacionEstado(): EstadoSincronizacionVista {
        val estado = leer(porDefecto = _sincronizacion.value) { it.sincronizacionEstado().aVista() }
        _sincronizacion.value = estado
        return estado
    }

    override suspend fun sincronizarAhora(): ResultadoAccion = accion { it.sincronizarAhora() }

    override suspend fun elegirCarpeta(uri: String): ResultadoAccion = accion { it.elegirCarpeta(uri) }

    override suspend fun unirConClave(texto: String): ResultadoAccion {
        // Si la clave pegada sigue en el portapapeles, la app lo vacía (§4.1).
        alUsarClave(texto)
        return accion { it.unirConClave(texto) }
    }

    override suspend fun dejarDeSincronizar(): ResultadoAccion = accion { it.dejarDeSincronizar() }

    override suspend fun conflictosHuella(): List<ConflictoHuellaVista> = leer(emptyList()) {
        it.conflictosHuella().map { conflicto -> conflicto.aVista() }
    }

    override suspend fun aceptarHuellaDeOtroEquipo(id: Long): ResultadoAccion =
        accion { it.aceptarHuellaDeOtroEquipo(id.toULong()) }

    override suspend fun mantenerMiHuella(id: Long): ResultadoAccion =
        accion { it.mantenerMiHuella(id.toULong()) }

    // -----------------------------------------------------------------------
    // Sesiones y terminal
    // -----------------------------------------------------------------------

    override suspend fun abrirSesion(hostUuid: String): ResultadoAperturaVista {
        val actual = nucleo ?: return ResultadoAperturaVista.Error(SIN_NUCLEO)
        return try {
            val id = withContext(Dispatchers.IO) { actual.abrirSesion(hostUuid) }
            ResultadoAperturaVista.Abierta(id.toLong())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            ResultadoAperturaVista.Error(mensajeDeError(e))
        }
    }

    override suspend fun escribir(sesion: Long, bytes: ByteArray): ResultadoAccion =
        accion { it.escribir(sesion.toULong(), bytes) }

    override suspend fun tecla(
        sesion: Long,
        tecla: TeclaVista,
        modificadores: ModificadoresVista,
    ): ResultadoAccion = accion {
        it.tecla(sesion.toULong(), tecla.aNucleo(), modificadores.aNucleo())
    }

    override suspend fun pegar(sesion: Long, texto: String): ResultadoAccion =
        accion { it.pegar(sesion.toULong(), texto) }

    override fun pegadoEntreCorchetes(sesion: Long): Boolean = try {
        nucleo?.pegadoEntreCorchetes(sesion.toULong()) ?: false
    } catch (e: CancellationException) {
        throw e
    } catch (_: Throwable) {
        false
    }

    override suspend fun redimensionar(sesion: Long, columnas: Int, filas: Int): ResultadoAccion =
        accion {
            it.redimensionar(
                sesion.toULong(),
                columnas.coerceAtLeast(0).toUInt(),
                filas.coerceAtLeast(0).toUInt(),
            )
        }

    override suspend fun desplazar(sesion: Long, filas: Int): ResultadoAccion =
        accion { it.desplazar(sesion.toULong(), filas) }

    override suspend fun textoSeleccion(
        sesion: Long,
        desde: PosicionVista,
        hasta: PosicionVista,
    ): String? = leerONulo {
        it.textoSeleccion(sesion.toULong(), desde.aNucleo(), hasta.aNucleo())
    }

    override suspend fun reconectar(sesion: Long): ResultadoAccion =
        accion { it.reconectar(sesion.toULong()) }

    override suspend fun cerrarSesion(sesion: Long): ResultadoAccion =
        accion { it.cerrarSesion(sesion.toULong()) }

    // -----------------------------------------------------------------------
    // Túneles
    // -----------------------------------------------------------------------

    override suspend fun tuneles(hostUuid: String?): List<TunelVista> = leer(emptyList()) {
        it.tuneles(hostUuid).map { tunel -> tunel.aVista() }
    }

    override suspend fun activarTunel(uuid: String): ResultadoAccion = accion { it.activarTunel(uuid) }

    override suspend fun pararTunel(uuid: String): ResultadoAccion = accion { it.pararTunel(uuid) }

    override suspend fun relanzarTunel(uuid: String): ResultadoAccion = accion { it.relanzarTunel(uuid) }

    override suspend fun activarEnPuertoLibre(uuid: String): ResultadoAccion =
        accion { it.activarEnPuertoLibre(uuid) }

    // -----------------------------------------------------------------------
    // Diálogos, clave del móvil y datos
    // -----------------------------------------------------------------------

    override suspend fun responderDialogo(
        peticionId: Long,
        respuesta: RespuestaDialogoVista,
    ): ResultadoAccion {
        // No se toca la cola de peticiones: puede haber llegado otra mientras
        // tanto y borrarla la perdería (M2 de la revisión).
        return accion { it.responderDialogo(peticionId.toULong(), respuesta.aNucleo()) }
    }

    override suspend fun clavePublicaMovil(): ClaveMovilVista? {
        val actual = nucleo ?: return null
        return withContext(Dispatchers.IO) {
            try {
                // La descripción de la clave toca el Keystore: también en E/S.
                val plataforma = infoClave()
                actual.clavePublicaMovil().aVista(
                    ubicacion = plataforma?.ubicacion ?: UbicacionClaveVista.DESCONOCIDA,
                    creadaEn = plataforma?.creadaEnMs?.div(1_000L),
                    invalidada = plataforma?.invalidada ?: false,
                )
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                null
            }
        }
    }

    /**
     * Recrea la clave del móvil cuando Android la invalida (se quitó el bloqueo
     * de pantalla, CA 31): borra la entrada vieja del Keystore, crea una nueva,
     * invalida la caché del núcleo y devuelve true si la clave queda LISTA.
     *
     * Después hay que volver a instalar la clave en los hosts y reescanear la
     * clave de sincronización; eso lo guía la interfaz.
     */
    override suspend fun crearClaveMovil(): Boolean = withContext(Dispatchers.IO) {
        try {
            olvidarClaveMovil()
            if (!asegurarClaveMovil()) {
                return@withContext false
            }
            nucleo?.olvidarCacheClaveMovil()
            // La descripción se lee del Keystore al pedirla; esto la refresca.
            infoClave()
            true
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            false
        }
    }

    override suspend fun instalarClaveEnHost(hostUuid: String): ResultadoInstalacionVista {
        val actual = nucleo ?: return ResultadoInstalacionVista.Error(SIN_NUCLEO)
        return try {
            val resultado = withContext(Dispatchers.IO) { actual.instalarClaveEnHost(hostUuid) }
            resultado.aVista()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            ResultadoInstalacionVista.Error(mensajeDeError(e))
        }
    }

    override suspend fun olvidarContrasena(hostUuid: String): ResultadoAccion =
        accion { it.olvidarContrasena(hostUuid) }

    /**
     * Borra los datos del móvil y deja la app coherente (M5 de la revisión):
     * - siempre: suelta el permiso SAF y olvida la carpeta recordada, para que
     *   el estado vuelva a «sin configurar»;
     * - con TODO: el núcleo ya olvidó la clave del móvil, así que se crea una
     *   nueva y se invalida su caché (la UI avisará de reinstalarla).
     */
    override suspend fun borrarDatos(todo: Boolean): ResultadoAccion {
        val resultado = accion {
            it.borrarDatos(if (todo) AlcanceBorrado.TODO else AlcanceBorrado.SINCRONIZACION)
        }
        if (resultado !is ResultadoAccion.Ok) return resultado
        try {
            olvidarCarpeta()
            if (todo) {
                olvidarClaveMovil()
                asegurarClaveMovil()
                nucleo?.olvidarCacheClaveMovil()
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            return ResultadoAccion.Error(mensajeDeError(e))
        }
        return resultado
    }

    override suspend fun cambiarTamanoLetra(sp: Int): ResultadoAccion {
        val tamano = acotarZoom(sp)
        _tamanoLetra.value = tamano
        return try {
            preferencias.guardarTamanoLetra(tamano)
            ResultadoAccion.Ok
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            ResultadoAccion.Error(mensajeDeError(e))
        }
    }

    /**
     * Cierra el núcleo y suelta la referencia (B7 de la revisión): la fachada
     * no queda apuntando a un objeto destruido. La UI no debe usarse después.
     */
    fun cerrarNucleo() {
        nucleo = null
        NucleoRepositorio.cerrar()
    }

    // -----------------------------------------------------------------------
    // Ayudas
    // -----------------------------------------------------------------------

    /** Ejecuta una llamada síncrona del núcleo en E/S y traduce sus errores. */
    private suspend fun <T> leer(
        porDefecto: T,
        bloque: (NucleoInterface) -> T,
    ): T {
        val actual = nucleo ?: return porDefecto
        return withContext(Dispatchers.IO) {
            try {
                bloque(actual)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                porDefecto
            }
        }
    }

    /** Como [leer], pero null si no hay núcleo o falla. */
    private suspend fun <T> leerONulo(bloque: (NucleoInterface) -> T): T? {
        val actual = nucleo ?: return null
        return withContext(Dispatchers.IO) {
            try {
                bloque(actual)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                null
            }
        }
    }

    /** Acción del núcleo sin valor: Ok o el mensaje legible del fallo. */
    private suspend fun accion(bloque: suspend (NucleoInterface) -> Unit): ResultadoAccion {
        val actual = nucleo ?: return ResultadoAccion.Error(SIN_NUCLEO)
        return try {
            withContext(Dispatchers.IO) { bloque(actual) }
            ResultadoAccion.Ok
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            ResultadoAccion.Error(mensajeDeError(e))
        }
    }
}
