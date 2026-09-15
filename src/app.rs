use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as ConfigNucleo, Matcher};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::warn;
use zeroize::Zeroizing;

use crate::almacen::Almacen;
use crate::conexion::{self, ComandoConexion, EventoConexion, PlanConexion};
use crate::config::{Config, Rutas};
use crate::identidades::Identidades;
use crate::modelo::{
    self, fecha_ahora, DatosHost, EstadoSesion, Grupo, Host, IdentidadRef, Origen, UltimoEstado,
};
use crate::sshconfig;
use crate::tema::Tema;
use crate::ui::componentes::{AreaTexto, CampoTexto, Desplegable, Opcion, ValorOpcion};
use crate::ui::Vista;

/// Eventos que llegan al bucle principal de la UI.
pub enum Evento {
    Tecla(KeyEvent),
    Redimension(u16, u16),
    Tick,
    Conexion(EventoConexion),
    Identidades(Identidades),
}

#[derive(Debug, Clone)]
pub struct Mensaje {
    pub texto: String,
    pub error: bool,
    pub creado: Instant,
}

#[derive(Debug, Clone)]
pub enum Fila {
    Grupo {
        grupo_id: Option<i64>,
        nombre: String,
        plegado: bool,
        total: usize,
        filtrado: bool,
    },
    Host {
        indice: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoFicha {
    Nombre,
    Direccion,
    Puerto,
    Grupo,
    Etiquetas,
    Usuario,
    Identidad,
    Salto,
    Multiplexar,
    Mantener,
    Keepalive,
    Opciones,
}

pub const ORDEN_CAMPOS: [CampoFicha; 12] = [
    CampoFicha::Nombre,
    CampoFicha::Direccion,
    CampoFicha::Puerto,
    CampoFicha::Grupo,
    CampoFicha::Etiquetas,
    CampoFicha::Usuario,
    CampoFicha::Identidad,
    CampoFicha::Salto,
    CampoFicha::Multiplexar,
    CampoFicha::Mantener,
    CampoFicha::Keepalive,
    CampoFicha::Opciones,
];

pub enum AccionFicha {
    Nada,
    Guardar,
    Probar,
    Cerrar,
    DescartarConCambios,
}

pub struct Ficha {
    pub host_id: Option<i64>,
    pub original: DatosHost,
    pub titulo: String,
    pub campo: CampoFicha,
    pub nombre: CampoTexto,
    pub direccion: CampoTexto,
    pub puerto: CampoTexto,
    pub etiquetas: CampoTexto,
    pub usuario: CampoTexto,
    pub keepalive: CampoTexto,
    pub opciones: AreaTexto,
    pub grupo: Desplegable,
    pub identidad: Desplegable,
    pub salto: Desplegable,
    pub multiplexar: bool,
    pub mantener: bool,
    pub desplegable_abierto: Option<CampoFicha>,
    pub sugerencias: Vec<String>,
    pub indice_sugerencia: usize,
}

impl Ficha {
    pub fn datos(&self) -> DatosHost {
        let puerto = self.puerto.texto.trim().parse::<u16>().unwrap_or(0);
        let keepalive = if self.mantener {
            self.keepalive.texto.trim().parse::<u32>().ok()
        } else {
            None
        };
        let usuario = self.usuario.texto.trim();
        let grupo_id = match self.grupo.valor_seleccionado() {
            ValorOpcion::Grupo(id) => Some(id),
            _ => None,
        };
        let salto_host_id = match self.salto.valor_seleccionado() {
            ValorOpcion::Salto(id) => Some(id),
            _ => None,
        };
        let identidad_ref = match self.identidad.valor_seleccionado() {
            ValorOpcion::Identidad(identidad) => identidad,
            _ => IdentidadRef::Auto,
        };
        DatosHost {
            nombre: self.nombre.texto.trim().to_string(),
            grupo_id,
            direccion: self.direccion.texto.trim().to_string(),
            puerto,
            usuario: if usuario.is_empty() {
                None
            } else {
                Some(usuario.to_string())
            },
            identidad_ref,
            salto_host_id,
            multiplexar: self.multiplexar,
            keepalive_seg: keepalive,
            opciones_extra: self.opciones.texto(),
            etiquetas: self
                .etiquetas
                .texto
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        }
    }

    pub fn sucio(&self) -> bool {
        self.datos() != self.original
    }

    fn campo_texto_mut(&mut self, campo: CampoFicha) -> Option<&mut CampoTexto> {
        match campo {
            CampoFicha::Nombre => Some(&mut self.nombre),
            CampoFicha::Direccion => Some(&mut self.direccion),
            CampoFicha::Puerto => Some(&mut self.puerto),
            CampoFicha::Etiquetas => Some(&mut self.etiquetas),
            CampoFicha::Usuario => Some(&mut self.usuario),
            CampoFicha::Keepalive => Some(&mut self.keepalive),
            _ => None,
        }
    }

    fn desplegable_mut(&mut self, campo: CampoFicha) -> Option<&mut Desplegable> {
        match campo {
            CampoFicha::Grupo => Some(&mut self.grupo),
            CampoFicha::Identidad => Some(&mut self.identidad),
            CampoFicha::Salto => Some(&mut self.salto),
            _ => None,
        }
    }

    fn es_casilla(campo: CampoFicha) -> bool {
        matches!(campo, CampoFicha::Multiplexar | CampoFicha::Mantener)
    }

    fn es_desplegable(campo: CampoFicha) -> bool {
        matches!(
            campo,
            CampoFicha::Grupo | CampoFicha::Identidad | CampoFicha::Salto
        )
    }

    fn avanzar(&mut self, paso: i32) {
        let posicion = ORDEN_CAMPOS
            .iter()
            .position(|campo| *campo == self.campo)
            .unwrap_or(0) as i32;
        let total = ORDEN_CAMPOS.len() as i32;
        let nueva = (posicion + paso).rem_euclid(total) as usize;
        self.campo = ORDEN_CAMPOS[nueva];
        self.sugerencias.clear();
        self.indice_sugerencia = 0;
    }

    fn actualizar_sugerencias(&mut self, disponibles: &[String]) {
        let token = self
            .etiquetas
            .texto
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_lowercase();
        let ya_puestas: Vec<String> = self
            .etiquetas
            .texto
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.sugerencias = disponibles
            .iter()
            .filter(|etiqueta| {
                etiqueta.starts_with(&token) && !ya_puestas.iter().any(|p| p == *etiqueta)
            })
            .take(6)
            .cloned()
            .collect();
        self.indice_sugerencia = 0;
    }

    fn aceptar_sugerencia(&mut self) -> bool {
        if self.sugerencias.is_empty() {
            return false;
        }
        let sugerencia = self.sugerencias[self.indice_sugerencia].clone();
        let texto = self.etiquetas.texto.clone();
        let mut tokens: Vec<String> = texto.split_whitespace().map(str::to_string).collect();
        if let Some(ultimo) = tokens.last_mut() {
            *ultimo = sugerencia;
        } else {
            tokens.push(sugerencia);
        }
        self.etiquetas = CampoTexto::nuevo(format!("{} ", tokens.join(" ")));
        self.sugerencias.clear();
        true
    }

    /// Procesa una tecla del formulario.
    pub fn manejar_tecla(&mut self, tecla: KeyEvent, disponibles: &[String]) -> AccionFicha {
        if let Some(campo) = self.desplegable_abierto {
            if let Some(desplegable) = self.desplegable_mut(campo) {
                match desplegable.manejar_tecla(&tecla) {
                    crate::ui::componentes::ResultadoDesplegable::Seleccionado(_) => {
                        self.desplegable_abierto = None;
                    }
                    crate::ui::componentes::ResultadoDesplegable::Cerrado => {
                        self.desplegable_abierto = None;
                    }
                    crate::ui::componentes::ResultadoDesplegable::SinCambio => {}
                }
            }
            return AccionFicha::Nada;
        }
        if tecla.modifiers.contains(KeyModifiers::CONTROL) {
            return match tecla.code {
                KeyCode::Char('s') => AccionFicha::Guardar,
                KeyCode::Char('t') => AccionFicha::Probar,
                _ => AccionFicha::Nada,
            };
        }
        match tecla.code {
            KeyCode::Esc => {
                if self.sucio() {
                    AccionFicha::DescartarConCambios
                } else {
                    AccionFicha::Cerrar
                }
            }
            KeyCode::Tab => {
                self.avanzar(1);
                AccionFicha::Nada
            }
            KeyCode::BackTab => {
                self.avanzar(-1);
                AccionFicha::Nada
            }
            KeyCode::Enter => {
                if self.campo == CampoFicha::Etiquetas && self.aceptar_sugerencia() {
                    return AccionFicha::Nada;
                }
                if Self::es_desplegable(self.campo) {
                    if let Some(desplegable) = self.desplegable_mut(self.campo) {
                        desplegable.abrir();
                        self.desplegable_abierto = Some(self.campo);
                    }
                    return AccionFicha::Nada;
                }
                self.editar(tecla, disponibles)
            }
            KeyCode::Up if self.campo == CampoFicha::Etiquetas && !self.sugerencias.is_empty() => {
                self.indice_sugerencia = self.indice_sugerencia.saturating_sub(1);
                AccionFicha::Nada
            }
            KeyCode::Down
                if self.campo == CampoFicha::Etiquetas && !self.sugerencias.is_empty() =>
            {
                if self.indice_sugerencia + 1 < self.sugerencias.len() {
                    self.indice_sugerencia += 1;
                }
                AccionFicha::Nada
            }
            KeyCode::Char(' ') if Self::es_casilla(self.campo) => {
                match self.campo {
                    CampoFicha::Multiplexar => self.multiplexar = !self.multiplexar,
                    CampoFicha::Mantener => {
                        self.mantener = !self.mantener;
                        if self.mantener && self.keepalive.texto.trim().is_empty() {
                            self.keepalive = CampoTexto::nuevo("30");
                        }
                    }
                    _ => {}
                }
                AccionFicha::Nada
            }
            _ => self.editar(tecla, disponibles),
        }
    }

    fn editar(&mut self, tecla: KeyEvent, disponibles: &[String]) -> AccionFicha {
        if self.campo == CampoFicha::Opciones {
            self.opciones.manejar_tecla(&tecla);
            return AccionFicha::Nada;
        }
        if let Some(campo) = self.campo_texto_mut(self.campo) {
            campo.manejar_tecla(&tecla);
            if self.campo == CampoFicha::Etiquetas {
                self.actualizar_sugerencias(disponibles);
            }
        }
        AccionFicha::Nada
    }
}

pub enum AccionDialogo {
    Nada,
    BorrarHost(i64),
    BorrarGrupo(i64),
    RenombrarGrupo { id: i64, nombre: String },
    CrearGrupo { nombre: String },
    MoverHost { host_id: i64, grupo_id: Option<i64> },
    CerrarSesion,
    CerrarSesionYAbrir(i64),
    Salir,
    DescartarFicha,
    InsertarInclude,
}

pub enum EntradaTextoAccion {
    CrearGrupo,
    RenombrarGrupo(i64),
}

pub enum Dialogo {
    Confirmar {
        titulo: String,
        lineas: Vec<String>,
        peligro: bool,
        accion: AccionDialogo,
    },
    HuellaDesconocida {
        host: String,
        tipo: String,
        huella: String,
        responder: oneshot::Sender<bool>,
    },
    HuellaCambiada {
        host: String,
        tipo: String,
        anterior: String,
        nueva: String,
        campo: CampoTexto,
        responder: oneshot::Sender<bool>,
    },
    Frase {
        host: String,
        intento: u8,
        campo: CampoTexto,
        responder: oneshot::Sender<Option<Zeroizing<String>>>,
    },
    MenuGrupo {
        seleccion: usize,
    },
    EntradaTexto {
        titulo: String,
        etiqueta: String,
        campo: CampoTexto,
        accion: EntradaTextoAccion,
    },
    MoverHost {
        host_id: i64,
        host_nombre: String,
        desplegable: Desplegable,
    },
    ResumenImportacion {
        titulo: String,
        lineas: Vec<String>,
    },
    ConflictoImportacion {
        nombre: String,
        restantes: usize,
    },
}

pub struct ImportacionPendiente {
    pub analisis: sshconfig::importar::Analisis,
    pub decisiones: HashMap<String, bool>,
    pub conflictos: Vec<String>,
    pub indice: usize,
}

pub struct SesionUI {
    pub host_id: i64,
    pub host_nombre: String,
    pub pantalla: conexion::Pantalla,
    pub identidad: String,
    pub iniciada: Instant,
    pub estado: EstadoSesion,
    pub cols: u16,
    pub filas: u16,
}

pub struct EntradaPaleta {
    pub etiqueta: String,
    pub categoria: &'static str,
    pub accion: AccionPaleta,
}

impl AsRef<str> for EntradaPaleta {
    fn as_ref(&self) -> &str {
        &self.etiqueta
    }
}

pub enum AccionPaleta {
    Conectar(i64),
    Editar(i64),
    Nuevo,
    Importar,
    Exportar,
    IrASesion,
}

pub struct PaletaCmd {
    pub consulta: CampoTexto,
    pub entradas: Vec<EntradaPaleta>,
    pub filtradas: Vec<usize>,
    pub seleccion: usize,
    matcher: Matcher,
}

impl PaletaCmd {
    pub fn recalcular(&mut self) {
        let patron = Pattern::parse(
            &self.consulta.texto,
            CaseMatching::Ignore,
            Normalization::Smart,
        );
        let referencias: Vec<&EntradaPaleta> = self.entradas.iter().collect();
        let coincidencias = patron.match_list(referencias, &mut self.matcher);
        self.filtradas = coincidencias
            .into_iter()
            .map(|(entrada, _)| {
                self.entradas
                    .iter()
                    .position(|otra| std::ptr::eq(otra, entrada))
                    .unwrap_or(0)
            })
            .collect();
        self.seleccion = 0;
    }

    pub fn entrada_seleccionada(&self) -> Option<&EntradaPaleta> {
        self.filtradas
            .get(self.seleccion)
            .and_then(|indice| self.entradas.get(*indice))
    }
}

pub struct App {
    pub rutas: Rutas,
    pub config: Config,
    pub tema: Tema,
    pub almacen: Almacen,
    pub runtime: tokio::runtime::Runtime,
    pub eventos_tx: mpsc::UnboundedSender<Evento>,
    eventos_rx: mpsc::UnboundedReceiver<Evento>,
    salir_flag: Arc<AtomicBool>,
    pub salir: bool,
    salir_pendiente: bool,
    abrir_al_cerrar: Option<i64>,
    pub vista: Vista,
    pub filtro: String,
    pub filtro_activo: bool,
    pub hosts: Vec<Host>,
    pub grupos: Vec<Grupo>,
    pub etiquetas: Vec<String>,
    pub filas: Vec<Fila>,
    pub seleccion: usize,
    pub desplazamiento: usize,
    pub columna_etiquetas: bool,
    pub ficha: Option<Ficha>,
    pub paleta: Option<PaletaCmd>,
    pub ayuda: bool,
    pub dialogo: Option<Dialogo>,
    importacion: Option<ImportacionPendiente>,
    pub sesion: Option<SesionUI>,
    comandos_sesion: Option<mpsc::UnboundedSender<ComandoConexion>>,
    pub host_conectando: Option<i64>,
    pub prefijo: (KeyCode, KeyModifiers),
    pub modo_prefijo: bool,
    pub mensaje: Option<Mensaje>,
    pub destello: Option<(i64, Instant)>,
    pub identidades: Identidades,
    pub contador_ticks: u64,
    sucio: bool,
    segundos_sesion: u64,
}

impl App {
    pub fn nuevo(
        rutas: Rutas,
        config: Config,
        tema: Tema,
        almacen: Almacen,
        runtime: tokio::runtime::Runtime,
        aviso_inicial: Option<String>,
    ) -> Result<Self> {
        let (eventos_tx, eventos_rx) = mpsc::unbounded_channel();
        let salir_flag = Arc::new(AtomicBool::new(false));
        let prefijo = crate::teclas::parsear_prefijo(&config.prefijo_escape)
            .map_err(|error| anyhow::anyhow!("prefijo_escape no válido: {error}"))?;
        let mut app = Self {
            rutas,
            config,
            tema,
            almacen,
            runtime,
            eventos_tx: eventos_tx.clone(),
            eventos_rx,
            salir_flag: salir_flag.clone(),
            salir: false,
            salir_pendiente: false,
            abrir_al_cerrar: None,
            vista: Vista::Hosts,
            filtro: String::new(),
            filtro_activo: false,
            hosts: Vec::new(),
            grupos: Vec::new(),
            etiquetas: Vec::new(),
            filas: Vec::new(),
            seleccion: 0,
            desplazamiento: 0,
            columna_etiquetas: false,
            ficha: None,
            paleta: None,
            ayuda: false,
            dialogo: None,
            importacion: None,
            sesion: None,
            comandos_sesion: None,
            host_conectando: None,
            prefijo,
            modo_prefijo: false,
            mensaje: None,
            destello: None,
            identidades: Identidades::default(),
            contador_ticks: 0,
            sucio: true,
            segundos_sesion: 0,
        };
        app.recargar_inventario()?;
        if let Some(aviso) = aviso_inicial {
            app.mensaje(aviso, true);
        }
        lanzar_hilo_teclas(eventos_tx.clone(), salir_flag.clone());
        lanzar_tick(&app.runtime, eventos_tx.clone());
        app.refrescar_identidades();
        Ok(app)
    }

    pub fn ejecutar(mut self) -> Result<()> {
        let mut terminal = crate::ui::iniciar_terminal()?;
        while !self.salir {
            if self.sucio {
                terminal.draw(|marco| crate::ui::dibujar(marco, &self))?;
                self.sucio = false;
            }
            match self.eventos_rx.blocking_recv() {
                Some(evento) => self.procesar(evento),
                None => break,
            }
        }
        crate::ui::restaurar_terminal();
        self.salir_flag.store(true, Ordering::Relaxed);
        self.almacen.cerrar()
    }

    fn procesar(&mut self, evento: Evento) {
        match evento {
            Evento::Tecla(tecla) => {
                self.sucio = true;
                self.procesar_tecla(tecla);
            }
            Evento::Redimension(cols, filas) => {
                self.sucio = true;
                let filas_pty = filas.saturating_sub(2).max(1);
                if let Some(sesion) = &mut self.sesion {
                    sesion.cols = cols;
                    sesion.filas = filas_pty;
                }
                if let Some(comandos) = &self.comandos_sesion {
                    let _ = comandos.send(ComandoConexion::Redimensionar(cols, filas_pty));
                }
            }
            Evento::Tick => self.tick(),
            Evento::Conexion(evento) => {
                self.sucio = true;
                self.evento_conexion(evento);
            }
            Evento::Identidades(identidades) => {
                self.sucio = true;
                self.identidades = identidades;
                self.refrescar_opciones_identidad();
            }
        }
    }

    fn tick(&mut self) {
        self.contador_ticks += 1;
        if let Some(mensaje) = &self.mensaje {
            if mensaje.creado.elapsed() > Duration::from_secs(4) {
                self.mensaje = None;
                self.sucio = true;
            }
        }
        if let Some((_, momento)) = &self.destello {
            if momento.elapsed() > Duration::from_millis(800) {
                self.destello = None;
                self.sucio = true;
            }
        }
        if self.filtro_activo && self.contador_ticks.is_multiple_of(3) {
            self.sucio = true;
        }
        if self.vista == Vista::Sesion {
            if let Some(sesion) = &self.sesion {
                let segundos = sesion.iniciada.elapsed().as_secs();
                if segundos != self.segundos_sesion {
                    self.segundos_sesion = segundos;
                    self.sucio = true;
                }
            }
        }
    }

    pub fn mensaje(&mut self, texto: impl Into<String>, error: bool) {
        self.mensaje = Some(Mensaje {
            texto: texto.into(),
            error,
            creado: Instant::now(),
        });
    }

    // ---------------------------------------------------------------- almacén

    pub fn recargar_inventario(&mut self) -> Result<()> {
        let seleccionado = self.host_seleccionado_id();
        self.hosts = self.almacen.listar_hosts()?;
        self.grupos = self.almacen.listar_grupos()?;
        self.etiquetas = self
            .almacen
            .listar_etiquetas()?
            .into_iter()
            .map(|etiqueta| etiqueta.nombre)
            .collect();
        self.reconstruir_filas();
        if let Some(id) = seleccionado {
            self.seleccionar_host_id(id);
        }
        Ok(())
    }

    fn host_seleccionado_id(&self) -> Option<i64> {
        match self.filas.get(self.seleccion) {
            Some(Fila::Host { indice }) => self.hosts.get(*indice).map(|host| host.id),
            _ => None,
        }
    }

    pub fn host_seleccionado(&self) -> Option<&Host> {
        match self.filas.get(self.seleccion) {
            Some(Fila::Host { indice }) => self.hosts.get(*indice),
            _ => None,
        }
    }

    pub fn seleccionar_host_id(&mut self, id: i64) {
        for (indice, fila) in self.filas.iter().enumerate() {
            if let Fila::Host { indice: host } = fila {
                if self.hosts.get(*host).is_some_and(|host| host.id == id) {
                    self.seleccion = indice;
                    return;
                }
            }
        }
    }

    pub fn reconstruir_filas(&mut self) {
        let consulta = if self.filtro.trim().is_empty() {
            None
        } else {
            Some(self.filtro.clone())
        };
        let mut por_grupo: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
        for (indice, host) in self.hosts.iter().enumerate() {
            if consulta
                .as_deref()
                .is_none_or(|texto| modelo::coincide(host, texto))
            {
                por_grupo.entry(host.grupo_id).or_default().push(indice);
            }
        }
        let mut filas = Vec::new();
        for grupo in &self.grupos {
            let indices = por_grupo.get(&Some(grupo.id)).cloned().unwrap_or_default();
            if consulta.is_some() && indices.is_empty() {
                continue;
            }
            let plegado = grupo.plegado && consulta.is_none();
            filas.push(Fila::Grupo {
                grupo_id: Some(grupo.id),
                nombre: grupo.nombre.clone(),
                plegado,
                total: indices.len(),
                filtrado: consulta.is_some(),
            });
            if !plegado {
                for indice in indices {
                    filas.push(Fila::Host { indice });
                }
            }
        }
        if let Some(indices) = por_grupo.get(&None) {
            if !indices.is_empty() {
                filas.push(Fila::Grupo {
                    grupo_id: None,
                    nombre: "sin grupo".to_string(),
                    plegado: false,
                    total: indices.len(),
                    filtrado: consulta.is_some(),
                });
                for indice in indices {
                    filas.push(Fila::Host { indice: *indice });
                }
            }
        }
        self.filas = filas;
        if self.seleccion >= self.filas.len() {
            self.seleccion = self.filas.len().saturating_sub(1);
        }
    }

    fn mover_seleccion(&mut self, delta: i32) {
        if self.filas.is_empty() {
            return;
        }
        let total = self.filas.len() as i32;
        let nueva = (self.seleccion as i32 + delta).clamp(0, total - 1);
        self.seleccion = nueva as usize;
    }

    fn grupo_fila_actual(&self) -> Option<i64> {
        match self.filas.get(self.seleccion) {
            Some(Fila::Grupo { grupo_id, .. }) => *grupo_id,
            Some(Fila::Host { indice }) => self.hosts.get(*indice).and_then(|h| h.grupo_id),
            None => None,
        }
    }

    fn alternar_plegado(&mut self) -> Result<()> {
        if let Some(grupo_id) = self.grupo_fila_actual() {
            if let Some(grupo) = self.grupos.iter().find(|g| g.id == grupo_id) {
                self.almacen.alternar_plegado(grupo_id, !grupo.plegado)?;
                self.recargar_inventario()?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- inventario

    fn tecla_hosts(&mut self, tecla: KeyEvent) {
        if self.filtro_activo {
            match tecla.code {
                KeyCode::Esc => {
                    self.filtro.clear();
                    self.filtro_activo = false;
                    self.reconstruir_filas();
                    return;
                }
                KeyCode::Enter => {
                    self.filtro_activo = false;
                    return;
                }
                KeyCode::Up => {
                    self.mover_seleccion(-1);
                    return;
                }
                KeyCode::Down => {
                    self.mover_seleccion(1);
                    return;
                }
                _ => {
                    if manejar_texto(&mut self.filtro, tecla) {
                        self.reconstruir_filas();
                    }
                    return;
                }
            }
        }
        match tecla.code {
            KeyCode::Char('j') | KeyCode::Down => self.mover_seleccion(1),
            KeyCode::Char('k') | KeyCode::Up => self.mover_seleccion(-1),
            KeyCode::PageDown => self.mover_seleccion(10),
            KeyCode::PageUp => self.mover_seleccion(-10),
            KeyCode::Home => self.seleccion = 0,
            KeyCode::End => self.seleccion = self.filas.len().saturating_sub(1),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('l') | KeyCode::Right => {
                if let Err(error) = self.alternar_plegado() {
                    self.mensaje(error.to_string(), true);
                }
            }
            KeyCode::Char('/') => {
                self.filtro_activo = true;
                self.filtro.clear();
                self.reconstruir_filas();
            }
            KeyCode::Esc => {
                if !self.filtro.is_empty() {
                    self.filtro.clear();
                    self.filtro_activo = false;
                    self.reconstruir_filas();
                }
            }
            KeyCode::Tab => self.columna_etiquetas = !self.columna_etiquetas,
            KeyCode::Enter => {
                if let Some(host) = self.host_seleccionado() {
                    self.conectar(host.id);
                }
            }
            KeyCode::Char('e') => {
                if let Some(id) = self.host_seleccionado().map(|host| host.id) {
                    self.abrir_ficha(Some(id));
                }
            }
            KeyCode::Char('n') => {
                let grupo = self.grupo_fila_actual();
                self.abrir_ficha_nueva(grupo);
            }
            KeyCode::Char('x') => {
                if let Some(host) = self.host_seleccionado() {
                    let id = host.id;
                    let nombre = host.nombre.clone();
                    let dependientes = self.almacen.dependientes_de_salto(id).unwrap_or(0);
                    let mut lineas = vec![format!("¿Borrar el host «{nombre}»?")];
                    if dependientes > 0 {
                        lineas.push(format!(
                            "{dependientes} host(s) lo usan como salto y quedarán sin salto."
                        ));
                    }
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo: "BORRAR HOST".to_string(),
                        lineas,
                        peligro: true,
                        accion: AccionDialogo::BorrarHost(id),
                    });
                }
            }
            KeyCode::Char('g') => self.abrir_menu_grupo(),
            KeyCode::Char('?') => self.ayuda = true,
            KeyCode::Char('I') => self.importar_ssh_config(),
            KeyCode::Char('E') => self.exportar(true, false),
            KeyCode::Char('q') => self.intentar_salir(),
            _ => {}
        }
    }

    fn intentar_salir(&mut self) {
        if self.sesion.is_some() {
            self.dialogo = Some(Dialogo::Confirmar {
                titulo: "SALIR".to_string(),
                lineas: vec![
                    "Hay una sesión abierta: se cerrará el canal remoto.".to_string(),
                    "¿Salir de MAGI?".to_string(),
                ],
                peligro: true,
                accion: AccionDialogo::Salir,
            });
        } else {
            self.salir = true;
        }
    }

    // ------------------------------------------------------------------ conexión

    fn conectar(&mut self, host_id: i64) {
        if let Some(sesion) = &self.sesion {
            if sesion.host_id == host_id {
                self.vista = Vista::Sesion;
                return;
            }
            let nombre = self
                .sesion
                .as_ref()
                .map(|sesion| sesion.host_nombre.clone())
                .unwrap_or_default();
            self.dialogo = Some(Dialogo::Confirmar {
                titulo: "CERRAR SESIÓN".to_string(),
                lineas: vec![
                    format!("Ya hay una sesión abierta con «{nombre}»."),
                    "¿Cerrarla y abrir la nueva?".to_string(),
                ],
                peligro: true,
                accion: AccionDialogo::CerrarSesionYAbrir(host_id),
            });
            return;
        }
        if self.comandos_sesion.is_some() {
            self.mensaje("ya hay una conexión en curso", true);
            return;
        }
        match self.almacen.obtener_host(host_id) {
            Ok(host) => self.iniciar_conexion(host, false),
            Err(error) => self.mensaje(error.to_string(), true),
        }
    }

    fn iniciar_conexion(&mut self, host: Host, solo_prueba: bool) {
        let todos_los_hosts: HashMap<i64, Host> = self
            .hosts
            .iter()
            .cloned()
            .map(|host| (host.id, host))
            .collect();
        let (cols, filas) = if solo_prueba {
            (80, 24)
        } else {
            let tamano = crossterm::terminal::size().unwrap_or((80, 24));
            (tamano.0, tamano.1.saturating_sub(2).max(1))
        };
        let usuario_local = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "root".to_string());
        let plan = PlanConexion {
            host: host.clone(),
            todos_los_hosts,
            known_hosts: self.rutas.fichero_known_hosts(),
            dir_ssh: self.rutas.dir_ssh(),
            hogar: self.rutas.hogar.clone(),
            usuario_local,
            solo_prueba,
            cols,
            filas,
        };
        let comandos = conexion::lanzar(&self.runtime, plan, self.eventos_tx.clone());
        self.comandos_sesion = Some(comandos);
        self.host_conectando = Some(host.id);
        if !solo_prueba {
            self.mensaje(format!("conectando con «{}»…", host.nombre), false);
        }
        self.sucio = true;
    }

    fn evento_conexion(&mut self, evento: EventoConexion) {
        match evento {
            EventoConexion::Estado { host_id, estado } => {
                self.host_conectando = Some(host_id);
                match estado {
                    EstadoSesion::Autenticando => {
                        if self.ficha.is_some() {
                            // La prueba de conexión no cambia de vista.
                        }
                    }
                    EstadoSesion::VerificandoHuella => {}
                    _ => {}
                }
            }
            EventoConexion::HuellaDesconocida {
                host,
                tipo,
                huella,
                responder,
            } => {
                self.dialogo = Some(Dialogo::HuellaDesconocida {
                    host,
                    tipo,
                    huella,
                    responder,
                });
            }
            EventoConexion::HuellaCambiada {
                host,
                tipo,
                anterior,
                nueva,
                responder,
            } => {
                self.dialogo = Some(Dialogo::HuellaCambiada {
                    host,
                    tipo,
                    anterior,
                    nueva,
                    campo: CampoTexto::default(),
                    responder,
                });
            }
            EventoConexion::PideFrase {
                host,
                intento,
                responder,
            } => {
                self.dialogo = Some(Dialogo::Frase {
                    host,
                    intento,
                    campo: CampoTexto::default(),
                    responder,
                });
            }
            EventoConexion::Abierta {
                host_id,
                pantalla,
                identidad,
                cols,
                filas,
            } => {
                let nombre = self
                    .hosts
                    .iter()
                    .find(|host| host.id == host_id)
                    .map(|host| host.nombre.clone())
                    .unwrap_or_default();
                if let Err(error) = self.almacen.marcar_conexion(host_id) {
                    warn!("no se pudo actualizar el estado del host: {error}");
                }
                self.sesion = Some(SesionUI {
                    host_id,
                    host_nombre: nombre.clone(),
                    pantalla,
                    identidad,
                    iniciada: Instant::now(),
                    estado: EstadoSesion::Abierta,
                    cols,
                    filas,
                });
                self.host_conectando = None;
                self.vista = Vista::Sesion;
                self.destello = Some((host_id, Instant::now()));
                if let Err(error) = self.recargar_inventario() {
                    warn!("no se pudo recargar el inventario: {error}");
                }
                self.mensaje(format!("sesión abierta con «{nombre}»"), false);
            }
            EventoConexion::Pantalla => {
                if self.vista == Vista::Sesion {
                    self.sucio = true;
                }
            }
            EventoConexion::PruebaOk { host_id, identidad } => {
                if host_id != 0 {
                    let _ = self.almacen.marcar_estado(host_id, Some(UltimoEstado::Ok));
                    self.destello = Some((host_id, Instant::now()));
                }
                self.host_conectando = None;
                self.comandos_sesion = None;
                let _ = self.recargar_inventario();
                self.mensaje(format!("conexión correcta · {identidad}"), false);
            }
            EventoConexion::Cerrada { host_id, motivo } => {
                self.sesion = None;
                self.comandos_sesion = None;
                self.host_conectando = None;
                if self.vista == Vista::Sesion {
                    self.vista = Vista::Hosts;
                }
                let nombre = self
                    .hosts
                    .iter()
                    .find(|host| host.id == host_id)
                    .map(|host| host.nombre.clone())
                    .unwrap_or_default();
                match motivo {
                    Some(motivo) => self.mensaje(format!("sesión cerrada: {motivo}"), false),
                    None => self.mensaje(format!("sesión cerrada con «{nombre}»"), false),
                }
                let _ = self.recargar_inventario();
                if self.salir_pendiente {
                    self.salir = true;
                } else if let Some(host_id) = self.abrir_al_cerrar.take() {
                    self.conectar(host_id);
                }
            }
            EventoConexion::Error { host_id, motivo } => {
                if host_id != 0 {
                    let _ = self
                        .almacen
                        .marcar_estado(host_id, Some(UltimoEstado::Error));
                    self.destello = Some((host_id, Instant::now()));
                }
                if self
                    .sesion
                    .as_ref()
                    .is_some_and(|sesion| sesion.host_id == host_id)
                {
                    self.sesion = None;
                }
                self.comandos_sesion = None;
                self.host_conectando = None;
                if self.vista == Vista::Sesion {
                    self.vista = Vista::Hosts;
                }
                let _ = self.recargar_inventario();
                self.mensaje(motivo, true);
            }
            EventoConexion::Cancelada { .. } => {
                self.comandos_sesion = None;
                self.host_conectando = None;
                self.mensaje("conexión cancelada", false);
            }
        }
    }

    fn cerrar_sesion_actual(&mut self) {
        if let Some(comandos) = &self.comandos_sesion {
            let _ = comandos.send(ComandoConexion::Cerrar);
        }
    }

    fn enviar_a_sesion(&mut self, bytes: Vec<u8>) {
        if let Some(comandos) = &self.comandos_sesion {
            let _ = comandos.send(ComandoConexion::Teclas(bytes));
        }
    }

    fn tecla_sesion(&mut self, tecla: KeyEvent) {
        if self.modo_prefijo {
            self.modo_prefijo = false;
            if crate::teclas::es_prefijo(&tecla, self.prefijo) {
                if let Some(bytes) = crate::teclas::bytes_de_tecla(tecla) {
                    self.enviar_a_sesion(bytes);
                }
                return;
            }
            match tecla.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    if let Some(sesion) = &mut self.sesion {
                        sesion.estado = EstadoSesion::EnSegundoPlano;
                    }
                    self.vista = Vista::Hosts;
                    self.mensaje("sesión en segundo plano", false);
                }
                KeyCode::Char('x') => {
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo: "CERRAR SESIÓN".to_string(),
                        lineas: vec!["¿Cerrar la sesión remota?".to_string()],
                        peligro: true,
                        accion: AccionDialogo::CerrarSesion,
                    });
                }
                _ => {}
            }
            return;
        }
        if crate::teclas::es_prefijo(&tecla, self.prefijo) {
            self.modo_prefijo = true;
            return;
        }
        if tecla.kind == KeyEventKind::Release {
            return;
        }
        if let Some(bytes) = crate::teclas::bytes_de_tecla(tecla) {
            self.enviar_a_sesion(bytes);
        }
    }

    // -------------------------------------------------------------------- ficha

    fn abrir_ficha(&mut self, host_id: Option<i64>) {
        let host = match host_id {
            Some(id) => match self.almacen.obtener_host(id) {
                Ok(host) => host,
                Err(error) => {
                    self.mensaje(error.to_string(), true);
                    return;
                }
            },
            None => {
                self.mensaje("no hay host seleccionado", true);
                return;
            }
        };
        self.ficha = Some(self.construir_ficha(Some(&host), host.grupo_id));
        self.vista = Vista::Ficha;
        self.refrescar_identidades();
    }

    fn abrir_ficha_nueva(&mut self, grupo_id: Option<i64>) {
        self.ficha = Some(self.construir_ficha(None, grupo_id));
        self.vista = Vista::Ficha;
        self.refrescar_identidades();
    }

    fn construir_ficha(&self, host: Option<&Host>, grupo_id: Option<i64>) -> Ficha {
        let datos = host
            .map(|host| DatosHost {
                nombre: host.nombre.clone(),
                grupo_id: host.grupo_id,
                direccion: host.direccion.clone(),
                puerto: host.puerto,
                usuario: host.usuario.clone(),
                identidad_ref: host.identidad_ref.clone(),
                salto_host_id: host.salto_host_id,
                multiplexar: host.multiplexar,
                keepalive_seg: host.keepalive_seg,
                opciones_extra: host.opciones_extra.clone(),
                etiquetas: host.etiquetas.clone(),
            })
            .unwrap_or_else(|| DatosHost {
                grupo_id,
                ..DatosHost::default()
            });
        let mut grupo = Desplegable::nuevo(opciones_grupo(&self.grupos));
        grupo.seleccionar_valor(&match datos.grupo_id {
            Some(id) => ValorOpcion::Grupo(id),
            None => ValorOpcion::Ninguno,
        });
        let mut identidad = Desplegable::nuevo(self.opciones_identidad());
        identidad.seleccionar_valor(&ValorOpcion::Identidad(datos.identidad_ref.clone()));
        if !identidad
            .opciones
            .iter()
            .any(|opcion| opcion.valor == ValorOpcion::Identidad(datos.identidad_ref.clone()))
        {
            identidad.opciones.insert(
                1,
                Opcion {
                    etiqueta: etiqueta_identidad(&datos.identidad_ref),
                    valor: ValorOpcion::Identidad(datos.identidad_ref.clone()),
                },
            );
        }
        let mut salto = Desplegable::nuevo(opciones_salto(&self.hosts, host.map(|h| h.id)));
        salto.seleccionar_valor(&match datos.salto_host_id {
            Some(id) => ValorOpcion::Salto(id),
            None => ValorOpcion::Ninguno,
        });
        Ficha {
            host_id: host.map(|host| host.id),
            original: datos.clone(),
            titulo: if datos.nombre.is_empty() {
                "nuevo".to_string()
            } else {
                datos.nombre.clone()
            },
            campo: CampoFicha::Nombre,
            nombre: CampoTexto::nuevo(datos.nombre.clone()),
            direccion: CampoTexto::nuevo(datos.direccion.clone()),
            puerto: CampoTexto::nuevo(datos.puerto.to_string()),
            etiquetas: CampoTexto::nuevo(datos.etiquetas.join(" ")),
            usuario: CampoTexto::nuevo(datos.usuario.clone().unwrap_or_default()),
            keepalive: CampoTexto::nuevo(
                datos
                    .keepalive_seg
                    .map(|valor| valor.to_string())
                    .unwrap_or_default(),
            ),
            opciones: AreaTexto::nuevo(&datos.opciones_extra),
            grupo,
            identidad,
            salto,
            multiplexar: datos.multiplexar,
            mantener: datos.keepalive_seg.is_some(),
            desplegable_abierto: None,
            sugerencias: Vec::new(),
            indice_sugerencia: 0,
        }
    }

    fn opciones_identidad(&self) -> Vec<Opcion> {
        let mut opciones = vec![Opcion {
            etiqueta: "auto".to_string(),
            valor: ValorOpcion::Identidad(IdentidadRef::Auto),
        }];
        if let Some(agente) = &self.identidades.agente {
            for clave in agente {
                let etiqueta = if clave.comentario.is_empty() {
                    format!("agente · {} · {}", clave.tipo, acorta_huella(&clave.huella))
                } else {
                    format!(
                        "agente · {} · {} · {}",
                        clave.comentario,
                        clave.tipo,
                        acorta_huella(&clave.huella)
                    )
                };
                opciones.push(Opcion {
                    etiqueta,
                    valor: ValorOpcion::Identidad(IdentidadRef::Agente(clave.huella.clone())),
                });
            }
        }
        for clave in &self.identidades.ficheros {
            opciones.push(Opcion {
                etiqueta: format!("fichero · {}", clave.ruta.display()),
                valor: ValorOpcion::Identidad(IdentidadRef::Fichero(
                    clave.ruta.display().to_string(),
                )),
            });
        }
        opciones
    }

    fn refrescar_opciones_identidad(&mut self) {
        let opciones = self.opciones_identidad();
        if let Some(ficha) = &mut self.ficha {
            let seleccion = ficha.identidad.valor_seleccionado();
            ficha.identidad.opciones = opciones;
            ficha.identidad.seleccionar_valor(&seleccion);
        }
    }

    fn refrescar_identidades(&mut self) {
        let dir_ssh = self.rutas.dir_ssh();
        let tx = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            let identidades = crate::identidades::escanear(&dir_ssh).await;
            let _ = tx.send(Evento::Identidades(identidades));
        });
    }

    fn tecla_ficha(&mut self, tecla: KeyEvent) {
        let disponibles = self.etiquetas.clone();
        let Some(ficha) = &mut self.ficha else {
            return;
        };
        match ficha.manejar_tecla(tecla, &disponibles) {
            AccionFicha::Nada => {}
            AccionFicha::Guardar => self.guardar_ficha(),
            AccionFicha::Probar => self.probar_ficha(),
            AccionFicha::Cerrar => {
                self.ficha = None;
                self.vista = Vista::Hosts;
            }
            AccionFicha::DescartarConCambios => {
                self.dialogo = Some(Dialogo::Confirmar {
                    titulo: "DESCARTAR CAMBIOS".to_string(),
                    lineas: vec!["Hay cambios sin guardar. ¿Descartarlos?".to_string()],
                    peligro: true,
                    accion: AccionDialogo::DescartarFicha,
                });
            }
        }
    }

    fn validar_ficha(&self, datos: &DatosHost, host_id: Option<i64>) -> Result<(), String> {
        modelo::validar_nombre(&datos.nombre)?;
        if self
            .almacen
            .existe_nombre_host(&datos.nombre, host_id)
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "ya existe un host con el nombre «{}»",
                datos.nombre
            ));
        }
        modelo::validar_direccion(&datos.direccion)?;
        if datos.puerto == 0 {
            return Err("el puerto debe estar entre 1 y 65535".to_string());
        }
        modelo::validar_usuario(datos.usuario.as_deref())?;
        modelo::validar_keepalive(datos.keepalive_seg)?;
        modelo::validar_opciones_extra(&datos.opciones_extra)?;
        let mapa: HashMap<i64, Host> = self
            .hosts
            .iter()
            .cloned()
            .map(|host| (host.id, host))
            .collect();
        modelo::validar_salto(host_id, datos.salto_host_id, &mapa)?;
        Ok(())
    }

    fn guardar_ficha(&mut self) {
        let Some(ficha) = &self.ficha else {
            return;
        };
        let host_id = ficha.host_id;
        let datos = ficha.datos();
        if let Err(motivo) = self.validar_ficha(&datos, host_id) {
            self.mensaje(motivo, true);
            return;
        }
        let resultado = match host_id {
            Some(id) => self.almacen.actualizar_host(id, &datos).map(|_| id),
            None => self.almacen.crear_host(&datos, Origen::Manual),
        };
        match resultado {
            Ok(id) => {
                let aviso_clave = match &datos.identidad_ref {
                    IdentidadRef::Agente(huella) => {
                        let cargada = self
                            .identidades
                            .agente
                            .as_ref()
                            .is_some_and(|agente| agente.iter().any(|c| &c.huella == huella));
                        if cargada {
                            None
                        } else {
                            Some(format!(
                                "la clave {} no está en el agente ahora mismo",
                                acorta_huella(huella)
                            ))
                        }
                    }
                    _ => None,
                };
                self.ficha = None;
                self.vista = Vista::Hosts;
                if let Err(error) = self.recargar_inventario() {
                    self.mensaje(error.to_string(), true);
                    return;
                }
                self.seleccionar_host_id(id);
                let mut texto = "host guardado".to_string();
                if let Some(aviso) = aviso_clave {
                    texto.push_str(" · ");
                    texto.push_str(&aviso);
                }
                self.mensaje(texto, false);
                if self.config.exportar_al_guardar {
                    self.exportar(true, true);
                }
            }
            Err(error) => self.mensaje(error.to_string(), true),
        }
    }

    fn probar_ficha(&mut self) {
        if self.sesion.is_some() {
            self.mensaje("cierra la sesión activa antes de probar la conexión", true);
            return;
        }
        if self.comandos_sesion.is_some() {
            self.mensaje("ya hay una conexión en curso", true);
            return;
        }
        let Some(ficha) = &self.ficha else {
            return;
        };
        let host_id = ficha.host_id.unwrap_or(0);
        let datos = ficha.datos();
        if modelo::validar_direccion(&datos.direccion).is_err() || datos.puerto == 0 {
            self.mensaje(
                "dirección y puerto válidos son necesarios para probar",
                true,
            );
            return;
        }
        let host = Host {
            id: host_id,
            nombre: if datos.nombre.is_empty() {
                "(sin guardar)".to_string()
            } else {
                datos.nombre.clone()
            },
            grupo_id: datos.grupo_id,
            direccion: datos.direccion.clone(),
            puerto: datos.puerto,
            usuario: datos.usuario.clone(),
            identidad_ref: datos.identidad_ref.clone(),
            salto_host_id: datos.salto_host_id,
            multiplexar: datos.multiplexar,
            keepalive_seg: datos.keepalive_seg,
            opciones_extra: datos.opciones_extra.clone(),
            origen: Origen::Manual,
            ultimo_estado: None,
            ultima_conexion_en: None,
            creado_en: fecha_ahora(),
            actualizado_en: fecha_ahora(),
            etiquetas: datos.etiquetas.clone(),
            grupo_nombre: None,
            salto_nombre: None,
        };
        self.mensaje("probando la conexión…", false);
        self.iniciar_conexion(host, true);
    }

    // -------------------------------------------------------------------- grupos

    fn abrir_menu_grupo(&mut self) {
        self.dialogo = Some(Dialogo::MenuGrupo { seleccion: 0 });
    }

    fn ejecutar_menu_grupo(&mut self, seleccion: usize) {
        match seleccion {
            0 => {
                self.dialogo = Some(Dialogo::EntradaTexto {
                    titulo: "NUEVO GRUPO".to_string(),
                    etiqueta: "Nombre".to_string(),
                    campo: CampoTexto::default(),
                    accion: EntradaTextoAccion::CrearGrupo,
                });
            }
            1 => {
                let Some(grupo_id) = self.grupo_fila_actual() else {
                    self.mensaje("selecciona un grupo o un host con grupo", true);
                    self.dialogo = None;
                    return;
                };
                let nombre = self
                    .grupos
                    .iter()
                    .find(|grupo| grupo.id == grupo_id)
                    .map(|grupo| grupo.nombre.clone())
                    .unwrap_or_default();
                self.dialogo = Some(Dialogo::EntradaTexto {
                    titulo: "RENOMBRAR GRUPO".to_string(),
                    etiqueta: "Nombre".to_string(),
                    campo: CampoTexto::nuevo(nombre),
                    accion: EntradaTextoAccion::RenombrarGrupo(grupo_id),
                });
            }
            2 => {
                let Some(host) = self.host_seleccionado() else {
                    self.mensaje("selecciona un host para moverlo", true);
                    self.dialogo = None;
                    return;
                };
                let host_id = host.id;
                let host_nombre = host.nombre.clone();
                let mut desplegable = Desplegable::nuevo(opciones_grupo(&self.grupos));
                desplegable.seleccionar_valor(&match host.grupo_id {
                    Some(id) => ValorOpcion::Grupo(id),
                    None => ValorOpcion::Ninguno,
                });
                self.dialogo = Some(Dialogo::MoverHost {
                    host_id,
                    host_nombre,
                    desplegable,
                });
            }
            3 => {
                let Some(grupo_id) = self.grupo_fila_actual() else {
                    self.mensaje("selecciona un grupo para borrarlo", true);
                    self.dialogo = None;
                    return;
                };
                let nombre = self
                    .grupos
                    .iter()
                    .find(|grupo| grupo.id == grupo_id)
                    .map(|grupo| grupo.nombre.clone())
                    .unwrap_or_default();
                let hosts = self.almacen.hosts_en_grupo(grupo_id).unwrap_or(0);
                self.dialogo = Some(Dialogo::Confirmar {
                    titulo: "BORRAR GRUPO".to_string(),
                    lineas: vec![
                        format!("¿Borrar el grupo «{nombre}»?"),
                        format!("{hosts} host(s) pasarán a «sin grupo»."),
                    ],
                    peligro: true,
                    accion: AccionDialogo::BorrarGrupo(grupo_id),
                });
            }
            4 => self.mover_grupo(-1),
            5 => self.mover_grupo(1),
            _ => {}
        }
    }

    fn mover_grupo(&mut self, direccion: i8) {
        self.dialogo = None;
        let Some(grupo_id) = self.grupo_fila_actual() else {
            self.mensaje("selecciona un grupo para reordenarlo", true);
            return;
        };
        if let Err(error) = self.almacen.mover_grupo(grupo_id, direccion) {
            self.mensaje(error.to_string(), true);
            return;
        }
        if let Err(error) = self.recargar_inventario() {
            self.mensaje(error.to_string(), true);
        }
    }

    // ------------------------------------------------------------- import/export

    fn importar_ssh_config(&mut self) {
        let ruta = self.rutas.fichero_ssh_config();
        match sshconfig::importar::analizar_fichero(&ruta, &self.rutas.hogar) {
            Ok(analisis) => self.iniciar_importacion(analisis),
            Err(error) => self.mensaje(error.to_string(), true),
        }
    }

    fn iniciar_importacion(&mut self, analisis: sshconfig::importar::Analisis) {
        let conflictos = match sshconfig::importar::conflictos(self.almacen.conexion(), &analisis) {
            Ok(conflictos) => conflictos,
            Err(error) => {
                self.mensaje(error.to_string(), true);
                return;
            }
        };
        if conflictos.is_empty() {
            let decisiones = HashMap::new();
            self.aplicar_importacion(analisis, decisiones);
            return;
        }
        self.importacion = Some(ImportacionPendiente {
            analisis,
            decisiones: HashMap::new(),
            conflictos,
            indice: 0,
        });
        self.mostrar_conflicto_actual();
    }

    fn mostrar_conflicto_actual(&mut self) {
        let Some(importacion) = &self.importacion else {
            return;
        };
        if importacion.indice >= importacion.conflictos.len() {
            let importacion = self.importacion.take().expect("importación pendiente");
            self.aplicar_importacion(importacion.analisis, importacion.decisiones);
            return;
        }
        let nombre = importacion.conflictos[importacion.indice].clone();
        self.dialogo = Some(Dialogo::ConflictoImportacion {
            nombre,
            restantes: importacion.conflictos.len() - importacion.indice,
        });
    }

    fn aplicar_importacion(
        &mut self,
        analisis: sshconfig::importar::Analisis,
        decisiones: HashMap<String, bool>,
    ) {
        match sshconfig::importar::aplicar(self.almacen.conexion(), &analisis, &decisiones) {
            Ok(resumen) => {
                let mut lineas = vec![
                    format!("Importados: {}", resumen.importados),
                    format!("Sobrescritos: {}", resumen.sobrescritos),
                    format!(
                        "Omitidos: {}",
                        resumen.omitidos.len() + analisis.avisos.len()
                    ),
                ];
                for omitido in resumen.omitidos.iter().take(12) {
                    lineas.push(format!("· {}: {}", omitido.descripcion, omitido.motivo));
                }
                for aviso in analisis.avisos.iter().take(6) {
                    lineas.push(format!("· aviso: {aviso}"));
                }
                if let Err(error) = self.recargar_inventario() {
                    self.mensaje(error.to_string(), true);
                    return;
                }
                self.dialogo = Some(Dialogo::ResumenImportacion {
                    titulo: "RESUMEN DE IMPORTACIÓN".to_string(),
                    lineas,
                });
            }
            Err(error) => self.mensaje(error.to_string(), true),
        }
    }

    fn exportar(&mut self, comprobar_include: bool, silencioso: bool) {
        match sshconfig::exportar::exportar(self.almacen.conexion(), &self.rutas.dir_ssh()) {
            Ok(resultado) => {
                if !silencioso {
                    self.mensaje(
                        format!(
                            "exportados {} hosts a {}",
                            resultado.hosts,
                            resultado.ruta.display()
                        ),
                        false,
                    );
                }
                if comprobar_include && !resultado.include_presente {
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo: "INCLUDE".to_string(),
                        lineas: vec![
                            "~/.ssh/config no incluye todavía magi_config.".to_string(),
                            "¿Añadir «Include ~/.ssh/magi_config» al principio?".to_string(),
                            "(se guardará una copia config.bak-<fecha>)".to_string(),
                        ],
                        peligro: false,
                        accion: AccionDialogo::InsertarInclude,
                    });
                }
            }
            Err(error) => self.mensaje(error.to_string(), true),
        }
    }

    // -------------------------------------------------------------------- paleta

    fn abrir_paleta(&mut self) {
        let mut entradas = Vec::new();
        for host in &self.hosts {
            entradas.push(EntradaPaleta {
                etiqueta: format!("conectar · {}", host.nombre),
                categoria: "host",
                accion: AccionPaleta::Conectar(host.id),
            });
            entradas.push(EntradaPaleta {
                etiqueta: format!("editar host · {}", host.nombre),
                categoria: "acción",
                accion: AccionPaleta::Editar(host.id),
            });
        }
        entradas.push(EntradaPaleta {
            etiqueta: "nuevo host".to_string(),
            categoria: "acción",
            accion: AccionPaleta::Nuevo,
        });
        entradas.push(EntradaPaleta {
            etiqueta: "importar ~/.ssh/config".to_string(),
            categoria: "acción",
            accion: AccionPaleta::Importar,
        });
        entradas.push(EntradaPaleta {
            etiqueta: "exportar magi_config".to_string(),
            categoria: "acción",
            accion: AccionPaleta::Exportar,
        });
        entradas.push(EntradaPaleta {
            etiqueta: "ir a sesión".to_string(),
            categoria: "acción",
            accion: AccionPaleta::IrASesion,
        });
        let mut paleta = PaletaCmd {
            consulta: CampoTexto::default(),
            entradas,
            filtradas: Vec::new(),
            seleccion: 0,
            matcher: Matcher::new(ConfigNucleo::DEFAULT),
        };
        paleta.recalcular();
        self.paleta = Some(paleta);
    }

    fn tecla_paleta(&mut self, tecla: KeyEvent) {
        let Some(paleta) = &mut self.paleta else {
            return;
        };
        match tecla.code {
            KeyCode::Esc => self.paleta = None,
            KeyCode::Enter => {
                let accion = paleta.entrada_seleccionada().map(|entrada| &entrada.accion);
                match accion {
                    Some(AccionPaleta::Conectar(id)) => {
                        let id = *id;
                        self.paleta = None;
                        self.conectar(id);
                    }
                    Some(AccionPaleta::Editar(id)) => {
                        let id = *id;
                        self.paleta = None;
                        self.abrir_ficha(Some(id));
                    }
                    Some(AccionPaleta::Nuevo) => {
                        self.paleta = None;
                        self.abrir_ficha_nueva(None);
                    }
                    Some(AccionPaleta::Importar) => {
                        self.paleta = None;
                        self.importar_ssh_config();
                    }
                    Some(AccionPaleta::Exportar) => {
                        self.paleta = None;
                        self.exportar(true, false);
                    }
                    Some(AccionPaleta::IrASesion) => {
                        self.paleta = None;
                        self.ir_a_sesion();
                    }
                    None => {}
                }
            }
            KeyCode::Up => {
                paleta.seleccion = paleta.seleccion.saturating_sub(1);
            }
            KeyCode::Down => {
                if paleta.seleccion + 1 < paleta.filtradas.len() {
                    paleta.seleccion += 1;
                }
            }
            _ => {
                if paleta.consulta.manejar_tecla(&tecla) {
                    paleta.recalcular();
                }
            }
        }
    }

    fn ir_a_hosts(&mut self) {
        self.vista = Vista::Hosts;
        self.ficha = None;
        self.paleta = None;
    }

    fn ir_a_sesion(&mut self) {
        if self.sesion.is_some() {
            self.vista = Vista::Sesion;
        } else {
            self.mensaje("no hay sesión activa", true);
        }
    }

    // -------------------------------------------------------------------- teclas

    fn procesar_tecla(&mut self, tecla: KeyEvent) {
        if tecla.kind == KeyEventKind::Release {
            return;
        }
        self.mensaje = None;
        if self.dialogo.is_some() {
            self.tecla_dialogo(tecla);
            return;
        }
        if self.paleta.is_some() {
            self.tecla_paleta(tecla);
            return;
        }
        if self.ayuda {
            if matches!(
                tecla.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.ayuda = false;
            }
            return;
        }
        if self.vista == Vista::Sesion {
            self.tecla_sesion(tecla);
            return;
        }
        match tecla.code {
            KeyCode::F(2) => {
                if self.ficha.as_ref().is_some_and(|ficha| ficha.sucio()) {
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo: "DESCARTAR CAMBIOS".to_string(),
                        lineas: vec!["Hay cambios sin guardar. ¿Descartarlos?".to_string()],
                        peligro: true,
                        accion: AccionDialogo::DescartarFicha,
                    });
                } else {
                    self.ir_a_hosts();
                }
                return;
            }
            KeyCode::F(3) => {
                if self.ficha.as_ref().is_some_and(|ficha| ficha.sucio()) {
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo: "DESCARTAR CAMBIOS".to_string(),
                        lineas: vec!["Hay cambios sin guardar. ¿Descartarlos?".to_string()],
                        peligro: true,
                        accion: AccionDialogo::DescartarFicha,
                    });
                } else {
                    self.ir_a_sesion();
                }
                return;
            }
            KeyCode::F(1) | KeyCode::F(4) | KeyCode::F(5) | KeyCode::F(6) | KeyCode::F(7) => {
                self.mensaje("vista no disponible en esta fase", true);
                return;
            }
            KeyCode::Char('p') if tecla.modifiers.contains(KeyModifiers::CONTROL) => {
                self.abrir_paleta();
                return;
            }
            _ => {}
        }
        match self.vista {
            Vista::Hosts => self.tecla_hosts(tecla),
            Vista::Ficha => self.tecla_ficha(tecla),
            Vista::Sesion => {}
        }
    }

    fn tecla_dialogo(&mut self, tecla: KeyEvent) {
        let Some(dialogo) = self.dialogo.take() else {
            return;
        };
        match dialogo {
            Dialogo::Confirmar {
                titulo,
                lineas,
                peligro,
                accion,
            } => match tecla.code {
                KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                    self.ejecutar_accion(accion);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
                _ => {
                    self.dialogo = Some(Dialogo::Confirmar {
                        titulo,
                        lineas,
                        peligro,
                        accion,
                    });
                }
            },
            Dialogo::HuellaDesconocida {
                host,
                tipo,
                huella,
                responder,
            } => match tecla.code {
                KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Enter => {
                    let _ = responder.send(true);
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    let _ = responder.send(false);
                }
                _ => {
                    self.dialogo = Some(Dialogo::HuellaDesconocida {
                        host,
                        tipo,
                        huella,
                        responder,
                    });
                }
            },
            Dialogo::HuellaCambiada {
                host,
                tipo,
                anterior,
                nueva,
                mut campo,
                responder,
            } => match tecla.code {
                KeyCode::Char('r') | KeyCode::Char('R') if campo.texto.trim() == host => {
                    let _ = responder.send(true);
                }
                KeyCode::Esc => {
                    let _ = responder.send(false);
                }
                _ => {
                    campo.manejar_tecla(&tecla);
                    self.dialogo = Some(Dialogo::HuellaCambiada {
                        host,
                        tipo,
                        anterior,
                        nueva,
                        campo,
                        responder,
                    });
                }
            },
            Dialogo::Frase {
                host,
                intento,
                mut campo,
                responder,
            } => match tecla.code {
                KeyCode::Enter => {
                    let frase = Zeroizing::new(campo.texto.clone());
                    campo.limpiar();
                    let _ = responder.send(Some(frase));
                }
                KeyCode::Esc => {
                    let _ = responder.send(None);
                }
                _ => {
                    campo.manejar_tecla(&tecla);
                    self.dialogo = Some(Dialogo::Frase {
                        host,
                        intento,
                        campo,
                        responder,
                    });
                }
            },
            Dialogo::MenuGrupo { mut seleccion } => match tecla.code {
                KeyCode::Esc | KeyCode::Char('q') => {}
                KeyCode::Up | KeyCode::Char('k') => {
                    seleccion = seleccion.saturating_sub(1);
                    self.dialogo = Some(Dialogo::MenuGrupo { seleccion });
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    seleccion = (seleccion + 1).min(5);
                    self.dialogo = Some(Dialogo::MenuGrupo { seleccion });
                }
                KeyCode::Enter => self.ejecutar_menu_grupo(seleccion),
                _ => {
                    self.dialogo = Some(Dialogo::MenuGrupo { seleccion });
                }
            },
            Dialogo::EntradaTexto {
                titulo,
                etiqueta,
                mut campo,
                accion,
            } => match tecla.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let nombre = campo.texto.trim().to_string();
                    if nombre.is_empty() {
                        self.mensaje("el nombre no puede estar vacío", true);
                    } else {
                        match accion {
                            EntradaTextoAccion::CrearGrupo => {
                                self.ejecutar_accion(AccionDialogo::CrearGrupo { nombre });
                            }
                            EntradaTextoAccion::RenombrarGrupo(id) => {
                                self.ejecutar_accion(AccionDialogo::RenombrarGrupo { id, nombre });
                            }
                        }
                    }
                }
                _ => {
                    campo.manejar_tecla(&tecla);
                    self.dialogo = Some(Dialogo::EntradaTexto {
                        titulo,
                        etiqueta,
                        campo,
                        accion,
                    });
                }
            },
            Dialogo::MoverHost {
                host_id,
                host_nombre,
                mut desplegable,
            } => match desplegable.manejar_tecla(&tecla) {
                crate::ui::componentes::ResultadoDesplegable::Seleccionado(valor) => {
                    let grupo_id = match valor {
                        ValorOpcion::Grupo(id) => Some(id),
                        _ => None,
                    };
                    let nombre = desplegable.etiqueta_seleccionada().to_string();
                    self.ejecutar_accion(AccionDialogo::MoverHost { host_id, grupo_id });
                    self.mensaje(format!("«{host_nombre}» movido a {nombre}"), false);
                }
                crate::ui::componentes::ResultadoDesplegable::Cerrado => {}
                crate::ui::componentes::ResultadoDesplegable::SinCambio => {
                    self.dialogo = Some(Dialogo::MoverHost {
                        host_id,
                        host_nombre,
                        desplegable,
                    });
                }
            },
            Dialogo::ResumenImportacion { titulo, lineas } => {
                if !matches!(tecla.code, KeyCode::Enter | KeyCode::Esc) {
                    self.dialogo = Some(Dialogo::ResumenImportacion { titulo, lineas });
                }
            }
            Dialogo::ConflictoImportacion { nombre, restantes } => {
                let decision = match tecla.code {
                    KeyCode::Char('s') => Some(true),
                    KeyCode::Char('o') => Some(false),
                    KeyCode::Char('S') => {
                        if let Some(importacion) = &mut self.importacion {
                            for conflicto in &importacion.conflictos {
                                importacion.decisiones.insert(conflicto.clone(), true);
                            }
                            importacion.indice = importacion.conflictos.len();
                        }
                        self.mostrar_conflicto_actual();
                        return;
                    }
                    KeyCode::Char('O') => {
                        if let Some(importacion) = &mut self.importacion {
                            for conflicto in &importacion.conflictos {
                                importacion.decisiones.insert(conflicto.clone(), false);
                            }
                            importacion.indice = importacion.conflictos.len();
                        }
                        self.mostrar_conflicto_actual();
                        return;
                    }
                    KeyCode::Esc => {
                        self.importacion = None;
                        self.mensaje("importación cancelada", false);
                        return;
                    }
                    _ => {
                        self.dialogo = Some(Dialogo::ConflictoImportacion { nombre, restantes });
                        return;
                    }
                };
                if let Some(importacion) = &mut self.importacion {
                    importacion
                        .decisiones
                        .insert(nombre, decision.unwrap_or(false));
                    importacion.indice += 1;
                }
                self.mostrar_conflicto_actual();
            }
        }
    }

    fn ejecutar_accion(&mut self, accion: AccionDialogo) {
        match accion {
            AccionDialogo::Nada => {}
            AccionDialogo::BorrarHost(id) => {
                if let Err(error) = self.almacen.borrar_host(id) {
                    self.mensaje(error.to_string(), true);
                } else {
                    self.mensaje("host borrado", false);
                    let _ = self.recargar_inventario();
                }
            }
            AccionDialogo::BorrarGrupo(id) => match self.almacen.borrar_grupo(id) {
                Ok(()) => {
                    self.mensaje("grupo borrado; sus hosts pasan a «sin grupo»", false);
                    let _ = self.recargar_inventario();
                }
                Err(error) => self.mensaje(error.to_string(), true),
            },
            AccionDialogo::RenombrarGrupo { id, nombre } => {
                match self.almacen.renombrar_grupo(id, &nombre) {
                    Ok(()) => {
                        let _ = self.recargar_inventario();
                    }
                    Err(error) => self.mensaje(error.to_string(), true),
                }
            }
            AccionDialogo::CrearGrupo { nombre } => match self.almacen.crear_grupo(&nombre) {
                Ok(_) => {
                    let _ = self.recargar_inventario();
                }
                Err(error) => self.mensaje(error.to_string(), true),
            },
            AccionDialogo::MoverHost { host_id, grupo_id } => {
                if let Err(error) = self.almacen.mover_host_a_grupo(host_id, grupo_id) {
                    self.mensaje(error.to_string(), true);
                } else {
                    let _ = self.recargar_inventario();
                }
            }
            AccionDialogo::CerrarSesion => self.cerrar_sesion_actual(),
            AccionDialogo::CerrarSesionYAbrir(host_id) => {
                self.abrir_al_cerrar = Some(host_id);
                self.cerrar_sesion_actual();
            }
            AccionDialogo::Salir => {
                if self.sesion.is_some() {
                    self.salir_pendiente = true;
                    self.cerrar_sesion_actual();
                } else {
                    self.salir = true;
                }
            }
            AccionDialogo::DescartarFicha => {
                self.ficha = None;
                self.vista = Vista::Hosts;
            }
            AccionDialogo::InsertarInclude => {
                let config = self.rutas.fichero_ssh_config();
                let magi_config = self.rutas.fichero_magi_config();
                match sshconfig::exportar::insertar_include(&config, &magi_config) {
                    Ok(copia) => self.mensaje(
                        format!("Include añadido; copia de seguridad en {}", copia.display()),
                        false,
                    ),
                    Err(error) => self.mensaje(error.to_string(), true),
                }
            }
        }
    }
}

fn lanzar_hilo_teclas(tx: mpsc::UnboundedSender<Evento>, salir: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !salir.load(Ordering::Relaxed) {
            match crossterm::event::poll(Duration::from_millis(100)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(tecla)) => {
                        if tx.send(Evento::Tecla(tecla)).is_err() {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Resize(cols, filas)) => {
                        if tx.send(Evento::Redimension(cols, filas)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
}

fn lanzar_tick(runtime: &tokio::runtime::Runtime, tx: mpsc::UnboundedSender<Evento>) {
    runtime.spawn(async move {
        let mut intervalo = tokio::time::interval(Duration::from_millis(200));
        loop {
            intervalo.tick().await;
            if tx.send(Evento::Tick).is_err() {
                break;
            }
        }
    });
}

fn manejar_texto(texto: &mut String, tecla: KeyEvent) -> bool {
    match tecla.code {
        KeyCode::Char(caracter) if !tecla.modifiers.contains(KeyModifiers::CONTROL) => {
            texto.push(caracter);
            true
        }
        KeyCode::Backspace => {
            texto.pop();
            true
        }
        KeyCode::Delete => true,
        _ => false,
    }
}

fn opciones_grupo(grupos: &[Grupo]) -> Vec<Opcion> {
    let mut opciones = vec![Opcion {
        etiqueta: "sin grupo".to_string(),
        valor: ValorOpcion::Ninguno,
    }];
    for grupo in grupos {
        opciones.push(Opcion {
            etiqueta: grupo.nombre.clone(),
            valor: ValorOpcion::Grupo(grupo.id),
        });
    }
    opciones
}

fn opciones_salto(hosts: &[Host], excluido: Option<i64>) -> Vec<Opcion> {
    let mut opciones = vec![Opcion {
        etiqueta: "ninguno".to_string(),
        valor: ValorOpcion::Ninguno,
    }];
    for host in hosts {
        if Some(host.id) == excluido {
            continue;
        }
        opciones.push(Opcion {
            etiqueta: host.nombre.clone(),
            valor: ValorOpcion::Salto(host.id),
        });
    }
    opciones
}

fn etiqueta_identidad(identidad: &IdentidadRef) -> String {
    match identidad {
        IdentidadRef::Auto => "auto".to_string(),
        IdentidadRef::Agente(huella) => format!("agente · {}", acorta_huella(huella)),
        IdentidadRef::Fichero(ruta) => format!("fichero · {ruta}"),
    }
}

fn acorta_huella(huella: &str) -> String {
    let sin_prefijo = huella.trim_start_matches("SHA256:");
    let recorte: String = sin_prefijo.chars().take(12).collect();
    format!("SHA256:{recorte}…")
}

/// Instala el hook que restaura el terminal ante un panic.
pub fn instalar_hook_panico() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |informacion| {
        crate::ui::restaurar_terminal();
        original(informacion);
    }));
}

/// Arranca el runtime de tokio, construye la app y ejecuta el bucle de la UI.
pub fn ejecutar(
    rutas: Rutas,
    config: Config,
    tema: Tema,
    almacen: Almacen,
    aviso: Option<String>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creando el runtime de tokio")?;
    let app = App::nuevo(rutas, config, tema, almacen, runtime, aviso)?;
    app.ejecutar()
}
