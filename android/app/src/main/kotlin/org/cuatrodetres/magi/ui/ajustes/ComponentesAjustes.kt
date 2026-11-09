package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.coloresCampo
import org.cuatrodetres.magi.ui.tema.Paleta
import java.util.Locale
import kotlin.math.roundToInt

/**
 * Piezas compartidas de Ajustes > Flota, Servicios y Deliberación (informe
 * §5.4 y §6): deslizadores con su valor y su ayuda, campos acotados, avisos de
 * la acotación y el botón de guardar. Todo en 48 dp como mínimo (T5).
 */

/** Un decimal con la coma del idioma del teléfono («1,0»). */
fun unDecimal(valor: Double): String = String.format(Locale.getDefault(), "%.1f", valor)

/** Un porcentaje sin decimales («90»). */
fun sinDecimales(valor: Double): String = String.format(Locale.getDefault(), "%.0f", valor)

/** Valores del auto-refresco de Flota: 0 (apagado) o de 15 en 15 s hasta 300. */
fun valoresRefrescoFlota(): List<Int> = listOf(0) + (15..300 step 15).toList()

/** Valores del refresco de Servicios: 0 (solo al entrar) o de 5 en 5 s hasta 300. */
fun valoresRefrescoServicios(): List<Int> = listOf(0) + (5..300 step 5).toList()

/** Valores de la espera de una acción: de 5 en 5 s entre 5 y 300. */
fun valoresEsperaServicios(): List<Int> = (5..300 step 5).toList()

/**
 * Deslizador de un ajuste entero con una lista cerrada de valores (el usuario
 * no puede salirse del rango: fuera de rango solo llega desde DataStore y se
 * acota con aviso).
 */
@Composable
fun DeslizadorEnteroAjuste(
    titulo: String,
    valores: List<Int>,
    valor: Int,
    etiquetaValor: String,
    ayuda: String,
    alCambiar: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val indice = valores.indexOf(valor).coerceAtLeast(0).coerceAtMost(valores.lastIndex)
    Column(modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = titulo, style = MaterialTheme.typography.titleSmall, color = Paleta.TEXTO)
            Text(text = etiquetaValor, style = MaterialTheme.typography.labelLarge, color = Paleta.AMBAR)
        }
        Slider(
            value = indice.toFloat(),
            onValueChange = { paso -> alCambiar(valores[paso.roundToInt().coerceIn(0, valores.lastIndex)]) },
            valueRange = 0f..valores.lastIndex.toFloat(),
            steps = (valores.size - 2).coerceAtLeast(0),
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
            colors = SliderDefaults.colors(
                thumbColor = Paleta.AMBAR,
                activeTrackColor = Paleta.AMBAR,
                inactiveTrackColor = Paleta.GRIS,
            ),
        )
        Text(text = ayuda, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
    }
}

/** Deslizador de un ajuste decimal con paso fijo (carga por núcleo). */
@Composable
fun DeslizadorDecimalAjuste(
    titulo: String,
    minimo: Double,
    maximo: Double,
    paso: Double,
    valor: Double,
    etiquetaValor: String,
    ayuda: String,
    alCambiar: (Double) -> Unit,
    modifier: Modifier = Modifier,
) {
    val pasos = (((maximo - minimo) / paso).roundToInt() - 1).coerceAtLeast(0)
    Column(modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = titulo, style = MaterialTheme.typography.titleSmall, color = Paleta.TEXTO)
            Text(text = etiquetaValor, style = MaterialTheme.typography.labelLarge, color = Paleta.AMBAR)
        }
        Slider(
            value = valor.toFloat().coerceIn(minimo.toFloat(), maximo.toFloat()),
            onValueChange = { bruto ->
                val acotado = bruto.toDouble().coerceIn(minimo, maximo)
                alCambiar((acotado / paso).roundToInt() * paso)
            },
            valueRange = minimo.toFloat()..maximo.toFloat(),
            steps = pasos,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
            colors = SliderDefaults.colors(
                thumbColor = Paleta.AMBAR,
                activeTrackColor = Paleta.AMBAR,
                inactiveTrackColor = Paleta.GRIS,
            ),
        )
        Text(text = ayuda, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
    }
}

/** Campo de un ajuste entero con su ayuda y su error de rango. */
@Composable
fun CampoEnteroAjuste(
    titulo: String,
    texto: String,
    ayuda: String,
    alCambiar: (String) -> Unit,
    modifier: Modifier = Modifier,
    error: String? = null,
) {
    Column(modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        OutlinedTextField(
            value = texto,
            onValueChange = alCambiar,
            modifier = Modifier.fillMaxWidth(),
            singleLine = true,
            label = { Text(titulo) },
            textStyle = MaterialTheme.typography.bodyLarge,
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Number,
                imeAction = ImeAction.Done,
            ),
            colors = coloresCampo(),
        )
        if (error != null) {
            Text(
                text = error,
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.ROJO,
                modifier = Modifier.padding(horizontal = 4.dp, vertical = 2.dp),
            )
        }
        Text(text = ayuda, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
    }
}

/** Casilla de un ajuste de sí o no (motores de contenedores). */
@Composable
fun CasillaAjuste(
    texto: String,
    marcada: Boolean,
    alCambiar: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clickable(
                role = Role.Checkbox,
                onClickLabel = texto,
                onClick = { alCambiar(!marcada) },
            )
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Checkbox(
            checked = marcada,
            onCheckedChange = { alCambiar(it) },
            colors = CheckboxDefaults.colors(
                checkedColor = Paleta.AMBAR,
                uncheckedColor = Paleta.GRIS,
                checkmarkColor = Paleta.FONDO,
            ),
        )
        Text(text = texto, style = MaterialTheme.typography.bodyLarge, color = Paleta.TEXTO)
    }
}

/** Texto de ayuda de una sección de ajustes. */
@Composable
fun TextoAyuda(texto: String, modifier: Modifier = Modifier) {
    Text(
        text = texto,
        style = MaterialTheme.typography.bodyMedium,
        color = Paleta.GRIS,
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
    )
}

/** Avisos de la última acotación: se enseña qué se corrigió y por qué. */
@Composable
fun AvisosDeAjustes(avisos: List<String>, modifier: Modifier = Modifier) {
    if (avisos.isEmpty()) return
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .background(Paleta.SUPERFICIE)
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(
            text = stringResource(R.string.ajustes_avisos_titulo),
            style = MaterialTheme.typography.labelLarge,
            color = Paleta.AMBAR,
        )
        for (aviso in avisos) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(text = "!", style = MaterialTheme.typography.labelLarge, color = Paleta.AMBAR)
                Text(
                    text = aviso,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.TEXTO,
                )
            }
        }
    }
}

/** Pie de una pantalla de ajustes: la ayuda y el botón de guardar. */
@Composable
fun PieGuardarAjustes(
    ayuda: String,
    hayCambios: Boolean,
    alGuardar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxWidth().padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(text = ayuda, style = MaterialTheme.typography.bodyMedium, color = Paleta.GRIS)
        BotonCabina(
            texto = stringResource(R.string.ajustes_guardar),
            alPulsar = alGuardar,
            habilitado = hayCambios,
            primario = true,
            modifier = Modifier.fillMaxWidth(),
        )
        if (!hayCambios) {
            Text(
                text = stringResource(R.string.ajustes_sin_cambios),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.GRIS,
            )
        }
    }
}
