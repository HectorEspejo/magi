package org.cuatrodetres.magi.plataforma

import androidx.fragment.app.FragmentActivity

/**
 * Servicios de plataforma que pide el núcleo móvil y que usa la propia app
 * (informe de la Fase 11, §5.2).
 *
 * Las firmas de [CarpetaMovil], [LlaveroMovil] y [FirmanteMovil] espejan los
 * traits foreign de `crates/magi-movil/src/retrollamadas.rs` (métodos
 * síncronos, `ByteArray` y `List<FicheroCarpeta>`), pero con tipos propios de
 * la capa de plataforma. Cuando existan los bindings generados por UniFFI se
 * añadirá un adaptador fino que traduzca estos tipos y [ErrorPlataforma] a los
 * generados (`ErrorNucleo`); por eso aquí no se importa nada generado.
 *
 * Ningún secreto puede acabar en logs, DataStore ni ficheros en claro.
 */
interface FachadaPlataforma {
    /** Carpeta sincronizada, leída con Storage Access Framework y solo lectura. */
    val carpeta: CarpetaMovil

    /** Secretos cifrados con AES-256-GCM y una clave del Keystore. */
    val llavero: LlaveroMovil

    /** Clave del móvil ECDSA P-256: firma sin exportar la clave. */
    val firmante: FirmanteMovil

    /** Bloqueo de la app con biometría fuerte o credencial del teléfono. */
    val bloqueo: BloqueoApp
}

/**
 * Fichero de la carpeta compartida tal como lo lista SAF (espejo de
 * `FicheroCarpeta` del contrato UniFFI). El tamaño y la fecha son datos no
 * fiables: la pasada vuelve a validarlos (T81).
 */
data class FicheroCarpeta(
    val nombre: String,
    val tamano: Long,
    /** Última modificación, en segundos desde la época (0 si SAF no la da). */
    val modificadoEn: Long,
)

/** Listado y lectura de los ficheros `magi-<uuid>.age` de la carpeta elegida. */
interface CarpetaMovil {
    /** Lista solo los ficheros `magi-<uuid>.age` canónicos y de tamaño válido. */
    fun listar(): List<FicheroCarpeta>

    /** Lee un fichero entero; falla sin leerlo si supera [maximo] bytes. */
    fun leer(nombre: String, maximo: Long): ByteArray
}

/** Clave de sincronización y contraseñas de host, siempre cifradas. */
interface LlaveroMovil {
    /** Devuelve el secreto o `null` si no existe. */
    fun leer(clave: String): ByteArray?

    /** Guarda (o sustituye) un secreto. */
    fun guardar(clave: String, secreto: ByteArray)

    /** Borra un secreto; no falla si no existía. */
    fun borrar(clave: String)
}

/** Firma ECDSA P-256 con la clave del Keystore (StrongBox si existe; si no, TEE). */
interface FirmanteMovil {
    /** Clave pública X.509 (DER) de la clave del móvil. */
    fun clavePublicaX509(): ByteArray

    /** Firma con SHA256withECDSA; devuelve la firma DER. */
    fun firmar(datos: ByteArray): ByteArray

    /** Borra la clave del móvil del Keystore (solo para «borrar todos los datos»). */
    fun olvidarClave()
}

/** Desbloqueo de MAGI: huella o PIN del teléfono, con 5 minutos de margen. */
interface BloqueoApp {
    /** El teléfono tiene PIN, patrón o contraseña (sin él MAGI no arranca). */
    fun elTelefonoTieneBloqueo(): Boolean

    /** Abre BiometricPrompt; `true` si el usuario se desbloquea. */
    suspend fun desbloquear(actividad: FragmentActivity): Boolean
}
