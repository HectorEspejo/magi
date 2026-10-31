//! Diálogo SERVIDOR DE SESIONES ANTERIOR (Fase 9, §6.2 del informe): qué
//! versión corre al otro lado del socket, qué custodia y qué se pierde si se
//! para para arrancar el servidor de este binario.
//!
//! `p` pide confirmación; `s` la confirma; `n` vuelve al aviso; `Esc` cierra
//! el diálogo y deja la TUI sin sesiones; `q` sale de MAGI. Mientras se
//! sustituye se ignora el teclado. Es una `Hoja` de `dialogos.rs`: encoge con
//! el área, su cuerpo se desplaza (↑ ↓ PgUp PgDn) y el ASCII lo pone
//! `disposicion::texto_ascii`. Mínimo y modo estrecho en
//! `disposicion::MINIMO_SERVIDOR_ANTERIOR` / `ESTRECHO_SERVIDOR_ANTERIOR`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::servidor_anterior::{self, DialogoServidorAnterior};
use crate::app::App;
use crate::cliente::RechazoVersion;
use crate::protocolo::VERSION_PROTOCOLO;
use crate::tema::Tema;
use crate::ui::dialogos::{teclas, Hoja};
use crate::ui::disposicion::{Disposicion, ESTRECHO_SERVIDOR_ANTERIOR};

/// Título del recuadro (sin signos de apertura, T45).
const TITULO: &str = "SERVIDOR DE SESIONES ANTERIOR";

/// Ancho deseado de la hoja: la maqueta §6.2 con sus márgenes.
const ANCHO: u16 = 64;

/// Ancho de las etiquetas de la cabecera («Servidor activo: »).
const ETIQUETA: usize = 17;

/// Sangría de los renglones de continuación de «Custodia:».
const SANGRIA: usize = 11;

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoServidorAnterior,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_SERVIDOR_ANTERIOR;
    let rechazo = dialogo.rechazo();
    let (hoja, peligro) = match dialogo {
        DialogoServidorAnterior::Aviso(_) => (
            Hoja::nueva(
                TITULO,
                cuerpo_aviso(rechazo, estrecho, tema),
                vec![teclas(
                    &[
                        ("p", "parar y arrancar el nuevo"),
                        ("esc", "seguir sin sesiones"),
                        ("q", "salir"),
                    ],
                    tema,
                )],
            ),
            true,
        ),
        DialogoServidorAnterior::Confirmacion(_) => (
            Hoja::nueva(
                TITULO,
                cuerpo_confirmacion(rechazo, estrecho, tema),
                vec![teclas(&[("s", "confirmar"), ("n", "volver")], tema)],
            ),
            true,
        ),
        DialogoServidorAnterior::Sustituyendo(_) => (
            Hoja::nueva(TITULO, cuerpo_sustituyendo(rechazo, tema), Vec::new()),
            false,
        ),
        DialogoServidorAnterior::Error {
            motivo,
            anterior_parado,
            ..
        } => (
            Hoja::nueva(
                TITULO,
                cuerpo_error(rechazo, motivo, *anterior_parado, estrecho, tema),
                vec![teclas(
                    &[("esc", "seguir sin sesiones"), ("q", "salir")],
                    tema,
                )],
            ),
            true,
        ),
    };
    hoja.peligro(peligro).pintar(marco, area, ANCHO, tema, disp);
}

fn linea(texto: impl Into<String>) -> Line<'static> {
    Line::from(Span::raw(texto.into()))
}

/// Las dos versiones que se están comparando: este binario y el servidor.
fn cabecera(rechazo: &RechazoVersion, tema: &Tema) -> Vec<Line<'static>> {
    let punto = tema.glifos.punto_medio;
    let pid = rechazo
        .pid
        .map(|pid| format!(" {punto} pid {pid}"))
        .unwrap_or_default();
    let este = format!("{:<ETIQUETA$}", "Este magi:");
    let servidor = format!("{:<ETIQUETA$}", "Servidor activo:");
    vec![
        linea(format!(
            "  {este}{} {punto} protocolo {}",
            env!("CARGO_PKG_VERSION"),
            VERSION_PROTOCOLO
        )),
        linea(format!(
            "  {servidor}{} {punto} protocolo {}{pid}",
            servidor_anterior::version_app(rechazo),
            rechazo.version
        )),
    ]
}

/// «Custodia»: los recuentos en dos líneas o, en estrecho, uno por renglón.
/// Un servidor anterior a la v6 no lo informa.
fn custodia(rechazo: &RechazoVersion, estrecho: bool, tema: &Tema) -> Vec<Line<'static>> {
    let punto = tema.glifos.punto_medio;
    let Some(recuentos) = servidor_anterior::recuentos_custodia(rechazo) else {
        return vec![linea(format!(
            "  {:<SANGRIA$}{}",
            "Custodia:",
            servidor_anterior::lineas_custodia(rechazo)[0]
        ))];
    };
    let sangria = " ".repeat(SANGRIA);
    let mut lineas = Vec::new();
    if estrecho {
        for (indice, recuento) in recuentos.into_iter().enumerate() {
            let etiqueta = if indice == 0 { "Custodia:" } else { "" };
            lineas.push(linea(format!("  {etiqueta:<SANGRIA$}{recuento}")));
        }
    } else {
        lineas.push(linea(format!(
            "  {:<SANGRIA$}{} {punto} {}",
            "Custodia:", recuentos[0], recuentos[1]
        )));
        lineas.push(linea(format!(
            "  {sangria}{} {punto} {}",
            recuentos[2], recuentos[3]
        )));
    }
    if let Some(ventanas) = servidor_anterior::ventanas(rechazo) {
        lineas.push(linea(format!("  {sangria}{ventanas}")));
    }
    lineas
}

fn cuerpo_aviso(rechazo: &RechazoVersion, estrecho: bool, tema: &Tema) -> Vec<Line<'static>> {
    let mut cuerpo = cabecera(rechazo, tema);
    cuerpo.push(linea(""));
    cuerpo.extend(custodia(rechazo, estrecho, tema));
    cuerpo.push(linea(""));
    cuerpo.push(linea("  Pararlo cierra todo lo anterior."));
    cuerpo
}

fn cuerpo_confirmacion(
    rechazo: &RechazoVersion,
    estrecho: bool,
    tema: &Tema,
) -> Vec<Line<'static>> {
    let mut cuerpo = cabecera(rechazo, tema);
    cuerpo.push(linea(""));
    cuerpo.extend(custodia(rechazo, estrecho, tema));
    cuerpo.push(linea(""));
    let aviso = match rechazo.trabajo {
        Some(_) => format!(
            "  Al pararlo, {}.",
            servidor_anterior::confirmacion(rechazo)
        ),
        None => "  Al pararlo se cerrará lo que custodie: la versión anterior no lo informa."
            .to_string(),
    };
    cuerpo.push(linea(aviso));
    cuerpo
}

fn cuerpo_sustituyendo(rechazo: &RechazoVersion, tema: &Tema) -> Vec<Line<'static>> {
    let mut cuerpo = cabecera(rechazo, tema);
    cuerpo.push(linea(""));
    cuerpo.push(linea(format!(
        "  Parando el servidor anterior{}",
        tema.glifos.puntos
    )));
    cuerpo.push(linea(
        "  Esperando a que libere el socket y el lock (máx. 5 s).",
    ));
    cuerpo
}

fn cuerpo_error(
    rechazo: &RechazoVersion,
    motivo: &str,
    anterior_parado: bool,
    estrecho: bool,
    tema: &Tema,
) -> Vec<Line<'static>> {
    let mut cuerpo = cabecera(rechazo, tema);
    cuerpo.push(linea(""));
    cuerpo.extend(custodia(rechazo, estrecho, tema));
    cuerpo.push(linea(""));
    cuerpo.push(Line::from(Span::styled(
        format!(
            "  {} No se pudo sustituir el servidor anterior",
            tema.glifos.error
        ),
        Style::default()
            .fg(tema.paleta.critico)
            .add_modifier(Modifier::BOLD),
    )));
    cuerpo.push(linea(format!("  {motivo}")));
    cuerpo.push(linea(if anterior_parado {
        "  El servidor anterior se paró, pero el nuevo no ha arrancado."
    } else {
        "  El servidor anterior sigue en marcha."
    }));
    cuerpo
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn rechazo() -> RechazoVersion {
        let mut rechazo = RechazoVersion::nueva(5);
        rechazo.version_app = Some("0.9.0".to_string());
        rechazo.pid = Some(48211);
        rechazo.clientes = Some(1);
        rechazo.trabajo = Some(crate::protocolo::TrabajoServidor {
            sesiones: 2,
            transferencias: 0,
            tuneles: 1,
            ejecuciones: 0,
        });
        rechazo
    }

    fn texto(linea: &Line<'static>) -> String {
        linea
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn la_cabecera_alinea_las_dos_versiones() {
        let tema = Tema::respaldo();
        let lineas = cabecera(&rechazo(), &tema);
        assert_eq!(texto(&lineas[0]), "  Este magi:       1.0.0 · protocolo 6");
        assert_eq!(
            texto(&lineas[1]),
            "  Servidor activo: 0.9.0 · protocolo 5 · pid 48211"
        );
    }

    #[test]
    fn la_custodia_va_en_dos_lineas_o_una_por_recuento() {
        let tema = Tema::respaldo();
        let ancho = custodia(&rechazo(), false, &tema);
        assert_eq!(ancho.len(), 3);
        assert_eq!(texto(&ancho[0]), "  Custodia:  2 sesiones · 1 túnel");
        assert_eq!(
            texto(&ancho[1]),
            "             0 transferencias · 0 ejecuciones"
        );
        assert_eq!(
            texto(&ancho[2]),
            "             1 ventana más de la versión anterior"
        );
        let estrecho = custodia(&rechazo(), true, &tema);
        assert_eq!(estrecho.len(), 5);
        let sin_datos = custodia(&RechazoVersion::nueva(5), false, &tema);
        assert_eq!(sin_datos.len(), 1);
    }
}
