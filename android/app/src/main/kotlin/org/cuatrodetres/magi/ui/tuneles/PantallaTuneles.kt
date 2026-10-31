package org.cuatrodetres.magi.ui.tuneles

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraMagi
import org.cuatrodetres.magi.ui.componentes.EncabezadoSeccion
import org.cuatrodetres.magi.ui.componentes.MensajeVacio
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.agruparTuneles

/**
 * Pantalla Túneles (checklist de túneles): túneles del inventario agrupados por
 * host, con interruptor, estado glifo+texto, contadores, relanzar, puerto libre
 * y abrir en el navegador.
 */
@Composable
fun PantallaTuneles(
    tuneles: List<TunelVista>,
    alAlternar: (TunelVista) -> Unit,
    alRelanzar: (TunelVista) -> Unit,
    alPuertoLibre: (TunelVista) -> Unit,
    alAbrirNavegador: (TunelVista) -> Unit,
    alIrAHosts: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val activos = tuneles.count {
        it.estado == EstadoTunelVista.ACTIVO || it.estado == EstadoTunelVista.ACTIVANDO
    }
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraMagi(titulo = stringResource(R.string.tuneles_titulo, activos))
        if (tuneles.isEmpty()) {
            MensajeVacio(
                titulo = stringResource(R.string.tuneles_vacio_titulo),
                detalle = stringResource(R.string.tuneles_vacio_detalle),
                glifo = "⇄",
                textoBoton = stringResource(R.string.accion_ir_a_hosts),
                alPulsar = alIrAHosts,
                modifier = Modifier.weight(1f),
            )
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                agruparTuneles(tuneles).forEach { bloque ->
                    item(key = "host:" + bloque.hostUuid) {
                        EncabezadoSeccion(bloque.hostNombre.uppercase())
                    }
                    items(
                        count = bloque.tuneles.size,
                        key = { indice -> "tunel:" + bloque.tuneles[indice].uuid },
                    ) { indice ->
                        FilaTunel(
                            tunel = bloque.tuneles[indice],
                            alAlternar = alAlternar,
                            alRelanzar = alRelanzar,
                            alPuertoLibre = alPuertoLibre,
                            alAbrirNavegador = alAbrirNavegador,
                        )
                    }
                }
            }
        }
    }
}
