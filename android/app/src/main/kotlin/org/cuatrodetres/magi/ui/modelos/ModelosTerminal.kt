package org.cuatrodetres.magi.ui.modelos

/**
 * Sesiones y terminal (informe §6.3, §7.6 y §7.7).
 *
 * La pantalla llega del núcleo por tramos: solo las filas que cambiaron. Aquí
 * se acumulan esas filas en una pantalla del tamaño del PTY (no el historial:
 * el historial vive en el núcleo y se pide con `desplazar`).
 */

/** Estado de una sesión en memoria (diagrama de §3). */
enum class EstadoSesionVista {
    ABRIENDO,
    ABIERTA,
    CAIDA,
}

/** Entrada de la lista completa de sesiones que llega del núcleo. */
data class SesionVista(
    val id: Long,
    val hostUuid: String,
    val hostNombre: String,
    val estado: EstadoSesionVista,
    /** Fase del «abriendo» (resolviendo, conectando, verificando huella…) o el motivo de la caída. */
    val detalle: String? = null,
    val abiertaEn: Long? = null,
    /** Número de sesión de ese host: 1, 2… (nombre `host (n)`). */
    val numero: Int = 1,
) {
    /** Nombre visible: `host` o `host (2)` (F3). */
    val titulo: String
        get() = if (numero > 1) hostNombre + " (" + numero + ")" else hostNombre
}

/** Resultado de abrir una sesión (§4.3): id de la sesión o el motivo del fallo. */
sealed interface ResultadoAperturaVista {
    data class Abierta(val sesion: Long) : ResultadoAperturaVista
    data class Error(val mensaje: String) : ResultadoAperturaVista
}

/** Numera las sesiones de cada host por id: `host`, `host (2)`, `host (3)`. */
fun numerarSesiones(sesiones: List<SesionVista>): List<SesionVista> {
    val contador = mutableMapOf<String, Int>()
    return sesiones.map { sesion ->
        val numero = (contador[sesion.hostUuid] ?: 0) + 1
        contador[sesion.hostUuid] = numero
        sesion.copy(numero = numero)
    }
}

/** Glifo del estado de la sesión; el texto va en el recurso de cadena (T5). */
fun glifoSesion(estado: EstadoSesionVista): String = when (estado) {
    EstadoSesionVista.ABRIENDO -> "◐"
    EstadoSesionVista.ABIERTA -> "●"
    EstadoSesionVista.CAIDA -> "✕"
}

/**
 * Error de conexión de un host: los tres estados de §6.1 son excluyentes con las
 * sesiones; este helper decide el estado que se pinta en la fila.
 */
fun estadoHostDeSesiones(estadoActual: EstadoHostVista, abiertas: Int, abriendo: Int): EstadoHostVista =
    when {
        abiertas > 0 -> EstadoHostVista.CON_SESIONES
        abriendo > 0 -> EstadoHostVista.CONECTANDO
        else -> estadoActual
    }

// ===========================================================================
// Pantalla por tramos
// ===========================================================================

/** Color de un tramo: predeterminado, índice de 256 o 24 bits (informe §6.7). */
sealed interface ColorVista {
    data object Predeterminado : ColorVista
    data class Indice(val indice: Int) : ColorVista
    data class Rgb(val rojo: Int, val verde: Int, val azul: Int) : ColorVista
}

/** Tramo de texto con el mismo estilo; un carácter ancho ocupa dos celdas. */
data class TramoVista(
    val texto: String,
    val primerPlano: ColorVista = ColorVista.Predeterminado,
    val fondo: ColorVista = ColorVista.Predeterminado,
    val negrita: Boolean = false,
    val cursiva: Boolean = false,
    val subrayado: Boolean = false,
    val inverso: Boolean = false,
)

/** Celda del terminal (columna, fila), base 0. */
data class PosicionVista(val columna: Int, val fila: Int)

/**
 * Pantalla acumulada de una sesión: [filasCambiadas] solo trae las filas de la
 * última notificación; la pantalla completa es la fusión de todas las recibidas
 * (o la que llega con `completa = true`).
 */
data class PantallaVista(
    val sesion: Long,
    val columnas: Int,
    val filas: Int,
    val filasCambiadas: Map<Int, List<TramoVista>> = emptyMap(),
    val cursor: PosicionVista? = null,
    val desplazamiento: Int = 0,
    val completa: Boolean = false,
)

/**
 * Fusiona una pantalla recibida con la acumulada de esa sesión.
 *
 * - Si es la primera, cambia el tamaño o llega `completa = true`, sustituye.
 * - Si no, aplica solo las filas cambiadas. El estado queda acotado por el
 *   tamaño del PTY: nunca se guarda el historial completo en Compose.
 */
fun aplicarPantalla(actual: PantallaVista?, nueva: PantallaVista): PantallaVista {
    if (actual == null ||
        nueva.completa ||
        actual.sesion != nueva.sesion ||
        actual.columnas != nueva.columnas ||
        actual.filas != nueva.filas
    ) {
        return nueva.copy(completa = true)
    }
    return actual.copy(
        filasCambiadas = actual.filasCambiadas + nueva.filasCambiadas,
        cursor = nueva.cursor,
        desplazamiento = nueva.desplazamiento,
        completa = false,
    )
}

/** Filas de la pantalla en orden, rellenando con vacío las que no llegaron. */
fun filasDePantalla(pantalla: PantallaVista): List<List<TramoVista>> =
    (0 until pantalla.filas.coerceAtLeast(0)).map { fila ->
        pantalla.filasCambiadas[fila].orEmpty()
    }

/**
 * Celdas de una fila: el índice es la columna (base 0) y el valor es el texto
 * de la celda. La segunda celda de un carácter ancho queda vacía. Sirve para
 * pintar por celdas y para calcular selecciones y palabras.
 */
fun celdasDeFila(tramos: List<TramoVista>): List<String> {
    val celdas = mutableListOf<String>()
    tramos.forEach { tramo ->
        var i = 0
        while (i < tramo.texto.length) {
            val punto = tramo.texto.codePointAt(i)
            val caracteres = Character.charCount(punto)
            val texto = tramo.texto.substring(i, i + caracteres)
            val ancho = anchoDeCaracter(punto)
            celdas.add(texto)
            repeat((ancho - 1).coerceAtLeast(0)) { celdas.add("") }
            i += caracteres
        }
    }
    return celdas
}

/**
 * Ancho en celdas de un punto de código (East Asian Width simplificado): 0 para
 * combinantes y variaciones, 2 para CJK y emoji, 1 para el resto. Así los
 * caracteres anchos no descuadran las columnas del terminal.
 */
fun anchoDeCaracter(punto: Int): Int {
    if (punto == 0) return 0
    if (punto < 0x20 || (punto in 0x7F..0x9F)) return 0
    val ancho = when {
        punto in 0x0300..0x036F -> 0
        punto in 0x1AB0..0x1AFF -> 0
        punto in 0x1DC0..0x1DFF -> 0
        punto in 0x20D0..0x20FF -> 0
        punto in 0xFE00..0xFE0F -> 0
        punto in 0xFE20..0xFE2F -> 0
        punto == 0x200B -> 0
        punto in 0x1100..0x115F -> 2
        punto in 0x2E80..0x303E -> 2
        punto in 0x3041..0x33FF -> 2
        punto in 0x3400..0x4DBF -> 2
        punto in 0x4E00..0x9FFF -> 2
        punto in 0xA000..0xA4CF -> 2
        punto in 0xAC00..0xD7A3 -> 2
        punto in 0xF900..0xFAFF -> 2
        punto in 0xFE10..0xFE19 -> 2
        punto in 0xFE30..0xFE6F -> 2
        punto in 0xFF00..0xFF60 -> 2
        punto in 0xFFE0..0xFFE6 -> 2
        punto in 0x1F000..0x1FAFF -> 2
        punto in 0x20000..0x2FFFD -> 2
        punto in 0x30000..0x3FFFD -> 2
        else -> 1
    }
    return ancho
}

// ===========================================================================
// Teclas y modificadores
// ===========================================================================

/** Modificadores de una pulsación (informe §7.6). */
data class ModificadoresVista(
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val mayus: Boolean = false,
) {
    companion object {
        val NINGUNO = ModificadoresVista()
    }
}

/** Tecla que la interfaz manda al núcleo; él la codifica según el modo del terminal. */
sealed interface TeclaVista {
    data object Escape : TeclaVista
    data object Tab : TeclaVista
    data object Intro : TeclaVista
    data object Retroceso : TeclaVista
    data object Arriba : TeclaVista
    data object Abajo : TeclaVista
    data object Izquierda : TeclaVista
    data object Derecha : TeclaVista
    data object Inicio : TeclaVista
    data object Fin : TeclaVista
    data object RePag : TeclaVista
    data object AvPag : TeclaVista
    data object Suprimir : TeclaVista
    data object Insertar : TeclaVista
    data class F(val numero: Int) : TeclaVista
    data class Caracter(val texto: String) : TeclaVista
}

/** Estado de Ctrl/Alt de la fila de teclas: una pulsación, fijo, o apagado (D183). */
enum class ModoModificador { NINGUNO, PENDIENTE, FIJO }

/** Ctrl y Alt de la fila de teclas extra, con el estado visible (informe §6.3). */
data class ModificadoresFila(
    val ctrl: ModoModificador = ModoModificador.NINGUNO,
    val alt: ModoModificador = ModoModificador.NINGUNO,
) {
    val ctrlActivo: Boolean get() = ctrl != ModoModificador.NINGUNO
    val altActivo: Boolean get() = alt != ModoModificador.NINGUNO

    fun alPulsarCtrl(): ModificadoresFila = copy(ctrl = siguiente(ctrl))

    fun alPulsarAlt(): ModificadoresFila = copy(alt = siguiente(alt))

    /** Tras enviar una tecla, los pendientes se consumen y los fijos se quedan. */
    fun trasTecla(): ModificadoresFila = copy(
        ctrl = if (ctrl == ModoModificador.PENDIENTE) ModoModificador.NINGUNO else ctrl,
        alt = if (alt == ModoModificador.PENDIENTE) ModoModificador.NINGUNO else alt,
    )

    /** Modificadores que se mandan al núcleo en la próxima tecla. */
    fun aModificadores(): ModificadoresVista =
        ModificadoresVista(ctrl = ctrlActivo, alt = altActivo, mayus = false)

    private fun siguiente(modo: ModoModificador): ModoModificador = when (modo) {
        ModoModificador.NINGUNO -> ModoModificador.PENDIENTE
        ModoModificador.PENDIENTE -> ModoModificador.FIJO
        ModoModificador.FIJO -> ModoModificador.NINGUNO
    }
}

/** Entrada de texto: escribir tal cual o pulsar una tecla especial. */
sealed interface EntradaVista {
    data class Texto(val texto: String) : EntradaVista
    data class Pulsacion(val tecla: TeclaVista, val modificadores: ModificadoresVista = ModificadoresVista.NINGUNO) : EntradaVista
}

/**
 * Traduce lo que llega del teclado del sistema (modo bruto) a texto y teclas:
 * el intro del teclado virtual se convierte en tecla Intro y el tabulador en
 * Tab; el resto se escribe tal cual.
 */
fun interpretarEntrada(texto: String): List<EntradaVista> {
    val entradas = mutableListOf<EntradaVista>()
    val pendiente = StringBuilder()
    fun volcar() {
        if (pendiente.isNotEmpty()) {
            entradas.add(EntradaVista.Texto(pendiente.toString()))
            pendiente.clear()
        }
    }
    for (caracter in texto) {
        when (caracter) {
            '\r', '\n' -> {
                volcar()
                entradas.add(EntradaVista.Pulsacion(TeclaVista.Intro))
            }
            '\t' -> {
                volcar()
                entradas.add(EntradaVista.Pulsacion(TeclaVista.Tab))
            }
            else -> pendiente.append(caracter)
        }
    }
    volcar()
    return entradas
}

// ===========================================================================
// Zoom (8-24 sp, informe §7.11)
// ===========================================================================

const val ZOOM_MINIMO: Int = 8
const val ZOOM_MAXIMO: Int = 24
const val ZOOM_POR_DEFECTO: Int = 13

/** Acota un tamaño de letra al rango permitido. */
fun acotarZoom(valor: Int): Int = valor.coerceIn(ZOOM_MINIMO, ZOOM_MAXIMO)

/** Nuevo zoom al pellizcar: actual * factor, acotado y redondeado. */
fun ajustarZoom(actual: Int, factor: Float): Int =
    acotarZoom(Math.round(actual * factor).toInt())

// ===========================================================================
// Selección de texto
// ===========================================================================

/** Selección ordenada entre dos celdas de la pantalla (base 0). */
data class SeleccionVista(val desde: PosicionVista, val hasta: PosicionVista) {
    /** Si la celda está dentro de la selección (en orden de lectura). */
    fun contiene(fila: Int, columna: Int): Boolean {
        if (fila < desde.fila || fila > hasta.fila) return false
        if (desde.fila == hasta.fila) return columna in desde.columna..hasta.columna
        if (fila == desde.fila) return columna >= desde.columna
        if (fila == hasta.fila) return columna <= hasta.columna
        return true
    }

    val vacia: Boolean get() = desde == hasta
}

/** Ordena ancla y extremo en orden de lectura. */
fun normalizarSeleccion(ancla: PosicionVista, extremo: PosicionVista): SeleccionVista {
    val antes = ancla.fila < extremo.fila ||
        (ancla.fila == extremo.fila && ancla.columna <= extremo.columna)
    return if (antes) SeleccionVista(ancla, extremo) else SeleccionVista(extremo, ancla)
}

private fun esCaracterDePalabra(celda: String): Boolean {
    if (celda.length != 1) return false
    val caracter = celda[0]
    return caracter.isLetterOrDigit() || caracter == '_' || caracter == '-' ||
        caracter == '.' || caracter == '/' || caracter == ':' || caracter == '@'
}

/**
 * Selecciona la palabra bajo una celda (mantener pulsado, §6.3). Si la celda no
 * es de palabra, selecciona solo esa celda.
 */
fun palabraEnCelda(pantalla: PantallaVista, fila: Int, columna: Int): SeleccionVista? {
    if (fila < 0 || fila >= pantalla.filas) return null
    val celdas = celdasDeFila(pantalla.filasCambiadas[fila].orEmpty())
    if (celdas.isEmpty()) return null
    val columnaAcotada = columna.coerceIn(0, celdas.lastIndex)
    if (!esCaracterDePalabra(celdas[columnaAcotada])) {
        val posicion = PosicionVista(columnaAcotada, fila)
        return SeleccionVista(posicion, posicion)
    }
    var inicio = columnaAcotada
    while (inicio > 0 && esCaracterDePalabra(celdas[inicio - 1])) inicio--
    var fin = columnaAcotada
    while (fin < celdas.lastIndex && esCaracterDePalabra(celdas[fin + 1])) fin++
    return SeleccionVista(PosicionVista(inicio, fila), PosicionVista(fin, fila))
}
