package org.cuatrodetres.magi.navegacion

import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.ui.tema.Paleta

/** Barra inferior para pantalla compacta (mockup §6.1). */
@Composable
fun BarraInferior(destinoActual: Destino, alElegir: (Destino) -> Unit) {
    NavigationBar(
        containerColor = Paleta.FONDO,
        contentColor = Paleta.AMBAR,
    ) {
        Destino.entries.forEach { destino ->
            NavigationBarItem(
                selected = destino == destinoActual,
                onClick = { alElegir(destino) },
                icon = { Text(destino.glifo) },
                label = { Text(stringResource(destino.etiqueta)) },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = Paleta.FONDO,
                    selectedTextColor = Paleta.AMBAR,
                    indicatorColor = Paleta.AMBAR,
                    unselectedIconColor = Paleta.GRIS,
                    unselectedTextColor = Paleta.GRIS,
                ),
            )
        }
    }
}

/** Riel de navegación para plegable abierto o pantalla ancha (mockup §6.4). */
@Composable
fun RielNavegacion(destinoActual: Destino, alElegir: (Destino) -> Unit) {
    NavigationRail(
        containerColor = Paleta.FONDO,
        contentColor = Paleta.AMBAR,
    ) {
        Destino.entries.forEach { destino ->
            NavigationRailItem(
                selected = destino == destinoActual,
                onClick = { alElegir(destino) },
                icon = { Text(destino.glifo) },
                label = { Text(stringResource(destino.etiqueta)) },
                colors = NavigationRailItemDefaults.colors(
                    selectedIconColor = Paleta.FONDO,
                    selectedTextColor = Paleta.AMBAR,
                    indicatorColor = Paleta.AMBAR,
                    unselectedIconColor = Paleta.GRIS,
                    unselectedTextColor = Paleta.GRIS,
                ),
            )
        }
    }
}
