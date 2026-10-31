//! Gestor del núcleo móvil: una conexión viva por host (compartida por
//! sesiones y túneles), sesiones con su terminal por tramos, túneles local,
//! remoto y dinámico, automáticos, trabajo vivo e «instalar mi clave».

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use magi_nucleo::almacen::{hosts as hosts_bd, movil as movil_bd};
use magi_nucleo::conexion::cliente::{abrir_canal, ClaveMovil, Contexto};
use magi_nucleo::conexion::reenvios::{Contadores, Reenvio, Reenvios};
use magi_nucleo::conexion::salto::{conectar_cadena_movil, construir_cadena};
use magi_nucleo::conexion::terminal::Pantalla;
use magi_nucleo::conexion::{EventoConexion, FuenteContrasena};
use magi_nucleo::modelo::{Host, TipoTunel};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::terminal::teclas::{self, ModoTerminal};
use magi_nucleo::terminal::tramos::GeneradorTramos;
use magi_nucleo::tuneles::escucha::{
    abrir_escucha_movil, escucha_movil, partir_escucha, validar_puerto_movil,
};
use magi_nucleo::tuneles::local::{servir_local, servir_socks};
use russh::client::Handle;
use russh::ChannelMsg;
use tokio::sync::{mpsc, watch};

use crate::api::{
    Color, ErrorNucleo, EstadoSesion, EstadoTunel, FilaPantalla, Modificadores,
    Pantalla as PantallaDto, PeticionDialogo, Posicion, RespuestaDialogo,
    ResultadoInstalacionClave, Tecla, Tramo, TunelVivo,
};
use crate::estado::{
    con_almacen, ComandoSesion, ConexionHost, EstadoNucleo, EstadoTunelVivo, SesionViva,
    TunelActivo,
};

/// Plazo de las operaciones con el host (pedir reenvíos, abrir canales).
const PLAZO: Duration = Duration::from_secs(10);

/// Conexión viva del host, compartida por sus sesiones y túneles.
pub async fn conexion_para_host(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<Arc<ConexionHost>, ErrorNucleo> {
    {
        let conexiones = estado.conexiones.lock().await;
        if let Some(conexion) = conexiones.get(host_uuid) {
            return Ok(conexion.clone());
        }
    }
    let cerrojo = estado.cerrojo_de_host(host_uuid);
    let _guardia = cerrojo.lock().await;
    {
        let conexiones = estado.conexiones.lock().await;
        if let Some(conexion) = conexiones.get(host_uuid) {
            return Ok(conexion.clone());
        }
    }
    let conexion = conectar_host(estado, host_uuid).await?;
    estado
        .conexiones
        .lock()
        .await
        .insert(host_uuid.to_string(), conexion.clone());
    Ok(conexion)
}

async fn conectar_host(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<Arc<ConexionHost>, ErrorNucleo> {
    let (host, otros) = con_almacen(estado, {
        let host_uuid = host_uuid.to_string();
        move |almacen| {
            let conexion = almacen.conexion();
            let host = movil_bd::host_por_uuid(conexion, &host_uuid)?;
            let otros = hosts_bd::listar(conexion)?;
            Ok::<_, anyhow::Error>((host, otros))
        }
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el host: {error:#}"),
    })?;
    let Some(host) = host else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el host {host_uuid}"),
        });
    };
    let mapa: HashMap<i64, Host> = otros.into_iter().map(|host| (host.id, host)).collect();
    let cadena = construir_cadena(&host, &mapa).map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?;
    let (tx, rx) = mpsc::unbounded_channel::<EventoConexion>();
    lanzar_bucle_eventos(estado.clone(), rx);
    let reenvios = Arc::new(Reenvios::default());
    let contexto = Contexto {
        known_hosts: estado.rutas.fichero_known_hosts(),
        dir_ssh: estado.rutas.dir_ssh(),
        hogar: estado.rutas.hogar.clone(),
        usuario_local: host
            .usuario
            .clone()
            .unwrap_or_else(|| "usuario".to_string()),
        tx: tx.clone(),
        interactivo: true,
        fuente_contrasena: FuenteContrasena::Llavero,
        reenvios: Some(reenvios.clone()),
        llavero: estado.llavero.clone(),
    };
    let publica = estado.clave_publica()?;
    let clave = ClaveMovil {
        linea_publica: publica.linea_openssh,
        firmante: estado.firmante.clone(),
    };
    let transporte = conectar_cadena_movil(&cadena, &contexto, &clave)
        .await
        .map_err(|error| {
            let motivo = error.to_string();
            estado.anotar_error(host_uuid, &motivo);
            registrar_fallo(estado, host.id, &motivo);
            ErrorNucleo::General { detalle: motivo }
        })?;
    estado.limpiar_error(host_uuid);
    let identidad = transporte.identidad.descripcion.clone();
    let host_id = host.id;
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::CONEXION_ABIERTA,
            Some(host_id),
            None,
            &format!("móvil · {identidad}"),
            ResultadoRegistro::Ok,
        )
    });
    Ok(Arc::new(ConexionHost {
        handle: transporte.handle,
        saltos: transporte.saltos,
        reenvios,
        tx,
    }))
}

fn registrar_fallo(estado: &EstadoNucleo, host_id: i64, motivo: &str) {
    let motivo = motivo.to_string();
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::CONEXION_FALLIDA,
            Some(host_id),
            None,
            &motivo,
            ResultadoRegistro::Error,
        )
    });
}

/// Traduce los eventos de conexión a diálogos de la app y respuestas.
fn lanzar_bucle_eventos(
    estado: Arc<EstadoNucleo>,
    mut eventos: mpsc::UnboundedReceiver<EventoConexion>,
) {
    let manejador = estado.runtime.handle().clone();
    manejador.spawn(async move {
        while let Some(evento) = eventos.recv().await {
            match evento {
                EventoConexion::HuellaDesconocida {
                    host,
                    tipo,
                    huella,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::HuellaNueva {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: datos
                                .as_ref()
                                .map(|d| d.1.clone())
                                .unwrap_or_else(|| host.clone()),
                            tipo,
                            sha256: huella,
                        });
                    let aceptado = matches!(receptor.await, Ok(RespuestaDialogo::AceptarHuella));
                    let _ = responder.send(aceptado);
                }
                EventoConexion::HuellaCambiada {
                    host,
                    tipo,
                    anterior,
                    nueva,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let nombre = datos
                        .as_ref()
                        .map(|d| d.1.clone())
                        .unwrap_or_else(|| host.clone());
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::HuellaCambiada {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: nombre.clone(),
                            tipo,
                            anterior,
                            nueva,
                            nombre_a_escribir: nombre.clone(),
                        });
                    let sustituir = match receptor.await {
                        Ok(RespuestaDialogo::SustituirHuella { nombre_escrito }) => {
                            nombre_escrito.trim() == nombre.trim()
                        }
                        _ => false,
                    };
                    let _ = responder.send(sustituir);
                }
                EventoConexion::PideContrasena {
                    host,
                    intento,
                    recordar_por_defecto,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::Contrasena {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: datos
                                .as_ref()
                                .map(|d| d.1.clone())
                                .unwrap_or_else(|| host.clone()),
                            usuario: datos.as_ref().map(|d| d.2.clone()).unwrap_or_default(),
                            direccion: host,
                            intentos_restantes: u32::from(4u8.saturating_sub(intento)),
                            recordar_ofrecido: recordar_por_defecto,
                        });
                    match receptor.await {
                        Ok(RespuestaDialogo::Contrasena { bytes, recordar }) => {
                            let texto = String::from_utf8_lossy(&bytes).to_string();
                            let contrasena = zeroize::Zeroizing::new(texto);
                            let _ = responder.send(Some((contrasena, recordar)));
                        }
                        _ => {
                            let _ = responder.send(None);
                        }
                    }
                }
                EventoConexion::PideContrasenaLlavero {
                    host,
                    usuario,
                    responder,
                } => {
                    let contrasena = magi_nucleo::llavero::LlaveroHost::recuperar_contrasena(
                        estado.llavero.as_ref(),
                        &host,
                        &usuario,
                    )
                    .ok()
                    .flatten();
                    let _ = responder.send(contrasena);
                }
                EventoConexion::PideFrase { responder, .. } => {
                    let _ = responder.send(None);
                }
                EventoConexion::HuellaRegistrada {
                    host_id,
                    anterior,
                    nueva,
                } => {
                    let tipo = if anterior.is_some() {
                        registro::HUELLA_SUSTITUIDA
                    } else {
                        registro::HUELLA_ACEPTADA
                    };
                    let detalle = match &anterior {
                        Some(anterior) => format!("anterior {anterior}; nueva {nueva}"),
                        None => format!("nueva {nueva}"),
                    };
                    let _ = estado.escritor.con_almacen(move |almacen| {
                        registro::anotar(
                            almacen.conexion(),
                            tipo,
                            Some(host_id),
                            None,
                            &detalle,
                            ResultadoRegistro::Ok,
                        )
                    });
                }
                _ => {}
            }
        }
    });
}

fn datos_host_por_direccion(
    estado: &EstadoNucleo,
    direccion: &str,
) -> Option<(String, String, String)> {
    let direccion = direccion.to_string();
    con_almacen(estado, move |almacen| {
        let conexion = almacen.conexion();
        let hosts = hosts_bd::listar(conexion).ok()?;
        let host = hosts.into_iter().find(|host| host.direccion == direccion)?;
        let uuid = movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)
            .ok()?
            .get(&host.id)
            .cloned()
            .unwrap_or_default();
        Some((
            uuid,
            host.nombre.clone(),
            host.usuario.clone().unwrap_or_default(),
        ))
    })
    .ok()
    .flatten()
}

// ----------------------------------------------------------------- sesiones

/// Abre una sesión nueva contra el host y devuelve su id.
pub async fn abrir_sesion(estado: &Arc<EstadoNucleo>, host_uuid: &str) -> Result<u64, ErrorNucleo> {
    let host = con_almacen(estado, {
        let host_uuid = host_uuid.to_string();
        move |almacen| movil_bd::host_por_uuid(almacen.conexion(), &host_uuid)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el host: {error:#}"),
    })?
    .ok_or_else(|| ErrorNucleo::NoEncontrado {
        detalle: format!("el host {host_uuid}"),
    })?;
    let id = estado.siguiente_id();
    let pantalla = nueva_pantalla(24, 80);
    let sesion = nueva_sesion(
        id,
        host_uuid,
        &host.nombre,
        pantalla,
        EstadoSesion::Abriendo,
        Some("resolviendo".to_string()),
    );
    estado
        .sesiones
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de sesiones envenenado".to_string(),
        })?
        .insert(id, sesion.clone());
    estado.emitir_sesiones();
    let estado_tarea = estado.clone();
    estado.runtime.spawn(async move {
        abrir_y_servir(estado_tarea, sesion).await;
    });
    Ok(id)
}

/// Parser de la sesión con 1000 líneas de historial (checklist de la Fase 11).
fn nueva_pantalla(filas: u16, columnas: u16) -> Pantalla {
    std::sync::Arc::new(std::sync::Mutex::new(vt100::Parser::new(
        filas, columnas, 1000,
    )))
}

fn nueva_sesion(
    id: u64,
    host_uuid: &str,
    host_nombre: &str,
    pantalla: Pantalla,
    estado_inicial: EstadoSesion,
    detalle: Option<String>,
) -> Arc<SesionViva> {
    let (comandos, _receptor) = mpsc::unbounded_channel();
    Arc::new(SesionViva {
        id,
        host_uuid: host_uuid.to_string(),
        host_nombre: host_nombre.to_string(),
        estado: std::sync::Mutex::new(estado_inicial),
        detalle: std::sync::Mutex::new(detalle),
        comandos: std::sync::Mutex::new(comandos),
        pantalla,
        abierta_en: Some(chrono::Utc::now().timestamp()),
    })
}

async fn abrir_y_servir(estado: Arc<EstadoNucleo>, sesion: Arc<SesionViva>) {
    let host_uuid = sesion.host_uuid.clone();
    let conexion = match conexion_para_host(&estado, &host_uuid).await {
        Ok(conexion) => conexion,
        Err(error) => {
            fallar_sesion(&estado, &sesion, &error.to_string());
            return;
        }
    };
    sesion.fijar_estado(EstadoSesion::Abriendo, Some("conectando".to_string()));
    estado.emitir_sesiones();
    let (filas, columnas) = match sesion.pantalla.lock() {
        Ok(pantalla) => pantalla.screen().size(),
        Err(_) => (24, 80),
    };
    let mut canal = match abrir_canal(&conexion.handle, columnas, filas).await {
        Ok(canal) => canal,
        Err(error) => {
            fallar_sesion(&estado, &sesion, &error.to_string());
            return;
        }
    };
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    if let Ok(mut comandos) = sesion.comandos.lock() {
        *comandos = tx_comandos;
    }
    sesion.fijar_estado(EstadoSesion::Abierta, None);
    let _ = estado.escritor.con_almacen({
        let host_uuid = host_uuid.clone();
        move |almacen| movil_bd::marcar_conexion(almacen.conexion(), &host_uuid)
    });
    estado.limpiar_error(&host_uuid);
    estado.emitir_sesiones();
    actualizar_automaticos(&estado, &host_uuid);
    bucle_sesion(estado, sesion, &mut canal, rx_comandos).await;
}

fn fallar_sesion(estado: &EstadoNucleo, sesion: &SesionViva, motivo: &str) {
    if let Ok(mut sesiones) = estado.sesiones.lock() {
        sesiones.remove(&sesion.id);
    }
    estado.anotar_error(&sesion.host_uuid, motivo);
    let _ = estado.escritor.con_almacen({
        let host_uuid = sesion.host_uuid.clone();
        move |almacen| {
            movil_bd::marcar_estado(
                almacen.conexion(),
                &host_uuid,
                Some(magi_nucleo::modelo::UltimoEstado::Error),
            )
        }
    });
    estado.emitir_sesiones();
}

async fn bucle_sesion(
    estado: Arc<EstadoNucleo>,
    sesion: Arc<SesionViva>,
    canal: &mut russh::Channel<russh::client::Msg>,
    mut comandos: mpsc::UnboundedReceiver<ComandoSesion>,
) {
    let mut generador = GeneradorTramos::nuevo(sesion.id);
    let mut intervalo = tokio::time::interval(Duration::from_millis(33));
    let mut sucio = true;
    let mut completa = true;
    let mut vio_eof = false;
    loop {
        tokio::select! {
            comando = comandos.recv() => match comando {
                Some(ComandoSesion::Datos(bytes)) => {
                    if canal.data_bytes(bytes).await.is_err() {
                        sesion.fijar_estado(EstadoSesion::Caida, Some("la conexión se ha perdido".to_string()));
                        estado.emitir_sesiones();
                        actualizar_automaticos(&estado, &sesion.host_uuid);
                        return;
                    }
                }
                Some(ComandoSesion::Redimensionar(columnas, filas)) => {
                    let columnas = columnas.max(2);
                    let filas = filas.max(1);
                    let _ = canal.window_change(u32::from(columnas), u32::from(filas), 0, 0).await;
                    if let Ok(mut pantalla) = sesion.pantalla.lock() {
                        pantalla.screen_mut().set_size(filas, columnas);
                    }
                    completa = true;
                    sucio = true;
                }
                Some(ComandoSesion::Desplazar(filas)) => {
                    if let Ok(mut pantalla) = sesion.pantalla.lock() {
                        let actual = pantalla.screen().scrollback() as i32;
                        let nuevo = (actual + filas).max(0) as usize;
                        pantalla.screen_mut().set_scrollback(nuevo);
                    }
                    completa = true;
                    sucio = true;
                }
                Some(ComandoSesion::Cerrar) | None => {
                    let _ = canal.eof().await;
                    let _ = canal.close().await;
                    cerrar_sesion_definitiva(&estado, &sesion).await;
                    return;
                }
            },
            mensaje = canal.wait() => match mensaje {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if let Ok(mut pantalla) = sesion.pantalla.lock() {
                        pantalla.process(&data);
                    }
                    sucio = true;
                }
                Some(ChannelMsg::Eof) => {
                    vio_eof = true;
                }
                Some(ChannelMsg::Close) | None => {
                    if vio_eof {
                        cerrar_sesion_definitiva(&estado, &sesion).await;
                    } else {
                        sesion.fijar_estado(
                            EstadoSesion::Caida,
                            Some("la conexión se ha perdido; pulsa Reconectar".to_string()),
                        );
                        estado.emitir_sesiones();
                        actualizar_automaticos(&estado, &sesion.host_uuid);
                    }
                    return;
                }
                _ => {}
            },
            _ = intervalo.tick() => {
                if sucio {
                    let pantalla = sesion.pantalla.lock().ok().and_then(|pantalla| {
                        generador.actualizar(&pantalla, completa)
                    });
                    if let Some(pantalla) = pantalla {
                        estado.retrollamadas.oyente.pantalla(mapear_pantalla(&pantalla));
                        sucio = false;
                        completa = false;
                    }
                }
            }
        }
    }
}

async fn cerrar_sesion_definitiva(estado: &Arc<EstadoNucleo>, sesion: &SesionViva) {
    if let Ok(mut sesiones) = estado.sesiones.lock() {
        sesiones.remove(&sesion.id);
    }
    let _ = estado.escritor.con_almacen({
        let host_uuid = sesion.host_uuid.clone();
        move |almacen| {
            let conexion = almacen.conexion();
            let host_id: Option<i64> = conexion
                .query_row(
                    "SELECT id FROM HOSTS WHERE uuid = ?1",
                    [&host_uuid],
                    |fila| fila.get(0),
                )
                .ok();
            if let Some(host_id) = host_id {
                let _ = registro::anotar(
                    conexion,
                    registro::SESION_CERRADA,
                    Some(host_id),
                    None,
                    "la sesión del móvil terminó",
                    ResultadoRegistro::Ok,
                );
            }
        }
    });
    estado.emitir_sesiones();
    actualizar_automaticos(estado, &sesion.host_uuid);
    cerrar_conexion_sin_trabajo(estado, &sesion.host_uuid).await;
}

fn mapear_pantalla(pantalla: &magi_nucleo::terminal::tramos::PantallaTramos) -> PantallaDto {
    let filas_cambiadas = pantalla
        .filas_cambiadas
        .iter()
        .map(|(fila, tramos)| FilaPantalla {
            fila: u32::from(*fila),
            tramos: tramos.iter().map(mapear_tramo).collect(),
        })
        .collect();
    PantallaDto {
        sesion: pantalla.sesion,
        columnas: u32::from(pantalla.columnas),
        filas: u32::from(pantalla.filas),
        filas_cambiadas,
        cursor: pantalla.cursor.map(|(columna, fila)| Posicion {
            columna: u32::from(columna),
            fila: u32::from(fila),
        }),
        desplazamiento: pantalla.desplazamiento.min(u32::MAX as usize) as u32,
        completa: pantalla.completa,
    }
}

fn mapear_tramo(tramo: &magi_nucleo::terminal::tramos::Tramo) -> Tramo {
    Tramo {
        texto: tramo.texto.clone(),
        primer_plano: mapear_color(tramo.primer_plano),
        fondo: mapear_color(tramo.fondo),
        negrita: tramo.negrita,
        cursiva: tramo.cursiva,
        subrayado: tramo.subrayado,
        inverso: tramo.inverso,
    }
}

fn mapear_color(color: magi_nucleo::terminal::tramos::Color) -> Color {
    match color {
        magi_nucleo::terminal::tramos::Color::Predeterminado => Color::Predeterminado,
        magi_nucleo::terminal::tramos::Color::Indice(indice) => Color::Indice(indice),
        magi_nucleo::terminal::tramos::Color::Rgb(rojo, verde, azul) => {
            Color::Rgb(rojo, verde, azul)
        }
    }
}

fn sesion_de(estado: &EstadoNucleo, id: u64) -> Result<Arc<SesionViva>, ErrorNucleo> {
    estado
        .sesiones
        .lock()
        .ok()
        .and_then(|sesiones| sesiones.get(&id).cloned())
        .ok_or(ErrorNucleo::NoEncontrado {
            detalle: format!("la sesión {id}"),
        })
}

/// Escribe bytes tal cual en el PTY.
pub fn escribir(estado: &EstadoNucleo, id: u64, bytes: Vec<u8>) -> Result<(), ErrorNucleo> {
    sesion_de(estado, id)?.ordenar(ComandoSesion::Datos(bytes));
    Ok(())
}

/// Codifica una tecla según el modo del terminal y la escribe.
pub fn tecla(
    estado: &EstadoNucleo,
    id: u64,
    tecla: Tecla,
    modificadores: Modificadores,
) -> Result<(), ErrorNucleo> {
    let sesion = sesion_de(estado, id)?;
    let modo = sesion
        .pantalla
        .lock()
        .map(|pantalla| ModoTerminal::desde_parser(&pantalla))
        .unwrap_or_default();
    let tecla = mapear_tecla(tecla)?;
    let modificadores = teclas::Modificadores {
        mayusculas: modificadores.mayus,
        alt: modificadores.alt,
        ctrl: modificadores.ctrl,
    };
    if let Some(bytes) = teclas::codificar(tecla, modificadores, modo) {
        sesion.ordenar(ComandoSesion::Datos(bytes));
    }
    Ok(())
}

fn mapear_tecla(tecla: Tecla) -> Result<teclas::Tecla, ErrorNucleo> {
    Ok(match tecla {
        Tecla::Escape => teclas::Tecla::Escape,
        Tecla::Tab => teclas::Tecla::Tabulador,
        Tecla::Enter => teclas::Tecla::Intro,
        Tecla::Retroceso => teclas::Tecla::Retroceso,
        Tecla::Arriba => teclas::Tecla::Arriba,
        Tecla::Abajo => teclas::Tecla::Abajo,
        Tecla::Izquierda => teclas::Tecla::Izquierda,
        Tecla::Derecha => teclas::Tecla::Derecha,
        Tecla::Inicio => teclas::Tecla::Inicio,
        Tecla::Fin => teclas::Tecla::Fin,
        Tecla::RePag => teclas::Tecla::RePagina,
        Tecla::AvPag => teclas::Tecla::AvPagina,
        Tecla::Supr => teclas::Tecla::Suprimir,
        Tecla::Insert => teclas::Tecla::Insertar,
        Tecla::F(numero) => teclas::Tecla::F(numero),
        Tecla::Caracter(texto) => {
            let mut caracteres = texto.chars();
            match (caracteres.next(), caracteres.next()) {
                (Some(caracter), None) => teclas::Tecla::Caracter(caracter),
                _ => {
                    return Err(ErrorNucleo::EstadoInvalido {
                        detalle: format!("«{texto}» no es un solo carácter"),
                    })
                }
            }
        }
    })
}

/// Pega texto (pegado entre corchetes si el remoto lo activó).
pub fn pegar(estado: &EstadoNucleo, id: u64, texto: String) -> Result<(), ErrorNucleo> {
    let sesion = sesion_de(estado, id)?;
    let modo = sesion
        .pantalla
        .lock()
        .map(|pantalla| ModoTerminal::desde_parser(&pantalla))
        .unwrap_or_default();
    let bytes = teclas::pegar(&texto, modo);
    sesion.ordenar(ComandoSesion::Datos(bytes));
    Ok(())
}

pub fn pegado_entre_corchetes(estado: &EstadoNucleo, id: u64) -> Result<bool, ErrorNucleo> {
    Ok(sesion_de(estado, id)?.pegado_entre_corchetes())
}

pub fn redimensionar(
    estado: &EstadoNucleo,
    id: u64,
    columnas: u32,
    filas: u32,
) -> Result<(), ErrorNucleo> {
    sesion_de(estado, id)?.ordenar(ComandoSesion::Redimensionar(
        columnas.min(u32::from(u16::MAX)) as u16,
        filas.min(u32::from(u16::MAX)) as u16,
    ));
    Ok(())
}

pub fn desplazar(estado: &EstadoNucleo, id: u64, filas: i32) -> Result<(), ErrorNucleo> {
    sesion_de(estado, id)?.ordenar(ComandoSesion::Desplazar(filas));
    Ok(())
}

pub fn texto_seleccion(
    estado: &EstadoNucleo,
    id: u64,
    desde: Posicion,
    hasta: Posicion,
) -> Result<String, ErrorNucleo> {
    let sesion = sesion_de(estado, id)?;
    sesion
        .texto_seleccion(
            (
                desde.columna.min(u32::from(u16::MAX)) as u16,
                desde.fila.min(u32::from(u16::MAX)) as u16,
            ),
            (
                hasta.columna.min(u32::from(u16::MAX)) as u16,
                hasta.fila.min(u32::from(u16::MAX)) as u16,
            ),
        )
        .ok_or_else(|| ErrorNucleo::General {
            detalle: "no se pudo leer la pantalla".to_string(),
        })
}

pub fn cerrar_sesion(estado: &EstadoNucleo, id: u64) -> Result<(), ErrorNucleo> {
    let sesion = sesion_de(estado, id)?;
    if sesion.estado() == EstadoSesion::Caida {
        // Una sesión caída ya no tiene tarea que atienda el cierre.
        if let Ok(mut sesiones) = estado.sesiones.lock() {
            sesiones.remove(&id);
        }
        estado.emitir_sesiones();
        return Ok(());
    }
    sesion.ordenar(ComandoSesion::Cerrar);
    Ok(())
}

/// Reconecta una sesión caída con el mismo nombre.
pub async fn reconectar(estado: &Arc<EstadoNucleo>, id: u64) -> Result<(), ErrorNucleo> {
    let sesion = sesion_de(estado, id)?;
    if sesion.estado() != EstadoSesion::Caida {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "solo se reconecta una sesión caída".to_string(),
        });
    }
    if let Ok(mut sesiones) = estado.sesiones.lock() {
        sesiones.remove(&id);
    }
    let pantalla = nueva_pantalla(24, 80);
    let nueva = nueva_sesion(
        id,
        &sesion.host_uuid,
        &sesion.host_nombre,
        pantalla,
        EstadoSesion::Abriendo,
        Some("resolviendo".to_string()),
    );
    if let Ok(mut sesiones) = estado.sesiones.lock() {
        sesiones.insert(id, nueva.clone());
    }
    estado.emitir_sesiones();
    let estado_tarea = estado.clone();
    estado.runtime.spawn(async move {
        abrir_y_servir(estado_tarea, nueva).await;
    });
    Ok(())
}

// ---------------------------------------------------------------- túneles

fn mapear_tipo_tunel(tipo: TipoTunel) -> crate::api::TipoTunel {
    match tipo {
        TipoTunel::Local => crate::api::TipoTunel::Local,
        TipoTunel::Remoto => crate::api::TipoTunel::Remoto,
        TipoTunel::Dinamico => crate::api::TipoTunel::Dinamico,
    }
}

/// Túneles del inventario con su estado vivo, de un host o todos.
pub fn tuneles(
    estado: &EstadoNucleo,
    host_uuid: Option<&str>,
) -> Result<Vec<TunelVivo>, ErrorNucleo> {
    let (tuneles, uuids_hosts) = con_almacen(estado, |almacen| {
        let conexion = almacen.conexion();
        let tuneles = almacen.listar_tuneles()?;
        let uuids = movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)?;
        Ok::<_, anyhow::Error>((tuneles, uuids))
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los túneles: {error:#}"),
    })?;
    let vivos = estado
        .tuneles
        .lock()
        .map(|tuneles| tuneles.clone())
        .unwrap_or_default();
    let mut lista = Vec::new();
    for tunel in tuneles {
        let uuid_host = uuids_hosts.get(&tunel.host_id).cloned().unwrap_or_default();
        if let Some(filtro) = host_uuid {
            if uuid_host != filtro {
                continue;
            }
        }
        let uuid = con_almacen(estado, {
            let id = tunel.id;
            move |almacen| {
                movil_bd::uuid_por_id(
                    almacen.conexion(),
                    magi_nucleo::almacen::versiones::Tabla::Tuneles,
                )
                .ok()
                .and_then(|mapa| mapa.get(&id).cloned())
            }
        })
        .ok()
        .flatten()
        .unwrap_or_default();
        let vivo = vivos.get(&uuid);
        let escucha = escucha_movil(tunel.tipo, &tunel.escucha);
        let (estado_tunel, error, conexiones, subidos, bajados, puerto_efectivo) = match vivo {
            Some(activo) => {
                let (conexiones, _aceptadas, subidos, bajados) = activo.contadores.instantanea();
                let estado_vivo = activo.estado();
                let estado_tunel = match &estado_vivo {
                    EstadoTunelVivo::Inactivo => EstadoTunel::Inactivo,
                    EstadoTunelVivo::Activando => EstadoTunel::Activando,
                    EstadoTunelVivo::Activo => EstadoTunel::Activo,
                    EstadoTunelVivo::Caido(_) => EstadoTunel::Caido,
                };
                let motivo = match estado_vivo {
                    EstadoTunelVivo::Caido(motivo) => Some(motivo),
                    _ => None,
                };
                let puerto = activo
                    .puerto_efectivo
                    .lock()
                    .map(|puerto| *puerto)
                    .unwrap_or(None);
                (
                    estado_tunel,
                    motivo,
                    conexiones,
                    subidos,
                    bajados,
                    puerto.map(u32::from),
                )
            }
            None => (EstadoTunel::Inactivo, None, 0, 0, 0, None),
        };
        let (escucha_texto, aviso) = match (&escucha, tunel.tipo) {
            (Ok(_escucha), TipoTunel::Remoto) => (tunel.escucha.clone(), false),
            (Ok(escucha), _) => (escucha.texto(), escucha.aviso),
            (Err(_), _) => (tunel.escucha.clone(), false),
        };
        lista.push(TunelVivo {
            uuid,
            host_uuid: uuid_host,
            host_nombre: tunel.host_nombre,
            nombre: tunel.nombre,
            tipo: mapear_tipo_tunel(tunel.tipo),
            escucha: escucha_texto,
            escucha_aviso: aviso,
            destino: tunel.destino.unwrap_or_default(),
            automatico: tunel.automatico,
            estado: estado_tunel,
            error,
            conexiones,
            bytes_subida: subidos,
            bytes_bajada: bajados,
            puerto_efectivo,
        });
    }
    Ok(lista)
}

pub fn emitir_tuneles(estado: &EstadoNucleo) {
    if let Ok(lista) = tuneles(estado, None) {
        estado.retrollamadas.oyente.tuneles(lista);
    }
}

/// Activa un túnel; con puerto_libre lo abre en un puerto que asigna el sistema.
pub async fn activar_tunel(
    estado: &Arc<EstadoNucleo>,
    uuid: &str,
    puerto_libre: bool,
) -> Result<(), ErrorNucleo> {
    let (tunel, host, uuid_host) = con_almacen(estado, {
        let uuid = uuid.to_string();
        move |almacen| {
            let conexion = almacen.conexion();
            let tunel = movil_bd::tunel_por_uuid(conexion, &uuid)?;
            let mut uuid_host = String::new();
            let host = match &tunel {
                Some(tunel) => {
                    let uuids = movil_bd::uuid_por_id(
                        conexion,
                        magi_nucleo::almacen::versiones::Tabla::Hosts,
                    )?;
                    uuid_host = uuids.get(&tunel.host_id).cloned().unwrap_or_default();
                    movil_bd::host_por_uuid(conexion, &uuid_host)?
                }
                None => None,
            };
            Ok::<_, anyhow::Error>((tunel, host, uuid_host))
        }
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el túnel: {error:#}"),
    })?;
    let Some(tunel) = tunel else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el túnel {uuid}"),
        });
    };
    let Some(host) = host else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: "el host del túnel".to_string(),
        });
    };
    {
        let activos = estado.tuneles.lock().map_err(|_| ErrorNucleo::General {
            detalle: "estado de túneles envenenado".to_string(),
        })?;
        if let Some(activo) = activos.get(uuid) {
            if matches!(
                activo.estado(),
                EstadoTunelVivo::Activo | EstadoTunelVivo::Activando
            ) {
                return Ok(());
            }
        }
    }
    let escucha =
        escucha_movil(tunel.tipo, &tunel.escucha).map_err(|detalle| ErrorNucleo::General {
            detalle: format!("la escucha del túnel no es válida: {detalle}"),
        })?;
    let puerto = if puerto_libre {
        0
    } else {
        if tunel.tipo != TipoTunel::Remoto {
            validar_puerto_movil(escucha.puerto)
                .map_err(|detalle| ErrorNucleo::General { detalle })?;
        }
        escucha.puerto
    };
    let (cancelacion, receptor_cancelacion) = watch::channel(false);
    let host_uuid = uuid_host;
    let activo = Arc::new(TunelActivo {
        uuid: uuid.to_string(),
        host_uuid,
        host_nombre: host.nombre.clone(),
        nombre: tunel.nombre.clone(),
        tipo: tunel.tipo,
        escucha: escucha.clone(),
        escucha_efectiva: std::sync::Mutex::new(escucha.texto()),
        destino: tunel.destino.clone(),
        automatico: tunel.automatico,
        estado: std::sync::Mutex::new(EstadoTunelVivo::Activando),
        contadores: Arc::new(Contadores::default()),
        parada: std::sync::Mutex::new(None),
        cancelacion,
        reenvio: std::sync::Mutex::new(None),
        puerto_efectivo: std::sync::Mutex::new(None),
        id_numerico: estado.siguiente_id() as i64,
    });
    if let Ok(mut activos) = estado.tuneles.lock() {
        activos.insert(uuid.to_string(), activo.clone());
    }
    emitir_tuneles(estado);
    let resultado =
        levantar_tunel(estado, &activo, &host, &tunel, puerto, receptor_cancelacion).await;
    match resultado {
        Ok(()) => {
            activo.fijar_estado(EstadoTunelVivo::Activo);
            let detalle = format!(
                "{} · {} · {} → {}",
                tunel.nombre,
                tunel.tipo.etiqueta(),
                activo
                    .escucha_efectiva
                    .lock()
                    .map(|escucha| escucha.clone())
                    .unwrap_or_default(),
                tunel.destino.as_deref().unwrap_or("(socks5)")
            );
            let host_id = host.id;
            let _ = estado.escritor.con_almacen(move |almacen| {
                registro::anotar(
                    almacen.conexion(),
                    registro::TUNEL_ABIERTO,
                    Some(host_id),
                    None,
                    &detalle,
                    ResultadoRegistro::Ok,
                )
            });
            emitir_tuneles(estado);
            Ok(())
        }
        Err(motivo) => {
            activo.fijar_estado(EstadoTunelVivo::Caido(motivo.clone()));
            let detalle = format!("{}: {motivo}", tunel.nombre);
            let host_id = host.id;
            let _ = estado.escritor.con_almacen(move |almacen| {
                registro::anotar(
                    almacen.conexion(),
                    registro::TUNEL_FALLIDO,
                    Some(host_id),
                    None,
                    &detalle,
                    ResultadoRegistro::Error,
                )
            });
            emitir_tuneles(estado);
            Err(ErrorNucleo::General { detalle: motivo })
        }
    }
}

async fn levantar_tunel(
    estado: &Arc<EstadoNucleo>,
    activo: &Arc<TunelActivo>,
    host: &Host,
    tunel: &magi_nucleo::modelo::Tunel,
    puerto: u16,
    cancelacion: watch::Receiver<bool>,
) -> Result<(), String> {
    let conexion = match conexion_para_host(estado, &activo.host_uuid).await {
        Ok(conexion) => conexion,
        Err(error) => return Err(error.to_string()),
    };
    match tunel.tipo {
        TipoTunel::Local | TipoTunel::Dinamico => {
            let (listener, efectiva) =
                abrir_escucha_movil(&activo.escucha.direccion, puerto).await?;
            if let Ok(mut escucha) = activo.escucha_efectiva.lock() {
                *escucha = efectiva.clone();
            }
            if let Ok((_, puerto)) = partir_escucha(&efectiva) {
                if let Ok(mut guardado) = activo.puerto_efectivo.lock() {
                    *guardado = Some(puerto);
                }
            }
            let parada = if tunel.tipo == TipoTunel::Dinamico {
                servir_socks(
                    listener,
                    conexion.handle.clone(),
                    activo.contadores.clone(),
                    cancelacion,
                )
            } else {
                let destino = tunel.destino.clone().unwrap_or_default();
                servir_local(
                    listener,
                    conexion.handle.clone(),
                    destino,
                    activo.contadores.clone(),
                    cancelacion,
                )
            };
            if let Ok(mut guardado) = activo.parada.lock() {
                *guardado = Some(parada);
            }
            Ok(())
        }
        TipoTunel::Remoto => {
            let (direccion, puerto_pedido) = partir_escucha(&tunel.escucha)?;
            let respuesta = tokio::time::timeout(
                PLAZO,
                conexion
                    .handle
                    .tcpip_forward(direccion.clone(), u32::from(puerto_pedido)),
            )
            .await
            .map_err(|_| "se agotó el plazo pidiendo el reenvío".to_string())?
            .map_err(|error| match error {
                russh::Error::RequestDenied => {
                    format!("el host rechazó el reenvío de {}", tunel.escucha)
                }
                otro => format!("no se pudo pedir el reenvío: {otro}"),
            })?;
            let efectivo = if puerto_pedido == 0 {
                respuesta
            } else {
                u32::from(puerto_pedido)
            };
            let efectiva = magi_nucleo::modelo::juntar_direccion_puerto(
                &direccion,
                efectivo.min(u32::from(u16::MAX)) as u16,
            );
            if let Ok(mut guardado) = activo.escucha_efectiva.lock() {
                *guardado = efectiva.clone();
            }
            if let Ok((_, puerto)) = partir_escucha(&efectiva) {
                if let Ok(mut guardado) = activo.puerto_efectivo.lock() {
                    *guardado = Some(puerto);
                }
            }
            let reenvio = Arc::new(Reenvio {
                tunel_id: activo.id_numerico,
                destino: tunel.destino.clone().unwrap_or_default(),
                contadores: activo.contadores.clone(),
                cancelacion,
            });
            conexion
                .reenvios
                .registrar(host.id, &direccion, efectivo, reenvio);
            if let Ok(mut guardado) = activo.reenvio.lock() {
                *guardado = Some((direccion, efectivo));
            }
            Ok(())
        }
    }
}

/// Para un túnel activo y libera sus recursos.
pub async fn parar_tunel(estado: &Arc<EstadoNucleo>, uuid: &str) -> Result<(), ErrorNucleo> {
    let activo = {
        let mut activos = estado.tuneles.lock().map_err(|_| ErrorNucleo::General {
            detalle: "estado de túneles envenenado".to_string(),
        })?;
        activos.remove(uuid)
    };
    let Some(activo) = activo else {
        return Ok(());
    };
    if let Ok(mut parada) = activo.parada.lock() {
        if let Some(parada) = parada.take() {
            let _ = parada.send(());
        }
    }
    let _ = activo.cancelacion.send(true);
    let reenvio = activo
        .reenvio
        .lock()
        .ok()
        .and_then(|reenvio| reenvio.clone());
    if let Some((direccion, puerto)) = reenvio {
        let host_id = con_almacen(estado, {
            let uuid = activo.host_uuid.clone();
            move |almacen| {
                movil_bd::host_por_uuid(almacen.conexion(), &uuid)
                    .ok()
                    .flatten()
                    .map(|host| host.id)
            }
        })
        .ok()
        .flatten();
        let conexiones = estado.conexiones.lock().await;
        if let Some(conexion) = conexiones.get(&activo.host_uuid) {
            let _ = conexion
                .handle
                .cancel_tcpip_forward(direccion.clone(), puerto)
                .await;
            if let Some(host_id) = host_id {
                conexion.reenvios.quitar(host_id, &direccion, puerto);
            }
        }
    }
    let (conexiones, _aceptadas, subidos, bajados) = activo.contadores.instantanea();
    let detalle = format!(
        "{} · {conexiones} conexiones · ↑{subidos} B ↓{bajados} B",
        activo.nombre
    );
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::TUNEL_CERRADO,
            None,
            None,
            &detalle,
            ResultadoRegistro::Ok,
        )
    });
    emitir_tuneles(estado);
    cerrar_conexion_sin_trabajo(estado, &activo.host_uuid).await;
    Ok(())
}

/// Relanza un túnel caído.
pub async fn relanzar_tunel(estado: &Arc<EstadoNucleo>, uuid: &str) -> Result<(), ErrorNucleo> {
    parar_tunel(estado, uuid).await?;
    activar_tunel(estado, uuid, false).await
}

/// Activa o para los túneles automáticos del host según sus sesiones.
pub fn actualizar_automaticos(estado: &Arc<EstadoNucleo>, host_uuid: &str) {
    let (abiertas, _, _) = estado.recuento_sesiones(host_uuid);
    let tuneles = match con_almacen(estado, {
        let host_uuid = host_uuid.to_string();
        move |almacen| {
            let conexion = almacen.conexion();
            let todos = almacen.listar_tuneles()?;
            let uuids =
                movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)?;
            Ok::<_, anyhow::Error>(
                todos
                    .into_iter()
                    .filter(|tunel| {
                        uuids.get(&tunel.host_id).map(String::as_str) == Some(host_uuid.as_str())
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }) {
        Ok(Ok(tuneles)) => tuneles,
        _ => return,
    };
    for tunel in tuneles {
        if !tunel.automatico {
            continue;
        }
        let uuid = match con_almacen(estado, {
            let id = tunel.id;
            move |almacen| {
                movil_bd::uuid_por_id(
                    almacen.conexion(),
                    magi_nucleo::almacen::versiones::Tabla::Tuneles,
                )
                .ok()
                .and_then(|mapa| mapa.get(&id).cloned())
            }
        }) {
            Ok(Some(uuid)) => uuid,
            _ => continue,
        };
        let vivo = estado
            .tuneles
            .lock()
            .ok()
            .and_then(|tuneles| tuneles.get(&uuid).cloned());
        if abiertas > 0 {
            let ya_activo = matches!(
                vivo.as_ref().map(|activo| activo.estado()),
                Some(EstadoTunelVivo::Activo) | Some(EstadoTunelVivo::Activando)
            );
            if !ya_activo {
                let estado_tarea = estado.clone();
                let manejador = estado.runtime.handle().clone();
                manejador.spawn(async move {
                    let _ = activar_tunel(&estado_tarea, &uuid, false).await;
                });
            }
        } else if vivo.is_some() {
            let estado_tarea = estado.clone();
            let manejador = estado.runtime.handle().clone();
            manejador.spawn(async move {
                let _ = parar_tunel(&estado_tarea, &uuid).await;
            });
        }
    }
}

async fn cerrar_conexion_sin_trabajo(estado: &Arc<EstadoNucleo>, host_uuid: &str) {
    let hay_sesiones = estado
        .sesiones
        .lock()
        .map(|sesiones| {
            sesiones
                .values()
                .any(|sesion| sesion.host_uuid == host_uuid)
        })
        .unwrap_or(false);
    let hay_tuneles = estado
        .tuneles
        .lock()
        .map(|tuneles| tuneles.values().any(|tunel| tunel.host_uuid == host_uuid))
        .unwrap_or(false);
    if hay_sesiones || hay_tuneles {
        return;
    }
    let conexion = estado.conexiones.lock().await.remove(host_uuid);
    if let Some(conexion) = conexion {
        let handle: Arc<Handle<magi_nucleo::conexion::cliente::Cliente>> = conexion.handle.clone();
        tokio::spawn(async move {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "")
                .await;
        });
    }
}

// ------------------------------------------------------- instalar mi clave

/// Instala la clave del móvil en el host (flujo §4.4): entra con contraseña,
/// ejecuta el script embebido y comprueba que la clave entra.
pub async fn instalar_clave_en_host(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<ResultadoInstalacionClave, ErrorNucleo> {
    let (host, otros) = con_almacen(estado, {
        let host_uuid = host_uuid.to_string();
        move |almacen| {
            let conexion = almacen.conexion();
            Ok::<_, anyhow::Error>((
                movil_bd::host_por_uuid(conexion, &host_uuid)?,
                hosts_bd::listar(conexion)?,
            ))
        }
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el host: {error:#}"),
    })?;
    let Some(host) = host else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el host {host_uuid}"),
        });
    };
    let linea = estado.clave_publica()?.linea_openssh;
    magi_nucleo::instalar_clave::validar_linea_publica(&linea)
        .map_err(|detalle| ErrorNucleo::Firmante { detalle })?;
    let comando = magi_nucleo::instalar_clave::comando_instalar_clave(&linea)
        .map_err(|detalle| ErrorNucleo::General { detalle })?;
    let mapa: HashMap<i64, Host> = otros.into_iter().map(|host| (host.id, host)).collect();
    let cadena: Vec<Host> = construir_cadena(&host, &mapa)
        .map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?
        .into_iter()
        .map(|mut salto| {
            salto.identidad_ref = magi_nucleo::modelo::IdentidadRef::ContrasenaLlavero;
            salto
        })
        .collect();
    let (tx, rx) = mpsc::unbounded_channel::<EventoConexion>();
    lanzar_bucle_eventos(estado.clone(), rx);
    let contexto = Contexto {
        known_hosts: estado.rutas.fichero_known_hosts(),
        dir_ssh: estado.rutas.dir_ssh(),
        hogar: estado.rutas.hogar.clone(),
        usuario_local: host
            .usuario
            .clone()
            .unwrap_or_else(|| "usuario".to_string()),
        tx,
        interactivo: true,
        fuente_contrasena: FuenteContrasena::Llavero,
        reenvios: None,
        llavero: estado.llavero.clone(),
    };
    let transporte = magi_nucleo::conexion::salto::conectar_cadena(&cadena, &contexto)
        .await
        .map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?;
    let mut canal = transporte
        .handle
        .channel_open_session()
        .await
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("no se pudo abrir el canal: {error}"),
        })?;
    canal
        .exec(true, comando.into_bytes())
        .await
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("no se pudo ejecutar el script: {error}"),
        })?;
    let mut salida = Vec::new();
    while let Some(mensaje) = canal.wait().await {
        match mensaje {
            ChannelMsg::Data { data } => salida.extend_from_slice(&data),
            ChannelMsg::Close | ChannelMsg::Eof => break,
            _ => {}
        }
    }
    let _ = canal.close().await;
    let texto = String::from_utf8_lossy(&salida);
    let ya_estaba = texto.contains("YA_ESTABA");
    let instalada = texto.contains("INSTALADA");
    if !ya_estaba && !instalada {
        let motivo = texto.trim();
        let motivo = if motivo.is_empty() {
            "el script no pudo instalar la clave".to_string()
        } else {
            motivo.to_string()
        };
        return Err(ErrorNucleo::General { detalle: motivo });
    }
    if instalada {
        let nombre = host.nombre.clone();
        let host_id = host.id;
        let _ = estado.escritor.con_almacen(move |almacen| {
            registro::anotar(
                almacen.conexion(),
                registro::CLAVE_INSTALADA,
                Some(host_id),
                None,
                &format!("clave del móvil instalada en «{nombre}»"),
                ResultadoRegistro::Ok,
            )
        });
    }
    let aceptada = comprobar_clave_movil(estado, &host, &mapa).await;
    Ok(match (ya_estaba, aceptada) {
        (true, true) => ResultadoInstalacionClave::YaEstaba,
        (false, true) => ResultadoInstalacionClave::InstaladaYAceptada,
        _ => ResultadoInstalacionClave::InstaladaPeroNoAceptada,
    })
}

async fn comprobar_clave_movil(
    estado: &Arc<EstadoNucleo>,
    host: &Host,
    mapa: &HashMap<i64, Host>,
) -> bool {
    let Ok(cadena) = construir_cadena(host, mapa) else {
        return false;
    };
    let (tx, rx) = mpsc::unbounded_channel::<EventoConexion>();
    lanzar_bucle_eventos(estado.clone(), rx);
    let contexto = Contexto {
        known_hosts: estado.rutas.fichero_known_hosts(),
        dir_ssh: estado.rutas.dir_ssh(),
        hogar: estado.rutas.hogar.clone(),
        usuario_local: host
            .usuario
            .clone()
            .unwrap_or_else(|| "usuario".to_string()),
        tx,
        interactivo: true,
        fuente_contrasena: FuenteContrasena::Llavero,
        reenvios: None,
        llavero: estado.llavero.clone(),
    };
    let Ok(publica) = estado.clave_publica() else {
        return false;
    };
    let clave = ClaveMovil {
        linea_publica: publica.linea_openssh,
        firmante: estado.firmante.clone(),
    };
    match conectar_cadena_movil(&cadena, &contexto, &clave).await {
        Ok(transporte) => {
            let con_clave = transporte.identidad.descripcion == "clave del móvil";
            let handle = transporte.handle.clone();
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "")
                .await;
            con_clave
        }
        Err(_) => false,
    }
}
