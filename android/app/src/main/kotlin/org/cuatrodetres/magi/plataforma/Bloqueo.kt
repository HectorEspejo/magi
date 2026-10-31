package org.cuatrodetres.magi.plataforma

import android.app.KeyguardManager
import android.content.Context
import android.os.SystemClock
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.cuatrodetres.magi.R
import kotlin.coroutines.resume

/**
 * Bloqueo de la app con BiometricPrompt (informe §7.8 y §9): biometría fuerte o
 * PIN/patrón/contraseña del teléfono, arranque en frío bloqueado y margen de
 * cinco minutos al volver.
 *
 * Lo que se comprueba aquí es solo el bloqueo de MAGI: las sesiones y los
 * túneles siguen vivos mientras la app está bloqueada (D174) y la clave del
 * móvil no pide biometría en cada firma. La regla de tiempo es pura y está en
 * [ReglaBloqueo] (probada en la JVM).
 */
class BloqueoBiometrico(private val contexto: Context) : BloqueoApp {

    companion object {
        /**
         * Biometría fuerte o credencial del dispositivo. La credencial cubre
         * los teléfonos que no tengan huella, y el PIN es el plan B cuando la
         * huella falla.
         */
        val AUTENTICADORES: Int =
            BiometricManager.Authenticators.BIOMETRIC_STRONG or
                BiometricManager.Authenticators.DEVICE_CREDENTIAL
    }

    private val cerrojo = Any()

    /** La app está desbloqueada en este proceso (no sobrevive al proceso). */
    @Volatile
    private var desbloqueada = false

    /** Instante de salida a segundo plano, en milisegundos monotónicos. */
    private var salioEnSegundos: Long? = null

    /** true = el teléfono no tiene PIN, patrón ni contraseña: MAGI no arranca. */
    override fun elTelefonoTieneBloqueo(): Boolean {
        val gestor = contexto.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return gestor?.isDeviceSecure == true
    }

    /** Si BiometricPrompt puede autenticar aquí (biometría o credencial). */
    fun puedeUsarAutenticacion(): Boolean =
        BiometricManager.from(contexto).canAuthenticate(AUTENTICADORES) ==
            BiometricManager.BIOMETRIC_SUCCESS

    /**
     * La app tiene que empezar bloqueada: en frío siempre y, mientras no se
     * desbloquee en este proceso, también tras cada vuelta.
     */
    fun debeBloquearAlArrancar(): Boolean = synchronized(cerrojo) { !desbloqueada }

    /** La app pasa a segundo plano: se anota la hora de salida. */
    fun alSalirDeLaApp() {
        synchronized(cerrojo) {
            salioEnSegundos = ahoraSegundos()
        }
    }

    /**
     * La app vuelve a primer plano. Devuelve true si toca bloquear: nunca se
     * desbloquea sola (si estaba bloqueada sigue bloqueada) y si estaba
     * desbloqueada manda el margen de cinco minutos.
     */
    fun alVolverALaApp(): Boolean = synchronized(cerrojo) {
        val estado = when {
            !desbloqueada -> ReglaBloqueo.Estado.BLOQUEADA
            // No llegó a salir (p. ej. una recomposición): no se bloquea.
            salioEnSegundos == null -> ReglaBloqueo.Estado.DESBLOQUEADA
            else -> ReglaBloqueo.alVolver(
                ahoraSegundos = ahoraSegundos(),
                salioEn = salioEnSegundos,
                arranqueEnFrio = false,
            )
        }
        if (estado == ReglaBloqueo.Estado.BLOQUEADA) {
            desbloqueada = false
        }
        estado == ReglaBloqueo.Estado.BLOQUEADA
    }

    /** Marca la app como desbloqueada (lo hace [solicitarDesbloqueo] al acertar). */
    fun marcarDesbloqueada() {
        synchronized(cerrojo) {
            desbloqueada = true
            salioEnSegundos = null
        }
    }

    override suspend fun desbloquear(actividad: FragmentActivity): Boolean =
        solicitarDesbloqueo(actividad)

    /**
     * Abre BiometricPrompt y espera el resultado. No lanza: un error (cancelar,
     * bloqueo del sistema, sin biometría…) devuelve false y la app sigue
     * bloqueada.
     */
    suspend fun solicitarDesbloqueo(actividad: FragmentActivity): Boolean {
        if (!elTelefonoTieneBloqueo() || !puedeUsarAutenticacion()) return false
        val resultado = withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { continuacion ->
                val ejecutor = ContextCompat.getMainExecutor(actividad)
                val dialogo = BiometricPrompt(
                    actividad,
                    ejecutor,
                    object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationSucceeded(
                            resultado: BiometricPrompt.AuthenticationResult,
                        ) {
                            if (continuacion.isActive) continuacion.resume(true)
                        }

                        override fun onAuthenticationError(
                            codigoDeError: Int,
                            mensaje: CharSequence,
                        ) {
                            // Cancelar o "demasiados intentos": se sigue bloqueada.
                            if (continuacion.isActive) continuacion.resume(false)
                        }

                        override fun onAuthenticationFailed() {
                            // Intento fallido: el diálogo del sistema sigue abierto.
                        }
                    },
                )
                val informacion = BiometricPrompt.PromptInfo.Builder()
                    .setTitle(contexto.getString(R.string.plataforma_bloqueo_titulo))
                    .setSubtitle(contexto.getString(R.string.plataforma_bloqueo_subtitulo))
                    .setAllowedAuthenticators(AUTENTICADORES)
                    .setConfirmationRequired(false)
                    .build()
                continuacion.invokeOnCancellation { dialogo.cancelAuthentication() }
                dialogo.authenticate(informacion)
            }
        }
        if (resultado) marcarDesbloqueada()
        return resultado
    }

    private fun ahoraSegundos(): Long = SystemClock.elapsedRealtime() / 1_000L
}
