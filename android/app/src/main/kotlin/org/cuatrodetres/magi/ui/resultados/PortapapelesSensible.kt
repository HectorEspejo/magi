package org.cuatrodetres.magi.ui.resultados

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.PersistableBundle

/**
 * Copia la salida de una ejecución al portapapeles marcándola como sensible
 * (informe §6.9): la salida de un comando remoto es dato del host y no debe
 * quedar a la vista de otras apps en la vista previa del portapapeles.
 *
 * En Android 13+ la marca llega con EXTRA_IS_SENSITIVE; en versiones anteriores
 * se deja la clave en los extras (el sistema la ignora, pero el contrato no
 * cambia). Nunca va al registro ni al log.
 */
fun copiarSensible(contexto: Context, texto: String): Boolean = try {
    val gestor = contexto.getSystemService(ClipboardManager::class.java) ?: return false
    if (texto.isEmpty()) return false
    val clip = ClipData.newPlainText("MAGI · salida", texto)
    val extras = PersistableBundle()
    extras.putBoolean(CLAVE_CLIP_SENSIBLE, true)
    clip.description.extras = extras
    gestor.setPrimaryClip(clip)
    true
} catch (_: SecurityException) {
    false
}

/** Clave estándar de Android para marcar un clip como sensible (API 33). */
const val CLAVE_CLIP_SENSIBLE: String = "android.content.extra.IS_SENSITIVE"
