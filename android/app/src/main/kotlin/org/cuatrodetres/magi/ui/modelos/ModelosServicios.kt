package org.cuatrodetres.magi.ui.modelos

/**
 * Servicios de un host (Fase 14, informe §5.1, §6.3 y §6.4): espejo de los
 * tipos del núcleo de la Fase 13 más lo que la app necesita para pintarlos.
 *
 * Kotlin no decide qué acción se permite, qué exige deliberación ni cómo se
 * lee la salida de un host: el núcleo lo calcula y aquí solo hay modelos de
 * presentación y ayudas de ordenación. La app solo ofrece las acciones que
 * vienen en UnidadVista.acciones, TimerVista.acciones, ContenedorVista.acciones
 * y ProyectoVista.acciones.
 */

/** Tipo de objetivo sobre el que actúa la vista o la acción. */
enum class TipoObjetivoVista(
    /** Nombre técnico del tipo, como en el núcleo. */
    val texto: String,
) {
    UNIDAD("unidad"),
    TIMER("timer"),
    CONTENEDOR("contenedor"),
    PROYECTO("proyecto"),
    ;

    /** ¿Se maneja en el host con systemctl (unidad o timer)? */
    val esSystemd: Boolean get() = this == UNIDAD || this == TIMER
}

/** Objetivo concreto: tipo y nombre (unidad, timer, contenedor o proyecto). */
data class ObjetivoVista(
    val tipo: TipoObjetivoVista = TipoObjetivoVista.UNIDAD,
    val nombre: String = "",
) {
    /** «unidad app.service» o «contenedor web-app-1». */
    val textoCompleto: String get() = tipo.texto + " " + nombre

    /** Título de una pestaña con orden: «web-01 · app.service». */
    fun titulo(hostNombre: String): String = hostNombre + " · " + nombre
}

/** Acción de la vista sobre un servicio, un timer, un contenedor o un proyecto. */
enum class AccionServicioVista(
    /** Infinitivo: «reiniciar». */
    val texto: String,
    /** Pasado para el registro y el resultado: «reiniciado». */
    val participio: String,
    /** Gerundio para una acción en curso: «reiniciando». */
    val gerundio: String,
) {
    ARRANCAR("arrancar", "arrancado", "arrancando"),
    PARAR("parar", "parado", "parando"),
    REINICIAR("reiniciar", "reiniciado", "reiniciando"),
    RECARGAR("recargar", "recargado", "recargando"),
    ACTIVAR("activar", "activado", "activando"),
    DESACTIVAR("desactivar", "desactivado", "desactivando"),
    LANZAR("lanzar", "lanzado", "lanzando"),
    ;

    /** Texto del botón, en mayúsculas: «REINICIAR». */
    val textoBoton: String get() = texto.uppercase()

    /** ¿La acción deja algo parado y por tanto exige deliberación? */
    val dejaAlgoParado: Boolean
        get() = this == PARAR || this == REINICIAR || this == DESACTIVAR
}

/** Privilegio del usuario SSH en el host (Fase 13 §7.3). */
enum class PrivilegioVista {
    ROOT,
    SUDO_SIN_CLAVE,
    SUDO_CON_CLAVE,
    SIN_SUDO,
    ;

    /** ¿Hay que pedir la contraseña de sudo? */
    val pideContrasena: Boolean get() = this == SUDO_CON_CLAVE

    /** ¿Puede ejecutar órdenes privilegiadas? */
    val puede: Boolean get() = this != SIN_SUDO
}

/** ¿Se puede leer el diario del sistema entero o solo lo del usuario? */
enum class PermisoDiarioVista {
    TODO,
    PROPIO,
    ;

    /** Sin el diario entero hace falta sudo para ver los logs del sistema. */
    val necesitaSudo: Boolean get() = this == PROPIO
}

/** Estado de systemd en el host (Fase 13 §5.1). */
sealed interface SystemdVista {
    /** Versión de systemd (p. ej. «252»). */
    data class Disponible(val version: String) : SystemdVista

    /** El host no tiene systemd. */
    data object Ausente : SystemdVista

    /** systemd anterior a 230 (se ve la versión). */
    data class Antiguo(val version: String) : SystemdVista

    /** ¿Se puede usar systemd en este host? */
    val utilizable: Boolean get() = this is Disponible

    /** Versión de systemd, si se conoce. */
    val versionTexto: String?
        get() = when (this) {
            is Disponible -> version
            is Antiguo -> version
            Ausente -> null
        }
}

/** Motor de contenedores (Docker o Podman). */
enum class MotorNombreVista(
    /** Nombre técnico que usa el script. */
    val texto: String,
) {
    DOCKER("docker"),
    PODMAN("podman"),
    ;

    /** Con este motor se usa sudo; con Podman nunca (F13 §7.3). */
    val usaSudo: Boolean get() = this == DOCKER
}

/** Motor de contenedores con su versión. */
data class MotorVista(
    val nombre: MotorNombreVista,
    val version: String = "",
)

/** Estado de los contenedores en el host (informe §6.3). */
sealed interface MotorEstadoVista {
    /** No hay Docker ni Podman instalados. */
    data object NO : MotorEstadoVista

    /** Hay motor pero el usuario no tiene permiso. */
    data object SIN_PERMISO : MotorEstadoVista

    /** Hay motor y se puede usar. */
    data class DISPONIBLE(val motor: MotorVista) : MotorEstadoVista

    /** El motor utilizable, si lo hay. */
    val motorUtilizable: MotorVista?
        get() = (this as? DISPONIBLE)?.motor
}

/** Estado de arranque de una unidad (UnitFileState). */
enum class EstadoArranqueVista(
    /** Texto de la interfaz: «activado». */
    val texto: String,
) {
    ACTIVADO("activado"),
    DESACTIVADO("desactivado"),
    ESTATICO("estático"),
    INDIRECTO("indirecto"),
    GENERADO("generado"),
    TRANSITORIO("transitorio"),
    ALIAS("alias"),
    ENMASCARADO("enmascarado"),
    DESCONOCIDO("desconocido"),
    ;

    /** ¿Se puede activar o desactivar el arranque? */
    val conmutable: Boolean
        get() = this == ACTIVADO || this == DESACTIVADO
}

/**
 * Reloj del host: época en segundos y uptime. La app lo usa solo si necesita
 * convertir un sello del host; las fechas ya llegan resueltas del núcleo.
 */
data class RelojHostVista(
    val epoch: Long = 0,
    val uptimeSeg: Long = 0,
)

/** Una propiedad clave=valor de systemctl show o de las etiquetas de inspect. */
data class PropiedadVista(
    val clave: String = "",
    val valor: String = "",
)

/**
 * Unidad de systemd del listado. Las propiedades llegan ya parseadas por el
 * núcleo; las acciones permitidas también, y la app solo ofrece esas.
 */
data class UnidadVista(
    val nombre: String = "",
    val propiedades: List<PropiedadVista> = emptyList(),
    /** Está en HOSTS.servicios (se pinta con ★ y arriba). */
    val vigilada: Boolean = false,
    /** Acciones permitidas en su estado actual; la app solo ofrece estas. */
    val acciones: List<AccionServicioVista> = emptyList(),
    /**
     * Instante (época del host) en el que entró en el estado actual. Lo calcula
     * el núcleo con el reloj del host; mientras el contrato UniFFI no lo
     * exponga, queda null y la vista no enseña el «desde».
     */
    val desdeEn: Long? = null,
    /**
     * Estado de arranque ya parseado por el núcleo. Mientras el contrato
     * UniFFI no lo exponga, DESCONOCIDO.
     */
    val arranque: EstadoArranqueVista = EstadoArranqueVista.DESCONOCIDO,
) {
    /** Propiedad por clave; vacío si no existe. */
    fun propiedad(clave: String): String =
        propiedades.firstOrNull { it.clave == clave }?.valor.orEmpty()

    /** ActiveState («active», «inactive», «failed», «activating»…). */
    val estadoActivo: String get() = propiedad("ActiveState")

    /** SubState («running», «dead», «exited»…). */
    val subestado: String get() = propiedad("SubState")

    /** Descripción de la unidad. */
    val descripcion: String get() = propiedad("Description")

    /** ¿Está en marcha? */
    val activa: Boolean get() = estadoActivo == "active"

    /** ¿Falló el último arranque? */
    val fallida: Boolean get() = estadoActivo == "failed"

    /** PID principal, si corre. */
    val pid: Int? get() = propiedad("ExecMainPID").toIntOrNull()?.takeIf { it > 0 }

    /** Memoria actual en bytes, si el host la da. */
    val memoriaBytes: Long?
        get() = propiedad("MemoryCurrent").toLongOrNull()?.takeIf { it > 0 }

    /** Número de tareas del proceso principal, si el host lo da. */
    val tareas: Int? get() = propiedad("TasksCurrent").toIntOrNull()?.takeIf { it > 0 }

    /** Fichero de la unidad, si se conoce. */
    val fichero: String? get() = propiedad("FragmentPath").takeIf { it.isNotEmpty() }

    /** Texto del estado con su subestado: «activo (running)». */
    val textoEstado: String
        get() = when (estadoActivo) {
            "active" -> if (subestado.isEmpty()) "activo" else "activo (" + subestado + ")"
            "inactive" -> "inactivo"
            "failed" -> "fallido"
            "activating" -> "arrancando"
            "deactivating" -> "parando"
            "reloading" -> "recargando"
            else -> estadoActivo.ifEmpty { "desconocido" }
        }
}

/** Timer de systemd del listado, con las acciones permitidas. */
data class TimerVista(
    val nombre: String = "",
    val propiedades: List<PropiedadVista> = emptyList(),
    /** Servicio que activa, si se conoce. */
    val servicio: String? = null,
    val acciones: List<AccionServicioVista> = emptyList(),
    /**
     * Próxima y última ejecución (época del host). Las calcula el núcleo con
     * el reloj del host; mientras el contrato UniFFI no las exponga, quedan
     * null.
     */
    val proximaEn: Long? = null,
    val ultimaEn: Long? = null,
) {
    fun propiedad(clave: String): String =
        propiedades.firstOrNull { it.clave == clave }?.valor.orEmpty()

    val descripcion: String get() = propiedad("Description")

    val estadoActivo: String get() = propiedad("ActiveState")

    /** El servicio que activa, del nombre del timer si no viene en Unit. */
    val servicioActivado: String?
        get() = servicio ?: nombre.removeSuffix(".timer").takeIf { it != nombre }?.plus(".service")
}

/** Estado de un contenedor (informe §6.3). */
enum class EstadoContenedorVista(
    /** Texto de la interfaz. */
    val texto: String,
) {
    CREADO("creado"),
    EN_MARCHA("en marcha"),
    PARADO("parado"),
    REINICIANDO("reiniciándose"),
    PAUSADO("en pausa"),
    MUERTO("muerto"),
    DESCONOCIDO("desconocido"),
    ;

    /** ¿Está en marcha (o reiniciándose)? */
    val enMarcha: Boolean
        get() = this == EN_MARCHA || this == REINICIANDO
}

/** Salud de un contenedor con healthcheck. */
enum class SaludVista(
    /** Texto de la interfaz; vacío cuando no hay healthcheck. */
    val texto: String,
) {
    NINGUNA(""),
    SANA("sana"),
    INSEGURA("unhealthy"),
    COMPROBANDO("comprobando"),
}

/** Un contenedor del listado, con las acciones que se le permiten. */
data class ContenedorVista(
    val id: String = "",
    val nombre: String = "",
    val imagen: String = "",
    val estado: EstadoContenedorVista = EstadoContenedorVista.DESCONOCIDO,
    val codigoSalida: Int? = null,
    val salud: SaludVista = SaludVista.NINGUNA,
    val puertos: String = "",
    val proyecto: String? = null,
    val ficheros: List<String> = emptyList(),
    val servicio: String? = null,
    val orden: String? = null,
    val etiquetas: List<PropiedadVista> = emptyList(),
    val acciones: List<AccionServicioVista> = emptyList(),
) {
    /** ¿Está caído según la F13: unhealthy, reiniciándose, muerto o salida ≠ 0? */
    val caido: Boolean
        get() = when (estado) {
            EstadoContenedorVista.REINICIANDO, EstadoContenedorVista.MUERTO -> true
            EstadoContenedorVista.EN_MARCHA -> salud == SaludVista.INSEGURA
            EstadoContenedorVista.PARADO -> codigoSalida != null && codigoSalida != 0
            else -> false
        }

    /** ¿Se puede abrir una shell? Solo si corre. */
    val admiteShell: Boolean get() = estado == EstadoContenedorVista.EN_MARCHA

    /** Una etiqueta del contenedor, si la trae. */
    fun etiqueta(clave: String): String =
        etiquetas.firstOrNull { it.clave == clave }?.valor.orEmpty()
}

/** Proyecto Compose del listado, con las acciones que se le permiten. */
data class ProyectoVista(
    val nombre: String = "",
    val ficheros: List<String> = emptyList(),
    val acciones: List<AccionServicioVista> = emptyList(),
) {
    /** ¿Se conocen los ficheros Compose? Sin ellos no hay acciones de proyecto. */
    val conFicheros: Boolean get() = ficheros.isNotEmpty()
}

/** Las tres secciones de la pantalla Servicios de un host. */
enum class SeccionServiciosVista(
    /** Etiqueta de la pestaña. */
    val etiqueta: String,
) {
    SERVICIOS("SERVICIOS"),
    TIMERS("TIMERS"),
    CONTENEDORES("CONT."),
}

/** Contenedores de un proyecto Compose, con «sin proyecto» al final. */
data class GrupoContenedoresVista(
    val proyecto: ProyectoVista? = null,
    val contenedores: List<ContenedorVista> = emptyList(),
) {
    val clave: String get() = proyecto?.nombre ?: SIN_PROYECTO

    companion object {
        const val SIN_PROYECTO: String = "sin-proyecto"
    }
}

/**
 * Listado completo de servicios de un host (espejo del Listado del núcleo):
 * lo que trae servicios.sh listar, más si hay contraseña de sudo en memoria y
 * la hora de la lectura. Si el host deja de responder, la pantalla conserva
 * el listado anterior con su hora y enseña el aviso.
 */
data class ListadoServiciosVista(
    val systemd: SystemdVista = SystemdVista.Ausente,
    val privilegio: PrivilegioVista = PrivilegioVista.SIN_SUDO,
    val diario: PermisoDiarioVista = PermisoDiarioVista.PROPIO,
    val reloj: RelojHostVista = RelojHostVista(),
    val motor: MotorEstadoVista = MotorEstadoVista.NO,
    val unidades: List<UnidadVista> = emptyList(),
    val timers: List<TimerVista> = emptyList(),
    val contenedores: List<ContenedorVista> = emptyList(),
    val proyectos: List<ProyectoVista> = emptyList(),
    /** Avisos del host («sin permiso para Docker: …»). */
    val avisos: List<String> = emptyList(),
    /** Se recortó por los límites de entrada no fiable (F13 §8). */
    val incompleto: Boolean = false,
    /** True si hay contraseña de sudo en memoria para ese host. */
    val sudoEnMemoria: Boolean = false,
    /** Instante de la lectura (epoch en segundos); null si no se ha leído. */
    val hora: Long? = null,
) {
    /** Secciones elegibles: un host sin systemd solo abre Contenedores. */
    val secciones: List<SeccionServiciosVista>
        get() = if (systemd.utilizable) {
            listOf(
                SeccionServiciosVista.SERVICIOS,
                SeccionServiciosVista.TIMERS,
                SeccionServiciosVista.CONTENEDORES,
            )
        } else {
            listOf(SeccionServiciosVista.CONTENEDORES)
        }

    /** Sección con la que se abre la pantalla. */
    val seccionInicial: SeccionServiciosVista
        get() = if (systemd.utilizable) SeccionServiciosVista.SERVICIOS
        else SeccionServiciosVista.CONTENEDORES

    /** Número de elementos de una sección. */
    fun cuantos(seccion: SeccionServiciosVista): Int = when (seccion) {
        SeccionServiciosVista.SERVICIOS -> unidades.size
        SeccionServiciosVista.TIMERS -> timers.size
        SeccionServiciosVista.CONTENEDORES -> contenedores.size
    }

    /** Unidades fallidas (se pintan primero). */
    val unidadesFallidas: List<UnidadVista> get() = unidades.filter { it.fallida }

    /** Unidades vigiladas. */
    val unidadesVigiladas: List<UnidadVista> get() = unidades.filter { it.vigilada }

    /** Contenedores caídos (se pintan primero). */
    val contenedoresCaidos: List<ContenedorVista> get() = contenedores.filter { it.caido }

    /**
     * Unidades en el orden de la F13 §6.3: primero las fallidas, después las
     * vigiladas con ★ y luego el resto, cada grupo por nombre.
     */
    val unidadesOrdenadas: List<UnidadVista>
        get() = ordenarUnidades(unidades)

    /** Timers con las fallidas primero y luego por nombre. */
    val timersOrdenados: List<TimerVista>
        get() = timers.sortedWith(
            compareBy<TimerVista> { if (it.estadoActivo == "failed") 0 else 1 }
                .thenBy(String.CASE_INSENSITIVE_ORDER) { it.nombre },
        )

    /** Contenedores agrupados por proyecto Compose, con «sin proyecto» al final. */
    fun contenedoresPorProyecto(): List<GrupoContenedoresVista> {
        val conProyecto = proyectos.map { proyecto ->
            GrupoContenedoresVista(
                proyecto = proyecto,
                contenedores = contenedores
                    .filter { it.proyecto == proyecto.nombre }
                    .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre }),
            )
        }.filter { it.contenedores.isNotEmpty() }
        val sinProyecto = contenedores
            .filter { it.proyecto.isNullOrEmpty() }
            .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.nombre })
        return if (sinProyecto.isEmpty()) conProyecto
        else conProyecto + GrupoContenedoresVista(contenedores = sinProyecto)
    }

    /** Pie del listado: «142 servicios · 1 fallido · 3 vigilados». */
    val textoPie: String
        get() {
            val partes = mutableListOf<String>()
            if (systemd.utilizable) {
                partes += unidades.size.toString() + " servicios"
                val fallidos = unidadesFallidas.size
                if (fallidos > 0) {
                    partes += fallidos.toString() + (if (fallidos == 1) " fallido" else " fallidos")
                }
                val vigilados = unidadesVigiladas.size
                if (vigilados > 0) {
                    partes += vigilados.toString() +
                        (if (vigilados == 1) " vigilado" else " vigilados")
                }
            }
            if (contenedores.isNotEmpty()) {
                partes += contenedores.size.toString() + " contenedores"
                val caidos = contenedoresCaidos.size
                if (caidos > 0) {
                    partes += caidos.toString() + (if (caidos == 1) " caído" else " caídos")
                }
            }
            return partes.joinToString(" · ")
        }
}

/** Ordena unidades como en la F13: fallidas, vigiladas y el resto por nombre. */
fun ordenarUnidades(unidades: List<UnidadVista>): List<UnidadVista> =
    unidades.sortedWith(
        compareBy<UnidadVista> { if (it.fallida) 0 else if (it.vigilada) 1 else 2 }
            .thenBy(String.CASE_INSENSITIVE_ORDER) { it.nombre },
    )

/** Consumo de un contenedor en marcha (suborden consumo de servicios.sh). */
data class ConsumoVista(
    val id: String = "",
    val cpuPct: Double? = null,
    val memoriaBytes: Long? = null,
    val memoriaLimiteBytes: Long? = null,
) {
    /** Texto de la fila: «CPU 2,4 % · mem 180 MB». */
    val texto: String
        get() {
            val partes = mutableListOf<String>()
            cpuPct?.let { partes += "CPU " + unDecimal(it) + " %" }
            memoriaBytes?.let { partes += "mem " + formatoBytesServicios(it) }
            return partes.joinToString(" · ")
        }
}

/** Datos de un contenedor leídos de inspect (pestaña ESTADO del detalle). */
data class DetalleContenedorVista(
    val id: String = "",
    val nombre: String = "",
    val imagen: String = "",
    val creado: String = "",
    val estado: EstadoContenedorVista = EstadoContenedorVista.DESCONOCIDO,
    val codigoSalida: Int? = null,
    val reinicios: Long = 0,
    val salud: SaludVista = SaludVista.NINGUNA,
    val puertos: List<String> = emptyList(),
    val montajes: List<String> = emptyList(),
    val redes: List<String> = emptyList(),
    val proyecto: String? = null,
    val orden: String? = null,
    /** Texto de inspect (hasta 256 KiB). */
    val texto: String = "",
)

/** Pestañas del detalle de un objetivo (informe §6.4). */
enum class PestanaDetalleVista(
    val etiqueta: String,
) {
    ESTADO("ESTADO"),
    DEPENDENCIAS("DEPENDENCIAS"),
    UNIDAD("UNIDAD"),
}

/**
 * Detalle de un objetivo (informe §6.4): propiedades, dependencias y el texto
 * de systemctl cat o de inspect. Para un contenedor, además, sus datos ya
 * leídos del inspect.
 */
data class DetalleServiciosVista(
    val objetivo: ObjetivoVista = ObjetivoVista(),
    val propiedades: List<PropiedadVista> = emptyList(),
    /** Dependencias que necesita. */
    val necesita: List<String> = emptyList(),
    /** Dependencias que la necesitan. */
    val necesitada: List<String> = emptyList(),
    /** Texto de la unidad o del inspect. */
    val texto: String = "",
    val contenedor: DetalleContenedorVista? = null,
) {
    /** Propiedad por clave; vacío si no existe. */
    fun propiedad(clave: String): String =
        propiedades.firstOrNull { it.clave == clave }?.valor.orEmpty()

    /** El timer muestra la próxima y la última ejecución en la pestaña UNIDAD. */
    val esTimer: Boolean get() = objetivo.tipo == TipoObjetivoVista.TIMER
}

/**
 * Lo que la app necesita saber antes de confirmar o deliberar una acción
 * (informe §4.3 y §6.4). Lo calcula el núcleo: la app no decide nada.
 */
data class InfoAccionVista(
    /** Si la acción se permite en el estado actual. */
    val permitida: Boolean = false,
    /** Por qué no se permite, si no se permite. */
    val motivoNoPermitida: String? = null,
    /** Si deja algo parado y exige deliberación MAGI. */
    val delibera: Boolean = false,
    /** Si hace falta la contraseña de sudo para ejecutarla. */
    val sudoConClave: Boolean = false,
    /** Comprobación de configuración que se aplicará antes, si la hay. */
    val comprobacion: String? = null,
    /** True si el objetivo es una unidad crítica (puede cortar la conexión). */
    val critica: Boolean = false,
    /** Hosts que van a través de este, para el aviso de unidad crítica. */
    val hostsTraves: List<String> = emptyList(),
) {
    /** ¿Hay que enseñar el aviso rojo de unidad crítica? */
    val avisaCritica: Boolean get() = critica
}

/** Resultado de una acción de servicio (informe §6.4). */
enum class ResultadoAccionServicioVista(
    /** Texto del estado en la hoja de resultado. */
    val texto: String,
    /** Glifo que lo acompaña. */
    val glifo: String,
) {
    HECHO("hecho", "✓"),
    FALLIDO("fallido", "✕"),
    /** Se acabó el plazo: la orden sigue en el host. */
    SIN_CONFIRMAR("sin confirmar", "…"),
    /** La comprobación de configuración falló y no se ejecutó nada. */
    RECHAZADA_CONFIGURACION("rechazada por la configuración", "✕"),
    /** Error del host o de la conexión. */
    ERROR("error", "✕"),
    ;

    /** Solo HECHO es un éxito. */
    val esOk: Boolean get() = this == HECHO

    /** Un resultado que la hoja pinta en rojo o ámbar. */
    val esFallo: Boolean
        get() = this == FALLIDO || this == ERROR || this == RECHAZADA_CONFIGURACION
}

/**
 * Resultado de una acción de servicio (espejo del ResultadoServicio del
 * núcleo, más el contexto para poder pintarlo y difundirlo aunque la pantalla
 * que la pidió ya no esté).
 *
 * [lineasDiario] y [salidaValidador] salen del mismo bloque de líneas del
 * núcleo: son las últimas 20 del diario si la acción falló, o la salida de la
 * comprobación si la rechazó.
 */
data class ResultadoServicioVista(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val objetivo: ObjetivoVista = ObjetivoVista(),
    val accion: AccionServicioVista? = null,
    val resultado: ResultadoAccionServicioVista = ResultadoAccionServicioVista.ERROR,
    /** Estado final del objetivo («active», «inactive», «running»…). */
    val estado: String = "",
    /** Segundos que tardó la acción en el host. */
    val segundos: Float = 0f,
    val mensaje: String = "",
    /** Últimas líneas del diario (fallo); vacío si no las hay. */
    val lineasDiario: List<String> = emptyList(),
    /** Salida del validador de configuración, si la acción se rechazó. */
    val salidaValidador: List<String> = emptyList(),
    /** La comprobación de configuración que se aplicó, si hubo. */
    val comprobacion: String? = null,
) {
    /** Hay algo que enseñar como detalle del resultado. */
    val tieneDetalle: Boolean get() = lineasDiario.isNotEmpty() || salidaValidador.isNotEmpty()

    /** «reiniciar app.service → web-01», para la hoja de resultado. */
    val textoAccion: String
        get() {
            val accionTexto = accion?.texto ?: "acción"
            return accionTexto + " " + objetivo.nombre + " → " + hostNombre
        }
}

/** Fase de una acción de servicio en curso (informe §3.1). */
enum class FaseAccionVista(
    val texto: String,
) {
    COMPROBANDO("comprobando"),
    EJECUTANDO("ejecutando"),
    ESPERANDO("esperando"),
}

/**
 * Una acción de servicio en curso, tal como la difunde la lista completa del
 * núcleo. Una sola acción a la vez por objetivo; la fila del elemento pone ◐ y
 * la acción («reiniciando…»), venga de esta pantalla o de otra.
 */
data class AccionEnCursoVista(
    val id: Long = 0,
    val hostUuid: String? = null,
    val hostNombre: String = "",
    val objetivo: ObjetivoVista = ObjetivoVista(),
    val accion: AccionServicioVista = AccionServicioVista.ARRANCAR,
    val fase: FaseAccionVista = FaseAccionVista.COMPROBANDO,
    /** Deliberación que la autoriza, si la hubo. */
    val deliberacionId: Long? = null,
    /** Instante de inicio (epoch local en milisegundos). */
    val desdeMs: Long = 0,
) {
    /** «reiniciando…», el texto de la fila. */
    val texto: String get() = accion.gerundio + "…"

    /** ¿La acción es sobre este objetivo de este host? */
    fun esDe(hostUuid: String, objetivo: ObjetivoVista): Boolean =
        this.hostUuid == hostUuid && this.objetivo == objetivo
}

/** Busca la acción en curso de un objetivo concreto, si la hay. */
fun List<AccionEnCursoVista>.deObjetivo(
    hostUuid: String,
    objetivo: ObjetivoVista,
): AccionEnCursoVista? = firstOrNull { it.esDe(hostUuid, objetivo) }

/** Resultado de comprobar la contraseña de sudo (servicios.sh sudo). */
sealed interface ResultadoSudoVista {
    /** Correcta: queda en memoria hasta que caduque o se olvide. */
    data object OK : ResultadoSudoVista

    /** El host la rechazó. */
    data object RECHAZADA : ResultadoSudoVista

    /** El usuario no puede usar sudo en ese host. */
    data object SIN_SUDO : ResultadoSudoVista

    /** No se pudo comprobar (conexión, huella, credenciales); mensaje legible. */
    data class ERROR(val mensaje: String) : ResultadoSudoVista
}

/**
 * Orden con la que se abre una pestaña desde Servicios. El núcleo arma la
 * orden real; la app nunca manda texto para ejecutar (T108).
 */
sealed interface OrdenPestanaVista {
    /** Shell dentro de un contenedor. */
    data class ShellContenedor(val motor: MotorNombreVista, val id: String) : OrdenPestanaVista

    /** Seguimiento del diario de una unidad. */
    data class SeguirDiario(val unidad: String) : OrdenPestanaVista

    /** Seguimiento de los logs de un contenedor. */
    data class SeguirContenedor(val motor: MotorNombreVista, val id: String) : OrdenPestanaVista
}

/** Tipo de fallo del sondeo ampliado: una unidad o un contenedor. */
enum class TipoFalloVista(val texto: String) {
    SERVICIO("servicio"),
    CONTENEDOR("contenedor"),
}

/** Un fallo del último sondeo: una unidad fallida o un contenedor caído. */
data class FalloFlotaVista(
    val tipo: TipoFalloVista = TipoFalloVista.SERVICIO,
    /** Nombre de la unidad o del contenedor. */
    val nombre: String = "",
    /** Estado con el que se vio («fallido», «caído», «unhealthy»…). */
    val estado: String = "",
    /** Motor que vio el contenedor, si es un contenedor. */
    val motor: MotorNombreVista? = null,
) {
    /** Una unidad se pinta como servicio y un contenedor con su motor. */
    val esContenedor: Boolean get() = tipo == TipoFalloVista.CONTENEDOR
}

/** Una fila de «Toda la flota»: un fallo del último sondeo de un host. */
data class FilaTodaLaFlotaVista(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val fallo: FalloFlotaVista = FalloFlotaVista(),
    /** Instante del sondeo del que sale el fallo (epoch en segundos). */
    val sondeoEn: Long? = null,
)

/** Un host SSH en la lista de «Toda la flota», con su estado y sus fallos. */
data class HostTodaLaFlotaVista(
    val hostUuid: String = "",
    val hostNombre: String = "",
    val estado: EstadoFlotaVista = EstadoFlotaVista.FRIA,
    /** Motivo legible de la caída, del estado o de no tener sondeo. */
    val motivo: String? = null,
    /** Número de fallos del último sondeo. */
    val fallos: Int = 0,
    /** Instante del último sondeo (epoch en segundos). */
    val sondeoEn: Long? = null,
) {
    /** La fila de un host caído o sin sondeo. */
    val esAviso: Boolean
        get() = estado == EstadoFlotaVista.CAIDA || estado == EstadoFlotaVista.FRIA ||
            estado == EstadoFlotaVista.SIN_SONDEO
}

/**
 * «Toda la flota» (informe §5.1 y §6.4): los fallos del último sondeo de cada
 * host, con su hora, y los hosts caídos o sin sondeo. Entrar no abre
 * conexiones.
 */
data class TodaLaFlotaVista(
    val fallos: List<FilaTodaLaFlotaVista> = emptyList(),
    val hosts: List<HostTodaLaFlotaVista> = emptyList(),
) {
    /** Sin fallos ni hosts con problemas. */
    val vacia: Boolean get() = fallos.isEmpty() && hosts.isEmpty()

    /** Total de fallos, para la cabecera «TODA LA FLOTA · 4 fallos». */
    val numFallos: Int get() = fallos.size
}

/** Formatea bytes para el consumo: «180 MB», «1,2 GB». */
internal fun formatoBytesServicios(bytes: Long): String {
    val valor = bytes.coerceAtLeast(0)
    val kb = 1024.0
    val mb = kb * 1024.0
    val gb = mb * 1024.0
    return when {
        valor < 1024L -> valor.toString() + " B"
        valor < 1024L * 1024L -> unDecimal(valor / kb) + " KB"
        valor < 1024L * 1024L * 1024L -> unDecimal(valor / mb) + " MB"
        else -> unDecimal(valor / gb) + " GB"
    }
}
