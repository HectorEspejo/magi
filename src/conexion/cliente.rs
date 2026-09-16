use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use russh::client::Handle;
use russh::keys::agent::client::AgentClient;
use russh::keys::agent::AgentIdentity;
use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg, Disconnect};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use zeroize::Zeroizing;

use super::huellas::{self, EstadoHuella};
use super::salto::{conectar_cadena, construir_cadena, Transporte};
use super::terminal::{self, Pantalla};
use super::{ComandoConexion, EventoConexion, PlanConexion};
use crate::modelo::{EstadoSesion, Host, IdentidadRef};

#[derive(Debug, thiserror::Error)]
pub enum ErrorCliente {
    #[error("{0}")]
    Russh(#[from] russh::Error),
    #[error("{0}")]
    Mensaje(String),
    #[error("cancelado por el usuario")]
    Cancelado,
}

impl From<anyhow::Error> for ErrorCliente {
    fn from(error: anyhow::Error) -> Self {
        ErrorCliente::Mensaje(error.to_string())
    }
}

/// Datos compartidos por toda la tarea de conexión. `interactivo` distingue
/// las conexiones de la UI (pueden dialogar) de las automáticas como el sondeo
/// (nunca abren diálogos: huella o frase pendientes son un error).
pub struct Contexto {
    pub known_hosts: PathBuf,
    pub dir_ssh: PathBuf,
    pub hogar: PathBuf,
    pub usuario_local: String,
    pub tx: mpsc::UnboundedSender<EventoConexion>,
    pub interactivo: bool,
    pub fuente_contrasena: super::FuenteContrasena,
    /// Reenvíos remotos vivos del servidor. Es `None` en la TUI y en el sondeo,
    /// que no aceptan canales `forwarded-tcpip`; con `None` se rechazan.
    pub reenvios: Option<std::sync::Arc<super::reenvios::Reenvios>>,
}

/// Implementación del `Handler` de russh: verifica la huella del servidor y,
/// cuando hace falta, pregunta a la UI (solo en modo interactivo).
pub struct Cliente {
    tx: mpsc::UnboundedSender<EventoConexion>,
    host_id: i64,
    host: String,
    puerto: u16,
    known_hosts: PathBuf,
    interactivo: bool,
    reenvios: Option<std::sync::Arc<super::reenvios::Reenvios>>,
}

impl Cliente {
    pub fn nuevo(
        tx: mpsc::UnboundedSender<EventoConexion>,
        host_id: i64,
        host: &str,
        puerto: u16,
        known_hosts: PathBuf,
        interactivo: bool,
    ) -> Self {
        Self {
            tx,
            host_id,
            host: host.to_string(),
            puerto,
            known_hosts,
            interactivo,
            reenvios: None,
        }
    }

    /// El servidor de sesiones comparte aquí el registro de reenvíos remotos.
    pub fn con_reenvios(
        mut self,
        reenvios: Option<std::sync::Arc<super::reenvios::Reenvios>>,
    ) -> Self {
        self.reenvios = reenvios;
        self
    }

    fn error_no_interactivo(&self, motivo: String) -> ErrorCliente {
        ErrorCliente::Mensaje(format!("{motivo}; conéctate una vez con ↵"))
    }
}

impl russh::client::Handler for Cliente {
    type Error = ErrorCliente;

    async fn check_server_key(
        &mut self,
        clave: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let clave = match clave {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(certificado) => {
                certificado.public_key().clone().into()
            }
        };
        match huellas::comprobar(&self.known_hosts, &self.host, self.puerto, &clave) {
            Ok(EstadoHuella::Conocida) => {
                info!(host = %self.host, "huella conocida");
                Ok(true)
            }
            Ok(EstadoHuella::Desconocida) => {
                if !self.interactivo {
                    return Err(self.error_no_interactivo(format!(
                        "huella desconocida de {} en known_hosts",
                        self.host
                    )));
                }
                let (responder, decision) = oneshot::channel();
                let _ = self.tx.send(EventoConexion::HuellaDesconocida {
                    host: self.host.clone(),
                    tipo: huellas::tipo_clave(&clave),
                    huella: huellas::huella(&clave),
                    responder,
                });
                if matches!(decision.await, Ok(true)) {
                    huellas::aprender(&self.known_hosts, &self.host, self.puerto, &clave)?;
                    let _ = self.tx.send(EventoConexion::HuellaRegistrada {
                        host_id: self.host_id,
                        anterior: None,
                        nueva: huellas::huella(&clave),
                    });
                    info!(
                        host = %self.host,
                        huella = %huellas::huella(&clave),
                        "huella nueva aceptada y añadida a known_hosts"
                    );
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Ok(EstadoHuella::Cambiada { anterior }) => {
                if !self.interactivo {
                    return Err(self.error_no_interactivo(format!(
                        "huella cambiada de {} (anterior {})",
                        self.host,
                        huellas::huella(&anterior)
                    )));
                }
                let (responder, decision) = oneshot::channel();
                let _ = self.tx.send(EventoConexion::HuellaCambiada {
                    host: self.host.clone(),
                    tipo: huellas::tipo_clave(&anterior),
                    anterior: format!(
                        "{} {}",
                        huellas::tipo_clave(&anterior),
                        huellas::huella(&anterior)
                    ),
                    nueva: format!(
                        "{} {}",
                        huellas::tipo_clave(&clave),
                        huellas::huella(&clave)
                    ),
                    responder,
                });
                if matches!(decision.await, Ok(true)) {
                    huellas::sustituir(&self.known_hosts, &self.host, self.puerto, &clave)?;
                    let _ = self.tx.send(EventoConexion::HuellaRegistrada {
                        host_id: self.host_id,
                        anterior: Some(huellas::huella(&anterior)),
                        nueva: huellas::huella(&clave),
                    });
                    info!(
                        host = %self.host,
                        anterior = %huellas::huella(&anterior),
                        nueva = %huellas::huella(&clave),
                        "huella sustituida en known_hosts"
                    );
                    Ok(true)
                } else {
                    warn!(host = %self.host, "huella cambiada rechazada por el usuario");
                    Ok(false)
                }
            }
            Err(error) => {
                warn!(host = %self.host, "no se pudo comprobar known_hosts: {error}");
                if self.interactivo {
                    Ok(false)
                } else {
                    Err(ErrorCliente::Mensaje(format!(
                        "no se pudo comprobar known_hosts de {}: {error}",
                        self.host
                    )))
                }
            }
        }
    }

    /// Un canal `forwarded-tcpip`: el host ha recibido una conexión en un
    /// puerto que le pedimos reenviar. Solo se acepta si esa `(dirección,
    /// puerto)` es de un túnel remoto registrado de este host; el resto se
    /// rechaza. El handler por defecto de russh los aceptaría todos.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        canal: Channel<russh::client::Msg>,
        direccion: &str,
        puerto: u32,
        origen_direccion: &str,
        origen_puerto: u32,
        respuesta: russh::client::ChannelOpenHandle,
        _sesion: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let reenvio = self
            .reenvios
            .as_ref()
            .and_then(|reenvios| reenvios.buscar(self.host_id, direccion, puerto));
        let Some(reenvio) = reenvio else {
            warn!(
                host = %self.host,
                direccion = %direccion,
                puerto,
                "canal forwarded-tcpip sin túnel registrado: se rechaza"
            );
            respuesta
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };
        respuesta.accept().await;
        let origen = format!("{origen_direccion}:{origen_puerto}");
        tokio::spawn(super::reenvios::atender(reenvio, canal, origen));
        Ok(())
    }
}

/// Flujo completo de una sesión: conectar, autenticar, abrir pty y shell y
/// servir el bucle de teclas y pantalla hasta el cierre. La lanza el cliente
/// TUI (con puente hacia la UI) y el servidor de sesiones.
pub async fn sesion_completa(
    plan: PlanConexion,
    mut comandos: mpsc::UnboundedReceiver<ComandoConexion>,
    tx: mpsc::UnboundedSender<EventoConexion>,
) {
    let host_id = plan.host.id;
    let nombre = plan.host.nombre.clone();
    let contexto = Contexto {
        known_hosts: plan.known_hosts.clone(),
        dir_ssh: plan.dir_ssh.clone(),
        hogar: plan.hogar.clone(),
        usuario_local: plan.usuario_local.clone(),
        tx: tx.clone(),
        interactivo: true,
        fuente_contrasena: super::FuenteContrasena::Llavero,
        // La TUI no custodia túneles: viven en el servidor de sesiones.
        reenvios: None,
    };
    emitir_estado(&tx, host_id, EstadoSesion::Resolviendo);
    let cadena = match construir_cadena(&plan.host, &plan.todos_los_hosts) {
        Ok(cadena) => cadena,
        Err(error) => return emitir_error(&tx, host_id, &error.to_string()),
    };
    emitir_estado(&tx, host_id, EstadoSesion::Conectando);
    let transporte = match conectar_cadena(&cadena, &contexto).await {
        Ok(transporte) => transporte,
        Err(ErrorCliente::Cancelado) => {
            let _ = tx.send(EventoConexion::Cancelada { host_id });
            return;
        }
        Err(error) => return emitir_error(&tx, host_id, &error.to_string()),
    };

    if plan.solo_prueba {
        let _ = transporte
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
        let _ = tx.send(EventoConexion::PruebaOk {
            host_id,
            identidad: transporte.identidad.descripcion,
            huella: transporte.identidad.huella,
        });
        return;
    }

    let pantalla = terminal::nuevo(plan.filas, plan.cols);
    let mut canal = match abrir_canal(&transporte, plan.cols, plan.filas).await {
        Ok(canal) => canal,
        Err(error) => return emitir_error(&tx, host_id, &error.to_string()),
    };
    plan.registro
        .lock()
        .await
        .insert(host_id, transporte.handle.clone());
    let _ = tx.send(EventoConexion::Abierta {
        host_id,
        pantalla: pantalla.clone(),
        identidad: transporte.identidad.descripcion.clone(),
        huella: transporte.identidad.huella.clone(),
        cols: plan.cols,
        filas: plan.filas,
    });
    info!(host = %nombre, "sesión abierta");
    bucle_sesion(&mut canal, &pantalla, &mut comandos, &tx, host_id).await;
    plan.registro.lock().await.remove(&host_id);
}

pub async fn abrir_canal(
    transporte: &Transporte,
    cols: u16,
    filas: u16,
) -> Result<Channel<russh::client::Msg>, ErrorCliente> {
    let canal = transporte.handle.channel_open_session().await?;
    canal
        .request_pty(
            false,
            "xterm-256color",
            u32::from(cols),
            u32::from(filas),
            0,
            0,
            &[],
        )
        .await
        .context("solicitando PTY")?;
    canal
        .request_shell(true)
        .await
        .context("solicitando shell")?;
    Ok(canal)
}

async fn bucle_sesion(
    canal: &mut Channel<russh::client::Msg>,
    pantalla: &Pantalla,
    comandos: &mut mpsc::UnboundedReceiver<ComandoConexion>,
    tx: &mpsc::UnboundedSender<EventoConexion>,
    host_id: i64,
) {
    let mut intervalo = tokio::time::interval(Duration::from_millis(33));
    let mut sucio = false;
    let mut vio_eof = false;
    loop {
        tokio::select! {
            comando = comandos.recv() => match comando {
                Some(ComandoConexion::Teclas(bytes)) => {
                    if let Err(error) = canal.data_bytes(bytes).await {
                        emitir_error(tx, host_id, &format!("se perdió la conexión: {error}"));
                        return;
                    }
                }
                Some(ComandoConexion::Redimensionar(cols, filas)) => {
                    let _ = canal.window_change(u32::from(cols), u32::from(filas), 0, 0).await;
                    terminal::redimensionar(pantalla, filas, cols);
                    sucio = true;
                }
                Some(ComandoConexion::Cerrar) | None => {
                    let _ = canal.eof().await;
                    let _ = canal.close().await;
                    let _ = tx.send(EventoConexion::Cerrada {
                        host_id,
                        motivo: None,
                    });
                    return;
                }
            },
            mensaje = canal.wait() => match mensaje {
                Some(ChannelMsg::Data { data }) => {
                    procesar(pantalla, &data);
                    sucio = true;
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    procesar(pantalla, &data);
                    sucio = true;
                }
                Some(ChannelMsg::Eof) => {
                    vio_eof = true;
                }
                Some(ChannelMsg::Close) => {
                    let _ = tx.send(EventoConexion::Cerrada {
                        host_id,
                        motivo: None,
                    });
                    return;
                }
                None => {
                    if vio_eof {
                        let _ = tx.send(EventoConexion::Cerrada {
                            host_id,
                            motivo: None,
                        });
                    } else {
                        emitir_error(tx, host_id, "la conexión se ha perdido");
                    }
                    return;
                }
                _ => {}
            },
            _ = intervalo.tick() => {
                if sucio {
                    let _ = tx.send(EventoConexion::Pantalla);
                    sucio = false;
                }
            }
        }
    }
}

fn procesar(pantalla: &Pantalla, datos: &[u8]) {
    if let Ok(mut parser) = pantalla.lock() {
        parser.process(datos);
    }
}

fn emitir_estado(tx: &mpsc::UnboundedSender<EventoConexion>, host_id: i64, estado: EstadoSesion) {
    let _ = tx.send(EventoConexion::Estado { host_id, estado });
}

fn emitir_error(tx: &mpsc::UnboundedSender<EventoConexion>, host_id: i64, motivo: &str) {
    let _ = tx.send(EventoConexion::Error {
        host_id,
        motivo: motivo.to_string(),
    });
}

/// Identidad con la que se autenticó: descripción para la barra y huella para
/// actualizar `IDENTIDADES.ultimo_uso_en`.
#[derive(Debug)]
pub struct IdentidadUsada {
    pub descripcion: String,
    pub huella: Option<String>,
}

/// Autentica según `identidad_ref` y devuelve la identidad usada.
pub async fn autenticar(
    handle: &mut Handle<Cliente>,
    host: &Host,
    contexto: &Contexto,
) -> Result<IdentidadUsada, ErrorCliente> {
    emitir_estado(&contexto.tx, host.id, EstadoSesion::Autenticando);
    let usuario = host
        .usuario
        .clone()
        .unwrap_or_else(|| contexto.usuario_local.clone());
    match &host.identidad_ref {
        IdentidadRef::Auto => {
            if let Some(identidad) = autenticar_con_agente(handle, &usuario, None).await? {
                return Ok(identidad);
            }
            let mut motivo_frase: Option<String> = None;
            for ruta in ficheros_por_defecto(&contexto.dir_ssh) {
                if !ruta.exists() {
                    continue;
                }
                let clave = match cargar_clave(&ruta, contexto, &host.nombre).await {
                    Ok(Some(clave)) => clave,
                    Ok(None) => return Err(ErrorCliente::Cancelado),
                    Err(error) => {
                        motivo_frase = Some(error.to_string());
                        continue;
                    }
                };
                let tipo = clave.algorithm().as_str().to_string();
                let huella = huellas::huella(clave.public_key());
                if autenticar_clave(handle, &usuario, clave).await? {
                    return Ok(IdentidadUsada {
                        descripcion: format!(
                            "{tipo} · fichero {}",
                            acortar_hogar(&ruta, &contexto.hogar)
                        ),
                        huella: Some(huella),
                    });
                }
            }
            Err(ErrorCliente::Mensaje(motivo_frase.unwrap_or_else(|| {
                "autenticación rechazada: agente y claves por defecto agotados".to_string()
            })))
        }
        IdentidadRef::Agente(huella) => {
            match autenticar_con_agente(handle, &usuario, Some(huella)).await? {
                Some(identidad) => Ok(identidad),
                None => Err(ErrorCliente::Mensaje(format!(
                    "la clave {huella} no está cargada en el agente; ejecuta ssh-add"
                ))),
            }
        }
        IdentidadRef::Contrasena => {
            autenticar_con_contrasena(handle, host, contexto, &usuario, false).await
        }
        IdentidadRef::ContrasenaLlavero => {
            if contexto.fuente_contrasena == super::FuenteContrasena::Solicitante {
                // El servidor no toca el llavero: la contraseña la aporta el
                // cliente solicitante por el protocolo.
                return autenticar_con_contrasena(handle, host, contexto, &usuario, true).await;
            }
            let mut habia_entrada = false;
            match crate::llavero::recuperar(&host.nombre, &usuario) {
                Ok(Some(contrasena)) => {
                    habia_entrada = true;
                    let resultado = handle
                        .authenticate_password(usuario.as_str(), contrasena.as_str())
                        .await?;
                    if resultado.success() {
                        return Ok(IdentidadUsada {
                            descripcion: "contraseña · llavero".to_string(),
                            huella: None,
                        });
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if !contexto.interactivo {
                        return Err(ErrorCliente::Mensaje(format!(
                            "«{}» usa contraseña y el llavero no está disponible ({error}); \
                             conéctate una vez con ↵ o usa una clave",
                            host.nombre
                        )));
                    }
                }
            }
            if !contexto.interactivo {
                let motivo = if habia_entrada {
                    "la contraseña guardada en el llavero ya no vale"
                } else {
                    "no hay ninguna contraseña guardada en el llavero"
                };
                return Err(ErrorCliente::Mensaje(format!(
                    "«{}» usa contraseña y {motivo}; conéctate una vez con ↵ o usa una clave",
                    host.nombre
                )));
            }
            autenticar_con_contrasena(handle, host, contexto, &usuario, true).await
        }
        IdentidadRef::Fichero(ruta) => {
            let ruta = expandir_home(ruta, &contexto.hogar);
            if !ruta.exists() {
                return Err(ErrorCliente::Mensaje(format!(
                    "la clave {} no existe",
                    ruta.display()
                )));
            }
            let Some(clave) = cargar_clave(&ruta, contexto, &host.nombre).await? else {
                return Err(ErrorCliente::Cancelado);
            };
            let tipo = clave.algorithm().as_str().to_string();
            let huella = huellas::huella(clave.public_key());
            if !autenticar_clave(handle, &usuario, clave).await? {
                return Err(ErrorCliente::Mensaje("autenticación rechazada".to_string()));
            }
            Ok(IdentidadUsada {
                descripcion: format!("{tipo} · fichero {}", acortar_hogar(&ruta, &contexto.hogar)),
                huella: Some(huella),
            })
        }
    }
}

/// Pide la contraseña en la UI (hasta 3 intentos) y, si el usuario lo marca,
/// la guarda en el llavero del sistema. El secreto no toca `magi.db`.
async fn autenticar_con_contrasena(
    handle: &mut Handle<Cliente>,
    host: &Host,
    contexto: &Contexto,
    usuario: &str,
    recordar_por_defecto: bool,
) -> Result<IdentidadUsada, ErrorCliente> {
    if !contexto.interactivo {
        return Err(ErrorCliente::Mensaje(format!(
            "«{}» autentica con contraseña y el sondeo no dialoga; \
             conéctate una vez con ↵ o usa una clave",
            host.nombre
        )));
    }
    for intento in 1..=3u8 {
        let (responder, respuesta) = oneshot::channel();
        let _ = contexto.tx.send(EventoConexion::PideContrasena {
            host: host.nombre.clone(),
            intento,
            recordar_por_defecto,
            responder,
        });
        match respuesta.await {
            Ok(Some((contrasena, recordar))) => {
                let resultado = handle
                    .authenticate_password(usuario, contrasena.as_str())
                    .await?;
                if !resultado.success() {
                    continue;
                }
                let mut en_llavero = false;
                if recordar && contexto.fuente_contrasena == super::FuenteContrasena::Llavero {
                    match crate::llavero::guardar(&host.nombre, usuario, contrasena.as_str()) {
                        Ok(()) => en_llavero = true,
                        Err(error) => {
                            let _ = contexto.tx.send(EventoConexion::ContrasenaGuardada {
                                host_id: host.id,
                                ok: false,
                                motivo: Some(error),
                            });
                        }
                    }
                    if en_llavero {
                        let _ = contexto.tx.send(EventoConexion::ContrasenaGuardada {
                            host_id: host.id,
                            ok: true,
                            motivo: None,
                        });
                    }
                }
                return Ok(IdentidadUsada {
                    descripcion: if en_llavero {
                        "contraseña · llavero".to_string()
                    } else {
                        "contraseña".to_string()
                    },
                    huella: None,
                });
            }
            _ => return Err(ErrorCliente::Cancelado),
        }
    }
    Err(ErrorCliente::Mensaje(
        "contraseña incorrecta tras 3 intentos".to_string(),
    ))
}

/// Intenta todas las claves del agente o solo la de la huella indicada.
async fn autenticar_con_agente(
    handle: &mut Handle<Cliente>,
    usuario: &str,
    huella_buscada: Option<&str>,
) -> Result<Option<IdentidadUsada>, ErrorCliente> {
    let mut agente = match AgentClient::connect_env().await {
        Ok(agente) => agente,
        Err(error) => {
            if huella_buscada.is_some() {
                return Err(ErrorCliente::Mensaje(format!(
                    "el agente SSH no está disponible: {error}"
                )));
            }
            return Ok(None);
        }
    };
    let identidades = agente
        .request_identities()
        .await
        .map_err(|error| ErrorCliente::Mensaje(format!("error del agente: {error}")))?;
    for identidad in identidades {
        match identidad {
            AgentIdentity::PublicKey { key, comment } => {
                if let Some(buscada) = huella_buscada {
                    if huellas::huella(&key) != buscada {
                        continue;
                    }
                }
                let hash = if key.algorithm().is_rsa() {
                    handle.best_supported_rsa_hash().await?.flatten()
                } else {
                    None
                };
                match handle
                    .authenticate_publickey_with(usuario, key.clone(), hash, &mut agente)
                    .await
                {
                    Ok(resultado) if resultado.success() => {
                        let huella = huellas::huella(&key);
                        return Ok(Some(IdentidadUsada {
                            descripcion: descripcion_agente(&key, &comment, &huella),
                            huella: Some(huella),
                        }));
                    }
                    Ok(_) => continue,
                    Err(error) => {
                        return Err(ErrorCliente::Mensaje(format!("error del agente: {error}")))
                    }
                }
            }
            AgentIdentity::Certificate {
                certificate,
                comment,
            } => {
                let clave_cert: russh::keys::PublicKey = certificate.public_key().clone().into();
                if let Some(buscada) = huella_buscada {
                    if huellas::huella(&clave_cert) != buscada {
                        continue;
                    }
                }
                match handle
                    .authenticate_certificate_with(usuario, certificate.clone(), None, &mut agente)
                    .await
                {
                    Ok(resultado) if resultado.success() => {
                        let huella = huellas::huella(&clave_cert);
                        return Ok(Some(IdentidadUsada {
                            descripcion: descripcion_agente(&clave_cert, &comment, &huella),
                            huella: Some(huella),
                        }));
                    }
                    Ok(_) => continue,
                    Err(error) => {
                        return Err(ErrorCliente::Mensaje(format!("error del agente: {error}")))
                    }
                }
            }
        }
    }
    Ok(None)
}

fn descripcion_agente(clave: &russh::keys::PublicKey, comentario: &str, huella: &str) -> String {
    let algoritmo = clave.algorithm();
    let tipo = algoritmo.as_str();
    if comentario.is_empty() {
        format!("{tipo} · agente · {huella}")
    } else {
        format!("{tipo} · agente · {comentario} · {huella}")
    }
}

async fn autenticar_clave(
    handle: &mut Handle<Cliente>,
    usuario: &str,
    clave: russh::keys::PrivateKey,
) -> Result<bool, ErrorCliente> {
    let hash = if clave.algorithm().is_rsa() {
        handle.best_supported_rsa_hash().await?.flatten()
    } else {
        None
    };
    let con_hash = PrivateKeyWithHashAlg::new(Arc::new(clave), hash);
    Ok(handle
        .authenticate_publickey(usuario, con_hash)
        .await?
        .success())
}

/// Carga una clave de fichero; si está cifrada pide la frase en la TUI con
/// tres intentos. La frase vive en memoria con `zeroize` y se descarta.
async fn cargar_clave(
    ruta: &Path,
    contexto: &Contexto,
    host_nombre: &str,
) -> Result<Option<russh::keys::PrivateKey>, ErrorCliente> {
    let datos = Zeroizing::new(
        std::fs::read_to_string(ruta).with_context(|| format!("leyendo {}", ruta.display()))?,
    );
    if let Ok(clave) = decode_secret_key(&datos, None) {
        return Ok(Some(clave));
    }
    if !contexto.interactivo {
        return Err(ErrorCliente::Mensaje(format!(
            "la clave {} requiere frase y no hay diálogo posible",
            ruta.display()
        )));
    }
    for intento in 1..=3u8 {
        let (responder, respuesta) = oneshot::channel();
        let _ = contexto.tx.send(EventoConexion::PideFrase {
            host: host_nombre.to_string(),
            intento,
            responder,
        });
        match respuesta.await {
            Ok(Some(frase)) => {
                if let Ok(clave) = decode_secret_key(&datos, Some(frase.as_str())) {
                    return Ok(Some(clave));
                }
            }
            _ => return Ok(None),
        }
    }
    Err(ErrorCliente::Mensaje(format!(
        "frase incorrecta tras 3 intentos en {}",
        ruta.display()
    )))
}

fn ficheros_por_defecto(dir_ssh: &Path) -> Vec<PathBuf> {
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|nombre| dir_ssh.join(nombre))
        .collect()
}

pub fn expandir_home(ruta: &str, hogar: &Path) -> PathBuf {
    if let Some(resto) = ruta.strip_prefix("~/") {
        hogar.join(resto)
    } else if ruta == "~" {
        hogar.to_path_buf()
    } else {
        PathBuf::from(ruta)
    }
}

fn acortar_hogar(ruta: &Path, hogar: &Path) -> String {
    match ruta.strip_prefix(hogar) {
        Ok(resto) => format!("~/{}", resto.display()),
        Err(_) => ruta.display().to_string(),
    }
}
