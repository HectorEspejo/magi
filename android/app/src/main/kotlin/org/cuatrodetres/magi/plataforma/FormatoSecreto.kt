package org.cuatrodetres.magi.plataforma

/**
 * Formato en disco de un secreto del llavero: cabecera, IV y texto cifrado con
 * AES-256-GCM. El AAD no viaja en el fichero: se ata al nombre de la clave al
 * descifrar, para que no se puedan intercambiar secretos entre claves.
 *
 * Puro (sin Android) para poder probarlo en la JVM.
 */
object FormatoSecreto {

    /** Bytes mágicos: distinguen un fichero del llavero de cualquier otro. */
    private val MAGICO = byteArrayOf(0x4D, 0x41, 0x47, 0x49, 0x4C, 0x4C) // "MAGILL"

    const val VERSION: Int = 1
    const val TAMANO_IV: Int = 12
    const val TAMANO_ETIQUETA: Int = 16

    data class Contenido(val version: Int, val iv: ByteArray, val cifrado: ByteArray)

    fun empaquetar(iv: ByteArray, cifrado: ByteArray): ByteArray {
        require(iv.size == TAMANO_IV) { "el IV debe medir $TAMANO_IV bytes" }
        val salida = ByteArray(MAGICO.size + 2 + iv.size + cifrado.size)
        var posicion = 0
        MAGICO.copyInto(salida, posicion)
        posicion += MAGICO.size
        salida[posicion] = VERSION.toByte()
        posicion += 1
        salida[posicion] = iv.size.toByte()
        posicion += 1
        iv.copyInto(salida, posicion)
        posicion += iv.size
        cifrado.copyInto(salida, posicion)
        return salida
    }

    /** Separa la cabecera; lanza [IllegalArgumentException] si no es válida. */
    fun desempaquetar(datos: ByteArray): Contenido {
        val minimo = MAGICO.size + 2 + TAMANO_IV + TAMANO_ETIQUETA
        require(datos.size >= minimo) { "secreto del llavero incompleto" }
        var posicion = 0
        for (esperado in MAGICO) {
            require(datos[posicion] == esperado) { "no es un secreto del llavero" }
            posicion += 1
        }
        val version = datos[posicion].toInt() and 0xFF
        posicion += 1
        require(version == VERSION) { "versión de secreto no conocida: $version" }
        val tamanoIv = datos[posicion].toInt() and 0xFF
        posicion += 1
        require(tamanoIv == TAMANO_IV) { "IV de tamaño no válido: $tamanoIv" }
        require(datos.size >= posicion + tamanoIv + TAMANO_ETIQUETA) {
            "secreto del llavero incompleto"
        }
        val iv = datos.copyOfRange(posicion, posicion + tamanoIv)
        posicion += tamanoIv
        val cifrado = datos.copyOfRange(posicion, datos.size)
        return Contenido(version, iv, cifrado)
    }
}
