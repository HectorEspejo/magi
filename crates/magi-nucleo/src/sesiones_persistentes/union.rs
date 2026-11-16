//! Unión de dos fichas del mismo host y nombre (§7.5, D337): queda la de
//! `uuid` menor, con lo que le falte de la otra.
//!
//! Dos fichas con el mismo host y nombre son la misma sesión de tmux, así que
//! no se pueden renombrar a «x (2)» como el resto de elementos (D136): la
//! ficha renombrada apuntaría a una sesión que no existe. La regla es pura y
//! determinista, sin reloj: cada equipo llega a la misma ficha aunque las una
//! en orden distinto, y la siguiente pasada iguala las versiones.
//!
//! Este módulo solo decide quién gana y con qué campos. Quien llama (la
//! fusión de la sincronización) traduce el resultado a su plan, y la
//! aplicación escribe la lápida de la perdedora y la edición de la ganadora.

/// Campos de una ficha que se completan desde la otra al unirlas, en el orden
/// en que se miran.
pub const CAMPOS_UNIBLES: [&str; 4] = ["carpeta", "comando", "comando_reanudar", "nota"];

/// Lo que interviene de una ficha en la unión: su `uuid` y los campos que
/// pueden rellenarse. El resto (host, nombre, origen, quién la creó) es igual o
/// se queda como lo tiene la ganadora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FichaUnible {
    pub uuid: String,
    pub carpeta: Option<String>,
    pub comando: Option<String>,
    pub comando_reanudar: Option<String>,
    pub nota: Option<String>,
}

impl FichaUnible {
    /// Campo por su nombre en `CAMPOS_UNIBLES`.
    fn campo(&self, nombre: &str) -> &Option<String> {
        match nombre {
            "carpeta" => &self.carpeta,
            "comando" => &self.comando,
            "comando_reanudar" => &self.comando_reanudar,
            "nota" => &self.nota,
            _ => &None,
        }
    }

    fn campo_mut(&mut self, nombre: &str) -> Option<&mut Option<String>> {
        match nombre {
            "carpeta" => Some(&mut self.carpeta),
            "comando" => Some(&mut self.comando),
            "comando_reanudar" => Some(&mut self.comando_reanudar),
            "nota" => Some(&mut self.nota),
            _ => None,
        }
    }

    /// Valor de un campo, o `None` si falta o está vacío.
    pub fn valor(&self, nombre: &str) -> Option<&str> {
        self.campo(nombre)
            .as_deref()
            .filter(|texto| !texto.is_empty())
    }
}

/// Resultado de unir un grupo de fichas del mismo host y nombre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoUnion {
    /// La ficha que queda, con los campos que le faltaban ya rellenados.
    pub ganadora: FichaUnible,
    /// `uuid` de las que desaparecen, de menor a mayor.
    pub perdedoras: Vec<String>,
    /// La ganadora ha recibido algún campo de otra: es una edición nueva.
    pub editada: bool,
}

/// Une dos fichas: gana la de `uuid` menor y, de lo que le falte (carpeta,
/// comando, comando de reanudación y nota), se queda con lo que traiga la
/// otra. Devuelve la ganadora (rellenada), la perdedora tal cual y si la
/// ganadora ha cambiado.
pub fn unir(a: &FichaUnible, b: &FichaUnible) -> (FichaUnible, FichaUnible, bool) {
    let (gana, pierde) = if a.uuid < b.uuid { (a, b) } else { (b, a) };
    let mut ganadora = gana.clone();
    let mut editada = false;
    for campo in CAMPOS_UNIBLES {
        if ganadora.valor(campo).is_none() {
            if let Some(valor) = pierde.valor(campo) {
                if let Some(destino) = ganadora.campo_mut(campo) {
                    *destino = Some(valor.to_string());
                    editada = true;
                }
            }
        }
    }
    (ganadora, pierde.clone(), editada)
}

/// Une todas las fichas de un grupo (mismo host y nombre). Con menos de dos
/// `uuid` distintos no hay nada que unir y devuelve `None`.
///
/// El resultado no depende del orden de entrada: gana el `uuid` menor y cada
/// campo vacío se rellena con el de la primera perdedora, por `uuid`
/// creciente, que lo tenga.
pub fn unir_grupo(fichas: &[FichaUnible]) -> Option<ResultadoUnion> {
    let mut ordenadas: Vec<&FichaUnible> = fichas.iter().collect();
    ordenadas.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    ordenadas.dedup_by(|a, b| a.uuid == b.uuid);
    let (primera, resto) = ordenadas.split_first()?;
    if resto.is_empty() {
        return None;
    }
    let mut ganadora = (*primera).clone();
    let mut editada = false;
    let mut perdedoras = Vec::with_capacity(resto.len());
    for otra in resto {
        let (rellenada, _, cambio) = unir(&ganadora, otra);
        ganadora = rellenada;
        editada |= cambio;
        perdedoras.push(otra.uuid.clone());
    }
    Some(ResultadoUnion {
        ganadora,
        perdedoras,
        editada,
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ficha(uuid: &str) -> FichaUnible {
        FichaUnible {
            uuid: uuid.to_string(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
        }
    }

    #[test]
    fn gana_el_uuid_menor_sea_cual_sea_el_orden() {
        let a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        let b = ficha("bbbbbbbb-0000-4000-8000-000000000002");
        let (gana, pierde, editada) = unir(&a, &b);
        assert_eq!(gana.uuid, a.uuid);
        assert_eq!(pierde.uuid, b.uuid);
        assert!(!editada);
        let (gana, pierde, _) = unir(&b, &a);
        assert_eq!(gana.uuid, a.uuid);
        assert_eq!(pierde.uuid, b.uuid);
    }

    #[test]
    fn rellena_lo_que_falta_y_conserva_lo_que_tiene() {
        let mut a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        a.carpeta = Some("~/proyectos/api".to_string());
        a.nota = Some("la mía".to_string());
        let mut b = ficha("bbbbbbbb-0000-4000-8000-000000000002");
        b.carpeta = Some("/srv/otra".to_string());
        b.comando = Some("claude".to_string());
        b.comando_reanudar = Some("claude --continue".to_string());
        b.nota = Some("la otra".to_string());

        let (gana, _, editada) = unir(&a, &b);
        assert!(editada);
        // Lo que ya tenía no se pisa.
        assert_eq!(gana.carpeta.as_deref(), Some("~/proyectos/api"));
        assert_eq!(gana.nota.as_deref(), Some("la mía"));
        // Lo que le faltaba lo trae la otra.
        assert_eq!(gana.comando.as_deref(), Some("claude"));
        assert_eq!(gana.comando_reanudar.as_deref(), Some("claude --continue"));
    }

    #[test]
    fn un_texto_vacio_cuenta_como_falta() {
        let mut a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        a.comando = Some(String::new());
        let mut b = ficha("bbbbbbbb-0000-4000-8000-000000000002");
        b.comando = Some("htop".to_string());
        let (gana, _, editada) = unir(&a, &b);
        assert!(editada);
        assert_eq!(gana.comando.as_deref(), Some("htop"));
    }

    #[test]
    fn sin_nada_que_aportar_no_es_una_edicion() {
        let mut a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        a.carpeta = Some("~".to_string());
        let b = ficha("bbbbbbbb-0000-4000-8000-000000000002");
        let (gana, _, editada) = unir(&a, &b);
        assert!(!editada);
        assert_eq!(gana, a);
    }

    #[test]
    fn el_grupo_no_depende_del_orden() {
        let a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        let mut b = ficha("bbbbbbbb-0000-4000-8000-000000000002");
        b.comando = Some("de-b".to_string());
        b.nota = Some("nota-b".to_string());
        let mut c = ficha("cccccccc-0000-4000-8000-000000000003");
        c.comando = Some("de-c".to_string());
        c.carpeta = Some("/c".to_string());

        let referencia = unir_grupo(&[a.clone(), b.clone(), c.clone()]).expect("hay unión");
        assert_eq!(referencia.ganadora.uuid, a.uuid);
        assert_eq!(referencia.perdedoras, vec![b.uuid.clone(), c.uuid.clone()]);
        assert!(referencia.editada);
        // El campo vacío se rellena con la primera perdedora (por uuid) que lo
        // tenga: el comando de b, no el de c.
        assert_eq!(referencia.ganadora.comando.as_deref(), Some("de-b"));
        assert_eq!(referencia.ganadora.carpeta.as_deref(), Some("/c"));
        assert_eq!(referencia.ganadora.nota.as_deref(), Some("nota-b"));

        let permutaciones = [
            [b.clone(), a.clone(), c.clone()],
            [c.clone(), b.clone(), a.clone()],
            [b.clone(), c.clone(), a.clone()],
            [c.clone(), a.clone(), b.clone()],
        ];
        for permutacion in permutaciones {
            assert_eq!(unir_grupo(&permutacion).as_ref(), Some(&referencia));
        }
    }

    #[test]
    fn una_sola_ficha_no_se_une() {
        let a = ficha("aaaaaaaa-0000-4000-8000-000000000001");
        assert_eq!(unir_grupo(std::slice::from_ref(&a)), None);
        // El mismo uuid repetido tampoco es una unión.
        assert_eq!(unir_grupo(&[a.clone(), a]), None);
        assert_eq!(unir_grupo(&[]), None);
    }
}
