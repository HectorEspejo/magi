package org.cuatrodetres.magi.nucleo

import org.cuatrodetres.magi.ui.modelos.ClaveMovilVista
import org.cuatrodetres.magi.ui.modelos.ColorVista
import org.cuatrodetres.magi.ui.modelos.ConflictoHuellaVista
import org.cuatrodetres.magi.ui.modelos.EquipoVista
import org.cuatrodetres.magi.ui.modelos.EstadoEquipoVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostVista
import org.cuatrodetres.magi.ui.modelos.EstadoSesionVista
import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.EstadoTunelVista
import org.cuatrodetres.magi.ui.modelos.FaseSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.FichaHostVista
import org.cuatrodetres.magi.ui.modelos.GrupoVista
import org.cuatrodetres.magi.ui.modelos.HostVista
import org.cuatrodetres.magi.ui.modelos.HuellaHostVista
import org.cuatrodetres.magi.ui.modelos.ModificadoresVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.PosicionVista
import org.cuatrodetres.magi.ui.modelos.RespuestaDialogoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoInstalacionVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TeclaVista
import org.cuatrodetres.magi.ui.modelos.TipoHostVista
import org.cuatrodetres.magi.ui.modelos.TipoTunelVista
import org.cuatrodetres.magi.ui.modelos.TramoVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.UbicacionClaveVista
import org.cuatrodetres.magi.ui.modelos.VersionVista
import org.cuatrodetres.magi.ui.modelos.AccionEnCursoVista
import org.cuatrodetres.magi.ui.modelos.AccionServicioVista
import org.cuatrodetres.magi.ui.modelos.AjustesMovilVista
import org.cuatrodetres.magi.ui.modelos.ConsumoVista
import org.cuatrodetres.magi.ui.modelos.ContenedorVista
import org.cuatrodetres.magi.ui.modelos.DecisionDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.DestinoSnippetVista
import org.cuatrodetres.magi.ui.modelos.DetalleContenedorVista
import org.cuatrodetres.magi.ui.modelos.DetalleDeliberacionRegistroVista
import org.cuatrodetres.magi.ui.modelos.DetalleFlotaVista
import org.cuatrodetres.magi.ui.modelos.DetalleRegistroVista
import org.cuatrodetres.magi.ui.modelos.DetalleServiciosVista
import org.cuatrodetres.magi.ui.modelos.EjecucionVista
import org.cuatrodetres.magi.ui.modelos.EntradaRegistroVista
import org.cuatrodetres.magi.ui.modelos.ErrorCampoVista
import org.cuatrodetres.magi.ui.modelos.EstadoArranqueVista
import org.cuatrodetres.magi.ui.modelos.EstadoContenedorVista
import org.cuatrodetres.magi.ui.modelos.EstadoDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.EstadoFlotaVista
import org.cuatrodetres.magi.ui.modelos.EstadoHostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.EstadoLogsVista
import org.cuatrodetres.magi.ui.modelos.EstadoSuscripcionLogVista
import org.cuatrodetres.magi.ui.modelos.FalloFlotaVista
import org.cuatrodetres.magi.ui.modelos.FallosFlotaVista
import org.cuatrodetres.magi.ui.modelos.FaseAccionVista
import org.cuatrodetres.magi.ui.modelos.FilaHostDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.FilaTodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.FiltroLogsVista
import org.cuatrodetres.magi.ui.modelos.FiltroRegistroVista
import org.cuatrodetres.magi.ui.modelos.HostDestinoVista
import org.cuatrodetres.magi.ui.modelos.HostEjecucionVista
import org.cuatrodetres.magi.ui.modelos.HostFlotaVista
import org.cuatrodetres.magi.ui.modelos.HostTodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.InfoAccionVista
import org.cuatrodetres.magi.ui.modelos.LineaLogVista
import org.cuatrodetres.magi.ui.modelos.ListadoServiciosVista
import org.cuatrodetres.magi.ui.modelos.MetricasFlotaVista
import org.cuatrodetres.magi.ui.modelos.MotivoFinLogVista
import org.cuatrodetres.magi.ui.modelos.MotorEstadoVista
import org.cuatrodetres.magi.ui.modelos.MotorNombreVista
import org.cuatrodetres.magi.ui.modelos.MotorVista
import org.cuatrodetres.magi.ui.modelos.NivelLogVista
import org.cuatrodetres.magi.ui.modelos.ObjetivoVista
import org.cuatrodetres.magi.ui.modelos.OrdenPestanaVista
import org.cuatrodetres.magi.ui.modelos.PermisoDiarioVista
import org.cuatrodetres.magi.ui.modelos.PlanDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.PreparacionEjecucionVista
import org.cuatrodetres.magi.ui.modelos.PrivilegioVista
import org.cuatrodetres.magi.ui.modelos.PropiedadVista
import org.cuatrodetres.magi.ui.modelos.ProyectoVista
import org.cuatrodetres.magi.ui.modelos.RelojHostVista
import org.cuatrodetres.magi.ui.modelos.ResultadoAccion
import org.cuatrodetres.magi.ui.modelos.ResultadoAccionServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoDeliberacionRegistroVista
import org.cuatrodetres.magi.ui.modelos.ResultadoEjecucionVista
import org.cuatrodetres.magi.ui.modelos.ResultadoNucleoVista
import org.cuatrodetres.magi.ui.modelos.ResultadoRegistroVista
import org.cuatrodetres.magi.ui.modelos.ResultadoServicioVista
import org.cuatrodetres.magi.ui.modelos.ResultadoSudoVista
import org.cuatrodetres.magi.ui.modelos.SalidaEjecucionVista
import org.cuatrodetres.magi.ui.modelos.SaludVista
import org.cuatrodetres.magi.ui.modelos.SnippetAlConectarVista
import org.cuatrodetres.magi.ui.modelos.SnippetVista
import org.cuatrodetres.magi.ui.modelos.SystemdVista
import org.cuatrodetres.magi.ui.modelos.TablaDeliberacionVista
import org.cuatrodetres.magi.ui.modelos.TimerVista
import org.cuatrodetres.magi.ui.modelos.TipoFalloVista
import org.cuatrodetres.magi.ui.modelos.TipoObjetivoVista
import org.cuatrodetres.magi.ui.modelos.TipoRegistroVista
import org.cuatrodetres.magi.ui.modelos.TodaLaFlotaVista
import org.cuatrodetres.magi.ui.modelos.UnidadVista
import org.cuatrodetres.magi.ui.modelos.VariableSnippetVista
import org.cuatrodetres.magi.ui.modelos.VentanaLogsVista
import org.cuatrodetres.magi.ui.modelos.VeredictoVista
import org.cuatrodetres.magi.ui.modelos.VerificacionesVista
import org.cuatrodetres.magi.ui.modelos.VigiladoVista
import org.cuatrodetres.magi.ui.modelos.nivelLineaLog

/**
 * Conversión entre los DTO generados por UniFFI (paquete de este mismo módulo)
 * y los modelos de vista que consume la interfaz (ui/modelos).
 *
 * Es código puro (sin Android) para poder probarlo en la JVM: aquí no hay
 * llamadas al núcleo ni al Keystore, solo traducción de datos.
 */

// ===========================================================================
// Inventario
// ===========================================================================

fun GrupoInventario.aVista(): GrupoVista = GrupoVista(
    uuid = uuid,
    nombre = nombre,
    numHosts = numHosts.toInt(),
    plegado = plegado,
)

fun TipoHostMovil.aVista(): TipoHostVista = when (this) {
    TipoHostMovil.SSH -> TipoHostVista.SSH
    TipoHostMovil.RDP -> TipoHostVista.RDP
    TipoHostMovil.VNC -> TipoHostVista.VNC
    TipoHostMovil.VOXL -> TipoHostVista.VOXL
}

fun HostInventario.aVista(): HostVista = HostVista(
    uuid = uuid,
    nombre = nombre,
    direccion = direccion,
    puerto = puerto.toInt(),
    tipo = tipo.aVista(),
    usuario = usuario,
    grupoUuid = grupoUuid,
    etiquetas = etiquetas,
    estado = when (estado) {
        EstadoHost.SIN_SESION -> EstadoHostVista.SIN_SESION
        EstadoHost.CONECTANDO -> EstadoHostVista.CONECTANDO
        EstadoHost.CON_SESIONES -> EstadoHostVista.CON_SESIONES
        EstadoHost.ERROR -> EstadoHostVista.ERROR
    },
    sesionesAbiertas = sesionesAbiertas.toInt(),
    ultimoError = ultimoError,
    ultimaConexionEn = ultimaConexionEn,
)

fun HuellaHost.aVista(): HuellaHostVista = HuellaHostVista(tipo = tipo, sha256 = sha256)

fun FichaHost.aVista(): FichaHostVista = FichaHostVista(
    uuid = uuid,
    nombre = nombre,
    direccion = direccion,
    puerto = puerto.toInt(),
    tipo = tipo.aVista(),
    usuario = usuario,
    grupoNombre = grupoNombre,
    etiquetas = etiquetas,
    saltoNombre = saltoNombre,
    keepaliveSegundos = keepaliveSegundos?.toInt(),
    identidadEscritorio = identidadEscritorio,
    huellas = huellas.map { it.aVista() },
    tieneContrasena = tieneContrasena,
    flota = flota?.aVista(),
    verificaciones = verificaciones?.aVista(),
    snippetAlConectar = snippetAlConectar?.aVista(),
)

// ===========================================================================
// Sincronización
// ===========================================================================

fun FaseSincronizacion.aVista(): FaseSincronizacionVista = when (this) {
    FaseSincronizacion.SIN_CONFIGURAR -> FaseSincronizacionVista.SIN_CONFIGURAR
    FaseSincronizacion.SIN_CLAVE -> FaseSincronizacionVista.SIN_CLAVE
    FaseSincronizacion.ACTIVA -> FaseSincronizacionVista.ACTIVA
    FaseSincronizacion.SINCRONIZANDO -> FaseSincronizacionVista.SINCRONIZANDO
    FaseSincronizacion.ESPERANDO_CLAVE -> FaseSincronizacionVista.ESPERANDO_CLAVE
    FaseSincronizacion.ERROR_CARPETA -> FaseSincronizacionVista.ERROR_CARPETA
}

fun EstadoEquipo.aVista(): EstadoEquipoVista = when (this) {
    EstadoEquipo.ESTE_EQUIPO -> EstadoEquipoVista.ESTE_EQUIPO
    EstadoEquipo.AL_DIA -> EstadoEquipoVista.AL_DIA
    EstadoEquipo.ILEGIBLE -> EstadoEquipoVista.ILEGIBLE
    EstadoEquipo.DANADO -> EstadoEquipoVista.DANADO
    EstadoEquipo.FORMATO_NUEVO -> EstadoEquipoVista.FORMATO_NUEVO
    EstadoEquipo.QUITADO -> EstadoEquipoVista.QUITADO
}

fun EquipoSincronizacion.aVista(): EquipoVista = EquipoVista(
    nombre = nombre,
    plataforma = plataforma,
    version = version,
    estado = estado.aVista(),
    ultimaVez = ultimaVez,
    ultimoFichero = ultimoFichero,
)

fun EstadoSincronizacion.aVista(): EstadoSincronizacionVista = EstadoSincronizacionVista(
    fase = fase.aVista(),
    carpetaUri = carpetaUri,
    carpetaNombre = carpetaNombre,
    huellaCortaClave = huellaCortaClave,
    ultimaPasada = ultimaPasada,
    equipos = equipos.map { it.aVista() },
    conflictosHuella = conflictosHuella.toInt(),
    error = error,
)

fun ConflictoHuella.aVista(): ConflictoHuellaVista = ConflictoHuellaVista(
    id = id.toLong(),
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    direccion = direccion,
    huellaMia = huellaMia,
    huellaOtra = huellaOtra,
    equipoOrigen = equipoOrigen,
)

// ===========================================================================
// Sesiones y terminal
// ===========================================================================

fun EstadoSesion.aVista(): EstadoSesionVista = when (this) {
    EstadoSesion.ABRIENDO -> EstadoSesionVista.ABRIENDO
    EstadoSesion.ABIERTA -> EstadoSesionVista.ABIERTA
    EstadoSesion.CAIDA -> EstadoSesionVista.CAIDA
}

fun Sesion.aVista(): SesionVista = SesionVista(
    id = id.toLong(),
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    estado = estado.aVista(),
    detalle = detalle,
    abiertaEn = abiertaEn,
)

fun Color.aVista(): ColorVista = when (this) {
    Color.Predeterminado -> ColorVista.Predeterminado
    is Color.Indice -> ColorVista.Indice(v1.toInt())
    is Color.Rgb -> ColorVista.Rgb(v1.toInt(), v2.toInt(), v3.toInt())
}

fun Tramo.aVista(): TramoVista = TramoVista(
    texto = texto,
    primerPlano = primerPlano.aVista(),
    fondo = fondo.aVista(),
    negrita = negrita,
    cursiva = cursiva,
    subrayado = subrayado,
    inverso = inverso,
)

fun Posicion.aVista(): PosicionVista = PosicionVista(columna = columna.toInt(), fila = fila.toInt())

fun PosicionVista.aNucleo(): Posicion = Posicion(
    columna = columna.coerceAtLeast(0).toUInt(),
    fila = fila.coerceAtLeast(0).toUInt(),
)

fun Pantalla.aVista(): PantallaVista = PantallaVista(
    sesion = sesion.toLong(),
    columnas = columnas.toInt(),
    filas = filas.toInt(),
    filasCambiadas = filasCambiadas.associate { fila ->
        fila.fila.toInt() to fila.tramos.map { it.aVista() }
    },
    cursor = cursor?.aVista(),
    desplazamiento = desplazamiento.toInt(),
    completa = completa,
)

fun ModificadoresVista.aNucleo(): Modificadores = Modificadores(ctrl = ctrl, alt = alt, mayus = mayus)

fun TeclaVista.aNucleo(): Tecla = when (this) {
    TeclaVista.Escape -> Tecla.Escape
    TeclaVista.Tab -> Tecla.Tab
    TeclaVista.Intro -> Tecla.Enter
    TeclaVista.Retroceso -> Tecla.Retroceso
    TeclaVista.Arriba -> Tecla.Arriba
    TeclaVista.Abajo -> Tecla.Abajo
    TeclaVista.Izquierda -> Tecla.Izquierda
    TeclaVista.Derecha -> Tecla.Derecha
    TeclaVista.Inicio -> Tecla.Inicio
    TeclaVista.Fin -> Tecla.Fin
    TeclaVista.RePag -> Tecla.RePag
    TeclaVista.AvPag -> Tecla.AvPag
    TeclaVista.Suprimir -> Tecla.Supr
    TeclaVista.Insertar -> Tecla.Insert
    is TeclaVista.F -> Tecla.F(numero.coerceIn(1, 12).toUByte())
    is TeclaVista.Caracter -> Tecla.Caracter(texto)
}

// ===========================================================================
// Túneles
// ===========================================================================

fun TipoTunel.aVista(): TipoTunelVista = when (this) {
    TipoTunel.LOCAL -> TipoTunelVista.LOCAL
    TipoTunel.REMOTO -> TipoTunelVista.REMOTO
    TipoTunel.DINAMICO -> TipoTunelVista.DINAMICO
}

fun EstadoTunel.aVista(): EstadoTunelVista = when (this) {
    EstadoTunel.INACTIVO -> EstadoTunelVista.INACTIVO
    EstadoTunel.ACTIVANDO -> EstadoTunelVista.ACTIVANDO
    EstadoTunel.ACTIVO -> EstadoTunelVista.ACTIVO
    EstadoTunel.CAIDO -> EstadoTunelVista.CAIDO
}

fun TunelVivo.aVista(): TunelVista = TunelVista(
    uuid = uuid,
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    nombre = nombre,
    tipo = tipo.aVista(),
    escucha = escucha,
    escuchaAviso = escuchaAviso,
    destino = destino,
    automatico = automatico,
    estado = estado.aVista(),
    error = error,
    conexiones = conexiones.toInt(),
    aceptadas = aceptadas.toInt(),
    ultimoError = ultimoError,
    ultimoErrorEn = ultimoErrorEn,
    bytesSubida = bytesSubida.toLong(),
    bytesBajada = bytesBajada.toLong(),
    puertoEfectivo = puertoEfectivo?.toInt(),
)

/** Túneles que cuentan como trabajo vivo para el servicio en primer plano. */
fun List<TunelVista>.tunelesActivos(): Int = count {
    it.estado == EstadoTunelVista.ACTIVO || it.estado == EstadoTunelVista.ACTIVANDO
}

// ===========================================================================
// Diálogos
// ===========================================================================

fun PeticionDialogo.aVista(): PeticionDialogoVista = when (this) {
    is PeticionDialogo.HuellaNueva -> PeticionDialogoVista.HuellaNueva(
        peticionId = id.toLong(),
        hostUuid = hostUuid,
        hostNombre = hostNombre,
        tipo = tipo,
        sha256 = sha256,
    )

    is PeticionDialogo.HuellaCambiada -> PeticionDialogoVista.HuellaCambiada(
        peticionId = id.toLong(),
        hostUuid = hostUuid,
        hostNombre = hostNombre,
        tipo = tipo,
        anterior = anterior,
        nueva = nueva,
        nombreAEscribir = nombreAEscribir,
    )

    is PeticionDialogo.Contrasena -> PeticionDialogoVista.Contrasena(
        peticionId = id.toLong(),
        hostUuid = hostUuid,
        hostNombre = hostNombre,
        usuario = usuario,
        direccion = direccion,
        intentosRestantes = intentosRestantes.toInt(),
        recordarOfrecido = recordarOfrecido,
    )
}

fun RespuestaDialogoVista.aNucleo(): RespuestaDialogo = when (this) {
    RespuestaDialogoVista.AceptarHuella -> RespuestaDialogo.AceptarHuella
    RespuestaDialogoVista.CancelarHuella -> RespuestaDialogo.CancelarHuella
    is RespuestaDialogoVista.SustituirHuella -> RespuestaDialogo.SustituirHuella(nombreEscrito)
    is RespuestaDialogoVista.Contrasena -> RespuestaDialogo.Contrasena(
        bytes = bytes,
        recordar = recordar,
    )
    RespuestaDialogoVista.CancelarContrasena -> RespuestaDialogo.CancelarContrasena
}

// ===========================================================================
// Clave del móvil, versiones y otros
// ===========================================================================

fun ResultadoInstalacionClave.aVista(): ResultadoInstalacionVista = when (this) {
    ResultadoInstalacionClave.YA_ESTABA -> ResultadoInstalacionVista.YaEstaba
    ResultadoInstalacionClave.INSTALADA_Y_ACEPTADA -> ResultadoInstalacionVista.InstaladaAceptada
    ResultadoInstalacionClave.INSTALADA_PERO_NO_ACEPTADA ->
        ResultadoInstalacionVista.InstaladaNoAceptada
}

/**
 * Combina la línea pública que da el núcleo con la información del Keystore
 * que solo conoce Kotlin (dónde vive la clave y cuándo se creó).
 */
fun ClavePublicaMovil.aVista(
    ubicacion: UbicacionClaveVista,
    creadaEn: Long?,
    invalidada: Boolean,
): ClaveMovilVista = ClaveMovilVista(
    lineaOpenssh = lineaOpenssh,
    huellaSha256 = huellaSha256,
    ubicacion = ubicacion,
    creadaEn = creadaEn,
    invalidada = invalidada,
)

fun VersionNucleo.aVista(app: String, codigo: Int): VersionVista = VersionVista(
    app = app,
    codigo = codigo,
    nucleo = nucleo,
    formato = formato.toInt(),
    userVersion = userVersion.toInt(),
)

/**
 * Mensaje legible de un error del núcleo. El detalle de Rust ya viene en
 * castellano y con la instrucción cuando aplica; aquí solo se ponen en limpio
 * las variantes sin datos.
 */
fun ErrorNucleo.mensajeLegible(): String = when (this) {
    is ErrorNucleo.NoIniciado -> "El núcleo de MAGI no está iniciado."
    is ErrorNucleo.NoEncontrado -> detalle
    is ErrorNucleo.EstadoInvalido -> detalle
    is ErrorNucleo.Carpeta -> detalle
    is ErrorNucleo.Llavero -> detalle
    is ErrorNucleo.Firmante -> detalle
    is ErrorNucleo.Cancelado -> "Cancelado."
    is ErrorNucleo.Ocupado -> detalle
    is ErrorNucleo.HuellaSinAceptar -> "Huella sin aceptar: conéctate una vez desde la ficha."
    is ErrorNucleo.SinCredenciales -> "Sin credenciales: instala tu clave o guarda la contraseña."
    is ErrorNucleo.NoResponde -> "El host no responde."
    is ErrorNucleo.SudoRechazada ->
        "La contraseña de sudo es incorrecta: se ha olvidado; vuelve a pedirla."
    is ErrorNucleo.SinPermiso -> "Sin permiso: pide la contraseña de sudo."
    is ErrorNucleo.General -> detalle
}

/** Cualquier fallo del núcleo o de la app, con un mensaje para el usuario. */
fun mensajeDeError(error: Throwable): String = when (error) {
    is ErrorNucleo -> error.mensajeLegible()
    else -> error.message ?: "Error inesperado del núcleo de MAGI."
}

// ===========================================================================
// Fase 14 · errores y flota (informe §5.1, §6.1 y §6.2)
// ===========================================================================

/**
 * Convierte un fallo del núcleo en un error de acción legible. El detalle de
 * Rust ya viene en castellano y con la instrucción cuando aplica (huella sin
 * aceptar, sin credenciales, sin permiso, sudo rechazada…).
 */
fun Throwable.aResultadoAccion(): ResultadoAccion = ResultadoAccion.Error(mensajeDeError(this))

/** Igual que [aResultadoAccion], para una lectura que devuelve un dato. */
fun <T> Throwable.aLecturaFallida(): ResultadoNucleoVista<T> =
    ResultadoNucleoVista.Error(mensajeDeError(this))

fun EstadoFlotaMovil.aVista(): EstadoFlotaVista = when (this) {
    EstadoFlotaMovil.FRIA -> EstadoFlotaVista.FRIA
    EstadoFlotaMovil.SONDEANDO -> EstadoFlotaVista.SONDEANDO
    EstadoFlotaMovil.CAIDA -> EstadoFlotaVista.CAIDA
    EstadoFlotaMovil.ALCANZABLE -> EstadoFlotaVista.ALCANZABLE
    EstadoFlotaMovil.CARGA -> EstadoFlotaVista.CARGA
    EstadoFlotaMovil.NOMINAL -> EstadoFlotaVista.NOMINAL
    EstadoFlotaMovil.SIN_SONDEO -> EstadoFlotaVista.SIN_SONDEO
}

/** Resumen de métricas de la fila de Flota (solo trae los porcentajes y tasas). */
fun ResumenMetricas.aVista(): MetricasFlotaVista = MetricasFlotaVista(
    nucleos = nucleos?.toInt(),
    carga1m = carga1m,
    memoriaPct = memoriaPct,
    discoPct = discoPct,
    redRxBps = redRxBps,
    redTxBps = redTxBps,
    uptimeSeg = uptimeSeg,
)

/** Métricas completas del bloque FLOTA de la ficha. */
fun MetricasFlota.aVista(): MetricasFlotaVista = MetricasFlotaVista(
    nucleos = nucleos?.toInt(),
    carga1m = carga1m,
    carga5m = carga5m,
    carga15m = carga15m,
    memoriaTotalKb = memoriaTotalKb,
    memoriaDisponibleKb = memoriaDisponibleKb,
    memoriaPct = memoriaPct,
    discoTotalKb = discoTotalKb,
    discoUsadoKb = discoUsadoKb,
    discoPct = discoPct,
    redRxBytes = redRxBytes,
    redTxBytes = redTxBytes,
    redRxBps = redRxBps,
    redTxBps = redTxBps,
    uptimeSeg = uptimeSeg,
)

fun VigiladoFlota.aVista(): VigiladoVista = VigiladoVista(
    nombre = nombre,
    estado = estado,
    activo = activo,
)

fun TipoFalloMovil.aVista(): TipoFalloVista = when (this) {
    TipoFalloMovil.SERVICIO -> TipoFalloVista.SERVICIO
    TipoFalloMovil.CONTENEDOR -> TipoFalloVista.CONTENEDOR
}

fun FalloFlota.aVista(): FalloFlotaVista = FalloFlotaVista(
    tipo = tipo.aVista(),
    nombre = nombre,
    estado = estado,
    motor = motor?.aVista(),
)

/**
 * Fila de Flota. El contrato solo trae el recuento de fallos en la lista; los
 * nombres llegan en la ficha (DetalleFlota).
 */
fun HostFlota.aVista(): HostFlotaVista = HostFlotaVista(
    uuid = uuid,
    nombre = nombre,
    direccion = direccion,
    puerto = puerto.toInt(),
    tipo = tipo.aVista(),
    grupoUuid = grupoUuid,
    etiquetas = etiquetas,
    estado = estado.aVista(),
    culpables = culpables,
    metricas = resumen?.aVista(),
    fallos = FallosFlotaVista(total = fallos.toInt()),
    sesiones = sesiones.toInt(),
    ultimoSondeoEn = ultimoSondeoEn,
    motivo = motivo,
    duracionMs = duracionMs,
)

fun DetalleFlota.aVista(): DetalleFlotaVista = DetalleFlotaVista(
    estado = estado.aVista(),
    culpables = culpables,
    motivo = motivo,
    metricas = metricas?.aVista(),
    vigilados = vigilados.map { it.aVista() },
    fallos = fallos.map { it.aVista() },
    ultimoSondeoEn = ultimoSondeoEn,
    duracionMs = duracionMs,
    aviso = aviso,
)

fun VerificacionesMovil.aVista(): VerificacionesVista = VerificacionesVista(
    salud = salud,
    backup = backup,
    backupRuta = backupRuta,
    backupPatron = backupPatron,
    tests = tests,
    testsComando = testsComando,
)

fun SnippetAlConectarMovil.aVista(): SnippetAlConectarVista = SnippetAlConectarVista(
    uuid = uuid,
    nombre = nombre,
    valido = valido,
)

fun FilaTodaLaFlota.aVista(): FilaTodaLaFlotaVista = FilaTodaLaFlotaVista(
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    fallo = fallo.aVista(),
    sondeoEn = sondeoEn,
)

fun HostTodaLaFlota.aVista(): HostTodaLaFlotaVista = HostTodaLaFlotaVista(
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    estado = estado.aVista(),
    motivo = motivo,
    fallos = fallos.toInt(),
    sondeoEn = sondeoEn,
)

fun TodaLaFlota.aVista(): TodaLaFlotaVista = TodaLaFlotaVista(
    fallos = fallos.map { it.aVista() },
    hosts = hosts.map { it.aVista() },
)

// ===========================================================================
// Fase 14 · servicios (informe §5.1, §6.3 y §6.4)
// ===========================================================================

fun TipoObjetivoMovil.aVista(): TipoObjetivoVista = when (this) {
    TipoObjetivoMovil.UNIDAD -> TipoObjetivoVista.UNIDAD
    TipoObjetivoMovil.TIMER -> TipoObjetivoVista.TIMER
    TipoObjetivoMovil.CONTENEDOR -> TipoObjetivoVista.CONTENEDOR
    TipoObjetivoMovil.PROYECTO -> TipoObjetivoVista.PROYECTO
}

fun TipoObjetivoVista.aNucleo(): TipoObjetivoMovil = when (this) {
    TipoObjetivoVista.UNIDAD -> TipoObjetivoMovil.UNIDAD
    TipoObjetivoVista.TIMER -> TipoObjetivoMovil.TIMER
    TipoObjetivoVista.CONTENEDOR -> TipoObjetivoMovil.CONTENEDOR
    TipoObjetivoVista.PROYECTO -> TipoObjetivoMovil.PROYECTO
}

fun ObjetivoMovil.aVista(): ObjetivoVista = ObjetivoVista(tipo = tipo.aVista(), nombre = nombre)

fun ObjetivoVista.aNucleo(): ObjetivoMovil = ObjetivoMovil(tipo = tipo.aNucleo(), nombre = nombre)

fun AccionMovil.aVista(): AccionServicioVista = when (this) {
    AccionMovil.ARRANCAR -> AccionServicioVista.ARRANCAR
    AccionMovil.PARAR -> AccionServicioVista.PARAR
    AccionMovil.REINICIAR -> AccionServicioVista.REINICIAR
    AccionMovil.RECARGAR -> AccionServicioVista.RECARGAR
    AccionMovil.ACTIVAR -> AccionServicioVista.ACTIVAR
    AccionMovil.DESACTIVAR -> AccionServicioVista.DESACTIVAR
    AccionMovil.LANZAR -> AccionServicioVista.LANZAR
}

fun AccionServicioVista.aNucleo(): AccionMovil = when (this) {
    AccionServicioVista.ARRANCAR -> AccionMovil.ARRANCAR
    AccionServicioVista.PARAR -> AccionMovil.PARAR
    AccionServicioVista.REINICIAR -> AccionMovil.REINICIAR
    AccionServicioVista.RECARGAR -> AccionMovil.RECARGAR
    AccionServicioVista.ACTIVAR -> AccionMovil.ACTIVAR
    AccionServicioVista.DESACTIVAR -> AccionMovil.DESACTIVAR
    AccionServicioVista.LANZAR -> AccionMovil.LANZAR
}

fun PrivilegioMovil.aVista(): PrivilegioVista = when (this) {
    PrivilegioMovil.ROOT -> PrivilegioVista.ROOT
    PrivilegioMovil.SUDO_SIN_CLAVE -> PrivilegioVista.SUDO_SIN_CLAVE
    PrivilegioMovil.SUDO_CON_CLAVE -> PrivilegioVista.SUDO_CON_CLAVE
    PrivilegioMovil.SIN_SUDO -> PrivilegioVista.SIN_SUDO
}

fun PermisoDiarioMovil.aVista(): PermisoDiarioVista = when (this) {
    PermisoDiarioMovil.TODO -> PermisoDiarioVista.TODO
    PermisoDiarioMovil.PROPIO -> PermisoDiarioVista.PROPIO
}

fun SystemdMovil.aVista(): SystemdVista = when (this) {
    is SystemdMovil.Disponible -> SystemdVista.Disponible(v1)
    is SystemdMovil.Antiguo -> SystemdVista.Antiguo(v1)
    SystemdMovil.Ausente -> SystemdVista.Ausente
}

fun MotorNombreMovil.aVista(): MotorNombreVista = when (this) {
    MotorNombreMovil.DOCKER -> MotorNombreVista.DOCKER
    MotorNombreMovil.PODMAN -> MotorNombreVista.PODMAN
}

fun MotorNombreVista.aNucleo(): MotorNombreMovil = when (this) {
    MotorNombreVista.DOCKER -> MotorNombreMovil.DOCKER
    MotorNombreVista.PODMAN -> MotorNombreMovil.PODMAN
}

fun MotorMovil.aVista(): MotorVista = MotorVista(nombre = nombre.aVista(), version = version)

fun MotorEstadoMovil.aVista(): MotorEstadoVista = when (this) {
    is MotorEstadoMovil.Disponible -> MotorEstadoVista.DISPONIBLE(v1.aVista())
    MotorEstadoMovil.No -> MotorEstadoVista.NO
    MotorEstadoMovil.SinPermiso -> MotorEstadoVista.SIN_PERMISO
}

fun EstadoArranqueMovil.aVista(): EstadoArranqueVista = when (this) {
    EstadoArranqueMovil.ACTIVADO -> EstadoArranqueVista.ACTIVADO
    EstadoArranqueMovil.DESACTIVADO -> EstadoArranqueVista.DESACTIVADO
    EstadoArranqueMovil.ESTATICO -> EstadoArranqueVista.ESTATICO
    EstadoArranqueMovil.INDIRECTO -> EstadoArranqueVista.INDIRECTO
    EstadoArranqueMovil.GENERADO -> EstadoArranqueVista.GENERADO
    EstadoArranqueMovil.TRANSITORIO -> EstadoArranqueVista.TRANSITORIO
    EstadoArranqueMovil.ALIAS -> EstadoArranqueVista.ALIAS
    EstadoArranqueMovil.ENMASCARADO -> EstadoArranqueVista.ENMASCARADO
    EstadoArranqueMovil.DESCONOCIDO -> EstadoArranqueVista.DESCONOCIDO
}

fun RelojHostMovil.aVista(): RelojHostVista =
    RelojHostVista(epoch = epoch, uptimeSeg = uptimeSeg.toLong())

fun PropiedadMovil.aVista(): PropiedadVista = PropiedadVista(clave = clave, valor = valor)

/**
 * Unidad del listado. Las propiedades ya vienen parseadas por el núcleo y la
 * lista de acciones permitidas la calcula él: aquí no se decide nada. El PID,
 * la memoria y las tareas se leen de props con las ayudas de UnidadVista.
 */
fun UnidadMovil.aVista(): UnidadVista = UnidadVista(
    nombre = nombre,
    propiedades = props.map { it.aVista() },
    vigilada = vigilada,
    acciones = acciones.map { it.aVista() },
    desdeEn = desdeEn,
    arranque = arranque.aVista(),
)

fun TimerMovil.aVista(): TimerVista = TimerVista(
    nombre = nombre,
    propiedades = props.map { it.aVista() },
    servicio = servicio,
    acciones = acciones.map { it.aVista() },
    proximaEn = proximaEn,
    ultimaEn = ultimaEn,
)

fun EstadoContenedorMovil.aVista(): EstadoContenedorVista = when (this) {
    EstadoContenedorMovil.CREADO -> EstadoContenedorVista.CREADO
    EstadoContenedorMovil.EN_MARCHA -> EstadoContenedorVista.EN_MARCHA
    EstadoContenedorMovil.PARADO -> EstadoContenedorVista.PARADO
    EstadoContenedorMovil.REINICIANDO -> EstadoContenedorVista.REINICIANDO
    EstadoContenedorMovil.PAUSADO -> EstadoContenedorVista.PAUSADO
    EstadoContenedorMovil.MUERTO -> EstadoContenedorVista.MUERTO
    EstadoContenedorMovil.DESCONOCIDO -> EstadoContenedorVista.DESCONOCIDO
}

fun SaludMovil.aVista(): SaludVista = when (this) {
    SaludMovil.NINGUNA -> SaludVista.NINGUNA
    SaludMovil.SANA -> SaludVista.SANA
    SaludMovil.INSEGURA -> SaludVista.INSEGURA
    SaludMovil.COMPROBANDO -> SaludVista.COMPROBANDO
}

fun ContenedorMovil.aVista(): ContenedorVista = ContenedorVista(
    id = id,
    nombre = nombre,
    imagen = imagen,
    estado = estado.aVista(),
    codigoSalida = codigoSalida,
    salud = salud.aVista(),
    puertos = puertos,
    proyecto = proyecto,
    ficheros = ficheros,
    servicio = servicio,
    orden = orden,
    etiquetas = etiquetas.map { it.aVista() },
    acciones = acciones.map { it.aVista() },
)

fun ProyectoMovil.aVista(): ProyectoVista = ProyectoVista(
    nombre = nombre,
    ficheros = ficheros,
    acciones = acciones.map { it.aVista() },
)

fun ListadoServicios.aVista(): ListadoServiciosVista = ListadoServiciosVista(
    systemd = systemd.aVista(),
    privilegio = privilegio.aVista(),
    diario = diario.aVista(),
    reloj = reloj.aVista(),
    motor = motor.aVista(),
    unidades = unidades.map { it.aVista() },
    timers = timers.map { it.aVista() },
    contenedores = contenedores.map { it.aVista() },
    proyectos = proyectos.map { it.aVista() },
    avisos = avisos,
    incompleto = incompleto,
    sudoEnMemoria = sudoEnMemoria,
    hora = hora,
)

fun Consumo.aVista(): ConsumoVista = ConsumoVista(
    id = id,
    cpuPct = cpuPct,
    memoriaBytes = memoriaBytes?.toLong(),
    memoriaLimiteBytes = memoriaLimiteBytes?.toLong(),
)

fun DetalleContenedorMovil.aVista(): DetalleContenedorVista = DetalleContenedorVista(
    id = id,
    nombre = nombre,
    imagen = imagen,
    creado = creado,
    estado = estado.aVista(),
    codigoSalida = codigoSalida,
    reinicios = reinicios.toLong(),
    salud = salud.aVista(),
    puertos = puertos,
    montajes = montajes,
    redes = redes,
    proyecto = proyecto,
    orden = orden,
    texto = texto,
)

fun DetalleServicios.aVista(): DetalleServiciosVista = DetalleServiciosVista(
    objetivo = objetivo.aVista(),
    propiedades = props.map { it.aVista() },
    necesita = necesita,
    necesitada = necesitada,
    texto = texto,
    contenedor = contenedor?.aVista(),
)

fun InfoAccionMovil.aVista(): InfoAccionVista = InfoAccionVista(
    permitida = permitida,
    motivoNoPermitida = motivo,
    delibera = delibera,
    sudoConClave = pideSudo,
    comprobacion = comprobacion,
    critica = critica,
    hostsTraves = hostsCriticos,
)

fun ResultadoAccionMovil.aVista(): ResultadoAccionServicioVista = when (this) {
    ResultadoAccionMovil.HECHO -> ResultadoAccionServicioVista.HECHO
    ResultadoAccionMovil.FALLIDO -> ResultadoAccionServicioVista.FALLIDO
    ResultadoAccionMovil.SIN_CONFIRMAR -> ResultadoAccionServicioVista.SIN_CONFIRMAR
    ResultadoAccionMovil.RECHAZADA_CONFIGURACION ->
        ResultadoAccionServicioVista.RECHAZADA_CONFIGURACION
    ResultadoAccionMovil.ERROR -> ResultadoAccionServicioVista.ERROR
}

/**
 * Resultado de una acción. El núcleo manda un solo bloque de líneas: son las
 * últimas del diario si falló o la salida del validador si la rechazó; aquí se
 * separan para que la hoja de resultado pinte lo que toca.
 */
fun ResultadoServicioMovil.aVista(): ResultadoServicioVista {
    val resultadoVista = resultado.aVista()
    val rechazada = resultadoVista == ResultadoAccionServicioVista.RECHAZADA_CONFIGURACION
    return ResultadoServicioVista(
        hostUuid = hostUuid,
        hostNombre = hostNombre,
        objetivo = objetivo.aVista(),
        accion = accion.aVista(),
        resultado = resultadoVista,
        estado = estado,
        segundos = segundos,
        mensaje = mensaje,
        lineasDiario = if (rechazada) emptyList() else lineas,
        salidaValidador = if (rechazada) lineas else emptyList(),
        comprobacion = comprobacion,
    )
}

fun FaseAccionMovil.aVista(): FaseAccionVista = when (this) {
    FaseAccionMovil.COMPROBANDO -> FaseAccionVista.COMPROBANDO
    FaseAccionMovil.EJECUTANDO -> FaseAccionVista.EJECUTANDO
    FaseAccionMovil.ESPERANDO -> FaseAccionVista.ESPERANDO
}

fun AccionServicioMovil.aVista(): AccionEnCursoVista = AccionEnCursoVista(
    id = id.toLong(),
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    objetivo = objetivo.aVista(),
    accion = accion.aVista(),
    fase = fase.aVista(),
    deliberacionId = deliberacionId?.toLong(),
    desdeMs = desdeMs,
)

fun ResultadoSudo.aVista(): ResultadoSudoVista = when (this) {
    ResultadoSudo.OK -> ResultadoSudoVista.OK
    ResultadoSudo.RECHAZADA -> ResultadoSudoVista.RECHAZADA
    ResultadoSudo.SIN_SUDO -> ResultadoSudoVista.SIN_SUDO
}

/** Pestaña con orden que la app manda al núcleo (tipo cerrado, T108). */
fun OrdenPestanaVista.aNucleo(): OrdenPestanaMovil = when (this) {
    is OrdenPestanaVista.ShellContenedor ->
        OrdenPestanaMovil.ShellContenedor(motor = motor.aNucleo(), id = id)
    is OrdenPestanaVista.SeguirDiario ->
        OrdenPestanaMovil.SeguirDiario(unidad = unidad)
    is OrdenPestanaVista.SeguirContenedor ->
        OrdenPestanaMovil.SeguirContenedor(motor = motor.aNucleo(), id = id)
}

// ===========================================================================
// Fase 14 · logs (informe §5.1, §6.5 y §7.4)
// ===========================================================================

fun NivelLogMovil.aVista(): NivelLogVista = when (this) {
    NivelLogMovil.TODO -> NivelLogVista.TODO
    NivelLogMovil.INFO -> NivelLogVista.INFO
    NivelLogMovil.AVISO -> NivelLogVista.AVISO
    NivelLogMovil.ERROR -> NivelLogVista.ERROR
}

fun NivelLogVista.aNucleo(): NivelLogMovil = when (this) {
    NivelLogVista.TODO -> NivelLogMovil.TODO
    NivelLogVista.INFO -> NivelLogMovil.INFO
    NivelLogVista.AVISO -> NivelLogMovil.AVISO
    NivelLogVista.ERROR -> NivelLogMovil.ERROR
}

fun EstadoSuscripcionLog.aVista(): EstadoSuscripcionLogVista = when (this) {
    EstadoSuscripcionLog.SIGUIENDO -> EstadoSuscripcionLogVista.SIGUIENDO
    EstadoSuscripcionLog.PARADA -> EstadoSuscripcionLogVista.PARADA
    EstadoSuscripcionLog.TERMINADA -> EstadoSuscripcionLogVista.TERMINADA
    EstadoSuscripcionLog.SIN_PERMISO -> EstadoSuscripcionLogVista.SIN_PERMISO
    EstadoSuscripcionLog.CONEXION_CAIDA -> EstadoSuscripcionLogVista.CONEXION_CAIDA
    EstadoSuscripcionLog.NO_ENCONTRADA -> EstadoSuscripcionLogVista.NO_ENCONTRADA
}

fun MotivoFinLog.aVista(): MotivoFinLogVista = when (this) {
    MotivoFinLog.CERRADO -> MotivoFinLogVista.CERRADO
    MotivoFinLog.TERMINADO -> MotivoFinLogVista.TERMINADO
    MotivoFinLog.SIN_PERMISO -> MotivoFinLogVista.SIN_PERMISO
    MotivoFinLog.CONEXION_CAIDA -> MotivoFinLogVista.CONEXION_CAIDA
    MotivoFinLog.NO_ENCONTRADO -> MotivoFinLogVista.NO_ENCONTRADO
}

/** Una línea del anillo, con su nivel ya derivado de la prioridad y stderr. */
fun LineaLogMovil.aVista(): LineaLogVista = LineaLogVista(
    numero = numero.toLong(),
    texto = texto,
    nivel = nivelLineaLog(prioridad.toInt(), stderr),
    hora = hora,
    stderr = stderr,
)

fun FiltroLogsVista.aNucleo(): FiltroLogs = FiltroLogs(
    texto = texto,
    nivel = nivel.aNucleo(),
    soloStderr = soloStderr,
    desde = desde?.toULong(),
    cuantas = cuantasAcotadas.toUInt(),
)

fun EstadoLogs.aVista(): EstadoLogsVista = EstadoLogsVista(
    suscripcion = suscripcion.toLong(),
    total = total.toLong(),
    nuevas = nuevas.toLong(),
    omitidas = omitidas.toLong(),
    estado = estado.aVista(),
)

fun VentanaLogs.aVista(): VentanaLogsVista = VentanaLogsVista(
    lineas = lineas.map { it.aVista() },
    total = total.toLong(),
    omitidas = omitidas.toLong(),
    estado = estado.aVista(),
    motivo = motivo?.aVista(),
)

// ===========================================================================
// Fase 14 · snippets (informe §5.1, §6.8 y §7.6)
// ===========================================================================

fun HostDestinoMovil.aVista(): HostDestinoVista =
    HostDestinoVista(uuid = uuid, nombre = nombre)

fun DestinoMovil.aVista(): DestinoSnippetVista = when (this) {
    is DestinoMovil.Etiqueta -> DestinoSnippetVista.Etiqueta(v1)
    is DestinoMovil.Host -> DestinoSnippetVista.Host(v1.aVista())
}

fun VariableSnippetMovil.aVista(): VariableSnippetVista = VariableSnippetVista(
    nombre = nombre,
    valorDefecto = valorDefecto,
    valor = valor,
)

/** Variable que la app manda al núcleo al preparar una ejecución. */
fun VariableSnippetVista.aNucleo(): VariableSnippetMovil = VariableSnippetMovil(
    nombre = nombre,
    valorDefecto = valorDefecto,
    valor = valor,
)

fun SnippetMovil.aVista(): SnippetVista = SnippetVista(
    uuid = uuid,
    nombre = nombre,
    comando = comando,
    descripcion = descripcion,
    etiquetas = etiquetas,
    critico = critico,
    timeoutSeg = timeoutSeg.toInt(),
    pararAlFallo = pararAlFallo,
    usadoVeces = usadoVeces.toLong(),
    ultimoUsoEn = ultimoUsoEn,
    destinos = destinos.map { it.aVista() },
    hosts = hosts.map { it.aVista() },
    variables = variables.map { it.aVista() },
    escritoriosOmitidos = escritoriosOmitidos.toInt(),
)

fun ErrorCampoMovil.aVista(): ErrorCampoVista = ErrorCampoVista(campo = campo, mensaje = mensaje)

fun PreparacionEjecucion.aVista(): PreparacionEjecucionVista = PreparacionEjecucionVista(
    snippetUuid = snippetUuid,
    snippetNombre = snippetNombre,
    comando = comando,
    hosts = hosts.map { it.aVista() },
    variables = variables.map { it.aVista() },
    errores = errores.map { it.aVista() },
    delibera = delibera,
    motivos = motivos,
    critico = critico,
    timeoutSeg = timeoutSeg.toInt(),
    pararAlFallo = pararAlFallo,
    valores = valores.associate { it.clave to it.valor },
)

/**
 * La preparación de vuelta al núcleo para lanzar la ejecución: es el tipo
 * cerrado que espera ejecutar_snippet, con los valores de las variables.
 */
fun PreparacionEjecucionVista.aNucleo(): PreparacionEjecucion = PreparacionEjecucion(
    snippetUuid = snippetUuid,
    snippetNombre = snippetNombre,
    comando = comando,
    hosts = hosts.map { HostDestinoMovil(uuid = it.uuid, nombre = it.nombre) },
    variables = variables.map { it.aNucleo() },
    errores = errores.map { ErrorCampoMovil(campo = it.campo, mensaje = it.mensaje) },
    delibera = delibera,
    motivos = motivos,
    critico = critico,
    timeoutSeg = timeoutSeg.toUInt(),
    pararAlFallo = pararAlFallo,
    valores = valores.map { (clave, valor) -> PropiedadMovil(clave = clave, valor = valor) },
)

// ===========================================================================
// Fase 14 · ejecuciones (informe §5.1, §6.9 y §7.6)
// ===========================================================================

fun EstadoHostEjecucionMovil.aVista(): EstadoHostEjecucionVista = when (this) {
    EstadoHostEjecucionMovil.EN_COLA -> EstadoHostEjecucionVista.EN_COLA
    EstadoHostEjecucionMovil.CONECTANDO -> EstadoHostEjecucionVista.CONECTANDO
    EstadoHostEjecucionMovil.EJECUTANDO -> EstadoHostEjecucionVista.EJECUTANDO
    EstadoHostEjecucionMovil.OK -> EstadoHostEjecucionVista.OK
    EstadoHostEjecucionMovil.FALLO -> EstadoHostEjecucionVista.FALLO
    EstadoHostEjecucionMovil.ERROR -> EstadoHostEjecucionVista.ERROR
    EstadoHostEjecucionMovil.CANCELADO -> EstadoHostEjecucionVista.CANCELADO
    EstadoHostEjecucionMovil.OMITIDO -> EstadoHostEjecucionVista.OMITIDO
}

fun ResultadoEjecucionMovil.aVista(): ResultadoEjecucionVista = when (this) {
    ResultadoEjecucionMovil.OK -> ResultadoEjecucionVista.OK
    ResultadoEjecucionMovil.PARCIAL -> ResultadoEjecucionVista.PARCIAL
    ResultadoEjecucionMovil.ERROR -> ResultadoEjecucionVista.ERROR
    ResultadoEjecucionMovil.CANCELADA -> ResultadoEjecucionVista.CANCELADA
}

fun HostEjecucionMovil.aVista(): HostEjecucionVista = HostEjecucionVista(
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    estado = estado.aVista(),
    codigo = codigo,
    duracionMs = duracionMs,
    bytesStdout = bytesStdout.toLong(),
    bytesStderr = bytesStderr.toLong(),
    error = error,
)

fun EjecucionMovil.aVista(): EjecucionVista = EjecucionVista(
    id = id.toLong(),
    snippetUuid = snippetUuid,
    snippetNombre = snippetNombre,
    hosts = hosts.map { it.aVista() },
    timeoutSeg = timeoutSeg.toInt(),
    pararAlFallo = pararAlFallo,
    deliberacionId = deliberacionId?.toLong(),
    creadaEn = creadaEn,
    terminadaEn = terminadaEn,
    resultado = resultado?.aVista(),
    cancelada = cancelada,
    forzada = forzada,
)

fun SalidaEjecucion.aVista(): SalidaEjecucionVista = SalidaEjecucionVista(
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    codigo = codigo,
    stdout = stdout,
    stderr = stderr,
    stdoutTruncada = stdoutTruncada,
    stderrTruncada = stderrTruncada,
    duracionMs = duracionMs,
)

// ===========================================================================
// Fase 14 · deliberación (informe §5.1, §6.6, §6.7 y §7.7)
// ===========================================================================

fun VeredictoMovil.aVista(): VeredictoVista = when (this) {
    VeredictoMovil.NoActiva -> VeredictoVista.NoActiva
    VeredictoMovil.Pendiente -> VeredictoVista.Pendiente
    is VeredictoMovil.Aprueba -> VeredictoVista.Aprueba(detalle = detalle, ms = ms.toLong())
    is VeredictoMovil.Rechaza -> VeredictoVista.Rechaza(detalle = detalle, ms = ms.toLong())
}

fun FilaHostDeliberacion.aVista(): FilaHostDeliberacionVista = FilaHostDeliberacionVista(
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    salud = salud.aVista(),
    backup = backup.aVista(),
    tests = tests.aVista(),
)

fun EstadoDeliberacion.aVista(): EstadoDeliberacionVista = when (this) {
    EstadoDeliberacion.APROBADO -> EstadoDeliberacionVista.APROBADA
    EstadoDeliberacion.BLOQUEADO -> EstadoDeliberacionVista.BLOQUEADA
    EstadoDeliberacion.FORZADO -> EstadoDeliberacionVista.FORZADA
}

fun TablaDeliberacion.aVista(): TablaDeliberacionVista = TablaDeliberacionVista(
    id = id.toLong(),
    filas = filas.map { it.aVista() },
    aprobadas = aprobadas.toInt(),
    activas = activas.toInt(),
    rechazos = rechazos.toInt(),
    pendientes = pendientes.toInt(),
    unanime = unanime,
    estado = estado.aVista(),
)

/** El plan de deliberación que la app manda al núcleo (tipo cerrado). */
fun PlanDeliberacionVista.aNucleo(): PlanDeliberadoMovil = when (this) {
    is PlanDeliberacionVista.Snippet -> PlanDeliberadoMovil.Snippet(preparacion.aNucleo())
    is PlanDeliberacionVista.Servicio -> PlanDeliberadoMovil.Servicio(
        host = hostUuid,
        objetivo = objetivo.aNucleo(),
        accion = accionServicio.aNucleo(),
        comprobacion = comprobacion,
    )
}

fun DecisionDeliberacionVista.aNucleo(): DecisionDeliberacion = when (this) {
    DecisionDeliberacionVista.Aprobar -> DecisionDeliberacion.Aprobar
    is DecisionDeliberacionVista.Forzar -> DecisionDeliberacion.Forzar(motivo)
    DecisionDeliberacionVista.Cancelar -> DecisionDeliberacion.Cancelar
}

// ===========================================================================
// Fase 14 · registro (informe §5.1, §6.10 y §7.10)
// ===========================================================================

fun ResultadoRegistroMovil.aVista(): ResultadoRegistroVista = when (this) {
    ResultadoRegistroMovil.OK -> ResultadoRegistroVista.OK
    ResultadoRegistroMovil.ERROR -> ResultadoRegistroVista.ERROR
}

fun TipoRegistroVista.aNucleo(): TipoRegistroFiltro = when (this) {
    TipoRegistroVista.CONEXIONES -> TipoRegistroFiltro.CONEXIONES
    TipoRegistroVista.HUELLAS -> TipoRegistroFiltro.HUELLAS
    TipoRegistroVista.TUNELES -> TipoRegistroFiltro.TUNELES
    TipoRegistroVista.SONDEOS -> TipoRegistroFiltro.SONDEOS
    TipoRegistroVista.SNIPPETS -> TipoRegistroFiltro.SNIPPETS
    TipoRegistroVista.DELIBERACIONES -> TipoRegistroFiltro.DELIBERACIONES
    TipoRegistroVista.SERVICIOS -> TipoRegistroFiltro.SERVICIOS
    TipoRegistroVista.CLAVES -> TipoRegistroFiltro.CLAVES
    TipoRegistroVista.SINCRONIZACION -> TipoRegistroFiltro.SINCRONIZACION
}

fun FiltroRegistroVista.aNucleo(): FiltroRegistro = FiltroRegistro(
    hostUuid = hostUuid,
    tipos = tipos.map { it.aNucleo() },
    texto = texto,
)

fun EntradaRegistro.aVista(): EntradaRegistroVista = EntradaRegistroVista(
    id = id.toLong(),
    fecha = fecha,
    tipo = tipo,
    hostUuid = hostUuid,
    hostNombre = hostNombre,
    resultado = resultado.aVista(),
    detalle = detalle,
)

fun ResultadoDeliberacionMovil.aVista(): ResultadoDeliberacionRegistroVista = when (this) {
    ResultadoDeliberacionMovil.APROBADA -> ResultadoDeliberacionRegistroVista.APROBADA
    ResultadoDeliberacionMovil.FORZADA -> ResultadoDeliberacionRegistroVista.FORZADA
    ResultadoDeliberacionMovil.CANCELADA -> ResultadoDeliberacionRegistroVista.CANCELADA
}

fun DetalleDeliberacion.aVista(): DetalleDeliberacionRegistroVista =
    DetalleDeliberacionRegistroVista(
        id = id.toLong(),
        fecha = fecha,
        accion = accion,
        resultado = resultado.aVista(),
        bloqueada = bloqueada,
        motivo = motivo,
        usuario = usuario,
        ejecucionResultado = ejecucionResultado?.aVista(),
        sinResultado = sinResultado,
        filas = filas.map { it.aVista() },
        aprobadas = aprobadas.toInt(),
        activas = activas.toInt(),
    )

fun DetalleRegistro.aVista(): DetalleRegistroVista = DetalleRegistroVista(
    entrada = entrada.aVista(),
    deliberacion = deliberacion?.aVista(),
)

// ===========================================================================
// Fase 14 · ajustes (informe §5.4)
// ===========================================================================

fun AjustesMovilVista.aNucleo(): AjustesMovil = AjustesMovil(
    flotaRefrescoSeg = flotaRefrescoSeg.toULong(),
    umbralCargaPorNucleo = umbralCargaPorNucleo,
    umbralMemoriaPct = umbralMemoriaPct,
    umbralDiscoPct = umbralDiscoPct,
    serviciosRefrescoSeg = serviciosRefrescoSeg.toULong(),
    serviciosEsperaSeg = serviciosEsperaSeg.toULong(),
    serviciosContenedores = serviciosContenedores,
    sudoMemoriaMin = sudoMemoriaMin.toULong(),
    deliberacionBackupHoras = deliberacionBackupHoras.toULong(),
    deliberacionSaludMaxMin = deliberacionSaludMaxMin.toULong(),
)



