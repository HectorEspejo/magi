package org.cuatrodetres.magi.ui.modelos

/**
 * Túneles del inventario con su estado en vivo (informe §6 y §7.5).
 *
 * El móvil fuerza loopback en local y dinámico (D181) y ofrece «abrir en un
 * puerto libre» cuando el puerto está ocupado o reservado.
 */

/** Tipo de túnel (mecánica del escritorio, F5 §7). */
enum class TipoTunelVista {
    LOCAL,
    REMOTO,
    DINAMICO,
    ;

    /** Letra que enseña la fila: L, R o D. */
    val letra: String
        get() = when (this) {
            LOCAL -> "L"
            REMOTO -> "R"
            DINAMICO -> "D"
        }
}

/** Estado vivo de un túnel (diagrama de §3). */
enum class EstadoTunelVista {
    INACTIVO,
    ACTIVANDO,
    ACTIVO,
    CAIDO,
}

/** Túnel del inventario con su estado y contadores. */
data class TunelVista(
    val uuid: String,
    val hostUuid: String,
    val hostNombre: String,
    val nombre: String,
    val tipo: TipoTunelVista,
    /** Dirección de escucha del inventario, tal cual (p. ej. 0.0.0.0:8080). */
    val escucha: String,
    /** La dirección del inventario no era 127.0.0.1/localhost: se avisa en la fila. */
    val escuchaAviso: Boolean = false,
    val destino: String = "",
    val automatico: Boolean = false,
    val estado: EstadoTunelVista = EstadoTunelVista.INACTIVO,
    val error: String? = null,
    /** Conexiones abiertas ahora mismo. */
    val conexiones: Int = 0,
    /** Conexiones aceptadas desde que se abrió (acumulado). */
    val aceptadas: Int = 0,
    /** Último error de una conexión suelta, aunque el túnel siga activo. */
    val ultimoError: String? = null,
    /** Instante (epoch en segundos) del último error. */
    val ultimoErrorEn: Long? = null,
    val bytesSubida: Long = 0,
    val bytesBajada: Long = 0,
    /** Puerto asignado cuando se abrió en un puerto libre (no se guarda). */
    val puertoEfectivo: Int? = null,
)

/** Bloque de la pantalla Túneles: un host y sus túneles. */
data class BloqueTunelesVista(
    val hostUuid: String,
    val hostNombre: String,
    val tuneles: List<TunelVista>,
)

/** Agrupa los túneles por host, con los hosts ordenados por nombre. */
fun agruparTuneles(tuneles: List<TunelVista>): List<BloqueTunelesVista> =
    tuneles.groupBy { it.hostUuid }
        .map { (hostUuid, lista) ->
            BloqueTunelesVista(
                hostUuid = hostUuid,
                hostNombre = lista.first().hostNombre,
                tuneles = lista.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre }),
            )
        }
        .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.hostNombre })

/** Glifo del estado del túnel; el texto va aparte (T5). */
fun glifoTunel(estado: EstadoTunelVista): String = when (estado) {
    EstadoTunelVista.INACTIVO -> "○"
    EstadoTunelVista.ACTIVANDO -> "◐"
    EstadoTunelVista.ACTIVO -> "●"
    EstadoTunelVista.CAIDO -> "✕"
}

/**
 * Separa la escucha de su puerto: `0.0.0.0:8080` → (`0.0.0.0`, 8080).
 * Devuelve null si no hay un puerto válido al final.
 */
fun separarPuerto(escucha: String): Pair<String, Int>? {
    val corte = escucha.lastIndexOf(':')
    if (corte <= 0 || corte == escucha.lastIndex) return null
    val puerto = escucha.substring(corte + 1).toIntOrNull() ?: return null
    if (puerto !in 0..65535) return null
    return escucha.substring(0, corte) to puerto
}

/**
 * Escucha que se enseña en la fila. Local y dinámico escuchan siempre en
 * 127.0.0.1 (D181); si ya se abrió en un puerto libre, se muestra ese puerto.
 */
fun escuchaMostrada(tunel: TunelVista): String {
    if (tunel.puertoEfectivo != null) return "127.0.0.1:" + tunel.puertoEfectivo
    if (tunel.tipo == TipoTunelVista.REMOTO) return tunel.escucha
    val partes = separarPuerto(tunel.escucha) ?: return tunel.escucha
    return "127.0.0.1:" + partes.second
}

/** «L 127.0.0.1:3000 → 127.0.0.1:3000» como en el mockup de §6.2. */
fun descripcionTunel(tunel: TunelVista): String =
    tunel.tipo.letra + " " + escuchaMostrada(tunel) + " → " + tunel.destino

/** URL para «Abrir en el navegador»: solo un túnel local activo (informe §4.5). */
fun urlNavegador(tunel: TunelVista): String? {
    if (tunel.tipo != TipoTunelVista.LOCAL) return null
    if (tunel.estado != EstadoTunelVista.ACTIVO) return null
    val puerto = tunel.puertoEfectivo ?: separarPuerto(tunel.escucha)?.second ?: return null
    return "http://127.0.0.1:" + puerto
}

/** Contadores de la fila: «2 conex. · ↑14 KB ↓1,2 MB». */
fun contadoresTunel(tunel: TunelVista): String =
    tunel.conexiones.toString() + " abiertas · " + tunel.aceptadas.toString() +
        " aceptadas · ↑" + formatoBytes(tunel.bytesSubida) +
        " ↓" + formatoBytes(tunel.bytesBajada)

/** Formatea bytes en B, KB y MB con coma decimal y sin decimales de relleno. */
fun formatoBytes(bytes: Long): String {
    val valor = bytes.coerceAtLeast(0)
    return when {
        valor < 1024 -> valor.toString() + " B"
        valor < 1024L * 1024L -> formatearUnidad(valor / 1024.0) + " KB"
        valor < 1024L * 1024L * 1024L -> formatearUnidad(valor / (1024.0 * 1024.0)) + " MB"
        else -> formatearUnidad(valor / (1024.0 * 1024.0 * 1024.0)) + " GB"
    }
}

private fun formatearUnidad(valor: Double): String {
    if (valor >= 10.0) return Math.round(valor).toString()
    val entero = Math.round(valor * 10.0) / 10.0
    return if (entero == Math.floor(entero)) entero.toInt().toString()
    else entero.toString().replace('.', ',')
}
