package org.cuatrodetres.magi.ui.ajustes

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.CabeceraVolver
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.BACKUP_HORAS_MAX
import org.cuatrodetres.magi.ui.modelos.BACKUP_HORAS_MIN
import org.cuatrodetres.magi.ui.modelos.SALUD_MAX_MIN_MAX
import org.cuatrodetres.magi.ui.modelos.SALUD_MAX_MIN_MIN

/**
 * Ajustes > Deliberación (informe §5.4): horas del backup de BALTHASAR-2 y
 * minutos del sondeo de salud de MELCHIOR-1. CASPER-3 no se puede en el móvil.
 *
 * Son los mismos valores que el escritorio tiene en config.toml. Un campo fuera
 * de rango no deja guardar y dice el rango; al guardar se acota igualmente.
 */
@Composable
fun PantallaAjustesDeliberacion(
    ajustes: AjustesMovilVista,
    avisos: List<String>,
    alGuardar: (AjustesMovilVista) -> Unit,
    alVolver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var backupTexto by remember(ajustes) {
        mutableStateOf(ajustes.deliberacionBackupHoras.toString())
    }
    var saludTexto by remember(ajustes) {
        mutableStateOf(ajustes.deliberacionSaludMaxMin.toString())
    }
    val backup = backupTexto.trim().toIntOrNull()
    val salud = saludTexto.trim().toIntOrNull()
    val errorBackup = errorDeEntero(backup, BACKUP_HORAS_MIN, BACKUP_HORAS_MAX)
    val errorSalud = errorDeEntero(salud, SALUD_MAX_MIN_MIN, SALUD_MAX_MIN_MAX)
    val hayCambios = backup != ajustes.deliberacionBackupHoras ||
        salud != ajustes.deliberacionSaludMaxMin
    Column(modifier = modifier.fillMaxSize()) {
        CabeceraVolver(
            titulo = stringResource(R.string.ajustes_deliberacion),
            alVolver = alVolver,
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            AvisosDeAjustes(avisos)
            CampoEnteroAjuste(
                titulo = stringResource(R.string.ajustes_deliberacion_backup),
                texto = backupTexto,
                ayuda = stringResource(R.string.ajustes_deliberacion_backup_ayuda),
                alCambiar = { backupTexto = it },
                error = errorBackup?.let { error ->
                    stringResource(error, BACKUP_HORAS_MIN, BACKUP_HORAS_MAX)
                },
            )
            CampoEnteroAjuste(
                titulo = stringResource(R.string.ajustes_deliberacion_salud),
                texto = saludTexto,
                ayuda = stringResource(R.string.ajustes_deliberacion_salud_ayuda),
                alCambiar = { saludTexto = it },
                error = errorSalud?.let { error ->
                    stringResource(error, SALUD_MAX_MIN_MIN, SALUD_MAX_MIN_MAX)
                },
            )
            TextoAyuda(stringResource(R.string.ajustes_deliberacion_nota))
            PieGuardarAjustes(
                ayuda = stringResource(R.string.ajustes_deliberacion_nota_guardar),
                hayCambios = hayCambios && errorBackup == null && errorSalud == null,
                alGuardar = {
                    val backupValido = backup ?: return@PieGuardarAjustes
                    val saludValida = salud ?: return@PieGuardarAjustes
                    alGuardar(
                        ajustes.copy(
                            deliberacionBackupHoras = backupValido,
                            deliberacionSaludMaxMin = saludValida,
                        ),
                    )
                },
            )
        }
    }
}

/**
 * Devuelve el recurso del error de un entero: 0 = vacío o no numérico, 1 =
 * fuera de rango; null si el valor es válido.
 */
private fun errorDeEntero(valor: Int?, minimo: Int, maximo: Int): Int? = when {
    valor == null -> R.string.ajustes_campo_no_valido
    valor < minimo || valor > maximo -> R.string.ajustes_campo_fuera_de_rango
    else -> null
}
