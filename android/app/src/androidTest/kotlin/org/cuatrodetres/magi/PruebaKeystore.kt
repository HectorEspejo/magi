package org.cuatrodetres.magi

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.cuatrodetres.magi.plataforma.ClaveMovil
import org.cuatrodetres.magi.plataforma.FormatoSecreto
import org.cuatrodetres.magi.plataforma.LlaveroKeystore
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Prueba de humo del Keystore real: llavero AES-GCM, firma P-256 y migración
 * del llavero antiguo (corrección del fallo de desbloqueo facial en Honor).
 */
@RunWith(AndroidJUnit4::class)
class PruebaKeystore {
    private val contexto = ApplicationProvider.getApplicationContext<Context>()
    private val llavero = LlaveroKeystore(contexto)

    @After
    fun limpiar() {
        llavero.borrarTodo()
    }

    @Test
    fun elLlaveroGuardaYLeeConElTelefonoDesbloqueado() {
        val secreto = "secreto-de-prueba".toByteArray()
        llavero.guardar("prueba", secreto)
        assertArrayEquals(secreto, llavero.leer("prueba"))
    }

    @Test
    fun laClaveDelMovilFirmaConElTelefonoDesbloqueado() {
        val clave = ClaveMovil(contexto)
        if (clave.estado() != ClaveMovil.Estado.LISTA) {
            clave.asegurarClave()
        }
        assertTrue(clave.firmar("datos-de-prueba".toByteArray()).isNotEmpty())
    }

    @Test
    fun migraLosSecretosDeLaClaveAntiguaSinPerderNada() {
        // Simula una instalación anterior: alias viejo con requisito de
        // desbloqueo y un secreto cifrado en la carpeta "secretos".
        llavero.borrarTodo()
        val secreto = "clave-de-sincronizacion-de-prueba".toByteArray()
        val nombre = "sincronizacion"
        val directorioViejo = File(contexto.filesDir, "secretos").apply { mkdirs() }
        val (iv, cifrado) = cifrarConClaveAntigua(nombre, secreto)
        File(directorioViejo, nombre + ".enc").writeBytes(
            FormatoSecreto.empaquetar(iv, cifrado),
        )

        val resultado = llavero.migrarSinBloqueo()
        assertEquals(LlaveroKeystore.ResultadoMigracion.MIGRADO, resultado)
        assertArrayEquals(secreto, llavero.leer(nombre))
        assertTrue(File(contexto.filesDir, "secretos-v2/.completa").isFile)
    }

    private fun cifrarConClaveAntigua(clave: String, claro: ByteArray): Pair<ByteArray, ByteArray> {
        val cifrador = Cipher.getInstance("AES/GCM/NoPadding")
        cifrador.init(Cipher.ENCRYPT_MODE, claveAntigua())
        cifrador.updateAAD(clave.toByteArray(Charsets.UTF_8))
        return cifrador.iv to cifrador.doFinal(claro)
    }

    private fun claveAntigua(): SecretKey {
        val almacen = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (almacen.getEntry("magi-secretos", null) as? KeyStore.SecretKeyEntry)
            ?.let { return it.secretKey }
        val generador = KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES,
            "AndroidKeyStore",
        )
        generador.init(
            KeyGenParameterSpec.Builder(
                "magi-secretos",
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .setUnlockedDeviceRequired(true)
                .build(),
        )
        return generador.generateKey()
    }
}
