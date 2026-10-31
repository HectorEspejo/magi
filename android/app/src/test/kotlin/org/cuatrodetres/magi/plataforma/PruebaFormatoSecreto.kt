package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/** Formato en disco de los secretos del llavero, probado en la JVM. */
class PruebaFormatoSecreto {

    private fun ivDePrueba(): ByteArray = ByteArray(FormatoSecreto.TAMANO_IV) { (it + 1).toByte() }

    @Test
    fun idaYVuelta() {
        val iv = ivDePrueba()
        val cifrado = "secreto de prueba".toByteArray()
        val datos = FormatoSecreto.empaquetar(iv, cifrado)
        val contenido = FormatoSecreto.desempaquetar(datos)
        assertEquals(FormatoSecreto.VERSION, contenido.version)
        assertArrayEquals(iv, contenido.iv)
        assertArrayEquals(cifrado, contenido.cifrado)
    }

    @Test
    fun rechazaDatosCortos() {
        assertThrows(IllegalArgumentException::class.java) {
            FormatoSecreto.desempaquetar(ByteArray(4))
        }
    }

    @Test
    fun rechazaCabeceraAjena() {
        val datos = FormatoSecreto.empaquetar(ivDePrueba(), ByteArray(FormatoSecreto.TAMANO_ETIQUETA))
        datos[0] = 'X'.code.toByte()
        assertThrows(IllegalArgumentException::class.java) {
            FormatoSecreto.desempaquetar(datos)
        }
    }

    @Test
    fun rechazaVersionDesconocida() {
        val datos = FormatoSecreto.empaquetar(ivDePrueba(), ByteArray(FormatoSecreto.TAMANO_ETIQUETA))
        datos[6] = 99
        assertThrows(IllegalArgumentException::class.java) {
            FormatoSecreto.desempaquetar(datos)
        }
    }

    @Test
    fun rechazaIvConTamanoInesperado() {
        val datos = FormatoSecreto.empaquetar(ivDePrueba(), ByteArray(FormatoSecreto.TAMANO_ETIQUETA))
        datos[7] = 16
        assertThrows(IllegalArgumentException::class.java) {
            FormatoSecreto.desempaquetar(datos)
        }
    }

    @Test
    fun exigeIvDeDoceBytesAlEmpaquetar() {
        assertThrows(IllegalArgumentException::class.java) {
            FormatoSecreto.empaquetar(ByteArray(8), ByteArray(FormatoSecreto.TAMANO_ETIQUETA))
        }
    }
}
