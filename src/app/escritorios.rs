//! Escritorios remotos (Fase 12) en la TUI: apertura, enlaces del visor,
//! navegador, diálogos que pide el servidor y acciones de la ficha.
//!
//! El servidor custodia las sesiones y no dialoga nunca (T25): aquí solo se
//! piden, se pintan y se responden sus preguntas. El enlace del visor lo abre
//! este cliente en el navegador, sin shell (T30) y solo en 127.0.0.1.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::SeccionEscritorio;
use crate::escritorio::{
    CredencialesEscritorio, DatosCertificado, EstadoEscritorio, InfoEscritorio,
    PeticionCredenciales, TipoCambioCertificado, TipoHost,
};
use crate::modelo::aviso_vnc_directo;
use crate::modelo::{DatosHost, Host, IdentidadRef, ResultadoRegistro};
use crate::protocolo::{self, MensajeCliente};
use crate::ui::componentes::CampoTexto;
use crate::ui::Vista;

use super::{App, Dialogo};

// ---------------------------------------------------------------- navegador

/// Navegadores Chromium que se buscan en el PATH cuando [escritorio] navador
/// vale auto.
const CHROMIUM: [&str; 6] = [
    "chromium",
    "brave",
    "brave-browser",
    "google-chrome-stable",
    "google-chrome",
    "microsoft-edge-stable",
];

/// Aplicaciones que se buscan en /Applications en macOS.
const APPS_MAC: [&str; 3] = ["Google Chrome", "Brave Browser", "Chromium"];

/// Ruta de las aplicaciones de macOS.
const APLICACIONES_MAC: &str = "/Applications";

/// Cuánto vale un enlace de un solo uso (el servidor también lo hace caducar).
pub const DURACION_ENLACE: Duration = Duration::from_secs(60);

/// Plataforma para elegir el navegador. Se inyecta en las pruebas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plataforma {
    Linux,
    Macos,
}

/// Plataforma de este binario.
pub fn plataforma() -> Plataforma {
    if cfg!(target_os = "macos") {
        Plataforma::Macos
    } else {
        Plataforma::Linux
    }
}

/// De dónde salió la orden del navegador (para el diagnóstico).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrigenNavegador {
    /// Orden propia de [escritorio] navegador.
    Orden,
    /// Un Chromium del PATH lanzado con --app.
    Chromium(String),
    /// Una aplicación de macOS abierta con open -na.
    AplicacionMac(String),
    /// El abridor del sistema (xdg-open u open).
    Sistema,
}

/// Navegador resuelto: argv completo y de dónde salió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navegador {
    pub args: Vec<String>,
    pub origen: OrigenNavegador,
}

/// Tokeniza una orden con comillas dobles, sin shell (T30). Dentro de las
/// comillas todo es literal; fuera, los espacios separan tokens.
pub fn tokenizar(orden: &str) -> Result<Vec<String>, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut actual = String::new();
    let mut en_comillas = false;
    let mut hay_token = false;
    for caracter in orden.chars() {
        match caracter {
            '"' => {
                en_comillas = !en_comillas;
                hay_token = true;
            }
            c if c.is_whitespace() && !en_comillas => {
                if hay_token {
                    tokens.push(std::mem::take(&mut actual));
                    hay_token = false;
                }
            }
            c => {
                actual.push(c);
                hay_token = true;
            }
        }
    }
    if en_comillas {
        return Err("faltan las comillas de cierre de [escritorio] navegador".to_string());
    }
    if hay_token {
        tokens.push(actual);
    }
    if tokens.is_empty() || tokens[0].is_empty() {
        return Err(
            "[escritorio] navegador está vacío: usa auto o una orden con {url}".to_string(),
        );
    }
    Ok(tokens)
}

/// Resuelve la orden del navegador para url según la configuración (T30:
/// nunca por un shell). existe dice si un programa está en el PATH y app si
/// una aplicación está instalada en macOS; ambas se inyectan para poder
/// elegir sin depender de la máquina que ejecuta las pruebas.
pub fn resolver_navegador(
    config: &SeccionEscritorio,
    plataforma: Plataforma,
    url: &str,
    existe: &dyn Fn(&str) -> bool,
    app: &dyn Fn(&str) -> bool,
) -> Result<Navegador, String> {
    let orden = config.navegador.trim();
    if orden != "auto" {
        let tokens = tokenizar(orden)?;
        if let Some(programa) = tokens.first() {
            let base = programa.rsplit('/').next().unwrap_or(programa);
            if matches!(base, "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh") {
                return Err(
                    "[escritorio] navegador no puede ser un shell: escribe la orden del navegador con {url}"
                        .to_string(),
                );
            }
        }
        let args = tokens
            .iter()
            .map(|token| token.replace("{url}", url))
            .collect();
        return Ok(Navegador {
            args,
            origen: OrigenNavegador::Orden,
        });
    }
    match plataforma {
        Plataforma::Macos => {
            for aplicacion in APPS_MAC {
                if app(aplicacion) {
                    return Ok(Navegador {
                        args: vec![
                            "open".to_string(),
                            "-na".to_string(),
                            aplicacion.to_string(),
                            "--args".to_string(),
                            format!("--app={url}"),
                        ],
                        origen: OrigenNavegador::AplicacionMac(aplicacion.to_string()),
                    });
                }
            }
            Ok(Navegador {
                args: vec!["open".to_string(), url.to_string()],
                origen: OrigenNavegador::Sistema,
            })
        }
        Plataforma::Linux => {
            for nombre in CHROMIUM {
                if existe(nombre) {
                    return Ok(Navegador {
                        args: vec![nombre.to_string(), format!("--app={url}")],
                        origen: OrigenNavegador::Chromium(nombre.to_string()),
                    });
                }
            }
            Ok(Navegador {
                args: vec!["xdg-open".to_string(), url.to_string()],
                origen: OrigenNavegador::Sistema,
            })
        }
    }
}

/// ¿Está el programa en el PATH y es ejecutable?
pub fn disponible_en_path(programa: &str) -> bool {
    if programa.contains('/') {
        return es_ejecutable(std::path::Path::new(programa));
    }
    let Some(camino) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&camino).any(|directorio| es_ejecutable(&directorio.join(programa)))
}

fn es_ejecutable(ruta: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(ruta)
            .map(|metadatos| metadatos.is_file() && metadatos.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        ruta.is_file()
    }
}

/// ¿Está instalada esa aplicación en /Applications?
pub fn app_instalada(nombre: &str) -> bool {
    std::path::Path::new(APLICACIONES_MAC)
        .join(format!("{nombre}.app"))
        .exists()
}

/// Nombres que el diagnóstico mira en el PATH para el visor.
pub fn candidatos_para_diagnostico() -> Vec<&'static str> {
    match plataforma() {
        Plataforma::Macos => vec!["open"],
        Plataforma::Linux => CHROMIUM
            .iter()
            .copied()
            .chain(std::iter::once("xdg-open"))
            .collect(),
    }
}

/// Lanza el navegador con el enlace del visor. Nunca hereda la terminal y no
/// espera al proceso (el navegador vive hasta que el usuario lo cierre); un
/// hilo se queda esperándolo para no dejar zombis.
pub fn abrir_navegador(config: &SeccionEscritorio, url: &str) -> Result<(), String> {
    let navegador = resolver_navegador(
        config,
        plataforma(),
        url,
        &disponible_en_path,
        &app_instalada,
    )?;
    let (programa, argumentos) = navegador
        .args
        .split_first()
        .ok_or_else(|| "[escritorio] navegador no tiene programa".to_string())?;
    let mut hijo = Command::new(programa)
        .args(argumentos)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("no se pudo lanzar «{programa}»: {error}"))?;
    std::thread::spawn(move || {
        let _ = hijo.wait();
    });
    Ok(())
}

// ---------------------------------------------------------------- peticiones

/// Una petición de escritorio en vuelo, por peticion_id (rango desde 2^46).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeticionEscritorio {
    /// Abrir una sesión nueva al host (el servidor contestará
    /// EscritorioAbierto).
    Abrir { host_id: i64 },
    /// Enlace nuevo para reabrir el visor (abrir) o copiarlo.
    Enlace { escritorio_id: u32, abrir: bool },
    /// Volver a abrir una sesión caída.
    Reconectar { escritorio_id: u32 },
}

impl PeticionEscritorio {
    /// Texto para la barra cuando el servidor contesta con Error.
    pub fn etiqueta(&self) -> &'static str {
        match self {
            PeticionEscritorio::Abrir { .. } => "abrir el escritorio",
            PeticionEscritorio::Enlace { .. } => "pedir un enlace del visor",
            PeticionEscritorio::Reconectar { .. } => "reconectar el escritorio",
        }
    }
}

// ---------------------------------------------------------------- diálogos

/// Diálogos propios de un escritorio: certificado RDP (nuevo o cambiado),
/// credenciales y la confirmación de un VNC directo sin cifrar.
pub enum DialogoEscritorio {
    /// Certificado nuevo o cambiado. En uno cambiado, sustituir exige escribir
    /// el nombre del host tal cual.
    Certificado {
        peticion_id: u64,
        escritorio_id: u32,
        host_id: i64,
        host_nombre: String,
        datos: DatosCertificado,
        campo: CampoTexto,
        error: Option<String>,
    },
    /// Credenciales que pide el servidor al cliente solicitante (T25). El
    /// intento va de 1 a 3.
    Credenciales {
        peticion_id: u64,
        escritorio_id: u32,
        host_nombre: String,
        peticion: PeticionCredenciales,
        usuario: CampoTexto,
        dominio: CampoTexto,
        contrasena: CampoTexto,
        recordar: bool,
        /// 0 usuario · 1 dominio · 2 contraseña · 3 casilla recordar.
        foco: u8,
        error: Option<String>,
    },
    /// VNC directo fuera de loopback: aviso al abrirlo (y fija directo).
    VncDirecto { host_id: i64, aviso: String },
}

impl DialogoEscritorio {
    /// Título del recuadro.
    pub fn titulo(&self, tema: &crate::tema::Tema) -> String {
        match self {
            DialogoEscritorio::Certificado { datos, .. } => match datos.tipo {
                TipoCambioCertificado::Nuevo => "CERTIFICADO NUEVO".to_string(),
                TipoCambioCertificado::Cambiado => "CERTIFICADO CAMBIADO".to_string(),
            },
            DialogoEscritorio::Credenciales { host_nombre, .. } => {
                format!("CREDENCIALES {} {host_nombre}", tema.glifos.punto_medio)
            }
            DialogoEscritorio::VncDirecto { .. } => "VNC DIRECTO SIN CIFRAR".to_string(),
        }
    }

    /// ¿Pide respuesta al servidor (y por tanto se apila sobre el diálogo que
    /// hubiera)? La confirmación de VNC directo es local.
    pub fn del_servidor(&self) -> bool {
        !matches!(self, DialogoEscritorio::VncDirecto { .. })
    }

    /// Sesión de escritorio sobre la que actúa, si es una pregunta del
    /// servidor (para retirarla si la sesión desaparece).
    pub fn escritorio_id(&self) -> Option<u32> {
        match self {
            DialogoEscritorio::Certificado { escritorio_id, .. }
            | DialogoEscritorio::Credenciales { escritorio_id, .. } => Some(*escritorio_id),
            DialogoEscritorio::VncDirecto { .. } => None,
        }
    }
}

// -------------------------------------------------------------- datos de host

/// Datos editables de un host a partir de la fila del inventario.
pub(crate) fn datos_de_host(host: &Host) -> DatosHost {
    DatosHost {
        nombre: host.nombre.clone(),
        grupo_id: host.grupo_id,
        direccion: host.direccion.clone(),
        puerto: host.puerto,
        tipo: host.tipo,
        escritorio_dominio: host.escritorio_dominio.clone(),
        escritorio_opciones: host.escritorio_opciones.clone(),
        usuario: host.usuario.clone(),
        identidad_ref: host.identidad_ref.clone(),
        salto_host_id: host.salto_host_id,
        multiplexar: host.multiplexar,
        keepalive_seg: host.keepalive_seg,
        opciones_extra: host.opciones_extra.clone(),
        servicios: host.servicios.clone(),
        etiquetas: host.etiquetas.clone(),
    }
}

/// Usuario con el que se guarda o recupera la contraseña de un escritorio en
/// el llavero. VNC y voxl no tienen usuario de Windows.
pub(crate) fn usuario_llavero(host: &Host) -> String {
    match host.tipo {
        TipoHost::Vnc | TipoHost::Voxl => host.usuario.clone().unwrap_or_default(),
        _ => host.usuario.clone().unwrap_or_else(super::usuario_local),
    }
}

/// Estado de la lista del servidor como el de una pestaña.
fn estado_de_pestana(estado: EstadoEscritorio) -> protocolo::EstadoSesionRemota {
    use protocolo::EstadoSesionRemota;
    match estado {
        EstadoEscritorio::Abriendo => EstadoSesionRemota::Abriendo,
        EstadoEscritorio::Abierto => EstadoSesionRemota::Abierta,
        EstadoEscritorio::Caido => EstadoSesionRemota::Caida,
        EstadoEscritorio::Cerrando => EstadoSesionRemota::Cerrada,
    }
}

/// Siguiente foco válido del diálogo de credenciales.
fn siguiente_foco(actual: u8, atras: bool, es_vnc: bool, con_casilla: bool) -> u8 {
    let valido = |foco: u8| {
        let campo = if es_vnc { foco == 2 } else { foco <= 2 };
        campo || (con_casilla && foco == 3)
    };
    let mut foco = actual.min(3);
    for _ in 0..4 {
        foco = if atras {
            (foco + 3) % 4
        } else {
            (foco + 1) % 4
        };
        if valido(foco) {
            return foco;
        }
    }
    actual.min(3)
}

// ------------------------------------------------------------------- la App

impl App {
    // ------------------------------------------------------------ peticiones

    /// Registra una petición de escritorio y devuelve su peticion_id (rango
    /// propio desde 2^46: no se reinicia al cambiar de host).
    pub(crate) fn nueva_peticion_escritorio(&mut self, peticion: PeticionEscritorio) -> u64 {
        self.siguiente_peticion_escritorio += 1;
        let id = self.siguiente_peticion_escritorio;
        self.peticiones_escritorio.insert(id, peticion);
        id
    }

    /// Un Error del servidor con peticion_id de escritorio: se queda con la
    /// petición, la descarta y avisa. true si era de escritorio.
    pub(crate) fn resolver_error_escritorio(&mut self, peticion_id: u64, mensaje: &str) -> bool {
        let Some(peticion) = self.peticiones_escritorio.remove(&peticion_id) else {
            return false;
        };
        if let PeticionEscritorio::Abrir { host_id } = peticion {
            self.aperturas_escritorio.remove(&host_id);
        }
        self.mensaje(format!("{}: {mensaje}", peticion.etiqueta()), true);
        true
    }

    /// Sesión de escritorio viva de un host, si la hay.
    pub(crate) fn escritorio_de_host(&self, host_id: i64) -> Option<&InfoEscritorio> {
        self.escritorios
            .values()
            .find(|info| info.host_id == host_id)
    }

    /// Índice de la pestaña de un escritorio.
    pub(crate) fn indice_pestana_escritorio(&self, escritorio_id: u32) -> Option<usize> {
        self.pestanas
            .iter()
            .position(|pestana| pestana.escritorio_id == Some(escritorio_id))
    }

    /// Id del escritorio de la pestaña activa, si lo es.
    pub fn escritorio_activo_id(&self) -> Option<u32> {
        self.pestana_activa
            .and_then(|indice| self.pestanas.get(indice))
            .and_then(|pestana| pestana.escritorio_id)
    }

    /// Sesión de escritorio de la pestaña activa, si lo es.
    pub fn escritorio_activo(&self) -> Option<&InfoEscritorio> {
        let id = self.escritorio_activo_id()?;
        self.escritorios.get(&id)
    }

    /// Activa la pestaña de un escritorio y entra en Sesión.
    pub(crate) fn activar_pestana_escritorio(&mut self, escritorio_id: u32) -> bool {
        let Some(indice) = self.indice_pestana_escritorio(escritorio_id) else {
            return false;
        };
        if self.vista == Vista::Sesion {
            self.desadjuntar_pestaña_activa();
        } else {
            self.vista_previa = Some(self.vista);
        }
        self.pestana_activa = Some(indice);
        self.vista = Vista::Sesion;
        self.sucio = true;
        true
    }

    // ------------------------------------------------------------- apertura

    /// ↵ en un escritorio de Hosts, Flota, la paleta o magi conectar: abre la
    /// sesión (o pide un enlace nuevo si ya existe).
    pub(crate) fn abrir_escritorio(&mut self, host_id: i64) {
        let Some(host) = self.hosts.iter().find(|host| host.id == host_id).cloned() else {
            self.mensaje("no hay host seleccionado", true);
            return;
        };
        if let Some(motivo) = self.motivo_servidor_incompatible() {
            self.mensaje(motivo, true);
            return;
        }
        if self.servidor_caido {
            self.mensaje("el servidor de sesiones ha caído; relánzalo primero", true);
            return;
        }
        if host.tipo == TipoHost::Voxl && host.salto_host_id.is_none() {
            self.mensaje(
                "voxl se arranca en un host SSH: elige «a través de» en la ficha",
                true,
            );
            return;
        }
        // Un VNC directo fuera de loopback pide confirmación la primera vez
        // (§6.2); al aceptarla se fija directo en el inventario.
        if let Some(aviso) = aviso_vnc_directo(
            host.tipo,
            &host.direccion,
            host.salto_host_id,
            host.escritorio_opciones.directo,
        ) {
            // Se apila sobre cualquier pregunta abierta, como las del
            // servidor: no se pisa ningún diálogo (T33).
            self.mostrar_dialogo_servidor(Dialogo::Escritorio(DialogoEscritorio::VncDirecto {
                host_id,
                aviso,
            }));
            return;
        }
        self.lanzar_escritorio(&host);
    }

    /// Abre el escritorio ya confirmado (o pide un enlace si la sesión existe).
    pub(crate) fn lanzar_escritorio(&mut self, host: &Host) {
        if let Some(info) = self.escritorio_de_host(host.id).cloned() {
            let escritorio_id = info.id;
            if self.activar_pestana_escritorio(escritorio_id) {
                self.mensaje(
                    format!("escritorio «{}»: pidiendo un enlace nuevo…", host.nombre),
                    false,
                );
                self.pedir_enlace_escritorio(escritorio_id, true);
            }
            return;
        }
        let peticion_id =
            self.nueva_peticion_escritorio(PeticionEscritorio::Abrir { host_id: host.id });
        self.servidor.enviar(MensajeCliente::AbrirEscritorio {
            peticion_id,
            host_id: host.id,
        });
        self.aperturas_escritorio.insert(host.id);
        self.mensaje(format!("abriendo escritorio «{}»…", host.nombre), false);
        self.sucio = true;
    }

    /// Confirma un VNC directo sin cifrar: fija directo y abre. Si no se puede
    /// escribir el inventario, se abre igual (la confirmación ya está).
    pub(crate) fn confirmar_vnc_directo(&mut self, host_id: i64) {
        if let Some(host) = self.hosts.iter().find(|host| host.id == host_id).cloned() {
            let mut datos = datos_de_host(&host);
            datos.escritorio_opciones.directo = true;
            if let Err(error) = self.almacen.actualizar_host(host_id, &datos) {
                tracing::warn!("no se pudo fijar directo en el host {host_id}: {error}");
            }
            if let Err(error) = self.recargar_inventario() {
                tracing::warn!("no se pudo recargar el inventario: {error}");
            }
        }
        if let Some(host) = self.hosts.iter().find(|host| host.id == host_id).cloned() {
            self.lanzar_escritorio(&host);
        }
    }

    // -------------------------------------------------------------- enlaces

    /// Pide un enlace de un solo uso: abrirlo en el visor (abrir) o copiarlo
    /// al portapapeles.
    pub(crate) fn pedir_enlace_escritorio(&mut self, escritorio_id: u32, abrir: bool) {
        let peticion_id = self.nueva_peticion_escritorio(PeticionEscritorio::Enlace {
            escritorio_id,
            abrir,
        });
        self.servidor.enviar(MensajeCliente::PedirEnlace {
            peticion_id,
            escritorio_id,
        });
        self.sucio = true;
    }

    /// ↵ en la pestaña de un escritorio: abre el visor. Si hay un enlace
    /// reciente que no se gastó (porque falló el navegador), se usa ese.
    pub(crate) fn abrir_visor_activo(&mut self) {
        let Some(escritorio_id) = self.escritorio_activo_id() else {
            return;
        };
        if let Some(enlace) = self.enlace_reciente(escritorio_id) {
            self.abrir_visor_escritorio(escritorio_id, &enlace);
            return;
        }
        self.pedir_enlace_escritorio(escritorio_id, true);
        self.mensaje("pidiendo un enlace del visor…", false);
    }

    /// y en la pestaña de un escritorio: copia un enlace de un solo uso.
    pub(crate) fn copiar_enlace_activo(&mut self) {
        let Some(escritorio_id) = self.escritorio_activo_id() else {
            return;
        };
        if let Some(enlace) = self.enlace_reciente(escritorio_id) {
            self.copiar_enlace(&enlace);
            return;
        }
        self.pedir_enlace_escritorio(escritorio_id, false);
        self.mensaje("pidiendo un enlace del visor…", false);
    }

    /// Enlace que aún no ha caducado y que la pestaña puede enseñar.
    #[doc(hidden)]
    pub fn enlace_reciente(&self, escritorio_id: u32) -> Option<String> {
        let (id, enlace, momento) = self.enlace_visible.as_ref()?;
        if *id != escritorio_id || momento.elapsed() >= DURACION_ENLACE {
            None
        } else {
            Some(enlace.clone())
        }
    }

    /// Copia un enlace al portapapeles; si no se puede, la pestaña lo enseña.
    pub(crate) fn copiar_enlace(&mut self, enlace: &str) {
        // En las pruebas no se toca el portapapeles del sistema (wl-copy
        // puede quedarse esperando a un lector): se comporta como si fallara.
        if !self.efectos_externos {
            let escritorio_id = self.escritorio_activo_id().unwrap_or(0);
            self.enlace_visible = Some((escritorio_id, enlace.to_string(), Instant::now()));
            self.mensaje(
                "sin portapapeles en este entorno; el enlace está en la pestaña",
                true,
            );
            self.sucio = true;
            return;
        }
        match crate::portapapeles::copiar(enlace) {
            Ok(_) => self.mensaje("enlace del visor copiado (vale 60 s)", false),
            Err(error) => {
                let escritorio_id = self.escritorio_activo_id().unwrap_or(0);
                self.enlace_visible = Some((escritorio_id, enlace.to_string(), Instant::now()));
                self.mensaje(
                    format!("no se pudo copiar el enlace: {error}; está en la pestaña"),
                    true,
                );
            }
        }
        self.sucio = true;
    }

    /// Abre el visor en el navegador; si no se puede, deja el enlace a la
    /// vista para copiarlo con y durante 60 s.
    pub(crate) fn abrir_visor_escritorio(&mut self, escritorio_id: u32, enlace: &str) {
        match abrir_navegador(&self.config.escritorio, enlace) {
            Ok(()) => {
                self.enlace_visible = None;
                self.mensaje("visor abierto en el navegador", false);
            }
            Err(error) => {
                self.enlace_visible = Some((escritorio_id, enlace.to_string(), Instant::now()));
                self.mensaje(
                    format!("no se pudo abrir el navegador: {error}; copia el enlace con y"),
                    true,
                );
            }
        }
        self.sucio = true;
    }

    /// prefijo r: vuelve a abrir una sesión caída.
    pub(crate) fn reconectar_escritorio(&mut self, escritorio_id: u32) {
        let peticion_id =
            self.nueva_peticion_escritorio(PeticionEscritorio::Reconectar { escritorio_id });
        self.servidor.enviar(MensajeCliente::ReconectarEscritorio {
            peticion_id,
            escritorio_id,
        });
        self.mensaje("reconectando el escritorio…", false);
        self.sucio = true;
    }

    /// prefijo x ya confirmado: cierra la sesión (el visor se cierra solo al
    /// caer el WebSocket).
    pub(crate) fn cerrar_escritorio(&mut self, escritorio_id: u32) {
        self.servidor
            .enviar(MensajeCliente::CerrarEscritorio { escritorio_id });
        self.mensaje("cerrando el escritorio…", false);
        self.sucio = true;
    }

    /// Pregunta antes de cerrar (T6: ninguna acción destructiva con una tecla).
    pub(crate) fn preguntar_cerrar_escritorio(&mut self, escritorio_id: u32) {
        let nombre = self
            .escritorios
            .get(&escritorio_id)
            .map(|info| info.nombre.clone())
            .or_else(|| {
                self.indice_pestana_escritorio(escritorio_id)
                    .and_then(|indice| self.pestanas.get(indice))
                    .map(|pestana| pestana.host_nombre.clone())
            })
            .unwrap_or_else(|| format!("escritorio #{escritorio_id}"));
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "CERRAR ESCRITORIO".to_string(),
            lineas: vec![
                format!("¿Cerrar el escritorio «{nombre}»?"),
                "La ventana del visor se cerrará cuando caiga la sesión.".to_string(),
            ],
            peligro: true,
            accion: super::AccionDialogo::CerrarEscritorio(escritorio_id),
        });
    }

    // ------------------------------------------------------------- la lista

    /// Difusión Escritorios{lista}: crea, actualiza y retira las pestañas de
    /// escritorio. La lista completa manda (T27).
    pub(crate) fn evento_escritorios(&mut self, lista: Vec<InfoEscritorio>) {
        let vivos: std::collections::HashSet<u32> = lista.iter().map(|info| info.id).collect();
        let anteriores: std::collections::HashSet<u32> = self.escritorios.keys().copied().collect();
        let hosts_anteriores: std::collections::HashMap<u32, i64> = self
            .escritorios
            .iter()
            .map(|(id, info)| (*id, info.host_id))
            .collect();
        let desaparecidos: Vec<u32> = self
            .pestanas
            .iter()
            .filter_map(|pestana| pestana.escritorio_id)
            .filter(|id| !vivos.contains(id))
            .collect();
        for id in desaparecidos {
            self.quitar_pestana_escritorio(id);
        }
        for info in &lista {
            self.actualizar_pestana_escritorio(info);
        }
        // Las contraseñas pedidas con «recordar» solo se guardan cuando la
        // sesión ha quedado abierta de verdad (m5 de la revisión).
        for info in lista
            .iter()
            .filter(|info| info.estado == EstadoEscritorio::Abierto)
        {
            if let Some((usuario, contrasena)) =
                self.contrasenas_escritorio_pendientes.remove(&info.id)
            {
                match self.guardar_contrasena_escritorio(
                    info.host_id,
                    &usuario,
                    contrasena.as_str(),
                ) {
                    Ok(()) => self.mensaje(
                        format!("contraseña de «{}» guardada en el llavero", info.nombre),
                        false,
                    ),
                    Err(error) => {
                        self.mensaje(format!("no se pudo guardar la contraseña: {error}"), true);
                    }
                }
            }
        }
        self.escritorios = lista.into_iter().map(|info| (info.id, info)).collect();
        // Lo que apuntaba a una sesión que ya no está se retira: preguntas
        // del servidor, enlaces sin gastar y peticiones en vuelo.
        let caidos: Vec<u32> = anteriores.difference(&vivos).copied().collect();
        if !caidos.is_empty() {
            let hosts_caidos: std::collections::HashSet<i64> = caidos
                .iter()
                .filter_map(|id| hosts_anteriores.get(id).copied())
                .collect();
            self.aperturas_escritorio
                .retain(|host_id| !hosts_caidos.contains(host_id));
            self.contrasenas_escritorio_pendientes
                .retain(|escritorio_id, _| !caidos.contains(escritorio_id));
            self.peticiones_escritorio
                .retain(|_, peticion| match peticion {
                    PeticionEscritorio::Enlace {
                        escritorio_id: id, ..
                    }
                    | PeticionEscritorio::Reconectar {
                        escritorio_id: id, ..
                    } => !caidos.contains(id),
                    PeticionEscritorio::Abrir { host_id } => !hosts_caidos.contains(host_id),
                });
            if self
                .enlace_visible
                .as_ref()
                .is_some_and(|(id, _, _)| caidos.contains(id))
            {
                self.enlace_visible = None;
            }
            let caido = |dialogo: &Dialogo| {
                matches!(
                    dialogo,
                    Dialogo::Escritorio(escritorio)
                        if escritorio.escritorio_id().is_some_and(|id| caidos.contains(&id))
                )
            };
            let tenia = self.dialogo.as_ref().is_some_and(caido);
            self.pila_dialogos.retain(|dialogo| !caido(dialogo));
            if tenia {
                self.dialogo = self.pila_dialogos.pop();
            }
        }
        self.sucio = true;
    }

    fn actualizar_pestana_escritorio(&mut self, info: &InfoEscritorio) {
        let estado = estado_de_pestana(info.estado);
        if let Some(pestana) = self
            .pestanas
            .iter_mut()
            .find(|pestana| pestana.escritorio_id == Some(info.id))
        {
            pestana.nombre = info.nombre.clone();
            pestana.host_nombre = info.nombre.clone();
            pestana.host_id = info.host_id;
            pestana.estado = estado;
            pestana.motivo = info.motivo.clone();
            pestana.tipo = Some(info.tipo);
            pestana.identidad = String::new();
            pestana.abierta_en = info.desde;
            return;
        }
        let activar = self.aperturas_escritorio.remove(&info.host_id);
        self.pestanas.push(super::PestanaUI {
            sesion_id: 0,
            escritorio_id: Some(info.id),
            tipo: Some(info.tipo),
            nombre: info.nombre.clone(),
            host_id: info.host_id,
            host_nombre: info.nombre.clone(),
            estado,
            motivo: info.motivo.clone(),
            identidad: String::new(),
            pantalla: crate::conexion::terminal::nuevo(1, 1),
            actividad_no_vista: false,
            abierta_en: info.desde,
            ventanas: 0,
            cols_remoto: 0,
            filas_remoto: 0,
            ventana_minima: None,
        });
        if activar {
            let indice = self.pestanas.len() - 1;
            if self.vista == Vista::Sesion {
                self.desadjuntar_pestaña_activa();
            } else {
                self.vista_previa = Some(self.vista);
            }
            self.pestana_activa = Some(indice);
            self.vista = Vista::Sesion;
            self.mensaje(format!("escritorio «{}» abriéndose", info.nombre), false);
        }
    }

    fn quitar_pestana_escritorio(&mut self, escritorio_id: u32) {
        let Some(indice) = self.indice_pestana_escritorio(escritorio_id) else {
            return;
        };
        let activa = self.pestana_activa;
        self.pestanas.remove(indice);
        self.contrasenas_escritorio_pendientes
            .remove(&escritorio_id);
        // Las peticiones de esa sesión ya no van a resolverse.
        self.peticiones_escritorio
            .retain(|_, peticion| match peticion {
                PeticionEscritorio::Enlace {
                    escritorio_id: id, ..
                }
                | PeticionEscritorio::Reconectar {
                    escritorio_id: id, ..
                } => *id != escritorio_id,
                PeticionEscritorio::Abrir { .. } => true,
            });
        if self
            .enlace_visible
            .as_ref()
            .is_some_and(|(id, _, _)| *id == escritorio_id)
        {
            self.enlace_visible = None;
        }
        if self.seleccion_sesiones >= self.pestanas.len() {
            self.seleccion_sesiones = self.pestanas.len().saturating_sub(1);
        }
        match activa {
            Some(actual) if actual == indice => {
                if self.pestanas.is_empty() {
                    self.pestana_activa = None;
                    if self.vista == Vista::Sesion {
                        self.volver_tras_cierre();
                        self.mensaje("escritorio cerrado", false);
                    }
                } else {
                    self.pestana_activa = Some(indice.min(self.pestanas.len() - 1));
                    if self.vista == Vista::Sesion {
                        self.adjuntar_pestaña_activa();
                    }
                }
            }
            Some(actual) if actual > indice => self.pestana_activa = Some(actual - 1),
            _ => {}
        }
        self.sucio = true;
    }

    /// Entrada EscritorioAbierto: enlace del visor.
    pub(crate) fn escritorio_abierto(
        &mut self,
        peticion_id: u64,
        escritorio_id: u32,
        enlace: String,
    ) {
        let peticion = self.peticiones_escritorio.remove(&peticion_id);
        match peticion {
            // Respuesta de una petición que ya no está en vuelo (cancelada o
            // contestada antes): se descarta.
            None => {}
            Some(PeticionEscritorio::Enlace { abrir: false, .. }) => {
                self.copiar_enlace(&enlace);
            }
            Some(_) => self.abrir_visor_escritorio(escritorio_id, &enlace),
        }
    }

    /// Entrada Enlace: abrir el visor o copiarlo según lo que se pidió.
    pub(crate) fn enlace_escritorio(
        &mut self,
        peticion_id: u64,
        escritorio_id: u32,
        enlace: String,
    ) {
        let peticion = self.peticiones_escritorio.remove(&peticion_id);
        match peticion {
            // Respuesta de una petición que ya no está en vuelo: se descarta
            // (m3 de la revisión: no abrir el visor ni copiar por sorpresa).
            None => {}
            Some(PeticionEscritorio::Enlace { abrir: true, .. }) => {
                self.abrir_visor_escritorio(escritorio_id, &enlace);
            }
            Some(PeticionEscritorio::Enlace { abrir: false, .. }) => {
                self.copiar_enlace(&enlace);
            }
            // El servidor contestó con Enlace a una reconexión (alternativa
            // acordada): el enlace se aprovecha.
            Some(_) => self.abrir_visor_escritorio(escritorio_id, &enlace),
        }
    }

    // -------------------------------------------------------- diálogos: huella

    /// PideHuellaEscritorio: el cliente decide y guarda antes de responder.
    pub(crate) fn pedir_huella_escritorio(
        &mut self,
        peticion_id: u64,
        escritorio_id: u32,
        host_id: i64,
        host_nombre: String,
        datos: DatosCertificado,
    ) {
        self.mostrar_dialogo_servidor(Dialogo::Escritorio(DialogoEscritorio::Certificado {
            peticion_id,
            escritorio_id,
            host_id,
            host_nombre,
            datos,
            campo: CampoTexto::default(),
            error: None,
        }));
    }

    /// Guarda la huella, anota en REGISTRO y responde al servidor. Devuelve el
    /// motivo si no se pudo guardar (entonces no se responde: se reintenta).
    fn intentar_aceptar_certificado(
        &mut self,
        host_id: i64,
        datos: &DatosCertificado,
        sustituida: bool,
    ) -> Result<(), String> {
        let huella = datos.huella.clone();
        self.almacen
            .fijar_huella_escritorio(host_id, Some(&huella))
            .map_err(|error| format!("no se pudo guardar la huella: {error}"))?;
        let detalle = format!(
            "{} · {huella}",
            if sustituida { "sustituida" } else { "aceptada" }
        );
        self.anotar(
            crate::registro::HUELLA_ESCRITORIO,
            Some(host_id),
            None,
            &detalle,
            ResultadoRegistro::Ok,
        );
        if let Some(host) = self.hosts.iter_mut().find(|host| host.id == host_id) {
            host.huella_escritorio = Some(huella);
        }
        Ok(())
    }

    // -------------------------------------------------- diálogos: credenciales

    /// PideCredenciales: primero el llavero (si el host lo tiene marcado) y si
    /// no, el diálogo del intento 1 a 3.
    pub(crate) fn pedir_credenciales_escritorio(
        &mut self,
        peticion_id: u64,
        escritorio_id: u32,
        host_nombre: String,
        peticion: PeticionCredenciales,
    ) {
        let host = self
            .host_de_escritorio(escritorio_id)
            .or_else(|| self.hosts.iter().find(|host| host.nombre == host_nombre));
        // VNC sin contraseña: se contesta vacío y no se molesta al usuario.
        if peticion.tipo == TipoHost::Vnc
            && host.is_some_and(|host| host.identidad_ref.es_auto())
            && peticion.intento <= 1
        {
            self.enviar_credenciales(
                CredencialesEscritorio {
                    usuario: String::new(),
                    dominio: None,
                    contrasena: zeroize::Zeroizing::new(String::new()),
                    recordar: false,
                },
                peticion_id,
            );
            return;
        }
        if peticion.intento <= 1 {
            if let Some(credenciales) =
                self.credenciales_del_llavero(escritorio_id, &host_nombre, &peticion)
            {
                self.enviar_credenciales(credenciales, peticion_id);
                return;
            }
        }
        let usuario = peticion
            .usuario
            .clone()
            .or_else(|| host.and_then(|host| host.usuario.clone()))
            .unwrap_or_default();
        let dominio = peticion
            .dominio
            .clone()
            .or_else(|| host.and_then(|host| host.escritorio_dominio.clone()))
            .unwrap_or_default();
        let foco = if peticion.tipo == TipoHost::Vnc || !usuario.is_empty() {
            2
        } else {
            0
        };
        let recordar = crate::llavero::disponible()
            && host.is_some_and(|host| host.identidad_ref.usa_llavero());
        self.mostrar_dialogo_servidor(Dialogo::Escritorio(DialogoEscritorio::Credenciales {
            peticion_id,
            escritorio_id,
            host_nombre,
            peticion,
            usuario: CampoTexto::nuevo(usuario),
            dominio: CampoTexto::nuevo(dominio),
            contrasena: CampoTexto::default(),
            recordar,
            foco,
            error: None,
        }));
    }

    /// Host de una sesión de escritorio. Se resuelve por el id de la sesión y
    /// no por el nombre (m4 de la revisión: el nombre puede repetirse o
    /// cambiar mientras la sesión sigue abierta).
    pub(crate) fn host_de_escritorio(&self, escritorio_id: u32) -> Option<&Host> {
        let host_id = self.escritorios.get(&escritorio_id)?.host_id;
        self.hosts.iter().find(|host| host.id == host_id)
    }

    /// Credenciales del llavero para un escritorio, si el host las guarda.
    fn credenciales_del_llavero(
        &self,
        escritorio_id: u32,
        host_nombre: &str,
        peticion: &PeticionCredenciales,
    ) -> Option<CredencialesEscritorio> {
        let host = self
            .host_de_escritorio(escritorio_id)
            .or_else(|| self.hosts.iter().find(|host| host.nombre == host_nombre))?;
        if !host.identidad_ref.usa_llavero() {
            return None;
        }
        let usuario = peticion
            .usuario
            .clone()
            .or_else(|| host.usuario.clone())
            .unwrap_or_else(|| match host.tipo {
                TipoHost::Vnc | TipoHost::Voxl => String::new(),
                _ => super::usuario_local(),
            });
        let contrasena = crate::llavero::recuperar(&host.nombre, &usuario)
            .ok()
            .flatten()?;
        Some(CredencialesEscritorio {
            usuario,
            dominio: peticion
                .dominio
                .clone()
                .or_else(|| host.escritorio_dominio.clone()),
            contrasena,
            recordar: false,
        })
    }

    /// Manda unas credenciales al servidor (nunca van al log).
    fn enviar_credenciales(&mut self, credenciales: CredencialesEscritorio, peticion_id: u64) {
        self.servidor.enviar(MensajeCliente::Credenciales {
            peticion_id,
            usuario: credenciales.usuario,
            dominio: credenciales.dominio,
            contrasena: protocolo::Secreto::nuevo(credenciales.contrasena.as_str()),
            recordar: credenciales.recordar,
        });
        self.sucio = true;
    }

    /// Guarda una contraseña en el llavero y marca el host como
    /// contrasena:llavero. Devuelve el motivo si no se pudo.
    pub(crate) fn guardar_contrasena_escritorio(
        &mut self,
        host_id: i64,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        let Some(nombre) = self
            .hosts
            .iter()
            .find(|host| host.id == host_id)
            .map(|host| host.nombre.clone())
        else {
            return Err("el host ya no existe".to_string());
        };
        crate::llavero::guardar(&nombre, usuario, contrasena)?;
        if let Err(error) = self
            .almacen
            .marcar_identidad_ref(host_id, &IdentidadRef::ContrasenaLlavero)
        {
            tracing::warn!("no se pudo marcar la contraseña de escritorio: {error}");
        }
        if let Some(host) = self.hosts.iter_mut().find(|host| host.id == host_id) {
            host.identidad_ref = IdentidadRef::ContrasenaLlavero;
        }
        Ok(())
    }

    /// Cancela una apertura que esperaba credenciales: no hay un mensaje
    /// negativo en el protocolo, así que se cierra la sesión (el servidor lo
    /// anota como cancelación).
    pub(crate) fn cancelar_credenciales_escritorio(&mut self, escritorio_id: u32) {
        // Nada que recordar: la apertura se cancela.
        self.contrasenas_escritorio_pendientes
            .remove(&escritorio_id);
        self.servidor
            .enviar(MensajeCliente::CerrarEscritorio { escritorio_id });
        self.mensaje("apertura del escritorio cancelada", false);
        self.sucio = true;
    }

    // --------------------------------------------------------- olvidar (ficha)

    /// Pregunta antes de olvidar la huella del certificado (T6).
    pub(crate) fn preguntar_olvidar_huella(&mut self, host_id: i64) {
        let Some(host) = self.hosts.iter().find(|host| host.id == host_id) else {
            return;
        };
        let mut lineas = vec![format!(
            "¿Olvidar la huella del certificado de «{}»?",
            host.nombre
        )];
        if let Some(huella) = &host.huella_escritorio {
            lineas.push(format!("Huella: {huella}"));
        }
        lineas.push("La próxima conexión pedirá aceptarla otra vez.".to_string());
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "OLVIDAR HUELLA DEL CERTIFICADO".to_string(),
            lineas,
            peligro: true,
            accion: super::AccionDialogo::OlvidarHuellaEscritorio(host_id),
        });
    }

    /// Olvida la huella del certificado de un host (ya confirmado).
    pub(crate) fn olvidar_huella(&mut self, host_id: i64) {
        match self.almacen.fijar_huella_escritorio(host_id, None) {
            Ok(()) => {
                if let Some(host) = self.hosts.iter_mut().find(|host| host.id == host_id) {
                    host.huella_escritorio = None;
                }
                if let Some(ficha) = &mut self.ficha {
                    if ficha.host_id == Some(host_id) {
                        ficha.huella = None;
                    }
                }
                self.mensaje("huella del certificado olvidada", false);
            }
            Err(error) => self.mensaje(
                format!("no se pudo olvidar la huella del certificado: {error}"),
                true,
            ),
        }
        self.sucio = true;
    }

    /// Pregunta antes de olvidar la contraseña del llavero (T6).
    pub(crate) fn preguntar_olvidar_contrasena_escritorio(&mut self, host_id: i64) {
        let Some(host) = self.hosts.iter().find(|host| host.id == host_id) else {
            return;
        };
        let usuario = usuario_llavero(host);
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "OLVIDAR CONTRASEÑA DEL ESCRITORIO".to_string(),
            lineas: vec![
                format!(
                    "¿Olvidar la contraseña de «{}» del llavero del sistema?",
                    host.nombre
                ),
                "La próxima conexión la pedirá y no se podrá recuperar.".to_string(),
            ],
            peligro: true,
            accion: super::AccionDialogo::OlvidarContrasenaEscritorio { host_id, usuario },
        });
    }

    /// Olvida la contraseña de un escritorio (ya confirmado): la borra del
    /// llavero y deja el host en pedir siempre.
    pub(crate) fn olvidar_contrasena_escritorio(&mut self, host_id: i64, usuario: &str) {
        let Some(nombre) = self
            .hosts
            .iter()
            .find(|host| host.id == host_id)
            .map(|host| host.nombre.clone())
        else {
            return;
        };
        match crate::llavero::olvidar(&nombre, usuario) {
            Ok(_) => {
                if let Err(error) = self
                    .almacen
                    .marcar_identidad_ref(host_id, &IdentidadRef::Contrasena)
                {
                    tracing::warn!("no se pudo marcar pedir siempre: {error}");
                }
                if let Some(host) = self.hosts.iter_mut().find(|host| host.id == host_id) {
                    host.identidad_ref = IdentidadRef::Contrasena;
                }
                if let Some(ficha) = &mut self.ficha {
                    if ficha.host_id == Some(host_id) {
                        ficha.contrasena_guardada = false;
                        ficha.contrasena_modo = false;
                    }
                }
                self.mensaje("contraseña olvidada del llavero", false);
            }
            Err(error) => self.mensaje(format!("no se pudo olvidar la contraseña: {error}"), true),
        }
        self.sucio = true;
    }

    // -------------------------------------------------------- teclas: diálogos

    /// Teclas del diálogo de escritorio abierto. El diálogo ya salió de
    /// App::dialogo: si sigue, se vuelve a poner.
    pub(crate) fn tecla_dialogo_escritorio(&mut self, dialogo: DialogoEscritorio, tecla: KeyEvent) {
        match dialogo {
            DialogoEscritorio::Certificado {
                peticion_id,
                escritorio_id,
                host_id,
                host_nombre,
                datos,
                mut campo,
                error,
            } => {
                let cambiado = datos.tipo == TipoCambioCertificado::Cambiado;
                let aceptar = match tecla.code {
                    // Un certificado nuevo se acepta con una tecla explícita
                    // (T6: es una decisión de seguridad).
                    KeyCode::Char('a') | KeyCode::Char('A') if !cambiado => true,
                    // En uno cambiado hay que poder escribir el nombre del
                    // host, así que solo Enter sustituye (y Esc cancela).
                    KeyCode::Enter if cambiado => {
                        if campo.texto.trim() == host_nombre {
                            true
                        } else {
                            self.dialogo =
                                Some(Dialogo::Escritorio(DialogoEscritorio::Certificado {
                                    peticion_id,
                                    escritorio_id,
                                    host_id,
                                    host_nombre: host_nombre.clone(),
                                    datos,
                                    campo,
                                    error: Some(format!(
                                        "escribe «{host_nombre}» tal cual para sustituirla"
                                    )),
                                }));
                            return;
                        }
                    }
                    KeyCode::Esc => {
                        self.servidor
                            .enviar(MensajeCliente::RespuestaHuellaEscritorio {
                                peticion_id,
                                aceptada: false,
                            });
                        self.mensaje("certificado no aceptado: no se conecta", false);
                        self.sucio = true;
                        return;
                    }
                    _ => {
                        if cambiado {
                            campo.manejar_tecla(&tecla);
                        }
                        self.dialogo = Some(Dialogo::Escritorio(DialogoEscritorio::Certificado {
                            peticion_id,
                            escritorio_id,
                            host_id,
                            host_nombre,
                            datos,
                            campo,
                            error,
                        }));
                        return;
                    }
                };
                if !aceptar {
                    return;
                }
                match self.intentar_aceptar_certificado(host_id, &datos, cambiado) {
                    Ok(()) => {
                        self.servidor
                            .enviar(MensajeCliente::RespuestaHuellaEscritorio {
                                peticion_id,
                                aceptada: true,
                            });
                        self.mensaje(
                            if cambiado {
                                "huella del certificado sustituida"
                            } else {
                                "huella del certificado aceptada"
                            },
                            false,
                        );
                        self.sucio = true;
                    }
                    Err(motivo) => {
                        self.dialogo = Some(Dialogo::Escritorio(DialogoEscritorio::Certificado {
                            peticion_id,
                            escritorio_id,
                            host_id,
                            host_nombre,
                            datos,
                            campo,
                            error: Some(motivo),
                        }));
                    }
                }
            }
            DialogoEscritorio::Credenciales {
                peticion_id,
                escritorio_id,
                host_nombre,
                peticion,
                mut usuario,
                mut dominio,
                mut contrasena,
                mut recordar,
                mut foco,
                error,
            } => {
                let es_vnc = peticion.tipo == TipoHost::Vnc;
                let con_casilla = crate::llavero::disponible();
                let reponer = |app: &mut App,
                               usuario: CampoTexto,
                               dominio: CampoTexto,
                               contrasena: CampoTexto,
                               recordar: bool,
                               foco: u8,
                               error: Option<String>| {
                    app.dialogo = Some(Dialogo::Escritorio(DialogoEscritorio::Credenciales {
                        peticion_id,
                        escritorio_id,
                        host_nombre: host_nombre.clone(),
                        peticion: peticion.clone(),
                        usuario,
                        dominio,
                        contrasena,
                        recordar,
                        foco,
                        error,
                    }));
                };
                match tecla.code {
                    KeyCode::Esc => {
                        self.cancelar_credenciales_escritorio(escritorio_id);
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        foco = siguiente_foco(
                            foco,
                            tecla.code == KeyCode::BackTab,
                            es_vnc,
                            con_casilla,
                        );
                        reponer(self, usuario, dominio, contrasena, recordar, foco, error);
                    }
                    KeyCode::Char(' ') if con_casilla && foco == 3 => {
                        recordar = !recordar;
                        reponer(self, usuario, dominio, contrasena, recordar, foco, error);
                    }
                    KeyCode::Enter => {
                        if !es_vnc && usuario.texto.trim().is_empty() {
                            reponer(
                                self,
                                usuario,
                                dominio,
                                contrasena,
                                recordar,
                                foco,
                                Some("el usuario es obligatorio".to_string()),
                            );
                            return;
                        }
                        let usuario_limpio = usuario.texto.trim().to_string();
                        let credenciales = CredencialesEscritorio {
                            usuario: usuario_limpio.clone(),
                            dominio: if es_vnc || dominio.texto.trim().is_empty() {
                                None
                            } else {
                                Some(dominio.texto.trim().to_string())
                            },
                            contrasena: zeroize::Zeroizing::new(contrasena.texto.clone()),
                            recordar,
                        };
                        if recordar && !contrasena.texto.is_empty() {
                            // No se guarda todavía: el llavero se escribe
                            // cuando el servidor confirme que la sesión quedó
                            // abierta (m5 de la revisión). Así una contraseña
                            // equivocada no sustituye a la buena.
                            self.contrasenas_escritorio_pendientes.insert(
                                escritorio_id,
                                (
                                    usuario_limpio.clone(),
                                    zeroize::Zeroizing::new(contrasena.texto.clone()),
                                ),
                            );
                        }
                        self.enviar_credenciales(credenciales, peticion_id);
                    }
                    _ => {
                        match foco {
                            0 => {
                                usuario.manejar_tecla(&tecla);
                            }
                            1 => {
                                dominio.manejar_tecla(&tecla);
                            }
                            2 => {
                                contrasena.manejar_tecla(&tecla);
                            }
                            _ => {}
                        }
                        reponer(self, usuario, dominio, contrasena, recordar, foco, error);
                    }
                }
            }
            DialogoEscritorio::VncDirecto { host_id, .. } => match tecla.code {
                KeyCode::Enter
                | KeyCode::Char('s')
                | KeyCode::Char('S')
                | KeyCode::Char('a')
                | KeyCode::Char('A') => self.confirmar_vnc_directo(host_id),
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.mensaje("VNC directo cancelado", false);
                }
                _ => {
                    // El aviso se queda hasta que el usuario decide.
                    if let Some(host) = self.hosts.iter().find(|host| host.id == host_id) {
                        if let Some(aviso) = aviso_vnc_directo(
                            host.tipo,
                            &host.direccion,
                            host.salto_host_id,
                            host.escritorio_opciones.directo,
                        ) {
                            self.dialogo =
                                Some(Dialogo::Escritorio(DialogoEscritorio::VncDirecto {
                                    host_id,
                                    aviso,
                                }));
                        }
                    }
                }
            },
        }
    }

    /// ¿Hay contraseña del escritorio guardada en el llavero? Se mira una vez
    /// al abrir la ficha: la UI no puede consultarlo en cada pintado.
    pub(crate) fn contrasena_guardada(&self, host: &Host) -> bool {
        if !host.identidad_ref.usa_llavero() {
            return false;
        }
        let usuario = usuario_llavero(host);
        crate::llavero::recuperar(&host.nombre, &usuario)
            .ok()
            .flatten()
            .is_some()
    }
}

/// Todo lo que necesita el diagnóstico para comprobar el navegador.
pub fn descripcion_para_diagnostico(
    config: &SeccionEscritorio,
    existe: &dyn Fn(&str) -> bool,
    app: &dyn Fn(&str) -> bool,
) -> Result<(String, bool), String> {
    let navegador =
        resolver_navegador(config, plataforma(), "http://127.0.0.1/entrar", existe, app)?;
    let texto = match &navegador.origen {
        OrigenNavegador::Orden => format!("orden propia · {}", navegador.args.join(" ")),
        OrigenNavegador::Chromium(nombre) => format!("auto · {nombre} --app"),
        OrigenNavegador::AplicacionMac(nombre) => format!("auto · {nombre} (open -na)"),
        OrigenNavegador::Sistema => {
            let abridor = navegador
                .args
                .first()
                .cloned()
                .unwrap_or_else(|| "abridor del sistema".to_string());
            format!("auto · sin Chromium/Chrome/Edge; se usará {abridor}")
        }
    };
    let aviso = matches!(&navegador.origen, OrigenNavegador::Sistema);
    Ok((texto, aviso))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn config(navegador: &str) -> SeccionEscritorio {
        SeccionEscritorio {
            navegador: navegador.to_string(),
            ..SeccionEscritorio::default()
        }
    }

    #[test]
    fn una_orden_propia_se_tokeniza_y_sustituye_la_url() {
        let tokens = tokenizar(r#"brave --app={url}"#).unwrap();
        assert_eq!(tokens, vec!["brave", "--app={url}"]);
        let tokens = tokenizar(r#""Mi Navegador" --app="{url}""#).unwrap();
        assert_eq!(tokens, vec!["Mi Navegador", "--app={url}"]);
        let resultado = resolver_navegador(
            &config(r#""Mi Navegador" --app="{url}""#),
            Plataforma::Linux,
            "http://127.0.0.1/entrar?t=abc",
            &|_| false,
            &|_| false,
        )
        .unwrap();
        assert_eq!(
            resultado.args,
            vec![
                "Mi Navegador".to_string(),
                "--app=http://127.0.0.1/entrar?t=abc".to_string()
            ]
        );
        assert_eq!(resultado.origen, OrigenNavegador::Orden);
    }

    #[test]
    fn una_orden_sin_comillas_de_cierre_es_un_error() {
        assert!(tokenizar(r#""brave --app={url}"#).is_err());
        assert!(tokenizar("   ").is_err());
    }

    #[test]
    fn auto_prefiere_los_chromium_y_cae_a_xdg_open() {
        let existe = |programa: &str| programa == "microsoft-edge-stable";
        let navegador = resolver_navegador(
            &config("auto"),
            Plataforma::Linux,
            "http://127.0.0.1:8080/entrar?t=x",
            &existe,
            &|_| false,
        )
        .unwrap();
        assert_eq!(
            navegador.args,
            vec![
                "microsoft-edge-stable".to_string(),
                "--app=http://127.0.0.1:8080/entrar?t=x".to_string()
            ]
        );
        assert_eq!(
            navegador.origen,
            OrigenNavegador::Chromium("microsoft-edge-stable".to_string())
        );

        let navegador = resolver_navegador(
            &config("auto"),
            Plataforma::Linux,
            "http://127.0.0.1/entrar",
            &|_| false,
            &|_| false,
        )
        .unwrap();
        assert_eq!(
            navegador.args,
            vec![
                "xdg-open".to_string(),
                "http://127.0.0.1/entrar".to_string()
            ]
        );
        assert_eq!(navegador.origen, OrigenNavegador::Sistema);
    }

    #[test]
    fn en_macos_usa_open_na_y_si_no_hay_app_cae_a_open() {
        let navegador = resolver_navegador(
            &config("auto"),
            Plataforma::Macos,
            "http://127.0.0.1/entrar",
            &|_| false,
            &|app| app == "Brave Browser",
        )
        .unwrap();
        assert_eq!(
            navegador.args,
            vec![
                "open".to_string(),
                "-na".to_string(),
                "Brave Browser".to_string(),
                "--args".to_string(),
                "--app=http://127.0.0.1/entrar".to_string()
            ]
        );
        let navegador = resolver_navegador(
            &config("auto"),
            Plataforma::Macos,
            "http://127.0.0.1/entrar",
            &|_| false,
            &|_| false,
        )
        .unwrap();
        assert_eq!(
            navegador.args,
            vec!["open".to_string(), "http://127.0.0.1/entrar".to_string()]
        );
    }

    #[test]
    fn una_orden_que_sea_un_shell_se_rechaza() {
        let error = resolver_navegador(
            &config(r#"sh -c "chromium {url}""#),
            Plataforma::Linux,
            "http://127.0.0.1/entrar",
            &|_| true,
            &|_| true,
        )
        .unwrap_err();
        assert!(error.contains("shell"), "{error}");
    }

    #[test]
    fn el_diagnostico_avisa_si_solo_queda_el_abridor_del_sistema() {
        let (texto, aviso) =
            descripcion_para_diagnostico(&config("auto"), &|_| false, &|_| false).unwrap();
        assert!(aviso, "{texto}");
        assert!(texto.contains("xdg-open") || texto.contains("open"));

        // Los nombres de navegador dependen de la plataforma: en macOS se
        // buscan aplicaciones y en Linux, binarios del PATH.
        let (texto, aviso) = if cfg!(target_os = "macos") {
            descripcion_para_diagnostico(&config("auto"), &|_| false, &|p| p == "Google Chrome")
                .unwrap()
        } else {
            descripcion_para_diagnostico(&config("auto"), &|p| p == "chromium", &|_| false).unwrap()
        };
        assert!(!aviso);
        assert!(
            texto.contains(if cfg!(target_os = "macos") {
                "Google Chrome"
            } else {
                "chromium"
            }),
            "{texto}"
        );
    }

    #[test]
    fn los_estados_del_servidor_se_traducen_a_los_de_la_pestana() {
        use protocolo::EstadoSesionRemota;
        assert_eq!(
            estado_de_pestana(EstadoEscritorio::Abriendo),
            EstadoSesionRemota::Abriendo
        );
        assert_eq!(
            estado_de_pestana(EstadoEscritorio::Abierto),
            EstadoSesionRemota::Abierta
        );
        assert_eq!(
            estado_de_pestana(EstadoEscritorio::Caido),
            EstadoSesionRemota::Caida
        );
        assert_eq!(
            estado_de_pestana(EstadoEscritorio::Cerrando),
            EstadoSesionRemota::Cerrada
        );
    }

    #[test]
    fn el_foco_del_dialogo_salta_los_campos_que_no_aplican() {
        // VNC: contraseña (2) y casilla (3) si hay llavero.
        assert_eq!(siguiente_foco(2, false, true, true), 3);
        assert_eq!(siguiente_foco(3, false, true, true), 2);
        assert_eq!(siguiente_foco(2, false, true, false), 2);
        // RDP: tres campos y la casilla.
        assert_eq!(siguiente_foco(0, false, false, true), 1);
        assert_eq!(siguiente_foco(2, false, false, true), 3);
        assert_eq!(siguiente_foco(2, false, false, false), 0);
    }
}
