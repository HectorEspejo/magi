package org.cuatrodetres.magi.ui.modelos

/**
 * Sincronización en modo lector (informe §3, §4.2 y §6.5).
 *
 * El móvil nunca escribe en la carpeta: solo refleja lo que deciden los
 * escritorios. Estos modelos alimentan el indicador ⇅ de Hosts y la pantalla
 * Sincronización de Ajustes.
 */

/** Fase del estado de sincronización (diagrama de §3). */
enum class FaseSincronizacionVista {
    SIN_CONFIGURAR,
    SIN_CLAVE,
    ACTIVA,
    SINCRONIZANDO,
    ESPERANDO_CLAVE,
    ERROR_CARPETA,
}

/** Estado de otro equipo, igual que en la Fase 10. */
enum class EstadoEquipoVista {
    ESTE_EQUIPO,
    AL_DIA,
    ILEGIBLE,
    DANADO,
    FORMATO_NUEVO,
    QUITADO,
}

/** Un equipo de la carpeta compartida tal como lo enseña Ajustes. */
data class EquipoVista(
    val nombre: String,
    val plataforma: String,
    val version: String,
    val estado: EstadoEquipoVista,
    val ultimaVez: Long? = null,
    val ultimoFichero: String? = null,
)

/** Conflicto de huella de un host que llega de otro equipo y se resuelve en el móvil. */
data class ConflictoHuellaVista(
    val id: Long,
    val hostUuid: String,
    val hostNombre: String,
    val direccion: String,
    /** Huella que tiene el móvil (cadena vacía si no tiene). */
    val huellaMia: String,
    /** Huella que propone el otro equipo. */
    val huellaOtra: String,
    val equipoOrigen: String? = null,
)

/** Estado completo de la sincronización del móvil. */
data class EstadoSincronizacionVista(
    val fase: FaseSincronizacionVista = FaseSincronizacionVista.SIN_CONFIGURAR,
    /** URI SAF del árbol elegido, para enseñarlo en Ajustes. */
    val carpetaUri: String? = null,
    /** Nombre legible de la carpeta (si se conoce). */
    val carpetaNombre: String? = null,
    /** Huella corta de la clave de sincronización (p. ej. 7f3a·91c2). */
    val huellaCortaClave: String? = null,
    /** Instante de la última pasada (epoch en segundos). */
    val ultimaPasada: Long? = null,
    val equipos: List<EquipoVista> = emptyList(),
    /** Número de conflictos de huella pendientes de resolver. */
    val conflictosHuella: Int = 0,
    /** Error legible de la última pasada, si lo hubo. */
    val error: String? = null,
) {
    /** Si ya hay una carpeta elegida (aunque falte la clave). */
    val tieneCarpeta: Boolean get() = fase != FaseSincronizacionVista.SIN_CONFIGURAR

    /** Si la app escribe en la carpeta: siempre no (D172). */
    val esLectora: Boolean get() = true
}

/** Glifo del indicador ⇅. Va siempre acompañado de su texto (T5). */
fun glifoSincronizacion(fase: FaseSincronizacionVista): String = when (fase) {
    FaseSincronizacionVista.SINCRONIZANDO -> "⇅ …"
    else -> "⇅"
}

/** Si la fase necesita que el usuario haga algo (banner de aviso en Hosts). */
fun faseRequiereAccion(fase: FaseSincronizacionVista): Boolean = when (fase) {
    FaseSincronizacionVista.SIN_CONFIGURAR,
    FaseSincronizacionVista.SIN_CLAVE,
    FaseSincronizacionVista.ESPERANDO_CLAVE,
    FaseSincronizacionVista.ERROR_CARPETA,
    -> true
    FaseSincronizacionVista.ACTIVA,
    FaseSincronizacionVista.SINCRONIZANDO,
    -> false
}

/** Si la fase es un aviso que no rompe el inventario (ESPERANDO_CLAVE y error de carpeta). */
fun faseEsAviso(fase: FaseSincronizacionVista): Boolean = when (fase) {
    FaseSincronizacionVista.ESPERANDO_CLAVE,
    FaseSincronizacionVista.ERROR_CARPETA,
    -> true
    else -> false
}
