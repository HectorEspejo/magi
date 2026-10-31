package org.cuatrodetres.magi.ui.vistaModelo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.cuatrodetres.magi.ui.fuente.FuenteNucleo
import org.cuatrodetres.magi.ui.fuente.FuenteNucleoVacia
import org.cuatrodetres.magi.ui.modelos.AvisoVista
import org.cuatrodetres.magi.ui.modelos.EntradaVista
import org.cuatrodetres.magi.ui.modelos.ModificadoresFila
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.SeleccionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TipoAviso
import org.cuatrodetres.magi.ui.modelos.ZOOM_POR_DEFECTO
import org.cuatrodetres.magi.ui.modelos.acotarZoom
import org.cuatrodetres.magi.ui.modelos.ajustarZoom
import org.cuatrodetres.magi.ui.modelos.aplicarPantalla
import org.cuatrodetres.magi.ui.modelos.interpretarEntrada
import org.cuatrodetres.magi.ui.modelos.normalizarSeleccion
import org.cuatrodetres.magi.ui.modelos.palabraEnCelda

/**
 * Estado del terminal: una pantalla acumulada por sesión (solo el tamaño del
 * PTY, nunca el historial), zoom, selección y modificadores.
 */
data class EstadoTerminal(
    /** Pantalla acumulada por id de sesión. Acotada por el tamaño del PTY. */
    val pantallas: Map<Long, PantallaVista> = emptyMap(),
    val sesionVisible: Long? = null,
    /** Tamaño de letra en sp (8-24). */
    val zoom: Int = ZOOM_POR_DEFECTO,
    val modificadores: ModificadoresFila = ModificadoresFila(),
    val seleccion: SeleccionVista? = null,
    val anclaSeleccion: PosicionVista? = null,
    /** Pegado de varias líneas pendiente de confirmación (§6.3). */
    val pegadoPendiente: String? = null,
    val aviso: AvisoVista? = null,
) {
    /** Pantalla de la sesión visible. */
    val pantalla: PantallaVista? get() = sesionVisible?.let { pantallas[it] }
}

/**
 * ViewModel del terminal (informe §6.3 y §7).
 *
 * Recibe las pantallas por tramos del núcleo, acumula solo las filas cambiadas,
 * manda las teclas al núcleo para que las codifique (§7.6) y gestiona zoom,
 * selección, pegado con confirmación e historial. No guarda el historial
 * completo: el núcleo es quien lo tiene y la vista pide `desplazar`.
 */
class VistaModeloTerminal(
    private val fuente: FuenteNucleo = FuenteNucleoVacia,
    ambito: CoroutineScope? = null,
) : VistaModeloBase(ambito) {

    private val _estado = MutableStateFlow(EstadoTerminal())
    val estado: StateFlow<EstadoTerminal> = _estado.asStateFlow()

    private val tamanosEnviados = mutableMapOf<Long, Pair<Int, Int>>()
    private var trabajoPersistenciaZoom: Job? = null
    private var contadorAvisos = 0L

    init {
        alcance.launch {
            fuente.pantallas.collect { pantalla ->
                if (pantalla != null) recibirPantalla(pantalla)
            }
        }
        alcance.launch {
            fuente.tamanoLetra.collect { tamano ->
                _estado.update { it.copy(zoom = acotarZoom(tamano)) }
            }
        }
    }

    private fun recibirPantalla(nueva: PantallaVista) {
        _estado.update { actual ->
            actual.copy(
                pantallas = actual.pantallas +
                    (nueva.sesion to aplicarPantalla(actual.pantallas[nueva.sesion], nueva)),
            )
        }
    }

    /** Asocia la vista a una sesión (la que se acaba de abrir o de elegir). */
    fun mostrarSesion(sesionId: Long) {
        _estado.update {
            it.copy(sesionVisible = sesionId, seleccion = null, anclaSeleccion = null, pegadoPendiente = null)
        }
    }

    /** Al cerrarse una sesión se suelta su pantalla. */
    fun limpiarSesion(sesionId: Long) {
        tamanosEnviados.remove(sesionId)
        _estado.update { actual ->
            actual.copy(
                pantallas = actual.pantallas - sesionId,
                seleccion = if (actual.sesionVisible == sesionId) null else actual.seleccion,
                anclaSeleccion = if (actual.sesionVisible == sesionId) null else actual.anclaSeleccion,
                sesionVisible = if (actual.sesionVisible == sesionId) null else actual.sesionVisible,
            )
        }
    }

    fun limpiarTodo() {
        tamanosEnviados.clear()
        _estado.update { EstadoTerminal(zoom = it.zoom) }
    }

    // -----------------------------------------------------------------------
    // Entrada
    // -----------------------------------------------------------------------

    /** Texto del teclado del sistema en modo bruto (sin autocorrección). */
    fun alEscribir(texto: String) {
        val sesion = _estado.value.sesionVisible ?: return
        val entradas = interpretarEntrada(texto)
        if (entradas.isEmpty()) return
        alcance.launch {
            for (entrada in entradas) {
                val resultado = try {
                    when (entrada) {
                        is EntradaVista.Texto -> fuente.escribir(sesion, entrada.texto.toByteArray(Charsets.UTF_8))
                        is EntradaVista.Pulsacion -> fuente.tecla(sesion, entrada.tecla, entrada.modificadores)
                    }
                } catch (e: Exception) {
                    ResultadoAccion.Error(e.message.orEmpty())
                }
                if (resultado is ResultadoAccion.Error) avisoNucleo(resultado.mensaje)
            }
            volverAlFinal(sesion)
        }
    }

    /** Tecla de la fila extra: usa los modificadores activos (pendiente o fijo). */
    fun alPulsarTecla(tecla: TeclaVista) {
        val sesion = _estado.value.sesionVisible ?: return
        val modificadores = _estado.value.modificadores.aModificadores()
        _estado.update { it.copy(modificadores = it.modificadores.trasTecla()) }
        enviarTecla(sesion, tecla, modificadores)
        alcance.launch { volverAlFinal(sesion) }
    }

    /** Teclado físico: la tecla y sus modificadores van tal cual al núcleo. */
    fun alTeclaFisica(tecla: TeclaVista, modificadores: ModificadoresVista = ModificadoresVista.NINGUNO) {
        val sesion = _estado.value.sesionVisible ?: return
        enviarTecla(sesion, tecla, modificadores)
        alcance.launch { volverAlFinal(sesion) }
    }

    fun alPulsarCtrl() {
        _estado.update { it.copy(modificadores = it.modificadores.alPulsarCtrl()) }
    }

    fun alPulsarAlt() {
        _estado.update { it.copy(modificadores = it.modificadores.alPulsarAlt()) }
    }

    private fun enviarTecla(sesion: Long, tecla: TeclaVista, modificadores: ModificadoresVista) {
        alcance.launch {
            val resultado = try {
                fuente.tecla(sesion, tecla, modificadores)
            } catch (e: Exception) {
                ResultadoAccion.Error(e.message.orEmpty())
            }
            if (resultado is ResultadoAccion.Error) avisoNucleo(resultado.mensaje)
        }
    }

    // -----------------------------------------------------------------------
    // Historial
    // -----------------------------------------------------------------------

    /** Desplaza el historial (positivo = hacia arriba). */
    fun alDesplazar(filas: Int) {
        if (filas == 0) return
        val sesion = _estado.value.sesionVisible ?: return
        alcance.launch {
            try {
                fuente.desplazar(sesion, filas)
            } catch (e: Exception) {
                // El historial se reintentará en el siguiente gesto.
            }
        }
    }

    /** Al escribir se vuelve al final del historial (§6.3). */
    private suspend fun volverAlFinal(sesion: Long) {
        val pantalla = _estado.value.pantallas[sesion] ?: return
        if (pantalla.desplazamiento > 0) {
            try {
                fuente.desplazar(sesion, -pantalla.desplazamiento)
            } catch (e: Exception) {
                // Sin historial desplazado no hay nada que deshacer.
            }
        }
    }

    // -----------------------------------------------------------------------
    // Redimensionado (girar, plegar o abrir el teclado)
    // -----------------------------------------------------------------------

    fun alRedimensionar(columnas: Int, filas: Int) {
        val sesion = _estado.value.sesionVisible ?: return
        val columnasSaneadas = columnas.coerceAtLeast(2)
        val filasSaneadas = filas.coerceAtLeast(1)
        val tamano = columnasSaneadas to filasSaneadas
        if (tamanosEnviados[sesion] == tamano) return
        tamanosEnviados[sesion] = tamano
        alcance.launch {
            try {
                fuente.redimensionar(sesion, columnasSaneadas, filasSaneadas)
            } catch (e: Exception) {
                tamanosEnviados.remove(sesion)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Zoom (8-24 sp, persistido)
    // -----------------------------------------------------------------------

    fun alZoom(factor: Float) {
        val actual = _estado.value.zoom
        val nuevo = ajustarZoom(actual, factor)
        if (nuevo == actual) return
        _estado.update { it.copy(zoom = nuevo) }
        programarPersistenciaZoom()
    }

    fun fijarZoom(sp: Int) {
        val nuevo = acotarZoom(sp)
        if (nuevo == _estado.value.zoom) return
        _estado.update { it.copy(zoom = nuevo) }
        programarPersistenciaZoom()
    }

    private fun programarPersistenciaZoom() {
        trabajoPersistenciaZoom?.cancel()
        trabajoPersistenciaZoom = alcance.launch {
            delay(400)
            try {
                fuente.cambiarTamanoLetra(_estado.value.zoom)
            } catch (e: Exception) {
                // Si no se puede guardar, el zoom vale para esta vista.
            }
        }
    }

    // -----------------------------------------------------------------------
    // Selección y portapapeles
    // -----------------------------------------------------------------------

    /** Mantener pulsado: selecciona la palabra de la celda (o la celda). */
    fun iniciarSeleccion(fila: Int, columna: Int) {
        val pantalla = _estado.value.pantalla ?: return
        val seleccion = palabraEnCelda(pantalla, fila, columna) ?: return
        _estado.update { it.copy(seleccion = seleccion, anclaSeleccion = seleccion.desde) }
    }

    /** Arrastrar tras mantener pulsado: extiende la selección. */
    fun extenderSeleccion(fila: Int, columna: Int) {
        val actual = _estado.value
        val ancla = actual.anclaSeleccion ?: return
        val pantalla = actual.pantalla ?: return
        val columnaAcotada = columna.coerceIn(0, (pantalla.columnas - 1).coerceAtLeast(0))
        val filaAcotada = fila.coerceIn(0, (pantalla.filas - 1).coerceAtLeast(0))
        _estado.update {
            it.copy(seleccion = normalizarSeleccion(ancla, PosicionVista(columnaAcotada, filaAcotada)))
        }
    }

    /** Tirador de inicio: mueve el extremo «desde». */
    fun moverTiradorInicio(fila: Int, columna: Int) {
        val actual = _estado.value
        val seleccion = actual.seleccion ?: return
        val pantalla = actual.pantalla ?: return
        val columnaAcotada = columna.coerceIn(0, (pantalla.columnas - 1).coerceAtLeast(0))
        val filaAcotada = fila.coerceIn(0, (pantalla.filas - 1).coerceAtLeast(0))
        _estado.update {
            it.copy(seleccion = normalizarSeleccion(PosicionVista(columnaAcotada, filaAcotada), seleccion.hasta))
        }
    }

    /** Tirador de fin: mueve el extremo «hasta». */
    fun moverTiradorFin(fila: Int, columna: Int) {
        val actual = _estado.value
        val seleccion = actual.seleccion ?: return
        val pantalla = actual.pantalla ?: return
        val columnaAcotada = columna.coerceIn(0, (pantalla.columnas - 1).coerceAtLeast(0))
        val filaAcotada = fila.coerceIn(0, (pantalla.filas - 1).coerceAtLeast(0))
        _estado.update {
            it.copy(seleccion = normalizarSeleccion(seleccion.desde, PosicionVista(columnaAcotada, filaAcotada)))
        }
    }

    fun limpiarSeleccion() {
        _estado.update { it.copy(seleccion = null, anclaSeleccion = null) }
    }

    /** Texto seleccionado según el núcleo (null si no hay selección o sesión). */
    suspend fun textoSeleccionado(): String? {
        val seleccion = _estado.value.seleccion ?: return null
        val sesion = _estado.value.sesionVisible ?: return null
        val texto = try {
            fuente.textoSeleccion(sesion, seleccion.desde, seleccion.hasta)
        } catch (e: Exception) {
            null
        }
        limpiarSeleccion()
        return texto
    }

    /** Pegar: si es multilínea y el remoto no activó el modo 2004, pide confirmación. */
    fun solicitarPegado(texto: String) {
        if (texto.isEmpty()) return
        val sesion = _estado.value.sesionVisible ?: return
        val multilinea = texto.contains('\n') || texto.contains('\r')
        if (multilinea && !fuente.pegadoEntreCorchetes(sesion)) {
            _estado.update { it.copy(pegadoPendiente = texto) }
        } else {
            pegar(sesion, texto)
        }
    }

    fun confirmarPegado() {
        val texto = _estado.value.pegadoPendiente ?: return
        val sesion = _estado.value.sesionVisible ?: return
        _estado.update { it.copy(pegadoPendiente = null) }
        pegar(sesion, texto)
    }

    fun cancelarPegado() {
        _estado.update { it.copy(pegadoPendiente = null) }
    }

    private fun pegar(sesion: Long, texto: String) {
        alcance.launch {
            val resultado = try {
                fuente.pegar(sesion, texto)
            } catch (e: Exception) {
                ResultadoAccion.Error(e.message.orEmpty())
            }
            if (resultado is ResultadoAccion.Error) avisoNucleo(resultado.mensaje)
            volverAlFinal(sesion)
        }
    }

    // -----------------------------------------------------------------------
    // Avisos
    // -----------------------------------------------------------------------

    fun limpiarAviso(identificador: Long) {
        _estado.update { actual ->
            if (actual.aviso?.identificador == identificador) actual.copy(aviso = null) else actual
        }
    }

    /** Aviso generado por la interfaz (copiar, portapapeles vacío…). */
    fun avisar(tipo: TipoAviso) {
        _estado.update { it.copy(aviso = AvisoVista.DeInterfaz(++contadorAvisos, tipo)) }
    }

    private fun avisoNucleo(mensaje: String) {
        val aviso = if (mensaje.isBlank()) {
            AvisoVista.DeInterfaz(++contadorAvisos, TipoAviso.ERROR_INESPERADO)
        } else {
            AvisoVista.DeNucleo(++contadorAvisos, mensaje, true)
        }
        _estado.update { it.copy(aviso = aviso) }
    }
}
