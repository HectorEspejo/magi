package org.cuatrodetres.magi.ui.terminal

import androidx.compose.ui.graphics.Color
import org.cuatrodetres.magi.ui.modelos.ColorVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Color del terminal (informe §6.7): los 16 fijos de la cabina, los índices
 * 16-255 de la tabla estándar de xterm y los de 24 bits tal cual.
 */

/** Niveles del cubo de 6×6×6 de xterm (16-231). */
private val NIVELES: IntArray = intArrayOf(0, 95, 135, 175, 215, 255)

/** Color de la paleta de 256 colores de xterm. */
fun colorIndice(indice: Int): Color {
    val i = indice.coerceIn(0, 255)
    if (i < 16) return Paleta.TERMINAL[i]
    if (i < 232) {
        val n = i - 16
        return Color(
            red = NIVELES[n / 36],
            green = NIVELES[(n % 36) / 6],
            blue = NIVELES[n % 6],
        )
    }
    val gris = 8 + (i - 232) * 10
    return Color(red = gris, green = gris, blue = gris)
}

/** Color de un tramo; el predeterminado es el texto de la cabina. */
fun colorDe(color: ColorVista): Color = when (color) {
    ColorVista.Predeterminado -> Paleta.TEXTO
    is ColorVista.Indice -> colorIndice(color.indice)
    is ColorVista.Rgb -> Color(
        red = color.rojo.coerceIn(0, 255),
        green = color.verde.coerceIn(0, 255),
        blue = color.azul.coerceIn(0, 255),
    )
}

/** Color de fondo de un tramo; null si es el fondo de la app (no se pinta). */
fun colorFondoDe(color: ColorVista): Color? = when (color) {
    ColorVista.Predeterminado -> null
    else -> colorDe(color)
}
