package org.cuatrodetres.magi.ui.modelos

/**
 * Ejecuciones de snippets y sus resultados (Fase 14, informe §5.1, §6.9 y
 * §7.6). La salida de un host nunca va al registro ni a logcat: solo se pinta
 * aquí, ya sin secuencias de escape y recortada a 1 MiB por flujo.
 */

/**
 * Estado de un host dentro de una ejecución (diagrama de §3.3) con el glifo y
 * el texto de §6.9.
 */
enum class EstadoHostEjecucionVista(
    /** Glifo de la fila: ● ok, ✕ fallo o error, ◐ conectando o ejecutando. */
    val glifo: String,
    /** Texto de la fila. */
    val texto: String,
) {
    /** En espera de turno del semáforo: ○. */
    EN_COLA("○", "en cola"),

    /** Abriendo la conexión automática: ◐. */
    CONECTANDO("◐", "conectando"),

    /** Comando en marcha: ◐. */
    EJECUTANDO("◐", "ejecutando"),

    /** Terminó con código 0: ●. */
    OK("●", "ok"),

    /** Terminó con código distinto de 0: ✕. */
    FALLO("✕", "fallo"),

    /** No se pudo ejecutar (huella, credenciales, red, plazo): ✕. */
    ERROR("✕", "error"),

    /** Cancelada por el usuario: –. */
    CANCELADO("–", "cancelado"),

    /** No llegó a ejecutarse por «parar al primer fallo» o por cancelar: –. */
    OMITIDO("–", "omitido"),
    ;

    /** Ya no cambia. */
    val esFinal: Boolean
        get() = this == OK || this == FALLO || this == ERROR || this == CANCELADO || this == OMITIDO

    /** Cuenta como éxito para el resultado de la ejecución. */
    val esOk: Boolean get() = this == OK

    /** Está conectando o ejecutando (para el contador «en curso»). */
    val enCurso: Boolean get() = this == CONECTANDO || this == EJECUTANDO
}

/** Resultado final de una ejecución (informe §5.1). */
enum class ResultadoEjecucionVista(
    /** Texto de la fila de Resultados. */
    val texto: String,
) {
    OK("terminada"),
    PARCIAL("parcial"),
    ERROR("error"),
    CANCELADA("cancelada"),
}

/** Un host de una ejecución, con su estado y lo que duró y salió. */
data class HostEjecucionVista(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val estado: EstadoHostEjecucionVista = EstadoHostEjecucionVista.EN_COLA,
    /** Código de salida, si llegó a ejecutarse. */
    val codigo: Int? = null,
    val duracionMs: Long = 0,
    val bytesStdout: Long = 0,
    val bytesStderr: Long = 0,
    /** Motivo del error, si lo hubo (con instrucción cuando toca). */
    val error: String? = null,
) {
    /** «0 · 1,2 s · 212 B», el resumen de la fila del host. */
    val textoResumen: String
        get() {
            val partes = mutableListOf<String>()
            codigo?.let { partes += it.toString() }
            if (duracionMs > 0) partes += duracionEjecucion(duracionMs)
            val bytes = bytesStdout + bytesStderr
            if (bytes > 0) partes += formatoBytesServicios(bytes)
            return partes.joinToString(" · ")
        }
}

/**
 * Una ejecución de un snippet en uno o varios hosts, sin la salida (informe
 * §5.1 y §6.9).
 */
data class EjecucionVista(
    val id: Long = 0,
    val snippetUuid: String? = null,
    val snippetNombre: String = "",
    val hosts: List<HostEjecucionVista> = emptyList(),
    val timeoutSeg: Int = TIMEOUT_SNIPPET_POR_DEFECTO,
    val pararAlFallo: Boolean = false,
    /** Deliberación que la autoriza, si la hubo. */
    val deliberacionId: Long? = null,
    /** Instante de creación (epoch en segundos). */
    val creadaEn: Long = 0,
    /** Instante de fin (epoch en segundos); null mientras siga en curso. */
    val terminadaEn: Long? = null,
    /** Resultado final; null mientras siga en curso. */
    val resultado: ResultadoEjecucionVista? = null,
    val cancelada: Boolean = false,
    /** La deliberación que la autorizó fue forzada (marca ⚑). */
    val forzada: Boolean = false,
) {
    /** Ya terminó (todos los hosts en un estado final). */
    val terminada: Boolean get() = resultado != null || hosts.all { it.estado.esFinal }

    /** Hay algún host conectando o ejecutando. */
    val enCurso: Boolean get() = hosts.any { it.estado.enCurso } || hosts.any { it.estado == EstadoHostEjecucionVista.EN_COLA } && !terminada

    /** Se puede cancelar mientras siga en curso. */
    val sePuedeCancelar: Boolean get() = !terminada && !cancelada

    /** Recuento de hosts en estado ok. */
    val hostsOk: Int get() = hosts.count { it.estado.esOk }

    /** Recuento de hosts con fallo o error. */
    val hostsFallidos: Int get() = hosts.count {
        it.estado == EstadoHostEjecucionVista.FALLO || it.estado == EstadoHostEjecucionVista.ERROR
    }

    /** Recuentos de la fila: hosts, ok y fallidos. */
    val textoRecuentos: String
        get() {
            val partes = mutableListOf(hosts.size.toString() + " hosts")
            if (hostsOk > 0) partes += "✓ " + hostsOk
            if (hostsFallidos > 0) partes += "✕ " + hostsFallidos
            return partes.joinToString(" · ")
        }
}

/** Salida de un host de una ejecución, sin secuencias de escape (informe §6.9). */
data class SalidaEjecucionVista(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val codigo: Int? = null,
    val stdout: String = "",
    val stderr: String = "",
    /** True si stdout pasó de 1 MiB y se recortó. */
    val stdoutTruncada: Boolean = false,
    /** True si stderr pasó de 1 MiB y se recortó. */
    val stderrTruncada: Boolean = false,
    val duracionMs: Long = 0,
) {
    /** No hay nada que enseñar. */
    val vacia: Boolean get() = stdout.isEmpty() && stderr.isEmpty()

    /** Hay salida estándar que pintar. */
    val hayStdout: Boolean get() = stdout.isNotEmpty()

    /** Hay salida de errores que pintar. */
    val hayStderr: Boolean get() = stderr.isNotEmpty()

    /** Texto de «… truncada» si el flujo se recortó. */
    val avisoStdout: String? get() = if (stdoutTruncada) "… truncada" else null

    /** Texto de «… truncada» si el flujo de errores se recortó. */
    val avisoStderr: String? get() = if (stderrTruncada) "… truncada" else null
}

/** Duración legible de una ejecución: «820 ms», «4,1 s», «1 min 05 s». */
fun duracionEjecucion(milisegundos: Long): String {
    val ms = milisegundos.coerceAtLeast(0)
    return when {
        ms < 1_000 -> ms.toString() + " ms"
        ms < 60_000 -> unDecimal(ms / 1_000.0) + " s"
        else -> {
            val minutos = ms / 60_000
            val segundos = (ms % 60_000) / 1_000
            minutos.toString() + " min " + segundos.toString().padStart(2, '0') + " s"
        }
    }
}
