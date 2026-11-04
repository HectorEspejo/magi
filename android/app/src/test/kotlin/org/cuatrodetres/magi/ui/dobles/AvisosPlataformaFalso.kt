package org.cuatrodetres.magi.ui.dobles

import org.cuatrodetres.magi.ui.modelos.AvisosPlataforma

/** Avisos del segundo plano para pruebas: decide y anota sin DataStore. */
class AvisosPlataformaFalso(
    var sesionesPerdidas: Int = 0,
    var ofrecerGuiaBateria: Boolean = false,
) : AvisosPlataforma {

    var guiaMarcada: Boolean = false
    var vecesGuiaMarcada: Int = 0

    override suspend fun sesionesQueQuedaronAbiertas(): Int {
        val perdidas = sesionesPerdidas
        sesionesPerdidas = 0
        return perdidas
    }

    override suspend fun debeOfrecerseGuiaBateria(): Boolean = ofrecerGuiaBateria

    override suspend fun marcarGuiaBateriaOfrecida() {
        guiaMarcada = true
        vecesGuiaMarcada++
    }
}
