package org.cuatrodetres.magi.ui.modelos

/**
 * Registro del móvil (Fase 14, informe §5.1, §6.10 y §7.10): las entradas del
 * propio móvil, de la más nueva a la más vieja y por páginas de 50.
 *
 * El registro se pide por páginas con un cursor por id de entrada; no hay un
 * flujo de registro. Una deliberación antigua enseña sus comprobaciones
 * leídas de DELIBERACIONES y, si no consta su ejecución, lo dice.
 */

/** Entradas por página del registro (informe §7.10). */
const val TAMANO_PAGINA_REGISTRO: Int = 50

/** Resultado de una entrada del registro. */
enum class ResultadoRegistroVista(
    val glifo: String,
    val texto: String,
) {
    OK("✓", "ok"),
    ERROR("✕", "error"),
}

/**
 * Filtros rápidos por tipo del registro (informe §7.10). Una lista de tipos
 * vacía en [FiltroRegistroVista] equivale a «todos».
 */
enum class TipoRegistroVista(
    /** Etiqueta de la ficha de filtro. */
    val etiqueta: String,
    /** Tipos crudos de REGISTRO que entran en este filtro. */
    val tipos: List<String>,
) {
    CONEXIONES(
        "conexiones",
        listOf("conexion_abierta", "conexion_fallida", "sesion_cerrada", "sesion_reconectada"),
    ),
    HUELLAS("huellas", listOf("huella_aceptada", "huella_sustituida", "huella_sincronizada")),
    TUNELES("túneles", listOf("tunel_abierto", "tunel_cerrado", "tunel_fallido")),
    SONDEOS("sondeos", listOf("sondeo_fallido")),
    SNIPPETS("snippets", listOf("snippet_ejecutado")),
    DELIBERACIONES(
        "deliberaciones",
        listOf("deliberacion_aprobada", "deliberacion_forzada", "deliberacion_cancelada"),
    ),
    SERVICIOS("servicios", listOf("servicio_accion")),
    CLAVES("claves", listOf("clave_generada", "clave_instalada")),
    SINCRONIZACION("sincronización", listOf("inventario_importado")),
    ;

    companion object {
        /** Filtro al que pertenece un tipo crudo de REGISTRO; null si no es de ninguno. */
        fun desdeTipoCrudo(tipo: String): TipoRegistroVista? =
            entries.firstOrNull { tipo in it.tipos }
    }
}

/**
 * Filtro de la pantalla Registro: host, tipos y texto (informe §6.10).
 * [tipos] vacío = todos.
 */
data class FiltroRegistroVista(
    /** uuid del host por el que filtrar; null = todos. */
    val hostUuid: String? = null,
    /** Tipos por los que filtrar; vacío = todos. */
    val tipos: List<TipoRegistroVista> = emptyList(),
    /** Texto a buscar en el detalle (vacío = sin filtro). */
    val texto: String = "",
) {
    /** No filtra nada: es el filtro «todos». */
    val esTodos: Boolean
        get() = hostUuid == null && tipos.isEmpty() && texto.isBlank()

    /** Tipos crudos del filtro, listos para el núcleo. */
    val tiposCrudos: List<String>
        get() = tipos.flatMap { it.tipos }

    /** Alterna un tipo en el filtro (las fichas deslizables de §6.10). */
    fun conTipo(tipo: TipoRegistroVista): FiltroRegistroVista =
        if (tipo in tipos) copy(tipos = tipos - tipo) else copy(tipos = tipos + tipo)
}

/** Una entrada del historial de MAGI (informe §6.10). */
data class EntradaRegistroVista(
    val id: Long = 0,
    /** Instante de la entrada (epoch en segundos). */
    val fecha: Long = 0,
    /** Tipo de evento (conexion_abierta, servicio_accion…). */
    val tipo: String = "",
    val hostUuid: String? = null,
    val hostNombre: String? = null,
    val resultado: ResultadoRegistroVista = ResultadoRegistroVista.OK,
    val detalle: String = "",
) {
    /** Categoría del filtro a la que pertenece. */
    val tipoFiltro: TipoRegistroVista? get() = TipoRegistroVista.desdeTipoCrudo(tipo)

    /** ¿Es una anotación de deliberación? */
    val esDeliberacion: Boolean get() = tipo.startsWith("deliberacion_")

    /** Línea de la lista: «web-01 · reiniciar app.service». */
    val textoFila: String
        get() = if (hostNombre.isNullOrEmpty()) detalle else hostNombre + " · " + detalle
}

/**
 * Una deliberación antigua con sus comprobaciones, para el detalle de una
 * entrada deliberacion_* (informe §7.10).
 */
data class DetalleDeliberacionRegistroVista(
    val id: Long = 0,
    /** Instante de la deliberación (epoch en segundos). */
    val fecha: Long = 0,
    /** «reiniciar app.service → web-01». */
    val accion: String = "",
    val resultado: ResultadoDeliberacionRegistroVista = ResultadoDeliberacionRegistroVista.APROBADA,
    /** Hubo al menos un rechazo. */
    val bloqueada: Boolean = false,
    /** Motivo del forzado, si se forzó. */
    val motivo: String? = null,
    /** Usuario de auditoría («móvil (<equipo>)»). */
    val usuario: String = "",
    /** Cómo acabó la ejecución; null si no consta. */
    val ejecucionResultado: ResultadoEjecucionVista? = null,
    /** Se resolvió pero no consta su ejecución (MAGI se cerró durante ella). */
    val sinResultado: Boolean = false,
    val filas: List<FilaHostDeliberacionVista> = emptyList(),
    val aprobadas: Int = 0,
    val activas: Int = 0,
) {
    /** «2/3», consenso de las comprobaciones activas. */
    val textoConsenso: String get() = aprobadas.toString() + "/" + activas

    /** Texto del resultado: «sin resultado: MAGI se cerró durante la ejecución». */
    val textoResultado: String
        get() = when {
            ejecucionResultado != null -> ejecucionResultado.texto
            sinResultado -> "sin resultado: MAGI se cerró durante la ejecución"
            else -> "sin resultado"
        }
}

/** Cómo se resolvió una deliberación antigua. */
enum class ResultadoDeliberacionRegistroVista(
    val texto: String,
) {
    APROBADA("aprobada"),
    FORZADA("forzada"),
    CANCELADA("cancelada"),
}

/** Detalle completo de una entrada del registro, con su deliberación si la hay. */
data class DetalleRegistroVista(
    val entrada: EntradaRegistroVista = EntradaRegistroVista(),
    val deliberacion: DetalleDeliberacionRegistroVista? = null,
)
