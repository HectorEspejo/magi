package org.cuatrodetres.magi.ui.modelos

/**
 * Lo que la interfaz necesita de la plataforma para los avisos del segundo
 * plano, los ajustes de la Fase 14 y el trabajo que quedó en curso (informe
 * §4.6, §5.4 y §7.12).
 *
 * La implementación real la pone AppMagi sobre PreferenciasMagi y GuiaBateria;
 * en la JVM se sustituye por un doble. Si no se inyecta (app sin cablear o
 * previsualización), los avisos no se muestran y los ajustes se quedan en los
 * valores por defecto (que acota igualmente la interfaz).
 */
interface AvisosPlataforma {

    /**
     * Cuántas sesiones quedaron abiertas en la última ejecución y las reinicia
     * (la señal de «Android cerró MAGI con N sesiones»).
     */
    suspend fun sesionesQueQuedaronAbiertas(): Int

    /** Si toca ofrecer la guía de batería (primera sesión y nunca vista). */
    suspend fun debeOfrecerseGuiaBateria(): Boolean

    /** Marca la guía como vista: no se vuelve a ofrecer sola. */
    suspend fun marcarGuiaBateriaOfrecida()

    /**
     * Si la clave del móvil se recreó al actualizar (cambio de identidad):
     * hay que avisar de que se reinstale en los hosts. Sin implementación,
     * no se muestra.
     */
    suspend fun claveMovilRecreada(): Boolean = false

    /**
     * Acciones de servicio que quedaron en curso cuando Android cerró MAGI y
     * las reinicia (informe §7.12). Sin implementación, se da por cero.
     */
    suspend fun accionesQueQuedaronAbiertas(): Int = 0

    /**
     * Ejecuciones de snippets que quedaron en curso cuando Android cerró MAGI y
     * las reinicia (informe §7.12). Sin implementación, se da por cero.
     */
    suspend fun ejecucionesQueQuedaronAbiertas(): Int = 0

    /**
     * Guarda el trabajo vivo (sesiones, acciones y ejecuciones) para poder avisar
     * si Android cierra MAGI mientras sigue en curso (informe §7.12).
     */
    suspend fun guardarTrabajoAbierto(sesiones: Int, acciones: Int, ejecuciones: Int) = Unit

    /**
     * Ajustes de Flota, Servicios y Deliberación guardados en DataStore
     * (informe §5.4). Sin implementación, los valores por defecto.
     */
    suspend fun ajustesMovil(): AjustesMovilVista = AjustesMovilVista.POR_DEFECTO

    /**
     * Guarda los ajustes ya acotados y devuelve un aviso por cada valor que se
     * corrigió (informe §5.4). Sin implementación, no persiste nada.
     */
    suspend fun guardarAjustesMovil(ajustes: AjustesMovilVista): List<String> = emptyList()
}

/**
 * Trabajo que Android cerró con MAGI: acciones y ejecuciones que seguían en
 * curso y pueden haberse hecho en el host (informe §7.12).
 */
data class TrabajoPerdidoVista(
    val acciones: Int = 0,
    val ejecuciones: Int = 0,
) {
    /** Si hay algo que avisar. */
    val hayTrabajo: Boolean get() = acciones > 0 || ejecuciones > 0
}
