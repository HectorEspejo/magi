package org.cuatrodetres.magi.ui.dialogos

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.tema.Paleta

/** Tiempo que hay que mantener pulsado para ejecutar lo deliberado (§7.8). */
const val MANTENER_MS: Long = 1_000

/** Eventos de la máquina de «mantener pulsado». */
enum class EventoMantener {
    /** No pasa nada. */
    NADA,

    /** Se empezó a mantener. */
    EMPEZADO,

    /** Se cumplió el segundo y hay que ejecutar (una sola vez). */
    CUMPLIDO,

    /** Se soltó antes de tiempo: hay que avisar «mantén pulsado para ejecutar». */
    SOLTADO_ANTES,
}

/**
 * Máquina de estados de «mantener pulsado para ejecutar» (informe §7.8).
 *
 * Es pura y con reloj inyectable para probarla en la JVM con un reloj falso,
 * sin Compose ni Android: [alPulsar] empieza, cada fotograma avanza el relleno,
 * [alSoltar] avisa si se suelta antes de tiempo y [porAccesibilidad] ejecuta de
 * una vez cuando TalkBack lanza la acción de pulsación larga. Una vez cumplido,
 * no vuelve a dispararse hasta [reiniciar].
 */
class MaquinaBotonMantener(private val relojMs: () -> Long) {

    /** Relleno del botón, de 0 a 1. */
    var progreso: Float = 0f
        private set

    /** Hay una pulsación en curso (aún sin cumplir). */
    var pulsando: Boolean = false
        private set

    /** Ya se cumplió: no se dispara otra vez. */
    var disparado: Boolean = false
        private set

    private var desde: Long? = null

    /** Empieza a mantener; solo vale si no estaba pulsando ni cumplido. */
    fun alPulsar(): EventoMantener {
        if (disparado || desde != null) return EventoMantener.NADA
        desde = relojMs()
        pulsando = true
        progreso = 0f
        return EventoMantener.EMPEZADO
    }

    /** Avanza un fotograma; devuelve [EventoMantener.CUMPLIDO] justo al llegar a 1 s. */
    fun alFotograma(): EventoMantener {
        val inicio = desde ?: return EventoMantener.NADA
        val transcurrido = (relojMs() - inicio).coerceAtLeast(0L)
        progreso = (transcurrido.toFloat() / MANTENER_MS.toFloat()).coerceIn(0f, 1f)
        if (progreso < 1f) return EventoMantener.NADA
        desde = null
        pulsando = false
        disparado = true
        progreso = 1f
        return EventoMantener.CUMPLIDO
    }

    /** Suelta el dedo: si no se había cumplido, hay que avisar. */
    fun alSoltar(): EventoMantener {
        if (desde == null) return EventoMantener.NADA
        desde = null
        pulsando = false
        progreso = 0f
        return EventoMantener.SOLTADO_ANTES
    }

    /**
     * Acción de pulsación larga de TalkBack: se toca dos veces y se mantiene.
     * Cumple de una vez y una sola vez (el gesto ya es sostenido).
     */
    fun porAccesibilidad(): EventoMantener {
        if (disparado) return EventoMantener.NADA
        desde = null
        pulsando = false
        disparado = true
        progreso = 1f
        return EventoMantener.CUMPLIDO
    }

    /** Deja el botón otra vez listo (nueva deliberación). */
    fun reiniciar() {
        desde = null
        pulsando = false
        disparado = false
        progreso = 0f
    }
}

/**
 * Botón que se rellena durante 1 s y vibra al empezar y al cumplirse (D271).
 *
 * Lo que no delibera no usa este botón: las confirmaciones normales son de un
 * toque. Soltarlo antes no ejecuta nada y avisa «mantén pulsado para ejecutar».
 * Con TalkBack expone la acción de pulsación larga con su etiqueta.
 */
@Composable
fun BotonMantener(
    texto: String,
    alCumplirse: () -> Unit,
    modifier: Modifier = Modifier,
    etiquetaAccesible: String = texto,
    habilitado: Boolean = true,
    alSoltarAntes: () -> Unit = {},
) {
    val maquina = remember { MaquinaBotonMantener { System.nanoTime() / 1_000_000L } }
    var pulsando by remember { mutableStateOf(false) }
    var progreso by remember { mutableFloatStateOf(0f) }
    var cumplido by remember { mutableStateOf(false) }
    val haptico = LocalHapticFeedback.current
    val avisoMantener = stringResource(R.string.delib_mantener_para_ejecutar)

    // Un fotograma por refresco mientras se mantiene: rellena y dispara al 1 s.
    LaunchedEffect(pulsando) {
        if (!pulsando) return@LaunchedEffect
        while (true) {
            withFrameNanos { }
            val evento = maquina.alFotograma()
            progreso = maquina.progreso
            if (evento == EventoMantener.CUMPLIDO) {
                pulsando = false
                cumplido = true
                haptico.performHapticFeedback(HapticFeedbackType.LongPress)
                alCumplirse()
                break
            }
        }
    }

    // Si se deshabilita a mitad (se cerró la deliberación), se deja listo.
    LaunchedEffect(habilitado) {
        if (!habilitado) {
            maquina.reiniciar()
            pulsando = false
            progreso = 0f
            cumplido = false
        }
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Paleta.SUPERFICIE)
            .pointerInput(habilitado, cumplido) {
                if (!habilitado || cumplido) return@pointerInput
                detectTapGestures(
                    onPress = {
                        val empezo = maquina.alPulsar() == EventoMantener.EMPEZADO
                        if (!empezo) {
                            tryAwaitRelease()
                            return@detectTapGestures
                        }
                        pulsando = true
                        progreso = 0f
                        haptico.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        val soltado = tryAwaitRelease()
                        pulsando = false
                        progreso = 0f
                        val evento = maquina.alSoltar()
                        if (soltado && evento == EventoMantener.SOLTADO_ANTES) {
                            alSoltarAntes()
                        }
                    },
                )
            }
            .semantics {
                contentDescription = etiquetaAccesible
                role = Role.Button
                if (habilitado) {
                    onClick(label = avisoMantener) {
                        alSoltarAntes()
                        true
                    }
                    onLongClick(label = etiquetaAccesible) {
                        when (maquina.porAccesibilidad()) {
                            EventoMantener.CUMPLIDO -> {
                                cumplido = true
                                progreso = 1f
                                haptico.performHapticFeedback(HapticFeedbackType.LongPress)
                                alCumplirse()
                                true
                            }
                            else -> false
                        }
                    }
                } else {
                    disabled()
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        if (progreso > 0f) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .fillMaxWidth(progreso)
                    .background(Paleta.AMBAR.copy(alpha = 0.35f)),
            )
        }
        Text(
            text = texto,
            style = MaterialTheme.typography.labelLarge,
            color = if (habilitado) Paleta.AMBAR else Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 12.dp),
        )
    }
}

/**
 * Confirmación simple de la cabina (T6): un toque, porque ya es el segundo paso
 * de una acción que no delibera. La usan Snippets, Resultados y Servicios.
 */
@Composable
fun DialogoConfirmacionSimple(
    titulo: String,
    texto: String,
    alConfirmar: () -> Unit,
    alCancelar: () -> Unit,
    modifier: Modifier = Modifier,
    textoConfirmar: String = stringResource(R.string.accion_confirmar),
    textoCancelar: String = stringResource(R.string.accion_cancelar),
    destructivo: Boolean = true,
) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = alCancelar,
        modifier = modifier,
        title = {
            Text(
                text = titulo,
                color = if (destructivo) Paleta.ROJO else Paleta.TEXTO,
            )
        },
        text = { Text(text = texto) },
        confirmButton = {
            androidx.compose.material3.TextButton(onClick = alConfirmar) {
                Text(
                    text = textoConfirmar,
                    color = if (destructivo) Paleta.ROJO else Paleta.AMBAR,
                )
            }
        },
        dismissButton = {
            androidx.compose.material3.TextButton(onClick = alCancelar) {
                Text(text = textoCancelar)
            }
        },
    )
}
