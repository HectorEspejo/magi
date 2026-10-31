package org.cuatrodetres.magi.ui.tema

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import org.cuatrodetres.magi.R

/**
 * JetBrains Mono (OFL 1.1) en toda la app, como en la TUI. Es la fuente de la
 * cabina y también la del terminal: la familia se declara una sola vez.
 */
val FamiliaJetBrainsMono: FontFamily = FontFamily(
    Font(R.font.jetbrains_mono_regular, FontWeight.Normal, FontStyle.Normal),
    Font(R.font.jetbrains_mono_italic, FontWeight.Normal, FontStyle.Italic),
    Font(R.font.jetbrains_mono_bold, FontWeight.Bold, FontStyle.Normal),
    Font(R.font.jetbrains_mono_bold_italic, FontWeight.Bold, FontStyle.Italic),
)

private fun estilo(
    peso: FontWeight,
    tamanoSp: Int,
    espaciadoSp: Double = 0.0,
): TextStyle = TextStyle(
    fontFamily = FamiliaJetBrainsMono,
    fontWeight = peso,
    fontSize = tamanoSp.sp,
    letterSpacing = espaciadoSp.sp,
)

/** Tipografía Material 3 de MAGI: todo JetBrains Mono y cabeceras en mayúsculas. */
val TipografiaMagi: Typography = Typography(
    displayLarge = estilo(FontWeight.Bold, 40),
    displayMedium = estilo(FontWeight.Bold, 36),
    displaySmall = estilo(FontWeight.Bold, 32),
    headlineLarge = estilo(FontWeight.Bold, 28),
    headlineMedium = estilo(FontWeight.Bold, 24),
    headlineSmall = estilo(FontWeight.Bold, 20),
    titleLarge = estilo(FontWeight.Bold, 20),
    titleMedium = estilo(FontWeight.Bold, 16, 0.5),
    titleSmall = estilo(FontWeight.Bold, 14, 0.5),
    bodyLarge = estilo(FontWeight.Normal, 14),
    bodyMedium = estilo(FontWeight.Normal, 13),
    bodySmall = estilo(FontWeight.Normal, 12),
    labelLarge = estilo(FontWeight.Bold, 13, 0.5),
    labelMedium = estilo(FontWeight.Bold, 12, 0.5),
    labelSmall = estilo(FontWeight.Bold, 11, 0.5),
)
