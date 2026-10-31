package org.cuatrodetres.magi.ui.modelos

/**
 * Lo que la interfaz necesita de la plataforma para los avisos del segundo
 * plano (informe §4.6 y checklist «Segundo plano»).
 *
 * La implementación real la pone MainActivity sobre PreferenciasMagi y
 * GuiaBateria; en la JVM se sustituye por un doble. Si no se inyecta (app sin
 * cablear o previsualización), los avisos simplemente no se muestran.
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
}
