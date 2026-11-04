package org.cuatrodetres.magi.nucleo

import org.cuatrodetres.magi.plataforma.ErrorPlataforma
import org.cuatrodetres.magi.plataforma.CarpetaMovil as CarpetaPlataforma
import org.cuatrodetres.magi.plataforma.FirmanteMovil as FirmantePlataforma
import org.cuatrodetres.magi.plataforma.LlaveroMovil as LlaveroPlataforma

/**
 * Adaptadores entre la capa de plataforma (Kotlin puro, con sus propias
 * interfaces) y las interfaces foreign que genera UniFFI para el núcleo Rust
 * (paquete de este mismo módulo).
 *
 * Código fino y sin lógica: convierte tipos, respeta el tope de `maximo` y
 * traduce [ErrorPlataforma] a [ErrorNucleo] para que el núcleo reciba el motivo
 * en la misma forma que el resto de sus errores.
 */

/** Traduce un error de plataforma al error único del núcleo. */
fun ErrorPlataforma.aNucleo(): ErrorNucleo {
    val detalle = message ?: "error de la plataforma Android"
    return when (this) {
        is ErrorPlataforma.Carpeta -> ErrorNucleo.Carpeta(detalle)
        is ErrorPlataforma.Llavero -> ErrorNucleo.Llavero(detalle)
        is ErrorPlataforma.Firmante -> ErrorNucleo.Firmante(detalle)
        is ErrorPlataforma.General -> ErrorNucleo.General(detalle)
    }
}

/** CarpetaMovil del núcleo sobre la carpeta SAF de la app. */
class CarpetaMovilAdaptada(
    private val carpeta: CarpetaPlataforma,
) : CarpetaMovil {

    override fun listar(): List<FicheroCarpeta> = try {
        carpeta.listar().map { fichero ->
            FicheroCarpeta(
                nombre = fichero.nombre,
                tamano = fichero.tamano.coerceAtLeast(0L).toULong(),
                modificadoEn = fichero.modificadoEn,
            )
        }
    } catch (e: ErrorPlataforma) {
        throw e.aNucleo()
    }

    override fun leer(nombre: String, maximo: ULong): ByteArray {
        val tope = if (maximo > Long.MAX_VALUE.toULong()) Long.MAX_VALUE else maximo.toLong()
        return try {
            carpeta.leer(nombre, tope)
        } catch (e: ErrorPlataforma) {
            throw e.aNucleo()
        }
    }
}

/** LlaveroMovil del núcleo sobre el llavero AES-GCM del Keystore. */
class LlaveroMovilAdaptado(
    private val llavero: LlaveroPlataforma,
) : LlaveroMovil {

    override fun leer(clave: String): ByteArray? = try {
        llavero.leer(clave)
    } catch (e: ErrorPlataforma) {
        throw e.aNucleo()
    }

    override fun guardar(clave: String, secreto: ByteArray) {
        try {
            llavero.guardar(clave, secreto)
        } catch (e: ErrorPlataforma) {
            throw e.aNucleo()
        }
    }

    override fun borrar(clave: String) {
        try {
            llavero.borrar(clave)
        } catch (e: ErrorPlataforma) {
            throw e.aNucleo()
        }
    }
}

/** FirmanteMovil del núcleo sobre la clave ECDSA P-256 del Keystore. */
class FirmanteMovilAdaptado(
    private val firmante: FirmantePlataforma,
) : FirmanteMovil {

    override fun clavePublicaX509(): ByteArray = try {
        firmante.clavePublicaX509()
    } catch (e: ErrorPlataforma) {
        throw e.aNucleo()
    }

    override fun firmar(datos: ByteArray): ByteArray = try {
        firmante.firmar(datos)
    } catch (e: ErrorPlataforma) {
        throw e.aNucleo()
    }

    override fun olvidarClave() {
        try {
            firmante.olvidarClave()
        } catch (e: ErrorPlataforma) {
            throw e.aNucleo()
        }
    }
}
