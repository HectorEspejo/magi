//! Marcas de diferencia entre los dos paneles (§7.1 del informe): `✕` si el
//! elemento no existe al otro lado y `≠` si existe con otro tipo, otro tamaño
//! o una fecha que difiere en más de la tolerancia.
//!
//! Los directorios nunca se marcan `≠`: solo `✕` cuando faltan al otro lado.

use serde::{Deserialize, Serialize};

/// Tolerancia de `mtime`, en segundos: dos fechas que disten menos se
/// consideran la misma (los servidores redondean de formas distintas).
pub const TOLERANCIA_MTIME: i64 = 2;

/// Tipo de una entrada del panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoEntrada {
    Directorio,
    Fichero,
    Enlace,
}

impl TipoEntrada {
    pub fn es_dir(self) -> bool {
        matches!(self, TipoEntrada::Directorio)
    }

    pub fn texto(self) -> &'static str {
        match self {
            TipoEntrada::Directorio => "directorio",
            TipoEntrada::Fichero => "fichero",
            TipoEntrada::Enlace => "enlace",
        }
    }
}

/// Marca de un elemento comparado con el otro panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Marca {
    #[default]
    Ninguna,
    /// Existe al otro lado con otro tipo, tamaño o fecha (`≠`).
    Distinta,
    /// No existe al otro lado (`✕`).
    Ausente,
}

impl Marca {
    pub fn es_ninguna(&self) -> bool {
        matches!(self, Marca::Ninguna)
    }

    /// Glifo de la marca; en ASCII, `!=` y `x`.
    pub fn glifo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (Marca::Ninguna, _) => "",
            (Marca::Distinta, false) => "≠",
            (Marca::Distinta, true) => "!=",
            (Marca::Ausente, false) => "✕",
            (Marca::Ausente, true) => "x",
        }
    }

    /// Texto del detalle: `≠ tamaño`, `≠ fecha`, `no está al otro lado`.
    pub fn texto(self) -> &'static str {
        match self {
            Marca::Ninguna => "igual",
            Marca::Distinta => "≠",
            Marca::Ausente => "no está al otro lado",
        }
    }
}

/// Entrada de un panel: la misma para el lado local y el remoto. La `marca` la
/// rellena el cliente al comparar los dos listados, nunca el servidor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrada {
    pub nombre: String,
    pub tipo: TipoEntrada,
    pub tamano: u64,
    /// Época en segundos; cero si el otro extremo no la dio.
    pub mtime: i64,
    pub permisos: Option<u32>,
    pub propietario: Option<String>,
    /// Destino del enlace simbólico, si la entrada es un enlace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enlace: Option<String>,
    #[serde(default, skip_serializing_if = "Marca::es_ninguna")]
    pub marca: Marca,
}

impl Entrada {
    pub fn es_dir(&self) -> bool {
        self.tipo.es_dir()
    }

    /// Por qué difiere de la entrada del otro lado, para el detalle.
    pub fn motivo_diferencia(&self, otro: &Entrada) -> Option<&'static str> {
        if self.tipo != otro.tipo {
            return Some("≠ tipo");
        }
        if self.tipo.es_dir() {
            return None;
        }
        if self.tamano != otro.tamano {
            return Some("≠ tamaño");
        }
        if (self.mtime - otro.mtime).abs() > TOLERANCIA_MTIME {
            return Some("≠ fecha");
        }
        None
    }
}

/// Directorios primero y, dentro de cada grupo, por nombre sin distinguir
/// mayúsculas (con el nombre exacto como desempate, para que el orden sea
/// estable).
pub fn ordenar(entradas: &mut [Entrada]) {
    entradas.sort_by(|una, otra| {
        otra.es_dir()
            .cmp(&una.es_dir())
            .then_with(|| una.nombre.to_lowercase().cmp(&otra.nombre.to_lowercase()))
            .then_with(|| una.nombre.cmp(&otra.nombre))
    });
}

/// Calcula las marcas de los dos listados, que se pasan ya ordenados. Las
/// marcas de ambos lados quedan coherentes porque el cálculo se hace una sola
/// vez por nombre; el coste es lineal en el número de entradas.
pub fn calcular(local: &mut [Entrada], remoto: &mut [Entrada]) {
    let indice_remoto: std::collections::HashMap<&str, &Entrada> = remoto
        .iter()
        .map(|entrada| (entrada.nombre.as_str(), entrada))
        .collect();

    let mut marcas: std::collections::HashMap<String, Marca> =
        std::collections::HashMap::with_capacity(local.len());
    for entrada in local.iter() {
        let marca = match indice_remoto.get(entrada.nombre.as_str()) {
            None => Marca::Ausente,
            Some(otra) => {
                if entrada.es_dir() || otra.es_dir() {
                    // Un directorio solo se marca si falta al otro lado.
                    Marca::Ninguna
                } else if entrada.tamano != otra.tamano
                    || (entrada.mtime - otra.mtime).abs() > TOLERANCIA_MTIME
                {
                    Marca::Distinta
                } else {
                    Marca::Ninguna
                }
            }
        };
        marcas.insert(entrada.nombre.clone(), marca);
    }

    for entrada in local.iter_mut() {
        entrada.marca = marcas
            .get(&entrada.nombre)
            .copied()
            .unwrap_or(Marca::Ninguna);
    }
    for entrada in remoto.iter_mut() {
        // La marca del remoto es la misma que la del local, cuando existe.
        entrada.marca = marcas
            .get(&entrada.nombre)
            .copied()
            .unwrap_or(Marca::Ausente);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn fichero(nombre: &str, tamano: u64, mtime: i64) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            tipo: TipoEntrada::Fichero,
            tamano,
            mtime,
            permisos: Some(0o644),
            propietario: None,
            enlace: None,
            marca: Marca::Ninguna,
        }
    }

    fn directorio(nombre: &str) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            tipo: TipoEntrada::Directorio,
            tamano: 4096,
            mtime: 1_000,
            permisos: Some(0o755),
            propietario: None,
            enlace: None,
            marca: Marca::Ninguna,
        }
    }

    #[test]
    fn un_fichero_que_no_esta_al_otro_lado_se_marca_con_aspa() {
        let mut local = vec![fichero("main.py", 100, 1_000), fichero(".env", 10, 1_000)];
        let mut remoto = vec![fichero("main.py", 100, 1_000)];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[0].marca, Marca::Ninguna);
        assert_eq!(local[1].marca, Marca::Ausente);
        assert_eq!(remoto[0].marca, Marca::Ninguna);
    }

    /// El criterio de aceptación del checklist: `config.yaml` de 2 kB en local
    /// y 1 kB en remoto marca `≠` en las dos filas.
    #[test]
    fn dos_tamanos_distintos_se_marcan_como_diferentes_en_los_dos_lados() {
        let mut local = vec![fichero("config.yaml", 2048, 1_000)];
        let mut remoto = vec![fichero("config.yaml", 1024, 1_000)];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[0].marca, Marca::Distinta);
        assert_eq!(remoto[0].marca, Marca::Distinta);
        assert_eq!(local[0].motivo_diferencia(&remoto[0]), Some("≠ tamaño"));
    }

    #[test]
    fn una_diferencia_de_fecha_de_dos_segundos_no_marca() {
        let mut local = vec![fichero("a", 10, 1_002)];
        let mut remoto = vec![fichero("a", 10, 1_000)];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[0].marca, Marca::Ninguna);

        let mut local = vec![fichero("a", 10, 1_003)];
        let mut remoto = vec![fichero("a", 10, 1_000)];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[0].marca, Marca::Distinta);
        assert_eq!(local[0].motivo_diferencia(&remoto[0]), Some("≠ fecha"));
    }

    #[test]
    fn los_directorios_solo_se_marcan_si_faltan() {
        let mut local = vec![directorio("static"), directorio("solo_local")];
        let mut remoto = vec![directorio("static"), directorio("solo_remoto")];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[0].marca, Marca::Ninguna, "un directorio nunca es ≠");
        assert_eq!(local[1].marca, Marca::Ausente);
        assert_eq!(remoto[1].marca, Marca::Ausente);
    }

    #[test]
    fn la_marca_del_remoto_es_la_misma_que_la_del_local() {
        let mut local = vec![fichero("igual", 10, 1_000), fichero("otro", 20, 1_000)];
        let mut remoto = vec![fichero("igual", 10, 1_000), fichero("otro", 10, 1_000)];
        calcular(&mut local, &mut remoto);
        assert_eq!(local[1].marca, remoto[1].marca);
    }

    #[test]
    fn las_marcas_se_pintan_en_ascii_cuando_toca() {
        assert_eq!(Marca::Distinta.glifo(false), "≠");
        assert_eq!(Marca::Distinta.glifo(true), "!=");
        assert_eq!(Marca::Ausente.glifo(false), "✕");
        assert_eq!(Marca::Ausente.glifo(true), "x");
        assert_eq!(Marca::Ninguna.glifo(false), "");
    }

    #[test]
    fn el_orden_pone_los_directorios_primero_y_luego_por_nombre() {
        let mut entradas = vec![
            fichero("zeta", 1, 0),
            directorio("beta"),
            fichero("Alfa", 1, 0),
            directorio("alfa"),
        ];
        ordenar(&mut entradas);
        let nombres: Vec<&str> = entradas.iter().map(|e| e.nombre.as_str()).collect();
        assert_eq!(nombres, vec!["alfa", "beta", "Alfa", "zeta"]);
    }
}
