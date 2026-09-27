//! Diálogo de deliberación MAGI (maqueta §6.2): acción, tabla host ×
//! comprobación con glifo y dato de cada veredicto, barra de consenso y
//! estado (DELIBERANDO, APROBADO, BLOQUEADO, FORZADO). Los nombres MAGI van
//! siempre con la palabra que dice qué comprueban.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::{App, DeliberacionAbierta};
use crate::deliberacion::estado::Fase;
use crate::deliberacion::{Comprobacion, Veredicto};
use crate::tema::Tema;
use crate::ui::centrar;
use crate::ui::dialogos::{atajo, modal};

/// Ancho de la columna de hosts y largo de la barra de consenso.
const ANCHO_HOST: usize = 16;
const LARGO_BARRA: usize = 21;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, deliberacion: &DeliberacionAbierta) {
    let tema = &app.tema;
    let ancho = area.width.saturating_sub(2).min(100);
    // Lo que no es la tabla: bordes y márgenes (4), acción y motivos (2),
    // separadores (2), cabecera (2), detalle (1), consenso y estado (2),
    // hueco y teclas (2); forzando, el rótulo y el campo (2) y, sin
    // comprobaciones activas, el aviso (1).
    let fijo = 15
        + match deliberacion.maquina.fase {
            Fase::Motivo => 2,
            Fase::Aprobada if deliberacion.consenso().activas == 0 => 1,
            _ => 0,
        };
    let caben = (area.height.saturating_sub(2) as usize)
        .saturating_sub(fijo)
        .max(1);
    let alto = (fijo + deliberacion.filas.len().min(caben)) as u16;
    let recta = centrar(area, ancho, alto);
    let interior = recta.width.saturating_sub(6) as usize;
    let lineas = lineas(deliberacion, tema, interior, caben);
    let peligro = matches!(deliberacion.maquina.fase, Fase::Bloqueada | Fase::Motivo);
    modal(marco, recta, "DELIBERACIÓN MAGI", lineas, peligro, tema);
}

/// Las líneas del diálogo para un interior de `ancho` columnas y como mucho
/// `caben` filas de hosts.
pub fn lineas(
    deliberacion: &DeliberacionAbierta,
    tema: &Tema,
    ancho: usize,
    caben: usize,
) -> Vec<Line<'static>> {
    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let negrita = texto.add_modifier(Modifier::BOLD);
    let mut lineas = Vec::new();
    let plan = &deliberacion.plan;
    let destino = match plan.hosts.as_slice() {
        [(_, host)] => host.clone(),
        hosts => format!("{} hosts", hosts.len()),
    };
    let pestana = if plan.modo == crate::app::lanzar::ModoLanzamiento::Pestanas {
        " (en pestaña)"
    } else {
        ""
    };
    lineas.push(Line::from(vec![
        Span::styled("ACCIÓN: ", negrita),
        Span::styled(
            recortar(
                &format!("{} → {destino}{pestana}", plan.nombre),
                ancho.saturating_sub(8),
            ),
            texto,
        ),
    ]));
    lineas.push(Line::from(Span::styled(
        recortar(
            &format!("requerida por: {}", deliberacion.motivos.join(" · ")),
            ancho,
        ),
        tenue,
    )));
    lineas.push(separador(ancho, tema));

    let columna = ancho.saturating_sub(ANCHO_HOST) / 3;
    let mut nombres = vec![Span::raw(rellenar("", ANCHO_HOST))];
    let mut palabras = vec![Span::raw(rellenar("", ANCHO_HOST))];
    for comprobacion in Comprobacion::TODAS {
        nombres.push(Span::styled(
            rellenar(comprobacion.nombre_magi(), columna),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
        let palabra = match comprobacion {
            Comprobacion::Backup => format!("backup<{} h", deliberacion.backup_horas),
            otra => otra.palabra().to_string(),
        };
        palabras.push(Span::styled(rellenar(&palabra, columna), tenue));
    }
    lineas.push(Line::from(nombres));
    lineas.push(Line::from(palabras));

    let total = deliberacion.filas.len();
    let seleccion = deliberacion.seleccion.min(total.saturating_sub(1));
    let desde = (seleccion + 1)
        .saturating_sub(caben)
        .min(total.saturating_sub(caben));
    for (indice, fila) in deliberacion
        .filas
        .iter()
        .enumerate()
        .skip(desde)
        .take(caben)
    {
        let marcada = indice == seleccion && total > 1;
        let estilo_host = if marcada {
            negrita.add_modifier(Modifier::REVERSED)
        } else {
            texto
        };
        let mut spans = vec![Span::styled(
            rellenar(&recortar(&fila.host, ANCHO_HOST - 1), ANCHO_HOST),
            estilo_host,
        )];
        for comprobacion in Comprobacion::TODAS {
            let veredicto = fila.veredicto(comprobacion);
            let (glifo, color) = glifo(veredicto, tema);
            let dato = match veredicto {
                Veredicto::Pendiente => "comprobando".to_string(),
                otro => otro.detalle().unwrap_or("").to_string(),
            };
            let celda = format!("{glifo} {}", recortar(&dato, columna.saturating_sub(3)));
            spans.push(Span::styled(
                rellenar(&celda, columna),
                Style::default().fg(color),
            ));
        }
        lineas.push(Line::from(spans));
    }
    if total > caben {
        lineas.push(Line::from(Span::styled(
            format!("  ↑ ↓ recorrer hosts ({} de {total})", seleccion + 1),
            tenue,
        )));
    } else {
        lineas.push(detalle_seleccionado(deliberacion, seleccion, ancho, tema));
    }
    lineas.push(separador(ancho, tema));

    let consenso = deliberacion.consenso();
    let color_consenso = match deliberacion.maquina.fase {
        Fase::Comprobando => tema.paleta.acento,
        Fase::Aprobada => tema.paleta.correcto,
        Fase::Bloqueada | Fase::Motivo => tema.paleta.critico,
    };
    // Sin comprobaciones activas la barra va llena: no hay nada en contra.
    let llenas = (consenso.aprobadas * LARGO_BARRA)
        .checked_div(consenso.activas)
        .unwrap_or(LARGO_BARRA);
    let (lleno, vacio) = if tema.ascii {
        ("#", ".")
    } else {
        ("█", "░")
    };
    lineas.push(Line::from(vec![
        Span::styled("CONSENSO  ", negrita),
        Span::styled(
            format!("{} / {}", consenso.aprobadas, consenso.activas),
            negrita,
        ),
        Span::raw("     "),
        Span::styled(lleno.repeat(llenas), Style::default().fg(color_consenso)),
        Span::styled(vacio.repeat(LARGO_BARRA - llenas), tenue),
    ]));

    let maquina = &deliberacion.maquina;
    let estado = |texto: String, color: Color| {
        Line::from(Span::styled(
            texto,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
    };
    let mut teclas: Vec<Span<'static>> = Vec::new();
    match maquina.fase {
        Fase::Comprobando => {
            lineas.push(estado(
                format!(
                    "▸ DELIBERANDO — {} comprobación(es) en marcha",
                    consenso.pendientes
                ),
                tema.paleta.acento,
            ));
            teclas.extend([atajo("esc", tema), Span::styled(" cancelar", tenue)]);
        }
        Fase::Aprobada => {
            if consenso.activas == 0 {
                lineas.push(Line::from(Span::styled(
                    "sin comprobaciones configuradas",
                    tenue,
                )));
            }
            lineas.push(estado(
                "▸ APROBADO — ctrl+k ejecutar · esc cancelar".to_string(),
                tema.paleta.correcto,
            ));
            teclas.extend([
                atajo("ctrl+k", tema),
                Span::styled(" ejecutar    ", tenue),
                atajo("esc", tema),
                Span::styled(" cancelar", tenue),
            ]);
        }
        Fase::Bloqueada => {
            lineas.push(estado(
                "▸ BLOQUEADO — se requiere unanimidad".to_string(),
                tema.paleta.critico,
            ));
            teclas.extend([
                atajo("f", tema),
                Span::styled(" forzar (registra motivo)    ", tenue),
                atajo("esc", tema),
                Span::styled(" cancelar", tenue),
            ]);
        }
        Fase::Motivo => {
            lineas.push(Line::from(Span::styled(
                format!(
                    "Motivo del forzado (mín. {} caracteres):",
                    maquina.motivo_min()
                ),
                texto,
            )));
            lineas.push(Line::from(vec![
                Span::styled("[ ", tenue),
                maquina
                    .motivo
                    .span(true, crate::ui::componentes::estilo_campo(tema, true)),
                Span::styled(" ]", tenue),
            ]));
            if maquina.forzado_listo() {
                lineas.push(estado(
                    "▸ FORZADO — quedará registrado con usuario y hora".to_string(),
                    tema.paleta.critico,
                ));
            } else {
                let aviso = format!(
                    "mínimo {} caracteres ({}/{})",
                    maquina.motivo_min(),
                    maquina.largo_motivo(),
                    maquina.motivo_min()
                );
                let color = if maquina.aviso_minimo {
                    tema.paleta.critico
                } else {
                    tema.paleta.inactivo
                };
                lineas.push(estado(format!("▸ BLOQUEADO — {aviso}"), color));
            }
            teclas.extend([
                atajo("ctrl+k", tema),
                Span::styled(" ejecutar    ", tenue),
                atajo("esc", tema),
                Span::styled(" volver", tenue),
            ]);
        }
    }
    lineas.push(Line::from(""));
    // Escribiendo el motivo, `?` es un carácter más del campo.
    if maquina.fase != Fase::Motivo {
        teclas.extend([
            Span::raw("    "),
            atajo("?", tema),
            Span::styled(" ayuda", tenue),
        ]);
    }
    lineas.push(Line::from(teclas));
    lineas
}

/// Los datos completos del host seleccionado (las celdas se recortan).
fn detalle_seleccionado(
    deliberacion: &DeliberacionAbierta,
    seleccion: usize,
    ancho: usize,
    tema: &Tema,
) -> Line<'static> {
    let Some(fila) = deliberacion.filas.get(seleccion) else {
        return Line::from("");
    };
    let partes: Vec<String> = Comprobacion::TODAS
        .iter()
        .filter_map(|comprobacion| {
            let veredicto = fila.veredicto(*comprobacion);
            veredicto.rechaza().then(|| {
                format!(
                    "{}: {}",
                    comprobacion.palabra(),
                    veredicto.detalle().unwrap_or("")
                )
            })
        })
        .collect();
    if partes.is_empty() {
        return Line::from("");
    }
    Line::from(Span::styled(
        recortar(&format!("  {} · {}", fila.host, partes.join(" · ")), ancho),
        Style::default().fg(tema.paleta.critico),
    ))
}

/// Glifo y color de una celda: `✓` `✕` `—` `◐` (ASCII `* x - o`).
pub fn glifo(veredicto: &Veredicto, tema: &Tema) -> (&'static str, Color) {
    let (unicode, ascii, color) = match veredicto {
        Veredicto::Aprueba { .. } => ("✓", "*", tema.paleta.correcto),
        Veredicto::Rechaza { .. } => ("✕", "x", tema.paleta.critico),
        Veredicto::NoActiva => ("—", "-", tema.paleta.inactivo),
        Veredicto::Pendiente => ("◐", "o", tema.paleta.acento),
    };
    (if tema.ascii { ascii } else { unicode }, color)
}

fn separador(ancho: usize, tema: &Tema) -> Line<'static> {
    let raya = if tema.ascii { "-" } else { "─" };
    Line::from(Span::styled(
        raya.repeat(ancho),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

/// Recorta a `ancho` caracteres con «…», sin caracteres de control ni de
/// dirección (nombres de host o de snippet importados).
pub fn recortar(texto: &str, ancho: usize) -> String {
    let limpio: String = texto
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect();
    if limpio.chars().count() <= ancho {
        return limpio;
    }
    if ancho == 0 {
        return String::new();
    }
    let mut recortado: String = limpio.chars().take(ancho - 1).collect();
    recortado.push('…');
    recortado
}

fn rellenar(texto: &str, ancho: usize) -> String {
    let largo = texto.chars().count();
    if largo >= ancho {
        texto.to_string()
    } else {
        format!("{texto}{}", " ".repeat(ancho - largo))
    }
}

#[cfg(test)]
mod pruebas {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::lanzar::{ModoLanzamiento, PlanEjecucion};
    use crate::deliberacion::estado::AccionMaquina;
    use crate::deliberacion::ComprobacionesHost;

    fn plan(hosts: &[(i64, &str)]) -> PlanEjecucion {
        PlanEjecucion {
            snippet_id: 1,
            nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: hosts.iter().map(|(id, n)| (*id, n.to_string())).collect(),
            timeout_seg: 60,
            parar_al_fallo: false,
            critico: true,
            valores: BTreeMap::new(),
            modo: ModoLanzamiento::Servidor,
        }
    }

    fn aprueba(detalle: &str) -> Veredicto {
        Veredicto::Aprueba {
            detalle: detalle.to_string(),
            ms: 1,
        }
    }

    fn rechaza(detalle: &str) -> Veredicto {
        Veredicto::Rechaza {
            detalle: detalle.to_string(),
            ms: 1,
        }
    }

    fn fila(id: i64, host: &str, salud: Veredicto, backup: Veredicto) -> ComprobacionesHost {
        ComprobacionesHost {
            host_id: id,
            host: host.to_string(),
            salud,
            backup,
            tests: Veredicto::NoActiva,
        }
    }

    fn bloqueada() -> DeliberacionAbierta {
        DeliberacionAbierta::de_prueba(
            plan(&[(1, "hetzner-01"), (2, "hetzner-02")]),
            vec![
                fila(
                    1,
                    "hetzner-01",
                    aprueba("NOMINAL · 0,2 s"),
                    aprueba("hace 3 h"),
                ),
                fila(
                    2,
                    "hetzner-02",
                    aprueba("NOMINAL · 0,1 s"),
                    rechaza("hace 31 h"),
                ),
            ],
            10,
        )
    }

    fn pintar(deliberacion: &DeliberacionAbierta, ascii: bool) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
        let mut tema = Tema::respaldo();
        tema.ascii = ascii;
        terminal
            .draw(|marco| {
                let area = marco.area();
                let ancho = area.width.saturating_sub(2).min(100);
                let recta = centrar(area, ancho, 26);
                let interior = recta.width.saturating_sub(6) as usize;
                let lineas = lineas(deliberacion, &tema, interior, 8);
                modal(marco, recta, "DELIBERACIÓN MAGI", lineas, false, &tema);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn tecla(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn bloqueada_muestra_la_tabla_el_consenso_y_el_forzado() {
        let deliberacion = bloqueada();
        assert_eq!(deliberacion.maquina.fase, Fase::Bloqueada);
        let pantalla = pintar(&deliberacion, false);
        for esperado in [
            "DELIBERACIÓN MAGI",
            "ACCIÓN: reiniciar nginx → 2 hosts",
            "requerida por: crítico",
            "MELCHIOR-1",
            "BALTHASAR-2",
            "CASPER-3",
            "backup<24 h",
            "✕ hace 31 h",
            "✓ hace 3 h",
            "— ",
            "CONSENSO  3 / 4",
            "▸ BLOQUEADO — se requiere unanimidad",
            "f forzar (registra motivo)",
        ] {
            assert!(
                pantalla.contains(esperado),
                "falta «{esperado}» en\n{pantalla}"
            );
        }
        assert!(!pantalla.contains("ctrl+k ejecutar"));
    }

    #[test]
    fn un_motivo_corto_con_ctrl_k_no_ejecuta_y_enseña_el_minimo() {
        let mut deliberacion = bloqueada();
        let maquina = &mut deliberacion.maquina;
        assert_eq!(
            maquina.pulsar(&tecla(KeyCode::Char('f'))),
            AccionMaquina::Nada
        );
        for c in "ok".chars() {
            maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(maquina.pulsar(&ctrl_k), AccionMaquina::Nada);
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Enter)), AccionMaquina::Nada);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("Motivo del forzado (mín. 10 caracteres):"));
        assert!(
            pantalla.contains("mínimo 10 caracteres (2/10)"),
            "{pantalla}"
        );
        assert!(!pantalla.contains("FORZADO"));
    }

    #[test]
    fn con_motivo_suficiente_queda_forzado() {
        let mut deliberacion = bloqueada();
        let maquina = &mut deliberacion.maquina;
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        for c in "backup revisado a mano".chars() {
            maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("▸ FORZADO — quedará registrado con usuario y hora"));
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(
            deliberacion.maquina.pulsar(&ctrl_k),
            AccionMaquina::Ejecutar {
                forzada: true,
                motivo: Some("backup revisado a mano".to_string()),
            }
        );
    }

    #[test]
    fn sin_comprobaciones_activas_se_aprueba_y_sigue_pidiendo_ctrl_k() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "vps-openclaw")]),
            vec![fila(
                1,
                "vps-openclaw",
                Veredicto::NoActiva,
                Veredicto::NoActiva,
            )],
            10,
        );
        assert_eq!(deliberacion.maquina.fase, Fase::Aprobada);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("ACCIÓN: reiniciar nginx → vps-openclaw"));
        assert!(pantalla.contains("sin comprobaciones configuradas"));
        assert!(pantalla.contains("▸ APROBADO — ctrl+k ejecutar · esc cancelar"));
        assert!(pantalla.contains("CONSENSO  0 / 0"));
    }

    #[test]
    fn comprobando_enseña_el_glifo_de_espera_y_en_ascii_otro() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "hetzner-01")]),
            vec![fila(
                1,
                "hetzner-01",
                Veredicto::Pendiente,
                rechaza("hace 31 h"),
            )],
            10,
        );
        assert_eq!(deliberacion.maquina.fase, Fase::Comprobando);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("◐ comprobando"));
        assert!(pantalla.contains("DELIBERANDO"));
        let ascii = pintar(&deliberacion, true);
        assert!(ascii.contains("o comprobando"));
        assert!(ascii.contains("x hace 31 h"));
        assert!(!ascii.contains('◐'));
    }

    #[test]
    fn un_nombre_con_escapes_no_llega_a_la_terminal() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "malo\u{1b}[2J")]),
            vec![fila(
                1,
                "malo\u{1b}[2J",
                aprueba("NOMINAL"),
                Veredicto::NoActiva,
            )],
            10,
        );
        let pantalla = pintar(&deliberacion, false);
        assert!(!pantalla.contains('\u{1b}'));
        assert!(pantalla.contains("malo\u{FFFD}[2J"));
    }

    #[test]
    fn muchos_hosts_se_recorren_con_flechas() {
        let hosts: Vec<(i64, String)> = (1..=20).map(|i| (i, format!("host-{i:02}"))).collect();
        let referencias: Vec<(i64, &str)> = hosts.iter().map(|(i, n)| (*i, n.as_str())).collect();
        let filas = hosts
            .iter()
            .map(|(i, n)| fila(*i, n, aprueba("NOMINAL"), Veredicto::NoActiva))
            .collect();
        let mut deliberacion = DeliberacionAbierta::de_prueba(plan(&referencias), filas, 10);
        deliberacion.seleccion = 15;
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("host-16"));
        assert!(!pantalla.contains("host-01 "));
        assert!(pantalla.contains("recorrer hosts (16 de 20)"));
    }
}
