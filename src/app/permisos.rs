//! Diálogo PERMISOS (`p`, Fase 8, §4.2): casillas rwx y octal sincronizados
//! sobre los marcados (o la fila actual), con estado mixto y recursivo
//! opcional. Lo local va por `std::fs` en un hilo; lo remoto, por
//! `CambiarPermisos`. La lógica pura vive en `archivos::permisos`.
//!
//! El diálogo fija al abrirse el lado, el host y las rutas (T33): ni el
//! recuento ni la confirmación releen el panel. Con recursivo, primero se
//! cuenta lo que hay dentro (en local, un hilo; en remoto, un `ListarArbol`
//! por directorio) y se pide confirmación con el recuento.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::archivos::permisos::{self as puro, ModoPermisos, Recuento, ResultadoLocal};
use crate::archivos::{Entrada, Lado, TipoEntrada};
use crate::protocolo::{self, AlcancePermisos};

use super::{
    AccionDialogo, App, Dialogo, Evento, EventoArchivos, PeticionArchivos, RespuestaArchivos,
};

/// Una ruta elegida, fijada al abrir el diálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objetivo {
    /// Absoluta, del lado del diálogo.
    pub ruta: String,
    pub es_dir: bool,
}

/// Campo con el foco del diálogo PERMISOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoPermisos {
    /// La rejilla de casillas (la celda va en `FormularioPermisos::celda`).
    Rejilla,
    Octal,
    Recursivo,
    Alcance,
}

/// El diálogo PERMISOS (§6.1).
pub struct FormularioPermisos {
    pub lado: Lado,
    pub host_id: i64,
    pub objetivos: Vec<Objetivo>,
    pub modo: ModoPermisos,
    pub foco: FocoPermisos,
    /// Casilla de la rejilla con el cursor: (usuario/grupo/otros, r/w/x).
    pub celda: (usize, usize),
    pub recursivo: bool,
    pub alcance: AlcancePermisos,
}

/// Lo que pide una tecla del formulario.
enum Pide {
    Seguir,
    Aplicar,
    Cancelar,
}

impl FormularioPermisos {
    /// El diálogo para estas rutas, con el valor inicial de sus modos (§4.2):
    /// el de la fila si es una, y `[~]` donde varias no coinciden.
    pub fn nuevo(lado: Lado, host_id: i64, elegidos: Vec<(Objetivo, Option<u32>)>) -> Self {
        let modos: Vec<Option<u32>> = elegidos.iter().map(|(_, modo)| *modo).collect();
        Self {
            lado,
            host_id,
            objetivos: elegidos.into_iter().map(|(objetivo, _)| objetivo).collect(),
            modo: ModoPermisos::desde_modos(&modos),
            foco: FocoPermisos::Rejilla,
            celda: (0, 0),
            recursivo: false,
            alcance: AlcancePermisos::Todo,
        }
    }

    pub fn directorios(&self) -> usize {
        self.objetivos
            .iter()
            .filter(|objetivo| objetivo.es_dir)
            .count()
    }

    pub fn ficheros(&self) -> usize {
        self.objetivos.len() - self.directorios()
    }

    /// El recursivo solo se ofrece con directorios entre lo elegido.
    pub fn hay_directorios(&self) -> bool {
        self.directorios() > 0
    }

    /// Alcance que se envía: ninguno sin recursivo.
    pub fn alcance_efectivo(&self) -> Option<AlcancePermisos> {
        (self.recursivo && self.hay_directorios()).then_some(self.alcance)
    }

    /// Aviso ámbar: el modo quita `x` a directorios con alcance «todo».
    pub fn aviso(&self) -> bool {
        puro::aviso_directorios(self.alcance_efectivo(), &self.modo)
    }

    /// Campos que recorre `Tab`: el recursivo solo con directorios y el
    /// alcance solo con el recursivo marcado.
    fn focos(&self) -> Vec<FocoPermisos> {
        let mut focos = vec![FocoPermisos::Rejilla, FocoPermisos::Octal];
        if self.hay_directorios() {
            focos.push(FocoPermisos::Recursivo);
            if self.recursivo {
                focos.push(FocoPermisos::Alcance);
            }
        }
        focos
    }

    fn foco_vecino(&self, atras: bool) -> FocoPermisos {
        let focos = self.focos();
        let total = focos.len();
        let actual = focos
            .iter()
            .position(|foco| *foco == self.foco)
            .unwrap_or(0);
        focos[if atras {
            (actual + total - 1) % total
        } else {
            (actual + 1) % total
        }]
    }

    fn tecla(&mut self, tecla: &KeyEvent) -> Pide {
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        match tecla.code {
            KeyCode::Char('s') | KeyCode::Char('S') if control => return Pide::Aplicar,
            KeyCode::Esc => return Pide::Cancelar,
            KeyCode::Tab => self.foco = self.foco_vecino(false),
            KeyCode::BackTab => self.foco = self.foco_vecino(true),
            _ if control => {}
            codigo => self.tecla_en_foco(codigo),
        }
        Pide::Seguir
    }

    fn tecla_en_foco(&mut self, codigo: KeyCode) {
        let (quien, que) = self.celda;
        match (self.foco, codigo) {
            (FocoPermisos::Rejilla, KeyCode::Up) => self.celda = (quien.saturating_sub(1), que),
            (FocoPermisos::Rejilla, KeyCode::Down) => self.celda = ((quien + 1).min(2), que),
            (FocoPermisos::Rejilla, KeyCode::Left) => self.celda = (quien, que.saturating_sub(1)),
            (FocoPermisos::Rejilla, KeyCode::Right) => self.celda = (quien, (que + 1).min(2)),
            (FocoPermisos::Rejilla, KeyCode::Char(' ')) => self.modo.alternar(quien, que),
            (FocoPermisos::Octal, KeyCode::Char(digito)) => {
                self.modo.escribir(digito);
            }
            (FocoPermisos::Octal, KeyCode::Backspace) => self.modo.borrar(),
            (FocoPermisos::Recursivo, KeyCode::Char(' ')) => self.recursivo = !self.recursivo,
            (FocoPermisos::Alcance, KeyCode::Left) => {
                self.alcance = match self.alcance {
                    AlcancePermisos::Todo | AlcancePermisos::Directorios => AlcancePermisos::Todo,
                    AlcancePermisos::Ficheros => AlcancePermisos::Directorios,
                };
            }
            (FocoPermisos::Alcance, KeyCode::Right) => {
                self.alcance = match self.alcance {
                    AlcancePermisos::Todo => AlcancePermisos::Directorios,
                    AlcancePermisos::Directorios | AlcancePermisos::Ficheros => {
                        AlcancePermisos::Ficheros
                    }
                };
            }
            (FocoPermisos::Alcance, KeyCode::Char(' ')) => {
                self.alcance = match self.alcance {
                    AlcancePermisos::Todo => AlcancePermisos::Directorios,
                    AlcancePermisos::Directorios => AlcancePermisos::Ficheros,
                    AlcancePermisos::Ficheros => AlcancePermisos::Todo,
                };
            }
            _ => {}
        }
    }

    /// Lo que se aplicaría ahora, o por qué no se puede.
    pub fn orden(&self) -> Result<OrdenPermisos, String> {
        let (modo, mascara) = self.modo.modo_y_mascara()?;
        if mascara == 0 {
            return Err("todas las casillas son mixtas: no hay nada que cambiar".to_string());
        }
        Ok(OrdenPermisos {
            lado: self.lado,
            host_id: self.host_id,
            objetivos: self.objetivos.clone(),
            modo,
            mascara,
            alcance: self.alcance_efectivo(),
            texto: self.modo.texto(),
            aviso: self.aviso(),
        })
    }
}

/// Un `chmod` decidido: todo fijado al abrir el diálogo (T33).
#[derive(Debug, Clone)]
pub struct OrdenPermisos {
    pub lado: Lado,
    pub host_id: i64,
    pub objetivos: Vec<Objetivo>,
    pub modo: u32,
    pub mascara: u32,
    pub alcance: Option<AlcancePermisos>,
    /// El modo para los mensajes: `0644` o `rw-r~-r~-`.
    pub texto: String,
    /// El modo quita `x` a directorios con alcance «todo».
    pub aviso: bool,
}

/// Recuento de un recursivo en marcha. Lo elegido cuenta desde el principio;
/// lo de dentro llega del hilo local o de los `ListarArbol`.
pub struct RecuentoEnVuelo {
    pub id: u64,
    pub orden: OrdenPermisos,
    pub recuento: Recuento,
    /// Recorridos que faltan por terminar: el hilo local, o un `ListarArbol`
    /// por directorio.
    pub pendientes: usize,
}

/// Diálogos propios de los permisos (viven en el hueco `App::dialogo`).
pub enum DialogoPermisos {
    /// El diálogo PERMISOS.
    Formulario(FormularioPermisos),
    /// Contando lo que tocará un recursivo, antes de confirmarlo.
    Contando(RecuentoEnVuelo),
}

/// Acciones confirmadas (viajan en `AccionDialogo::Permisos`).
pub enum AccionPermisos {
    /// El recursivo, confirmado con su recuento.
    Aplicar(OrdenPermisos),
}

/// Peticiones al servidor en vuelo.
#[derive(Debug)]
pub enum PeticionPermisosCliente {
    /// `ListarArbol` de un directorio elegido, para el recuento `recuento`.
    Recuento { recuento: u64 },
    /// `CambiarPermisos` enviado.
    Aplicar { host_id: i64, texto: String },
}

/// Resultados de los hilos locales (recuento y aplicación).
#[derive(Debug)]
pub enum EventoPermisos {
    Recuento {
        recuento: u64,
        resultado: Recuento,
    },
    Aplicado {
        texto: String,
        resultado: ResultadoLocal,
    },
}

/// Un enlace no tiene permisos propios que cambiar: `chmod` cambiaría lo que
/// apunta. En local, uno a directorio se lista como directorio con destino.
fn es_enlace(entrada: &Entrada) -> bool {
    entrada.tipo == TipoEntrada::Enlace || entrada.enlace.is_some()
}

fn tipo_de(objetivo: &Objetivo) -> TipoEntrada {
    if objetivo.es_dir {
        TipoEntrada::Directorio
    } else {
        TipoEntrada::Fichero
    }
}

impl App {
    /// `p` en Archivos.
    pub(super) fn abrir_permisos(&mut self) {
        let Some(estado) = &self.archivos else {
            return;
        };
        let lado = estado.activo;
        let panel = super::panel_activo(estado);
        if panel.ruta.is_empty() || (lado == Lado::Remoto && estado.solo_local) {
            return;
        }
        let seleccionados: Vec<&Entrada> = panel
            .seleccionados()
            .into_iter()
            .filter(|entrada| entrada.nombre != "..")
            .collect();
        if seleccionados.is_empty() {
            self.mensaje("no hay nada seleccionado", true);
            return;
        }
        let enlaces = seleccionados
            .iter()
            .filter(|entrada| es_enlace(entrada))
            .count() as u64;
        let elegidos: Vec<(Objetivo, Option<u32>)> = seleccionados
            .iter()
            .filter(|entrada| !es_enlace(entrada))
            .map(|entrada| {
                let ruta = match lado {
                    Lado::Local => std::path::Path::new(&panel.ruta)
                        .join(&entrada.nombre)
                        .display()
                        .to_string(),
                    Lado::Remoto => crate::servidor::sftp::join(&panel.ruta, &entrada.nombre),
                };
                let objetivo = Objetivo {
                    ruta,
                    es_dir: entrada.es_dir(),
                };
                (objetivo, entrada.permisos)
            })
            .collect();
        let host_id = estado.host_id;
        if elegidos.is_empty() {
            self.mensaje("los enlaces no se tocan: no hay permisos que cambiar", true);
            return;
        }
        if enlaces > 0 {
            self.mensaje(
                format!(
                    "{} sin tocar: chmod cambiaría lo que apunta",
                    puro::cantidad(enlaces, "enlace", "enlaces")
                ),
                false,
            );
        }
        self.dialogo = Some(Dialogo::Permisos(DialogoPermisos::Formulario(
            FormularioPermisos::nuevo(lado, host_id, elegidos),
        )));
    }

    pub(super) fn respuesta_permisos(
        &mut self,
        peticion_id: u64,
        peticion: PeticionPermisosCliente,
        respuesta: RespuestaArchivos,
    ) {
        match peticion {
            PeticionPermisosCliente::Recuento { recuento } => {
                self.bloque_de_recuento(peticion_id, recuento, respuesta);
            }
            PeticionPermisosCliente::Aplicar { host_id, texto } => match respuesta {
                RespuestaArchivos::Hecho(detalle) => {
                    self.mensaje(
                        format!(
                            "permisos {texto}: {}",
                            detalle.unwrap_or_else(|| "aplicados".to_string())
                        ),
                        false,
                    );
                    // El panel se refresca si sigue en ese host.
                    let ruta = self
                        .archivos
                        .as_ref()
                        .filter(|estado| estado.host_id == host_id)
                        .map(|estado| estado.remoto.ruta.clone())
                        .filter(|ruta| !ruta.is_empty());
                    if let Some(ruta) = ruta {
                        self.pedir_listado_remoto(
                            ruta,
                            crate::archivos::panel::MotivoListado::Operacion,
                        );
                    }
                }
                RespuestaArchivos::Error(motivo) => {
                    self.mensaje(format!("permisos {texto}: {motivo}"), true);
                }
                _ => {}
            },
        }
    }

    pub(super) fn evento_permisos(&mut self, evento: EventoPermisos) {
        match evento {
            EventoPermisos::Recuento {
                recuento,
                resultado,
            } => {
                // Un recuento cancelado se descarta.
                if let Some(en_vuelo) = self.recuento_en_vuelo(recuento) {
                    en_vuelo.recuento = resultado;
                    en_vuelo.pendientes = 0;
                }
                self.cerrar_recuento_si_termino(recuento);
            }
            EventoPermisos::Aplicado { texto, resultado } => {
                self.mensaje(
                    format!("permisos {texto}: {}", resultado.resumen()),
                    resultado.errores > 0,
                );
                if let Some(estado) = &mut self.archivos {
                    super::listar_local(estado, true);
                }
            }
        }
    }

    pub(super) fn tecla_dialogo_permisos(&mut self, dialogo: DialogoPermisos, tecla: KeyEvent) {
        match dialogo {
            DialogoPermisos::Formulario(mut formulario) => match formulario.tecla(&tecla) {
                Pide::Seguir => {
                    self.dialogo = Some(Dialogo::Permisos(DialogoPermisos::Formulario(formulario)));
                }
                Pide::Cancelar => {}
                Pide::Aplicar => match formulario.orden() {
                    Ok(orden) if orden.alcance.is_some() => self.contar_recursivo(orden),
                    Ok(orden) => self.aplicar_permisos(orden),
                    Err(motivo) => {
                        self.mensaje(motivo, true);
                        self.dialogo =
                            Some(Dialogo::Permisos(DialogoPermisos::Formulario(formulario)));
                    }
                },
            },
            DialogoPermisos::Contando(en_vuelo) => {
                if tecla.code == KeyCode::Esc {
                    self.cancelar_recuento(en_vuelo.id);
                } else {
                    self.dialogo = Some(Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo)));
                }
            }
        }
    }

    pub(super) fn ejecutar_accion_permisos(&mut self, accion: AccionPermisos) {
        match accion {
            AccionPermisos::Aplicar(orden) => self.aplicar_permisos(orden),
        }
    }

    /// Identidad de un recuento: sale del mismo contador que las peticiones
    /// de Archivos, así nunca coincide con otra en vuelo.
    fn nuevo_recuento(&mut self) -> u64 {
        self.siguiente_peticion_archivos += 1;
        self.siguiente_peticion_archivos
    }

    /// Empieza el recuento de un recursivo; la confirmación sale al terminar.
    fn contar_recursivo(&mut self, orden: OrdenPermisos) {
        let id = self.nuevo_recuento();
        let en_vuelo = match orden.lado {
            Lado::Local => {
                let rutas: Vec<PathBuf> = orden
                    .objetivos
                    .iter()
                    .map(|objetivo| PathBuf::from(&objetivo.ruta))
                    .collect();
                let (alcance, tx) = (orden.alcance, self.eventos_tx.clone());
                self.runtime.spawn_blocking(move || {
                    let resultado = puro::contar_local(&rutas, alcance);
                    let _ = tx.send(Evento::Archivos(EventoArchivos::Permisos(
                        EventoPermisos::Recuento {
                            recuento: id,
                            resultado,
                        },
                    )));
                });
                RecuentoEnVuelo {
                    id,
                    orden,
                    recuento: Recuento::default(),
                    pendientes: 1,
                }
            }
            Lado::Remoto => {
                // `ListarArbol` devuelve lo de dentro; lo elegido se cuenta ya.
                let mut recuento = Recuento::default();
                for objetivo in &orden.objetivos {
                    recuento.contar(orden.alcance, tipo_de(objetivo));
                }
                let mut pendientes = 0;
                for objetivo in orden.objetivos.iter().filter(|objetivo| objetivo.es_dir) {
                    let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Permisos(
                        PeticionPermisosCliente::Recuento { recuento: id },
                    ));
                    self.servidor
                        .enviar(protocolo::MensajeCliente::ListarArbol {
                            peticion_id,
                            host_id: orden.host_id,
                            ruta: objetivo.ruta.clone(),
                            exclusiones: Vec::new(),
                            usar_magiignore: false,
                            exclusiones_extra: Vec::new(),
                        });
                    pendientes += 1;
                }
                RecuentoEnVuelo {
                    id,
                    orden,
                    recuento,
                    pendientes,
                }
            }
        };
        self.dialogo = Some(Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo)));
        self.cerrar_recuento_si_termino(id);
    }

    /// Un bloque de `ListarArbol` del recuento (o su error).
    fn bloque_de_recuento(
        &mut self,
        peticion_id: u64,
        recuento: u64,
        respuesta: RespuestaArchivos,
    ) {
        // Si el recuento ya no está (cancelado), el bloque se descarta y los
        // siguientes no encuentran petición.
        let Some(en_vuelo) = self.recuento_en_vuelo(recuento) else {
            return;
        };
        let mut faltan_bloques = false;
        match respuesta {
            RespuestaArchivos::Arbol { entradas, fin, .. } => {
                for entrada in &entradas {
                    en_vuelo
                        .recuento
                        .contar(en_vuelo.orden.alcance, entrada.tipo);
                }
                faltan_bloques = !fin;
            }
            RespuestaArchivos::Error(motivo) => {
                en_vuelo.recuento.incompleto.get_or_insert(motivo);
            }
            _ => {}
        }
        if faltan_bloques {
            // Sigue esperando más bloques con el mismo id.
            self.peticiones_archivos.insert(
                peticion_id,
                PeticionArchivos::Permisos(PeticionPermisosCliente::Recuento { recuento }),
            );
        } else {
            en_vuelo.pendientes = en_vuelo.pendientes.saturating_sub(1);
        }
        self.cerrar_recuento_si_termino(recuento);
    }

    /// El recuento en vuelo con esa identidad, esté a la vista o apartado
    /// bajo una pregunta del servidor.
    fn recuento_en_vuelo(&mut self, id: u64) -> Option<&mut RecuentoEnVuelo> {
        self.dialogo
            .iter_mut()
            .chain(self.pila_dialogos.iter_mut())
            .find_map(|dialogo| match dialogo {
                Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo)) if en_vuelo.id == id => {
                    Some(en_vuelo)
                }
                _ => None,
            })
    }

    /// Con todo contado, el recuento deja paso a la confirmación (en el mismo
    /// sitio: a la vista o en la pila).
    fn cerrar_recuento_si_termino(&mut self, id: u64) {
        if !self
            .recuento_en_vuelo(id)
            .is_some_and(|en_vuelo| en_vuelo.pendientes == 0)
        {
            return;
        }
        let es_este = |dialogo: &Dialogo| matches!(dialogo, Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo)) if en_vuelo.id == id);
        if self.dialogo.as_ref().is_some_and(es_este) {
            if let Some(Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo))) =
                self.dialogo.take()
            {
                self.dialogo = self.confirmacion_recursiva(en_vuelo);
            }
        } else if let Some(indice) = self.pila_dialogos.iter().position(es_este) {
            if let Dialogo::Permisos(DialogoPermisos::Contando(en_vuelo)) =
                self.pila_dialogos.remove(indice)
            {
                if let Some(confirmacion) = self.confirmacion_recursiva(en_vuelo) {
                    self.pila_dialogos.insert(indice, confirmacion);
                }
            }
        }
        self.sucio = true;
    }

    /// La confirmación con peligro del recursivo, o nada (con un mensaje) si
    /// el alcance no toca nada.
    fn confirmacion_recursiva(&mut self, en_vuelo: RecuentoEnVuelo) -> Option<Dialogo> {
        let RecuentoEnVuelo {
            orden, recuento, ..
        } = en_vuelo;
        let alcance = orden.alcance.unwrap_or(AlcancePermisos::Todo);
        if recuento.total() == 0 {
            self.mensaje(
                format!(
                    "permisos {}: el alcance «{}» no toca nada",
                    orden.texto,
                    alcance.texto()
                ),
                true,
            );
            return None;
        }
        let mut lineas = vec![
            format!("  Aplicará {} a {}.", orden.texto, recuento.descripcion()),
            format!(
                "  Alcance: {} · {} elegido(s) y lo que contienen.",
                alcance.texto(),
                orden.objetivos.len()
            ),
        ];
        if recuento.enlaces > 0 {
            lineas.push(format!(
                "  {} sin tocar.",
                puro::cantidad(recuento.enlaces, "enlace", "enlaces")
            ));
        }
        if orden.aviso {
            lineas.push(format!(
                "  ⚠ {} en directorios impide entrar en ellos",
                orden.texto
            ));
        }
        if let Some(motivo) = &recuento.incompleto {
            lineas.push(format!("  Recuento incompleto: {motivo}"));
        }
        lineas.push(String::new());
        lineas.push("  ¿Seguro?".to_string());
        Some(Dialogo::Confirmar {
            titulo: "PERMISOS RECURSIVOS".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Permisos(AccionPermisos::Aplicar(orden)),
        })
    }

    /// `Esc` en el recuento: sus `ListarArbol` dejan de esperarse.
    fn cancelar_recuento(&mut self, id: u64) {
        self.peticiones_archivos.retain(|_, peticion| {
            !matches!(
                peticion,
                PeticionArchivos::Permisos(PeticionPermisosCliente::Recuento { recuento })
                    if *recuento == id
            )
        });
    }

    /// Aplica de verdad: en local, en un hilo; en remoto, `CambiarPermisos`.
    fn aplicar_permisos(&mut self, orden: OrdenPermisos) {
        let OrdenPermisos {
            lado,
            host_id,
            objetivos,
            modo,
            mascara,
            alcance,
            texto,
            ..
        } = orden;
        self.mensaje(format!("permisos {texto}: aplicando…"), false);
        match lado {
            Lado::Local => {
                let rutas: Vec<PathBuf> = objetivos
                    .iter()
                    .map(|objetivo| PathBuf::from(&objetivo.ruta))
                    .collect();
                let tx = self.eventos_tx.clone();
                self.runtime.spawn_blocking(move || {
                    let resultado = puro::aplicar_local(&rutas, modo, mascara, alcance);
                    let _ = tx.send(Evento::Archivos(EventoArchivos::Permisos(
                        EventoPermisos::Aplicado { texto, resultado },
                    )));
                });
            }
            Lado::Remoto => {
                let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Permisos(
                    PeticionPermisosCliente::Aplicar { host_id, texto },
                ));
                self.servidor
                    .enviar(protocolo::MensajeCliente::CambiarPermisos {
                        peticion_id,
                        host_id,
                        rutas: objetivos
                            .into_iter()
                            .map(|objetivo| objetivo.ruta)
                            .collect(),
                        modo,
                        mascara,
                        alcance,
                    });
            }
        }
    }
}
