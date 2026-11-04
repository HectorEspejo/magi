//! Variables de un snippet: `{{nombre}}` y `{{nombre:defecto}}`, con nombre
//! `[a-z_][a-z0-9_]*`. Cualquier otra cosa entre llaves (`{{.Names}}` de
//! `docker`, `{{ x }}`) es texto literal del comando.
//!
//! La sustitución es **siempre** escapada para el shell (`shell-escape`): el
//! valor viaja como un único argumento. Eso solo protege una variable escrita
//! fuera de comillas, así que una variable dentro de `'…'`, `"…"` o del cuerpo
//! de un heredoc sin comillas no se admite (`validar_contexto`): dentro de
//! comillas dobles, un `$(…)` del valor se ejecutaría pese al escapado.

use std::borrow::Cow;
use std::collections::BTreeMap;

/// Una variable del comando, con su defecto si lo tiene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub nombre: String,
    pub defecto: Option<String>,
}

/// Por qué no se puede sustituir un comando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSustitucion {
    /// La variable está vacía y no tiene defecto.
    Vacia(String),
    /// La variable está en un sitio donde el escapado no la protege.
    Contexto(String),
}

impl std::fmt::Display for ErrorSustitucion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSustitucion::Vacia(nombre) => {
                write!(
                    f,
                    "la variable «{nombre}» está vacía y no tiene valor por defecto"
                )
            }
            ErrorSustitucion::Contexto(motivo) => f.write_str(motivo),
        }
    }
}

/// Una aparición de variable en el texto: dónde empieza y acaba (en bytes) y
/// qué dice.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Aparicion {
    inicio: usize,
    fin: usize,
    variable: Variable,
}

/// ¿Es un nombre de variable válido? `[a-z_][a-z0-9_]*`.
pub fn nombre_valido(nombre: &str) -> bool {
    let mut caracteres = nombre.chars();
    match caracteres.next() {
        Some(primero) if primero == '_' || primero.is_ascii_lowercase() => {}
        _ => return false,
    }
    caracteres.all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Intenta leer una variable que empieza en `inicio` (en `{{`).
fn leer_en(texto: &str, inicio: usize) -> Option<Aparicion> {
    let resto = texto.get(inicio..)?;
    let dentro = resto.strip_prefix("{{")?;
    let cierre = dentro.find("}}")?;
    let cuerpo = &dentro[..cierre];
    let (nombre, defecto) = match cuerpo.split_once(':') {
        Some((nombre, defecto)) => (nombre, Some(defecto.to_string())),
        None => (cuerpo, None),
    };
    if !nombre_valido(nombre) {
        return None;
    }
    if defecto
        .as_deref()
        .is_some_and(|defecto| defecto.contains('\n'))
    {
        return None;
    }
    Some(Aparicion {
        inicio,
        fin: inicio + 2 + cierre + 2,
        variable: Variable {
            nombre: nombre.to_string(),
            defecto,
        },
    })
}

/// Todas las apariciones de variables, en orden.
fn apariciones(texto: &str) -> Vec<Aparicion> {
    let mut encontradas = Vec::new();
    let mut posicion = 0;
    while let Some(relativa) = texto.get(posicion..).and_then(|resto| resto.find("{{")) {
        let inicio = posicion + relativa;
        match leer_en(texto, inicio) {
            Some(aparicion) => {
                posicion = aparicion.fin;
                encontradas.push(aparicion);
            }
            None => posicion = inicio + 1,
        }
    }
    encontradas
}

/// Variables del comando, sin repetir y en orden de aparición. Si una misma
/// variable aparece con defectos distintos, manda el primero.
pub fn detectar(comando: &str) -> Vec<Variable> {
    let mut vistas: Vec<Variable> = Vec::new();
    for aparicion in apariciones(comando) {
        match vistas
            .iter_mut()
            .find(|vista| vista.nombre == aparicion.variable.nombre)
        {
            Some(vista) => {
                if vista.defecto.is_none() {
                    vista.defecto = aparicion.variable.defecto;
                }
            }
            None => vistas.push(aparicion.variable),
        }
    }
    vistas
}

/// Contexto léxico del shell en un punto del comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Contexto {
    Normal,
    ComillaSimple,
    ComillaDoble,
    Heredoc,
}

/// Un heredoc anunciado en una línea, cuyo cuerpo empieza en la siguiente.
#[derive(Debug, Clone)]
struct HeredocPendiente {
    delimitador: String,
    /// Delimitador entre comillas: el cuerpo es literal (no expande nada).
    literal: bool,
    /// `<<-`: se quitan los tabuladores iniciales de cada línea.
    quitar_tabs: bool,
}

/// Contexto de cada byte del comando (aproximación de un lexer de `sh`
/// suficiente para saber si una variable queda entre comillas o en el cuerpo
/// de un heredoc que expande).
fn contextos(comando: &str) -> Vec<Contexto> {
    let bytes = comando.as_bytes();
    let mut contextos = vec![Contexto::Normal; bytes.len()];
    let mut i = 0;
    let mut estado = Contexto::Normal;
    let mut pendientes: Vec<HeredocPendiente> = Vec::new();
    while i < bytes.len() {
        match estado {
            Contexto::Normal => {
                contextos[i] = Contexto::Normal;
                match bytes[i] {
                    b'\\' => {
                        if i + 1 < bytes.len() {
                            contextos[i + 1] = Contexto::Normal;
                        }
                        i += 2;
                        continue;
                    }
                    b'\'' => estado = Contexto::ComillaSimple,
                    b'"' => estado = Contexto::ComillaDoble,
                    b'<' if bytes.get(i + 1) == Some(&b'<') && bytes.get(i + 2) != Some(&b'<') => {
                        let (pendiente, avance) = leer_heredoc(comando, i + 2);
                        if let Some(pendiente) = pendiente {
                            pendientes.push(pendiente);
                        }
                        let hasta = (i + 2 + avance).min(bytes.len());
                        for contexto in contextos.iter_mut().take(hasta).skip(i) {
                            *contexto = Contexto::Normal;
                        }
                        i += 2 + avance;
                        continue;
                    }
                    b'\n' if !pendientes.is_empty() => {
                        // Los cuerpos de los heredocs de esta línea empiezan en
                        // la siguiente, uno tras otro.
                        i += 1;
                        for pendiente in std::mem::take(&mut pendientes) {
                            i = recorrer_cuerpo(comando, i, &pendiente, &mut contextos);
                        }
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            Contexto::ComillaSimple => {
                contextos[i] = Contexto::ComillaSimple;
                if bytes[i] == b'\'' {
                    contextos[i] = Contexto::Normal;
                    estado = Contexto::Normal;
                }
                i += 1;
            }
            Contexto::ComillaDoble => {
                contextos[i] = Contexto::ComillaDoble;
                match bytes[i] {
                    b'\\' => {
                        if i + 1 < bytes.len() {
                            contextos[i + 1] = Contexto::ComillaDoble;
                        }
                        i += 2;
                        continue;
                    }
                    b'"' => {
                        contextos[i] = Contexto::Normal;
                        estado = Contexto::Normal;
                    }
                    _ => {}
                }
                i += 1;
            }
            Contexto::Heredoc => unreachable!("los cuerpos se recorren aparte"),
        }
    }
    contextos
}

/// Lee el delimitador de un heredoc tras `<<`; devuelve lo leído y cuántos
/// bytes ocupa (el `-`, espacios y el delimitador).
fn leer_heredoc(comando: &str, desde: usize) -> (Option<HeredocPendiente>, usize) {
    let resto = &comando[desde..];
    let mut avance = 0;
    let quitar_tabs = resto.starts_with('-');
    if quitar_tabs {
        avance += 1;
    }
    let sin_espacios = resto[avance..].trim_start_matches([' ', '\t']);
    avance = resto.len() - sin_espacios.len();
    let mut delimitador = String::new();
    let mut literal = false;
    let mut chars = sin_espacios.char_indices().peekable();
    let mut fin = 0;
    while let Some((posicion, c)) = chars.next() {
        match c {
            '\'' | '"' => {
                literal = true;
                let cierre = sin_espacios[posicion + 1..].find(c);
                match cierre {
                    Some(cierre) => {
                        delimitador.push_str(&sin_espacios[posicion + 1..posicion + 1 + cierre]);
                        // Saltar hasta después de la comilla de cierre.
                        let destino = posicion + 1 + cierre + 1;
                        while chars.peek().is_some_and(|(p, _)| *p < destino) {
                            chars.next();
                        }
                        fin = destino;
                    }
                    None => {
                        fin = sin_espacios.len();
                        break;
                    }
                }
            }
            '\\' => {
                literal = true;
                if let Some((_, siguiente)) = chars.next() {
                    delimitador.push(siguiente);
                }
                fin = posicion + 2;
            }
            c if c.is_whitespace() || matches!(c, ';' | '|' | '&' | '<' | '>' | '(' | ')') => {
                break;
            }
            c => {
                delimitador.push(c);
                fin = posicion + c.len_utf8();
            }
        }
    }
    if delimitador.is_empty() {
        return (None, avance);
    }
    (
        Some(HeredocPendiente {
            delimitador,
            literal,
            quitar_tabs,
        }),
        avance + fin,
    )
}

/// Marca el cuerpo de un heredoc desde `desde` hasta su línea de cierre
/// (incluida) y devuelve dónde sigue el comando.
fn recorrer_cuerpo(
    comando: &str,
    desde: usize,
    heredoc: &HeredocPendiente,
    contextos: &mut [Contexto],
) -> usize {
    let mut i = desde;
    while i < comando.len() {
        let fin_linea = comando[i..].find('\n').map_or(comando.len(), |p| i + p);
        let linea = &comando[i..fin_linea];
        let comparada = if heredoc.quitar_tabs {
            linea.trim_start_matches('\t')
        } else {
            linea
        };
        let contexto = if comparada == heredoc.delimitador || heredoc.literal {
            // La línea de cierre es sintaxis; un cuerpo literal no expande.
            Contexto::Normal
        } else {
            Contexto::Heredoc
        };
        for posicion in contextos.iter_mut().take(fin_linea).skip(i) {
            *posicion = contexto;
        }
        let siguiente = (fin_linea + 1).min(comando.len());
        if comparada == heredoc.delimitador {
            return siguiente;
        }
        i = siguiente;
        if fin_linea == comando.len() {
            break;
        }
    }
    comando.len()
}

/// Comprueba que ninguna variable queda donde el escapado no la protege:
/// entre comillas simples o dobles, o en el cuerpo de un heredoc que expande.
pub fn validar_contexto(comando: &str) -> Result<(), String> {
    let contextos = contextos(comando);
    for aparicion in apariciones(comando) {
        let donde = match contextos.get(aparicion.inicio) {
            Some(Contexto::ComillaSimple) => "entre comillas simples",
            Some(Contexto::ComillaDoble) => "entre comillas dobles",
            Some(Contexto::Heredoc) => "dentro de un heredoc",
            _ => continue,
        };
        return Err(format!(
            "la variable «{}» está {donde}: escríbela sin comillas, MAGI ya entrecomilla su valor",
            aparicion.variable.nombre
        ));
    }
    Ok(())
}

/// Valor que se usará para una variable: lo escrito o, si está vacío, el
/// defecto. Vacía y sin defecto no vale.
pub fn valor_efectivo(variable: &Variable, escrito: &str) -> Result<String, ErrorSustitucion> {
    if !escrito.is_empty() {
        return Ok(escrito.to_string());
    }
    match &variable.defecto {
        Some(defecto) if !defecto.is_empty() => Ok(defecto.clone()),
        _ => Err(ErrorSustitucion::Vacia(variable.nombre.clone())),
    }
}

/// Sustituye cada variable por su valor escapado para el shell, en una sola
/// pasada (un valor que contenga `{{otra}}` no se vuelve a expandir). Los
/// valores que falten o estén vacíos toman el defecto; si no lo hay, error.
pub fn sustituir(
    comando: &str,
    valores: &BTreeMap<String, String>,
) -> Result<String, ErrorSustitucion> {
    validar_contexto(comando).map_err(ErrorSustitucion::Contexto)?;
    let mut resultado = String::with_capacity(comando.len());
    let mut posicion = 0;
    for aparicion in apariciones(comando) {
        resultado.push_str(&comando[posicion..aparicion.inicio]);
        let escrito = valores
            .get(&aparicion.variable.nombre)
            .map(String::as_str)
            .unwrap_or("");
        let valor = valor_efectivo(&aparicion.variable, escrito)?;
        resultado.push_str(&shell_escape::unix::escape(Cow::from(valor.as_str())));
        posicion = aparicion.fin;
    }
    resultado.push_str(&comando[posicion..]);
    Ok(resultado)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn valores(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(nombre, valor)| (nombre.to_string(), valor.to_string()))
            .collect()
    }

    #[test]
    fn detecta_variables_con_y_sin_defecto() {
        let variables = detectar("journalctl --vacuum-time={{dias:7}}d -u {{unidad}} {{dias}}");
        assert_eq!(
            variables,
            vec![
                Variable {
                    nombre: "dias".to_string(),
                    defecto: Some("7".to_string())
                },
                Variable {
                    nombre: "unidad".to_string(),
                    defecto: None
                },
            ]
        );
    }

    #[test]
    fn lo_que_no_es_variable_es_texto_literal() {
        assert!(detectar("docker ps --format {{.Names}}").is_empty());
        assert!(detectar("echo {{ x }} {{Mayus}} {{1a}} {{sin_cerrar").is_empty());
        assert_eq!(detectar("{{x:}}")[0].defecto.as_deref(), Some(""));
    }

    #[test]
    fn un_valor_sencillo_no_lleva_comillas() {
        let comando = sustituir(
            "systemctl restart {{servicio}}",
            &valores(&[("servicio", "nginx")]),
        )
        .unwrap();
        assert_eq!(comando, "systemctl restart nginx");
    }

    /// AC (checklist l. 32): el valor viaja como un único argumento.
    #[test]
    fn un_valor_peligroso_viaja_como_un_solo_argumento() {
        let comando = sustituir(
            "systemctl restart {{servicio}}",
            &valores(&[("servicio", "a b; rm -rf /")]),
        )
        .unwrap();
        assert_eq!(comando, "systemctl restart 'a b; rm -rf /'");
        // Comprobado con un `sh` de verdad: un solo argumento, intacto.
        let prueba = sustituir(
            "printf '%s|' {{v}}",
            &valores(&[("v", "a b; $(echo x) `id` '\"")]),
        );
        let prueba = prueba.expect("sin comillas alrededor de la variable");
        let salida = std::process::Command::new("sh")
            .arg("-c")
            .arg(&prueba)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&salida.stdout),
            "a b; $(echo x) `id` '\"|"
        );
    }

    #[test]
    fn vacia_sin_defecto_no_deja_continuar_y_con_defecto_usa_el_defecto() {
        assert_eq!(
            sustituir("echo {{x}}", &valores(&[("x", "")])),
            Err(ErrorSustitucion::Vacia("x".to_string()))
        );
        assert_eq!(
            sustituir("echo {{x:hola}}", &valores(&[("x", "")])).unwrap(),
            "echo hola"
        );
        assert_eq!(
            sustituir("echo {{x:}}", &BTreeMap::new()),
            Err(ErrorSustitucion::Vacia("x".to_string()))
        );
    }

    #[test]
    fn un_valor_con_otra_variable_no_se_vuelve_a_expandir() {
        let comando = sustituir(
            "echo {{a}} {{b}}",
            &valores(&[("a", "{{b}}"), ("b", "dos")]),
        )
        .unwrap();
        assert_eq!(comando, "echo '{{b}}' dos");
    }

    #[test]
    fn la_misma_variable_se_sustituye_en_todas_sus_apariciones() {
        let comando = sustituir("cp {{f}} {{f}}.bak", &valores(&[("f", "x y")])).unwrap();
        assert_eq!(comando, "cp 'x y' 'x y'.bak");
    }

    #[test]
    fn una_variable_entre_comillas_no_se_admite() {
        assert!(validar_contexto("grep '{{p}}' f").is_err());
        assert!(validar_contexto("echo \"hola {{nombre}}\"").is_err());
        assert!(validar_contexto("echo \"a\\\"{{x}}\"").is_err());
        assert!(matches!(
            sustituir("grep '{{p}}' f", &valores(&[("p", "x")])),
            Err(ErrorSustitucion::Contexto(_))
        ));
    }

    #[test]
    fn fuera_de_comillas_si_se_admite() {
        assert!(validar_contexto("echo 'a' {{x}} \"b\" {{y}}").is_ok());
        assert!(validar_contexto("echo $(cat {{f}})").is_ok());
        assert!(validar_contexto("echo \\'{{x}}").is_ok());
        assert!(validar_contexto("docker ps --format '{{.Names}}'").is_ok());
    }

    #[test]
    fn el_cuerpo_de_un_heredoc_que_expande_no_admite_variables() {
        let comando = "cat <<EOF > /tmp/x\nhola {{nombre}}\nEOF\necho {{otra}}";
        assert!(validar_contexto(comando).is_err());
        let con_tabs = "cat <<-FIN\n\t{{x}}\n\tFIN";
        assert!(validar_contexto(con_tabs).is_err());
    }

    #[test]
    fn tras_el_heredoc_y_en_uno_literal_si_se_admiten() {
        let despues = "cat <<EOF\nhola\nEOF\necho {{x}}";
        assert!(validar_contexto(despues).is_ok());
        let literal = "cat <<'EOF'\n{{x}}\nEOF";
        assert!(validar_contexto(literal).is_ok());
        let here_string = "cat <<< {{x}}";
        assert!(validar_contexto(here_string).is_ok());
    }

    #[test]
    fn los_nombres_validos() {
        assert!(nombre_valido("dias"));
        assert!(nombre_valido("_x1"));
        assert!(!nombre_valido("1x"));
        assert!(!nombre_valido("Dias"));
        assert!(!nombre_valido(""));
        assert!(!nombre_valido("a-b"));
    }
}
