package org.cuatrodetres.magi.ui.modelos

import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.navegacion.DetalleVista
import org.cuatrodetres.magi.navegacion.SeccionMas

/**
 * Estado de la app que consume Compose (una sola instantánea observable).
 *
 * Todo lo que se pinta sale de aquí: la interfaz no guarda estado propio de
 * negocio. Vive en modelos para que las pantallas y los ViewModels compartan el
 * mismo tipo sin depender unos de otros.
 */
data class EstadoMagi(
    val bloqueo: EstadoBloqueoVista = EstadoBloqueoVista.DESBLOQUEADA,
    val destino: Destino = Destino.FLOTA,
    /** Sección elegida dentro de MÁS; null = el menú de MÁS. */
    val seccionMas: SeccionMas? = null,
    /** Host cuyos servicios se abren al entrar en SERVICIOS (desde Flota). */
    val hostServiciosInicial: String? = null,
    /** Host cuyos snippets se abren al entrar en MÁS > SNIPPETS (desde Flota). */
    val hostSnippetsInicial: String? = null,
    /** Ejecución que se abre al entrar en MÁS > RESULTADOS (al lanzar un snippet). */
    val ejecucionInicial: Long? = null,
    val inventario: InventarioVista = InventarioVista(),
    val filtro: String = "",
    val sincronizacion: EstadoSincronizacionVista = EstadoSincronizacionVista(),
    val sesiones: List<SesionVista> = emptyList(),
    val tuneles: List<TunelVista> = emptyList(),
    val conflictos: List<ConflictoHuellaVista> = emptyList(),
    val detalle: DetalleVista? = null,
    val ficha: FichaHostVista? = null,
    val tunelesFicha: List<TunelVista> = emptyList(),
    /** Pila de diálogos; el último es el que se pinta (se apilan sobre el abierto). */
    val dialogos: List<DialogoVista> = emptyList(),
    val asistente: AsistenteVista? = null,
    val claveMovil: ClaveMovilVista? = null,
    val version: VersionVista? = null,
    val cargando: Boolean = false,
    /** Se está creando una clave del móvil nueva (CA 31). */
    val creandoClave: Boolean = false,
    val iniciado: Boolean = false,
    val tamanoLetra: Int = ZOOM_POR_DEFECTO,
    /**
     * Ajustes de Flota, Servicios y Deliberación vigentes (informe §5.4): los
     * mismos del escritorio, en DataStore y aplicados al núcleo.
     */
    val ajustesMovil: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
    /** Avisos de la última acotación o guardado de ajustes, para enseñarlos. */
    val avisosAjustes: List<String> = emptyList(),
    /** Sesiones que quedaron abiertas cuando Android mató MAGI (aviso §4.6). */
    val sesionesPerdidas: Int? = null,
    /** Acciones o ejecuciones que Android cerró con MAGI (aviso §7.12). */
    val trabajoPerdido: TrabajoPerdidoVista? = null,
    /** La clave del móvil se recreó: hay que reinstalarla en los hosts. */
    val avisoClaveRecreada: Boolean = false,
    val aviso: AvisoVista? = null,
) {
    /** Diálogo visible: el último que llegó. */
    val dialogoActual: DialogoVista? get() = dialogos.lastOrNull()

    /** Sesión visible en el panel derecho, si la hay. */
    val sesionVisible: Long? get() = (detalle as? DetalleVista.Terminal)?.sesionId

    /** Host del detalle de servicios o logs, si lo hay. */
    val hostServiciosVisible: String?
        get() = when (val actual = detalle) {
            is DetalleVista.ServiciosHost -> actual.hostUuid
            is DetalleVista.Servicio -> actual.hostUuid
            is DetalleVista.Logs -> actual.hostUuid
            else -> null
        }
}
