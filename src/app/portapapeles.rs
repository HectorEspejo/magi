//! Portapapeles del cliente (Fase 15): copiar texto y peticiones del remoto.
//!
//! Copiar usa `wl-copy`/`pbcopy` (F2) y, si no hay o MAGI corre por SSH,
//! OSC 52 hacia el terminal exterior. El remoto (OSC 52) solo escribe en el
//! portapapeles con permiso por host y siempre con aviso; sin permiso se
//! guarda solo su última petición, 60 s, hasta que `prefijo P` la permite.
//!
//! Desde la Fase 17 hay una sola vía para copiar: [`App::copiar`] (y su forma
//! corta `copiar_texto`). La copia de una clave pública, del enlace del visor
//! o de la clave recién generada pasan por ella y solo cambian en `ParaQue`,
//! que decide qué se dice en la barra y qué hacer si no se pudo. En Termux la
//! decisión (`termux::portapapeles`) elige entre `termux-clipboard-set` (en
//! una tarea, con plazo) y el OSC 52, que escribe siempre el hilo de la
//! interfaz.
//!
//! En el escritorio nada cambia respecto a la Fase 15: esas tres vías usan solo
//! la herramienta del sistema (`wl-copy`, `pbcopy`) y, si no hay, conservan su
//! respaldo de siempre (el diálogo con la línea a mano, el enlace a la vista);
//! el OSC 52 solo lo usan el historial, los enlaces del diálogo y lo que copia
//! un host (`Texto` y `Remoto`), como entonces. La clave recién generada es la
//! excepción en el sitio, no en el resultado: su copia la hace la tarea de
//! `identidades::generar`, como antes de la Fase 17 (la espera a la
//! herramienta no cae en el hilo de la interfaz), y `App::publica_generada`
//! solo dice lo que pasó. Las copias de la clave de una identidad y del enlace
//! del visor siguen en el hilo de la interfaz, igual que desde la Fase 2 y la
//! 12.
//!
//! En Termux las copias por la orden se serializan: una en vuelo y, detrás, la
//! última que se pidió (la anterior que esperaba se da por no hecha y su
//! `ParaQue` lo dice). Así el portapapeles queda con lo último que se pidió
//! aunque una orden vieja sea lenta.

use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::app::historial::EstadoHistorial;
use crate::app::termux::{CopiaPendiente, CopiaTerminada, EventoTermux};
use crate::app::{App, Dialogo, Evento};
use crate::termux::portapapeles::{
    mensaje_copiado_api, plan_osc52, planificar, secuencia_osc52, PlanCopia, AVISO_API_APAGADA,
    COPIADO_OSC52,
};

/// Segundos que se guarda una petición del remoto sin permiso.
pub const PENDIENTE: Duration = Duration::from_secs(60);

/// Por qué no se hizo una copia que esperaba su turno y se pidió otra después.
pub const COPIA_SUSTITUIDA: &str = "sustituida por una copia más reciente";

/// De dónde salió la copia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    /// Se copió con `wl-copy` o `pbcopy`.
    Sistema(&'static str),
    /// Se copió con la herramienta del sistema en la tarea que generó la
    /// clave (escritorio): el resultado llega ya hecho y no se sabe con cuál.
    TareaDelEscritorio,
    /// Se pidió al terminal exterior con OSC 52.
    Osc52,
    /// Termux: `termux-clipboard-set` respondió; son los bytes copiados.
    TermuxApi(usize),
    /// Termux: OSC 52 escrito por el hilo de la interfaz.
    TermuxOsc52,
    /// Sin efectos externos (pruebas): no se tocó el portapapeles.
    Simulado,
}

/// Para qué se copia: decide qué dice la barra al terminar y qué se hace si no
/// se pudo copiar. Lleva fijados los datos sobre los que actúa (el escritorio
/// del enlace, el resumen de la clave), porque en Termux el resultado llega
/// más tarde y no se relee el estado.
#[derive(Debug, Clone)]
pub enum ParaQue {
    /// Un texto cualquiera («historial (12 líneas) copiado con wl-copy»).
    Texto(String),
    /// Lo que un host copió con OSC 52 y tiene permiso: «web-01 copió 1,2 KB
    /// al portapapeles». Solo en Termux, donde el resultado llega después; en
    /// el escritorio sigue siendo `Texto` y el aviso detrás.
    Remoto { host: String },
    /// La clave pública de una identidad: si no se pudo, un diálogo con la
    /// línea para copiarla a mano.
    ClavePublica,
    /// El enlace de un solo uso del visor de un escritorio: si no se pudo, se
    /// queda a la vista en su pestaña.
    EnlaceVisor { escritorio_id: u32 },
    /// La clave pública recién generada: su resumen (lo que dice la barra de la
    /// generación) y si es un aviso; la copia le añade su parte.
    ClaveGenerada { resumen: String, error: bool },
}

/// Cómo terminó una copia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinCopia {
    /// Copiado.
    Copiado(Origen),
    /// No se copió y el texto es el mensaje completo de la barra (Termux:
    /// demasiado grande o no cabe en el OSC 52).
    Rechazado(String),
    /// No se pudo copiar, con el motivo.
    Fallo(String),
}

/// Una petición de copia del remoto (OSC 52) sin permiso.
#[derive(Debug, Clone)]
pub struct Pendiente {
    pub sesion_id: u32,
    pub host_nombre: String,
    pub bytes: u64,
    pub desde: Instant,
}

impl Pendiente {
    /// ¿Ha caducado?
    #[must_use]
    pub fn caducada(&self, ahora: Instant) -> bool {
        ahora.duration_since(self.desde) > PENDIENTE
    }
}

/// Estado del portapapeles del remoto en esta ventana.
#[derive(Debug, Clone, Default)]
pub struct EstadoPortapapelesRemoto {
    /// Solo hay una petición pendiente: la última.
    pub pendiente: Option<Pendiente>,
    /// Copias del remoto entregadas en esta sesión (para las pruebas).
    pub copias: u64,
}

impl EstadoPortapapelesRemoto {
    /// Guarda una petición sin permiso (sustituye la anterior).
    pub fn pedir(&mut self, sesion_id: u32, host_nombre: String, bytes: u64) {
        self.pendiente = Some(Pendiente {
            sesion_id,
            host_nombre,
            bytes,
            desde: Instant::now(),
        });
    }

    /// La petición pendiente, si sigue viva.
    #[must_use]
    pub fn vigente(&self, ahora: Instant) -> Option<&Pendiente> {
        self.pendiente
            .as_ref()
            .filter(|pendiente| !pendiente.caducada(ahora))
    }

    /// Se lleva la petición al permitirla.
    pub fn tomar(&mut self) -> Option<Pendiente> {
        self.pendiente.take()
    }

    /// Descarta la petición si ha caducado.
    pub fn limpiar(&mut self, ahora: Instant) -> bool {
        if self.pendiente.as_ref().is_some_and(|p| p.caducada(ahora)) {
            self.pendiente = None;
            return true;
        }
        false
    }
}

/// Tamaño legible de una copia: `1,2 KB` o `312 B`.
#[must_use]
pub fn tamano_legible(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB").replace('.', ",");
    }
    let mb = kb / 1024.0;
    format!("{mb:.1} MB").replace('.', ",")
}

/// ¿MAGI corre dentro de otra sesión SSH? Entonces OSC 52 sale por ella.
#[must_use]
pub fn dentro_de_ssh() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some()
}

/// Copia al portapapeles del sistema y, si no hay, al terminal exterior con
/// OSC 52. Devuelve de dónde salió.
pub fn copiar(texto: &str) -> Result<Origen, String> {
    if dentro_de_ssh() {
        return osc52(texto);
    }
    match crate::portapapeles::copiar(texto) {
        Ok(herramienta) => Ok(Origen::Sistema(herramienta.nombre())),
        Err(motivo) => match osc52(texto) {
            Ok(origen) => Ok(origen),
            Err(error) => Err(format!("{motivo}; y OSC 52 falló: {error}")),
        },
    }
}

/// Copia solo con la herramienta del sistema (`wl-copy`, `pbcopy`), sin OSC 52:
/// la clave pública de una identidad, el enlace del visor y la clave recién
/// generada tienen su respaldo (el diálogo, el enlace a la vista) y no deben
/// darse por copiados por una vía que no confirma nada.
pub fn copiar_con_herramienta(texto: &str) -> Result<Origen, String> {
    crate::portapapeles::copiar(texto).map(|herramienta| Origen::Sistema(herramienta.nombre()))
}

/// ¿Puede esta copia, en el escritorio, caer al OSC 52 si no hay herramienta?
/// Solo las que ya podían en la Fase 15: un texto cualquiera (el historial, el
/// enlace del diálogo) y lo que copia un host.
fn admite_osc52_en_el_escritorio(para: &ParaQue) -> bool {
    matches!(para, ParaQue::Texto(_) | ParaQue::Remoto { .. })
}

/// Copia por OSC 52 hacia el terminal exterior.
fn osc52(texto: &str) -> Result<Origen, String> {
    use base64::Engine as _;
    use std::io::Write as _;
    let codificado = base64::engine::general_purpose::STANDARD.encode(texto.as_bytes());
    let mut salida = std::io::stdout();
    write!(salida, "\x1b]52;c;{codificado}\x07").map_err(|error| error.to_string())?;
    salida.flush().map_err(|error| error.to_string())?;
    Ok(Origen::Osc52)
}

/// Escribe una secuencia en la terminal exterior. Solo la llama el hilo de la
/// interfaz: ninguna tarea escribe en la terminal.
fn escribir_en_terminal(secuencia: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let mut salida = std::io::stdout();
    salida
        .write_all(secuencia)
        .map_err(|error| error.to_string())?;
    salida.flush().map_err(|error| error.to_string())
}

// ------------------------------------------------------------------ la App

impl App {
    /// Copia texto al portapapeles del sistema o por OSC 52 (en Termux, por
    /// `termux-clipboard-set` o por OSC 52). `que` es lo que se dice de él en
    /// la barra.
    pub(crate) fn copiar_texto(&mut self, texto: &str, que: &str) {
        self.copiar(texto.to_string(), ParaQue::Texto(que.to_string()));
    }

    /// La única vía para copiar al portapapeles (Fase 17). En el escritorio es
    /// síncrona, como siempre; en Termux la orden va en una tarea y el
    /// resultado vuelve por `Evento::Termux`. `para` decide lo que dice la
    /// barra y qué se hace si no se pudo copiar.
    pub(crate) fn copiar(&mut self, texto: String, para: ParaQue) {
        let texto = Zeroizing::new(texto);
        if texto.is_empty() {
            self.mensaje("no hay nada que copiar", true);
            return;
        }
        if self.termux.activo {
            self.copiar_en_termux(texto, para);
            return;
        }
        if !EstadoHistorial::cabe_en_copia(&texto) {
            self.mensaje(
                "demasiado grande para copiar (más de 1 MiB): w para guardarlo",
                true,
            );
            return;
        }
        if !self.efectos_externos {
            self.terminar_copia(&texto, para, FinCopia::Copiado(Origen::Simulado));
            return;
        }
        let copia = if admite_osc52_en_el_escritorio(&para) {
            copiar(&texto)
        } else {
            copiar_con_herramienta(&texto)
        };
        let fin = match copia {
            Ok(origen) => FinCopia::Copiado(origen),
            Err(motivo) => FinCopia::Fallo(motivo),
        };
        self.terminar_copia(&texto, para, fin);
    }

    /// La clave pública recién generada, con lo que su tarea dijo de la copia
    /// (`identidades::generar`). En el escritorio la tarea ya la intentó con la
    /// herramienta del sistema, fuera del hilo de la interfaz: si la copió se
    /// dice, y si no, sale el respaldo de siempre sin repetir la copia aquí (que
    /// esperaría a la herramienta hasta 750 ms con la interfaz parada). Si la
    /// tarea no lo intentó (Termux, donde la copia es de `App::copiar`), se
    /// copia por la única vía.
    pub(crate) fn publica_generada(
        &mut self,
        publica: String,
        copiada: bool,
        fallo_copia: Option<String>,
        resumen: String,
        error: bool,
    ) {
        let para = ParaQue::ClaveGenerada { resumen, error };
        if copiada {
            self.terminar_copia(
                &publica,
                para,
                FinCopia::Copiado(Origen::TareaDelEscritorio),
            );
        } else if let Some(motivo) = fallo_copia {
            self.terminar_copia(&publica, para, FinCopia::Fallo(motivo));
        } else {
            self.copiar(publica, para);
        }
    }

    /// Lo que se hace y se dice cuando la copia ha terminado, sea cual sea el
    /// camino que siguió.
    pub(crate) fn terminar_copia(&mut self, texto: &str, para: ParaQue, fin: FinCopia) {
        match para {
            ParaQue::Texto(que) => {
                let (mensaje, error) = mensaje_de_texto(&que, fin);
                self.mensaje(mensaje, error);
            }
            ParaQue::Remoto { host } => match fin {
                FinCopia::Copiado(_) => {
                    let bytes = texto.len() as u64;
                    self.mensaje(
                        format!("{host} copió {} al portapapeles", tamano_legible(bytes)),
                        false,
                    );
                }
                otro => {
                    let (mensaje, error) = mensaje_de_texto(&host, otro);
                    self.mensaje(mensaje, error);
                }
            },
            ParaQue::ClavePublica => match fin {
                FinCopia::Copiado(origen) => {
                    let mensaje = mensaje_de_exito(origen, |origen| match origen {
                        Origen::Sistema(herramienta) => {
                            format!("clave pública copiada con {herramienta}")
                        }
                        Origen::Osc52 => "clave pública copiada por OSC 52".to_string(),
                        _ => "clave pública copiada".to_string(),
                    });
                    self.mensaje(mensaje, false);
                }
                FinCopia::Rechazado(motivo) | FinCopia::Fallo(motivo) => {
                    self.abrir_dialogo_de_respaldo(
                        Dialogo::Detalle {
                            tunel_caido: None,
                            titulo: "CLAVE PÚBLICA".to_string(),
                            lineas: vec![
                                format!("No se pudo copiar al portapapeles ({motivo})."),
                                String::new(),
                                "Copia la línea a mano:".to_string(),
                                String::new(),
                                texto.to_string(),
                            ],
                        },
                        &format!("no se pudo copiar la clave pública: {motivo}"),
                    );
                }
            },
            ParaQue::EnlaceVisor { escritorio_id } => match fin {
                FinCopia::Copiado(origen) => {
                    let mensaje = mensaje_de_exito(origen, |_| {
                        "enlace del visor copiado (vale 60 s)".to_string()
                    });
                    self.mensaje(mensaje, false);
                }
                FinCopia::Rechazado(motivo) | FinCopia::Fallo(motivo) => {
                    self.enlace_visible = Some((escritorio_id, texto.to_string(), Instant::now()));
                    self.mensaje(
                        format!("no se pudo copiar el enlace: {motivo}; está en la pestaña"),
                        true,
                    );
                }
            },
            ParaQue::ClaveGenerada { resumen, error } => match fin {
                FinCopia::Copiado(_) => {
                    self.mensaje(
                        format!("{resumen} · pública copiada al portapapeles"),
                        error,
                    );
                }
                FinCopia::Rechazado(motivo) | FinCopia::Fallo(motivo) => {
                    self.mensaje(resumen.clone(), error);
                    self.abrir_dialogo_de_respaldo(
                        Dialogo::Detalle {
                            tunel_caido: None,
                            titulo: "CLAVE PÚBLICA".to_string(),
                            lineas: vec![
                                "No se pudo copiar al portapapeles; copia la línea a mano:"
                                    .to_string(),
                                String::new(),
                                texto.trim().to_string(),
                            ],
                        },
                        &format!("{resumen} · no se pudo copiar la pública: {motivo}"),
                    );
                }
            },
        }
        self.sucio = true;
    }

    /// Abre el diálogo con la línea para copiarla a mano. En Termux el resultado
    /// de la copia llega después y el usuario puede tener ya otro diálogo
    /// abierto: este no lo pisa, queda debajo (con `aviso` en la barra) y sale
    /// cuando el otro se cierra.
    fn abrir_dialogo_de_respaldo(&mut self, dialogo: Dialogo, aviso: &str) {
        if self.dialogo.is_some() {
            self.pila_dialogos.insert(0, dialogo);
            self.mensaje(
                format!("{aviso}; la línea saldrá al cerrar este diálogo"),
                true,
            );
        } else {
            self.dialogo = Some(dialogo);
        }
    }

    // ------------------------------------------------------------- Termux

    /// Copia en Termux (§4.4, §7.4): `termux-clipboard-set` por la entrada
    /// estándar si Termux:API no está apagada y el texto cabe; si no, OSC 52 si
    /// cabe; si no, el aviso con el tamaño y la alternativa. Con una orden en
    /// vuelo, la copia espera su turno: nada (ni siquiera el OSC 52) pasa por
    /// delante de lo que se pidió antes.
    fn copiar_en_termux(&mut self, texto: Zeroizing<String>, para: ParaQue) {
        if self.termux.copias_en_curso > 0 {
            self.dejar_copia_pendiente(texto, para);
            return;
        }
        self.planificar_copia_en_termux(texto, para);
    }

    /// Deja `texto` esperando a la orden en vuelo. Solo se guarda una copia
    /// pendiente (la última): la que esperaba antes se da por no hecha y su
    /// `ParaQue` lo dice (el diálogo con la línea, el enlace a la vista...).
    fn dejar_copia_pendiente(&mut self, texto: Zeroizing<String>, para: ParaQue) {
        let nueva = CopiaPendiente { texto, para };
        if let Some(vieja) = self.termux.copia_pendiente.replace(nueva) {
            self.terminar_copia(
                &vieja.texto,
                vieja.para,
                FinCopia::Rechazado(COPIA_SUSTITUIDA.to_string()),
            );
        }
        self.mensaje("copiando…", false);
        self.sucio = true;
    }

    /// Elige el camino de una copia cuando no hay ninguna orden en vuelo.
    fn planificar_copia_en_termux(&mut self, texto: Zeroizing<String>, para: ParaQue) {
        let api_apagada = self.termux.ordenes.api().apagada();
        match planificar(texto.len(), api_apagada) {
            PlanCopia::Rechazada(motivo) => {
                self.terminar_copia(&texto, para, FinCopia::Rechazado(motivo));
            }
            PlanCopia::Osc52 => {
                let fin = self.escribir_osc52_en_termux(&texto);
                self.terminar_copia(&texto, para, fin);
            }
            PlanCopia::Api => self.copiar_por_la_api(texto, para),
        }
    }

    /// Lanza `termux-clipboard-set` en una tarea, con el texto por la entrada
    /// estándar (nunca por un argumento) y el plazo de `[termux] api_plazo_seg`.
    /// El resultado vuelve por `Evento::Termux` con el texto, por si hay que
    /// seguir con el OSC 52.
    fn copiar_por_la_api(&mut self, texto: Zeroizing<String>, para: ParaQue) {
        self.termux.copias_en_curso += 1;
        let ordenes = std::sync::Arc::clone(&self.termux.ordenes);
        let eventos = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            let entrada = texto.as_bytes().to_vec();
            let (resultado, recien_apagada) = ordenes
                .ejecutar_async_avisando(vec!["termux-clipboard-set".to_string()], Some(entrada))
                .await;
            let _ = eventos.send(Evento::Termux(EventoTermux::Copia(CopiaTerminada {
                texto,
                para,
                resultado: resultado.map(|_| ()),
                recien_apagada,
            })));
        });
        self.mensaje("copiando…", false);
        self.sucio = true;
    }

    /// Vuelve `termux-clipboard-set`: copiado, o a OSC 52 si no respondió o
    /// falló. Si esta llamada apagó Termux:API, la barra lo dice una vez.
    pub(crate) fn copia_terminada(&mut self, copia: CopiaTerminada) {
        self.termux.copias_en_curso = self.termux.copias_en_curso.saturating_sub(1);
        let CopiaTerminada {
            texto,
            para,
            resultado,
            recien_apagada,
        } = copia;
        let fin = match resultado {
            Ok(()) => FinCopia::Copiado(Origen::TermuxApi(texto.len())),
            Err(_) => match plan_osc52(texto.len()) {
                PlanCopia::Rechazada(motivo) => FinCopia::Rechazado(motivo),
                _ => self.escribir_osc52_en_termux(&texto),
            },
        };
        self.terminar_copia(&texto, para, fin);
        if recien_apagada {
            self.decir_en_barra(AVISO_API_APAGADA, true);
        }
        // Le toca a la última que esperaba, con el estado de la API de ahora
        // (esta orden pudo apagarla).
        if let Some(pendiente) = self.termux.copia_pendiente.take() {
            self.planificar_copia_en_termux(pendiente.texto, pendiente.para);
        }
    }

    /// Escribe el OSC 52 de `texto` en la terminal exterior. Solo desde el hilo
    /// de la interfaz. Quien llama ya comprobó con `plan_osc52` que cabe. Sin
    /// efectos externos (pruebas) lo anota en vez de escribirlo.
    fn escribir_osc52_en_termux(&mut self, texto: &str) -> FinCopia {
        let secuencia = secuencia_osc52(texto.as_bytes());
        if !self.efectos_externos {
            self.termux
                .osc52_de_prueba
                .push((std::thread::current().id(), secuencia));
            return FinCopia::Copiado(Origen::TermuxOsc52);
        }
        match escribir_en_terminal(&secuencia) {
            Ok(()) => FinCopia::Copiado(Origen::TermuxOsc52),
            Err(error) => FinCopia::Fallo(format!("OSC 52 falló: {error}")),
        }
    }
}

/// El mensaje de un texto copiado (o no) con su etiqueta, como siempre en el
/// escritorio y con los textos de §4.4 en Termux.
fn mensaje_de_texto(que: &str, fin: FinCopia) -> (String, bool) {
    match fin {
        FinCopia::Copiado(origen) => (
            mensaje_de_exito(origen, |origen| match origen {
                Origen::Sistema(herramienta) => format!("{que} copiado con {herramienta}"),
                Origen::Osc52 => format!("{que} copiado por OSC 52"),
                _ => format!("{que} copiado"),
            }),
            false,
        ),
        FinCopia::Rechazado(mensaje) => (mensaje, true),
        FinCopia::Fallo(motivo) => (format!("no se pudo copiar: {motivo}"), true),
    }
}

/// Lo que se dice al copiar: en Termux, siempre lo de §4.4 («copiado (3,2
/// KiB)» o «copiado»); en el escritorio, lo que dé `escritorio` según la
/// herramienta.
fn mensaje_de_exito(origen: Origen, escritorio: impl FnOnce(Origen) -> String) -> String {
    match origen {
        Origen::TermuxApi(bytes) => mensaje_copiado_api(bytes),
        Origen::TermuxOsc52 => COPIADO_OSC52.to_string(),
        otro => escritorio(otro),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_se_guarda_la_ultima_peticion_y_caduca_a_los_sesenta_segundos() {
        let mut estado = EstadoPortapapelesRemoto::default();
        estado.pedir(1, "web-01".to_string(), 10);
        estado.pedir(2, "db-01".to_string(), 20);
        let ahora = Instant::now();
        assert_eq!(estado.vigente(ahora).unwrap().sesion_id, 2);
        assert!(!estado.limpiar(ahora));
        // Sesenta segundos después ya no vale.
        assert!(estado.limpiar(ahora + PENDIENTE + Duration::from_secs(1)));
        assert!(estado.tomar().is_none());
    }

    #[test]
    fn el_escritorio_dice_lo_de_siempre_y_termux_lo_de_la_especificacion() {
        let texto = |que: &str, fin: FinCopia| mensaje_de_texto(que, fin);
        // Escritorio: sin cambios respecto a la Fase 15.
        assert_eq!(
            texto(
                "historial (3 líneas)",
                FinCopia::Copiado(Origen::Sistema("wl-copy"))
            ),
            (
                "historial (3 líneas) copiado con wl-copy".to_string(),
                false
            )
        );
        assert_eq!(
            texto("enlace", FinCopia::Copiado(Origen::Osc52)),
            ("enlace copiado por OSC 52".to_string(), false)
        );
        assert_eq!(
            texto("enlace", FinCopia::Copiado(Origen::Simulado)),
            ("enlace copiado".to_string(), false)
        );
        assert_eq!(
            texto("enlace", FinCopia::Fallo("no hay wl-copy".to_string())),
            ("no se pudo copiar: no hay wl-copy".to_string(), true)
        );
        // Termux: los textos de §4.4, sin la etiqueta.
        assert_eq!(
            texto("historial", FinCopia::Copiado(Origen::TermuxApi(3277))),
            ("copiado (3,2 KiB)".to_string(), false)
        );
        assert_eq!(
            texto("historial", FinCopia::Copiado(Origen::TermuxOsc52)),
            ("copiado".to_string(), false)
        );
        assert_eq!(
            texto("historial", FinCopia::Rechazado("no cabe".to_string())),
            ("no cabe".to_string(), true),
            "el rechazo ya es el mensaje completo"
        );
    }

    // ------------------------------------------------ el escritorio de siempre

    /// Una `App` de prueba de escritorio con efectos externos y una carpeta de
    /// herramientas de copia a medida (por hilo: el `PATH` no se toca). Sin
    /// `fabricar_herramientas`, la carpeta está vacía: no hay `wl-copy` ni
    /// `pbcopy`.
    struct Escritorio {
        app: App,
        carpeta: std::path::PathBuf,
        _enviados: tokio::sync::mpsc::UnboundedReceiver<crate::protocolo::MensajeCliente>,
        _temporal: tempfile::TempDir,
    }

    impl Escritorio {
        fn nuevo() -> Self {
            let temporal = tempfile::Builder::new()
                .prefix("magipe")
                .tempdir()
                .expect("directorio temporal");
            let raiz = temporal.path().to_path_buf();
            let rutas = crate::config::Rutas {
                datos: raiz.join("datos"),
                config: raiz.join("config"),
                estado: raiz.join("estado"),
                hogar: raiz.join("hogar"),
                runtime: raiz.join("runtime"),
            };
            std::fs::create_dir_all(&rutas.datos).expect("datos");
            let almacen = crate::almacen::abrir(&rutas.base_datos()).expect("almacén");
            let (mut app, enviados) = App::de_prueba(
                rutas,
                crate::config::Config::default(),
                crate::tema::Tema::respaldo(),
                almacen,
            )
            .expect("app");
            // Escritorio de verdad: sin Termux y con las herramientas del
            // sistema (las de la carpeta de prueba).
            app.termux.activo = false;
            app.efectos_externos = true;
            let carpeta = raiz.join("herramientas");
            std::fs::create_dir_all(&carpeta).expect("herramientas");
            crate::portapapeles::DIRECTORIO_DE_HERRAMIENTAS
                .with(|directorio| *directorio.borrow_mut() = Some(carpeta.clone()));
            Self {
                app,
                carpeta,
                _enviados: enviados,
                _temporal: temporal,
            }
        }

        /// Pone un `wl-copy` y un `pbcopy` que guardan lo que reciben en
        /// `copiado` (en la propia carpeta de herramientas).
        fn fabricar_herramientas(&self) {
            use std::os::unix::fs::PermissionsExt as _;
            for nombre in ["wl-copy", "pbcopy"] {
                let ruta = self.carpeta.join(nombre);
                std::fs::write(
                    &ruta,
                    format!(
                        "#!/bin/sh\ncat > '{}'\n",
                        self.carpeta.join("copiado").display()
                    ),
                )
                .expect("herramienta");
                std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755))
                    .expect("permisos");
            }
        }

        /// Pone un `wl-copy` y un `pbcopy` que anotan que los llamaron
        /// (`llamado`, en la carpeta de herramientas) y se quedan sirviendo el
        /// portapapeles 5 s, como un daemon: una copia hecha con ellos en el
        /// hilo de la interfaz la deja parada los 750 ms de `ESPERA_COPIA`.
        fn fabricar_herramientas_lentas(&self) {
            use std::os::unix::fs::PermissionsExt as _;
            for nombre in ["wl-copy", "pbcopy"] {
                let ruta = self.carpeta.join(nombre);
                std::fs::write(
                    &ruta,
                    format!(
                        "#!/bin/sh\necho x > '{}'\ncat > /dev/null\nsleep 5\n",
                        self.carpeta.join("llamado").display()
                    ),
                )
                .expect("herramienta");
                std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755))
                    .expect("permisos");
            }
        }

        fn herramienta_llamada(&self) -> bool {
            self.carpeta.join("llamado").exists()
        }

        fn copiado(&self) -> Option<String> {
            // El programa se recoge en segundo plano: se espera a que escriba.
            let limite = Instant::now() + Duration::from_secs(5);
            while Instant::now() < limite {
                if let Ok(texto) = std::fs::read_to_string(self.carpeta.join("copiado")) {
                    if !texto.is_empty() {
                        return Some(texto);
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            None
        }

        fn mensaje(&self) -> Option<(String, bool)> {
            self.app
                .mensaje
                .as_ref()
                .map(|mensaje| (mensaje.texto.clone(), mensaje.error))
        }
    }

    impl Drop for Escritorio {
        fn drop(&mut self) {
            crate::portapapeles::DIRECTORIO_DE_HERRAMIENTAS
                .with(|directorio| *directorio.borrow_mut() = None);
        }
    }

    const SIN_HERRAMIENTA: &str = "no se encontró wl-copy ni pbcopy en el sistema";
    const PUBLICA: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEjemplo usuario@ejemplo.org";

    /// El OSC 52 solo es respaldo de lo que ya lo era en la Fase 15.
    #[test]
    fn solo_el_texto_y_el_remoto_admiten_osc_52_en_el_escritorio() {
        assert!(admite_osc52_en_el_escritorio(&ParaQue::Texto(
            "historial".to_string()
        )));
        assert!(admite_osc52_en_el_escritorio(&ParaQue::Remoto {
            host: "web-01".to_string()
        }));
        assert!(!admite_osc52_en_el_escritorio(&ParaQue::ClavePublica));
        assert!(!admite_osc52_en_el_escritorio(&ParaQue::EnlaceVisor {
            escritorio_id: 1
        }));
        assert!(!admite_osc52_en_el_escritorio(&ParaQue::ClaveGenerada {
            resumen: "clave".to_string(),
            error: false
        }));
    }

    /// Regresión (C1): en el escritorio, sin `wl-copy` ni `pbcopy`, la clave
    /// pública de una identidad abre el diálogo para copiar la línea a mano en
    /// vez de darse por copiada por OSC 52.
    #[test]
    fn la_publica_sin_herramienta_abre_el_dialogo_de_siempre_en_el_escritorio() {
        let mut escritorio = Escritorio::nuevo();
        escritorio
            .app
            .copiar(PUBLICA.to_string(), ParaQue::ClavePublica);
        match &escritorio.app.dialogo {
            Some(Dialogo::Detalle { titulo, lineas, .. }) => {
                assert_eq!(titulo, "CLAVE PÚBLICA");
                assert_eq!(
                    lineas[0],
                    format!("No se pudo copiar al portapapeles ({SIN_HERRAMIENTA}).")
                );
                assert_eq!(lineas[2], "Copia la línea a mano:");
                assert_eq!(lineas.last().map(String::as_str), Some(PUBLICA));
            }
            _ => panic!("sin el diálogo con la línea a mano"),
        }
        assert_eq!(escritorio.mensaje(), None, "el diálogo ya lo explica");
    }

    /// Regresión (C1): el enlace del visor se queda a la vista en su pestaña.
    #[test]
    fn el_enlace_sin_herramienta_queda_a_la_vista_en_el_escritorio() {
        let mut escritorio = Escritorio::nuevo();
        let enlace = "http://127.0.0.1:8080/entrar?t=de-prueba";
        escritorio.app.copiar(
            enlace.to_string(),
            ParaQue::EnlaceVisor { escritorio_id: 7 },
        );
        let visible = escritorio.app.enlace_visible.clone().expect("a la vista");
        assert_eq!((visible.0, visible.1.as_str()), (7, enlace));
        assert_eq!(
            escritorio.mensaje(),
            Some((
                format!("no se pudo copiar el enlace: {SIN_HERRAMIENTA}; está en la pestaña"),
                true
            ))
        );
    }

    /// Regresión (C1): la clave recién generada enseña su resumen y el diálogo.
    #[test]
    fn la_clave_generada_sin_herramienta_abre_el_dialogo_de_siempre_en_el_escritorio() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.app.copiar(
            format!("{PUBLICA}\n"),
            ParaQue::ClaveGenerada {
                resumen: "clave ssh-ed25519 generada en /home/usuario/.ssh/id_nueva".to_string(),
                error: false,
            },
        );
        assert_eq!(
            escritorio.mensaje(),
            Some((
                "clave ssh-ed25519 generada en /home/usuario/.ssh/id_nueva".to_string(),
                false
            ))
        );
        match &escritorio.app.dialogo {
            Some(Dialogo::Detalle { titulo, lineas, .. }) => {
                assert_eq!(titulo, "CLAVE PÚBLICA");
                assert_eq!(
                    lineas[0],
                    "No se pudo copiar al portapapeles; copia la línea a mano:"
                );
                assert_eq!(lineas[2], PUBLICA);
            }
            _ => panic!("sin el diálogo con la línea a mano"),
        }
    }

    /// Con herramienta, las tres vías dicen lo de siempre del escritorio.
    #[test]
    fn con_herramienta_las_tres_vias_dicen_lo_de_siempre() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.fabricar_herramientas();
        escritorio
            .app
            .copiar(PUBLICA.to_string(), ParaQue::ClavePublica);
        // La primera herramienta que prueba cada sistema (en macOS, `pbcopy`).
        let herramienta = if cfg!(target_os = "macos") {
            "pbcopy"
        } else {
            "wl-copy"
        };
        assert_eq!(
            escritorio.mensaje(),
            Some((format!("clave pública copiada con {herramienta}"), false))
        );
        assert_eq!(escritorio.copiado().as_deref(), Some(PUBLICA));
        assert!(escritorio.app.dialogo.is_none());

        escritorio.app.copiar(
            "http://127.0.0.1:8080/entrar?t=otro".to_string(),
            ParaQue::EnlaceVisor { escritorio_id: 3 },
        );
        assert_eq!(
            escritorio.mensaje(),
            Some(("enlace del visor copiado (vale 60 s)".to_string(), false))
        );
        assert!(escritorio.app.enlace_visible.is_none());

        escritorio.app.copiar(
            PUBLICA.to_string(),
            ParaQue::ClaveGenerada {
                resumen: "clave generada".to_string(),
                error: false,
            },
        );
        assert_eq!(
            escritorio.mensaje(),
            Some((
                "clave generada · pública copiada al portapapeles".to_string(),
                false
            ))
        );
        assert!(escritorio.app.dialogo.is_none());
    }

    /// La clave recién generada tal y como la devuelve su tarea: con la copia
    /// ya intentada (escritorio) o sin intentar (Termux).
    fn clave_generada(
        copiada: bool,
        fallo_copia: Option<String>,
    ) -> crate::identidades::ClaveGenerada {
        crate::identidades::ClaveGenerada {
            tipo: "ed25519".to_string(),
            huella: "SHA256:T2qRAb3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7".to_string(),
            ruta: std::path::PathBuf::from("/home/usuario/.ssh/id_nueva"),
            publica: format!("{PUBLICA}\n"),
            comentario: Some("usuario@ejemplo.org".to_string()),
            en_agente: false,
            aviso_agente: None,
            copiada,
            solicito_copia: true,
            fallo_copia,
        }
    }

    const RESUMEN_GENERADA: &str = "clave ed25519 generada en /home/usuario/.ssh/id_nueva";

    /// Regresión (revisión de cierre de la Fase 17, D5): en el escritorio, la
    /// copia de la clave recién generada la hace la tarea de generación (como
    /// antes de la Fase 17) y la interfaz no vuelve a copiar: ni llama a la
    /// herramienta del sistema ni la espera (hasta 750 ms con la interfaz
    /// parada).
    #[test]
    fn la_clave_generada_copiada_por_la_tarea_no_llama_a_la_herramienta_en_la_interfaz() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.fabricar_herramientas_lentas();
        let inicio = Instant::now();
        escritorio
            .app
            .procesar(Evento::ClaveGenerada(Ok(clave_generada(true, None))));
        assert!(
            inicio.elapsed() < Duration::from_millis(400),
            "la interfaz esperó a la herramienta: {:?}",
            inicio.elapsed()
        );
        assert!(
            !escritorio.herramienta_llamada(),
            "la interfaz llamó a la herramienta de copia"
        );
        assert_eq!(
            escritorio.mensaje(),
            Some((
                format!("{RESUMEN_GENERADA} · pública copiada al portapapeles"),
                false
            ))
        );
        assert!(escritorio.app.dialogo.is_none());
    }

    /// Lo mismo cuando la tarea no pudo copiar: sale el respaldo de siempre
    /// (resumen en la barra y el diálogo con la línea a mano) sin repetir la
    /// copia en la interfaz, aunque ahora hubiera herramienta.
    #[test]
    fn la_clave_generada_que_la_tarea_no_pudo_copiar_abre_el_respaldo_sin_reintentar() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.fabricar_herramientas_lentas();
        let inicio = Instant::now();
        escritorio
            .app
            .procesar(Evento::ClaveGenerada(Ok(clave_generada(
                false,
                Some(SIN_HERRAMIENTA.to_string()),
            ))));
        assert!(
            inicio.elapsed() < Duration::from_millis(400),
            "la interfaz esperó a la herramienta: {:?}",
            inicio.elapsed()
        );
        assert!(
            !escritorio.herramienta_llamada(),
            "la interfaz repitió la copia que acababa de fallar"
        );
        assert_eq!(
            escritorio.mensaje(),
            Some((RESUMEN_GENERADA.to_string(), false))
        );
        match &escritorio.app.dialogo {
            Some(Dialogo::Detalle { titulo, lineas, .. }) => {
                assert_eq!(titulo, "CLAVE PÚBLICA");
                assert_eq!(
                    lineas[0],
                    "No se pudo copiar al portapapeles; copia la línea a mano:"
                );
                assert_eq!(lineas[2], PUBLICA);
            }
            _ => panic!("sin el diálogo con la línea a mano"),
        }
    }

    /// Si la tarea no intentó copiar (Termux, y las pruebas sin tarea), la copia
    /// es de `App::copiar`, la única vía.
    #[test]
    fn la_clave_generada_que_la_tarea_no_intento_copiar_va_por_la_unica_via() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.fabricar_herramientas();
        escritorio
            .app
            .procesar(Evento::ClaveGenerada(Ok(clave_generada(false, None))));
        assert_eq!(
            escritorio.copiado().as_deref(),
            Some(format!("{PUBLICA}\n").as_str())
        );
        assert_eq!(
            escritorio.mensaje(),
            Some((
                format!("{RESUMEN_GENERADA} · pública copiada al portapapeles"),
                false
            ))
        );
    }

    /// Regresión (C4): el respaldo de una copia que termina tarde no pisa el
    /// diálogo que el usuario tenga abierto: queda debajo y sale al cerrarlo.
    #[test]
    fn el_dialogo_de_respaldo_no_pisa_el_que_hay_abierto() {
        let mut escritorio = Escritorio::nuevo();
        escritorio.app.dialogo = Some(Dialogo::Detalle {
            tunel_caido: None,
            titulo: "OTRO DIÁLOGO".to_string(),
            lineas: vec!["lo que el usuario estaba haciendo".to_string()],
        });
        escritorio.app.terminar_copia(
            PUBLICA,
            ParaQue::ClavePublica,
            FinCopia::Fallo("Termux:API no responde".to_string()),
        );
        match &escritorio.app.dialogo {
            Some(Dialogo::Detalle { titulo, .. }) => assert_eq!(titulo, "OTRO DIÁLOGO"),
            _ => panic!("se perdió el diálogo del usuario"),
        }
        assert_eq!(escritorio.app.dialogos_apilados(), 1);
        let (mensaje, error) = escritorio.mensaje().expect("la barra lo dice");
        assert!(error, "{mensaje}");
        assert!(
            mensaje.starts_with("no se pudo copiar la clave pública: Termux:API no responde"),
            "{mensaje}"
        );
        // La clave generada igual: no pisa y se apila.
        escritorio.app.terminar_copia(
            PUBLICA,
            ParaQue::ClaveGenerada {
                resumen: "clave generada".to_string(),
                error: false,
            },
            FinCopia::Rechazado("no cabe".to_string()),
        );
        assert_eq!(escritorio.app.dialogos_apilados(), 2);
        // Sin otro diálogo abierto, abre el suyo directamente.
        escritorio.app.dialogo = None;
        escritorio.app.terminar_copia(
            PUBLICA,
            ParaQue::ClavePublica,
            FinCopia::Fallo("x".to_string()),
        );
        assert!(matches!(
            escritorio.app.dialogo,
            Some(Dialogo::Detalle { ref titulo, .. }) if titulo == "CLAVE PÚBLICA"
        ));
        assert_eq!(escritorio.app.dialogos_apilados(), 2);
    }

    #[test]
    fn el_tamano_se_dice_en_bytes_kb_o_mb() {
        assert_eq!(tamano_legible(312), "312 B");
        assert_eq!(tamano_legible(1229), "1,2 KB");
        assert_eq!(tamano_legible(2 * 1024 * 1024), "2,0 MB");
    }
}
