//! Disposición adaptable (Fase 7): una sola tabla de puntos de corte, mínimos
//! por vista, columnas y atajos por prioridad, encaje de diálogos y ventanas
//! de lista.
//!
//! Ninguna vista guarda tamaños entre pintados: todo se deriva del área en
//! cada pintado. La única copia es la `Disposicion` del último pintado, que el
//! bucle guarda y que usan las teclas de página y de «filas visibles».

use std::borrow::Cow;

use ratatui::layout::Rect;

use crate::ui::Vista;

/// Por debajo de este ancho la vista pasa a modo estrecho (un panel, columnas
/// secundarias ocultas).
pub const ANCHO_ESTRECHO: u16 = 100;
/// Por debajo de este ancho las tablas se quedan con las columnas mínimas.
pub const ANCHO_COLUMNAS_MINIMAS: u16 = 80;
/// Por debajo de este ancho Hosts oculta usuario·puerto (o las etiquetas) y
/// Sesiones su columna de host.
pub const ANCHO_SIN_USUARIO: u16 = 60;
/// Por debajo de este alto la vista es «baja»: detalles plegados.
pub const ALTO_BAJO: u16 = 20;
/// Ventana «muy grande»: paneles, detalles y diálogos con ancho máximo.
pub const ANCHO_GRANDE: u16 = 200;
pub const ALTO_GRANDE: u16 = 60;
/// Con menos alto, la salida de Resultados solo se ve con `↵`.
pub const ALTO_SALIDA_RESULTADOS: u16 = 24;
/// Ancho máximo de un panel de detalle en ventanas muy grandes.
pub const ANCHO_MAX_DETALLE: u16 = 120;
/// Ancho máximo de un diálogo.
pub const ANCHO_MAX_DIALOGO: u16 = 100;

/// Mínimo de cualquier vista que no declare el suyo.
pub const MINIMO_GLOBAL: Tamano = Tamano::new(40, 12);
/// Por debajo de esto no cabe ni el aviso: se pinta solo `MAGI c×f`.
pub const MINIMO_AVISO: Tamano = Tamano::new(20, 3);
/// Mínimo del diálogo MAGI (deliberación).
pub const MINIMO_DELIBERACION: Tamano = Tamano::new(50, 12);

/// Un tamaño de terminal o de área, en columnas y filas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tamano {
    pub cols: u16,
    pub filas: u16,
}

impl Tamano {
    pub const fn new(cols: u16, filas: u16) -> Self {
        Self { cols, filas }
    }

    pub fn de(area: Rect) -> Self {
        Self::new(area.width, area.height)
    }

    /// ¿Cabe este mínimo en el área?
    pub fn cabe_en(self, area: Rect) -> bool {
        area.width >= self.cols && area.height >= self.filas
    }

    /// El mayor por componente.
    pub fn max(self, otro: Tamano) -> Tamano {
        Tamano::new(self.cols.max(otro.cols), self.filas.max(otro.filas))
    }

    /// `40×12` (o `40x12` en ASCII).
    pub fn texto(self, ascii: bool) -> String {
        let por = if ascii { "x" } else { "×" };
        format!("{}{por}{}", self.cols, self.filas)
    }
}

/// Modo de disposición de una vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoAncho {
    /// Paneles dobles, todas las columnas, detalles abiertos.
    #[default]
    Normal,
    /// Menos de 100 columnas: un panel con `Tab`, columnas secundarias ocultas.
    Estrecho,
    /// Por debajo del mínimo de la vista: se pinta el aviso en su lugar.
    Minimo,
}

/// Modo para un área y el mínimo de la vista.
pub fn modo(area: Rect, minimo: Tamano) -> ModoAncho {
    if !minimo.cabe_en(area) {
        ModoAncho::Minimo
    } else if es_estrecho(area.width) {
        ModoAncho::Estrecho
    } else {
        ModoAncho::Normal
    }
}

pub fn es_estrecho(ancho: u16) -> bool {
    ancho < ANCHO_ESTRECHO
}

pub fn columnas_minimas(ancho: u16) -> bool {
    ancho < ANCHO_COLUMNAS_MINIMAS
}

pub fn es_bajo(filas: u16) -> bool {
    filas < ALTO_BAJO
}

pub fn es_grande(area: Rect) -> bool {
    area.width >= ANCHO_GRANDE && area.height >= ALTO_GRANDE
}

/// Mínimo que exige lo que hay en pantalla y quién lo exige.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minimo {
    pub tamano: Tamano,
    /// Nombre de la vista o diálogo que lo exige («Archivos», «diálogo MAGI»).
    pub exige: &'static str,
}

/// Mínimo de cada vista (§7.2). El de una vista sustituye al global: Sesión
/// baja a 40×8 (decisión del desarrollador).
pub fn minimo_vista(vista: Vista) -> Tamano {
    match vista {
        Vista::Ficha | Vista::Archivos => Tamano::new(50, 14),
        Vista::Sesion => Tamano::new(40, 8),
        Vista::Transferencias
        | Vista::Tuneles
        | Vista::Registro
        | Vista::Identidades
        | Vista::Snippets => Tamano::new(50, 12),
        Vista::Resultados => Tamano::new(60, 14),
        Vista::Flota | Vista::Hosts | Vista::Sesiones => MINIMO_GLOBAL,
    }
}

pub fn nombre_vista(vista: Vista) -> &'static str {
    match vista {
        Vista::Flota => "Flota",
        Vista::Hosts => "Hosts",
        Vista::Ficha => "Ficha",
        Vista::Sesion => "Sesión",
        Vista::Sesiones => "Sesiones",
        Vista::Identidades => "Identidades",
        Vista::Registro => "Registro",
        Vista::Archivos => "Archivos",
        Vista::Transferencias => "Transferencias",
        Vista::Tuneles => "Túneles",
        Vista::Snippets => "Snippets",
        Vista::Resultados => "Resultados",
    }
}

/// Mínimo de la pantalla: el de la vista y, con la deliberación abierta, el
/// del diálogo MAGI si es mayor.
pub fn minimo_de(vista: Vista, deliberacion: bool) -> Minimo {
    let propio = minimo_vista(vista);
    if deliberacion
        && !(propio.cols >= MINIMO_DELIBERACION.cols && propio.filas >= MINIMO_DELIBERACION.filas)
    {
        return Minimo {
            tamano: propio.max(MINIMO_DELIBERACION),
            exige: "diálogo MAGI",
        };
    }
    Minimo {
        tamano: propio,
        exige: nombre_vista(vista),
    }
}

/// Mínimos de los diálogos de la Fase 8 (T45). Un diálogo encoge con
/// desplazamiento dentro de su hoja y debe verse entero a 40×12; por debajo
/// de su mínimo entra en el de la pantalla, como el diálogo MAGI.
pub const MINIMO_EDICION: Tamano = Tamano::new(40, 12);
pub const MINIMO_PERMISOS: Tamano = Tamano::new(40, 12);
pub const MINIMO_SINCRONIZAR: Tamano = Tamano::new(40, 12);
pub const MINIMO_VISTA_PREVIA: Tamano = Tamano::new(40, 12);
pub const MINIMO_SINCRONIZACIONES: Tamano = Tamano::new(40, 12);

/// Por debajo de estos anchos, cada diálogo pasa a su modo estrecho (rótulos
/// cortos, columnas de menos).
pub const ESTRECHO_EDICION: u16 = 60;
pub const ESTRECHO_PERMISOS: u16 = 60;
pub const ESTRECHO_SINCRONIZAR: u16 = 64;
pub const ESTRECHO_VISTA_PREVIA: u16 = 80;
pub const ESTRECHO_SINCRONIZACIONES: u16 = 70;

/// Mínimo que declara el diálogo abierto, si es de los que lo declaran.
pub fn minimo_dialogo(dialogo: &crate::app::Dialogo) -> Option<Minimo> {
    use crate::app::Dialogo;
    match dialogo {
        Dialogo::Edicion(dialogo) => Some(crate::ui::edicion::minimo(dialogo)),
        Dialogo::Permisos(dialogo) => Some(crate::ui::permisos::minimo(dialogo)),
        Dialogo::Sincronizar(dialogo) => Some(crate::ui::sincronizar::minimo(dialogo)),
        Dialogo::Guardadas(dialogo) => Some(crate::ui::sincronizaciones::minimo(dialogo)),
        _ => None,
    }
}

/// El mínimo de la pantalla con el del diálogo abierto si es mayor.
pub fn con_dialogo(minimo: Minimo, dialogo: Option<&crate::app::Dialogo>) -> Minimo {
    match dialogo.and_then(minimo_dialogo) {
        Some(propio)
            if !(minimo.tamano.cols >= propio.tamano.cols
                && minimo.tamano.filas >= propio.tamano.filas) =>
        {
            Minimo {
                tamano: minimo.tamano.max(propio.tamano),
                exige: propio.exige,
            }
        }
        _ => minimo,
    }
}

// ---------------------------------------------------------------- columnas

/// Una columna de tabla con su prioridad (1 = imprescindible: nunca se oculta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columna {
    /// Ancho mínimo (sin el separador).
    pub ancho: u16,
    pub prioridad: u8,
    /// Se lleva el ancho sobrante.
    pub flexible: bool,
    /// Se oculta por debajo de este ancho de tabla aunque quepa (p. ej. la
    /// dirección de Hosts por debajo de 80). 0 = sin umbral.
    pub desde: u16,
}

impl Columna {
    pub const fn fija(ancho: u16, prioridad: u8) -> Self {
        Self {
            ancho,
            prioridad,
            flexible: false,
            desde: 0,
        }
    }

    pub const fn flexible(ancho: u16, prioridad: u8) -> Self {
        Self {
            ancho,
            prioridad,
            flexible: true,
            desde: 0,
        }
    }

    pub const fn desde(mut self, ancho: u16) -> Self {
        self.desde = ancho;
        self
    }
}

/// Qué columnas se ven en `ancho` (§7.3): se quitan las de menor prioridad
/// hasta que caben; las de prioridad 1 no se quitan nunca. Conserva el orden.
pub fn columnas_visibles(ancho: u16, columnas: &[Columna], separador: u16) -> Vec<bool> {
    let mut visibles: Vec<bool> = columnas
        .iter()
        .map(|columna| columna.prioridad <= 1 || ancho >= columna.desde)
        .collect();
    let necesario = |visibles: &[bool]| -> u32 {
        let (suma, cuantas) = columnas
            .iter()
            .zip(visibles)
            .filter(|(_, visible)| **visible)
            .fold((0u32, 0u32), |(suma, cuantas), (columna, _)| {
                (suma + u32::from(columna.ancho), cuantas + 1)
            });
        suma + cuantas.saturating_sub(1) * u32::from(separador)
    };
    while necesario(&visibles) > u32::from(ancho) {
        // La visible de menor prioridad (la última, a igualdad) que se pueda
        // quitar.
        let Some(quitar) = columnas
            .iter()
            .enumerate()
            .filter(|(indice, columna)| visibles[*indice] && columna.prioridad > 1)
            .max_by_key(|(indice, columna)| (columna.prioridad, *indice))
            .map(|(indice, _)| indice)
        else {
            break;
        };
        visibles[quitar] = false;
    }
    visibles
}

/// Ancho de cada columna visible: su mínimo más, para las flexibles, el
/// sobrante repartido. Las ocultas valen 0.
pub fn repartir(ancho: u16, columnas: &[Columna], visibles: &[bool], separador: u16) -> Vec<u16> {
    let mut anchos: Vec<u16> = columnas
        .iter()
        .zip(visibles)
        .map(|(columna, visible)| if *visible { columna.ancho } else { 0 })
        .collect();
    let cuantas = visibles.iter().filter(|visible| **visible).count() as u16;
    let usado: u16 = anchos
        .iter()
        .sum::<u16>()
        .saturating_add(cuantas.saturating_sub(1).saturating_mul(separador));
    let flexibles: Vec<usize> = columnas
        .iter()
        .enumerate()
        .filter(|(indice, columna)| visibles[*indice] && columna.flexible)
        .map(|(indice, _)| indice)
        .collect();
    let mut sobrante = ancho.saturating_sub(usado);
    if !flexibles.is_empty() {
        let parte = sobrante / flexibles.len() as u16;
        for (posicion, indice) in flexibles.iter().enumerate() {
            let extra = if posicion + 1 == flexibles.len() {
                sobrante
            } else {
                parte
            };
            anchos[*indice] += extra;
            sobrante -= extra;
        }
    }
    anchos
}

// ---------------------------------------------------------------- atajos

/// Un atajo de la barra inferior con su prioridad (1 = el más importante).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atajo {
    pub tecla: Cow<'static, str>,
    pub texto: Cow<'static, str>,
    pub prioridad: u8,
}

impl Atajo {
    pub fn new(
        tecla: impl Into<Cow<'static, str>>,
        texto: impl Into<Cow<'static, str>>,
        prioridad: u8,
    ) -> Self {
        Self {
            tecla: tecla.into(),
            texto: texto.into(),
            prioridad,
        }
    }

    /// Ancho en la barra: `tecla texto` más dos espacios de separación.
    pub fn ancho(&self) -> u16 {
        (self.tecla.chars().count() + 1 + self.texto.chars().count() + 2) as u16
    }
}

/// Ancho de «? más».
pub const ANCHO_MAS: u16 = 6;

/// Qué atajos caben en `ancho`, por prioridad y conservando el orden. Si no
/// caben todos, deja sitio para «? más» y devuelve `true`.
pub fn atajos_que_caben(atajos: &[Atajo], ancho: u16) -> (Vec<bool>, bool) {
    let total: u32 = atajos.iter().map(|atajo| u32::from(atajo.ancho())).sum();
    if total <= u32::from(ancho) {
        return (vec![true; atajos.len()], false);
    }
    let disponible = ancho.saturating_sub(ANCHO_MAS);
    let mut orden: Vec<usize> = (0..atajos.len()).collect();
    orden.sort_by_key(|indice| (atajos[*indice].prioridad, *indice));
    let mut visibles = vec![false; atajos.len()];
    let mut usado = 0u16;
    for indice in orden {
        let ancho_atajo = atajos[indice].ancho();
        if usado + ancho_atajo <= disponible {
            visibles[indice] = true;
            usado += ancho_atajo;
        }
    }
    (visibles, true)
}

// ---------------------------------------------------------------- encaje

/// Centra un rectángulo de `ancho`×`alto` en el área sin salirse nunca de
/// ella: si no cabe, encoge.
pub fn centrar_limitado(area: Rect, ancho: u16, alto: u16) -> Rect {
    let ancho = ancho.min(area.width);
    let alto = alto.min(area.height);
    Rect {
        x: area.x + (area.width - ancho) / 2,
        y: area.y + (area.height - alto) / 2,
        width: ancho,
        height: alto,
    }
}

/// Limita el ancho de un panel y lo centra (ventanas muy grandes).
pub fn limitar_ancho(area: Rect, maximo: u16) -> Rect {
    if area.width <= maximo {
        return area;
    }
    Rect {
        x: area.x + (area.width - maximo) / 2,
        width: maximo,
        ..area
    }
}

// ---------------------------------------------------------------- listas

/// Primera fila visible de una lista: parte de `ancla` (el desplazamiento
/// anterior), mantiene la selección a la vista y no deja huecos al final.
pub fn ventana(ancla: usize, seleccion: usize, filas: usize, total: usize) -> usize {
    let filas = filas.max(1);
    let mut inicio = ancla;
    if seleccion < inicio {
        inicio = seleccion;
    }
    if seleccion >= inicio + filas {
        inicio = seleccion + 1 - filas;
    }
    inicio.min(total.saturating_sub(filas))
}

/// Recorta un texto a `ancho` caracteres con `…` (`~` en ASCII).
pub fn recortar(texto: &str, ancho: usize, ascii: bool) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    if ancho == 0 {
        return String::new();
    }
    let marca = if ascii { "~" } else { "…" };
    let recortado: String = texto.chars().take(ancho - 1).collect();
    format!("{recortado}{marca}")
}

/// Texto recortado y rellenado a `ancho` exacto.
pub fn columna(texto: &str, ancho: usize, ascii: bool) -> String {
    let recortado = recortar(texto, ancho, ascii);
    let relleno = ancho.saturating_sub(recortado.chars().count());
    format!("{recortado}{}", " ".repeat(relleno))
}

/// Texto sin glifos Unicode para el modo ASCII degradado: las letras (con
/// tilde o sin ella) se quedan; cada glifo conocido pasa a su equivalente y
/// cualquier otro a `?`. Cubre los textos que llegan hechos de otras partes
/// (líneas de detalle, entradas de la paleta, teclas de la ayuda).
pub fn texto_ascii(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    for caracter in texto.chars() {
        if caracter.is_ascii() || caracter.is_alphabetic() {
            salida.push(caracter);
            continue;
        }
        let sustituto = match caracter {
            '·' | '—' | '–' | '─' | '━' => "-",
            '…' => "~",
            '×' => "x",
            '→' => "->",
            '←' => "<-",
            '⇄' | '↔' => "<->",
            '↑' => "^",
            '↓' => "v",
            '⇥' => "tab",
            '↵' | '⏎' => "enter",
            '⇧' => "shift+",
            '⌫' => "bksp",
            // Los signos de apertura se quitan: «Seguro?» se lee mejor que
            // «?Seguro?».
            '¿' | '¡' => "",
            '«' | '»' | '“' | '”' | '„' => "\"",
            '‘' | '’' => "'",
            '▸' | '▶' | '❯' | '›' | '⤴' => ">",
            '◂' | '◀' | '‹' => "<",
            '▾' | '▼' => "v",
            '▴' | '▲' | '⇅' => "^",
            '●' | '•' | '◉' => "*",
            '○' | '◐' | '◑' => "o",
            '✕' | '✗' | '✘' => "x",
            '✓' | '✔' => "+",
            '┃' | '│' | '║' | '▏' => "|",
            '≥' => ">=",
            '≤' => "<=",
            '≠' => "!=",
            '⚠' => "!",
            '░' | '▒' => ".",
            '█' | '▓' => "#",
            '\u{a0}' => " ",
            _ => "?",
        };
        salida.push_str(sustituto);
    }
    salida
}

/// El texto tal cual o, en modo ASCII, sin glifos Unicode (`texto_ascii`).
pub fn adaptar(texto: &str, ascii: bool) -> String {
    if ascii {
        texto_ascii(texto)
    } else {
        texto.to_string()
    }
}

/// Listas cuya ventana visible usan las teclas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lista {
    Hosts,
    Flota,
    Sesiones,
    Identidades,
    Registro,
    ArchivosLocal,
    ArchivosRemoto,
    Transferencias,
    Tuneles,
    Snippets,
    ResultadosEjecuciones,
    ResultadosHosts,
    VisorSalida,
    VisorErrores,
    Paleta,
    Ayuda,
    Modal,
    DeliberacionHosts,
}

/// Lo que se ve de una lista en el último pintado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VentanaLista {
    /// Primera fila visible.
    pub inicio: usize,
    /// Filas visibles.
    pub filas: usize,
    /// Filas en total.
    pub total: usize,
}

/// Disposición del último pintado. La crea `ui::dibujar` en cada pintado y el
/// bucle la guarda; las teclas de página la leen.
#[derive(Debug, Clone, Default)]
pub struct Disposicion {
    /// Área de la terminal.
    pub area: Rect,
    pub modo: ModoAncho,
    /// Alto < 20: detalles plegados.
    pub bajo: bool,
    /// Mínimo de lo que hay en pantalla.
    pub minimo: Option<Minimo>,
    /// Hay aviso de tamaño en lugar de la vista.
    pub aviso: bool,
    listas: Vec<(Lista, VentanaLista)>,
    /// Columnas de la rejilla de hosts del diálogo EJECUTAR.
    pub columnas_rejilla: Option<usize>,
}

impl Disposicion {
    pub fn nueva(area: Rect, minimo: Minimo) -> Self {
        let modo = modo(area, minimo.tamano);
        Self {
            area,
            modo,
            bajo: es_bajo(area.height),
            minimo: Some(minimo),
            aviso: modo == ModoAncho::Minimo,
            listas: Vec::new(),
            columnas_rejilla: None,
        }
    }

    pub fn estrecho(&self) -> bool {
        self.modo != ModoAncho::Normal
    }

    /// Registra la ventana de una lista (la última gana).
    pub fn registrar(&mut self, lista: Lista, ventana: VentanaLista) {
        self.listas.retain(|(otra, _)| *otra != lista);
        self.listas.push((lista, ventana));
    }

    pub fn lista(&self, lista: Lista) -> Option<VentanaLista> {
        self.listas
            .iter()
            .find(|(otra, _)| *otra == lista)
            .map(|(_, ventana)| *ventana)
    }

    /// Filas visibles de una lista (≥ 1). Si no se pintó, una estimación a
    /// partir del alto de la terminal.
    pub fn filas(&self, lista: Lista) -> usize {
        self.lista(lista)
            .map(|ventana| ventana.filas)
            .unwrap_or_else(|| usize::from(self.area.height.saturating_sub(4)))
            .max(1)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn modos_por_puntos_de_corte() {
        let minimo = MINIMO_GLOBAL;
        assert_eq!(modo(Rect::new(0, 0, 100, 30), minimo), ModoAncho::Normal);
        assert_eq!(modo(Rect::new(0, 0, 99, 30), minimo), ModoAncho::Estrecho);
        assert_eq!(modo(Rect::new(0, 0, 39, 30), minimo), ModoAncho::Minimo);
        assert_eq!(modo(Rect::new(0, 0, 60, 11), minimo), ModoAncho::Minimo);
        assert!(es_bajo(19) && !es_bajo(20));
        assert!(columnas_minimas(79) && !columnas_minimas(80));
    }

    #[test]
    fn sesion_baja_a_40x8_y_el_dialogo_magi_sube_el_minimo() {
        assert_eq!(minimo_vista(Vista::Sesion), Tamano::new(40, 8));
        assert_eq!(
            minimo_de(Vista::Archivos, false).tamano,
            Tamano::new(50, 14)
        );
        let magi = minimo_de(Vista::Snippets, true);
        assert_eq!(magi.tamano, Tamano::new(50, 12));
        let magi = minimo_de(Vista::Hosts, true);
        assert_eq!(
            (magi.tamano, magi.exige),
            (Tamano::new(50, 12), "diálogo MAGI")
        );
        let resultados = minimo_de(Vista::Resultados, true);
        assert_eq!(resultados.exige, "Resultados");
    }

    #[test]
    fn columnas_por_prioridad_conservan_el_orden_y_las_imprescindibles() {
        let columnas = [
            Columna::fija(2, 1),
            Columna::flexible(20, 1),
            Columna::fija(20, 3),
            Columna::fija(10, 2),
        ];
        assert_eq!(columnas_visibles(60, &columnas, 1), vec![true; 4]);
        assert_eq!(
            columnas_visibles(40, &columnas, 1),
            vec![true, true, false, true]
        );
        assert_eq!(
            columnas_visibles(5, &columnas, 1),
            vec![true, true, false, false]
        );
        let anchos = repartir(40, &columnas, &[true, true, false, true], 1);
        assert_eq!(anchos, vec![2, 26, 0, 10]);
    }

    #[test]
    fn columnas_con_umbral_de_ancho() {
        let columnas = [
            Columna::flexible(20, 1),
            Columna::fija(20, 2).desde(80),
            Columna::fija(10, 3).desde(60),
        ];
        assert_eq!(columnas_visibles(90, &columnas, 1), vec![true, true, true]);
        assert_eq!(columnas_visibles(79, &columnas, 1), vec![true, false, true]);
        assert_eq!(
            columnas_visibles(59, &columnas, 1),
            vec![true, false, false]
        );
    }

    #[test]
    fn atajos_por_prioridad_con_mas() {
        let atajos = [
            Atajo::new("↵", "ssh", 1),
            Atajo::new("r", "sondear", 2),
            Atajo::new("e", "editar", 3),
            Atajo::new("?", "ayuda", 1),
        ];
        let total: u16 = atajos.iter().map(Atajo::ancho).sum();
        assert_eq!(atajos_que_caben(&atajos, total), (vec![true; 4], false));
        // 34 de ancho: 28 para atajos tras «? más»; `e editar` no cabe.
        let (visibles, mas) = atajos_que_caben(&atajos, 34);
        assert!(mas);
        assert_eq!(visibles, vec![true, true, false, true]);
    }

    #[test]
    fn centrar_nunca_se_sale_del_area() {
        let area = Rect::new(0, 0, 30, 8);
        assert_eq!(centrar_limitado(area, 60, 20), area);
        assert_eq!(centrar_limitado(area, 10, 4), Rect::new(10, 2, 10, 4));
        assert_eq!(
            limitar_ancho(Rect::new(0, 0, 200, 60), 120),
            Rect::new(40, 0, 120, 60)
        );
    }

    #[test]
    fn la_ventana_mantiene_la_seleccion_y_no_deja_huecos() {
        assert_eq!(ventana(0, 30, 10, 50), 21);
        assert_eq!(ventana(25, 30, 10, 50), 25);
        assert_eq!(ventana(45, 48, 10, 50), 40, "sin hueco al final");
        assert_eq!(ventana(10, 3, 10, 50), 3);
        assert_eq!(ventana(10, 0, 10, 5), 0);
    }

    #[test]
    fn recortar_marca_el_corte() {
        assert_eq!(recortar("hetzner-01", 6, false), "hetzn…");
        assert_eq!(recortar("hetzner-01", 6, true), "hetzn~");
        assert_eq!(columna("abc", 5, false), "abc  ");
    }
}
