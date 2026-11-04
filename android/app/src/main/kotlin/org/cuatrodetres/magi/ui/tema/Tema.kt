package org.cuatrodetres.magi.ui.tema

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

/** Esquema Material 3 de MAGI: solo tema oscuro con la paleta fija de la cabina. */
private val EsquemaMagi = darkColorScheme(
    primary = Paleta.AMBAR,
    onPrimary = Paleta.FONDO,
    primaryContainer = Paleta.AMBAR,
    onPrimaryContainer = Paleta.FONDO,
    secondary = Paleta.GRIS,
    onSecondary = Paleta.TEXTO,
    tertiary = Paleta.VERDE,
    onTertiary = Paleta.FONDO,
    background = Paleta.FONDO,
    onBackground = Paleta.TEXTO,
    surface = Paleta.FONDO,
    onSurface = Paleta.TEXTO,
    surfaceVariant = Paleta.SUPERFICIE,
    onSurfaceVariant = Paleta.TEXTO,
    outline = Paleta.GRIS,
    outlineVariant = Paleta.GRIS,
    error = Paleta.ROJO,
    onError = Paleta.FONDO,
)

/**
 * Tema de MAGI: la cabina sobre Material 3. No hay variante clara ni modo
 * dinámico (la paleta es fija); las pantallas se pintan con [Paleta] o con el
 * esquema de Material según convenga.
 */
@Composable
fun TemaMagi(contenido: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = EsquemaMagi,
        typography = TipografiaMagi,
        content = contenido,
    )
}
