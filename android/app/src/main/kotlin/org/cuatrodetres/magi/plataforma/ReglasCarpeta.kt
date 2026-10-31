package org.cuatrodetres.magi.plataforma

/**
 * Reglas puras de la carpeta compartida (T81): qué nombres son ficheros de
 * equipo de MAGI y hasta qué tamaño se aceptan. Sin Android, para probarlas en
 * la JVM.
 */
object ReglasCarpeta {

    /** Tope compartido con el núcleo (`carpeta::TAMANO_MAXIMO`, T81). */
    const val TAMANO_MAXIMO: Long = 16L * 1024 * 1024

    /**
     * `magi-<uuid>.age` canónico: uuid en minúsculas y con guiones, sin sufijos
     * de Syncthing, temporales ni subcarpetas. Mismo criterio que
     * `uuid_de_nombre` del núcleo.
     */
    private val PATRON_NOMBRE = Regex(
        "^magi-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\\.age"
    )

    fun nombreValido(nombre: String): Boolean = PATRON_NOMBRE.matches(nombre)

    fun tamanoValido(tamano: Long): Boolean = tamano in 0L..TAMANO_MAXIMO

    fun esAceptable(nombre: String, tamano: Long): Boolean =
        nombreValido(nombre) && tamanoValido(tamano)

    /** Filtra una lista cruda de SAF dejando solo lo que la pasada puede leer. */
    fun filtrar(ficheros: List<FicheroCarpeta>): List<FicheroCarpeta> =
        ficheros.filter { esAceptable(it.nombre, it.tamano) }.sortedBy { it.nombre }
}
