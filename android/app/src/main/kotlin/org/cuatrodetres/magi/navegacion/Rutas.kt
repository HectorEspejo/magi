package org.cuatrodetres.magi.navegacion

import org.cuatrodetres.magi.ui.modelos.ObjetivoVista

/**
 * Rutas de la app (informe «Mapa de navegación», §6).
 *
 * La app no usa Navigation Compose: con cinco destinos y un detalle, un estado
 * propio (destino + sección de Más + detalle) da lo mismo, conserva la sesión
 * visible al plegar o desplegar y se prueba en la JVM. La barra inferior o el
 * riel eligen [Destino]; el detalle pinta ficha, terminal, servicios, logs,
 * snippets, resultados, registro o una sección de Ajustes en el segundo panel.
 */

/** Secciones de Ajustes (§5.4 y §6). */
enum class SeccionAjustes {
    SINCRONIZACION,
    CLAVE_MOVIL,
    TERMINAL,
    FLOTA,
    SERVICIOS,
    DELIBERACION,
    BATERIA,
    BORRAR_DATOS,
    ACERCA_DE,
    LICENCIAS,
}

/** Secciones que agrupa el destino Más (D265). */
enum class SeccionMas {
    SNIPPETS,
    RESULTADOS,
    REGISTRO,
    AJUSTES,
}

/** Lo que se pinta en el panel de detalle (o a pantalla completa en compacta). */
sealed interface DetalleVista {
    data class Ficha(val hostUuid: String) : DetalleVista
    data class Terminal(val sesionId: Long) : DetalleVista
    data class Ajustes(val seccion: SeccionAjustes) : DetalleVista

    /** Servicios de un host (lista de unidades, timers y contenedores). */
    data class ServiciosHost(val hostUuid: String) : DetalleVista

    /** Detalle de un servicio, timer, contenedor o proyecto. */
    data class Servicio(val hostUuid: String, val objetivo: ObjetivoVista) : DetalleVista

    /** Panel de logs de un objetivo. */
    data class Logs(val hostUuid: String, val objetivo: ObjetivoVista) : DetalleVista

    /** Ejecución de un snippet: preparación y resultado. */
    data class Snippet(val uuid: String) : DetalleVista

    /** Detalle de una ejecución de snippets. */
    data class Ejecucion(val id: Long) : DetalleVista

    /** Detalle de una entrada del registro. */
    data class EntradaRegistro(val id: Long) : DetalleVista

    /** Clave estable para conservar el panel entre recomposiciones. */
    val clave: String
        get() = when (this) {
            is Ficha -> "ficha:" + hostUuid
            is Terminal -> "terminal:" + sesionId
            is Ajustes -> "ajustes:" + seccion.name
            is ServiciosHost -> "servicios-host:" + hostUuid
            is Servicio -> "servicio:" + hostUuid + ":" + objetivo.tipo.name + ":" + objetivo.nombre
            is Logs -> "logs:" + hostUuid + ":" + objetivo.tipo.name + ":" + objetivo.nombre
            is Snippet -> "snippet:" + uuid
            is Ejecucion -> "ejecucion:" + id
            is EntradaRegistro -> "registro:" + id
        }
}
