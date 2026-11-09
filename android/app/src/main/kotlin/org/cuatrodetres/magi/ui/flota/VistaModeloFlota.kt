package org.cuatrodetres.magi.ui.flota

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.BloqueFlotaVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.GrupoVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.agruparFlota
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/**
 * Estado de la pantalla Flota (informe §6.1): hosts con su estado de sondeo,
 * grupos plegables, búsqueda y la ficha del host seleccionado.
 */
data class EstadoFlotaUi(
    val hosts: List<HostFlotaVista> = emptyList(),
    val grupos: List<GrupoVista> = emptyList(),
    val filtro: String = "",
    val ficha: FichaHostVista? = null,
    val hostSeleccionado: String? = null,
    val tunelesFicha: List<TunelVista> = emptyList(),
    val conflicto: ConflictoHuellaVista? = null,
    val cargando: Boolean = false,
    /** Estado completo de la sincronización (indicador ⇅ de la cabecera). */
    val sincronizacion: EstadoSincronizacionVista = EstadoSincronizacionVista(),
    val aviso: AvisoVista? = null,
) {
    /** Hay una pasada de sincronización en curso. */
    val sincronizando: Boolean
        get() = sincronizacion.fase == FaseSincronizacionVista.SINCRONIZANDO

    /** Hay algún host sondeando ahora mismo (indicador ◐ N/M). */
    val sondeando: Boolean get() = hosts.any { it.estado == EstadoFlotaVista.SONDEANDO }

    /** Hosts que ya tienen un estado leído, para el contador N/M. */
    val leidos: Int
        get() = hosts.count {
            it.estado != EstadoFlotaVista.SONDEANDO && it.estado != EstadoFlotaVista.FRIA
        }

    /** Hosts que casan con el filtro (mismo criterio que el inventario). */
    val visibles: List<HostFlotaVista>
        get() = if (filtro.isBlank()) hosts else hosts.filter { casa(it, filtro) }

    /** Bloques de la lista: grupos plegables y «sin grupo» al final. */
    val bloques: List<BloqueFlotaVista> get() = agruparFlota(visibles, grupos)

    /** Hosts que se están viendo: casan con el filtro y su grupo está desplegado. */
    val visiblesDesplegados: List<HostFlotaVista>
        get() = bloques.filter { !it.plegado }.flatMap { it.hosts }

    /** Hosts sondeables (SSH) que casan con el filtro. */
    val sondeablesVisibles: List<HostFlotaVista>
        get() = visibles.filter { it.esSondeable }

    /** Hosts sondeables visibles y fuera de grupos plegados. */
    val sondeablesDesplegados: List<HostFlotaVista>
        get() = visiblesDesplegados.filter { it.esSondeable }
}

/** ¿El host casa con la búsqueda? Mismo criterio que el inventario de la F11. */
internal fun casa(host: HostFlotaVista, consulta: String): Boolean {
    val texto = consulta.trim().lowercase()
    if (texto.isEmpty()) return true
    return host.nombre.lowercase().contains(texto) ||
        host.direccion.lowercase().contains(texto) ||
        host.etiquetas.any { it.lowercase().contains(texto) } ||
        host.grupoUuid?.lowercase()?.contains(texto) == true ||
        host.culpables.any { it.lowercase().contains(texto) }
}

/**
 * ViewModel de Flota (informe §4.1 y §7.1). Sondea al abrir y al volver, al
 * deslizar (con la pasada de sincronización) y en el refresco automático, que
 * solo lanza los hosts que se están viendo. No decide estados: los recibe del
 * núcleo por el flujo [FuenteNucleo.flota].
 */
class VistaModeloFlota(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoFlotaUi(cargando = true))
    val estado: StateFlow<EstadoFlotaUi> = _estado.asStateFlow()

    private var iniciado = false

    init {
        alcance.launch {
            fuente.flota.collect { lista ->
                _estado.update { it.copy(hosts = lista, cargando = false) }
                actualizarFicha()
            }
        }
        alcance.launch {
            fuente.sincronizacion.collect { sincronizacion ->
                _estado.update { it.copy(sincronizacion = sincronizacion) }
            }
        }
    }

    /** Abre Flota: carga grupos y hosts y sondea todos (motivo manual). */
    fun iniciar() {
        if (iniciado) return
        iniciado = true
        _estado.update { it.copy(cargando = true) }
        alcance.launch {
            cargarGrupos()
            val lista = try {
                fuente.flota(null)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty())
                emptyList()
            }
            // Si el flujo ya trajo una lista más nueva mientras se leía la
            // instantánea, no se pisa con datos viejos.
            if (_estado.value.cargando) {
                _estado.update { it.copy(hosts = lista, cargando = false) }
            }
            sondearTodos()
        }
    }

    /** Vuelve a Flota desde otra pantalla: sondea todos otra vez (manual). */
    fun alVolverALaVista() {
        if (!iniciado) {
            iniciar()
            return
        }
        alcance.launch {
            cargarGrupos()
            sondearTodos()
        }
    }

    /** Deslizar hacia abajo: sondea los que casan con el filtro y sincroniza. */
    fun deslizar() {
        val hosts = _estado.value.sondeablesVisibles.map { it.uuid }
        alcance.launch {
            if (hosts.isNotEmpty()) {
                lanzarSondeo(hosts)
            }
            val resultado = try {
                fuente.sincronizarAhora()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                ResultadoAccion.Error(e.message.orEmpty())
            }
            if (resultado is ResultadoAccion.Error) {
                avisoNucleo(resultado.mensaje)
            }
        }
    }

    /**
     * Refresco automático (cada flota_refresco_seg con Flota a la vista): solo
     * los hosts que se están viendo (filtro y grupos desplegados).
     */
    fun refrescarAutomatico() {
        val hosts = _estado.value.sondeablesDesplegados.map { it.uuid }
        if (hosts.isEmpty()) return
        alcance.launch { lanzarSondeo(hosts) }
    }

    fun cambiarFiltro(texto: String) {
        _estado.update { it.copy(filtro = texto) }
    }

    fun plegarGrupo(uuid: String) {
        val grupo = _estado.value.grupos.firstOrNull { it.uuid == uuid } ?: return
        val plegado = !grupo.plegado
        _estado.update { actual ->
            actual.copy(
                grupos = actual.grupos.map {
                    if (it.uuid == uuid) it.copy(plegado = plegado) else it
                },
            )
        }
        alcance.launch {
            try {
                fuente.plegarGrupo(uuid, plegado)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty())
            }
        }
    }

    /** Abre la ficha de un host en el panel de detalle. */
    fun abrirFicha(hostUuid: String) {
        _estado.update { it.copy(hostSeleccionado = hostUuid, ficha = null) }
        alcance.launch {
            val ficha = try {
                fuente.fichaHost(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                avisoNucleo(e.message.orEmpty())
                null
            }
            if (ficha == null) {
                // Solo se cierra la selección si sigue siendo esta ficha.
                if (_estado.value.hostSeleccionado == hostUuid) {
                    _estado.update { it.copy(hostSeleccionado = null) }
                }
                avisoNucleo("el host ya no existe")
                return@launch
            }
            val tuneles = try {
                fuente.tuneles(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                emptyList()
            }
            val conflictos = try {
                fuente.conflictosHuella()
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                emptyList()
            }
            // Si entretanto se abrió otra ficha o se cerró, este resultado ya no
            // vale: aplicarlo dejaría el panel con datos de otro host.
            if (_estado.value.hostSeleccionado != hostUuid) return@launch
            _estado.update {
                it.copy(
                    ficha = ficha,
                    tunelesFicha = tuneles,
                    conflicto = conflictos.firstOrNull { conflicto -> conflicto.hostUuid == hostUuid },
                )
            }
        }
    }

    fun cerrarFicha() {
        _estado.update { it.copy(hostSeleccionado = null, ficha = null, tunelesFicha = emptyList(), conflicto = null) }
    }

    fun refrescarTunelesDeLaFicha() {
        val hostUuid = _estado.value.hostSeleccionado ?: return
        alcance.launch {
            val tuneles = try {
                fuente.tuneles(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                return@launch
            }
            if (_estado.value.hostSeleccionado != hostUuid) return@launch
            _estado.update { it.copy(tunelesFicha = tuneles) }
        }
    }

    fun limpiarAviso(identificador: Long) {
        _estado.update { actual ->
            if (actual.aviso?.identificador == identificador) actual.copy(aviso = null) else actual
        }
    }

    // ------------------------------------------------------------ internos

    private fun sondearTodos() {
        val hosts = _estado.value.sondeablesVisibles.map { it.uuid }
        if (hosts.isEmpty()) return
        alcance.launch { lanzarSondeo(hosts) }
    }

    private suspend fun lanzarSondeo(hosts: List<String>) {
        val resultado = try {
            fuente.sondear(hosts)
        } catch (e: Exception) {
            if (e is kotlinx.coroutines.CancellationException) throw e
            ResultadoAccion.Error(e.message.orEmpty())
        }
        if (resultado is ResultadoAccion.Error) {
            avisoNucleo(resultado.mensaje)
        }
    }

    private suspend fun cargarGrupos() {
        val inventario = try {
            fuente.inventario(null)
        } catch (e: Exception) {
            if (e is kotlinx.coroutines.CancellationException) throw e
            return
        }
        _estado.update { it.copy(grupos = inventario.grupos) }
    }

    /** La ficha abierta se refresca con cada lista nueva de Flota. */
    private fun actualizarFicha() {
        val hostUuid = _estado.value.hostSeleccionado ?: return
        alcance.launch {
            val ficha = try {
                fuente.fichaHost(hostUuid)
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                return@launch
            }
            if (ficha != null && _estado.value.hostSeleccionado == hostUuid) {
                _estado.update { it.copy(ficha = ficha) }
            }
        }
    }

    private fun avisoNucleo(mensaje: String) {
        _estado.update {
            it.copy(
                aviso = AvisoVista.DeNucleo(
                    identificador = System.nanoTime(),
                    mensaje = mensaje,
                    esError = true,
                ),
            )
        }
    }
}
