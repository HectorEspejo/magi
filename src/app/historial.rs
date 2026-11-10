//! Modo historial de un panel (Fase 15).
//!
//! El historial vive en el servidor: al abrir el modo se pide una captura
//! (`CapturarHistorial`) que llega en lotes (`HistorialLote`) y se congela
//! aquí. La vista no se mueve sola con la salida nueva: se avisa con
//! «↓ salida nueva». La búsqueda, la selección y el texto de copia son las
//! funciones comunes de `magi_nucleo::terminal::historial`.

use magi_nucleo::terminal::historial::{
    self, ClaseBusqueda, Coincidencia, ErrorBusqueda, LineaHistorial,
};

/// Bytes que se pueden copiar de una vez; por encima, se propone guardar.
pub const COPIA_MAX: usize = 1 << 20;

/// Filas que baja o sube la rueda en el modo historial.
pub const RUEDA_FILAS: i64 = 3;

/// Una búsqueda hecha sobre la captura.
#[derive(Debug, Clone)]
pub struct Busqueda {
    pub patron: String,
    pub clase: ClaseBusqueda,
    pub coincidencias: Vec<Coincidencia>,
    /// Índice de la coincidencia actual dentro de `coincidencias`.
    pub actual: Option<usize>,
    /// Por qué no se pudo buscar (expresión inválida, patrón vacío…).
    pub error: Option<String>,
}

impl Default for Busqueda {
    fn default() -> Self {
        Self {
            patron: String::new(),
            clase: ClaseBusqueda::Texto,
            coincidencias: Vec::new(),
            actual: None,
            error: None,
        }
    }
}

/// Una selección del modo historial, en coordenadas de la captura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seleccion {
    pub inicio: (usize, usize),
    pub fin: (usize, usize),
    pub por_lineas: bool,
}

/// Estado del modo historial de un panel.
pub struct EstadoHistorial {
    pub sesion_id: u32,
    pub peticion_id: u64,
    pub captura: Vec<LineaHistorial>,
    pub total: u32,
    pub lote: u32,
    /// Aún no ha llegado el último lote.
    pub esperando: bool,
    pub busqueda: Option<Busqueda>,
    /// Se está escribiendo el patrón de búsqueda.
    pub editando: bool,
    pub seleccion: Option<Seleccion>,
    /// Fila de la captura donde está la vista.
    pub cursor: usize,
    /// Primera fila visible; la fija el pintado al desplazar.
    pub vista: usize,
    /// Filas que caben en el último pintado.
    pub alto: usize,
    /// Llegó salida nueva desde la captura.
    pub nueva_salida: bool,
    /// Filas que se desplaza la vista al llegar la captura (rueda del ratón).
    pub desplazamiento_inicial: i64,
}

impl EstadoHistorial {
    #[must_use]
    pub fn nueva(sesion_id: u32, peticion_id: u64) -> Self {
        Self {
            sesion_id,
            peticion_id,
            captura: Vec::new(),
            total: 0,
            lote: 0,
            esperando: true,
            busqueda: None,
            editando: false,
            seleccion: None,
            cursor: 0,
            vista: 0,
            alto: 1,
            nueva_salida: false,
            desplazamiento_inicial: 0,
        }
    }

    /// Una captura ya hecha (para los enlaces de lo visible).
    #[must_use]
    pub fn con_captura(sesion_id: u32, captura: Vec<LineaHistorial>) -> Self {
        let mut estado = Self::nueva(sesion_id, 0);
        estado.total = u32::try_from(captura.len()).unwrap_or(u32::MAX);
        estado.captura = captura;
        estado.esperando = false;
        estado.al_final();
        estado
    }

    /// Añade un lote. Devuelve si era el último.
    pub fn apilar_lote(
        &mut self,
        lote: u32,
        ultimo: bool,
        total: u32,
        lineas: Vec<LineaHistorial>,
    ) -> bool {
        // Fuera de orden o repetido: se descarta (una captura es una sola).
        if lote <= self.lote && !self.captura.is_empty() {
            return ultimo;
        }
        self.lote = lote;
        self.total = total;
        self.captura.extend(lineas);
        self.esperando = !ultimo;
        if ultimo {
            self.al_final();
        }
        ultimo
    }

    /// ¿La captura está lista?
    #[must_use]
    pub fn lista(&self) -> bool {
        !self.esperando && !self.captura.is_empty()
    }

    /// Filas de la captura.
    #[must_use]
    pub fn filas(&self) -> usize {
        self.captura.len()
    }

    /// Primera fila visible.
    #[must_use]
    pub fn primera(&self) -> usize {
        self.vista.min(self.captura.len().saturating_sub(1))
    }

    /// Filas visibles con el alto del último pintado.
    #[must_use]
    pub fn visibles(&self) -> std::ops::Range<usize> {
        let primera = self.primera();
        let fin = (primera + self.alto.max(1)).min(self.captura.len());
        primera..fin
    }

    /// Deja el cursor a la vista.
    pub fn ajustar_vista(&mut self) {
        let alto = self.alto.max(1);
        if self.cursor < self.vista {
            self.vista = self.cursor;
        } else if self.cursor >= self.vista + alto {
            self.vista = self.cursor + 1 - alto;
        }
        let maximo = self.captura.len().saturating_sub(alto);
        if self.vista > maximo {
            self.vista = maximo;
        }
    }

    /// Mueve el cursor `delta` filas.
    pub fn mover(&mut self, delta: i64) {
        if self.captura.is_empty() {
            return;
        }
        let ultima = self.captura.len() - 1;
        self.cursor = (self.cursor as i64 + delta).clamp(0, ultima as i64) as usize;
        self.ajustar_vista();
        self.saltar_a_coincidencia_cercana();
    }

    /// Una página (o media, con `Ctrl+U`/`Ctrl+D`).
    pub fn pagina(&mut self, delta: i64, media: bool) {
        let paso = i64::try_from(self.alto.max(1)).unwrap_or(1);
        self.mover(delta * if media { (paso / 2).max(1) } else { paso });
    }

    /// Principio de la captura.
    pub fn principio(&mut self) {
        self.cursor = 0;
        self.vista = 0;
    }

    /// Final de la captura: la salida más reciente.
    pub fn al_final(&mut self) {
        if self.captura.is_empty() {
            self.cursor = 0;
            self.vista = 0;
            return;
        }
        self.cursor = self.captura.len() - 1;
        self.vista = self.captura.len().saturating_sub(self.alto.max(1));
    }

    /// Al mover el cursor, si está sobre una coincidencia, se convierte en la
    /// actual (así la barra dice cuál se está mirando).
    fn saltar_a_coincidencia_cercana(&mut self) {
        let Some(busqueda) = &mut self.busqueda else {
            return;
        };
        let cercana = busqueda
            .coincidencias
            .iter()
            .enumerate()
            .filter(|(_, coincidencia)| coincidencia.linea <= self.cursor)
            .max_by_key(|(_, coincidencia)| coincidencia.linea)
            .map(|(indice, _)| indice);
        if let Some(indice) = cercana {
            busqueda.actual = Some(indice);
        }
    }

    /// Pega texto en el patrón de búsqueda que se está escribiendo: los
    /// controles del pegado se juntan y se respeta el tope de 256 caracteres.
    pub fn pegar_en_patron(&mut self, texto: &str) {
        let busqueda = self.busqueda.get_or_insert_with(Busqueda::default);
        for caracter in texto.chars().filter(|caracter| !caracter.is_control()) {
            if busqueda.patron.chars().count() >= 256 {
                break;
            }
            busqueda.patron.push(caracter);
        }
        busqueda.error = None;
    }

    /// Ejecuta la búsqueda desde la vista hacia arriba.
    pub fn buscar(&mut self) {
        let Some(busqueda) = &mut self.busqueda else {
            return;
        };
        match historial::buscar(&self.captura, &busqueda.patron, busqueda.clase, self.cursor) {
            Ok(coincidencias) => {
                busqueda.error = None;
                busqueda.actual = (!coincidencias.is_empty()).then_some(0);
                busqueda.coincidencias = coincidencias;
                self.ir_a_actual();
            }
            Err(error) => {
                busqueda.error = Some(texto_error(&error));
                busqueda.coincidencias.clear();
                busqueda.actual = None;
            }
        }
    }

    /// Pone la vista en la coincidencia actual.
    fn ir_a_actual(&mut self) {
        let Some(busqueda) = &self.busqueda else {
            return;
        };
        let Some(actual) = busqueda.actual else {
            return;
        };
        if let Some(coincidencia) = busqueda.coincidencias.get(actual) {
            self.cursor = coincidencia.linea;
            let alto = self.alto.max(1);
            self.vista = self.cursor.saturating_sub(alto / 2);
            self.ajustar_vista();
        }
    }

    /// Siguiente coincidencia (`hacia_arriba`) o anterior.
    pub fn siguiente_coincidencia(&mut self, hacia_arriba: bool) {
        let Some(busqueda) = &mut self.busqueda else {
            return;
        };
        if busqueda.coincidencias.is_empty() {
            return;
        }
        let total = busqueda.coincidencias.len();
        let actual = busqueda.actual.unwrap_or(0);
        let siguiente = if hacia_arriba {
            (actual + 1) % total
        } else {
            (actual + total - 1) % total
        };
        busqueda.actual = Some(siguiente);
        self.ir_a_actual();
    }

    /// ¿Qué coincidencias tocan esta fila? (la actual, si alguna, y si hay)
    #[must_use]
    pub fn coincidencias_en(&self, fila: usize) -> (bool, bool) {
        let Some(busqueda) = &self.busqueda else {
            return (false, false);
        };
        let mut hay = false;
        let mut es_actual = false;
        for (indice, coincidencia) in busqueda.coincidencias.iter().enumerate() {
            if coincidencia.linea == fila {
                hay = true;
                if busqueda.actual == Some(indice) {
                    es_actual = true;
                }
            }
        }
        (hay, es_actual)
    }

    /// Empieza una selección en el cursor (por caracteres o por líneas).
    pub fn empezar_seleccion(&mut self, por_lineas: bool) {
        let columna = 0;
        let inicio = (self.cursor, columna);
        self.seleccion = Some(Seleccion {
            inicio,
            fin: inicio,
            por_lineas,
        });
    }

    /// Extiende la selección hasta (`fila`, `columna`).
    pub fn extender_seleccion(&mut self, fila: usize, columna: usize) {
        if let Some(seleccion) = &mut self.seleccion {
            seleccion.fin = (fila.min(self.captura.len().saturating_sub(1)), columna);
        }
    }

    /// ¿La selección es por líneas?
    #[must_use]
    pub fn seleccion_por_lineas(&self) -> bool {
        self.seleccion.is_some_and(|seleccion| seleccion.por_lineas)
    }

    /// Texto de la selección, uniendo las líneas envueltas.
    #[must_use]
    pub fn texto_seleccion(&self) -> Option<String> {
        let seleccion = self.seleccion?;
        Some(historial::texto_seleccion(
            &self.captura,
            seleccion.inicio,
            seleccion.fin,
            seleccion.por_lineas,
        ))
    }

    /// Texto plano de la captura entera, para guardar.
    #[must_use]
    pub fn texto_plano(&self) -> String {
        historial::texto_plano(&self.captura)
    }

    /// Texto que se copia: la selección o, sin ella, todo lo visible.
    #[must_use]
    pub fn texto_a_copiar(&self) -> String {
        if let Some(texto) = self.texto_seleccion() {
            if !texto.is_empty() {
                return texto;
            }
        }
        let mut texto = String::new();
        for linea in &self.captura[self.visibles()] {
            texto.push_str(&linea.texto());
            texto.push('\n');
        }
        texto.trim_end().to_string()
    }

    /// ¿Cabe en el tope de copia?
    #[must_use]
    pub fn cabe_en_copia(texto: &str) -> bool {
        texto.len() <= COPIA_MAX
    }
}

fn texto_error(error: &ErrorBusqueda) -> String {
    match error {
        ErrorBusqueda::Vacio => "patrón vacío".to_string(),
        ErrorBusqueda::DemasiadoLargo => "el patrón es demasiado largo (256)".to_string(),
        ErrorBusqueda::Invalida(motivo) => format!("expresión no válida: {motivo}"),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
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

    fn estado(lineas: Vec<LineaHistorial>) -> EstadoHistorial {
        let mut estado = EstadoHistorial::nueva(1, 1);
        let total = u32::try_from(lineas.len()).unwrap();
        estado.apilar_lote(0, true, total, lineas);
        estado
    }

    #[test]
    fn una_captura_llega_en_lotes_y_se_queda_al_final() {
        let mut estado = EstadoHistorial::nueva(1, 1);
        assert!(!estado.lista());
        estado.apilar_lote(0, false, 3, vec![linea("a", false), linea("b", false)]);
        assert!(!estado.lista());
        estado.alto = 2;
        estado.apilar_lote(1, true, 3, vec![linea("c", false)]);
        assert!(estado.lista());
        assert_eq!(estado.filas(), 3);
        assert_eq!(estado.primera(), 1);
        assert_eq!(estado.cursor, 2);
    }

    #[test]
    fn la_busqueda_va_hacia_arriba_y_se_recorre_con_n_y_n_mayuscula() {
        let mut estado = estado(vec![
            linea("hola", false),
            linea("error uno", false),
            linea("nada", false),
            linea("error dos", false),
        ]);
        estado.alto = 4;
        estado.busqueda = Some(Busqueda {
            patron: "error".to_string(),
            ..Busqueda::default()
        });
        estado.buscar();
        let busqueda = estado.busqueda.as_ref().unwrap();
        assert_eq!(busqueda.coincidencias.len(), 2);
        assert_eq!(busqueda.actual, Some(0));
        assert_eq!(estado.cursor, 3);
        estado.siguiente_coincidencia(true);
        assert_eq!(estado.cursor, 1);
        estado.siguiente_coincidencia(false);
        assert_eq!(estado.cursor, 3);
    }

    #[test]
    fn una_expresion_invalida_se_dice_sin_buscar() {
        let mut estado = estado(vec![linea("hola", false)]);
        estado.busqueda = Some(Busqueda {
            patron: "(".to_string(),
            clase: ClaseBusqueda::Expresion,
            ..Busqueda::default()
        });
        estado.buscar();
        let busqueda = estado.busqueda.as_ref().unwrap();
        assert!(busqueda.error.is_some());
        assert!(busqueda.coincidencias.is_empty());
    }

    #[test]
    fn la_seleccion_por_lineas_coge_la_ultima_entera() {
        let mut estado = estado(vec![
            linea("uno", false),
            linea("dos", true),
            linea("tres", false),
        ]);
        estado.principio();
        estado.empezar_seleccion(true);
        estado.extender_seleccion(2, 1);
        assert_eq!(estado.texto_seleccion().unwrap(), "uno\ndostres");
    }

    #[test]
    fn la_seleccion_por_caracteres_y_el_tope_de_copia() {
        let mut estado = estado(vec![linea("abcdef", false), linea("ghij", false)]);
        estado.principio();
        estado.empezar_seleccion(false);
        // La primera columna de la selección se extiende desde 0.
        estado.extender_seleccion(0, 2);
        assert_eq!(estado.texto_seleccion().unwrap(), "abc");
        assert!(EstadoHistorial::cabe_en_copia("corto"));
        assert!(!EstadoHistorial::cabe_en_copia(&"x".repeat(COPIA_MAX + 1)));
    }
}
