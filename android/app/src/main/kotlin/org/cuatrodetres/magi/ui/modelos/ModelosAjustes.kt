package org.cuatrodetres.magi.ui.modelos

/**
 * Ajustes y «Acerca de» (informe §6 y checklist de interfaz).
 *
 * La clave del móvil se crea en el Android Keystore (StrongBox si existe, si no
 * TEE) y nunca sale del teléfono; la interfaz solo enseña su línea pública, su
 * huella, dónde vive y cuándo se creó.
 */

/** Dónde vive la clave del móvil (informe §7.2). */
enum class UbicacionClaveVista {
    STRONGBOX,
    TEE,
    DESCONOCIDA,
}

/** Clave pública de la clave del móvil y sus datos de Ajustes. */
data class ClaveMovilVista(
    val lineaOpenssh: String,
    val huellaSha256: String,
    val ubicacion: UbicacionClaveVista = UbicacionClaveVista.DESCONOCIDA,
    /** Instante de creación (epoch en segundos). */
    val creadaEn: Long? = null,
    /** Android invalidó la clave (se quitó el bloqueo de pantalla). */
    val invalidada: Boolean = false,
)

/** Versiones de la app y del núcleo para «Acerca de» (informe §7.10). */
data class VersionVista(
    val app: String,
    val codigo: Int,
    val nucleo: String,
    /** Formato de sincronización que entiende el núcleo. */
    val formato: Int,
    /** `user_version` de magi.db. */
    val userVersion: Int,
)

/** Resultado de «instalar mi clave en este host» (informe §4.4). */
sealed interface ResultadoInstalacionVista {
    data object YaEstaba : ResultadoInstalacionVista
    data object InstaladaAceptada : ResultadoInstalacionVista
    data object InstaladaNoAceptada : ResultadoInstalacionVista
    data class Error(val mensaje: String) : ResultadoInstalacionVista
}

/** Entrada de la pantalla Licencias. */
data class EntradaLicenciaVista(
    val nombre: String,
    val version: String,
    val licencia: String,
)

/**
 * Licencias de las dependencias que no se pueden leer de un fichero generado
 * en la compilación. La OFL de JetBrains Mono se lee de `res/raw`.
 */
val LICENCIAS_BASE: List<EntradaLicenciaVista> = listOf(
    EntradaLicenciaVista("Kotlin", "2.4.21", "Apache-2.0"),
    EntradaLicenciaVista("Jetpack Compose · Material 3", "BOM 2026.09.00", "Apache-2.0"),
    EntradaLicenciaVista("AndroidX Core, Activity, Lifecycle, Navigation y DataStore", "Varias", "Apache-2.0"),
    EntradaLicenciaVista("androidx.biometric", "1.1.0", "Apache-2.0"),
    EntradaLicenciaVista("CameraX", "1.6.2", "Apache-2.0"),
    EntradaLicenciaVista("ZXing core", "3.5.4", "Apache-2.0"),
    EntradaLicenciaVista("JNA", "5.19.1", "Apache-2.0 o LGPL-2.1"),
    EntradaLicenciaVista("UniFFI", "0.32.2", "MPL-2.0"),
    EntradaLicenciaVista("MAGI", "0.1.0", "MIT o Apache-2.0"),
    EntradaLicenciaVista("JetBrains Mono", "2.304", "OFL-1.1"),
    EntradaLicenciaVista("Crates de Rust de MAGI (tokio, russh, rusqlite, age, vt100…)", "—", "Las de Cargo.lock"),
)

// ===========================================================================
// Ajustes de la Fase 14 (informe §5.4)
// ===========================================================================

/*
 * En el escritorio, estos ajustes viven en config.toml; en el móvil, en
 * DataStore. Los valores por defecto y los rangos son los del escritorio y el
 * núcleo: un valor fuera de rango no impide arrancar, se acota y Ajustes dice
 * por qué.
 */

/** Auto-refresco de Flota por defecto (30 s). */
const val FLOTA_REFRESCO_SEG_POR_DEFECTO: Int = 30

/** Auto-refresco mínimo de Flota (15 s; 0 = apagado). */
const val FLOTA_REFRESCO_SEG_MIN: Int = 15

/** Auto-refresco máximo de Flota (300 s). */
const val FLOTA_REFRESCO_SEG_MAX: Int = 300

/** Umbral de carga por núcleo por defecto. */
const val CARGA_POR_NUCLEO_POR_DEFECTO: Double = 1.0

/** Umbral de carga por núcleo mínimo (0,1). */
const val CARGA_POR_NUCLEO_MIN: Double = 0.1

/** Umbral de carga por núcleo máximo (16,0). */
const val CARGA_POR_NUCLEO_MAX: Double = 16.0

/** Umbral de memoria por defecto (90 %). */
const val MEMORIA_PCT_POR_DEFECTO: Double = 90.0

/** Umbral de disco por defecto (90 %). */
const val DISCO_PCT_POR_DEFECTO: Double = 90.0

/** Umbral mínimo de memoria y disco (0 %). */
const val UMBRAL_PCT_MIN: Double = 0.0

/** Umbral máximo de memoria y disco (100 %). */
const val UMBRAL_PCT_MAX: Double = 100.0

/** Refresco de Servicios por defecto (10 s). */
const val SERVICIOS_REFRESCO_SEG_POR_DEFECTO: Int = 10

/** Refresco de Servicios mínimo (5 s; 0 = solo al entrar). */
const val SERVICIOS_REFRESCO_SEG_MIN: Int = 5

/** Refresco de Servicios máximo (300 s). */
const val SERVICIOS_REFRESCO_SEG_MAX: Int = 300

/** Espera al estado final de una acción por defecto (30 s). */
const val SERVICIOS_ESPERA_SEG_POR_DEFECTO: Int = 30

/** Espera mínima al estado final de una acción (5 s). */
const val SERVICIOS_ESPERA_SEG_MIN: Int = 5

/** Espera máxima al estado final de una acción (300 s). */
const val SERVICIOS_ESPERA_SEG_MAX: Int = 300

/** Minutos de sudo en memoria por defecto (5). */
const val SUDO_MEMORIA_MIN_POR_DEFECTO: Int = 5

/** Minutos máximos de sudo en memoria (15; 0 = pedirla siempre). */
const val SUDO_MEMORIA_MIN_MAX: Int = 15

/** Antigüedad máxima del backup por defecto (24 h). */
const val BACKUP_HORAS_POR_DEFECTO: Int = 24

/** Antigüedad mínima del backup (1 h). */
const val BACKUP_HORAS_MIN: Int = 1

/** Antigüedad máxima del backup (720 h, 30 días). */
const val BACKUP_HORAS_MAX: Int = 720

/** Antigüedad máxima del sondeo de MELCHIOR-1 por defecto (5 min). */
const val SALUD_MAX_MIN_POR_DEFECTO: Int = 5

/** Antigüedad mínima del sondeo de MELCHIOR-1 (1 min). */
const val SALUD_MAX_MIN_MIN: Int = 1

/** Antigüedad máxima del sondeo de MELCHIOR-1 (1 440 min, 24 h). */
const val SALUD_MAX_MIN_MAX: Int = 1_440

/** Motores de contenedores que admite el ajuste, por orden de preferencia. */
val MOTORES_POR_DEFECTO: List<String> = listOf("docker", "podman")

/** Ajustes del móvil que en el escritorio viven en config.toml (§5.4). */
data class AjustesMovilVista(
    /** Auto-refresco de Flota: 0 (apagado) o 15-300 s; por defecto 30. */
    val flotaRefrescoSeg: Int = FLOTA_REFRESCO_SEG_POR_DEFECTO,
    /** Umbral de carga por núcleo; por defecto 1,0. */
    val umbralCargaPorNucleo: Double = CARGA_POR_NUCLEO_POR_DEFECTO,
    /** Umbral de memoria en porcentaje; por defecto 90. */
    val umbralMemoriaPct: Double = MEMORIA_PCT_POR_DEFECTO,
    /** Umbral de disco en porcentaje; por defecto 90. */
    val umbralDiscoPct: Double = DISCO_PCT_POR_DEFECTO,
    /** Refresco de la vista Servicios: 0 (solo al entrar) o 5-300 s. */
    val serviciosRefrescoSeg: Int = SERVICIOS_REFRESCO_SEG_POR_DEFECTO,
    /** Espera máxima al estado final de una acción: 5-300 s. */
    val serviciosEsperaSeg: Int = SERVICIOS_ESPERA_SEG_POR_DEFECTO,
    /** Motores de contenedores, sin repetir y en orden; vacío = sin contenedores. */
    val serviciosContenedores: List<String> = MOTORES_POR_DEFECTO,
    /** Minutos sin uso tras los que se olvida la contraseña de sudo: 0-15. */
    val sudoMemoriaMin: Int = SUDO_MEMORIA_MIN_POR_DEFECTO,
    /** BALTHASAR-2: antigüedad máxima del último backup en horas. */
    val deliberacionBackupHoras: Int = BACKUP_HORAS_POR_DEFECTO,
    /** MELCHIOR-1: antigüedad máxima del último sondeo en minutos. */
    val deliberacionSaludMaxMin: Int = SALUD_MAX_MIN_POR_DEFECTO,
) {
    /** Motores válidos del ajuste, ya como enum. */
    val motores: List<MotorNombreVista>
        get() = serviciosContenedores.mapNotNull { texto ->
            when (texto.trim().lowercase()) {
                MotorNombreVista.DOCKER.texto -> MotorNombreVista.DOCKER
                MotorNombreVista.PODMAN.texto -> MotorNombreVista.PODMAN
                else -> null
            }
        }

    companion object {
        /** Los ajustes por defecto del móvil (los mismos del escritorio). */
        val POR_DEFECTO: AjustesMovilVista = AjustesMovilVista()
    }
}

/** Ajustes ya acotados más los avisos de lo que se corrigió. */
data class ResultadoAjustesVista(
    val ajustes: AjustesMovilVista = AjustesMovilVista.POR_DEFECTO,
    /** Un aviso legible por cada valor corregido. */
    val avisos: List<String> = emptyList(),
) {
    /** No hubo que corregir nada. */
    val sinAvisos: Boolean get() = avisos.isEmpty()
}

/**
 * Acota los valores fuera de rango y devuelve los ajustes normalizados con un
 * aviso por cada corrección. Es idempotente: normalizar lo ya normalizado no
 * produce avisos. La app lo usa antes de guardar en DataStore y el núcleo hace
 * lo mismo al aplicar los ajustes.
 */
fun acotarAjustes(ajustes: AjustesMovilVista): ResultadoAjustesVista {
    val avisos = mutableListOf<String>()
    val normalizados = AjustesMovilVista(
        flotaRefrescoSeg = acotarRefresco(
            ajustes.flotaRefrescoSeg,
            FLOTA_REFRESCO_SEG_MIN,
            FLOTA_REFRESCO_SEG_MAX,
            "flota_refresco_seg",
            avisos,
        ),
        umbralCargaPorNucleo = acotarUmbral(
            ajustes.umbralCargaPorNucleo,
            CARGA_POR_NUCLEO_MIN,
            CARGA_POR_NUCLEO_MAX,
            CARGA_POR_NUCLEO_POR_DEFECTO,
            "umbral_carga_por_nucleo",
            avisos,
        ),
        umbralMemoriaPct = acotarUmbral(
            ajustes.umbralMemoriaPct,
            UMBRAL_PCT_MIN,
            UMBRAL_PCT_MAX,
            MEMORIA_PCT_POR_DEFECTO,
            "umbral_memoria_pct",
            avisos,
        ),
        umbralDiscoPct = acotarUmbral(
            ajustes.umbralDiscoPct,
            UMBRAL_PCT_MIN,
            UMBRAL_PCT_MAX,
            DISCO_PCT_POR_DEFECTO,
            "umbral_disco_pct",
            avisos,
        ),
        serviciosRefrescoSeg = acotarRefresco(
            ajustes.serviciosRefrescoSeg,
            SERVICIOS_REFRESCO_SEG_MIN,
            SERVICIOS_REFRESCO_SEG_MAX,
            "servicios_refresco_seg",
            avisos,
        ),
        serviciosEsperaSeg = acotarEntero(
            ajustes.serviciosEsperaSeg,
            SERVICIOS_ESPERA_SEG_MIN,
            SERVICIOS_ESPERA_SEG_MAX,
            "servicios_espera_seg",
            "s",
            avisos,
        ),
        serviciosContenedores = acotarMotores(ajustes.serviciosContenedores, avisos),
        sudoMemoriaMin = acotarEntero(
            ajustes.sudoMemoriaMin,
            0,
            SUDO_MEMORIA_MIN_MAX,
            "sudo_memoria_min",
            "min",
            avisos,
        ),
        deliberacionBackupHoras = acotarEntero(
            ajustes.deliberacionBackupHoras,
            BACKUP_HORAS_MIN,
            BACKUP_HORAS_MAX,
            "deliberacion_backup_horas",
            "h",
            avisos,
        ),
        deliberacionSaludMaxMin = acotarEntero(
            ajustes.deliberacionSaludMaxMin,
            SALUD_MAX_MIN_MIN,
            SALUD_MAX_MIN_MAX,
            "deliberacion_salud_max_min",
            "min",
            avisos,
        ),
    )
    return ResultadoAjustesVista(normalizados, avisos)
}

/** Un refresco admite 0 (apagado) o un rango con mínimo. */
private fun acotarRefresco(
    valor: Int,
    minimo: Int,
    maximo: Int,
    campo: String,
    avisos: MutableList<String>,
): Int {
    if (valor == 0) return 0
    val acotado = valor.coerceIn(minimo, maximo)
    if (acotado != valor) {
        avisos += campo + " = " + valor +
            " s: fuera de rango, se usa " + acotado + " s (0 = apagado)"
    }
    return acotado
}

/** Acota un entero a su rango, con aviso si había que corregirlo. */
private fun acotarEntero(
    valor: Int,
    minimo: Int,
    maximo: Int,
    campo: String,
    unidad: String,
    avisos: MutableList<String>,
): Int {
    val acotado = valor.coerceIn(minimo, maximo)
    if (acotado != valor) {
        avisos += campo + " = " + valor + " " + unidad +
            ": fuera de rango, se usa " + acotado + " " + unidad
    }
    return acotado
}

/** Acota un umbral; un valor no finito vuelve al de defecto. */
private fun acotarUmbral(
    valor: Double,
    minimo: Double,
    maximo: Double,
    defecto: Double,
    campo: String,
    avisos: MutableList<String>,
): Double {
    if (!valor.isFinite()) {
        avisos += campo + " no es un número válido: se usa " + unDecimal(defecto)
        return defecto
    }
    val acotado = valor.coerceIn(minimo, maximo)
    if (acotado != valor) {
        avisos += campo + " = " + unDecimal(valor) +
            ": fuera de rango, se usa " + unDecimal(acotado)
    }
    return acotado
}

/** Deja solo docker y podman, sin repetir y en el orden recibido. */
private fun acotarMotores(
    motores: List<String>,
    avisos: MutableList<String>,
): List<String> {
    val limpios = mutableListOf<String>()
    for (original in motores) {
        val motor = original.trim().lowercase()
        if (motor.isEmpty()) {
            avisos += "servicios_contenedores: se ignora un motor vacío"
            continue
        }
        if (motor != "docker" && motor != "podman") {
            avisos += "servicios_contenedores solo admite docker o podman, no «" +
                motor + "»: se ignora"
            continue
        }
        if (motor in limpios) {
            avisos += "servicios_contenedores repite «" + motor + "»: se usa una vez"
            continue
        }
        limpios += motor
    }
    return limpios
}
