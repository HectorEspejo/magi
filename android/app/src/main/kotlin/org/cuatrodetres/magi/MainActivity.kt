package org.cuatrodetres.magi

import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.remember
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.nucleo.FuenteNucleoReal
import org.cuatrodetres.magi.plataforma.AdaptadorControlBloqueo
import org.cuatrodetres.magi.plataforma.ServicioSesiones
import org.cuatrodetres.magi.ui.AppMagi
import org.cuatrodetres.magi.ui.tema.TemaMagi

/**
 * Única actividad de MAGI en Android.
 *
 * - Es una FragmentActivity porque androidx.biometric 1.1.0 exige una para
 *   BiometricPrompt; sigue siendo un ComponentActivity para Compose.
 * - Cablea la capa de plataforma con la interfaz: adaptador de bloqueo
 *   (biometría y regla de cinco minutos), selector SAF con permiso persistente
 *   de solo lectura, guía de batería y la fachada real del núcleo.
 * - Dispara la pasada de sincronización en modo lector al volver a la app y cada
 *   60 s mientras está visible; en segundo plano el temporizador se para
 *   (informe §4.2 y D178). La primera pasada tras arrancar el núcleo la hace
 *   `FuenteNucleoReal.iniciar()`.
 * - Pide POST_NOTIFICATIONS en Android 13+ para que la notificación del
 *   servicio en primer plano se vea cuando haya sesiones o túneles.
 * - Edge to edge: las barras del sistema y el teclado se respetan con insets
 *   (fila de teclas del terminal pegada al teclado, informe §6).
 */
class MainActivity : FragmentActivity() {

    companion object {
        /** Cada cuánto se lanza una pasada mientras la app está visible (§4.2). */
        private const val INTERVALO_PASADA_MS = 60_000L
    }

    private val solicitudNotificaciones =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            // Si se deniega, el servicio sigue vivo pero sin notificación visible.
        }

    private lateinit var fuente: FuenteNucleoReal
    private lateinit var aplicacion: MagiApp

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        aplicacion = application as MagiApp
        val plataforma = aplicacion.plataforma
        val actividad = this
        fuente = aplicacion.fuente
        pedirPermisoDeNotificaciones()
        programarSincronizacionVisible()
        setContent {
            TemaMagi {
                AppMagi(
                    fuente = fuente,
                    controlBloqueo = remember {
                        AdaptadorControlBloqueo(actividad, plataforma.bloqueo)
                    },
                    crearIntentCarpeta = plataforma.carpeta::crearIntentElegirCarpeta,
                    recordarCarpeta = plataforma.carpeta::recordarCarpeta,
                    alAbrirBateria = plataforma.bateria::abrirAjustes,
                )
            }
        }
    }

    /**
     * Pasada en modo lector al volver a la app (ON_RESUME) y cada 60 s mientras
     * está visible. El temporizador se cancela al pasar a segundo plano: MAGI no
     * hace trabajo en segundo plano sin sesiones ni túneles (D178). Si el núcleo
     * aún no está iniciado, `sincronizarAhora` devuelve un aviso que se ignora.
     */
    private fun programarSincronizacionVisible() {
        lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onResume(owner: LifecycleOwner) {
                // A5: la pasada corre en el ámbito de la app; si la Activity se
                // cancela, la pasada en curso no muere a medias.
                aplicacion.sincronizarEnSegundoPlano()
            }
        })
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.RESUMED) {
                while (isActive) {
                    delay(INTERVALO_PASADA_MS)
                    aplicacion.sincronizarEnSegundoPlano()
                }
            }
        }
    }

    /** Android 13+: sin permiso, la notificación del servicio no se ve. */
    private fun pedirPermisoDeNotificaciones() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (ServicioSesiones.notificacionesPermitidas(this)) return
        solicitudNotificaciones.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}
