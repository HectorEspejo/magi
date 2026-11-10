//! Vista Sesión de la Fase 15: paneles, historial, enlaces, escritura en
//! varios, ratón, portapapeles del remoto, tmux y foco.
//!
//! Todo se apoya en el emulador del núcleo de cada pestaña (sin responder) y en
//! los mensajes del protocolo v9. Las teclas y el pegado se codifican con los
//! modos de la pestaña que los recibe; el prefijo nunca se difunde.

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::enlaces::{self, DialogoEnlace};
use crate::app::escritura_varios::{EstadoEscritura, MotivoFin, Peticion};
use crate::app::historial::{Busqueda, EstadoHistorial, RUEDA_FILAS};
use crate::app::portapapeles::{self, Origen};
use crate::app::{
    paneles::PANELES_MAX, pegado, raton, AccionDialogo, App, CampoTexto, Dialogo,
    EntradaTextoAccion, Vista,
};
use crate::protocolo::{self, EstadoSesionRemota, MensajeCliente, MensajeServidor};

/// Lista de sesiones de tmux de un host, en la vista Sesiones.
#[derive(Debug, Clone, Default)]
pub struct EstadoTmux {
    pub host_id: i64,
    pub host_nombre: String,
    /// La petición está en vuelo.
    pub cargando: bool,
    /// El host no tiene tmux (o falló): el motivo.
    pub sin_tmux: Option<String>,
    pub sesiones: Vec<protocolo::InfoSesionTmux>,
    pub seleccion: usize,
    pub desplazamiento: usize,
}

/// Una petición del terminal en vuelo (rango 2^49).
#[derive(Debug, Clone)]
pub enum PeticionTerminal {
    Historial(u32),
    TmuxLista(i64),
    TmuxTerminar { host_id: i64, nombre: String },
    EscrituraVarios(Peticion),
    TunelTemporal { sesion_id: u32, url: String },
}

/// Texto de un pegado para un campo de una línea: sin controles (los saltos
/// de línea se juntan) y acotado para que un pegado enorme no infle el campo.
fn texto_para_campo(texto: &str) -> String {
    texto
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(4096)
        .collect()
}

impl App {
    // ------------------------------------------------------------ paneles

    /// Paneles visibles con su pestaña (panel, índice en `pestanas`).
    #[must_use]
    pub fn paneles_visibles(&self) -> Vec<(usize, usize)> {
        let indices = self.paneles.indices(&self.pestanas);
        (0..self.paneles.visibles())
            .filter_map(|panel| {
                indices
                    .get(panel)
                    .copied()
                    .flatten()
                    .map(|indice| (panel, indice))
            })
            .collect()
    }

    /// La pestaña del panel enfocado.
    #[must_use]
    pub fn pestana_enfocada(&self) -> Option<&crate::app::PestanaUI> {
        self.paneles
            .indice_enfocado(&self.pestanas)
            .and_then(|indice| self.pestanas.get(indice))
    }

    /// La sesión de terminal del panel enfocado, si la hay.
    #[must_use]
    pub fn sesion_enfocada(&self) -> Option<u32> {
        self.pestana_enfocada()
            .filter(|pestana| pestana.escritorio_id.is_none())
            .map(|pestana| pestana.sesion_id)
    }

    /// Copia el índice de la pestaña enfocada a `pestana_activa` (la usan las
    /// barras y las teclas de siempre).
    pub(crate) fn fijar_pestana_activa(&mut self) {
        self.pestana_activa = self.paneles.indice_enfocado(&self.pestanas);
    }

    /// Rehace los paneles a partir de las pestañas (tras abrir, cerrar o
    /// reordenar) y deja la activa en el panel enfocado.
    pub(crate) fn sincronizar_paneles(&mut self) {
        self.paneles
            .sincronizar(&self.pestanas, self.pestana_activa);
        self.fijar_pestana_activa();
    }

    /// Tamaño con el que se pide un PTY nuevo: el de la vista menos la fila de
    /// cabecera del panel (Fase 15). Con varias disposiciones el tamaño real
    /// de cada panel llega después con su `Geometria`.
    #[must_use]
    pub(crate) fn tamano_pty_inicial(&self) -> (u16, u16) {
        let (cols, filas) = self.tamano_pty();
        (cols, filas.saturating_sub(1).max(1))
    }

    /// Tamaño del PTY de un panel: el de su contenido en el último pintado o,
    /// sin pintado todavía, el de la vista entera.
    #[must_use]
    fn tamano_panel_pty(&self, panel: usize) -> (u16, u16) {
        if let Some((_, rect)) = self
            .disposicion
            .panes
            .iter()
            .find(|(otro, _)| *otro == panel)
        {
            (rect.width.max(2), rect.height.max(1))
        } else {
            let (cols, filas) = self.tamano_pty();
            (cols, alto_de_panel(filas))
        }
    }

    /// Pide `Adjuntar` a cada panel visible y suelta los que ya no se ven (o
    /// los de una vista que se deja). Es la única forma de que el servidor
    /// mande los datos de cada panel.
    pub(crate) fn adjuntar_paneles(&mut self) {
        let mut adjuntar = Vec::new();
        for (panel, indice) in self.paneles_visibles() {
            let Some(pestana) = self.pestanas.get(indice) else {
                continue;
            };
            if pestana.escritorio_id.is_some() {
                continue;
            }
            let (cols, filas) = self.tamano_panel_pty(panel);
            adjuntar.push((pestana.sesion_id, cols, filas));
        }
        let ids: HashSet<u32> = adjuntar.iter().map(|(id, _, _)| *id).collect();
        let anteriores: Vec<u32> = self.paneles_adjuntados.difference(&ids).copied().collect();
        for sesion_id in anteriores {
            self.servidor
                .enviar(MensajeCliente::Desadjuntar { sesion_id });
        }
        let colores = self.colores_tema();
        for (sesion_id, cols, filas) in adjuntar {
            self.servidor.enviar(MensajeCliente::Adjuntar {
                sesion_id,
                cols,
                filas,
                colores,
            });
            self.paneles.tamanos.insert(sesion_id, (cols, filas));
        }
        self.paneles_adjuntados = ids;
        self.enviar_foco();
    }

    /// Suelta todos los paneles adjuntos (al salir de la vista Sesión).
    pub(crate) fn desadjuntar_paneles(&mut self) {
        for sesion_id in std::mem::take(&mut self.paneles_adjuntados) {
            self.servidor
                .enviar(MensajeCliente::Desadjuntar { sesion_id });
        }
        self.paneles.tamanos.clear();
        self.foco_enviado = None;
    }

    /// Aplica la geometría del último pintado: un `Redimensionar` por panel
    /// cuyo contenido cambió de tamaño. Es el único sitio que lo envía.
    pub(crate) fn aplicar_geometria_paneles(&mut self, panes: &[(usize, ratatui::layout::Rect)]) {
        let mut cambios = Vec::new();
        // Sin paneles pintados (aviso de tamaño) se sigue mandando el tamaño de
        // la pestaña activa saneado como antes de la Fase 15.
        if panes.is_empty() {
            if let Some(sesion_id) = self.pestana_activa_id() {
                let tamano = self.tamano_pty();
                if self.paneles.tamanos.get(&sesion_id) != Some(&tamano) {
                    self.servidor.enviar(MensajeCliente::Redimensionar {
                        sesion_id,
                        cols: tamano.0,
                        filas: tamano.1,
                    });
                    self.paneles.tamanos.insert(sesion_id, tamano);
                }
            }
            return;
        }
        for (panel, rect) in panes.iter().copied() {
            let Some(indice) = self
                .paneles
                .indices(&self.pestanas)
                .get(panel)
                .copied()
                .flatten()
            else {
                continue;
            };
            let Some(pestana) = self.pestanas.get(indice) else {
                continue;
            };
            if pestana.escritorio_id.is_some() {
                continue;
            }
            let tamano = (rect.width.max(2), rect.height.max(1));
            if self.paneles.tamanos.get(&pestana.sesion_id) != Some(&tamano) {
                cambios.push((pestana.sesion_id, tamano));
            }
        }
        for (sesion_id, (cols, filas)) in cambios {
            self.servidor.enviar(MensajeCliente::Redimensionar {
                sesion_id,
                cols,
                filas,
            });
            self.paneles.tamanos.insert(sesion_id, (cols, filas));
        }
    }

    /// Los colores del tema de la ventana, para responder a OSC 10, 11 y 12 en
    /// el servidor.
    #[must_use]
    fn colores_tema(&self) -> Option<protocolo::ColoresTema> {
        fn rgb(color: ratatui::style::Color) -> Option<(u8, u8, u8)> {
            match color {
                ratatui::style::Color::Rgb(rojo, verde, azul) => Some((rojo, verde, azul)),
                _ => None,
            }
        }
        let paleta = &self.tema.paleta;
        Some(protocolo::ColoresTema {
            primer_plano: rgb(paleta.texto)?,
            fondo: rgb(paleta.fondo)?,
            cursor: rgb(paleta.acento).unwrap_or((0xf0, 0xf0, 0xf0)),
        })
    }

    /// El terminal exterior gana o pierde el foco (modo 1004 del remoto).
    fn enviar_foco(&mut self) {
        let nuevo = (self.vista == Vista::Sesion && self.foco_exterior)
            .then(|| self.sesion_enfocada())
            .flatten()
            .map(|sesion_id| (sesion_id, true));
        if nuevo == self.foco_enviado {
            return;
        }
        if let Some((anterior, activa)) = self.foco_enviado {
            if activa {
                self.servidor.enviar(MensajeCliente::Foco {
                    sesion_id: anterior,
                    enfocada: false,
                });
            }
        }
        if let Some((sesion_id, activa)) = nuevo {
            self.servidor.enviar(MensajeCliente::Foco {
                sesion_id,
                enfocada: activa,
            });
        }
        self.foco_enviado = nuevo;
    }

    /// Sincroniza el foco y el ratón del terminal exterior con la vista, el
    /// panel enfocado y los modos del remoto.
    pub(crate) fn sincronizar_terminal_exterior(&mut self) {
        self.enviar_foco();
        let capturado = self.captura_raton();
        if capturado != self.raton_capturado {
            self.raton_capturado = capturado;
            if self.efectos_externos {
                crate::ui::fijar_raton(capturado);
            }
        }
    }

    /// ¿Pide el ratón el remoto del panel enfocado, o lo enciende la config?
    #[must_use]
    fn captura_raton(&self) -> bool {
        let Some(sesion_id) = self.sesion_enfocada() else {
            return false;
        };
        let modo = self.pantallas.modo_raton(sesion_id).unwrap_or_default();
        raton::capturar(self.config.terminal.raton, modo)
    }

    // ---------------------------------------------------------- escritura

    /// Petición de escritura en varios a partir de los paneles visibles.
    #[must_use]
    fn peticion_escritura_varios(&self) -> Option<Peticion> {
        let mut sesiones = Vec::new();
        let mut hosts = Vec::new();
        let mut paneles = Vec::new();
        for (panel, indice) in self.paneles_visibles() {
            let Some(pestana) = self.pestanas.get(indice) else {
                continue;
            };
            if pestana.escritorio_id.is_some() || pestana.estado != EstadoSesionRemota::Abierta {
                continue;
            }
            if sesiones.contains(&pestana.sesion_id) {
                continue;
            }
            sesiones.push(pestana.sesion_id);
            paneles.push(panel);
            if !hosts.iter().any(|(id, _)| *id == pestana.host_id) {
                hosts.push((pestana.host_id, pestana.host_nombre.clone()));
            }
        }
        if sesiones.len() < 2 {
            return None;
        }
        hosts.sort_by_key(|(id, _)| *id);
        Some(Peticion {
            sesiones,
            hosts,
            paneles,
        })
    }

    /// ¿Hace falta deliberar para la escritura en varios? (más de un host o
    /// alguna comprobación activa, §7.7).
    fn escritura_delibera(&self, peticion: &Peticion) -> bool {
        if peticion.hosts.len() > 1 {
            return true;
        }
        let verificaciones = self.almacen.verificaciones_por_host().unwrap_or_default();
        peticion.hosts.iter().any(|(host_id, _)| {
            verificaciones
                .get(host_id)
                .is_some_and(|verificacion| verificacion.alguna_activa())
        })
    }

    /// `prefijo E`: activa o para la escritura en varios.
    pub(crate) fn alternar_escritura_varios(&mut self) {
        if self.escritura_varios.activa() {
            self.terminar_escritura_varios(MotivoFin::Apagada);
            return;
        }
        let Some(peticion) = self.peticion_escritura_varios() else {
            self.mensaje("hace falta más de un panel con la pestaña abierta", true);
            return;
        };
        if self.escritura_delibera(&peticion) {
            self.abrir_deliberacion(
                crate::app::dialogo_magi::PlanDeliberado::EscrituraVarios {
                    sesiones: peticion.sesiones.clone(),
                    hosts: peticion.hosts.clone(),
                },
                {
                    let verificaciones = self.almacen.verificaciones_por_host().unwrap_or_default();
                    magi_nucleo::snippets::destinos::motivos_deliberacion(
                        false,
                        &peticion.hosts,
                        &verificaciones,
                    )
                },
            );
            return;
        }
        let nombres: Vec<String> = peticion
            .hosts
            .iter()
            .map(|(_, nombre)| nombre.clone())
            .collect();
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "ESCRITURA EN VARIOS".to_string(),
            lineas: vec![
                format!(
                    "Se escribirá lo que teclees en {} paneles de {}.",
                    peticion.sesiones.len(),
                    nombres.join(" · ")
                ),
                format!("{}{} E para parar.", self.config.prefijo_escape, ""),
            ],
            peligro: false,
            accion: AccionDialogo::IniciarEscrituraVarios(peticion.sesiones),
        });
    }

    /// Empieza la escritura en varios a partir de los ids de sesión (lo usa
    /// la deliberación al aprobar).
    pub(crate) fn iniciar_escritura_varios_desde_sesiones(
        &mut self,
        sesiones: Vec<u32>,
        deliberacion_id: Option<i64>,
    ) {
        let indices = self.paneles.indices(&self.pestanas);
        let mut peticion = Peticion {
            sesiones: Vec::new(),
            hosts: Vec::new(),
            paneles: Vec::new(),
        };
        for sesion_id in sesiones {
            let Some(indice) = self
                .pestanas
                .iter()
                .position(|pestana| pestana.sesion_id == sesion_id)
            else {
                continue;
            };
            let pestana = &self.pestanas[indice];
            peticion.sesiones.push(sesion_id);
            if !peticion.hosts.iter().any(|(id, _)| *id == pestana.host_id) {
                peticion
                    .hosts
                    .push((pestana.host_id, pestana.host_nombre.clone()));
            }
            if let Some(panel) = indices.iter().position(|otro| *otro == Some(indice)) {
                peticion.paneles.push(panel);
            }
        }
        peticion.hosts.sort_by_key(|(id, _)| *id);
        self.iniciar_escritura_varios(peticion, deliberacion_id);
    }

    /// Manda `IniciarEscrituraVarios` con las pestañas concedidas.
    fn iniciar_escritura_varios(&mut self, peticion: Peticion, deliberacion_id: Option<i64>) {
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        self.servidor
            .enviar(MensajeCliente::IniciarEscrituraVarios {
                peticion_id,
                sesiones: peticion.sesiones.clone(),
                deliberacion_id,
            });
        self.peticiones_terminal.insert(
            peticion_id,
            PeticionTerminal::EscrituraVarios(peticion.clone()),
        );
        self.escritura_varios.estado = EstadoEscritura::Pidiendo {
            peticion_id,
            peticion,
        };
        self.mensaje("pidiendo la escritura en varios…", false);
    }

    /// Termina la escritura en varios con su motivo y anota el REGISTRO.
    pub(crate) fn terminar_escritura_varios(&mut self, motivo: MotivoFin) {
        let Some((id, motivo, resumen)) = self.escritura_varios.terminar(motivo) else {
            return;
        };
        self.servidor
            .enviar(MensajeCliente::TerminarEscrituraVarios {
                id,
                motivo: motivo.texto().to_string(),
            });
        self.anotar(
            crate::registro::ESCRITURA_VARIOS_TERMINADA,
            None,
            None,
            &match resumen {
                Some((segundos, envios)) => {
                    format!("{id} · {} · {segundos} s · {envios} envíos", motivo.texto())
                }
                None => format!("{id} · {}", motivo.texto()),
            },
            crate::modelo::ResultadoRegistro::Ok,
        );
        self.mensaje(
            format!("escritura en varios terminada: {}", motivo.texto()),
            false,
        );
    }

    fn escritura_activa_del_motivo(&mut self, motivo: MotivoFin) -> bool {
        if !self.escritura_varios.activa() {
            return false;
        }
        self.terminar_escritura_varios(motivo);
        true
    }

    /// Diffunde una tecla a los paneles concedidos con los modos de cada uno.
    fn escribir_en_varios_tecla(&mut self, tecla: KeyEvent) {
        let sesiones = self.escritura_varios.sesiones();
        let mut pestanas = Vec::new();
        for sesion_id in sesiones {
            let modo = self.pantallas.modo_terminal(sesion_id).unwrap_or_default();
            if let Some(bytes) = crate::teclas::bytes_de_tecla_con_modo(tecla, modo) {
                pestanas.push(protocolo::BytesPestana { sesion_id, bytes });
            }
        }
        self.enviar_varios(pestanas);
    }

    fn enviar_varios(&mut self, pestanas: Vec<protocolo::BytesPestana>) {
        let Some(id) = self.escritura_varios.id() else {
            return;
        };
        if pestanas.is_empty() {
            return;
        }
        self.servidor
            .enviar(MensajeCliente::EscribirVarios { id, pestanas });
        self.escritura_varios.enviado();
    }

    // ------------------------------------------------------------ portapapeles

    /// Copia texto al portapapeles del sistema o por OSC 52.
    pub(crate) fn copiar_texto(&mut self, texto: &str, que: &str) {
        if texto.is_empty() {
            self.mensaje("no hay nada que copiar", true);
            return;
        }
        if !EstadoHistorial::cabe_en_copia(texto) {
            self.mensaje(
                "demasiado grande para copiar (más de 1 MiB): w para guardarlo",
                true,
            );
            return;
        }
        if !self.efectos_externos {
            self.mensaje(format!("{que} copiado"), false);
            return;
        }
        match portapapeles::copiar(texto) {
            Ok(Origen::Sistema(herramienta)) => {
                self.mensaje(format!("{que} copiado con {herramienta}"), false)
            }
            Ok(Origen::Osc52) => self.mensaje(format!("{que} copiado por OSC 52"), false),
            Err(motivo) => self.mensaje(format!("no se pudo copiar: {motivo}"), true),
        }
    }

    /// Abre una URL con `[terminal] abrir_enlaces`, sin shell.
    fn abrir_url(&mut self, url: &str) {
        if !magi_nucleo::terminal::enlaces::es_web(url) {
            self.mensaje("solo se abren enlaces http y https", true);
            return;
        }
        let programa = self.config.terminal.abrir_enlaces.trim().to_string();
        if programa.is_empty() {
            self.mensaje("no hay [terminal] abrir_enlaces configurado", true);
            return;
        }
        if !self.efectos_externos {
            self.mensaje(format!("abriría {url}"), false);
            return;
        }
        match std::process::Command::new(&programa)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => self.mensaje(format!("abriendo {url}"), false),
            Err(error) => self.mensaje(format!("no se pudo abrir {programa}: {error}"), true),
        }
    }

    // ------------------------------------------------------------ tmux

    /// `t` en Sesiones: pide la lista de tmux del host de la pestaña elegida.
    pub(crate) fn pedir_lista_tmux(&mut self) {
        let Some(pestana) = self.pestana_seleccionada() else {
            self.mensaje("no hay ninguna pestaña elegida", true);
            return;
        };
        let (host_id, host_nombre) = (pestana.host_id, pestana.host_nombre.clone());
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        self.servidor.enviar(MensajeCliente::ListarTmux {
            peticion_id,
            host_id,
        });
        self.peticiones_terminal
            .insert(peticion_id, PeticionTerminal::TmuxLista(host_id));
        self.tmux = Some(EstadoTmux {
            host_id,
            host_nombre,
            cargando: true,
            ..EstadoTmux::default()
        });
        self.sucio = true;
    }

    /// `T` en la lista de tmux: pregunta antes de terminarla.
    pub(crate) fn preguntar_terminar_tmux(&mut self) {
        let Some(estado) = &self.tmux else {
            return;
        };
        let Some(sesion) = estado.sesiones.get(estado.seleccion).cloned() else {
            return;
        };
        let host_id = estado.host_id;
        let host_nombre = estado.host_nombre.clone();
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "TERMINAR SESIÓN DE TMUX".to_string(),
            lineas: vec![
                format!(
                    "Se terminará la sesión «{}» de tmux en {host_nombre}.",
                    sesion.nombre
                ),
                "Lo que corra dentro se perderá.".to_string(),
            ],
            peligro: true,
            accion: AccionDialogo::TerminarTmux {
                host_id,
                nombre: sesion.nombre,
            },
        });
    }

    pub(crate) fn terminar_tmux(&mut self, host_id: i64, nombre: String) {
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        self.servidor.enviar(MensajeCliente::TerminarTmux {
            peticion_id,
            host_id,
            nombre: nombre.clone(),
        });
        self.peticiones_terminal.insert(
            peticion_id,
            PeticionTerminal::TmuxTerminar { host_id, nombre },
        );
    }

    // ------------------------------------------------------------ rejilla

    /// Paleta «abrir en rejilla…»: abre de 2 a 4 hosts (por nombre, separados
    /// por comas) y, cuando aparecen sus pestañas, las reparte en la
    /// disposición que toque (2 lado a lado, 3 «1 grande + 2», 4 rejilla).
    /// Paleta «abrir en rejilla…»: abre de 2 a 4 hosts ya elegidos en el
    /// diálogo y, cuando aparecen sus pestañas, las reparte en la disposición
    /// que toque (2 lado a lado, 3 «1 grande + 2», 4 rejilla).
    pub(crate) fn abrir_en_rejilla(&mut self, host_ids: Vec<i64>) {
        let mut validos = Vec::new();
        for host_id in host_ids {
            if validos.len() >= crate::app::rejilla::MAXIMO {
                break;
            }
            if self
                .hosts
                .iter()
                .any(|host| host.id == host_id && host.tipo.es_ssh())
                && !validos.contains(&host_id)
            {
                validos.push(host_id);
            }
        }
        if validos.len() < 2 {
            self.mensaje("hacen falta de 2 a 4 hosts para abrir en rejilla", true);
            return;
        }
        self.rejilla_sesiones.clear();
        self.rejilla_pendiente = Some(validos.clone());
        for host_id in validos {
            self.abrir_sesion_en_segundo_plano(host_id);
        }
        self.mensaje("abriendo los hosts en rejilla…", false);
    }

    /// Una pestaña nueva de una «abrir en rejilla»: cuando están todas, se
    /// reparten.
    pub(crate) fn rejilla_sesion_nueva(&mut self, sesion_id: u32, host_id: i64) {
        let Some(pendientes) = &mut self.rejilla_pendiente else {
            return;
        };
        let Some(posicion) = pendientes.iter().position(|id| *id == host_id) else {
            return;
        };
        pendientes.remove(posicion);
        if !self.rejilla_sesiones.contains(&sesion_id) {
            self.rejilla_sesiones.push(sesion_id);
        }
        if pendientes.is_empty() {
            self.rejilla_lista();
        }
    }

    /// Reparte las sesiones de la rejilla en la disposición que les toca.
    fn rejilla_lista(&mut self) {
        let sesiones = std::mem::take(&mut self.rejilla_sesiones);
        self.rejilla_pendiente = None;
        let cuantas = sesiones.len();
        if cuantas < 2 {
            self.mensaje("la rejilla necesita al menos dos sesiones", true);
            return;
        }
        self.paneles.disposicion = match cuantas {
            2 => crate::app::paneles::DisposicionPaneles::LadoALado,
            3 => crate::app::paneles::DisposicionPaneles::Grande2,
            _ => crate::app::paneles::DisposicionPaneles::Rejilla,
        };
        self.vista_previa = Some(self.vista);
        self.vista = Vista::Sesion;
        for (panel, sesion_id) in sesiones.iter().enumerate() {
            self.paneles.enfocado = panel.min(PANELES_MAX - 1);
            self.paneles
                .poner(crate::app::paneles::ClavePestana::Terminal(*sesion_id));
        }
        self.paneles.enfocado = 0;
        self.fijar_pestana_activa();
        self.tras_cambiar_paneles();
        self.mensaje(
            format!(
                "{} sesiones en {}",
                cuantas,
                self.paneles.disposicion.nombre()
            ),
            false,
        );
    }

    // ------------------------------------------------------------ mensajes

    /// Mensajes del terminal (Fase 15). Devuelve si era uno de los suyos.
    pub(crate) fn evento_terminal(&mut self, mensaje: &MensajeServidor) -> bool {
        match mensaje {
            MensajeServidor::HistorialLote {
                peticion_id,
                sesion_id,
                lote,
                ultimo,
                total,
                lineas,
            } => {
                self.historial_lote(
                    *peticion_id,
                    *sesion_id,
                    *lote,
                    *ultimo,
                    *total,
                    lineas.clone(),
                );
                true
            }
            MensajeServidor::EscrituraVariosIniciada { peticion_id, id } => {
                self.escritura_iniciada(*peticion_id, *id);
                true
            }
            MensajeServidor::EscrituraVariosTerminada { id, motivo } => {
                if self.escritura_varios.id() == Some(*id) {
                    self.escritura_varios.cancelar();
                    self.mensaje(
                        format!("la escritura en varios se ha terminado: {motivo}"),
                        false,
                    );
                }
                true
            }
            MensajeServidor::TunelTemporal {
                peticion_id,
                puerto_local,
                ..
            } => {
                self.tunel_temporal(*peticion_id, *puerto_local);
                true
            }
            MensajeServidor::TunelesTemporales { lista } => {
                self.tuneles_temporales = lista.clone();
                true
            }
            MensajeServidor::SesionesTmux {
                peticion_id,
                host_id,
                sesiones,
                sin_tmux,
            } => {
                self.sesiones_tmux(*peticion_id, *host_id, sesiones.clone(), sin_tmux.clone());
                true
            }
            MensajeServidor::TmuxTerminada {
                peticion_id,
                resultado,
            } => {
                self.tmux_terminada(*peticion_id, *resultado);
                true
            }
            MensajeServidor::Portapapeles { sesion_id, texto } => {
                self.portapapeles_remoto(*sesion_id, texto);
                true
            }
            MensajeServidor::PortapapelesPedido { sesion_id, bytes } => {
                self.portapapeles_pedido(*sesion_id, *bytes);
                true
            }
            _ => false,
        }
    }

    fn historial_lote(
        &mut self,
        peticion_id: u64,
        sesion_id: u32,
        lote: u32,
        ultimo: bool,
        total: u32,
        lineas: Vec<magi_nucleo::terminal::historial::LineaHistorial>,
    ) {
        let Some(estado) = &mut self.historial else {
            return;
        };
        if estado.peticion_id != peticion_id || estado.sesion_id != sesion_id {
            return;
        }
        if estado.apilar_lote(lote, ultimo, total, lineas) {
            self.historial_terminado();
        }
    }

    fn historial_terminado(&mut self) {
        self.peticiones_terminal
            .retain(|_, peticion| !matches!(peticion, PeticionTerminal::Historial(_)));
        if let Some(estado) = &mut self.historial {
            estado.al_final();
            let inicial = estado.desplazamiento_inicial;
            if inicial != 0 {
                estado.mover(inicial);
            }
        }
        self.sucio = true;
    }

    fn escritura_iniciada(&mut self, peticion_id: u64, id: u32) {
        let Some(PeticionTerminal::EscrituraVarios(peticion)) =
            self.peticiones_terminal.remove(&peticion_id)
        else {
            return;
        };
        self.escritura_varios.activar(id, peticion.clone());
        let hosts: Vec<String> = peticion
            .hosts
            .iter()
            .map(|(_, nombre)| nombre.clone())
            .collect();
        let paneles = peticion.paneles.len();
        self.anotar(
            crate::registro::ESCRITURA_VARIOS_ACTIVADA,
            None,
            None,
            &format!(
                "{paneles} paneles · {} · {}",
                hosts.join(" · "),
                match &self.escritura_varios.estado {
                    EstadoEscritura::Activa { .. } => "concedida",
                    _ => "concedida",
                }
            ),
            crate::modelo::ResultadoRegistro::Ok,
        );
        self.mensaje(
            format!("escribiendo en {paneles} paneles · prefijo E para parar"),
            false,
        );
        self.sucio = true;
    }

    fn tunel_temporal(&mut self, peticion_id: u64, puerto_local: u16) {
        let Some(PeticionTerminal::TunelTemporal { url, .. }) =
            self.peticiones_terminal.remove(&peticion_id)
        else {
            return;
        };
        match magi_nucleo::terminal::enlaces::url_por_tunel(&url, puerto_local) {
            Some(reescrita) => {
                self.mensaje(
                    format!("túnel temporal de {} en 127.0.0.1:{puerto_local}", url),
                    false,
                );
                self.abrir_url(&reescrita);
            }
            None => self.mensaje("no se pudo reescribir la URL del túnel", true),
        }
    }

    fn sesiones_tmux(
        &mut self,
        peticion_id: u64,
        host_id: i64,
        sesiones: Vec<protocolo::InfoSesionTmux>,
        sin_tmux: Option<String>,
    ) {
        if self.peticiones_terminal.remove(&peticion_id).is_none() {
            return;
        }
        let Some(estado) = &mut self.tmux else {
            return;
        };
        if estado.host_id != host_id {
            return;
        }
        estado.cargando = false;
        estado.sin_tmux = sin_tmux.clone();
        estado.seleccion = estado.seleccion.min(sesiones.len().saturating_sub(1));
        estado.sesiones = sesiones;
        self.sucio = true;
    }

    fn tmux_terminada(&mut self, peticion_id: u64, resultado: protocolo::ResultadoTmux) {
        let Some(PeticionTerminal::TmuxTerminar { host_id, nombre }) =
            self.peticiones_terminal.remove(&peticion_id)
        else {
            return;
        };
        match resultado {
            protocolo::ResultadoTmux::Ok => {
                let host = self
                    .hosts
                    .iter()
                    .find(|host| host.id == host_id)
                    .map(|host| host.nombre.clone())
                    .unwrap_or_default();
                self.anotar(
                    crate::registro::TMUX_TERMINADA,
                    Some(host_id),
                    None,
                    &format!("{host} · {nombre}"),
                    crate::modelo::ResultadoRegistro::Ok,
                );
                self.mensaje(format!("sesión de tmux «{nombre}» terminada"), false);
                if let Some(estado) = &mut self.tmux {
                    estado.sesiones.retain(|sesion| sesion.nombre != nombre);
                    estado.seleccion = estado
                        .seleccion
                        .min(estado.sesiones.len().saturating_sub(1));
                }
            }
            protocolo::ResultadoTmux::NoExiste => {
                self.mensaje(format!("la sesión «{nombre}» ya no existía"), true);
            }
        }
        self.sucio = true;
    }

    /// Abre una pestaña enganchada a una sesión de tmux (o una nueva, con
    /// `tmux` a `None`). El servidor solo escribe los comandos iniciales si la
    /// sesión es nueva (§7.10).
    /// Abre una sesión sin cambiar de vista (para la rejilla y las pestañas en
    /// segundo plano).
    fn abrir_sesion_en_segundo_plano(&mut self, host_id: i64) {
        let comandos = self.comandos_al_conectar(host_id);
        self.abrir_sesion_con_opciones(host_id, comandos, None, false);
    }

    pub(crate) fn abrir_sesion_tmux(&mut self, host_id: i64, tmux: Option<String>) {
        let comandos = self.comandos_al_conectar(host_id);
        let (cols, filas) = self.tamano_pty();
        self.servidor.enviar(MensajeCliente::AbrirSesion {
            host_id,
            cols,
            filas,
            comandos_iniciales: comandos,
            orden: None,
            tmux,
        });
        self.aperturas_pendientes.insert(host_id);
        self.tmux = None;
        self.sucio = true;
    }

    fn portapapeles_remoto(&mut self, sesion_id: u32, texto: &str) {
        let host = self
            .pestanas
            .iter()
            .find(|pestana| pestana.sesion_id == sesion_id)
            .map(|pestana| pestana.host_nombre.clone())
            .unwrap_or_else(|| "el host".to_string());
        let bytes = texto.len() as u64;
        self.portapapeles_remoto.copias += 1;
        self.copiar_texto(texto, &host);
        self.mensaje(
            format!(
                "{host} copió {} al portapapeles",
                portapapeles::tamano_legible(bytes)
            ),
            false,
        );
    }

    fn portapapeles_pedido(&mut self, sesion_id: u32, bytes: u64) {
        let host = self
            .pestanas
            .iter()
            .find(|pestana| pestana.sesion_id == sesion_id)
            .map(|pestana| pestana.host_nombre.clone())
            .unwrap_or_else(|| "el host".to_string());
        self.portapapeles_remoto
            .pedir(sesion_id, host.clone(), bytes);
        self.mensaje(
            format!(
                "{host} quiere copiar {} al portapapeles · {} P permitir en este host",
                portapapeles::tamano_legible(bytes),
                self.config.prefijo_escape
            ),
            false,
        );
        self.sucio = true;
    }

    /// `prefijo P`: permite el portapapeles del remoto a este host y entrega
    /// la petición pendiente si sigue viva.
    fn permitir_portapapeles(&mut self) {
        let Some(pendiente) = self.portapapeles_remoto.tomar() else {
            self.mensaje("no hay ninguna petición de portapapeles pendiente", true);
            return;
        };
        let host_id = self
            .pestanas
            .iter()
            .find(|pestana| pestana.sesion_id == pendiente.sesion_id)
            .map(|pestana| pestana.host_id);
        match host_id {
            Some(host_id) => match crate::almacen::hosts::fijar_portapapeles_remoto(
                self.almacen.conexion(),
                host_id,
                true,
            ) {
                Ok(()) => {
                    self.servidor.enviar(MensajeCliente::PortapapelesPermitido {
                        sesion_id: pendiente.sesion_id,
                    });
                    let _ = self.recargar_inventario();
                    self.mensaje(
                        format!(
                            "portapapeles permitido a {} en este equipo",
                            pendiente.host_nombre
                        ),
                        false,
                    );
                }
                Err(error) => self.mensaje(error.to_string(), true),
            },
            None => self.mensaje("la pestaña de la petición ya no existe", true),
        }
        self.sucio = true;
    }

    /// Resuelve un `Error` de una petición del terminal.
    pub(crate) fn resolver_error_terminal(&mut self, peticion_id: u64, mensaje: &str) -> bool {
        let Some(peticion) = self.peticiones_terminal.remove(&peticion_id) else {
            return false;
        };
        match peticion {
            PeticionTerminal::Historial(_) => {
                self.historial = None;
                self.mensaje(format!("no se pudo capturar el historial: {mensaje}"), true);
            }
            PeticionTerminal::TmuxLista(host_id) => {
                if let Some(estado) = &mut self.tmux {
                    estado.cargando = false;
                    estado.sin_tmux = Some(mensaje.to_string());
                    let _ = host_id;
                }
            }
            PeticionTerminal::TmuxTerminar { nombre, .. } => {
                self.mensaje(format!("no se pudo terminar «{nombre}»: {mensaje}"), true);
            }
            PeticionTerminal::EscrituraVarios(_) => {
                self.escritura_varios.cancelar();
                self.mensaje(
                    format!("la escritura en varios no se concedió: {mensaje}"),
                    true,
                );
            }
            PeticionTerminal::TunelTemporal { .. } => {
                self.mensaje(
                    format!("no se pudo abrir el túnel temporal: {mensaje}"),
                    true,
                );
            }
        }
        // La petición no está en vuelo: si es de la escritura, se apaga.
        if peticion_id >= crate::app::RANGO_TERMINAL {
            self.sucio = true;
        }
        true
    }

    // ------------------------------------------------------------ teclas

    pub(crate) fn tecla_sesion(&mut self, tecla: KeyEvent) {
        if self.modo_prefijo {
            self.modo_prefijo = false;
            if crate::teclas::es_prefijo(&tecla, self.prefijo) {
                // prefijo + prefijo: el prefijo literal hacia el remoto.
                self.enviar_tecla_panel(tecla);
                return;
            }
            self.tecla_prefijo_sesion(tecla);
            return;
        }
        if crate::teclas::es_prefijo(&tecla, self.prefijo) {
            self.modo_prefijo = true;
            return;
        }
        if tecla.kind == KeyEventKind::Release {
            return;
        }
        // El modo historial y el de enlaces se quedan con sus teclas.
        if self.historial.is_some() {
            self.tecla_historial(tecla);
            return;
        }
        if self.enlaces.activo {
            self.tecla_enlaces(tecla);
            return;
        }
        // En una pestaña de escritorio no hay terminal.
        if self
            .pestana_enfocada()
            .is_some_and(|pestana| pestana.escritorio_id.is_some())
        {
            match tecla.code {
                KeyCode::Enter => self.abrir_visor_activo(),
                KeyCode::Char('y') => self.copiar_enlace_activo(),
                _ => {}
            }
            return;
        }
        // Shift+RePág: modo historial solo con la pantalla principal.
        if tecla.code == KeyCode::PageUp && tecla.modifiers.contains(KeyModifiers::SHIFT) {
            // Con la pantalla alternativa, el programa tiene su propio
            // historial: la tecla va al remoto.
            if self
                .sesion_enfocada()
                .is_some_and(|sesion| self.pantalla_alternativa(sesion))
            {
                self.enviar_tecla_panel(tecla);
            } else {
                self.abrir_historial(false);
            }
            return;
        }
        if self.escritura_varios.activa() {
            self.escribir_en_varios_tecla(tecla);
            return;
        }
        self.enviar_tecla_panel(tecla);
    }

    fn enviar_tecla_panel(&mut self, tecla: KeyEvent) {
        // Sin panels pintados (por debajo del mínimo de Sesión) las teclas
        // siguen yendo a la pestaña activa, como antes de la Fase 15.
        let Some(sesion_id) = self.sesion_enfocada().or_else(|| self.pestana_activa_id()) else {
            return;
        };
        let modo = self.pantallas.modo_terminal(sesion_id).unwrap_or_default();
        if let Some(bytes) = crate::teclas::bytes_de_tecla_con_modo(tecla, modo) {
            self.servidor
                .enviar(MensajeCliente::Teclas { sesion_id, bytes });
        }
    }

    fn tecla_prefijo_sesion(&mut self, tecla: KeyEvent) {
        let prefijo = self.config.prefijo_escape.clone();
        match tecla.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.salir_de_la_vista_sesion();
                self.mensaje("sesiones en el servidor", false);
            }
            KeyCode::Char('n') => self.navegar_pestañas(1),
            KeyCode::Char('p') => self.navegar_pestañas(-1),
            KeyCode::Char('?') => self.ayuda = true,
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                self.ir_a_pestaña(c.to_digit(10).unwrap() as usize);
            }
            KeyCode::Char('f') => {
                if let Some(pestana) = self.pestana_enfocada() {
                    if pestana.escritorio_id.is_some() {
                        self.mensaje("un escritorio no tiene vista Archivos", true);
                    } else {
                        let host_id = pestana.host_id;
                        self.salir_de_sesion_si_hace_falta();
                        self.abrir_archivos(Some(host_id));
                    }
                }
            }
            KeyCode::Char('l') => {
                self.desadjuntar_paneles();
                self.vista_previa = Some(self.vista);
                self.vista = Vista::Sesiones;
            }
            KeyCode::Char('c') => self.abrir_paleta_conectar(),
            KeyCode::Char('x') => self.preguntar_cerrar_panel(),
            KeyCode::Char('r') => {
                let escritorio = self
                    .pestana_enfocada()
                    .and_then(|pestana| pestana.escritorio_id);
                match escritorio {
                    Some(escritorio_id) => self.reconectar_escritorio(escritorio_id),
                    None => {
                        if let Some(sesion_id) = self.sesion_enfocada() {
                            self.reconectar_sesion(sesion_id);
                        }
                    }
                }
            }
            KeyCode::Char('w') => {
                if let Err(error) = crate::cliente::ventana::abrir(&self.config) {
                    self.mensaje(error, true);
                }
            }
            KeyCode::Char(' ') => self.cambiar_disposicion_paneles(),
            KeyCode::Char('z') => {
                self.escritura_activa_del_motivo(MotivoFin::Disposicion);
                self.paneles.alternar_ampliado();
                self.tras_cambiar_paneles();
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                if self.paneles.mover_foco(tecla.code) {
                    self.fijar_pestana_activa();
                    self.tras_cambiar_paneles();
                    if let Some(sesion_id) = self.sesion_enfocada() {
                        self.limpiar_actividad(sesion_id);
                    }
                }
            }
            KeyCode::Char('[') => {
                self.abrir_historial(false);
            }
            KeyCode::Char('/') => {
                self.abrir_historial(true);
            }
            KeyCode::Char('o') => self.abrir_enlaces(),
            KeyCode::Char('E') => self.alternar_escritura_varios(),
            KeyCode::Char('P') => self.permitir_portapapeles(),
            KeyCode::Char(',') => self.renombrar_pestana_enfocada(),
            KeyCode::Char('<') => self.mover_pestana_enfocada(-1),
            KeyCode::Char('>') => self.mover_pestana_enfocada(1),
            _ => {
                let _ = prefijo;
            }
        }
    }

    /// Tras cambiar de panel, de disposición o de pestaña: adjunta los paneles
    /// nuevos, suelta los viejos y sincroniza foco y ratón.
    pub(crate) fn tras_cambiar_paneles(&mut self) {
        self.sincronizar_paneles();
        self.adjuntar_paneles();
        self.sincronizar_terminal_exterior();
        self.sucio = true;
    }

    pub(crate) fn cambiar_disposicion_paneles(&mut self) {
        self.escritura_activa_del_motivo(MotivoFin::Disposicion);
        self.paneles
            .cambiar_disposicion(&self.pestanas, self.pestana_activa);
        self.tras_cambiar_paneles();
        self.mensaje(
            format!("disposición {}", self.paneles.disposicion.nombre()),
            false,
        );
    }

    fn preguntar_cerrar_panel(&mut self) {
        let Some(pestana) = self.pestana_enfocada() else {
            return;
        };
        if let Some(escritorio_id) = pestana.escritorio_id {
            self.preguntar_cerrar_escritorio(escritorio_id);
            return;
        }
        let sesion_id = pestana.sesion_id;
        match pestana.tmux.clone() {
            Some(nombre) => {
                self.dialogo = Some(Dialogo::Elegir {
                    titulo: "CERRAR PESTAÑA CON TMUX".to_string(),
                    lineas: vec![
                        format!(
                            "La sesión «{nombre}» sigue viva en {}.",
                            pestana.host_nombre
                        ),
                        "Puedes dejarla para volver desde Sesiones o terminarla.".to_string(),
                    ],
                    peligro: false,
                    opciones: vec![
                        (
                            'n',
                            "dejarla viva".to_string(),
                            AccionDialogo::CerrarPestana(sesion_id),
                        ),
                        (
                            'T',
                            "terminarla también".to_string(),
                            AccionDialogo::CerrarPestanaTerminandoTmux {
                                sesion_id,
                                host_id: pestana.host_id,
                                nombre,
                            },
                        ),
                    ],
                });
            }
            None => {
                self.dialogo = Some(Dialogo::Confirmar {
                    titulo: "CERRAR PESTAÑA".to_string(),
                    lineas: vec!["¿Cerrar la sesión remota?".to_string()],
                    peligro: true,
                    accion: AccionDialogo::CerrarPestana(sesion_id),
                });
            }
        }
    }

    /// Suelta lo que la vista Sesión tenía en el servidor y en el terminal
    /// exterior, sin cambiar de vista (lo usa también `ir_a_vista`).
    pub(crate) fn dejar_vista_sesion(&mut self) {
        self.terminar_escritura_varios(MotivoFin::SalidaDeLaVista);
        self.cerrar_historial();
        self.enlaces.cerrar();
        self.tmux = None;
        self.desadjuntar_paneles();
        if self.raton_capturado {
            self.raton_capturado = false;
            if self.efectos_externos {
                crate::ui::fijar_raton(false);
            }
        }
    }

    fn salir_de_la_vista_sesion(&mut self) {
        self.dejar_vista_sesion();
        let destino = self.vista_previa.unwrap_or(Vista::Hosts);
        self.vista = if destino == Vista::Sesion {
            Vista::Sesiones
        } else {
            destino
        };
        self.sucio = true;
    }

    fn renombrar_pestana_enfocada(&mut self) {
        let Some(pestana) = self.pestana_enfocada() else {
            return;
        };
        if pestana.escritorio_id.is_some() {
            self.mensaje("un escritorio no se renombra: es su host", true);
            return;
        }
        let sesion_id = pestana.sesion_id;
        self.dialogo = Some(Dialogo::EntradaTexto {
            titulo: "RENOMBRAR PESTAÑA".to_string(),
            etiqueta: "Nombre".to_string(),
            campo: CampoTexto::nuevo(pestana.nombre.clone()),
            accion: EntradaTextoAccion::RenombrarSesion(sesion_id),
        });
    }

    fn mover_pestana_enfocada(&mut self, paso: i32) {
        let Some(indice) = self.paneles.indice_enfocado(&self.pestanas) else {
            return;
        };
        let Some(pestana) = self.pestanas.get(indice) else {
            return;
        };
        if pestana.escritorio_id.is_some() {
            self.mensaje("un escritorio no se mueve desde aquí", true);
            return;
        }
        let destino = (indice as i32 + paso).clamp(0, self.pestanas.len() as i32 - 1) as u32;
        self.servidor.enviar(MensajeCliente::MoverSesion {
            sesion_id: pestana.sesion_id,
            posicion: destino,
        });
    }

    // --------------------------------------------------------- historial

    pub(crate) fn abrir_historial(&mut self, buscar: bool) {
        if self.historial.is_some() {
            return;
        }
        let Some(sesion_id) = self.sesion_enfocada() else {
            self.mensaje("no hay ningún terminal en el panel enfocado", true);
            return;
        };
        if self.pantalla_alternativa(sesion_id) {
            self.mensaje(
                "este programa tiene su propio historial (pantalla alternativa)",
                true,
            );
            return;
        }
        if self.config.terminal.historial_lineas_efectivo() == 0 {
            self.mensaje(
                "el historial está apagado en [terminal] historial_lineas",
                true,
            );
            return;
        }
        let peticion_id = self.siguiente_peticion_terminal;
        self.siguiente_peticion_terminal += 1;
        self.servidor.enviar(MensajeCliente::CapturarHistorial {
            peticion_id,
            sesion_id,
        });
        self.peticiones_terminal
            .insert(peticion_id, PeticionTerminal::Historial(sesion_id));
        let mut estado = EstadoHistorial::nueva(sesion_id, peticion_id);
        if buscar {
            estado.editando = true;
            estado.busqueda = Some(Busqueda::default());
        }
        self.historial = Some(estado);
        self.mensaje("capturando el historial…", false);
        self.sucio = true;
    }

    fn pantalla_alternativa(&self, sesion_id: u32) -> bool {
        self.pantallas.pantalla_alternativa(sesion_id)
    }

    fn cerrar_historial(&mut self) {
        self.historial = None;
        self.sucio = true;
    }

    fn tecla_historial(&mut self, tecla: KeyEvent) {
        let Some(estado) = self.historial.as_mut() else {
            return;
        };
        self.sucio = true;
        if estado.editando {
            match tecla.code {
                KeyCode::Esc => {
                    estado.editando = false;
                    estado.busqueda = None;
                }
                KeyCode::Enter => {
                    estado.editando = false;
                    match estado.busqueda.as_mut() {
                        Some(busqueda) if !busqueda.patron.is_empty() => estado.buscar(),
                        Some(_) => estado.busqueda = None,
                        None => {}
                    }
                }
                KeyCode::Tab => {
                    let busqueda = estado.busqueda.get_or_insert_with(Busqueda::default);
                    busqueda.clase = match busqueda.clase {
                        magi_nucleo::terminal::historial::ClaseBusqueda::Texto => {
                            magi_nucleo::terminal::historial::ClaseBusqueda::Expresion
                        }
                        magi_nucleo::terminal::historial::ClaseBusqueda::Expresion => {
                            magi_nucleo::terminal::historial::ClaseBusqueda::Texto
                        }
                    };
                }
                KeyCode::Backspace => {
                    if let Some(busqueda) = estado.busqueda.as_mut() {
                        busqueda.patron.pop();
                        busqueda.error = None;
                    }
                }
                KeyCode::Char(caracter)
                    if !tecla
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if let Some(busqueda) = estado.busqueda.as_mut() {
                        if busqueda.patron.chars().count() < 256 {
                            busqueda.patron.push(caracter);
                            busqueda.error = None;
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match tecla.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if estado.seleccion.is_some() {
                    estado.seleccion = None;
                } else if estado.busqueda.is_some() {
                    estado.busqueda = None;
                } else {
                    self.historial = None;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                estado.mover(-1);
                self.extender_seleccion_historial();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                estado.mover(1);
                self.extender_seleccion_historial();
            }
            KeyCode::PageUp => {
                estado.pagina(-1, false);
                self.extender_seleccion_historial();
            }
            KeyCode::PageDown => {
                estado.pagina(1, false);
                self.extender_seleccion_historial();
            }
            KeyCode::Char('u') if tecla.modifiers.contains(KeyModifiers::CONTROL) => {
                estado.pagina(-1, true);
                self.extender_seleccion_historial();
            }
            KeyCode::Char('d') if tecla.modifiers.contains(KeyModifiers::CONTROL) => {
                estado.pagina(1, true);
                self.extender_seleccion_historial();
            }
            KeyCode::Char('g') => {
                estado.principio();
                self.extender_seleccion_historial();
            }
            KeyCode::Char('G') => {
                estado.al_final();
                self.extender_seleccion_historial();
            }
            KeyCode::Char('/') => {
                estado.editando = true;
                if estado.busqueda.is_none() {
                    estado.busqueda = Some(Busqueda::default());
                }
            }
            KeyCode::Char('n') => estado.siguiente_coincidencia(true),
            KeyCode::Char('N') => estado.siguiente_coincidencia(false),
            KeyCode::Char('v') => estado.empezar_seleccion(false),
            KeyCode::Char('V') => estado.empezar_seleccion(true),
            KeyCode::Char('y') | KeyCode::Enter => {
                let texto = estado.texto_a_copiar();
                let lineas = texto.lines().count();
                self.copiar_texto(&texto, &format!("historial ({lineas} líneas)"));
            }
            KeyCode::Char('w') => self.pedir_guardar_historial(),
            KeyCode::Char('o') => {
                let lineas = match &self.historial {
                    Some(estado) => enlaces::lineas_logicas(&estado.captura),
                    None => return,
                };
                self.abrir_enlaces_con(&lineas);
            }
            _ => {}
        }
    }

    fn extender_seleccion_historial(&mut self) {
        let Some(estado) = &mut self.historial else {
            return;
        };
        if estado.seleccion.is_none() {
            return;
        }
        let cursor = estado.cursor;
        let columna = estado
            .captura
            .get(cursor)
            .map(|linea| linea.texto().chars().count().saturating_sub(1))
            .unwrap_or(0);
        estado.extender_seleccion(cursor, columna);
    }

    fn pedir_guardar_historial(&mut self) {
        let Some(estado) = &self.historial else {
            return;
        };
        let nombre = self
            .pestana_enfocada()
            .map(|pestana| pestana.nombre.clone())
            .unwrap_or_else(|| "sesion".to_string());
        let limpio: String = nombre
            .chars()
            .map(|caracter| {
                if caracter.is_ascii_alphanumeric() || caracter == '-' || caracter == '_' {
                    caracter
                } else {
                    '-'
                }
            })
            .collect();
        let ruta = self.rutas.hogar.join(format!(
            "magi-historial-{limpio}-{}.txt",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        let _ = estado;
        self.dialogo = Some(Dialogo::EntradaTexto {
            titulo: "GUARDAR HISTORIAL".to_string(),
            etiqueta: "Ruta".to_string(),
            campo: CampoTexto::nuevo(ruta.display().to_string()),
            accion: EntradaTextoAccion::GuardarHistorial,
        });
    }

    /// Confirma el diálogo de guardar: escribe con `create_new` y permisos 600.
    pub(crate) fn guardar_historial_en(&mut self, ruta: &str) {
        let texto = match self.historial.as_ref() {
            Some(estado) => estado.texto_plano(),
            None => {
                self.mensaje("ya no hay historial que guardar", true);
                return;
            }
        };
        let ruta = crate::conexion::cliente::expandir_home(ruta, &self.rutas.hogar);
        let resultado = escribir_nuevo(&ruta, texto.as_bytes());
        match resultado {
            Ok((lineas, bytes)) => {
                self.anotar(
                    crate::registro::HISTORIAL_GUARDADO,
                    None,
                    None,
                    &format!("{} · {lineas} líneas · {bytes} bytes", ruta.display()),
                    crate::modelo::ResultadoRegistro::Ok,
                );
                self.mensaje(
                    format!("historial guardado en {} ({lineas} líneas)", ruta.display()),
                    false,
                );
            }
            Err(motivo) => self.mensaje(
                format!(
                    "no se pudo guardar el historial en {}: {motivo}",
                    ruta.display()
                ),
                true,
            ),
        }
    }

    // ----------------------------------------------------------- enlaces

    pub(crate) fn abrir_enlaces(&mut self) {
        let lineas = match self.sesion_enfocada() {
            Some(sesion_id) => self
                .pantallas
                .captura(sesion_id)
                .map(|captura| enlaces::lineas_logicas(&captura))
                .unwrap_or_default(),
            None => {
                self.mensaje("no hay ningún terminal en el panel enfocado", true);
                return;
            }
        };
        self.abrir_enlaces_con(&lineas);
    }

    fn abrir_enlaces_con(&mut self, lineas: &[(usize, String)]) {
        self.enlaces.abrir(lineas);
        if !self.enlaces.activo {
            self.mensaje("sin enlaces a la vista", false);
            return;
        }
        self.sucio = true;
    }

    fn tecla_enlaces(&mut self, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Esc | KeyCode::Char('q') => self.enlaces.cerrar(),
            KeyCode::Char(caracter) if caracter.is_ascii_lowercase() => {
                if let Some(enlace) = self.enlaces.por_letra(caracter) {
                    let host = self
                        .pestana_enfocada()
                        .map(|pestana| pestana.host_nombre.clone())
                        .unwrap_or_default();
                    self.dialogo = Some(Dialogo::Enlace(
                        DialogoEnlace::nuevo(enlace.url).con_host(&host),
                    ));
                    self.sucio = true;
                }
            }
            _ => {}
        }
    }

    /// Teclas del diálogo del enlace: `↵` abre, `c` copia, `t` túnel.
    pub(crate) fn tecla_dialogo_enlace(&mut self, dialogo: DialogoEnlace, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Enter => self.abrir_url(&dialogo.url),
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.copiar_texto(&dialogo.url, "enlace");
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                let Some((destino, puerto)) = dialogo.tunel.clone() else {
                    self.mensaje(
                        "solo se abre por túnel un enlace a localhost del host",
                        true,
                    );
                    return;
                };
                let Some(sesion_id) = self.sesion_enfocada() else {
                    self.mensaje("la pestaña del enlace ya no existe", true);
                    return;
                };
                let peticion_id = self.siguiente_peticion_terminal;
                self.siguiente_peticion_terminal += 1;
                self.servidor.enviar(MensajeCliente::AbrirTunelTemporal {
                    peticion_id,
                    sesion_id,
                    destino,
                    puerto,
                });
                self.peticiones_terminal.insert(
                    peticion_id,
                    PeticionTerminal::TunelTemporal {
                        sesion_id,
                        url: dialogo.url.clone(),
                    },
                );
                self.mensaje("abriendo un túnel temporal…", false);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.dialogo = None;
            }
            _ => {
                self.dialogo = Some(Dialogo::Enlace(dialogo));
            }
        }
    }

    // ------------------------------------------------------------ ratón

    pub(crate) fn despachar_raton(&mut self, evento: crossterm::event::MouseEvent) {
        if self.vista != Vista::Sesion {
            return;
        }
        if self.historial.is_some() {
            self.raton_historial(&evento);
            return;
        }
        let posicion = ratatui::layout::Position::new(evento.column, evento.row);
        let Some((panel, rect)) = self
            .disposicion
            .panes
            .iter()
            .find(|(_, rect)| rect.contains(posicion))
            .copied()
        else {
            return;
        };
        let enfocado = self.paneles.enfocado();
        // 1. Reenvío al remoto del panel enfocado si lo pide.
        if panel == enfocado {
            if let Some(indice) = self
                .paneles
                .indices(&self.pestanas)
                .get(panel)
                .copied()
                .flatten()
            {
                if let Some(pestana) = self.pestanas.get(indice) {
                    if pestana.escritorio_id.is_none() {
                        let modo = self
                            .pantallas
                            .modo_raton(pestana.sesion_id)
                            .unwrap_or_default();
                        if modo.pedido() {
                            if let Some(relativo) = raton::a_nucleo(&evento, rect) {
                                if let Some(bytes) = raton::codificar(&relativo, modo) {
                                    self.servidor.enviar(MensajeCliente::Teclas {
                                        sesion_id: pestana.sesion_id,
                                        bytes,
                                    });
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
        if !self.config.terminal.raton {
            return;
        }
        // 2. Clic para enfocar otro panel.
        if raton::clic(&evento) && panel != enfocado {
            self.paneles.enfocado = panel;
            self.tras_cambiar_paneles();
            return;
        }
        // 3. Rueda: historial en la pantalla principal o 3 flechas en la
        // alternativa (si el remoto no pidió el ratón).
        if let Some(pasos) = raton::rueda(&evento) {
            let Some(indice) = self
                .paneles
                .indices(&self.pestanas)
                .get(panel)
                .copied()
                .flatten()
            else {
                return;
            };
            let Some(pestana) = self.pestanas.get(indice) else {
                return;
            };
            if pestana.escritorio_id.is_some() {
                return;
            }
            if self.pantalla_alternativa(pestana.sesion_id) {
                let modo = self
                    .pantallas
                    .modo_terminal(pestana.sesion_id)
                    .unwrap_or_default();
                let tecla = if pasos > 0 {
                    crossterm::event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
                } else {
                    crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
                };
                let mut bytes = Vec::new();
                for _ in 0..3 * pasos.abs() {
                    if let Some(codificada) = crate::teclas::bytes_de_tecla_con_modo(tecla, modo) {
                        bytes.extend(codificada);
                    }
                }
                if !bytes.is_empty() {
                    self.servidor.enviar(MensajeCliente::Teclas {
                        sesion_id: pestana.sesion_id,
                        bytes,
                    });
                }
            } else {
                self.abrir_historial(false);
                if let Some(estado) = &mut self.historial {
                    estado.desplazamiento_inicial = i64::from(pasos) * RUEDA_FILAS;
                }
            }
        }
    }

    fn raton_historial(&mut self, evento: &crossterm::event::MouseEvent) {
        let Some(area) = self.disposicion.modo_area else {
            return;
        };
        let Some(estado) = &mut self.historial else {
            return;
        };
        if let Some(pasos) = raton::rueda(evento) {
            estado.mover(i64::from(pasos) * RUEDA_FILAS);
            return;
        }
        let (columna, fila) = raton::posicion(evento, area);
        let linea = estado.primera() + fila;
        if raton::clic(evento) {
            estado.cursor = linea.min(estado.captura.len().saturating_sub(1));
            estado.empezar_seleccion(false);
            estado.extender_seleccion(estado.cursor, columna);
        } else if raton::arrastre(evento) && estado.seleccion.is_some() {
            estado.extender_seleccion(linea.min(estado.captura.len().saturating_sub(1)), columna);
        }
        self.sucio = true;
    }

    pub(crate) fn despachar_foco_exterior(&mut self, enfocado: bool) {
        self.foco_exterior = enfocado;
        self.sincronizar_terminal_exterior();
    }

    // ------------------------------------------------------------ pegado

    pub(crate) fn despachar_pegado(&mut self, texto: String) {
        // Fuera de la vista Sesión en vivo, el pegado va al campo con foco
        // (diálogo, búsqueda, filtro) si lo hay; si no, se descarta (§4.3).
        if let Some(dialogo) = self.dialogo.as_mut() {
            if dialogo.pegar_texto(&texto_para_campo(&texto)) {
                self.sucio = true;
            }
            return;
        }
        if let Some(paleta) = self.paleta.as_mut() {
            let limpio = texto_para_campo(&texto);
            if !limpio.is_empty() {
                paleta.consulta.insertar_texto(&limpio);
                paleta.recalcular();
                self.sucio = true;
            }
            return;
        }
        if let Some(estado) = self.historial.as_mut() {
            if estado.editando {
                estado.pegar_en_patron(&texto_para_campo(&texto));
                self.sucio = true;
            }
            return;
        }
        if self.vista == Vista::Ficha {
            if let Some(ficha) = self.ficha.as_mut() {
                let campo = ficha.campo;
                if let Some(texto_campo) = ficha.campo_texto_mut(campo) {
                    texto_campo.insertar_texto(&texto_para_campo(&texto));
                    self.sucio = true;
                }
            }
            return;
        }
        if self.vista == Vista::Servicios {
            if let Some(logs) = self.servicios.logs.as_mut() {
                if logs.filtro_activo {
                    logs.filtro.push_str(&texto_para_campo(&texto));
                    self.sucio = true;
                    return;
                }
            }
            if self.servicios.filtro_activo {
                self.servicios.filtro.push_str(&texto_para_campo(&texto));
                self.sucio = true;
            }
            return;
        }
        if self.vista == Vista::Registro && self.registro.texto_activo {
            self.registro
                .campo
                .insertar_texto(&texto_para_campo(&texto));
            self.sucio = true;
            return;
        }
        if self.filtro_activo {
            self.filtro.push_str(&texto_para_campo(&texto));
            self.sucio = true;
            return;
        }
        if self.vista != Vista::Sesion {
            return;
        }
        if self.enlaces.activo {
            return;
        }
        if self.escritura_varios.activa() {
            self.pegar_en_varios(&texto);
            return;
        }
        let Some(sesion_id) = self.sesion_enfocada() else {
            return;
        };
        let modo = self.pantallas.modo_terminal(sesion_id).unwrap_or_default();
        match pegado::preparar(&texto, modo, self.config.terminal.avisar_pegado) {
            pegado::AccionPegado::Nada => {}
            pegado::AccionPegado::Enviar(bytes) => {
                self.servidor
                    .enviar(MensajeCliente::Teclas { sesion_id, bytes });
            }
            pegado::AccionPegado::Avisar(saneado) => {
                self.preguntar_pegar_varias_lineas(vec![(sesion_id, saneado)]);
            }
        }
    }

    fn pegar_en_varios(&mut self, texto: &str) {
        let sesiones = self.escritura_varios.sesiones();
        let mut aviso = Vec::new();
        let mut enviar = Vec::new();
        for sesion_id in sesiones {
            let modo = self.pantallas.modo_terminal(sesion_id).unwrap_or_default();
            match pegado::preparar(texto, modo, self.config.terminal.avisar_pegado) {
                pegado::AccionPegado::Nada => {}
                pegado::AccionPegado::Enviar(bytes) => {
                    enviar.push(protocolo::BytesPestana { sesion_id, bytes });
                }
                pegado::AccionPegado::Avisar(saneado) => aviso.push((sesion_id, saneado)),
            }
        }
        if !aviso.is_empty() {
            self.preguntar_pegar_varias_lineas(aviso);
            // El resto también espera la confirmación: se guarda en el aviso.
            if let Some(Dialogo::Confirmar { .. }) = &self.dialogo {
                if let Some(pendiente) = &mut self.pegado_en_espera {
                    pendiente.otros = enviar;
                }
            }
            return;
        }
        self.enviar_varios(enviar);
    }

    fn preguntar_pegar_varias_lineas(&mut self, pendientes: Vec<(u32, String)>) {
        let Some((sesion_id, saneado)) = pendientes.first().cloned() else {
            return;
        };
        let host = self
            .pestanas
            .iter()
            .find(|pestana| pestana.sesion_id == sesion_id)
            .map(|pestana| pestana.host_nombre.clone())
            .unwrap_or_else(|| "el host".to_string());
        let lineas = pegado::vista_previa(&saneado, 3);
        let total = saneado.split('\r').count();
        let mut cuerpo = vec![format!(
            "Vas a pegar {total} líneas en {host} y el programa no protege el pegado:"
        )];
        cuerpo.push("cada línea se ejecutaría al llegar.".to_string());
        for (numero, linea) in lineas.iter().enumerate() {
            cuerpo.push(format!("  {}  {linea}", numero + 1));
        }
        self.pegado_en_espera = Some(crate::app::pegado::PendientePegado {
            sesion_id,
            texto: saneado,
            otros: Vec::new(),
            avisos: pendientes,
        });
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "PEGAR VARIAS LÍNEAS".to_string(),
            lineas: cuerpo,
            peligro: false,
            accion: AccionDialogo::PegarVariasLineas,
        });
    }

    /// Confirmado el aviso: pega (con su codificación actual, sin marcas) en
    /// cada pestaña que esperaba.
    pub(crate) fn confirmar_pegado_en_espera(&mut self) {
        let Some(pendiente) = self.pegado_en_espera.take() else {
            return;
        };
        for (sesion_id, saneado) in pendiente.avisos {
            let modo = self.pantallas.modo_terminal(sesion_id).unwrap_or_default();
            if let Some(bytes) =
                magi_nucleo::terminal::pegado::pegar(&saneado, modo.pegado_corchetes)
            {
                self.servidor
                    .enviar(MensajeCliente::Teclas { sesion_id, bytes });
            }
        }
        if !pendiente.otros.is_empty() {
            self.enviar_varios(pendiente.otros);
        }
    }

    pub(crate) fn cancelar_pegado_en_espera(&mut self) {
        self.pegado_en_espera = None;
    }
}

/// Alto del contenido de un panel a partir del alto de la vista entera (sin
/// la cabecera de la barra global).
fn alto_de_panel(filas: u16) -> u16 {
    filas.saturating_sub(1).max(1)
}

/// Escribe un fichero nuevo con `create_new` y permisos 600 (nunca
/// sobrescribe). Devuelve las líneas y los bytes escritos.
fn escribir_nuevo(ruta: &std::path::Path, contenido: &[u8]) -> Result<(usize, u64), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut fichero = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(ruta)
        .map_err(|error| error.to_string())?;
    fichero
        .write_all(contenido)
        .map_err(|error| error.to_string())?;
    let bytes = contenido.len() as u64;
    let lineas = contenido.iter().filter(|byte| **byte == b'\n').count();
    Ok((lineas, bytes))
}
