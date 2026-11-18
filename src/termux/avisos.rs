//! Avisos de Android (§4.7, §7.6): lo que el servidor publica con
//! `termux-notification` y retira con `termux-notification-remove` cuando una
//! sesión persistente o una pestaña pide atención y nadie la mira.
//!
//! Aquí vive la parte pura: los ids, los textos, la acción fija, cuándo se
//! «mira» una pestaña, el conjunto de avisos que toca tener (`deseados`) y qué
//! hay que publicar o retirar para llegar a él (`planificar`). La tarea
//! reconciliadora que lo ejecuta, con el estado del servidor y las órdenes de
//! Termux, está en `servidor::android`.
//!
//! Un aviso lleva solo el nombre de la sesión o de la pestaña y el del host
//! (T140): nunca el título del remoto, el texto del aviso ni la salida. Los
//! nombres llegan de fuera (un host remoto, una carpeta sincronizada), así que
//! se limpian de caracteres de control y se acotan antes de armar un texto.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::estado::id_de_aviso_valido;

/// La acción que lleva el aviso: trae Termux delante. Termux:API la ejecuta
/// con un shell, así que es una constante de MAGI, sin datos del inventario ni
/// del remoto (§5.2, §7.6).
pub const ACCION: &str = "am start -n com.termux/com.termux.app.TermuxActivity";

/// Grupo de todos los avisos de MAGI.
pub const GRUPO: &str = "magi";

/// Id del aviso de prueba de `magi termux aviso`.
pub const ID_PRUEBA: &str = "magi-test";

/// Caracteres que caben como mucho en el título y en el texto de un aviso
/// (§7.10).
pub const LARGO_MAXIMO: usize = 80;

/// Caracteres del nombre de una pestaña en su título (§7.6).
pub const LARGO_NOMBRE_PESTANA: usize = 40;

/// Avisos que el servidor tiene publicados a la vez como mucho: una lista de
/// fichas o de pestañas desmesurada no puede inundar la barra del móvil. Los
/// que sobran se quedan sin aviso (la barra de MAGI los sigue enseñando).
pub const MAXIMO_AVISOS: usize = 64;

/// ¿Manda Termux los eventos de foco al cambiar de aplicación, de modo que
/// «enfocada» significa «la estoy mirando»? D379.
///
/// PROVISIONAL-POC: lo decide la prueba de concepto en el plegable (guion del
/// informe de implementación). Mientras no pase, vale `false`, el valor
/// prudente de la especificación: una pestaña avisa aunque esté delante y el
/// aviso se retira en cuanto se escribe en ella. Con `true`, una pestaña
/// enfocada cuenta como mirada y ni siquiera se marca.
pub const FOCO_FIABLE: bool = false;

/// ¿Cuenta una pestaña como mirada? Con el foco poco fiable, nunca.
#[must_use]
pub fn mirando(enfocada: bool) -> bool {
    mirando_con(FOCO_FIABLE, enfocada)
}

/// [`mirando`] con el valor de `FOCO_FIABLE` dado (para probar los dos).
#[must_use]
pub fn mirando_con(foco_fiable: bool, enfocada: bool) -> bool {
    foco_fiable && enfocada
}

/// De dónde sale un aviso: una sesión persistente (con su ficha) o una pestaña.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origen {
    /// `nombre` es el de la sesión de tmux (validado al crear la ficha).
    Persistente { nombre: String },
    /// `nombre` es el de la pestaña.
    Pestana { nombre: String },
}

/// Quita caracteres de control, espacios raros y marcas de dirección del
/// texto y colapsa los espacios: lo que llega de fuera no puede meter saltos
/// de línea ni cambiar el orden de lo que se lee en un aviso.
#[must_use]
pub fn sin_controles(texto: &str) -> String {
    let mut limpio = String::with_capacity(texto.len());
    let mut separar = false;
    for caracter in texto.chars() {
        if caracter.is_control()
            || caracter.is_whitespace()
            || matches!(caracter, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        {
            separar = !limpio.is_empty();
            continue;
        }
        if separar {
            limpio.push(' ');
            separar = false;
        }
        limpio.push(caracter);
    }
    limpio
}

/// Los `maximo` primeros caracteres.
fn recortar(texto: &str, maximo: usize) -> String {
    texto.chars().take(maximo).collect()
}

/// El título del aviso (§7.6): «◆ <sesión> pide atención» o «! <pestaña> pide
/// atención», de 80 caracteres como mucho. Lo que se acorta es el nombre,
/// nunca el final.
#[must_use]
pub fn titulo(origen: &Origen) -> String {
    const SUFIJO: &str = " pide atención";
    let (marca, nombre, tope) = match origen {
        Origen::Persistente { nombre } => ("◆ ", nombre, LARGO_MAXIMO),
        Origen::Pestana { nombre } => ("! ", nombre, LARGO_NOMBRE_PESTANA),
    };
    let sitio = LARGO_MAXIMO
        .saturating_sub(marca.chars().count() + SUFIJO.chars().count())
        .min(tope);
    let nombre = recortar(&sin_controles(nombre), sitio);
    let nombre = if nombre.is_empty() {
        "sesión".to_string()
    } else {
        nombre
    };
    recortar(&format!("{marca}{nombre}{SUFIJO}"), LARGO_MAXIMO)
}

/// El texto del aviso (§7.6): «<host> · MAGI», de 80 caracteres como mucho.
#[must_use]
pub fn texto(host: &str) -> String {
    const COLA: &str = " · MAGI";
    let sitio = LARGO_MAXIMO.saturating_sub(COLA.chars().count());
    let host = recortar(&sin_controles(host), sitio);
    let host = if host.is_empty() {
        "host".to_string()
    } else {
        host
    };
    format!("{host}{COLA}")
}

/// El id del aviso de una ficha: `magi-<uuid[:8]>`. `None` si el uuid no da un
/// id válido (el aviso se salta: nunca se pasa a una orden un id sin
/// comprobar).
#[must_use]
pub fn id_de_ficha(uuid: &str) -> Option<String> {
    let corto: String = uuid.chars().take(8).collect();
    let id = format!("magi-{corto}");
    (corto.len() == 8 && id_de_aviso_valido(&id)).then_some(id)
}

/// El id del aviso de una pestaña: `magi-p<id de la pestaña>`.
#[must_use]
pub fn id_de_pestana(pestana_id: u32) -> String {
    format!("magi-p{pestana_id}")
}

/// Una ficha de sesión persistente tal como la ve el vigilante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaFicha {
    pub uuid: String,
    pub host_id: i64,
    /// Nombre de la sesión de tmux.
    pub nombre: String,
    /// `Some(true)`: viva y con la campana; `Some(false)`: se sabe que no
    /// pide atención (viva sin campana, o terminada); `None`: no se sabe (el
    /// host no respondió o aún no se ha comprobado).
    pub pide: Option<bool>,
}

/// Una pestaña tal como la ve el servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaPestana {
    pub id: u32,
    pub nombre: String,
    pub host_id: i64,
    pub host: String,
    /// El nombre de la sesión de tmux propia de MAGI a la que está enganchada,
    /// si lo está (para no avisar dos veces de la misma sesión).
    pub persistente: Option<String>,
    /// La marca `atencion_remota`: el emulador vio un aviso del remoto sin
    /// que nadie la mirara y nadie ha escrito en ella desde entonces.
    pub pide: bool,
    /// ¿Alguna ventana la tiene enfocada?
    pub enfocada: bool,
}

/// Un aviso que toca tener publicado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deseado {
    pub id: String,
    pub titulo: String,
    pub host_id: i64,
    /// El nombre del host, si ya se conoce; si no, quien publica lo busca.
    pub host: Option<String>,
}

/// El conjunto de avisos que toca tener (§4.7): una sesión persistente que
/// pide atención (campana de tmux: nadie la mira) y una pestaña con la marca
/// `atencion_remota` que nadie mira. Una pestaña enganchada a una sesión
/// persistente que ya tiene su ficha pidiendo atención no suma otro aviso
/// (dedupe por host y nombre de sesión de tmux).
///
/// Un aviso ya publicado de una ficha de la que no se sabe nada (el host no
/// responde) se conserva: no se retira una atención por no poder mirarla, y
/// volver a publicarla al volver el host la haría sonar otra vez.
#[must_use]
pub fn deseados(
    fichas: &[EntradaFicha],
    pestanas: &[EntradaPestana],
    publicados: &BTreeSet<String>,
    foco_fiable: bool,
) -> BTreeMap<String, Deseado> {
    let mut deseados: BTreeMap<String, Deseado> = BTreeMap::new();
    let mut sesiones_con_aviso: HashSet<(i64, &str)> = HashSet::new();

    for ficha in fichas {
        let Some(id) = id_de_ficha(&ficha.uuid) else {
            continue;
        };
        let pide = match ficha.pide {
            Some(pide) => pide,
            None => publicados.contains(&id),
        };
        if !pide {
            continue;
        }
        sesiones_con_aviso.insert((ficha.host_id, ficha.nombre.as_str()));
        deseados.entry(id.clone()).or_insert_with(|| Deseado {
            titulo: titulo(&Origen::Persistente {
                nombre: ficha.nombre.clone(),
            }),
            id,
            host_id: ficha.host_id,
            host: None,
        });
    }

    for pestana in pestanas {
        if !pestana.pide || mirando_con(foco_fiable, pestana.enfocada) {
            continue;
        }
        let repetida = pestana
            .persistente
            .as_deref()
            .is_some_and(|nombre| sesiones_con_aviso.contains(&(pestana.host_id, nombre)));
        if repetida {
            continue;
        }
        let id = id_de_pestana(pestana.id);
        deseados.entry(id.clone()).or_insert_with(|| Deseado {
            titulo: titulo(&Origen::Pestana {
                nombre: pestana.nombre.clone(),
            }),
            id,
            host_id: pestana.host_id,
            host: Some(pestana.host.clone()),
        });
    }

    // Tope: se quedan los de id menor (el orden es estable de un ciclo al
    // siguiente, así que no se publican y retiran avisos en cada vuelta).
    while deseados.len() > MAXIMO_AVISOS {
        deseados.pop_last();
    }
    deseados
}

/// Lo que hay que hacer para pasar de lo publicado a lo deseado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Ids publicados que ya no tocan, en orden.
    pub retirar: Vec<String>,
    /// Avisos que faltan, en orden de id.
    pub publicar: Vec<Deseado>,
}

/// Compara lo deseado con lo publicado: retira lo que sobra y publica lo que
/// falta. Lo ya publicado que sigue deseado no se toca (el mismo id no se
/// vuelve a publicar: no suena otra vez).
#[must_use]
pub fn planificar(deseados: &BTreeMap<String, Deseado>, publicados: &BTreeSet<String>) -> Plan {
    Plan {
        retirar: publicados
            .iter()
            .filter(|id| !deseados.contains_key(*id))
            .cloned()
            .collect(),
        publicar: deseados
            .iter()
            .filter(|(id, _)| !publicados.contains(*id))
            .map(|(_, deseado)| deseado.clone())
            .collect(),
    }
}

/// Los argumentos de `termux-notification` para un aviso (§5.2). Sueltos,
/// nunca por un shell: la única cadena que pasa por uno en Termux:API es la
/// acción, que es una constante.
#[must_use]
pub fn argumentos_publicar(id: &str, titulo: &str, host: &str) -> Vec<String> {
    argumentos_con_texto(id, titulo, &texto(host))
}

/// Como [`argumentos_publicar`], con el texto ya armado (el aviso de prueba de
/// `magi termux aviso` no lleva host). Título y texto se limpian y se acotan a
/// 80 caracteres.
#[must_use]
pub fn argumentos_con_texto(id: &str, titulo: &str, cuerpo: &str) -> Vec<String> {
    vec![
        "termux-notification".to_string(),
        "--id".to_string(),
        id.to_string(),
        "--group".to_string(),
        GRUPO.to_string(),
        "--alert-once".to_string(),
        "-t".to_string(),
        recortar(&sin_controles(titulo), LARGO_MAXIMO),
        "-c".to_string(),
        recortar(&sin_controles(cuerpo), LARGO_MAXIMO),
        "--action".to_string(),
        ACCION.to_string(),
    ]
}

/// Los argumentos de `termux-notification-remove`.
#[must_use]
pub fn argumentos_retirar(id: &str) -> Vec<String> {
    vec!["termux-notification-remove".to_string(), id.to_string()]
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ficha(uuid: &str, host_id: i64, nombre: &str, pide: Option<bool>) -> EntradaFicha {
        EntradaFicha {
            uuid: uuid.to_string(),
            host_id,
            nombre: nombre.to_string(),
            pide,
        }
    }

    fn pestana(id: u32, nombre: &str, host_id: i64) -> EntradaPestana {
        EntradaPestana {
            id,
            nombre: nombre.to_string(),
            host_id,
            host: "dev-01".to_string(),
            persistente: None,
            pide: true,
            enfocada: false,
        }
    }

    #[test]
    fn el_titulo_de_una_sesion_persistente_lleva_el_rombo() {
        let titulo = titulo(&Origen::Persistente {
            nombre: "agente-api".into(),
        });
        assert_eq!(titulo, "◆ agente-api pide atención");
        assert_eq!(texto("dev-01"), "dev-01 · MAGI");
    }

    #[test]
    fn el_titulo_de_una_pestana_lleva_la_admiracion_y_40_caracteres() {
        let corto = titulo(&Origen::Pestana {
            nombre: "web-01".into(),
        });
        assert_eq!(corto, "! web-01 pide atención");
        let largo = titulo(&Origen::Pestana {
            nombre: "x".repeat(100),
        });
        assert_eq!(largo, format!("! {} pide atención", "x".repeat(40)));
        assert!(largo.chars().count() <= LARGO_MAXIMO);
    }

    #[test]
    fn los_textos_no_pasan_de_80_caracteres_ni_pierden_su_final() {
        let titulo_largo = titulo(&Origen::Persistente {
            nombre: "ñ".repeat(500),
        });
        assert_eq!(titulo_largo.chars().count(), LARGO_MAXIMO);
        assert!(titulo_largo.ends_with(" pide atención"), "{titulo_largo}");
        let texto_largo = texto(&"h".repeat(500));
        assert_eq!(texto_largo.chars().count(), LARGO_MAXIMO);
        assert!(texto_largo.ends_with(" · MAGI"), "{texto_largo}");
    }

    #[test]
    fn los_nombres_de_fuera_se_limpian_de_controles() {
        let sucio = "web\x1b[31m-01\r\n\u{202e}x\u{7}\t y";
        let limpio = sin_controles(sucio);
        assert!(!limpio.chars().any(char::is_control), "{limpio:?}");
        assert!(!limpio.contains('\u{202e}'), "{limpio:?}");
        assert_eq!(limpio, "web [31m-01 x y");
        let titulo = titulo(&Origen::Pestana {
            nombre: sucio.into(),
        });
        assert!(!titulo.chars().any(char::is_control), "{titulo:?}");
        // Un nombre que se queda sin nada no deja un aviso sin nombre.
        assert_eq!(
            super::titulo(&Origen::Pestana {
                nombre: "\u{7}\r\n".into()
            }),
            "! sesión pide atención"
        );
        assert_eq!(texto("\n"), "host · MAGI");
    }

    #[test]
    fn los_ids_salen_del_uuid_corto_o_del_id_de_la_pestana() {
        assert_eq!(
            id_de_ficha("7c1e4a90-1111-4222-8333-444455556666").as_deref(),
            Some("magi-7c1e4a90")
        );
        assert_eq!(id_de_pestana(3), "magi-p3");
        assert!(id_de_aviso_valido(&id_de_pestana(4_000_000_000)));
        // Un uuid que no da un id válido no produce aviso.
        assert_eq!(id_de_ficha("corto"), None);
        assert_eq!(id_de_ficha("ABCDEF12-0000"), None, "mayúsculas");
        assert_eq!(id_de_ficha("../../etc"), None);
        assert_eq!(id_de_ficha("$(reboot)-1234"), None);
        assert_eq!(id_de_ficha("ñññññññññ"), None);
    }

    #[test]
    fn el_id_de_prueba_es_un_id_valido() {
        assert!(id_de_aviso_valido(ID_PRUEBA));
    }

    #[test]
    fn con_el_foco_poco_fiable_una_pestana_delante_tambien_cuenta() {
        // El valor de la constante lo decide la prueba de concepto
        // (PROVISIONAL-POC): la función solo lo respeta.
        assert_eq!(mirando(true), FOCO_FIABLE);
        assert!(!mirando(false));
        assert!(!mirando_con(false, true));
        assert!(mirando_con(true, true));
        assert!(!mirando_con(true, false));
    }

    #[test]
    fn una_pestana_enfocada_no_avisa_solo_si_el_foco_es_fiable() {
        let mut delante = pestana(1, "web", 1);
        delante.enfocada = true;
        let sin_fiar = deseados(&[], &[delante.clone()], &BTreeSet::new(), false);
        assert!(
            sin_fiar.contains_key("magi-p1"),
            "avisa aunque esté delante"
        );
        let fiable = deseados(&[], &[delante], &BTreeSet::new(), true);
        assert!(fiable.is_empty(), "con foco fiable, delante es mirada");
        let detras = pestana(2, "web", 1);
        assert!(deseados(&[], &[detras], &BTreeSet::new(), true).contains_key("magi-p2"));
    }

    #[test]
    fn solo_pide_la_pestana_con_la_marca() {
        let mut quieta = pestana(1, "web", 1);
        quieta.pide = false;
        assert!(deseados(&[], &[quieta], &BTreeSet::new(), false).is_empty());
    }

    #[test]
    fn una_ficha_que_pide_da_su_aviso_con_el_uuid_corto() {
        let fichas = [
            ficha(
                "7c1e4a90-0000-4000-8000-000000000000",
                1,
                "agente-api",
                Some(true),
            ),
            ficha(
                "aaaaaaaa-0000-4000-8000-000000000000",
                1,
                "otra",
                Some(false),
            ),
        ];
        let deseados = deseados(&fichas, &[], &BTreeSet::new(), false);
        assert_eq!(deseados.len(), 1);
        let aviso = &deseados["magi-7c1e4a90"];
        assert_eq!(aviso.titulo, "◆ agente-api pide atención");
        assert_eq!(aviso.host_id, 1);
        assert_eq!(aviso.host, None, "el host lo busca quien publica");
    }

    #[test]
    fn la_misma_sesion_en_pestana_y_en_ficha_da_un_solo_aviso() {
        let fichas = [ficha(
            "7c1e4a90-0000-4000-8000-000000000000",
            1,
            "agente-api",
            Some(true),
        )];
        let mut enganchada = pestana(3, "agente-api", 1);
        enganchada.persistente = Some("agente-api".to_string());
        let deseados = deseados(&fichas, &[enganchada.clone()], &BTreeSet::new(), false);
        assert_eq!(
            deseados.keys().collect::<Vec<_>>(),
            vec!["magi-7c1e4a90"],
            "gana el de la ficha"
        );
        // La misma sesión con otro nombre o en otro host no es la misma.
        let mut otra = enganchada.clone();
        otra.persistente = Some("otra".to_string());
        assert_eq!(
            super::deseados(&fichas, &[otra], &BTreeSet::new(), false).len(),
            2
        );
        let mut otro_host = enganchada;
        otro_host.host_id = 2;
        assert_eq!(
            super::deseados(&fichas, &[otro_host], &BTreeSet::new(), false).len(),
            2
        );
        // Si la ficha no pide, la pestaña avisa sola.
        let sin_campana = [ficha(
            "7c1e4a90-0000-4000-8000-000000000000",
            1,
            "agente-api",
            Some(false),
        )];
        let mut sola = pestana(3, "agente-api", 1);
        sola.persistente = Some("agente-api".to_string());
        assert_eq!(
            super::deseados(&sin_campana, &[sola], &BTreeSet::new(), false)
                .keys()
                .collect::<Vec<_>>(),
            vec!["magi-p3"]
        );
    }

    #[test]
    fn una_ficha_sin_comprobar_conserva_su_aviso_pero_no_crea_uno() {
        let desconocida = [ficha(
            "7c1e4a90-0000-4000-8000-000000000000",
            1,
            "agente-api",
            None,
        )];
        let publicados: BTreeSet<String> = ["magi-7c1e4a90".to_string()].into();
        let conservado = deseados(&desconocida, &[], &publicados, false);
        assert!(
            conservado.contains_key("magi-7c1e4a90"),
            "no se retira una atención por no poder mirarla"
        );
        assert!(deseados(&desconocida, &[], &BTreeSet::new(), false).is_empty());
        // Sabiendo que ya no pide, sí se retira.
        let sabida = [ficha(
            "7c1e4a90-0000-4000-8000-000000000000",
            1,
            "agente-api",
            Some(false),
        )];
        assert!(deseados(&sabida, &[], &publicados, false).is_empty());
    }

    #[test]
    fn el_plan_publica_lo_que_falta_y_retira_lo_que_sobra() {
        let fichas = [
            ficha("11111111-0000-4000-8000-000000000000", 1, "a", Some(true)),
            ficha("22222222-0000-4000-8000-000000000000", 1, "b", Some(true)),
        ];
        let deseados = deseados(&fichas, &[], &BTreeSet::new(), false);
        let publicados: BTreeSet<String> =
            ["magi-22222222".to_string(), "magi-p9".to_string()].into();
        let plan = planificar(&deseados, &publicados);
        assert_eq!(plan.retirar, vec!["magi-p9".to_string()]);
        assert_eq!(
            plan.publicar
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec!["magi-11111111"],
            "lo ya publicado no se vuelve a publicar"
        );
        // Sin cambios, nada que hacer.
        let igual: BTreeSet<String> = deseados.keys().cloned().collect();
        assert_eq!(planificar(&deseados, &igual), Plan::default());
        // Sin nada deseado, se retira todo.
        let todo = planificar(&BTreeMap::new(), &publicados);
        assert_eq!(todo.retirar.len(), 2);
        assert!(todo.publicar.is_empty());
    }

    #[test]
    fn nunca_hay_mas_de_64_avisos_a_la_vez() {
        let pestanas: Vec<EntradaPestana> = (1..=200).map(|id| pestana(id, "web", 1)).collect();
        let deseados = deseados(&[], &pestanas, &BTreeSet::new(), false);
        assert_eq!(deseados.len(), MAXIMO_AVISOS);
    }

    #[test]
    fn los_argumentos_son_los_de_la_especificacion_y_la_accion_es_fija() {
        let argumentos =
            argumentos_publicar("magi-7c1e4a90", "◆ agente-api pide atención", "dev-01");
        assert_eq!(
            argumentos,
            [
                "termux-notification",
                "--id",
                "magi-7c1e4a90",
                "--group",
                "magi",
                "--alert-once",
                "-t",
                "◆ agente-api pide atención",
                "-c",
                "dev-01 · MAGI",
                "--action",
                "am start -n com.termux/com.termux.app.TermuxActivity",
            ]
        );
        assert_eq!(
            argumentos_retirar("magi-p3"),
            ["termux-notification-remove", "magi-p3"]
        );
    }

    #[test]
    fn los_argumentos_nunca_pasan_de_80_caracteres_ni_llevan_controles() {
        let argumentos = argumentos_publicar("magi-p1", &"t".repeat(300), "h\u{7}\n");
        assert_eq!(argumentos[7].chars().count(), LARGO_MAXIMO);
        assert_eq!(argumentos[9], "h · MAGI");
    }
}
