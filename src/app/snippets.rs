//! Vista Snippets (`F8`): lista con destino resumido, panel inferior con el
//! comando, los destinos resueltos, la confirmación y el uso; filtro `/`;
//! alta, edición y borrado. `↵`, `a` y `p` lanzan el seleccionado (ver
//! `lanzar`).

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};

use crate::deliberacion::Verificaciones;
use crate::snippets::Snippet;
use crate::ui::snippets::{limpio, partir};
use crate::ui::Vista;

use super::formulario_snippet::{AccionFormulario, FormularioSnippet};
use super::lanzar::OrigenLanzamiento;
use super::{AccionDialogo, AccionPaleta, App, Dialogo, EntradaPaleta};

/// Ancho útil de las líneas del diálogo de confirmación (66 de ancho menos
/// bordes y márgenes): el modal no parte las líneas largas.
const ANCHO_CONFIRMACION: usize = 58;

/// Estado de la vista Snippets.
#[derive(Default)]
pub struct EstadoSnippets {
    /// Todos los snippets, por nombre (se releen de `SNIPPETS`).
    pub lista: Vec<Snippet>,
    /// Selección sobre `snippets_visibles`.
    pub seleccion: usize,
    pub desplazamiento: usize,
    pub filtro: String,
    pub filtro_activo: bool,
    /// Verificaciones previas por host (`VERIFICACIONES_HOST`): con ellas el
    /// panel inferior dice si una ejecución pasaría por la deliberación MAGI.
    pub verificaciones: HashMap<i64, Verificaciones>,
}

/// Diálogos propios de la vista (viven en el hueco `App::dialogo`).
pub enum DialogoSnippets {
    Formulario(FormularioSnippet),
}

/// Acciones confirmadas de la vista (viajan en `AccionDialogo::Snippets`).
pub enum AccionSnippets {
    /// Borrar un snippet (id y nombre fijados al abrir la confirmación).
    Borrar { id: i64, nombre: String },
}

/// Entradas de la paleta de la vista.
#[derive(Debug, Clone)]
pub enum AccionPaletaSnippets {
    IrASnippets,
    IrAResultados,
    Nuevo,
}

impl App {
    /// Relee `SNIPPETS` (y las verificaciones previas) conservando la
    /// selección por id: la lista va por nombre y renombrar mueve la fila.
    pub(super) fn recargar_snippets(&mut self) {
        let seleccionado = self.snippet_seleccionado().map(|snippet| snippet.id);
        match self.almacen.listar_snippets() {
            Ok(lista) => self.snippets.lista = lista,
            Err(error) => {
                self.mensaje(format!("no se pudieron leer los snippets: {error}"), true);
                return;
            }
        }
        match self.almacen.verificaciones_por_host() {
            Ok(verificaciones) => self.snippets.verificaciones = verificaciones,
            // Se quedan las de la última lectura buena; el aviso dice que la
            // «Confirmación» del panel puede no estar al día.
            Err(error) => self.mensaje(
                format!("no se pudieron leer las verificaciones previas: {error}"),
                true,
            ),
        }
        if let Some(id) = seleccionado {
            self.seleccionar_snippet_id(id);
        }
        self.ajustar_snippets();
    }

    /// Snippets que pasan el filtro, en el orden de la lista.
    pub fn snippets_visibles(&self) -> Vec<&Snippet> {
        let filtro = self.snippets.filtro.trim();
        if filtro.is_empty() {
            return self.snippets.lista.iter().collect();
        }
        self.snippets
            .lista
            .iter()
            .filter(|snippet| crate::snippets::coincide(snippet, filtro))
            .collect()
    }

    /// El snippet bajo la selección, si hay alguno visible.
    pub fn snippet_seleccionado(&self) -> Option<&Snippet> {
        self.snippets_visibles()
            .get(self.snippets.seleccion)
            .copied()
    }

    /// Deja seleccionado el snippet con ese id, si sigue en la lista visible.
    fn seleccionar_snippet_id(&mut self, id: i64) -> bool {
        match self
            .snippets_visibles()
            .iter()
            .position(|snippet| snippet.id == id)
        {
            Some(indice) => {
                self.snippets.seleccion = indice;
                true
            }
            None => false,
        }
    }

    /// Acota la selección a lo visible y la deja dentro de la ventana.
    fn ajustar_snippets(&mut self) {
        let total = self.snippets_visibles().len();
        let altura = self
            .disposicion
            .filas(crate::ui::disposicion::Lista::Snippets);
        let estado = &mut self.snippets;
        if estado.seleccion >= total {
            estado.seleccion = total.saturating_sub(1);
        }
        estado.desplazamiento =
            desplazamiento_ajustado(estado.seleccion, estado.desplazamiento, altura, total);
    }

    /// Lleva la selección a `indice` (acotado a la lista visible).
    fn mover_snippet_a(&mut self, indice: usize) {
        let total = self.snippets_visibles().len();
        self.snippets.seleccion = indice.min(total.saturating_sub(1));
        self.ajustar_snippets();
    }

    /// El filtro cambió: la selección vuelve al principio.
    fn reiniciar_seleccion_snippets(&mut self) {
        self.snippets.seleccion = 0;
        self.snippets.desplazamiento = 0;
    }

    /// Entra en la vista Snippets.
    pub(super) fn ir_a_snippets(&mut self) {
        self.salir_de_sesion_si_hace_falta();
        if !matches!(self.vista, Vista::Snippets | Vista::Resultados) {
            self.vista_previa = Some(self.vista);
        }
        self.vista = Vista::Snippets;
        self.ficha = None;
        self.recargar_snippets();
    }

    /// `q`/`Esc`: vuelve a la vista de la que se entró en Snippets.
    fn volver_de_snippets(&mut self) {
        match destino_al_volver(self.vista_previa, self.ficha.is_some()) {
            Vista::Flota => self.entrar_en_flota(),
            Vista::Hosts => self.ir_a_hosts(),
            Vista::Sesion => self.ir_a_sesion(),
            otro => self.vista = otro,
        }
    }

    /// Teclas de la vista Snippets.
    pub(super) fn tecla_snippets(&mut self, tecla: KeyEvent) {
        let altura = self
            .disposicion
            .filas(crate::ui::disposicion::Lista::Snippets);
        let seleccion = self.snippets.seleccion;
        if self.snippets.filtro_activo {
            match tecla.code {
                KeyCode::Esc => {
                    self.snippets.filtro.clear();
                    self.snippets.filtro_activo = false;
                    self.reiniciar_seleccion_snippets();
                }
                KeyCode::Enter => self.snippets.filtro_activo = false,
                KeyCode::Up => self.mover_snippet_a(seleccion.saturating_sub(1)),
                KeyCode::Down => self.mover_snippet_a(seleccion + 1),
                _ => {
                    if super::manejar_texto(&mut self.snippets.filtro, tecla) {
                        self.reiniciar_seleccion_snippets();
                    }
                }
            }
            return;
        }
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => self.mover_snippet_a(seleccion.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.mover_snippet_a(seleccion + 1),
            KeyCode::PageUp => self.mover_snippet_a(seleccion.saturating_sub(altura)),
            KeyCode::PageDown => self.mover_snippet_a(seleccion + altura),
            KeyCode::Home => self.mover_snippet_a(0),
            KeyCode::End => self.mover_snippet_a(usize::MAX),
            KeyCode::Char('/') => {
                self.snippets.filtro_activo = true;
                self.snippets.filtro.clear();
                self.reiniciar_seleccion_snippets();
            }
            KeyCode::Char('n') => self.nuevo_snippet(),
            KeyCode::Char('e') => self.editar_snippet_seleccionado(),
            KeyCode::Char('x') => self.confirmar_borrado_snippet(),
            KeyCode::Char('t') => self.ir_a_resultados(),
            // `↵` ejecuta: el detalle completo (plegado con la vista baja) se
            // abre con `i`.
            KeyCode::Char('i') => {
                let ascii = self.tema.ascii;
                let detalle = self.snippet_seleccionado().map(|snippet| {
                    (
                        crate::ui::snippets::titulo_detalle(snippet, ascii),
                        crate::ui::snippets::lineas_detalle(self, snippet, ascii),
                    )
                });
                if let Some((titulo, lineas)) = detalle {
                    self.dialogo = Some(Dialogo::Detalle {
                        titulo,
                        lineas,
                        tunel_caido: None,
                    });
                }
            }
            KeyCode::Enter => self.lanzar_seleccionado(OrigenLanzamiento::Dialogo),
            KeyCode::Char('a') => self.lanzar_seleccionado(OrigenLanzamiento::Todos),
            KeyCode::Char('p') => self.lanzar_seleccionado(OrigenLanzamiento::Pestanas),
            KeyCode::Esc => {
                if self.snippets.filtro.is_empty() {
                    self.volver_de_snippets();
                } else {
                    self.snippets.filtro.clear();
                    self.reiniciar_seleccion_snippets();
                }
            }
            KeyCode::Char('q') => self.volver_de_snippets(),
            KeyCode::Char('?') => self.ayuda = true,
            _ => {}
        }
    }

    /// `↵` (diálogo EJECUTAR), `a` (todos) y `p` (en pestaña) sobre el
    /// seleccionado: se lanza por id, releído de la base.
    fn lanzar_seleccionado(&mut self, origen: OrigenLanzamiento) {
        if let Some(id) = self.snippet_seleccionado().map(|snippet| snippet.id) {
            self.iniciar_snippet(id, origen);
        }
    }

    /// `n`: formulario de alta con los hosts y las etiquetas de host de ahora.
    fn nuevo_snippet(&mut self) {
        let formulario = FormularioSnippet::nuevo(&self.hosts, &self.etiquetas);
        self.dialogo = Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario)));
    }

    /// `e`: formulario de edición del seleccionado.
    fn editar_snippet_seleccionado(&mut self) {
        let Some(snippet) = self.snippet_seleccionado() else {
            return;
        };
        let formulario = FormularioSnippet::editar(snippet, &self.hosts, &self.etiquetas);
        self.dialogo = Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario)));
    }

    /// `x`: confirmación de borrado con el id y el nombre fijados; si algún
    /// host lo tiene como «snippet al conectar», lo dice.
    fn confirmar_borrado_snippet(&mut self) {
        let Some((id, nombre)) = self
            .snippet_seleccionado()
            .map(|snippet| (snippet.id, snippet.nombre.clone()))
        else {
            return;
        };
        let al_conectar = match self.almacen.hosts_con_snippet_al_conectar(id) {
            Ok(hosts) => hosts
                .into_iter()
                .map(|(_, nombre)| nombre)
                .collect::<Vec<_>>(),
            Err(error) => {
                // Sin saber a quién deja sin «snippet al conectar» no se ofrece
                // el borrado: el aviso es parte de la confirmación.
                self.mensaje(
                    format!("no se pudo comprobar quién lo usa al conectar: {error}"),
                    true,
                );
                return;
            }
        };
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "BORRAR SNIPPET".to_string(),
            lineas: lineas_borrado(&nombre, &al_conectar),
            peligro: true,
            accion: AccionDialogo::Snippets(AccionSnippets::Borrar { id, nombre }),
        });
    }

    /// Tecla con un diálogo de la vista abierto. El diálogo llega sacado del
    /// hueco: si sigue abierto, hay que devolverlo a `self.dialogo`.
    pub(super) fn tecla_dialogo_snippets(&mut self, dialogo: DialogoSnippets, tecla: KeyEvent) {
        match dialogo {
            DialogoSnippets::Formulario(mut formulario) => match formulario.manejar_tecla(&tecla) {
                AccionFormulario::Nada => {
                    self.dialogo = Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario)));
                }
                AccionFormulario::Cancelar => {}
                AccionFormulario::Guardar => self.guardar_snippet(formulario),
            },
        }
    }

    /// `Ctrl+S` en el formulario: crea o actualiza (el almacén normaliza y
    /// valida). Si falla, el error va al pie y el formulario sigue abierto.
    fn guardar_snippet(&mut self, mut formulario: FormularioSnippet) {
        let datos = formulario.datos();
        let guardado = match formulario.id {
            Some(id) => self.almacen.actualizar_snippet(id, &datos).map(|()| id),
            None => self.almacen.crear_snippet(&datos),
        };
        let id = match guardado {
            Ok(id) => id,
            Err(error) => {
                formulario.error = Some(error.to_string());
                self.dialogo = Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario)));
                return;
            }
        };
        self.recargar_snippets();
        // Si el filtro de ahora lo esconde, se quita: tras guardar hay que ver
        // lo guardado.
        if !self.seleccionar_snippet_id(id) {
            self.snippets.filtro.clear();
            self.snippets.filtro_activo = false;
            self.seleccionar_snippet_id(id);
        }
        self.ajustar_snippets();
        let Some(snippet) = self.snippets.lista.iter().find(|snippet| snippet.id == id) else {
            self.mensaje("snippet guardado", false);
            return;
        };
        let nombre = snippet.nombre.clone();
        let apto_al_conectar = snippet.apto_al_conectar();
        let resueltos =
            crate::snippets::resolver(&snippet.destinos, &self.hosts, &self.grupos).len();
        let mut avisos = Vec::new();
        if resueltos == 0 {
            // Guardar con cero hosts no se bloquea (una etiqueta puede no
            // tener hosts todavía), pero se avisa.
            avisos.push("todavía no apunta a ningún host".to_string());
        }
        if !apto_al_conectar {
            // Crítico o con variables ya no se escribe al abrir una pestaña:
            // quien lo tenga como «snippet al conectar» debe saberlo.
            match self.almacen.hosts_con_snippet_al_conectar(id) {
                Ok(hosts) if !hosts.is_empty() => {
                    let nombres: Vec<String> = hosts
                        .into_iter()
                        .map(|(_, nombre)| limpio(&nombre))
                        .collect();
                    avisos.push(aviso_al_conectar(&nombres));
                }
                Ok(_) => {}
                Err(error) => avisos.push(format!(
                    "no se pudo comprobar quién lo usa al conectar: {error}"
                )),
            }
        }
        if avisos.is_empty() {
            self.mensaje(format!("snippet «{nombre}» guardado"), false);
        } else {
            self.mensaje(
                format!("snippet «{nombre}» guardado; {}", avisos.join("; ")),
                true,
            );
        }
    }

    pub(super) fn ejecutar_accion_snippets(&mut self, accion: AccionSnippets) {
        match accion {
            AccionSnippets::Borrar { id, nombre } => {
                if let Err(error) = self.almacen.borrar_snippet(id) {
                    self.mensaje(format!("no se pudo borrar «{nombre}»: {error}"), true);
                    return;
                }
                self.recargar_snippets();
                // Los hosts que lo tenían al conectar quedan con `NULL` en la
                // tabla: la copia en memoria del inventario se pone al día.
                if let Err(error) = self.recargar_inventario() {
                    self.mensaje(format!("snippet «{nombre}» borrado; {error}"), true);
                    return;
                }
                self.mensaje(format!("snippet «{nombre}» borrado"), false);
            }
        }
    }

    /// Entradas de la paleta: `ir a snippets`, `ir a resultados`, `nuevo
    /// snippet`.
    pub(super) fn entradas_paleta_snippets(&self) -> Vec<EntradaPaleta> {
        [
            ("ir a snippets", AccionPaletaSnippets::IrASnippets),
            ("ir a resultados", AccionPaletaSnippets::IrAResultados),
            ("nuevo snippet", AccionPaletaSnippets::Nuevo),
        ]
        .into_iter()
        .map(|(etiqueta, accion)| EntradaPaleta {
            etiqueta: etiqueta.to_string(),
            categoria: "snippets",
            accion: AccionPaleta::Snippets(accion),
        })
        .collect()
    }

    pub(super) fn accion_paleta_snippets(&mut self, accion: AccionPaletaSnippets) {
        match accion {
            AccionPaletaSnippets::IrASnippets => self.ir_a_vista(Vista::Snippets),
            AccionPaletaSnippets::IrAResultados => self.ir_a_vista(Vista::Resultados),
            AccionPaletaSnippets::Nuevo => {
                self.ir_a_vista(Vista::Snippets);
                // Con la ficha a medio editar, `ir_a_vista` pregunta antes de
                // descartar: ese diálogo no se pisa con el formulario.
                if self.vista == Vista::Snippets && self.dialogo.is_none() {
                    self.nuevo_snippet();
                }
            }
        }
    }
}

/// Desplazamiento que deja la fila `seleccion` dentro de una ventana de
/// `altura` filas sobre `total`, moviéndolo lo mínimo (como en Túneles).
fn desplazamiento_ajustado(
    seleccion: usize,
    desplazamiento: usize,
    altura: usize,
    total: usize,
) -> usize {
    let altura = altura.max(1);
    let mut desplazamiento = desplazamiento;
    if seleccion < desplazamiento {
        desplazamiento = seleccion;
    }
    if seleccion >= desplazamiento + altura {
        desplazamiento = seleccion + 1 - altura;
    }
    if desplazamiento + altura > total {
        desplazamiento = total.saturating_sub(altura);
    }
    desplazamiento
}

/// A qué vista vuelve `q` desde Snippets: la previa, salvo las que no se
/// pueden retomar sin más (la propia F8, la ficha ya cerrada, o Archivos, que
/// necesita un host y se reabre con `abrir_archivos`).
fn destino_al_volver(previa: Option<Vista>, hay_ficha: bool) -> Vista {
    match previa {
        Some(Vista::Ficha) if !hay_ficha => Vista::Hosts,
        Some(vista)
            if !matches!(
                vista,
                Vista::Snippets | Vista::Resultados | Vista::Archivos | Vista::Transferencias
            ) =>
        {
            vista
        }
        _ => Vista::Hosts,
    }
}

/// Aviso al guardar un snippet que deja de ser apto para «snippet al
/// conectar» de estos hosts.
fn aviso_al_conectar(hosts: &[String]) -> String {
    format!(
        "es el snippet al conectar de {}: dejará de escribirse al abrir",
        hosts.join(", ")
    )
}

/// Líneas de la confirmación de borrado: el nombre y, si algún host lo tiene
/// como «snippet al conectar», quiénes se quedarán sin él (partido a lo ancho
/// del diálogo, que no parte líneas).
fn lineas_borrado(nombre: &str, al_conectar: &[String]) -> Vec<String> {
    let mut lineas = partir(
        &limpio(&format!("¿Borrar el snippet «{nombre}»?")),
        ANCHO_CONFIRMACION,
    );
    if !al_conectar.is_empty() {
        lineas.extend(partir(
            &limpio(&format!(
                "Es el snippet al conectar de: {} (se quedarán sin él)",
                al_conectar.join(", ")
            )),
            ANCHO_CONFIRMACION,
        ));
    }
    lineas
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_desplazamiento_sigue_a_la_seleccion() {
        // Bajando más allá de la ventana, la ventana baja lo justo.
        assert_eq!(desplazamiento_ajustado(0, 0, 5, 20), 0);
        assert_eq!(desplazamiento_ajustado(4, 0, 5, 20), 0);
        assert_eq!(desplazamiento_ajustado(5, 0, 5, 20), 1);
        assert_eq!(desplazamiento_ajustado(19, 0, 5, 20), 15);
        // Subiendo por encima, la ventana sube hasta la selección.
        assert_eq!(desplazamiento_ajustado(3, 10, 5, 20), 3);
        // Si la lista encoge, no queda hueco al final.
        assert_eq!(desplazamiento_ajustado(2, 10, 5, 3), 0);
        // Altura 0 se trata como 1.
        assert_eq!(desplazamiento_ajustado(7, 0, 0, 20), 7);
    }

    #[test]
    fn volver_evita_las_vistas_que_no_se_retoman() {
        assert_eq!(destino_al_volver(Some(Vista::Flota), false), Vista::Flota);
        assert_eq!(
            destino_al_volver(Some(Vista::Tuneles), false),
            Vista::Tuneles
        );
        assert_eq!(destino_al_volver(Some(Vista::Ficha), true), Vista::Ficha);
        assert_eq!(destino_al_volver(Some(Vista::Ficha), false), Vista::Hosts);
        assert_eq!(
            destino_al_volver(Some(Vista::Snippets), false),
            Vista::Hosts
        );
        assert_eq!(
            destino_al_volver(Some(Vista::Resultados), false),
            Vista::Hosts
        );
        assert_eq!(
            destino_al_volver(Some(Vista::Archivos), false),
            Vista::Hosts
        );
        assert_eq!(destino_al_volver(None, false), Vista::Hosts);
    }

    #[test]
    fn guardar_uno_que_deja_de_ser_apto_avisa_a_sus_hosts() {
        assert_eq!(
            aviso_al_conectar(&["web-01".to_string(), "web-02".to_string()]),
            "es el snippet al conectar de web-01, web-02: dejará de escribirse al abrir"
        );
    }

    #[test]
    fn el_borrado_avisa_de_los_hosts_que_lo_usan_al_conectar() {
        assert_eq!(
            lineas_borrado("uptime", &[]),
            vec!["¿Borrar el snippet «uptime»?"]
        );
        let lineas = lineas_borrado("tmux", &["web-01".to_string(), "web-02".to_string()]);
        assert_eq!(lineas[0], "¿Borrar el snippet «tmux»?");
        let aviso = lineas[1..].join(" ");
        assert_eq!(
            aviso,
            "Es el snippet al conectar de: web-01, web-02 (se quedarán sin él)"
        );
        assert!(lineas
            .iter()
            .all(|linea| linea.chars().count() <= ANCHO_CONFIRMACION));
        // Un nombre tocado a mano en la base no cuela escapes en el diálogo.
        let lineas = lineas_borrado("mal\u{1b}[2Jo", &[]);
        assert_eq!(lineas, vec!["¿Borrar el snippet «malo»?"]);
    }
}
