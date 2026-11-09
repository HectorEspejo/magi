package org.cuatrodetres.magi.preferencias

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.doublePreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.acotarAjustes

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
     * Sesiones, acciones y ejecuciones que quedaron en curso la última vez. Si
     * el proceso murió sin cerrarlas, al volver la app avisa de que Android
     * cerró MAGI (informe §7.12).
     */
    val sesionesAbiertas: Int = 0,
    /** Acciones de servicio en curso al morir el proceso (informe §7.12). */
    val accionesAbiertas: Int = 0,
    /** Ejecuciones de snippets en curso al morir el proceso (informe §7.12). */
    val ejecucionesAbiertas: Int = 0,
    /** Ya se ofreció la guía de batería: no se insiste. */
    val guiaBateriaVista: Boolean = false,
    /**
     * Ajustes de Flota, Servicios y Deliberación (informe §5.4). En el
     * escritorio viven en config.toml; aquí, en DataStore, y llegan al núcleo
     * con aplicar_ajustes.
     */
    val ajustesMovil: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
)

/** Reglas puras de los ajustes: saneado y conversión desde DataStore. */
object ReglasPreferencias {

    const val TAMANO_LETRA_MIN: Int = 8
    const val TAMANO_LETRA_MAX: Int = 24
    const val TAMANO_LETRA_DEFECTO: Int = 13
    const val SESIONES_ABIERTAS_MAX: Int = 1_000

    /** Tope del trabajo en curso que se anota para el aviso de Android (§7.12). */
    const val TRABAJO_ABIERTO_MAX: Int = 1_000

    fun sanearTamanoLetra(tamano: Int): Int = tamano.coerceIn(TAMANO_LETRA_MIN, TAMANO_LETRA_MAX)

    fun sanearSesionesAbiertas(cantidad: Int): Int =
        cantidad.coerceIn(0, SESIONES_ABIERTAS_MAX)

    /** Nunca se anota un recuento de trabajo en curso negativo ni desmedido. */
    fun sanearTrabajoAbierto(cantidad: Int): Int =
        cantidad.coerceIn(0, TRABAJO_ABIERTO_MAX)

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
            accionesAbiertas = sanearTrabajoAbierto(
                preferencias[CLAVE_ACCIONES_ABIERTAS] ?: 0,
            ),
            ejecucionesAbiertas = sanearTrabajoAbierto(
                preferencias[CLAVE_EJECUCIONES_ABIERTAS] ?: 0,
            ),
            guiaBateriaVista = preferencias[CLAVE_GUIA_BATERIA_VISTA] ?: false,
            ajustesMovil = aAjustesMovil(preferencias),
        )
    }

    /**
     * Ajustes de Flota, Servicios y Deliberación leídos de DataStore y
     * acotados (un valor fuera de rango no impide arrancar: se acota).
     */
    fun aAjustesMovil(preferencias: Preferences): AjustesMovilVista {
        val motores = preferencias[CLAVE_SERVICIOS_CONTENEDORES]
            ?.split(',')
            ?.map { it.trim() }
            ?.filter { it.isNotEmpty() }
            ?: AjustesMovilVista.POR_DEFECTO.serviciosContenedores
        val leidos = AjustesMovilVista(
            flotaRefrescoSeg = preferencias[CLAVE_FLOTA_REFRESCO_SEG]
                ?: AjustesMovilVista.POR_DEFECTO.flotaRefrescoSeg,
            umbralCargaPorNucleo = preferencias[CLAVE_UMBRAL_CARGA_POR_NUCLEO]
                ?: AjustesMovilVista.POR_DEFECTO.umbralCargaPorNucleo,
            umbralMemoriaPct = preferencias[CLAVE_UMBRAL_MEMORIA_PCT]
                ?: AjustesMovilVista.POR_DEFECTO.umbralMemoriaPct,
            umbralDiscoPct = preferencias[CLAVE_UMBRAL_DISCO_PCT]
                ?: AjustesMovilVista.POR_DEFECTO.umbralDiscoPct,
            serviciosRefrescoSeg = preferencias[CLAVE_SERVICIOS_REFRESCO_SEG]
                ?: AjustesMovilVista.POR_DEFECTO.serviciosRefrescoSeg,
            serviciosEsperaSeg = preferencias[CLAVE_SERVICIOS_ESPERA_SEG]
                ?: AjustesMovilVista.POR_DEFECTO.serviciosEsperaSeg,
            serviciosContenedores = motores,
            sudoMemoriaMin = preferencias[CLAVE_SUDO_MEMORIA_MIN]
                ?: AjustesMovilVista.POR_DEFECTO.sudoMemoriaMin,
            deliberacionBackupHoras = preferencias[CLAVE_DELIBERACION_BACKUP_HORAS]
                ?: AjustesMovilVista.POR_DEFECTO.deliberacionBackupHoras,
            deliberacionSaludMaxMin = preferencias[CLAVE_DELIBERACION_SALUD_MAX_MIN]
                ?: AjustesMovilVista.POR_DEFECTO.deliberacionSaludMaxMin,
        )
        return acotarAjustes(leidos).ajustes
    }

    val CLAVE_CARPETA_URI = stringPreferencesKey("carpeta_uri")
    val CLAVE_TAMANO_LETRA = intPreferencesKey("tamano_letra")
    val CLAVE_CLAVE_ANTERIOR_HASTA = longPreferencesKey("clave_anterior_hasta")
    val CLAVE_SESIONES_ABIERTAS = intPreferencesKey("sesiones_abiertas")
    /** Acciones de servicio en curso cuando Android cerró MAGI (§7.12). */
    val CLAVE_ACCIONES_ABIERTAS = intPreferencesKey("acciones_abiertas")
    /** Ejecuciones de snippets en curso cuando Android cerró MAGI (§7.12). */
    val CLAVE_EJECUCIONES_ABIERTAS = intPreferencesKey("ejecuciones_abiertas")
    val CLAVE_GUIA_BATERIA_VISTA = booleanPreferencesKey("guia_bateria_vista")

    /** La clave del móvil se recreó al corregir el desbloqueo: hay que avisar. */
    val CLAVE_AVISO_CLAVE_RECREADA = booleanPreferencesKey("aviso_clave_recreada")

    // Ajustes de la Fase 14 (§5.4): los mismos del escritorio, en DataStore.
    val CLAVE_FLOTA_REFRESCO_SEG = intPreferencesKey("flota_refresco_seg")
    val CLAVE_UMBRAL_CARGA_POR_NUCLEO = doublePreferencesKey("umbral_carga_por_nucleo")
    val CLAVE_UMBRAL_MEMORIA_PCT = doublePreferencesKey("umbral_memoria_pct")
    val CLAVE_UMBRAL_DISCO_PCT = doublePreferencesKey("umbral_disco_pct")
    val CLAVE_SERVICIOS_REFRESCO_SEG = intPreferencesKey("servicios_refresco_seg")
    val CLAVE_SERVICIOS_ESPERA_SEG = intPreferencesKey("servicios_espera_seg")
    val CLAVE_SERVICIOS_CONTENEDORES = stringPreferencesKey("servicios_contenedores")
    val CLAVE_SUDO_MEMORIA_MIN = intPreferencesKey("sudo_memoria_min")
    val CLAVE_DELIBERACION_BACKUP_HORAS = intPreferencesKey("deliberacion_backup_horas")
    val CLAVE_DELIBERACION_SALUD_MAX_MIN = intPreferencesKey("deliberacion_salud_max_min")
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

    /**
     * Guarda los ajustes de Flota, Servicios y Deliberación ya acotados (§5.4).
     * Devuelve los avisos de lo que se corrigió, para enseñarlos.
     */
    suspend fun guardarAjustesMovil(ajustes: AjustesMovilVista): List<String> {
        val resultado = acotarAjustes(ajustes)
        val normalizados = resultado.ajustes
        almacen.edit { preferencias ->
            preferencias[ReglasPreferencias.CLAVE_FLOTA_REFRESCO_SEG] = normalizados.flotaRefrescoSeg
            preferencias[ReglasPreferencias.CLAVE_UMBRAL_CARGA_POR_NUCLEO] =
                normalizados.umbralCargaPorNucleo
            preferencias[ReglasPreferencias.CLAVE_UMBRAL_MEMORIA_PCT] =
                normalizados.umbralMemoriaPct
            preferencias[ReglasPreferencias.CLAVE_UMBRAL_DISCO_PCT] = normalizados.umbralDiscoPct
            preferencias[ReglasPreferencias.CLAVE_SERVICIOS_REFRESCO_SEG] =
                normalizados.serviciosRefrescoSeg
            preferencias[ReglasPreferencias.CLAVE_SERVICIOS_ESPERA_SEG] =
                normalizados.serviciosEsperaSeg
            preferencias[ReglasPreferencias.CLAVE_SERVICIOS_CONTENEDORES] =
                normalizados.serviciosContenedores.joinToString(",")
            preferencias[ReglasPreferencias.CLAVE_SUDO_MEMORIA_MIN] = normalizados.sudoMemoriaMin
            preferencias[ReglasPreferencias.CLAVE_DELIBERACION_BACKUP_HORAS] =
                normalizados.deliberacionBackupHoras
            preferencias[ReglasPreferencias.CLAVE_DELIBERACION_SALUD_MAX_MIN] =
                normalizados.deliberacionSaludMaxMin
        }
        return resultado.avisos
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

    /**
     * Anota el trabajo que sigue vivo (sesiones, acciones y ejecuciones) para
     * poder avisar de «Android cerró MAGI con N acciones en curso» si el proceso
     * muere (informe §7.12). Es una sola edición: los tres recuentos van juntos.
     */
    suspend fun guardarTrabajoAbierto(sesiones: Int, acciones: Int, ejecuciones: Int) {
        almacen.edit { preferencias ->
            preferencias[ReglasPreferencias.CLAVE_SESIONES_ABIERTAS] =
                ReglasPreferencias.sanearSesionesAbiertas(sesiones)
            preferencias[ReglasPreferencias.CLAVE_ACCIONES_ABIERTAS] =
                ReglasPreferencias.sanearTrabajoAbierto(acciones)
            preferencias[ReglasPreferencias.CLAVE_EJECUCIONES_ABIERTAS] =
                ReglasPreferencias.sanearTrabajoAbierto(ejecuciones)
        }
    }

    /** Acciones que quedaron en curso cuando Android cerró MAGI; las reinicia. */
    suspend fun consumirAccionesAbiertas(): Int {
        var anteriores = 0
        almacen.edit { preferencias ->
            anteriores = preferencias[ReglasPreferencias.CLAVE_ACCIONES_ABIERTAS] ?: 0
            preferencias[ReglasPreferencias.CLAVE_ACCIONES_ABIERTAS] = 0
        }
        return ReglasPreferencias.sanearTrabajoAbierto(anteriores)
    }

    /** Ejecuciones que quedaron en curso cuando Android cerró MAGI; las reinicia. */
    suspend fun consumirEjecucionesAbiertas(): Int {
        var anteriores = 0
        almacen.edit { preferencias ->
            anteriores = preferencias[ReglasPreferencias.CLAVE_EJECUCIONES_ABIERTAS] ?: 0
            preferencias[ReglasPreferencias.CLAVE_EJECUCIONES_ABIERTAS] = 0
        }
        return ReglasPreferencias.sanearTrabajoAbierto(anteriores)
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
