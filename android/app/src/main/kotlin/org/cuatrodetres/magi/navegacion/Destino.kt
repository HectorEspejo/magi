package org.cuatrodetres.magi.navegacion

import androidx.annotation.StringRes
import org.cuatrodetres.magi.R

/**
 * Destinos de primer nivel de MAGI (informe §6): Hosts · Sesiones · Túneles ·
 * Ajustes. En pantalla compacta se pinta la barra inferior; en plegable abierto
 * o pantalla ancha, el riel de navegación.
 */
enum class Destino(
    @param:StringRes val etiqueta: Int,
    /** Glifo de texto: la cabina no usa iconos de relleno, glifo y texto a la vez (T5). */
    val glifo: String,
) {
    HOSTS(R.string.destino_hosts, "▣"),
    SESIONES(R.string.destino_sesiones, "▤"),
    TUNELES(R.string.destino_tuneles, "⇄"),
    AJUSTES(R.string.destino_ajustes, "⚙"),
}
