package org.cuatrodetres.magi.ui.hosts

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BarraAviso
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.componentes.ChipEtiqueta
import org.cuatrodetres.magi.ui.componentes.FilaDato
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.Separador
import org.cuatrodetres.magi.ui.componentes.TipoBarra
import org.cuatrodetres.magi.ui.componentes.colorEstadoHost
import org.cuatrodetres.magi.ui.componentes.textoEstadoHost
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.glifoEstadoHost
import org.cuatrodetres.magi.ui.modelos.keepaliveEfectivo
import org.cuatrodetres.magi.ui.modelos.keepaliveEsDelMovil
import org.cuatrodetres.magi.ui.tema.Paleta
import org.cuatrodetres.magi.ui.tuneles.FilaTunel

/**
 * Ficha de host de solo lectura (informe §6.2): todos los campos del inventario,
 * huellas conocidas, si hay contraseña guardada, túneles del host y las
 * acciones Conectar · Otra sesión · Instalar mi clave · Olvidar contraseña.
 */
@Composable
fun PantallaFicha(
    ficha: FichaHostVista,
    estadoHost: EstadoHostVista,
    sesionesAbiertas: Int,
    tuneles: List<TunelVista>,
    conflicto: ConflictoHuellaVista?,
    alVolver: () -> Unit,
    alConectar: () -> Unit,
    alOtraSesion: () -> Unit,
    alInstalarClave: () -> Unit,
    alOlvidarContrasena: () -> Unit,
    alResolverConflicto: () -> Unit,
    alAlternarTunel: (TunelVista) -> Unit,
    alRelanzarTunel: (TunelVista) -> Unit,
    alTunelPuertoLibre: (TunelVista) -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alDescartarTunel: (TunelVista) -> Unit,
    modifier: Modifier = Modifier,
) {
    val estadoTexto = textoEstadoHost(estadoHost, sesionesAbiertas)
    val escritorio = ficha.tipo.esEscritorio
    Column(modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        CabeceraVolver(
            titulo = ficha.nombre,
            alVolver = alVolver,
            glifoEstado = glifoEstadoHost(estadoHost, sesionesAbiertas),
            textoEstado = estadoTexto,
            colorEstado = colorEstadoHost(estadoHost),
        )

        if (conflicto != null) {
            BarraAviso(
                texto = stringResource(R.string.ficha_conflicto_huella, conflicto.hostNombre),
                tipo = TipoBarra.AVISO,
                textoAccion = stringResource(R.string.accion_resolver),
                alAccion = alResolverConflicto,
            )
        }

        FilaDato(stringResource(R.string.ficha_tipo), ficha.tipo.etiqueta)
        if (escritorio) {
            BarraAviso(
                texto = stringResource(R.string.ficha_escritorio_aviso),
                tipo = TipoBarra.AVISO,
            )
        }
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
            stringResource(R.string.ficha_escritorio),
            ficha.identidadEscritorio ?: stringResource(R.string.ficha_ninguna),
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
            FilaDato(
                stringResource(R.string.ficha_huella),
                stringResource(R.string.ficha_sin_huellas),
            )
        } else {
            ficha.huellas.forEachIndexed { indice, huella ->
                FilaDato(
                    etiqueta = if (indice == 0) stringResource(R.string.ficha_huella) else "",
                    valor = huella.tipo + " " + huella.sha256,
                )
            }
        }

        // Un escritorio no tiene túneles ni acciones de SSH: el móvil solo lo
        // enseña y dice que se abre en el ordenador.
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
                    FilaTunel(
                        tunel = tunel,
                        alAlternar = alAlternarTunel,
                        alRelanzar = alRelanzarTunel,
                        alPuertoLibre = alTunelPuertoLibre,
                        alAbrirNavegador = alAbrirNavegador,
                        alDescartar = alDescartarTunel,
                    )
                }
            }
        }

        if (!escritorio) {
            Separador()
            Column(
                modifier = Modifier.fillMaxWidth().padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BotonCabina(
                    texto = stringResource(R.string.accion_conectar),
                    alPulsar = alConectar,
                    primario = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                BotonCabina(
                    texto = stringResource(R.string.accion_otra_sesion),
                    alPulsar = alOtraSesion,
                    modifier = Modifier.fillMaxWidth(),
                )
                BotonCabina(
                    texto = stringResource(R.string.accion_instalar_clave),
                    alPulsar = alInstalarClave,
                    modifier = Modifier.fillMaxWidth(),
                )
                if (ficha.tieneContrasena) {
                    BotonCabina(
                        texto = stringResource(R.string.accion_olvidar_contrasena),
                        alPulsar = alOlvidarContrasena,
                        destructivo = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}
