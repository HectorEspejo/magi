package org.cuatrodetres.magi.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * Compatibilidad mientras MainActivity se cablea al punto de entrada real.
 *
 * La app ya no tiene una pantalla de esqueleto: esto pinta [AppMagi] con la
 * fuente vacía y sin bloqueo. MainActivity debería llamar a [AppMagi] con la
 * fuente del núcleo y el control de bloqueo de la plataforma.
 */
@Composable
fun PantallaEsqueleto(modifier: Modifier = Modifier) {
    AppMagi()
}
