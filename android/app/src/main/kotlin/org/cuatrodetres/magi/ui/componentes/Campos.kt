package org.cuatrodetres.magi.ui.componentes

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.tema.Paleta

/** Colores comunes de los campos de la cabina. */
@Composable
fun coloresCampo() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Paleta.AMBAR,
    unfocusedBorderColor = Paleta.GRIS,
    cursorColor = Paleta.AMBAR,
    focusedTextColor = Paleta.TEXTO,
    unfocusedTextColor = Paleta.TEXTO,
    focusedLabelColor = Paleta.AMBAR,
    unfocusedLabelColor = Paleta.GRIS,
    focusedPlaceholderColor = Paleta.GRIS,
    unfocusedPlaceholderColor = Paleta.GRIS,
)

/** Buscador de la pantalla Hosts (búsqueda por subcadena). */
@Composable
fun CampoBusqueda(
    texto: String,
    alCambiar: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    OutlinedTextField(
        value = texto,
        onValueChange = alCambiar,
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        singleLine = true,
        placeholder = { Text(stringResource(R.string.hosts_buscar)) },
        leadingIcon = { Text(text = "⌕", color = Paleta.GRIS) },
        textStyle = MaterialTheme.typography.bodyLarge,
        colors = coloresCampo(),
    )
}

/** Campo de texto monoespaciado; con [contrasena] se enmascara. */
@Composable
fun CampoTextoMagi(
    valor: String,
    alCambiar: (String) -> Unit,
    etiqueta: String,
    modifier: Modifier = Modifier,
    habilitado: Boolean = true,
    contrasena: Boolean = false,
    unaLinea: Boolean = true,
    modifierInterno: Modifier = Modifier,
) {
    OutlinedTextField(
        value = valor,
        onValueChange = alCambiar,
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).then(modifierInterno),
        enabled = habilitado,
        label = { Text(etiqueta) },
        singleLine = unaLinea,
        textStyle = MaterialTheme.typography.bodyLarge,
        visualTransformation = if (contrasena) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            keyboardType = if (contrasena) KeyboardType.Password else KeyboardType.Text,
            imeAction = ImeAction.Done,
        ),
        colors = coloresCampo(),
    )
}
