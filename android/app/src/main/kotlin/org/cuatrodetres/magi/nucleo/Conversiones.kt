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
    is ErrorNucleo.General -> detalle
}

/** Cualquier fallo del núcleo o de la app, con un mensaje para el usuario. */
fun mensajeDeError(error: Throwable): String = when (error) {
    is ErrorNucleo -> error.mensajeLegible()
    else -> error.message ?: "Error inesperado del núcleo de MAGI."
}
