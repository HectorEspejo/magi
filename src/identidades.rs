use std::path::{Path, PathBuf};

use russh::keys::agent::client::AgentClient;
use russh::keys::agent::{AgentIdentity, Constraint};
use russh::keys::{HashAlg, PublicKey};
use ssh_key::rand_core::UnwrapErr;
use ssh_key::{getrandom::SysRng, Algorithm, LineEnding, PrivateKey};
use zeroize::Zeroizing;

use crate::almacen::identidades::ClaveSincronizada;
use crate::modelo::OrigenIdentidad;

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

/// Tipo legible de la clave: `ed25519`, `rsa`, `ecdsa`, `ed25519-sk` o
/// `ecdsa-sk`.
pub fn normalizar_tipo(algoritmo: &str) -> String {
    let algoritmo = algoritmo.to_lowercase();
    if algoritmo.starts_with("sk-ssh-ed25519") {
        "ed25519-sk".to_string()
    } else if algoritmo.starts_with("sk-ecdsa") {
        "ecdsa-sk".to_string()
    } else if algoritmo.starts_with("ssh-ed25519") || algoritmo == "ed25519" {
        "ed25519".to_string()
    } else if algoritmo.starts_with("ssh-rsa") || algoritmo == "rsa" {
        "rsa".to_string()
    } else if algoritmo.starts_with("ecdsa") {
        "ecdsa".to_string()
    } else {
        algoritmo
    }
}

pub fn tipo(clave: &PublicKey) -> String {
    normalizar_tipo(clave.algorithm().as_str())
}

/// Claves del último escaneo, listas para el *upsert* en IDENTIDADES. Los
/// certificados no se registran como identidades.
pub fn claves_sincronizables(identidades: &Identidades) -> Vec<ClaveSincronizada> {
    let mut claves = Vec::new();
    for clave in &identidades.ficheros {
        claves.push(ClaveSincronizada {
            origen: origen_de(&clave.tipo, true),
            tipo: clave.tipo.clone(),
            huella: clave.huella.clone(),
            ruta: Some(clave.ruta.display().to_string()),
            comentario: (!clave.comentario.is_empty()).then(|| clave.comentario.clone()),
        });
    }
    if let Some(agente) = &identidades.agente {
        for clave in agente {
            if clave.tipo.ends_with("-cert") {
                continue;
            }
            claves.push(ClaveSincronizada {
                origen: origen_de(&clave.tipo, false),
                tipo: clave.tipo.clone(),
                huella: clave.huella.clone(),
                ruta: None,
                comentario: (!clave.comentario.is_empty()).then(|| clave.comentario.clone()),
            });
        }
    }
    claves
}

fn origen_de(tipo: &str, en_fichero: bool) -> OrigenIdentidad {
    if tipo.ends_with("-sk") {
        OrigenIdentidad::Token
    } else if en_fichero {
        OrigenIdentidad::Fichero
    } else {
        OrigenIdentidad::Agente
    }
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

/// Tipo de clave a generar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoClave {
    Ed25519,
    Rsa4096,
}

/// Petición de generación, tal y como sale del diálogo.
pub struct PeticionGeneracion {
    pub dir_ssh: PathBuf,
    pub nombre: String,
    pub tipo: TipoClave,
    pub comentario: String,
    /// Frase vacía = clave sin cifrar (el diálogo avisa de ello).
    pub frase: Zeroizing<String>,
    pub anadir_agente: bool,
    pub copiar_publica: bool,
}

/// Resultado de generar una clave.
#[derive(Debug)]
pub struct ClaveGenerada {
    pub tipo: String,
    pub huella: String,
    pub ruta: PathBuf,
    pub publica: String,
    pub comentario: Option<String>,
    pub en_agente: bool,
    pub aviso_agente: Option<String>,
    pub copiada: bool,
    pub solicito_copia: bool,
}

/// Genera el par de claves con `ssh-key` y lo escribe en `~/.ssh` sin
/// sobrescribir nunca. La parte CPU va en `spawn_blocking`; el alta en el
/// agente se hace después con `add_identity`.
pub async fn generar(peticion: PeticionGeneracion) -> Result<ClaveGenerada, String> {
    let anadir_agente = peticion.anadir_agente;
    let copiar_publica = peticion.copiar_publica;
    let (clave, datos) = tokio::task::spawn_blocking(move || generar_en_disco(peticion))
        .await
        .map_err(|error| error.to_string())??;
    let mut en_agente = false;
    let mut aviso_agente = None;
    if anadir_agente {
        match AgentClient::connect_env().await {
            Ok(mut agente) => match agente.add_identity(&clave, &[] as &[Constraint]).await {
                Ok(()) => en_agente = true,
                Err(error) => aviso_agente = Some(format!("el agente rechazó la clave: {error}")),
            },
            Err(error) => aviso_agente = Some(format!("el agente SSH no está disponible: {error}")),
        }
    }
    drop(clave);
    let copiada = copiar_publica && crate::portapapeles::copiar(&datos.publica).is_ok();
    Ok(ClaveGenerada {
        tipo: datos.tipo,
        huella: datos.huella,
        ruta: datos.ruta,
        publica: datos.publica,
        comentario: datos.comentario,
        en_agente,
        aviso_agente,
        copiada,
        solicito_copia: copiar_publica,
    })
}

struct DatosEnDisco {
    tipo: String,
    huella: String,
    ruta: PathBuf,
    publica: String,
    comentario: Option<String>,
}

fn generar_en_disco(peticion: PeticionGeneracion) -> Result<(PrivateKey, DatosEnDisco), String> {
    let nombre = peticion.nombre.trim();
    if nombre.is_empty()
        || !nombre
            .chars()
            .all(|caracter| caracter.is_ascii_alphanumeric() || matches!(caracter, '.' | '_' | '-'))
    {
        return Err("nombre de fichero no válido".to_string());
    }
    if !peticion.dir_ssh.exists() {
        return Err(format!(
            "no existe el directorio {}; no se pueden escribir claves",
            peticion.dir_ssh.display()
        ));
    }
    let ruta_privada = peticion.dir_ssh.join(nombre);
    let ruta_publica = PathBuf::from(format!("{}.pub", ruta_privada.display()));
    if ruta_privada.exists() || ruta_publica.exists() {
        return Err(format!("ya existe {}", ruta_privada.display()));
    }

    let mut rng = UnwrapErr(SysRng);
    let algoritmo = match peticion.tipo {
        TipoClave::Ed25519 => Algorithm::Ed25519,
        TipoClave::Rsa4096 => Algorithm::Rsa { hash: None },
    };
    let mut clave = PrivateKey::random(&mut rng, algoritmo).map_err(|error| error.to_string())?;
    let comentario = peticion.comentario.trim();
    if !comentario.is_empty() {
        clave.set_comment(comentario);
    }
    let tipo = normalizar_tipo(clave.algorithm().as_str());
    let huella = huella(clave.public_key());
    let publica = format!(
        "{}\n",
        clave
            .public_key()
            .to_openssh()
            .map_err(|error| error.to_string())?
    );
    let privada = if peticion.frase.is_empty() {
        clave
            .to_openssh(LineEnding::LF)
            .map_err(|error| error.to_string())?
    } else {
        let cifrada = clave
            .encrypt(&mut SysRng, peticion.frase.as_bytes())
            .map_err(|error| error.to_string())?;
        cifrada
            .to_openssh(LineEnding::LF)
            .map_err(|error| error.to_string())?
    };
    crate::ficheros::escribir_atomico(&ruta_privada, privada.as_bytes(), 0o600)
        .map_err(|error| format!("no se pudo escribir {}: {error}", ruta_privada.display()))?;
    crate::ficheros::escribir_atomico(&ruta_publica, publica.as_bytes(), 0o644)
        .map_err(|error| format!("no se pudo escribir {}: {error}", ruta_publica.display()))?;
    Ok((
        clave,
        DatosEnDisco {
            tipo,
            huella,
            ruta: ruta_privada,
            publica,
            comentario: (!comentario.is_empty()).then(|| comentario.to_string()),
        },
    ))
}

/// Clave leída de un fichero para registrarla en MAGI.
pub struct ClaveImportada {
    pub tipo: String,
    pub huella: String,
    pub ruta: PathBuf,
    pub publica: Option<String>,
    pub comentario: Option<String>,
}

pub enum LecturaClave {
    Ok(ClaveImportada),
    NecesitaFrase,
    Error(String),
}

/// Lee una clave de fichero: primero `<ruta>.pub` y, si no existe, la privada
/// (pidiendo frase si está cifrada). La clave descifrada se descarta.
pub fn leer_clave(ruta: &Path, frase: Option<&str>) -> LecturaClave {
    let publica = ruta_publica(ruta);
    if publica.exists() {
        if let Ok(texto) = std::fs::read_to_string(&publica) {
            if let Ok(clave) = PublicKey::from_openssh(&texto) {
                let comentario = clave.comment().to_string();
                let ruta_guardada = if ruta.exists() {
                    ruta.to_path_buf()
                } else {
                    publica.clone()
                };
                return LecturaClave::Ok(ClaveImportada {
                    tipo: tipo(&clave),
                    huella: huella(&clave),
                    ruta: ruta_guardada,
                    publica: Some(texto.trim().to_string()),
                    comentario: (!comentario.is_empty()).then_some(comentario),
                });
            }
        }
    }
    if !ruta.exists() {
        return LecturaClave::Error(format!("no existe {}", ruta.display()));
    }
    let datos = match std::fs::read_to_string(ruta) {
        Ok(datos) => Zeroizing::new(datos),
        Err(error) => {
            return LecturaClave::Error(format!("no se pudo leer {}: {error}", ruta.display()))
        }
    };
    let cifrada = PrivateKey::from_openssh(&datos).is_ok_and(|clave| clave.is_encrypted());
    if cifrada && frase.is_none() {
        return LecturaClave::NecesitaFrase;
    }
    match russh::keys::decode_secret_key(&datos, frase) {
        Ok(clave) => {
            let comentario = clave.comment().to_string();
            let publica = clave
                .public_key()
                .to_openssh()
                .ok()
                .map(|texto| texto.trim().to_string());
            LecturaClave::Ok(ClaveImportada {
                tipo: tipo(clave.public_key()),
                huella: huella(clave.public_key()),
                ruta: ruta.to_path_buf(),
                publica,
                comentario: (!comentario.is_empty()).then_some(comentario),
            })
        }
        Err(error) => LecturaClave::Error(if cifrada {
            format!("frase incorrecta para {}", ruta.display())
        } else {
            format!("no se pudo leer la clave {}: {error}", ruta.display())
        }),
    }
}

/// Ruta del fichero público asociado a una clave.
pub fn ruta_publica(ruta: &Path) -> PathBuf {
    if ruta.extension().and_then(|extension| extension.to_str()) == Some("pub") {
        ruta.to_path_buf()
    } else {
        PathBuf::from(format!("{}.pub", ruta.display()))
    }
}

/// Texto de la clave pública de un fichero, si se puede leer.
pub fn publica_de(ruta: &Path) -> Option<String> {
    let publica = ruta_publica(ruta);
    std::fs::read_to_string(publica)
        .ok()
        .map(|texto| texto.trim().to_string())
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

#[cfg(test)]
mod pruebas {
    use super::*;

    fn peticion(dir: &Path, nombre: &str, frase: &str) -> PeticionGeneracion {
        PeticionGeneracion {
            dir_ssh: dir.to_path_buf(),
            nombre: nombre.to_string(),
            tipo: TipoClave::Ed25519,
            comentario: "prueba@magi".to_string(),
            frase: Zeroizing::new(frase.to_string()),
            anadir_agente: false,
            copiar_publica: false,
        }
    }

    #[test]
    fn normaliza_los_tipos_de_clave() {
        assert_eq!(normalizar_tipo("ssh-ed25519"), "ed25519");
        assert_eq!(normalizar_tipo("ssh-rsa"), "rsa");
        assert_eq!(normalizar_tipo("ecdsa-sha2-nistp256"), "ecdsa");
        assert_eq!(normalizar_tipo("sk-ssh-ed25519@openssh.com"), "ed25519-sk");
        assert_eq!(
            normalizar_tipo("sk-ecdsa-sha2-nistp256@openssh.com"),
            "ecdsa-sk"
        );
    }

    #[test]
    fn generar_no_sobrescribe_y_la_importacion_recupera_la_huella() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let clave = runtime
            .block_on(generar(peticion(dir.path(), "id_prueba", "")))
            .unwrap();
        assert!(clave.ruta.exists());
        assert!(ruta_publica(&clave.ruta).exists());
        assert_eq!(clave.tipo, "ed25519");
        assert!(!clave.en_agente);

        let error = runtime
            .block_on(generar(peticion(dir.path(), "id_prueba", "")))
            .unwrap_err();
        assert!(error.contains("ya existe"), "{error}");

        match leer_clave(&clave.ruta, None) {
            LecturaClave::Ok(importada) => {
                assert_eq!(importada.huella, clave.huella);
                assert_eq!(importada.tipo, "ed25519");
                assert_eq!(importada.comentario.as_deref(), Some("prueba@magi"));
                assert!(importada.publica.is_some());
            }
            _ => panic!("la clave recién generada debería leerse sin frase"),
        }
    }

    #[test]
    fn una_clave_cifrada_pide_frase_y_la_valida() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let clave = runtime
            .block_on(generar(peticion(dir.path(), "id_cifrada", "secreta")))
            .unwrap();
        std::fs::remove_file(ruta_publica(&clave.ruta)).unwrap();
        assert!(matches!(
            leer_clave(&clave.ruta, None),
            LecturaClave::NecesitaFrase
        ));
        match leer_clave(&clave.ruta, Some("mala")) {
            LecturaClave::Error(motivo) => assert!(motivo.contains("frase incorrecta")),
            _ => panic!("una frase incorrecta debe dar error"),
        }
        match leer_clave(&clave.ruta, Some("secreta")) {
            LecturaClave::Ok(importada) => assert_eq!(importada.huella, clave.huella),
            _ => panic!("la frase correcta debe abrir la clave"),
        }
    }

    #[test]
    fn las_claves_de_ficha_se_sincronizan_como_fichero() {
        let identidades = Identidades {
            ficheros: vec![ClaveFichero {
                ruta: PathBuf::from("/home/x/.ssh/id_ed25519"),
                tipo: "ed25519".to_string(),
                huella: "SHA256:uno".to_string(),
                comentario: "x@magi".to_string(),
            }],
            agente: None,
            aviso_agente: None,
        };
        let claves = claves_sincronizables(&identidades);
        assert_eq!(claves.len(), 1);
        assert_eq!(claves[0].origen, OrigenIdentidad::Fichero);
        assert_eq!(claves[0].ruta.as_deref(), Some("/home/x/.ssh/id_ed25519"));
    }
}
