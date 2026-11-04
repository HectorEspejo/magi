package org.cuatrodetres.magi.ui.modelos

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * Utilidades puras de presentación: tiempo relativo, fechas y versiones.
 * No dependen de recursos de Android para poder probarse en la JVM.
 */

/** Unidad del tiempo relativo que se enseña en sincronización y sesiones. */
enum class UnidadTemporal { SEGUNDOS, MINUTOS, HORAS, DIAS }

/** «hace N unidad» sin el texto: el recurso de cadena pone la unidad. */
data class TiempoRelativo(val cantidad: Long, val unidad: UnidadTemporal)

/**
 * Tiempo transcurrido desde [instanteSegundos] hasta [ahoraSegundos].
 * Devuelve null si no hay instante.
 */
fun tiempoRelativo(instanteSegundos: Long?, ahoraSegundos: Long): TiempoRelativo? {
    if (instanteSegundos == null) return null
    val diferencia = (ahoraSegundos - instanteSegundos).coerceAtLeast(0)
    return when {
        diferencia < 60 -> TiempoRelativo(diferencia, UnidadTemporal.SEGUNDOS)
        diferencia < 60 * 60 -> TiempoRelativo(diferencia / 60, UnidadTemporal.MINUTOS)
        diferencia < 24 * 60 * 60 -> TiempoRelativo(diferencia / (60 * 60), UnidadTemporal.HORAS)
        else -> TiempoRelativo(diferencia / (24 * 60 * 60), UnidadTemporal.DIAS)
    }
}

private val FORMATO_FECHA: DateTimeFormatter = DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm")

/** Fecha y hora local de un instante epoch, para la ficha de la clave del móvil. */
fun formatoFecha(instanteSegundos: Long?, zona: ZoneId = ZoneId.systemDefault()): String? {
    if (instanteSegundos == null) return null
    return FORMATO_FECHA.format(Instant.ofEpochSecond(instanteSegundos).atZone(zona))
}

/** Instante actual en segundos, encapsulado para que las pruebas lo puedan fijar. */
fun ahoraSegundos(): Long = System.currentTimeMillis() / 1000L

/** Texto de una versión con su versionCode: «0.1.0 (1009)». */
fun textoVersion(app: String, codigo: Int): String = app + " (" + codigo + ")"

/** Formato corto de la huella de la clave de sincronización (7f3a·91c2). */
fun huellaCorta(huella: String?): String? {
    if (huella.isNullOrBlank()) return null
    val limpia = huella.removePrefix("SHA256:").replace("=", "")
    if (limpia.length <= 8) return limpia
    return limpia.substring(0, 4) + "·" + limpia.substring(limpia.length - 4)
}
