//! Lectura de lo que devuelve `servicios.sh`: líneas `MAGI_*`, bloques de
//! `systemctl show` (clave=valor) y JSON de Docker, Podman y `journalctl`.
//!
//! Todo es entrada no fiable: los límites de `limites.rs` se aplican aquí y un
//! dato que no cuadra se descarta; nunca hay pánico.
//!
//! Gramática de la salida del script (la misma que implementa
//! `crates/magi-nucleo/scripts/servicios.sh`).

use std::collections::BTreeMap;

use chrono::DateTime;
use serde_json::Value;

use super::{
    limites, Consumo, Contenedor, Detalle, DetalleContenedor, EstadoContenedor, FallosContenedores,
    FallosSondeo, FallosUnidades, FaseAccion, LineaLog, Listado, Motor, MotorEstado, MotorNombre,
    Objetivo, PermisoDiario, Privilegio, Proyecto, RelojHost, ResultadoAccion, ResultadoServicio,
    Salud, Systemd, Timer, Unidad,
};

/// Error de lectura: la salida no tiene el formato esperado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorLectura(pub String);

impl std::fmt::Display for ErrorLectura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ErrorLectura {}

/// Valor de una línea `MAGI_CLAVE=valor`.
fn campo<'a>(linea: &'a str, prefijo: &str) -> Option<&'a str> {
    linea.strip_prefix(prefijo)
}

/// Privilegio y uid del host a partir de la salida de `privilegio`
/// (`MAGI_UID=`, `MAGI_PRIV=`).
pub fn privilegio(salida: &str) -> Result<(u32, Privilegio), ErrorLectura> {
    let mut uid = None;
    let mut privilegio = None;
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if let Some(valor) = campo(linea, "MAGI_UID=") {
            uid = valor.trim().parse::<u32>().ok();
        } else if let Some(valor) = campo(linea, "MAGI_PRIV=") {
            privilegio = Privilegio::desde_texto(valor.trim());
        }
    }
    let uid = uid.ok_or_else(|| ErrorLectura("falta MAGI_UID o no es un número".to_string()))?;
    let privilegio =
        privilegio.ok_or_else(|| ErrorLectura("falta MAGI_PRIV o no es válido".to_string()))?;
    Ok((uid, privilegio))
}

/// Resultado de la suborden `sudo`: `ok`, `rechazada` o `sin_sudo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComprobacionSudo {
    Ok,
    Rechazada,
    SinSudo,
}

pub fn sudo(salida: &str) -> Result<ComprobacionSudo, ErrorLectura> {
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if let Some(valor) = campo(linea, "MAGI_SUDO=") {
            return match valor.trim() {
                "ok" => Ok(ComprobacionSudo::Ok),
                "rechazada" => Ok(ComprobacionSudo::Rechazada),
                "sin_sudo" => Ok(ComprobacionSudo::SinSudo),
                otro => Err(ErrorLectura(format!("MAGI_SUDO no válido: «{otro}»"))),
            };
        }
    }
    Err(ErrorLectura("falta MAGI_SUDO".to_string()))
}

/// Versión de systemd tal como la da `systemctl --version` en la primera
/// línea (`systemd 252 (252.31-1~deb12u1)`).
fn sistema_desde_texto(texto: &str) -> Systemd {
    let texto = texto.trim();
    if texto.is_empty() || texto == "no" {
        return Systemd::Ausente;
    }
    let version: String = texto
        .chars()
        .take_while(|caracter| caracter.is_ascii_digit() || *caracter == '.')
        .collect();
    let mayor = version
        .split('.')
        .next()
        .and_then(|parte| parte.parse::<u32>().ok())
        .unwrap_or(0);
    if mayor == 0 {
        return Systemd::Ausente;
    }
    if mayor < 230 {
        Systemd::Antiguo(version)
    } else {
        Systemd::Disponible(version)
    }
}

/// Reloj del host: `MAGI_RELOJ=<epoch> <uptime>`.
fn reloj_desde_texto(texto: &str) -> RelojHost {
    let mut partes = texto.split_whitespace();
    let epoch = partes
        .next()
        .and_then(|valor| valor.parse::<i64>().ok())
        .filter(|epoch| *epoch >= 0)
        .unwrap_or(0);
    let uptime_seg = partes
        .next()
        .and_then(|valor| valor.parse::<u64>().ok())
        .unwrap_or(0);
    RelojHost { epoch, uptime_seg }
}

/// `MAGI_MOTOR=<motor> <versión>`, `MAGI_MOTOR=<motor> sin_permiso` o
/// `MAGI_MOTOR=no`.
fn motor_desde_texto(texto: &str) -> MotorEstado {
    let texto = texto.trim();
    if texto == "no" || texto.is_empty() {
        return MotorEstado::No;
    }
    let mut partes = texto.split_whitespace();
    let Some(nombre) = partes.next().and_then(MotorNombre::desde_texto) else {
        return MotorEstado::No;
    };
    match partes.next() {
        Some("sin_permiso") | None => MotorEstado::SinPermiso,
        Some(version) => MotorEstado::Disponible(Motor {
            nombre,
            version: version.chars().take(40).collect(),
        }),
    }
}

/// Bloques de `systemctl show` de la suborden `listar`.
struct Bloques {
    servicios: Vec<Vec<String>>,
    timers: Vec<Vec<String>>,
    actual: Option<(bool, Vec<String>)>,
}

impl Bloques {
    fn nuevo() -> Self {
        Self {
            servicios: Vec::new(),
            timers: Vec::new(),
            actual: None,
        }
    }

    fn abrir(&mut self, timer: bool) {
        self.cerrar();
        self.actual = Some((timer, Vec::new()));
    }

    fn cerrar(&mut self) {
        if let Some((timer, lineas)) = self.actual.take() {
            if lineas.is_empty() {
                return;
            }
            if timer {
                self.timers.push(lineas);
            } else {
                self.servicios.push(lineas);
            }
        }
    }
}

/// Listado completo del host a partir de la salida de `listar`.
/// `vigiladas` son las unidades de `HOSTS.servicios`.
pub fn listado(salida: &str, vigiladas: &[String]) -> Result<Listado, ErrorLectura> {
    let recortada = salida.len() > limites::LISTADO_MAX_BYTES;
    let salida = recortar_utf8(salida, limites::LISTADO_MAX_BYTES);
    let mut listado = Listado::vacio(Privilegio::SinSudo);
    let mut incompleto = recortada;
    let mut bloques = Bloques::nuevo();
    let mut systemd: Option<Systemd> = None;
    let mut avisos: Vec<String> = Vec::new();
    let mut contenedores: Vec<Contenedor> = Vec::new();
    let mut contenedores_crudos = 0usize;
    let mut proyectos_script: Vec<Proyecto> = Vec::new();
    let mut proyectos_crudos = 0usize;

    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if let Some(resto) = linea.strip_prefix("MAGI_") {
            bloques.cerrar();
            if resto == "FIN" || resto == "FIN_BLOQUE" {
                continue;
            }
            if let Some(tipo) = resto.strip_prefix("BLOQUE ") {
                bloques.abrir(tipo.trim() == "timer");
                continue;
            }
            if let Some(valor) = resto.strip_prefix("SYSTEMD=") {
                systemd = Some(sistema_desde_texto(valor));
            } else if let Some(valor) = resto.strip_prefix("PRIV=") {
                if let Some(privilegio) = Privilegio::desde_texto(valor.trim()) {
                    listado.privilegio = privilegio;
                }
            } else if let Some(valor) = resto.strip_prefix("DIARIO=") {
                if let Some(diario) = PermisoDiario::desde_texto(valor.trim()) {
                    listado.diario = diario;
                }
            } else if let Some(valor) = resto.strip_prefix("RELOJ=") {
                listado.reloj = reloj_desde_texto(valor);
            } else if let Some(valor) = resto.strip_prefix("MOTOR=") {
                listado.motor = motor_desde_texto(valor);
            } else if let Some(valor) = resto.strip_prefix("CONTENEDOR ") {
                contenedores_crudos += 1;
                if let MotorEstado::Disponible(motor) = &listado.motor {
                    if let Some(contenedor) = contenedor_json(valor.trim(), motor.nombre) {
                        if contenedores.len() < limites::MAX_CONTENEDORES {
                            contenedores.push(contenedor);
                        } else {
                            incompleto = true;
                        }
                    }
                }
            } else if let Some(valor) = resto.strip_prefix("PROYECTO ") {
                proyectos_crudos += 1;
                if let Some(proyecto) = proyecto_json(valor.trim()) {
                    if proyectos_script.len() < limites::MAX_PROYECTOS {
                        proyectos_script.push(proyecto);
                    } else {
                        incompleto = true;
                    }
                }
            } else if let Some(valor) = resto.strip_prefix("AVISO ") {
                if avisos.len() < 20 {
                    avisos.push(recortar_chars(valor, 512));
                }
            }
            continue;
        }
        if let Some((_, lineas)) = bloques.actual.as_mut() {
            lineas.push(linea.to_string());
        }
    }
    bloques.cerrar();

    let systemd = systemd.unwrap_or(Systemd::Ausente);
    if systemd.utilizable() {
        if bloques.servicios.len() > limites::MAX_UNIDADES
            || bloques.timers.len() > limites::MAX_UNIDADES
        {
            incompleto = true;
        }
        listado.unidades = unidades(&bloques.servicios, vigiladas);
        listado.timers = timers(&bloques.timers, &listado.unidades);
    }
    // Un host sin systemd o con systemd anterior a 230 solo tiene
    // contenedores (§7.12): las unidades no se leen.
    if contenedores_crudos > limites::MAX_CONTENEDORES || proyectos_crudos > limites::MAX_PROYECTOS
    {
        incompleto = true;
    }
    listado.contenedores = contenedores;

    // Los proyectos salen del script; si no los mandó, se deducen de las
    // etiquetas de los contenedores.
    let mut proyectos_finales = proyectos_script;
    for proyecto in proyectos(&listado.contenedores) {
        match proyectos_finales
            .iter_mut()
            .find(|previo| previo.nombre == proyecto.nombre)
        {
            Some(previo) => {
                for fichero in proyecto.ficheros {
                    if !previo.ficheros.contains(&fichero) {
                        previo.ficheros.push(fichero);
                    }
                }
            }
            None => {
                if proyectos_finales.len() < limites::MAX_PROYECTOS {
                    proyectos_finales.push(proyecto);
                } else {
                    incompleto = true;
                }
            }
        }
    }
    proyectos_finales.truncate(limites::MAX_PROYECTOS);
    listado.proyectos = proyectos_finales;
    listado.systemd = systemd;
    listado.avisos = avisos;
    listado.incompleto = incompleto;
    Ok(listado)
}

/// ¿El nombre se puede enseñar en un listado?
fn nombre_listable(nombre: &str) -> bool {
    !nombre.is_empty()
        && nombre.len() <= limites::MAX_NOMBRE
        && !nombre.chars().any(|caracter| caracter.is_control())
}

/// ¿El nombre es una plantilla (`app@.service`)? Las plantillas no se listan.
fn nombre_plantilla(nombre: &str) -> bool {
    nombre
        .split('.')
        .next()
        .is_some_and(|antes| antes.ends_with('@'))
}

/// Bloque `clave=valor` de `systemctl show`.
pub fn propiedades_bloque(lineas: &[&str]) -> BTreeMap<String, String> {
    let mut propiedades = BTreeMap::new();
    for linea in lineas {
        let linea = linea.trim_end_matches('\r');
        let Some((clave, valor)) = linea.split_once('=') else {
            continue;
        };
        let clave = clave.trim();
        if clave.is_empty()
            || clave.len() > 128
            || !clave
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            continue;
        }
        let valor = recortar_utf8(valor, limites::PROPIEDAD_MAX_BYTES).to_string();
        propiedades.entry(clave.to_string()).or_insert(valor);
    }
    propiedades
}

/// Unidades a partir de los bloques de `systemctl show` de `listar`.
pub fn unidades(bloques: &[Vec<String>], vigiladas: &[String]) -> Vec<Unidad> {
    let mut unidades: Vec<Unidad> = Vec::new();
    let mut vistas: Vec<String> = Vec::new();
    for bloque in bloques {
        if unidades.len() >= limites::MAX_UNIDADES {
            break;
        }
        let referencias: Vec<&str> = bloque.iter().map(String::as_str).collect();
        let propiedades = propiedades_bloque(&referencias);
        let Some(nombre) = propiedades.get("Id") else {
            continue;
        };
        let nombre = nombre.trim().to_string();
        if !nombre_listable(&nombre) || nombre_plantilla(&nombre) || !nombre.ends_with(".service") {
            continue;
        }
        if vistas.contains(&nombre) {
            continue;
        }
        vistas.push(nombre.clone());
        let vigilada = vigiladas.contains(&nombre);
        unidades.push(Unidad {
            nombre,
            props: propiedades,
            vigilada,
        });
    }
    unidades
}

/// Timers a partir de los bloques de `systemctl show` de `listar`.
pub fn timers(bloques: &[Vec<String>], servicios: &[Unidad]) -> Vec<Timer> {
    let mut timers: Vec<Timer> = Vec::new();
    let mut vistos: Vec<String> = Vec::new();
    for bloque in bloques {
        if timers.len() >= limites::MAX_UNIDADES {
            break;
        }
        let referencias: Vec<&str> = bloque.iter().map(String::as_str).collect();
        let propiedades = propiedades_bloque(&referencias);
        let Some(nombre) = propiedades.get("Id") else {
            continue;
        };
        let nombre = nombre.trim().to_string();
        if !nombre_listable(&nombre) || nombre_plantilla(&nombre) || !nombre.ends_with(".timer") {
            continue;
        }
        if vistos.contains(&nombre) {
            continue;
        }
        vistos.push(nombre.clone());
        let servicio = propiedades
            .get("Unit")
            .map(|servicio| servicio.trim().to_string())
            .filter(|servicio| super::orden::nombre_unidad_valido(servicio))
            .or_else(|| {
                let derivado = format!("{}.service", nombre.trim_end_matches(".timer"));
                let existe = servicios.iter().any(|unidad| unidad.nombre == derivado);
                existe.then_some(derivado)
            });
        timers.push(Timer {
            nombre,
            props: propiedades,
            servicio,
        });
    }
    timers
}

/// `MAGI_PROYECTO {"nombre":…,"ficheros":[…]}`.
fn proyecto_json(linea: &str) -> Option<Proyecto> {
    let valor: Value = serde_json::from_str(linea).ok()?;
    let objeto = valor.as_object()?;
    let nombre = objeto.get("nombre")?.as_str()?.trim();
    if !super::orden::nombre_proyecto_valido(nombre) {
        return None;
    }
    let mut ficheros = Vec::new();
    if let Some(lista) = objeto.get("ficheros").and_then(Value::as_array) {
        for fichero in lista.iter().filter_map(Value::as_str) {
            if super::orden::ruta_compose_valida(fichero) && !ficheros.iter().any(|f| f == fichero)
            {
                ficheros.push(fichero.to_string());
            }
            if ficheros.len() >= 32 {
                break;
            }
        }
    }
    Some(Proyecto {
        nombre: nombre.to_string(),
        ficheros,
    })
}

/// Consumo de los contenedores a partir de la salida de `consumo`.
pub fn consumo(salida: &str) -> Result<Vec<Consumo>, ErrorLectura> {
    let salida = recortar_utf8(salida, limites::LISTADO_MAX_BYTES);
    let mut consumos = Vec::new();
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        let Some(payload) = linea.strip_prefix("MAGI_CONSUMO ") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(payload) {
            Ok(Value::Array(elementos)) => {
                for elemento in elementos {
                    if consumos.len() >= limites::MAX_CONTENEDORES {
                        break;
                    }
                    if let Some(consumo) = consumo_json(&elemento) {
                        consumos.push(consumo);
                    }
                }
            }
            Ok(objeto @ Value::Object(_)) => {
                if let Some(consumo) = consumo_json(&objeto) {
                    consumos.push(consumo);
                }
            }
            _ => {}
        }
        if consumos.len() >= limites::MAX_CONTENEDORES {
            break;
        }
    }
    Ok(consumos)
}

/// Un objeto de `docker stats` o de `podman stats`.
fn consumo_json(valor: &Value) -> Option<Consumo> {
    let objeto = valor.as_object()?;
    let id = valor_texto(
        objeto,
        &["ID", "Id", "id", "Container", "container", "ContainerID"],
    )?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let cpu_pct = valor_texto(objeto, &["CPUPerc", "cpu", "CPU"])
        .and_then(porcentaje)
        .filter(|valor| valor.is_finite());
    let (memoria_bytes, memoria_limite_bytes) =
        valor_texto(objeto, &["MemUsage", "mem_usage", "MemUsageBytes"])
            .map(uso_de_memoria)
            .unwrap_or((None, None));
    Some(Consumo {
        id: recortar_chars(id, limites::MAX_NOMBRE),
        cpu_pct,
        memoria_bytes,
        memoria_limite_bytes,
    })
}

/// `"1.25%"` → `1.25`.
fn porcentaje(texto: &str) -> Option<f64> {
    texto
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .ok()
}

/// `"180.5MiB / 1GiB"` → `(Some(180.5 MiB), Some(1 GiB))`.
fn uso_de_memoria(texto: &str) -> (Option<u64>, Option<u64>) {
    let mut partes = texto.split('/');
    let uso = partes.next().and_then(parsear_tamano);
    let limite = partes.next().and_then(parsear_tamano);
    (uso, limite)
}

/// `"12.5MiB"` → bytes (1024) o `"12.5MB"` → bytes (1000).
fn parsear_tamano(texto: &str) -> Option<u64> {
    let texto = texto.trim();
    let corte = texto
        .find(|caracter: char| !(caracter.is_ascii_digit() || caracter == '.'))
        .unwrap_or(texto.len());
    let numero = texto[..corte].trim().parse::<f64>().ok()?;
    if !numero.is_finite() || numero < 0.0 {
        return None;
    }
    let unidad = texto[corte..].trim().to_ascii_uppercase();
    let factor = match unidad.as_str() {
        "" | "B" => 1.0,
        "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1_000_000_000_000.0,
        "KIB" => 1024.0,
        "MIB" => 1024.0 * 1024.0,
        "GIB" => 1024.0 * 1024.0 * 1024.0,
        "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let bytes = numero * factor;
    if bytes > u64::MAX as f64 {
        return None;
    }
    Some(bytes.round() as u64)
}

/// Detalle de un objetivo a partir de la salida de `detalle`.
pub fn detalle(salida: &str, objetivo: &Objetivo) -> Result<Detalle, ErrorLectura> {
    let salida = recortar_utf8(salida, limites::LISTADO_MAX_BYTES);
    let mut detalle = Detalle {
        objetivo: objetivo.clone(),
        props: BTreeMap::new(),
        necesita: Vec::new(),
        necesitada: Vec::new(),
        texto: String::new(),
        contenedor: None,
    };
    let mut en_texto = false;
    let mut texto: Vec<&str> = Vec::new();
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if en_texto {
            texto.push(linea);
            continue;
        }
        if linea == "MAGI_TEXTO" {
            en_texto = true;
        } else if let Some(valor) = linea.strip_prefix("MAGI_PROP ") {
            if let Some((clave, valor)) = valor.split_once('=') {
                let clave = clave.trim();
                if !clave.is_empty()
                    && clave.len() <= 128
                    && detalle.props.len() < 600
                    && !detalle.props.contains_key(clave)
                {
                    detalle.props.insert(
                        clave.to_string(),
                        recortar_utf8(valor, limites::PROPIEDAD_MAX_BYTES).to_string(),
                    );
                }
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_DEP ") {
            let valor = valor.trim();
            let Some((tipo, nombre)) = valor.split_once(' ') else {
                continue;
            };
            let nombre = nombre.trim();
            if nombre.is_empty()
                || nombre.len() > limites::MAX_NOMBRE
                || nombre.chars().any(|caracter| caracter.is_control())
            {
                continue;
            }
            let lista = match tipo {
                "necesita" => &mut detalle.necesita,
                "necesitada" => &mut detalle.necesitada,
                _ => continue,
            };
            if lista.len() < limites::MAX_DEPENDENCIAS
                && !lista.iter().any(|previo| previo == nombre)
            {
                lista.push(nombre.to_string());
            }
        }
    }
    if texto.last() == Some(&"MAGI_FIN") {
        texto.pop();
    }
    let texto = texto.join("\n");
    detalle.texto = recortar_utf8(&texto, limites::TEXTO_MAX_BYTES).to_string();
    if objetivo.tipo == super::TipoObjetivo::Contenedor {
        detalle.contenedor = detalle_contenedor(&detalle.texto, objetivo).ok();
    }
    Ok(detalle)
}

/// Resultado de una acción a partir de la salida de `accion`.
pub fn accion(salida: &str, objetivo: &Objetivo) -> Result<ResultadoServicio, ErrorLectura> {
    let _ = objetivo;
    let salida = recortar_utf8(salida, limites::LISTADO_MAX_BYTES);
    let mut resultado = None;
    let mut mensajes: Vec<String> = Vec::new();
    let mut lineas: Vec<String> = Vec::new();
    let mut bytes_lineas = 0usize;
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if let Some(valor) = linea.strip_prefix("MAGI_RESULTADO=") {
            resultado = Some(parsear_resultado(valor)?);
        } else if let Some(valor) = linea.strip_prefix("MAGI_MENSAJE ") {
            if mensajes.len() < 8 {
                mensajes.push(recortar_chars(valor, 512));
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_LINEA ") {
            let recortada = recortar_utf8(valor, limites::LINEA_LOG_MAX_BYTES);
            if bytes_lineas + recortada.len() > limites::SALIDA_COMPROBACION_MAX_BYTES {
                break;
            }
            if lineas.len() < 100 {
                bytes_lineas += recortada.len();
                lineas.push(recortada.to_string());
            }
        }
    }
    let (resultado, estado, segundos) =
        resultado.ok_or_else(|| ErrorLectura("falta MAGI_RESULTADO".to_string()))?;
    Ok(ResultadoServicio {
        resultado,
        estado,
        segundos,
        mensaje: mensajes.join("\n"),
        lineas,
        comprobacion: None,
    })
}

/// `MAGI_RESULTADO=<resultado> <estado|-> <segundos>`.
fn parsear_resultado(valor: &str) -> Result<(ResultadoAccion, String, f32), ErrorLectura> {
    let mut partes = valor.split_whitespace();
    let resultado = partes
        .next()
        .and_then(ResultadoAccion::desde_texto)
        .ok_or_else(|| ErrorLectura(format!("MAGI_RESULTADO no válido: «{valor}»")))?;
    let estado = partes
        .next()
        .filter(|estado| *estado != "-")
        .unwrap_or("")
        .chars()
        .take(64)
        .collect::<String>();
    let segundos = partes
        .next()
        .and_then(|segundos| segundos.parse::<f32>().ok())
        .filter(|segundos| segundos.is_finite() && (0.0..=86_400.0).contains(segundos))
        .unwrap_or(0.0);
    Ok((resultado, estado, segundos))
}

/// Fase de una acción (`MAGI_FASE=…`), para que el servidor la difunda.
pub fn fase(linea: &str) -> Option<FaseAccion> {
    match campo(linea.trim_end_matches('\r'), "MAGI_FASE=")? {
        "comprobando" => Some(FaseAccion::Comprobando),
        "ejecutando" => Some(FaseAccion::Ejecutando),
        "esperando" => Some(FaseAccion::Esperando),
        _ => None,
    }
}

/// Resultado de una comprobación de configuración
/// (`MAGI_COMPROBACION=ok` o `MAGI_COMPROBACION=fallo <código>`).
pub fn estado_comprobacion(linea: &str) -> Option<Result<(), i32>> {
    let valor = campo(linea.trim_end_matches('\r'), "MAGI_COMPROBACION=")?;
    if valor.trim() == "ok" {
        return Some(Ok(()));
    }
    let codigo = valor
        .strip_prefix("fallo ")
        .and_then(|codigo| codigo.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    Some(Err(codigo))
}

/// Una línea del diario en JSON de `journalctl -o json` (`MESSAGE`,
/// `__REALTIME_TIMESTAMP`, `PRIORITY`).
pub fn linea_diario(linea: &str) -> Option<LineaLog> {
    let linea = linea.trim_end_matches('\r');
    let valor: Value = serde_json::from_str(linea).ok()?;
    let objeto = valor.as_object()?;
    let texto = match objeto.get("MESSAGE") {
        Some(Value::String(texto)) => texto.clone(),
        Some(Value::Array(bits)) => {
            let bytes: Vec<u8> = bits
                .iter()
                .filter_map(Value::as_u64)
                .filter(|byte| *byte <= 255)
                .map(|byte| byte as u8)
                .collect();
            if bytes.len() != bits.len() {
                return None;
            }
            String::from_utf8_lossy(&bytes).to_string()
        }
        _ => return None,
    };
    let prioridad = match objeto.get("PRIORITY") {
        Some(Value::String(prioridad)) => prioridad.parse::<u8>().ok(),
        Some(Value::Number(prioridad)) => prioridad.as_u64().map(|valor| valor as u8),
        _ => None,
    }
    .filter(|prioridad| *prioridad <= 7)
    .unwrap_or(6);
    let hora = objeto
        .get("__REALTIME_TIMESTAMP")
        .and_then(|valor| match valor {
            Value::String(texto) => texto.parse::<i64>().ok(),
            Value::Number(numero) => numero.as_i64(),
            _ => None,
        })
        .filter(|microsegundos| *microsegundos > 0)
        .map(|microsegundos| microsegundos / 1_000_000);
    Some(LineaLog {
        hora,
        prioridad,
        stderr: prioridad <= 3,
        texto: recortar_utf8(&texto, limites::LINEA_LOG_MAX_BYTES).to_string(),
    })
}

/// Una línea `MAGI_LOG <flujo> <hora> <texto>` de los logs de un contenedor.
pub fn linea_contenedor(linea: &str) -> Option<LineaLog> {
    let resto = linea.trim_end_matches('\r').strip_prefix("MAGI_LOG ")?;
    let mut partes = resto.splitn(3, ' ');
    let flujo = partes.next()?;
    let stderr = match flujo {
        "out" => false,
        "err" => true,
        _ => return None,
    };
    let hora_texto = partes.next()?.trim();
    let hora = if hora_texto.is_empty() || hora_texto == "-" {
        None
    } else {
        DateTime::parse_from_rfc3339(hora_texto)
            .ok()
            .map(|fecha| fecha.timestamp())
    };
    let texto = partes.next().unwrap_or("");
    Some(LineaLog {
        hora,
        prioridad: if stderr { 3 } else { 6 },
        stderr,
        texto: recortar_utf8(texto, limites::LINEA_LOG_MAX_BYTES).to_string(),
    })
}

/// Línea de log según el tipo de objetivo (contenedor o diario).
pub fn linea_de_log(linea: &str, contenedor: bool) -> Option<LineaLog> {
    if contenedor {
        linea_contenedor(linea)
    } else {
        linea_diario(linea)
    }
}

/// Texto de un campo de un objeto JSON, probando varias claves.
fn valor_texto<'a>(objeto: &'a serde_json::Map<String, Value>, claves: &[&str]) -> Option<&'a str> {
    for clave in claves {
        if let Some(Value::String(valor)) = objeto.get(*clave) {
            return Some(valor.as_str());
        }
    }
    None
}

/// Número de un campo de un objeto JSON, probando varias claves.
fn valor_numero(objeto: &serde_json::Map<String, Value>, claves: &[&str]) -> Option<i64> {
    for clave in claves {
        match objeto.get(*clave) {
            Some(Value::Number(numero)) => {
                if let Some(valor) = numero.as_i64() {
                    return Some(valor);
                }
            }
            Some(Value::String(texto)) => {
                if let Ok(valor) = texto.parse::<i64>() {
                    return Some(valor);
                }
            }
            _ => {}
        }
    }
    None
}

/// Etiquetas de Docker o Podman (`"a=b,c=d"`).
fn etiquetas_desde_texto(texto: &str) -> BTreeMap<String, String> {
    let mut lista: Vec<(String, String)> = Vec::new();
    for segmento in texto.split(',') {
        if segmento.is_empty() {
            continue;
        }
        let mut es_clave = false;
        if let Some((clave, valor)) = segmento.split_once('=') {
            es_clave = !clave.is_empty()
                && clave
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && clave
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'));
            if es_clave {
                lista.push((clave.to_string(), valor.to_string()));
            }
        }
        if !es_clave {
            if let Some(ultimo) = lista.last_mut() {
                ultimo.1.push(',');
                ultimo.1.push_str(segmento);
            }
        }
    }
    lista.into_iter().collect()
}

/// Un contenedor de `ps -a --format '{{json .}}'` (Docker o Podman).
pub fn contenedor_json(linea: &str, motor: MotorNombre) -> Option<Contenedor> {
    let linea = recortar_utf8(linea, limites::LISTADO_MAX_BYTES);
    let valor: Value = serde_json::from_str(linea.trim()).ok()?;
    let objeto = valor.as_object()?;
    if motor == MotorNombre::Podman && matches!(objeto.get("IsInfra"), Some(Value::Bool(true))) {
        return None;
    }
    let id = valor_texto(objeto, &["ID", "Id", "id"]).unwrap_or("");
    let nombre = valor_texto(objeto, &["Names", "Name", "names", "name"]).unwrap_or("");
    if id.is_empty() && nombre.is_empty() {
        return None;
    }
    let estado_texto = valor_texto(objeto, &["State", "state"]);
    let status = valor_texto(objeto, &["Status", "status"]).unwrap_or("");
    let estado = estado_texto
        .map(EstadoContenedor::desde_texto)
        .unwrap_or_else(|| estado_desde_status(status));
    let identificador = if id.is_empty() { nombre } else { id };
    let id = recortar_chars(identificador, limites::MAX_NOMBRE);
    let nombre = if nombre.is_empty() {
        id.clone()
    } else {
        recortar_chars(nombre, limites::MAX_NOMBRE)
    };
    let imagen = recortar_chars(
        valor_texto(objeto, &["Image", "ImageName", "image"]).unwrap_or(""),
        512,
    );
    let codigo_salida = valor_numero(objeto, &["Exited", "exited"])
        .map(|codigo| codigo as i32)
        .or_else(|| codigo_de_status(status));
    let salud = salud_de_status(status);
    let puertos = recortar_chars(valor_texto(objeto, &["Ports", "ports"]).unwrap_or(""), 1024);
    let etiquetas = match objeto.get("Labels").or_else(|| objeto.get("labels")) {
        Some(Value::String(texto)) => etiquetas_desde_texto(texto),
        Some(Value::Object(mapa)) => mapa
            .iter()
            .filter_map(|(clave, valor)| {
                valor
                    .as_str()
                    .map(|valor| (clave.clone(), valor.to_string()))
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    let proyecto = etiquetas
        .get("com.docker.compose.project")
        .filter(|nombre| super::orden::nombre_proyecto_valido(nombre))
        .cloned();
    let ficheros = etiquetas
        .get("com.docker.compose.project.config_files")
        .map(|ficheros| {
            ficheros
                .split(',')
                .filter(|fichero| super::orden::ruta_compose_valida(fichero))
                .take(32)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let servicio = etiquetas
        .get("com.docker.compose.service")
        .filter(|servicio| nombre_contenedor_valido_legible(servicio))
        .cloned();
    let orden = etiquetas
        .get("com.docker.compose.container-number")
        .filter(|orden| orden.len() <= 16 && orden.bytes().all(|byte| byte.is_ascii_digit()))
        .cloned();
    Some(Contenedor {
        id,
        nombre,
        imagen,
        estado,
        codigo_salida,
        salud,
        puertos,
        proyecto,
        ficheros,
        servicio,
        orden,
        etiquetas,
    })
}

/// Nombre de servicio Compose: lo que admite una etiqueta del motor.
fn nombre_contenedor_valido_legible(nombre: &str) -> bool {
    !nombre.is_empty()
        && nombre.len() <= 128
        && nombre
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && nombre
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn estado_desde_status(status: &str) -> EstadoContenedor {
    let status = status.trim();
    if status.starts_with("Up") {
        EstadoContenedor::EnMarcha
    } else if status.starts_with("Exited") {
        EstadoContenedor::Parado
    } else if status.starts_with("Created") {
        EstadoContenedor::Creado
    } else if status.starts_with("Restarting") {
        EstadoContenedor::Reiniciando
    } else if status.starts_with("Paused") {
        EstadoContenedor::Pausado
    } else if status.starts_with("Dead") {
        EstadoContenedor::Muerto
    } else {
        EstadoContenedor::Desconocido
    }
}

fn codigo_de_status(status: &str) -> Option<i32> {
    let abierto = status.find('(')?;
    let cerrado = status[abierto + 1..].find(')')?;
    status[abierto + 1..abierto + 1 + cerrado]
        .trim()
        .parse::<i32>()
        .ok()
}

fn salud_de_status(status: &str) -> Salud {
    let status = status.to_ascii_lowercase();
    if status.contains("unhealthy") {
        Salud::Insegura
    } else if status.contains("health: starting") || status.contains("(starting)") {
        Salud::Comprobando
    } else if status.contains("healthy") {
        Salud::Sana
    } else {
        Salud::Ninguna
    }
}

/// Proyectos Compose a partir de los contenedores ya leídos.
pub fn proyectos(contenedores: &[Contenedor]) -> Vec<Proyecto> {
    let mut proyectos: Vec<Proyecto> = Vec::new();
    for contenedor in contenedores {
        let Some(nombre) = &contenedor.proyecto else {
            continue;
        };
        if !super::orden::nombre_proyecto_valido(nombre) {
            continue;
        }
        match proyectos
            .iter_mut()
            .find(|proyecto| &proyecto.nombre == nombre)
        {
            Some(proyecto) => {
                for fichero in &contenedor.ficheros {
                    if !proyecto.ficheros.contains(fichero) {
                        proyecto.ficheros.push(fichero.clone());
                    }
                }
            }
            None => {
                if proyectos.len() >= limites::MAX_PROYECTOS {
                    break;
                }
                proyectos.push(Proyecto {
                    nombre: nombre.clone(),
                    ficheros: contenedor.ficheros.iter().take(32).cloned().collect(),
                });
            }
        }
    }
    proyectos
}

/// Detalle de un contenedor a partir del texto de `inspect`.
pub fn detalle_contenedor(
    inspect: &str,
    objetivo: &Objetivo,
) -> Result<DetalleContenedor, ErrorLectura> {
    let recortado = recortar_utf8(inspect, limites::TEXTO_MAX_BYTES);
    let valor: Value = serde_json::from_str(recortado)
        .map_err(|error| ErrorLectura(format!("inspect no es JSON: {error}")))?;
    let primero = match &valor {
        Value::Array(elementos) => elementos.first(),
        Value::Object(_) => Some(&valor),
        _ => None,
    }
    .ok_or_else(|| ErrorLectura("inspect vacío".to_string()))?;
    let objeto = primero
        .as_object()
        .ok_or_else(|| ErrorLectura("inspect no es un objeto".to_string()))?;
    let id = valor_texto(objeto, &["Id", "ID", "id"])
        .map(|id| recortar_chars(id, limites::MAX_NOMBRE))
        .unwrap_or_else(|| objetivo.nombre.clone());
    let nombre = valor_texto(objeto, &["Name", "name"])
        .map(|nombre| nombre.trim_start_matches('/').to_string())
        .filter(|nombre| !nombre.is_empty())
        .unwrap_or_else(|| objetivo.nombre.clone());
    let imagen = objeto
        .get("Config")
        .and_then(Value::as_object)
        .and_then(|config| valor_texto(config, &["Image", "image"]))
        .or_else(|| valor_texto(objeto, &["Image", "image"]))
        .map(|imagen| recortar_chars(imagen, 512))
        .unwrap_or_default();
    let creado = valor_texto(objeto, &["Created", "created"])
        .map(|creado| recortar_chars(creado, 128))
        .unwrap_or_default();
    let estado_obj = objeto.get("State").and_then(Value::as_object);
    let estado = estado_obj
        .and_then(|state| valor_texto(state, &["Status", "status", "State", "state"]))
        .map(EstadoContenedor::desde_texto)
        .unwrap_or(EstadoContenedor::Desconocido);
    let codigo_salida = estado_obj
        .and_then(|state| valor_numero(state, &["ExitCode", "exit_code", "Exitcode"]))
        .map(|codigo| codigo as i32);
    let reinicios = valor_numero(objeto, &["RestartCount", "restart_count"])
        .filter(|reinicios| *reinicios >= 0)
        .unwrap_or(0) as u64;
    let salud = estado_obj
        .and_then(|state| state.get("Health"))
        .and_then(Value::as_object)
        .and_then(|health| valor_texto(health, &["Status", "status"]))
        .map(Salud::desde_texto)
        .unwrap_or(Salud::Ninguna);
    let puertos = puertos_de_inspect(objeto);
    let montajes = montajes_de_inspect(objeto);
    let redes = redes_de_inspect(objeto);
    let etiquetas = objeto
        .get("Config")
        .and_then(Value::as_object)
        .and_then(|config| config.get("Labels"))
        .and_then(Value::as_object)
        .map(|mapa| {
            mapa.iter()
                .filter_map(|(clave, valor)| {
                    valor
                        .as_str()
                        .map(|valor| (clave.clone(), valor.to_string()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();
    let proyecto = etiquetas
        .get("com.docker.compose.project")
        .filter(|nombre| super::orden::nombre_proyecto_valido(nombre))
        .cloned();
    let orden = etiquetas
        .get("com.docker.compose.container-number")
        .cloned();
    Ok(DetalleContenedor {
        id,
        nombre,
        imagen,
        creado,
        estado,
        codigo_salida,
        reinicios,
        salud,
        puertos,
        montajes,
        redes,
        proyecto,
        orden,
        texto: recortado.to_string(),
    })
}

fn puertos_de_inspect(objeto: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut puertos = Vec::new();
    let Some(mapa) = objeto
        .get("NetworkSettings")
        .and_then(Value::as_object)
        .and_then(|red| red.get("Ports"))
        .and_then(Value::as_object)
    else {
        return puertos;
    };
    for (puerto, enlaces) in mapa {
        if puertos.len() >= 100 {
            break;
        }
        match enlaces {
            Value::Array(enlaces) if !enlaces.is_empty() => {
                for enlace in enlaces.iter().filter_map(Value::as_object) {
                    let host = valor_texto(enlace, &["HostIp", "host_ip"]).unwrap_or("");
                    let puerto_host = valor_texto(enlace, &["HostPort", "host_port"]).unwrap_or("");
                    let texto = if puerto_host.is_empty() {
                        puerto.clone()
                    } else {
                        format!("{puerto} → {host}:{puerto_host}")
                    };
                    puertos.push(recortar_chars(&texto, 128));
                }
            }
            _ => puertos.push(recortar_chars(puerto, 128)),
        }
    }
    puertos
}

fn montajes_de_inspect(objeto: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut montajes = Vec::new();
    let Some(lista) = objeto.get("Mounts").and_then(Value::as_array) else {
        return montajes;
    };
    for montaje in lista.iter().filter_map(Value::as_object).take(100) {
        let origen = valor_texto(montaje, &["Source", "source"]).unwrap_or("");
        let destino = valor_texto(montaje, &["Destination", "destination"]).unwrap_or("");
        let tipo = valor_texto(montaje, &["Type", "type"]).unwrap_or("");
        let texto = match (origen.is_empty(), destino.is_empty()) {
            (false, false) => format!("{origen} → {destino} ({tipo})"),
            (false, true) => origen.to_string(),
            _ => destino.to_string(),
        };
        montajes.push(recortar_chars(&texto, 512));
    }
    montajes
}

fn redes_de_inspect(objeto: &serde_json::Map<String, Value>) -> Vec<String> {
    let mapa = objeto
        .get("NetworkSettings")
        .and_then(Value::as_object)
        .and_then(|red| red.get("Networks"))
        .and_then(Value::as_object)
        .or_else(|| objeto.get("Networks").and_then(Value::as_object));
    let Some(mapa) = mapa else {
        return Vec::new();
    };
    mapa.keys()
        .take(32)
        .map(|red| recortar_chars(red, 128))
        .collect()
}

/// Fallos de una unidad o un contenedor para el sondeo ampliado (`7.8).
pub fn fallos(
    unidades: &[String],
    contenedores: Option<(MotorNombre, Vec<String>)>,
) -> FallosSondeo {
    FallosSondeo {
        unidades: FallosUnidades {
            total: unidades.len() as u32,
            nombres: unidades.iter().take(20).cloned().collect(),
        },
        contenedores: contenedores.map(|(motor, nombres)| FallosContenedores {
            motor: Some(motor),
            total: nombres.len() as u32,
            nombres: nombres.iter().take(20).cloned().collect(),
        }),
    }
}

/// Recorta un texto a `max_bytes` sin partir un carácter UTF-8.
pub fn recortar_utf8(texto: &str, max_bytes: usize) -> &str {
    if texto.len() <= max_bytes {
        return texto;
    }
    let mut corte = max_bytes;
    while corte > 0 && !texto.is_char_boundary(corte) {
        corte -= 1;
    }
    &texto[..corte]
}

/// Recorta un texto a `max_chars` caracteres.
fn recortar_chars(texto: &str, max_chars: usize) -> String {
    texto.chars().take(max_chars).collect()
}

/// Parte una salida en líneas sin el salto final, con tope de líneas.
pub fn lineas(salida: &str, max: usize) -> Vec<&str> {
    salida.lines().take(max).collect()
}
