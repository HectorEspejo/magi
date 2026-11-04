package org.cuatrodetres.magi.plataforma

import android.content.ClipboardManager
import android.content.Context

/**
 * Acceso al portapapeles del sistema para el flujo de la clave de
 * sincronización (informe §4.1): tras pegar la clave, se vacía para que no
 * quede un secreto a la vista de otras apps.
 *
 * Solo se vacía si lo copiado coincide con la clave usada: nunca se borra el
 * portapapeles del usuario porque sí.
 */
class Portapapeles(private val contexto: Context) {

    /** Texto actual del portapapeles, o null si está vacío o no se puede leer. */
    fun leerTexto(): String? = try {
        val gestor = contexto.getSystemService(ClipboardManager::class.java) ?: return null
        val portapapeles = gestor.primaryClip ?: return null
        if (portapapeles.itemCount == 0) null
        else portapapeles.getItemAt(0).coerceToText(contexto)?.toString()?.takeIf { it.isNotEmpty() }
    } catch (_: SecurityException) {
        // Android 13+ puede negar la lectura si la app no está en primer plano.
        null
    }

    /** Vacía el portapapeles si contiene exactamente el texto indicado. */
    fun vaciarSiContiene(texto: String) {
        val actual = leerTexto()?.trim() ?: return
        if (actual == texto.trim()) vaciar()
    }

    /** Vacía el portapapeles del sistema (sin mostrar la clave ni dejarla). */
    fun vaciar() {
        try {
            val gestor = contexto.getSystemService(ClipboardManager::class.java) ?: return
            gestor.clearPrimaryClip()
        } catch (_: SecurityException) {
            // Sin permiso de portapapeles no hay nada que vaciar.
        }
    }
}
