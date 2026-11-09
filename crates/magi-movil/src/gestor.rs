//! Gestor del núcleo móvil: una conexión viva por host (compartida por
//! sesiones y túneles), sesiones con su terminal por tramos, túneles local,
//! remoto y dinámico, automáticos, trabajo vivo e «instalar mi clave».

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use magi_nucleo::almacen::{hosts as hosts_bd, movil as movil_bd};
use magi_nucleo::conexion::cliente::abrir_canal;
use magi_nucleo::conexion::reenvios::{Contadores, Reenvio};
use magi_nucleo::conexion::salto::construir_cadena;
use magi_nucleo::conexion::terminal::Pantalla;
use magi_nucleo::modelo::{Host, TipoTunel};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::terminal::teclas::{self, ModoTerminal};
use magi_nucleo::terminal::tramos::GeneradorTramos;
use magi_nucleo::tuneles::escucha::{
    abrir_escucha_movil, escucha_movil, partir_escucha, validar_puerto_movil,
};
use magi_nucleo::tuneles::local::{servir_local, servir_socks};
use russh::ChannelMsg;
use tokio::sync::{mpsc, watch};

use crate::api::{
    Color, ErrorNucleo, EstadoSesion, EstadoTunel, FilaPantalla, Modificadores,
    Pantalla as PantallaDto, Posicion, ResultadoInstalacionClave, Tecla, Tramo, TunelVivo,
};
use crate::conexiones::{self, Politica, UsoConexion};
use crate::estado::{
    con_almacen, ComandoSesion, EstadoNucleo, EstadoTunelVivo, SesionViva, TunelActivo,
};

/// Plazo de las operaciones con el host (pedir reenvíos, abrir canales).
const PLAZO: Duration = Duration::from_secs(10);

/// Qué hay que escribir en una sesión normal al conectar por el snippet del
/// host (F14, §7.9).
#[derive(Debug, Clone, PartialEq, Eq)]
enum AlConectar {
    /// El host no tiene snippet al conectar (o ya no existe).
    Nada,
    /// El snippet se puede escribir: su comando, con el salto de línea final.
    Comando(String),
    /// El snippet existe pero no se puede escribir (crítico o con variables):
    /// se ignora con un aviso en la sesión.
    Invalido(String),
}

/// Decide qué hacer con el snippet al conectar a partir del snippet del host.
/// El escritorio solo deja elegir snippets no críticos y sin variables, pero lo
/// que llega por la sincronización es entrada no fiable: el móvil lo vuelve a
/// comprobar.
fn decidir_al_conectar(snippet: Option<&magi_nucleo::snippets::Snippet>) -> AlConectar {
    match snippet {
        None => AlConectar::Nada,
        Some(snippet) if snippet.apto_al_conectar() => AlConectar::Comando(
            magi_nucleo::snippets::texto_comando_inicial(&snippet.comando),
        ),
        Some(snippet) => AlConectar::Invalido(snippet.nombre.clone()),
    }
}

/// Snippet al conectar del host, leído de la base de datos del móvil (solo
/// lectura: lo edita el escritorio y llega por la sincronización).
fn resolver_snippet_al_conectar(estado: &EstadoNucleo, host_uuid: &str) -> AlConectar {
    let uuid = host_uuid.to_string();
    let snippet = con_almacen(
        estado,
        move |almacen| -> anyhow::Result<Option<magi_nucleo::snippets::Snippet>> {
            let Some(host) = movil_bd::host_por_uuid(almacen.conexion(), &uuid)? else {
                return Ok(None);
            };
            let Some(snippet_id) = host.snippet_al_conectar_id else {
                return Ok(None);
            };
            Ok(almacen.obtener_snippet(snippet_id).ok())
        },
    );
    match snippet {
        Ok(Ok(snippet)) => decidir_al_conectar(snippet.as_ref()),
        _ => AlConectar::Nada,
    }
}

/// Texto del aviso que se escribe en la pantalla de la sesión cuando el snippet
/// al conectar no vale.
fn aviso_al_conectar(nombre_snippet: &str) -> String {
    format!(
        "snippet al conectar «{nombre_snippet}» no válido: es crítico o tiene variables; edítalo en el escritorio"
    )
}

/// Toma un uso interactivo de la conexión del host. La Fase 11 no tenía
/// políticas: todo lo que pedía el usuario podía dialogar. El código nuevo usa
/// conexiones::conexion_para_host con su política; esta función se conserva
/// porque la prueba de extremo a extremo de los túneles de la Fase 11 la llama
/// por su nombre.
pub async fn conexion_para_host(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<UsoConexion, ErrorNucleo> {
    conexiones::conexion_para_host(estado, host_uuid, Politica::Interactiva).await
}

// ----------------------------------------------------------------- sesiones

/// Abre una sesión nueva contra el host y devuelve su id.
pub async fn abrir_sesion(estado: &Arc<EstadoNucleo>, host_uuid: &str) -> Result<u64, ErrorNucleo> {
    abrir_sesion_interna(estado, host_uuid, None, None).await
}

/// Abre una sesión con nombre propio y comando inicial (D286): la usan las
/// pestañas ordenadas de Servicios (shell de un contenedor o seguimiento de
/// logs). No escribe snippet al conectar.
pub async fn abrir_sesion_con_orden(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    nombre_sesion: String,
    comando_inicial: String,
) -> Result<u64, ErrorNucleo> {
    abrir_sesion_interna(
        estado,
        host_uuid,
        Some(nombre_sesion),
        Some(comando_inicial),
    )
    .await
}

async fn abrir_sesion_interna(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    nombre: Option<String>,
    comando_inicial: Option<String>,
) -> Result<u64, ErrorNucleo> {
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
    // Un escritorio no se abre desde el móvil (F12): se ve en el inventario y se
    // abre en el ordenador con MAGI de escritorio.
    if !host.tipo.es_ssh() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "«{}» es un escritorio {} y se abre en el ordenador, no en el móvil",
                host.nombre,
                host.tipo.etiqueta()
            ),
        });
    }
    // Snippet al conectar (F14, §7.9): solo en las sesiones normales; las
    // ordenadas de Servicios llevan su propio comando (D286).
    let (comando_inicial, aviso_al_conectar) = if nombre.is_none() && comando_inicial.is_none() {
        match resolver_snippet_al_conectar(estado, host_uuid) {
            AlConectar::Comando(comando) => (Some(comando), None),
            AlConectar::Invalido(nombre_snippet) => {
                (None, Some(aviso_al_conectar(&nombre_snippet)))
            }
            AlConectar::Nada => (None, None),
        }
    } else {
        (comando_inicial, None)
    };
    let id = estado.siguiente_id();
    let pantalla = nueva_pantalla(24, 80);
    let sesion = nueva_sesion(
        id,
        host_uuid,
        nombre.as_deref().unwrap_or(&host.nombre),
        pantalla,
        EstadoSesion::Abriendo,
        Some("resolviendo".to_string()),
        comando_inicial,
        aviso_al_conectar,
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

#[allow(clippy::too_many_arguments)]
fn nueva_sesion(
    id: u64,
    host_uuid: &str,
    host_nombre: &str,
    pantalla: Pantalla,
    estado_inicial: EstadoSesion,
    detalle: Option<String>,
    comando_inicial: Option<String>,
    aviso_al_conectar: Option<String>,
) -> Arc<SesionViva> {
    let (comandos, receptor) = mpsc::unbounded_channel();
    Arc::new(SesionViva {
        id,
        host_uuid: host_uuid.to_string(),
        host_nombre: host_nombre.to_string(),
        estado: std::sync::Mutex::new(estado_inicial),
        detalle: std::sync::Mutex::new(detalle),
        comandos: std::sync::Mutex::new(comandos),
        receptor: std::sync::Mutex::new(Some(receptor)),
        pantalla,
        abierta_en: Some(chrono::Utc::now().timestamp()),
        comando_inicial,
        aviso_al_conectar,
    })
}

async fn abrir_y_servir(estado: Arc<EstadoNucleo>, sesion: Arc<SesionViva>) {
    let host_uuid = sesion.host_uuid.clone();
    // Los comandos que llegan mientras la sesión abre no se pierden: el tamaño
    // pedido se aplica al PTY y un cierre se atiende antes de abrir el canal.
    let mut receptor = sesion
        .receptor
        .lock()
        .ok()
        .and_then(|mut receptor| receptor.take());
    let mut tamano_pedido: Option<(u16, u16)> = None;
    let mut bytes_tempranos: Vec<u8> = Vec::new();
    if let Some(receptor) = receptor.as_mut() {
        while let Ok(comando) = receptor.try_recv() {
            match comando {
                ComandoSesion::Datos(bytes) => bytes_tempranos.extend_from_slice(&bytes),
                ComandoSesion::Redimensionar(columnas, filas) => {
                    tamano_pedido = Some((columnas.max(2), filas.max(1)));
                }
                ComandoSesion::Cerrar => {
                    fallar_sesion(&estado, &sesion, "cerrada antes de abrir").await;
                    return;
                }
                ComandoSesion::Desplazar(_) => {}
            }
        }
    }
    let uso = match conexion_para_host(&estado, &host_uuid).await {
        Ok(uso) => uso,
        Err(error) => {
            fallar_sesion(&estado, &sesion, &error.to_string()).await;
            return;
        }
    };
    sesion.fijar_estado(EstadoSesion::Abriendo, Some("conectando".to_string()));
    estado.emitir_sesiones();
    let (columnas, filas) = match tamano_pedido {
        Some(tamano) => tamano,
        None => match sesion.pantalla.lock() {
            Ok(pantalla) => pantalla.screen().size(),
            Err(_) => (80, 24),
        },
    };
    if let Ok(mut pantalla) = sesion.pantalla.lock() {
        pantalla.screen_mut().set_size(filas, columnas);
    }
    // Aviso del snippet al conectar no válido: se escribe solo en la pantalla
    // local de la sesión (nunca se manda al host) antes de que llegue la salida
    // del shell.
    if let Some(aviso) = sesion.aviso_al_conectar.clone() {
        if let Ok(mut pantalla) = sesion.pantalla.lock() {
            pantalla.process(format!("\r\n[aviso] {aviso}\r\n").as_bytes());
        }
    }
    let mut canal = match tokio::time::timeout(
        Duration::from_secs(10),
        abrir_canal(&uso.handle, columnas, filas),
    )
    .await
    {
        Err(_) => {
            fallar_sesion(&estado, &sesion, "se agotó el plazo abriendo el PTY (10 s)").await;
            return;
        }
        Ok(Err(error)) => {
            fallar_sesion(&estado, &sesion, &error.to_string()).await;
            return;
        }
        Ok(Ok(canal)) => canal,
    };
    if !bytes_tempranos.is_empty() {
        let _ = canal.data_bytes(bytes_tempranos).await;
    }
    // Comando inicial de las sesiones ordenadas (D286): el shell del
    // contenedor o el seguimiento de logs. Se escribe como lo haría el
    // usuario con un intro; no hay snippet al conectar en esta variante.
    if let Some(comando) = sesion.comando_inicial.clone() {
        let mut bytes = comando.into_bytes();
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        if let Err(error) = canal.data_bytes(bytes).await {
            fallar_sesion(&estado, &sesion, &error.to_string()).await;
            return;
        }
    }
    let rx_comandos = match receptor {
        Some(receptor) => receptor,
        None => mpsc::unbounded_channel().1,
    };
    sesion.fijar_estado(EstadoSesion::Abierta, None);
    let _ = estado.escritor.con_almacen({
        let host_uuid = host_uuid.clone();
        move |almacen| movil_bd::marcar_conexion(almacen.conexion(), &host_uuid)
    });
    estado.limpiar_error(&host_uuid);
    estado.emitir_sesiones();
    actualizar_automaticos(&estado, &host_uuid);
    bucle_sesion(estado, sesion, &mut canal, rx_comandos, uso).await;
}

async fn fallar_sesion(estado: &Arc<EstadoNucleo>, sesion: &SesionViva, motivo: &str) {
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
    _uso: UsoConexion,
) {
    let mut generador = GeneradorTramos::nuevo(sesion.id);
    let mut intervalo = tokio::time::interval(Duration::from_millis(33));
    let mut sucio = true;
    let mut completa = true;
    let mut vio_eof = false;
    // El redimensionado se agrupa 50 ms (F7): la UI puede pedir varios al
    // abrirse o cerrarse el teclado y el remoto recibe el último.
    let mut redimension_pendiente: Option<(u16, u16)> = None;
    let mut ultimo_redimension = std::time::Instant::now();
    loop {
        tokio::select! {
            comando = comandos.recv() => match comando {
                Some(ComandoSesion::Datos(bytes)) => {
                    if canal.data_bytes(bytes).await.is_err() {
                        sesion.fijar_estado(EstadoSesion::Caida, Some("la conexión se ha perdido".to_string()));
                        estado.emitir_sesiones();
                        marcar_tuneles_caidos(
                            &estado,
                            &sesion.host_uuid,
                            "la conexión se ha perdido",
                        );
                        actualizar_automaticos(&estado, &sesion.host_uuid);
                        return;
                    }
                }
                Some(ComandoSesion::Redimensionar(columnas, filas)) => {
                    redimension_pendiente = Some((columnas.max(2), filas.max(1)));
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
                if let Some((columnas, filas)) = redimension_pendiente {
                    if ultimo_redimension.elapsed() >= Duration::from_millis(50) {
                        let _ = canal
                            .window_change(u32::from(columnas), u32::from(filas), 0, 0)
                            .await;
                        if let Ok(mut pantalla) = sesion.pantalla.lock() {
                            pantalla.screen_mut().set_size(filas, columnas);
                        }
                        redimension_pendiente = None;
                        ultimo_redimension = std::time::Instant::now();
                        completa = true;
                        sucio = true;
                    }
                }
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
    // D277: hay una sola conexión por host para todo. No se fuerza su cierre al
    // reconectar: si sigue viva (o en gracia) se reutiliza, y si el host la
    // cerró, el vigilante ya la retiró. Cerrarla aquí tiraría las sesiones y
    // los túneles de otros que comparten la conexión (aviso de revisión
    // adversarial de la F14).
    if let Ok(mut sesiones) = estado.sesiones.lock() {
        sesiones.remove(&id);
    }
    // Al reconectar se vuelve a resolver el snippet al conectar del host (una
    // sesión normal no guarda comando); las ordenadas reescriben el suyo.
    let (comando_inicial, aviso_al_conectar) = match sesion.comando_inicial.clone() {
        Some(comando) => (Some(comando), None),
        None => match resolver_snippet_al_conectar(estado, &sesion.host_uuid) {
            AlConectar::Comando(comando) => (Some(comando), None),
            AlConectar::Invalido(nombre_snippet) => {
                (None, Some(aviso_al_conectar(&nombre_snippet)))
            }
            AlConectar::Nada => (None, None),
        },
    };
    let pantalla = nueva_pantalla(24, 80);
    let nueva = nueva_sesion(
        id,
        &sesion.host_uuid,
        &sesion.host_nombre,
        pantalla,
        EstadoSesion::Abriendo,
        Some("resolviendo".to_string()),
        comando_inicial,
        aviso_al_conectar,
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
    let (tuneles, uuids_hosts, tipos_hosts) = con_almacen(estado, |almacen| {
        let conexion = almacen.conexion();
        let tuneles = almacen.listar_tuneles()?;
        let uuids = movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)?;
        // Un túnel solo cuelga de un host SSH; el resto (un escritorio) no
        // debe salir en la vista ni poder activarse desde el móvil.
        let tipos = hosts_bd::listar(conexion)?
            .into_iter()
            .map(|host| (host.id, host.tipo))
            .collect::<HashMap<_, _>>();
        Ok::<_, anyhow::Error>((tuneles, uuids, tipos))
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
        if !tipos_hosts
            .get(&tunel.host_id)
            .is_some_and(|tipo| tipo.es_ssh())
        {
            continue;
        }
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
        let vivo = vivos.get(&uuid).cloned();
        let escucha = escucha_movil(tunel.tipo, &tunel.escucha);
        let (
            estado_tunel,
            error,
            conexiones,
            aceptadas,
            ultimo_error,
            ultimo_error_en,
            subidos,
            bajados,
            puerto_efectivo,
        ) = match vivo.as_ref() {
            Some(activo) => {
                let (conexiones, aceptadas, subidos, bajados) = activo.contadores.instantanea();
                let (ultimo_error, ultimo_error_en) = activo.contadores.error_con_fecha();
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
                    aceptadas.min(u64::from(u32::MAX)) as u32,
                    ultimo_error,
                    ultimo_error_en,
                    subidos,
                    bajados,
                    puerto.map(u32::from),
                )
            }
            None => (EstadoTunel::Inactivo, None, 0, 0, None, None, 0, 0, None),
        };
        // Con el túnel vivo se enseña la escucha de verdad (puede ser un
        // puerto libre); si no, la del inventario normalizada para el móvil.
        let (escucha_texto, aviso) = match vivo.as_ref() {
            Some(activo) => {
                let texto = activo
                    .escucha_efectiva
                    .lock()
                    .map(|texto| texto.clone())
                    .unwrap_or_else(|_| tunel.escucha.clone());
                (texto, escucha.as_ref().map(|e| e.aviso).unwrap_or(false))
            }
            None => match (&escucha, tunel.tipo) {
                (Ok(_escucha), TipoTunel::Remoto) => (tunel.escucha.clone(), false),
                (Ok(escucha), _) => (escucha.texto(), escucha.aviso),
                (Err(_), _) => (tunel.escucha.clone(), false),
            },
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
            aceptadas,
            ultimo_error,
            ultimo_error_en,
            bytes_subida: subidos,
            bytes_bajada: bajados,
            puerto_efectivo,
        });
    }
    Ok(lista)
}

/// Marca caídos los túneles activos de un host cuya conexión se ha perdido y
/// suelta el uso que cada uno tenía tomado (F14, D277).
pub(crate) fn marcar_tuneles_caidos(estado: &EstadoNucleo, host_uuid: &str, motivo: &str) {
    if let Ok(activos) = estado.tuneles.lock() {
        for tunel in activos.values() {
            if tunel.host_uuid != host_uuid {
                continue;
            }
            if !matches!(
                tunel.estado(),
                EstadoTunelVivo::Activo | EstadoTunelVivo::Activando
            ) {
                continue;
            }
            tunel.fijar_estado(EstadoTunelVivo::Caido(motivo.to_string()));
            if let Ok(mut parada) = tunel.parada.lock() {
                if let Some(parada) = parada.take() {
                    let _ = parada.send(());
                }
            }
            if let Ok(mut uso) = tunel.uso.lock() {
                uso.take();
            }
        }
    }
    emitir_tuneles(estado);
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
        uso: std::sync::Mutex::new(None),
    });
    {
        let mut activos = estado.tuneles.lock().map_err(|_| ErrorNucleo::General {
            detalle: "estado de túneles envenenado".to_string(),
        })?;
        if let Some(existente) = activos.get(uuid) {
            if matches!(
                existente.estado(),
                EstadoTunelVivo::Activo | EstadoTunelVivo::Activando
            ) {
                return Ok(());
            }
        }
        activos.insert(uuid.to_string(), activo.clone());
    }
    emitir_tuneles(estado);
    let resultado = levantar_tunel(
        estado,
        &activo,
        &host,
        &tunel,
        puerto,
        puerto_libre,
        receptor_cancelacion,
    )
    .await;
    match resultado {
        Ok(uso) => {
            // M3: si se pidió parar mientras el túnel se levantaba, se deshace
            // lo levantado (listener y reenvío) en vez de dejarlo huérfano.
            if *activo.cancelacion.borrow() {
                if let Ok(mut parada) = activo.parada.lock() {
                    if let Some(parada) = parada.take() {
                        let _ = parada.send(());
                    }
                }
                if let Some((direccion, puerto_reenvio)) = activo
                    .reenvio
                    .lock()
                    .ok()
                    .and_then(|reenvio| reenvio.clone())
                {
                    let _ = uso
                        .handle
                        .cancel_tcpip_forward(direccion, puerto_reenvio)
                        .await;
                }
                if let Ok(mut activos) = estado.tuneles.lock() {
                    if activos
                        .get(uuid)
                        .map(|existente| Arc::ptr_eq(existente, &activo))
                        .unwrap_or(false)
                    {
                        activos.remove(uuid);
                    }
                }
                drop(uso);
                emitir_tuneles(estado);
                return Ok(());
            }
            activo.fijar_estado(EstadoTunelVivo::Activo);
            if let Ok(mut guardado) = activo.uso.lock() {
                *guardado = Some(uso);
            }
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
            // Si el usuario paró mientras se levantaba, no es un fallo: la
            // parada ya quitó el túnel del mapa y no hay nada que marcar.
            if *activo.cancelacion.borrow() {
                if let Ok(mut activos) = estado.tuneles.lock() {
                    if activos
                        .get(uuid)
                        .map(|existente| Arc::ptr_eq(existente, &activo))
                        .unwrap_or(false)
                    {
                        activos.remove(uuid);
                    }
                }
                emitir_tuneles(estado);
                return Ok(());
            }
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
    puerto_libre: bool,
    cancelacion: watch::Receiver<bool>,
) -> Result<UsoConexion, String> {
    let uso = match conexiones::conexion_para_host(estado, &activo.host_uuid, Politica::Interactiva)
        .await
    {
        Ok(uso) => uso,
        Err(error) => return Err(error.to_string()),
    };
    match tunel.tipo {
        TipoTunel::Local | TipoTunel::Dinamico => {
            let (listener, efectiva) =
                abrir_escucha_movil(&activo.escucha.direccion, puerto).await?;
            if let Ok(mut escucha) = activo.escucha_efectiva.lock() {
                *escucha = efectiva.clone();
            }
            if puerto_libre {
                if let Ok((_, puerto)) = partir_escucha(&efectiva) {
                    if let Ok(mut guardado) = activo.puerto_efectivo.lock() {
                        *guardado = Some(puerto);
                    }
                }
            }
            let parada = if tunel.tipo == TipoTunel::Dinamico {
                servir_socks(
                    listener,
                    uso.handle.clone(),
                    activo.contadores.clone(),
                    cancelacion,
                )
            } else {
                let destino = tunel.destino.clone().unwrap_or_default();
                servir_local(
                    listener,
                    uso.handle.clone(),
                    destino,
                    activo.contadores.clone(),
                    cancelacion,
                )
            };
            if let Ok(mut guardado) = activo.parada.lock() {
                *guardado = Some(parada);
            }
            Ok(uso)
        }
        TipoTunel::Remoto => {
            let (direccion, puerto_pedido) = partir_escucha(&tunel.escucha)?;
            let respuesta = tokio::time::timeout(
                PLAZO,
                uso.handle
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
            if puerto_libre {
                if let Ok((_, puerto)) = partir_escucha(&efectiva) {
                    if let Ok(mut guardado) = activo.puerto_efectivo.lock() {
                        *guardado = Some(puerto);
                    }
                }
            }
            let reenvio = Arc::new(Reenvio {
                tunel_id: activo.id_numerico,
                destino: tunel.destino.clone().unwrap_or_default(),
                contadores: activo.contadores.clone(),
                cancelacion,
            });
            uso.reenvios
                .registrar(host.id, &direccion, efectivo, reenvio);
            if let Ok(mut guardado) = activo.reenvio.lock() {
                *guardado = Some((direccion, efectivo));
            }
            Ok(uso)
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
    // El uso del túnel se suelta al terminar (RAII), después de deshacer el
    // reenvío: así el cierre por gracia no se lleva la conexión por delante.
    let uso = activo.uso.lock().ok().and_then(|mut uso| uso.take());
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
        let conexion = uso
            .as_ref()
            .map(|uso| uso.conexion().clone())
            .or_else(|| estado.conexiones.viva(&activo.host_uuid));
        if let Some(conexion) = conexion {
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
    drop(uso);
    emitir_tuneles(estado);
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
    // Instalar una clave SSH no tiene sentido en un escritorio.
    if !host.tipo.es_ssh() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "«{}» es un escritorio {}: la clave del móvil solo se instala en hosts SSH",
                host.nombre,
                host.tipo.etiqueta()
            ),
        });
    }
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
    let transporte =
        conexiones::conectar_efimera(estado, &cadena, &host, Politica::Interactiva, false).await?;
    let mut canal = tokio::time::timeout(
        Duration::from_secs(10),
        transporte.handle.channel_open_session(),
    )
    .await
    .map_err(|_| ErrorNucleo::General {
        detalle: "se agotó el plazo abriendo el canal (10 s)".to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo abrir el canal: {error}"),
    })?;
    tokio::time::timeout(
        Duration::from_secs(10),
        canal.exec(true, comando.into_bytes()),
    )
    .await
    .map_err(|_| ErrorNucleo::General {
        detalle: "se agotó el plazo lanzando el script (10 s)".to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo ejecutar el script: {error}"),
    })?;
    let mut salida = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(mensaje) = canal.wait().await {
            match mensaje {
                ChannelMsg::Data { data } => salida.extend_from_slice(&data),
                ChannelMsg::Close | ChannelMsg::Eof => break,
                _ => {}
            }
        }
    })
    .await;
    let _ = tokio::time::timeout(Duration::from_secs(5), canal.close()).await;
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
    match conexiones::conectar_efimera(estado, &cadena, host, Politica::Interactiva, true).await {
        Ok(transporte) => {
            let con_clave =
                transporte.identidad.descripcion == magi_nucleo::conexion::DESCRIPCION_CLAVE_MOVIL;
            let handle = transporte.handle.clone();
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                handle.disconnect(russh::Disconnect::ByApplication, "", ""),
            )
            .await;
            con_clave
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use magi_nucleo::snippets::Snippet;

    fn snippet(comando: &str, critico: bool) -> Snippet {
        Snippet {
            id: 1,
            nombre: "al conectar".to_string(),
            comando: comando.to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: Vec::new(),
        }
    }

    #[test]
    fn sin_snippet_no_se_escribe_nada() {
        assert_eq!(decidir_al_conectar(None), AlConectar::Nada);
    }

    #[test]
    fn un_snippet_valido_se_escribe_con_su_salto() {
        assert_eq!(
            decidir_al_conectar(Some(&snippet("uptime", false))),
            AlConectar::Comando("uptime\n".to_string())
        );
    }

    #[test]
    fn un_snippet_critico_o_con_variables_se_ignora_con_aviso() {
        assert_eq!(
            decidir_al_conectar(Some(&snippet("uptime", true))),
            AlConectar::Invalido("al conectar".to_string())
        );
        assert_eq!(
            decidir_al_conectar(Some(&snippet("cd {{ruta}}", false))),
            AlConectar::Invalido("al conectar".to_string())
        );
        let aviso = aviso_al_conectar("al conectar");
        assert!(aviso.contains("no válido"));
        assert!(aviso.contains("edítalo en el escritorio"));
    }
}
