//! Upgrade TLS manual con rustls y aws-lc-rs.
//!
//! El verificador no valida la cadena (la política de MAGI es la misma que con
//! las huellas SSH: decide el usuario) pero CAPTURA el DER de la hoja para
//! poder calcular la huella antes de mandar ninguna credencial.
//!
//! La reanudación TLS se desactiva a propósito: CredSSP no la admite
//! (MS-CSSP 3.1.5). No se activa el volcado de claves de sesión.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName as NombreTls, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, Error as ErrorTls, SignatureScheme};

use super::certificado::CertificadoServidor;
use super::transporte::FlujoConSobrante;
use crate::escritorio::ErrorEscritorio;

/// Plazo del saludo TLS: es red, no diálogo.
const PLAZO_TLS: Duration = Duration::from_secs(10);

/// Verificador que acepta cualquier certificado y captura el DER de la hoja.
///
/// Aceptar sin validar es deliberado: la huella se compara después con la que
/// aceptó el usuario y la decisión se toma fuera del saludo TLS.
struct VerificadorQueCaptura {
    capturado: Arc<Mutex<Option<Vec<u8>>>>,
}

impl std::fmt::Debug for VerificadorQueCaptura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerificadorQueCaptura")
    }
}

impl ServerCertVerifier for VerificadorQueCaptura {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermedios: &[CertificateDer<'_>],
        _nombre: &NombreTls<'_>,
        _ocsp: &[u8],
        _ahora: UnixTime,
    ) -> Result<ServerCertVerified, ErrorTls> {
        let mut capturado = self
            .capturado
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner());
        *capturado = Some(end_entity.as_ref().to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _mensaje: &[u8],
        _certificado: &CertificateDer<'_>,
        _firma: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, ErrorTls> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _mensaje: &[u8],
        _certificado: &CertificateDer<'_>,
        _firma: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, ErrorTls> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

/// Hace el saludo TLS sobre el flujo en claro y devuelve el flujo cifrado con
/// el certificado del servidor ya diseccionado.
pub(crate) async fn actualizar(
    flujo: FlujoConSobrante,
    servidor: &str,
) -> Result<(TlsStream<FlujoConSobrante>, CertificadoServidor), ErrorEscritorio> {
    let capturado = Arc::new(Mutex::new(None));

    let verificador = VerificadorQueCaptura {
        capturado: Arc::clone(&capturado),
    };

    let mut configuracion = tokio_rustls::rustls::client::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verificador))
        .with_no_client_auth();

    // CredSSP no soporta la reanudación de sesiones TLS (MS-CSSP 3.1.5).
    configuracion.resumption = tokio_rustls::rustls::client::Resumption::disabled();

    let conector = tokio_rustls::TlsConnector::from(Arc::new(configuracion));
    let nombre = NombreTls::try_from(servidor.to_owned()).map_err(|_| {
        ErrorEscritorio::Conexion(format!("nombre de servidor TLS inválido: {servidor}"))
    })?;

    let saludo = async {
        let mut tls = conector.connect(nombre, flujo).await?;
        tls.flush().await?;
        Ok::<_, std::io::Error>(tls)
    };

    let tls = tokio::time::timeout(PLAZO_TLS, saludo)
        .await
        .map_err(|_| {
            ErrorEscritorio::Conexion(
                "el servidor RDP no ha completado el saludo TLS en 10 s".to_string(),
            )
        })?
        .map_err(|error| {
            ErrorEscritorio::Conexion(format!(
                "no se pudo establecer TLS con el servidor: {error}"
            ))
        })?;

    // El verificador lo capturó durante el saludo; si por lo que sea no lo
    // hiciera, la conexión deja el certificado a mano.
    let der = {
        let mut capturado = capturado
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner());
        capturado.take()
    }
    .or_else(|| {
        tls.get_ref()
            .1
            .peer_certificates()
            .and_then(|certificados| certificados.first())
            .map(|certificado| certificado.as_ref().to_vec())
    })
    .ok_or_else(|| {
        ErrorEscritorio::Protocolo("el servidor RDP no presentó certificado TLS".to_string())
    })?;

    let certificado = CertificadoServidor::desde_der(der)?;

    Ok((tls, certificado))
}
