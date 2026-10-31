package org.cuatrodetres.magi.ui.componentes

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.modelos.EstadoEquipoVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Texto y color de cada estado. El glifo va aparte (modelos) y el color nunca
 * es la única señal: siempre hay glifo y texto (T5).
 */

/** Texto del estado de un host de la lista. */
@Composable
fun textoEstadoHost(estado: EstadoHostVista, sesionesAbiertas: Int): String = when (estado) {
    EstadoHostVista.SIN_SESION -> stringResource(R.string.estado_host_sin_sesion)
    EstadoHostVista.CONECTANDO -> stringResource(R.string.estado_host_conectando)
    EstadoHostVista.CON_SESIONES -> stringResource(R.string.estado_host_con_sesiones, sesionesAbiertas)
    EstadoHostVista.ERROR -> stringResource(R.string.estado_host_error)
}

fun colorEstadoHost(estado: EstadoHostVista): Color = when (estado) {
    EstadoHostVista.SIN_SESION -> Paleta.GRIS
    EstadoHostVista.CONECTANDO -> Paleta.AMBAR
    EstadoHostVista.CON_SESIONES -> Paleta.VERDE
    EstadoHostVista.ERROR -> Paleta.ROJO
}

/** Texto del estado de una sesión. */
@Composable
fun textoEstadoSesion(estado: EstadoSesionVista): String = when (estado) {
    EstadoSesionVista.ABRIENDO -> stringResource(R.string.estado_sesion_abriendo)
    EstadoSesionVista.ABIERTA -> stringResource(R.string.estado_sesion_abierta)
    EstadoSesionVista.CAIDA -> stringResource(R.string.estado_sesion_caida)
}

fun colorEstadoSesion(estado: EstadoSesionVista): Color = when (estado) {
    EstadoSesionVista.ABRIENDO -> Paleta.AMBAR
    EstadoSesionVista.ABIERTA -> Paleta.VERDE
    EstadoSesionVista.CAIDA -> Paleta.ROJO
}

/** Texto del estado de un túnel. */
@Composable
fun textoEstadoTunel(estado: EstadoTunelVista): String = when (estado) {
    EstadoTunelVista.INACTIVO -> stringResource(R.string.estado_tunel_inactivo)
    EstadoTunelVista.ACTIVANDO -> stringResource(R.string.estado_tunel_activando)
    EstadoTunelVista.ACTIVO -> stringResource(R.string.estado_tunel_activo)
    EstadoTunelVista.CAIDO -> stringResource(R.string.estado_tunel_caido)
}

fun colorEstadoTunel(estado: EstadoTunelVista): Color = when (estado) {
    EstadoTunelVista.INACTIVO -> Paleta.GRIS
    EstadoTunelVista.ACTIVANDO -> Paleta.AMBAR
    EstadoTunelVista.ACTIVO -> Paleta.VERDE
    EstadoTunelVista.CAIDO -> Paleta.ROJO
}

/** Texto del estado de un equipo de la carpeta compartida (F10 §3). */
@Composable
fun textoEstadoEquipo(estado: EstadoEquipoVista): String = when (estado) {
    EstadoEquipoVista.ESTE_EQUIPO -> stringResource(R.string.estado_equipo_este)
    EstadoEquipoVista.AL_DIA -> stringResource(R.string.estado_equipo_al_dia)
    EstadoEquipoVista.ILEGIBLE -> stringResource(R.string.estado_equipo_ilegible)
    EstadoEquipoVista.DANADO -> stringResource(R.string.estado_equipo_danado)
    EstadoEquipoVista.FORMATO_NUEVO -> stringResource(R.string.estado_equipo_formato_nuevo)
    EstadoEquipoVista.QUITADO -> stringResource(R.string.estado_equipo_quitado)
}

fun colorEstadoEquipo(estado: EstadoEquipoVista): Color = when (estado) {
    EstadoEquipoVista.ESTE_EQUIPO -> Paleta.AMBAR
    EstadoEquipoVista.AL_DIA -> Paleta.VERDE
    EstadoEquipoVista.ILEGIBLE, EstadoEquipoVista.DANADO -> Paleta.ROJO
    EstadoEquipoVista.FORMATO_NUEVO -> Paleta.AMBAR
    EstadoEquipoVista.QUITADO -> Paleta.GRIS
}

/** Texto de la fase de sincronización (pantalla Sincronización y avisos). */
@Composable
fun textoFaseSincronizacion(fase: FaseSincronizacionVista): String = when (fase) {
    FaseSincronizacionVista.SIN_CONFIGURAR -> stringResource(R.string.sync_fase_sin_configurar)
    FaseSincronizacionVista.SIN_CLAVE -> stringResource(R.string.sync_fase_sin_clave)
    FaseSincronizacionVista.ACTIVA -> stringResource(R.string.sync_fase_activa)
    FaseSincronizacionVista.SINCRONIZANDO -> stringResource(R.string.sync_fase_sincronizando)
    FaseSincronizacionVista.ESPERANDO_CLAVE -> stringResource(R.string.sync_fase_esperando_clave)
    FaseSincronizacionVista.ERROR_CARPETA -> stringResource(R.string.sync_fase_error_carpeta)
}

/** Indicador ⇅ de la cabecera de Hosts: glifo, texto y color (siempre texto). */
data class IndicadorSincronizacion(
    val glifo: String,
    val texto: String,
    val color: Color,
)

@Composable
fun indicadorSincronizacion(estado: EstadoSincronizacionVista): IndicadorSincronizacion {
    val (glifo, texto, color) = when (estado.fase) {
        FaseSincronizacionVista.ACTIVA ->
            Triple("⇅", stringResource(R.string.sync_indicador_al_dia), Paleta.VERDE)
        FaseSincronizacionVista.SINCRONIZANDO ->
            Triple("⇅ …", stringResource(R.string.sync_indicador_sincronizando), Paleta.AMBAR)
        FaseSincronizacionVista.ESPERANDO_CLAVE ->
            Triple("⇅", stringResource(R.string.sync_indicador_clave_nueva), Paleta.AMBAR)
        FaseSincronizacionVista.ERROR_CARPETA ->
            Triple("⇅", stringResource(R.string.sync_indicador_error), Paleta.ROJO)
        FaseSincronizacionVista.SIN_CONFIGURAR,
        FaseSincronizacionVista.SIN_CLAVE,
        -> Triple("⇅", stringResource(R.string.sync_indicador_sin_configurar), Paleta.GRIS)
    }
    return IndicadorSincronizacion(glifo = glifo, texto = texto, color = color)
}
