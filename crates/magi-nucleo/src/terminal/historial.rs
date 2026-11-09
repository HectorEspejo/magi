//! Historial del terminal: captura en tramos, búsqueda, selección y copia
//! (Fase 15).
//!
//! El historial vive en el servidor (el de vt100) y el cliente solo lo ve en
//! capturas temporales. Cada línea capturada lleva sus tramos de estilo y si
//! siguió en la siguiente por el ancho (`envuelta`), para poder unir las
//! líneas lógicas al buscar y al copiar.

use serde::{Deserialize, Serialize};

use crate::terminal::emulador::PantallaEmulada;
use crate::terminal::tramos::Tramo;

/// Bytes que ocupa cada celda de la pantalla de vt100 0.16. La prueba
/// `terminal_historial` lo mide y falla si cambia.
pub const BYTES_POR_CELDA: usize = 40;

/// Número de líneas de historial que caben con los topes configurados.
#[must_use]
pub fn lineas_de_historial(historial_lineas: usize, historial_mib: u64, columnas: u16) -> usize {
    if historial_lineas == 0 {
        return 0;
    }
    let por_bytes =
        (historial_mib as usize * 1024 * 1024) / (usize::from(columnas.max(2)) * BYTES_POR_CELDA);
    historial_lineas.min(por_bytes)
}

/// Una fila capturada del historial o de la pantalla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineaHistorial {
    pub tramos: Vec<Tramo>,
    /// La línea siguió en la siguiente por el ancho del terminal.
    pub envuelta: bool,
}

impl LineaHistorial {
    /// Texto de la fila sin espacios finales.
    #[must_use]
    pub fn texto(&self) -> String {
        let mut texto = String::new();
        for tramo in &self.tramos {
            texto.push_str(&tramo.texto);
        }
        texto.trim_end().to_string()
    }
}

/// Captura de una pestaña: historial y pantalla principal, en filas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captura {
    pub lineas: Vec<LineaHistorial>,
    /// Filas de historial que hay al principio de `lineas`.
    pub historial: usize,
}

/// Captura el historial y la pantalla principal de una pestaña.
pub fn capturar(pantalla: &mut PantallaEmulada) -> Captura {
    unimplemented!("historial: lo implementa la pista del núcleo")
}

/// Cómo se interpreta el patrón de búsqueda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaseBusqueda {
    Texto,
    Expresion,
}

/// Una coincidencia: línea lógica y rango de caracteres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coincidencia {
    pub linea: usize,
    pub inicio: usize,
    pub fin: usize,
}

/// Por qué no se pudo buscar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorBusqueda {
    Vacio,
    DemasiadoLargo,
    Invalida(String),
}

/// Busca el patrón desde `desde` hacia arriba. La búsqueda es insensible a
/// mayúsculas salvo que el patrón tenga alguna (como `less -i`), y trabaja
/// sobre líneas lógicas.
pub fn buscar(
    lineas: &[LineaHistorial],
    patron: &str,
    clase: ClaseBusqueda,
    desde: usize,
) -> Result<Vec<Coincidencia>, ErrorBusqueda> {
    let _ = (lineas, patron, clase, desde);
    unimplemented!("historial: lo implementa la pista del núcleo")
}

/// Texto de una selección entre dos celdas (línea, columna), uniendo las
/// líneas envueltas y sin espacios finales.
#[must_use]
pub fn texto_seleccion(
    lineas: &[LineaHistorial],
    inicio: (usize, usize),
    fin: (usize, usize),
    por_lineas: bool,
) -> String {
    let _ = (lineas, inicio, fin, por_lineas);
    unimplemented!("historial: lo implementa la pista del núcleo")
}

/// Texto plano de una captura, una línea lógica por renglón.
#[must_use]
pub fn texto_plano(lineas: &[LineaHistorial]) -> String {
    let _ = lineas;
    unimplemented!("historial: lo implementa la pista del núcleo")
}
