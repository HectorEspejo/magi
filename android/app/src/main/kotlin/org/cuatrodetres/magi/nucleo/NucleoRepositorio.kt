package org.cuatrodetres.magi.nucleo

import android.content.Context
import android.os.Build
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.cuatrodetres.magi.plataforma.FachadaPlataforma
import java.io.File

/**
 * Puerta de la app al núcleo Rust (`magi-movil`, Fase 11).
 *
 * - Carga la cdylib que UniFFI empaqueta por ABI en `jniLibs` (lo hace el
 *   propio código generado al primer uso).
 * - Construye las rutas privadas de la app (nada de `HOME`: el núcleo no lee el
 *   entorno), el nombre del equipo y las retrollamadas de plataforma.
 * - Llama a `Nucleo.iniciar` UNA vez, en un hilo de E/S y tras el desbloqueo
 *   (lo decide la interfaz); a partir de ahí expone la instancia.
 *
 * No se escribe a mano ningún binding: la API es la generada por UniFFI en este
 * mismo paquete.
 */
object NucleoRepositorio {

    /** Nombre de la biblioteca nativa, sin prefijo `lib` ni extensión. */
    const val BIBLIOTECA = "magi_movil"

    /** Paquete Kotlin de los bindings generados por UniFFI. */
    const val PAQUETE_BINDINGS = "org.cuatrodetres.magi.nucleo"

    /** Plataforma del equipo que se declara al núcleo (informe §2). */
    const val PLATAFORMA = "android"

    /** Carpeta privada de MAGI dentro de `filesDir`. */
    const val DIRECTORIO_RAIZ = "magi"

    /** Longitud máxima del nombre de equipo que viaja en la sincronización. */
    const val NOMBRE_EQUIPO_MAXIMO = 64

    /** Estado del núcleo dentro de la app. */
    enum class Estado {
        SIN_INICIAR,
        INICIANDO,
        LISTO,
        ERROR,
    }

    @Volatile
    var estado: Estado = Estado.SIN_INICIAR
        private set

    @Volatile
    private var instancia: Nucleo? = null

    private val cerrojo = Mutex()

    /** El núcleo ya iniciado, o null si todavía no lo está. */
    fun nucleo(): Nucleo? = instancia

    /**
     * Arranca el núcleo (idempotente). Es la única llamada a
     * `Nucleo.iniciar`: si ya está listo, devuelve la instancia.
     */
    suspend fun iniciar(
        contexto: Context,
        plataforma: FachadaPlataforma,
        oyente: OyenteNucleo,
    ): Nucleo {
        instancia?.let {
            estado = Estado.LISTO
            return it
        }
        return cerrojo.withLock {
            instancia?.let {
                estado = Estado.LISTO
                return@withLock it
            }
            estado = Estado.INICIANDO
            try {
                val nucleo = withContext(Dispatchers.IO) {
                    Nucleo.iniciar(
                        rutas = rutas(contexto),
                        equipo = equipo(),
                        retrollamadas = Retrollamadas(
                            carpeta = CarpetaMovilAdaptada(plataforma.carpeta),
                            llavero = LlaveroMovilAdaptado(plataforma.llavero),
                            firmante = FirmanteMovilAdaptado(plataforma.firmante),
                            oyente = oyente,
                        ),
                    )
                }
                instancia = nucleo
                estado = Estado.LISTO
                nucleo
            } catch (e: Throwable) {
                estado = Estado.ERROR
                throw e
            }
        }
    }

    /** Libera el núcleo (al borrar todos los datos o al cerrar la app). */
    fun cerrar() {
        instancia?.let { nucleo ->
            instancia = null
            estado = Estado.SIN_INICIAR
            try {
                nucleo.destroy()
            } catch (_: Throwable) {
                // El núcleo ya podía estar destruido: no hay nada que hacer.
            }
        }
    }

    /**
     * Rutas privadas del móvil. El núcleo no calcula rutas: se las da la app.
     * - `datos`: magi.db, temporales y el known_hosts privado;
     * - `config`: configuración (sin config.toml en el móvil);
     * - `estado`: logs;
     * - `hogar`: raíz del hogar privado (contiene `.ssh/known_hosts`);
     * - `runtime`: vacío (el móvil no tiene servidor ni socket Unix).
     */
    fun rutas(contexto: Context): Rutas {
        val raiz = File(contexto.filesDir, DIRECTORIO_RAIZ)
        val datos = crearDirectorio(File(raiz, "datos"))
        val config = crearDirectorio(File(raiz, "config"))
        val estado = crearDirectorio(File(raiz, "estado"))
        val hogar = crearDirectorio(File(raiz, "hogar"))
        crearDirectorio(File(hogar, ".ssh"))
        return Rutas(
            datos = datos.absolutePath,
            config = config.absolutePath,
            estado = estado.absolutePath,
            hogar = hogar.absolutePath,
            runtime = "",
        )
    }

    /** Identidad del equipo en la sincronización: modelo del teléfono y android. */
    fun equipo(): Equipo {
        val modelo = Build.MODEL?.trim().orEmpty().ifEmpty { "Android" }
        return Equipo(
            nombre = modelo.take(NOMBRE_EQUIPO_MAXIMO),
            plataforma = PLATAFORMA,
        )
    }

    private fun crearDirectorio(directorio: File): File {
        if (!directorio.isDirectory && !directorio.mkdirs()) {
            throw IllegalStateException("No se pudo crear el directorio " + directorio.absolutePath)
        }
        return directorio
    }
}
