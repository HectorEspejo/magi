package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.modelos.LICENCIAS_BASE
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Ajustes > Acerca de > Licencias.
 *
 * El texto completo lo genera la compilación en R.raw.licencias (dependencias
 * Kotlin/AndroidX, crates de Rust y la OFL de JetBrains Mono). Si esa tarea no
 * estuviera disponible, se enseña la lista base y la OFL que hay en res/raw.
 */
@Composable
fun PantallaLicencias(
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val contexto = LocalContext.current
    var licenciasGeneradas by remember { mutableStateOf<String?>(null) }
    var ofl by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        licenciasGeneradas = withContext(Dispatchers.IO) {
            runCatching {
                contexto.resources.openRawResource(R.raw.licencias).bufferedReader().use { it.readText() }
            }.getOrNull()
        }
        ofl = withContext(Dispatchers.IO) {
            runCatching {
                contexto.resources.openRawResource(R.raw.licencia_jetbrains_mono)
                    .bufferedReader().use { it.readText() }
            }.getOrNull()
        }
    }
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = stringResource(R.string.licencias_titulo),
            alVolver = alVolver,
        )
        Text(
            text = stringResource(R.string.licencias_explicacion),
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
        val generadas = licenciasGeneradas
        if (generadas != null) {
            SelectionContainer {
                Text(
                    text = generadas,
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.TEXTO,
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(Paleta.SUPERFICIE)
                        .padding(12.dp),
                )
            }
        } else {
            LICENCIAS_BASE.forEach { entrada ->
                Text(
                    text = entrada.nombre + " · " + entrada.version + " · " + entrada.licencia,
                    style = MaterialTheme.typography.bodySmall,
                    color = Paleta.TEXTO,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
                )
            }
            Separador()
            val textoOfl = ofl
            if (textoOfl != null) {
                SelectionContainer {
                    Text(
                        text = textoOfl,
                        style = MaterialTheme.typography.bodySmall,
                        color = Paleta.TEXTO,
                        modifier = Modifier.padding(12.dp),
                    )
                }
            }
        }
    }
}
