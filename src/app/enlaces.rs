//! Modo enlaces y diálogo del enlace (Fase 15).
//!
//! Los enlaces se reconocen con `magi_nucleo::terminal::enlaces` sobre las
//! líneas lógicas de lo visible (las envueltas se unen). Se marcan con letras
//! de la `a` a la `z` los 26 más cercanos al final; al elegir uno se enseña
//! la URL entera antes de abrirlo. Solo `http` y `https`, y nunca por un
//! shell. Los que apuntan a localhost del host se abren por un túnel temporal.

use magi_nucleo::terminal::enlaces::{self, Enlace};

/// Estado del modo enlaces de un panel.
#[derive(Debug, Clone, Default)]
pub struct EstadoEnlaces {
    /// Se están enseñando las letras sobre lo visible.
    pub activo: bool,
    /// Los enlaces visibles con su letra.
    pub letras: Vec<(char, Enlace)>,
    /// El enlace elegido: su diálogo manda sobre las letras.
    pub elegido: Option<Enlace>,
}

impl EstadoEnlaces {
    /// Abre el modo con lo visible de una captura (sus filas).
    pub fn abrir(&mut self, lineas: &[(usize, String)]) {
        let encontrados = enlaces::enlaces(lineas);
        self.letras = enlaces::con_letras(&encontrados);
        self.activo = !self.letras.is_empty();
        self.elegido = None;
    }

    /// Cierra el modo y su diálogo.
    pub fn cerrar(&mut self) {
        self.activo = false;
        self.letras.clear();
        self.elegido = None;
    }

    /// El enlace de una letra, si la hay.
    #[must_use]
    pub fn por_letra(&self, letra: char) -> Option<Enlace> {
        self.letras
            .iter()
            .find(|(otra, _)| *otra == letra)
            .map(|(_, enlace)| enlace.clone())
    }
}

/// Datos del diálogo del enlace: se fijan al abrirlo y no se releen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogoEnlace {
    pub url: String,
    /// Host de la pestaña desde la que se abrió, para el texto del diálogo.
    pub host_nombre: String,
    /// Destino y puerto del túnel temporal, si apunta a localhost del host.
    pub tunel: Option<(String, u16)>,
    /// Petición del túnel en vuelo, si se pidió.
    pub peticion_id: Option<u64>,
}

impl DialogoEnlace {
    #[must_use]
    pub fn nuevo(url: String) -> Self {
        let tunel = enlaces::destino_tunel(&url);
        Self {
            url,
            host_nombre: String::new(),
            tunel,
            peticion_id: None,
        }
    }

    /// Fija el host de la pestaña (para el texto del diálogo).
    #[must_use]
    pub fn con_host(mut self, host_nombre: &str) -> Self {
        self.host_nombre = host_nombre.to_string();
        self
    }

    /// El host de la URL, para resaltarlo en el diálogo.
    #[must_use]
    pub fn host(&self) -> String {
        let sin_esquema = self
            .url
            .split_once("://")
            .map(|(_, resto)| resto)
            .unwrap_or(&self.url);
        let fin = sin_esquema
            .find(['/', '?', '#'])
            .unwrap_or(sin_esquema.len());
        sin_esquema[..fin].to_string()
    }

    /// ¿Es un enlace web abrible (http o https)?
    #[must_use]
    pub fn es_web(&self) -> bool {
        enlaces::es_web(&self.url)
    }
}

/// Las líneas lógicas de una captura (para enlaces): une las envueltas.
///
/// Devuelve `(fila donde empieza, texto)` de cada línea lógica.
#[must_use]
pub fn lineas_logicas(
    captura: &[magi_nucleo::terminal::historial::LineaHistorial],
) -> Vec<(usize, String)> {
    let mut lineas = Vec::new();
    let mut indice = 0;
    while indice < captura.len() {
        let inicio = indice;
        let mut texto = String::new();
        loop {
            texto.push_str(&captura[indice].texto());
            let envuelta = captura[indice].envuelta;
            indice += 1;
            if !envuelta || indice >= captura.len() {
                break;
            }
        }
        lineas.push((inicio, texto));
    }
    lineas
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use magi_nucleo::terminal::historial::LineaHistorial;
    use magi_nucleo::terminal::tramos::{Color, Tramo};

    fn linea(texto: &str, envuelta: bool) -> LineaHistorial {
        LineaHistorial {
            tramos: vec![Tramo {
                texto: texto.to_string(),
                primer_plano: Color::Predeterminado,
                fondo: Color::Predeterminado,
                negrita: false,
                cursiva: false,
                subrayado: false,
                inverso: false,
            }],
            envuelta,
        }
    }

    #[test]
    fn una_url_partida_en_dos_lineas_se_reconoce_entera() {
        let lineas = lineas_logicas(&[
            linea("mira https://ejemplo.org/gu", true),
            linea("ia#arranque y ya", false),
        ]);
        assert_eq!(lineas.len(), 1);
        let mut estado = EstadoEnlaces::default();
        estado.abrir(&lineas);
        assert!(estado.activo);
        assert_eq!(estado.letras.len(), 1);
        assert_eq!(estado.letras[0].0, 'a');
        assert_eq!(estado.letras[0].1.url, "https://ejemplo.org/guia#arranque");
    }

    #[test]
    fn sin_enlaces_no_se_abre_el_modo() {
        let mut estado = EstadoEnlaces::default();
        estado.abrir(&[(0, "nada que ver".to_string())]);
        assert!(!estado.activo);
        assert!(estado.por_letra('a').is_none());
    }

    #[test]
    fn el_dialogo_separa_el_host_y_solo_abre_http_y_https() {
        let dialogo = DialogoEnlace::nuevo("http://localhost:8000/informe?x=1".to_string());
        assert_eq!(dialogo.host(), "localhost:8000");
        assert_eq!(dialogo.tunel, Some(("localhost".to_string(), 8000)));
        assert!(dialogo.es_web());
        assert!(DialogoEnlace::nuevo("file:///etc/passwd".to_string())
            .tunel
            .is_none());
        assert!(!DialogoEnlace::nuevo("file:///etc/passwd".to_string()).es_web());
    }
}
