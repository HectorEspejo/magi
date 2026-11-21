//! Sesiones persistentes (Fase 16): el estado del cliente, el bloque
//! «Persistentes» de la vista Sesiones, la vista previa y los diálogos de las
//! sesiones (nueva, ficha, renombrar, terminar, borrar y limpiar).
//!
//! Lo fijado en el contrato del Sprint 3 y que otras pistas usan:
//!
//! - `EstadoPersistentes.lista`: la última `ListaPersistentes` que difundió el
//!   servidor (la guarda `evento_persistentes`; la pista B la lee para el aviso
//!   de la barra global y la pestaña soltada).
//! - `App::persistentes_con_atencion`: las sesiones que piden atención ahora.
//! - `App::pestana_persistente_abierta` y `App::persistentes_apertura_fallida`
//!   (en `persistentes_nueva.rs`, pista A2).
//! - `App::abrir_dialogo_nueva_sesion` y `App::reabrir_ficha` (pista A2): las
//!   llaman las teclas `n` y `↵` sobre una terminada de este bloque.
//!
//! **Todo el estado de una sesión sale de la lista del servidor.** Este módulo
//! no inventa estados: las filas (`filas.rs`) se derivan de `FichaVista` y de
//! `HostVisto`, que ya traen el estado del núcleo
//! (`sesiones_persistentes::estado`). El estado de una sesión (viva, clientes,
//! equipo, actividad, atención, título) y su contenido (vista previa) viven
//! solo en memoria: nunca van a disco, al log ni a REGISTRO.
//!
//! La única excepción es una sesión que el propio usuario acaba de terminar:
//! hasta que el servidor vuelva a mirar el host, su fila se ve terminada (el
//! host contestó `ok`). Una ventana nueva recibe la lista que había al irse la
//! última ventana, que puede ser vieja: la cabecera dice la edad real de la
//! comprobación (`comprobada_en`) y nunca la presenta como actual.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::persistentes_nueva::{CreacionPendiente, FormularioNueva};
use crate::app::sesion::DestinoPersistente;
use crate::app::{AccionDialogo, AccionPaleta, App, Dialogo, EntradaPaleta};
use crate::modelo::ResultadoRegistro;
use crate::protocolo::{
    self, EstadoFicha, InfoSesion, ListaPersistentes, MensajeCliente, MensajeServidor,
    ResultadoRenombrar, ResultadoTmux, ServidorTmux,
};
use crate::ui::componentes::CampoTexto;
use magi_nucleo::sesiones_persistentes::validadores::{
    nombre_valido, validar_datos, validar_nombre, CampoFicha, ErrorFicha,
};
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

pub mod filas;

pub use filas::{
    construir_filas, contar, hace, quien_la_tiene, ClaveFila, FilaPersistente, ObjetivoSesion,
    TerminadaLocal, TipoFila,
};

/// Una consulta sin respuesta se da por perdida pasado este plazo y se puede
/// repetir (el servidor siempre contesta, salvo que se cierre la ventana).
const PLAZO_CONSULTA: Duration = Duration::from_secs(20);
/// La comprobación de un host pedida con `t` caduca si nunca llega.
const CADUCIDAD_HOST_PEDIDO: Duration = Duration::from_secs(30);
/// `C` (borrar fichas) y `a` (crearlas) solo escriben si la comprobación de su
/// host es de hace menos de esto: una lista vieja (la de una ventana que se
/// cerró hace rato) puede dar por terminada una sesión que ya vive o enseñar
/// «sin ficha» una que ya no existe, y la ficha viaja a los demás equipos.
const EDAD_MAXIMA_PARA_ESCRIBIR: i64 = 120;
/// Etiquetas de los campos de la ficha, en el orden del formulario.
pub const ETIQUETAS_FICHA: [&str; 4] = ["Carpeta", "Comando", "Reanudar", "Nota"];

/// Qué bloque de la vista Sesiones recibe las teclas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoSesiones {
    Persistentes,
    Pestanas,
}

/// Una consulta al servidor que aún no tiene respuesta.
#[derive(Debug, Clone)]
pub struct ConsultaEnVuelo {
    pub peticion_id: u64,
    pub host_id: Option<i64>,
    pub desde: Instant,
}

/// Una petición al servidor que espera respuesta y lleva fijados los datos
/// sobre los que actúa: la respuesta no relee el estado.
#[derive(Debug, Clone)]
pub enum PeticionPersistente {
    Renombrar {
        objetivo: ObjetivoSesion,
        nuevo: String,
    },
    Terminar {
        objetivo: ObjetivoSesion,
    },
}

/// El host que se pidió comprobar con `t`: al llegar su resultado se dice qué
/// hay (o por qué no se pudo mirar).
#[derive(Debug, Clone)]
pub struct HostPedido {
    pub host_id: i64,
    pub host_nombre: String,
    /// Segundos Unix de la petición: solo vale una comprobación posterior.
    pub epoca: i64,
    pub desde: Instant,
}

/// Estado del cliente de las sesiones persistentes.
#[derive(Default)]
pub struct EstadoPersistentes {
    /// Última lista que difundió el servidor (lista completa, T27).
    pub lista: Option<ListaPersistentes>,
    /// Cuándo llegó esa lista (reloj del cliente).
    pub recibida_en: Option<Instant>,
    /// Segundos Unix en que esta ventana recibió la `Bienvenida` del servidor
    /// (la última, si reconectó). La lista que llega detrás de ella puede ser
    /// la que el servidor guardó hace horas, al irse la última ventana: una
    /// comprobación anterior a este momento no vale para avisar de atención
    /// (D368). `None` mientras no ha habido `Bienvenida`.
    pub conectada_en: Option<i64>,
    /// Aperturas pedidas que aún no tienen pestaña con su tmux: crear (la
    /// ficha se guarda solo cuando la sesión ya existe en el host), reabrir y
    /// engancharse (↵ y `c` de la lista, el diálogo de nombre ocupado y
    /// `magi sesion`). Es el único sitio donde espera lo que se anota o se
    /// guarda «cuando el efecto se ha producido».
    pub creaciones: Vec<CreacionPendiente>,
    // ------------------------------------------------ bloque (pista A1)
    /// Bloque con las teclas. `None` hasta la primera tecla: mientras tanto es
    /// «Persistentes» si tiene filas y «Pestañas» si no, para que una lista que
    /// llega tarde no cambie lo que hace `↵` bajo los dedos del usuario.
    pub foco: Option<FocoSesiones>,
    /// Fila elegida (el índice sirve si la fila ya no está en la lista).
    pub seleccion: usize,
    /// Identidad de la fila elegida: la selección la sigue aunque la lista se
    /// refresque y las filas cambien de sitio.
    pub seleccion_clave: Option<ClaveFila>,
    /// Primera fila visible (la reescribe cada pintado).
    pub desplazamiento: usize,
    /// Texto de `/`.
    pub filtro: CampoTexto,
    /// El filtro se está escribiendo.
    pub filtrando: bool,
    /// La vista Sesiones estaba a la vista en el último evento.
    pub a_la_vista: bool,
    /// Cuándo se mandó la última consulta.
    pub ultima_consulta: Option<Instant>,
    /// La consulta que espera respuesta, si la hay.
    pub consulta: Option<ConsultaEnVuelo>,
    /// Peticiones en vuelo (renombrar y terminar), por `peticion_id`.
    pub peticiones: HashMap<u64, PeticionPersistente>,
    /// Sesiones terminadas por el usuario que el servidor aún no ha vuelto a
    /// mirar.
    pub terminadas: Vec<TerminadaLocal>,
    /// Host pedido con `t`, si espera resultado.
    pub host_pedido: Option<HostPedido>,
    /// Etiqueta de este equipo (la de `@magi_equipo`): la deja `lista_recibida`
    /// la primera vez y las pruebas pueden fijarla antes.
    pub etiqueta_equipo: Option<String>,
}

/// La vista previa de una sesión (`v`, §6.3): las últimas líneas en texto
/// plano. El contenido vive solo en este diálogo, en memoria.
pub struct VistaPrevia {
    /// Sobre qué actúa `↵`: fijado al abrir.
    pub objetivo: ObjetivoSesion,
    /// «agente-api · web-01 · libre · actividad hace 4 m · ! pide atención».
    pub cabecera: String,
    /// La petición en vuelo; `None` cuando ya hay respuesta.
    pub peticion_id: Option<u64>,
    pub lineas: Vec<String>,
    /// Segundos Unix de la lectura.
    pub leida_en: Option<i64>,
    pub error: Option<String>,
    /// Cuántas líneas se pidieron (`[persistentes] vista_previa_lineas`).
    pub lineas_pedidas: usize,
}

/// Editar la ficha de una sesión (`e`): carpeta, comando, reanudar y nota.
pub struct FormularioFicha {
    pub objetivo: ObjetivoSesion,
    /// Carpeta, comando, reanudar y nota, en este orden.
    pub campos: [CampoTexto; 4],
    pub foco: usize,
    /// Campo que falla y por qué.
    pub error: Option<(usize, String)>,
}

/// Renombrar una sesión (`,`): en el host y en su ficha.
pub struct Renombrar {
    pub objetivo: ObjetivoSesion,
    pub campo: CampoTexto,
    pub error: Option<String>,
}

/// Diálogos de las sesiones persistentes (viven en `App.dialogo`).
pub enum DialogoPersistentes {
    /// «Nueva sesión persistente» (lo define la pista A2, `persistentes_nueva`).
    Nueva(FormularioNueva),
    /// Vista previa de una sesión.
    VistaPrevia(Box<VistaPrevia>),
    /// Ficha de una sesión (carpeta, comando, reanudar y nota).
    Ficha(Box<FormularioFicha>),
    /// Renombrar una sesión.
    Renombrar(Box<Renombrar>),
}

/// Un texto pegado para un campo de una línea: sin controles y acotado.
fn texto_para_campo(texto: &str) -> String {
    texto
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(4096)
        .collect()
}

impl DialogoPersistentes {
    /// Pega texto en el campo con el foco; `false` si no admite texto.
    pub fn pegar_texto(&mut self, texto: &str) -> bool {
        match self {
            DialogoPersistentes::Nueva(formulario) => formulario.pegar_texto(texto),
            DialogoPersistentes::VistaPrevia(_) => false,
            DialogoPersistentes::Ficha(formulario) => {
                let foco = formulario.foco;
                if let Some(campo) = formulario.campos.get_mut(foco) {
                    campo.insertar_texto(&texto_para_campo(texto));
                    formulario.error = None;
                }
                true
            }
            DialogoPersistentes::Renombrar(dialogo) => {
                dialogo.campo.insertar_texto(&texto_para_campo(texto));
                dialogo.error = None;
                true
            }
        }
    }
}

/// Acciones confirmadas de las sesiones persistentes (viven en
/// `AccionDialogo::Persistentes`). Cada una lleva fijados los datos sobre los
/// que actúa: confirmar no relee el estado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPersistentes {
    /// Terminar la sesión (`T`): se cierra lo que corre dentro.
    Terminar { objetivo: ObjetivoSesion },
    /// Borrar la ficha (`x`); `viva` dice si la sesión existía al preguntar
    /// (`None`: no se pudo comprobar). Nunca toca el host.
    BorrarFicha {
        objetivo: ObjetivoSesion,
        viva: Option<bool>,
    },
    /// Borrar las fichas terminadas (`C`): las que se contaron al preguntar.
    LimpiarTerminadas { fichas: Vec<ObjetivoSesion> },
}

/// Entradas de la paleta de las sesiones persistentes («nueva sesión
/// persistente», «engancharse a <nombre>»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPaletaPersistentes {
    /// «nueva sesión persistente».
    Nueva,
    /// «engancharse a <nombre> · <host>»: una por ficha viva o terminada.
    Enganchar { host_id: i64, nombre: String },
}

/// Una sesión que pide atención ahora (la campana de tmux, D335).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtencionPersistente {
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
}

/// Margen sobre el instante de conexión al decidir si una comprobación es
/// anterior a ella: las horas se guardan en segundos enteros y la `Bienvenida`
/// llega unos instantes después de que el servidor registre la ventana, así
/// que una comprobación hecha justo en el límite de la repetición mínima del
/// servidor (5 s) podría caer un segundo por debajo del umbral.
const MARGEN_DE_CONEXION_SEG: i64 = 2;

fn ahora_epoca() -> i64 {
    chrono::Utc::now().timestamp()
}

impl App {
    // ------------------------------------------------------------ eventos

    /// Mensajes de las sesiones persistentes que llegan del servidor
    /// (`SesionesPersistentes`, `VistaPrevia`, `TmuxRenombrada`, la
    /// terminación y los errores de sus peticiones). Devuelve si lo atendió;
    /// `Sesiones` y `Bienvenida` solo se miran de paso.
    pub(crate) fn evento_persistentes(&mut self, mensaje: &MensajeServidor) -> bool {
        match mensaje {
            MensajeServidor::SesionesPersistentes { lista } => {
                self.lista_persistentes_recibida(lista.clone());
                true
            }
            MensajeServidor::VistaPrevia {
                peticion_id,
                host_id,
                nombre,
                lineas,
                leida_en,
                error,
            } => {
                self.vista_previa_sesion_recibida(
                    *peticion_id,
                    *host_id,
                    nombre,
                    lineas,
                    *leida_en,
                    error.as_deref(),
                );
                true
            }
            MensajeServidor::TmuxRenombrada {
                peticion_id,
                resultado,
            } => {
                self.renombrada(*peticion_id, resultado);
                true
            }
            MensajeServidor::TmuxTerminada {
                peticion_id,
                resultado,
                ..
            } if self.persistentes.peticiones.contains_key(peticion_id) => {
                self.sesion_terminada(*peticion_id, *resultado);
                true
            }
            MensajeServidor::Error {
                peticion_id: Some(peticion_id),
                mensaje,
            } if self.peticion_de_persistentes(*peticion_id) => {
                self.error_de_persistentes(*peticion_id, mensaje);
                true
            }
            // Las pestañas del tmux habitual no pasan por
            // `pestana_persistente_abierta` (solo las del tmux de MAGI): sus
            // enganches se resuelven aquí, antes de reconciliar la lista.
            MensajeServidor::Sesiones { lista } => {
                self.anotar_enganches_del_tmux_habitual(lista);
                false
            }
            MensajeServidor::Bienvenida { .. } => {
                // Lo que estaba en vuelo se perdió con la conexión anterior.
                self.perder_lo_en_vuelo_de_persistentes();
                self.persistentes.conectada_en = Some(ahora_epoca());
                false
            }
            _ => false,
        }
    }

    /// Lo que se pidió al servidor y esperaba respuesta no la va a tener (se
    /// perdió la conexión): ni consultas, ni terminaciones, ni renombrados, ni
    /// vistas previas «leyendo…» para siempre.
    fn perder_lo_en_vuelo_de_persistentes(&mut self) {
        self.persistentes.peticiones.clear();
        self.persistentes.consulta = None;
        self.persistentes.host_pedido = None;
        self.persistentes.a_la_vista = false;
        for dialogo in self.dialogo.iter_mut().chain(self.pila_dialogos.iter_mut()) {
            if let Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(vista)) = dialogo {
                if vista.peticion_id.take().is_some() {
                    vista.error = Some("se perdió la conexión con el servidor".to_string());
                }
            }
        }
        self.sucio = true;
    }

    /// El servidor de sesiones se ha ido: además de lo que estaba en vuelo, la
    /// lista que se tenía ya no es la de nadie. Sin esto, tras «seguir sin
    /// sesiones» la vista seguiría enseñando (y dejando actuar sobre) filas y
    /// avisos de atención de un servidor que ya no está.
    pub(crate) fn persistentes_servidor_caido(&mut self) {
        self.perder_lo_en_vuelo_de_persistentes();
        // Una apertura pedida y sin pestaña no la va a tener (en `Bienvenida`
        // no se tocan: `magi sesion <nombre>` las deja apuntadas al arrancar,
        // antes de que llegue).
        self.persistentes.creaciones.clear();
        self.persistentes.lista = None;
        self.persistentes.recibida_en = None;
        self.persistentes.conectada_en = None;
        self.persistentes.ultima_consulta = None;
        self.persistentes.terminadas.clear();
        self.persistentes.foco = None;
        self.persistentes.filtrando = false;
    }

    /// ¿La petición es una de las de este módulo?
    fn peticion_de_persistentes(&self, peticion_id: u64) -> bool {
        self.persistentes.peticiones.contains_key(&peticion_id)
            || self
                .persistentes
                .consulta
                .as_ref()
                .is_some_and(|consulta| consulta.peticion_id == peticion_id)
            || self.buscar_vista_previa_sesion(peticion_id).is_some()
    }

    fn nueva_peticion_persistentes(&mut self) -> u64 {
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        peticion_id
    }

    /// Llega la lista del servidor.
    fn lista_persistentes_recibida(&mut self, lista: ListaPersistentes) {
        self.asegurar_etiqueta_equipo();
        // Una terminación local solo vale hasta que el host se vuelve a mirar.
        let terminadas = std::mem::take(&mut self.persistentes.terminadas);
        self.persistentes.terminadas = terminadas
            .into_iter()
            .filter(|terminada| {
                lista
                    .hosts
                    .iter()
                    .find(|host| host.host_id == terminada.host_id)
                    .is_none_or(|host| host.comprobada_en <= terminada.epoca)
            })
            .collect();
        self.persistentes.lista = Some(lista);
        self.persistentes.recibida_en = Some(Instant::now());
        self.persistentes.consulta = None;
        // La selección sigue a su fila aunque cambie de sitio. Si todavía no
        // tiene identidad (el usuario aún no ha movido nada), se la da ahora:
        // así un refresco que mete filas por delante no cambia, bajo `↵`, la
        // sesión que el usuario tiene a la vista.
        let filas = self.filas_persistentes();
        if !filas.is_empty() {
            let indice = self.indice_persistente(&filas);
            self.fijar_seleccion_persistentes(&filas, indice);
        }
        self.mostrar_host_pedido();
        self.sucio = true;
    }

    /// El resultado del host que se pidió con `t`: lleva la selección a su
    /// primera fila o dice por qué no hay nada que enseñar.
    fn mostrar_host_pedido(&mut self) {
        let Some(pedido) = self.persistentes.host_pedido.clone() else {
            return;
        };
        let Some(visto) = self.persistentes.lista.as_ref().and_then(|lista| {
            lista
                .hosts
                .iter()
                .find(|host| host.host_id == pedido.host_id && host.comprobada_en >= pedido.epoca)
                .cloned()
        }) else {
            return;
        };
        self.persistentes.host_pedido = None;
        let nombre = pedido.host_nombre;
        if let Some(error) = visto.error {
            let motivo = match error {
                protocolo::ErrorComprobacion::NecesitaDialogo => {
                    "pide huella, frase o contraseña: conéctate a mano"
                }
                protocolo::ErrorComprobacion::SinRespuesta => "no responde",
                protocolo::ErrorComprobacion::Plazo => "no respondió en 10 s",
            };
            self.mensaje(format!("no se pudo mirar tmux en {nombre}: {motivo}"), true);
            return;
        }
        if visto.tmux.is_none() {
            self.mensaje(
                format!("{nombre} no tiene tmux: instálalo (3.0 o posterior)"),
                true,
            );
            return;
        }
        let filas = self.filas_persistentes();
        match filas.iter().position(|fila| fila.host_id == pedido.host_id) {
            Some(indice) => {
                self.fijar_seleccion_persistentes(&filas, indice);
                self.persistentes.foco = Some(FocoSesiones::Persistentes);
                self.mensaje(format!("sesiones de tmux en {nombre}"), false);
            }
            None => self.mensaje(format!("{nombre} no tiene sesiones de tmux"), false),
        }
    }

    // ----------------------------------------------------------- consultas

    /// Cada evento mira si la vista Sesiones está a la vista: al abrirla pide
    /// una consulta y, con la lista a la vista, otra cada `[persistentes]
    /// refresco_seg`. Tampoco deja caducado el host pedido (las aperturas
    /// pedidas caducan en `tick`, con `caducar_creaciones`).
    pub(crate) fn vigilar_persistentes(&mut self) {
        let ahora = Instant::now();
        if self
            .persistentes
            .host_pedido
            .as_ref()
            .is_some_and(|pedido| ahora.duration_since(pedido.desde) >= CADUCIDAD_HOST_PEDIDO)
        {
            self.persistentes.host_pedido = None;
        }
        let a_la_vista = self.vista == crate::ui::Vista::Sesiones
            && !self.servidor_caido
            && self.servidor_incompatible.is_none();
        if !a_la_vista {
            if self.persistentes.a_la_vista {
                self.persistentes.a_la_vista = false;
                self.persistentes.foco = None;
                self.persistentes.filtrando = false;
            }
            return;
        }
        if !self.persistentes.a_la_vista {
            // Al abrir la lista.
            self.persistentes.a_la_vista = true;
            self.persistentes.foco = None;
            self.consultar_persistentes(None, false);
            return;
        }
        let refresco = Duration::from_secs(self.config.persistentes.refresco_seg_efectivo());
        let toca = self
            .persistentes
            .ultima_consulta
            .is_none_or(|ultima| ahora.duration_since(ultima) >= refresco);
        if toca {
            self.consultar_persistentes(None, false);
        }
    }

    /// Manda `ConsultarSesionesPersistentes` (rango del terminal, 2^49). Sin
    /// `forzar`, no repite una consulta que sigue en vuelo; con `host_id`
    /// (`t`) pide además ese host. Devuelve si la mandó.
    pub(crate) fn consultar_persistentes(&mut self, host_id: Option<i64>, forzar: bool) -> bool {
        if self.servidor_caido || self.servidor_incompatible.is_some() {
            return false;
        }
        let ahora = Instant::now();
        if !forzar {
            if let Some(consulta) = &self.persistentes.consulta {
                let sirve = consulta.desde.elapsed() < PLAZO_CONSULTA
                    && (host_id.is_none() || host_id == consulta.host_id);
                if sirve {
                    return false;
                }
            }
        }
        let peticion_id = self.nueva_peticion_persistentes();
        self.servidor
            .enviar(MensajeCliente::ConsultarSesionesPersistentes {
                peticion_id,
                host_id,
            });
        self.persistentes.consulta = Some(ConsultaEnVuelo {
            peticion_id,
            host_id,
            desde: ahora,
        });
        self.persistentes.ultima_consulta = Some(ahora);
        self.sucio = true;
        true
    }

    /// `t` en la lista de pestañas (y la paleta): comprueba el host de la
    /// pestaña elegida; sus sesiones salen en «Persistentes».
    pub(crate) fn pedir_sesiones_del_host_de_la_pestana(&mut self) {
        let Some(pestana) = self.pestanas.get(self.seleccion_sesiones) else {
            self.mensaje("no hay ninguna pestaña elegida", true);
            return;
        };
        if pestana.escritorio_id.is_some() {
            self.mensaje("un escritorio no tiene sesiones de tmux", true);
            return;
        }
        let (host_id, host_nombre) = (pestana.host_id, pestana.host_nombre.clone());
        self.persistentes.host_pedido = Some(HostPedido {
            host_id,
            host_nombre: host_nombre.clone(),
            epoca: ahora_epoca(),
            desde: Instant::now(),
        });
        if self.consultar_persistentes(Some(host_id), false) {
            self.mensaje(format!("comprobando tmux en {host_nombre}…"), false);
        } else {
            self.mensaje(
                "no se puede comprobar ahora (sin servidor o ya en curso)",
                true,
            );
        }
        self.sucio = true;
    }

    // ------------------------------------------------------------- filas

    /// Etiqueta de este equipo (`@magi_equipo`): minúsculas, sin tildes y con
    /// guiones, como la pone el servidor.
    fn asegurar_etiqueta_equipo(&mut self) {
        if self.persistentes.etiqueta_equipo.is_none() {
            let etiqueta =
                magi_nucleo::terminal::tmux::etiqueta_equipo(&self.nombre_de_este_equipo());
            self.persistentes.etiqueta_equipo = Some(etiqueta);
        }
    }

    /// El nombre de este equipo: el de la sincronización (fila `es_este = 1`
    /// de EQUIPOS) o, sin ella, el del sistema. El mismo que usa el servidor.
    fn nombre_de_este_equipo(&self) -> String {
        self.almacen
            .conexion()
            .query_row("SELECT nombre FROM EQUIPOS WHERE es_este = 1", [], |fila| {
                fila.get::<_, String>(0)
            })
            .ok()
            .filter(|nombre| !nombre.trim().is_empty())
            .unwrap_or_else(crate::config::nombre_actual)
    }

    fn nombre_de_host_persistentes(&self, host_id: i64) -> String {
        self.hosts
            .iter()
            .find(|host| host.id == host_id)
            .map(|host| host.nombre.clone())
            .unwrap_or_else(|| format!("host {host_id}"))
    }

    /// Las filas del bloque con el filtro puesto.
    #[must_use]
    pub fn filas_persistentes(&self) -> Vec<FilaPersistente> {
        self.construir_filas_persistentes(self.persistentes.filtro.texto.as_str())
    }

    /// Todas las filas, sin filtro (cabecera y paleta).
    #[must_use]
    pub fn filas_persistentes_todas(&self) -> Vec<FilaPersistente> {
        self.construir_filas_persistentes("")
    }

    fn construir_filas_persistentes(&self, filtro: &str) -> Vec<FilaPersistente> {
        let Some(lista) = &self.persistentes.lista else {
            return Vec::new();
        };
        let nombre_host = |host_id: i64| self.nombre_de_host_persistentes(host_id);
        construir_filas(
            lista,
            &nombre_host,
            self.config.persistentes.antigua_dias_efectivo(),
            &self.persistentes.terminadas,
            filtro,
        )
    }

    /// Índice de la fila elegida: la de la identidad guardada o, si ya no está,
    /// la más cercana al índice anterior.
    #[must_use]
    pub fn indice_persistente(&self, filas: &[FilaPersistente]) -> usize {
        if let Some(clave) = &self.persistentes.seleccion_clave {
            if let Some(indice) = filas.iter().position(|fila| &fila.clave() == clave) {
                return indice;
            }
        }
        self.persistentes
            .seleccion
            .min(filas.len().saturating_sub(1))
    }

    /// La fila elegida.
    #[must_use]
    pub fn fila_persistente_elegida(&self) -> Option<FilaPersistente> {
        let filas = self.filas_persistentes();
        let indice = self.indice_persistente(&filas);
        filas.into_iter().nth(indice)
    }

    fn fijar_seleccion_persistentes(&mut self, filas: &[FilaPersistente], indice: usize) {
        let indice = indice.min(filas.len().saturating_sub(1));
        self.persistentes.seleccion = indice;
        self.persistentes.seleccion_clave = filas.get(indice).map(FilaPersistente::clave);
        self.sucio = true;
    }

    /// Mueve la selección y, si cae en una fila cuyo host no se pudo mirar,
    /// dice el motivo en la barra.
    fn mover_seleccion_persistentes(&mut self, desplazamiento: isize) {
        let filas = self.filas_persistentes();
        if filas.is_empty() {
            return;
        }
        let actual = self.indice_persistente(&filas);
        let nuevo = actual
            .saturating_add_signed(desplazamiento)
            .min(filas.len() - 1);
        self.fijar_seleccion_persistentes(&filas, nuevo);
        if let Some(motivo) = filas.get(nuevo).and_then(FilaPersistente::motivo) {
            let host = filas[nuevo].host_nombre.clone();
            self.mensaje(format!("{host}: {motivo}"), false);
        }
    }

    /// ¿Qué bloque tiene las teclas? Hasta la primera tecla es automático:
    /// «Persistentes» si tiene filas y «Pestañas» si no.
    #[must_use]
    pub fn foco_sesiones(&self) -> FocoSesiones {
        self.persistentes.foco.unwrap_or_else(|| {
            if self.persistentes.filtrando || !self.filas_persistentes().is_empty() {
                FocoSesiones::Persistentes
            } else {
                FocoSesiones::Pestanas
            }
        })
    }

    /// ¿Las teclas de la vista Sesiones son las del bloque «Persistentes»?
    #[must_use]
    pub fn persistentes_enfocado(&self) -> bool {
        self.foco_sesiones() == FocoSesiones::Persistentes
    }

    // -------------------------------------------------------------- teclas

    /// Teclas de la vista Sesiones para el bloque «Persistentes». Devuelve si
    /// la tecla era suya: con el foco en «Pestañas» solo `Tab` lo es.
    pub(crate) fn tecla_persistentes_sesiones(&mut self, tecla: KeyEvent) -> bool {
        // La primera tecla fija el foco automático.
        if self.persistentes.foco.is_none() {
            self.persistentes.foco = Some(self.foco_sesiones());
        }
        if self.persistentes.filtrando {
            self.tecla_filtro_persistentes(tecla);
            return true;
        }
        if matches!(tecla.code, KeyCode::Tab | KeyCode::BackTab) {
            self.alternar_foco_sesiones();
            return true;
        }
        if !self.persistentes_enfocado() {
            return false;
        }
        // Con Ctrl o Alt no es ninguna tecla del bloque (`Ctrl+x` no borra).
        if tecla
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return true;
        }
        let paginas = self
            .disposicion
            .filas(crate::ui::disposicion::Lista::Persistentes) as isize;
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => self.mover_seleccion_persistentes(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover_seleccion_persistentes(1),
            KeyCode::PageUp => self.mover_seleccion_persistentes(-paginas),
            KeyCode::PageDown => self.mover_seleccion_persistentes(paginas),
            KeyCode::Home => self.mover_seleccion_persistentes(isize::MIN / 2),
            KeyCode::End => self.mover_seleccion_persistentes(isize::MAX / 2),
            KeyCode::Enter => self.enganchar_elegida(false),
            KeyCode::Char('c') => self.enganchar_elegida(true),
            KeyCode::Char('v') => self.abrir_vista_previa_sesion(),
            KeyCode::Char('n') => {
                let host_id = self.fila_persistente_elegida().map(|fila| fila.host_id);
                self.abrir_dialogo_nueva_sesion(host_id);
            }
            KeyCode::Char('e') => self.abrir_ficha_de_sesion(),
            KeyCode::Char(',') => self.abrir_renombrar_sesion(),
            KeyCode::Char('T') => self.preguntar_terminar_sesion(),
            KeyCode::Char('x') => self.preguntar_borrar_ficha(),
            KeyCode::Char('a') => self.anadir_sesion_sin_ficha(),
            KeyCode::Char('C') => self.preguntar_limpiar_terminadas(),
            KeyCode::Char('r') => {
                if self.consultar_persistentes(None, false) {
                    self.mensaje("comprobando las sesiones…", false);
                } else {
                    self.mensaje("ya se están comprobando las sesiones", false);
                }
            }
            KeyCode::Char('/') => {
                self.persistentes.filtrando = true;
                self.persistentes.filtro.limpiar();
                self.persistentes.seleccion = 0;
                self.persistentes.seleccion_clave = None;
                self.persistentes.desplazamiento = 0;
            }
            // Con un filtro puesto, `Esc` lo quita antes de salir de la vista.
            KeyCode::Esc if !self.persistentes.filtro.texto.is_empty() => {
                self.persistentes.filtro.limpiar();
                self.persistentes.seleccion_clave = None;
                self.persistentes.desplazamiento = 0;
            }
            // Salir de la vista y apagar el servidor siguen siendo de la vista.
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('S') => return false,
            _ => {}
        }
        self.sucio = true;
        true
    }

    /// `Tab`: del bloque «Persistentes» a «Pestañas» y vuelta.
    fn alternar_foco_sesiones(&mut self) {
        let nuevo = match self.foco_sesiones() {
            FocoSesiones::Persistentes => FocoSesiones::Pestanas,
            FocoSesiones::Pestanas => FocoSesiones::Persistentes,
        };
        self.persistentes.foco = Some(nuevo);
        self.sucio = true;
    }

    /// Teclas con el filtro de `/` abierto: escribe, `↵` lo deja puesto y
    /// `Esc` lo quita.
    fn tecla_filtro_persistentes(&mut self, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Enter => self.persistentes.filtrando = false,
            KeyCode::Esc => {
                self.persistentes.filtrando = false;
                self.persistentes.filtro.limpiar();
            }
            KeyCode::Up => self.mover_seleccion_persistentes(-1),
            KeyCode::Down => self.mover_seleccion_persistentes(1),
            _ => {
                if self.persistentes.filtro.manejar_tecla(&tecla) {
                    self.persistentes.seleccion = 0;
                    self.persistentes.seleccion_clave = None;
                    self.persistentes.desplazamiento = 0;
                }
            }
        }
        self.sucio = true;
    }

    /// Tecla para un diálogo de las sesiones persistentes.
    pub(crate) fn tecla_dialogo_persistentes(
        &mut self,
        dialogo: DialogoPersistentes,
        tecla: KeyEvent,
    ) {
        match dialogo {
            DialogoPersistentes::Nueva(formulario) => {
                self.tecla_dialogo_nueva(formulario, tecla);
            }
            DialogoPersistentes::VistaPrevia(vista) => self.tecla_vista_previa_sesion(vista, tecla),
            DialogoPersistentes::Ficha(formulario) => self.tecla_ficha_de_sesion(formulario, tecla),
            DialogoPersistentes::Renombrar(dialogo) => self.tecla_renombrar_sesion(dialogo, tecla),
        }
    }

    // ----------------------------------------------------------- engancharse

    fn enganchar_elegida(&mut self, compartir: bool) {
        match self.fila_persistente_elegida() {
            Some(fila) => self.enganchar_objetivo(&fila.objetivo(), compartir),
            None => self.mensaje("no hay ninguna sesión persistente elegida", true),
        }
    }

    /// Engancharse a una sesión, soltando a los demás (o compartiendo). En una
    /// terminada, reabre la ficha; en una sin ficha o del tmux habitual, se
    /// engancha sin ficha. Si esta ventana ya ve una pestaña enganchada a ella,
    /// `abrir_sesion_persistente` va a esa pestaña. `sesion_persistente_
    /// enganchada` se anota cuando la pestaña pasa a tener su tmux (el
    /// servidor la difunde antes `Abriendo`, sin él), solo por orden del
    /// usuario (nunca en una reconexión): el enganche espera en
    /// `persistentes.creaciones`.
    pub(crate) fn enganchar_objetivo(&mut self, objetivo: &ObjetivoSesion, compartir: bool) {
        if objetivo.tipo == TipoFila::Ficha && objetivo.estado == EstadoFicha::Terminada {
            if compartir {
                self.mensaje(
                    format!(
                        "«{}» ya terminó en {}: {} la vuelve a crear",
                        objetivo.nombre, objetivo.host_nombre, self.tema.glifos.intro
                    ),
                    true,
                );
            } else {
                self.reabrir_ficha(objetivo.host_id, &objetivo.nombre);
            }
            return;
        }
        if objetivo.estado == EstadoFicha::SinTmux {
            self.mensaje(
                format!(
                    "{} no tiene tmux 3.0 o posterior: instálalo o actualízalo en el host",
                    objetivo.host_nombre
                ),
                true,
            );
            return;
        }
        let servidor = objetivo.servidor();
        // En el tmux habitual nunca se suelta a nadie.
        let compartir = compartir || servidor == ServidorTmux::Habitual;
        let ya_hay_pestana = self.pestanas.iter().any(|pestana| {
            pestana.escritorio_id.is_none()
                && pestana.host_id == objetivo.host_id
                && pestana
                    .tmux
                    .as_ref()
                    .is_some_and(|tmux| tmux.servidor == servidor && tmux.nombre == objetivo.nombre)
        });
        let detalle = self.detalle_de_enganche_lista(objetivo, compartir);
        let hecho = self.abrir_sesion_persistente(DestinoPersistente {
            host_id: objetivo.host_id,
            servidor,
            nombre: objetivo.nombre.clone(),
            crear: None,
            compartir,
        });
        if hecho && !ya_hay_pestana {
            self.apuntar_enganche(objetivo.host_id, servidor, &objetivo.nombre, detalle);
        }
    }

    /// El detalle de REGISTRO de un enganche: host, nombre y «soltando a
    /// <equipo>» o «compartiendo». Nunca comandos, títulos ni contenido.
    fn detalle_de_enganche_lista(&self, objetivo: &ObjetivoSesion, compartir: bool) -> String {
        let motivo = if compartir {
            "compartiendo".to_string()
        } else if objetivo.clientes > 0 {
            match &objetivo.equipo {
                Some(equipo) => format!("soltando a {equipo}"),
                None => "soltando a otro equipo".to_string(),
            }
        } else {
            "sin nadie enganchado".to_string()
        };
        format!(
            "«{}» · {} · {motivo}",
            objetivo.host_nombre, objetivo.nombre
        )
    }

    /// Una pestaña de la lista que llega pasa a tener el tmux **habitual** del
    /// host (antes no tenía ninguno o no existía): se anota el enganche que el
    /// usuario pidió. Mira antes de que se reconcilie la lista, así que
    /// `self.pestanas` aún tiene la anterior. Con el servidor real la pestaña
    /// llega primero `Abriendo`, sin tmux, y el tmux se rellena una lista
    /// después: por eso se compara con lo que ya tenía la pestaña y no con su
    /// primera aparición. Las del tmux de MAGI las resuelve
    /// `pestana_persistente_abierta`, que llama la reconciliación.
    fn anotar_enganches_del_tmux_habitual(&mut self, sesiones: &[InfoSesion]) {
        if self.persistentes.creaciones.is_empty() {
            return;
        }
        for info in sesiones {
            let Some(tmux) = info
                .tmux
                .as_ref()
                .filter(|tmux| tmux.servidor == ServidorTmux::Habitual)
            else {
                continue;
            };
            let ya_tenia_tmux = self.pestanas.iter().any(|pestana| {
                pestana.sesion_id == info.id
                    && pestana.escritorio_id.is_none()
                    && pestana.tmux.is_some()
            });
            if !ya_tenia_tmux {
                self.pestana_del_tmux_habitual_abierta(info.host_id, &tmux.nombre);
            }
        }
    }

    // --------------------------------------------------------- vista previa

    /// `v`: pide las últimas líneas de la sesión y abre la vista previa.
    fn abrir_vista_previa_sesion(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        if fila.tipo == TipoFila::Ficha && fila.estado == EstadoFicha::Terminada {
            self.mensaje(
                format!(
                    "«{}» ya terminó: no hay nada que ver ({} la vuelve a crear)",
                    fila.nombre, self.tema.glifos.intro
                ),
                true,
            );
            return;
        }
        if fila.estado == EstadoFicha::SinTmux {
            self.mensaje(
                format!("{} no tiene tmux 3.0 o posterior", fila.host_nombre),
                true,
            );
            return;
        }
        let objetivo = fila.objetivo();
        let cabecera = self.cabecera_de_vista_previa(&fila);
        let mut vista = VistaPrevia {
            objetivo,
            cabecera,
            peticion_id: None,
            lineas: Vec::new(),
            leida_en: None,
            error: None,
            lineas_pedidas: self.config.persistentes.vista_previa_lineas_efectivo(),
        };
        self.pedir_vista_previa_sesion(&mut vista);
        self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(
            Box::new(vista),
        )));
        self.sucio = true;
    }

    /// «agente-api · web-01 · libre · actividad hace 4 m · ! pide atención».
    fn cabecera_de_vista_previa(&self, fila: &FilaPersistente) -> String {
        let este = self.persistentes.etiqueta_equipo.as_deref().unwrap_or("");
        let mut partes = vec![fila.nombre.clone(), fila.host_nombre.clone()];
        partes.push(match fila.estado {
            EstadoFicha::VivaEnganchada => {
                format!("enganchada ({})", quien_la_tiene(fila_equipo(fila), este))
            }
            EstadoFicha::VivaLibre => "libre".to_string(),
            _ => "sin comprobar".to_string(),
        });
        if let Some(segundos) = fila.actividad_hace() {
            partes.push(format!("actividad {}", hace(segundos)));
        }
        if fila.atencion() {
            partes.push(format!("{} pide atención", self.tema.glifos.aviso));
        }
        partes.join(&format!(" {} ", self.tema.glifos.punto_medio))
    }

    /// Manda `PedirVistaPrevia` y apunta la petición en el diálogo: solo la
    /// respuesta de esta petición lo rellena.
    fn pedir_vista_previa_sesion(&mut self, vista: &mut VistaPrevia) {
        let peticion_id = self.nueva_peticion_persistentes();
        self.servidor.enviar(MensajeCliente::PedirVistaPrevia {
            peticion_id,
            host_id: vista.objetivo.host_id,
            servidor: vista.objetivo.servidor(),
            nombre: vista.objetivo.nombre.clone(),
        });
        vista.peticion_id = Some(peticion_id);
        vista.error = None;
    }

    /// La vista previa en vuelo con esa petición, en el diálogo abierto o en
    /// los apilados.
    fn buscar_vista_previa_sesion(&self, peticion_id: u64) -> Option<&VistaPrevia> {
        self.dialogo
            .iter()
            .chain(self.pila_dialogos.iter())
            .find_map(|dialogo| match dialogo {
                Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(vista))
                    if vista.peticion_id == Some(peticion_id) =>
                {
                    Some(vista.as_ref())
                }
                _ => None,
            })
    }

    fn buscar_vista_previa_sesion_mut(&mut self, peticion_id: u64) -> Option<&mut VistaPrevia> {
        self.dialogo
            .iter_mut()
            .chain(self.pila_dialogos.iter_mut())
            .find_map(|dialogo| match dialogo {
                Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(vista))
                    if vista.peticion_id == Some(peticion_id) =>
                {
                    Some(vista.as_mut())
                }
                _ => None,
            })
    }

    /// Llega la vista previa: se descarta si la petición ya no está en vuelo
    /// (el diálogo se cerró o se pidió otra).
    fn vista_previa_sesion_recibida(
        &mut self,
        peticion_id: u64,
        host_id: i64,
        nombre: &str,
        lineas: &[String],
        leida_en: i64,
        error: Option<&str>,
    ) {
        let Some(vista) = self.buscar_vista_previa_sesion_mut(peticion_id) else {
            return;
        };
        if vista.objetivo.host_id != host_id || vista.objetivo.nombre != nombre {
            return;
        }
        vista.peticion_id = None;
        vista.leida_en = Some(leida_en);
        vista.error = error.map(limpiar_texto);
        // Entrada no fiable: nada de controles en lo que se pinta.
        vista.lineas = lineas.iter().map(|linea| limpiar_texto(linea)).collect();
        self.sucio = true;
    }

    fn tecla_vista_previa_sesion(&mut self, mut vista: Box<VistaPrevia>, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Esc | KeyCode::Char('q') => {}
            KeyCode::Enter => {
                let objetivo = vista.objetivo.clone();
                self.enganchar_objetivo(&objetivo, false);
            }
            KeyCode::Char('r') => {
                self.pedir_vista_previa_sesion(&mut vista);
                self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(
                    vista,
                )));
            }
            _ => {
                self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::VistaPrevia(
                    vista,
                )));
            }
        }
    }

    // ---------------------------------------------------------------- ficha

    /// `e`: edita la ficha (carpeta, comando, reanudar y nota).
    fn abrir_ficha_de_sesion(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        let Some(uuid) = fila.uuid.clone() else {
            self.mensaje(
                format!("«{}» no tiene ficha: pulsa a para añadirla", fila.nombre),
                true,
            );
            return;
        };
        match crate::almacen::sesiones_persistentes::por_uuid(self.almacen.conexion(), &uuid) {
            Ok(Some(ficha)) => {
                let texto =
                    |valor: &Option<String>| CampoTexto::nuevo(valor.clone().unwrap_or_default());
                let formulario = FormularioFicha {
                    objetivo: fila.objetivo(),
                    campos: [
                        texto(&ficha.carpeta),
                        texto(&ficha.comando),
                        texto(&ficha.comando_reanudar),
                        texto(&ficha.nota),
                    ],
                    foco: 0,
                    error: None,
                };
                self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Ficha(Box::new(
                    formulario,
                ))));
                self.sucio = true;
            }
            Ok(None) => self.mensaje(format!("la ficha de «{}» ya no existe", fila.nombre), true),
            Err(error) => self.mensaje(format!("no se pudo leer la ficha: {error}"), true),
        }
    }

    fn tecla_ficha_de_sesion(&mut self, mut formulario: Box<FormularioFicha>, tecla: KeyEvent) {
        let total = formulario.campos.len();
        match tecla.code {
            KeyCode::Esc => return,
            KeyCode::Tab | KeyCode::Down => formulario.foco = (formulario.foco + 1) % total,
            KeyCode::BackTab | KeyCode::Up => {
                formulario.foco = (formulario.foco + total - 1) % total;
            }
            KeyCode::Enter => {
                if let Some(formulario) = self.guardar_ficha_de_sesion(formulario) {
                    self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Ficha(
                        formulario,
                    )));
                }
                return;
            }
            _ => {
                let foco = formulario.foco;
                if formulario.campos[foco].manejar_tecla(&tecla) {
                    formulario.error = None;
                }
            }
        }
        self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Ficha(
            formulario,
        )));
    }

    /// Guarda la ficha con el almacén (sube la versión y viaja). Si no es
    /// válida, devuelve el formulario con el error en su campo.
    fn guardar_ficha_de_sesion(
        &mut self,
        mut formulario: Box<FormularioFicha>,
    ) -> Option<Box<FormularioFicha>> {
        let uuid = formulario.objetivo.uuid.clone()?;
        let conexion = self.almacen.conexion();
        let ficha = match crate::almacen::sesiones_persistentes::por_uuid(conexion, &uuid) {
            Ok(Some(ficha)) => ficha,
            Ok(None) => {
                self.mensaje(
                    format!(
                        "la ficha de «{}» ya no existe (la borró otro equipo)",
                        formulario.objetivo.nombre
                    ),
                    true,
                );
                return None;
            }
            Err(error) => {
                self.mensaje(format!("no se pudo leer la ficha: {error}"), true);
                return None;
            }
        };
        let valor = |indice: usize| Some(formulario.campos[indice].texto.clone());
        let datos = DatosFicha {
            nombre: ficha.nombre.clone(),
            carpeta: valor(0),
            comando: valor(1),
            comando_reanudar: valor(2),
            nota: valor(3),
            origen: ficha.origen,
            creada_por: ficha.creada_por.clone(),
        };
        // La validación normaliza (recorta y deja `None` lo vacío): se guarda
        // lo normalizado.
        let datos = match validar_datos(&datos) {
            Ok(datos) => datos,
            Err(error) => {
                poner_error_de_ficha(&mut formulario, &error);
                return Some(formulario);
            }
        };
        match crate::almacen::sesiones_persistentes::actualizar(conexion, ficha.id, &datos) {
            Ok(()) => {
                self.mensaje(format!("ficha de «{}» guardada", ficha.nombre), false);
                None
            }
            Err(error) => {
                match error.downcast_ref::<ErrorFicha>() {
                    Some(error_ficha) => poner_error_de_ficha(&mut formulario, error_ficha),
                    None => {
                        formulario.error = Some((formulario.foco, error.to_string()));
                    }
                }
                Some(formulario)
            }
        }
    }

    // ------------------------------------------------------------ renombrar

    /// `,`: renombra la sesión en el host y, si tiene ficha, la ficha.
    fn abrir_renombrar_sesion(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        if fila.tipo == TipoFila::Habitual {
            self.mensaje(
                "las sesiones de tu tmux de siempre no se renombran desde MAGI",
                true,
            );
            return;
        }
        if fila.estado == EstadoFicha::SinTmux {
            self.mensaje(
                format!("{} no tiene tmux 3.0 o posterior", fila.host_nombre),
                true,
            );
            return;
        }
        self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Renombrar(
            Box::new(Renombrar {
                campo: CampoTexto::nuevo(fila.nombre.clone()),
                objetivo: fila.objetivo(),
                error: None,
            }),
        )));
        self.sucio = true;
    }

    fn tecla_renombrar_sesion(&mut self, mut dialogo: Box<Renombrar>, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                if let Some(error) = self.enviar_renombrado(&dialogo) {
                    dialogo.error = Some(error);
                    self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Renombrar(
                        dialogo,
                    )));
                }
            }
            _ => {
                if dialogo.campo.manejar_tecla(&tecla) {
                    dialogo.error = None;
                }
                self.dialogo = Some(Dialogo::Persistentes(DialogoPersistentes::Renombrar(
                    dialogo,
                )));
            }
        }
    }

    /// Valida el nombre nuevo y manda `RenombrarTmux`. Devuelve el error que
    /// el diálogo enseña, o `None` si se mandó.
    fn enviar_renombrado(&mut self, dialogo: &Renombrar) -> Option<String> {
        let objetivo = &dialogo.objetivo;
        let nuevo = dialogo.campo.texto.trim().to_string();
        if let Err(error) = validar_nombre(&nuevo) {
            return Some(error.to_string());
        }
        if nuevo == objetivo.nombre {
            return Some("es el mismo nombre".to_string());
        }
        // Libre entre las fichas del host (también las que no se pueden leer) y
        // entre las sesiones que ya se ven en él.
        let en_fichas = crate::almacen::sesiones_persistentes::nombres_de_host(
            self.almacen.conexion(),
            objetivo.host_id,
        )
        .unwrap_or_default();
        let en_el_host = self.filas_persistentes_todas().into_iter().any(|fila| {
            fila.host_id == objetivo.host_id
                && fila.tipo != TipoFila::Habitual
                && fila.nombre == nuevo
        });
        if en_fichas.contains(&nuevo) || en_el_host {
            return Some(format!(
                "ya hay otra sesión con ese nombre en {}",
                objetivo.host_nombre
            ));
        }
        let peticion_id = self.nueva_peticion_persistentes();
        self.servidor.enviar(MensajeCliente::RenombrarTmux {
            peticion_id,
            host_id: objetivo.host_id,
            nombre: objetivo.nombre.clone(),
            nuevo: nuevo.clone(),
        });
        self.persistentes.peticiones.insert(
            peticion_id,
            PeticionPersistente::Renombrar {
                objetivo: objetivo.clone(),
                nuevo,
            },
        );
        self.mensaje(
            format!(
                "renombrando «{}» en {}…",
                objetivo.nombre, objetivo.host_nombre
            ),
            false,
        );
        None
    }

    /// Llega `TmuxRenombrada`. La ficha solo cambia cuando el host dijo `ok`
    /// (o la sesión ya no existía: entonces solo se renombra la ficha).
    fn renombrada(&mut self, peticion_id: u64, resultado: &ResultadoRenombrar) {
        let Some(PeticionPersistente::Renombrar { objetivo, nuevo }) =
            self.persistentes.peticiones.remove(&peticion_id)
        else {
            // Una respuesta de una petición que ya no está en vuelo.
            return;
        };
        let host = objetivo.host_nombre.clone();
        match resultado {
            ResultadoRenombrar::Ok => {
                let cambio = self.renombrar_ficha_de(&objetivo, &nuevo);
                match cambio {
                    Ok(()) => self.mensaje(
                        format!("«{}» ahora se llama «{nuevo}» en {host}", objetivo.nombre),
                        false,
                    ),
                    Err(error) => self.mensaje(
                        format!(
                            "«{}» se renombró en {host}, pero no su ficha: {error}",
                            objetivo.nombre
                        ),
                        true,
                    ),
                }
                self.consultar_persistentes(None, true);
            }
            ResultadoRenombrar::NoExiste => {
                if objetivo.uuid.is_some() {
                    match self.renombrar_ficha_de(&objetivo, &nuevo) {
                        Ok(()) => self.mensaje(
                            format!(
                                "«{}» ya no existía en {host}: solo se ha renombrado la ficha",
                                objetivo.nombre
                            ),
                            false,
                        ),
                        Err(error) => self.mensaje(
                            format!(
                                "no se pudo renombrar la ficha de «{}»: {error}",
                                objetivo.nombre
                            ),
                            true,
                        ),
                    }
                } else {
                    self.mensaje(
                        format!("«{}» ya no existe en {host}", objetivo.nombre),
                        true,
                    );
                }
                self.consultar_persistentes(None, true);
            }
            ResultadoRenombrar::Ocupado => self.mensaje(
                format!("ya hay otra sesión con el nombre «{nuevo}» en {host}"),
                true,
            ),
            ResultadoRenombrar::Error(motivo) => self.mensaje(
                format!(
                    "no se pudo renombrar «{}» en {host}: {}",
                    objetivo.nombre,
                    limpiar_texto(motivo)
                ),
                true,
            ),
        }
        self.sucio = true;
    }

    /// Cambia el nombre de la ficha (versión nueva, viaja). Sin ficha, no hay
    /// nada que cambiar.
    fn renombrar_ficha_de(&mut self, objetivo: &ObjetivoSesion, nuevo: &str) -> Result<(), String> {
        let Some(uuid) = &objetivo.uuid else {
            return Ok(());
        };
        let conexion = self.almacen.conexion();
        match crate::almacen::sesiones_persistentes::por_uuid(conexion, uuid) {
            Ok(Some(ficha)) => {
                crate::almacen::sesiones_persistentes::renombrar(conexion, ficha.id, nuevo)
                    .map_err(|error| texto_de_error_de_ficha(&error))
            }
            Ok(None) => Err("la ficha ya no existe".to_string()),
            Err(error) => Err(error.to_string()),
        }
    }

    // ------------------------------------------------------------- terminar

    /// `T`: pregunta antes de terminar (nombra la sesión y el host).
    fn preguntar_terminar_sesion(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        if fila.tipo == TipoFila::Ficha && fila.estado == EstadoFicha::Terminada {
            self.mensaje(
                format!(
                    "«{}» ya está terminada ({} la vuelve a crear)",
                    fila.nombre, self.tema.glifos.intro
                ),
                false,
            );
            return;
        }
        if fila.estado == EstadoFicha::SinTmux {
            self.mensaje(
                format!("{} no tiene tmux 3.0 o posterior", fila.host_nombre),
                true,
            );
            return;
        }
        let mut lineas = vec![
            format!("Terminar «{}» en {}.", fila.nombre, fila.host_nombre),
            "Se cerrará lo que corre dentro, y no se puede deshacer.".to_string(),
        ];
        let este = self.persistentes.etiqueta_equipo.as_deref().unwrap_or("");
        if fila.enganchada() {
            lineas.push(format!(
                "Ahora la tiene enganchada {}.",
                quien_la_tiene(fila_equipo(&fila), este)
            ));
        }
        match fila.tipo {
            TipoFila::Ficha => {
                lineas.push("La ficha se queda como «terminada»: puedes reabrirla.".to_string());
            }
            TipoFila::SinFicha => {}
            TipoFila::Habitual => {
                lineas.push("Es una sesión de tu tmux de siempre.".to_string());
            }
        }
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "TERMINAR SESIÓN".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Persistentes(AccionPersistentes::Terminar {
                objetivo: fila.objetivo(),
            }),
        });
        self.sucio = true;
    }

    /// Confirmado: manda `TerminarTmux`. `tmux_terminada` lo anota el servidor,
    /// donde ocurre el efecto: aquí no se anota otra vez.
    fn terminar_sesion_persistente(&mut self, objetivo: ObjetivoSesion) {
        let peticion_id = self.nueva_peticion_persistentes();
        self.servidor.enviar(MensajeCliente::TerminarTmux {
            peticion_id,
            host_id: objetivo.host_id,
            servidor: objetivo.servidor(),
            nombre: objetivo.nombre.clone(),
        });
        self.mensaje(
            format!(
                "terminando «{}» en {}…",
                objetivo.nombre, objetivo.host_nombre
            ),
            false,
        );
        self.persistentes
            .peticiones
            .insert(peticion_id, PeticionPersistente::Terminar { objetivo });
    }

    /// Llega `TmuxTerminada` de una terminación pedida desde aquí.
    fn sesion_terminada(&mut self, peticion_id: u64, resultado: ResultadoTmux) {
        let Some(PeticionPersistente::Terminar { objetivo }) =
            self.persistentes.peticiones.remove(&peticion_id)
        else {
            return;
        };
        let host = objetivo.host_nombre.clone();
        match resultado {
            ResultadoTmux::Ok => {
                self.mensaje(format!("«{}» terminada en {host}", objetivo.nombre), false);
            }
            ResultadoTmux::NoExiste => {
                self.mensaje(
                    format!("«{}» ya no existía en {host}", objetivo.nombre),
                    true,
                );
            }
        }
        // El host ya no la tiene: así se ve hasta que el servidor lo vuelva a
        // mirar, que se pide ahora.
        self.persistentes.terminadas.retain(|terminada| {
            !(terminada.host_id == objetivo.host_id
                && terminada.servidor == objetivo.servidor()
                && terminada.nombre == objetivo.nombre)
        });
        self.persistentes.terminadas.push(TerminadaLocal {
            host_id: objetivo.host_id,
            servidor: objetivo.servidor(),
            nombre: objetivo.nombre,
            epoca: ahora_epoca(),
        });
        self.consultar_persistentes(None, true);
        self.sucio = true;
    }

    // --------------------------------------------------------------- borrar

    /// `x`: pregunta antes de borrar la ficha (no toca el host).
    fn preguntar_borrar_ficha(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        if fila.tipo != TipoFila::Ficha {
            self.mensaje(format!("«{}» no tiene ficha que borrar", fila.nombre), true);
            return;
        }
        let viva = match fila.estado {
            EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada => Some(true),
            EstadoFicha::Terminada => Some(false),
            _ => None,
        };
        let mut lineas = vec![format!(
            "¿Borrar la ficha de «{}» en {}?",
            fila.nombre, fila.host_nombre
        )];
        lineas.push(match viva {
            Some(true) => format!(
                "La sesión seguirá viva en {}, sin ficha: la verás como «sin ficha».",
                fila.host_nombre
            ),
            Some(false) => "La sesión ya no existe en el host: solo se quita la ficha.".to_string(),
            None => format!(
                "No se pudo comprobar {}: si la sesión sigue viva, seguirá en el host sin ficha.",
                fila.host_nombre
            ),
        });
        // Lo de los otros equipos solo es cierto con la sincronización
        // configurada (D370): sin ella la ficha no viaja a ningún sitio.
        lineas.push(if self.config.sincronizacion.activa() {
            "Borrar la ficha no toca el host; se borra también en tus otros equipos.".to_string()
        } else {
            "Borrar la ficha no toca el host.".to_string()
        });
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "BORRAR FICHA".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Persistentes(AccionPersistentes::BorrarFicha {
                objetivo: fila.objetivo(),
                viva,
            }),
        });
        self.sucio = true;
    }

    /// Borra una ficha y anota `sesion_persistente_borrada` solo si se borró.
    fn borrar_ficha_de(&mut self, objetivo: &ObjetivoSesion, viva: Option<bool>) -> bool {
        let Some(uuid) = &objetivo.uuid else {
            return false;
        };
        let conexion = self.almacen.conexion();
        let ficha = match crate::almacen::sesiones_persistentes::por_uuid(conexion, uuid) {
            Ok(Some(ficha)) => ficha,
            Ok(None) => {
                self.mensaje(
                    format!("la ficha de «{}» ya no existe", objetivo.nombre),
                    true,
                );
                return false;
            }
            Err(error) => {
                self.mensaje(format!("no se pudo leer la ficha: {error}"), true);
                return false;
            }
        };
        if let Err(error) = crate::almacen::sesiones_persistentes::borrar(conexion, ficha.id) {
            self.mensaje(format!("no se pudo borrar la ficha: {error}"), true);
            return false;
        }
        let mut detalle = format!("«{}» · {}", objetivo.host_nombre, objetivo.nombre);
        if viva == Some(true) {
            detalle.push_str(" · la sesión sigue en el host");
        }
        self.anotar(
            crate::registro::SESION_PERSISTENTE_BORRADA,
            Some(objetivo.host_id),
            None,
            &detalle,
            ResultadoRegistro::Ok,
        );
        true
    }

    /// `C`: pregunta antes de borrar las fichas terminadas (con recuento).
    fn preguntar_limpiar_terminadas(&mut self) {
        let todas = self.filas_persistentes_todas();
        let terminadas: Vec<&FilaPersistente> = todas
            .iter()
            .filter(|fila| fila.tipo == TipoFila::Ficha && fila.estado == EstadoFicha::Terminada)
            .collect();
        if terminadas.is_empty() {
            self.mensaje("no hay fichas terminadas que borrar", false);
            return;
        }
        // Una lista vieja puede dar por terminada una sesión que ya vive.
        let ahora = ahora_epoca();
        let mas_vieja = terminadas
            .iter()
            .map(|fila| ahora.saturating_sub(fila.comprobada_en))
            .max()
            .unwrap_or(0);
        if mas_vieja > EDAD_MAXIMA_PARA_ESCRIBIR {
            self.mensaje(
                format!(
                    "la comprobación es de {}: pulsa r y vuelve a pulsar C",
                    hace(mas_vieja)
                ),
                true,
            );
            return;
        }
        let total = terminadas.len();
        let mut lineas = vec![format!(
            "Borrar {total} {} terminada{}:",
            if total == 1 { "ficha" } else { "fichas" },
            if total == 1 { "" } else { "s" }
        )];
        for fila in terminadas.iter().take(6) {
            lineas.push(format!("  {} ({})", fila.nombre, fila.host_nombre));
        }
        if total > 6 {
            lineas.push(format!("  …y {} más", total - 6));
        }
        lineas.push("Las sesiones ya no existen en sus hosts.".to_string());
        let fichas = terminadas.iter().map(|fila| fila.objetivo()).collect();
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "BORRAR FICHAS TERMINADAS".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Persistentes(AccionPersistentes::LimpiarTerminadas { fichas }),
        });
        self.sucio = true;
    }

    // ---------------------------------------------------------------- añadir

    /// `a`: añade a la lista una sesión sin ficha (origen `descubierta`). La
    /// sesión ya existe en el host, así que la ficha se guarda ahora.
    fn anadir_sesion_sin_ficha(&mut self) {
        let Some(fila) = self.fila_persistente_elegida() else {
            self.mensaje("no hay ninguna sesión persistente elegida", true);
            return;
        };
        match fila.tipo {
            TipoFila::Ficha => {
                self.mensaje(format!("«{}» ya tiene ficha", fila.nombre), false);
                return;
            }
            TipoFila::Habitual => {
                self.mensaje(
                    "las sesiones de tu tmux de siempre no se añaden: se enganchan sin ficha",
                    true,
                );
                return;
            }
            TipoFila::SinFicha => {}
        }
        if !nombre_valido(&fila.nombre) {
            self.mensaje(
                format!("el nombre «{}» no es válido para una ficha", fila.nombre),
                true,
            );
            return;
        }
        // La ficha solo se guarda si la sesión existe ahora en el host: con una
        // lista vieja (la de una ventana que se cerró hace rato) no se sabe.
        let edad = ahora_epoca().saturating_sub(fila.comprobada_en);
        if fila.comprobada_en <= 0 || edad > EDAD_MAXIMA_PARA_ESCRIBIR {
            self.mensaje(
                format!(
                    "la comprobación de {} es de {}: pulsa r y vuelve a pulsar a",
                    fila.host_nombre,
                    hace(edad)
                ),
                true,
            );
            return;
        }
        let datos = DatosFicha {
            nombre: fila.nombre.clone(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Descubierta,
            creada_por: self.nombre_de_este_equipo(),
        };
        match crate::almacen::sesiones_persistentes::crear(
            self.almacen.conexion(),
            fila.host_id,
            &datos,
        ) {
            Ok(_) => {
                self.anotar(
                    crate::registro::SESION_PERSISTENTE_CREADA,
                    Some(fila.host_id),
                    None,
                    &format!(
                        "«{}» · {} · {}",
                        fila.host_nombre,
                        fila.nombre,
                        OrigenFicha::Descubierta.texto()
                    ),
                    ResultadoRegistro::Ok,
                );
                self.mensaje(
                    format!("«{}» añadida a las sesiones persistentes", fila.nombre),
                    false,
                );
                self.consultar_persistentes(None, true);
            }
            Err(error) => self.mensaje(texto_de_error_de_ficha(&error), true),
        }
        self.sucio = true;
    }

    // --------------------------------------------------------- confirmadas

    /// Ejecuta una acción confirmada: cada una lleva fijados los datos.
    pub(crate) fn ejecutar_accion_persistentes(&mut self, accion: AccionPersistentes) {
        match accion {
            AccionPersistentes::Terminar { objetivo } => {
                self.terminar_sesion_persistente(objetivo);
            }
            AccionPersistentes::BorrarFicha { objetivo, viva } => {
                if self.borrar_ficha_de(&objetivo, viva) {
                    self.mensaje(format!("ficha de «{}» borrada", objetivo.nombre), false);
                    self.consultar_persistentes(None, true);
                }
            }
            AccionPersistentes::LimpiarTerminadas { fichas } => {
                let mut borradas = 0usize;
                for objetivo in &fichas {
                    if self.borrar_ficha_de(objetivo, Some(false)) {
                        borradas += 1;
                    }
                }
                // Cada fallo dejó su propio mensaje, pero el último pisa a los
                // demás: el resumen dice también cuántas no se borraron.
                let sin_borrar = fichas.len() - borradas;
                if borradas > 0 || sin_borrar > 0 {
                    let mut texto = if borradas == 0 {
                        String::new()
                    } else {
                        format!(
                            "{borradas} {} borrada{}",
                            if borradas == 1 {
                                "ficha terminada"
                            } else {
                                "fichas terminadas"
                            },
                            if borradas == 1 { "" } else { "s" }
                        )
                    };
                    if sin_borrar > 0 {
                        if !texto.is_empty() {
                            texto.push_str(" · ");
                        }
                        texto.push_str(&format!(
                            "{sin_borrar} no se {} borrar (ya no existían o falló el almacén)",
                            if sin_borrar == 1 { "pudo" } else { "pudieron" }
                        ));
                    }
                    self.mensaje(texto, sin_borrar > 0);
                    self.consultar_persistentes(None, true);
                }
            }
        }
        self.sucio = true;
    }

    // --------------------------------------------------------------- errores

    /// Un `Error` del servidor a una de las peticiones de este módulo.
    fn error_de_persistentes(&mut self, peticion_id: u64, mensaje: &str) {
        let mensaje = limpiar_texto(mensaje);
        if self
            .persistentes
            .consulta
            .as_ref()
            .is_some_and(|consulta| consulta.peticion_id == peticion_id)
        {
            self.persistentes.consulta = None;
            self.mensaje(
                format!("no se pudieron comprobar las sesiones: {mensaje}"),
                true,
            );
        } else if let Some(vista) = self.buscar_vista_previa_sesion_mut(peticion_id) {
            vista.peticion_id = None;
            vista.error = Some(mensaje);
        } else {
            match self.persistentes.peticiones.remove(&peticion_id) {
                Some(PeticionPersistente::Renombrar { objetivo, .. }) => self.mensaje(
                    format!(
                        "no se pudo renombrar «{}» en {}: {mensaje}",
                        objetivo.nombre, objetivo.host_nombre
                    ),
                    true,
                ),
                Some(PeticionPersistente::Terminar { objetivo }) => self.mensaje(
                    format!(
                        "no se pudo terminar «{}» en {}: {mensaje}",
                        objetivo.nombre, objetivo.host_nombre
                    ),
                    true,
                ),
                None => {}
            }
        }
        self.sucio = true;
    }

    // --------------------------------------------------------------- paleta

    /// Las entradas de la paleta de las sesiones persistentes: «nueva sesión
    /// persistente» y «engancharse a <nombre>» (una por ficha viva o
    /// terminada, con el host).
    pub(crate) fn entradas_paleta_persistentes(&self) -> Vec<EntradaPaleta> {
        let mut entradas = vec![EntradaPaleta {
            etiqueta: "nueva sesión persistente".to_string(),
            categoria: "sesiones",
            accion: AccionPaleta::Persistentes(AccionPaletaPersistentes::Nueva),
        }];
        for fila in self.filas_persistentes_todas() {
            let util = fila.tipo == TipoFila::Ficha
                && matches!(
                    fila.estado,
                    EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada | EstadoFicha::Terminada
                );
            if util {
                entradas.push(EntradaPaleta {
                    etiqueta: format!("engancharse a {} · {}", fila.nombre, fila.host_nombre),
                    categoria: "sesiones",
                    accion: AccionPaleta::Persistentes(AccionPaletaPersistentes::Enganchar {
                        host_id: fila.host_id,
                        nombre: fila.nombre,
                    }),
                });
            }
        }
        entradas
    }

    /// Una entrada de la paleta.
    pub(crate) fn accion_paleta_persistentes(&mut self, accion: AccionPaletaPersistentes) {
        match accion {
            AccionPaletaPersistentes::Nueva => self.abrir_dialogo_nueva_sesion(None),
            AccionPaletaPersistentes::Enganchar { host_id, nombre } => {
                let fila = self.filas_persistentes_todas().into_iter().find(|fila| {
                    fila.tipo == TipoFila::Ficha && fila.host_id == host_id && fila.nombre == nombre
                });
                match fila {
                    Some(fila) => self.enganchar_objetivo(&fila.objetivo(), false),
                    None => self.mensaje(format!("«{nombre}» ya no está en la lista"), true),
                }
            }
        }
    }

    /// Las sesiones que piden atención según la última lista del servidor, en
    /// el orden de la lista: las fichas vivas con la campana y las sesiones sin
    /// ficha del tmux propio de los hosts comprobados. Las del tmux habitual no
    /// cuentan (lo usa el aviso de la barra global de la pista B).
    ///
    /// Solo cuenta lo comprobado desde que esta ventana se conectó (menos la
    /// repetición mínima del servidor, como en `magi sesiones`, y un pequeño
    /// margen, `MARGEN_DE_CONEXION_SEG`): la lista que llega detrás de la
    /// `Bienvenida` puede ser la guardada al irse la última ventana, y una
    /// campana de hace horas no es una atención de ahora. El servidor hace una
    /// pasada enseguida y difunde la lista fresca (D368).
    #[must_use]
    pub fn persistentes_con_atencion(&self) -> Vec<AtencionPersistente> {
        let Some(lista) = &self.persistentes.lista else {
            return Vec::new();
        };
        let umbral = self.persistentes.conectada_en.map_or(0, |conectada| {
            crate::sesiones_cli::umbral_de_frescura(
                conectada.saturating_sub(MARGEN_DE_CONEXION_SEG),
            )
        });
        let host_nombre = |host_id: i64| {
            self.hosts
                .iter()
                .find(|host| host.id == host_id)
                .map(|host| host.nombre.clone())
                .unwrap_or_else(|| format!("host {host_id}"))
        };
        let mut atencion = Vec::new();
        for ficha in &lista.fichas {
            let campana = ficha.sesion.as_ref().is_some_and(|sesion| sesion.campana);
            if campana && ficha.comprobada_en >= umbral {
                atencion.push(AtencionPersistente {
                    host_id: ficha.host_id,
                    host_nombre: host_nombre(ficha.host_id),
                    nombre: ficha.nombre.clone(),
                });
            }
        }
        for host in &lista.hosts {
            if host.comprobada_en < umbral {
                continue;
            }
            for sesion in &host.sin_ficha {
                if sesion.campana && sesion.servidor == protocolo::ServidorTmux::Magi {
                    atencion.push(AtencionPersistente {
                        host_id: host.host_id,
                        host_nombre: host_nombre(host.host_id),
                        nombre: sesion.nombre.clone(),
                    });
                }
            }
        }
        atencion
    }
}

/// El equipo que tiene la sesión de una fila, si consta.
fn fila_equipo(fila: &FilaPersistente) -> Option<&str> {
    fila.sesion
        .as_ref()
        .and_then(|sesion| sesion.equipo.as_deref())
}

/// Texto de lo que llega del host o del servidor, sin caracteres de control
/// (los tabuladores pasan a espacios): entrada no fiable.
fn limpiar_texto(texto: &str) -> String {
    texto
        .chars()
        .map(|caracter| if caracter == '\t' { ' ' } else { caracter })
        .filter(|caracter| !caracter.is_control())
        .collect()
}

/// Marca el campo del formulario al que se refiere un error de la ficha.
fn poner_error_de_ficha(formulario: &mut FormularioFicha, error: &ErrorFicha) {
    let campo = match error.campo() {
        CampoFicha::Carpeta => 0,
        CampoFicha::Comando => 1,
        CampoFicha::ComandoReanudar => 2,
        CampoFicha::Nota => 3,
        _ => formulario.foco,
    };
    formulario.foco = campo;
    formulario.error = Some((campo, error.to_string()));
}

/// El texto de un error del almacén de fichas: el del propio error si es de
/// validación y, si no, el de SQLite o el sistema.
fn texto_de_error_de_ficha(error: &anyhow::Error) -> String {
    error.to_string()
}
