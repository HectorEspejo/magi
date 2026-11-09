package org.cuatrodetres.magi.ui.modelos

/**
 * Snippets (Fase 14, informe §5.1, §6.8 y §7.6): el móvil solo los lee. Los
 * destinos y las variables los resuelve el núcleo, que también dice si la
 * ejecución delibera y devuelve los errores por campo.
 */

/** Timeout mínimo de un snippet, en segundos (el del núcleo). */
const val TIMEOUT_SNIPPET_MIN: Int = 5

/** Timeout máximo de un snippet, en segundos (el del núcleo). */
const val TIMEOUT_SNIPPET_MAX: Int = 3_600

/** Timeout por defecto de un snippet, en segundos. */
const val TIMEOUT_SNIPPET_POR_DEFECTO: Int = 60

/** Un host al que apunta un snippet (uuid de la sincronización y nombre). */
data class HostDestinoVista(
    val uuid: String = "",
    val nombre: String = "",
)

/** Un destino del snippet: una etiqueta (por nombre) o un host suelto. */
sealed interface DestinoSnippetVista {
    /** Etiqueta de host; una etiqueta sin hosts resuelve a cero hosts. */
    data class Etiqueta(val etiqueta: String) : DestinoSnippetVista

    /** Host suelto. */
    data class Host(val host: HostDestinoVista) : DestinoSnippetVista

    /** Texto del destino, para la lista y el detalle. */
    val texto: String
        get() = when (this) {
            is Etiqueta -> etiqueta
            is Host -> host.nombre
        }
}

/**
 * Una variable del comando de un snippet, con su valor por defecto y el valor
 * que haya escrito el usuario. Una variable vacía sin valor por defecto no
 * deja continuar.
 */
data class VariableSnippetVista(
    /** Nombre tal como aparece en el comando ([a-z_][a-z0-9_]*). */
    val nombre: String = "",
    /** Valor por defecto del snippet, si lo tiene. */
    val valorDefecto: String? = null,
    /** Valor escrito por el usuario; null = usar el valor por defecto. */
    val valor: String? = null,
) {
    /** Valor que se usará en la sustitución. */
    val valorEfectivo: String? get() = valor ?: valorDefecto

    /** Vacía y sin valor por defecto: no deja continuar. */
    val faltaValor: Boolean get() = valorEfectivo.isNullOrEmpty()
}

/**
 * Un snippet del inventario (solo lectura en el móvil), con sus destinos ya
 * resueltos y sus variables.
 */
data class SnippetVista(
    /** uuid de sincronización: identifica el snippet en toda la interfaz. */
    val uuid: String = "",
    val nombre: String = "",
    val comando: String = "",
    val descripcion: String = "",
    /** Clasificación propia del snippet (no son etiquetas de host). */
    val etiquetas: List<String> = emptyList(),
    /** Crítico: siempre delibera. */
    val critico: Boolean = false,
    val timeoutSeg: Int = TIMEOUT_SNIPPET_POR_DEFECTO,
    val pararAlFallo: Boolean = false,
    val usadoVeces: Long = 0,
    /** Instante del último uso (epoch en segundos); null si no se ha usado. */
    val ultimoUsoEn: Long? = null,
    val destinos: List<DestinoSnippetVista> = emptyList(),
    /** Hosts a los que apunta, ya resueltos y en el orden de la vista Hosts. */
    val hosts: List<HostDestinoVista> = emptyList(),
    val variables: List<VariableSnippetVista> = emptyList(),
    /** Destinos que son escritorios (RDP, VNC o voxl): no ejecutan snippets. */
    val escritoriosOmitidos: Int = 0,
) {
    /** ¿Se puede escribir al conectar? No si es crítico o tiene variables. */
    val aptoAlConectar: Boolean
        get() = !critico && variables.isEmpty()

    /** Resumen de destinos para la lista: «web +2», «todos» o el nombre. */
    val resumenDestinos: String get() = resumenDestinosSnippet(this)

    /** Variables sin valor: con alguna no se puede continuar. */
    val variablesSinValor: List<VariableSnippetVista>
        get() = variables.filter { it.faltaValor }
}

/** Resumen de los destinos de un snippet, como en la lista del escritorio. */
fun resumenDestinosSnippet(snippet: SnippetVista): String {
    val destinos = snippet.destinos
    val primero = destinos.firstOrNull()?.texto
    return when {
        primero == null -> "—"
        destinos.size == 1 -> primero
        else -> primero + " +" + (destinos.size - 1)
    }
}

/** Texto de los hosts resueltos: «web-01, web-02». */
fun textoHostsDestino(hosts: List<HostDestinoVista>): String =
    if (hosts.isEmpty()) "—" else hosts.joinToString(", ") { it.nombre }

/** Un error de validación de la preparación, en su campo (informe §4.5). */
data class ErrorCampoVista(
    /** Campo al que afecta: «snippet», «hosts» o el nombre de una variable. */
    val campo: String = "",
    val mensaje: String = "",
)

/**
 * Ejecución preparada por el núcleo (informe §4.5): el comando con las
 * variables ya sustituidas y escapadas, los hosts elegidos y si delibera.
 */
data class PreparacionEjecucionVista(
    val snippetUuid: String = "",
    val snippetNombre: String = "",
    /** El comando con las variables ya sustituidas (escapadas). */
    val comando: String = "",
    val hosts: List<HostDestinoVista> = emptyList(),
    /** Variables con los valores usados en la sustitución. */
    val variables: List<VariableSnippetVista> = emptyList(),
    /** Errores por campo; si hay alguno, la app no deja lanzar. */
    val errores: List<ErrorCampoVista> = emptyList(),
    /** Si hace falta deliberar (crítico, varios hosts o verificaciones). */
    val delibera: Boolean = false,
    /** Motivos por los que delibera, ya legibles. */
    val motivos: List<String> = emptyList(),
    val critico: Boolean = false,
    val timeoutSeg: Int = TIMEOUT_SNIPPET_POR_DEFECTO,
    /** Valor inicial de «parar al primer fallo» (el del snippet). */
    val pararAlFallo: Boolean = false,
    /** Valores de las variables, para repetir la ejecución. */
    val valores: Map<String, String> = emptyMap(),
) {
    /** Se puede lanzar: sin errores y con al menos un host. */
    val puedeLanzar: Boolean get() = errores.isEmpty() && hosts.isNotEmpty()

    /** Texto de la línea inferior: «timeout 60 s · delibera: 4 hosts». */
    val textoPie: String
        get() {
            val deliberaTexto = if (delibera) {
                "delibera" + (if (motivos.isEmpty()) "" else ": " + motivos.joinToString(", "))
            } else {
                "sin deliberación"
            }
            return "timeout " + timeoutSeg + " s · " + deliberaTexto
        }
}
