use std::collections::{BTreeMap, HashMap};

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

/// Referencia a la identidad usada al conectar: nunca contiene la clave ni la
/// contraseña. `Contrasena` marca que MAGI la pide al conectar y
/// `ContrasenaLlavero` que además puede recuperarla del llavero del sistema
/// (el secreto no está en `magi.db`, solo esta referencia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentidadRef {
    Auto,
    Contrasena,
    ContrasenaLlavero,
    Agente(String),
    Fichero(String),
}

impl IdentidadRef {
    pub fn desde_bd(texto: Option<&str>) -> Self {
        match texto {
            Some("contrasena:llavero") => IdentidadRef::ContrasenaLlavero,
            Some("contrasena") => IdentidadRef::Contrasena,
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
            IdentidadRef::Contrasena => Some("contrasena".to_string()),
            IdentidadRef::ContrasenaLlavero => Some("contrasena:llavero".to_string()),
            IdentidadRef::Agente(huella) => Some(format!("agente:{huella}")),
            IdentidadRef::Fichero(ruta) => Some(format!("fichero:{ruta}")),
        }
    }

    pub fn es_auto(&self) -> bool {
        matches!(self, IdentidadRef::Auto)
    }

    pub fn usa_llavero(&self) -> bool {
        matches!(self, IdentidadRef::ContrasenaLlavero)
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
    pub servicios: String,
    pub origen: Origen,
    pub ultimo_estado: Option<UltimoEstado>,
    pub ultima_conexion_en: Option<String>,
    pub creado_en: String,
    pub actualizado_en: String,
    pub etiquetas: Vec<String>,
    pub grupo_nombre: Option<String>,
    pub salto_nombre: Option<String>,
    /// Último directorio local usado en la vista Archivos (F4).
    pub sftp_dir_local: Option<String>,
    /// Último directorio remoto usado en la vista Archivos (F4); nulo es el
    /// directorio de inicio del usuario remoto.
    pub sftp_dir_remoto: Option<String>,
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
    pub servicios: String,
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
            servicios: String::new(),
            etiquetas: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------- túneles

/// Los tres reenvíos de ssh: `-L`, `-R` y `-D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoTunel {
    /// `-L`: MAGI escucha en local y el host abre el destino.
    Local,
    /// `-R`: el host escucha en `escucha` y MAGI abre el destino en local.
    Remoto,
    /// `-D`: proxy SOCKS5 que escucha en local.
    Dinamico,
}

impl TipoTunel {
    /// Valor que se guarda en `TUNELES.tipo` (sin `CHECK`, se valida aquí).
    pub fn como_texto(self) -> &'static str {
        match self {
            TipoTunel::Local => "local",
            TipoTunel::Remoto => "remoto",
            TipoTunel::Dinamico => "dinamico",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "local" => Some(TipoTunel::Local),
            "remoto" => Some(TipoTunel::Remoto),
            "dinamico" => Some(TipoTunel::Dinamico),
            _ => None,
        }
    }

    /// Etiqueta para la interfaz (con tilde).
    pub fn etiqueta(self) -> &'static str {
        match self {
            TipoTunel::Local => "local",
            TipoTunel::Remoto => "remoto",
            TipoTunel::Dinamico => "dinámico",
        }
    }

    /// Directiva de ssh_config equivalente.
    pub fn directiva(self) -> &'static str {
        match self {
            TipoTunel::Local => "LocalForward",
            TipoTunel::Remoto => "RemoteForward",
            TipoTunel::Dinamico => "DynamicForward",
        }
    }

    /// ¿Abre conexiones salientes hacia un destino fijo?
    pub fn lleva_destino(self) -> bool {
        !matches!(self, TipoTunel::Dinamico)
    }
}

/// Túnel del inventario, con el nombre del host ya resuelto.
#[derive(Debug, Clone)]
pub struct Tunel {
    pub id: i64,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub tipo: TipoTunel,
    /// `dirección:puerto` (IPv6 entre corchetes).
    pub escucha: String,
    /// `host:puerto`; nulo en dinámico.
    pub destino: Option<String>,
    pub automatico: bool,
    pub creado_en: String,
    pub actualizado_en: String,
}

/// Datos editables de un túnel (diálogo), sin id ni marcas de tiempo.
#[derive(Debug, Clone, PartialEq)]
pub struct DatosTunel {
    pub host_id: i64,
    pub nombre: String,
    pub tipo: TipoTunel,
    pub escucha: String,
    pub destino: Option<String>,
    pub automatico: bool,
}

/// Dirección y puerto de una escucha o un destino. El puerto 0 significa «el
/// que asigne el sistema» (como `ssh -L 0:`), útil sobre todo en remoto.
pub fn partir_direccion_puerto(texto: &str) -> Option<(String, u16)> {
    let texto = texto.trim();
    let (direccion, puerto) = if let Some(resto) = texto.strip_prefix('[') {
        // IPv6 entre corchetes: `[::1]:1080`.
        let (dentro, resto) = resto.split_once(']')?;
        (dentro, resto.strip_prefix(':')?)
    } else {
        texto.rsplit_once(':')?
    };
    let direccion = direccion.trim();
    if direccion.is_empty() || direccion.chars().any(char::is_whitespace) {
        return None;
    }
    let puerto: u16 = puerto.trim().parse().ok()?;
    Some((direccion.to_string(), puerto))
}

/// Junta dirección y puerto en la forma canónica `dirección:puerto`, con los
/// corchetes que necesita IPv6.
pub fn juntar_direccion_puerto(direccion: &str, puerto: u16) -> String {
    let direccion = direccion.trim();
    if direccion.contains(':') && !direccion.starts_with('[') {
        format!("[{direccion}]:{puerto}")
    } else {
        format!("{direccion}:{puerto}")
    }
}

/// Valida un túnel entero. Devuelve el primer error legible.
pub fn validar_tunel(datos: &DatosTunel) -> Result<(), String> {
    validar_nombre(&datos.nombre)?;
    if datos.host_id <= 0 {
        return Err("el túnel necesita un host".to_string());
    }
    let Some((direccion, _puerto)) = partir_direccion_puerto(&datos.escucha) else {
        return Err("la escucha debe ser «dirección:puerto»".to_string());
    };
    if direccion.is_empty() {
        return Err("la escucha necesita una dirección".to_string());
    }
    if datos.tipo.lleva_destino() {
        let Some(destino) = datos.destino.as_deref() else {
            return Err("el destino es obligatorio en los túneles local y remoto".to_string());
        };
        if partir_direccion_puerto(destino).is_none() {
            return Err("el destino debe ser «host:puerto»".to_string());
        }
    }
    Ok(())
}

/// Aviso en ámbar del diálogo de túnel, si la escucha lo merece. En un reenvío
/// remoto pueden coincidir los dos (puerto privilegiado y escucha abierta), así
/// que se juntan.
pub fn aviso_escucha(tipo: TipoTunel, escucha: &str) -> Option<String> {
    let (_, puerto) = partir_direccion_puerto(escucha)?;
    let expuesta = escucha_expuesta(escucha);
    let mut avisos: Vec<String> = Vec::new();
    if tipo == TipoTunel::Remoto {
        if puerto != 0 && puerto < 1024 {
            avisos.push(format!(
                "el puerto {puerto} es privilegiado: el host lo rechazará sin root"
            ));
        }
        if expuesta {
            avisos.push("el host solo lo expondrá si tiene GatewayPorts".to_string());
        }
    } else if expuesta {
        avisos.push("cualquier equipo de tu red podrá usar este túnel".to_string());
    }
    if avisos.is_empty() {
        return None;
    }
    Some(avisos.join(" · "))
}

/// ¿Acepta esta escucha cualquier origen? `ssh` escribe «cualquiera» de tres
/// formas (`0.0.0.0`, `::` y `*`), así que las tres cuentan como expuestas.
pub fn escucha_expuesta(escucha: &str) -> bool {
    match partir_direccion_puerto(escucha) {
        Some((direccion, _)) => direccion == "0.0.0.0" || direccion == "::" || direccion == "*",
        None => false,
    }
}

/// Resultado de un sondeo de flota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultadoSondeo {
    #[default]
    Ok,
    SinMetricas,
    Error,
}

impl ResultadoSondeo {
    pub fn como_texto(self) -> &'static str {
        match self {
            ResultadoSondeo::Ok => "ok",
            ResultadoSondeo::SinMetricas => "sin_metricas",
            ResultadoSondeo::Error => "error",
        }
    }

    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "sin_metricas" => ResultadoSondeo::SinMetricas,
            "error" => ResultadoSondeo::Error,
            _ => ResultadoSondeo::Ok,
        }
    }
}

/// Instantánea del estado de un host tras ejecutar `sondeo.sh`.
#[derive(Debug, Clone, Default)]
pub struct Sondeo {
    pub id: i64,
    pub host_id: i64,
    pub fecha: String,
    pub resultado: ResultadoSondeo,
    pub error: Option<String>,
    pub nucleos: Option<i64>,
    pub carga_1m: Option<f64>,
    pub carga_5m: Option<f64>,
    pub carga_15m: Option<f64>,
    pub mem_total_kb: Option<i64>,
    pub mem_disponible_kb: Option<i64>,
    pub disco_total_kb: Option<i64>,
    pub disco_usado_kb: Option<i64>,
    pub red_rx_bytes: Option<i64>,
    pub red_tx_bytes: Option<i64>,
    pub uptime_seg: Option<i64>,
    pub servicios: BTreeMap<String, String>,
    pub duracion_ms: i64,
}

impl Sondeo {
    pub fn vacio(host_id: i64) -> Self {
        Self {
            host_id,
            fecha: fecha_ahora(),
            ..Self::default()
        }
    }

    /// Porcentaje de memoria en uso (0-100).
    pub fn memoria_pct(&self) -> Option<f64> {
        let total = self.mem_total_kb? as f64;
        let disponible = self.mem_disponible_kb? as f64;
        if total <= 0.0 {
            return None;
        }
        Some(((total - disponible) / total * 100.0).clamp(0.0, 100.0))
    }

    /// Porcentaje de disco usado (0-100).
    pub fn disco_pct(&self) -> Option<f64> {
        let total = self.disco_total_kb? as f64;
        let usado = self.disco_usado_kb? as f64;
        if total <= 0.0 {
            return None;
        }
        Some((usado / total * 100.0).clamp(0.0, 100.0))
    }
}

/// Origen de una identidad registrada en MAGI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigenIdentidad {
    Agente,
    Fichero,
    Token,
}

impl OrigenIdentidad {
    pub fn como_texto(self) -> &'static str {
        match self {
            OrigenIdentidad::Agente => "agente",
            OrigenIdentidad::Fichero => "fichero",
            OrigenIdentidad::Token => "token",
        }
    }

    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "fichero" => OrigenIdentidad::Fichero,
            "token" => OrigenIdentidad::Token,
            _ => OrigenIdentidad::Agente,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Identidad {
    pub id: i64,
    pub alias: String,
    pub tipo: String,
    pub huella: String,
    pub origen: OrigenIdentidad,
    pub ruta: Option<String>,
    pub comentario: Option<String>,
    pub anadida_en: String,
    pub ultimo_uso_en: Option<String>,
    pub revocada_en: Option<String>,
}

impl Identidad {
    pub fn revocada(&self) -> bool {
        self.revocada_en.is_some()
    }
}

/// Resultado de una entrada del registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoRegistro {
    Ok,
    Error,
}

impl ResultadoRegistro {
    pub fn como_texto(self) -> &'static str {
        match self {
            ResultadoRegistro::Ok => "ok",
            ResultadoRegistro::Error => "error",
        }
    }

    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "error" => ResultadoRegistro::Error,
            _ => ResultadoRegistro::Ok,
        }
    }
}

/// Entrada del historial de MAGI, con los nombres resueltos para la vista.
#[derive(Debug, Clone)]
pub struct EntradaRegistro {
    pub id: i64,
    pub fecha: String,
    pub tipo: String,
    pub host_id: Option<i64>,
    pub host_nombre: Option<String>,
    pub identidad_id: Option<i64>,
    pub identidad_alias: Option<String>,
    pub detalle: String,
    pub resultado: ResultadoRegistro,
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

/// Una unidad systemd válida: `[A-Za-z0-9@._-]+`, sin `.service` final.
pub fn unidad_valida(unidad: &str) -> bool {
    !unidad.is_empty()
        && unidad
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-'))
}

/// Normaliza la lista de servicios: recorta espacios, descarta líneas vacías,
/// quita el sufijo `.service`, elimina duplicados y valida cada unidad.
pub fn normalizar_servicios(texto: &str) -> Result<String, String> {
    let mut unidades: Vec<String> = Vec::new();
    for (indice, linea) in texto.lines().enumerate() {
        let unidad = linea.trim();
        if unidad.is_empty() {
            continue;
        }
        let unidad = unidad.strip_suffix(".service").unwrap_or(unidad);
        if !unidad_valida(unidad) {
            return Err(format!(
                "línea {}: «{unidad}» no es una unidad systemd válida",
                indice + 1
            ));
        }
        if !unidades.iter().any(|existente| existente == unidad) {
            unidades.push(unidad.to_string());
        }
    }
    Ok(unidades.join("\n"))
}

/// Lista de unidades de un host, ya normalizada.
pub fn servicios_de(host: &Host) -> Vec<String> {
    host.servicios
        .lines()
        .map(str::trim)
        .filter(|linea| !linea.is_empty())
        .map(str::to_string)
        .collect()
}

/// Directivas que MAGI gestiona y que no pueden duplicarse en opciones extra.
///
/// Los reenvíos (`LocalForward`, `RemoteForward`, `DynamicForward`) no están
/// aquí: desde la Fase 5 viven en `TUNELES`, pero solo se rechazan los que MAGI
/// sabe representar —de eso se encarga `sshconfig::tuneles::reenvio_gestionado`
/// en la validación de la ficha—, porque `ssh` admite formas que MAGI no
/// representa (varios destinos en una directiva, reenvíos a un socket local) y
/// esas siguen siendo opciones extra legítimas.
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

/// Fecha local del día (AAAA-MM-DD), como la del nombre de los logs.
pub fn fecha_hoy() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Época en segundos. Es la unidad de `mtime` del panel y de la cola de
/// transferencias, que se comparan con tolerancia.
pub fn fecha_ahora_epoca() -> i64 {
    chrono::Local::now().timestamp()
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
            servicios: String::new(),
            origen: Origen::Manual,
            ultimo_estado: None,
            ultima_conexion_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            etiquetas: vec!["web".to_string(), "crítico".to_string()],
            grupo_nombre: Some("4d3 · producción".to_string()),
            salto_nombre: None,
            sftp_dir_local: None,
            sftp_dir_remoto: None,
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
    fn la_referencia_de_contrasena_va_y_vuelve_de_la_bd() {
        assert_eq!(
            IdentidadRef::Contrasena.a_bd().as_deref(),
            Some("contrasena")
        );
        assert_eq!(
            IdentidadRef::desde_bd(Some("contrasena")),
            IdentidadRef::Contrasena
        );
        assert_eq!(
            IdentidadRef::ContrasenaLlavero.a_bd().as_deref(),
            Some("contrasena:llavero")
        );
        assert_eq!(
            IdentidadRef::desde_bd(Some("contrasena:llavero")),
            IdentidadRef::ContrasenaLlavero
        );
        assert!(IdentidadRef::ContrasenaLlavero.usa_llavero());
        assert!(!IdentidadRef::Contrasena.es_auto());
    }

    #[test]
    fn los_servicios_se_normalizan_y_validan() {
        let texto = normalizar_servicios(" nginx.service \n\npostgresql\nnginx\n").unwrap();
        assert_eq!(texto, "nginx\npostgresql");
        assert!(normalizar_servicios("nginx; rm -rf /").is_err());
        assert!(normalizar_servicios("unidad con espacio").is_err());
        assert!(normalizar_servicios("").unwrap().is_empty());
        assert!(unidad_valida("magi@.service"));
    }

    #[test]
    fn validacion_de_opciones_extra_rechaza_las_gestionadas() {
        assert!(validar_opciones_extra("ForwardAgent yes").is_ok());
        assert!(validar_opciones_extra("HostName otro").is_err());
        assert!(validar_opciones_extra("User root").is_err());
        assert!(validar_opciones_extra("ControlPersist 10m").is_ok());
        // Los reenvíos ya no se rechazan aquí: solo los que MAGI sabe
        // representar, y de eso decide `sshconfig::tuneles::reenvio_gestionado`.
        assert!(validar_opciones_extra("LocalForward 5432 10.0.0.5:5432").is_ok());
    }

    #[test]
    fn los_avisos_de_escucha_cubren_las_formas_de_ssh() {
        // Las tres formas de «cualquiera» que admite `ssh` avisan.
        assert!(aviso_escucha(TipoTunel::Local, "0.0.0.0:8080")
            .unwrap_or_default()
            .contains("cualquier equipo"));
        assert!(aviso_escucha(TipoTunel::Local, "[::]:8080")
            .unwrap_or_default()
            .contains("cualquier equipo"));
        assert!(aviso_escucha(TipoTunel::Local, "*:8080")
            .unwrap_or_default()
            .contains("cualquier equipo"));
        assert!(aviso_escucha(TipoTunel::Local, "127.0.0.1:8080").is_none());

        // En remoto, el puerto privilegiado y la escucha abierta se dicen los dos.
        let remoto = aviso_escucha(TipoTunel::Remoto, "0.0.0.0:80").unwrap_or_default();
        assert!(remoto.contains("privilegiado"), "{remoto}");
        assert!(remoto.contains("GatewayPorts"), "{remoto}");
        assert!(aviso_escucha(TipoTunel::Remoto, "127.0.0.1:9000").is_none());
        // El puerto 0 (el que elija el host) no es privilegiado.
        assert!(aviso_escucha(TipoTunel::Remoto, "127.0.0.1:0").is_none());
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
