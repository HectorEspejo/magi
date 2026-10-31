package org.cuatrodetres.magi.plataforma

/**
 * Nombres de clave válidos para el llavero. Los secretos de MAGI son
 * `sincronizacion`, `sincronizacion_anterior` y el uuid de cada host, pero el
 * llavero no confía en quien lo llame: un nombre con barras, `..` o caracteres
 * de control podría salirse de su carpeta.
 */
object ReglasLlavero {

    private val PATRON = Regex("^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$")

    fun claveValida(clave: String): Boolean = PATRON.matches(clave)

    /** Nombre del fichero cifrado, o `null` si la clave no vale. */
    fun nombreDeFichero(clave: String): String? =
        if (claveValida(clave)) "$clave.enc" else null
}
