package org.cuatrodetres.magi.plataforma

/** Reglas del servicio en primer plano (informe §4.6 y diagrama de §3). */
object ReglasServicio {

    /** Nunca se publica un número negativo en la notificación. */
    fun sanear(cantidad: Int): Int = cantidad.coerceAtLeast(0)

    /** El servicio vive mientras haya alguna sesión o algún túnel. */
    fun debeEstarActivo(sesiones: Int, tuneles: Int): Boolean =
        sanear(sesiones) > 0 || sanear(tuneles) > 0
}
