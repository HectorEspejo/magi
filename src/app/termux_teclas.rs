//! Interfaz de la fila de teclas de Termux (Fase 17, §4.8, §5.4 y §6.4): el
//! diálogo FILA DE TECLAS DE TERMUX, su entrada de la paleta y la oferta del
//! primer arranque.
//!
//! La lógica (la fila, la fusión de `termux.properties`, la copia y la
//! escritura atómica) vive en `crate::termux::teclas` y se comparte con
//! `magi termux teclas`. Aquí solo está lo que ve el usuario:
//!
//! - **Nada bloquea el bucle de la interfaz.** Leer el fichero para armar la
//!   propuesta y, después, instalar (leer, copiar, escribir y pedir a Termux
//!   que recargue) van en hilos propios (no en `spawn_blocking`: con el
//!   almacenamiento sin responder, el cierre del runtime esperaría a la tarea
//!   para siempre) y su resultado vuelve por `Evento::Termux`. El diálogo,
//!   mientras instala, solo atiende `Esc`: lo cierra, la tarea no se puede
//!   abortar y el fichero se escribe igualmente, y su resultado se atiende
//!   por `id` (se anota y se dice por la barra, o el error sale por ella).
//!   Mientras haya una escritura en vuelo no se abre otro diálogo (dos
//!   escrituras a la vez sobre el mismo fichero no son de fiar).
//! - **El diálogo lleva fijados los datos sobre los que actúa** (la ruta, el
//!   texto del prefijo y la fecha de la copia) y no los relee al confirmar. Un
//!   resultado cuyo `id` ya no está en vuelo se descarta.
//! - **Nunca pisa a otro diálogo.** La propuesta llega de forma asíncrona (en el
//!   primer arranque, justo cuando puede estar abierto el del llavero): si ya
//!   hay un diálogo abierto, este queda debajo y sale cuando se cierre el de
//!   encima, para que una tecla pensada para el otro no instale nada.
//! - **REGISTRO, con el efecto producido:** `termux_teclas_instaladas` se anota
//!   al llegar el resultado de la escritura (nunca si falla ni si ya estaba),
//!   con el nombre de la copia o «no existía».
//! - **Primer arranque:** se ofrece una sola vez (marca `primer-arranque`, que
//!   se pone al ofrecerlo, también si se responde Esc) y la barra remite a
//!   `magi diagnostico`. No se ofrece si la fila ya es la de MAGI. La marca se
//!   crea de forma exclusiva ([`tomar_marca_del_primer_arranque`]): dos
//!   ventanas lanzadas a la vez no ofrecen el diálogo las dos. Como la
//!   propuesta llega sola, sobre lo que el usuario esté haciendo, el diálogo
//!   del primer arranque ignora `↵` durante [`GUARDA_DE_INTRO`] desde que se
//!   ve (también si sale de debajo de otro): un `↵` tecleado para otra cosa no
//!   puede confirmar una escritura que nadie ha leído. `Esc` y lo demás
//!   siempre valen.
//! - **Una sola recarga vale:** `termux-reload-settings` responde de forma
//!   asíncrona; el resultado de una instalación que ya no es la última de esta
//!   ventana se descarta (como el de una escritura).
//!
//! Ni la fila del usuario, ni el contenido de `termux.properties` van al log ni
//! a REGISTRO.

use std::os::unix::fs::OpenOptionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent};
use tracing::warn;

use crate::config::Rutas;
use crate::modelo::ResultadoRegistro;
use crate::registro;
use crate::termux::estado;
use crate::termux::teclas::{self, ErrorTeclas, Escritura, EstadoFila, Propuesta};
use crate::termux::{EntornoTermux, FalloOrden};

use super::termux::EventoTermux;
use super::{AccionPaleta, App, Dialogo, EntradaPaleta, Evento};

/// Lo que dice la entrada de la paleta (solo en Termux, §5.4).
pub const ETIQUETA_PALETA: &str = "Instalar la fila de teclas de Termux";

/// Lo que dice la barra en el primer arranque (§4.1).
pub const MENSAJE_PRIMER_ARRANQUE: &str = "MAGI en Termux: mira magi diagnostico";

/// Ids de las instalaciones en vuelo: cada confirmación lleva el suyo y el
/// diálogo solo acepta el resultado de la que espera.
static SIGUIENTE_ID: AtomicU64 = AtomicU64::new(1);

/// Nombre del fichero de la marca del primer arranque en `<estado>/termux/`:
/// el mismo que usa `estado::Marca::PrimerArranque` (una prueba lo vigila).
const MARCA_PRIMER_ARRANQUE: &str = "primer-arranque";

/// Cuánto ignora `↵` el diálogo del primer arranque desde que se ve: lo que un
/// `↵` tecleado para otra cosa tarda en dejar de ser «justo antes».
pub const GUARDA_DE_INTRO: Duration = Duration::from_millis(500);

/// Lo propio de la fila de teclas en una `App`.
#[derive(Debug, Default)]
pub struct EstadoTeclasTermux {
    /// La última instalación lanzada por esta ventana: la única cuyo resultado
    /// de recarga se atiende (una recarga lenta de una instalación anterior
    /// llega tarde y no dice nada de lo que hay ahora). Vuelve a `None` cuando
    /// no queda recarga que esperar.
    pub ultima_instalacion: Option<u64>,
    /// La instalación cuya escritura sigue en vuelo (aunque su diálogo ya se
    /// haya cerrado con `Esc`): hasta que llega su resultado no se abre otro
    /// diálogo, para no tener dos escrituras sobre el mismo fichero.
    pub escribiendo: Option<u64>,
}

/// Pone la marca del primer arranque si no estaba y dice si esta llamada fue
/// la que la puso (la primera: `true`) o ya estaba (`false`: otra ventana, o un
/// arranque anterior, ya ofreció la fila). La crea con `create_new`, así que
/// de dos ventanas lanzadas a la vez solo una la ve nacer: comprobar la marca y
/// ponerla por separado dejaba una ventana en la que las dos ofrecían el
/// diálogo. Sin sustituir nada (nunca sigue un enlace) y con 600, como el
/// resto del estado de Termux; `Err` si no se pudo crear (sin permisos, el
/// directorio no se crea): quien llama decide, la oferta no depende de ello.
pub fn tomar_marca_del_primer_arranque(rutas: &Rutas) -> std::io::Result<bool> {
    let directorio = estado::directorio(rutas);
    std::fs::create_dir_all(&directorio)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directorio.join(MARCA_PRIMER_ARRANQUE))
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
}

/// Quién pidió la propuesta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigenOferta {
    /// El primer arranque en Termux: se ofrece una sola vez.
    PrimerArranque,
    /// La entrada «Instalar la fila de teclas de Termux» de la paleta.
    Paleta,
}

/// El diálogo FILA DE TECLAS DE TERMUX.
pub struct DialogoTeclasTermux {
    /// Lo que se propone y dónde: se arma al abrir y no se relee.
    pub propuesta: Propuesta,
    /// El texto del prefijo (`[prefijo_escape]`) con el que se armó.
    pub prefijo: String,
    /// La fecha de la copia que se hará.
    pub fecha: NaiveDate,
    /// Id de la instalación en curso (se pinta «instalando…» y no se atienden
    /// teclas).
    pub trabajando: Option<u64>,
    /// Por qué falló la última instalación (se puede reintentar con `↵`).
    pub error: Option<String>,
    /// La copia que dejó la última instalación que falló al escribir, ya
    /// hecha en disco: un reintento la reutiliza (si sigue intacta e idéntica)
    /// en vez de hacer otra.
    pub copia_hecha: Option<String>,
    /// Hasta cuándo se ignora `↵` (solo el diálogo del primer arranque, que
    /// llega sin que el usuario lo espere; `None` no ignora nada). Se pone al
    /// verse el diálogo y se vuelve a poner si sale de debajo de otro. Es un
    /// `Instant` a la vista para que las pruebas lo pongan donde quieran.
    pub intro_ignorada_hasta: Option<Instant>,
}

impl DialogoTeclasTermux {
    #[must_use]
    pub fn nuevo(propuesta: Propuesta, prefijo: String, fecha: NaiveDate) -> Self {
        Self {
            propuesta,
            prefijo,
            fecha,
            trabajando: None,
            error: None,
            copia_hecha: None,
            intro_ignorada_hasta: None,
        }
    }

    /// El diálogo se acaba de ver (o de volver a ver, si estaba debajo de otro):
    /// si ignora `↵` al salir, empieza a contar desde `ahora`.
    pub(crate) fn visto_en(&mut self, ahora: Instant) {
        if self.intro_ignorada_hasta.is_some() {
            self.intro_ignorada_hasta = Some(ahora + GUARDA_DE_INTRO);
        }
    }

    /// ¿Se ignora `↵` en `ahora`?
    #[must_use]
    pub fn ignora_intro(&self, ahora: Instant) -> bool {
        self.intro_ignorada_hasta.is_some_and(|hasta| ahora < hasta)
    }
}

/// Lo que vuelve de las tareas de la fila de teclas (dentro de
/// `EventoTermux::Teclas`).
pub enum EventoTeclas {
    /// La propuesta ya está armada (se leyó el fichero en una tarea).
    Propuesta {
        origen: OrigenOferta,
        propuesta: Box<Propuesta>,
        prefijo: String,
        fecha: NaiveDate,
    },
    /// Terminó la copia y la escritura (o falló) de la instalación `id`.
    Escrita {
        id: u64,
        resultado: Result<Escritura, ErrorTeclas>,
    },
    /// Respondió (o no) `termux-reload-settings` de la instalación `id`.
    Recargada {
        id: u64,
        resultado: Result<(), FalloOrden>,
    },
}

impl App {
    /// El entorno que hace falta para saber dónde está `termux.properties`: el
    /// home de las rutas de la App (en Termux, `$HOME`).
    fn entorno_de_teclas(&self) -> EntornoTermux {
        EntornoTermux {
            home: Some(self.rutas.hogar.clone()),
            ..EntornoTermux::default()
        }
    }

    /// Primer arranque en Termux: ofrece la fila de teclas una sola vez y la
    /// barra remite a `magi diagnostico`. La marca, su comprobación y la
    /// lectura del fichero van en una tarea; si la marca ya estaba, no pasa
    /// nada. Fuera de Termux no hace nada.
    pub(crate) fn ofrecer_fila_en_el_primer_arranque(&mut self) {
        if !self.termux.activo {
            return;
        }
        self.pedir_propuesta_de_teclas(OrigenOferta::PrimerArranque);
    }

    /// Paleta: «Instalar la fila de teclas de Termux».
    pub(crate) fn paleta_instalar_fila_de_teclas(&mut self) {
        if !self.termux.activo {
            return;
        }
        if self.dialogo_de_teclas_abierto() {
            self.mensaje("el diálogo de la fila de teclas ya está abierto", false);
            return;
        }
        if self.teclas_termux.escribiendo.is_some() {
            // Se cerró con Esc mientras instalaba: la escritura sigue y su
            // resultado todavía no ha llegado.
            self.mensaje(
                "la fila de teclas se está instalando todavía: espera a que acabe",
                false,
            );
            return;
        }
        self.pedir_propuesta_de_teclas(OrigenOferta::Paleta);
    }

    /// Las entradas de la paleta de la fila de teclas: solo en Termux.
    pub(crate) fn entradas_paleta_teclas(&self) -> Vec<EntradaPaleta> {
        if !self.termux.activo {
            return Vec::new();
        }
        vec![EntradaPaleta {
            etiqueta: ETIQUETA_PALETA.to_string(),
            categoria: "termux",
            accion: AccionPaleta::InstalarFilaDeTeclas,
        }]
    }

    /// Lanza la tarea que arma la propuesta (lee el fichero) y devuelve el
    /// resultado por `Evento::Termux`. Con el primer arranque, la tarea mira y
    /// pone antes la marca: solo si no estaba hay algo que ofrecer.
    fn pedir_propuesta_de_teclas(&mut self, origen: OrigenOferta) {
        let eventos = self.eventos_tx.clone();
        let rutas = self.rutas.clone();
        let prefijo = self.config.prefijo_escape.clone();
        let entorno = self.entorno_de_teclas();
        // En un hilo propio y no en `spawn_blocking`: si el almacenamiento al
        // que apunta `termux.properties` no responde, el cierre del runtime
        // esperaría a la tarea y la TUI no podría salir.
        std::thread::spawn(move || {
            if origen == OrigenOferta::PrimerArranque {
                // Se pone al ofrecerlo, también si luego se responde Esc, y se
                // pone de forma exclusiva: solo la ventana que la crea ofrece.
                match tomar_marca_del_primer_arranque(&rutas) {
                    Ok(true) => {}
                    Ok(false) => return,
                    // Sin poder dejar la marca se ofrece igualmente (volverá a
                    // salir en el próximo arranque, que es lo que pasaba antes
                    // de que existiera la marca).
                    Err(error) => {
                        warn!("no se pudo poner la marca del primer arranque: {error}");
                    }
                }
            }
            let fecha = chrono::Local::now().date_naive();
            let propuesta = teclas::preparar(&entorno, &prefijo, fecha);
            let _ = eventos.send(Evento::Termux(EventoTermux::Teclas(
                EventoTeclas::Propuesta {
                    origen,
                    propuesta: Box::new(propuesta),
                    prefijo,
                    fecha,
                },
            )));
        });
    }

    /// Lo que vuelve de una tarea de la fila de teclas.
    pub(crate) fn evento_teclas(&mut self, evento: EventoTeclas) {
        match evento {
            EventoTeclas::Propuesta {
                origen,
                propuesta,
                prefijo,
                fecha,
            } => self.propuesta_de_teclas(origen, *propuesta, prefijo, fecha),
            EventoTeclas::Escrita { id, resultado } => self.fila_escrita(id, resultado),
            EventoTeclas::Recargada { id, resultado } => self.fila_recargada(id, &resultado),
        }
        self.sucio = true;
    }

    /// La propuesta está lista: abre el diálogo, o dice por la barra por qué no.
    fn propuesta_de_teclas(
        &mut self,
        origen: OrigenOferta,
        propuesta: Propuesta,
        prefijo: String,
        fecha: NaiveDate,
    ) {
        let primer_arranque = origen == OrigenOferta::PrimerArranque;
        if primer_arranque {
            self.decir_en_barra(MENSAJE_PRIMER_ARRANQUE, false);
        }
        let texto = match &propuesta.estado {
            // Ya es la de MAGI: no hay nada que ofrecer (la paleta lo dice).
            EstadoFila::LaDeMagi => {
                if !primer_arranque {
                    self.mensaje(
                        "la fila de teclas ya es la de MAGI: no hay nada que instalar",
                        false,
                    );
                }
                return;
            }
            EstadoFila::Ilegible(motivo) => {
                format!("termux.properties no se puede leer ({motivo}): edítalo a mano")
            }
            EstadoFila::SinFichero | EstadoFila::SinExtraKeys | EstadoFila::Otra => {
                if self.dialogo_de_teclas_abierto() {
                    return;
                }
                let mut dialogo = DialogoTeclasTermux::nuevo(propuesta, prefijo, fecha);
                if primer_arranque {
                    // Sale solo, sobre lo que el usuario estuviera haciendo.
                    dialogo.intro_ignorada_hasta = Some(Instant::now() + GUARDA_DE_INTRO);
                }
                self.mostrar_dialogo_de_teclas(dialogo);
                return;
            }
        };
        if primer_arranque {
            self.decir_en_barra(texto, true);
        } else {
            self.mensaje(texto, true);
        }
    }

    /// Pone el diálogo a la vista, o debajo del que ya esté abierto (la
    /// propuesta llega sola, sin que el usuario la esté esperando delante).
    fn mostrar_dialogo_de_teclas(&mut self, dialogo: DialogoTeclasTermux) {
        let dialogo = Dialogo::TeclasTermux(Box::new(dialogo));
        if self.dialogo.is_some() {
            self.pila_dialogos.insert(0, dialogo);
        } else {
            self.dialogo = Some(dialogo);
        }
    }

    /// ¿Hay un diálogo de la fila de teclas a la vista o apilado?
    fn dialogo_de_teclas_abierto(&self) -> bool {
        let es_el_suyo = |dialogo: &Dialogo| matches!(dialogo, Dialogo::TeclasTermux(_));
        self.dialogo.as_ref().is_some_and(es_el_suyo) || self.pila_dialogos.iter().any(es_el_suyo)
    }

    /// El diálogo de la fila de teclas que espera la instalación `id`.
    fn dialogo_de_teclas_en_vuelo(&mut self, id: u64) -> Option<&mut DialogoTeclasTermux> {
        let espera = |dialogo: &Dialogo| matches!(dialogo, Dialogo::TeclasTermux(teclas) if teclas.trabajando == Some(id));
        let en_pantalla = self.dialogo.as_ref().is_some_and(espera);
        let abierto = if en_pantalla {
            self.dialogo.as_mut()
        } else {
            self.pila_dialogos
                .iter_mut()
                .find(|dialogo| espera(dialogo))
        };
        match abierto {
            Some(Dialogo::TeclasTermux(dialogo)) => Some(&mut **dialogo),
            _ => None,
        }
    }

    /// Quita el diálogo que esperaba la instalación `id` (ya terminó bien).
    fn cerrar_dialogo_de_teclas(&mut self, id: u64) {
        let espera = |dialogo: &Dialogo| matches!(dialogo, Dialogo::TeclasTermux(teclas) if teclas.trabajando == Some(id));
        if self.dialogo.as_ref().is_some_and(espera) {
            self.dialogo = None;
            self.restaurar_dialogo_apilado();
        } else {
            self.pila_dialogos.retain(|dialogo| !espera(dialogo));
        }
    }

    /// Tecla sobre el diálogo (ya sacado de `App.dialogo`): `↵` instala y
    /// `Esc` lo deja para otra vez. Mientras instala solo atiende `Esc`, que
    /// cierra el diálogo: la tarea sigue (no se puede abortar y el fichero se
    /// escribe igualmente) y su resultado se atiende por `id` cuando llegue.
    /// Sin esa salida, un almacenamiento que no responde dejaría la TUI
    /// atascada en «instalando…».
    pub(crate) fn tecla_dialogo_de_teclas(
        &mut self,
        dialogo: Box<DialogoTeclasTermux>,
        tecla: KeyEvent,
    ) {
        if dialogo.trabajando.is_some() {
            if tecla.code == KeyCode::Esc {
                self.mensaje(
                    "la fila de teclas se sigue instalando: el resultado saldrá aquí",
                    false,
                );
            } else {
                self.dialogo = Some(Dialogo::TeclasTermux(dialogo));
            }
            return;
        }
        match tecla.code {
            KeyCode::Esc => {}
            KeyCode::Enter if dialogo.ignora_intro(Instant::now()) => {
                // Un `↵` tecleado para otra cosa justo antes de que saliera
                // el diálogo: no confirma nada.
                self.dialogo = Some(Dialogo::TeclasTermux(dialogo));
            }
            KeyCode::Enter => self.instalar_fila_de_teclas(dialogo),
            _ => self.dialogo = Some(Dialogo::TeclasTermux(dialogo)),
        }
    }

    /// Confirmado: instala en una tarea (copia, escritura y recarga) con los
    /// datos que el diálogo llevaba fijados.
    fn instalar_fila_de_teclas(&mut self, mut dialogo: Box<DialogoTeclasTermux>) {
        let Some(ruta) = dialogo.propuesta.ruta.clone() else {
            dialogo.error = Some(ErrorTeclas::SinHome.to_string());
            self.dialogo = Some(Dialogo::TeclasTermux(dialogo));
            return;
        };
        let id = SIGUIENTE_ID.fetch_add(1, Ordering::Relaxed);
        dialogo.trabajando = Some(id);
        dialogo.error = None;
        // Desde ahora solo se atiende la recarga de esta instalación.
        self.teclas_termux.ultima_instalacion = Some(id);
        self.teclas_termux.escribiendo = Some(id);
        let prefijo = dialogo.prefijo.clone();
        let fecha = dialogo.fecha;
        let copia_previa = dialogo.copia_hecha.clone();
        self.dialogo = Some(Dialogo::TeclasTermux(dialogo));
        let eventos = self.eventos_tx.clone();
        let ordenes = Arc::clone(&self.termux.ordenes);
        // En un hilo propio (véase `pedir_propuesta_de_teclas`): el disco o el
        // almacenamiento compartido pueden no responder y esa espera no debe
        // retener el cierre del runtime.
        std::thread::spawn(move || {
            let resultado = teclas::escribir_fila_con_copia_previa(
                &ruta,
                &prefijo,
                fecha,
                copia_previa.as_deref(),
            );
            // Solo se recarga si de verdad se escribió algo.
            let recargar = matches!(&resultado, Ok(escritura) if !escritura.ya_estaba);
            let _ = eventos.send(Evento::Termux(EventoTermux::Teclas(
                EventoTeclas::Escrita { id, resultado },
            )));
            if recargar {
                let resultado = teclas::recargar(&ordenes);
                let _ = eventos.send(Evento::Termux(EventoTermux::Teclas(
                    EventoTeclas::Recargada { id, resultado },
                )));
            }
        });
    }

    /// Terminó la copia y la escritura: se anota el efecto, se cierra el
    /// diálogo y se dice por la barra. Si falló, el diálogo se queda con el
    /// motivo para reintentar o salir.
    fn fila_escrita(&mut self, id: u64, resultado: Result<Escritura, ErrorTeclas>) {
        if self.teclas_termux.escribiendo == Some(id) {
            self.teclas_termux.escribiendo = None;
        }
        match resultado {
            Ok(escritura) => {
                self.cerrar_dialogo_de_teclas(id);
                if escritura.ya_estaba {
                    // No se ha escrito nada: no vendrá ninguna recarga.
                    self.dejar_de_esperar_la_recarga(id);
                    self.mensaje(
                        "la fila de teclas ya es la de MAGI: no se ha tocado nada",
                        false,
                    );
                    return;
                }
                // El fichero ya está escrito: ahora sí se anota (§3.3).
                self.anotar(
                    registro::TERMUX_TECLAS_INSTALADAS,
                    None,
                    None,
                    &escritura.detalle_de_registro(),
                    ResultadoRegistro::Ok,
                );
                if escritura.copia_reutilizada {
                    // Se dice: no se ha hecho una copia nueva.
                    let copia = escritura.copia.as_deref().unwrap_or_default();
                    self.mensaje(
                        format!(
                            "fila de teclas instalada (con la copia {copia} de antes) · recargando Termux…"
                        ),
                        false,
                    );
                } else {
                    self.mensaje("fila de teclas instalada · recargando Termux…", false);
                }
            }
            Err(error) => {
                // Falló antes de la recarga: tampoco vendrá ninguna.
                self.dejar_de_esperar_la_recarga(id);
                let texto = error.to_string();
                let copia = error.copia_hecha().map(str::to_string);
                match self.dialogo_de_teclas_en_vuelo(id) {
                    Some(dialogo) => {
                        dialogo.trabajando = None;
                        dialogo.error = Some(texto);
                        // La copia ya está en disco: el reintento la reutiliza.
                        if copia.is_some() {
                            dialogo.copia_hecha = copia;
                        }
                    }
                    // El diálogo ya no está: el motivo no se pierde.
                    None => self.mensaje(texto, true),
                }
            }
        }
    }

    /// La instalación `id` no va a tener recarga (no escribió nada o falló): si
    /// era la última, ya no se espera ninguna.
    fn dejar_de_esperar_la_recarga(&mut self, id: u64) {
        if self.teclas_termux.ultima_instalacion == Some(id) {
            self.teclas_termux.ultima_instalacion = None;
        }
    }

    /// Respondió (o no) Termux a la recarga: la fila ya se ve o hay que
    /// reiniciar Termux. La instalación ya estaba hecha y anotada.
    fn fila_recargada(&mut self, id: u64, resultado: &Result<(), FalloOrden>) {
        // Solo vale la recarga de la última instalación de esta ventana: la de
        // una anterior que respondió tarde ya no dice nada de lo que hay.
        if self.teclas_termux.ultima_instalacion != Some(id) {
            return;
        }
        self.teclas_termux.ultima_instalacion = None;
        match resultado {
            Ok(()) => self.mensaje("fila de teclas instalada: ya se ve en Termux", false),
            Err(_) => self.mensaje(format!("instalada · {}", teclas::REINICIA_TERMUX), false),
        }
    }
}
