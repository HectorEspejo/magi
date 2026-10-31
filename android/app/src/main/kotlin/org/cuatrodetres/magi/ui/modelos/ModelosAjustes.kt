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
