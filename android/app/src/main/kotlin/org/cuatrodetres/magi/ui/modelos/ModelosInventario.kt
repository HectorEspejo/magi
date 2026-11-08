package org.cuatrodetres.magi.ui.modelos

/**
 * Inventario de solo lectura (informe §6.1 y §6.2).
 *
 * El móvil es lector: la pasada en modo lector escribe estas filas y la
 * interfaz solo las enseña. El plegado de los grupos sí es un campo propio del
 * móvil (GRUPOS.plegado).
 */

/**
 * Tipo de host del inventario (Fase 12): SSH o un escritorio remoto (RDP, VNC o
 * voxl). Los escritorios se enseñan en el móvil, pero solo se abren en el
 * ordenador con MAGI de escritorio: el teléfono no conecta a ellos ni les crea
 * túneles.
 */
enum class TipoHostVista {
    SSH,
    RDP,
    VNC,
    VOXL;

    /** Etiqueta corta de la interfaz (es el nombre técnico del protocolo). */
    val etiqueta: String
        get() = when (this) {
            SSH -> "SSH"
            RDP -> "RDP"
            VNC -> "VNC"
            VOXL -> "voxl"
        }

    /** Un escritorio no se abre desde el móvil. */
    val esEscritorio: Boolean get() = this != SSH
}

/** Estado de un host en la lista (glifos de §6.1). */
enum class EstadoHostVista {
    /** Sin sesión: ○. */
    SIN_SESION,

    /** Conectando: ◐. */
    CONECTANDO,

    /** Con una o más sesiones abiertas: ●N. */
    CON_SESIONES,

    /** Último intento con error: ✕. */
    ERROR,
}

/** Fila de host de la pantalla Hosts. */
data class HostVista(
    val uuid: String,
    val nombre: String,
    val direccion: String,
    val puerto: Int,
    val tipo: TipoHostVista = TipoHostVista.SSH,
    val usuario: String,
    val grupoUuid: String? = null,
    val etiquetas: List<String> = emptyList(),
    val estado: EstadoHostVista = EstadoHostVista.SIN_SESION,
    val sesionesAbiertas: Int = 0,
    val ultimoError: String? = null,
    val ultimaConexionEn: Long? = null,
)

/** Grupo del inventario con su número de hosts y el plegado del móvil. */
data class GrupoVista(
    val uuid: String,
    val nombre: String,
    val numHosts: Int = 0,
    val plegado: Boolean = false,
)

/** Inventario completo tal como lo devuelve el núcleo (ya filtrado si se pidió). */
data class InventarioVista(
    val grupos: List<GrupoVista> = emptyList(),
    val hosts: List<HostVista> = emptyList(),
) {
    val vacio: Boolean get() = grupos.isEmpty() && hosts.isEmpty()
}

/** Bloque de la lista de Hosts: un grupo y sus hosts (o «sin grupo»). */
data class BloqueInventarioVista(
    val grupo: GrupoVista?,
    val hosts: List<HostVista>,
) {
    /** Clave estable para LazyColumn. */
    val clave: String get() = grupo?.uuid ?: SIN_GRUPO

    companion object {
        const val SIN_GRUPO: String = "sin-grupo"
    }
}

/** Huella conocida de un host en el known_hosts privado del móvil. */
data class HuellaHostVista(
    val tipo: String,
    val sha256: String,
)

/** Ficha de host de solo lectura (todos los campos de §6.2). */
data class FichaHostVista(
    val uuid: String,
    val nombre: String,
    val direccion: String,
    val puerto: Int,
    val tipo: TipoHostVista = TipoHostVista.SSH,
    val usuario: String,
    val grupoNombre: String? = null,
    val etiquetas: List<String> = emptyList(),
    val saltoNombre: String? = null,
    /** Keepalive del inventario; null = el móvil aplica 30 s (D180). */
    val keepaliveSegundos: Int? = null,
    /** Identidad del escritorio (auto, agente:..., fichero:...), solo informativa. */
    val identidadEscritorio: String? = null,
    val huellas: List<HuellaHostVista> = emptyList(),
    val tieneContrasena: Boolean = false,
)

/** Si el host es un escritorio: el móvil lo enseña pero no lo abre. */
fun esEscritorio(host: HostVista): Boolean = host.tipo.esEscritorio

/** Dirección de la ficha: usuario@dirección:puerto (informe §6.2). */
fun textoDireccion(host: HostVista): String =
    host.usuario + "@" + host.direccion + ":" + host.puerto

/** Dirección de la ficha con los datos de la ficha. */
fun textoDireccion(ficha: FichaHostVista): String =
    ficha.usuario + "@" + ficha.direccion + ":" + ficha.puerto

/** Keepalive efectivo: el del inventario o los 30 s del móvil (D180). */
fun keepaliveEfectivo(segundos: Int?): Int = segundos ?: 30

/** Si el keepalive es el que aplica el móvil por defecto (D180). */
fun keepaliveEsDelMovil(segundos: Int?): Boolean = segundos == null

/** Glifo del estado del host; el texto va aparte para TalkBack (T5). */
fun glifoEstadoHost(estado: EstadoHostVista, sesionesAbiertas: Int): String = when (estado) {
    EstadoHostVista.SIN_SESION -> "○"
    EstadoHostVista.CONECTANDO -> "◐"
    EstadoHostVista.CON_SESIONES -> "●" + sesionesAbiertas.coerceAtLeast(1)
    EstadoHostVista.ERROR -> "✕"
}

/** Primera letra del tipo de huella, para resumirla en la ficha. */
fun tipoHuellaCorto(tipo: String): String = tipo.substringAfterLast("-").ifEmpty { tipo }

/**
 * Aplica las sesiones en vivo a la lista de hosts: el glifo ●N, ◐ y ✕ sale de
 * las sesiones abiertas del móvil, no del campo `ultimo_estado` de la base.
 */
fun inventarioConSesiones(inventario: InventarioVista, sesiones: List<SesionVista>): InventarioVista {
    if (sesiones.isEmpty()) return inventario
    val abiertas = sesiones.count { it.estado == EstadoSesionVista.ABIERTA }
    val porHost = sesiones.groupBy { it.hostUuid }
    if (abiertas == 0 && porHost.isEmpty()) return inventario
    return inventario.copy(
        hosts = inventario.hosts.map { host ->
            val delHost = porHost[host.uuid].orEmpty()
            val conSesion = delHost.count { it.estado == EstadoSesionVista.ABIERTA }
            val abriendo = delHost.count { it.estado == EstadoSesionVista.ABRIENDO }
            val estado = when {
                conSesion > 0 -> EstadoHostVista.CON_SESIONES
                abriendo > 0 -> EstadoHostVista.CONECTANDO
                else -> host.estado
            }
            host.copy(estado = estado, sesionesAbiertas = conSesion)
        },
    )
}

/**
 * Agrupa los hosts como en el mockup de §6.1: grupos por nombre (sin distinguir
 * mayúsculas), hosts por nombre dentro del grupo y «sin grupo» siempre al final.
 * Los grupos sin hosts no se pintan.
 */
fun agruparInventario(inventario: InventarioVista): List<BloqueInventarioVista> {
    val hostsPorGrupo = inventario.hosts.groupBy { it.grupoUuid }
    val bloques = inventario.grupos
        .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
        .mapNotNull { grupo ->
            val hosts = hostsPorGrupo[grupo.uuid].orEmpty()
                .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
            if (hosts.isEmpty()) null else BloqueInventarioVista(grupo, hosts)
        }
    val sinGrupo = hostsPorGrupo[null].orEmpty()
        .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
    return if (sinGrupo.isEmpty()) bloques else bloques + BloqueInventarioVista(null, sinGrupo)
}
