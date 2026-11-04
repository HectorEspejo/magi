package org.cuatrodetres.magi.ui.dialogos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.plataforma.RecintoSeguro
import org.cuatrodetres.magi.ui.componentes.CampoTextoMagi
import org.cuatrodetres.magi.ui.componentes.Chiptexto
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.DialogoVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.TipoConfirmacion
import org.cuatrodetres.magi.ui.tema.Paleta

/**
 * Diálogos de la app (informe §6.6 y §7.1): huella nueva, huella cambiada
 * (escribir el nombre), contraseña enmascarada con intentos, confirmaciones
 * destructivas y conflicto de huella de otro equipo.
 */
@Composable
fun DialogosMagi(
    dialogo: DialogoVista?,
    alCancelar: (DialogoVista) -> Unit,
    alResponder: (DialogoVista.DeNucleo, RespuestaDialogoVista) -> Unit,
    alConfirmar: (DialogoVista.Confirmacion) -> Unit,
    alResolverConflicto: (ConflictoHuellaVista, Boolean) -> Unit,
    alAbrirGuiaBateria: () -> Unit = {},
    alDescartarGuia: () -> Unit = {},
) {
    when (dialogo) {
        null -> Unit
        is DialogoVista.DeNucleo -> when (val peticion = dialogo.peticion) {
            is PeticionDialogoVista.HuellaNueva -> DialogoHuellaNueva(
                dialogo = dialogo,
                peticion = peticion,
                alCancelar = alCancelar,
                alResponder = alResponder,
            )
            is PeticionDialogoVista.HuellaCambiada -> DialogoHuellaCambiada(
                dialogo = dialogo,
                peticion = peticion,
                alCancelar = alCancelar,
                alResponder = alResponder,
            )
            is PeticionDialogoVista.Contrasena -> DialogoContrasena(
                dialogo = dialogo,
                peticion = peticion,
                alCancelar = alCancelar,
                alResponder = alResponder,
            )
        }
        is DialogoVista.Confirmacion -> DialogoConfirmacion(
            dialogo = dialogo,
            alCancelar = alCancelar,
            alConfirmar = alConfirmar,
        )
        is DialogoVista.ConflictoHuella -> DialogoConflictoHuella(
            dialogo = dialogo,
            alResolver = alResolverConflicto,
        )
        DialogoVista.GuiaBateria -> DialogoGuiaBateria(
            alAbrirGuia = alAbrirGuiaBateria,
            alAhoraNo = alDescartarGuia,
        )
    }
}

/** Guía de batería ofrecida al abrir la primera sesión (§4.6). */
@Composable
private fun DialogoGuiaBateria(alAbrirGuia: () -> Unit, alAhoraNo: () -> Unit) {
    AlertDialog(
        onDismissRequest = alAhoraNo,
        title = { Text(stringResource(R.string.avisos_guia_bateria_titulo)) },
        text = { Text(stringResource(R.string.avisos_guia_bateria_texto)) },
        confirmButton = {
            TextButton(onClick = alAbrirGuia) {
                Text(stringResource(R.string.avisos_abrir_guia))
            }
        },
        dismissButton = {
            TextButton(onClick = alAhoraNo) {
                Text(stringResource(R.string.avisos_ahora_no))
            }
        },
    )
}

@Composable
private fun DialogoHuellaNueva(
    dialogo: DialogoVista.DeNucleo,
    peticion: PeticionDialogoVista.HuellaNueva,
    alCancelar: (DialogoVista) -> Unit,
    alResponder: (DialogoVista.DeNucleo, RespuestaDialogoVista) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { alCancelar(dialogo) },
        title = { Text(stringResource(R.string.huella_nueva_titulo, peticion.hostNombre)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(peticion.tipo + " " + peticion.sha256, color = Paleta.AMBAR)
                Text(stringResource(R.string.huella_nueva_texto))
            }
        },
        confirmButton = {
            TextButton(onClick = { alResponder(dialogo, RespuestaDialogoVista.AceptarHuella) }) {
                Text(stringResource(R.string.accion_aceptar))
            }
        },
        dismissButton = {
            TextButton(onClick = { alCancelar(dialogo) }) {
                Text(stringResource(R.string.accion_cancelar))
            }
        },
    )
}

@Composable
private fun DialogoHuellaCambiada(
    dialogo: DialogoVista.DeNucleo,
    peticion: PeticionDialogoVista.HuellaCambiada,
    alCancelar: (DialogoVista) -> Unit,
    alResponder: (DialogoVista.DeNucleo, RespuestaDialogoVista) -> Unit,
) {
    var escrito by remember(dialogo) { mutableStateOf("") }
    val correcto = escrito.trim() == peticion.nombreAEscribir
    AlertDialog(
        onDismissRequest = { alCancelar(dialogo) },
        title = { Text(stringResource(R.string.huella_cambiada_titulo, peticion.hostNombre), color = Paleta.ROJO) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.huella_cambiada_texto))
                Chiptexto(stringResource(R.string.huella_anterior, peticion.anterior))
                Chiptexto(stringResource(R.string.huella_nueva, peticion.nueva))
                Text(stringResource(R.string.huella_cambiada_aviso))
                Text(stringResource(R.string.huella_cambiada_escribe, peticion.nombreAEscribir))
                CampoTextoMagi(
                    valor = escrito,
                    alCambiar = { escrito = it },
                    etiqueta = peticion.nombreAEscribir,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { alResponder(dialogo, RespuestaDialogoVista.SustituirHuella(escrito.trim())) },
                enabled = correcto,
            ) {
                Text(stringResource(R.string.huella_cambiada_sustituir), color = Paleta.ROJO)
            }
        },
        dismissButton = {
            TextButton(onClick = { alCancelar(dialogo) }) {
                Text(stringResource(R.string.huella_cambiada_cancelar))
            }
        },
    )
}

@Composable
private fun DialogoContrasena(
    dialogo: DialogoVista.DeNucleo,
    peticion: PeticionDialogoVista.Contrasena,
    alCancelar: (DialogoVista) -> Unit,
    alResponder: (DialogoVista.DeNucleo, RespuestaDialogoVista) -> Unit,
) {
    var texto by remember(dialogo) { mutableStateOf("") }
    var recordar by remember(dialogo) { mutableStateOf(false) }
    // La contraseña es un secreto: sin capturas ni vista previa en recientes.
    RecintoSeguro {
        DialogoContrasenaContenido(
            dialogo = dialogo,
            peticion = peticion,
            texto = texto,
            alCambiarTexto = { texto = it },
            recordar = recordar,
            alCambiarRecordar = { recordar = it },
            alCancelar = alCancelar,
            alResponder = alResponder,
        )
    }
}

@Composable
private fun DialogoContrasenaContenido(
    dialogo: DialogoVista.DeNucleo,
    peticion: PeticionDialogoVista.Contrasena,
    texto: String,
    alCambiarTexto: (String) -> Unit,
    recordar: Boolean,
    alCambiarRecordar: (Boolean) -> Unit,
    alCancelar: (DialogoVista) -> Unit,
    alResponder: (DialogoVista.DeNucleo, RespuestaDialogoVista) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { alCancelar(dialogo) },
        title = { Text(stringResource(R.string.contrasena_titulo, peticion.hostNombre)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(peticion.usuario + "@" + peticion.direccion, color = Paleta.GRIS)
                Text(stringResource(R.string.contrasena_intentos, peticion.intentosRestantes))
                CampoTextoMagi(
                    valor = texto,
                    alCambiar = alCambiarTexto,
                    etiqueta = stringResource(R.string.contrasena_etiqueta),
                    contrasena = true,
                )
                if (peticion.recordarOfrecido) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(checked = recordar, onCheckedChange = alCambiarRecordar)
                        Text(stringResource(R.string.contrasena_recordar))
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    alResponder(
                        dialogo,
                        RespuestaDialogoVista.Contrasena(texto.toByteArray(Charsets.UTF_8), recordar),
                    )
                },
                enabled = texto.isNotEmpty(),
            ) {
                Text(stringResource(R.string.accion_aceptar))
            }
        },
        dismissButton = {
            TextButton(onClick = { alCancelar(dialogo) }) {
                Text(stringResource(R.string.accion_cancelar))
            }
        },
    )
}

@Composable
private fun DialogoConfirmacion(
    dialogo: DialogoVista.Confirmacion,
    alCancelar: (DialogoVista) -> Unit,
    alConfirmar: (DialogoVista.Confirmacion) -> Unit,
) {
    val titulo = when (dialogo.tipo) {
        TipoConfirmacion.CERRAR_SESION -> stringResource(R.string.confirmar_cerrar_sesion_titulo)
        TipoConfirmacion.DEJAR_DE_SINCRONIZAR -> stringResource(R.string.confirmar_dejar_sync_titulo)
        TipoConfirmacion.OLVIDAR_CONTRASENA -> stringResource(R.string.confirmar_olvidar_titulo)
        TipoConfirmacion.INSTALAR_CLAVE -> stringResource(R.string.confirmar_instalar_titulo)
        TipoConfirmacion.ACEPTAR_HUELLA_DE_EQUIPO -> stringResource(R.string.confirmar_aceptar_huella_titulo)
        TipoConfirmacion.BORRAR_DATOS -> stringResource(R.string.ajustes_borrar_datos)
    }
    val detalle = dialogo.detalle.orEmpty()
    val texto = when (dialogo.tipo) {
        TipoConfirmacion.CERRAR_SESION -> stringResource(R.string.confirmar_cerrar_sesion_texto, detalle)
        TipoConfirmacion.DEJAR_DE_SINCRONIZAR -> stringResource(R.string.confirmar_dejar_sync_texto)
        TipoConfirmacion.OLVIDAR_CONTRASENA -> stringResource(R.string.confirmar_olvidar_texto, detalle)
        TipoConfirmacion.INSTALAR_CLAVE -> stringResource(R.string.confirmar_instalar_texto, detalle)
        TipoConfirmacion.ACEPTAR_HUELLA_DE_EQUIPO -> stringResource(R.string.confirmar_aceptar_huella_texto, detalle)
        TipoConfirmacion.BORRAR_DATOS -> stringResource(R.string.borrar_aviso)
    }
    val destructivo = dialogo.tipo != TipoConfirmacion.INSTALAR_CLAVE
    AlertDialog(
        onDismissRequest = { alCancelar(dialogo) },
        title = { Text(titulo, color = if (destructivo) Paleta.ROJO else Paleta.TEXTO) },
        text = { Text(texto) },
        confirmButton = {
            TextButton(onClick = { alConfirmar(dialogo) }) {
                Text(
                    text = stringResource(R.string.accion_confirmar),
                    color = if (destructivo) Paleta.ROJO else Paleta.AMBAR,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = { alCancelar(dialogo) }) {
                Text(stringResource(R.string.accion_cancelar))
            }
        },
    )
}

@Composable
private fun DialogoConflictoHuella(
    dialogo: DialogoVista.ConflictoHuella,
    alResolver: (ConflictoHuellaVista, Boolean) -> Unit,
) {
    val conflicto = dialogo.conflicto
    AlertDialog(
        onDismissRequest = { },
        title = { Text(stringResource(R.string.conflicto_titulo, conflicto.hostNombre), color = Paleta.AMBAR) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.conflicto_texto))
                Chiptexto(stringResource(R.string.conflicto_mia, conflicto.huellaMia.ifEmpty { "—" }))
                Chiptexto(stringResource(R.string.conflicto_otra, conflicto.huellaOtra))
                if (conflicto.equipoOrigen != null) {
                    Text(stringResource(R.string.conflicto_equipo, conflicto.equipoOrigen), color = Paleta.GRIS)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { alResolver(conflicto, true) }) {
                Text(stringResource(R.string.conflicto_aceptar), color = Paleta.ROJO)
            }
        },
        dismissButton = {
            TextButton(onClick = { alResolver(conflicto, false) }) {
                Text(stringResource(R.string.conflicto_mantener))
            }
        },
    )
}
