package org.cuatrodetres.magi.ui.modelos

import org.cuatrodetres.magi.navegacion.Destino
import org.cuatrodetres.magi.navegacion.DetalleVista

/**
 * Estado de la app que consume Compose (una sola instantánea observable).
 *
 * Todo lo que se pinta sale de aquí: la interfaz no guarda estado propio de
 * negocio. Vive en modelos para que las pantallas y los ViewModels compartan el
 * mismo tipo sin depender unos de otros.
 */
data class EstadoMagi(
    val bloqueo: EstadoBloqueoVista = EstadoBloqueoVista.DESBLOQUEADA,
    val destino: Destino = Destino.HOSTS,
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
    val iniciado: Boolean = false,
    val tamanoLetra: Int = ZOOM_POR_DEFECTO,
    val aviso: AvisoVista? = null,
) {
    /** Diálogo visible: el último que llegó. */
    val dialogoActual: DialogoVista? get() = dialogos.lastOrNull()

    /** Sesión visible en el panel derecho, si la hay. */
    val sesionVisible: Long? get() = (detalle as? DetalleVista.Terminal)?.sesionId
}
