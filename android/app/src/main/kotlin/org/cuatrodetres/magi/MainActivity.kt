package org.cuatrodetres.magi

import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.remember
import androidx.fragment.app.FragmentActivity
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
 * - Pide POST_NOTIFICATIONS en Android 13+ para que la notificación del
 *   servicio en primer plano se vea cuando haya sesiones o túneles.
 * - Edge to edge: las barras del sistema y el teclado se respetan con insets
 *   (fila de teclas del terminal pegada al teclado, informe §6).
 */
class MainActivity : FragmentActivity() {

    private val solicitudNotificaciones =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            // Si se deniega, el servicio sigue vivo pero sin notificación visible.
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val aplicacion = application as MagiApp
        val plataforma = aplicacion.plataforma
        val fuente = aplicacion.fuente
        val actividad = this
        pedirPermisoDeNotificaciones()
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

    /** Android 13+: sin permiso, la notificación del servicio no se ve. */
    private fun pedirPermisoDeNotificaciones() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (ServicioSesiones.notificacionesPermitidas(this)) return
        solicitudNotificaciones.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}
