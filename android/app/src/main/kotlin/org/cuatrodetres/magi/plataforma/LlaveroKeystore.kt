package org.cuatrodetres.magi.plataforma

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.UserNotAuthenticatedException
import org.cuatrodetres.magi.R
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.UnrecoverableKeyException
import java.util.UUID
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Llavero del móvil (informe §5.2 y §8): guarda la clave de sincronización (y
 * la anterior) y las contraseñas de host cifradas con AES-256-GCM y una clave
 * del Android Keystore.
 *
 * - Un fichero por secreto en `filesDir/secretos/`, con escritura atómica
 *   (temporal + `rename`) y un formato con cabecera, IV y etiqueta GCM
 *   ([FormatoSecreto]). El nombre de la clave se ata como AAD: no se pueden
 *   intercambiar ficheros entre claves.
 * - La clave `magi-secretos` se crea con `setUnlockedDeviceRequired(true)`: con
 *   el teléfono bloqueado no se puede cifrar ni descifrar.
 * - Nunca se escribe nada en logcat ni en DataStore, y los bytes en claro no se
 *   copian a ningún sitio.
 */
class LlaveroKeystore(private val contexto: Context) : LlaveroMovil {

    companion object {
        /** Alias de la clave AES del Keystore (informe §9). */
        const val ALIAS_CLAVE = "magi-secretos"

        private const val PROVEEDOR = "AndroidKeyStore"
        private const val TRANSFORMACION = "AES/GCM/NoPadding"
        private const val TAMANO_CLAVE = 256
        private const val TAMANO_BITS_ETIQUETA = 128
        private const val DIRECTORIO_SECRETOS = "secretos"
    }

    private val cerrojo = Any()

    private val directorio: File by lazy {
        File(contexto.filesDir, DIRECTORIO_SECRETOS).apply { mkdirs() }
    }

    /** Devuelve el secreto descifrado, o `null` si no existe. */
    override fun leer(clave: String): ByteArray? = synchronized(cerrojo) {
        val fichero = ficheroDe(clave)
        if (!fichero.isFile) return@synchronized null
        val datos = try {
            fichero.readBytes()
        } catch (e: IOException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_ilegible, clave),
                e,
            )
        } catch (e: SecurityException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_ilegible, clave),
                e,
            )
        }
        val contenido = try {
            FormatoSecreto.desempaquetar(datos)
        } catch (e: IllegalArgumentException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_danado, clave),
                e,
            )
        }
        descifrar(clave, contenido)
    }

    /** Guarda (o sustituye) un secreto. */
    override fun guardar(clave: String, secreto: ByteArray) {
        synchronized(cerrojo) {
            val fichero = ficheroDe(clave)
            val (iv, cifrado) = cifrar(clave, secreto)
            escribirAtomico(fichero, FormatoSecreto.empaquetar(iv, cifrado))
        }
    }

    /** Borra un secreto; no falla si no existía. */
    override fun borrar(clave: String) {
        synchronized(cerrojo) {
            val fichero = ficheroDe(clave)
            if (fichero.exists() && !fichero.delete()) {
                throw ErrorPlataforma.Llavero(
                    contexto.getString(R.string.plataforma_llavero_no_borrado, clave)
                )
            }
        }
    }

    /** Si hay un secreto guardado con esa clave. */
    fun existe(clave: String): Boolean = synchronized(cerrojo) { ficheroDe(clave).isFile }

    /**
     * Borra todos los secretos y la clave del Keystore (para «borrar todos los
     * datos»). Después hay que volver a crear la clave al guardar el primero.
     */
    fun borrarTodo() {
        synchronized(cerrojo) {
            directorio.listFiles()?.forEach { if (it.isFile) it.delete() }
            try {
                almacenDeClaves().deleteEntry(ALIAS_CLAVE)
            } catch (_: GeneralSecurityException) {
                // Si la clave ya no estaba, no hay nada más que hacer.
            }
        }
    }

    // -----------------------------------------------------------------------
    // Interno
    // -----------------------------------------------------------------------

    private fun ficheroDe(clave: String): File {
        val nombre = ReglasLlavero.nombreDeFichero(clave)
            ?: throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_clave_invalida, clave)
            )
        return File(directorio, nombre)
    }

    private fun almacenDeClaves(): KeyStore =
        KeyStore.getInstance(PROVEEDOR).apply { load(null) }

    /** Clave AES del Keystore: la recupera o la crea la primera vez. */
    private fun claveDelKeystore(): SecretKey {
        val almacen = try {
            almacenDeClaves()
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_keystore_no_disponible),
                e,
            )
        }
        val entrada = try {
            almacen.getEntry(ALIAS_CLAVE, null)
        } catch (e: UnrecoverableKeyException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_clave_invalidada),
                e,
            )
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_clave_invalidada),
                e,
            )
        }
        if (entrada is KeyStore.SecretKeyEntry) return entrada.secretKey
        return try {
            val generador = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVEEDOR)
            generador.init(
                KeyGenParameterSpec.Builder(
                    ALIAS_CLAVE,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(TAMANO_CLAVE)
                    .setRandomizedEncryptionRequired(true)
                    .setUnlockedDeviceRequired(true)
                    .build(),
            )
            generador.generateKey()
        } catch (e: Exception) {
            throw errorDeKeystore(e, alLeer = false)
        }
    }

    private fun cifrar(clave: String, claro: ByteArray): Pair<ByteArray, ByteArray> {
        return try {
            val cifrador = Cipher.getInstance(TRANSFORMACION)
            cifrador.init(Cipher.ENCRYPT_MODE, claveDelKeystore())
            cifrador.updateAAD(clave.toByteArray(Charsets.UTF_8))
            val iv = cifrador.iv
                ?: throw GeneralSecurityException("el Keystore no devolvió IV")
            require(iv.size == FormatoSecreto.TAMANO_IV) {
                "el Keystore devolvió un IV de tamaño no válido"
            }
            iv to cifrador.doFinal(claro)
        } catch (e: ErrorPlataforma.Llavero) {
            throw e
        } catch (e: Exception) {
            throw errorDeKeystore(e, alLeer = false)
        }
    }

    private fun descifrar(clave: String, contenido: FormatoSecreto.Contenido): ByteArray {
        return try {
            val cifrador = Cipher.getInstance(TRANSFORMACION)
            cifrador.init(
                Cipher.DECRYPT_MODE,
                claveDelKeystore(),
                GCMParameterSpec(TAMANO_BITS_ETIQUETA, contenido.iv),
            )
            cifrador.updateAAD(clave.toByteArray(Charsets.UTF_8))
            cifrador.doFinal(contenido.cifrado)
        } catch (e: AEADBadTagException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_danado, clave),
                e,
            )
        } catch (e: ErrorPlataforma.Llavero) {
            throw e
        } catch (e: Exception) {
            throw errorDeKeystore(e, alLeer = true)
        }
    }

    private fun errorDeKeystore(error: Exception, alLeer: Boolean): ErrorPlataforma.Llavero {
        val mensaje = when (error) {
            is UserNotAuthenticatedException ->
                contexto.getString(R.string.plataforma_llavero_telefono_bloqueado)

            is KeyPermanentlyInvalidatedException, is UnrecoverableKeyException ->
                contexto.getString(R.string.plataforma_llavero_clave_invalidada)

            else -> contexto.getString(
                if (alLeer) {
                    R.string.plataforma_llavero_no_leido
                } else {
                    R.string.plataforma_llavero_no_guardado
                },
            )
        }
        return ErrorPlataforma.Llavero(mensaje, error)
    }

    /** Escritura atómica: temporal en el mismo directorio y `rename`. */
    private fun escribirAtomico(destino: File, datos: ByteArray) {
        if (!directorio.isDirectory && !directorio.mkdirs()) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_no_guardado)
            )
        }
        val temporal = File(directorio, ".${destino.name}.${UUID.randomUUID()}.tmp")
        try {
            FileOutputStream(temporal).use { salida ->
                salida.write(datos)
                salida.flush()
                salida.fd.sync()
            }
            if (!temporal.renameTo(destino)) {
                throw IOException("no se pudo renombrar ${temporal.name}")
            }
        } catch (e: IOException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_no_guardado),
                e,
            )
        } finally {
            if (temporal.exists()) temporal.delete()
        }
    }
}
