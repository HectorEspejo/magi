package org.cuatrodetres.magi.ui.terminal

import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.utf16CodePoint
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.tema.FamiliaJetBrainsMono

/**
 * Entrada en bruto del teclado del sistema (informe §6.3 y §7.6).
 *
 * El campo de texto es invisible: todo lo que se escribe se manda al núcleo,
 * que codifica las teclas según el modo del terminal. Un centinela de ancho cero
 * permite detectar el borrado del teclado virtual; el teclado físico se
 * intercepta antes de que el campo lo procese (Ctrl+Mayús+C/V copian y pegan en
 * el móvil, no en el remoto).
 */
@Composable
fun EntradaTerminal(
    alEscribir: (String) -> Unit,
    alTecla: (TeclaVista, ModificadoresVista) -> Unit,
    alCopiar: () -> Unit,
    alPegar: () -> Unit,
    foco: FocusRequester,
    modifier: Modifier = Modifier,
) {
    var campo by remember { mutableStateOf(TextFieldValue(SENTINELA, selection = TextRange(SENTINELA.length))) }
    BasicTextField(
        value = campo,
        onValueChange = { nuevo ->
            if (nuevo.text.length < SENTINELA.length) {
                alTecla(TeclaVista.Retroceso, ModificadoresVista.NINGUNO)
            } else {
                val insertado = nuevo.text.removePrefix(SENTINELA)
                if (insertado.isNotEmpty()) alEscribir(insertado)
            }
            campo = TextFieldValue(SENTINELA, selection = TextRange(SENTINELA.length))
        },
        modifier = modifier
            .size(1.dp)
            .alpha(0f)
            .focusRequester(foco)
            .onPreviewKeyEvent { evento -> manejarTeclaFisica(evento, alCopiar, alPegar, alTecla) },
        textStyle = TextStyle(
            fontFamily = FamiliaJetBrainsMono,
            fontSize = 1.sp,
            color = Color.Transparent,
        ),
        cursorBrush = SolidColor(Color.Transparent),
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.None,
            autoCorrectEnabled = false,
            keyboardType = KeyboardType.Ascii,
            imeAction = ImeAction.None,
        ),
        singleLine = false,
    )
}

/** Centinela de ancho cero que vive siempre en el campo invisible. */
private const val SENTINELA: String = "\u200B"

/** Traduce una tecla física a la tecla que entiende el núcleo, si es especial. */
internal fun teclaEspecial(key: Key): TeclaVista? = when (key) {
    Key.Escape -> TeclaVista.Escape
    Key.Tab -> TeclaVista.Tab
    Key.Enter -> TeclaVista.Intro
    Key.Backspace -> TeclaVista.Retroceso
    Key.DirectionUp -> TeclaVista.Arriba
    Key.DirectionDown -> TeclaVista.Abajo
    Key.DirectionLeft -> TeclaVista.Izquierda
    Key.DirectionRight -> TeclaVista.Derecha
    Key.MoveHome -> TeclaVista.Inicio
    Key.MoveEnd -> TeclaVista.Fin
    Key.PageUp -> TeclaVista.RePag
    Key.PageDown -> TeclaVista.AvPag
    Key.Delete -> TeclaVista.Suprimir
    Key.Insert -> TeclaVista.Insertar
    Key.F1 -> TeclaVista.F(1)
    Key.F2 -> TeclaVista.F(2)
    Key.F3 -> TeclaVista.F(3)
    Key.F4 -> TeclaVista.F(4)
    Key.F5 -> TeclaVista.F(5)
    Key.F6 -> TeclaVista.F(6)
    Key.F7 -> TeclaVista.F(7)
    Key.F8 -> TeclaVista.F(8)
    Key.F9 -> TeclaVista.F(9)
    Key.F10 -> TeclaVista.F(10)
    Key.F11 -> TeclaVista.F(11)
    Key.F12 -> TeclaVista.F(12)
    else -> null
}

/**
 * Carácter que produce una tecla con Ctrl o Alt. Con Ctrl, Android devuelve el
 * código de control (1-26); se recupera la letra. Sin Ctrl, el punto de código.
 */
internal fun caracterDeTecla(evento: KeyEvent): String? {
    val codigo = evento.utf16CodePoint
    if (codigo in 1..26) return ('a' + (codigo - 1)).toString()
    if (codigo in 0x20..0x7E) return codigo.toChar().toString()
    return when (evento.key) {
        Key.Spacebar -> " "
        else -> null
    }
}

private fun manejarTeclaFisica(
    evento: KeyEvent,
    alCopiar: () -> Unit,
    alPegar: () -> Unit,
    alTecla: (TeclaVista, ModificadoresVista) -> Unit,
): Boolean {
    if (evento.type != KeyEventType.KeyDown) return false
    val ctrl = evento.isCtrlPressed
    val alt = evento.isAltPressed
    val mayus = evento.isShiftPressed

    // Atajos de la app: copiar y pegar en el móvil, nunca en el remoto.
    if (ctrl && mayus) {
        when (evento.key) {
            Key.C -> {
                alCopiar()
                return true
            }
            Key.V -> {
                alPegar()
                return true
            }
            else -> Unit
        }
    }
    val especial = teclaEspecial(evento.key)
    if (especial != null) {
        alTecla(especial, ModificadoresVista(ctrl = ctrl, alt = alt, mayus = mayus))
        return true
    }
    if (ctrl || alt) {
        val caracter = caracterDeTecla(evento)
        if (caracter != null) {
            alTecla(TeclaVista.Caracter(caracter), ModificadoresVista(ctrl = ctrl, alt = alt, mayus = mayus))
            return true
        }
        return false
    }
    // Teclas imprimibles: texto directo al terminal (modo bruto, sin autocorrección).
    val codigo = evento.utf16CodePoint
    if (codigo in 0x20..0x7E) {
        alTecla(TeclaVista.Caracter(codigo.toChar().toString()), ModificadoresVista.NINGUNO)
        return true
    }
    return false
}
