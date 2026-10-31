package org.cuatrodetres.magi.ui.tema

import androidx.compose.ui.graphics.Color

/**
 * Paleta fija de la cabina de MAGI (Fase 1, D6; informe §6.7 y notas de UX).
 *
 * Solo hay tema oscuro: ámbar para lo activo, verde para lo correcto, rojo para
 * lo crítico, gris para lo inactivo y texto hueso sobre negro. Estos valores no
 * se cambian por pantalla: toda la app los toma de aquí.
 */
object Paleta {

    /** Fondo de la app y del terminal (#0A0A0A). */
    val FONDO: Color = Color(0xFF0A0A0A)

    /** Superficie derivada del fondo para tarjetas de Material 3. */
    val SUPERFICIE: Color = Color(0xFF141414)

    /** Texto principal (#EDE8DC). */
    val TEXTO: Color = Color(0xFFEDE8DC)

    /** Ámbar de la cabina: acento, cursor y selección (#FFA000). */
    val AMBAR: Color = Color(0xFFFFA000)

    /** Verde: estado correcto y conectado (#00FF6A). */
    val VERDE: Color = Color(0xFF00FF6A)

    /** Rojo: error y crítico (#FF2A1A). */
    val ROJO: Color = Color(0xFFFF2A1A)

    /** Gris: inactivo y secundario (#5A5A5A). */
    val GRIS: Color = Color(0xFF5A5A5A)

    /** Selección de texto: ámbar al 35 % (informe §6.7). */
    val SELECCION: Color = AMBAR.copy(alpha = 0.35f)

    /**
     * Paleta del terminal: los 16 colores fijos del informe §6.7. Los índices
     * 16-255 siguen la tabla estándar de xterm y los de 24 bits se pintan tal cual.
     */
    val TERMINAL: List<Color> = listOf(
        Color(0xFF1C1C1C), // 0  negro
        Color(0xFFFF2A1A), // 1  rojo
        Color(0xFF00FF6A), // 2  verde
        Color(0xFFFFA000), // 3  amarillo
        Color(0xFF4D8DFF), // 4  azul
        Color(0xFFD66BFF), // 5  magenta
        Color(0xFF2ED8E6), // 6  cian
        Color(0xFFEDE8DC), // 7  blanco
        Color(0xFF5A5A5A), // 8  negro brillante
        Color(0xFFFF6B5E), // 9  rojo brillante
        Color(0xFF6BFFA6), // 10 verde brillante
        Color(0xFFFFC65C), // 11 amarillo brillante
        Color(0xFF8AB4FF), // 12 azul brillante
        Color(0xFFE6A3FF), // 13 magenta brillante
        Color(0xFF8CEEF5), // 14 cian brillante
        Color(0xFFFFFFFF), // 15 blanco brillante
    )
}
