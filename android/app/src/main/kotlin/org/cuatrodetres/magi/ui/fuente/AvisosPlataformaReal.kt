package org.cuatrodetres.magi.ui.fuente

import org.cuatrodetres.magi.plataforma.GuiaBateria
import org.cuatrodetres.magi.preferencias.PreferenciasMagi
import org.cuatrodetres.magi.ui.modelos.AvisosPlataforma

/**
 * Adaptador de los avisos del segundo plano sobre los helpers de plataforma.
 *
 * - Las sesiones que quedaron abiertas las persiste MagiApp y las consume
 *   PreferenciasMagi en una sola edición (lectura + borrado).
 * - La guía de batería la decide GuiaBateria (no exento + no vista) y se marca
 *   al ofrecerla.
 */
class AvisosPlataformaReal(
    private val preferencias: PreferenciasMagi,
    private val guiaBateria: GuiaBateria,
) : AvisosPlataforma {

    override suspend fun sesionesQueQuedaronAbiertas(): Int =
        preferencias.consumirSesionesAbiertas()

    override suspend fun debeOfrecerseGuiaBateria(): Boolean = guiaBateria.debeOfrecerse()

    override suspend fun marcarGuiaBateriaOfrecida() {
        guiaBateria.marcarOfrecida()
    }
}
