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
 * - Un fichero por secreto, con escritura atómica (temporal + rename) y un
 *   formato con cabecera, IV y etiqueta GCM (FormatoSecreto). El nombre de la
 *   clave se ata como AAD: no se pueden intercambiar ficheros entre claves.
 * - La clave del Keystore NO lleva setUnlockedDeviceRequired: se comprobó en un
 *   Honor MagicOS que Android devuelve UserNotAuthenticatedException aunque el
 *   teléfono esté desbloqueado si se desbloqueó con la cara (fallo conocido de
 *   Android/OEM; Google issue 506989112). La protección equivalente la da el
 *   bloqueo propio de MAGI: el núcleo no arranca hasta que el usuario desbloquea
 *   la app con biometría fuerte o PIN.
 * - Compatibilidad: los secretos creados por versiones anteriores se migran a
 *   secretos-v2 con una clave nueva sin ese requisito. La migración es atómica
 *   por fichero y deja intacta la carpeta vieja hasta completarse; si Android
 *   no reconoce el desbloqueo actual, se reintenta en el siguiente uso.
 * - Nunca se escribe nada en logcat ni en DataStore, y los bytes en claro no se
 *   copian a ningún sitio.
 */
class LlaveroKeystore(private val contexto: Context) : LlaveroMovil {

    companion object {
        /** Alias de la clave AES antigua (informe §9, versiones anteriores). */
        const val ALIAS_CLAVE = "magi-secretos"

        /** Alias de la clave AES actual, sin requisito de dispositivo desbloqueado. */
        const val ALIAS_CLAVE_ACTUAL = "magi-secretos-v2"

        private const val PROVEEDOR = "AndroidKeyStore"
        private const val TRANSFORMACION = "AES/GCM/NoPadding"
        private const val TAMANO_CLAVE = 256
        private const val TAMANO_BITS_ETIQUETA = 128
        private const val DIRECTORIO_VIEJO = "secretos"
        private const val DIRECTORIO_ACTUAL = "secretos-v2"
        private const val MARCADOR = ".completa"
        private const val EXTENSION = ".enc"
    }

    /** Resultado de la migración de compatibilidad del llavero. */
    enum class ResultadoMigracion {
        /** No había nada que migrar o ya estaba hecha. */
        SIN_NOVEDAD,

        /** Secretos viejos re-cifrados con la clave nueva. */
        MIGRADO,

        /** Android no reconoce el desbloqueo; se reintentará (PIN una vez). */
        PENDIENTE_PIN,

        /** No se pudieron leer los secretos viejos (clave perdida o dañada). */
        ERROR,
    }

    private val cerrojo = Any()

    @Volatile
    private var pendientePin = false

    /** Si la última migración quedó pendiente de un desbloqueo con PIN. */
    fun pendienteDesbloqueoPin(): Boolean = pendientePin

    private val directorioViejo: File get() = File(contexto.filesDir, DIRECTORIO_VIEJO)
    private val directorioActual: File get() = File(contexto.filesDir, DIRECTORIO_ACTUAL)

    private fun migracionHecha(): Boolean = File(directorioActual, MARCADOR).isFile

    private fun directorioEnUso(): File =
        if (migracionHecha()) directorioActual else directorioViejo

    private fun aliasEnUso(): String =
        if (migracionHecha()) ALIAS_CLAVE_ACTUAL else ALIAS_CLAVE

    /**
     * Migra el llavero antiguo a la clave nueva si hace falta. Devuelve el
     * resultado y deja pendienteDesbloqueoPin al día.
     */
    fun migrarSinBloqueo(): ResultadoMigracion = synchronized(cerrojo) {
        asegurarCompatibilidad()
    }

    /** Devuelve el secreto descifrado, o null si no existe. */
    override fun leer(clave: String): ByteArray? = synchronized(cerrojo) {
        asegurarCompatibilidad()
        val fichero = ficheroDe(clave, directorioEnUso())
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
        descifrarConAlias(aliasEnUso(), clave, contenido)
    }

    /** Guarda (o sustituye) un secreto. */
    override fun guardar(clave: String, secreto: ByteArray) {
        synchronized(cerrojo) {
            asegurarCompatibilidad()
            val directorio = directorioEnUso()
            val fichero = ficheroDe(clave, directorio)
            val (iv, cifrado) = cifrarConAlias(aliasEnUso(), clave, secreto)
            escribirAtomico(directorio, fichero, FormatoSecreto.empaquetar(iv, cifrado))
        }
    }

    /** Borra un secreto; no falla si no existía. */
    override fun borrar(clave: String) {
        synchronized(cerrojo) {
            asegurarCompatibilidad()
            val fichero = ficheroDe(clave, directorioEnUso())
            if (fichero.exists() && !fichero.delete()) {
                throw ErrorPlataforma.Llavero(
                    contexto.getString(R.string.plataforma_llavero_no_borrado, clave)
                )
            }
        }
    }

    /** Si hay un secreto guardado con esa clave. */
    fun existe(clave: String): Boolean = synchronized(cerrojo) {
        asegurarCompatibilidad()
        ficheroDe(clave, directorioEnUso()).isFile
    }

    /**
     * Borra todos los secretos y las claves del Keystore (para «borrar todos
     * los datos»). Después se vuelve a crear la clave actual al guardar.
     */
    fun borrarTodo() {
        synchronized(cerrojo) {
            directorioViejo.deleteRecursively()
            directorioActual.deleteRecursively()
            pendientePin = false
            try {
                val almacen = almacenDeClaves()
                almacen.deleteEntry(ALIAS_CLAVE)
                almacen.deleteEntry(ALIAS_CLAVE_ACTUAL)
            } catch (_: GeneralSecurityException) {
                // Si las claves ya no estaban, no hay nada más que hacer.
            }
        }
    }

    // -----------------------------------------------------------------------
    // Compatibilidad (corrección del fallo de desbloqueo facial)
    // -----------------------------------------------------------------------

    private fun asegurarCompatibilidad(): ResultadoMigracion {
        if (migracionHecha()) {
            limpiarRestosViejos()
            pendientePin = false
            return ResultadoMigracion.SIN_NOVEDAD
        }
        val resultado = intentarMigrar()
        pendientePin = resultado == ResultadoMigracion.PENDIENTE_PIN
        return resultado
    }

    /**
     * Pasa los secretos viejos a secretos-v2 con la clave nueva. La carpeta
     * vieja no se toca hasta que la nueva está completa y marcada, así que un
     * cierre a mitad no pierde nada: se repite en el siguiente uso.
     */
    private fun intentarMigrar(): ResultadoMigracion {
        val ficheros = directorioViejo
            .listFiles()
            ?.filter { it.isFile && it.name.endsWith(EXTENSION) }
            ?: emptyList()
        if (ficheros.isNotEmpty() && !aliasExiste(ALIAS_CLAVE)) {
            return ResultadoMigracion.ERROR
        }
        val claros = LinkedHashMap<String, ByteArray>()
        for (fichero in ficheros) {
            val contenido = try {
                FormatoSecreto.desempaquetar(fichero.readBytes())
            } catch (e: Exception) {
                return ResultadoMigracion.ERROR
            }
            val nombre = fichero.name.removeSuffix(EXTENSION)
            val claro = try {
                descifrarConAlias(ALIAS_CLAVE, nombre, contenido)
            } catch (e: UserNotAuthenticatedException) {
                return ResultadoMigracion.PENDIENTE_PIN
            } catch (e: GeneralSecurityException) {
                return ResultadoMigracion.PENDIENTE_PIN
            } catch (e: RuntimeException) {
                return ResultadoMigracion.PENDIENTE_PIN
            }
            claros[fichero.name] = claro
        }
        return try {
            prepararDirectorioActual(claros)
            runCatching { almacenDeClaves().deleteEntry(ALIAS_CLAVE) }
            limpiarRestosViejos()
            ResultadoMigracion.MIGRADO
        } catch (e: Exception) {
            ResultadoMigracion.ERROR
        }
    }

    /** Crea la clave nueva, escribe los secretos y deja el marcador al final. */
    private fun prepararDirectorioActual(claros: Map<String, ByteArray>) {
        val directorio = directorioActual
        if (directorio.exists()) {
            directorio.listFiles()?.forEach { if (it.isFile) it.delete() }
        }
        if (!directorio.isDirectory && !directorio.mkdirs()) {
            throw IOException("no se pudo crear el directorio del llavero")
        }
        for ((nombreFichero, claro) in claros) {
            val nombre = nombreFichero.removeSuffix(EXTENSION)
            val (iv, cifrado) = cifrarConAlias(ALIAS_CLAVE_ACTUAL, nombre, claro)
            val destino = File(directorio, nombre + EXTENSION)
            escribirAtomico(directorio, destino, FormatoSecreto.empaquetar(iv, cifrado))
        }
        File(directorio, MARCADOR).writeText("1")
    }

    /** Borra la carpeta y la clave viejas en cuanto la migración está hecha. */
    private fun limpiarRestosViejos() {
        if (directorioViejo.exists()) {
            directorioViejo.deleteRecursively()
        }
        if (aliasExiste(ALIAS_CLAVE)) {
            runCatching { almacenDeClaves().deleteEntry(ALIAS_CLAVE) }
        }
    }

    // -----------------------------------------------------------------------
    // Interno
    // -----------------------------------------------------------------------

    private fun ficheroDe(clave: String, directorio: File): File {
        val nombre = ReglasLlavero.nombreDeFichero(clave)
            ?: throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_clave_invalida, clave)
            )
        return File(directorio, nombre)
    }

    private fun almacenDeClaves(): KeyStore =
        KeyStore.getInstance(PROVEEDOR).apply { load(null) }

    private fun aliasExiste(alias: String): Boolean = try {
        almacenDeClaves().containsAlias(alias)
    } catch (e: GeneralSecurityException) {
        false
    }

    /** Clave AES de un alias: la recupera o la crea la primera vez. */
    private fun claveDeAlias(alias: String): SecretKey {
        val almacen = try {
            almacenDeClaves()
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_keystore_no_disponible),
                e,
            )
        }
        val entrada = try {
            almacen.getEntry(alias, null)
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
                    alias,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(TAMANO_CLAVE)
                    .setRandomizedEncryptionRequired(true)
                    .build(),
            )
            generador.generateKey()
        } catch (e: Exception) {
            throw errorDeKeystore(e, alLeer = false)
        }
    }

    /** Clave existente; no la crea (para leer los secretos viejos). */
    private fun claveExistente(alias: String): SecretKey {
        val almacen = try {
            almacenDeClaves()
        } catch (e: GeneralSecurityException) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_keystore_no_disponible),
                e,
            )
        }
        val entrada = try {
            almacen.getEntry(alias, null)
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
        throw ErrorPlataforma.Llavero(
            contexto.getString(R.string.plataforma_llavero_clave_invalidada)
        )
    }

    private fun cifrarConAlias(
        alias: String,
        clave: String,
        claro: ByteArray,
    ): Pair<ByteArray, ByteArray> {
        return try {
            val cifrador = Cipher.getInstance(TRANSFORMACION)
            cifrador.init(Cipher.ENCRYPT_MODE, claveDeAlias(alias))
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

    private fun descifrarConAlias(
        alias: String,
        clave: String,
        contenido: FormatoSecreto.Contenido,
    ): ByteArray {
        return try {
            val cifrador = Cipher.getInstance(TRANSFORMACION)
            cifrador.init(
                Cipher.DECRYPT_MODE,
                claveExistente(alias),
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

    /** Escritura atómica: temporal en el mismo directorio y rename. */
    private fun escribirAtomico(directorio: File, destino: File, datos: ByteArray) {
        if (!directorio.isDirectory && !directorio.mkdirs()) {
            throw ErrorPlataforma.Llavero(
                contexto.getString(R.string.plataforma_llavero_no_guardado)
            )
        }
        val temporal = File(
            directorio,
            "." + destino.name + "." + UUID.randomUUID().toString() + ".tmp",
        )
        try {
            FileOutputStream(temporal).use { salida ->
                salida.write(datos)
                salida.flush()
                salida.fd.sync()
            }
            if (!temporal.renameTo(destino)) {
                throw IOException("no se pudo renombrar el secreto temporal")
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
