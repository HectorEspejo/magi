package org.cuatrodetres.magi.ui.terminal

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context

/**
 * Portapapeles del sistema para copiar la selección y pegar en el terminal.
 * El terminal no es un campo de texto normal: leer y escribir pasa por aquí.
 */

/** Texto del portapapeles; null si está vacío o no se puede leer. */
fun leerPortapapeles(contexto: Context): String? {
    val gestor = contexto.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return null
    val portapapeles = gestor.primaryClip ?: return null
    if (portapapeles.itemCount == 0) return null
    return portapapeles.getItemAt(0).coerceToText(contexto)?.toString()?.takeIf { it.isNotEmpty() }
}

/** Copia texto al portapapeles del sistema. */
fun escribirPortapapeles(contexto: Context, texto: String) {
    val gestor = contexto.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    gestor.setPrimaryClip(ClipData.newPlainText("MAGI", texto))
}
