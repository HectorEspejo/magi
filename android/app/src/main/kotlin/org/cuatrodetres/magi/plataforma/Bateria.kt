package org.cuatrodetres.magi.plataforma

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.core.net.toUri
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.preferencias.PreferenciasMagi

/**
 * Guía para sacar MAGI del ahorro de batería (informe §4.6, checklist «Segundo
 * plano»). Se ofrece al abrir la primera sesión (una sola vez, marca en
 * DataStore) y siempre desde Ajustes.
 *
 * Incluye los pasos de MagicOS cuando el fabricante es Honor o Huawei; en el
 * resto, pasos genéricos. El intent de exención usa
 * ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS y, si el sistema no lo acepta,
 * cae a los ajustes de ahorro de batería y a la ficha de la app.
 */
class GuiaBateria(
    private val contexto: Context,
    private val preferencias: PreferenciasMagi,
) {

    data class Estado(
        /** true = la app ya está exenta del ahorro de batería. */
        val optimizacionIgnorada: Boolean,
        val fabricante: String,
        /** true = hay pasos específicos de MagicOS/EMUI. */
        val conPasosMagicOs: Boolean,
    )

    fun estado(): Estado {
        val gestor = contexto.getSystemService(PowerManager::class.java)
        val ignorada = gestor?.isIgnoringBatteryOptimizations(contexto.packageName) == true
        val fabricante = Build.MANUFACTURER ?: ""
        return Estado(
            optimizacionIgnorada = ignorada,
            fabricante = fabricante,
            conPasosMagicOs = ReglasBateria.esMagicOs(fabricante),
        )
    }

    /** Si merece la pena ofrecer la guía: la app no está exenta. */
    fun necesitaGuia(): Boolean = !estado().optimizacionIgnorada

    /** Si hay que ofrecerla automáticamente (primera sesión y nunca vista). */
    suspend fun debeOfrecerse(): Boolean =
        necesitaGuia() && !preferencias.primeraLectura().guiaBateriaVista

    /** Marca la guía como vista: no se vuelve a ofrecer sola. */
    suspend fun marcarOfrecida() {
        preferencias.marcarGuiaBateriaVista()
    }

    /** Pasos a pintar en Ajustes > Batería, en castellano. */
    fun pasos(): List<String> {
        val recurso = if (estado().conPasosMagicOs) {
            R.array.plataforma_bateria_pasos_magic
        } else {
            R.array.plataforma_bateria_pasos_genericos
        }
        return contexto.resources.getStringArray(recurso).toList()
    }

    /** Intent de la petición directa de exención del ahorro de batería. */
    fun crearIntentPedirExencion(): Intent =
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(("package:" + contexto.packageName).toUri())

    /** Ajustes de ahorro de batería del sistema (respaldo de la petición directa). */
    fun crearIntentAjustesBateria(): Intent =
        Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)

    /** Ficha de la app en Ajustes (último recurso si no hay ajuste de batería). */
    fun crearIntentFichaApp(): Intent =
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(("package:" + contexto.packageName).toUri())

    /**
     * Lanza los ajustes de exención desde donde no hay Activity a mano (por
     * ejemplo, como callback `alAbrirBateria` de la interfaz). Si el sistema
     * no ofrece la petición directa, cae a los ajustes de batería y a la ficha
     * de la app.
     */
    fun abrirAjustes() {
        try {
            contexto.startActivity(crearIntentPedirExencion().conNuevaTarea())
            return
        } catch (_: ActivityNotFoundException) {
            // El sistema no acepta la petición directa: se prueban los ajustes.
        }
        try {
            contexto.startActivity(crearIntentAjustesBateria().conNuevaTarea())
        } catch (_: ActivityNotFoundException) {
            contexto.startActivity(crearIntentFichaApp().conNuevaTarea())
        }
    }

    private fun Intent.conNuevaTarea(): Intent = apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
}
