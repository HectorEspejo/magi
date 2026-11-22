//! Pruebas del lector de cobertura de glifos (`fuentes`, informe §7.8).
//!
//! Las que miran fuentes reales leen los TTF de la app Android (JetBrains Mono 2.305)
//! y `web/fuentes/magi-simbolos.unicodes` desde la raíz del repositorio. Las del
//! formato 4 y el 12 sintetizan TTF mínimos a mano.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use magi_web::fuentes::{codigos_de_ttf, leer_unicodes};

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn raiz() -> PathBuf {
    // `crates/magi-web` → raíz del repositorio.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("la raíz del repositorio")
}

fn leer(ruta: &str) -> Vec<u8> {
    let completa = raiz().join(ruta);
    fs::read(&completa).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", completa.display()))
}

fn jetbrains(peso: &str) -> BTreeSet<u32> {
    let ruta = format!("android/app/src/main/res/font/jetbrains_mono_{peso}.ttf");
    codigos_de_ttf(&leer(&ruta)).unwrap_or_else(|e| panic!("{ruta}: {e}"))
}

fn de(texto: &str) -> BTreeSet<u32> {
    texto.chars().map(u32::from).collect()
}

/// Generador congruencial lineal (los de Knuth/MMIX), determinista y sin dependencias.
struct Lcg(u64);

impl Lcg {
    fn siguiente(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn hasta(&mut self, tope: usize) -> usize {
        (self.siguiente() % tope as u64) as usize
    }

    fn byte(&mut self) -> u8 {
        (self.siguiente() & 0xFF) as u8
    }
}

// ---------------------------------------------------------------------------
// TTF sintéticos
// ---------------------------------------------------------------------------

fn u16b(valor: u16) -> [u8; 2] {
    valor.to_be_bytes()
}

fn u32b(valor: u32) -> [u8; 4] {
    valor.to_be_bytes()
}

/// Un segmento de un formato 4: si `glifos` no está vacío, el segmento usa
/// `idRangeOffset` y la tabla de glifos (uno por código, `0` = sin glifo).
struct Segmento4 {
    inicio: u16,
    fin: u16,
    delta: u16,
    glifos: Vec<u16>,
}

impl Segmento4 {
    fn con_delta(inicio: u16, fin: u16, delta: u16) -> Self {
        Self {
            inicio,
            fin,
            delta,
            glifos: Vec::new(),
        }
    }
}

/// Subtabla de formato 4 con su segmento centinela `U+FFFF` al final.
fn subtabla_4(segmentos: &[Segmento4]) -> Vec<u8> {
    let mut todos: Vec<(u16, u16, u16, &[u16])> = segmentos
        .iter()
        .map(|s| (s.inicio, s.fin, s.delta, s.glifos.as_slice()))
        .collect();
    todos.push((0xFFFF, 0xFFFF, 1, &[]));
    let cuantos = todos.len();

    let mut fines = Vec::new();
    let mut inicios = Vec::new();
    let mut deltas = Vec::new();
    let mut rangos = Vec::new();
    let mut matriz: Vec<u16> = Vec::new();
    for (i, (inicio, fin, delta, glifos)) in todos.iter().enumerate() {
        fines.extend(u16b(*fin));
        inicios.extend(u16b(*inicio));
        deltas.extend(u16b(*delta));
        if glifos.is_empty() {
            rangos.extend(u16b(0));
        } else {
            // Desde idRangeOffset[i] hasta glyphIdArray[k]: lo que queda de idRangeOffset más k.
            let desplazamiento = (cuantos - i) * 2 + matriz.len() * 2;
            rangos.extend(u16b(desplazamiento as u16));
            matriz.extend_from_slice(glifos);
        }
    }

    let mut cuerpo = Vec::new();
    cuerpo.extend(fines);
    cuerpo.extend(u16b(0)); // reservedPad
    cuerpo.extend(inicios);
    cuerpo.extend(deltas);
    cuerpo.extend(rangos);
    for g in matriz {
        cuerpo.extend(u16b(g));
    }

    let mut sub = Vec::new();
    sub.extend(u16b(4)); // formato
    sub.extend(u16b((14 + cuerpo.len()) as u16)); // length
    sub.extend(u16b(0)); // language
    sub.extend(u16b((cuantos * 2) as u16)); // segCountX2
    sub.extend(u16b(0)); // searchRange (no se usa)
    sub.extend(u16b(0)); // entrySelector
    sub.extend(u16b(0)); // rangeShift
    sub.extend(cuerpo);
    sub
}

/// Subtabla de formato 12 con grupos `(inicio, fin, glifo_inicial)`.
fn subtabla_12(grupos: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut sub = Vec::new();
    sub.extend(u16b(12)); // formato
    sub.extend(u16b(0)); // reservado
    sub.extend(u32b((16 + 12 * grupos.len()) as u32)); // length
    sub.extend(u32b(0)); // language
    sub.extend(u32b(grupos.len() as u32)); // numGroups
    for (inicio, fin, glifo) in grupos {
        sub.extend(u32b(*inicio));
        sub.extend(u32b(*fin));
        sub.extend(u32b(*glifo));
    }
    sub
}

/// Subtabla de formato 0 (Mac, 256 glifos): la lectura debe ignorarla.
fn subtabla_0() -> Vec<u8> {
    let mut sub = Vec::new();
    sub.extend(u16b(0));
    sub.extend(u16b(262));
    sub.extend(u16b(0));
    sub.extend((0..=255u8).map(|b| if b == b'A' { 1 } else { 0 }));
    sub
}

/// Tabla `cmap` con los registros `(plataforma, codificación, subtabla)`; las
/// subtablas con el mismo contenido y registros distintos pueden compartir
/// desplazamiento con `compartir_con`.
fn tabla_cmap(registros: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut cabecera = Vec::new();
    cabecera.extend(u16b(0));
    cabecera.extend(u16b(registros.len() as u16));
    let mut desplazamiento = 4 + 8 * registros.len();
    let mut cuerpo = Vec::new();
    for (plataforma, codificacion, sub) in registros {
        cabecera.extend(u16b(*plataforma));
        cabecera.extend(u16b(*codificacion));
        cabecera.extend(u32b(desplazamiento as u32));
        desplazamiento += sub.len();
        cuerpo.extend_from_slice(sub);
    }
    cabecera.extend(cuerpo);
    cabecera
}

/// Un TTF mínimo: la cabecera `sfnt`, un directorio y las tablas en orden, cada una
/// alineada a 4 bytes salvo la última (el fichero acaba justo donde acaba su
/// contenido, así que todo prefijo propio queda truncado).
fn ttf(tablas: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut salida = Vec::new();
    salida.extend([0x00, 0x01, 0x00, 0x00]);
    salida.extend(u16b(tablas.len() as u16));
    salida.extend(u16b(0));
    salida.extend(u16b(0));
    salida.extend(u16b(0));
    let mut desplazamiento = 12 + 16 * tablas.len();
    let mut cuerpo = Vec::new();
    for (i, (etiqueta, datos)) in tablas.iter().enumerate() {
        salida.extend(*etiqueta);
        salida.extend(u32b(0)); // suma de comprobación: no se comprueba
        salida.extend(u32b(desplazamiento as u32));
        salida.extend(u32b(datos.len() as u32));
        cuerpo.extend_from_slice(datos);
        desplazamiento += datos.len();
        if i + 1 < tablas.len() {
            while desplazamiento % 4 != 0 {
                cuerpo.push(0);
                desplazamiento += 1;
            }
        }
    }
    salida.extend(cuerpo);
    salida
}

/// Un TTF con una tabla de relleno antes y la `cmap` al final.
fn ttf_con_cmap(registros: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    ttf(&[(b"head", vec![0xAB; 54]), (b"cmap", tabla_cmap(registros))])
}

// ---------------------------------------------------------------------------
// Fuentes reales
// ---------------------------------------------------------------------------

#[test]
fn jetbrains_mono_no_tiene_los_diez_glifos_de_la_tui() {
    let faltan = "↵↻⇄⇅▣◐◑★⚑⤴";
    let estan = "▸▾●○◆✓✗⚠─│┌┐└┘█";
    assert_eq!(faltan.chars().count(), 10);
    assert_eq!(estan.chars().count(), 15);

    let regular = jetbrains("regular");
    let negrita = jetbrains("bold");
    for (peso, codigos) in [("regular", &regular), ("bold", &negrita)] {
        for c in faltan.chars() {
            assert!(
                !codigos.contains(&u32::from(c)),
                "JetBrains Mono {peso} no debería tener {c} (U+{:04X})",
                u32::from(c)
            );
        }
        for c in estan.chars() {
            assert!(
                codigos.contains(&u32::from(c)),
                "JetBrains Mono {peso} debería tener {c} (U+{:04X})",
                u32::from(c)
            );
        }
        // Lo básico también está: la ñ, las tildes y el ASCII imprimible.
        for c in "ñáéíóúÑ¿¡ÁÉÍÓÚü€·×…".chars().chain(' '..='~') {
            assert!(codigos.contains(&u32::from(c)), "{peso}: falta {c:?}");
        }
        // Ni el nulo ni el centinela de la tabla (U+FFFF) cuentan como glifos.
        assert!(!codigos.contains(&0xFFFF), "{peso}: U+FFFF es el centinela");
    }
    // Las dos pesos cubren lo mismo, y el total coincide con lo que da fontTools
    // (`getBestCmap`) sobre estos mismos ficheros.
    assert_eq!(regular, negrita);
    assert_eq!(regular.len(), 1372, "puntos de código con glifo");
}

#[test]
fn el_subconjunto_declara_nueve_de_los_diez() {
    let texto = String::from_utf8(leer("web/fuentes/magi-simbolos.unicodes"))
        .expect("magi-simbolos.unicodes es UTF-8");
    let declarados = leer_unicodes(&texto).expect("magi-simbolos.unicodes se entiende");

    let nueve = de("↵↻⇄⇅▣◐◑★⚑");
    assert_eq!(nueve.len(), 9);
    assert_eq!(
        declarados, nueve,
        "el subconjunto declara exactamente U+21B5, U+21BB, U+21C4, U+21C5, U+25A3, U+25D0, U+25D1, U+2605 y U+2691"
    );
    assert!(
        !declarados.contains(&0x2934),
        "U+2934 (⤴) no está en DejaVu Sans Mono: se degrada con texto_ascii"
    );

    // Cierra el círculo: lo que JetBrains Mono no trae de los diez es justo el
    // subconjunto más lo que se degrada.
    let diez = de("↵↻⇄⇅▣◐◑★⚑⤴");
    let jetbrains = jetbrains("regular");
    let sin_glifo: BTreeSet<u32> = diez
        .iter()
        .copied()
        .filter(|c| !jetbrains.contains(c))
        .collect();
    assert_eq!(sin_glifo, diez, "los diez faltan en JetBrains Mono");
    let cubiertos: BTreeSet<u32> = sin_glifo
        .iter()
        .copied()
        .filter(|c| declarados.contains(c))
        .collect();
    assert_eq!(cubiertos, nueve);
    assert_eq!(
        sin_glifo
            .difference(&cubiertos)
            .copied()
            .collect::<Vec<_>>(),
        vec![0x2934]
    );
}

#[test]
fn los_ficheros_de_fuentes_son_woff2_y_llevan_su_licencia() {
    for nombre in [
        "JetBrainsMono-Regular.woff2",
        "JetBrainsMono-Bold.woff2",
        "magi-simbolos.woff2",
    ] {
        let bytes = leer(&format!("web/fuentes/{nombre}"));
        assert_eq!(&bytes[..4], b"wOF2", "{nombre} no es un woff2");
        // El woff2 declara su tamaño total en la cabecera (bytes 8..12).
        let declarado = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        assert_eq!(declarado, bytes.len(), "{nombre}: tamaño declarado");
    }
    let simbolos = leer("web/fuentes/magi-simbolos.woff2").len();
    assert!(
        simbolos < 8 * 1024,
        "magi-simbolos.woff2 debe pesar unos pocos KB y pesa {simbolos} bytes"
    );
    // Un woff2 no es un TTF: el lector lo dice, no entra en pánico.
    let error = codigos_de_ttf(&leer("web/fuentes/magi-simbolos.woff2"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("woff"), "{error}");

    let ofl = String::from_utf8(leer("web/fuentes/OFL.txt")).expect("OFL.txt es UTF-8");
    assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert!(ofl.contains("JetBrains Mono Project Authors"));
    let dejavu = String::from_utf8(leer("web/fuentes/LICENSE-DejaVu.txt"))
        .expect("LICENSE-DejaVu.txt es UTF-8");
    assert!(dejavu.contains("Bitstream Vera Fonts Copyright"));
    assert!(dejavu.contains("DejaVu changes are in public domain"));
    assert!(dejavu.contains("Arev Fonts Copyright"));
}

// ---------------------------------------------------------------------------
// Formato 4 y formato 12 sintéticos
// ---------------------------------------------------------------------------

#[test]
fn el_formato_4_lee_deltas_tablas_de_glifos_y_el_centinela() {
    let sub = subtabla_4(&[
        // 'A'..='C' con delta que da glifos 0, 1 y 2: la 'A' (glifo 0) no cuenta.
        Segmento4::con_delta(0x41, 0x43, 0u16.wrapping_sub(0x41)),
        // Un delta que da la vuelta: U+2605 + 0xDA00 = 0x10005, que módulo 65 536 es el glifo 5.
        Segmento4::con_delta(0x2605, 0x2606, 0xDA00),
        // Con tabla de glifos: el segundo código no tiene glifo (0), el tercero sí.
        Segmento4 {
            inicio: 0x3000,
            fin: 0x3002,
            delta: 0,
            glifos: vec![7, 0, 9],
        },
        // Otro segmento con tabla y delta: 0 sigue siendo «sin glifo» y lo demás se desplaza.
        Segmento4 {
            inicio: 0x4000,
            fin: 0x4001,
            delta: 100,
            glifos: vec![0, 5],
        },
    ]);
    let codigos = codigos_de_ttf(&ttf_con_cmap(&[(3, 1, sub)])).expect("formato 4 válido");
    let esperados: BTreeSet<u32> = [0x42, 0x43, 0x2605, 0x2606, 0x3000, 0x3002, 0x4001]
        .into_iter()
        .collect();
    assert_eq!(codigos, esperados);
    assert!(!codigos.contains(&0x41), "glifo 0 = sin glifo");
    assert!(!codigos.contains(&0xFFFF), "U+FFFF es el centinela");
}

#[test]
fn el_formato_4_con_el_centinela_sin_delta_tampoco_cuenta_u_ffff() {
    // Hay fuentes cuyo último segmento deja `idDelta = 0`: sin la regla, U+FFFF
    // saldría con el glifo 0xFFFF.
    let mut sub = subtabla_4(&[Segmento4::con_delta(0x41, 0x41, 0)]);
    // idDelta del centinela: tras la cabecera (14), endCode (2×2), relleno (2),
    // startCode (2×2) y el idDelta del primero (2).
    let pos = 14 + 4 + 2 + 4 + 2;
    sub[pos..pos + 2].copy_from_slice(&u16b(0));
    let codigos = codigos_de_ttf(&ttf_con_cmap(&[(0, 3, sub)])).expect("formato 4 válido");
    assert_eq!(codigos, BTreeSet::from([0x41]));
}

#[test]
fn el_formato_12_lee_grupos_y_descarta_el_glifo_cero() {
    let sub = subtabla_12(&[
        (0x41, 0x43, 0), // el primero (glifo 0) no cuenta
        (0x2605, 0x2605, 17),
        (0x1F600, 0x1F602, 100),
    ]);
    let codigos = codigos_de_ttf(&ttf_con_cmap(&[(3, 10, sub)])).expect("formato 12 válido");
    assert_eq!(
        codigos,
        BTreeSet::from([0x42, 0x43, 0x2605, 0x1F600, 0x1F601, 0x1F602])
    );
}

#[test]
fn se_unen_todas_las_subtablas_unicode_y_se_ignoran_las_demas() {
    let bmp = subtabla_4(&[Segmento4::con_delta(0x41, 0x42, 0)]);
    let completa = subtabla_12(&[(0x1F600, 0x1F600, 5)]);
    let otra_plataforma_0 = subtabla_12(&[(0x2605, 0x2605, 3)]);
    let registros = vec![
        (3, 1, bmp),
        (3, 10, completa),
        (0, 4, otra_plataforma_0),
        // Mac Roman (formato 0) y símbolos (3, 0): no son Unicode, no cuentan.
        (1, 0, subtabla_0()),
        (3, 0, subtabla_4(&[Segmento4::con_delta(0x50, 0x50, 0)])),
        // Formato 14 (variaciones) de plataforma 0: es Unicode pero no se lee.
        (0, 5, {
            let mut sub = Vec::new();
            sub.extend(u16b(14));
            sub.extend(u32b(10));
            sub.extend(u32b(0));
            sub
        }),
    ];
    let codigos = codigos_de_ttf(&ttf_con_cmap(&registros)).expect("unión de subtablas");
    assert_eq!(
        codigos,
        BTreeSet::from([0x41, 0x42, 0x2605, 0x1F600]),
        "A, B (4), ★ (12 de la plataforma 0) y 😀 (12): ni 'A' de Mac ni 'P' de símbolos"
    );
}

#[test]
fn dos_registros_pueden_apuntar_a_la_misma_subtabla() {
    // Lo normal en una fuente real: (0, 3) y (3, 1) comparten la subtabla de formato 4.
    let sub = subtabla_4(&[Segmento4::con_delta(0x41, 0x42, 0)]);
    let mut cmap = Vec::new();
    cmap.extend(u16b(0));
    cmap.extend(u16b(2));
    for (plataforma, codificacion) in [(0u16, 3u16), (3, 1)] {
        cmap.extend(u16b(plataforma));
        cmap.extend(u16b(codificacion));
        cmap.extend(u32b(20));
    }
    cmap.extend(sub);
    let fuente = ttf(&[(b"cmap", cmap)]);
    assert_eq!(
        codigos_de_ttf(&fuente).unwrap(),
        BTreeSet::from([0x41, 0x42])
    );
}

#[test]
fn sin_subtabla_unicode_legible_es_un_error() {
    // Solo Mac Roman.
    let solo_mac = ttf_con_cmap(&[(1, 0, subtabla_0())]);
    let error = codigos_de_ttf(&solo_mac).unwrap_err().to_string();
    assert!(error.contains("ninguna subtabla Unicode"), "{error}");
    // Solo formato 14.
    let mut sub14 = Vec::new();
    sub14.extend(u16b(14));
    sub14.extend(u32b(10));
    sub14.extend(u32b(0));
    let solo_14 = ttf_con_cmap(&[(0, 5, sub14)]);
    assert!(codigos_de_ttf(&solo_14).is_err());
    // Sin registros.
    assert!(codigos_de_ttf(&ttf_con_cmap(&[])).is_err());
}

#[test]
fn un_fichero_sin_cmap_o_con_otra_cabecera_dice_que_no_es_valido() {
    let sin_cmap = ttf(&[(b"head", vec![0; 54])]);
    let error = codigos_de_ttf(&sin_cmap).unwrap_err().to_string();
    assert!(error.contains("no tiene tabla cmap"), "{error}");

    for (cabecera, trozo) in [
        (&b"wOF2"[..], "woff"),
        (&b"wOFF"[..], "woff"),
        (&b"ttcf"[..], "TTC"),
        (&b"%PDF"[..], "no es un TTF"),
    ] {
        let mut bytes = cabecera.to_vec();
        bytes.extend([0u8; 64]);
        let error = codigos_de_ttf(&bytes).unwrap_err().to_string();
        assert!(error.contains(trozo), "{cabecera:?}: {error}");
    }
}

#[test]
fn los_segmentos_y_los_grupos_desordenados_o_solapados_son_un_error() {
    // Un segmento que se solapa con el anterior (podría hacer el trabajo cuadrático).
    let solapados = subtabla_4(&[
        Segmento4::con_delta(0x41, 0x50, 0),
        Segmento4::con_delta(0x48, 0x60, 0),
    ]);
    assert!(codigos_de_ttf(&ttf_con_cmap(&[(3, 1, solapados)])).is_err());
    // Desordenados.
    let desordenados = subtabla_4(&[
        Segmento4::con_delta(0x60, 0x61, 0),
        Segmento4::con_delta(0x41, 0x42, 0),
    ]);
    assert!(codigos_de_ttf(&ttf_con_cmap(&[(3, 1, desordenados)])).is_err());
    // Inicio mayor que el fin.
    let al_reves = subtabla_4(&[Segmento4::con_delta(0x50, 0x41, 0)]);
    assert!(codigos_de_ttf(&ttf_con_cmap(&[(3, 1, al_reves)])).is_err());
    // Con una tabla de glifos que se sale de la subtabla.
    let mut fuera = subtabla_4(&[Segmento4 {
        inicio: 0x41,
        fin: 0x43,
        delta: 0,
        glifos: vec![1, 2, 3],
    }]);
    fuera.truncate(fuera.len() - 2); // le falta el último glifo
    assert!(codigos_de_ttf(&ttf_con_cmap(&[(3, 1, fuera)])).is_err());

    // Grupos del formato 12.
    for grupos in [
        vec![(0x50u32, 0x60u32, 1u32), (0x58, 0x70, 20)], // solapados
        vec![(0x60, 0x70, 1), (0x41, 0x42, 20)],          // desordenados
        vec![(0x70, 0x60, 1)],                            // al revés
        vec![(0x41, 0x110000, 1)],                        // pasa de Unicode
        vec![(0x41, u32::MAX, 1)],                        // gigante
    ] {
        let sub = subtabla_12(&grupos);
        let error = codigos_de_ttf(&ttf_con_cmap(&[(3, 10, sub)]))
            .expect_err(&format!("{grupos:X?} debería fallar"))
            .to_string();
        assert!(error.contains("formato 12"), "{error}");
    }
}

#[test]
fn una_fuente_con_el_bmp_y_todo_unicode_se_lee_entera() {
    // Lo más grande que cabe en una fuente real: el BMP por formato 4 y todo
    // Unicode por formato 12 (sin los sustitutos). Pasa con holgura el tope de
    // trabajo de la lectura.
    let bmp = subtabla_4(&[Segmento4::con_delta(0x20, 0xD7FF, 0)]);
    let completa = subtabla_12(&[(0x20, 0xD7FF, 1), (0xE000, 0x10FFFF, 0xD800)]);
    let codigos = codigos_de_ttf(&ttf_con_cmap(&[(3, 1, bmp), (3, 10, completa)]))
        .expect("el BMP y todo Unicode caben");
    let esperados = (0x20..=0xD7FFu32).chain(0xE000..=0x10FFFF).count();
    assert_eq!(codigos.len(), esperados);
    assert!(codigos.contains(&0x20) && codigos.contains(&0x10FFFF));
    assert!(!codigos.contains(&0xD800) && !codigos.contains(&0x1F));
}

#[test]
fn un_cmap_con_muchas_subtablas_enormes_no_cuelga_la_lectura() {
    // La bomba: 256 registros (el tope de la lectura) con 256 subtablas distintas
    // de formato 12, cada una de 28 bytes y con todo Unicode en un solo grupo. Cada
    // subtabla está bien formada y su trabajo está acotado a 1,1 M de códigos, pero
    // las 256 juntas son 285 M: sin un tope de trabajo total, una fuente de 9 KB
    // tardaba casi un minuto en leerse (y minutos en depuración).
    let todo_unicode = subtabla_12(&[(0, 0x10FFFF, 1)]);
    assert_eq!(todo_unicode.len(), 28);
    let registros: Vec<(u16, u16, Vec<u8>)> =
        (0..256).map(|_| (0, 4, todo_unicode.clone())).collect();
    let fuente = ttf_con_cmap(&registros);
    assert!(
        fuente.len() < 10_000,
        "la fuente hostil pesa {} bytes",
        fuente.len()
    );

    let error = codigos_de_ttf(&fuente)
        .expect_err("debería pasar el tope de trabajo")
        .to_string();
    assert!(error.contains("puntos de código"), "{error}");
    assert!(error.contains("2500000"), "{error}");
}

// ---------------------------------------------------------------------------
// Entrada hostil
// ---------------------------------------------------------------------------

fn fuente_sintetica_completa() -> Vec<u8> {
    ttf_con_cmap(&[
        (
            3,
            1,
            subtabla_4(&[
                Segmento4::con_delta(0x41, 0x5A, 0),
                Segmento4 {
                    inicio: 0x3000,
                    fin: 0x3002,
                    delta: 0,
                    glifos: vec![7, 0, 9],
                },
            ]),
        ),
        (3, 10, subtabla_12(&[(0x1F600, 0x1F602, 100)])),
    ])
}

/// Posición de `cmap` dentro del TTF sintético (su directorio la dice).
fn inicio_de_cmap(fuente: &[u8]) -> usize {
    // La segunda tabla (`cmap`) del directorio: cabecera de 12 + un registro de 16 + 8.
    let pos = 12 + 16 + 8;
    u32::from_be_bytes([
        fuente[pos],
        fuente[pos + 1],
        fuente[pos + 2],
        fuente[pos + 3],
    ]) as usize
}

#[test]
fn el_lector_de_cmap_no_hace_panico_con_basura() {
    // 0. La fuente sintética sin tocar se lee bien: la base de todo lo demás.
    let buena = fuente_sintetica_completa();
    let leidos = codigos_de_ttf(&buena).expect("la fuente sintética es válida");
    assert!(leidos.contains(&0x41) && leidos.contains(&0x1F601) && leidos.contains(&0x3002));
    assert!(!leidos.contains(&0x3001), "sin glifo en la tabla");

    // 1. Vacío, cortísimo y con cabeceras que no son.
    assert!(codigos_de_ttf(&[]).is_err());
    for largo in 0..12 {
        assert!(codigos_de_ttf(&buena[..largo]).is_err(), "{largo} bytes");
    }
    assert!(codigos_de_ttf(&[0u8; 4096]).is_err());
    assert!(codigos_de_ttf(&[0xFFu8; 4096]).is_err());

    // 2. Truncados: todo prefijo propio de la fuente sintética es un error con
    // mensaje, y ninguno entra en pánico.
    for largo in 0..buena.len() {
        let error = codigos_de_ttf(&buena[..largo])
            .expect_err(&format!("el prefijo de {largo} bytes está truncado"))
            .to_string();
        assert!(!error.is_empty());
    }

    // 3. Truncados de una fuente real: nunca pánico y, mientras falte contenido
    // de verdad (más que el relleno final), siempre error.
    let real = leer("android/app/src/main/res/font/jetbrains_mono_regular.ttf");
    assert!(codigos_de_ttf(&real).is_ok());
    for largo in (0..real.len() - 16).step_by(1009) {
        assert!(
            codigos_de_ttf(&real[..largo]).is_err(),
            "JetBrains Mono truncada a {largo} de {} bytes",
            real.len()
        );
    }

    // 4. Desplazamientos y longitudes enormes en el directorio de tablas.
    let mut enorme = buena.clone();
    let pos_cmap = 12 + 16; // registro de `cmap`
    enorme[pos_cmap + 8..pos_cmap + 12].copy_from_slice(&u32b(u32::MAX));
    assert!(codigos_de_ttf(&enorme).is_err(), "desplazamiento u32::MAX");
    let mut enorme = buena.clone();
    enorme[pos_cmap + 12..pos_cmap + 16].copy_from_slice(&u32b(u32::MAX));
    assert!(codigos_de_ttf(&enorme).is_err(), "longitud u32::MAX");
    let mut enorme = buena.clone();
    enorme[pos_cmap + 8..pos_cmap + 12].copy_from_slice(&u32b(u32::MAX - 3));
    enorme[pos_cmap + 12..pos_cmap + 16].copy_from_slice(&u32b(8));
    assert!(
        codigos_de_ttf(&enorme).is_err(),
        "desplazamiento + longitud desborda 32 bits"
    );
    // Un directorio con más tablas de las que caben.
    let mut muchas = buena.clone();
    muchas[4..6].copy_from_slice(&u16b(u16::MAX));
    assert!(codigos_de_ttf(&muchas).is_err(), "65 535 tablas");
    muchas[4..6].copy_from_slice(&u16b(0));
    assert!(codigos_de_ttf(&muchas).is_err(), "0 tablas");

    // 5. Desplazamientos enormes dentro de cmap y números de segmentos o grupos absurdos.
    let cmap = inicio_de_cmap(&buena);
    // 5a. Desplazamiento de la subtabla del primer registro.
    let mut roto = buena.clone();
    roto[cmap + 8..cmap + 12].copy_from_slice(&u32b(u32::MAX));
    assert!(codigos_de_ttf(&roto).is_err(), "subtabla en u32::MAX");
    // 5b. Número de registros de cmap enorme.
    let mut roto = buena.clone();
    roto[cmap + 2..cmap + 4].copy_from_slice(&u16b(u16::MAX));
    assert!(codigos_de_ttf(&roto).is_err(), "65 535 registros");
    // 5c. Número de segmentos absurdo (segCountX2 = 65 534) en el formato 4.
    let sub4 = cmap
        + usize::try_from(u32::from_be_bytes([
            buena[cmap + 8],
            buena[cmap + 9],
            buena[cmap + 10],
            buena[cmap + 11],
        ]))
        .unwrap();
    let mut roto = buena.clone();
    roto[sub4 + 6..sub4 + 8].copy_from_slice(&u16b(0xFFFE));
    assert!(codigos_de_ttf(&roto).is_err(), "65 534 segmentos");
    roto[sub4 + 6..sub4 + 8].copy_from_slice(&u16b(0));
    assert!(codigos_de_ttf(&roto).is_err(), "0 segmentos");
    roto[sub4 + 6..sub4 + 8].copy_from_slice(&u16b(5));
    assert!(codigos_de_ttf(&roto).is_err(), "segCountX2 impar");
    // 5d. Número de grupos absurdo en el formato 12.
    let sub12 = cmap
        + usize::try_from(u32::from_be_bytes([
            buena[cmap + 16],
            buena[cmap + 17],
            buena[cmap + 18],
            buena[cmap + 19],
        ]))
        .unwrap();
    for grupos in [u32::MAX, 0x7FFF_FFFF, 1_000_000, 4] {
        let mut roto = buena.clone();
        roto[sub12 + 12..sub12 + 16].copy_from_slice(&u32b(grupos));
        assert!(codigos_de_ttf(&roto).is_err(), "{grupos} grupos");
    }
    // 5e. idRangeOffset enorme: apunta fuera de la tabla.
    let mut roto = buena.clone();
    let seg_x2 = 3 * 2; // dos segmentos y el centinela
    let ini_rango = sub4 + 16 + 3 * seg_x2;
    // Segundo segmento (el de la tabla de glifos): su idRangeOffset.
    roto[ini_rango + 2..ini_rango + 4].copy_from_slice(&u16b(0xFFFE));
    assert!(
        codigos_de_ttf(&roto).is_err(),
        "idRangeOffset fuera de la tabla"
    );
    roto[ini_rango + 2..ini_rango + 4].copy_from_slice(&u16b(0x0003));
    assert!(codigos_de_ttf(&roto).is_err(), "idRangeOffset impar");

    // 6. Bytes aleatorios deterministas (LCG propio): ningún pánico.
    let mut lcg = Lcg(0x9E37_79B9_7F4A_7C15);
    for _ in 0..2000 {
        let largo = lcg.hasta(1500);
        let mut bytes: Vec<u8> = (0..largo).map(|_| lcg.byte()).collect();
        // Que algunos casos pasen de la cabecera: sfnt y un directorio verosímil.
        if largo >= 12 && lcg.hasta(2) == 0 {
            bytes[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
            bytes[4..6].copy_from_slice(&u16b((lcg.hasta(6) + 1) as u16));
        }
        let _ = codigos_de_ttf(&bytes);
    }

    // 7. Mutaciones de la fuente sintética buena (un byte cada vez, y algún tramo).
    let mut lcg = Lcg(42);
    for _ in 0..1500 {
        let mut mutada = buena.clone();
        for _ in 0..=lcg.hasta(4) {
            let pos = lcg.hasta(mutada.len());
            mutada[pos] = lcg.byte();
        }
        let _ = codigos_de_ttf(&mutada);
    }
    // 8. Lo mismo sobre la cabecera, el directorio y la `cmap` de una fuente real
    // (sin tocar `glyf`, que no se lee), con y sin truncar.
    let inicio_cmap_real = {
        let tablas = usize::from(u16::from_be_bytes([real[4], real[5]]));
        (0..tablas)
            .map(|i| 12 + 16 * i)
            .find(|b| &real[*b..*b + 4] == b"cmap")
            .map(|b| {
                u32::from_be_bytes([real[b + 8], real[b + 9], real[b + 10], real[b + 11]]) as usize
            })
            .expect("JetBrains Mono tiene cmap")
    };
    let mut lcg = Lcg(7);
    for _ in 0..300 {
        let mut mutada = real.clone();
        for _ in 0..=lcg.hasta(3) {
            let pos = if lcg.hasta(2) == 0 {
                lcg.hasta(200)
            } else {
                inicio_cmap_real + lcg.hasta(400)
            };
            mutada[pos] = lcg.byte();
        }
        if lcg.hasta(4) == 0 {
            let largo = lcg.hasta(mutada.len());
            mutada.truncate(largo);
        }
        let _ = codigos_de_ttf(&mutada);
    }
}

// ---------------------------------------------------------------------------
// leer_unicodes
// ---------------------------------------------------------------------------

#[test]
fn leer_unicodes_entiende_rangos_y_comentarios() {
    // Entradas sueltas, varias por línea y separadas por espacios o comas.
    assert_eq!(
        leer_unicodes("U+21B5\nU+21BB, U+21C4 U+21C5").unwrap(),
        BTreeSet::from([0x21B5, 0x21BB, 0x21C4, 0x21C5])
    );
    // Rangos cerrados.
    assert_eq!(
        leer_unicodes("U+25D0-25D1").unwrap(),
        BTreeSet::from([0x25D0, 0x25D1])
    );
    assert_eq!(
        leer_unicodes("U+41-45, U+61").unwrap(),
        BTreeSet::from([0x41, 0x42, 0x43, 0x44, 0x45, 0x61])
    );
    // Un rango de un solo punto y un carácter fuera del BMP.
    assert_eq!(
        leer_unicodes("U+2605-2605 U+1F600").unwrap(),
        BTreeSet::from([0x2605, 0x1F600])
    );
    // Comentarios (de línea entera y al final), líneas en blanco, sangrías y CRLF.
    let texto = "\
# Cabecera con U+9999 que no cuenta
\r
   U+21B5   # ↵ intro, U+9998 tampoco cuenta
\t
U+2605\r\n# U+9997
    U+2691 , # nada después
";
    assert_eq!(
        leer_unicodes(texto).unwrap(),
        BTreeSet::from([0x21B5, 0x2605, 0x2691])
    );
    // Mayúsculas y minúsculas, y duplicados.
    assert_eq!(
        leer_unicodes("u+21b5 U+21B5 U+21b5").unwrap(),
        BTreeSet::from([0x21B5])
    );
    // BOM al principio.
    assert_eq!(
        leer_unicodes("\u{feff}U+41").unwrap(),
        BTreeSet::from([0x41])
    );
    // Vacío, solo comentarios.
    assert!(leer_unicodes("").unwrap().is_empty());
    assert!(leer_unicodes("# nada\n\n   \n").unwrap().is_empty());

    // Lo que no se entiende es un error que dice la línea y la entrada.
    for (texto, linea, entrada) in [
        ("U+21B5\nU+21BB\n21B5\n", 3, "21B5"),
        ("# c\nU+XYZ\n", 2, "U+XYZ"),
        ("U+\n", 1, "U+"),
        ("\n\nU+25D1-25D0\n", 3, "U+25D1-25D0"),
        ("U+25D0-\n", 1, "U+25D0-"),
        ("U+110000\n", 1, "U+110000"),
        ("U+D800\n", 1, "U+D800"),
        ("U+0000000041\n", 1, "U+0000000041"),
        ("U+41 hola\n", 1, "hola"),
        ("U+41;U+42\n", 1, "U+41;U+42"),
        ("U+41-U+42\n", 1, "U+41-U+42"),
        ("U+41 # bien\nU+-1\n", 2, "U+-1"),
    ] {
        let error = leer_unicodes(texto)
            .expect_err(&format!("{texto:?} debería fallar"))
            .to_string();
        assert!(
            error.contains(&format!("línea {linea}")),
            "{texto:?}: falta «línea {linea}» en {error}"
        );
        assert!(
            error.contains(&format!("«{entrada}»")),
            "{texto:?}: falta «{entrada}» en {error}"
        );
    }
}
