//! Ficha de host: formulario, bloque de túneles y áreas de texto.
//!
//! Todo se deriva del área en cada pintado (Fase 7): el formulario se construye
//! para el ancho y el modo, con la posición de cada campo; el desplazamiento
//! sale del campo con el foco, que siempre queda a la vista, y los desplegables
//! y las sugerencias se anclan a esa posición sin salirse del área. En modo
//! estrecho (menos de 100 columnas) las etiquetas van encima de los campos.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, CampoFicha, Ficha};
use crate::config::SeccionDeliberacion;
use crate::modelo::Tunel;
use crate::protocolo::EstadoTunelRemoto;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, AreaTexto, CampoTexto, Desplegable};
use crate::ui::disposicion::{self, Disposicion};

/// Ancho de la lista de un desplegable abierto.
const ANCHO_DESPLEGABLE: u16 = 34;
/// Opciones que enseña a la vez un desplegable abierto, si caben.
const ALTO_MAXIMO_DESPLEGABLE: u16 = 8;
/// Ancho y filas de la lista de sugerencias de etiquetas.
const ANCHO_SUGERENCIAS: u16 = 30;
const ALTO_MAXIMO_SUGERENCIAS: u16 = 6;
/// Lo que se deja como mínimo debajo del formulario: el bloque de túneles
/// (cabecera y una fila) y las áreas de texto (bordes y una línea). Con
/// reenvíos en las opciones extra se añade la fila de su aviso.
const ALTO_MINIMO_DEBAJO: u16 = 2 + 3;
/// Tope de filas de túnel que muestra el bloque.
const MAX_FILAS_TUNELES: usize = 4;
/// Columna en la que empiezan los campos de ruta, patrón y comando del bloque
/// «Verificaciones previas» en modo normal: sangría, casilla y la etiqueta más
/// larga.
const COLUMNA_VERIFICACION: usize = 24;
/// Sangría de los campos y ancho de sus etiquetas en modo normal: el valor
/// empieza tras `[ `.
const SANGRIA: usize = 2;
const ANCHO_ETIQUETA: usize = 13;
/// En modo estrecho, los campos de una verificación van bajo el texto de su
/// casilla (`  [x] `).
const SANGRIA_VERIFICACION: usize = 6;
/// Ancho preferido del valor de los desplegables.
const ANCHO_GRUPO: usize = 26;
const ANCHO_DESPLEGABLE_CAMPO: usize = 40;
/// Hueco del puerto: cinco cifras, el cursor y margen.
const ANCHO_PUERTO: usize = 8;
/// Cursor de los campos de texto (el mismo que pinta `componentes`).
const CURSOR: char = '\u{2503}';

// ---------------------------------------------------------------- formulario

/// Dónde cae un campo en el formulario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosicionCampo {
    pub campo: CampoFicha,
    /// Primera línea que ocupa: la de su etiqueta en modo estrecho.
    pub primera: u16,
    /// Línea de su valor.
    pub linea: u16,
    /// Columna en la que empieza el valor: ahí se anclan los desplegables y
    /// las sugerencias.
    pub columna: u16,
}

/// Las líneas del formulario (sin túneles ni áreas de texto) para un ancho y
/// un modo, con la posición de cada campo. Se construye en cada pintado.
pub struct Formulario {
    pub lineas: Vec<Line<'static>>,
    pub posiciones: Vec<PosicionCampo>,
}

impl Formulario {
    pub fn alto(&self) -> u16 {
        self.lineas.len() as u16
    }

    /// Posición de un campo; el bloque de túneles y las áreas de texto no
    /// viven en el formulario sino en su propia región, así que no tienen.
    pub fn posicion(&self, campo: CampoFicha) -> Option<PosicionCampo> {
        self.posiciones
            .iter()
            .find(|posicion| posicion.campo == campo)
            .copied()
    }

    /// Primera línea visible cuando solo caben `alto` líneas: el campo con el
    /// foco (etiqueta y valor) y la línea siguiente (el patrón bajo la ruta,
    /// la pista de la salud) quedan dentro; si no cabe todo, manda el valor.
    /// Se desplaza lo justo desde arriba y nunca se pasa del final. Con el
    /// foco fuera del formulario (túneles, áreas de texto) se ve el final, que
    /// es lo que queda junto a ellos.
    pub fn desplazamiento(&self, campo: CampoFicha, alto: u16) -> u16 {
        let maximo = self.alto().saturating_sub(alto);
        let Some(posicion) = self.posicion(campo) else {
            return maximo;
        };
        let con_siguiente = (posicion.linea + 2).saturating_sub(alto);
        let desde = if con_siguiente <= posicion.primera {
            con_siguiente
        } else {
            (posicion.linea + 1).saturating_sub(alto)
        };
        desde.min(maximo)
    }

    /// ¿Hay alguna línea con contenido entre `desde` y `hasta` (sin incluir)?
    /// Las líneas en blanco que separan los bloques no cuentan: una marca de
    /// «hay más» que solo esconde un separador engaña.
    pub fn hay_contenido(&self, desde: u16, hasta: u16) -> bool {
        let hasta = usize::from(hasta).min(self.lineas.len());
        let desde = usize::from(desde).min(hasta);
        self.lineas[desde..hasta].iter().any(|linea| {
            linea
                .spans
                .iter()
                .any(|span| !span.content.trim().is_empty())
        })
    }
}

/// Construye el formulario. `agente_no_disponible` añade el aviso al campo
/// Identidad. En ASCII no queda ningún glifo Unicode.
pub fn formulario(
    ficha: &Ficha,
    tema: &Tema,
    agente_no_disponible: bool,
    deliberacion: &SeccionDeliberacion,
    ancho: u16,
    estrecho: bool,
) -> Formulario {
    let mut constructor = Constructor {
        ficha,
        tema,
        ancho: usize::from(ancho),
        estrecho,
        lineas: Vec::with_capacity(48),
        posiciones: Vec::with_capacity(crate::app::ORDEN_CAMPOS.len()),
    };
    constructor.identificacion();
    constructor.acceso(agente_no_disponible);
    constructor.al_conectar();
    constructor.verificaciones(deliberacion);
    constructor.vacia();
    let Constructor {
        lineas, posiciones, ..
    } = constructor;
    let lineas = lineas
        .into_iter()
        .map(|linea| degradar(linea, tema.ascii))
        .collect();
    Formulario { lineas, posiciones }
}

/// Va añadiendo líneas al formulario y anotando dónde cae cada campo.
struct Constructor<'a> {
    ficha: &'a Ficha,
    tema: &'a Tema,
    ancho: usize,
    estrecho: bool,
    lineas: Vec<Line<'static>>,
    posiciones: Vec<PosicionCampo>,
}

impl Constructor<'_> {
    fn activo(&self, campo: CampoFicha) -> bool {
        self.ficha.campo == campo
    }

    fn estilo(&self, campo: CampoFicha) -> Style {
        estilo_campo(self.tema, self.activo(campo))
    }

    fn estilo_etiqueta(&self, enfocada: bool) -> Style {
        Style::default().fg(if enfocada {
            self.tema.paleta.acento
        } else {
            self.tema.paleta.inactivo
        })
    }

    fn corchete(&self, texto: &'static str) -> Span<'static> {
        Span::styled(texto, Style::default().fg(self.tema.paleta.acento))
    }

    fn pista(&self, texto: String) -> Span<'static> {
        Span::styled(texto, Style::default().fg(self.tema.paleta.texto))
    }

    /// Línea que se añadirá a continuación.
    fn siguiente(&self) -> u16 {
        self.lineas.len() as u16
    }

    /// Columnas libres entre los corchetes de un valor que empieza en
    /// `columna`.
    fn hueco(&self, columna: usize) -> usize {
        self.ancho.saturating_sub(columna + 2)
    }

    fn anotar(&mut self, campo: CampoFicha, primera: u16, linea: u16, columna: usize) {
        self.posiciones.push(PosicionCampo {
            campo,
            primera,
            linea,
            columna: columna as u16,
        });
    }

    fn vacia(&mut self) {
        self.lineas.push(Line::from(""));
    }

    fn cabecera(&mut self, texto: &str) {
        let texto = disposicion::recortar(texto, self.ancho.saturating_sub(1), self.tema.ascii);
        self.lineas.push(cabecera(&texto, self.tema));
    }

    /// Un campo `[ valor ]`: en normal, con la etiqueta a la izquierda; en
    /// estrecho, con la etiqueta encima. `valor` recibe las columnas libres
    /// entre los corchetes.
    fn campo(
        &mut self,
        campo: CampoFicha,
        etiqueta: &str,
        valor: impl FnOnce(usize) -> Span<'static>,
    ) {
        let estilo_etiqueta = self.estilo_etiqueta(self.activo(campo));
        let primera = self.siguiente();
        let (columna, mut spans) = if self.estrecho {
            self.lineas.push(Line::from(vec![
                Span::raw(" ".repeat(SANGRIA)),
                Span::styled(etiqueta.to_string(), estilo_etiqueta),
            ]));
            (SANGRIA + 2, vec![Span::raw(" ".repeat(SANGRIA))])
        } else {
            (
                SANGRIA + ANCHO_ETIQUETA + 2,
                vec![
                    Span::raw(" ".repeat(SANGRIA)),
                    Span::styled(format!("{etiqueta:<ANCHO_ETIQUETA$}"), estilo_etiqueta),
                ],
            )
        };
        spans.push(self.corchete("[ "));
        spans.push(valor(self.hueco(columna)));
        spans.push(self.corchete(" ]"));
        let linea = self.siguiente();
        self.lineas.push(Line::from(spans));
        self.anotar(campo, primera, linea, columna);
    }

    /// Un campo de texto recortado a su hueco alrededor del cursor.
    fn campo_texto(&mut self, campo: CampoFicha, etiqueta: &str, texto: &CampoTexto) {
        let activo = self.activo(campo);
        let estilo = self.estilo(campo);
        let ascii = self.tema.ascii;
        self.campo(campo, etiqueta, |hueco| {
            span_ajustado(texto, activo, estilo, hueco, ascii)
        });
    }

    /// Un desplegable con su valor recortado a `preferido` o a lo que quepa.
    fn campo_desplegable(
        &mut self,
        campo: CampoFicha,
        etiqueta: &str,
        desplegable: &Desplegable,
        preferido: usize,
    ) {
        let activo = self.activo(campo);
        let tema = self.tema;
        self.campo(campo, etiqueta, |hueco| {
            valor_desplegable(
                desplegable,
                activo,
                tema,
                preferido.min(hueco.saturating_sub(2)),
            )
        });
    }

    fn identificacion(&mut self) {
        let ficha = self.ficha;
        // En estrecho no hay línea en blanco bajo el título: cada fila cuenta.
        if !self.estrecho {
            self.vacia();
        }
        self.cabecera("IDENTIFICACIÓN");
        self.campo_texto(CampoFicha::Nombre, "Nombre", &ficha.nombre);
        self.campo_texto(CampoFicha::Direccion, "Dirección", &ficha.direccion);
        if self.estrecho {
            self.campo_texto(CampoFicha::Puerto, "Puerto", &ficha.puerto);
            self.campo_desplegable(CampoFicha::Grupo, "Grupo", &ficha.grupo, ANCHO_GRUPO);
        } else {
            self.puerto_y_grupo();
        }
        self.campo_texto(CampoFicha::Etiquetas, "Etiquetas", &ficha.etiquetas);
        self.vacia();
    }

    /// Modo normal: puerto y grupo en la misma línea.
    fn puerto_y_grupo(&mut self) {
        let ficha = self.ficha;
        let linea = self.siguiente();
        let puerto = span_ajustado(
            &ficha.puerto,
            self.activo(CampoFicha::Puerto),
            self.estilo(CampoFicha::Puerto),
            ANCHO_PUERTO,
            self.tema.ascii,
        );
        let columna_puerto = SANGRIA + ANCHO_ETIQUETA + 2;
        let separacion = "     ";
        let etiqueta_grupo = "Grupo";
        let columna_grupo = columna_puerto
            + puerto.content.chars().count()
            + 2
            + separacion.len()
            + etiqueta_grupo.len()
            + 1;
        let ancho_grupo = ANCHO_GRUPO.min(self.hueco(columna_grupo));
        let spans = vec![
            Span::raw(" ".repeat(SANGRIA)),
            Span::styled(
                format!("{:<ANCHO_ETIQUETA$}", "Puerto"),
                self.estilo_etiqueta(self.activo(CampoFicha::Puerto)),
            ),
            self.corchete("[ "),
            puerto,
            self.corchete(" ]"),
            Span::raw(separacion),
            Span::styled(
                etiqueta_grupo,
                self.estilo_etiqueta(self.activo(CampoFicha::Grupo)),
            ),
            Span::raw(" "),
            valor_desplegable(
                &ficha.grupo,
                self.activo(CampoFicha::Grupo),
                self.tema,
                ancho_grupo,
            ),
        ];
        self.lineas.push(Line::from(spans));
        self.anotar(CampoFicha::Puerto, linea, linea, columna_puerto);
        self.anotar(CampoFicha::Grupo, linea, linea, columna_grupo);
    }

    fn acceso(&mut self, agente_no_disponible: bool) {
        let ficha = self.ficha;
        let tema = self.tema;
        self.cabecera("ACCESO");
        self.campo_texto(CampoFicha::Usuario, "Usuario", &ficha.usuario);
        self.campo(CampoFicha::Identidad, "Identidad", |hueco| {
            valor_identidad(ficha, tema, agente_no_disponible, hueco)
        });
        self.campo_desplegable(
            CampoFicha::Salto,
            "Salto vía",
            &ficha.salto,
            ANCHO_DESPLEGABLE_CAMPO,
        );
        self.vacia();
    }

    fn al_conectar(&mut self) {
        let ficha = self.ficha;
        let tema = self.tema;
        self.cabecera("AL CONECTAR");
        self.campo(CampoFicha::Snippet, "Snippet", |hueco| {
            valor_snippet(
                ficha,
                tema,
                ANCHO_DESPLEGABLE_CAMPO.min(hueco.saturating_sub(2)),
            )
        });
        self.multiplexar();
        self.mantener();
        self.vacia();
    }

    /// `Multiplexar [x] ControlMaster auto al exportar`.
    fn multiplexar(&mut self) {
        let campo = CampoFicha::Multiplexar;
        let etiqueta = Span::styled("Multiplexar", self.estilo_etiqueta(self.activo(campo)));
        let primera = self.siguiente();
        let mut spans = vec![Span::raw(" ".repeat(SANGRIA))];
        if self.estrecho {
            spans.push(etiqueta);
            self.lineas.push(Line::from(spans));
            spans = vec![Span::raw(" ".repeat(SANGRIA))];
        } else {
            spans.push(etiqueta);
            spans.push(Span::raw("   "));
        }
        let columna: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        let descripcion = disposicion::recortar(
            " ControlMaster auto al exportar",
            self.ancho.saturating_sub(columna + 3),
            self.tema.ascii,
        );
        spans.push(Span::styled(
            casilla(self.ficha.multiplexar).to_string(),
            self.estilo(campo),
        ));
        spans.push(self.pista(descripcion));
        let linea = self.siguiente();
        self.lineas.push(Line::from(spans));
        self.anotar(campo, primera, linea, columna);
    }

    /// `Mantener [x] keepalive cada [ 30 ] s`.
    fn mantener(&mut self) {
        let ficha = self.ficha;
        let enfocada = self.activo(CampoFicha::Mantener) || self.activo(CampoFicha::Keepalive);
        let etiqueta = Span::styled("Mantener", self.estilo_etiqueta(enfocada));
        let primera = self.siguiente();
        let mut spans = vec![Span::raw(" ".repeat(SANGRIA))];
        if self.estrecho {
            spans.push(etiqueta);
            self.lineas.push(Line::from(spans));
            spans = vec![Span::raw(" ".repeat(SANGRIA))];
        } else {
            spans.push(etiqueta);
            spans.push(Span::raw("      "));
        }
        let columna: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        let texto_cada = " keepalive cada ";
        let columna_keepalive = columna + 3 + texto_cada.chars().count() + 2;
        spans.push(Span::styled(
            casilla(ficha.mantener).to_string(),
            self.estilo(CampoFicha::Mantener),
        ));
        spans.push(self.pista(texto_cada.to_string()));
        spans.push(self.corchete("[ "));
        spans.push(span_ajustado(
            &ficha.keepalive,
            self.activo(CampoFicha::Keepalive) && ficha.mantener,
            self.estilo(CampoFicha::Keepalive),
            ANCHO_PUERTO.min(self.hueco(columna_keepalive + 2)),
            self.tema.ascii,
        ));
        spans.push(self.pista(" ] s".to_string()));
        let linea = self.siguiente();
        self.lineas.push(Line::from(spans));
        self.anotar(CampoFicha::Mantener, primera, linea, columna);
        self.anotar(CampoFicha::Keepalive, primera, linea, columna_keepalive);
    }

    /// Bloque «Verificaciones previas (deliberación MAGI)»: cabecera, salud,
    /// backup (ruta y, debajo, patrón) y tests (comando local).
    fn verificaciones(&mut self, deliberacion: &SeccionDeliberacion) {
        let ficha = self.ficha;
        self.cabecera("VERIFICACIONES PREVIAS (deliberación MAGI)");
        let pista_salud = format!(
            "(último sondeo NOMINAL < {} min)",
            deliberacion.salud_max_min
        );
        let horas = format!("(< {} h)", deliberacion.backup_horas);
        if self.estrecho {
            self.verificaciones_estrechas(&pista_salud, &horas);
            return;
        }
        let ancho = self.ancho;

        let mut salud = inicio_verificacion(
            ficha,
            self.tema,
            CampoFicha::Salud,
            ficha.salud,
            "salud del host",
            true,
        );
        salud.push(self.pista(pista_salud));
        let linea = self.siguiente();
        self.lineas.push(Line::from(salud));
        self.anotar(CampoFicha::Salud, linea, linea, SANGRIA);

        // Cada campo se queda con lo que deja la línea tras etiquetas y
        // corchetes.
        let mut backup = inicio_verificacion(
            ficha,
            self.tema,
            CampoFicha::Backup,
            ficha.backup,
            "backup reciente",
            true,
        );
        backup.extend(campo_verificacion(
            ficha,
            self.tema,
            "ruta",
            CampoFicha::BackupRuta,
            ficha.backup,
            ancho.saturating_sub(COLUMNA_VERIFICACION + "ruta [  ]".len()),
        ));
        let linea = self.siguiente();
        self.lineas.push(Line::from(backup));
        self.anotar(CampoFicha::Backup, linea, linea, SANGRIA);
        self.anotar(
            CampoFicha::BackupRuta,
            linea,
            linea,
            COLUMNA_VERIFICACION + "ruta [ ".len(),
        );

        let horas = format!("  {horas}");
        let mut patron = vec![Span::raw(" ".repeat(COLUMNA_VERIFICACION))];
        patron.extend(campo_verificacion(
            ficha,
            self.tema,
            "patrón",
            CampoFicha::BackupPatron,
            ficha.backup,
            ancho.saturating_sub(
                COLUMNA_VERIFICACION + "patrón [  ]".chars().count() + horas.chars().count(),
            ),
        ));
        patron.push(self.pista(horas));
        let linea = self.siguiente();
        self.lineas.push(Line::from(patron));
        self.anotar(
            CampoFicha::BackupPatron,
            linea,
            linea,
            COLUMNA_VERIFICACION + "patrón [ ".chars().count(),
        );

        let mut tests = inicio_verificacion(
            ficha,
            self.tema,
            CampoFicha::Tests,
            ficha.tests,
            "tests en verde",
            true,
        );
        tests.extend(campo_verificacion(
            ficha,
            self.tema,
            "",
            CampoFicha::TestsComando,
            ficha.tests,
            ancho.saturating_sub(COLUMNA_VERIFICACION + "[  ]".len()),
        ));
        let linea = self.siguiente();
        self.lineas.push(Line::from(tests));
        self.anotar(CampoFicha::Tests, linea, linea, SANGRIA);
        self.anotar(
            CampoFicha::TestsComando,
            linea,
            linea,
            COLUMNA_VERIFICACION + "[ ".len(),
        );
    }

    /// Modo estrecho: cada casilla en su línea y, debajo, sus campos con la
    /// etiqueta encima.
    fn verificaciones_estrechas(&mut self, pista_salud: &str, horas: &str) {
        let ficha = self.ficha;
        let ascii = self.tema.ascii;
        let hueco_texto = self.ancho.saturating_sub(SANGRIA_VERIFICACION);

        self.casilla_sola(CampoFicha::Salud, ficha.salud, "salud del host");
        let pista = self.pista(disposicion::recortar(pista_salud, hueco_texto, ascii));
        self.lineas.push(Line::from(vec![
            Span::raw(" ".repeat(SANGRIA_VERIFICACION)),
            pista,
        ]));

        self.casilla_sola(CampoFicha::Backup, ficha.backup, "backup reciente");
        self.subcampo_verificacion(CampoFicha::BackupRuta, "ruta", None, ficha.backup);
        self.subcampo_verificacion(
            CampoFicha::BackupPatron,
            "patrón",
            Some(horas),
            ficha.backup,
        );

        self.casilla_sola(CampoFicha::Tests, ficha.tests, "tests en verde");
        self.subcampo_verificacion(CampoFicha::TestsComando, "comando", None, ficha.tests);
    }

    /// Modo estrecho: `  [x] etiqueta` sola en su línea.
    fn casilla_sola(&mut self, campo: CampoFicha, marcada: bool, etiqueta: &str) {
        let spans = inicio_verificacion(self.ficha, self.tema, campo, marcada, etiqueta, false);
        let linea = self.siguiente();
        self.lineas.push(Line::from(spans));
        self.anotar(campo, linea, linea, SANGRIA);
    }

    /// Modo estrecho: `etiqueta  (pista)` y debajo `[ valor ]`, sangrados bajo
    /// el texto de su casilla.
    fn subcampo_verificacion(
        &mut self,
        campo: CampoFicha,
        etiqueta: &str,
        pista: Option<&str>,
        marcada: bool,
    ) {
        let sangria = " ".repeat(SANGRIA_VERIFICACION);
        let primera = self.siguiente();
        let mut cabeza = vec![
            Span::raw(sangria.clone()),
            Span::styled(
                etiqueta.to_string(),
                self.estilo_etiqueta(self.activo(campo)),
            ),
        ];
        if let Some(pista) = pista {
            cabeza.push(self.pista(format!("  {pista}")));
        }
        self.lineas.push(Line::from(cabeza));
        let columna = SANGRIA_VERIFICACION + 2;
        let mut valor = vec![Span::raw(sangria)];
        valor.extend(campo_verificacion(
            self.ficha,
            self.tema,
            "",
            campo,
            marcada,
            self.hueco(columna),
        ));
        let linea = self.siguiente();
        self.lineas.push(Line::from(valor));
        self.anotar(campo, primera, linea, columna);
    }
}

/// Valor del desplegable «Snippet al conectar»; el snippet que el host tenía
/// y ha dejado de ser apto se pinta en rojo, con su «(no apto)».
fn valor_snippet(ficha: &Ficha, tema: &Tema, ancho: usize) -> Span<'static> {
    let activo = ficha.campo == CampoFicha::Snippet;
    let valor = valor_desplegable(&ficha.snippet.desplegable, activo, tema, ancho);
    if ficha.snippet.en_no_apto() {
        return valor.style(estilo_campo(tema, activo).fg(tema.paleta.critico));
    }
    valor
}

/// Valor de identidad con el aviso de agente no disponible cuando aplica.
fn valor_identidad(
    ficha: &Ficha,
    tema: &Tema,
    agente_no_disponible: bool,
    hueco: usize,
) -> Span<'static> {
    let activo = ficha.campo == CampoFicha::Identidad;
    if !agente_no_disponible {
        return valor_desplegable(
            &ficha.identidad,
            activo,
            tema,
            ANCHO_DESPLEGABLE_CAMPO.min(hueco.saturating_sub(2)),
        );
    }
    let aviso = format!(
        " {} agente no disponible (solo ficheros)",
        tema.glifos.punto_medio
    );
    // El valor cede sitio al aviso, sin bajar de lo que ocupa la etiqueta.
    let etiqueta = ficha.identidad.etiqueta_seleccionada().chars().count();
    let ancho = ANCHO_DESPLEGABLE_CAMPO
        .min(hueco.saturating_sub(2 + aviso.chars().count()))
        .max(etiqueta.min(hueco.saturating_sub(2)));
    let valor = valor_desplegable(&ficha.identidad, activo, tema, ancho);
    Span::styled(
        disposicion::recortar(&format!("{}{aviso}", valor.content), hueco, tema.ascii),
        Style::default().fg(tema.paleta.inactivo),
    )
}

/// `  [x] etiqueta` de una verificación. En modo normal la etiqueta se
/// rellena hasta la columna de los campos (`rellenar`). Se ilumina con el foco
/// en la casilla o en cualquiera de sus campos.
fn inicio_verificacion(
    ficha: &Ficha,
    tema: &Tema,
    campo: CampoFicha,
    marcada: bool,
    etiqueta: &str,
    rellenar: bool,
) -> Vec<Span<'static>> {
    let enfocada = match campo {
        CampoFicha::Backup => matches!(
            ficha.campo,
            CampoFicha::Backup | CampoFicha::BackupRuta | CampoFicha::BackupPatron
        ),
        CampoFicha::Tests => matches!(ficha.campo, CampoFicha::Tests | CampoFicha::TestsComando),
        _ => ficha.campo == campo,
    };
    // Sangría (2), casilla (3) y un espacio: la etiqueta llena el resto hasta
    // los campos. La salud no tiene campos: solo la separa de su pista.
    let ancho_etiqueta = COLUMNA_VERIFICACION - 6;
    let etiqueta = if !rellenar {
        etiqueta.to_string()
    } else if campo == CampoFicha::Salud {
        format!("{etiqueta} ")
    } else {
        format!("{etiqueta:<ancho_etiqueta$}")
    };
    vec![
        Span::raw(" ".repeat(SANGRIA)),
        Span::styled(
            casilla(marcada).to_string(),
            estilo_campo(tema, ficha.campo == campo),
        ),
        Span::raw(" "),
        Span::styled(
            etiqueta,
            Style::default().fg(if enfocada {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
    ]
}

/// `etiqueta [ valor ]` de un campo de una verificación, con el valor
/// recortado a `hueco` columnas. Con la casilla desmarcada el valor se
/// conserva y se puede editar, pero va apagado: no se exige.
fn campo_verificacion(
    ficha: &Ficha,
    tema: &Tema,
    etiqueta: &str,
    campo: CampoFicha,
    marcada: bool,
    hueco: usize,
) -> Vec<Span<'static>> {
    let texto = match campo {
        CampoFicha::BackupRuta => &ficha.backup_ruta,
        CampoFicha::BackupPatron => &ficha.backup_patron,
        _ => &ficha.tests_comando,
    };
    let activo = ficha.campo == campo;
    let estilo = if activo || marcada {
        estilo_campo(tema, activo)
    } else {
        Style::default().fg(tema.paleta.inactivo)
    };
    let mut spans = Vec::with_capacity(4);
    if !etiqueta.is_empty() {
        spans.push(Span::styled(
            format!("{etiqueta} "),
            Style::default().fg(if activo {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ));
    }
    spans.push(Span::styled("[ ", Style::default().fg(tema.paleta.acento)));
    spans.push(span_ajustado(texto, activo, estilo, hueco, tema.ascii));
    spans.push(Span::styled(" ]", Style::default().fg(tema.paleta.acento)));
    spans
}

/// Valor de un campo de texto recortado a `ancho` columnas alrededor del
/// cursor, con `…` (`~` en ASCII) donde se corta: un comando largo sigue
/// viéndose por donde se escribe. Sin el foco se ve el principio.
fn span_ajustado(
    campo: &CampoTexto,
    activo: bool,
    estilo: Style,
    ancho: usize,
    ascii: bool,
) -> Span<'static> {
    Span::styled(
        ajustar(&campo.texto, activo.then_some(campo.cursor), ancho, ascii),
        estilo,
    )
}

/// `texto` con el cursor (si lo lleva) y recortado a `ancho` columnas: la
/// ventana sigue al cursor y marca por dónde se corta.
fn ajustar(texto: &str, cursor: Option<usize>, ancho: usize, ascii: bool) -> String {
    let mut caracteres: Vec<char> = texto.chars().collect();
    let cursor = cursor.map(|cursor| cursor.min(caracteres.len()));
    if let Some(cursor) = cursor {
        caracteres.insert(cursor, if ascii { '|' } else { CURSOR });
    }
    if ancho < 3 || caracteres.len() <= ancho {
        return caracteres.into_iter().collect();
    }
    let marca = if ascii { '~' } else { '…' };
    // El cursor no cae nunca en la última columna si detrás queda texto: ahí
    // va la marca.
    let inicio = (cursor.unwrap_or(0) + 2)
        .saturating_sub(ancho)
        .min(caracteres.len() - ancho);
    let mut ventana: Vec<char> = caracteres[inicio..inicio + ancho].to_vec();
    if inicio > 0 {
        ventana[0] = marca;
    }
    if inicio + ancho < caracteres.len() {
        ventana[ancho - 1] = marca;
    }
    ventana.into_iter().collect()
}

// ------------------------------------------------------------------- pintado

/// Etiqueta del desplegable de un campo.
fn desplegable_de(campo: CampoFicha, ficha: &Ficha) -> Option<&Desplegable> {
    match campo {
        CampoFicha::Grupo => Some(&ficha.grupo),
        CampoFicha::Identidad => Some(&ficha.identidad),
        CampoFicha::Salto => Some(&ficha.salto),
        CampoFicha::Snippet => Some(&ficha.snippet.desplegable),
        _ => None,
    }
}

/// Título del marco, recortado al ancho.
fn titulo(ficha: &Ficha, tema: &Tema, ancho: u16) -> String {
    let punto = tema.glifos.punto_medio;
    let titulo = if let Some(id) = ficha.host_id {
        format!(
            "MAGI {punto} HOST {punto} {} {punto} editar {punto} #{id}",
            ficha.titulo
        )
    } else {
        format!("MAGI {punto} HOST {punto} {} {punto} nuevo", ficha.titulo)
    };
    let titulo = disposicion::recortar(&titulo, usize::from(ancho).saturating_sub(4), tema.ascii);
    if tema.ascii {
        titulo.chars().map(a_ascii).collect()
    } else {
        titulo
    }
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let Some(ficha) = &app.ficha else {
        return;
    };
    let bloque = super::bloque(&titulo(ficha, tema, area.width), tema);
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    // En ventanas muy anchas el formulario no crece sin límite: se centra.
    let contenido = disposicion::limitar_ancho(interior, disposicion::ANCHO_MAX_DETALLE);
    let agente_no_disponible =
        app.identidades.agente.is_none() && app.identidades.aviso_agente.is_some();
    // La última columna queda libre para las marcas de «hay más arriba o
    // abajo».
    let formulario = formulario(
        ficha,
        tema,
        agente_no_disponible,
        &app.config.deliberacion,
        contenido.width.saturating_sub(1),
        disp.estrecho(),
    );
    let total = formulario.alto();
    // Si no cabe entero, el formulario cede lo justo para que debajo sigan
    // viéndose los túneles (con el aviso de reenvíos, si lo hay) y las áreas
    // de texto, y se desplaza con el foco.
    let debajo = ALTO_MINIMO_DEBAJO + u16::from(reenvios_en_opciones(ficha) > 0);
    let alto_formulario = total.min(contenido.height.saturating_sub(debajo));
    // El bloque de túneles va entre los campos y las áreas de texto; se
    // recorta a lo que quede libre para no dejar sin sitio a los servicios.
    let alto_tuneles =
        alto_bloque_tuneles(app, ficha).min(contenido.height.saturating_sub(alto_formulario + 3));
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(alto_formulario),
            Constraint::Length(alto_tuneles),
            Constraint::Min(3),
        ])
        .split(contenido);
    let region = trozos[0];
    let desplazamiento = formulario.desplazamiento(ficha.campo, region.height);
    // Dónde queda en pantalla un campo, si su valor está a la vista.
    let ancla = |campo: CampoFicha| {
        let posicion = formulario.posicion(campo)?;
        let linea = posicion.linea.checked_sub(desplazamiento)?;
        (linea < region.height).then_some(Ancla {
            x: region.x + posicion.columna,
            primera: region.y + posicion.primera.saturating_sub(desplazamiento),
            fila: region.y + linea,
        })
    };
    let ancla_desplegable = ficha
        .desplegable_abierto
        .and_then(|campo| Some((desplegable_de(campo, ficha)?, ancla(campo)?)));
    let ancla_sugerencias = if ficha.campo == CampoFicha::Etiquetas && !ficha.sugerencias.is_empty()
    {
        ancla(CampoFicha::Etiquetas)
    } else {
        None
    };
    let arriba = formulario.hay_contenido(0, desplazamiento);
    let abajo = formulario.hay_contenido(desplazamiento.saturating_add(region.height), total);
    marco.render_widget(
        Paragraph::new(formulario.lineas).scroll((desplazamiento, 0)),
        region,
    );
    dibujar_marcas_desplazamiento(marco, region, arriba, abajo, tema);
    dibujar_tuneles(marco, trozos[1], app, ficha);

    let columnas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(trozos[2]);
    let servicios = ("SERVICIOS (systemd, una por línea)", "SERVICIOS");
    let opciones = (
        "OPCIONES EXTRA (ssh_config, una por línea)",
        "OPCIONES EXTRA",
    );
    // Los dos títulos largos o los dos cortos: mezclados parecen un error.
    let largos =
        cabe_titulo(servicios.0, columnas[0].width) && cabe_titulo(opciones.0, columnas[1].width);
    let titulo_de = |(largo, corto): (&'static str, &'static str), ancho: u16| {
        let elegido = if largos { largo } else { corto };
        if cabe_titulo(elegido, ancho) {
            format!(" {elegido} ")
        } else {
            disposicion::recortar(elegido, usize::from(ancho).saturating_sub(2), tema.ascii)
        }
    };
    dibujar_area_texto(
        marco,
        columnas[0],
        titulo_de(servicios, columnas[0].width),
        &ficha.servicios,
        ficha.campo == CampoFicha::Servicios,
        tema,
    );
    dibujar_area_texto(
        marco,
        columnas[1],
        titulo_de(opciones, columnas[1].width),
        &ficha.opciones,
        ficha.campo == CampoFicha::Opciones,
        tema,
    );

    // Las listas emergentes van encima, ancladas al valor de su campo y sin
    // salirse nunca del interior de la ficha.
    if let Some((desplegable, ancla)) = ancla_desplegable {
        dibujar_desplegable(marco, interior, ancla, desplegable, tema);
    }
    if let Some(ancla) = ancla_sugerencias {
        dibujar_sugerencias(marco, interior, ancla, ficha, tema);
    }
}

/// Dónde está en pantalla un campo a la vista: la columna de su valor, la
/// fila de su etiqueta (la misma que la del valor en modo normal) y la de su
/// valor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ancla {
    pub x: u16,
    pub primera: u16,
    pub fila: u16,
}

/// Rectángulo de una lista emergente de `ancho`×`alto` anclada bajo el valor
/// de un campo a partir de su columna o, si debajo no cabe, encima de su
/// etiqueta. Si no cabe entera por ningún lado, se queda con el lado más
/// holgado y encoge. Nunca se sale de `limite` ni tapa el campo.
pub fn colocar_emergente(limite: Rect, ancla: Ancla, ancho: u16, alto: u16) -> Rect {
    if limite.width == 0 || limite.height == 0 {
        return Rect {
            width: 0,
            height: 0,
            ..limite
        };
    }
    let ancho = ancho.min(limite.width);
    let x = ancla.x.clamp(limite.x, limite.right() - ancho);
    let fila = ancla.fila.clamp(limite.y, limite.bottom() - 1);
    let primera = ancla.primera.clamp(limite.y, fila);
    let debajo = limite.bottom() - (fila + 1);
    let encima = primera - limite.y;
    let (y, alto) = if alto <= debajo {
        (fila + 1, alto)
    } else if alto <= encima {
        (primera - alto, alto)
    } else if debajo >= encima {
        (fila + 1, debajo)
    } else {
        (limite.y, encima)
    };
    Rect {
        x,
        y,
        width: ancho,
        height: alto,
    }
}

// ------------------------------------------------------------------ túneles

/// Filas que necesita el bloque: cabecera, filas de túnel (una aunque no haya,
/// para el aviso de vacío) y el aviso de reenvíos sueltos.
fn alto_bloque_tuneles(app: &App, ficha: &Ficha) -> u16 {
    let filas = match ficha.host_id {
        Some(host_id) => app.tuneles_de_host(host_id).len().min(MAX_FILAS_TUNELES),
        None => 0,
    };
    let aviso = u16::from(reenvios_en_opciones(ficha) > 0);
    1 + filas.max(1) as u16 + aviso
}

fn dibujar_tuneles(marco: &mut Frame, area: Rect, app: &App, ficha: &Ficha) {
    if area.height == 0 || area.width < 12 {
        return;
    }
    let tema = &app.tema;
    let ancho = usize::from(area.width);
    let enfocado = ficha.campo == CampoFicha::Tuneles;
    let reenvios = reenvios_en_opciones(ficha);
    let aviso = u16::from(reenvios > 0);
    let mut lineas = vec![cabecera_tuneles(area.width, enfocado, tema)];
    let apagado = |texto: &str| {
        Line::from(Span::styled(
            disposicion::recortar(texto, ancho, tema.ascii),
            Style::default().fg(tema.paleta.inactivo),
        ))
    };

    match ficha.host_id {
        // Un host sin guardar no puede tener túneles colgando.
        None => lineas.push(apagado("    guarda el host para añadir túneles")),
        Some(host_id) => {
            let filas = app.tuneles_de_host(host_id);
            if filas.is_empty() {
                lineas.push(apagado("    aún no hay túneles: n para crear uno"));
            } else {
                // La ventana sigue a la selección con el alto que quede.
                let altura = (area.height.saturating_sub(1 + aviso) as usize).max(1);
                let seleccionado = ficha.indice_tunel_visible(filas.len());
                let inicio = inicio_ventana(seleccionado, filas.len(), altura);
                let ancho_nombre = filas
                    .iter()
                    .map(|tunel| tunel.nombre.chars().count())
                    .max()
                    .unwrap_or(8)
                    .clamp(8, 16);
                // Sangrías, marca, tipo y separadores: el resto es el tramo.
                let ancho_tramo = ancho.saturating_sub(19 + ancho_nombre).max(10);
                for (posicion, tunel) in filas.iter().enumerate().skip(inicio).take(altura) {
                    lineas.push(linea_tunel(
                        app,
                        tunel,
                        enfocado && posicion == seleccionado,
                        ancho_tramo,
                        ancho_nombre,
                    ));
                }
            }
        }
    }
    if reenvios > 0 {
        lineas.push(linea_reenvios(reenvios, ancho, tema));
    }
    let lineas: Vec<Line> = lineas
        .into_iter()
        .map(|linea| degradar(linea, tema.ascii))
        .collect();
    marco.render_widget(Paragraph::new(lineas), area);
}

/// «TÚNELES» con las teclas a la derecha: sin el foco solo el alta, con el
/// foco todas las del bloque.
fn cabecera_tuneles(ancho: u16, enfocado: bool, tema: &Tema) -> Line<'static> {
    let etiqueta = "TÚNELES";
    let punto = tema.glifos.punto_medio;
    let pista = if enfocado {
        format!("n nuevo {punto} e editar {punto} x borrar {punto} a automático")
    } else {
        "n nuevo".to_string()
    };
    // Sangría, etiqueta, un espacio que las separa siempre de la pista y la
    // última columna libre.
    let hueco = (ancho as usize).saturating_sub(2 + etiqueta.chars().count() + 1 + 1);
    let pista = disposicion::recortar(&pista, hueco, tema.ascii);
    let relleno = 1 + hueco.saturating_sub(pista.chars().count());
    Line::from(vec![
        Span::styled(
            format!("  {etiqueta}"),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(relleno)),
        Span::styled(
            pista,
            Style::default().fg(if enfocado {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
    ])
}

/// Fila: `[a] tipo  escucha → destino  nombre`, con el glifo de estado delante
/// de los que no están inactivos.
fn linea_tunel(
    app: &App,
    tunel: &Tunel,
    seleccionada: bool,
    ancho_tramo: usize,
    ancho_nombre: usize,
) -> Line<'static> {
    let tema = &app.tema;
    let estado = app.estado_de(tunel.id);
    let mut estilo = Style::default().fg(tema.paleta.texto);
    if seleccionada {
        estilo = estilo
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD);
    }
    // Sobre la fila seleccionada el glifo toma el color de la selección.
    let color_estado = if seleccionada {
        estilo
    } else {
        Style::default().fg(color_estado(estado, tema))
    };
    let glifo = if estado == EstadoTunelRemoto::Inactivo {
        " "
    } else {
        estado.glifo(tema.ascii)
    };
    let marca = if seleccionada {
        tema.glifos.seleccion
    } else {
        " "
    };
    let tramo = crate::ui::tuneles::tramo(tunel, app.tuneles_activos.get(&tunel.id));
    Line::from(vec![
        Span::styled(format!("  {marca} {glifo} "), color_estado),
        Span::styled(
            format!("{} ", if tunel.automatico { "[a]" } else { "[ ]" }),
            estilo,
        ),
        Span::styled(format!("{:<8} ", tunel.tipo.etiqueta()), estilo),
        Span::styled(
            disposicion::columna(
                &disposicion::recortar(&tramo, ancho_tramo.saturating_sub(2), tema.ascii),
                ancho_tramo,
                tema.ascii,
            ),
            estilo,
        ),
        Span::styled(
            disposicion::recortar(&tunel.nombre, ancho_nombre, tema.ascii),
            estilo,
        ),
    ])
}

/// Aviso de reenvíos que siguen en `opciones_extra`, en ámbar: se importan a
/// la tabla con `i` desde el bloque.
fn linea_reenvios(reenvios: usize, ancho: usize, tema: &Tema) -> Line<'static> {
    let sustantivo = if reenvios == 1 {
        "reenvío"
    } else {
        "reenvíos"
    };
    let texto = format!(
        "    {} hay {reenvios} {sustantivo} en opciones extra {} i importar a túneles",
        tema.glifos.error, tema.glifos.punto_medio
    );
    Line::from(Span::styled(
        disposicion::recortar(&texto, ancho, tema.ascii),
        Style::default().fg(tema.paleta.acento),
    ))
}

/// Primera fila visible: la selección siempre dentro de la ventana.
fn inicio_ventana(seleccionado: usize, total: usize, altura: usize) -> usize {
    let altura = altura.max(1);
    seleccionado
        .saturating_sub(altura - 1)
        .min(total.saturating_sub(altura))
}

/// Mismo criterio de color que la vista Túneles.
fn color_estado(estado: EstadoTunelRemoto, tema: &Tema) -> Color {
    match estado {
        EstadoTunelRemoto::Activo => tema.paleta.correcto,
        EstadoTunelRemoto::Activando | EstadoTunelRemoto::Parando => tema.paleta.acento,
        EstadoTunelRemoto::Caido => tema.paleta.critico,
        EstadoTunelRemoto::Inactivo => tema.paleta.inactivo,
    }
}

/// Líneas de `opciones_extra` que son directivas de reenvío: las mismas que
/// cuenta el aviso y las que importa `i`.
fn reenvios_en_opciones(ficha: &Ficha) -> usize {
    ficha
        .opciones
        .lineas
        .iter()
        .filter(|linea| es_reenvio(linea))
        .count()
}

/// Directiva de reenvío, ignorando espacios y comentarios.
fn es_reenvio(linea: &str) -> bool {
    let limpia = linea.trim();
    if limpia.is_empty() || limpia.starts_with('#') {
        return false;
    }
    matches!(
        limpia
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "localforward" | "remoteforward" | "dynamicforward"
    )
}

// ----------------------------------------------------------- áreas de texto

/// ¿Cabe ` titulo ` en el borde superior de un área de `ancho` columnas?
fn cabe_titulo(titulo: &str, ancho: u16) -> bool {
    titulo.chars().count() + 2 <= usize::from(ancho).saturating_sub(2)
}

/// Marcas de «hay más arriba» y «hay más abajo» en la última columna de la
/// región del formulario, que el formulario deja libre.
fn dibujar_marcas_desplazamiento(
    marco: &mut Frame,
    region: Rect,
    arriba: bool,
    abajo: bool,
    tema: &Tema,
) {
    if region.width == 0 || region.height == 0 {
        return;
    }
    let estilo = Style::default().fg(tema.paleta.inactivo);
    let x = region.right() - 1;
    let mut marca = |glifo: &'static str, y: u16| {
        marco.render_widget(
            Paragraph::new(Span::styled(glifo, estilo)),
            Rect::new(x, y, 1, 1),
        );
    };
    if arriba {
        marca(tema.glifos.arriba, region.y);
    }
    if abajo {
        marca(tema.glifos.abajo, region.bottom() - 1);
    }
}

/// Un área de texto con borde. Con el foco, la línea del cursor queda a la
/// vista (desplazamiento vertical) y, si es más larga que el área, se ve por
/// donde se escribe.
fn dibujar_area_texto(
    marco: &mut Frame,
    area: Rect,
    titulo: String,
    area_texto: &AreaTexto,
    activo: bool,
    tema: &Tema,
) {
    let color_borde = if activo {
        tema.paleta.acento
    } else {
        tema.paleta.inactivo
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(titulo, Style::default().fg(color_borde)))
        .border_style(Style::default().fg(color_borde));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let alto = usize::from(interior.height);
    let ancho = usize::from(interior.width);
    let desde = if activo {
        (area_texto.fila + 1).saturating_sub(alto)
    } else {
        0
    };
    let estilo = estilo_campo(tema, activo);
    let lineas: Vec<Line> = area_texto
        .lineas
        .iter()
        .enumerate()
        .skip(desde)
        .take(alto)
        .map(|(indice, linea)| {
            let cursor = (activo && indice == area_texto.fila).then_some(area_texto.columna);
            let texto = ajustar(linea, cursor, ancho, tema.ascii);
            degradar(Line::from(Span::styled(texto, estilo)), tema.ascii)
        })
        .collect();
    marco.render_widget(Paragraph::new(lineas), interior);
}

fn cabecera(texto: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {texto}"),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    ))
}

/// `valor ▾` rellenado a `ancho` columnas más la flecha.
fn valor_desplegable(
    desplegable: &Desplegable,
    activo: bool,
    tema: &Tema,
    ancho: usize,
) -> Span<'static> {
    let flecha = super::tecla(tema, "▾", "v");
    let recortado = disposicion::columna(desplegable.etiqueta_seleccionada(), ancho, tema.ascii);
    Span::styled(format!("{recortado} {flecha}"), estilo_campo(tema, activo))
}

// --------------------------------------------------------- listas emergentes

fn dibujar_desplegable(
    marco: &mut Frame,
    limite: Rect,
    ancla: Ancla,
    desplegable: &Desplegable,
    tema: &Tema,
) {
    let filtradas = desplegable.filtradas();
    let deseado = (filtradas.len().max(1) as u16).min(ALTO_MAXIMO_DESPLEGABLE) + 2;
    // Tan ancho como su opción más larga (bordes y margen incluidos), si cabe.
    let ancho = desplegable
        .opciones
        .iter()
        .map(|opcion| opcion.etiqueta.chars().count() as u16 + 3)
        .max()
        .unwrap_or(0)
        .max(ANCHO_DESPLEGABLE);
    let recta = colocar_emergente(limite, ancla, ancho, deseado);
    if recta.height < 3 || recta.width < 5 {
        return;
    }
    // La opción resaltada siempre a la vista, con las filas que haya.
    let filas = usize::from(recta.height - 2);
    let inicio = disposicion::ventana(0, desplegable.resaltado, filas, filtradas.len());
    let ancho_texto = usize::from(recta.width).saturating_sub(3);
    let mut lineas = Vec::new();
    for (posicion, indice) in filtradas.iter().enumerate().skip(inicio).take(filas) {
        let opcion = &desplegable.opciones[*indice];
        let estilo = if posicion == desplegable.resaltado {
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        lineas.push(Line::from(Span::styled(
            format!(
                " {}",
                disposicion::columna(&opcion.etiqueta, ancho_texto, tema.ascii)
            ),
            estilo,
        )));
    }
    if lineas.is_empty() {
        lineas.push(Line::from(Span::styled(
            disposicion::recortar(" (sin coincidencias)", ancho_texto + 1, tema.ascii),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let titulo = if desplegable.filtro.texto.is_empty() {
        String::new()
    } else {
        let texto = disposicion::recortar(
            &desplegable.filtro.texto,
            usize::from(recta.width).saturating_sub(4),
            tema.ascii,
        );
        format!(" {texto} ")
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(degradar(
            Line::from(Span::styled(
                titulo,
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            )),
            tema.ascii,
        ))
        .border_style(Style::default().fg(tema.paleta.acento));
    let lineas: Vec<Line> = lineas
        .into_iter()
        .map(|linea| degradar(linea, tema.ascii))
        .collect();
    marco.render_widget(Clear, recta);
    marco.render_widget(Paragraph::new(lineas).block(bloque), recta);
}

fn dibujar_sugerencias(marco: &mut Frame, limite: Rect, ancla: Ancla, ficha: &Ficha, tema: &Tema) {
    let total = ficha.sugerencias.len();
    let deseado = (total as u16).min(ALTO_MAXIMO_SUGERENCIAS) + 2;
    let recta = colocar_emergente(limite, ancla, ANCHO_SUGERENCIAS, deseado);
    if recta.height < 3 || recta.width < 5 {
        return;
    }
    let filas = usize::from(recta.height - 2);
    let inicio = disposicion::ventana(0, ficha.indice_sugerencia, filas, total);
    let ancho_texto = usize::from(recta.width).saturating_sub(3);
    let mut lineas = Vec::new();
    for (indice, sugerencia) in ficha
        .sugerencias
        .iter()
        .enumerate()
        .skip(inicio)
        .take(filas)
    {
        let estilo = if indice == ficha.indice_sugerencia {
            Style::default()
                .bg(tema.paleta.correcto)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        lineas.push(degradar(
            Line::from(Span::styled(
                format!(
                    " {}",
                    disposicion::recortar(sugerencia, ancho_texto, tema.ascii)
                ),
                estilo,
            )),
            tema.ascii,
        ));
    }
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            " etiquetas ",
            Style::default().fg(tema.paleta.correcto),
        ))
        .border_style(Style::default().fg(tema.paleta.correcto));
    marco.render_widget(Clear, recta);
    marco.render_widget(Paragraph::new(lineas).block(bloque), recta);
}

// --------------------------------------------------------------------- ASCII

/// Equivalente ASCII de un carácter (uno por uno, para no mover columnas).
/// Las letras, con tilde o sin ella, se quedan como están.
fn a_ascii(caracter: char) -> char {
    match caracter {
        c if c.is_ascii() || c.is_alphabetic() => c,
        '·' | '—' | '–' | '─' => '-',
        '…' => '~',
        '→' | '▸' | '▶' | '›' | '⤴' => '>',
        '←' | '‹' => '<',
        '▾' | '▼' | '↓' => 'v',
        '↑' | '▲' => '^',
        '┃' | '│' => '|',
        '×' | '✕' => 'x',
        '●' | '•' => '*',
        '○' | '◐' => 'o',
        _ => '?',
    }
}

/// En modo ASCII, cambia por su equivalente todo carácter que no sea ASCII
/// ni letra: también los que traen los datos (nombres, rutas, etiquetas de
/// los desplegables).
fn degradar(mut linea: Line<'static>, ascii: bool) -> Line<'static> {
    if !ascii {
        return linea;
    }
    for span in &mut linea.spans {
        if span
            .content
            .chars()
            .any(|caracter| !caracter.is_ascii() && !caracter.is_alphabetic())
        {
            span.content = span.content.chars().map(a_ascii).collect::<String>().into();
        }
    }
    linea
}

#[cfg(test)]
mod pruebas {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::pruebas_ficha::{ficha_con, snippets, verificaciones};
    use crate::app::ORDEN_CAMPOS;

    /// Alto del formulario en modo normal: los campos y sus cabeceras.
    const ALTO_NORMAL: u16 = 23;

    fn construir(ficha: &Ficha, ancho: u16, estrecho: bool, tema: &Tema) -> Formulario {
        formulario(
            ficha,
            tema,
            false,
            &SeccionDeliberacion::default(),
            ancho,
            estrecho,
        )
    }

    /// Pinta las líneas del formulario en un terminal de pruebas y devuelve
    /// cada fila como texto.
    fn pintar_con(ficha: &Ficha, ancho: u16, estrecho: bool, tema: &Tema) -> Vec<String> {
        let formulario = construir(ficha, ancho, estrecho, tema);
        let alto = formulario.alto();
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        terminal
            .draw(|marco| {
                marco.render_widget(Paragraph::new(formulario.lineas.clone()), marco.area())
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn pintar(ficha: &Ficha, ancho: u16) -> Vec<String> {
        let filas = pintar_con(ficha, ancho, false, &Tema::respaldo());
        assert_eq!(filas.len(), ALTO_NORMAL as usize);
        filas
    }

    fn linea_de(ficha: &Ficha, campo: CampoFicha) -> usize {
        construir(ficha, 140, false, &Tema::respaldo())
            .posicion(campo)
            .unwrap()
            .linea as usize
    }

    fn ficha_de_prueba() -> Ficha {
        let mut host = crate::modelo::host_de_prueba();
        host.snippet_al_conectar_id = Some(4);
        let mut ficha = ficha_con(Some(&host), &snippets(), Some(verificaciones()));
        ficha.tests_comando = CampoTexto::nuevo(
            "gh run list -R 4d3/cooperapp -L 1 --json conclusion -q '.[0].conclusion' \
             | grep -qx success",
        );
        ficha
    }

    #[test]
    fn se_pintan_el_snippet_al_conectar_y_las_verificaciones_previas() {
        let ficha = ficha_de_prueba();
        let filas = pintar(&ficha, 140);
        let fila = |campo: CampoFicha| filas[linea_de(&ficha, campo)].clone();
        assert_eq!(filas[12], " AL CONECTAR");
        assert!(
            fila(CampoFicha::Snippet).starts_with("  Snippet      [ tail log"),
            "{}",
            fila(CampoFicha::Snippet)
        );
        assert!(fila(CampoFicha::Multiplexar).starts_with("  Multiplexar"));
        assert_eq!(filas[17], " VERIFICACIONES PREVIAS (deliberación MAGI)");
        assert_eq!(
            fila(CampoFicha::Salud),
            "  [x] salud del host (último sondeo NOMINAL < 5 min)"
        );
        assert_eq!(
            fila(CampoFicha::BackupRuta),
            "  [x] backup reciente   ruta [ /var/backups/pg ]"
        );
        assert_eq!(
            fila(CampoFicha::BackupPatron),
            "                        patrón [ *.sql.gz ]  (< 24 h)"
        );
        assert_eq!(
            fila(CampoFicha::TestsComando),
            "  [x] tests en verde    [ gh run list -R 4d3/cooperapp -L 1 --json conclusion \
             -q '.[0].conclusion' | grep -qx success ]"
        );
    }

    #[test]
    fn las_lineas_de_los_campos_en_modo_normal_no_cambian() {
        let ficha = ficha_de_prueba();
        for (campo, linea) in [
            (CampoFicha::Nombre, 2),
            (CampoFicha::Direccion, 3),
            (CampoFicha::Puerto, 4),
            (CampoFicha::Grupo, 4),
            (CampoFicha::Etiquetas, 5),
            (CampoFicha::Usuario, 8),
            (CampoFicha::Identidad, 9),
            (CampoFicha::Salto, 10),
            (CampoFicha::Snippet, 13),
            (CampoFicha::Multiplexar, 14),
            (CampoFicha::Mantener, 15),
            (CampoFicha::Keepalive, 15),
            (CampoFicha::Salud, 18),
            (CampoFicha::Backup, 19),
            (CampoFicha::BackupRuta, 19),
            (CampoFicha::BackupPatron, 20),
            (CampoFicha::Tests, 21),
            (CampoFicha::TestsComando, 21),
        ] {
            assert_eq!(linea_de(&ficha, campo), linea, "{campo:?}");
        }
        let formulario = construir(&ficha, 140, false, &Tema::respaldo());
        for campo in [
            CampoFicha::Servicios,
            CampoFicha::Opciones,
            CampoFicha::Tuneles,
        ] {
            assert_eq!(formulario.posicion(campo), None);
        }
    }

    #[test]
    fn un_comando_largo_se_recorta_sin_salirse_de_la_linea() {
        let mut ficha = ficha_de_prueba();
        let linea = linea_de(&ficha, CampoFicha::TestsComando);
        let filas = pintar(&ficha, 80);
        let tests = &filas[linea];
        assert!(tests.ends_with("… ]"), "{tests}");
        assert_eq!(tests.chars().count(), 80);
        assert!(tests.contains("[ gh run list"));

        // Con el foco, se ve el final, donde está el cursor.
        ficha.campo = CampoFicha::TestsComando;
        let filas = pintar(&ficha, 80);
        let tests = &filas[linea];
        assert!(tests.ends_with("grep -qx success\u{2503} ]"), "{tests}");
        assert!(tests.contains("[ …"), "{tests}");
    }

    #[test]
    fn el_snippet_no_apto_se_pinta_con_su_marca() {
        let mut host = crate::modelo::host_de_prueba();
        host.snippet_al_conectar_id = Some(2);
        let ficha = ficha_con(Some(&host), &snippets(), None);
        let filas = pintar(&ficha, 100);
        assert!(filas[linea_de(&ficha, CampoFicha::Snippet)].contains("[ reiniciar (no apto)"));
        // Sin verificaciones, todas desmarcadas y con los campos vacíos.
        assert_eq!(
            filas[linea_de(&ficha, CampoFicha::Salud)],
            "  [ ] salud del host (último sondeo NOMINAL < 5 min)"
        );
        assert_eq!(
            filas[linea_de(&ficha, CampoFicha::Tests)],
            "  [ ] tests en verde    [  ]"
        );
    }

    #[test]
    fn el_formulario_se_desplaza_para_que_el_foco_se_vea() {
        let ficha = ficha_de_prueba();
        let formulario = construir(&ficha, 140, false, &Tema::respaldo());
        // Cabe entero: no se mueve.
        for campo in ORDEN_CAMPOS {
            assert_eq!(formulario.desplazamiento(campo, ALTO_NORMAL), 0);
        }
        // Diez líneas: arriba no se mueve; abajo, el foco y la línea siguiente
        // quedan dentro y nunca se pasa del final.
        let desplazamiento = |campo| formulario.desplazamiento(campo, 10);
        assert_eq!(desplazamiento(CampoFicha::Nombre), 0);
        assert_eq!(desplazamiento(CampoFicha::Snippet), 5);
        assert_eq!(desplazamiento(CampoFicha::BackupRuta), 11);
        assert_eq!(desplazamiento(CampoFicha::TestsComando), 13);
        assert_eq!(desplazamiento(CampoFicha::Tuneles), 13);
        for campo in ORDEN_CAMPOS {
            let desde = desplazamiento(campo);
            assert!(desde + 10 <= ALTO_NORMAL);
            if let Some(posicion) = formulario.posicion(campo) {
                assert!(posicion.linea >= desde && posicion.linea < desde + 10);
            }
        }
    }

    #[test]
    fn en_estrecho_las_etiquetas_van_encima_y_el_foco_se_ve_a_cualquier_alto() {
        let ficha = ficha_de_prueba();
        let tema = Tema::respaldo();
        let formulario = construir(&ficha, 58, true, &tema);
        let filas = pintar_con(&ficha, 58, true, &tema);
        assert_eq!(filas[0], " IDENTIFICACIÓN");
        assert_eq!(filas[1], "  Nombre");
        assert!(filas[2].starts_with("  [ "), "{}", filas[2]);
        // Cada campo con su etiqueta en la línea de encima (o su casilla).
        for campo in ORDEN_CAMPOS {
            let Some(posicion) = formulario.posicion(campo) else {
                continue;
            };
            assert!(posicion.primera <= posicion.linea, "{campo:?}");
            let valor = &filas[usize::from(posicion.linea)];
            assert!(
                valor.chars().count() <= 58,
                "{campo:?} se sale del ancho: {valor}"
            );
        }
        assert_eq!(
            filas[usize::from(formulario.posicion(CampoFicha::BackupRuta).unwrap().primera)],
            "      ruta"
        );
        let comando =
            &filas[usize::from(formulario.posicion(CampoFicha::TestsComando).unwrap().linea)];
        assert!(comando.starts_with("      [ gh run list"), "{comando}");
        assert!(comando.ends_with("… ]"), "{comando}");
        // A cualquier alto, el campo con el foco (etiqueta y valor) se ve.
        for alto in 2..=formulario.alto() {
            for campo in ORDEN_CAMPOS {
                let desde = formulario.desplazamiento(campo, alto);
                assert!(desde + alto <= formulario.alto());
                if let Some(posicion) = formulario.posicion(campo) {
                    assert!(
                        posicion.primera >= desde && posicion.linea < desde + alto,
                        "{campo:?} fuera de la vista con {alto} líneas"
                    );
                }
            }
        }
    }

    #[test]
    fn en_ascii_el_formulario_no_lleva_glifos_unicode() {
        let mut ficha = ficha_de_prueba();
        ficha.campo = CampoFicha::TestsComando;
        ficha.nombre = CampoTexto::nuevo("web → caché · ✕");
        let mut tema = Tema::respaldo();
        tema.ascii = true;
        tema.glifos = crate::tema::Glifos::ascii();
        for (ancho, estrecho) in [(140, false), (100, false), (58, true), (48, true)] {
            for fila in pintar_con(&ficha, ancho, estrecho, &tema) {
                assert!(
                    fila.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
                    "glifo Unicode a {ancho}: {fila}"
                );
            }
        }
    }

    #[test]
    fn un_texto_recortado_deja_el_cursor_a_la_vista() {
        let estilo = Style::default();
        let mut campo = CampoTexto::nuevo("abcdefghij");
        // Cabe: tal cual.
        assert_eq!(
            span_ajustado(&campo, false, estilo, 10, false).content,
            "abcdefghij"
        );
        // Sin foco, el principio.
        assert_eq!(
            span_ajustado(&campo, false, estilo, 6, false).content,
            "abcde…"
        );
        // Con el cursor al final, el final.
        assert_eq!(
            span_ajustado(&campo, true, estilo, 6, false).content,
            "…ghij\u{2503}"
        );
        // En ASCII, las marcas y el cursor también.
        assert_eq!(
            span_ajustado(&campo, true, estilo, 6, true).content,
            "~ghij|"
        );
        // Con el cursor en medio, recortado por los dos lados.
        campo.cursor = 5;
        let visto = span_ajustado(&campo, true, estilo, 6, false)
            .content
            .to_string();
        assert!(visto.starts_with('…') && visto.ends_with('…'), "{visto}");
        assert!(visto.contains('\u{2503}'), "{visto}");
    }

    #[test]
    fn la_lista_emergente_nunca_se_sale_ni_tapa_su_campo() {
        let limite = Rect::new(1, 1, 48, 11);
        // Un campo de una línea (modo normal) en la fila `fila`.
        let en = |x, fila| Ancla {
            x,
            primera: fila,
            fila,
        };
        // Cabe debajo.
        assert_eq!(
            colocar_emergente(limite, en(5, 2), 34, 7),
            Rect::new(5, 3, 34, 7)
        );
        // Debajo no cabe: encima.
        assert_eq!(
            colocar_emergente(limite, en(5, 10), 34, 7),
            Rect::new(5, 3, 34, 7)
        );
        // No cabe entera por ningún lado: el lado más holgado, encogida.
        assert_eq!(
            colocar_emergente(limite, en(5, 5), 34, 10),
            Rect::new(5, 6, 34, 6)
        );
        // Anclada demasiado a la derecha: se corre a la izquierda; más ancha
        // que el límite: lo ocupa entero.
        assert_eq!(colocar_emergente(limite, en(40, 2), 34, 4).x, 15);
        assert_eq!(
            colocar_emergente(limite, en(40, 2), 60, 4),
            Rect::new(1, 3, 48, 4)
        );
        // Con la etiqueta encima (modo estrecho), encima de la etiqueta.
        let estrecho = Ancla {
            x: 5,
            primera: 9,
            fila: 10,
        };
        assert_eq!(
            colocar_emergente(limite, estrecho, 34, 7),
            Rect::new(5, 2, 34, 7)
        );
        for fila in limite.y..limite.bottom() {
            for primera in [fila, fila.saturating_sub(1).max(limite.y)] {
                let ancla = Ancla {
                    x: 10,
                    primera,
                    fila,
                };
                let recta = colocar_emergente(limite, ancla, 34, 10);
                assert!(recta.y >= limite.y && recta.bottom() <= limite.bottom());
                for tapada in primera..=fila {
                    assert!(
                        tapada < recta.y || tapada >= recta.bottom(),
                        "tapa la fila {tapada} del campo {ancla:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn las_lineas_en_blanco_no_cuentan_para_las_marcas() {
        let ficha = ficha_de_prueba();
        let formulario = construir(&ficha, 140, false, &Tema::respaldo());
        // En modo normal el formulario empieza y acaba con una línea en blanco.
        assert!(!formulario.hay_contenido(0, 1));
        assert!(!formulario.hay_contenido(ALTO_NORMAL - 1, ALTO_NORMAL));
        assert!(formulario.hay_contenido(0, 2));
        assert!(formulario.hay_contenido(ALTO_NORMAL - 2, ALTO_NORMAL));
        // Rangos vacíos o fuera del final.
        assert!(!formulario.hay_contenido(5, 3));
        assert!(!formulario.hay_contenido(ALTO_NORMAL, ALTO_NORMAL + 10));
    }

    #[test]
    fn la_pista_de_tuneles_nunca_se_pega_a_su_etiqueta() {
        let tema = Tema::respaldo();
        for ancho in 12..=140u16 {
            for enfocado in [false, true] {
                let linea: String = cabecera_tuneles(ancho, enfocado, &tema)
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                assert!(linea.starts_with("  TÚNELES "), "{ancho}: «{linea}»");
                assert!(
                    !linea["  TÚNELES ".len()..].trim().is_empty(),
                    "{ancho}: sin pista «{linea}»"
                );
                // La última columna queda libre, como en el resto del bloque.
                assert_eq!(linea.chars().count(), usize::from(ancho) - 1, "{ancho}");
            }
        }
    }

    #[test]
    fn titulos_de_las_areas_de_texto_segun_el_ancho() {
        let largo = "SERVICIOS (systemd, una por línea)";
        assert!(cabe_titulo(largo, 38));
        assert!(!cabe_titulo(largo, 37));
        assert!(cabe_titulo("SERVICIOS", 13));
    }

    #[test]
    fn se_reconocen_las_tres_directivas_con_espacios_y_comentarios() {
        assert!(es_reenvio("LocalForward 127.0.0.1:5432 10.0.0.5:5432"));
        assert!(es_reenvio("   remoteforward 9000 127.0.0.1:9000"));
        assert!(es_reenvio("DYNAMICFORWARD [::1]:1080"));
        assert!(es_reenvio("\tdynamicforward 1080"));
        assert!(!es_reenvio(""));
        assert!(!es_reenvio("   "));
        assert!(!es_reenvio("# LocalForward 5432 10.0.0.5:5432"));
        assert!(!es_reenvio("ForwardAgent yes"));
        assert!(!es_reenvio("LocalForwardX 5432"));
    }

    #[test]
    fn la_ventana_sigue_a_la_seleccion_y_no_se_sale_de_la_lista() {
        // Con sitio para todo, no hay desplazamiento.
        assert_eq!(inicio_ventana(0, 3, 4), 0);
        // Selección al final: la última fila visible es la elegida.
        assert_eq!(inicio_ventana(9, 10, 3), 7);
        // Índice fuera de la lista (tras borrar) y lista vacía.
        assert_eq!(inicio_ventana(5, 3, 2), 1);
        assert_eq!(inicio_ventana(0, 0, 0), 0);
    }
}
