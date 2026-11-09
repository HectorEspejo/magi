package org.cuatrodetres.magi.ui.dobles

import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.AvisosPlataforma
import org.cuatrodetres.magi.ui.modelos.acotarAjustes

/**
 * Avisos del segundo plano y ajustes de la Fase 14 para pruebas: decide y
 * anota sin DataStore.
 */
class AvisosPlataformaFalso(
    var sesionesPerdidas: Int = 0,
    var ofrecerGuiaBateria: Boolean = false,
    /** Acciones que Android cerró con MAGI (§7.12). */
    var accionesPerdidas: Int = 0,
    /** Ejecuciones que Android cerró con MAGI (§7.12). */
    var ejecucionesPerdidas: Int = 0,
    /** Ajustes que devuelve DataStore al arrancar o al volver. */
    var ajustesGuardados: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
    /** Avisos que devuelve guardarAjustesMovil (además de los de la acotación). */
    var avisosAlGuardar: List<String> = emptyList(),
) : AvisosPlataforma {

    var guiaMarcada: Boolean = false
    var vecesGuiaMarcada: Int = 0

    /** Último trabajo anotado para el próximo arranque. */
    var trabajoGuardado: Triple<Int, Int, Int>? = null

    /** Veces que se pidieron los ajustes guardados. */
    var vecesAjustesLeidos: Int = 0

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

    override suspend fun accionesQueQuedaronAbiertas(): Int {
        val perdidas = accionesPerdidas
        accionesPerdidas = 0
        return perdidas
    }

    override suspend fun ejecucionesQueQuedaronAbiertas(): Int {
        val perdidas = ejecucionesPerdidas
        ejecucionesPerdidas = 0
        return perdidas
    }

    override suspend fun guardarTrabajoAbierto(sesiones: Int, acciones: Int, ejecuciones: Int) {
        trabajoGuardado = Triple(sesiones, acciones, ejecuciones)
    }

    override suspend fun ajustesMovil(): AjustesMovilVista {
        vecesAjustesLeidos++
        return ajustesGuardados
    }

    override suspend fun guardarAjustesMovil(ajustes: AjustesMovilVista): List<String> {
        val acotado = acotarAjustes(ajustes)
        ajustesGuardados = acotado.ajustes
        return acotado.avisos + avisosAlGuardar
    }
}
