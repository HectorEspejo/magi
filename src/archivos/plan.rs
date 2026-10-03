//! Plan de sincronización (Fase 8, §7.1), sin red ni interfaz: cruza el árbol
//! del origen con el del destino (ya sin excluidos) y decide crear, actualizar
//! (tamaño o mtime ±2 s), borrar (solo con la casilla) u omitir con motivo.
//! Al crear, permisos del origen `& 0o777`; al actualizar, los del destino.
//! Las rutas a borrar quedan fijadas aquí; el servidor no recalcula nada (T33).
//!
//! Los choques fichero/directorio, en cualquier sentido, y los enlaces a
//! directorio se omiten con su motivo, y nada de lo que cuelga de una ruta
//! omitida se crea ni se borra: nunca se borra un directorio (ni su contenido)
//! para crear un fichero, ni se escribe a través de un enlace.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use crate::archivos::marcas::{TipoEntrada, TOLERANCIA_MTIME};
use crate::protocolo::EntradaArbol;

/// Un árbol recorrido y ya filtrado: ruta relativa a la raíz (con `/`, sin
/// `/` inicial) → entrada. Ordenado: un directorio va siempre antes que lo que
/// contiene.
pub type Arbol = BTreeMap<String, EntradaArbol>;

/// Por qué una ruta del origen se queda fuera del plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoOmision {
    /// En el origen es un enlace a directorio: no se sigue.
    EnlaceADirectorio,
    /// En el origen es un fichero y en el destino un directorio.
    DestinoEsDirectorio,
    /// En el origen es un directorio y en el destino un fichero.
    DestinoEsFichero,
    /// En el destino es un enlace a directorio: copiar escribiría en otro árbol.
    DestinoEsEnlace,
}

impl MotivoOmision {
    pub fn texto(self) -> &'static str {
        match self {
            MotivoOmision::EnlaceADirectorio => "enlace a directorio",
            MotivoOmision::DestinoEsDirectorio => "en destino es un directorio",
            MotivoOmision::DestinoEsFichero => "en destino es un fichero",
            MotivoOmision::DestinoEsEnlace => "en destino es un enlace a directorio",
        }
    }
}

/// Una decisión del plan sobre una ruta relativa.
#[derive(Debug, Clone, PartialEq)]
pub enum Cambio {
    /// Directorio que falta en el destino.
    CrearDir {
        ruta: String,
        permisos: Option<u32>,
    },
    /// Fichero que falta en el destino (un enlace a fichero del origen se
    /// copia como el fichero al que apunta).
    Crear {
        ruta: String,
        tamano: u64,
        permisos: Option<u32>,
    },
    /// Fichero que difiere en tamaño o en fecha; los permisos son los que ya
    /// tenía el destino.
    Actualizar {
        ruta: String,
        tamano: u64,
        mtime: i64,
        permisos: Option<u32>,
        tamano_destino: u64,
        mtime_destino: i64,
    },
    /// Sobra en el destino (solo con «borrar»).
    Borrar {
        ruta: String,
        es_dir: bool,
        tamano: u64,
    },
    Omitir {
        ruta: String,
        motivo: MotivoOmision,
    },
}

/// Los cuatro tipos de cambio, con su signo (`+ ~ − ·`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoCambio {
    Crear,
    Actualizar,
    Borrar,
    Omitir,
}

impl TipoCambio {
    /// Signo de la vista previa; en ASCII `+ ~ - .` (el `·` degradado sería
    /// un `-` y se confundiría con borrar).
    pub fn signo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (TipoCambio::Crear, _) => "+",
            (TipoCambio::Actualizar, _) => "~",
            (TipoCambio::Borrar, false) => "−",
            (TipoCambio::Borrar, true) => "-",
            (TipoCambio::Omitir, false) => "·",
            (TipoCambio::Omitir, true) => ".",
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            TipoCambio::Crear => "crear",
            TipoCambio::Actualizar => "actualizar",
            TipoCambio::Borrar => "borrar",
            TipoCambio::Omitir => "omitidos",
        }
    }
}

impl Cambio {
    pub fn ruta(&self) -> &str {
        match self {
            Cambio::CrearDir { ruta, .. }
            | Cambio::Crear { ruta, .. }
            | Cambio::Actualizar { ruta, .. }
            | Cambio::Borrar { ruta, .. }
            | Cambio::Omitir { ruta, .. } => ruta,
        }
    }

    pub fn tipo(&self) -> TipoCambio {
        match self {
            Cambio::CrearDir { .. } | Cambio::Crear { .. } => TipoCambio::Crear,
            Cambio::Actualizar { .. } => TipoCambio::Actualizar,
            Cambio::Borrar { .. } => TipoCambio::Borrar,
            Cambio::Omitir { .. } => TipoCambio::Omitir,
        }
    }

    /// Es un directorio (para pintarlo con `/` final).
    pub fn es_dir(&self) -> bool {
        matches!(
            self,
            Cambio::CrearDir { .. } | Cambio::Borrar { es_dir: true, .. }
        )
    }

    /// Bytes que se copian (los de crear y actualizar un fichero).
    pub fn bytes(&self) -> u64 {
        match self {
            Cambio::Crear { tamano, .. } | Cambio::Actualizar { tamano, .. } => *tamano,
            _ => 0,
        }
    }
}

/// Recuentos de la vista previa. Los excluidos no van aquí: los cuenta quien
/// recorre el árbol del origen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumen {
    /// Ficheros y directorios que se crean.
    pub crear: usize,
    pub actualizar: usize,
    pub borrar: usize,
    pub omitidos: usize,
    /// Ficheros que se copian (crear y actualizar, sin directorios).
    pub ficheros: usize,
    pub bytes: u64,
}

/// El plan: primero lo del origen en orden de ruta (un directorio antes de su
/// contenido) y al final lo que se borra (ficheros primero y directorios de
/// más profundo a menos).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub cambios: Vec<Cambio>,
}

impl Plan {
    pub fn resumen(&self) -> Resumen {
        let mut resumen = Resumen::default();
        for cambio in &self.cambios {
            match cambio {
                Cambio::CrearDir { .. } => resumen.crear += 1,
                Cambio::Crear { tamano, .. } => {
                    resumen.crear += 1;
                    resumen.ficheros += 1;
                    resumen.bytes += tamano;
                }
                Cambio::Actualizar { tamano, .. } => {
                    resumen.actualizar += 1;
                    resumen.ficheros += 1;
                    resumen.bytes += tamano;
                }
                Cambio::Borrar { .. } => resumen.borrar += 1,
                Cambio::Omitir { .. } => resumen.omitidos += 1,
            }
        }
        resumen
    }

    /// Nada que crear, actualizar ni borrar: «todo al día» (puede haber
    /// omitidos).
    pub fn al_dia(&self) -> bool {
        self.cambios
            .iter()
            .all(|cambio| cambio.tipo() == TipoCambio::Omitir)
    }

    /// Las rutas relativas que se borran, en el orden fijado.
    pub fn borrados(&self) -> impl Iterator<Item = &str> {
        self.cambios.iter().filter_map(|cambio| match cambio {
            Cambio::Borrar { ruta, .. } => Some(ruta.as_str()),
            _ => None,
        })
    }
}

/// Los directorios que contienen `ruta`, de fuera a dentro (`a`, `a/b` para
/// `a/b/c`).
fn ancestros(ruta: &str) -> impl Iterator<Item = &str> {
    ruta.match_indices('/')
        .map(move |(posicion, _)| &ruta[..posicion])
}

fn profundidad(ruta: &str) -> usize {
    ruta.matches('/').count()
}

/// ¿Cuelga `ruta` de alguna de las bloqueadas?
fn bajo_bloqueada(ruta: &str, bloqueadas: &BTreeSet<String>) -> bool {
    !bloqueadas.is_empty() && ancestros(ruta).any(|padre| bloqueadas.contains(padre))
}

/// Un directorio de verdad (un enlace a directorio no lo es: se borra o se
/// sustituye como un fichero, sin entrar en él).
fn es_directorio(entrada: &EntradaArbol) -> bool {
    entrada.tipo == TipoEntrada::Directorio && !entrada.enlace_a_dir
}

/// Calcula el plan (§7.1). `origen` y `destino` ya vienen sin lo excluido: lo
/// que no está en ellos ni se crea, ni se actualiza, ni se borra.
pub fn planificar(origen: &Arbol, destino: &Arbol, borrar: bool) -> Plan {
    let mut cambios = Vec::new();
    // Rutas omitidas por choque o enlace: lo que cuelga de ellas no se toca
    // en ningún sentido.
    let mut bloqueadas: BTreeSet<String> = BTreeSet::new();
    for (ruta, entrada) in origen {
        if bajo_bloqueada(ruta, &bloqueadas) {
            continue;
        }
        let mut omitir = |motivo: MotivoOmision, cambios: &mut Vec<Cambio>| {
            bloqueadas.insert(ruta.clone());
            cambios.push(Cambio::Omitir {
                ruta: ruta.clone(),
                motivo,
            });
        };
        if entrada.enlace_a_dir {
            omitir(MotivoOmision::EnlaceADirectorio, &mut cambios);
            continue;
        }
        let otro = destino.get(ruta);
        if entrada.tipo == TipoEntrada::Directorio {
            match otro {
                None => cambios.push(Cambio::CrearDir {
                    ruta: ruta.clone(),
                    permisos: entrada.permisos.map(|permisos| permisos & 0o777),
                }),
                Some(otro) if otro.enlace_a_dir => {
                    omitir(MotivoOmision::DestinoEsEnlace, &mut cambios)
                }
                Some(otro) if otro.tipo == TipoEntrada::Directorio => {}
                Some(_) => omitir(MotivoOmision::DestinoEsFichero, &mut cambios),
            }
            continue;
        }
        // Fichero, o enlace a fichero (viaja con el tamaño y la fecha de su
        // destino): se copia su contenido.
        match otro {
            None => cambios.push(Cambio::Crear {
                ruta: ruta.clone(),
                tamano: entrada.tamano,
                permisos: entrada.permisos.map(|permisos| permisos & 0o777),
            }),
            Some(otro) if otro.enlace_a_dir => omitir(MotivoOmision::DestinoEsEnlace, &mut cambios),
            Some(otro) if otro.tipo == TipoEntrada::Directorio => {
                omitir(MotivoOmision::DestinoEsDirectorio, &mut cambios)
            }
            Some(otro) => {
                let distinto = entrada.tamano != otro.tamano
                    || (entrada.mtime - otro.mtime).abs() > TOLERANCIA_MTIME;
                if distinto {
                    cambios.push(Cambio::Actualizar {
                        ruta: ruta.clone(),
                        tamano: entrada.tamano,
                        mtime: entrada.mtime,
                        // Los del destino, con los bits especiales: se
                        // conservan como estaban.
                        permisos: otro.permisos.map(|permisos| permisos & 0o7777),
                        tamano_destino: otro.tamano,
                        mtime_destino: otro.mtime,
                    });
                }
            }
        }
    }
    if borrar {
        let mut sobran: Vec<(&String, &EntradaArbol)> = destino
            .iter()
            .filter(|(ruta, _)| !origen.contains_key(*ruta) && !bajo_bloqueada(ruta, &bloqueadas))
            .collect();
        // Ficheros (y enlaces) primero; después los directorios, de más
        // profundo a menos, para que estén vacíos al llegarles el turno.
        sobran.sort_by_key(|(ruta, entrada)| {
            (es_directorio(entrada), Reverse(profundidad(ruta)), *ruta)
        });
        cambios.extend(sobran.into_iter().map(|(ruta, entrada)| Cambio::Borrar {
            ruta: ruta.clone(),
            es_dir: es_directorio(entrada),
            tamano: if es_directorio(entrada) {
                0
            } else {
                entrada.tamano
            },
        }));
    }
    Plan { cambios }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::archivos::exclusiones::Exclusiones;

    const FECHA: i64 = 1_757_670_067;

    fn fichero(ruta: &str, tamano: u64, mtime: i64) -> EntradaArbol {
        EntradaArbol {
            ruta: ruta.to_string(),
            tipo: TipoEntrada::Fichero,
            tamano,
            mtime,
            permisos: Some(0o100644),
            propietario: None,
            enlace_a_dir: false,
        }
    }

    fn directorio(ruta: &str) -> EntradaArbol {
        EntradaArbol {
            ruta: ruta.to_string(),
            tipo: TipoEntrada::Directorio,
            tamano: 4096,
            mtime: FECHA,
            permisos: Some(0o040755),
            propietario: None,
            enlace_a_dir: false,
        }
    }

    fn enlace_a_dir(ruta: &str) -> EntradaArbol {
        EntradaArbol {
            ruta: ruta.to_string(),
            tipo: TipoEntrada::Enlace,
            tamano: 0,
            mtime: 0,
            permisos: None,
            propietario: None,
            enlace_a_dir: true,
        }
    }

    fn arbol(entradas: Vec<EntradaArbol>) -> Arbol {
        entradas
            .into_iter()
            .map(|entrada| (entrada.ruta.clone(), entrada))
            .collect()
    }

    fn rutas_de(plan: &Plan, tipo: TipoCambio) -> Vec<&str> {
        plan.cambios
            .iter()
            .filter(|cambio| cambio.tipo() == tipo)
            .map(Cambio::ruta)
            .collect()
    }

    #[test]
    fn crea_lo_que_falta_con_los_permisos_del_origen() {
        let mut ejecutable = fichero("bin/arrancar.sh", 10, FECHA);
        ejecutable.permisos = Some(0o104755);
        let origen = arbol(vec![
            directorio("bin"),
            ejecutable,
            fichero("index.html", 120, FECHA),
        ]);
        let plan = planificar(&origen, &Arbol::new(), false);
        assert_eq!(
            plan.cambios,
            vec![
                Cambio::CrearDir {
                    ruta: "bin".to_string(),
                    permisos: Some(0o755),
                },
                Cambio::Crear {
                    ruta: "bin/arrancar.sh".to_string(),
                    tamano: 10,
                    // `& 0o777`: ni el tipo ni el setuid del origen.
                    permisos: Some(0o755),
                },
                Cambio::Crear {
                    ruta: "index.html".to_string(),
                    tamano: 120,
                    permisos: Some(0o644),
                },
            ]
        );
        let resumen = plan.resumen();
        assert_eq!(
            (resumen.crear, resumen.ficheros, resumen.bytes),
            (3, 2, 130)
        );
    }

    #[test]
    fn actualiza_por_tamano_o_por_fecha_fuera_de_la_tolerancia() {
        let origen = arbol(vec![
            fichero("igual.txt", 10, FECHA),
            fichero("tamano.txt", 11, FECHA),
            fichero("fecha.txt", 10, FECHA + TOLERANCIA_MTIME + 1),
            fichero("en_tolerancia.txt", 10, FECHA - TOLERANCIA_MTIME),
        ]);
        let mut destino_tamano = fichero("tamano.txt", 10, FECHA);
        destino_tamano.permisos = Some(0o102640);
        let destino = arbol(vec![
            fichero("igual.txt", 10, FECHA),
            destino_tamano,
            fichero("fecha.txt", 10, FECHA),
            fichero("en_tolerancia.txt", 10, FECHA),
        ]);
        let plan = planificar(&origen, &destino, false);
        assert_eq!(
            rutas_de(&plan, TipoCambio::Actualizar),
            vec!["fecha.txt", "tamano.txt"]
        );
        let Some(Cambio::Actualizar {
            permisos,
            tamano,
            tamano_destino,
            ..
        }) = plan
            .cambios
            .iter()
            .find(|cambio| cambio.ruta() == "tamano.txt")
        else {
            panic!("{plan:?}");
        };
        // Los del destino, con sus bits especiales y sin el tipo.
        assert_eq!(*permisos, Some(0o2640));
        assert_eq!((*tamano, *tamano_destino), (11, 10));
    }

    #[test]
    fn sin_la_casilla_no_se_borra_nada() {
        let destino = arbol(vec![directorio("viejo"), fichero("viejo/a.css", 3, FECHA)]);
        let plan = planificar(&Arbol::new(), &destino, false);
        assert!(plan.cambios.is_empty());
        assert!(plan.al_dia());
    }

    #[test]
    fn borra_ficheros_primero_y_directorios_de_mas_profundo_a_menos() {
        let origen = arbol(vec![fichero("main.py", 1, FECHA)]);
        let destino = arbol(vec![
            fichero("main.py", 1, FECHA),
            directorio("a"),
            directorio("a/b"),
            fichero("a/b/c.txt", 5, FECHA),
            fichero("a/d.txt", 6, FECHA),
            fichero("z.txt", 7, FECHA),
            enlace_a_dir("compartido"),
        ]);
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            rutas_de(&plan, TipoCambio::Borrar),
            vec!["a/b/c.txt", "a/d.txt", "compartido", "z.txt", "a/b", "a"]
        );
        // Un enlace a directorio se borra como un fichero (sin entrar).
        assert!(plan.cambios.iter().any(|cambio| matches!(
            cambio,
            Cambio::Borrar { ruta, es_dir: false, .. } if ruta == "compartido"
        )));
        assert_eq!(plan.resumen().borrar, 6);
        assert_eq!(
            plan.borrados().collect::<Vec<_>>(),
            rutas_de(&plan, TipoCambio::Borrar)
        );
    }

    #[test]
    fn un_enlace_a_directorio_del_origen_se_omite_y_protege_lo_de_debajo() {
        let origen = arbol(vec![
            enlace_a_dir("static/compartido"),
            directorio("static"),
        ]);
        let destino = arbol(vec![
            directorio("static"),
            directorio("static/compartido"),
            fichero("static/compartido/logo.svg", 9, FECHA),
        ]);
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            plan.cambios,
            vec![Cambio::Omitir {
                ruta: "static/compartido".to_string(),
                motivo: MotivoOmision::EnlaceADirectorio,
            }]
        );
        assert_eq!(plan.resumen().omitidos, 1);
        assert!(plan.al_dia(), "solo omitidos: no hay nada que hacer");
    }

    #[test]
    fn un_enlace_a_fichero_del_origen_se_copia_como_fichero() {
        let mut enlace = fichero("actual.conf", 33, FECHA);
        enlace.tipo = TipoEntrada::Enlace;
        let plan = planificar(&arbol(vec![enlace]), &Arbol::new(), false);
        assert_eq!(
            plan.cambios,
            vec![Cambio::Crear {
                ruta: "actual.conf".to_string(),
                tamano: 33,
                permisos: Some(0o644),
            }]
        );
    }

    #[test]
    fn un_fichero_que_en_destino_es_directorio_se_omite_sin_borrar_nada() {
        let origen = arbol(vec![fichero("datos", 10, FECHA)]);
        let destino = arbol(vec![directorio("datos"), fichero("datos/x.csv", 4, FECHA)]);
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            plan.cambios,
            vec![Cambio::Omitir {
                ruta: "datos".to_string(),
                motivo: MotivoOmision::DestinoEsDirectorio,
            }],
            "nunca se vacía un directorio para poner un fichero"
        );
    }

    #[test]
    fn un_directorio_que_en_destino_es_fichero_se_omite_con_su_contenido() {
        let origen = arbol(vec![
            directorio("logs"),
            fichero("logs/hoy.log", 10, FECHA),
            fichero("otro.txt", 1, FECHA),
        ]);
        let destino = arbol(vec![fichero("logs", 99, FECHA)]);
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            plan.cambios,
            vec![
                Cambio::Omitir {
                    ruta: "logs".to_string(),
                    motivo: MotivoOmision::DestinoEsFichero,
                },
                Cambio::Crear {
                    ruta: "otro.txt".to_string(),
                    tamano: 1,
                    permisos: Some(0o644),
                },
            ]
        );
    }

    #[test]
    fn no_se_escribe_a_traves_de_un_enlace_a_directorio_del_destino() {
        let origen = arbol(vec![
            directorio("static"),
            fichero("static/app.js", 88, FECHA),
            fichero("ultimo", 1, FECHA),
        ]);
        let destino = arbol(vec![enlace_a_dir("static"), enlace_a_dir("ultimo")]);
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            rutas_de(&plan, TipoCambio::Omitir),
            vec!["static", "ultimo"]
        );
        assert!(plan.cambios.iter().all(|cambio| matches!(
            cambio,
            Cambio::Omitir {
                motivo: MotivoOmision::DestinoEsEnlace,
                ..
            }
        )));
    }

    #[test]
    fn un_plan_vacio_esta_al_dia() {
        let arbol = arbol(vec![directorio("a"), fichero("a/b.txt", 3, FECHA)]);
        let plan = planificar(&arbol, &arbol, true);
        assert!(plan.cambios.is_empty());
        assert!(plan.al_dia());
        assert_eq!(plan.resumen(), Resumen::default());
    }

    /// Filtra un árbol completo como lo haría quien lo recorre.
    fn filtrar(arbol: Vec<EntradaArbol>, exclusiones: &Exclusiones) -> Arbol {
        arbol
            .into_iter()
            .filter(|entrada| !exclusiones.excluida(&entrada.ruta, es_directorio(entrada)))
            .map(|entrada| (entrada.ruta.clone(), entrada))
            .collect()
    }

    /// AC del checklist: dado `.env` en el destino y `.env` en las
    /// exclusiones, cuando se sincroniza con «borrar», `.env` sigue en el
    /// destino (y tampoco se toca nada de `node_modules/`).
    #[test]
    fn un_env_excluido_no_se_borra_con_borrar_marcado() {
        let por_defecto: Vec<String> = crate::config::EXCLUIR_POR_DEFECTO
            .iter()
            .map(|patron| patron.to_string())
            .collect();
        let exclusiones =
            Exclusiones::nueva(&por_defecto, Some("*.log\n"), &[".env".to_string()]).unwrap();
        let origen = filtrar(
            vec![
                fichero("main.py", 10, FECHA),
                fichero(".env", 50, FECHA),
                fichero("error.log", 5, FECHA),
            ],
            &exclusiones,
        );
        let destino = filtrar(
            vec![
                fichero("main.py", 10, FECHA),
                fichero(".env", 70, FECHA),
                fichero("viejo.py", 1, FECHA),
                fichero("acceso.log", 9, FECHA),
                directorio("node_modules"),
                fichero("node_modules/react.js", 9, FECHA),
                fichero(".magiignore", 6, FECHA),
            ],
            &exclusiones,
        );
        let plan = planificar(&origen, &destino, true);
        assert_eq!(
            plan.cambios,
            vec![Cambio::Borrar {
                ruta: "viejo.py".to_string(),
                es_dir: false,
                tamano: 1,
            }],
            "solo sobra lo que no está excluido"
        );
    }
}
