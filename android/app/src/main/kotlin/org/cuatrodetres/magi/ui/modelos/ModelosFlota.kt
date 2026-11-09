package org.cuatrodetres.magi.ui.modelos

/**
 * Flota (informe §5.1, §6.1 y §6.2): el estado de cada host sale del último
 * sondeo del móvil (FRÍA, SONDEANDO, CAÍDA, ALCANZABLE, CARGA o NOMINAL) con
 * sus culpables, el resumen de métricas, los fallos, las sesiones abiertas y
 * la hora del último sondeo.
 *
 * El móvil es quien sondea: estos modelos son de presentación y no calculan
 * umbrales ni leen la salida del host. Eso lo hace el núcleo con la misma
 * función que el escritorio (F2 §7.2); aquí solo se agrupa y se formatea.
 */

/**
 * Estado de un host en Flota (diagrama de §3.3 y mockup de §6.1).
 *
 * Los cinco primeros salen del último sondeo; [SONDEANDO] y [SIN_SONDEO] son
 * propios del móvil. Un host que no es SSH nunca se sondea y queda
 * [SIN_SONDEO] con su último estado.
 */
enum class EstadoFlotaVista(
    /** Glifo que acompaña siempre a la palabra (T5). */
    val glifo: String,
    /** Palabra del estado, tal y como se pinta en la fila. */
    val palabra: String,
) {
    /** Sin sondeo guardado en el móvil: ○. */
    FRIA("○", "FRÍA"),

    /** Sondeo en curso: ◐. */
    SONDEANDO("◐", "sondeando"),

    /** El último sondeo falló: ✕. */
    CAIDA("✕", "CAÍDA"),

    /** Responde pero sin métricas: ●. */
    ALCANZABLE("●", "ALCANZ."),

    /** Algún umbral superado o un vigilado caído: ◐. */
    CARGA("◐", "CARGA"),

    /** Todo dentro de los umbrales: ●. */
    NOMINAL("●", "NOMINAL"),

    /** Host que no se sondea (no es SSH): ○ y «se abre en el ordenador». */
    SIN_SONDEO("○", "sin sondeo"),
    ;

    /** Estado de un sondeo que no llegó a responder. */
    val esCaida: Boolean get() = this == CAIDA

    /** Hay un sondeo en curso para este host. */
    val sondeando: Boolean get() = this == SONDEANDO

    /** Host que no se sondea en el móvil (RDP, VNC o voxl). */
    val esSinSondeo: Boolean get() = this == SIN_SONDEO

    /** El host respondió y está dentro de los umbrales. */
    val esNominal: Boolean get() = this == NOMINAL || this == ALCANZABLE

    /** El estado vive en un sondeo guardado: FRÍA y SIN_SONDEO no lo están. */
    val tieneSondeo: Boolean get() = this != FRIA && this != SIN_SONDEO
}

/**
 * Métricas de un sondeo (bloque FLOTA de la ficha y segunda línea de Flota).
 *
 * Los porcentajes y las tasas llegan ya calculados por el núcleo; los campos
 * que no trae el sondeo (un host sin /proc/loadavg, un contenedor sin
 * métricas…) quedan a null y no se pintan.
 */
data class MetricasFlotaVista(
    val nucleos: Int? = null,
    val carga1m: Double? = null,
    val carga5m: Double? = null,
    val carga15m: Double? = null,
    val memoriaTotalKb: Long? = null,
    val memoriaDisponibleKb: Long? = null,
    /** Memoria en uso, en porcentaje (0-100). */
    val memoriaPct: Double? = null,
    val discoTotalKb: Long? = null,
    val discoUsadoKb: Long? = null,
    /** Disco en uso, en porcentaje (0-100). */
    val discoPct: Double? = null,
    val redRxBytes: Long? = null,
    val redTxBytes: Long? = null,
    /** Tasa de bajada contra el sondeo anterior (bytes por segundo). */
    val redRxBps: Double? = null,
    /** Tasa de subida contra el sondeo anterior (bytes por segundo). */
    val redTxBps: Double? = null,
    val uptimeSeg: Long? = null,
) {
    /** Si hay algún dato numérico que enseñar. */
    val hayMetricas: Boolean
        get() = carga1m != null || memoriaPct != null || discoPct != null ||
            uptimeSeg != null || redRxBps != null || redTxBps != null

    /** Carga del último minuto relativa a los núcleos (null si faltan datos). */
    val cargaPorNucleo: Double?
        get() {
            val carga = carga1m ?: return null
            val nucleos = (nucleos ?: 1).coerceAtLeast(1)
            return carga / nucleos
        }
}

/** Un servicio vigilado del host con su estado (bloque VIGILADOS de §6.2). */
data class VigiladoVista(
    val nombre: String = "",
    /** Estado tal cual lo dio el host («active», «failed», «inactive»…). */
    val estado: String = "",
    /** El núcleo ya dice si cuenta como activo; el glifo lo pone la vista. */
    val activo: Boolean = false,
) {
    /** ● activo, ✕ fallido y ○ el resto. */
    val glifo: String
        get() = when {
            activo -> "●"
            estado == "failed" -> "✕"
            estado == "activating" || estado == "deactivating" -> "◐"
            else -> "○"
        }
}

/**
 * Fallos del último sondeo de un host (sondeo ampliado de la Fase 13).
 *
 * En la fila de Flota basta el recuento ([total], la marca ⚠N); en la ficha
 * llega la lista con los nombres. Los fallos no cambian el estado del host:
 * solo los vigilados lo hacen (D240).
 */
data class FallosFlotaVista(
    val total: Int = 0,
    val fallos: List<FalloFlotaVista> = emptyList(),
) {
    val vacio: Boolean get() = total == 0

    /** Unidades fallidas, en el orden en que las dio el sondeo. */
    val unidades: List<String>
        get() = fallos.filter { it.tipo == TipoFalloVista.SERVICIO }.map { it.nombre }

    /** Contenedores caídos, en el orden en que los dio el sondeo. */
    val contenedores: List<String>
        get() = fallos.filter { it.tipo == TipoFalloVista.CONTENEDOR }.map { it.nombre }
}

/**
 * Fila de la lista de Flota (informe §6.1).
 *
 * La segunda línea la compone la vista con [metricas], [culpables], [motivo] o
 * la dirección según el estado; los helpers [textoMetricas], [textoCulpables]
 * y [textoSegundaLineaFlota] ya hacen esa composición para las pruebas y las
 * previsualizaciones.
 */
data class HostFlotaVista(
    val uuid: String,
    val nombre: String,
    val direccion: String,
    val puerto: Int,
    val tipo: TipoHostVista = TipoHostVista.SSH,
    val grupoUuid: String? = null,
    val etiquetas: List<String> = emptyList(),
    val estado: EstadoFlotaVista = EstadoFlotaVista.FRIA,
    /** Culpables del estado CARGA («carga», «memoria», «disco» o unidades). */
    val culpables: List<String> = emptyList(),
    /** Resumen de métricas para la segunda línea; null si no hay sondeo. */
    val metricas: MetricasFlotaVista? = null,
    /** Fallos del último sondeo (marca ⚠N); en la ficha llegan con nombres. */
    val fallos: FallosFlotaVista = FallosFlotaVista(),
    /** Sesiones abiertas del host ahora mismo. */
    val sesiones: Int = 0,
    /** Instante del último sondeo (epoch en segundos); null si no hay. */
    val ultimoSondeoEn: Long? = null,
    /** Motivo legible de la caída o de no tener sondeo. */
    val motivo: String? = null,
    /** Duración del último sondeo en milisegundos. */
    val duracionMs: Long? = null,
) {
    /** Número de fallos para la marca ⚠N. */
    val numFallos: Int get() = fallos.total

    /** ¿Se puede sondear desde el móvil? Solo los hosts SSH. */
    val esSondeable: Boolean get() = tipo == TipoHostVista.SSH

    /** Hay sesiones abiertas del host ahora mismo. */
    val conSesiones: Boolean get() = sesiones > 0
}

/**
 * Detalle de Flota de la ficha de un host (informe §6.2): métricas completas,
 * vigilados con su estado, fallos con nombres y la hora y duración del último
 * sondeo. Null en la ficha si el host no es SSH o no tiene sondeo del móvil.
 */
data class DetalleFlotaVista(
    val estado: EstadoFlotaVista = EstadoFlotaVista.FRIA,
    val culpables: List<String> = emptyList(),
    val motivo: String? = null,
    /** Métricas completas; null si el sondeo no trajo métricas. */
    val metricas: MetricasFlotaVista? = null,
    val vigilados: List<VigiladoVista> = emptyList(),
    val fallos: List<FalloFlotaVista> = emptyList(),
    /** Instante del último sondeo (epoch en segundos). */
    val ultimoSondeoEn: Long? = null,
    val duracionMs: Long? = null,
    /** Aviso de datos del sondeo ignorados (unidades vigiladas que no casan…). */
    val aviso: String? = null,
) {
    val numFallos: Int get() = fallos.size

    /** Fallos de un tipo concreto (unidades o contenedores). */
    fun fallosDe(tipo: TipoFalloVista): List<FalloFlotaVista> =
        fallos.filter { it.tipo == tipo }
}

/**
 * Verificaciones previas de un host (informe §6.2 y §7.11).
 *
 * Son de solo lectura en el móvil: se editan en el escritorio y llegan por la
 * sincronización. El núcleo las usa al deliberar y al actuar; la ficha solo
 * las enseña.
 */
data class VerificacionesVista(
    /** MELCHIOR-1: comprobar la salud del host. */
    val salud: Boolean = false,
    /** BALTHASAR-2: comprobar la antigüedad del último backup. */
    val backup: Boolean = false,
    val backupRuta: String? = null,
    val backupPatron: String? = null,
    /** CASPER-3: comprobar los tests (solo en el escritorio). */
    val tests: Boolean = false,
    val testsComando: String? = null,
) {
    val algunaActiva: Boolean get() = salud || backup || tests

    /** Líneas del bloque VERIFICACIONES de la ficha, sin inventar texto libre. */
    val lineas: List<String>
        get() = buildList {
            if (salud) add("salud")
            if (backup) {
                val ruta = backupRuta.orEmpty()
                val patron = backupPatron.orEmpty()
                add(if (patron.isEmpty()) "backup " + ruta else "backup " + ruta + " " + patron)
            }
            if (tests) add("tests")
        }
}

/**
 * Snippet que se escribe al conectar con un host (informe §6.2 y §7.9).
 *
 * El móvil vuelve a comprobarlo antes de escribirlo: si es crítico o tiene
 * variables, no se escribe y se avisa en la sesión. [valido] ya lo dice el
 * núcleo.
 */
data class SnippetAlConectarVista(
    /** uuid de sincronización del snippet. */
    val uuid: String = "",
    val nombre: String = "",
    /** True si el móvil lo puede escribir (no es crítico y no tiene variables). */
    val valido: Boolean = false,
)

/**
 * Bloque de la lista de Flota: un grupo y sus hosts, o «sin grupo» al final.
 *
 * [grupoUuid] es null para el bloque de hosts sin grupo. [plegado] decide si
 * la lista pinta los hosts; el estado del plegado es propio del móvil.
 */
data class BloqueFlotaVista(
    val grupoUuid: String? = null,
    val grupoNombre: String? = null,
    val plegado: Boolean = false,
    val hosts: List<HostFlotaVista> = emptyList(),
) {
    /** Clave estable para LazyColumn. */
    val clave: String get() = grupoUuid ?: SIN_GRUPO

    val numHosts: Int get() = hosts.size

    val numFallos: Int get() = hosts.sumOf { it.numFallos }

    companion object {
        const val SIN_GRUPO: String = "sin-grupo"
    }
}

/**
 * Agrupa los hosts de Flota como en el mockup de §6.1: grupos por nombre (sin
 * distinguir mayúsculas), hosts por nombre dentro del grupo y «sin grupo» al
 * final. Los grupos sin hosts no se pintan. Si no se pasan grupos, todos los
 * hosts van al bloque «sin grupo».
 */
fun agruparFlota(
    hosts: List<HostFlotaVista>,
    grupos: List<GrupoVista> = emptyList(),
): List<BloqueFlotaVista> {
    val porGrupo = hosts.groupBy { it.grupoUuid }
    val bloques = grupos
        .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
        .mapNotNull { grupo ->
            val delGrupo = porGrupo[grupo.uuid].orEmpty()
                .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
            if (delGrupo.isEmpty()) {
                null
            } else {
                BloqueFlotaVista(
                    grupoUuid = grupo.uuid,
                    grupoNombre = grupo.nombre,
                    plegado = grupo.plegado,
                    hosts = delGrupo,
                )
            }
        }
    val conocidos = grupos.map { it.uuid }.toSet()
    val sinGrupo = hosts
        .filter { it.grupoUuid == null || it.grupoUuid !in conocidos }
        .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
    return if (sinGrupo.isEmpty()) bloques else bloques + BloqueFlotaVista(hosts = sinGrupo)
}

/** Glifo del estado de Flota; la palabra va aparte para TalkBack (T5). */
fun glifoEstadoFlota(estado: EstadoFlotaVista): String = estado.glifo

/** Palabra del estado de Flota. */
fun palabraEstadoFlota(estado: EstadoFlotaVista): String = estado.palabra

/** Dirección de la fila para un host sin sondeo: «192.0.2.10:22». */
fun textoDireccionFlota(host: HostFlotaVista): String = host.direccion + ":" + host.puerto

/**
 * Segunda línea de la fila para los estados NOMINAL, ALCANZABLE, CARGA y
 * SONDEANDO: sesiones, carga, memoria y disco, con la coma decimal.
 */
fun textoMetricas(host: HostFlotaVista): String {
    val partes = mutableListOf<String>()
    if (host.sesiones > 0) {
        partes += if (host.sesiones == 1) "1 sesión" else host.sesiones.toString() + " sesiones"
    }
    val metricas = host.metricas
    if (metricas != null) {
        metricas.carga1m?.let { partes += "carga " + unDecimal(it) }
        metricas.memoriaPct?.let { partes += "mem " + porcentaje(it) }
        metricas.discoPct?.let { partes += "disco " + porcentaje(it) }
    }
    if (partes.isEmpty()) {
        if (host.estado == EstadoFlotaVista.ALCANZABLE) return "sin métricas"
        return textoDireccionFlota(host)
    }
    return partes.joinToString(" · ")
}

/**
 * Culpables del estado CARGA con su dato: «memoria 94 % · exportar.service»,
 * «carga 4,2 · disco 96 %». Las claves conocidas («carga», «memoria» y
 * «disco») se acompañan de su métrica; el resto son unidades y van tal cual.
 */
fun textoCulpables(host: HostFlotaVista): String {
    if (host.culpables.isEmpty()) return ""
    return host.culpables.joinToString(" · ") { culpable ->
        when (culpable.lowercase()) {
            "carga" -> host.metricas?.carga1m?.let { "carga " + unDecimal(it) } ?: "carga"
            "memoria" -> host.metricas?.memoriaPct?.let { "memoria " + porcentaje(it) } ?: "memoria"
            "disco" -> host.metricas?.discoPct?.let { "disco " + porcentaje(it) } ?: "disco"
            else -> culpable
        }
    }
}

/**
 * Segunda línea de la fila según el estado, como en el mockup de §6.1:
 * NOMINAL las métricas, CARGA los culpables, CAÍDA el motivo, FRÍA o sin
 * sondeo la dirección y SONDEANDO «sondeando».
 */
fun textoSegundaLineaFlota(host: HostFlotaVista): String = when (host.estado) {
    EstadoFlotaVista.SONDEANDO -> "sondeando"
    EstadoFlotaVista.CARGA -> textoCulpables(host).ifEmpty { textoMetricas(host) }
    EstadoFlotaVista.CAIDA -> host.motivo ?: "no responde"
    EstadoFlotaVista.FRIA, EstadoFlotaVista.SIN_SONDEO -> textoDireccionFlota(host)
    EstadoFlotaVista.ALCANZABLE -> textoMetricas(host)
    EstadoFlotaVista.NOMINAL -> textoMetricas(host)
}

/** Formatea una tasa de red legible: «2,4 MB/s», «840 kB/s» o «0 B/s». */
fun textoRed(metricas: MetricasFlotaVista): String {
    val bajada = metricas.redRxBps
    val subida = metricas.redTxBps
    if (bajada == null && subida == null) return "—"
    return "↓ " + (bajada?.let { tasaRed(it) } ?: "—") +
        " · ↑ " + (subida?.let { tasaRed(it) } ?: "—")
}

/** Uptime legible: «14 d 02 h» o «3 h 05 m» (formato de la F2). */
fun textoUptime(segundos: Long?): String {
    if (segundos == null || segundos < 0) return "—"
    val dias = segundos / 86_400
    val horas = (segundos % 86_400) / 3_600
    val minutos = (segundos % 3_600) / 60
    return if (dias > 0) {
        dias.toString() + " d " + derecha(horas, 2) + " h"
    } else {
        horas.toString() + " h " + derecha(minutos, 2) + " m"
    }
}

/** Un decimal con coma (0,4), sin decimales de relleno. */
internal fun unDecimal(valor: Double): String {
    if (!valor.isFinite()) return "—"
    val redondeado = Math.round(valor * 10.0) / 10.0
    val texto = if (redondeado == Math.floor(redondeado)) {
        Math.round(redondeado).toString()
    } else {
        redondeado.toString()
    }
    return texto.replace('.', ',')
}

/** Porcentaje entero con su signo: «61 %». */
internal fun porcentaje(valor: Double): String {
    if (!valor.isFinite()) return "—"
    return Math.round(valor).toString() + " %"
}

/** Tasa de bytes por segundo legible (1014 B/s, 2,5 MB/s…). */
private fun tasaRed(bytesPorSegundo: Double): String {
    if (!bytesPorSegundo.isFinite() || bytesPorSegundo < 0) return "—"
    val kb = 1024.0
    val mb = kb * 1024.0
    return when {
        bytesPorSegundo >= mb -> unDecimal(bytesPorSegundo / mb) + " MB/s"
        bytesPorSegundo >= kb -> Math.round(bytesPorSegundo / kb).toString() + " kB/s"
        else -> Math.round(bytesPorSegundo).toString() + " B/s"
    }
}

/** Rellena un número con ceros a la izquierda. */
private fun derecha(valor: Long, ancho: Int): String = valor.toString().padStart(ancho, '0')
