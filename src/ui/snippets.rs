//! Vista Snippets (F8): lista con el destino resumido y panel inferior con el
//! snippet seleccionado (comando, destinos resueltos, variables, confirmación
//! y uso).
//!
//! Nada del snippet llega a la terminal sin sanear: el comando y los defectos
//! de las variables pueden llevar cualquier carácter salvo el nulo, y una
//! secuencia de escape pintada tal cual la interpretaría la terminal.
//!
//! Disposición adaptable (Fase 7): columnas por prioridad (el nombre y la
//! marca de crítico no se ocultan; el destino resumido cae el primero) y
//! panel inferior plegado con la vista baja. Aquí `↵` ejecuta, así que el
//! detalle plegado se abre con `i`.

use std::collections::HashMap;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::deliberacion::Verificaciones;
use crate::flota::estado::{antiguedad_segundos, formatear_antiguedad};
use crate::modelo::{Grupo, Host};
use crate::snippets::salida::{sanear_linea, texto_limpio};
use crate::snippets::{motivos_deliberacion, resolver, resumen_destino, Destino, Snippet};
use crate::ui::archivos::acortar;
use crate::ui::bloque;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::tuneles::{
    estilo_fila, marco_detalle, prefijo, repartir_vista, texto_plegado, Tabla, ANCHO_PREFIJO,
};

/// Filas del panel inferior, bordes incluidos: hasta tres del comando y una
/// por destinos, variables, confirmación y uso.
pub const ALTO_DETALLE: u16 = 9;

/// Líneas del comando que enseña el panel.
const LINEAS_COMANDO: usize = 3;

/// Líneas del comando que enseña el diálogo de detalle (`i`).
const LINEAS_COMANDO_DIALOGO: usize = 12;

/// Ancho útil de las líneas del diálogo de detalle (74 de ancho menos bordes
/// y márgenes): el modal no parte las líneas largas.
const ANCHO_DIALOGO: usize = 68;

/// Ancho de la columna de rótulos del panel («Confirmación» y aire).
const ANCHO_ROTULO: usize = 14;

/// Ancho de la marca «CRÍTICO».
const ANCHO_CRITICO: u16 = 7;

/// Una fila de la lista con lo que se calcula una vez por dibujo.
struct Fila<'a> {
    snippet: &'a Snippet,
    nombre: String,
    resumen: String,
    /// «→ 3 hosts».
    hosts: String,
    resueltos: usize,
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let punto = tema.glifos.punto_medio;
    let visibles = app.snippets_visibles();
    let total = app.snippets.lista.len();
    // Con el filtro, cuántos se ven de cuántos hay.
    let titulo = if visibles.len() == total {
        format!("MAGI {punto} SNIPPETS {punto} {total}")
    } else {
        format!(
            "MAGI {punto} SNIPPETS {punto} {} de {total}",
            visibles.len()
        )
    };
    let marco_bloque = bloque(&titulo, tema);
    let interior = marco_bloque.inner(area);
    if interior.height == 0 {
        marco.render_widget(marco_bloque, area);
        return;
    }

    let seleccionado = visibles.get(app.snippets.seleccion).copied();
    let (zona, detalle) = repartir_vista(interior, ALTO_DETALLE, disp);
    let marco_bloque = if detalle.is_none() && seleccionado.is_some() {
        marco_bloque.title_bottom(Span::styled(
            format!(" {} ", texto_plegado(tema, "i")),
            Style::default().fg(tema.paleta.inactivo),
        ))
    } else {
        marco_bloque
    };
    marco.render_widget(marco_bloque, area);

    let con_filtro = app.snippets.filtro_activo || !app.snippets.filtro.is_empty();
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(u16::from(con_filtro)),
            Constraint::Min(0),
        ])
        .split(zona);
    if con_filtro {
        let filtro: String = app
            .snippets
            .filtro
            .chars()
            .filter(|caracter| !caracter.is_control())
            .collect();
        let mut spans = vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                disposicion::adaptar(&filtro, ascii),
                Style::default()
                    .fg(tema.paleta.texto)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if app.snippets.filtro_activo {
            spans.push(Span::styled(
                crate::ui::tecla(tema, "▏", "_"),
                Style::default().fg(tema.paleta.acento),
            ));
        }
        marco.render_widget(Paragraph::new(Line::from(spans)), trozos[0]);
    }

    let lista = trozos[1];
    if visibles.is_empty() {
        let mensaje = if total == 0 {
            "Sin snippets: n para crear uno"
        } else {
            "sin resultados para el filtro"
        };
        let centrado = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(lista);
        marco.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    mensaje,
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ])
            .alignment(Alignment::Center),
            centrado[1],
        );
        return;
    }

    if lista.height > 0 {
        dibujar_lista(marco, lista, app, &visibles, disp);
    }
    if let (Some(detalle), Some(snippet)) = (detalle, seleccionado) {
        dibujar_detalle(marco, detalle, app, snippet);
    }
}

/// Columnas: nombre, destino resumido, «→ N hosts» y la marca de crítico. El
/// nombre (que pide su largo, hasta 24) y la marca no se ocultan; antes de
/// recortar el nombre cae el destino resumido.
fn columnas(minimo_nombre: u16, ancho_resumen: u16, ancho_hosts: u16) -> [Columna; 4] {
    [
        Columna::flexible(minimo_nombre, 1),
        Columna::fija(ancho_resumen, 3),
        Columna::fija(ancho_hosts, 2),
        Columna::fija(ANCHO_CRITICO, 1),
    ]
}

fn dibujar_lista(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    visibles: &[&Snippet],
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let flecha = crate::ui::tecla(tema, "→", "->");
    let filas: Vec<Fila> = visibles
        .iter()
        .map(|snippet| {
            let resueltos = resolver(&snippet.destinos, &app.hosts, &app.grupos).len();
            Fila {
                snippet,
                nombre: limpio(&snippet.nombre),
                resumen: limpio(&resumen_destino(
                    &snippet.destinos,
                    resueltos,
                    app.hosts.len(),
                )),
                hosts: format!("{flecha} {}", texto_hosts(resueltos)),
                resueltos,
            }
        })
        .collect();

    // Anchos sobre todas las filas, para que no bailen al desplazarse.
    let largo =
        |medida: fn(&Fila) -> usize| -> u16 { filas.iter().map(medida).max().unwrap_or(1) as u16 };
    let largo_nombre = largo(|fila| fila.nombre.chars().count()).max(8);
    let ancho_resumen = largo(|fila| fila.resumen.chars().count()).clamp(1, 20);
    let ancho_hosts = largo(|fila| fila.hosts.chars().count());
    let columnas = columnas(largo_nombre.min(24), ancho_resumen, ancho_hosts);
    let naturales = [largo_nombre, ancho_resumen, ancho_hosts, ANCHO_CRITICO];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &naturales,
    );

    // La selección siempre a la vista, aunque el desplazamiento guardado se
    // calculara con otra altura.
    let altura = area.height as usize;
    let seleccion = app.snippets.seleccion;
    let inicio = disposicion::ventana(app.snippets.desplazamiento, seleccion, altura, filas.len());
    disp.registrar(
        Lista::Snippets,
        VentanaLista {
            inicio,
            filas: altura,
            total: filas.len(),
        },
    );

    let lineas: Vec<Line> = filas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(altura)
        .map(|(posicion, fila)| {
            let seleccionada = posicion == seleccion;
            let base = estilo_fila(tema, seleccionada);
            // Sobre la fila seleccionada todo va con el color de la selección.
            let (estilo_destino, estilo_critico) = if seleccionada {
                (base, base)
            } else {
                let destino = if fila.resueltos == 0 {
                    tema.paleta.inactivo
                } else {
                    tema.paleta.texto
                };
                (
                    Style::default().fg(destino),
                    Style::default()
                        .fg(tema.paleta.critico)
                        .add_modifier(Modifier::BOLD),
                )
            };
            tabla.linea(
                prefijo(seleccionada, tema),
                vec![
                    (fila.nombre.clone(), base),
                    (fila.resumen.clone(), estilo_destino),
                    (fila.hosts.clone(), estilo_destino),
                    (
                        if fila.snippet.critico { "CRÍTICO" } else { "" }.to_string(),
                        estilo_critico,
                    ),
                ],
                base,
                ascii,
            )
        })
        .collect();
    marco.render_widget(Paragraph::new(lineas), area);
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, snippet: &Snippet) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let interior = marco_detalle(marco, area, tema);
    if interior.height == 0 {
        return;
    }
    let ancho = (interior.width as usize).saturating_sub(2);
    let ancho_valor = ancho.saturating_sub(ANCHO_ROTULO).max(1);
    let filas = interior.height as usize;
    let estilo_rotulo = Style::default().fg(tema.paleta.inactivo);
    let estilo_valor = Style::default().fg(tema.paleta.texto);
    // Todo pasa a ASCII si toca y se vuelve a recortar: la conversión puede
    // alargar un texto (`→` pasa a `->`).
    let recorte = |texto: &str, ancho: usize| {
        disposicion::recortar(&disposicion::adaptar(texto, ascii), ancho, ascii)
    };

    // Destinos, variables, confirmación y uso llevan una fila cada uno; el
    // comando se queda con lo que sobre (hasta tres) y, si aún sobra, hay
    // una fila de aire y los destinos pueden ocupar más de una.
    let fijas = 4;
    let comando = lineas_comando(
        &snippet.comando,
        LINEAS_COMANDO.min(filas.saturating_sub(fijas)),
        ancho,
    );
    let sobrantes = filas.saturating_sub(comando.len() + fijas);
    let mut lineas: Vec<Line> = comando
        .into_iter()
        .map(|linea| {
            Line::from(Span::styled(
                format!("  {}", recorte(&linea, ancho)),
                estilo_valor,
            ))
        })
        .collect();
    if sobrantes > 0 {
        lineas.push(Line::from(""));
    }

    let resueltos = resolver(&snippet.destinos, &app.hosts, &app.grupos);
    let destinos = disposicion::adaptar(
        &texto_destinos(&snippet.destinos, &app.hosts, &app.grupos),
        ascii,
    );
    let filas_destinos = 1 + sobrantes.saturating_sub(1);
    for (indice, trozo) in recortar_en_lineas(&destinos, ancho_valor, filas_destinos)
        .into_iter()
        .enumerate()
    {
        let rotulo = if indice == 0 { "Destinos" } else { "" };
        lineas.push(Line::from(vec![
            Span::styled(format!("  {rotulo:<ANCHO_ROTULO$}"), estilo_rotulo),
            Span::styled(recorte(&trozo, ancho_valor), estilo_valor),
        ]));
    }

    lineas.push(Line::from(vec![
        Span::styled(format!("  {:<ANCHO_ROTULO$}", "Variables"), estilo_rotulo),
        Span::styled(
            recorte(&texto_variables(&snippet.comando), ancho_valor),
            estilo_valor,
        ),
    ]));

    let confirmacion =
        texto_confirmacion(snippet.critico, &resueltos, &app.snippets.verificaciones);
    let estilo_confirmacion = if confirmacion == SIN_DELIBERACION {
        Style::default().fg(tema.paleta.inactivo)
    } else {
        Style::default().fg(tema.paleta.acento)
    };
    lineas.push(Line::from(vec![
        Span::styled(
            format!("  {:<ANCHO_ROTULO$}", "Confirmación"),
            estilo_rotulo,
        ),
        Span::styled(recorte(&confirmacion, ancho_valor), estilo_confirmacion),
    ]));

    lineas.push(Line::from(Span::styled(
        format!("  {}", recorte(&texto_uso(snippet), ancho)),
        estilo_rotulo,
    )));

    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Título del diálogo de detalle (`i`).
pub fn titulo_detalle(snippet: &Snippet, ascii: bool) -> String {
    disposicion::adaptar(&format!("SNIPPET · {}", limpio(&snippet.nombre)), ascii)
}

/// Líneas del diálogo de detalle (`i`): las columnas de la lista y todo lo
/// del panel inferior, con más líneas del comando y los valores partidos a lo
/// ancho del diálogo (que no parte líneas).
pub fn lineas_detalle(app: &App, snippet: &Snippet, ascii: bool) -> Vec<String> {
    let resueltos = resolver(&snippet.destinos, &app.hosts, &app.grupos);
    let mut cabecera = format!(
        "{} → {}",
        limpio(&resumen_destino(
            &snippet.destinos,
            resueltos.len(),
            app.hosts.len()
        )),
        texto_hosts(resueltos.len())
    );
    if snippet.critico {
        cabecera.push_str(" · CRÍTICO");
    }
    let mut lineas = vec![cabecera, String::new()];
    lineas.extend(
        lineas_comando(&snippet.comando, LINEAS_COMANDO_DIALOGO, ANCHO_DIALOGO - 2)
            .into_iter()
            .map(|linea| format!("  {linea}")),
    );
    lineas.push(String::new());
    let campos = [
        (
            "Destinos",
            texto_destinos(&snippet.destinos, &app.hosts, &app.grupos),
        ),
        ("Variables", texto_variables(&snippet.comando)),
        (
            "Confirmación",
            texto_confirmacion(snippet.critico, &resueltos, &app.snippets.verificaciones),
        ),
    ];
    for (rotulo, valor) in campos {
        for (indice, trozo) in partir(&valor, ANCHO_DIALOGO - ANCHO_ROTULO)
            .into_iter()
            .enumerate()
        {
            let rotulo = if indice == 0 { rotulo } else { "" };
            lineas.push(format!("{rotulo:<ANCHO_ROTULO$}{trozo}"));
        }
    }
    lineas.push(texto_uso(snippet));
    lineas
        .into_iter()
        .map(|linea| disposicion::adaptar(&linea, ascii))
        .collect()
}

/// Texto de la confirmación cuando no hay que deliberar.
const SIN_DELIBERACION: &str = "sin deliberación";

/// «requiere deliberación MAGI (crítico, 3 hosts)» o «sin deliberación»,
/// con los mismos motivos que decidirán al ejecutar (§7.3).
pub fn texto_confirmacion(
    critico: bool,
    resueltos: &[&Host],
    verificaciones: &HashMap<i64, Verificaciones>,
) -> String {
    let hosts: Vec<(i64, String)> = resueltos
        .iter()
        .map(|host| (host.id, host.nombre.clone()))
        .collect();
    let motivos = motivos_deliberacion(critico, &hosts, verificaciones);
    if motivos.is_empty() {
        return SIN_DELIBERACION.to_string();
    }
    let motivos: Vec<String> = motivos.iter().map(|motivo| motivo.texto()).collect();
    limpio(&format!(
        "requiere deliberación MAGI ({})",
        motivos.join(", ")
    ))
}

/// «1 host» / «N hosts».
pub fn texto_hosts(cuantos: usize) -> String {
    if cuantos == 1 {
        "1 host".to_string()
    } else {
        format!("{cuantos} hosts")
    }
}

/// Destinos con sus hosts resueltos: «etiqueta «web»: a, b · hosts: c».
pub fn texto_destinos(destinos: &[Destino], hosts: &[Host], grupos: &[Grupo]) -> String {
    let mut partes = Vec::new();
    for destino in destinos {
        if let Destino::Etiqueta(etiqueta) = destino {
            let nombres: Vec<&str> = resolver(std::slice::from_ref(destino), hosts, grupos)
                .iter()
                .map(|host| host.nombre.as_str())
                .collect();
            let lista = if nombres.is_empty() {
                "ningún host".to_string()
            } else {
                nombres.join(", ")
            };
            partes.push(format!("etiqueta «{etiqueta}»: {lista}"));
        }
    }
    let sueltos: Vec<&str> = destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Host { nombre, .. } => Some(nombre.as_str()),
            Destino::Etiqueta(_) => None,
        })
        .collect();
    match sueltos.len() {
        0 => {}
        1 => partes.push(format!("host {}", sueltos[0])),
        _ => partes.push(format!("hosts {}", sueltos.join(", "))),
    }
    if partes.is_empty() {
        return "\u{2014}".to_string();
    }
    limpio(&partes.join(" · "))
}

/// Variables del comando: «servicio, dir=/tmp» o «ninguna».
pub fn texto_variables(comando: &str) -> String {
    let variables = crate::snippets::variables::detectar(comando);
    if variables.is_empty() {
        return "ninguna".to_string();
    }
    variables
        .iter()
        .map(|variable| match &variable.defecto {
            Some(defecto) => format!("{}={}", variable.nombre, sanear_linea(defecto, 24)),
            None => variable.nombre.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// «Timeout 60 s · usado 11 veces · última hace 2 d».
pub fn texto_uso(snippet: &Snippet) -> String {
    let veces = if snippet.usado_veces == 1 {
        "1 vez".to_string()
    } else {
        format!("{} veces", snippet.usado_veces)
    };
    let ultima = match snippet.ultimo_uso_en.as_deref() {
        None => "nunca".to_string(),
        Some(fecha) => match antiguedad_segundos(fecha) {
            Some(segundos) => format!("hace {}", formatear_antiguedad(segundos)),
            None => sanear_linea(fecha, 32),
        },
    };
    format!(
        "Timeout {} s · usado {veces} · última {ultima}",
        snippet.timeout_seg
    )
}

/// Las primeras `maximo` líneas del comando, saneadas y recortadas a `ancho`;
/// si quedan más, la última lo dice.
fn lineas_comando(comando: &str, maximo: usize, ancho: usize) -> Vec<String> {
    if maximo == 0 {
        return Vec::new();
    }
    let limpio = texto_limpio(comando.as_bytes());
    let todas: Vec<&str> = limpio.lines().collect();
    let mut lineas: Vec<String> = todas
        .iter()
        .take(maximo)
        .map(|linea| acortar(linea, ancho))
        .collect();
    if todas.len() > maximo {
        let aviso = match todas.len() - maximo {
            1 => " … (+1 línea)".to_string(),
            resto => format!(" … (+{resto} líneas)"),
        };
        let ultima = todas[maximo - 1];
        let largo_aviso = aviso.chars().count();
        if let Some(linea) = lineas.last_mut() {
            *linea = if ancho >= largo_aviso + 4 {
                format!("{}{aviso}", acortar(ultima, ancho - largo_aviso))
            } else {
                // Tan estrecho que no cabe el recuento: basta con que se vea
                // que el comando sigue.
                let recortada: String = ultima.chars().take(ancho.saturating_sub(1)).collect();
                format!("{recortada}…")
            };
        }
    }
    lineas
}

/// Parte un texto en como mucho `maximo` líneas de `ancho`; lo que no quepa
/// se recorta con «…» en la última.
pub fn recortar_en_lineas(texto: &str, ancho: usize, maximo: usize) -> Vec<String> {
    let maximo = maximo.max(1);
    let mut lineas = partir(texto, ancho);
    if lineas.len() > maximo {
        let resto = lineas.split_off(maximo - 1).join(" ");
        lineas.push(acortar(&resto, ancho));
    }
    lineas
}

/// Parte un texto en líneas de como mucho `ancho` caracteres, por espacios
/// (una palabra más larga que el ancho se corta). Siempre devuelve al menos
/// una línea.
pub fn partir(texto: &str, ancho: usize) -> Vec<String> {
    let ancho = ancho.max(1);
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        let mut palabra: Vec<char> = palabra.chars().collect();
        while palabra.len() > ancho {
            if !actual.is_empty() {
                lineas.push(std::mem::take(&mut actual));
            }
            lineas.push(palabra.drain(..ancho).collect());
        }
        if palabra.is_empty() {
            continue;
        }
        if !actual.is_empty() && actual.chars().count() + 1 + palabra.len() > ancho {
            lineas.push(std::mem::take(&mut actual));
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.extend(palabra);
    }
    if !actual.is_empty() || lineas.is_empty() {
        lineas.push(actual);
    }
    lineas
}

/// Texto de una sola línea sin secuencias de escape ni caracteres de
/// control (nombres de host y etiquetas vienen de fuera: importaciones; y la
/// base la puede tocar otro proceso).
pub fn limpio(texto: &str) -> String {
    texto_limpio(texto.as_bytes()).replace('\n', " ")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn host(id: i64, nombre: &str, etiquetas: &[&str]) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = None;
        host.etiquetas = etiquetas.iter().map(|e| e.to_string()).collect();
        host
    }

    fn snippet(comando: &str) -> Snippet {
        Snippet {
            id: 1,
            nombre: "x".to_string(),
            comando: comando.to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico: false,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: Vec::new(),
        }
    }

    #[test]
    fn la_confirmacion_dice_por_que_se_delibera() {
        let uno = host(1, "uno", &[]);
        let dos = host(2, "dos", &[]);
        let mut verificaciones = HashMap::new();
        assert_eq!(
            texto_confirmacion(false, &[&uno], &verificaciones),
            "sin deliberación"
        );
        assert_eq!(
            texto_confirmacion(true, &[&uno], &verificaciones),
            "requiere deliberación MAGI (crítico)"
        );
        assert_eq!(
            texto_confirmacion(false, &[&uno, &dos], &verificaciones),
            "requiere deliberación MAGI (2 hosts)"
        );
        verificaciones.insert(
            2,
            Verificaciones {
                host_id: 2,
                backup: true,
                ..Default::default()
            },
        );
        assert_eq!(
            texto_confirmacion(true, &[&uno, &dos], &verificaciones),
            "requiere deliberación MAGI (crítico, 2 hosts, verificaciones en dos)"
        );
        assert_eq!(
            texto_confirmacion(false, &[&dos], &verificaciones),
            "requiere deliberación MAGI (verificaciones en dos)"
        );
        // Sin hosts no hay a quién preguntar, salvo que sea crítico.
        assert_eq!(
            texto_confirmacion(false, &[], &verificaciones),
            "sin deliberación"
        );
    }

    #[test]
    fn los_destinos_nombran_los_hosts_resueltos() {
        let hosts = vec![
            host(1, "hetzner-01", &["web"]),
            host(2, "hetzner-02", &["Web"]),
            host(3, "db", &["db"]),
        ];
        let destinos = vec![
            Destino::Etiqueta("web".to_string()),
            Destino::Etiqueta("vacia".to_string()),
            Destino::Host {
                id: 3,
                nombre: "db".to_string(),
            },
        ];
        assert_eq!(
            texto_destinos(&destinos, &hosts, &[]),
            "etiqueta «web»: hetzner-01, hetzner-02 · etiqueta «vacia»: ningún host · host db"
        );
        assert_eq!(texto_destinos(&[], &hosts, &[]), "\u{2014}");
    }

    #[test]
    fn los_nombres_no_cuelan_secuencias_de_escape() {
        let hosts = vec![host(1, "malo\u{1b}]0;titulo\u{7}-01", &["web"])];
        let destinos = vec![Destino::Etiqueta("web".to_string())];
        let texto = texto_destinos(&destinos, &hosts, &[]);
        assert!(!texto.chars().any(char::is_control), "{texto:?}");
        assert_eq!(texto, "etiqueta «web»: malo-01");
    }

    #[test]
    fn las_variables_con_su_defecto() {
        assert_eq!(texto_variables("uptime"), "ninguna");
        assert_eq!(
            texto_variables("journalctl -u {{servicio}} -n {{lineas:50}} {{servicio}}"),
            "servicio, lineas=50"
        );
    }

    #[test]
    fn el_uso_dice_timeout_veces_y_ultima() {
        let mut uno = snippet("uptime");
        assert_eq!(
            texto_uso(&uno),
            "Timeout 60 s · usado 0 veces · última nunca"
        );
        uno.usado_veces = 1;
        uno.timeout_seg = 5;
        uno.ultimo_uso_en = Some(crate::modelo::fecha_ahora());
        let texto = texto_uso(&uno);
        assert!(
            texto.starts_with("Timeout 5 s · usado 1 vez · última hace "),
            "{texto}"
        );
        uno.usado_veces = 11;
        uno.ultimo_uso_en = Some("ayer\u{1b}[31m".to_string());
        assert_eq!(
            texto_uso(&uno),
            "Timeout 5 s · usado 11 veces · última ayer"
        );
    }

    #[test]
    fn el_comando_se_sanea_y_se_recorta() {
        assert_eq!(
            lineas_comando("printf '\u{1b}[2J'\n\tls", 3, 40),
            vec!["printf ''", "    ls"]
        );
        let largo = "uno\ndos\ntres\ncuatro\ncinco";
        let lineas = lineas_comando(largo, 3, 40);
        assert_eq!(lineas, vec!["uno", "dos", "tres … (+2 líneas)"]);
        assert_eq!(
            lineas_comando("uno\ndos\ntres\ncuatro", 3, 40),
            vec!["uno", "dos", "tres … (+1 línea)"]
        );
        assert!(lineas_comando(largo, 0, 40).is_empty());
        for linea in lineas_comando(largo, 3, 12) {
            assert!(linea.chars().count() <= 12, "{linea}");
        }
    }

    #[test]
    fn partir_respeta_el_ancho() {
        assert_eq!(partir("", 10), vec![""]);
        assert_eq!(partir("uno dos tres", 7), vec!["uno dos", "tres"]);
        assert_eq!(partir("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(partir("a abcdefgh b", 4), vec!["a", "abcd", "efgh", "b"]);
        assert_eq!(partir("abcd efgh", 4), vec!["abcd", "efgh"]);
        let largo = "hetzner-01, hetzner-02, vps-openclaw, db-principal, cache-01";
        for linea in partir(largo, 20) {
            assert!(linea.chars().count() <= 20, "{linea}");
        }
    }

    #[test]
    fn recortar_en_lineas_cierra_con_puntos() {
        let lineas = recortar_en_lineas("a b c d e f g h", 3, 2);
        assert_eq!(lineas.len(), 2);
        assert_eq!(lineas[0], "a b");
        assert!(lineas[1].ends_with('…'), "{lineas:?}");
        assert_eq!(recortar_en_lineas("corto", 20, 1), vec!["corto"]);
    }

    #[test]
    fn texto_hosts_en_singular_y_plural() {
        assert_eq!(texto_hosts(0), "0 hosts");
        assert_eq!(texto_hosts(1), "1 host");
        assert_eq!(texto_hosts(12), "12 hosts");
    }
}
