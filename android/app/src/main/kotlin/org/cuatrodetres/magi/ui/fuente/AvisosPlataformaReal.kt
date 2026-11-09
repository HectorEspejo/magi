package org.cuatrodetres.magi.ui.fuente

import org.cuatrodetres.magi.plataforma.GuiaBateria
import org.cuatrodetres.magi.preferencias.PreferenciasMagi
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.AvisosPlataforma

/**
 * Adaptador de los avisos del segundo plano y de los ajustes de la Fase 14
 * sobre los helpers de plataforma.
 *
 * - El trabajo que quedó abierto (sesiones, acciones y ejecuciones) lo persiste
 *   VistaModeloMagi y lo consume PreferenciasMagi en una sola edición por
 *   recuento (lectura + borrado), para avisar al volver (informe §7.12).
 * - La guía de batería la decide GuiaBateria (no exento + no vista) y se marca
 *   al ofrecerla.
 * - Los ajustes de Flota, Servicios y Deliberación viven en DataStore (§5.4) y
 *   se guardan ya acotados.
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

    override suspend fun claveMovilRecreada(): Boolean =
        preferencias.consumirAvisoClaveRecreada()

    override suspend fun accionesQueQuedaronAbiertas(): Int =
        preferencias.consumirAccionesAbiertas()

    override suspend fun ejecucionesQueQuedaronAbiertas(): Int =
        preferencias.consumirEjecucionesAbiertas()

    override suspend fun guardarTrabajoAbierto(sesiones: Int, acciones: Int, ejecuciones: Int) {
        preferencias.guardarTrabajoAbierto(sesiones, acciones, ejecuciones)
    }

    override suspend fun ajustesMovil(): AjustesMovilVista =
        preferencias.primeraLectura().ajustesMovil

    override suspend fun guardarAjustesMovil(ajustes: AjustesMovilVista): List<String> =
        preferencias.guardarAjustesMovil(ajustes)
}
