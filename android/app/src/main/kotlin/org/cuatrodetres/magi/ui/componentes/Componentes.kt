package org.cuatrodetres.magi.ui.componentes

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Piezas comunes de la cabina (informe §6 y notas de UX).
 *
 * Toda la app usa estas cabeceras, filas, chips y botones para que la estética
 * sea una sola: cabeceras en mayúsculas, glifo + texto + color (T5) y objetivos
 * táctiles de 48 dp.
 */

/** Cabecera de primer nivel: título, indicador ⇅ y menú (mockup §6.1). */
@Composable
fun CabeceraMagi(
    titulo: String,
    modifier: Modifier = Modifier,
    indicador: String? = null,
    colorIndicador: Color = Paleta.GRIS,
    alAbrirMenu: (() -> Unit)? = null,
) {
    Row(
        modifier = modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = titulo,
            style = MaterialTheme.typography.titleMedium,
            color = Paleta.TEXTO,
        )
        Spacer(Modifier.weight(1f))
        if (indicador != null) {
            Text(
                text = indicador,
                style = MaterialTheme.typography.labelMedium,
                color = colorIndicador,
            )
        }
        if (alAbrirMenu != null) {
            BotonGlifo(
                glifo = "⋮",
                descripcion = stringResource(R.string.accion_abrir_menu),
                alPulsar = alAbrirMenu,
            )
        }
    }
}

/** Cabecera con «atrás», estado a la derecha y acciones opcionales (§6.2 y §6.3). */
@Composable
fun CabeceraVolver(
    titulo: String,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
    glifoEstado: String? = null,
    textoEstado: String? = null,
    colorEstado: Color = Paleta.GRIS,
    acciones: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BotonGlifo(
            glifo = "←",
            descripcion = stringResource(R.string.accion_volver),
            alPulsar = alVolver,
        )
        Text(
            text = titulo,
            style = MaterialTheme.typography.titleMedium,
            color = Paleta.TEXTO,
            modifier = Modifier.weight(1f, fill = false),
        )
        Spacer(Modifier.weight(1f))
        if (textoEstado != null || glifoEstado != null) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (glifoEstado != null) {
                    Text(
                        text = glifoEstado,
                        style = MaterialTheme.typography.titleMedium,
                        color = colorEstado,
                    )
                }
                if (textoEstado != null) {
                    Text(
                        text = textoEstado,
                        style = MaterialTheme.typography.labelMedium,
                        color = colorEstado,
                    )
                }
            }
        }
        acciones()
    }
}

/** Título de sección en mayúsculas, como en la TUI. */
@Composable
fun EncabezadoSeccion(texto: String, modifier: Modifier = Modifier) {
    Text(
        text = texto,
        style = MaterialTheme.typography.labelLarge,
        color = Paleta.GRIS,
        modifier = modifier.padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

/** Botón con glifo de la cabina; el área táctil nunca baja de 48 dp. */
@Composable
fun BotonGlifo(
    glifo: String,
    descripcion: String,
    alPulsar: () -> Unit,
    modifier: Modifier = Modifier,
    habilitado: Boolean = true,
    color: Color = Paleta.AMBAR,
) {
    Box(
        modifier = modifier
            .sizeIn(minWidth = 48.dp, minHeight = 48.dp)
            .clip(RoundedCornerShape(8.dp))
            .clickable(enabled = habilitado, onClickLabel = descripcion, role = Role.Button, onClick = alPulsar)
            .semantics {
                contentDescription = descripcion
                role = Role.Button
            }
            .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = glifo, style = MaterialTheme.typography.titleMedium, color = color)
    }
}

/** Botón principal (ámbar) o secundario (contorno) de la cabina. */
@Composable
fun BotonCabina(
    texto: String,
    alPulsar: () -> Unit,
    modifier: Modifier = Modifier,
    habilitado: Boolean = true,
    primario: Boolean = false,
    destructivo: Boolean = false,
) {
    val colorTexto = if (destructivo) Paleta.ROJO else Paleta.AMBAR
    if (primario || destructivo) {
        Button(
            onClick = alPulsar,
            enabled = habilitado,
            modifier = modifier.heightIn(min = 48.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = colorTexto,
                contentColor = Paleta.FONDO,
                disabledContainerColor = Paleta.GRIS.copy(alpha = 0.4f),
                disabledContentColor = Paleta.FONDO.copy(alpha = 0.7f),
            ),
        ) {
            Text(text = texto, style = MaterialTheme.typography.labelLarge)
        }
    } else {
        OutlinedButton(
            onClick = alPulsar,
            enabled = habilitado,
            modifier = modifier.heightIn(min = 48.dp),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = colorTexto),
        ) {
            Text(text = texto, style = MaterialTheme.typography.labelLarge)
        }
    }
}

/** Fila de dato de una ficha: etiqueta a la izquierda y valor monoespaciado. */
@Composable
fun FilaDato(
    etiqueta: String,
    valor: String,
    modifier: Modifier = Modifier,
    colorValor: Color = Paleta.TEXTO,
) {
    Row(
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = etiqueta,
            style = MaterialTheme.typography.labelMedium,
            color = Paleta.GRIS,
            modifier = Modifier.widthIn(min = 84.dp),
        )
        Text(
            text = valor,
            style = MaterialTheme.typography.bodyMedium,
            color = colorValor,
            modifier = Modifier.weight(1f),
        )
    }
}

/** Chip de etiqueta del inventario ([web], [prod]). */
@Composable
fun ChipEtiqueta(texto: String, modifier: Modifier = Modifier) {
    Surface(
        color = Paleta.SUPERFICIE,
        shape = RoundedCornerShape(4.dp),
        modifier = modifier,
    ) {
        Text(
            text = texto,
            style = MaterialTheme.typography.labelSmall,
            color = Paleta.TEXTO,
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

/** Estado con glifo y texto a la vez (T5). */
@Composable
fun ChipEstado(
    glifo: String,
    texto: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(text = glifo, style = MaterialTheme.typography.labelLarge, color = color)
        Text(text = texto, style = MaterialTheme.typography.labelMedium, color = color)
    }
}

/** Bloque monoespaciado para huellas y líneas largas en los diálogos. */
@Composable
fun Chiptexto(texto: String, modifier: Modifier = Modifier) {
    Text(
        text = texto,
        style = MaterialTheme.typography.bodySmall,
        color = Paleta.TEXTO,
        modifier = modifier
            .fillMaxWidth()
            .background(Paleta.SUPERFICIE)
            .padding(horizontal = 8.dp, vertical = 6.dp),
    )
}

/** Separador de secciones. */
@Composable
fun Separador(modifier: Modifier = Modifier) {
    HorizontalDivider(
        modifier = modifier.padding(vertical = 8.dp),
        thickness = 1.dp,
        color = Paleta.GRIS.copy(alpha = 0.35f),
    )
}

/** Tipo de la barra de aviso estática (no confundir con el snackbar efímero). */
enum class TipoBarra { INFO, AVISO, ERROR }

/** Barra de aviso o error con acción opcional e instrucción (§8). */
@Composable
fun BarraAviso(
    texto: String,
    modifier: Modifier = Modifier,
    tipo: TipoBarra = TipoBarra.INFO,
    glifo: String? = null,
    textoAccion: String? = null,
    alAccion: (() -> Unit)? = null,
    alCerrar: (() -> Unit)? = null,
) {
    val color = when (tipo) {
        TipoBarra.INFO -> Paleta.GRIS
        TipoBarra.AVISO -> Paleta.AMBAR
        TipoBarra.ERROR -> Paleta.ROJO
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 4.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(Paleta.SUPERFICIE)
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = glifo ?: when (tipo) {
                TipoBarra.ERROR -> "✕"
                TipoBarra.AVISO -> "!"
                TipoBarra.INFO -> "⇅"
            },
            style = MaterialTheme.typography.labelLarge,
            color = color,
        )
        Text(
            text = texto,
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.TEXTO,
            modifier = Modifier.weight(1f),
        )
        if (textoAccion != null && alAccion != null) {
            BotonCabina(texto = textoAccion, alPulsar = alAccion, modifier = Modifier.heightIn(min = 48.dp))
        }
        if (alCerrar != null) {
            BotonGlifo(
                glifo = "✕",
                descripcion = stringResource(R.string.accion_cerrar_aviso),
                alPulsar = alCerrar,
                color = Paleta.GRIS,
            )
        }
    }
}

/** Indicador de carga con mensaje (conectando, sincronizando…). */
@Composable
fun IndicadorCarga(texto: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(20.dp),
            color = Paleta.AMBAR,
            strokeWidth = 2.dp,
        )
        Text(text = texto, style = MaterialTheme.typography.bodyMedium, color = Paleta.TEXTO)
    }
}

/** Estado vacío con explicación y, si aplica, botón de acción (§8). */
@Composable
fun MensajeVacio(
    titulo: String,
    detalle: String,
    modifier: Modifier = Modifier,
    glifo: String = "⌗",
    textoBoton: String? = null,
    alPulsar: (() -> Unit)? = null,
) {
    Column(
        modifier = modifier.fillMaxWidth().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text = glifo, style = MaterialTheme.typography.headlineMedium, color = Paleta.GRIS)
        Text(
            text = titulo,
            style = MaterialTheme.typography.titleMedium,
            color = Paleta.TEXTO,
        )
        Text(
            text = detalle,
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.GRIS,
        )
        if (textoBoton != null && alPulsar != null) {
            BotonCabina(
                texto = textoBoton,
                alPulsar = alPulsar,
                primario = true,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}
