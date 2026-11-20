//! Sesiones persistentes (Fase 16): el diálogo «Nueva sesión persistente»
//! (host, nombre, carpeta, comando, comando de reanudación y nota), la ficha
//! que solo se guarda cuando la sesión ya existe en el host, reabrir una
//! terminada, el nombre ocupado y la ficha de un host marcado.
//!
//! La regla de oro es la de CLAUDE.md: el cliente guarda una ficha solo cuando
//! la sesión ya existe en el host y anota en REGISTRO cuando el efecto se ha
//! producido, no cuando se decide. Por eso aceptar el diálogo solo manda
//! `AbrirSesion` con `crear` y apunta una `CreacionPendiente`; la ficha y la
//! anotación llegan cuando la pestaña existe (`pestana_persistente_abierta`) y
//! si el servidor no abre la pestaña (`persistentes_apertura_fallida`) no queda
//! nada.
//!
//! El diálogo lleva fijados los datos sobre los que actúa (los hosts `ssh`, los
//! nombres ya usados y la etiqueta del equipo se leen al abrirlo) y valida con
//! `validar_datos` del núcleo, que normaliza espacios: lo que se manda es el
//! resultado normalizado. Nada de lo que se escribe (comando, nota, carpeta)
//! llega al log ni a REGISTRO.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use magi_nucleo::sesiones_persistentes::nombres::nombre_propuesto;
use magi_nucleo::sesiones_persistentes::validadores::{validar_datos, CampoFicha, ErrorFicha};
use magi_nucleo::sesiones_persistentes::{DatosFicha, EstadoFicha, OrigenFicha};
use magi_nucleo::terminal::tmux::etiqueta_equipo;

use crate::app::persistentes::DialogoPersistentes;
use crate::app::sesion::DestinoPersistente;
use crate::app::{App, Dialogo};
use crate::modelo::ResultadoRegistro;
use crate::protocolo::{self, CrearTmux, MensajeCliente, ServidorTmux, SesionVista};
use crate::ui::componentes::{CampoTexto, Desplegable, Opcion, ResultadoDesplegable, ValorOpcion};

/// Cuánto vive una creación pedida si nunca llega la pestaña ni un fallo (y la
/// conexión con el host ya no está en curso).
pub const CADUCIDAD_CREACION: Duration = Duration::from_secs(60);
/// Mientras la apertura del host sigue en curso, la creación espera más: el
/// usuario puede estar contestando un diálogo (huella, frase, contraseña), que
/// no tiene plazo propio (el servidor espera 5 min).
pub const CADUCIDAD_CON_APERTURA: Duration = Duration::from_secs(6 * 60);
/// Qué se pidió abrir con `abrir_sesion_persistente` y se está esperando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaseCreacion {
    /// Una sesión nueva desde el diálogo: al existir la pestaña se guarda su
    /// ficha (`origen = manual`) y se anota `sesion_persistente_creada`.
    Nueva,
    /// Una ficha terminada que se vuelve a crear: al existir la pestaña se
    /// anota `sesion_persistente_reabierta` (la ficha ya existe).
    Reabrir {
        /// Se usó el comando de reanudación (y no el de arranque).
        con_comando_de_reanudacion: bool,
    },
    /// Engancharse a una sesión que ya existe (sin crear): al existir la
    /// pestaña, con su tmux, se anota `sesion_persistente_enganchada` con este
    /// detalle y no se guarda ninguna ficha (la sesión no la creó este
    /// equipo ahora). La piden `↵` y `c` de la lista (también sobre una
    /// sesión del tmux habitual), el diálogo de nombre ocupado y `magi
    /// sesion`.
    Enganchar {
        servidor: ServidorTmux,
        detalle: String,
    },
}

/// Una apertura pedida y aún sin pestaña: lo que hace falta para guardar la
/// ficha o anotar el efecto cuando la sesión exista. Se quita al llegar la
/// pestaña, al fallar la apertura o al caducar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreacionPendiente {
    pub host_id: i64,
    pub nombre: String,
    pub clase: ClaseCreacion,
    pub carpeta: Option<String>,
    pub comando: Option<String>,
    pub comando_reanudar: Option<String>,
    pub nota: Option<String>,
    /// Cuándo se pidió (reloj del cliente), para caducarla.
    pub pedida_en: Instant,
}

impl CreacionPendiente {
    fn es(&self, host_id: i64, nombre: &str) -> bool {
        self.host_id == host_id && self.nombre == nombre
    }

    /// El servidor de tmux de la sesión pedida: el de MAGI, salvo un enganche
    /// a una del tmux habitual.
    #[must_use]
    pub fn servidor(&self) -> ServidorTmux {
        match &self.clase {
            ClaseCreacion::Enganchar { servidor, .. } => *servidor,
            ClaseCreacion::Nueva | ClaseCreacion::Reabrir { .. } => ServidorTmux::Magi,
        }
    }

    fn es_de(&self, host_id: i64, servidor: ServidorTmux, nombre: &str) -> bool {
        self.es(host_id, nombre) && self.servidor() == servidor
    }
}

// ------------------------------------------------------------- formulario

/// Campos del formulario, en el orden en el que los recorre `Tab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CampoNueva {
    Host,
    Nombre,
    Carpeta,
    Comando,
    Reanudar,
    Nota,
}

pub const ORDEN_CAMPOS_NUEVA: [CampoNueva; 6] = [
    CampoNueva::Host,
    CampoNueva::Nombre,
    CampoNueva::Carpeta,
    CampoNueva::Comando,
    CampoNueva::Reanudar,
    CampoNueva::Nota,
];

impl CampoNueva {
    /// Rótulo del campo (el mismo del campo del núcleo).
    #[must_use]
    pub fn etiqueta(self) -> &'static str {
        match self {
            CampoNueva::Host => CampoFicha::Host.etiqueta(),
            CampoNueva::Nombre => CampoFicha::Nombre.etiqueta(),
            CampoNueva::Carpeta => CampoFicha::Carpeta.etiqueta(),
            CampoNueva::Comando => CampoFicha::Comando.etiqueta(),
            CampoNueva::Reanudar => CampoFicha::ComandoReanudar.etiqueta(),
            CampoNueva::Nota => CampoFicha::Nota.etiqueta(),
        }
    }

    fn desde_ficha(campo: CampoFicha) -> Self {
        match campo {
            CampoFicha::Host => CampoNueva::Host,
            CampoFicha::Nombre | CampoFicha::Equipo => CampoNueva::Nombre,
            CampoFicha::Carpeta => CampoNueva::Carpeta,
            CampoFicha::Comando => CampoNueva::Comando,
            CampoFicha::ComandoReanudar => CampoNueva::Reanudar,
            CampoFicha::Nota => CampoNueva::Nota,
        }
    }
}

/// Lo que pide una tecla del formulario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccionNueva {
    Nada,
    /// `↵` en el formulario: validar y crear.
    Aceptar,
    /// `↵` con el nombre ocupado: engancharse (o reabrir) a la existente.
    Enganchar,
    Cancelar,
}

/// Lo que se sabe de la sesión de la ficha que ocupa el nombre que se quiso
/// crear. Sale de la última lista del servidor: la ficha local no sabe si su
/// sesión vive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoOcupado {
    /// La lista la da por viva: se ofrece engancharse a la existente.
    Viva,
    /// La lista la da por terminada: en vez de engancharse se reabre.
    Terminada,
    /// No hay lista del servidor, no trae esa ficha o no pudo comprobarla: no
    /// se sabe si la sesión existe y no se afirma. `↵` la abre como una
    /// reapertura, que nunca pisa una sesión viva: si ya existe, el servidor
    /// lo dice y entonces sí se ofrece engancharse.
    SinComprobar,
}

/// Formulario «Nueva sesión persistente». Los campos y los datos fijados al
/// abrir viajan en caja: el formulario va dentro de `Dialogo` y no debe
/// agrandar el enum.
#[derive(Debug, Clone)]
pub struct FormularioNueva {
    estado: Box<Estado>,
}

#[derive(Debug, Clone)]
struct Estado {
    foco: CampoNueva,
    /// Los hosts `ssh` fijados al abrir (valor: su id).
    host: Desplegable,
    nombre: CampoTexto,
    /// El nombre lo escribió el usuario: deja de proponerse el de la carpeta.
    nombre_manual: bool,
    carpeta: CampoTexto,
    comando: CampoTexto,
    reanudar: CampoTexto,
    nota: CampoTexto,
    /// Etiqueta de este equipo (`creada_por` de la ficha), fijada al abrir.
    equipo: String,
    /// Nombres ya usados por host (fichas y sesiones sin ficha), fijados al
    /// abrir: solo sirven para proponer un nombre libre.
    usados: HashMap<i64, Vec<String>>,
    /// Lo que enseña el pie: el primer error de la validación local o el
    /// último de un intento de crear. Se retira al cambiar lo escrito.
    error: Option<String>,
    ocupado: Option<EstadoOcupado>,
}

impl FormularioNueva {
    /// Formulario nuevo. `hosts` son los hosts `ssh` (id y nombre) y `propuesto`
    /// el que viene elegido; sin propuesto, el primero y el foco en el host.
    #[must_use]
    pub fn nuevo(
        hosts: &[(i64, String)],
        propuesto: Option<i64>,
        usados: HashMap<i64, Vec<String>>,
        equipo: String,
    ) -> Self {
        let mut host = Desplegable::nuevo(
            hosts
                .iter()
                .map(|(id, nombre)| Opcion {
                    etiqueta: nombre.clone(),
                    valor: ValorOpcion::Salto(*id),
                })
                .collect(),
        );
        let con_propuesta = propuesto.is_some_and(|id| hosts.iter().any(|(otro, _)| *otro == id));
        if let Some(id) = propuesto {
            host.seleccionar_valor(&ValorOpcion::Salto(id));
        }
        let foco = if con_propuesta || hosts.len() <= 1 {
            CampoNueva::Nombre
        } else {
            CampoNueva::Host
        };
        let mut formulario = Self {
            estado: Box::new(Estado {
                foco,
                host,
                nombre: CampoTexto::default(),
                nombre_manual: false,
                carpeta: CampoTexto::default(),
                comando: CampoTexto::default(),
                reanudar: CampoTexto::default(),
                nota: CampoTexto::default(),
                equipo,
                usados,
                error: None,
                ocupado: None,
            }),
        };
        formulario.reproponer_nombre();
        formulario.revalidar();
        formulario
    }

    // ------------------------------------------------------------ lectura

    /// Host elegido.
    #[must_use]
    pub fn host_id(&self) -> Option<i64> {
        match self.estado.host.valor_seleccionado() {
            ValorOpcion::Salto(id) => Some(id),
            _ => None,
        }
    }

    /// Nombre del host elegido.
    #[must_use]
    pub fn host_nombre(&self) -> &str {
        self.estado.host.etiqueta_seleccionada()
    }

    #[must_use]
    pub fn foco(&self) -> CampoNueva {
        self.estado.foco
    }

    /// El campo de texto de `campo` (`None` para el host, que es un
    /// desplegable).
    #[must_use]
    pub fn campo(&self, campo: CampoNueva) -> Option<&CampoTexto> {
        match campo {
            CampoNueva::Host => None,
            CampoNueva::Nombre => Some(&self.estado.nombre),
            CampoNueva::Carpeta => Some(&self.estado.carpeta),
            CampoNueva::Comando => Some(&self.estado.comando),
            CampoNueva::Reanudar => Some(&self.estado.reanudar),
            CampoNueva::Nota => Some(&self.estado.nota),
        }
    }

    /// Lo escrito en un campo (vacío para el host).
    #[must_use]
    pub fn texto(&self, campo: CampoNueva) -> &str {
        self.campo(campo).map_or("", |campo| campo.texto.as_str())
    }

    #[must_use]
    pub fn desplegable(&self) -> &Desplegable {
        &self.estado.host
    }

    /// El error que enseña el pie, si lo hay.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.estado.error.as_deref()
    }

    /// `Some(terminada)` si el nombre está ocupado y el diálogo ofrece
    /// engancharse (o reabrir, si es una ficha terminada) o cambiar el nombre.
    /// Con la sesión sin comprobar es `Some(false)`: véase `estado_ocupado`.
    #[must_use]
    pub fn ocupado(&self) -> Option<bool> {
        self.estado
            .ocupado
            .map(|estado| estado == EstadoOcupado::Terminada)
    }

    /// Lo que se sabe de la sesión cuyo nombre está ocupado, si lo está.
    #[must_use]
    pub fn estado_ocupado(&self) -> Option<EstadoOcupado> {
        self.estado.ocupado
    }

    /// Los datos tal como se mandarían, normalizados, o el primer error.
    pub fn datos_validados(&self) -> Result<DatosFicha, ErrorFicha> {
        validar_datos(&self.estado.datos_crudos())
    }

    // ------------------------------------------------------------ edición

    /// Procesa una tecla.
    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> AccionNueva {
        if self.estado.ocupado.is_some() {
            return self.tecla_ocupado(tecla);
        }
        self.tecla_edicion(tecla)
    }

    fn tecla_ocupado(&mut self, tecla: &KeyEvent) -> AccionNueva {
        if tecla.modifiers.contains(KeyModifiers::CONTROL) {
            return AccionNueva::Nada;
        }
        match tecla.code {
            KeyCode::Enter => AccionNueva::Enganchar,
            KeyCode::Esc => AccionNueva::Cancelar,
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.volver_a_editar();
                AccionNueva::Nada
            }
            _ => AccionNueva::Nada,
        }
    }

    fn tecla_edicion(&mut self, tecla: &KeyEvent) -> AccionNueva {
        // El desplegable abierto se queda con las teclas (filtro, ↑↓, ↵, esc):
        // `Esc` solo lo cierra.
        if self.estado.host.abierto {
            if let ResultadoDesplegable::Seleccionado(_) = self.estado.host.manejar_tecla(tecla) {
                self.tras_editar(CampoNueva::Host);
            }
            return AccionNueva::Nada;
        }
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        match tecla.code {
            KeyCode::Esc => return AccionNueva::Cancelar,
            KeyCode::Enter => return AccionNueva::Aceptar,
            KeyCode::Tab | KeyCode::Down => self.mover_foco(1),
            KeyCode::BackTab | KeyCode::Up => self.mover_foco(-1),
            KeyCode::Char(' ') if !control && self.estado.foco == CampoNueva::Host => {
                if !self.estado.host.opciones.is_empty() {
                    self.estado.host.abrir();
                    // La lista se abre sobre el host elegido (sin filtro, la
                    // posición es el índice).
                    self.estado.host.resaltado = self.estado.host.seleccion;
                }
            }
            _ => {
                let foco = self.estado.foco;
                if let Some(campo) = self.estado.campo_mut(foco) {
                    let antes = campo.texto.clone();
                    campo.manejar_tecla(tecla);
                    // Mover el cursor no es editar: solo cuenta si el texto
                    // cambió.
                    if campo.texto != antes {
                        self.tras_editar(foco);
                    }
                }
            }
        }
        AccionNueva::Nada
    }

    /// Pega texto en el campo con el foco; `false` si no admite texto.
    pub fn pegar_texto(&mut self, texto: &str) -> bool {
        if self.estado.ocupado.is_some() {
            return false;
        }
        let foco = self.estado.foco;
        if foco == CampoNueva::Host {
            if !self.estado.host.abierto {
                return false;
            }
            self.estado.host.filtro.insertar_texto(texto);
            self.estado.host.resaltado = 0;
            return true;
        }
        if let Some(campo) = self.estado.campo_mut(foco) {
            campo.insertar_texto(texto);
            self.tras_editar(foco);
            return true;
        }
        false
    }

    fn mover_foco(&mut self, delta: isize) {
        let total = ORDEN_CAMPOS_NUEVA.len() as isize;
        let actual = ORDEN_CAMPOS_NUEVA
            .iter()
            .position(|campo| *campo == self.estado.foco)
            .unwrap_or(0) as isize;
        let siguiente = (actual + delta).rem_euclid(total) as usize;
        self.estado.foco = ORDEN_CAMPOS_NUEVA[siguiente];
    }

    /// Algo cambió en `campo`: el nombre escrito deja de proponerse, la carpeta
    /// y el host proponen otro, y se vuelve a validar.
    fn tras_editar(&mut self, campo: CampoNueva) {
        match campo {
            CampoNueva::Nombre => self.estado.nombre_manual = true,
            CampoNueva::Carpeta | CampoNueva::Host => self.reproponer_nombre(),
            _ => {}
        }
        self.revalidar();
    }

    /// Si el usuario no ha escrito el nombre, propone el de la carpeta (o
    /// `sesion-<n>`) libre en el host elegido.
    fn reproponer_nombre(&mut self) {
        if self.estado.nombre_manual {
            return;
        }
        let usados = self
            .host_id()
            .and_then(|id| self.estado.usados.get(&id))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let propuesto = nombre_propuesto(self.estado.carpeta.texto.trim(), usados);
        self.estado.nombre = CampoTexto::nuevo(propuesto);
    }

    /// Valida lo escrito y deja el primer error (o ninguno) en el pie. Un error
    /// de un intento anterior de crear se retira en cuanto cambia lo escrito.
    fn revalidar(&mut self) {
        self.estado.error = self.datos_validados().err().map(|error| error.to_string());
    }

    /// Valida para crear. Con un error, lo deja en el pie y lleva el foco al
    /// campo que falla.
    pub fn validar_para_crear(&mut self) -> Option<(i64, DatosFicha)> {
        match self.datos_validados() {
            Ok(datos) => match self.host_id() {
                Some(host_id) => Some((host_id, datos)),
                None => {
                    self.estado.foco = CampoNueva::Host;
                    self.estado.error = Some("no hay ningún host SSH donde crearla".to_string());
                    None
                }
            },
            Err(error) => {
                self.estado.foco = CampoNueva::desde_ficha(error.campo());
                self.estado.error = Some(error.to_string());
                None
            }
        }
    }

    /// Deja un error de un intento de crear en el pie (no vale una sesión
    /// abierta con el servidor caído, por ejemplo).
    pub fn fijar_error(&mut self, mensaje: impl Into<String>) {
        self.estado.error = Some(mensaje.into());
    }

    /// Vuelve al formulario con el foco en `campo` y el motivo de un intento
    /// fallido en el pie. El error se retira en cuanto se cambia algo.
    pub fn volver_con_error_en(&mut self, campo: CampoNueva, mensaje: impl Into<String>) {
        self.estado.ocupado = None;
        self.estado.foco = campo;
        self.estado.error = Some(mensaje.into());
    }

    /// El nombre ya existe: el diálogo ofrece engancharse a la existente
    /// (`terminada = false`) o reabrirla (`true`) o cambiar el nombre.
    pub fn poner_ocupado(&mut self, terminada: bool) {
        self.poner_estado_ocupado(if terminada {
            EstadoOcupado::Terminada
        } else {
            EstadoOcupado::Viva
        });
    }

    /// El nombre ya existe y el diálogo dice lo que se sabe de su sesión.
    pub fn poner_estado_ocupado(&mut self, estado: EstadoOcupado) {
        self.estado.ocupado = Some(estado);
        self.estado.error = None;
    }

    /// Vuelve del aviso de nombre ocupado al formulario, con los datos, en el
    /// nombre.
    pub fn volver_a_editar(&mut self) {
        self.estado.ocupado = None;
        self.estado.foco = CampoNueva::Nombre;
        self.estado.nombre_manual = true;
        self.revalidar();
    }

    /// Rellena los campos con lo que se pidió (al volver con el nombre ocupado).
    fn rellenar(&mut self, pendiente: &CreacionPendiente) {
        self.estado.nombre = CampoTexto::nuevo(pendiente.nombre.clone());
        self.estado.nombre_manual = true;
        self.estado.carpeta = CampoTexto::nuevo(pendiente.carpeta.clone().unwrap_or_default());
        self.estado.comando = CampoTexto::nuevo(pendiente.comando.clone().unwrap_or_default());
        self.estado.reanudar =
            CampoTexto::nuevo(pendiente.comando_reanudar.clone().unwrap_or_default());
        self.estado.nota = CampoTexto::nuevo(pendiente.nota.clone().unwrap_or_default());
        self.revalidar();
    }
}

impl Estado {
    fn campo_mut(&mut self, campo: CampoNueva) -> Option<&mut CampoTexto> {
        match campo {
            CampoNueva::Host => None,
            CampoNueva::Nombre => Some(&mut self.nombre),
            CampoNueva::Carpeta => Some(&mut self.carpeta),
            CampoNueva::Comando => Some(&mut self.comando),
            CampoNueva::Reanudar => Some(&mut self.reanudar),
            CampoNueva::Nota => Some(&mut self.nota),
        }
    }

    /// Lo escrito, sin normalizar (`validar_datos` recorta y quita lo vacío).
    fn datos_crudos(&self) -> DatosFicha {
        DatosFicha {
            nombre: self.nombre.texto.clone(),
            carpeta: Some(self.carpeta.texto.clone()),
            comando: Some(self.comando.texto.clone()),
            comando_reanudar: Some(self.reanudar.texto.clone()),
            nota: Some(self.nota.texto.clone()),
            origen: OrigenFicha::Manual,
            creada_por: self.equipo.clone(),
        }
    }
}

// ------------------------------------------------------------------- App

impl App {
    /// Abre el diálogo «Nueva sesión persistente», con el host propuesto si lo
    /// hay (el de la pestaña activa, el de la fila…). Lo llaman la tecla `n` del
    /// bloque, la paleta, `prefijo N` y las teclas `N` de Hosts y Flota.
    #[doc(hidden)]
    pub fn abrir_dialogo_nueva_sesion(&mut self, host_id: Option<i64>) {
        let Some(formulario) = self.formulario_nueva(host_id) else {
            return;
        };
        self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(
            formulario,
        )));
        self.sucio = true;
    }

    /// Los hosts `ssh` (id y nombre), en el orden de la vista Hosts.
    fn hosts_ssh(&self) -> Vec<(i64, String)> {
        self.hosts
            .iter()
            .filter(|host| host.tipo.es_ssh())
            .map(|host| (host.id, host.nombre.clone()))
            .collect()
    }

    /// El formulario con lo que se lee al abrirlo: hosts, nombres usados y la
    /// etiqueta del equipo. `None` (con aviso) si no hay hosts `ssh`.
    fn formulario_nueva(&mut self, host_id: Option<i64>) -> Option<FormularioNueva> {
        let hosts = self.hosts_ssh();
        if hosts.is_empty() {
            self.mensaje(
                "no hay ningún host SSH: crea uno en Hosts (n) para tener sesiones persistentes",
                true,
            );
            return None;
        }
        // Sin host dado, el de la pestaña activa (si es `ssh`).
        let propuesto = host_id
            .or_else(|| {
                self.pestana_activa
                    .and_then(|indice| self.pestanas.get(indice))
                    .map(|pestana| pestana.host_id)
            })
            .filter(|id| hosts.iter().any(|(otro, _)| otro == id));
        let usados = self.nombres_usados_por_host();
        Some(FormularioNueva::nuevo(
            &hosts,
            propuesto,
            usados,
            self.etiqueta_de_este_equipo(),
        ))
    }

    /// Nombres de las fichas y de las sesiones sin ficha que enseña la última
    /// lista del servidor, por host: solo para proponer un nombre libre.
    fn nombres_usados_por_host(&self) -> HashMap<i64, Vec<String>> {
        let mut usados: HashMap<i64, Vec<String>> = HashMap::new();
        match crate::almacen::sesiones_persistentes::listar(self.almacen.conexion()) {
            Ok(fichas) => {
                for ficha in fichas {
                    usados.entry(ficha.host_id).or_default().push(ficha.nombre);
                }
            }
            Err(error) => tracing::warn!("no se pudieron leer las fichas de sesión: {error}"),
        }
        if let Some(lista) = &self.persistentes.lista {
            for ficha in &lista.fichas {
                usados
                    .entry(ficha.host_id)
                    .or_default()
                    .push(ficha.nombre.clone());
            }
            for host in &lista.hosts {
                for sesion in &host.sin_ficha {
                    usados
                        .entry(host.host_id)
                        .or_default()
                        .push(sesion.nombre.clone());
                }
            }
        }
        usados
    }

    /// La etiqueta de este equipo (`creada_por` de la ficha): la misma que
    /// pone el servidor en `@magi_equipo` y en los nombres automáticos.
    fn etiqueta_de_este_equipo(&self) -> String {
        let nombre = crate::almacen::equipos::este(self.almacen.conexion())
            .map(|equipo| equipo.nombre)
            .ok()
            .filter(|nombre| !nombre.trim().is_empty())
            .unwrap_or_else(crate::config::nombre_actual);
        etiqueta_equipo(&nombre)
    }

    fn nombre_del_host(&self, host_id: i64) -> String {
        self.hosts
            .iter()
            .find(|host| host.id == host_id)
            .map_or_else(|| format!("host {host_id}"), |host| host.nombre.clone())
    }

    /// Tecla para el diálogo «Nueva sesión persistente».
    pub(crate) fn tecla_dialogo_nueva(&mut self, mut formulario: FormularioNueva, tecla: KeyEvent) {
        match formulario.manejar_tecla(&tecla) {
            AccionNueva::Nada => {
                self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(
                    formulario,
                )));
            }
            AccionNueva::Cancelar => {}
            AccionNueva::Aceptar => self.aceptar_nueva(formulario),
            AccionNueva::Enganchar => self.enganchar_al_ocupado(&formulario),
        }
        self.sucio = true;
    }

    /// `↵` en el formulario: valida, comprueba que el nombre no esté ya en una
    /// ficha del host y manda crear. La ficha no se guarda todavía: espera a
    /// que exista la pestaña.
    fn aceptar_nueva(&mut self, mut formulario: FormularioNueva) {
        let Some((host_id, datos)) = formulario.validar_para_crear() else {
            self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(
                formulario,
            )));
            return;
        };
        // ¿Ya hay una ficha con ese host y nombre? Se lee ahora: una ficha
        // llegada por la sincronización mientras se escribía también cuenta.
        if self.hay_ficha(host_id, &datos.nombre) {
            let estado = self.estado_de_la_ficha(host_id, &datos.nombre);
            formulario.poner_estado_ocupado(estado);
            self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(
                formulario,
            )));
            return;
        }
        let hecho = self.abrir_sesion_persistente(DestinoPersistente {
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: datos.nombre.clone(),
            crear: Some(CrearTmux {
                carpeta: datos.carpeta.clone(),
                comando: datos.comando.clone(),
            }),
            compartir: false,
        });
        if !hecho {
            // No se pidió nada (servidor caído o incompatible, ya hay una
            // conexión en curso con ese host…): el formulario se queda con lo
            // escrito y el motivo, que ya dijo `abrir_sesion_persistente`.
            if let Some(mensaje) = &self.mensaje {
                formulario.fijar_error(mensaje.texto.clone());
            }
            self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(
                formulario,
            )));
            return;
        }
        self.apuntar_creacion(CreacionPendiente {
            host_id,
            nombre: datos.nombre.clone(),
            clase: ClaseCreacion::Nueva,
            carpeta: datos.carpeta,
            comando: datos.comando,
            comando_reanudar: datos.comando_reanudar,
            nota: datos.nota,
            pedida_en: Instant::now(),
        });
    }

    /// ¿Hay una ficha con ese host y nombre? (Un error de lectura cuenta como
    /// que no: el servidor dirá si la sesión existe.)
    fn hay_ficha(&self, host_id: i64, nombre: &str) -> bool {
        match crate::almacen::sesiones_persistentes::nombres_de_host(
            self.almacen.conexion(),
            host_id,
        ) {
            Ok(nombres) => nombres.iter().any(|usado| usado == nombre),
            Err(error) => {
                tracing::warn!("no se pudieron leer las fichas del host: {error}");
                false
            }
        }
    }

    /// Lo que dice la última lista del servidor de la sesión de esa ficha. La
    /// ficha local no sabe si su sesión vive: sin lista (la ventana acaba de
    /// abrirse), sin esa ficha en ella o con el host sin comprobar, no se
    /// afirma que exista ni que haya terminado.
    fn estado_de_la_ficha(&self, host_id: i64, nombre: &str) -> EstadoOcupado {
        let estado = self.persistentes.lista.as_ref().and_then(|lista| {
            lista
                .fichas
                .iter()
                .find(|ficha| ficha.host_id == host_id && ficha.nombre == nombre)
                .map(|ficha| ficha.estado)
        });
        match estado {
            Some(EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada) => EstadoOcupado::Viva,
            Some(EstadoFicha::Terminada) => EstadoOcupado::Terminada,
            Some(EstadoFicha::SinComprobar | EstadoFicha::Inalcanzable | EstadoFicha::SinTmux)
            | None => EstadoOcupado::SinComprobar,
        }
    }

    /// `↵` con el nombre ocupado: engancharse a la existente o, si es una
    /// ficha terminada (o no se sabe si lo está), reabrirla: la reapertura no
    /// pisa una sesión que ya viva.
    fn enganchar_al_ocupado(&mut self, formulario: &FormularioNueva) {
        let Some(host_id) = formulario.host_id() else {
            return;
        };
        let Ok(datos) = formulario.datos_validados() else {
            return;
        };
        match formulario.estado_ocupado() {
            Some(EstadoOcupado::Terminada | EstadoOcupado::SinComprobar) => {
                self.reabrir_ficha(host_id, &datos.nombre);
            }
            Some(EstadoOcupado::Viva) | None => {
                self.engancharse_a_persistente(host_id, &datos.nombre, false);
            }
        }
    }

    /// Se engancha a una sesión del tmux propio que ya existe y anota
    /// `sesion_persistente_enganchada` cuando la pestaña exista (solo a
    /// petición del usuario: nunca en una reconexión). Si esta ventana ya ve
    /// una pestaña enganchada a ella, `abrir_sesion_persistente` va a esa
    /// pestaña y no queda nada pendiente.
    pub(crate) fn engancharse_a_persistente(
        &mut self,
        host_id: i64,
        nombre: &str,
        compartir: bool,
    ) {
        let ya_hay_pestana = self.pestanas.iter().any(|pestana| {
            pestana.escritorio_id.is_none()
                && pestana.host_id == host_id
                && pestana.tmux.as_ref().is_some_and(|tmux| {
                    tmux.servidor == ServidorTmux::Magi && tmux.nombre == nombre
                })
        });
        let detalle = self.detalle_enganche_del_dialogo(host_id, nombre, compartir);
        let hecho = self.abrir_sesion_persistente(DestinoPersistente {
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: nombre.to_string(),
            crear: None,
            compartir,
        });
        if hecho && !ya_hay_pestana {
            self.apuntar_enganche(host_id, ServidorTmux::Magi, nombre, detalle);
        }
    }

    /// Apunta un enganche pedido por el usuario (`↵` o `c` en la lista, el
    /// diálogo de nombre ocupado, `magi sesion`): se anota cuando la pestaña
    /// pasa a tener su tmux (el efecto), nunca al pedirlo. Solo se apunta si
    /// `abrir_sesion_persistente` mandó la apertura y no había ya una pestaña
    /// de esta ventana enganchada a esa sesión.
    pub(crate) fn apuntar_enganche(
        &mut self,
        host_id: i64,
        servidor: ServidorTmux,
        nombre: &str,
        detalle: String,
    ) {
        self.apuntar_creacion(CreacionPendiente {
            host_id,
            nombre: nombre.to_string(),
            clase: ClaseCreacion::Enganchar { servidor, detalle },
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            pedida_en: Instant::now(),
        });
    }

    /// `«host» · nombre · soltando a <equipo>` o `· compartiendo`.
    fn detalle_enganche_del_dialogo(&self, host_id: i64, nombre: &str, compartir: bool) -> String {
        let host = self.nombre_del_host(host_id);
        let accion = if compartir {
            "compartiendo".to_string()
        } else {
            match self
                .sesion_conocida(host_id, nombre)
                .filter(|sesion| sesion.clientes > 0)
            {
                Some(sesion) => format!(
                    "soltando a {}",
                    sesion.equipo.as_deref().unwrap_or("otro equipo")
                ),
                None => "sin nadie enganchado".to_string(),
            }
        };
        format!("«{host}» · {nombre} · {accion}")
    }

    /// La sesión del tmux propio tal como la vio el servidor por última vez.
    fn sesion_conocida(&self, host_id: i64, nombre: &str) -> Option<&SesionVista> {
        let lista = self.persistentes.lista.as_ref()?;
        let de_ficha = lista
            .fichas
            .iter()
            .find(|ficha| ficha.host_id == host_id && ficha.nombre == nombre)
            .and_then(|ficha| ficha.sesion.as_ref());
        de_ficha.or_else(|| {
            lista
                .hosts
                .iter()
                .find(|host| host.host_id == host_id)?
                .sin_ficha
                .iter()
                .find(|sesion| sesion.nombre == nombre && sesion.servidor == ServidorTmux::Magi)
        })
    }

    /// Reabre una ficha terminada: la crea con su `comando_reanudar` (o su
    /// `comando`) en su carpeta y anota `sesion_persistente_reabierta` cuando
    /// la pestaña exista. La llama la tecla `↵` del bloque sobre una terminada
    /// y el aviso de nombre ocupado.
    #[doc(hidden)]
    pub fn reabrir_ficha(&mut self, host_id: i64, nombre: &str) {
        let ficha = match crate::almacen::sesiones_persistentes::por_nombre(
            self.almacen.conexion(),
            host_id,
            nombre,
        ) {
            Ok(Some(ficha)) => ficha,
            Ok(None) => {
                self.mensaje(format!("la ficha «{nombre}» ya no existe"), true);
                return;
            }
            Err(error) => {
                self.mensaje(
                    format!("no se pudo leer la ficha «{nombre}»: {error}"),
                    true,
                );
                return;
            }
        };
        let con_comando_de_reanudacion = ficha.comando_reanudar.is_some();
        let comando = ficha
            .comando_reanudar
            .clone()
            .or_else(|| ficha.comando.clone());
        let hecho = self.abrir_sesion_persistente(DestinoPersistente {
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: ficha.nombre.clone(),
            crear: Some(CrearTmux {
                carpeta: ficha.carpeta.clone(),
                comando,
            }),
            compartir: false,
        });
        // Sin petición en vuelo (servidor caído, conexión en curso…) no queda
        // nada pendiente: no se anota lo que no ha pasado.
        if !hecho {
            return;
        }
        self.apuntar_creacion(CreacionPendiente {
            host_id,
            nombre: ficha.nombre.clone(),
            clase: ClaseCreacion::Reabrir {
                con_comando_de_reanudacion,
            },
            carpeta: ficha.carpeta,
            comando: ficha.comando,
            comando_reanudar: ficha.comando_reanudar,
            nota: ficha.nota,
            pedida_en: Instant::now(),
        });
    }

    // ------------------------------------------------------- pendientes

    /// Apunta una apertura pedida (una por host, servidor y nombre) y tira las
    /// caducadas.
    fn apuntar_creacion(&mut self, pendiente: CreacionPendiente) {
        self.caducar_creaciones(Instant::now());
        self.persistentes
            .creaciones
            .retain(|otra| !otra.es_de(pendiente.host_id, pendiente.servidor(), &pendiente.nombre));
        self.persistentes.creaciones.push(pendiente);
    }

    /// Quita las creaciones pedidas que nunca recibieron pestaña ni fallo: a
    /// los 60 s, o a los 6 min si la conexión con el host sigue en curso (el
    /// usuario puede estar contestando un diálogo). Devuelve cuántas quitó.
    ///
    /// «En curso» no basta con `aperturas_pendientes`: el servidor difunde la
    /// pestaña (`Abriendo`, sin tmux) en cuanto empieza a abrirla y esa primera
    /// lista ya saca al host de `aperturas_pendientes`, mientras la huella o la
    /// frase siguen esperando al usuario. Por eso también cuenta una pestaña de
    /// ese host que sigue abriéndose.
    #[doc(hidden)]
    pub fn caducar_creaciones(&mut self, ahora: Instant) -> usize {
        let antes = self.persistentes.creaciones.len();
        let en_curso = &self.aperturas_pendientes;
        let pestanas = &self.pestanas;
        self.persistentes.creaciones.retain(|pendiente| {
            let abriendose = en_curso.contains(&pendiente.host_id)
                || pestanas.iter().any(|pestana| {
                    pestana.escritorio_id.is_none()
                        && pestana.host_id == pendiente.host_id
                        && pestana.estado == protocolo::EstadoSesionRemota::Abriendo
                });
            let plazo = if abriendose {
                CADUCIDAD_CON_APERTURA
            } else {
                CADUCIDAD_CREACION
            };
            ahora.saturating_duration_since(pendiente.pedida_en) <= plazo
        });
        antes - self.persistentes.creaciones.len()
    }

    /// Una pestaña acaba de pasar a tener su tmux propio en `Sesiones{lista}`
    /// (`reconciliar_sesiones` la llama en esa transición, que con el servidor
    /// real llega una lista después de la que enseña la pestaña `Abriendo` sin
    /// tmux): la sesión ya existe en el host. Si hay una apertura pendiente de
    /// ese host y nombre, se guarda la ficha (al crear) o se anota lo que se
    /// pidió (reabrir, engancharse; un enganche a una sesión que ya existía no
    /// guarda ficha); sin pendiente, en un host marcado se crea la ficha con
    /// `origen = host` si no la tiene **y la pestaña creó la sesión**
    /// (`creada` es el `TmuxPestana.creada` que difunde el servidor: D369,
    /// R163; un enganche a una sesión que ya existía no la crea).
    #[doc(hidden)]
    pub fn pestana_persistente_abierta(
        &mut self,
        host_id: i64,
        nombre: &str,
        creada: Option<bool>,
    ) {
        // Una pestaña tardía (tras un diálogo del host) sigue siendo válida:
        // lo que caduca es la espera, no la sesión que ya existe.
        let pendiente = self
            .persistentes
            .creaciones
            .iter()
            .position(|pendiente| pendiente.es_de(host_id, ServidorTmux::Magi, nombre))
            .map(|posicion| self.persistentes.creaciones.remove(posicion));
        match pendiente {
            Some(pendiente) => match &pendiente.clase {
                ClaseCreacion::Nueva => self.guardar_ficha_creada(&pendiente),
                ClaseCreacion::Reabrir {
                    con_comando_de_reanudacion,
                } => {
                    let host = self.nombre_del_host(host_id);
                    let como = if *con_comando_de_reanudacion {
                        "con su comando de reanudación"
                    } else {
                        "con su comando de arranque"
                    };
                    self.anotar_persistente(
                        crate::registro::SESION_PERSISTENTE_REABIERTA,
                        host_id,
                        format!("«{host}» · {nombre} · {como}"),
                    );
                    // Sin esto, su fila seguiría «terminada» hasta la
                    // siguiente comprobación del servidor, con la sesión ya
                    // viva y la pestaña abierta.
                    self.consultar_persistentes_tras_crear();
                }
                ClaseCreacion::Enganchar { detalle, .. } => {
                    self.anotar_persistente(
                        crate::registro::SESION_PERSISTENTE_ENGANCHADA,
                        host_id,
                        detalle.clone(),
                    );
                }
            },
            None => self.ficha_de_host_marcado(host_id, nombre, creada),
        }
        self.sucio = true;
    }

    /// Lo mismo para una pestaña que pasa a tener su tmux **habitual** (la
    /// reconciliación solo avisa de las del tmux de MAGI): se anota el
    /// enganche que el usuario pidió. Nunca guarda ficha ni anota otra cosa.
    pub(crate) fn pestana_del_tmux_habitual_abierta(&mut self, host_id: i64, nombre: &str) {
        let Some(posicion) = self
            .persistentes
            .creaciones
            .iter()
            .position(|pendiente| pendiente.es_de(host_id, ServidorTmux::Habitual, nombre))
        else {
            return;
        };
        let pendiente = self.persistentes.creaciones.remove(posicion);
        if let ClaseCreacion::Enganchar { detalle, .. } = pendiente.clase {
            self.anotar_persistente(
                crate::registro::SESION_PERSISTENTE_ENGANCHADA,
                host_id,
                detalle,
            );
        }
        self.sucio = true;
    }

    /// La sesión creada desde el diálogo ya existe: se guarda su ficha
    /// (`origen = manual`) y se anota `sesion_persistente_creada`.
    fn guardar_ficha_creada(&mut self, pendiente: &CreacionPendiente) {
        let datos = DatosFicha {
            nombre: pendiente.nombre.clone(),
            carpeta: pendiente.carpeta.clone(),
            comando: pendiente.comando.clone(),
            comando_reanudar: pendiente.comando_reanudar.clone(),
            nota: pendiente.nota.clone(),
            origen: OrigenFicha::Manual,
            creada_por: self.etiqueta_de_este_equipo(),
        };
        // Se intenta guardar directamente, sin mirar antes: el `UNIQUE(host_id,
        // nombre)` es lo único que dice de verdad si otra ventana de este
        // equipo llegó antes con su ficha automática (mirar y luego insertar
        // deja un hueco en el que lo escrito en el diálogo se perdería sin
        // aviso). Si ya estaba, se vuelca en ella si era la automática.
        if self.crear_ficha_y_anotar(pendiente.host_id, &datos) {
            self.completar_ficha_automatica(pendiente.host_id, &datos);
        }
    }

    /// En un host marcado, otra ventana de este mismo equipo ve la pestaña
    /// nueva a la vez y puede guardar antes su ficha automática (`origen =
    /// host`, sin carpeta, comando ni nota). Si el usuario pidió esta sesión
    /// con el diálogo, lo que escribió (comando de reanudación, nota…) no se
    /// pierde: se vuelca en esa ficha, que sigue siendo la misma y sigue con
    /// su anotación «creada». Devuelve `true` si la completó. Una ficha de
    /// otro equipo, o que ya tiene datos, no se toca.
    fn completar_ficha_automatica(&mut self, host_id: i64, datos: &DatosFicha) -> bool {
        let conexion = self.almacen.conexion();
        let Ok(Some(existente)) =
            crate::almacen::sesiones_persistentes::por_nombre(conexion, host_id, &datos.nombre)
        else {
            return false;
        };
        let intacta = existente.origen == OrigenFicha::Host
            && existente.creada_por == datos.creada_por
            && existente.carpeta.is_none()
            && existente.comando.is_none()
            && existente.comando_reanudar.is_none()
            && existente.nota.is_none();
        if !intacta {
            return false;
        }
        match crate::almacen::sesiones_persistentes::actualizar(conexion, existente.id, datos) {
            Ok(()) => {
                self.consultar_persistentes_tras_crear();
                true
            }
            Err(error) => {
                tracing::warn!("no se pudo completar la ficha de la sesión: {error}");
                false
            }
        }
    }

    /// Una pestaña persistente de un host marcado sin ficha de ese nombre
    /// (el servidor eligió el nombre automático): ficha con `origen = host`,
    /// solo si la pestaña **creó** la sesión en el host (`creada == Some(true)`).
    /// Una pestaña que se enganchó a una sesión que ya existía, una
    /// reconexión, o la lista de pestañas de una ventana que arranca no
    /// llevan `creada`: de esas no se crea ficha, o reaparecería la que el
    /// usuario borró (la sesión sigue en el host) con una anotación «creada»
    /// falsa (R163). Un servidor anterior, que no manda el campo, tampoco la
    /// crea: es lo seguro.
    fn ficha_de_host_marcado(&mut self, host_id: i64, nombre: &str, creada: Option<bool>) {
        let marcado = self
            .hosts
            .iter()
            .find(|host| host.id == host_id)
            .is_some_and(|host| host.sesiones_persistentes && host.tipo.es_ssh());
        if !marcado || creada != Some(true) || self.hay_ficha(host_id, nombre) {
            return;
        }
        let datos = DatosFicha {
            nombre: nombre.to_string(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Host,
            creada_por: self.etiqueta_de_este_equipo(),
        };
        // Si otra ventana la guarda a la vez no hay nada que completar: esta
        // ficha es la automática y no lleva datos del usuario.
        let _ = self.crear_ficha_y_anotar(host_id, &datos);
    }

    /// Guarda la ficha (la unión de fichas por host y nombre es del núcleo,
    /// pero aquí no se duplica: el `UNIQUE` lo impide) y anota
    /// `sesion_persistente_creada` solo si la guardó. Luego pide al servidor que
    /// vuelva a mirar para que la lista enseñe la ficha ya. Devuelve `true` si
    /// ya había una con ese host y nombre (otra ventana de este equipo o la
    /// sincronización llegaron antes): no es un error ni se anota otra vez.
    fn crear_ficha_y_anotar(&mut self, host_id: i64, datos: &DatosFicha) -> bool {
        let conexion = self.almacen.conexion();
        match crate::almacen::sesiones_persistentes::crear(conexion, host_id, datos) {
            Ok(_) => {
                let host = self.nombre_del_host(host_id);
                self.anotar_persistente(
                    crate::registro::SESION_PERSISTENTE_CREADA,
                    host_id,
                    format!("«{host}» · {} · {}", datos.nombre, datos.origen.texto()),
                );
                self.consultar_persistentes_tras_crear();
                false
            }
            // Ya había una ficha con ese host y nombre (otra ventana de este
            // equipo la guardó a la vez, o llegó por la sincronización): la
            // sesión existe y a quien la guardó le toca anotar su creación. No
            // es un error ni se anota dos veces.
            Err(error)
                if matches!(
                    error.downcast_ref::<ErrorFicha>(),
                    Some(ErrorFicha::NombreEnUso { .. })
                ) =>
            {
                tracing::debug!("la ficha de la sesión la guardó otra ventana");
                self.consultar_persistentes_tras_crear();
                true
            }
            Err(error) => {
                tracing::warn!("no se pudo guardar la ficha de la sesión: {error}");
                self.mensaje(
                    format!(
                        "la sesión «{}» ya existe en el host, pero no se pudo guardar su ficha: {error}",
                        datos.nombre
                    ),
                    true,
                );
                false
            }
        }
    }

    /// Pide al servidor que vuelva a comprobar (las fichas las relee en cada
    /// pasada) para que la lista enseñe la ficha nueva sin esperar al
    /// vigilante.
    pub(crate) fn consultar_persistentes_tras_crear(&mut self) {
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        self.servidor
            .enviar(MensajeCliente::ConsultarSesionesPersistentes {
                peticion_id,
                host_id: None,
            });
    }

    /// Anota un evento de las sesiones persistentes: solo host y nombre (y
    /// origen o forma de engancharse), nunca comandos, títulos ni contenido.
    fn anotar_persistente(&self, tipo: &str, host_id: i64, detalle: String) {
        self.anotar(tipo, Some(host_id), None, &detalle, ResultadoRegistro::Ok);
    }

    /// El servidor no pudo abrir la pestaña de tmux. La apertura pendiente de
    /// ese host y nombre se quita siempre: no queda ficha ni anotación.
    /// Devuelve `true` si esta pista enseñó su diálogo y la barra no debe dar
    /// el error genérico: con una sesión **nueva** que falló por el nombre
    /// (ocupado: engancharse a la existente o cambiar el nombre) o por la
    /// carpeta (no existe en el host), el diálogo vuelve con todo lo escrito
    /// y el motivo en el pie (D370: el usuario corrige y vuelve a intentarlo
    /// sin reescribir el comando ni la nota). `texto` es el motivo ya con su
    /// instrucción. Las demás causas (sin tmux, tmux antiguo, conexión…) y las
    /// aperturas que no vienen del diálogo devuelven `false` para que se diga
    /// el mensaje con instrucción.
    #[doc(hidden)]
    pub fn persistentes_apertura_fallida(
        &mut self,
        host_id: i64,
        nombre: &str,
        causa: protocolo::CausaAperturaTmux,
        texto: &str,
    ) -> bool {
        let Some(posicion) = self
            .persistentes
            .creaciones
            .iter()
            .position(|pendiente| pendiente.es(host_id, nombre))
        else {
            return false;
        };
        let pendiente = self.persistentes.creaciones.remove(posicion);
        if matches!(pendiente.clase, ClaseCreacion::Enganchar { .. }) {
            return false;
        }
        // La carpeta solo se vuelve a pedir en lo que se escribió en el
        // diálogo: reabrir una ficha usa la suya y no tiene a dónde volver.
        let por_carpeta = causa == protocolo::CausaAperturaTmux::SinCarpeta
            && pendiente.clase == ClaseCreacion::Nueva;
        if causa != protocolo::CausaAperturaTmux::Existe && !por_carpeta {
            return false;
        }
        let Some(mut formulario) = self.formulario_nueva(Some(host_id)) else {
            return false;
        };
        formulario.rellenar(&pendiente);
        if por_carpeta {
            formulario.volver_con_error_en(CampoNueva::Carpeta, texto);
        } else {
            // El servidor dice que existe: engancharse, nunca reabrir.
            formulario.poner_ocupado(false);
        }
        // Ya no se está «creando» nada: el aviso de la barra sobraría.
        self.mensaje = None;
        // Puede haber otro diálogo abierto: este se apila sobre él.
        self.mostrar_dialogo_servidor(Dialogo::Persistentes(DialogoPersistentes::Nueva(
            formulario,
        )));
        self.sucio = true;
        true
    }
}
