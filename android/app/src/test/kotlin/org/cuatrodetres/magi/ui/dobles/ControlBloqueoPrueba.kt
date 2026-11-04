package org.cuatrodetres.magi.ui.dobles

import org.cuatrodetres.magi.ui.modelos.ControlBloqueo

/** Control de bloqueo para pruebas: decide sin biometría ni teléfono real. */
class ControlBloqueoPrueba(
    var hayBloqueo: Boolean = true,
    var bloquearAlArrancar: Boolean = false,
    var desbloqueoPermitido: Boolean = true,
    var bloquearAlVolver: Boolean = false,
) : ControlBloqueo {

    var salidas: Int = 0
    var peticionesDesbloqueo: Int = 0

    override fun hayBloqueoDePantalla(): Boolean = hayBloqueo

    override fun debeBloquearAlArrancar(): Boolean = bloquearAlArrancar

    override suspend fun solicitarDesbloqueo(): Boolean {
        peticionesDesbloqueo++
        return desbloqueoPermitido
    }

    override fun alSalirDeLaApp() {
        salidas++
    }

    override fun alVolverALaApp(): Boolean = bloquearAlVolver
}
