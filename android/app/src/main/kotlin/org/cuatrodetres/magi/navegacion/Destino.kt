package org.cuatrodetres.magi.navegacion

import androidx.annotation.StringRes
import org.cuatrodetres.magi.R

/**
 * Destinos de primer nivel de MAGI (informe §6, D265): Flota · Sesiones ·
 * Túneles · Servicios · Más. En pantalla compacta se pinta la barra inferior;
 * en plegable abierto o pantalla ancha, el riel de navegación.
 *
 * Flota sustituye a Hosts y es la pantalla de inicio. Más agrupa Snippets,
 * Resultados, Registro y Ajustes (cinco destinos es el máximo de Material).
 */
enum class Destino(
    @param:StringRes val etiqueta: Int,
    /** Glifo de texto: la cabina no usa iconos de relleno, glifo y texto a la vez (T5). */
    val glifo: String,
) {
    FLOTA(R.string.destino_flota, "▣"),
    SESIONES(R.string.destino_sesiones, "▤"),
    TUNELES(R.string.destino_tuneles, "⇄"),
    SERVICIOS(R.string.destino_servicios, "▦"),
    MAS(R.string.destino_mas, "⋯"),
}
