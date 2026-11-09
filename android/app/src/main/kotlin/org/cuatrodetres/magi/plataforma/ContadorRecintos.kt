package org.cuatrodetres.magi.plataforma

/**
 * Cuenta recintos seguros anidados por anfitrión (una Activity).
 *
 * Varios composables pueden envolver contenido con FLAG_SECURE a la vez (por
 * ejemplo, un diálogo de contraseña sobre una pantalla con secretos). El flag
 * solo se aplica al entrar el primero y solo se retira al salir el último:
 * antes, al desmontarse el recinto interior se retiraba el flag aunque el
 * exterior siguiera vivo y la pantalla quedaba capturable.
 */
class ContadorRecintos<T> {

    private val profundidad = HashMap<T, Int>()

    /**
     * Entra en un recinto. Devuelve true si hay que aplicar el flag (es el
     * primer recinto de ese anfitrión).
     */
    @Synchronized
    fun entrar(anfitrion: T): Boolean {
        val actual = (profundidad[anfitrion] ?: 0) + 1
        profundidad[anfitrion] = actual
        return actual == 1
    }

    /**
     * Sale de un recinto. Devuelve true si hay que retirar el flag (era el
     * último recinto de ese anfitrión).
     */
    @Synchronized
    fun salir(anfitrion: T): Boolean {
        val actual = (profundidad[anfitrion] ?: 1) - 1
        if (actual <= 0) {
            profundidad.remove(anfitrion)
            return true
        }
        profundidad[anfitrion] = actual
        return false
    }

    /** Recintos abiertos ahora mismo para ese anfitrión (para pruebas). */
    @Synchronized
    fun cuantos(anfitrion: T): Int = profundidad[anfitrion] ?: 0
}
