package org.cuatrodetres.magi.ui.snippets

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.modelos.VariableSnippetVista
import org.cuatrodetres.magi.ui.vistaModelo.VistaModeloBase

/** Filtro de la lista de snippets: nombre, comando, descripción, etiquetas o destino. */
internal fun filtrarSnippets(snippets: List<SnippetVista>, consulta: String): List<SnippetVista> {
    val texto = consulta.trim().lowercase()
    if (texto.isEmpty()) return snippets
    return snippets.filter { snippet ->
        snippet.nombre.lowercase().contains(texto) ||
            snippet.comando.lowercase().contains(texto) ||
            snippet.descripcion.lowercase().contains(texto) ||
            snippet.etiquetas.any { it.lowercase().contains(texto) } ||
            snippet.destinos.any { it.texto.lowercase().contains(texto) } ||
            snippet.hosts.any { it.nombre.lowercase().contains(texto) }
    }
}

/**
 * Estado de la pantalla Snippets (informe §6.8 y §7.6): la lista con su filtro,
 * el snippet abierto y su preparación de ejecución.
 */
data class EstadoSnippetsUi(
    val todos: List<SnippetVista> = emptyList(),
    /** Si se llegó desde la ficha de un host: solo sus snippets. */
    val hostInicial: String? = null,
    val filtro: String = "",
    val cargando: Boolean = false,
    val seleccionado: SnippetVista? = null,
    val hostsMarcados: Set<String> = emptySet(),
    val variables: List<VariableSnippetVista> = emptyList(),
    val pararAlFallo: Boolean = false,
    val preparando: Boolean = false,
    val preparacion: PreparacionEjecucionVista? = null,
    val planDeliberacion: PlanDeliberacionVista? = null,
    val ejecucionLanzada: Long? = null,
    val error: String? = null,
) {
    /** Snippets que casan con la búsqueda. */
    val visibles: List<SnippetVista> get() = filtrarSnippets(todos, filtro)

    /** Hosts resueltos del snippet abierto. */
    val hosts: List<HostDestinoVista> get() = seleccionado?.hosts ?: emptyList()

    /** Variables detectadas sin valor (no dejan continuar). */
    val variablesSinValor: List<VariableSnippetVista>
        get() = variables.filter { it.faltaValor }

    /** Se puede preparar y lanzar. */
    val puedeContinuar: Boolean
        get() = seleccionado != null && hostsMarcados.isNotEmpty() &&
            variablesSinValor.isEmpty() && !preparando

    /** Errores por campo que devolvió el núcleo al preparar. */
    val erroresPorCampo: Map<String, String>
        get() = preparacion?.errores?.associate { it.campo to it.mensaje }.orEmpty()
}

/**
 * ViewModel de Snippets (D269): solo lee del núcleo los snippets sincronizados,
 * prepara la ejecución en uno o varios hosts y la lanza; la deliberación, si el
 * núcleo dice que hace falta, la pinta la pantalla con el diálogo MAGI.
 *
 * No decide destinos, variables ni deliberación: todo eso lo hace el núcleo.
 */
class VistaModeloSnippets(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoSnippetsUi())
    val estado: StateFlow<EstadoSnippetsUi> = _estado.asStateFlow()

    private var iniciadoCon: String? = null
    private var iniciado = false

    /** Carga la lista: todos los snippets o solo los de [hostInicial] (la ficha). */
    fun iniciar(hostInicial: String?) {
        if (iniciado && iniciadoCon == hostInicial) return
        iniciado = true
        iniciadoCon = hostInicial
        _estado.update {
            it.copy(
                hostInicial = hostInicial,
                cargando = true,
                seleccionado = null,
                error = null,
            )
        }
        alcance.launch {
            try {
                val lista = if (hostInicial.isNullOrEmpty()) {
                    fuente.snippets(null)
                } else {
                    fuente.snippetsDeHost(hostInicial)
                }
                _estado.update { it.copy(todos = lista, cargando = false) }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        cargando = false,
                        error = error.message ?: "no se pudieron leer los snippets",
                    )
                }
            }
        }
    }

    /** Vuelve a leer la lista (al volver a la pantalla o tras sincronizar). */
    fun recargar() {
        iniciado = false
        iniciar(_estado.value.hostInicial)
    }

    fun cambiarFiltro(texto: String) {
        _estado.update { it.copy(filtro = texto) }
    }

    /** Abre un snippet desde la lista: deja marcados todos sus hosts (o el de la ficha). */
    fun abrir(uuid: String) {
        val snippet = _estado.value.todos.firstOrNull { it.uuid == uuid } ?: return
        val hostInicial = _estado.value.hostInicial
        val marcados = if (hostInicial.isNullOrEmpty()) {
            snippet.hosts.map { it.uuid }.toSet()
        } else {
            snippet.hosts.filter { it.uuid == hostInicial }.map { it.uuid }.toSet()
        }
        _estado.update {
            it.copy(
                seleccionado = snippet,
                hostsMarcados = marcados,
                variables = snippet.variables.map { variable ->
                    VariableSnippetVista(
                        nombre = variable.nombre,
                        valorDefecto = variable.valorDefecto,
                        valor = null,
                    )
                },
                pararAlFallo = snippet.pararAlFallo,
                preparando = false,
                preparacion = null,
                planDeliberacion = null,
                error = null,
            )
        }
    }

    /** Cierra el detalle y vuelve a la lista. */
    fun cerrarDetalle() {
        _estado.update {
            it.copy(
                seleccionado = null,
                preparando = false,
                preparacion = null,
                planDeliberacion = null,
                error = null,
            )
        }
    }

    fun alternarHost(uuid: String) {
        _estado.update { actual ->
            val marcados = if (uuid in actual.hostsMarcados) {
                actual.hostsMarcados - uuid
            } else {
                actual.hostsMarcados + uuid
            }
            actual.copy(hostsMarcados = marcados)
        }
    }

    /** Valor escrito de una variable; vacío = usar el defecto del snippet. */
    fun cambiarVariable(nombre: String, texto: String) {
        _estado.update { actual ->
            actual.copy(
                variables = actual.variables.map { variable ->
                    if (variable.nombre == nombre) {
                        variable.copy(valor = texto.ifEmpty { null })
                    } else {
                        variable
                    }
                },
            )
        }
    }

    fun alternarPararAlFallo() {
        _estado.update { it.copy(pararAlFallo = !it.pararAlFallo) }
    }

    /**
     * CONTINUAR: prepara la ejecución con el núcleo. Si no delibera, la lanza;
     * si delibera, deja el plan para que la pantalla abra el diálogo MAGI.
     */
    fun continuar() {
        val snippet = _estado.value.seleccionado ?: return
        if (!_estado.value.puedeContinuar) return
        _estado.update { it.copy(preparando = true, error = null, preparacion = null) }
        alcance.launch {
            try {
                val pedido = fuente.prepararEjecucion(
                    snippet.uuid,
                    _estado.value.hostsMarcados.toList(),
                    _estado.value.variables,
                )
                when (pedido) {
                    is ResultadoNucleoVista.Listo -> {
                        val preparacion = pedido.valor
                        _estado.update {
                            it.copy(preparando = false, preparacion = preparacion)
                        }
                        if (preparacion.errores.isEmpty() && preparacion.hosts.isNotEmpty()) {
                            if (preparacion.delibera) {
                                _estado.update {
                                    it.copy(
                                        planDeliberacion =
                                            PlanDeliberacionVista.Snippet(preparacion),
                                    )
                                }
                            } else {
                                ejecutar(preparacion, null)
                            }
                        }
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(preparando = false, error = pedido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        preparando = false,
                        error = error.message ?: "no se pudo preparar la ejecución",
                    )
                }
            }
        }
    }

    /** Lanza una preparación con la deliberación que la autoriza, si la hay. */
    fun ejecutar(preparacion: PreparacionEjecucionVista, deliberacion: Long?) {
        _estado.update { it.copy(preparando = true, error = null) }
        alcance.launch {
            try {
                when (val leido = fuente.ejecutarSnippet(
                    preparacion,
                    _estado.value.pararAlFallo,
                    deliberacion,
                )) {
                    is ResultadoNucleoVista.Listo -> _estado.update {
                        it.copy(preparando = false, ejecucionLanzada = leido.valor)
                    }
                    is ResultadoNucleoVista.Error -> _estado.update {
                        it.copy(preparando = false, error = leido.mensaje)
                    }
                }
            } catch (error: Throwable) {
                _estado.update {
                    it.copy(
                        preparando = false,
                        error = error.message ?: "no se pudo lanzar la ejecución",
                    )
                }
            }
        }
    }

    /** Ejecuta lo deliberado y aprobado o forzado, con el id de DELIBERACIONES. */
    fun ejecutarDeliberado(plan: PlanDeliberacionVista, deliberacion: Long) {
        _estado.update { it.copy(planDeliberacion = null) }
        if (plan is PlanDeliberacionVista.Snippet) {
            ejecutar(plan.preparacion, deliberacion)
        }
    }

    /** El usuario cerró el diálogo MAGI sin resolver: no se ejecuta nada. */
    fun cancelarDeliberacion() {
        _estado.update { it.copy(planDeliberacion = null) }
    }

    /** La pantalla ya se ocupó de la ejecución lanzada (navegar o avisar). */
    fun descartarEjecucion() {
        _estado.update { it.copy(ejecucionLanzada = null) }
    }

    fun limpiarError() {
        _estado.update { it.copy(error = null) }
    }
}
