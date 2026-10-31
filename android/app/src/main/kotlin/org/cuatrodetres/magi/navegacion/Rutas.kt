package org.cuatrodetres.magi.navegacion

/**
 * Rutas de la app (informe «Mapa de navegación», §6).
 *
 * La app no usa Navigation Compose: con cuatro destinos y un detalle, un estado
 * propio (destino + detalle) da lo mismo, conserva la sesión visible al plegar
 * o desplegar y se prueba en la JVM. La barra inferior o el riel eligen
 * [org.cuatrodetres.magi.navegacion.Destino]; el detalle pinta ficha, terminal
 * o una sección de Ajustes en el segundo panel.
 */

/** Secciones de Ajustes (§6 y checklist de interfaz). */
enum class SeccionAjustes {
    SINCRONIZACION,
    CLAVE_MOVIL,
    TERMINAL,
    BATERIA,
    BORRAR_DATOS,
    ACERCA_DE,
    LICENCIAS,
}

/** Lo que se pinta en el panel de detalle (o a pantalla completa en compacta). */
sealed interface DetalleVista {
    data class Ficha(val hostUuid: String) : DetalleVista
    data class Terminal(val sesionId: Long) : DetalleVista
    data class Ajustes(val seccion: SeccionAjustes) : DetalleVista

    /** Clave estable para conservar el panel entre recomposiciones. */
    val clave: String
        get() = when (this) {
            is Ficha -> "ficha:" + hostUuid
            is Terminal -> "terminal:" + sesionId
            is Ajustes -> "ajustes:" + seccion.name
        }
}
