package org.cuatrodetres.magi.plataforma

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Build
import android.view.WindowManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalContext

/**
 * Pantallas con secretos (pegar o escanear la clave de sincronización, diálogo
 * de contraseña): FLAG_SECURE impide capturas y, desde Android 13, deja vacía
 * la vista previa de recientes (informe §8).
 *
 * La interfaz puede envolver esas pantallas con [RecintoSeguro] o llamar a
 * [aplicar]/[retirar] desde un Activity.
 */
object PantallaSegura {

    fun aplicar(actividad: Activity) {
        actividad.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            actividad.setRecentsScreenshotEnabled(false)
        }
    }

    fun retirar(actividad: Activity) {
        actividad.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            actividad.setRecentsScreenshotEnabled(true)
        }
    }
}

/**
 * Envuelve contenido y mantiene FLAG_SECURE mientras [activo] sea true. Fuera
 * de la pantalla con secretos no se toca el flag.
 */
@Composable
fun RecintoSeguro(activo: Boolean = true, contenido: @Composable () -> Unit) {
    val actividad = LocalContext.current.actividadAnfitriona()
    DisposableEffect(actividad, activo) {
        if (actividad != null) {
            if (activo) PantallaSegura.aplicar(actividad) else PantallaSegura.retirar(actividad)
        }
        onDispose {
            if (actividad != null) PantallaSegura.retirar(actividad)
        }
    }
    contenido()
}

private tailrec fun Context.actividadAnfitriona(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.actividadAnfitriona()
    else -> null
}
