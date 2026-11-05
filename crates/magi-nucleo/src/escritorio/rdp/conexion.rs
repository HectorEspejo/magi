//! Conexión RDP: X.224, TLS manual, política de huella, credenciales y
//! connect_finalize con NLA.
//!
//! El orden es el que exige la política de MAGI: primero la huella del
//! certificado, después las credenciales. La contraseña no entra en el conector
//! hasta que el usuario ha aceptado el certificado.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use ironrdp::cliprdr::CliprdrClient;
use ironrdp::connector::sspi::generator::NetworkRequest;
use ironrdp::connector::{
    ClientConnector, ClientConnectorState, Config, ConnectionResult, ConnectorError,
    ConnectorErrorKind, Credentials, DesktopSize, ServerName,
};
use ironrdp::displaycontrol::client::DisplayControlClient;
use ironrdp::dvc::DrdynvcClient;
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::nego::FailureCode;
use ironrdp::pdu::rdp::capability_sets::MajorPlatformType;
use ironrdp_tokio::{
    connect_begin, connect_finalize, mark_as_upgraded, MovableTokioFramed, NetworkClient,
    TokioFramed,
};
use tokio::sync::mpsc;

use super::certificado::CertificadoServidor;
use super::cliprdr::{AccionPortapapeles, Portapapeles};
use super::egfx::Egfx;
use super::tls;
use super::transporte::{FlujoConSobrante, FlujoFramed, FlujoUnido};
use crate::escritorio::{
    ConfigEscritorio, DatosCertificado, Dialogo, ErrorEscritorio, EventoEscritorio,
    PeticionCredenciales, TipoCambioCertificado, TipoHost,
};

/// Plazo de las operaciones de red del saludo (10 s, como el resto de MAGI).
pub(crate) const PLAZO_RED: Duration = Duration::from_secs(10);

/// Motivo exacto que pide el checklist para un servidor sin TLS.
pub(crate) const SIN_TLS: &str = "este servidor solo habla RDP antiguo, sin TLS: no admitido";

/// Motivo de un rechazo de credenciales.
pub(crate) const AUTENTICACION_FALLIDA: &str = "usuario o contraseña incorrectos";

/// Una conexión RDP ya finalizada, antes de empezar la sesión activa.
pub(crate) struct SesionConectada {
    pub(crate) framed: FlujoFramed,
    pub(crate) resultado: ConnectionResult,
    pub(crate) acciones: mpsc::Receiver<AccionPortapapeles>,
}

/// Recorre el saludo completo hasta tener la sesión activa.
///
/// Los diálogos (certificado y credenciales) no llevan plazo: los cierra el
/// usuario. La red sí: 10 s por paso.
pub(crate) async fn conectar(
    flujo: crate::escritorio::Flujo,
    config: &ConfigEscritorio,
    dialogo: &Dialogo,
    eventos: mpsc::Sender<EventoEscritorio>,
    intento: u8,
) -> Result<SesionConectada, ErrorEscritorio> {
    let servidor = ServerName::new(config.direccion.clone());
    let nombre_tls = servidor.as_str().to_owned();

    let (acciones_tx, acciones) = mpsc::channel(8);
    let mut conector = construir(config, eventos, acciones_tx);

    let mut framed = TokioFramed::new(FlujoUnido::nuevo(flujo));

    let deberia_actualizar =
        match tokio::time::timeout(PLAZO_RED, connect_begin(&mut framed, &mut conector)).await {
            Ok(Ok(deberia)) => deberia,
            Ok(Err(error)) => return Err(error_de_conector(error)),
            Err(_) => {
                // Si el servidor ha elegido la seguridad RDP clásica, IronRDP no
                // llega a pedir el upgrade de TLS y el saludo se queda parado.
                // Si aún estábamos esperando la confirmación X.224, el servidor
                // simplemente no contesta: no es lo mismo.
                let esperando_confirmacion = matches!(
                    conector.state,
                    ClientConnectorState::ConnectionInitiationSendRequest
                        | ClientConnectorState::ConnectionInitiationWaitConfirm { .. }
                );
                if !esperando_confirmacion && !conector.should_perform_security_upgrade() {
                    return Err(ErrorEscritorio::Protocolo(SIN_TLS.to_string()));
                }
                return Err(ErrorEscritorio::Conexion(
                    "el servidor RDP no ha contestado al saludo en 10 s".to_string(),
                ));
            }
        };

    // El servidor puede mandar bytes del siguiente PDU pegados al saludo: se
    // conservan y se entregan antes que el saludo TLS.
    let (flujo_en_claro, sobrante) = framed.into_inner();
    let sobrante = sobrante.to_vec();

    let (flujo_tls, certificado) = tls::actualizar(
        FlujoConSobrante::nuevo(flujo_en_claro, sobrante),
        &nombre_tls,
    )
    .await?;

    // Política de huella: igual, nueva o cambiada. Nunca se manda ninguna
    // credencial antes de este paso.
    if let Some((tipo, anterior)) = politica(config, &certificado) {
        let datos = DatosCertificado {
            tipo,
            sujeto: certificado.sujeto.clone(),
            caducidad: certificado.caducidad.clone(),
            huella: certificado.huella.clone(),
            anterior,
        };
        if !dialogo.certificado(datos).await? {
            return Err(ErrorEscritorio::Cancelado(
                "certificado no aceptado".to_string(),
            ));
        }
    }

    // Credenciales: el llavero o el diálogo del cliente que abrió la sesión.
    let peticion = PeticionCredenciales {
        tipo: TipoHost::Rdp,
        usuario: config.usuario.clone(),
        dominio: config.dominio.clone(),
        intento,
    };
    let Some(credenciales) = dialogo.credenciales(peticion).await? else {
        return Err(ErrorEscritorio::Cancelado(
            "no se han dado credenciales".to_string(),
        ));
    };
    let credenciales = config.credenciales(credenciales);
    if credenciales.usuario.trim().is_empty() {
        return Err(ErrorEscritorio::Cancelado(
            "no se han dado credenciales".to_string(),
        ));
    }

    conector.config.credentials = Credentials::UsernamePassword {
        username: credenciales.usuario.clone(),
        password: credenciales.contrasena.to_string(),
    };
    conector.config.domain = credenciales.dominio.clone();

    let actualizado = mark_as_upgraded(deberia_actualizar, &mut conector);

    let mut framed = MovableTokioFramed::new(flujo_tls);

    let finalizado = tokio::time::timeout(
        PLAZO_RED,
        connect_finalize(
            actualizado,
            conector,
            &mut framed,
            &mut ClienteRed,
            servidor,
            certificado.clave_publica,
            None,
        ),
    )
    .await;

    let resultado = match finalizado {
        Ok(Ok(resultado)) => resultado,
        Ok(Err(error)) => return Err(error_de_conector(error)),
        Err(_) => {
            return Err(ErrorEscritorio::Conexion(
                "el servidor RDP no ha terminado la autenticación en 10 s".to_string(),
            ));
        }
    };

    Ok(SesionConectada {
        framed,
        resultado,
        acciones,
    })
}

/// Decide qué enseñar del certificado. None si es el mismo que ya se aceptó.
fn politica(
    config: &ConfigEscritorio,
    certificado: &CertificadoServidor,
) -> Option<(TipoCambioCertificado, Option<String>)> {
    let guardada = config
        .huella_escritorio
        .as_deref()
        .map(str::trim)
        .filter(|huella| !huella.is_empty());

    match guardada {
        Some(guardada) if guardada == certificado.huella => None,
        Some(guardada) => Some((TipoCambioCertificado::Cambiado, Some(guardada.to_string()))),
        None => Some((TipoCambioCertificado::Nuevo, None)),
    }
}

/// Construye el conector con todos los canales antes de conectar: la lista de
/// canales viaja en el GCC y después ya no se negocia nada.
fn construir(
    config: &ConfigEscritorio,
    eventos: mpsc::Sender<EventoEscritorio>,
    acciones: mpsc::Sender<AccionPortapapeles>,
) -> ClientConnector {
    let mut drdynvc = DrdynvcClient::new()
        .with_dynamic_channel(DisplayControlClient::new(|_capacidades| Ok(Vec::new())));

    if config.h264 {
        drdynvc = drdynvc.with_dynamic_channel(Egfx::nuevo(eventos));
    }

    let conector = ClientConnector::new(configuracion(config), direccion_cliente())
        .with_static_channel(drdynvc);

    if config.portapapeles {
        conector.with_static_channel(CliprdrClient::new(Box::new(Portapapeles::nuevo(acciones))))
    } else {
        conector
    }
}

/// Dirección del cliente para el PDU de información: el flujo puede ser un
/// canal SSH y no se conoce la local. La dirección no participa en la
/// seguridad.
fn direccion_cliente() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
}

/// Configuración del conector de IronRDP.
fn configuracion(config: &ConfigEscritorio) -> Config {
    let (ancho, alto) = tamano_inicial(config.tamano_inicial);

    Config {
        desktop_size: DesktopSize {
            width: ancho,
            height: alto,
        },
        desktop_scale_factor: 0,
        // Se anuncian SSL y HYBRID: el servidor elige NLA si la tiene y TLS sin
        // NLA si no. El upgrade de TLS lo hace este cliente a mano.
        enable_tls: true,
        enable_credssp: true,
        credentials: Credentials::UsernamePassword {
            username: config.usuario.clone().unwrap_or_default(),
            password: String::new(),
        },
        domain: config.dominio.clone(),
        client_build: 0,
        client_name: "MAGI".to_string(),
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: klid(&config.distribucion_teclado),
        ime_file_name: String::new(),
        bitmap: None,
        dig_product_id: String::new(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_string(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: MajorPlatformType::UNIX,
        hardware_id: None,
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        performance_flags: Default::default(),
        license_cache: None,
        timezone_info: Default::default(),
        compression_type: None,
        enable_server_pointer: true,
        // El cursor se pinta en el visor con los eventos Cursor, no dentro de
        // la imagen: sin renderizado software no se duplica.
        pointer_software_rendering: false,
        multitransport_flags: None,
    }
}

/// Tamaño inicial acotado a lo que admite un servidor RDP (200..8192).
fn tamano_inicial((ancho, alto): (u16, u16)) -> (u16, u16) {
    (
        ancho.clamp(200, crate::escritorio::LADO_MAX),
        alto.clamp(200, crate::escritorio::LADO_MAX),
    )
}

/// Distribución configurada al identificador de distribución de Windows
/// (KLID). 0 deja que el servidor use la suya.
fn klid(distribucion: &str) -> u32 {
    match distribucion {
        "es-ES" => 0x0000_040A,
        "es-AR" => 0x0000_2C0A,
        "es-MX" => 0x0000_080A,
        "en-US" => 0x0000_0409,
        "en-GB" => 0x0000_0809,
        "pt-BR" => 0x0000_0416,
        "pt-PT" => 0x0000_0816,
        "fr-FR" => 0x0000_040C,
        "de-DE" => 0x0000_0407,
        "it-IT" => 0x0000_0410,
        _ => 0,
    }
}

/// Traduce un error del conector al error del contrato.
fn error_de_conector(error: ConnectorError) -> ErrorEscritorio {
    match error.kind() {
        ConnectorErrorKind::Negotiation(fallo)
            if fallo.code() == FailureCode::SSL_NOT_ALLOWED_BY_SERVER =>
        {
            ErrorEscritorio::Protocolo(SIN_TLS.to_string())
        }
        ConnectorErrorKind::Negotiation(fallo) => ErrorEscritorio::Protocolo(format!(
            "el servidor RDP no admite la seguridad negociada: {fallo}"
        )),
        ConnectorErrorKind::Reason(motivo) if motivo.contains("server selected") => {
            ErrorEscritorio::Protocolo(SIN_TLS.to_string())
        }
        ConnectorErrorKind::Reason(motivo) => ErrorEscritorio::Protocolo(format!(
            "el servidor RDP ha rechazado la negociación: {motivo}"
        )),
        // Con NTLM/NLA, un fallo de CredSSP en este punto es un rechazo de las
        // credenciales: la sesión puede reintentar con otras.
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => {
            ErrorEscritorio::Autenticacion(AUTENTICACION_FALLIDA.to_string())
        }
        _ => ErrorEscritorio::Conexion(format!("no se pudo conectar con el servidor RDP: {error}")),
    }
}

/// NetworkClient sin red: NTLM no genera peticiones de red (solo Kerberos, que
/// MAGI no usa). Si llegara alguna, se dice y no se abre nada.
struct ClienteRed;

impl NetworkClient for ClienteRed {
    async fn send(
        &mut self,
        peticion: &NetworkRequest,
    ) -> ironrdp::connector::ConnectorResult<Vec<u8>> {
        Err(ironrdp::connector::reason_err!(
            "NetworkClient sin red",
            "el servidor ha pedido {peticion:?}: MAGI solo admite NTLM"
        ))
    }
}
