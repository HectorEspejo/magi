package org.cuatrodetres.magi.plataforma

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.security.keystore.UserNotAuthenticatedException
import org.cuatrodetres.magi.R
import java.security.GeneralSecurityException
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.UnrecoverableKeyException
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec

/**
 * Clave del móvil (informe §7.2 y §9): ECDSA P-256 en el Android Keystore,
 * StrongBox si el teléfono lo tiene y TEE si no. La privada nunca sale del
 * Keystore ni pasa por la memoria del proceso: aquí solo se pide la pública
 * X.509 y la firma DER de SHA256withECDSA.
 *
 * - Sin setUnlockedDeviceRequired (corrección del fallo de desbloqueo facial):
 *   en algunos teléfonos —se comprobó en un Honor MagicOS— Android devuelve
 *   UserNotAuthenticatedException aunque el teléfono esté desbloqueado si se
 *   desbloqueó con la cara (fallo conocido de Android/OEM; Google issue
 *   506989112). La clave ya no depende de ese estado; el bloqueo propio de MAGI
 *   decide cuándo se muestra la app y cuándo se abre el núcleo.
 * - setUserAuthenticationRequired(false) (D174): no se pide biometría en cada
 *   uso; la clave se usa después de que el usuario desbloquee la app.
 * - Si Android invalida la clave (se quitó el bloqueo de pantalla), se detecta
 *   y se explica que hay que crear otra y repetir el «instalar mi clave».
 * - Compatibilidad: al arrancar, si existe la clave antigua se crea la nueva
 *   (alias -v2) y se borra la antigua. Eso cambia la identidad del móvil, así
 *   que la interfaz avisa de que hay que reinstalarla en los hosts.
 */
class ClaveMovil(private val contexto: Context) : FirmanteMovil {

    companion object {
        /** Alias antiguo de la clave del móvil (versiones anteriores). */
        const val ALIAS = "magi-clave-movil"

        /** Alias actual de la clave del móvil (sin requisito de desbloqueo). */
        const val ALIAS_ACTUAL = "magi-clave-movil-v2"

        private const val PROVEEDOR = "AndroidKeyStore"
        private const val ALGORITMO_FIRMA = "SHA256withECDSA"
        private const val CURVA = "secp256r1"
    }

    /** Estado de la clave del móvil (diagrama de §3). */
    enum class Estado { SIN_CREAR, LISTA, INVALIDADA }

    /** Dónde vive la clave privada: StrongBox o TEE. */
    enum class Ubicacion { STRONGBOX, TEE }

    /** Lo que Ajustes > Clave del móvil enseña, sin exponer la privada. */
    data class Descripcion(
        val estado: Estado,
        val alias: String,
        val ubicacion: Ubicacion? = null,
        /** Fecha de creación (epoch en milisegundos) según el certificado. */
        val creadaEnMs: Long? = null,
    )

    private val cerrojo = Any()

    /** Estado actual sin crear nada: consulta barata y segura. */
    fun estado(): Estado = synchronized(cerrojo) {
        try {
            val almacen = almacenDeClaves()
            val alias = aliasActivo(almacen)
            if (!almacen.containsAlias(alias)) return@synchronized Estado.SIN_CREAR
            val clave = almacen.getKey(alias, null)
            if (clave == null) Estado.INVALIDADA else Estado.LISTA
        } catch (e: UnrecoverableKeyException) {
            Estado.INVALIDADA
        } catch (e: KeyPermanentlyInvalidatedException) {
            Estado.INVALIDADA
        } catch (e: GeneralSecurityException) {
            Estado.INVALIDADA
        } catch (e: RuntimeException) {
            Estado.INVALIDADA
        }
    }

    /** Descripción completa (para Ajustes): estado, ubicación y fecha. */
    fun descripcion(): Descripcion = synchronized(cerrojo) {
        val actual = estado()
        if (actual != Estado.LISTA) return@synchronized Descripcion(actual, ALIAS_ACTUAL)
        val almacen = almacenDeClaves()
        val alias = aliasActivo(almacen)
        val certificado = almacen.getCertificate(alias) as? X509Certificate
        val clave = almacen.getKey(alias, null) as? PrivateKey
        Descripcion(
            estado = Estado.LISTA,
            alias = alias,
            ubicacion = clave?.let(::ubicacionDe),
            creadaEnMs = certificado?.notBefore?.time,
        )
    }

    /**
     * Crea la clave si no existe (primer arranque) y devuelve su descripción.
     * Falla con un motivo legible si el teléfono no tiene bloqueo de pantalla,
     * está bloqueado o la clave está invalidada.
     */
    fun asegurarClave(): Descripcion {
        synchronized(cerrojo) {
            when (estado()) {
                Estado.LISTA -> return descripcion()
                Estado.INVALIDADA -> throw ErrorPlataforma.Firmante(
                    contexto.getString(R.string.plataforma_clave_invalidada)
                )
                Estado.SIN_CREAR -> Unit
            }
            if (!elTelefonoTieneBloqueo()) {
                throw ErrorPlataforma.Firmante(
                    contexto.getString(R.string.plataforma_clave_sin_bloqueo_pantalla)
                )
            }
            generarConRespaldo()
        }
        return descripcion()
    }

    /**
     * Migración de compatibilidad: si existe la clave antigua y todavía no la
     * nueva, crea la nueva sin el requisito de desbloqueo y borra la antigua.
     * Devuelve true si se recreó la identidad (hay que reinstalarla en hosts).
     */
    fun migrarSinBloqueo(): Boolean = synchronized(cerrojo) {
        try {
            val almacen = almacenDeClaves()
            if (almacen.containsAlias(ALIAS_ACTUAL)) {
                if (almacen.containsAlias(ALIAS)) {
                    runCatching { almacen.deleteEntry(ALIAS) }
                }
                return@synchronized false
            }
            if (!almacen.containsAlias(ALIAS)) return@synchronized false
            generarConRespaldo()
            runCatching { almacen.deleteEntry(ALIAS) }
            true
        } catch (e: Exception) {
            // La clave antigua sigue en su sitio: no se pierde identidad y se
            // reintenta en el siguiente arranque.
            false
        }
    }

    /** Borra la clave del móvil del Keystore (solo para «borrar todos los datos»). */
    override fun olvidarClave() {
        synchronized(cerrojo) {
            try {
                val almacen = almacenDeClaves()
                almacen.deleteEntry(ALIAS_ACTUAL)
                almacen.deleteEntry(ALIAS)
            } catch (e: GeneralSecurityException) {
                throw ErrorPlataforma.Firmante(
                    contexto.getString(R.string.plataforma_clave_no_borrada),
                    e,
                )
            }
        }
    }

    /** El teléfono tiene PIN, patrón o contraseña (MAGI no arranca sin ello). */
    fun elTelefonoTieneBloqueo(): Boolean {
        val gestor = contexto.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return gestor?.isDeviceSecure == true
    }

    /** Clave pública X.509 (SubjectPublicKeyInfo DER) de la clave del móvil. */
    override fun clavePublicaX509(): ByteArray = synchronized(cerrojo) {
        val almacen = try {
            almacenDeClaves()
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_sin_crear),
                e,
            )
        }
        val alias = aliasActivo(almacen)
        val certificado = try {
            almacen.getCertificate(alias) as? X509Certificate
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_invalidada),
                e,
            )
        } ?: throw ErrorPlataforma.Firmante(
            contexto.getString(R.string.plataforma_clave_sin_crear)
        )
        certificado.publicKey?.encoded ?: throw ErrorPlataforma.Firmante(
            contexto.getString(R.string.plataforma_clave_sin_publica)
        )
    }

    /** Firma con SHA256withECDSA y devuelve la firma DER (la convierte el núcleo). */
    override fun firmar(datos: ByteArray): ByteArray = synchronized(cerrojo) {
        val privada = try {
            val almacen = almacenDeClaves()
            val alias = aliasActivo(almacen)
            almacen.getKey(alias, null) as? PrivateKey
        } catch (e: UnrecoverableKeyException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_invalidada),
                e,
            )
        } catch (e: KeyPermanentlyInvalidatedException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_invalidada),
                e,
            )
        } catch (e: GeneralSecurityException) {
            throw errorDeFirma(e)
        }
        if (privada == null) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_sin_crear)
            )
        }
        try {
            val firma = Signature.getInstance(ALGORITMO_FIRMA)
            firma.initSign(privada)
            firma.update(datos)
            firma.sign()
        } catch (e: UserNotAuthenticatedException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_telefono_bloqueado),
                e,
            )
        } catch (e: KeyPermanentlyInvalidatedException) {
            throw ErrorPlataforma.Firmante(
                contexto.getString(R.string.plataforma_clave_invalidada),
                e,
            )
        } catch (e: GeneralSecurityException) {
            throw errorDeFirma(e)
        } catch (e: RuntimeException) {
            throw errorDeFirma(e)
        }
    }

    // -----------------------------------------------------------------------
    // Interno
    // -----------------------------------------------------------------------

    private fun almacenDeClaves(): KeyStore =
        KeyStore.getInstance(PROVEEDOR).apply { load(null) }

    private fun aliasActivo(almacen: KeyStore): String =
        if (almacen.containsAlias(ALIAS_ACTUAL)) ALIAS_ACTUAL else ALIAS

    private fun generarConRespaldo(): Descripcion {
        try {
            generar(conStrongBox = true)
        } catch (e: StrongBoxUnavailableException) {
            // El teléfono no tiene StrongBox: se usa el TEE sin error (AC).
            try {
                generar(conStrongBox = false)
            } catch (e2: Exception) {
                throw errorDeCreacion(e2)
            }
        } catch (e: Exception) {
            try {
                generar(conStrongBox = false)
            } catch (e2: Exception) {
                throw errorDeCreacion(e2)
            }
        }
        return descripcion()
    }

    private fun generar(conStrongBox: Boolean) {
        val constructor = KeyGenParameterSpec.Builder(
            ALIAS_ACTUAL,
            KeyProperties.PURPOSE_SIGN,
        )
            .setAlgorithmParameterSpec(ECGenParameterSpec(CURVA))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setUserAuthenticationRequired(false) // D174
        if (conStrongBox) constructor.setIsStrongBoxBacked(true)
        // Una clave EC se genera con KeyPairGenerator: KeyGenerator solo vale
        // para claves simétricas y AndroidKeyStore lanza NoSuchAlgorithm para EC.
        val generador = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVEEDOR)
        generador.initialize(constructor.build())
        generador.generateKeyPair()
    }

    /**
     * StrongBox o TEE. securityLevel llega en Android 12 (API 31); en API 30
     * no se puede distinguir, así que se informa TEE (el respaldo del informe).
     */
    private fun ubicacionDe(privada: PrivateKey): Ubicacion = try {
        val fabrica = KeyFactory.getInstance(privada.algorithm, PROVEEDOR)
        val informacion = fabrica.getKeySpec(privada, KeyInfo::class.java) as KeyInfo
        val strongbox = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            informacion.securityLevel == KeyProperties.SECURITY_LEVEL_STRONGBOX
        } else {
            // En API 30 no se puede distinguir StrongBox del TEE: se informa TEE.
            false
        }
        if (strongbox) Ubicacion.STRONGBOX else Ubicacion.TEE
    } catch (e: GeneralSecurityException) {
        Ubicacion.TEE
    } catch (e: RuntimeException) {
        Ubicacion.TEE
    }

    private fun errorDeCreacion(error: Exception): ErrorPlataforma.Firmante {
        val mensaje = when (error) {
            is UserNotAuthenticatedException ->
                contexto.getString(R.string.plataforma_clave_telefono_bloqueado)

            is StrongBoxUnavailableException ->
                contexto.getString(R.string.plataforma_clave_strongbox_no_disponible)

            is KeyPermanentlyInvalidatedException ->
                contexto.getString(R.string.plataforma_clave_invalidada)

            else -> contexto.getString(R.string.plataforma_clave_no_creada)
        }
        return ErrorPlataforma.Firmante(mensaje, error)
    }

    private fun errorDeFirma(error: Exception): ErrorPlataforma.Firmante {
        val mensaje = when {
            telefonoBloqueado() ->
                contexto.getString(R.string.plataforma_clave_telefono_bloqueado)

            error is KeyPermanentlyInvalidatedException || error is UnrecoverableKeyException ->
                contexto.getString(R.string.plataforma_clave_invalidada)

            else -> contexto.getString(R.string.plataforma_clave_no_firmado)
        }
        return ErrorPlataforma.Firmante(mensaje, error)
    }

    private fun telefonoBloqueado(): Boolean {
        val gestor = contexto.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        return gestor?.isDeviceLocked == true
    }
}
