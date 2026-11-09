package org.cuatrodetres.magi.plataforma

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import org.cuatrodetres.magi.MainActivity
import org.cuatrodetres.magi.R

/**
 * Servicio en primer plano `specialUse` de MAGI (informe §3.3, D279): vive
 * mientras haya alguna sesión, túnel, acción de servicio o ejecución de
 * snippets, y su notificación es «MAGI · 2 sesiones · 1 acción».
 *
 * - Arranca con el primer trabajo y se para con el último ([actualizar] decide).
 *   El sondeo, los refrescos y los logs no lo mantienen vivo: solo corren con
 *   su pantalla a la vista.
 * - La notificación no lleva acciones (T6): tocarla solo abre la app, que pedirá
 *   el desbloqueo si toca.
 * - Al morir el proceso no se reintenta (`START_NOT_STICKY`): el trabajo vive en
 *   memoria y al volver la app avisa de que Android cerró MAGI.
 */
class ServicioSesiones : Service() {

    companion object {
        /** Canal de la notificación del servicio. */
        const val CANAL = "magi_sesiones"

        /** Identificador fijo de la notificación en primer plano. */
        const val ID_NOTIFICACION = 1

        const val EXTRA_SESIONES = "sesiones"
        const val EXTRA_TUNELES = "tuneles"
        const val EXTRA_ACCIONES = "acciones"
        const val EXTRA_EJECUCIONES = "ejecuciones"

        /**
         * Actualiza el servicio: lo arranca o refresca si hay trabajo vivo y lo
         * para cuando no queda ni una sesión, ni un túnel, ni una acción, ni una
         * ejecución.
         */
        fun actualizar(
            contexto: Context,
            sesiones: Int,
            tuneles: Int,
            acciones: Int = 0,
            ejecuciones: Int = 0,
        ) {
            val sesionesLimpias = ReglasServicio.sanear(sesiones)
            val tunelesLimpios = ReglasServicio.sanear(tuneles)
            val accionesLimpias = ReglasServicio.sanear(acciones)
            val ejecucionesLimpias = ReglasServicio.sanear(ejecuciones)
            if (ReglasServicio.debeEstarActivo(
                    sesionesLimpias,
                    tunelesLimpios,
                    accionesLimpias,
                    ejecucionesLimpias,
                )
            ) {
                val intent = Intent(contexto, ServicioSesiones::class.java).apply {
                    putExtra(EXTRA_SESIONES, sesionesLimpias)
                    putExtra(EXTRA_TUNELES, tunelesLimpios)
                    putExtra(EXTRA_ACCIONES, accionesLimpias)
                    putExtra(EXTRA_EJECUCIONES, ejecucionesLimpias)
                }
                try {
                    ContextCompat.startForegroundService(contexto, intent)
                } catch (_: RuntimeException) {
                    // Android 12+ puede rechazar arrancar un servicio desde
                    // segundo plano; si ya estaba en marcha, el intent de
                    // actualización le habrá llegado igualmente. Las sesiones
                    // no dependen de la notificación para seguir vivas.
                }
            } else {
                contexto.stopService(Intent(contexto, ServicioSesiones::class.java))
            }
        }

        /** Si en Android 13+ hay permiso para mostrar la notificación. */
        fun notificacionesPermitidas(contexto: Context): Boolean =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                ContextCompat.checkSelfPermission(
                    contexto,
                    android.Manifest.permission.POST_NOTIFICATIONS,
                ) == PackageManager.PERMISSION_GRANTED
            } else {
                true
            }

        /** Ajustes de notificaciones de la app, para pedir el permiso sin diálogo propio. */
        fun crearIntentPermisoNotificaciones(contexto: Context): Intent =
            Intent(android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, contexto.packageName)
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        crearCanalDeNotificaciones()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val sesiones = intent?.getIntExtra(EXTRA_SESIONES, 0) ?: 0
        val tuneles = intent?.getIntExtra(EXTRA_TUNELES, 0) ?: 0
        val acciones = intent?.getIntExtra(EXTRA_ACCIONES, 0) ?: 0
        val ejecuciones = intent?.getIntExtra(EXTRA_EJECUCIONES, 0) ?: 0
        if (!ReglasServicio.debeEstarActivo(sesiones, tuneles, acciones, ejecuciones)) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        val cuentas = ReglasServicio.cuentasDeLaNotificacion(
            sesiones,
            tuneles,
            acciones,
            ejecuciones,
        )
        val notificacion = construirNotificacion(cuentas)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                ID_NOTIFICACION,
                notificacion,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(ID_NOTIFICACION, notificacion)
        }
        return START_NOT_STICKY
    }

    private fun crearCanalDeNotificaciones() {
        val gestor = getSystemService(NotificationManager::class.java) ?: return
        val canal = NotificationChannel(
            CANAL,
            getString(R.string.plataforma_notificacion_canal),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = getString(R.string.plataforma_notificacion_canal_descripcion)
            setShowBadge(false)
            setSound(null, null)
        }
        gestor.createNotificationChannel(canal)
    }

    private fun construirNotificacion(cuentas: List<CuentaTrabajo>): Notification {
        val texto = textoDeLaNotificacion(cuentas)
        val abrir = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CANAL)
            .setSmallIcon(R.drawable.icono_monocromo)
            .setContentTitle(texto)
            .setOngoing(true)
            .setSilent(true)
            .setShowWhen(false)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .setContentIntent(abrir)
            .build()
    }

    /** «MAGI · 2 sesiones · 1 acción»: cada recuento con su plural. */
    private fun textoDeLaNotificacion(cuentas: List<CuentaTrabajo>): String {
        if (cuentas.isEmpty()) {
            return getString(R.string.plataforma_notificacion_servicio_sin_trabajo)
        }
        val partes = cuentas.map { cuenta ->
            when (cuenta.tipo) {
                TipoTrabajoNotificacion.SESIONES -> resources.getQuantityString(
                    R.plurals.plataforma_notificacion_sesiones,
                    cuenta.cantidad,
                    cuenta.cantidad,
                )
                TipoTrabajoNotificacion.TUNELES -> resources.getQuantityString(
                    R.plurals.plataforma_notificacion_tuneles,
                    cuenta.cantidad,
                    cuenta.cantidad,
                )
                TipoTrabajoNotificacion.ACCIONES -> resources.getQuantityString(
                    R.plurals.plataforma_notificacion_acciones,
                    cuenta.cantidad,
                    cuenta.cantidad,
                )
                TipoTrabajoNotificacion.EJECUCIONES -> resources.getQuantityString(
                    R.plurals.plataforma_notificacion_ejecuciones,
                    cuenta.cantidad,
                    cuenta.cantidad,
                )
            }
        }
        return getString(R.string.plataforma_notificacion_servicio, partes.joinToString(" · "))
    }
}
