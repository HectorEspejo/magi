package org.cuatrodetres.magi.ui.registro

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.DetalleRegistroVista
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.FiltroRegistroVista
import org.cuatrodetres.magi.ui.modelos.HostVista
import org.cuatrodetres.magi.ui.modelos.TAMANO_PAGINA_REGISTRO
import org.cuatrodetres.magi.ui.modelos.TipoRegistroVista
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/** Fecha corta local de una entrada: «9 nov 21:52». */
internal fun fechaRegistro(instanteSegundos: Long): String {
    if (instanteSegundos <= 0) return "—"
    val formato = DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.forLanguageTag("es"))
    return formato.format(Instant.ofEpochSecond(instanteSegundos).atZone(ZoneId.systemDefault()))
}

/**
 * Estado de Registro (informe §6.10 y §7.10): las entradas por páginas, el
 * filtro por host, tipo y texto, y el detalle de la entrada abierta.
 */
data class EstadoRegistroUi(
    val entradas: List<EntradaRegistroVista> = emptyList(),
    val filtro: FiltroRegistroVista = FiltroRegistroVista(),
    /** Hosts del inventario para el filtro del menú ⋮. */
    val hosts: List<HostVista> = emptyList(),
    val cargando: Boolean = false,
    val cargandoMas: Boolean = false,
    val hayMas: Boolean = false,
    val seleccionadaId: Long? = null,
    val detalle: DetalleRegistroVista? = null,
    val cargandoDetalle: Boolean = false,
    val error: String? = null,
) {
    /** Entrada abierta (se relee de la lista). */
    val seleccionada: EntradaRegistroVista?
        get() = seleccionadaId?.let { id -> entradas.firstOrNull { it.id == id } }

    /** Hay un filtro puesto (host, tipos o texto). */
    val filtrando: Boolean get() = !filtro.esTodos
}

/**
 * ViewModel de Registro (D272): pide páginas de 50 al núcleo con un cursor por
 * id de la última entrada, aplica los filtros en la consulta y pide el detalle
 * de una entrada (con la deliberación leída de DELIBERACIONES).
 *
 * La app no escribe nada: solo lee el registro del propio móvil.
 */
class VistaModeloRegistro(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoRegistroUi(cargando = true))
    val estado: StateFlow<EstadoRegistroUi> = _estado.asStateFlow()

    private var iniciado = false
    private var trabajoPagina: Job? = null

    /** Carga los hosts del filtro y la primera página. */
    fun iniciar() {
        if (iniciado) return
        iniciado = true
        _estado.update { it.copy(cargando = true) }
        alcance.launch {
            try {
                val inventario = fuente.inventario()
                _estado.update { it.copy(hosts = inventario.hosts) }
            } catch (_: Throwable) {
                // Sin inventario no se puede filtrar por host; el registro sigue.
            }
            cargarPrimeraPagina()
        }
    }

    /** Vuelve a la primera página con el filtro actual. */
    fun recargar() {
        iniciado = true
        _estado.update {
            it.copy(
                cargando = true,
                seleccionadaId = null,
                detalle = null,
                error = null,
            )
        }
        trabajoPagina?.cancel()
        trabajoPagina = alcance.launch { cargarPrimeraPagina() }
    }

    private suspend fun cargarPrimeraPagina() {
        val filtro = _estado.value.filtro
        try {
            val pagina = fuente.registro(
                filtro = filtro,
                desde = null,
                cuantas = TAMANO_PAGINA_REGISTRO,
            )
            // Si el filtro cambió mientras se leía, esta página ya no vale.
            if (_estado.value.filtro != filtro) return
            _estado.update {
                it.copy(
                    entradas = pagina,
                    cargando = false,
                    hayMas = pagina.size >= TAMANO_PAGINA_REGISTRO,
                    error = null,
                )
            }
        } catch (error: Throwable) {
            _estado.update {
                it.copy(
                    cargando = false,
                    error = error.message ?: "no se pudo leer el registro",
                )
            }
        }
    }

    /** Pide la siguiente página desde el id de la última entrada. */
    fun cargarMas() {
        if (!_estado.value.hayMas || _estado.value.cargandoMas) return
        val desde = _estado.value.entradas.lastOrNull()?.id ?: return
        val filtro = _estado.value.filtro
        _estado.update { it.copy(cargandoMas = true) }
        alcance.launch {
            try {
                val pagina = fuente.registro(
                    filtro = filtro,
                    desde = desde,
                    cuantas = TAMANO_PAGINA_REGISTRO,
                )
                // La página se une solo si es la misma lista y el mismo filtro;
                // si entretanto se recargó otra cosa, se descarta.
                if (_estado.value.filtro != filtro ||
                    _estado.value.entradas.lastOrNull()?.id != desde
                ) {
                    _estado.update { it.copy(cargandoMas = false) }
                    return@launch
                }
                _estado.update {
                    it.copy(
                        entradas = it.entradas + pagina,
                        cargandoMas = false,
                        hayMas = pagina.size >= TAMANO_PAGINA_REGISTRO,
                    )
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        cargandoMas = false,
                        error = error.message ?: "no se pudieron leer más entradas",
                    )
                }
            }
        }
    }

    /** Cambia el texto del filtro (se aplica con [aplicarFiltro]). */
    fun cambiarTexto(texto: String) {
        _estado.update { it.copy(filtro = it.filtro.copy(texto = texto)) }
    }

    /** Aplica el filtro actual: vuelve a la primera página. */
    fun aplicarFiltro() {
        recargar()
    }

    /** Alterna un tipo en los filtros rápidos de §7.10. */
    fun alternarTipo(tipo: TipoRegistroVista) {
        _estado.update { it.copy(filtro = it.filtro.conTipo(tipo)) }
        recargar()
    }

    /** Quita solo los tipos del filtro (la ficha «todos»). */
    fun limpiarTipos() {
        _estado.update { it.copy(filtro = it.filtro.copy(tipos = emptyList())) }
        recargar()
    }

    /** Filtra por host; null = todos. */
    fun cambiarHost(hostUuid: String?) {
        _estado.update {
            it.copy(
                filtro = it.filtro.copy(
                    hostUuid = hostUuid?.takeIf { uuid -> uuid.isNotBlank() },
                ),
            )
        }
        recargar()
    }

    /** Quita host, tipos y texto del filtro. */
    fun limpiarFiltros() {
        _estado.update { it.copy(filtro = FiltroRegistroVista()) }
        recargar()
    }

    /** Abre el detalle de una entrada (y su deliberación, si la hay). */
    fun seleccionar(id: Long) {
        _estado.update {
            it.copy(seleccionadaId = id, detalle = null, cargandoDetalle = true, error = null)
        }
        alcance.launch {
            try {
                val detalle = fuente.registroDetalle(id)
                // Si se abrió otra entrada mientras se leía, este detalle ya no
                // vale (enseñaría la deliberación de otra entrada).
                if (_estado.value.seleccionadaId != id) return@launch
                _estado.update {
                    if (detalle == null) {
                        it.copy(
                            cargandoDetalle = false,
                            error = "no se pudo leer el detalle de la entrada",
                        )
                    } else {
                        it.copy(detalle = detalle, cargandoDetalle = false)
                    }
                }
            } catch (error: Throwable) {
                if (_estado.value.seleccionadaId != id) return@launch
                _estado.update {
                    it.copy(
                        cargandoDetalle = false,
                        error = error.message ?: "no se pudo leer el detalle de la entrada",
                    )
                }
            }
        }
    }

    /** Cierra el detalle. */
    fun cerrarDetalle() {
        _estado.update { it.copy(seleccionadaId = null, detalle = null) }
    }

    fun limpiarError() {
        _estado.update { it.copy(error = null) }
    }
}
