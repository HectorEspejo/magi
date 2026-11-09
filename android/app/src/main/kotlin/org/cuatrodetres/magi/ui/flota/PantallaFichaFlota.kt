package org.cuatrodetres.magi.ui.flota

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.ChipEtiqueta
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.DetalleFlotaVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.VerificacionesVista
import org.cuatrodetres.magi.ui.modelos.keepaliveEfectivo
import org.cuatrodetres.magi.ui.modelos.keepaliveEsDelMovil
import org.cuatrodetres.magi.ui.modelos.porcentaje
import org.cuatrodetres.magi.ui.modelos.textoRed
import org.cuatrodetres.magi.ui.modelos.textoUptime
import org.cuatrodetres.magi.ui.modelos.unDecimal
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Ficha de host de la Fase 14 (informe §6.2): el bloque FLOTA arriba (es lo
 * que se mira en una guardia), después las acciones y los datos de la Fase 11.
 *
 * VERIFICACIONES y AL CONECTAR son de solo lectura: se editan en el escritorio.
 * El móvil solo enseña lo que llega por la sincronización.
 */
@Composable
fun PantallaFichaFlota(
    ficha: FichaHostVista,
    host: HostFlotaVista?,
    tuneles: List<TunelVista>,
    conflicto: ConflictoHuellaVista?,
    alVolver: () -> Unit,
    alConectar: () -> Unit,
    alAbrirServicios: () -> Unit,
    alAbrirSnippets: () -> Unit,
    alAlternarTunel: (TunelVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    val escritorio = ficha.tipo.esEscritorio
    val detalleFlota = ficha.flota
    val estadoFlota = detalleFlota?.estado ?: host?.estado
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = ficha.nombre,
            alVolver = alVolver,
            glifoEstado = estadoFlota?.glifo ?: host?.estado?.glifo,
            textoEstado = estadoFlota?.palabra ?: host?.estado?.palabra,
            colorEstado = estadoFlota?.let(::colorEstadoFlota) ?: Paleta.GRIS,
            acciones = {
                if (host != null && host.numFallos > 0) {
                    Text(
                        text = "⚠" + host.numFallos,
                        style = MaterialTheme.typography.labelMedium,
                        color = Paleta.AMBAR,
                    )
                }
            },
        )

        if (conflicto != null) {
            BarraAviso(
                texto = stringResource(R.string.ficha_conflicto_huella, conflicto.hostNombre),
                tipo = TipoBarra.AVISO,
            )
        }

        if (!escritorio) {
            BloqueFlotaFicha(detalleFlota)
        } else {
            BarraAviso(
                texto = stringResource(R.string.ficha_escritorio_aviso),
                tipo = TipoBarra.AVISO,
            )
        }

        // Acciones de la ficha (mockup §6.2).
        if (!escritorio) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    BotonCabina(
                        texto = stringResource(R.string.accion_conectar),
                        alPulsar = alConectar,
                        primario = true,
                        modifier = Modifier.weight(1f),
                    )
                    BotonCabina(
                        texto = stringResource(R.string.accion_otra_sesion),
                        alPulsar = alConectar,
                        modifier = Modifier.weight(1f),
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    BotonCabina(
                        texto = stringResource(R.string.flota_accion_servicios),
                        alPulsar = alAbrirServicios,
                        modifier = Modifier.weight(1f),
                    )
                    BotonCabina(
                        texto = stringResource(R.string.flota_accion_snippets),
                        alPulsar = alAbrirSnippets,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }

        Separador()

        // Datos de la Fase 11.
        FilaDato(stringResource(R.string.ficha_tipo), ficha.tipo.etiqueta)
        FilaDato(stringResource(R.string.ficha_direccion), ficha.direccion + ":" + ficha.puerto)
        FilaDato(stringResource(R.string.ficha_usuario), ficha.usuario)
        FilaDato(
            stringResource(R.string.ficha_grupo),
            ficha.grupoNombre ?: stringResource(R.string.ficha_sin_grupo),
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.ficha_etiquetas),
                style = MaterialTheme.typography.labelMedium,
                color = Paleta.GRIS,
            )
            if (ficha.etiquetas.isEmpty()) {
                Text(
                    text = stringResource(R.string.ficha_ninguna),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.TEXTO,
                )
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    ficha.etiquetas.forEach { ChipEtiqueta(it) }
                }
            }
        }
        FilaDato(
            stringResource(R.string.ficha_salto),
            ficha.saltoNombre ?: stringResource(R.string.ficha_directo),
        )
        FilaDato(
            stringResource(R.string.ficha_keepalive),
            if (keepaliveEsDelMovil(ficha.keepaliveSegundos)) {
                stringResource(R.string.ficha_keepalive_movil, keepaliveEfectivo(ficha.keepaliveSegundos))
            } else {
                stringResource(R.string.ficha_keepalive_valor, keepaliveEfectivo(ficha.keepaliveSegundos))
            },
        )
        FilaDato(
            stringResource(R.string.ficha_en_el_movil),
            if (ficha.tieneContrasena) {
                stringResource(R.string.ficha_autenticacion_guardada)
            } else {
                stringResource(R.string.ficha_autenticacion_sin_guardar)
            },
        )
        if (ficha.huellas.isEmpty()) {
            FilaDato(stringResource(R.string.ficha_huella), stringResource(R.string.ficha_sin_huellas))
        } else {
            ficha.huellas.forEachIndexed { indice, huella ->
                FilaDato(
                    etiqueta = if (indice == 0) stringResource(R.string.ficha_huella) else "",
                    valor = huella.tipo + " " + huella.sha256,
                )
            }
        }

        // Verificaciones y snippet al conectar: solo lectura (se editan en el
        // escritorio y llegan por la sincronización).
        if (!escritorio) {
            Separador()
            EncabezadoSeccion(stringResource(R.string.flota_verificaciones))
            val verificaciones = ficha.verificaciones
            if (verificaciones == null || !verificaciones.algunaActiva) {
                Text(
                    text = stringResource(R.string.flota_sin_verificaciones),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.GRIS,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                )
            } else {
                verificaciones.lineas.forEach { linea ->
                    Text(
                        text = "· " + linea,
                        style = MaterialTheme.typography.bodyMedium,
                        color = Paleta.TEXTO,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
                    )
                }
            }
            EncabezadoSeccion(stringResource(R.string.flota_al_conectar))
            val snippet = ficha.snippetAlConectar
            Text(
                text = if (snippet == null) {
                    stringResource(R.string.ficha_ninguna)
                } else if (snippet.valido) {
                    stringResource(R.string.flota_snippet_al_conectar, snippet.nombre)
                } else {
                    stringResource(R.string.flota_snippet_no_valido, snippet.nombre)
                },
                style = MaterialTheme.typography.bodyMedium,
                color = if (snippet?.valido == false) Paleta.AMBAR else Paleta.TEXTO,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            )
        }

        // Túneles del host (F11).
        if (!escritorio) {
            Separador()
            EncabezadoSeccion(stringResource(R.string.ficha_tuneles))
            if (tuneles.isEmpty()) {
                Text(
                    text = stringResource(R.string.ficha_sin_tuneles),
                    style = MaterialTheme.typography.bodyMedium,
                    color = Paleta.GRIS,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                )
            } else {
                tuneles.forEach { tunel ->
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = tunel.nombre,
                            style = MaterialTheme.typography.bodyMedium,
                            color = Paleta.TEXTO,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            text = tunel.estado.name.lowercase(),
                            style = MaterialTheme.typography.labelMedium,
                            color = Paleta.GRIS,
                        )
                    }
                }
            }
        }
    }
}

/** Bloque FLOTA de la ficha: barras, red, uptime, vigilados y fallos (§6.2). */
@Composable
private fun BloqueFlotaFicha(detalle: DetalleFlotaVista?) {
    if (detalle == null) {
        BarraAviso(
            texto = stringResource(R.string.flota_sin_sondeo),
            tipo = TipoBarra.AVISO,
        )
        return
    }
    EncabezadoSeccion(stringResource(R.string.flota_seccion_flota))
    val metricas = detalle.metricas
    if (metricas != null) {
        if (metricas.carga1m != null) {
            BarraFlota(
                etiqueta = stringResource(R.string.flota_barra_carga),
                texto = unDecimal(metricas.carga1m) + " / " + (metricas.nucleos ?: 1),
                fraccion = metricas.cargaPorNucleo?.toFloat() ?: 0f,
                color = Paleta.AMBAR,
            )
        }
        metricas.memoriaPct?.let {
            BarraFlota(
                etiqueta = stringResource(R.string.flota_barra_memoria),
                texto = porcentaje(it),
                fraccion = (it / 100.0).toFloat(),
                color = Paleta.VERDE,
            )
        }
        metricas.discoPct?.let {
            BarraFlota(
                etiqueta = stringResource(R.string.flota_barra_disco),
                texto = porcentaje(it),
                fraccion = (it / 100.0).toFloat(),
                color = Paleta.VERDE,
            )
        }
        FilaDato(stringResource(R.string.flota_red), textoRed(metricas))
        FilaDato(stringResource(R.string.flota_activo), textoUptime(metricas.uptimeSeg))
    } else {
        Text(
            text = detalle.motivo ?: stringResource(R.string.flota_sin_sondeo),
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
    }
    val duracion = detalle.duracionMs ?: 0
    Text(
        text = if (detalle.ultimoSondeoEn == null) {
            stringResource(R.string.flota_sondeo_sin)
        } else {
            stringResource(R.string.flota_sondeo_hace, tiempoHaceTexto(detalle.ultimoSondeoEn), duracion)
        },
        style = MaterialTheme.typography.labelMedium,
        color = Paleta.GRIS,
        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
    )

    EncabezadoSeccion(stringResource(R.string.flota_seccion_vigilados))
    if (detalle.vigilados.isEmpty()) {
        Text(
            text = stringResource(R.string.flota_vigilados_ninguno),
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
    } else {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            detalle.vigilados.forEach { vigilado ->
                Text(
                    text = vigilado.glifo + " " + vigilado.nombre,
                    style = MaterialTheme.typography.bodyMedium,
                    color = when {
                        vigilado.activo -> Paleta.VERDE
                        vigilado.estado == "failed" -> Paleta.ROJO
                        else -> Paleta.GRIS
                    },
                )
            }
        }
    }

    EncabezadoSeccion(stringResource(R.string.flota_seccion_fallos))
    if (detalle.fallos.isEmpty()) {
        Text(
            text = stringResource(R.string.flota_fallos_ninguno),
            style = MaterialTheme.typography.bodyMedium,
            color = Paleta.GRIS,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
    } else {
        detalle.fallos.forEach { fallo ->
            Text(
                text = "✕ " + fallo.nombre + " · " + fallo.estado,
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.ROJO,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
            )
        }
    }
    if (detalle.aviso != null) {
        Text(
            text = detalle.aviso,
            style = MaterialTheme.typography.labelMedium,
            color = Paleta.AMBAR,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
        )
    }
}

/** Barra de un umbral con su etiqueta y su valor. */
@Composable
private fun BarraFlota(etiqueta: String, texto: String, fraccion: Float, color: androidx.compose.ui.graphics.Color) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = etiqueta,
            style = MaterialTheme.typography.labelMedium,
            color = Paleta.GRIS,
            modifier = Modifier.width(56.dp),
        )
        Box(
            modifier = Modifier
                .weight(1f)
                .height(8.dp)
                .clip(RoundedCornerShape(2.dp))
                .background(Paleta.SUPERFICIE),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(fraccion.coerceIn(0f, 1f))
                    .height(8.dp)
                    .clip(RoundedCornerShape(2.dp))
                    .background(color),
            )
        }
        Text(
            text = texto,
            style = MaterialTheme.typography.labelMedium,
            color = Paleta.TEXTO,
            modifier = Modifier.width(88.dp),
        )
    }
}

/** «hace 12 s» a partir de un instante epoch, sin depender de la vista de Tiempo. */
private fun tiempoHaceTexto(instante: Long?): String {
    if (instante == null) return "—"
    val segundos = (System.currentTimeMillis() / 1000L - instante).coerceAtLeast(0)
    return when {
        segundos < 60 -> segundos.toString() + " s"
        segundos < 3_600 -> (segundos / 60).toString() + " min"
        segundos < 86_400 -> (segundos / 3_600).toString() + " h"
        else -> (segundos / 86_400).toString() + " d"
    }
}
