//! Cobertura de glifos de las fuentes de la web (informe §7.8).
//!
//! La prueba de cobertura de las capturas (S2) necesita saber qué caracteres
//! tienen glifo en JetBrains Mono (se lee de los TTF de la app) y en el
//! subconjunto `magi-simbolos.woff2` (se lee de `magi-simbolos.unicodes`, que es a
//! la vez la entrada de `pyftsubset` y la fuente de verdad de la prueba).
//!
//! El lector de `cmap` usa solo `std` y trata el fichero como entrada no fiable:
//! todo desplazamiento y toda longitud se comprueban antes de leer, nada se indexa
//! sin comprobar y un fichero truncado o corrupto es un [`ErrorWeb`], nunca un pánico.

use std::collections::BTreeSet;

use crate::error::{ErrorWeb, Resultado};

/// De dónde sale lo que se lee, para los mensajes de error.
const DONDE_TTF: &str = "tabla cmap de la fuente";
const DONDE_UNICODES: &str = "lista de unicodes";

/// Tope del tamaño de un TTF/OTF (el de JetBrains Mono pesa unos 270 KB).
const MAX_BYTES_FUENTE: usize = 64 * 1024 * 1024;
/// Tope de tablas en el directorio de la fuente (una fuente real lleva unas 20).
const MAX_TABLAS: usize = 512;
/// Tope de registros de codificación de `cmap` (una fuente real lleva 3 a 6).
const MAX_REGISTROS_CMAP: usize = 256;
/// Tope de grupos de una subtabla de formato 12 (las fuentes reales con todo
/// Unicode llevan decenas de miles).
const MAX_GRUPOS: usize = 200_000;
/// El último punto de código válido de Unicode.
const MAX_UNICODE: u32 = 0x10FFFF;
/// Tope de puntos de código que se recorren entre todas las subtablas de un `cmap`.
///
/// Los segmentos y los grupos sin solapes acotan una subtabla (65 536 o 1,1 M de
/// códigos), pero un `cmap` puede llevar hasta 256 registros que apunten a otras
/// tantas subtablas distintas con todo Unicode en 28 bytes cada una: sin este
/// tope, una fuente de 9 KB tardaba casi un minuto en leerse. Una fuente real con
/// el BMP y todo Unicode recorre unos 1,2 M; el tope deja sitio de sobra.
const MAX_CODIGOS_RECORRIDOS: u64 = 2_500_000;

/// Lo que queda de [`MAX_CODIGOS_RECORRIDOS`] mientras se leen las subtablas.
struct Presupuesto {
    restante: u64,
}

impl Presupuesto {
    fn nuevo() -> Self {
        Self {
            restante: MAX_CODIGOS_RECORRIDOS,
        }
    }

    /// Descuenta `codigos` puntos de código que se van a recorrer, o falla si el
    /// `cmap` pide más trabajo del que se admite.
    fn gastar(&mut self, codigos: u64, etiqueta: &str) -> Resultado<()> {
        match self.restante.checked_sub(codigos) {
            Some(resto) => {
                self.restante = resto;
                Ok(())
            }
            None => Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!(
                    "{etiqueta}: entre todas las subtablas de cmap recorren más de {MAX_CODIGOS_RECORRIDOS} puntos de código (¿fichero dañado o hostil?)"
                ),
            )),
        }
    }
}

/// Lector de enteros de la tabla en curso: toda lectura se comprueba.
struct Lector<'a> {
    datos: &'a [u8],
    /// Qué se está leyendo, para el mensaje de error.
    contexto: String,
}

impl<'a> Lector<'a> {
    fn nuevo(datos: &'a [u8], contexto: impl Into<String>) -> Self {
        Self {
            datos,
            contexto: contexto.into(),
        }
    }

    fn error_de_lectura(&self, pos: usize, bytes: usize) -> ErrorWeb {
        ErrorWeb::dato_raro(
            DONDE_TTF,
            format!(
                "{}: se esperaban {bytes} bytes en el desplazamiento {pos} y la tabla mide {} (¿fichero truncado o dañado?)",
                self.contexto,
                self.datos.len()
            ),
        )
    }

    fn u16(&self, pos: usize) -> Resultado<u16> {
        let fin = pos
            .checked_add(2)
            .ok_or_else(|| self.error_de_lectura(pos, 2))?;
        match self.datos.get(pos..fin) {
            Some([a, b]) => Ok(u16::from_be_bytes([*a, *b])),
            _ => Err(self.error_de_lectura(pos, 2)),
        }
    }

    fn u32(&self, pos: usize) -> Resultado<u32> {
        let fin = pos
            .checked_add(4)
            .ok_or_else(|| self.error_de_lectura(pos, 4))?;
        match self.datos.get(pos..fin) {
            Some([a, b, c, d]) => Ok(u32::from_be_bytes([*a, *b, *c, *d])),
            _ => Err(self.error_de_lectura(pos, 4)),
        }
    }
}

/// Puntos de código con glifo en un TTF (tabla `cmap`, formatos 4 y 12), solo con `std`.
///
/// Une todas las subtablas Unicode del fichero (plataforma 0, o plataforma 3 con
/// codificación 1 o 10) de formato 4 o 12; las demás (formato 14, las de Mac o las
/// de símbolos) se ignoran. Un punto de código cuyo glifo es el 0 (`.notdef`) no
/// cuenta como cubierto. Falla si el fichero está truncado o dañado, si cualquier
/// desplazamiento o longitud se sale de su tabla, si los segmentos o grupos se
/// solapan o van desordenados, si entre todas las subtablas piden recorrer más de
/// 2,5 M de puntos de código ([`MAX_CODIGOS_RECORRIDOS`]), o si no hay ninguna
/// subtabla Unicode legible.
pub fn codigos_de_ttf(bytes: &[u8]) -> Resultado<BTreeSet<u32>> {
    let cmap = localizar_cmap(bytes)?;
    let lector = Lector::nuevo(cmap, "cabecera de cmap");

    // Versión 0 y número de registros de codificación.
    let version = lector.u16(0)?;
    if version != 0 {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!("versión de cmap {version} desconocida (se esperaba la 0)"),
        ));
    }
    let registros = usize::from(lector.u16(2)?);
    if registros > MAX_REGISTROS_CMAP {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!("cmap declara {registros} registros de codificación (el tope es {MAX_REGISTROS_CMAP})"),
        ));
    }

    let mut codigos = BTreeSet::new();
    let mut leidas = 0usize;
    let mut vistas: BTreeSet<u32> = BTreeSet::new();
    let mut presupuesto = Presupuesto::nuevo();
    for i in 0..registros {
        let base = 4 + 8 * i;
        let plataforma = lector.u16(base)?;
        let codificacion = lector.u16(base + 2)?;
        let desplazamiento = lector.u32(base + 4)?;
        let es_unicode = plataforma == 0 || (plataforma == 3 && matches!(codificacion, 1 | 10));
        if !es_unicode {
            continue;
        }
        // Varios registros suelen apuntar a la misma subtabla: se lee una vez.
        if !vistas.insert(desplazamiento) {
            continue;
        }
        let etiqueta =
            format!("subtabla de la plataforma {plataforma}, codificación {codificacion}");
        let inicio = usize::try_from(desplazamiento).unwrap_or(usize::MAX);
        let Some(subtabla) = cmap.get(inicio..) else {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!(
                    "{etiqueta}: su desplazamiento {desplazamiento} sale de la tabla cmap, que mide {} bytes",
                    cmap.len()
                ),
            ));
        };
        let formato = Lector::nuevo(subtabla, etiqueta.clone()).u16(0)?;
        let leidos = match formato {
            4 => leer_formato_4(subtabla, &etiqueta, &mut presupuesto)?,
            12 => leer_formato_12(subtabla, &etiqueta, &mut presupuesto)?,
            // Formato 14 (variaciones), 0, 6, 10, 13...: no se necesitan.
            _ => continue,
        };
        leidas += 1;
        codigos.extend(leidos);
    }

    if leidas == 0 {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            "ninguna subtabla Unicode de formato 4 o 12 (plataforma 0, o 3 con codificación 1 o 10)",
        ));
    }
    Ok(codigos)
}

/// Comprueba la cabecera `sfnt` y su directorio de tablas, y devuelve la tabla `cmap`.
fn localizar_cmap(bytes: &[u8]) -> Resultado<&[u8]> {
    if bytes.len() > MAX_BYTES_FUENTE {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!(
                "el fichero mide {} bytes (el tope es {MAX_BYTES_FUENTE})",
                bytes.len()
            ),
        ));
    }
    let cabecera = Lector::nuevo(bytes, "cabecera de la fuente");
    if bytes.len() < 12 {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!(
                "el fichero mide {} bytes: no cabe ni la cabecera de un TTF/OTF",
                bytes.len()
            ),
        ));
    }
    match &bytes[..4] {
        [0x00, 0x01, 0x00, 0x00] | b"OTTO" | b"true" => {}
        b"wOF2" | b"wOFF" => {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                "es un woff o woff2: hace falta el TTF/OTF sin comprimir",
            ))
        }
        b"ttcf" => {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                "es una colección TTC: no se admite, hace falta un TTF/OTF suelto",
            ))
        }
        otro => {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!("no es un TTF/OTF: empieza por {otro:02X?}"),
            ))
        }
    }

    let tablas = usize::from(cabecera.u16(4)?);
    if tablas == 0 || tablas > MAX_TABLAS {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!("el directorio declara {tablas} tablas (se admiten de 1 a {MAX_TABLAS})"),
        ));
    }

    let mut cmap: Option<&[u8]> = None;
    for i in 0..tablas {
        let base = 12 + 16 * i;
        let Some(registro) = bytes.get(base..base + 16) else {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!(
                    "el directorio declara {tablas} tablas pero el fichero se acaba en la {} (¿truncado?)",
                    i + 1
                ),
            ));
        };
        let etiqueta = String::from_utf8_lossy(&registro[..4]).into_owned();
        let desplazamiento = u64::from(u32::from_be_bytes([
            registro[8],
            registro[9],
            registro[10],
            registro[11],
        ]));
        let longitud = u64::from(u32::from_be_bytes([
            registro[12],
            registro[13],
            registro[14],
            registro[15],
        ]));
        let fin = desplazamiento + longitud;
        if fin > bytes.len() as u64 {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!(
                    "la tabla «{etiqueta}» ocupa de {desplazamiento} a {fin} y el fichero mide {} bytes (¿truncado o dañado?)",
                    bytes.len()
                ),
            ));
        }
        if &registro[..4] == b"cmap" && cmap.is_none() {
            // `desplazamiento` y `fin` caben en usize: son ≤ bytes.len().
            cmap = bytes.get(desplazamiento as usize..fin as usize);
        }
    }
    cmap.ok_or_else(|| ErrorWeb::dato_raro(DONDE_TTF, "el fichero no tiene tabla cmap"))
}

/// Subtabla de formato 4 (BMP, por segmentos). `subtabla` va desde su inicio hasta
/// el final de `cmap`: el campo `length` de este formato es un `u16` y algunas
/// fuentes lo desbordan, así que no se usa; los límites son los de la tabla.
fn leer_formato_4(
    subtabla: &[u8],
    etiqueta: &str,
    presupuesto: &mut Presupuesto,
) -> Resultado<BTreeSet<u32>> {
    let lector = Lector::nuevo(subtabla, format!("{etiqueta}, formato 4"));
    let doble_de_segmentos = usize::from(lector.u16(6)?);
    if doble_de_segmentos == 0 || doble_de_segmentos % 2 != 0 {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!("{etiqueta}, formato 4: segCountX2 = {doble_de_segmentos} no es un número par mayor que 0"),
        ));
    }
    let segmentos = doble_de_segmentos / 2;
    // Cabecera (14) + endCode + relleno (2) + startCode + idDelta + idRangeOffset.
    let minimo = 16 + 4 * doble_de_segmentos;
    if minimo > subtabla.len() {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!(
                "{etiqueta}, formato 4: {segmentos} segmentos necesitan {minimo} bytes y la subtabla solo tiene {} (¿truncada?)",
                subtabla.len()
            ),
        ));
    }
    let ini_fin = 14;
    let ini_inicio = 16 + doble_de_segmentos;
    let ini_delta = ini_inicio + doble_de_segmentos;
    let ini_rango = ini_delta + doble_de_segmentos;

    let mut codigos = BTreeSet::new();
    let mut fin_anterior: Option<u32> = None;
    for i in 0..segmentos {
        let fin = u32::from(lector.u16(ini_fin + 2 * i)?);
        let inicio = u32::from(lector.u16(ini_inicio + 2 * i)?);
        let delta = lector.u16(ini_delta + 2 * i)?;
        let pos_rango = ini_rango + 2 * i;
        let desplazamiento_rango = usize::from(lector.u16(pos_rango)?);
        if inicio > fin {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!("{etiqueta}, formato 4: el segmento {i} empieza en U+{inicio:04X} y acaba en U+{fin:04X}"),
            ));
        }
        // Segmentos ordenados y sin solapes: acota el trabajo a 65 536 códigos.
        if let Some(anterior) = fin_anterior {
            if inicio <= anterior {
                return Err(ErrorWeb::dato_raro(
                    DONDE_TTF,
                    format!(
                        "{etiqueta}, formato 4: el segmento {i} (U+{inicio:04X}..U+{fin:04X}) se solapa con el anterior o va desordenado"
                    ),
                ));
            }
        }
        fin_anterior = Some(fin);
        presupuesto.gastar(u64::from(fin - inicio) + 1, etiqueta)?;
        if desplazamiento_rango % 2 != 0 && !(inicio == 0xFFFF && fin == 0xFFFF) {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!("{etiqueta}, formato 4: idRangeOffset impar ({desplazamiento_rango}) en el segmento {i}"),
            ));
        }

        for codigo in inicio..=fin {
            // U+FFFF cierra siempre la tabla (centinela) y no es un carácter.
            if codigo == 0xFFFF {
                continue;
            }
            let glifo = if desplazamiento_rango == 0 {
                (codigo as u16).wrapping_add(delta)
            } else {
                // El desplazamiento se cuenta desde la propia posición de idRangeOffset[i].
                let pos = pos_rango
                    .checked_add(desplazamiento_rango)
                    .and_then(|p| p.checked_add(2 * (codigo - inicio) as usize))
                    .ok_or_else(|| {
                        ErrorWeb::dato_raro(
                            DONDE_TTF,
                            format!("{etiqueta}, formato 4: desplazamiento desbordado en el segmento {i}"),
                        )
                    })?;
                match lector.u16(pos)? {
                    0 => 0,
                    g => g.wrapping_add(delta),
                }
            };
            if glifo != 0 {
                codigos.insert(codigo);
            }
        }
    }
    Ok(codigos)
}

/// Subtabla de formato 12 (todo Unicode, por grupos).
fn leer_formato_12(
    subtabla: &[u8],
    etiqueta: &str,
    presupuesto: &mut Presupuesto,
) -> Resultado<BTreeSet<u32>> {
    let lector = Lector::nuevo(subtabla, format!("{etiqueta}, formato 12"));
    let grupos = usize::try_from(lector.u32(12)?).unwrap_or(usize::MAX);
    if grupos > MAX_GRUPOS {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!("{etiqueta}, formato 12: declara {grupos} grupos (el tope es {MAX_GRUPOS})"),
        ));
    }
    let minimo = 16 + 12 * grupos;
    if minimo > subtabla.len() {
        return Err(ErrorWeb::dato_raro(
            DONDE_TTF,
            format!(
                "{etiqueta}, formato 12: {grupos} grupos necesitan {minimo} bytes y la subtabla solo tiene {} (¿truncada?)",
                subtabla.len()
            ),
        ));
    }

    let mut codigos = BTreeSet::new();
    let mut fin_anterior: Option<u32> = None;
    for i in 0..grupos {
        let base = 16 + 12 * i;
        let inicio = lector.u32(base)?;
        let fin = lector.u32(base + 4)?;
        let glifo_inicial = lector.u32(base + 8)?;
        if inicio > fin || fin > MAX_UNICODE {
            return Err(ErrorWeb::dato_raro(
                DONDE_TTF,
                format!(
                    "{etiqueta}, formato 12: el grupo {i} va de U+{inicio:04X} a U+{fin:04X} (debe ir inicio ≤ fin ≤ U+10FFFF)"
                ),
            ));
        }
        if let Some(anterior) = fin_anterior {
            if inicio <= anterior {
                return Err(ErrorWeb::dato_raro(
                    DONDE_TTF,
                    format!(
                        "{etiqueta}, formato 12: el grupo {i} (U+{inicio:04X}..U+{fin:04X}) se solapa con el anterior o va desordenado"
                    ),
                ));
            }
        }
        fin_anterior = Some(fin);
        presupuesto.gastar(u64::from(fin - inicio) + 1, etiqueta)?;
        for codigo in inicio..=fin {
            // El glifo 0 es `.notdef`: el carácter no tiene glifo propio.
            if u64::from(glifo_inicial) + u64::from(codigo - inicio) != 0 {
                codigos.insert(codigo);
            }
        }
    }
    Ok(codigos)
}

/// Lee un fichero de unicodes de `pyftsubset`: `U+21B5`, rangos `U+25D0-25D1`,
/// comentarios con `#` y líneas en blanco.
///
/// Admite una o varias entradas por línea, separadas por espacios o comas, con la
/// misma forma que `pyftsubset --unicodes-file` (el rango lleva `U+` solo delante
/// del primer extremo). Cualquier otra cosa es un error que nombra la línea.
pub fn leer_unicodes(texto: &str) -> Resultado<BTreeSet<u32>> {
    let mut codigos = BTreeSet::new();
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    for (indice, linea) in texto.lines().enumerate() {
        let numero = indice + 1;
        let sin_comentario = linea.split('#').next().unwrap_or("");
        for entrada in sin_comentario
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|e| !e.is_empty())
        {
            let donde = format!("{DONDE_UNICODES}, línea {numero}");
            let (desde, hasta) = leer_entrada(entrada).map_err(|que| {
                ErrorWeb::dato_raro(&donde, format!("no se entiende «{entrada}»: {que}"))
            })?;
            codigos.extend(desde..=hasta);
        }
    }
    Ok(codigos)
}

/// Una entrada: `U+XXXX` o `U+XXXX-YYYY`. Devuelve el rango (cerrado) que cubre.
fn leer_entrada(entrada: &str) -> Result<(u32, u32), String> {
    let resto = entrada
        .strip_prefix("U+")
        .or_else(|| entrada.strip_prefix("u+"))
        .ok_or("debe empezar por «U+»")?;
    let (primero, segundo) = match resto.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (resto, None),
    };
    let desde = leer_hexadecimal(primero)?;
    let hasta = match segundo {
        Some(b) => leer_hexadecimal(b)?,
        None => desde,
    };
    if desde > hasta {
        return Err(format!(
            "el rango va al revés (U+{desde:04X} es mayor que U+{hasta:04X})"
        ));
    }
    Ok((desde, hasta))
}

/// De 1 a 6 dígitos hexadecimales que son un carácter Unicode válido.
fn leer_hexadecimal(texto: &str) -> Result<u32, String> {
    if texto.is_empty() {
        return Err("falta el número hexadecimal".to_string());
    }
    if texto.len() > 6 || !texto.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("«{texto}» no es un hexadecimal de 1 a 6 dígitos"));
    }
    let valor = u32::from_str_radix(texto, 16)
        .map_err(|_| format!("«{texto}» no es un hexadecimal de 1 a 6 dígitos"))?;
    if valor > MAX_UNICODE {
        return Err(format!("U+{valor:X} pasa de U+10FFFF"));
    }
    if (0xD800..=0xDFFF).contains(&valor) {
        return Err(format!(
            "U+{valor:04X} es un sustituto de UTF-16, no un carácter"
        ));
    }
    Ok(valor)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_lector_comprueba_cada_lectura() {
        let lector = Lector::nuevo(&[0x12, 0x34, 0x56], "prueba");
        assert_eq!(lector.u16(0).unwrap(), 0x1234);
        assert_eq!(lector.u16(1).unwrap(), 0x3456);
        assert!(lector.u16(2).is_err());
        assert!(lector.u16(usize::MAX).is_err());
        assert!(lector.u32(0).is_err());
        assert!(lector.u32(usize::MAX - 1).is_err());
    }

    #[test]
    fn el_error_dice_que_y_donde() {
        let error = codigos_de_ttf(&[1, 2, 3]).unwrap_err().to_string();
        assert!(error.contains("tabla cmap de la fuente"), "{error}");
        assert!(error.contains("3 bytes"), "{error}");
    }

    #[test]
    fn leer_entrada_distingue_formas() {
        assert_eq!(leer_entrada("U+21B5"), Ok((0x21B5, 0x21B5)));
        assert_eq!(leer_entrada("u+21b5"), Ok((0x21B5, 0x21B5)));
        assert_eq!(leer_entrada("U+25D0-25D1"), Ok((0x25D0, 0x25D1)));
        assert!(leer_entrada("U+25D1-25D0").is_err());
        assert!(leer_entrada("U+").is_err());
        assert!(leer_entrada("U+25D0-").is_err());
        assert!(leer_entrada("21B5").is_err());
        assert!(leer_entrada("U+110000").is_err());
        assert!(leer_entrada("U+D800").is_err());
        assert!(leer_entrada("U+1234567").is_err());
        assert!(leer_entrada("U+12G4").is_err());
        assert!(leer_entrada("U+-1").is_err());
        assert!(leer_entrada("U++1").is_err());
    }

    #[test]
    fn un_formato_4_sin_cuerpo_es_un_error() {
        // Cabecera de 14 bytes con segCountX2 = 4 y nada más.
        let mut subtabla = vec![0u8; 14];
        subtabla[1] = 4;
        subtabla[7] = 4;
        assert!(leer_formato_4(&subtabla, "prueba", &mut Presupuesto::nuevo()).is_err());
    }

    #[test]
    fn un_formato_12_con_un_numero_de_grupos_gigante_no_reserva_memoria() {
        let mut subtabla = vec![0u8; 16];
        subtabla[1] = 12;
        subtabla[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        let error = leer_formato_12(&subtabla, "prueba", &mut Presupuesto::nuevo())
            .unwrap_err()
            .to_string();
        assert!(error.contains("tope"), "{error}");
    }
}
