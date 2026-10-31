package org.cuatrodetres.magi

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.drop
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
 * Crea la capa de plataforma (SAF, Keystore, firma P-256, bloqueo, batería y
 * DataStore) y la fachada real del núcleo. El núcleo NO se arranca aquí: lo
 * pide la interfaz al desbloquear (`fuente.iniciar()` → `NucleoRepositorio`),
 * porque necesita el Keystore desbloqueado y el permiso de la carpeta ya
 * restaurado.
 */
class MagiApp : Application() {

    /** Fachada real de plataforma; válida desde [onCreate]. */
    lateinit var plataforma: ContenedorPlataforma
        private set

    /** Fachada real del núcleo que consume la interfaz. */
    lateinit var fuente: FuenteNucleoReal
        private set

    private val alcance = CoroutineScope(SupervisorJob() + Dispatchers.IO)

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
                ServicioSesiones.actualizar(this, sesiones, tuneles)
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
        alcance.launch {
            // Deja anotadas las sesiones vivas: si Android mata el proceso, al
            // volver se puede avisar de «Android cerró MAGI con N sesiones»
            // (informe §4.6). Se salta el valor inicial vacío para no borrar el
            // aviso pendiente de un cierre anterior.
            fuente.sesiones.drop(1).collect { sesiones ->
                plataforma.preferencias.guardarSesionesAbiertas(sesiones.size)
            }
        }
    }
}
