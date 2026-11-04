package org.cuatrodetres.magi.preferencias

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

/**
 * Ajustes de MAGI en el móvil (informe §5.4). Nada de esto es secreto: la
 * clave de sincronización y las contraseñas viven cifradas en el llavero.
 */
data class AjustesMagi(
    /** URI SAF del árbol elegido (solo lectura). */
    val carpetaUri: String? = null,
    /** Tamaño de letra del terminal en sp (8-24; por defecto 13). */
    val tamanoLetra: Int = ReglasPreferencias.TAMANO_LETRA_DEFECTO,
    /** Instante (epoch en segundos) hasta el que la clave anterior solo lee. */
    val claveAnteriorHasta: Long? = null,
    /**
     * Sesiones que quedaron abiertas la última vez. Si el proceso murió sin
     * cerrarlas, al volver la app avisa de que Android cerró MAGI.
     */
    val sesionesAbiertas: Int = 0,
    /** Ya se ofreció la guía de batería: no se insiste. */
    val guiaBateriaVista: Boolean = false,
)

/** Reglas puras de los ajustes: saneado y conversión desde DataStore. */
object ReglasPreferencias {

    const val TAMANO_LETRA_MIN: Int = 8
    const val TAMANO_LETRA_MAX: Int = 24
    const val TAMANO_LETRA_DEFECTO: Int = 13
    const val SESIONES_ABIERTAS_MAX: Int = 1_000

    fun sanearTamanoLetra(tamano: Int): Int = tamano.coerceIn(TAMANO_LETRA_MIN, TAMANO_LETRA_MAX)

    fun sanearSesionesAbiertas(cantidad: Int): Int =
        cantidad.coerceIn(0, SESIONES_ABIERTAS_MAX)

    /** Convierte lo que hay en DataStore en ajustes saneados (nunca lanza). */
    fun aAjustes(preferencias: Preferences): AjustesMagi {
        val tamano = preferencias[CLAVE_TAMANO_LETRA] ?: TAMANO_LETRA_DEFECTO
        val sesiones = preferencias[CLAVE_SESIONES_ABIERTAS] ?: 0
        val uri = preferencias[CLAVE_CARPETA_URI]?.takeIf { it.isNotBlank() }
        val anterior = preferencias[CLAVE_CLAVE_ANTERIOR_HASTA]?.takeIf { it > 0 }
        return AjustesMagi(
            carpetaUri = uri,
            tamanoLetra = sanearTamanoLetra(tamano),
            claveAnteriorHasta = anterior,
            sesionesAbiertas = sanearSesionesAbiertas(sesiones),
            guiaBateriaVista = preferencias[CLAVE_GUIA_BATERIA_VISTA] ?: false,
        )
    }

    val CLAVE_CARPETA_URI = stringPreferencesKey("carpeta_uri")
    val CLAVE_TAMANO_LETRA = intPreferencesKey("tamano_letra")
    val CLAVE_CLAVE_ANTERIOR_HASTA = longPreferencesKey("clave_anterior_hasta")
    val CLAVE_SESIONES_ABIERTAS = intPreferencesKey("sesiones_abiertas")
    val CLAVE_GUIA_BATERIA_VISTA = booleanPreferencesKey("guia_bateria_vista")

    /** La clave del móvil se recreó al corregir el desbloqueo: hay que avisar. */
    val CLAVE_AVISO_CLAVE_RECREADA = booleanPreferencesKey("aviso_clave_recreada")
}

/** Un solo almacén por proceso: DataStore exige una instancia única por fichero. */
private val Context.almacenAjustes: DataStore<Preferences> by preferencesDataStore(name = "magi_ajustes")

/** Ajustes de MAGI en DataStore, con setters saneados. */
class PreferenciasMagi(private val almacen: DataStore<Preferences>) {

    companion object {
        fun de(contexto: Context): PreferenciasMagi =
            PreferenciasMagi(contexto.applicationContext.almacenAjustes)
    }

    val ajustes: Flow<AjustesMagi> = almacen.data.map(ReglasPreferencias::aAjustes)

    /** Primera lectura, para quien no puede observar el flujo (arranque). */
    suspend fun primeraLectura(): AjustesMagi = ajustes.first()

    suspend fun guardarCarpetaUri(uri: String?) {
        almacen.edit { preferencias ->
            if (uri.isNullOrBlank()) {
                preferencias.remove(ReglasPreferencias.CLAVE_CARPETA_URI)
            } else {
                preferencias[ReglasPreferencias.CLAVE_CARPETA_URI] = uri
            }
        }
    }

    suspend fun guardarTamanoLetra(tamano: Int) {
        almacen.edit {
            it[ReglasPreferencias.CLAVE_TAMANO_LETRA] = ReglasPreferencias.sanearTamanoLetra(tamano)
        }
    }

    suspend fun guardarClaveAnteriorHasta(segundos: Long?) {
        almacen.edit { preferencias ->
            if (segundos == null || segundos <= 0L) {
                preferencias.remove(ReglasPreferencias.CLAVE_CLAVE_ANTERIOR_HASTA)
            } else {
                preferencias[ReglasPreferencias.CLAVE_CLAVE_ANTERIOR_HASTA] = segundos
            }
        }
    }

    suspend fun guardarSesionesAbiertas(cantidad: Int) {
        almacen.edit {
            it[ReglasPreferencias.CLAVE_SESIONES_ABIERTAS] =
                ReglasPreferencias.sanearSesionesAbiertas(cantidad)
        }
    }

    suspend fun marcarGuiaBateriaVista(vista: Boolean = true) {
        almacen.edit { it[ReglasPreferencias.CLAVE_GUIA_BATERIA_VISTA] = vista }
    }

    /**
     * Lee cuántas sesiones quedaron abiertas la última vez y las reinicia: es
     * la señal de «Android cerró MAGI con N sesiones abiertas» (informe §4.6).
     * La lectura y el borrado van en la misma edición de DataStore.
     */
    suspend fun consumirSesionesAbiertas(): Int {
        var anteriores = 0
        almacen.edit { preferencias ->
            anteriores = preferencias[ReglasPreferencias.CLAVE_SESIONES_ABIERTAS] ?: 0
            preferencias[ReglasPreferencias.CLAVE_SESIONES_ABIERTAS] = 0
        }
        return ReglasPreferencias.sanearSesionesAbiertas(anteriores)
    }

    /** Marca que hay que avisar de que la clave del móvil se recreó. */
    suspend fun marcarAvisoClaveRecreada() {
        almacen.edit { it[ReglasPreferencias.CLAVE_AVISO_CLAVE_RECREADA] = true }
    }

    /** Lee y borra el aviso de clave recreada (se muestra una sola vez). */
    suspend fun consumirAvisoClaveRecreada(): Boolean {
        var aviso = false
        almacen.edit { preferencias ->
            aviso = preferencias[ReglasPreferencias.CLAVE_AVISO_CLAVE_RECREADA] ?: false
            preferencias[ReglasPreferencias.CLAVE_AVISO_CLAVE_RECREADA] = false
        }
        return aviso
    }
}
