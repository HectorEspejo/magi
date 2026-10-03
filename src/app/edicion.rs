//! Edición de ficheros con `E` (Fase 8, §4.1): un fichero remoto se baja a un
//! temporal privado del servidor, se abre con el editor del usuario con la TUI
//! suspendida, se compara por SHA-256 y, si cambió, se sube con los permisos
//! del original tras comprobar que el remoto no cambió entretanto.
//!
//! La lógica pura del ciclo vive en `archivos::edicion`; aquí están los
//! diálogos, las peticiones al servidor y el lanzamiento del editor. Las
//! ediciones viven en la `App` y no en `EstadoArchivos`: una subida en curso
//! sobrevive a salir de Archivos o a cambiar de host (T55).
//!
//! El temporal solo se borra en `cerrar_edicion`, por uno de los motivos de
//! `archivos::edicion::Cierre` y si la fase lo permite; ante un error, una
//! cancelación o el servidor caído se conserva y se dice su ruta.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};

use crate::archivos::edicion::{self as ciclo, Bajado, Cierre, Fase, Objetivo, Remoto};
use crate::archivos::{Entrada, Lado, TipoEntrada};
use crate::protocolo::{self, InfoTransferencia};
use crate::ui::Vista;

use super::{App, Dialogo, PeticionArchivos, RespuestaArchivos};

/// Ediciones en marcha en esta ventana.
#[derive(Default)]
pub struct Ediciones {
    pub siguiente: u64,
    pub vivas: BTreeMap<u64, Edicion>,
}

impl Ediciones {
    /// La edición viva de `host:ruta`, si la hay («ya lo estás editando»).
    pub fn de(&self, host_id: i64, ruta: &str) -> Option<u64> {
        self.vivas
            .iter()
            .find(|(_, edicion)| {
                edicion.objetivo.host_id == host_id && edicion.objetivo.ruta == ruta
            })
            .map(|(id, _)| *id)
    }
}

/// Una edición remota viva: lo fijado al pulsar `E`, lo fijado al bajar el
/// temporal y en qué punto del ciclo está.
#[derive(Debug, Clone)]
pub struct Edicion {
    pub objetivo: Objetivo,
    /// Ruta local del temporal, desde que el servidor la da.
    pub temporal: Option<PathBuf>,
    /// mtime, tamaño y SHA-256 del temporal recién bajado.
    pub bajado: Option<Bajado>,
    pub fase: Fase,
}

impl Edicion {
    fn temporal_legible(&self) -> String {
        self.temporal
            .as_ref()
            .map(|ruta| ruta.display().to_string())
            .unwrap_or_else(|| "—".to_string())
    }
}

/// Diálogos propios de la edición (viven en el hueco `App::dialogo`). Llevan
/// fijado lo que enseñan; las teclas actúan sobre la edición por su id.
pub enum DialogoEdicion {
    /// «¿Subir cambios a host:ruta?»: `s` verifica y sube, `n` deja elegir.
    Subir { edicion: u64, destino: String },
    /// Con «no»: `d` descartar (con confirmación), `c` copia local, `esc`
    /// vuelve a preguntar.
    NoSubir {
        edicion: u64,
        temporal: String,
        dir_local: String,
    },
    /// El remoto cambió desde que se bajó (§6.4): tamaños y fechas al abrirlo
    /// y ahora (`None`: ya no existe). Nada se sube sin pulsar `s`.
    Conflicto {
        edicion: u64,
        destino: String,
        antes: (u64, i64),
        ahora: Option<(u64, i64)>,
    },
    /// La verificación o la subida falló: el temporal se conserva y se dice
    /// dónde está.
    Fallida {
        edicion: u64,
        destino: String,
        error: String,
        temporal: String,
    },
    /// Una pregunta de sí o no con lo que hace cada respuesta ya fijado
    /// (parece binario, propietario, sensibles, descartar).
    Pregunta(Box<Pregunta>),
}

impl DialogoEdicion {
    /// La edición a la que pertenece el diálogo.
    pub fn edicion(&self) -> u64 {
        match self {
            DialogoEdicion::Subir { edicion, .. }
            | DialogoEdicion::NoSubir { edicion, .. }
            | DialogoEdicion::Conflicto { edicion, .. }
            | DialogoEdicion::Fallida { edicion, .. } => *edicion,
            DialogoEdicion::Pregunta(pregunta) => pregunta.edicion,
        }
    }
}

/// Pregunta de sí o no de la edición.
pub struct Pregunta {
    pub edicion: u64,
    pub titulo: String,
    pub lineas: Vec<String>,
    pub peligro: bool,
    /// Texto de la tecla `s` y lo que hace.
    pub si: (&'static str, AccionEdicion),
    /// Texto de `n` / `esc` y lo que hace.
    pub no: (&'static str, AccionEdicion),
}

/// Acciones confirmadas de la edición (viajan en `AccionDialogo::Edicion` o
/// en las respuestas de una `Pregunta`).
pub enum AccionEdicion {
    /// Bajar un fichero que pasa de `[archivos] editar_max_mb` (confirmado).
    Bajar(Box<Objetivo>),
    /// Abrir el editor aunque el fichero parezca binario.
    EditarBinario(u64),
    /// No editar lo que parece binario: el temporal sigue sin cambios.
    NoEditarBinario(u64),
    /// Seguir la subida desde un paso, tras un aviso confirmado.
    Subir { edicion: u64, paso: PasoSubida },
    /// Descartar los cambios (confirmado): se borra el temporal.
    Descartar(u64),
    /// Volver a un diálogo de la edición.
    Volver(Box<DialogoEdicion>),
}

/// Pasos de la subida tras verificar el remoto: el aviso de propietario, el
/// de sensibles (F4) y el envío.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasoSubida {
    Propietario,
    Sensibles,
    Enviar,
}

/// Peticiones al servidor en vuelo de una edición.
#[derive(Debug)]
pub enum PeticionEdicion {
    /// `StatRemoto` del destino de un enlace, antes de bajar nada.
    ResolverEnlace { objetivo: Objetivo, saltos: u8 },
    /// `DescargarTemporal{edicion: true}`.
    Descargar { edicion: u64 },
    /// `StatRemoto` justo antes de subir.
    Verificar { edicion: u64 },
    /// `Transferir` de la subida: `Hecho` al encolarla o `Error` si no.
    Subir { edicion: u64 },
}

/// Fichero que hay que abrir con el editor en cuanto el bucle pueda suspender
/// la TUI.
#[derive(Debug, Clone)]
pub struct PeticionEditor {
    pub ruta: std::path::PathBuf,
    /// Edición remota a la que pertenece el temporal; ninguna si es un
    /// fichero local.
    pub edicion: Option<u64>,
}

impl App {
    /// `E` en Archivos: editar el fichero de la fila.
    pub(super) fn editar_entrada(&mut self) {
        let Some(estado) = &self.archivos else {
            self.mensaje("abre Archivos para editar un fichero", true);
            return;
        };
        let lado = estado.activo;
        let panel = super::panel_activo(estado);
        let dir = panel.ruta.clone();
        let Some(entrada) = panel.entrada_actual().cloned() else {
            return;
        };
        if entrada.nombre == ".." {
            self.mensaje("«..» es un directorio: no se edita", true);
            return;
        }
        match lado {
            Lado::Local => self.editar_local(&dir, &entrada),
            Lado::Remoto => self.editar_remoto(&dir, entrada),
        }
    }

    /// `E` en el panel local: solo abre el editor; al volver se refresca.
    fn editar_local(&mut self, dir: &str, entrada: &Entrada) {
        let ruta = Path::new(dir).join(&entrada.nombre);
        // `metadata` sigue los enlaces: un enlace a directorio se trata como
        // directorio.
        match std::fs::metadata(&ruta) {
            Ok(metadata) if metadata.is_dir() => {
                let que = match entrada.enlace {
                    Some(_) => "es un enlace a un directorio",
                    None => "es un directorio",
                };
                self.mensaje(format!("«{}» {que}: no se edita", entrada.nombre), true);
            }
            Ok(_) => {
                self.peticion_editor = Some(PeticionEditor {
                    ruta,
                    edicion: None,
                });
            }
            Err(error) => self.mensaje(format!("{}: {error}", ruta.display()), true),
        }
    }

    /// `E` en el panel remoto: fija el objetivo y lo baja (o, si es un
    /// enlace, antes resuelve su destino).
    fn editar_remoto(&mut self, dir: &str, entrada: Entrada) {
        let Some(estado) = &self.archivos else {
            return;
        };
        let sin_directorio = estado.solo_local || dir.is_empty();
        let dir_local = match estado.local.ruta.is_empty() {
            true => self.rutas.hogar.clone(),
            false => PathBuf::from(&estado.local.ruta),
        };
        // Todo queda fijado al pulsar `E`: el usuario de conexión, el
        // directorio de la copia local y lo que dice el listado.
        let mut objetivo = Objetivo {
            host_id: estado.host_id,
            host_nombre: estado.host_nombre.clone(),
            ruta: crate::servidor::sftp::join(dir, &entrada.nombre),
            tamano: entrada.tamano,
            permisos: entrada.permisos,
            propietario: entrada.propietario.clone(),
            usuario_conexion: estado.usuario_conexion.clone(),
            uid_conexion: estado.uid_conexion,
            dir_local,
        };
        if sin_directorio {
            self.mensaje("el panel remoto todavía no tiene directorio", true);
            return;
        }
        if entrada.es_dir() {
            self.mensaje(
                format!("«{}» es un directorio: no se edita", entrada.nombre),
                true,
            );
            return;
        }
        if let Some(motivo) = self.motivo_sin_servidor() {
            self.mensaje(motivo, true);
            return;
        }
        if entrada.tipo == TipoEntrada::Enlace {
            // Se edita el destino, para que la subida no sustituya el enlace
            // por un fichero (decisión del plan).
            let Some(destino) = &entrada.enlace else {
                self.mensaje(
                    format!("no se sabe a dónde apunta «{}»", entrada.nombre),
                    true,
                );
                return;
            };
            objetivo.ruta = ciclo::resolver_enlace(dir, destino);
            self.resolver_enlace(objetivo, 1);
            return;
        }
        self.comenzar_edicion(objetivo);
    }

    /// Pide el `StatRemoto` del destino de un enlace.
    fn resolver_enlace(&mut self, objetivo: Objetivo, saltos: u8) {
        let (host_id, ruta) = (objetivo.host_id, objetivo.ruta.clone());
        let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Edicion(
            PeticionEdicion::ResolverEnlace { objetivo, saltos },
        ));
        self.servidor.enviar(protocolo::MensajeCliente::StatRemoto {
            peticion_id,
            host_id,
            ruta,
        });
    }

    /// Respuesta al `StatRemoto` del destino de un enlace.
    fn enlace_resuelto(
        &mut self,
        mut objetivo: Objetivo,
        saltos: u8,
        respuesta: RespuestaArchivos,
    ) {
        let entrada = match respuesta {
            RespuestaArchivos::Stat(Some(entrada)) => entrada,
            RespuestaArchivos::Stat(None) => {
                self.mensaje(
                    format!("el enlace apunta a {}, que no existe", objetivo.ruta),
                    true,
                );
                return;
            }
            RespuestaArchivos::Error(motivo) => {
                self.mensaje(format!("{}: {motivo}", objetivo.ruta), true);
                return;
            }
            _ => return,
        };
        match entrada.tipo {
            TipoEntrada::Directorio => self.mensaje(
                format!(
                    "el enlace apunta a un directorio ({}): no se edita",
                    objetivo.ruta
                ),
                true,
            ),
            TipoEntrada::Enlace => match &entrada.enlace {
                Some(destino) if saltos < ciclo::MAXIMO_SALTOS => {
                    let dir = crate::servidor::sftp::padre(&objetivo.ruta);
                    objetivo.ruta = ciclo::resolver_enlace(&dir, destino);
                    self.resolver_enlace(objetivo, saltos + 1);
                }
                _ => self.mensaje(
                    format!(
                        "{} es otro enlace: edita el fichero al que apunta",
                        objetivo.ruta
                    ),
                    true,
                ),
            },
            TipoEntrada::Fichero => {
                // Tamaño, permisos y dueño son los del destino, no los del
                // enlace del listado.
                objetivo.tamano = entrada.tamano;
                objetivo.permisos = entrada.permisos;
                objetivo.propietario = entrada.propietario;
                self.comenzar_edicion(objetivo);
            }
        }
    }

    /// «Ya lo estás editando» y el aviso de tamaño antes de bajar.
    fn comenzar_edicion(&mut self, objetivo: Objetivo) {
        if self.avisar_si_ya_se_edita(&objetivo) {
            return;
        }
        let limite = self
            .config
            .archivos
            .editar_max_mb
            .saturating_mul(1024 * 1024);
        if objetivo.tamano > limite {
            let lineas = vec![
                format!(
                    "  {} ocupa {}, más que [archivos] editar_max_mb ({} MB).",
                    objetivo.nombre(),
                    crate::archivos::tamano_legible(objetivo.tamano),
                    self.config.archivos.editar_max_mb
                ),
                String::new(),
                "  Se bajará entero a un temporal para editarlo.".to_string(),
                "  ¿Seguro?".to_string(),
            ];
            self.mostrar_edicion(Dialogo::Confirmar {
                titulo: "FICHERO GRANDE".to_string(),
                lineas,
                peligro: false,
                accion: super::AccionDialogo::Edicion(AccionEdicion::Bajar(Box::new(objetivo))),
            });
            return;
        }
        self.bajar_para_editar(objetivo);
    }

    /// Si ya hay una edición viva de ese fichero en esta ventana, lo dice (y,
    /// si espera al usuario y su diálogo se perdió, lo vuelve a enseñar).
    fn avisar_si_ya_se_edita(&mut self, objetivo: &Objetivo) -> bool {
        let Some(id) = self.ediciones.de(objetivo.host_id, &objetivo.ruta) else {
            return false;
        };
        self.mensaje(
            format!("ya lo estás editando: {}", objetivo.destino()),
            true,
        );
        self.reabrir_si_se_perdio(id);
        true
    }

    /// `DescargarTemporal{edicion: true}`: la edición nace en «descargando».
    fn bajar_para_editar(&mut self, objetivo: Objetivo) {
        if self.avisar_si_ya_se_edita(&objetivo) {
            return;
        }
        if let Some(motivo) = self.motivo_sin_servidor() {
            self.mensaje(motivo, true);
            return;
        }
        self.ediciones.siguiente += 1;
        let id = self.ediciones.siguiente;
        let (host_id, ruta, destino) =
            (objetivo.host_id, objetivo.ruta.clone(), objetivo.destino());
        self.ediciones.vivas.insert(
            id,
            Edicion {
                objetivo,
                temporal: None,
                bajado: None,
                fase: Fase::Descargando,
            },
        );
        let peticion_id =
            self.nueva_peticion_archivos(PeticionArchivos::Edicion(PeticionEdicion::Descargar {
                edicion: id,
            }));
        self.servidor
            .enviar(protocolo::MensajeCliente::DescargarTemporal {
                host_id,
                ruta,
                peticion_id,
                edicion: true,
            });
        self.mensaje(format!("bajando {destino} para editarlo…"), false);
    }

    /// Respuesta del servidor a una petición de edición.
    pub(super) fn respuesta_edicion(
        &mut self,
        _peticion_id: u64,
        peticion: PeticionEdicion,
        respuesta: RespuestaArchivos,
    ) {
        match peticion {
            PeticionEdicion::ResolverEnlace { objetivo, saltos } => {
                self.enlace_resuelto(objetivo, saltos, respuesta)
            }
            PeticionEdicion::Descargar { edicion } => self.temporal_bajado(edicion, respuesta),
            PeticionEdicion::Verificar { edicion } => self.remoto_verificado(edicion, respuesta),
            PeticionEdicion::Subir { edicion } => self.subida_encolada(edicion, respuesta),
        }
    }

    /// Llegó el temporal (o el error de la descarga).
    fn temporal_bajado(&mut self, id: u64, respuesta: RespuestaArchivos) {
        let ruta = match respuesta {
            RespuestaArchivos::RutaTemporal(ruta) => ruta,
            RespuestaArchivos::Error(motivo) => {
                if let Some(edicion) = self.ediciones.vivas.remove(&id) {
                    self.mensaje(
                        format!("no se pudo bajar {}: {motivo}", edicion.objetivo.destino()),
                        true,
                    );
                }
                return;
            }
            _ => return,
        };
        let Some(edicion) = self.ediciones.vivas.get_mut(&id) else {
            // Nadie lo espera: es una copia recién bajada que nadie ha tocado.
            self.servidor
                .enviar(protocolo::MensajeCliente::BorrarTemporal { ruta });
            return;
        };
        edicion.temporal = Some(PathBuf::from(&ruta));
        let bajado = match ciclo::leer_bajado(Path::new(&ruta)) {
            Ok(bajado) => bajado,
            Err(motivo) => {
                self.mensaje(format!("no se pudo leer el temporal: {motivo}"), true);
                self.cerrar_edicion(id, Cierre::SinCambios);
                return;
            }
        };
        let binario = bajado.binario;
        edicion.bajado = Some(bajado);
        // Si el usuario ya no está en Archivos no se le abre un editor (ni una
        // pregunta) encima: la edición se cancela sin cambios, como el visor
        // de la F4.
        if !matches!(self.vista, Vista::Archivos | Vista::Transferencias) {
            let destino = edicion.objetivo.destino();
            self.cerrar_edicion(id, Cierre::SinCambios);
            self.mensaje(
                format!("no se abre el editor de {destino}: saliste de Archivos"),
                true,
            );
            return;
        }
        if binario {
            edicion.fase = Fase::ConfirmandoBinario;
            if let Some(pregunta) = self.pregunta_binario(id) {
                self.mostrar_edicion(Dialogo::Edicion(pregunta));
            }
            return;
        }
        self.abrir_editor_de(id);
    }

    /// «Parece binario»: `s` abre el editor igualmente; `n` termina sin
    /// cambios (el temporal se borra).
    fn pregunta_binario(&self, id: u64) -> Option<DialogoEdicion> {
        let edicion = self.ediciones.vivas.get(&id)?;
        Some(DialogoEdicion::Pregunta(Box::new(Pregunta {
            edicion: id,
            titulo: "PARECE BINARIO".to_string(),
            lineas: vec![
                format!(
                    "{} tiene bytes nulos en sus primeros 8 KiB.",
                    edicion.objetivo.nombre()
                ),
                "El editor podría estropearlo al guardar.".to_string(),
            ],
            peligro: false,
            si: ("editar igualmente", AccionEdicion::EditarBinario(id)),
            no: ("no editar", AccionEdicion::NoEditarBinario(id)),
        })))
    }

    /// Pide el editor sobre el temporal de la edición (lo abre el bucle en
    /// cuanto puede suspender la TUI).
    fn abrir_editor_de(&mut self, id: u64) {
        let Some(edicion) = self.ediciones.vivas.get_mut(&id) else {
            return;
        };
        let Some(temporal) = edicion.temporal.clone() else {
            return;
        };
        edicion.fase = Fase::Editando;
        self.peticion_editor = Some(PeticionEditor {
            ruta: temporal,
            edicion: Some(id),
        });
    }

    /// `s` en «¿subir cambios?» o `r` tras un fallo: `StatRemoto` antes de
    /// subir nada (nunca se sube sin pasar por aquí).
    fn verificar_remoto(&mut self, id: u64) {
        if let Some(motivo) = self.motivo_sin_servidor() {
            self.subida_fallida(id, motivo);
            return;
        }
        let Some(edicion) = self.ediciones.vivas.get_mut(&id) else {
            return;
        };
        edicion.fase = Fase::Verificando;
        let (host_id, ruta, destino) = (
            edicion.objetivo.host_id,
            edicion.objetivo.ruta.clone(),
            edicion.objetivo.destino(),
        );
        let peticion_id =
            self.nueva_peticion_archivos(PeticionArchivos::Edicion(PeticionEdicion::Verificar {
                edicion: id,
            }));
        self.servidor.enviar(protocolo::MensajeCliente::StatRemoto {
            peticion_id,
            host_id,
            ruta,
        });
        self.mensaje(format!("comprobando {destino} antes de subir…"), false);
    }

    /// Respuesta al `StatRemoto` de antes de subir.
    fn remoto_verificado(&mut self, id: u64, respuesta: RespuestaArchivos) {
        let Some(edicion) = self.ediciones.vivas.get_mut(&id) else {
            return;
        };
        // Una respuesta que llega cuando la edición ya está en otra fase es
        // de una petición que ya no está en vuelo.
        if edicion.fase != Fase::Verificando {
            return;
        }
        let ahora = match respuesta {
            RespuestaArchivos::Stat(ahora) => ahora,
            RespuestaArchivos::Error(motivo) => {
                self.subida_fallida(id, motivo);
                return;
            }
            _ => return,
        };
        let Some(bajado) = edicion.bajado.clone() else {
            return;
        };
        edicion.fase = Fase::Preguntando;
        // El aviso de propietario mira el uid del remoto de ahora (§4.1) y la
        // subida reaplica los permisos que tiene ahora el original.
        if let Some(entrada) = ahora.as_ref().filter(|e| e.tipo == TipoEntrada::Fichero) {
            if entrada.permisos.is_some() {
                edicion.objetivo.permisos = entrada.permisos;
            }
            if entrada.propietario.is_some() {
                edicion.objetivo.propietario = entrada.propietario.clone();
            }
        }
        match ciclo::comparar_remoto(&bajado, ahora.as_ref()) {
            Remoto::Igual => self.seguir_subida(id, PasoSubida::Propietario),
            Remoto::Cambiado { tamano, mtime } => {
                self.mostrar_conflicto(id, &bajado, Some((tamano, mtime)))
            }
            Remoto::NoExiste => self.mostrar_conflicto(id, &bajado, None),
        }
    }

    fn mostrar_conflicto(&mut self, id: u64, bajado: &Bajado, ahora: Option<(u64, i64)>) {
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let dialogo = DialogoEdicion::Conflicto {
            edicion: id,
            destino: edicion.objetivo.destino(),
            antes: (bajado.tamano, bajado.mtime),
            ahora,
        };
        self.mensaje(
            format!(
                "{} cambió en el host mientras lo editabas",
                edicion.objetivo.destino()
            ),
            true,
        );
        self.mostrar_edicion(Dialogo::Edicion(dialogo));
    }

    /// Avisos antes de encolar la subida, en orden: propietario, sensibles y
    /// envío. Cada aviso es una pregunta; con «no» se vuelve a elegir qué
    /// hacer con los cambios.
    fn seguir_subida(&mut self, id: u64, paso: PasoSubida) {
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let objetivo = edicion.objetivo.clone();
        match paso {
            PasoSubida::Propietario => {
                let Some(nuevo) = objetivo.nuevo_propietario() else {
                    return self.seguir_subida(id, PasoSubida::Sensibles);
                };
                let no_subir = self.dialogo_no_subir(id);
                self.preguntar(Pregunta {
                    edicion: id,
                    titulo: "CAMBIA EL PROPIETARIO".to_string(),
                    lineas: vec![
                        format!(
                            "{} es de {}.",
                            objetivo.destino(),
                            objetivo.propietario_actual()
                        ),
                        format!("Tras subirlo pertenecerá a {nuevo}."),
                        "Los permisos se conservan; el propietario, no.".to_string(),
                    ],
                    peligro: false,
                    si: (
                        "subir igualmente",
                        AccionEdicion::Subir {
                            edicion: id,
                            paso: PasoSubida::Sensibles,
                        },
                    ),
                    no: ("no subir", AccionEdicion::Volver(Box::new(no_subir))),
                });
            }
            PasoSubida::Sensibles => {
                let nombre = objetivo.nombre();
                let sensibles =
                    crate::archivos::sensibles::Sensibles::nuevo(&self.config.archivos.avisar);
                let Some(patron) = sensibles.coincide(&nombre).map(str::to_string) else {
                    return self.seguir_subida(id, PasoSubida::Enviar);
                };
                let no_subir = self.dialogo_no_subir(id);
                self.preguntar(Pregunta {
                    edicion: id,
                    titulo: "FICHERO SENSIBLE".to_string(),
                    lineas: vec![
                        format!("«{nombre}» casa con «{patron}» de [archivos] avisar."),
                        format!("Se subirá a {}.", objetivo.destino()),
                    ],
                    peligro: true,
                    si: (
                        "subir igualmente",
                        AccionEdicion::Subir {
                            edicion: id,
                            paso: PasoSubida::Enviar,
                        },
                    ),
                    no: ("no subir", AccionEdicion::Volver(Box::new(no_subir))),
                });
            }
            PasoSubida::Enviar => self.enviar_subida(id),
        }
    }

    /// `Transferir` de la subida: política sobrescribir, los permisos del
    /// original y la etiqueta de edición. Nunca con `borrar_origen`: el
    /// temporal lo borra esta ventana cuando la fila llegue a `hecha`.
    fn enviar_subida(&mut self, id: u64) {
        if let Some(motivo) = self.motivo_sin_servidor() {
            self.subida_fallida(id, motivo);
            return;
        }
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let Some(temporal) = edicion.temporal.clone() else {
            return;
        };
        let objetivo = edicion.objetivo.clone();
        let bytes = std::fs::metadata(&temporal)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let peticion_id =
            self.nueva_peticion_archivos(PeticionArchivos::Edicion(PeticionEdicion::Subir {
                edicion: id,
            }));
        if let Some(edicion) = self.ediciones.vivas.get_mut(&id) {
            edicion.fase = Fase::Subiendo { peticion_id };
        }
        self.servidor.enviar(protocolo::MensajeCliente::Transferir {
            host_id: objetivo.host_id,
            direccion: protocolo::Direccion::Subida,
            elementos: vec![protocolo::ElementoTransferencia {
                origen: temporal.display().to_string(),
                destino: objetivo.ruta.clone(),
                bytes,
                es_directorio: false,
                politica: Some(protocolo::Politica::Sobrescribir),
                permisos: objetivo.permisos_a_conservar(),
            }],
            politica: protocolo::Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some(peticion_id),
            borrar_al_terminar: Vec::new(),
            deliberacion: None,
            sincronizacion: None,
            etiqueta: Some(protocolo::EtiquetaTransferencia::Edicion),
        });
        self.mensaje(
            format!("subiendo tus cambios a {}…", objetivo.destino()),
            false,
        );
    }

    /// Respuesta al `Transferir`: `Hecho` es «encolada»; el final llega por
    /// la cola.
    fn subida_encolada(&mut self, id: u64, respuesta: RespuestaArchivos) {
        let subiendo = self
            .ediciones
            .vivas
            .get(&id)
            .is_some_and(|edicion| matches!(edicion.fase, Fase::Subiendo { .. }));
        if !subiendo {
            return;
        }
        if let RespuestaArchivos::Error(motivo) = respuesta {
            self.subida_fallida(id, motivo);
        }
    }

    /// La cola cambió: las subidas de ediciones que terminan. La fila propia
    /// se reconoce por `(solicitante, peticion_id)`; el refresco del panel lo
    /// hace `actualizar_cola` con toda transferencia propia que termina.
    pub(super) fn cola_de_ediciones(&mut self, lista: &[InfoTransferencia]) {
        let Some(cliente_id) = self.cliente_id else {
            return;
        };
        let terminadas: Vec<(u64, InfoTransferencia)> = self
            .ediciones
            .vivas
            .iter()
            .filter_map(|(id, edicion)| {
                let Fase::Subiendo { peticion_id } = edicion.fase else {
                    return None;
                };
                lista
                    .iter()
                    .find(|fila| {
                        fila.solicitante == cliente_id && fila.peticion_id == Some(peticion_id)
                    })
                    .filter(|fila| fila.estado.terminada())
                    .map(|fila| (*id, fila.clone()))
            })
            .collect();
        for (id, fila) in terminadas {
            match fila.estado {
                // Con política sobrescribir no se omite nada; si aun así se
                // omitió, el fichero no subió y el temporal no se toca.
                protocolo::EstadoTransferencia::Hecha if fila.omitidos == 0 => {
                    let destino = self
                        .ediciones
                        .vivas
                        .get(&id)
                        .map(|edicion| edicion.objetivo.destino())
                        .unwrap_or_default();
                    self.cerrar_edicion(id, Cierre::Hecha);
                    self.mensaje(format!("cambios subidos a {destino}"), false);
                }
                protocolo::EstadoTransferencia::Hecha => {
                    self.subida_fallida(id, "el servidor omitió el fichero".to_string());
                }
                protocolo::EstadoTransferencia::Cancelada => {
                    self.subida_fallida(id, "subida cancelada".to_string());
                }
                _ => {
                    let error = fila
                        .error
                        .unwrap_or_else(|| "error en la subida".to_string());
                    self.subida_fallida(id, error);
                }
            }
        }
    }

    /// El servidor cayó: ninguna edición en subida borra su temporal. Las que
    /// verificaban o subían quedan fallidas; las que bajaban se pierden (no
    /// tienen nada que conservar). Los diálogos de la edición van a la pila,
    /// debajo del de SERVIDOR CAÍDO, para que no se pierdan.
    pub(super) fn ediciones_servidor_caido(&mut self) {
        if matches!(self.dialogo, Some(Dialogo::Edicion(_))) {
            if let Some(abierto) = self.dialogo.take() {
                self.pila_dialogos.push(abierto);
            }
        }
        let afectadas: Vec<(u64, Fase)> = self
            .ediciones
            .vivas
            .iter()
            .map(|(id, edicion)| (*id, edicion.fase.clone()))
            .collect();
        for (id, fase) in afectadas {
            match fase {
                Fase::Descargando => {
                    self.ediciones.vivas.remove(&id);
                }
                Fase::Verificando | Fase::Subiendo { .. } => {
                    let error = "se perdió el servidor de sesiones".to_string();
                    if let Some(dialogo) = self.marcar_fallida(id, error) {
                        self.pila_dialogos.push(Dialogo::Edicion(dialogo));
                    }
                }
                _ => {}
            }
        }
    }

    /// La verificación o la subida falló: el temporal se conserva y el
    /// diálogo dice dónde está.
    fn subida_fallida(&mut self, id: u64, error: String) {
        if let Some(dialogo) = self.marcar_fallida(id, error) {
            if let DialogoEdicion::Fallida {
                destino, temporal, ..
            } = &dialogo
            {
                self.mensaje(
                    format!("no se subió {destino}; tu versión sigue en {temporal}"),
                    true,
                );
            }
            self.mostrar_edicion(Dialogo::Edicion(dialogo));
        }
    }

    /// Pasa la edición a «fallida» y devuelve su diálogo.
    fn marcar_fallida(&mut self, id: u64, error: String) -> Option<DialogoEdicion> {
        let edicion = self.ediciones.vivas.get_mut(&id)?;
        edicion.fase = Fase::Fallida {
            error: error.clone(),
        };
        Some(DialogoEdicion::Fallida {
            edicion: id,
            destino: edicion.objetivo.destino(),
            error,
            temporal: edicion.temporal_legible(),
        })
    }

    fn dialogo_no_subir(&self, id: u64) -> DialogoEdicion {
        let (temporal, dir_local) = self
            .ediciones
            .vivas
            .get(&id)
            .map(|edicion| {
                (
                    edicion.temporal_legible(),
                    edicion.objetivo.dir_local.display().to_string(),
                )
            })
            .unwrap_or_default();
        DialogoEdicion::NoSubir {
            edicion: id,
            temporal,
            dir_local,
        }
    }

    fn dialogo_subir(&self, id: u64) -> DialogoEdicion {
        DialogoEdicion::Subir {
            edicion: id,
            destino: self
                .ediciones
                .vivas
                .get(&id)
                .map(|edicion| edicion.objetivo.destino())
                .unwrap_or_default(),
        }
    }

    /// Pregunta de confirmación antes de descartar (es destructivo); con
    /// «no» se vuelve a `volver`.
    fn confirmar_descarte(&mut self, id: u64, volver: DialogoEdicion) {
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let pregunta = Pregunta {
            edicion: id,
            titulo: "DESCARTAR CAMBIOS".to_string(),
            lineas: vec![
                format!(
                    "Se borrará el temporal con tus cambios a {}:",
                    edicion.objetivo.nombre()
                ),
                format!("  {}", edicion.temporal_legible()),
                "No se puede deshacer.".to_string(),
            ],
            peligro: true,
            si: ("descartar", AccionEdicion::Descartar(id)),
            no: ("volver", AccionEdicion::Volver(Box::new(volver))),
        };
        self.preguntar(pregunta);
    }

    /// `c`: guarda el temporal como copia local en el directorio del panel
    /// local fijado al pulsar `E`. Con la copia verificada el trabajo está a
    /// salvo y el temporal se borra; si no, se conserva y se vuelve a
    /// `volver`.
    fn guardar_copia_local(&mut self, id: u64, volver: DialogoEdicion) {
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let Some(temporal) = edicion.temporal.clone() else {
            return;
        };
        let objetivo = edicion.objetivo.clone();
        match ciclo::guardar_copia(
            &temporal,
            &objetivo.dir_local,
            &objetivo.nombre(),
            &chrono::Local::now(),
        ) {
            Ok(copia) => {
                self.cerrar_edicion(id, Cierre::CopiaGuardada);
                self.refrescar_panel_local();
                self.mensaje(
                    format!("tu versión está a salvo en {}", copia.display()),
                    false,
                );
            }
            Err(motivo) => {
                self.mensaje(
                    format!(
                        "no se pudo guardar la copia: {motivo}; el temporal sigue en {}",
                        temporal.display()
                    ),
                    true,
                );
                self.mostrar_edicion(Dialogo::Edicion(volver));
            }
        }
    }

    /// Termina una edición. El temporal solo se borra si la fase lo permite
    /// para ese `cierre` (T55); si no, se conserva y se dice dónde está.
    fn cerrar_edicion(&mut self, id: u64, cierre: Cierre) {
        let Some(edicion) = self.ediciones.vivas.remove(&id) else {
            return;
        };
        let Some(temporal) = edicion.temporal else {
            return;
        };
        if !ciclo::borrado_permitido(&edicion.fase, cierre) {
            tracing::warn!(
                "edición {id}: no se borra el temporal ({cierre:?} en {:?})",
                edicion.fase
            );
            self.mensaje(
                format!("el temporal se conserva en {}", temporal.display()),
                true,
            );
            return;
        }
        self.servidor
            .enviar(protocolo::MensajeCliente::BorrarTemporal {
                ruta: temporal.display().to_string(),
            });
    }

    /// `esc` en el diálogo de fallo: la edición termina sin borrar nada.
    fn abandonar_edicion(&mut self, id: u64) {
        if let Some(edicion) = self.ediciones.vivas.remove(&id) {
            self.mensaje(
                format!(
                    "no se subió nada; el temporal se queda en {}",
                    edicion.temporal_legible()
                ),
                true,
            );
        }
    }

    pub(super) fn tecla_dialogo_edicion(&mut self, dialogo: DialogoEdicion, tecla: KeyEvent) {
        match dialogo {
            DialogoEdicion::Subir { edicion, destino } => match tecla.code {
                KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                    self.verificar_remoto(edicion)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    let no_subir = self.dialogo_no_subir(edicion);
                    self.mostrar_edicion(Dialogo::Edicion(no_subir));
                }
                _ => {
                    self.dialogo =
                        Some(Dialogo::Edicion(DialogoEdicion::Subir { edicion, destino }))
                }
            },
            DialogoEdicion::NoSubir {
                edicion,
                temporal,
                dir_local,
            } => {
                let mismo = DialogoEdicion::NoSubir {
                    edicion,
                    temporal,
                    dir_local,
                };
                match tecla.code {
                    KeyCode::Char('d') => self.confirmar_descarte(edicion, mismo),
                    KeyCode::Char('c') => self.guardar_copia_local(edicion, mismo),
                    KeyCode::Esc => {
                        let subir = self.dialogo_subir(edicion);
                        self.mostrar_edicion(Dialogo::Edicion(subir));
                    }
                    _ => self.dialogo = Some(Dialogo::Edicion(mismo)),
                }
            }
            DialogoEdicion::Conflicto {
                edicion,
                destino,
                antes,
                ahora,
            } => {
                let mismo = DialogoEdicion::Conflicto {
                    edicion,
                    destino,
                    antes,
                    ahora,
                };
                match tecla.code {
                    // Solo `s` sobrescribe: ni `↵` ni ninguna otra tecla.
                    KeyCode::Char('s') => self.seguir_subida(edicion, PasoSubida::Propietario),
                    KeyCode::Char('c') => self.guardar_copia_local(edicion, mismo),
                    KeyCode::Char('d') => self.confirmar_descarte(edicion, mismo),
                    KeyCode::Esc => {
                        let subir = self.dialogo_subir(edicion);
                        self.mostrar_edicion(Dialogo::Edicion(subir));
                    }
                    _ => self.dialogo = Some(Dialogo::Edicion(mismo)),
                }
            }
            DialogoEdicion::Fallida {
                edicion,
                destino,
                error,
                temporal,
            } => {
                let mismo = DialogoEdicion::Fallida {
                    edicion,
                    destino,
                    error,
                    temporal,
                };
                match tecla.code {
                    KeyCode::Char('r') => {
                        // Sin servidor no hay nada que reintentar: el diálogo
                        // sigue ahí con el aviso en la barra.
                        if let Some(motivo) = self.motivo_sin_servidor() {
                            self.mensaje(motivo, true);
                            self.dialogo = Some(Dialogo::Edicion(mismo));
                        } else {
                            self.verificar_remoto(edicion);
                        }
                    }
                    KeyCode::Char('c') => self.guardar_copia_local(edicion, mismo),
                    KeyCode::Esc => self.abandonar_edicion(edicion),
                    _ => self.dialogo = Some(Dialogo::Edicion(mismo)),
                }
            }
            DialogoEdicion::Pregunta(pregunta) => match tecla.code {
                KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                    self.ejecutar_accion_edicion(pregunta.si.1)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.ejecutar_accion_edicion(pregunta.no.1)
                }
                _ => self.dialogo = Some(Dialogo::Edicion(DialogoEdicion::Pregunta(pregunta))),
            },
        }
    }

    pub(super) fn ejecutar_accion_edicion(&mut self, accion: AccionEdicion) {
        match accion {
            AccionEdicion::Bajar(objetivo) => self.bajar_para_editar(*objetivo),
            AccionEdicion::EditarBinario(id) => self.abrir_editor_de(id),
            AccionEdicion::NoEditarBinario(id) => {
                self.cerrar_edicion(id, Cierre::SinCambios);
                self.mensaje("no se edita: parece binario", false);
            }
            AccionEdicion::Subir { edicion, paso } => self.seguir_subida(edicion, paso),
            AccionEdicion::Descartar(id) => {
                let destino = self
                    .ediciones
                    .vivas
                    .get(&id)
                    .map(|edicion| edicion.objetivo.destino())
                    .unwrap_or_default();
                self.cerrar_edicion(id, Cierre::Descartada);
                self.mensaje(format!("cambios a {destino} descartados"), false);
            }
            AccionEdicion::Volver(dialogo) => self.mostrar_edicion(Dialogo::Edicion(*dialogo)),
        }
    }

    fn preguntar(&mut self, pregunta: Pregunta) {
        self.mostrar_edicion(Dialogo::Edicion(DialogoEdicion::Pregunta(Box::new(
            pregunta,
        ))));
    }

    /// Enseña un diálogo de la edición. Llega a menudo con la respuesta del
    /// servidor, así que no pisa el que esté abierto: lo aparta a la pila.
    fn mostrar_edicion(&mut self, dialogo: Dialogo) {
        self.desplazamiento_modal = 0;
        self.mostrar_dialogo_servidor(dialogo);
    }

    /// Si la edición espera al usuario y ningún diálogo suyo está abierto ni
    /// apartado (otro lo pisó), lo vuelve a enseñar.
    fn reabrir_si_se_perdio(&mut self, id: u64) {
        let suyo = |dialogo: &Dialogo| matches!(dialogo, Dialogo::Edicion(propio) if propio.edicion() == id);
        if self.dialogo.as_ref().is_some_and(suyo) || self.pila_dialogos.iter().any(suyo) {
            return;
        }
        let Some(edicion) = self.ediciones.vivas.get(&id) else {
            return;
        };
        let dialogo = match &edicion.fase {
            Fase::Preguntando => self.dialogo_subir(id),
            Fase::Fallida { error } => DialogoEdicion::Fallida {
                edicion: id,
                destino: edicion.objetivo.destino(),
                error: error.clone(),
                temporal: edicion.temporal_legible(),
            },
            Fase::ConfirmandoBinario => match self.pregunta_binario(id) {
                Some(pregunta) => pregunta,
                None => return,
            },
            _ => return,
        };
        self.mostrar_edicion(Dialogo::Edicion(dialogo));
    }

    /// Por qué no se puede hablar con el servidor ahora, si es el caso.
    fn motivo_sin_servidor(&self) -> Option<String> {
        if self.servidor_incompatible.is_some() {
            return Some(
                "servidor de otra versión de protocolo: magi servidor parar y volver a abrir"
                    .to_string(),
            );
        }
        if self.servidor_caido {
            return Some("el servidor de sesiones ha caído; relánzalo primero".to_string());
        }
        None
    }

    /// Vuelve a listar el panel local (tras editar un fichero local o guardar
    /// una copia).
    fn refrescar_panel_local(&mut self) {
        if let Some(estado) = &mut self.archivos {
            super::listar_local(estado, true);
        }
    }

    /// Suspende la TUI y abre el editor (desde `ejecutar`, con el terminal a
    /// mano). La vuelta aplica el tamaño real (T46) y sigue el ciclo de la
    /// edición en `editor_terminado`.
    pub(super) fn ver_en_editor(
        &mut self,
        terminal: &mut crate::ui::TerminalMagi,
        peticion: PeticionEditor,
    ) -> anyhow::Result<()> {
        // El hilo de teclas deja el tty antes de que el editor lo tome (T52).
        self.teclado
            .pausar_y_esperar(std::time::Duration::from_millis(200));
        let aviso = crate::ui::pager::suspender_y_editar(terminal, &self.config, &peticion.ruta);
        self.teclado.reanudar();
        let real = terminal
            .size()
            .map(|area| (area.width, area.height))
            .unwrap_or(self.geometria.aplicado());
        self.volver_de_suspension(terminal, real)?;
        self.editor_terminado(peticion, aviso);
        Ok(())
    }

    /// El editor volvió (o no se pudo lanzar, con `aviso`). Separado de la
    /// suspensión para poder probarlo sin terminal.
    #[doc(hidden)]
    pub fn editor_terminado(&mut self, peticion: PeticionEditor, aviso: Option<String>) {
        // Llega fuera del despacho de eventos: lo que cambie hay que pintarlo.
        self.sucio = true;
        let Some(id) = peticion.edicion else {
            // Fichero local: el panel enseña el tamaño y la fecha nuevos.
            self.refrescar_panel_local();
            if let Some(aviso) = aviso {
                self.mensaje(aviso, true);
            }
            return;
        };
        let Some(edicion) = self.ediciones.vivas.get_mut(&id) else {
            return;
        };
        if edicion.fase != Fase::Editando {
            return;
        }
        let destino = edicion.objetivo.destino();
        let antes = edicion.bajado.as_ref().map(|bajado| bajado.huella);
        let ahora = ciclo::huella(&peticion.ruta);
        let hay_cambios = match (&ahora, antes) {
            (Ok(ahora), Some(antes)) => *ahora != antes,
            // Sin poder comparar, se trata como cambiado: nunca se borra un
            // temporal que quizá tenga trabajo.
            _ => true,
        };
        if !hay_cambios {
            self.cerrar_edicion(id, Cierre::SinCambios);
            match aviso {
                Some(aviso) => self.mensaje(aviso, true),
                None => self.mensaje(format!("sin cambios en {destino}: no se sube nada"), false),
            }
            return;
        }
        edicion.fase = Fase::Preguntando;
        if let Err(motivo) = ahora {
            self.mensaje(format!("no se pudo leer el temporal: {motivo}"), true);
        } else if let Some(aviso) = aviso {
            self.mensaje(aviso, true);
        }
        let subir = self.dialogo_subir(id);
        self.mostrar_edicion(Dialogo::Edicion(subir));
    }
}
