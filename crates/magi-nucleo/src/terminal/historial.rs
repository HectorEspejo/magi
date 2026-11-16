//! Historial del terminal: captura en tramos, búsqueda, selección y copia
//! (Fase 15).
//!
//! El historial vive en el servidor (el de vt100) y el cliente solo lo ve en
//! capturas temporales. Cada línea capturada lleva sus tramos de estilo y si
//! siguió en la siguiente por el ancho (`envuelta`), para poder unir las
//! líneas lógicas al buscar y al copiar.

use serde::{Deserialize, Serialize};

use crate::terminal::emulador::PantallaEmulada;
use crate::terminal::tramos::{tramos_de_fila, Tramo};

/// Bytes que ocupa cada celda de la pantalla de vt100 0.16. La prueba
/// `terminal_historial` lo mide y falla si cambia.
pub const BYTES_POR_CELDA: usize = 40;

/// Longitud máxima (en caracteres) de un patrón de búsqueda.
const PATRON_MAX: usize = 256;

/// Coincidencias que se devuelven como mucho en una búsqueda.
const TOPE_COINCIDENCIAS: usize = 10_000;

/// Número de líneas de historial que caben con los topes configurados.
///
/// Manda el tope de memoria: con los valores por defecto (10 000 líneas y
/// 64 MiB) caben 10 000 líneas hasta 167 columnas, 8 388 con 200 y 5 592 con
/// 300. Con `historial_lineas` a 0 no hay historial.
#[must_use]
pub fn lineas_de_historial(historial_lineas: usize, historial_mib: u64, columnas: u16) -> usize {
    if historial_lineas == 0 {
        return 0;
    }
    let bytes = usize::try_from(historial_mib)
        .unwrap_or(usize::MAX)
        .saturating_mul(1024 * 1024);
    let por_bytes = bytes / (usize::from(columnas.max(2)) * BYTES_POR_CELDA);
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
///
/// Las primeras `captura.historial` filas son el historial de vt100 (de la más
/// vieja a la más nueva) y las siguientes, las filas de la pantalla tal como
/// están ahora. La pantalla alternativa no tiene historial: si el remoto está
/// en ella, la captura solo lleva sus filas visibles. Deja la vista al final,
/// sin desplazamiento.
pub fn capturar(pantalla: &mut PantallaEmulada) -> Captura {
    let (filas, columnas) = pantalla.screen().size();
    let total_historial = pantalla.lineas_historial();
    let mut lineas = Vec::with_capacity(total_historial + usize::from(filas));
    // Cada fila del historial se ve como la primera de la pantalla con su
    // desplazamiento: vt100 no deja leer el historial de otra forma.
    for indice in 0..total_historial {
        pantalla.desplazar_historial(total_historial - indice);
        let pantalla_vt100 = pantalla.screen();
        lineas.push(LineaHistorial {
            tramos: tramos_de_fila(pantalla_vt100, 0, columnas),
            envuelta: fila_envuelta(pantalla_vt100, 0),
        });
    }
    pantalla.desplazar_historial(0);
    let pantalla_vt100 = pantalla.screen();
    let total_lineas = lineas.len() + usize::from(filas);
    for fila in 0..filas {
        // La última fila capturada no puede continuar en ninguna otra.
        let es_ultima = lineas.len() + 1 == total_lineas;
        let envuelta = !es_ultima && fila_envuelta(pantalla_vt100, fila);
        lineas.push(LineaHistorial {
            tramos: tramos_de_fila(pantalla_vt100, fila, columnas),
            envuelta,
        });
    }
    let descartadas = acotar_captura(&mut lineas);
    let historial = total_historial.saturating_sub(descartadas.min(total_historial));
    Captura { lineas, historial }
}

/// Tope de tramos por fila de una captura: con un cambio de estilo por celda
/// el remoto podría generar cientos de MB de JSON.
const TRAMOS_POR_FILA: usize = 64;

/// Tope aproximado de bytes de una captura completa (texto y estilos).
const BYTES_CAPTURA_MAX: usize = 8 * 1024 * 1024;

/// Bytes aproximados de una fila (texto más un coste fijo por tramo).
fn bytes_de_fila(linea: &LineaHistorial) -> usize {
    linea
        .tramos
        .iter()
        .map(|tramo| tramo.texto.len() + 24)
        .sum()
}

/// Acota la captura: como mucho `TRAMOS_POR_FILA` tramos por fila (lo que
/// sobra se funde en el último) y `BYTES_CAPTURA_MAX` en total, descartando
/// las filas más viejas si hace falta. Devuelve cuántas filas del principio se
/// descartaron.
fn acotar_captura(lineas: &mut Vec<LineaHistorial>) -> usize {
    for linea in lineas.iter_mut() {
        if linea.tramos.len() <= TRAMOS_POR_FILA {
            continue;
        }
        let resto = linea.tramos.split_off(TRAMOS_POR_FILA);
        let mut texto = String::new();
        for tramo in resto {
            texto.push_str(&tramo.texto);
        }
        if let Some(ultimo) = linea.tramos.last_mut() {
            ultimo.texto.push_str(&texto);
        }
    }
    let mut total: usize = lineas.iter().map(bytes_de_fila).sum();
    let mut recortar = 0;
    while total > BYTES_CAPTURA_MAX && recortar < lineas.len() {
        total = total.saturating_sub(bytes_de_fila(&lineas[recortar]));
        recortar += 1;
    }
    if recortar > 0 {
        lineas.drain(..recortar);
    }
    recortar
}

/// ¿La fila visible `fila` continúa en la siguiente por el ancho?
///
/// `contents_between` termina en salto de línea cuando la fila no está
/// envuelta; la última fila de la pantalla también lo hace, porque el
/// recorrido se acaba antes de llegar a la fila siguiente.
fn fila_envuelta(pantalla: &vt100::Screen, fila: u16) -> bool {
    !pantalla
        .contents_between(fila, 0, fila.saturating_add(1), 0)
        .ends_with('\n')
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
    /// Fila de la captura en la que empieza la línea lógica coincidente.
    pub linea: usize,
    /// Primer carácter de la coincidencia dentro de la línea lógica.
    pub inicio: usize,
    /// Carácter siguiente al último de la coincidencia.
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
///
/// `desde` es la fila de la captura donde está la vista (se acota a la
/// captura); si cae dentro de una línea envuelta, se busca la línea lógica
/// entera. Las coincidencias llegan de la más cercana a `desde` a la más vieja
/// y, dentro de una línea, de izquierda a derecha. `Coincidencia::linea` es la
/// fila donde empieza la línea lógica y `inicio`/`fin` cuentan caracteres
/// dentro de ella. Se devuelven como mucho 10 000 coincidencias.
pub fn buscar(
    lineas: &[LineaHistorial],
    patron: &str,
    clase: ClaseBusqueda,
    desde: usize,
) -> Result<Vec<Coincidencia>, ErrorBusqueda> {
    let caracteres = patron.chars().count();
    if caracteres == 0 {
        return Err(ErrorBusqueda::Vacio);
    }
    if caracteres > PATRON_MAX {
        return Err(ErrorBusqueda::DemasiadoLargo);
    }
    let sensible = patron.chars().any(char::is_uppercase);
    let expresion = match clase {
        ClaseBusqueda::Texto => regex::escape(patron),
        ClaseBusqueda::Expresion => patron.to_string(),
    };
    let regex = regex::RegexBuilder::new(&expresion)
        .case_insensitive(!sensible)
        .size_limit(1 << 20)
        .build()
        .map_err(|error| ErrorBusqueda::Invalida(texto_de_error(&error)))?;
    let mut coincidencias = Vec::new();
    if lineas.is_empty() {
        return Ok(coincidencias);
    }
    let desde = desde.min(lineas.len() - 1);
    // Si `desde` cae dentro de una línea envuelta, se empieza por su principio.
    let mut fila = desde;
    while fila > 0 && lineas[fila - 1].envuelta {
        fila -= 1;
    }
    loop {
        let texto = texto_de_linea_logica(lineas, fila);
        for encontrada in regex.find_iter(&texto) {
            let inicio = texto[..encontrada.start()].chars().count();
            let fin = inicio + texto[encontrada.start()..encontrada.end()].chars().count();
            coincidencias.push(Coincidencia {
                linea: fila,
                inicio,
                fin,
            });
            if coincidencias.len() >= TOPE_COINCIDENCIAS {
                return Ok(coincidencias);
            }
        }
        if fila == 0 {
            break;
        }
        // La fila anterior es el final de la línea lógica anterior.
        fila -= 1;
        while fila > 0 && lineas[fila - 1].envuelta {
            fila -= 1;
        }
    }
    Ok(coincidencias)
}

/// Mensaje legible y de una sola línea para una expresión regular inválida.
fn texto_de_error(error: &regex::Error) -> String {
    error
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fila siguiente a la última de la línea lógica que empieza en `inicio`.
fn fin_de_linea_logica(lineas: &[LineaHistorial], inicio: usize) -> usize {
    let mut fin = inicio + 1;
    while fin < lineas.len() && lineas[fin - 1].envuelta {
        fin += 1;
    }
    fin
}

/// Texto de la línea lógica que empieza en `inicio`, con las envueltas unidas.
fn texto_de_linea_logica(lineas: &[LineaHistorial], inicio: usize) -> String {
    let fin = fin_de_linea_logica(lineas, inicio);
    let mut texto = String::new();
    for linea in &lineas[inicio..fin] {
        texto.push_str(&linea.texto());
    }
    texto
}

/// Texto de una selección entre dos celdas (línea, columna), uniendo las
/// líneas envueltas y sin espacios finales.
///
/// Las columnas cuentan caracteres dentro del texto de la fila. Si la
/// selección viene del revés, se ordena; con `por_lineas` se cogen las filas
/// enteras (también la última, aunque `fin` caiga a mitad).
#[must_use]
pub fn texto_seleccion(
    lineas: &[LineaHistorial],
    inicio: (usize, usize),
    fin: (usize, usize),
    por_lineas: bool,
) -> String {
    if lineas.is_empty() {
        return String::new();
    }
    let (inicio, fin) = if inicio <= fin {
        (inicio, fin)
    } else {
        (fin, inicio)
    };
    let ultima = lineas.len() - 1;
    if inicio.0 > ultima {
        return String::new();
    }
    let fin_linea = fin.0.min(ultima);
    let mut texto = String::new();
    let seleccionadas = lineas.iter().enumerate().take(fin_linea + 1).skip(inicio.0);
    for (indice, linea) in seleccionadas {
        let fila = linea.texto();
        let caracteres = fila.chars().count();
        let desde = if por_lineas || indice > inicio.0 {
            0
        } else {
            inicio.1.min(caracteres)
        };
        let hasta = if por_lineas || indice < fin.0 {
            caracteres
        } else {
            fin.1.saturating_add(1).min(caracteres)
        };
        let trozo: String = if hasta > desde {
            fila.chars().skip(desde).take(hasta - desde).collect()
        } else {
            String::new()
        };
        // Las envueltas se unen con la siguiente salvo la última seleccionada.
        if linea.envuelta && indice < fin_linea {
            texto.push_str(&trozo);
        } else {
            texto.push_str(trozo.trim_end());
            texto.push('\n');
        }
    }
    while texto.ends_with('\n') {
        texto.pop();
    }
    texto
}

/// Texto plano de una captura, una línea lógica por renglón.
///
/// Une las líneas envueltas, quita los espacios finales de cada renglón y
/// termina cada uno en salto de línea.
#[must_use]
pub fn texto_plano(lineas: &[LineaHistorial]) -> String {
    let mut texto = String::new();
    let mut indice = 0;
    while indice < lineas.len() {
        let mut renglon = String::new();
        loop {
            renglon.push_str(&lineas[indice].texto());
            let envuelta = lineas[indice].envuelta;
            indice += 1;
            if !envuelta || indice >= lineas.len() {
                break;
            }
        }
        texto.push_str(renglon.trim_end());
        texto.push('\n');
    }
    texto
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_captura_con_un_estilo_por_celda_queda_acotada() {
        let mut pantalla = PantallaEmulada::nueva(24, 200, 1000, false);
        for fila in 0..24u16 {
            for columna in 0..200u16 {
                let rojo = (fila * 7) as u8;
                let verde = (columna * 3) as u8;
                let azul = (fila + columna) as u8;
                pantalla.procesar(format!("\x1b[38;2;{rojo};{verde};{azul}mX\x1b[0m").as_bytes());
            }
            pantalla.procesar(b"\r\n");
        }
        let captura = capturar(&mut pantalla);
        assert!(captura.historial > 0, "la prueba necesita historial");
        for linea in &captura.lineas {
            assert!(
                linea.tramos.len() <= TRAMOS_POR_FILA,
                "una fila lleva {} tramos",
                linea.tramos.len()
            );
        }
        let bytes: usize = captura.lineas.iter().map(bytes_de_fila).sum();
        assert!(
            bytes <= BYTES_CAPTURA_MAX,
            "la captura pasa del tope: {bytes} bytes"
        );
        assert!(
            bytes < 1_048_576,
            "con 24 filas de estilos la captura debería quedar muy por debajo del tope: {bytes}"
        );
    }
}
