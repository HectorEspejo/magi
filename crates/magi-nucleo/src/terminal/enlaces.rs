//! Enlaces http y https en las líneas del terminal (Fase 15).
//!
//! Los enlaces se reconocen sobre líneas lógicas (las que se partieron por el
//! ancho se unen), sin la puntuación final. Solo `http` y `https`: lo demás no
//! es un enlace abrible. Los que apuntan a localhost del host se pueden abrir
//! por un túnel temporal, con la URL reescrita al puerto local.

/// Un enlace encontrado: línea lógica, columna y URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enlace {
    /// Índice de la línea lógica en la captura.
    pub linea: usize,
    /// Columna (en caracteres) donde empieza dentro de la línea.
    pub columna: usize,
    pub url: String,
}

/// Busca enlaces en las líneas lógicas (índice, texto).
#[must_use]
pub fn enlaces(lineas: &[(usize, String)]) -> Vec<Enlace> {
    unimplemented!("enlaces: lo implementa la pista del núcleo")
}

/// Pone letras de la `a` a la `z` a los 26 enlaces más cercanos al final.
#[must_use]
pub fn con_letras(enlaces: &[Enlace]) -> Vec<(char, Enlace)> {
    unimplemented!("enlaces: lo implementa la pista del núcleo")
}

/// ¿La URL es http o https?
#[must_use]
pub fn es_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Destino del túnel temporal si la URL apunta a localhost del host:
/// `localhost`, `127.0.0.1`, `::1` o `0.0.0.0`, con su puerto (o el de su
/// esquema).
#[must_use]
pub fn destino_tunel(url: &str) -> Option<(String, u16)> {
    unimplemented!("enlaces: lo implementa la pista del núcleo")
}

/// La misma URL con el puerto local del túnel: esquema, ruta, consulta y
/// fragmento no cambian; el host pasa a `127.0.0.1`.
#[must_use]
pub fn url_por_tunel(url: &str, puerto_local: u16) -> Option<String> {
    unimplemented!("enlaces: lo implementa la pista del núcleo")
}
