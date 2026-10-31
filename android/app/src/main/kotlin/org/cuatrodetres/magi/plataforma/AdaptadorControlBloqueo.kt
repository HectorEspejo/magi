package org.cuatrodetres.magi.plataforma

import androidx.fragment.app.FragmentActivity
import org.cuatrodetres.magi.ui.modelos.ControlBloqueo

/**
 * Adapta el bloqueo real ([BloqueoBiometrico]) a la interfaz que consume la UI
 * (`org.cuatrodetres.magi.ui.modelos.ControlBloqueo`).
 *
 * A diferencia del resto de la fachada, esto sí conoce la capa de presentación:
 * es el pegamento mínimo que MainActivity construye con su propia actividad,
 * porque BiometricPrompt exige una FragmentActivity.
 */
class AdaptadorControlBloqueo(
    private val actividad: FragmentActivity,
    private val bloqueo: BloqueoBiometrico,
) : ControlBloqueo {

    override fun hayBloqueoDePantalla(): Boolean = bloqueo.elTelefonoTieneBloqueo()

    override fun debeBloquearAlArrancar(): Boolean = bloqueo.debeBloquearAlArrancar()

    override suspend fun solicitarDesbloqueo(): Boolean =
        bloqueo.solicitarDesbloqueo(actividad)

    override fun alSalirDeLaApp() {
        bloqueo.alSalirDeLaApp()
    }

    override fun alVolverALaApp(): Boolean = bloqueo.alVolverALaApp()
}
