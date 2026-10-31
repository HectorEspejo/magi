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

    override fun onCreate() {
        super.onCreate()
        plataforma = ContenedorPlataforma(this)
        fuente = FuenteNucleoReal(
            iniciarNucleo = { oyente ->
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
