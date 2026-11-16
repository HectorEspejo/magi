//! Paneles de la vista Sesión (Fase 15).
//!
//! Cinco disposiciones fijas por ventana: completa, lado a lado, apilados,
//! 1 grande + 2 y rejilla de 4. La disposición es de la ventana y no se
//! guarda. Cada panel guarda la pestaña que enseña por su identidad (no por
//! su posición), para que cerrar o reordenar pestañas no cambie lo que se ve.
//! Por debajo del mínimo de la disposición solo se ve el panel enfocado.

use std::collections::HashMap;

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::PestanaUI;
use crate::ui::disposicion::Tamano;

/// Paneles como mucho (la rejilla de 4).
pub const PANELES_MAX: usize = 4;

/// La pestaña que enseña un panel, por identidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClavePestana {
    Terminal(u32),
    Escritorio(u32),
}

impl ClavePestana {
    /// La clave de una pestaña del espejo de sesiones.
    #[must_use]
    pub fn de(pestana: &PestanaUI) -> Self {
        match pestana.escritorio_id {
            Some(id) => ClavePestana::Escritorio(id),
            None => ClavePestana::Terminal(pestana.sesion_id),
        }
    }
}

/// Una de las cinco disposiciones de paneles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisposicionPaneles {
    #[default]
    Completa,
    LadoALado,
    Apilados,
    Grande2,
    Rejilla,
}

impl DisposicionPaneles {
    /// Todas, en el orden en que las recorre `prefijo Espacio`.
    pub const TODAS: [Self; 5] = [
        Self::Completa,
        Self::LadoALado,
        Self::Apilados,
        Self::Grande2,
        Self::Rejilla,
    ];

    /// La siguiente disposición (circular).
    #[must_use]
    pub fn siguiente(self) -> Self {
        let indice = Self::TODAS
            .iter()
            .position(|disposicion| *disposicion == self)
            .unwrap_or(0);
        Self::TODAS[(indice + 1) % Self::TODAS.len()]
    }

    /// Cuántos paneles se ven en esta disposición.
    #[must_use]
    pub fn cuantos(self) -> usize {
        match self {
            Self::Completa => 1,
            Self::LadoALado | Self::Apilados => 2,
            Self::Grande2 => 3,
            Self::Rejilla => 4,
        }
    }

    /// Mínimo del **contenido** (la ventana menos sus cuatro barras: marco,
    /// pestañas, estado y barra global) para que la disposición quepa entera.
    /// Los mínimos del informe son de la vista: 40×12, 60×12, 40×16, 80×18 y
    /// 80×20.
    #[must_use]
    pub fn minimo(self) -> Tamano {
        match self {
            Self::Completa => Tamano::new(40, 8),
            Self::LadoALado => Tamano::new(60, 8),
            Self::Apilados => Tamano::new(40, 12),
            Self::Grande2 => Tamano::new(80, 14),
            Self::Rejilla => Tamano::new(80, 16),
        }
    }

    /// Nombre de la disposición para el marco y la barra.
    #[must_use]
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Completa => "completa",
            Self::LadoALado => "lado a lado",
            Self::Apilados => "apilados",
            Self::Grande2 => "1 grande + 2",
            Self::Rejilla => "rejilla",
        }
    }

    /// Nombre para el título del marco, en mayúsculas.
    #[must_use]
    pub fn etiqueta(self) -> &'static str {
        match self {
            Self::Completa => "COMPLETA",
            Self::LadoALado => "LADO A LADO",
            Self::Apilados => "APILADOS",
            Self::Grande2 => "1 GRANDE + 2",
            Self::Rejilla => "REJILLA",
        }
    }
}

/// Estado de los paneles de la ventana.
#[derive(Debug, Clone, Default)]
pub struct Paneles {
    pub disposicion: DisposicionPaneles,
    pub enfocado: usize,
    /// `true`: el enfocado ocupa toda la vista sin perder la disposición.
    pub ampliado: bool,
    paneles: [Option<ClavePestana>; PANELES_MAX],
    /// Tamaño del contenido enviado a cada sesión (para no repetir
    /// `Redimensionar` ni `Adjuntar`).
    pub tamanos: HashMap<u32, (u16, u16)>,
}

impl Paneles {
    #[must_use]
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Paneles que se ven ahora mismo.
    #[must_use]
    pub fn visibles(&self) -> usize {
        if self.ampliado {
            1
        } else {
            self.disposicion.cuantos()
        }
    }

    /// ¿El panel `indice` se ve?
    #[must_use]
    pub fn es_visible(&self, indice: usize) -> bool {
        if self.ampliado {
            indice == self.enfocado
        } else {
            indice < self.visibles()
        }
    }

    /// Panel enfocado (dentro de los visibles).
    #[must_use]
    pub fn enfocado(&self) -> usize {
        if self.ampliado {
            self.enfocado
        } else {
            self.enfocado.min(self.visibles().saturating_sub(1))
        }
    }

    /// Pestaña de un panel, si tiene.
    #[must_use]
    pub fn pestana(&self, panel: usize) -> Option<ClavePestana> {
        self.paneles.get(panel).copied().flatten()
    }

    /// Índice (en `pestanas`) de la pestaña de cada panel visible; `None` si
    /// la pestaña ya no existe o el panel está vacío.
    #[must_use]
    pub fn indices(&self, pestanas: &[PestanaUI]) -> [Option<usize>; PANELES_MAX] {
        let mut indices = [None; PANELES_MAX];
        for (panel, clave) in self.paneles.iter().enumerate() {
            indices[panel] = clave.and_then(|clave| {
                pestanas
                    .iter()
                    .position(|pestana| ClavePestana::de(pestana) == clave)
            });
        }
        indices
    }

    /// Índice de la pestaña del panel enfocado.
    #[must_use]
    pub fn indice_enfocado(&self, pestanas: &[PestanaUI]) -> Option<usize> {
        let indices = self.indices(pestanas);
        indices.get(self.enfocado()).copied().flatten()
    }

    /// Panel en el que está una pestaña, si está en alguno.
    #[must_use]
    pub fn panel_de(&self, clave: ClavePestana) -> Option<usize> {
        self.paneles.iter().position(|otra| *otra == Some(clave))
    }

    /// Pone una pestaña en el panel enfocado; si ya estaba en otro panel, se
    /// intercambian (una pestaña está como mucho en un panel de la ventana).
    pub fn poner(&mut self, clave: ClavePestana) {
        let destino = self.enfocado();
        if let Some(otro) = self.panel_de(clave) {
            if otro != destino {
                let anterior = self.paneles[destino];
                self.paneles[destino] = Some(clave);
                self.paneles[otro] = anterior;
            }
            return;
        }
        self.paneles[destino] = Some(clave);
    }

    /// Ajusta los paneles a las pestañas que hay: quita las que ya no existen,
    /// rellena los visibles con las siguientes a la activa que no estén a la
    /// vista y deja la activa en el panel enfocado.
    pub fn sincronizar(&mut self, pestanas: &[PestanaUI], activa: Option<usize>) {
        for panel in &mut self.paneles {
            if let Some(clave) = *panel {
                if !pestanas
                    .iter()
                    .any(|pestana| ClavePestana::de(pestana) == clave)
                {
                    *panel = None;
                }
            }
        }
        // Una pestaña, un panel: se queda la primera aparición.
        for panel in 0..PANELES_MAX {
            let Some(clave) = self.paneles[panel] else {
                continue;
            };
            if self.paneles[..panel].contains(&Some(clave)) {
                self.paneles[panel] = None;
            }
        }
        let visibles = self.visibles();
        let activa_clave = activa.map(|indice| ClavePestana::de(&pestanas[indice]));
        if let Some(clave) = activa_clave {
            self.poner(clave);
        }
        // Rellena los paneles visibles que sigan vacíos con las pestañas
        // siguientes a la enfocada que no estén a la vista.
        let desde = activa.unwrap_or(0);
        for panel in 0..visibles {
            if self.paneles[panel].is_some() {
                continue;
            }
            let usadas: Vec<Option<ClavePestana>> = self.paneles.to_vec();
            let siguiente = (0..pestanas.len())
                .map(|paso| (desde + paso) % pestanas.len().max(1))
                .filter(|indice| *indice < pestanas.len())
                .map(|indice| ClavePestana::de(&pestanas[indice]))
                .find(|clave| !usadas.contains(&Some(*clave)));
            self.paneles[panel] = siguiente;
        }
        if self.enfocado >= visibles {
            self.enfocado = visibles.saturating_sub(1);
        }
    }

    /// Pasa a la siguiente disposición (y vuelve pestañas al reparto).
    pub fn cambiar_disposicion(&mut self, pestanas: &[PestanaUI], activa: Option<usize>) {
        self.disposicion = self.disposicion.siguiente();
        self.sincronizar(pestanas, activa);
    }

    /// Alterna el ampliado del panel enfocado.
    pub fn alternar_ampliado(&mut self) {
        self.ampliado = !self.ampliado;
    }

    /// Cambia el foco de panel con las flechas. Devuelve si lo movió.
    pub fn mover_foco(&mut self, tecla: KeyCode) -> bool {
        if self.ampliado {
            return false;
        }
        let destino = match (self.disposicion, tecla) {
            (DisposicionPaneles::Completa, _) => return false,
            (DisposicionPaneles::LadoALado, KeyCode::Left) => 0,
            (DisposicionPaneles::LadoALado, KeyCode::Right) => 1,
            (DisposicionPaneles::Apilados, KeyCode::Up) => 0,
            (DisposicionPaneles::Apilados, KeyCode::Down) => 1,
            (DisposicionPaneles::Grande2, KeyCode::Left) => 0,
            (DisposicionPaneles::Grande2, KeyCode::Right) => 1,
            (DisposicionPaneles::Grande2, KeyCode::Up) => 1,
            (DisposicionPaneles::Grande2, KeyCode::Down) => 2,
            (DisposicionPaneles::Rejilla, KeyCode::Left) => match self.enfocado {
                1 => 0,
                3 => 2,
                otro => otro,
            },
            (DisposicionPaneles::Rejilla, KeyCode::Right) => match self.enfocado {
                0 => 1,
                2 => 3,
                otro => otro,
            },
            (DisposicionPaneles::Rejilla, KeyCode::Up) => match self.enfocado {
                2 => 0,
                3 => 1,
                otro => otro,
            },
            (DisposicionPaneles::Rejilla, KeyCode::Down) => match self.enfocado {
                0 => 2,
                1 => 3,
                otro => otro,
            },
            _ => return false,
        };
        let destino = destino.min(self.visibles().saturating_sub(1));
        let movido = destino != self.enfocado;
        self.enfocado = destino;
        movido
    }

    /// ¿La ventana es demasiado pequeña para la disposición? Entonces solo se
    /// ve el panel enfocado («disposición en espera: ventana pequeña»).
    #[must_use]
    pub fn en_espera(&self, area: Rect) -> bool {
        !self.ampliado && !self.disposicion.minimo().cabe_en(area)
    }

    /// Reparto del área de contenido entre los paneles visibles (sin contar la
    /// fila de cabecera de cada uno, que descuenta el pintado).
    #[must_use]
    pub fn reparto(&self, area: Rect) -> Vec<(usize, Rect)> {
        let enfocado = self.enfocado();
        if self.ampliado || self.en_espera(area) || area.width == 0 || area.height == 0 {
            return vec![(enfocado, area)];
        }
        match self.disposicion {
            DisposicionPaneles::Completa => vec![(0, area)],
            DisposicionPaneles::LadoALado => {
                let trozos = Layout::horizontal([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
                vec![(0, trozos[0]), (1, trozos[2])]
            }
            DisposicionPaneles::Apilados => {
                let trozos = Layout::vertical([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
                vec![(0, trozos[0]), (1, trozos[2])]
            }
            DisposicionPaneles::Grande2 => {
                let columnas = Layout::horizontal([
                    Constraint::Percentage(50),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
                let filas = Layout::vertical([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(columnas[2]);
                vec![(0, columnas[0]), (1, filas[0]), (2, filas[2])]
            }
            DisposicionPaneles::Rejilla => {
                let filas = Layout::vertical([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
                let arriba = Layout::horizontal([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(filas[0]);
                let abajo = Layout::horizontal([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(filas[2]);
                vec![(0, arriba[0]), (1, arriba[2]), (2, abajo[0]), (3, abajo[2])]
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn clave(indice: u32) -> ClavePestana {
        ClavePestana::Terminal(indice)
    }

    fn pestanas(cuantas: u32) -> Vec<PestanaUI> {
        // Para estas pruebas solo hace falta la identidad de cada pestaña.
        (0..cuantas).map(PestanaUI::de_prueba).collect()
    }

    #[test]
    fn las_disposiciones_giran_y_declaran_sus_paneles() {
        let mut disposicion = DisposicionPaneles::default();
        let mut vistas = Vec::new();
        for _ in 0..6 {
            vistas.push((disposicion, disposicion.cuantos()));
            disposicion = disposicion.siguiente();
        }
        assert_eq!(
            vistas,
            vec![
                (DisposicionPaneles::Completa, 1),
                (DisposicionPaneles::LadoALado, 2),
                (DisposicionPaneles::Apilados, 2),
                (DisposicionPaneles::Grande2, 3),
                (DisposicionPaneles::Rejilla, 4),
                (DisposicionPaneles::Completa, 1),
            ]
        );
    }

    #[test]
    fn el_reparto_cubre_el_area_y_deja_separaciones() {
        let mut paneles = Paneles::nueva();
        paneles.disposicion = DisposicionPaneles::Rejilla;
        let area = Rect::new(0, 0, 80, 20);
        let trozos = paneles.reparto(area);
        assert_eq!(trozos.len(), 4);
        for (_, trozo) in &trozos {
            assert!(trozo.width > 0 && trozo.height > 0);
        }
        // Dos por dos: la columna de separación y la fila de separación quedan
        // fuera de los paneles.
        assert!(trozos.iter().all(|(_, trozo)| trozo.width < area.width));
    }

    #[test]
    fn por_debajo_del_minimo_solo_se_ve_el_enfocado() {
        let mut paneles = Paneles::nueva();
        paneles.disposicion = DisposicionPaneles::Rejilla;
        paneles.enfocado = 2;
        let area = Rect::new(0, 0, 40, 12);
        assert!(paneles.en_espera(area));
        assert_eq!(paneles.reparto(area), vec![(2, area)]);
        assert!(!paneles.en_espera(Rect::new(0, 0, 80, 20)));
    }

    #[test]
    fn poner_una_pestana_que_ya_esta_en_otro_panel_las_intercambia() {
        let mut paneles = Paneles::nueva();
        paneles.disposicion = DisposicionPaneles::LadoALado;
        paneles.paneles[0] = Some(clave(1));
        paneles.paneles[1] = Some(clave(2));
        paneles.enfocado = 1;
        paneles.poner(clave(1));
        assert_eq!(paneles.paneles[0], Some(clave(2)));
        assert_eq!(paneles.paneles[1], Some(clave(1)));
    }

    #[test]
    fn sincronizar_quita_las_que_ya_no_existen_y_rellena_huecos() {
        let mut paneles = Paneles::nueva();
        paneles.disposicion = DisposicionPaneles::LadoALado;
        paneles.paneles[0] = Some(clave(9));
        let lista = pestanas(3);
        paneles.sincronizar(&lista, Some(1));
        assert_eq!(paneles.paneles[0], Some(clave(1)));
        // El panel nuevo recibe la siguiente pestaña a la enfocada.
        assert_eq!(paneles.paneles[1], Some(clave(2)));
    }

    #[test]
    fn las_flechas_mueven_el_foco_segun_la_disposicion() {
        let mut paneles = Paneles::nueva();
        paneles.disposicion = DisposicionPaneles::Rejilla;
        paneles.enfocado = 0;
        assert!(paneles.mover_foco(KeyCode::Right));
        assert_eq!(paneles.enfocado, 1);
        assert!(paneles.mover_foco(KeyCode::Down));
        assert_eq!(paneles.enfocado, 3);
        assert!(paneles.mover_foco(KeyCode::Left));
        assert_eq!(paneles.enfocado, 2);
        assert!(paneles.mover_foco(KeyCode::Up));
        assert_eq!(paneles.enfocado, 0);
        paneles.ampliado = true;
        assert!(!paneles.mover_foco(KeyCode::Right));
    }
}
