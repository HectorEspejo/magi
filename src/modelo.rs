use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

/// Procedencia de un host en el inventario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    Manual,
    SshConfig,
}

impl Origen {
    pub fn como_texto(self) -> &'static str {
        match self {
            Origen::Manual => "manual",
            Origen::SshConfig => "ssh_config",
        }
    }

    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "ssh_config" => Origen::SshConfig,
            _ => Origen::Manual,
        }
    }
}

/// Resultado del último intento de conexión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UltimoEstado {
    Ok,
    Error,
}

impl UltimoEstado {
    pub fn como_texto(self) -> &'static str {
        match self {
            UltimoEstado::Ok => "ok",
            UltimoEstado::Error => "error",
        }
    }

    pub fn desde_texto(texto: Option<&str>) -> Option<Self> {
        match texto {
            Some("ok") => Some(UltimoEstado::Ok),
            Some("error") => Some(UltimoEstado::Error),
            _ => None,
        }
    }
}

/// Referencia a la identidad usada al conectar: nunca contiene la clave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentidadRef {
    Auto,
    Agente(String),
    Fichero(String),
}

impl IdentidadRef {
    pub fn desde_bd(texto: Option<&str>) -> Self {
        match texto {
            Some(valor) if valor.starts_with("agente:") => {
                IdentidadRef::Agente(valor["agente:".len()..].to_string())
            }
            Some(valor) if valor.starts_with("fichero:") => {
                IdentidadRef::Fichero(valor["fichero:".len()..].to_string())
            }
            _ => IdentidadRef::Auto,
        }
    }

    pub fn a_bd(&self) -> Option<String> {
        match self {
            IdentidadRef::Auto => None,
            IdentidadRef::Agente(huella) => Some(format!("agente:{huella}")),
            IdentidadRef::Fichero(ruta) => Some(format!("fichero:{ruta}")),
        }
    }

    pub fn es_auto(&self) -> bool {
        matches!(self, IdentidadRef::Auto)
    }
}

#[derive(Debug, Clone)]
pub struct Grupo {
    pub id: i64,
    pub nombre: String,
    pub orden: i64,
    pub plegado: bool,
    pub creado_en: String,
}

#[derive(Debug, Clone)]
pub struct Etiqueta {
    pub id: i64,
    pub nombre: String,
}

/// Host del inventario con los datos derivados que usan la vista y el filtro.
#[derive(Debug, Clone)]
pub struct Host {
    pub id: i64,
    pub nombre: String,
    pub grupo_id: Option<i64>,
    pub direccion: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    pub identidad_ref: IdentidadRef,
    pub salto_host_id: Option<i64>,
    pub multiplexar: bool,
    pub keepalive_seg: Option<u32>,
    pub opciones_extra: String,
    pub origen: Origen,
    pub ultimo_estado: Option<UltimoEstado>,
    pub ultima_conexion_en: Option<String>,
    pub creado_en: String,
    pub actualizado_en: String,
    pub etiquetas: Vec<String>,
    pub grupo_nombre: Option<String>,
    pub salto_nombre: Option<String>,
}

/// Datos editables de un host (ficha), sin id ni marcas de tiempo.
#[derive(Debug, Clone, PartialEq)]
pub struct DatosHost {
    pub nombre: String,
    pub grupo_id: Option<i64>,
    pub direccion: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    pub identidad_ref: IdentidadRef,
    pub salto_host_id: Option<i64>,
    pub multiplexar: bool,
    pub keepalive_seg: Option<u32>,
    pub opciones_extra: String,
    pub etiquetas: Vec<String>,
}

impl Default for DatosHost {
    fn default() -> Self {
        Self {
            nombre: String::new(),
            grupo_id: None,
            direccion: String::new(),
            puerto: 22,
            usuario: None,
            identidad_ref: IdentidadRef::Auto,
            salto_host_id: None,
            multiplexar: false,
            keepalive_seg: Some(30),
            opciones_extra: String::new(),
            etiquetas: Vec::new(),
        }
    }
}

/// Máquina de estados de la sesión SSH (en memoria, no persistida).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoSesion {
    Inactiva,
    Resolviendo,
    Conectando,
    VerificandoHuella,
    Autenticando,
    Abierta,
    EnSegundoPlano,
    Cerrada,
    Error,
    Cancelada,
}

impl EstadoSesion {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoSesion::Inactiva => "inactiva",
            EstadoSesion::Resolviendo => "resolviendo",
            EstadoSesion::Conectando => "conectando",
            EstadoSesion::VerificandoHuella => "verificando huella",
            EstadoSesion::Autenticando => "autenticando",
            EstadoSesion::Abierta => "abierta",
            EstadoSesion::EnSegundoPlano => "en segundo plano",
            EstadoSesion::Cerrada => "cerrada",
            EstadoSesion::Error => "error",
            EstadoSesion::Cancelada => "cancelada",
        }
    }

    /// Estados en los que hay una tarea de conexión en curso.
    pub fn en_vuelo(self) -> bool {
        matches!(
            self,
            EstadoSesion::Resolviendo
                | EstadoSesion::Conectando
                | EstadoSesion::VerificandoHuella
                | EstadoSesion::Autenticando
        )
    }

    pub fn viva(self) -> bool {
        matches!(self, EstadoSesion::Abierta | EstadoSesion::EnSegundoPlano)
    }
}

/// Normaliza para búsquedas: minúsculas y sin acentos.
pub fn normalizar_busqueda(texto: &str) -> String {
    texto
        .nfd()
        .filter(|caracter| !unicode_normalization::char::is_combining_mark(*caracter))
        .collect::<String>()
        .to_lowercase()
}

/// Filtro incremental: subcadena, sin mayúsculas ni acentos, tokens en AND.
pub fn coincide(host: &Host, consulta: &str) -> bool {
    let tokens: Vec<String> = normalizar_busqueda(consulta)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if tokens.is_empty() {
        return true;
    }
    let pajar = normalizar_busqueda(&format!(
        "{} {} {} {} {}",
        host.nombre,
        host.direccion,
        host.usuario.clone().unwrap_or_default(),
        host.grupo_nombre.clone().unwrap_or_default(),
        host.etiquetas.join(" ")
    ));
    tokens.iter().all(|token| pajar.contains(token.as_str()))
}

/// Valida el nombre de host: es el `Host` de ssh_config, sin espacios ni
/// comodines ni patrones múltiples.
pub fn validar_nombre(nombre: &str) -> Result<(), String> {
    let nombre = nombre.trim();
    if nombre.is_empty() {
        return Err("el nombre es obligatorio".to_string());
    }
    if !nombre
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("el nombre solo admite letras, dígitos, punto, guion y guion bajo".to_string());
    }
    Ok(())
}

pub fn validar_direccion(direccion: &str) -> Result<(), String> {
    let direccion = direccion.trim();
    if direccion.is_empty() {
        return Err("la dirección es obligatoria".to_string());
    }
    if direccion.chars().any(char::is_whitespace) {
        return Err("la dirección no admite espacios".to_string());
    }
    Ok(())
}

pub fn validar_usuario(usuario: Option<&str>) -> Result<(), String> {
    if let Some(usuario) = usuario {
        if usuario.chars().any(char::is_whitespace) {
            return Err("el usuario no admite espacios".to_string());
        }
    }
    Ok(())
}

pub fn validar_keepalive(segundos: Option<u32>) -> Result<(), String> {
    if let Some(segundos) = segundos {
        if !(5..=600).contains(&segundos) {
            return Err("el keepalive debe estar entre 5 y 600 segundos".to_string());
        }
    }
    Ok(())
}

/// Directivas que MAGI gestiona y que no pueden duplicarse en opciones extra.
const DIRECTIVAS_GESTIONADAS: [&str; 7] = [
    "hostname",
    "port",
    "user",
    "identityfile",
    "proxyjump",
    "controlmaster",
    "serveraliveinterval",
];

/// Valida las opciones extra: una directiva por línea y sin las que ya
/// gestiona MAGI.
pub fn validar_opciones_extra(texto: &str) -> Result<(), String> {
    for (indice, linea) in texto.lines().enumerate() {
        let linea = linea.trim();
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }
        let directiva = linea.split_whitespace().next().unwrap_or_default();
        if directiva.is_empty() {
            return Err(format!("línea {}: falta la directiva", indice + 1));
        }
        if DIRECTIVAS_GESTIONADAS.contains(&directiva.to_lowercase().as_str()) {
            return Err(format!(
                "línea {}: «{directiva}» ya la gestiona MAGI; usa los campos de la ficha",
                indice + 1
            ));
        }
    }
    Ok(())
}

/// Comprueba que la cadena de saltos no tiene ciclos y no pasa de tres
/// niveles. `host_id` se ignora si es `None` (host nuevo).
pub fn validar_salto(
    host_id: Option<i64>,
    salto_host_id: Option<i64>,
    hosts: &HashMap<i64, Host>,
) -> Result<(), String> {
    let Some(mut actual) = salto_host_id else {
        return Ok(());
    };
    if Some(actual) == host_id {
        return Err("un host no puede saltar a través de sí mismo".to_string());
    }
    let mut visitados = std::collections::HashSet::new();
    let mut niveles = 0u8;
    while let Some(id) = Some(actual) {
        if !visitados.insert(id) {
            return Err("ciclo de saltos".to_string());
        }
        niveles += 1;
        if niveles > 3 {
            return Err("la cadena de saltos supera los 3 niveles".to_string());
        }
        if Some(id) == host_id {
            return Err("ciclo de saltos".to_string());
        }
        match hosts.get(&id).and_then(|host| host.salto_host_id) {
            Some(siguiente) => actual = siguiente,
            None => break,
        }
    }
    Ok(())
}

/// Fecha local en ISO 8601.
pub fn fecha_ahora() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn host_de_prueba() -> Host {
        Host {
            id: 1,
            nombre: "Producción-Web".to_string(),
            grupo_id: None,
            direccion: "10.0.0.1".to_string(),
            puerto: 22,
            usuario: Some("Héctor".to_string()),
            identidad_ref: IdentidadRef::Auto,
            salto_host_id: None,
            multiplexar: false,
            keepalive_seg: Some(30),
            opciones_extra: String::new(),
            origen: Origen::Manual,
            ultimo_estado: None,
            ultima_conexion_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            etiquetas: vec!["web".to_string(), "crítico".to_string()],
            grupo_nombre: Some("4d3 · producción".to_string()),
            salto_nombre: None,
        }
    }

    #[test]
    fn el_filtro_ignora_mayusculas_y_acentos() {
        let host = host_de_prueba();
        assert!(coincide(&host, "produccion"));
        assert!(coincide(&host, "HECTOR"));
        assert!(coincide(&host, "critico"));
        assert!(coincide(&host, "PROD web"));
        assert!(!coincide(&host, "prod base"));
    }

    #[test]
    fn el_filtro_usa_tokens_en_and() {
        let host = host_de_prueba();
        assert!(coincide(&host, "produccion hector"));
        assert!(!coincide(&host, "produccion ausente"));
    }

    #[test]
    fn validacion_de_nombre_rechaza_comodines_y_espacios() {
        assert!(validar_nombre("hetzner-01").is_ok());
        assert!(validar_nombre("").is_err());
        assert!(validar_nombre("dos palabras").is_err());
        assert!(validar_nombre("host*").is_err());
        assert!(validar_nombre("host?").is_err());
    }

    #[test]
    fn validacion_de_opciones_extra_rechaza_las_gestionadas() {
        assert!(validar_opciones_extra("ForwardAgent yes").is_ok());
        assert!(validar_opciones_extra("HostName otro").is_err());
        assert!(validar_opciones_extra("User root").is_err());
        assert!(validar_opciones_extra("ControlPersist 10m").is_ok());
    }

    #[test]
    fn validacion_de_keepalive_entre_5_y_600() {
        assert!(validar_keepalive(None).is_ok());
        assert!(validar_keepalive(Some(5)).is_ok());
        assert!(validar_keepalive(Some(600)).is_ok());
        assert!(validar_keepalive(Some(4)).is_err());
        assert!(validar_keepalive(Some(601)).is_err());
    }

    #[test]
    fn los_ciclos_de_salto_se_rechazan() {
        let mut a = host_de_prueba();
        a.id = 1;
        let mut b = host_de_prueba();
        b.id = 2;
        b.salto_host_id = Some(1);
        a.salto_host_id = Some(2);
        let mut mapa = HashMap::new();
        mapa.insert(1, a.clone());
        mapa.insert(2, b.clone());
        assert_eq!(
            validar_salto(Some(2), Some(1), &mapa).unwrap_err(),
            "ciclo de saltos"
        );
        assert!(validar_salto(Some(1), Some(1), &mapa).is_err());
    }

    #[test]
    fn mas_de_tres_niveles_de_salto_se_rechazan() {
        let mut hosts: Vec<Host> = (1..=5)
            .map(|id| {
                let mut host = host_de_prueba();
                host.id = id;
                host.salto_host_id = if id > 1 { Some(id - 1) } else { None };
                host
            })
            .collect();
        let mapa: HashMap<i64, Host> = hosts.iter().cloned().map(|host| (host.id, host)).collect();
        assert!(validar_salto(None, Some(4), &mapa).is_err());
        assert!(validar_salto(None, Some(3), &mapa).is_ok());
        hosts.clear();
    }
}
