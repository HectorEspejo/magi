use std::path::{Path, PathBuf};

use russh::keys::agent::client::AgentClient;
use russh::keys::agent::AgentIdentity;
use russh::keys::{HashAlg, PublicKey};

#[derive(Debug, Clone)]
pub struct ClaveFichero {
    pub ruta: PathBuf,
    pub tipo: String,
    pub huella: String,
    pub comentario: String,
}

#[derive(Debug, Clone)]
pub struct ClaveAgente {
    pub tipo: String,
    pub huella: String,
    pub comentario: String,
    pub clave: PublicKey,
}

#[derive(Debug, Clone, Default)]
pub struct Identidades {
    pub ficheros: Vec<ClaveFichero>,
    pub agente: Option<Vec<ClaveAgente>>,
    pub aviso_agente: Option<String>,
}

pub fn huella(clave: &PublicKey) -> String {
    clave.fingerprint(HashAlg::Sha256).to_string()
}

pub fn tipo(clave: &PublicKey) -> String {
    clave.algorithm().as_str().to_string()
}

/// Escanea `~/.ssh/*.pub` y devuelve las claves cuyo fichero privado existe.
pub fn escanear_ficheros(dir_ssh: &Path) -> Vec<ClaveFichero> {
    let mut claves = Vec::new();
    let Ok(entradas) = std::fs::read_dir(dir_ssh) else {
        return claves;
    };
    for entrada in entradas.flatten() {
        let ruta_publica = entrada.path();
        if ruta_publica.extension().and_then(|e| e.to_str()) != Some("pub") {
            continue;
        }
        let Some(ruta_privada) = ruta_publica.to_str().map(|r| r.trim_end_matches(".pub")) else {
            continue;
        };
        let ruta_privada = PathBuf::from(ruta_privada);
        if !ruta_privada.exists() {
            continue;
        }
        let Ok(texto) = std::fs::read_to_string(&ruta_publica) else {
            continue;
        };
        let Ok(clave) = PublicKey::from_openssh(&texto) else {
            continue;
        };
        let comentario = texto
            .split_whitespace()
            .nth(2)
            .unwrap_or_default()
            .to_string();
        claves.push(ClaveFichero {
            ruta: ruta_privada,
            tipo: tipo(&clave),
            huella: huella(&clave),
            comentario,
        });
    }
    claves.sort_by(|a, b| a.ruta.cmp(&b.ruta));
    claves
}

/// Escanea el agente SSH (`SSH_AUTH_SOCK`) además de los ficheros.
pub async fn escanear(dir_ssh: &Path) -> Identidades {
    let ficheros = escanear_ficheros(dir_ssh);
    match AgentClient::connect_env().await {
        Ok(mut agente) => match agente.request_identities().await {
            Ok(lista) => {
                let mut claves = Vec::new();
                for identidad in lista {
                    match identidad {
                        AgentIdentity::PublicKey { key, comment } => claves.push(ClaveAgente {
                            tipo: tipo(&key),
                            huella: huella(&key),
                            comentario: comment,
                            clave: key,
                        }),
                        AgentIdentity::Certificate {
                            certificate,
                            comment,
                        } => {
                            let clave: PublicKey = certificate.public_key().clone().into();
                            claves.push(ClaveAgente {
                                tipo: format!("{}-cert", tipo(&clave)),
                                huella: huella(&clave),
                                comentario: comment,
                                clave,
                            });
                        }
                    }
                }
                Identidades {
                    ficheros,
                    agente: Some(claves),
                    aviso_agente: None,
                }
            }
            Err(error) => Identidades {
                ficheros,
                agente: None,
                aviso_agente: Some(format!("el agente SSH no respondió: {error}")),
            },
        },
        Err(error) => Identidades {
            ficheros,
            agente: None,
            aviso_agente: Some(format!("el agente SSH no está disponible: {error}")),
        },
    }
}
