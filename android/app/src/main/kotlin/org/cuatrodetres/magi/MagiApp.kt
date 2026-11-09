package org.cuatrodetres.magi

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.nucleo.FuenteNucleoReal
import org.cuatrodetres.magi.nucleo.InfoClavePlataforma
import org.cuatrodetres.magi.nucleo.NucleoRepositorio
import org.cuatrodetres.magi.plataforma.ClaveMovil
import org.cuatrodetres.magi.plataforma.ContenedorPlataforma
import org.cuatrodetres.magi.plataforma.ServicioSesiones
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista

/**
 * Aplicación de MAGI en Android (Fase 11).
 *
 * Crea la capa de plataforma (SAF, Keystore, firma P-256, bloqueo, batería,
 * DataStore y visibilidad) y la fachada real del núcleo. El núcleo NO se
 * arranca aquí: lo pide la interfaz al desbloquear (`fuente.iniciar()` →
 * `NucleoRepositorio`), porque necesita el Keystore desbloqueado y el permiso
 * de la carpeta ya restaurado.
 *
 * Desde la Fase 14 mantiene el servicio en primer plano al día con las
 * sesiones, los túneles, las acciones de servicio y las ejecuciones (D279).
 * El trabajo que quedó abierto para el aviso de «Android cerró MAGI» lo anota
 * VistaModeloMagi, cuando ya ha consumido el aviso pendiente.
 */
class MagiApp : Application() {

    /** Fachada real de plataforma; válida desde [onCreate]. */
    lateinit var plataforma: ContenedorPlataforma
        private set

    /** Fachada real del núcleo que consume la interfaz. */
    lateinit var fuente: FuenteNucleoReal
        private set

    private val alcance = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // Recuentos vivos para el servicio en primer plano (informe §3.3 y D279).
    // Los actualiza el oyente del núcleo (sesiones y túneles) y los flujos de
    // acciones y ejecuciones; el servicio se refresca con cada cambio.
    @Volatile
    private var sesionesVivas: Int = 0

    @Volatile
    private var tunelesVivos: Int = 0

    @Volatile
    private var accionesVivas: Int = 0

    @Volatile
    private var ejecucionesVivas: Int = 0

    /** Refresca la notificación del servicio con los recuentos vivos. */
    private fun actualizarServicioSesiones() {
        ServicioSesiones.actualizar(
            contexto = this,
            sesiones = sesionesVivas,
            tuneles = tunelesVivos,
            acciones = accionesVivas,
            ejecuciones = ejecucionesVivas,
        )
    }

    /**
     * Lanza una pasada de sincronización en el ámbito de la aplicación (A5 de
     * la revisión): no muere si la Activity se cancela. La agenda la Activity
     * mientras está visible, así que no hay trabajo de fondo.
     */
    fun sincronizarEnSegundoPlano() {
        alcance.launch { fuente.sincronizarAhora() }
    }

    override fun onCreate() {
        super.onCreate()
        plataforma = ContenedorPlataforma(this)
        fuente = FuenteNucleoReal(
            iniciarNucleo = { oyente ->
                // A4: restaura la carpeta SAF recordada antes de arrancar, para
                // que el núcleo la reconozca en la primera pasada.
                plataforma.iniciar()
                // Compatibilidad: recrea claves antiguas sin el requisito de
                // dispositivo desbloqueado que falla al desbloquear con la cara.
                plataforma.migrarClavesSiHaceFalta()
                NucleoRepositorio.iniciar(this, plataforma, oyente)
            },
            preferencias = plataforma.preferencias,
            hayBloqueoDePantalla = { plataforma.bloqueo.elTelefonoTieneBloqueo() },
            infoClave = {
                val descripcion = plataforma.firmante.descripcion()
                InfoClavePlataforma(
                    ubicacion = when (descripcion.ubicacion) {
                        ClaveMovil.Ubicacion.STRONGBOX -> UbicacionClaveVista.STRONGBOX
                        ClaveMovil.Ubicacion.TEE -> UbicacionClaveVista.TEE
                        null -> UbicacionClaveVista.DESCONOCIDA
                    },
                    creadaEnMs = descripcion.creadaEnMs,
                    invalidada = descripcion.estado == ClaveMovil.Estado.INVALIDADA,
                )
            },
            actualizarServicio = { sesiones, tuneles ->
                sesionesVivas = sesiones
                tunelesVivos = tuneles
                actualizarServicioSesiones()
            },
            alUsarClave = { texto ->
                // La clave de sincronización es un secreto: si sigue en el
                // portapapeles después de unirla, se vacía (informe §4.1).
                plataforma.portapapeles.vaciarSiContiene(texto)
            },
            // Recrear la clave del móvil cuando Android la invalida (CA 31).
            olvidarClaveMovil = { plataforma.firmante.olvidarClave() },
            asegurarClaveMovil = {
                val descripcion = plataforma.firmante.asegurarClave()
                descripcion.estado == ClaveMovil.Estado.LISTA
            },
            // M5: al borrar datos, el móvil vuelve a «sin configurar» y suelta SAF.
            olvidarCarpeta = { plataforma.carpeta.olvidarCarpeta() },
        )
        alcance.launch { plataforma.iniciar() }
        // El servicio en primer plano cuenta también las acciones y las
        // ejecuciones de la Fase 14, que siguen con la app en segundo plano
        // (D279). Las sesiones y los túneles llegan por el oyente del núcleo.
        alcance.launch {
            fuente.accionesServicio.collect { acciones ->
                accionesVivas = acciones.size
                actualizarServicioSesiones()
            }
        }
        alcance.launch {
            fuente.ejecuciones.collect { ejecuciones ->
                ejecucionesVivas = ejecuciones.size
                actualizarServicioSesiones()
            }
        }
        // El trabajo que queda abierto para el aviso de «Android cerró MAGI con
        // acciones o ejecuciones en curso» lo anota VistaModeloMagi, cuando ya
        // ha consumido el aviso pendiente (informe §7.12).
    }
}
