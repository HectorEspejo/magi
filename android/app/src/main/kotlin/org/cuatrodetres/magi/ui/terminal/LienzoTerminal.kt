package org.cuatrodetres.magi.ui.terminal

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.withTimeoutOrNull
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.modelos.ColorVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.SeleccionVista
import org.cuatrodetres.magi.ui.modelos.anchoDeCaracter
import org.cuatrodetres.magi.ui.modelos.celdasDeFila
import org.cuatrodetres.magi.ui.tema.FamiliaJetBrainsMono
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Pintado del terminal en Canvas por celdas (informe §6.3 y §7.7).
 *
 * - Los tramos se pintan en orden, respetando negrita, cursiva, subrayado,
 *   inverso y los colores de 16/256/24 bits de §6.7.
 * - Un carácter ancho ocupa dos celdas; nunca se pinta por anchura de fuente.
 * - El cursor es un bloque ámbar; la selección, ámbar al 35 % con tiradores.
 * - Gestos: pellizcar = zoom (8-24 sp), arrastrar = historial, mantener
 *   pulsado = seleccionar palabra y arrastrar para extender.
 */
@Composable
fun LienzoTerminal(
    pantalla: PantallaVista?,
    zoom: Int,
    seleccion: SeleccionVista?,
    alTocar: () -> Unit,
    alRedimensionar: (Int, Int) -> Unit,
    alDesplazar: (Int) -> Unit,
    alZoom: (Float) -> Unit,
    alIniciarSeleccion: (Int, Int) -> Unit,
    alExtenderSeleccion: (Int, Int) -> Unit,
    alMoverTiradorInicio: (Int, Int) -> Unit,
    alMoverTiradorFin: (Int, Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val medidor = rememberTextMeasurer()
    val estiloBase = remember(zoom) {
        TextStyle(fontFamily = FamiliaJetBrainsMono, fontSize = zoom.sp, color = Paleta.TEXTO)
    }
    val medida = remember(zoom, medidor) {
        val resultado = medidor.measure("M", estiloBase, maxLines = 1, softWrap = false)
        resultado.size.width.toFloat().coerceAtLeast(1f) to resultado.size.height.toFloat().coerceAtLeast(1f)
    }
    val anchoCelda = medida.first
    val altoFila = medida.second
    var tamano by remember { mutableStateOf(IntSize.Zero) }
    val densidad = LocalDensity.current

    LaunchedEffect(tamano, anchoCelda, altoFila) {
        if (tamano.width > 0 && tamano.height > 0) {
            val columnas = (tamano.width / anchoCelda).toInt()
            val filas = (tamano.height / altoFila).toInt()
            alRedimensionar(columnas, filas)
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Paleta.FONDO)
            .onSizeChanged { tamano = it },
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .pointerInput(pantalla?.sesion, anchoCelda, altoFila, seleccion) {
                    if (pantalla == null) return@pointerInput
                    val radioTirador = with(densidad) { 28.dp.toPx() }
                    fun celdaDe(posicion: Offset): PosicionVista {
                        val columna = (posicion.x / anchoCelda).toInt()
                            .coerceIn(0, (pantalla.columnas - 1).coerceAtLeast(0))
                        val fila = (posicion.y / altoFila).toInt()
                            .coerceIn(0, (pantalla.filas - 1).coerceAtLeast(0))
                        return PosicionVista(columna, fila)
                    }

                    fun centroDe(posicion: PosicionVista): Offset = Offset(
                        x = posicion.columna * anchoCelda + anchoCelda / 2f,
                        y = posicion.fila * altoFila + altoFila / 2f,
                    )

                    awaitEachGesture {
                        val abajo = awaitFirstDown(requireUnconsumed = false)
                        val celdaInicial = celdaDe(abajo.position)
                        var modo = MODO_PENDIENTE
                        var ultimo = abajo.position
                        var arrastre = 0f
                        var distanciaAnterior = 0f

                        if (seleccion != null) {
                            val radio = radioTirador
                            when {
                                (abajo.position - centroDe(seleccion.desde)).getDistance() < radio ->
                                    modo = MODO_TIRADOR_INICIO
                                (abajo.position - centroDe(seleccion.hasta)).getDistance() < radio ->
                                    modo = MODO_TIRADOR_FIN
                            }
                        }

                        var primerEvento = true
                        while (true) {
                            val evento = if (modo == MODO_PENDIENTE && primerEvento) {
                                withTimeoutOrNull(400L) { awaitPointerEvent() }
                            } else {
                                awaitPointerEvent()
                            }
                            primerEvento = false
                            if (evento == null) {
                                // 400 ms sin mover: mantener pulsado = seleccionar palabra.
                                modo = MODO_SELECCION
                                alIniciarSeleccion(celdaInicial.fila, celdaInicial.columna)
                                continue
                            }
                            val activos = evento.changes.filter { it.pressed }
                            if (activos.isEmpty()) break
                            if (activos.size >= 2) {
                                val a = activos[0].position
                                val b = activos[1].position
                                val distancia = (a - b).getDistance()
                                if (distancia > 0f) {
                                    if (distanciaAnterior > 0f) alZoom(distancia / distanciaAnterior)
                                    distanciaAnterior = distancia
                                }
                                modo = MODO_ZOOM
                                evento.changes.forEach { it.consume() }
                                continue
                            }
                            if (modo == MODO_ZOOM) {
                                distanciaAnterior = 0f
                                continue
                            }
                            val cambio: PointerInputChange = activos.first()
                            val delta = cambio.position - ultimo
                            ultimo = cambio.position
                            if (modo == MODO_PENDIENTE) {
                                val distanciaDesdeInicio = (cambio.position - abajo.position).getDistance()
                                if (distanciaDesdeInicio > viewConfiguration.touchSlop) {
                                    modo = MODO_DESPLAZAR
                                }
                            }
                            when (modo) {
                                MODO_DESPLAZAR -> {
                                    arrastre += -delta.y
                                    val filas = (arrastre / altoFila).toInt()
                                    if (filas != 0) {
                                        alDesplazar(filas)
                                        arrastre -= filas * altoFila
                                    }
                                }
                                MODO_SELECCION -> {
                                    val celda = celdaDe(cambio.position)
                                    alExtenderSeleccion(celda.fila, celda.columna)
                                }
                                MODO_TIRADOR_INICIO -> {
                                    val celda = celdaDe(cambio.position)
                                    alMoverTiradorInicio(celda.fila, celda.columna)
                                }
                                MODO_TIRADOR_FIN -> {
                                    val celda = celdaDe(cambio.position)
                                    alMoverTiradorFin(celda.fila, celda.columna)
                                }
                                else -> Unit
                            }
                            cambio.consume()
                        }
                        if (modo == MODO_PENDIENTE) alTocar()
                    }
                },
        ) {
            if (pantalla != null) {
                dibujarTerminal(
                    pantalla = pantalla,
                    seleccion = seleccion,
                    estiloBase = estiloBase,
                    medidor = medidor,
                    anchoCelda = anchoCelda,
                    altoFila = altoFila,
                )
            }
        }
        if (pantalla == null) {
            Text(
                text = stringResource(R.string.terminal_esperando),
                style = MaterialTheme.typography.bodyLarge,
                color = Paleta.GRIS,
                modifier = Modifier.align(Alignment.Center).padding(16.dp),
            )
        }
    }
}

private const val MODO_PENDIENTE = 0
private const val MODO_DESPLAZAR = 1
private const val MODO_SELECCION = 2
private const val MODO_TIRADOR_INICIO = 3
private const val MODO_TIRADOR_FIN = 4
private const val MODO_ZOOM = 5

/** Pinta la pantalla completa por celdas (filas, tramos y estilos). */
private fun DrawScope.dibujarTerminal(
    pantalla: PantallaVista,
    seleccion: SeleccionVista?,
    estiloBase: TextStyle,
    medidor: TextMeasurer,
    anchoCelda: Float,
    altoFila: Float,
) {
    val filas = pantalla.filas.coerceIn(0, 500)
    for (fila in 0 until filas) {
        val tramos = pantalla.filasCambiadas[fila].orEmpty()
        var columna = 0
        for (tramo in tramos) {
            val colorTexto = colorDe(tramo.primerPlano)
            val colorFondo = if (tramo.inverso) colorTexto else colorFondoDe(tramo.fondo)
            val textoFinal = if (tramo.inverso && tramo.fondo == ColorVista.Predeterminado) {
                Paleta.FONDO
            } else {
                colorTexto
            }
            val estilo = estiloBase.copy(
                color = textoFinal,
                fontWeight = if (tramo.negrita) FontWeight.Bold else FontWeight.Normal,
                fontStyle = if (tramo.cursiva) FontStyle.Italic else FontStyle.Normal,
                textDecoration = if (tramo.subrayado) TextDecoration.Underline else null,
            )
            var i = 0
            while (i < tramo.texto.length) {
                val punto = tramo.texto.codePointAt(i)
                val ancho = anchoDeCaracter(punto)
                if (ancho <= 0) {
                    i += Character.charCount(punto)
                    continue
                }
                if (ancho == 2) {
                    if (colorFondo != null) {
                        drawRect(
                            color = colorFondo,
                            topLeft = Offset(columna * anchoCelda, fila * altoFila),
                            size = Size(anchoCelda * 2f, altoFila),
                        )
                    }
                    val texto = String(Character.toChars(punto))
                    val pintado = medidor.measure(texto, estilo, maxLines = 1, softWrap = false)
                    drawText(pintado, topLeft = Offset(columna * anchoCelda, fila * altoFila))
                    i += Character.charCount(punto)
                    columna += 2
                } else {
                    val inicio = i
                    while (i < tramo.texto.length && anchoDeCaracter(tramo.texto.codePointAt(i)) == 1) {
                        i += Character.charCount(tramo.texto.codePointAt(i))
                    }
                    val texto = tramo.texto.substring(inicio, i)
                    val celdas = texto.codePointCount(0, texto.length)
                    if (colorFondo != null) {
                        drawRect(
                            color = colorFondo,
                            topLeft = Offset(columna * anchoCelda, fila * altoFila),
                            size = Size(anchoCelda * celdas, altoFila),
                        )
                    }
                    val pintado = medidor.measure(texto, estilo, maxLines = 1, softWrap = false)
                    drawText(pintado, topLeft = Offset(columna * anchoCelda, fila * altoFila))
                    columna += celdas
                }
            }
        }
    }

    // Selección (ámbar al 35 %, §6.7) por encima del texto.
    if (seleccion != null) {
        for (fila in seleccion.desde.fila..seleccion.hasta.fila) {
            if (fila < 0 || fila >= filas) continue
            val primera = if (fila == seleccion.desde.fila) seleccion.desde.columna else 0
            val ultima = if (fila == seleccion.hasta.fila) seleccion.hasta.columna else pantalla.columnas - 1
            drawRect(
                color = Paleta.SELECCION,
                topLeft = Offset(primera * anchoCelda, fila * altoFila),
                size = Size((ultima - primera + 1) * anchoCelda, altoFila),
            )
        }
        val radio = altoFila * 0.42f
        drawCircle(
            color = Paleta.AMBAR,
            radius = radio,
            center = Offset(
                seleccion.desde.columna * anchoCelda + anchoCelda / 2f,
                seleccion.desde.fila * altoFila + altoFila,
            ),
        )
        drawCircle(
            color = Paleta.AMBAR,
            radius = radio,
            center = Offset(
                seleccion.hasta.columna * anchoCelda + anchoCelda / 2f,
                seleccion.hasta.fila * altoFila + altoFila,
            ),
        )
    }

    // Cursor de bloque ámbar (solo al final del historial).
    val cursor = pantalla.cursor
    if (cursor != null && pantalla.desplazamiento == 0) {
        val x = cursor.columna * anchoCelda
        val y = cursor.fila * altoFila
        drawRect(color = Paleta.AMBAR, topLeft = Offset(x, y), size = Size(anchoCelda, altoFila))
        val celdas = celdasDeFila(pantalla.filasCambiadas[cursor.fila].orEmpty())
        val caracter = celdas.getOrNull(cursor.columna).orEmpty()
        if (caracter.isNotEmpty() && caracter != " ") {
            val estilo = estiloBase.copy(color = Paleta.FONDO, fontWeight = FontWeight.Bold)
            val pintado = medidor.measure(caracter, estilo, maxLines = 1, softWrap = false)
            drawText(pintado, topLeft = Offset(x, y))
        }
    }
}
