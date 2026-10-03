//! Permisos (Fase 8, §7.4), sin red ni interfaz: casillas rwx ⇄ octal (3 o 4
//! dígitos), estado mixto entre varios elementos, la máscara que se aplica
//! (`(viejo & !mascara) | (modo & mascara)`), el alcance recursivo y el aviso
//! de directorios que se quedan sin `x`.
//!
//! También el recorrido local (recuento y `chmod`), que nunca sigue enlaces:
//! `set_permissions` los seguiría y cambiaría lo que apuntan.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::archivos::TipoEntrada;
use crate::protocolo::AlcancePermisos;

/// Los nueve bits rwx: lo único que tocan las casillas.
pub const BITS_RWX: u32 = 0o777;
/// setuid, setgid y sticky: solo se aplican si se escriben en el octal.
pub const BITS_ESPECIALES: u32 = 0o7000;
pub const BITS_TODOS: u32 = BITS_RWX | BITS_ESPECIALES;

/// Estado de una casilla sobre los elementos elegidos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casilla {
    Si,
    No,
    /// Unos elementos tienen el bit y otros no: se conserva el de cada uno.
    Mixto,
}

impl Casilla {
    /// `[x]`, `[ ]` o `[~]` (los mismos en ASCII).
    pub fn texto(self) -> &'static str {
        match self {
            Casilla::Si => "[x]",
            Casilla::No => "[ ]",
            Casilla::Mixto => "[~]",
        }
    }
}

/// Bit de la casilla de `quien` (0 usuario, 1 grupo, 2 otros) y `que` (0
/// leer, 1 escribir, 2 ejecutar).
pub fn bit(quien: usize, que: usize) -> u32 {
    0o400 >> (quien * 3 + que)
}

/// La rejilla 3×3: filas usuario, grupo y otros; columnas leer, escribir y
/// ejecutar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Casillas([[Casilla; 3]; 3]);

impl Casillas {
    /// Las casillas de un solo modo (ninguna mixta).
    pub fn desde_modo(modo: u32) -> Self {
        Self::desde_modos(&[modo], false)
    }

    /// Las casillas de varios modos: mixta la que no coincide. Con algún modo
    /// `desconocido` (el listado no lo dio) no se puede afirmar nada: todas
    /// mixtas.
    fn desde_modos(modos: &[u32], desconocido: bool) -> Self {
        let mut rejilla = [[Casilla::No; 3]; 3];
        for (quien, fila) in rejilla.iter_mut().enumerate() {
            for (que, casilla) in fila.iter_mut().enumerate() {
                let con_bit = modos
                    .iter()
                    .filter(|modo| *modo & bit(quien, que) != 0)
                    .count();
                *casilla = if desconocido || (con_bit > 0 && con_bit < modos.len()) {
                    Casilla::Mixto
                } else if con_bit > 0 {
                    Casilla::Si
                } else {
                    Casilla::No
                };
            }
        }
        Self(rejilla)
    }

    pub fn casilla(&self, quien: usize, que: usize) -> Casilla {
        self.0[quien][que]
    }

    /// `Espacio`: una mixta pasa a sí; después alterna sí ⇄ no.
    pub fn alternar(&mut self, quien: usize, que: usize) {
        let casilla = &mut self.0[quien][que];
        *casilla = match *casilla {
            Casilla::Mixto | Casilla::No => Casilla::Si,
            Casilla::Si => Casilla::No,
        };
    }

    /// Bits de las casillas marcadas.
    pub fn modo(&self) -> u32 {
        self.bits(Casilla::Si)
    }

    /// Bits de las casillas mixtas: quedan fuera de la máscara.
    pub fn mixtos(&self) -> u32 {
        self.bits(Casilla::Mixto)
    }

    fn bits(&self, estado: Casilla) -> u32 {
        (0..3)
            .flat_map(|quien| (0..3).map(move |que| (quien, que)))
            .filter(|(quien, que)| self.casilla(*quien, *que) == estado)
            .fold(0, |bits, (quien, que)| bits | bit(quien, que))
    }

    /// Como `ls` sin el tipo, con `~` en las mixtas: `rw-r~-r~-`.
    pub fn simbolico(&self) -> String {
        let mut texto = String::with_capacity(9);
        for quien in 0..3 {
            for (que, letra) in ['r', 'w', 'x'].into_iter().enumerate() {
                texto.push(match self.casilla(quien, que) {
                    Casilla::Si => letra,
                    Casilla::No => '-',
                    Casilla::Mixto => '~',
                });
            }
        }
        texto
    }
}

/// Valor de un octal de 3 o 4 dígitos del 0 al 7.
pub fn octal_valido(texto: &str) -> Option<u32> {
    let valido =
        matches!(texto.len(), 3 | 4) && texto.chars().all(|digito| ('0'..='7').contains(&digito));
    valido.then(|| u32::from_str_radix(texto, 8).ok()).flatten()
}

/// Lo que el diálogo PERMISOS va a aplicar: casillas y octal sincronizados.
///
/// Las casillas solo tocan los 9 bits rwx; los especiales de cada elemento
/// se conservan salvo que se escriban en un octal de 4 dígitos (§7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModoPermisos {
    pub casillas: Casillas,
    /// Texto del campo octal: vacío con casillas mixtas hasta que se toque.
    pub octal: String,
    /// Cuarto dígito que se enseña sin haberse escrito: los bits especiales
    /// comunes a todos los elementos (`None` si difieren).
    especiales_vistos: Option<u32>,
    /// Bits especiales escritos en un octal de 4 dígitos: solo entonces entran
    /// en la máscara.
    especiales_escritos: Option<u32>,
    /// Casillas (y cuarto dígito) de antes de escribir en el octal: vuelven
    /// mientras el octal no sea válido, para que borrarlo dígito a dígito no
    /// deje las de un valor intermedio («0644» → «064» → «06»).
    previas: Option<(Casillas, Option<u32>)>,
}

impl ModoPermisos {
    /// Valor inicial con los modos de los elementos elegidos (`None` si el
    /// listado no lo dio).
    pub fn desde_modos(modos: &[Option<u32>]) -> Self {
        let conocidos: Vec<u32> = modos
            .iter()
            .flatten()
            .map(|modo| modo & BITS_TODOS)
            .collect();
        let desconocido = conocidos.len() < modos.len();
        let especiales = conocidos.first().map(|modo| modo & BITS_ESPECIALES);
        let especiales_vistos = especiales.filter(|especiales| {
            !desconocido
                && conocidos
                    .iter()
                    .all(|modo| modo & BITS_ESPECIALES == *especiales)
        });
        let mut modo = Self {
            casillas: Casillas::desde_modos(&conocidos, desconocido),
            octal: String::new(),
            especiales_vistos,
            especiales_escritos: None,
            previas: None,
        };
        modo.sincronizar_octal();
        modo
    }

    /// `Espacio` sobre una casilla: el octal pasa a decir lo mismo.
    pub fn alternar(&mut self, quien: usize, que: usize) {
        self.casillas.alternar(quien, que);
        self.previas = None;
        self.sincronizar_octal();
    }

    /// Un dígito en el campo octal (hasta 4). Devuelve si se aceptó.
    pub fn escribir(&mut self, digito: char) -> bool {
        if self.octal.len() >= 4 || !('0'..='7').contains(&digito) {
            return false;
        }
        self.recordar_previas();
        self.octal.push(digito);
        self.leer_octal();
        true
    }

    pub fn borrar(&mut self) {
        self.recordar_previas();
        self.octal.pop();
        self.leer_octal();
    }

    fn recordar_previas(&mut self) {
        if self.previas.is_none() {
            self.previas = Some((self.casillas.clone(), self.especiales_vistos));
        }
    }

    /// Un octal válido manda sobre las casillas; uno a medias devuelve las de
    /// antes de escribirlo.
    fn leer_octal(&mut self) {
        match octal_valido(&self.octal) {
            Some(valor) => {
                self.casillas = Casillas::desde_modo(valor & BITS_RWX);
                if self.octal.len() == 4 {
                    self.especiales_escritos = Some(valor & BITS_ESPECIALES);
                } else {
                    self.especiales_escritos = None;
                    self.especiales_vistos = None;
                }
            }
            None => {
                self.especiales_escritos = None;
                if let Some((casillas, vistos)) = self.previas.clone() {
                    self.casillas = casillas;
                    self.especiales_vistos = vistos;
                }
            }
        }
    }

    /// El octal de las casillas, o vacío si alguna es mixta.
    fn sincronizar_octal(&mut self) {
        self.octal = if self.casillas.mixtos() != 0 {
            String::new()
        } else {
            match self.especiales_escritos.or(self.especiales_vistos) {
                Some(especiales) => format!("{:04o}", especiales | self.casillas.modo()),
                None => format!("{:03o}", self.casillas.modo()),
            }
        };
    }

    /// Modo y máscara que se envían: con casillas, los 9 bits rwx menos los
    /// mixtos; con un octal de 4 dígitos, también los especiales.
    pub fn modo_y_mascara(&self) -> Result<(u32, u32), String> {
        if !self.octal.is_empty() && octal_valido(&self.octal).is_none() {
            return Err("el octal lleva 3 o 4 dígitos del 0 al 7".to_string());
        }
        let mascara = match self.especiales_escritos {
            Some(_) => BITS_TODOS,
            None => BITS_RWX,
        } & !self.casillas.mixtos();
        let modo = (self.casillas.modo() | self.especiales_escritos.unwrap_or(0)) & mascara;
        Ok((modo, mascara))
    }

    /// El modo para los mensajes: el octal si lo hay; si no, `rw-r~-r~-`.
    pub fn texto(&self) -> String {
        match octal_valido(&self.octal) {
            Some(_) => self.octal.clone(),
            None => self.casillas.simbolico(),
        }
    }

    /// Algún trío (usuario, grupo u otros) se queda con `r` y sin `x`: en un
    /// directorio, se ve el nombre de lo que hay dentro pero no se entra.
    fn quita_x_con_r(&self) -> bool {
        (0..3).any(|quien| {
            self.casillas.casilla(quien, 2) == Casilla::No
                && self.casillas.casilla(quien, 0) != Casilla::No
        })
    }
}

/// Aviso ámbar (§6.1): con alcance «todo», el modo quita `x` a directorios.
pub fn aviso_directorios(alcance: Option<AlcancePermisos>, modo: &ModoPermisos) -> bool {
    alcance == Some(AlcancePermisos::Todo) && modo.quita_x_con_r()
}

/// ¿Cambia el alcance esta entrada? Sin alcance solo se recorren las rutas
/// elegidas, y todas cambian; con alcance, cada entrada (elegida o de
/// dentro) según su tipo (§7.4). Los enlaces nunca llegan aquí.
pub fn afecta(alcance: Option<AlcancePermisos>, es_dir: bool) -> bool {
    match alcance {
        None | Some(AlcancePermisos::Todo) => true,
        Some(AlcancePermisos::Directorios) => es_dir,
        Some(AlcancePermisos::Ficheros) => !es_dir,
    }
}

/// Modo nuevo de una entrada.
pub fn modo_nuevo(viejo: u32, modo: u32, mascara: u32) -> u32 {
    (viejo & !mascara) | (modo & mascara)
}

/// Lo que tocará un `chmod` recursivo, para la confirmación.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recuento {
    pub ficheros: u64,
    pub directorios: u64,
    /// Enlaces encontrados: nunca se tocan.
    pub enlaces: u64,
    /// Primer directorio que no se pudo leer: el recuento se queda corto.
    pub incompleto: Option<String>,
}

impl Recuento {
    /// Cuenta una entrada si el alcance la cambia.
    pub fn contar(&mut self, alcance: Option<AlcancePermisos>, tipo: TipoEntrada) {
        match tipo {
            TipoEntrada::Enlace => self.enlaces += 1,
            TipoEntrada::Directorio if afecta(alcance, true) => self.directorios += 1,
            TipoEntrada::Fichero if afecta(alcance, false) => self.ficheros += 1,
            _ => {}
        }
    }

    pub fn total(&self) -> u64 {
        self.ficheros + self.directorios
    }

    /// `1 342 ficheros y 87 directorios`, `1 directorio`…
    pub fn descripcion(&self) -> String {
        let ficheros = cantidad(self.ficheros, "fichero", "ficheros");
        let directorios = cantidad(self.directorios, "directorio", "directorios");
        match (self.ficheros, self.directorios) {
            (0, 0) => "nada".to_string(),
            (_, 0) => ficheros,
            (0, _) => directorios,
            _ => format!("{ficheros} y {directorios}"),
        }
    }
}

/// `1 fichero`, `1 342 ficheros`.
pub fn cantidad(numero: u64, singular: &str, plural: &str) -> String {
    match numero {
        1 => format!("1 {singular}"),
        _ => format!("{} {plural}", con_miles(numero)),
    }
}

/// Número con los miles separados por un espacio: `1 342`.
pub fn con_miles(numero: u64) -> String {
    let cifras = numero.to_string();
    let mut texto = String::with_capacity(cifras.len() + cifras.len() / 3);
    for (indice, cifra) in cifras.chars().enumerate() {
        if indice > 0 && (cifras.len() - indice).is_multiple_of(3) {
            texto.push(' ');
        }
        texto.push(cifra);
    }
    texto
}

/// Tipo de una entrada local sin seguir enlaces.
fn tipo_de(metadata: &fs::Metadata) -> TipoEntrada {
    if metadata.file_type().is_symlink() {
        TipoEntrada::Enlace
    } else if metadata.is_dir() {
        TipoEntrada::Directorio
    } else {
        TipoEntrada::Fichero
    }
}

/// Recorre las rutas en local (sin seguir enlaces) y cuenta lo que cambiaría
/// el alcance. Lo que no se puede leer no detiene el recuento: es una
/// estimación, y el `chmod` puede ser justo lo que lo arregle.
pub fn contar_local(rutas: &[PathBuf], alcance: Option<AlcancePermisos>) -> Recuento {
    let mut recuento = Recuento::default();
    for ruta in rutas {
        contar_en(ruta, alcance, &mut recuento);
    }
    recuento
}

fn contar_en(ruta: &Path, alcance: Option<AlcancePermisos>, recuento: &mut Recuento) {
    let metadata = match fs::symlink_metadata(ruta) {
        Ok(metadata) => metadata,
        Err(error) => {
            recuento
                .incompleto
                .get_or_insert_with(|| format!("{}: {error}", ruta.display()));
            return;
        }
    };
    let tipo = tipo_de(&metadata);
    recuento.contar(alcance, tipo);
    if tipo != TipoEntrada::Directorio || alcance.is_none() {
        return;
    }
    match fs::read_dir(ruta) {
        Ok(lectura) => {
            for hijo in lectura {
                match hijo {
                    Ok(hijo) => contar_en(&hijo.path(), alcance, recuento),
                    Err(error) => {
                        recuento
                            .incompleto
                            .get_or_insert_with(|| format!("{}: {error}", ruta.display()));
                    }
                }
            }
        }
        Err(error) => {
            recuento
                .incompleto
                .get_or_insert_with(|| format!("{}: {error}", ruta.display()));
        }
    }
}

/// Resultado de un `chmod` local.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultadoLocal {
    pub afectados: u64,
    /// Enlaces que se saltaron (nunca se tocan).
    pub enlaces: u64,
    pub errores: u64,
    pub primer_error: Option<String>,
}

impl ResultadoLocal {
    fn error(&mut self, ruta: &Path, error: std::io::Error) {
        self.errores += 1;
        self.primer_error
            .get_or_insert_with(|| format!("{}: {error}", ruta.display()));
    }

    /// `12 cambiados · 1 enlace sin tocar · 2 errores (…)`.
    pub fn resumen(&self) -> String {
        let mut partes = vec![cantidad(self.afectados, "cambiado", "cambiados")];
        if self.enlaces > 0 {
            partes.push(format!(
                "{} sin tocar",
                cantidad(self.enlaces, "enlace", "enlaces")
            ));
        }
        if self.errores > 0 {
            let errores = cantidad(self.errores, "error", "errores");
            partes.push(match &self.primer_error {
                Some(primero) => format!("{errores} ({primero})"),
                None => errores,
            });
        }
        partes.join(" · ")
    }
}

/// `chmod` local: sin alcance, a las rutas; con alcance, también a lo que
/// contienen según su tipo. Un error en una ruta no detiene el resto.
pub fn aplicar_local(
    rutas: &[PathBuf],
    modo: u32,
    mascara: u32,
    alcance: Option<AlcancePermisos>,
) -> ResultadoLocal {
    let mut resultado = ResultadoLocal::default();
    for ruta in rutas {
        aplicar_en(ruta, modo, mascara, alcance, &mut resultado);
    }
    resultado
}

fn aplicar_en(
    ruta: &Path,
    modo: u32,
    mascara: u32,
    alcance: Option<AlcancePermisos>,
    resultado: &mut ResultadoLocal,
) {
    let metadata = match fs::symlink_metadata(ruta) {
        Ok(metadata) => metadata,
        Err(error) => return resultado.error(ruta, error),
    };
    if metadata.file_type().is_symlink() {
        resultado.enlaces += 1;
        return;
    }
    let es_dir = metadata.is_dir();
    let nuevo = modo_nuevo(metadata.permissions().mode() & BITS_TODOS, modo, mascara);
    let cambia = afecta(alcance, es_dir);
    let recorre = es_dir && alcance.is_some();
    // Si el modo nuevo deja al propietario sin entrar en el directorio (sin
    // `r` o sin `x`), primero se cambia lo de dentro y después el directorio;
    // si se lo da, al revés: así `0644` y `0755` recursivos llegan al fondo.
    let antes = cambia && !(recorre && nuevo & 0o500 != 0o500);
    if antes {
        cambiar(ruta, nuevo, resultado);
    }
    if recorre {
        match fs::read_dir(ruta) {
            Ok(lectura) => {
                for hijo in lectura {
                    match hijo {
                        Ok(hijo) => aplicar_en(&hijo.path(), modo, mascara, alcance, resultado),
                        Err(error) => resultado.error(ruta, error),
                    }
                }
            }
            Err(error) => resultado.error(ruta, error),
        }
    }
    if cambia && !antes {
        cambiar(ruta, nuevo, resultado);
    }
}

fn cambiar(ruta: &Path, modo: u32, resultado: &mut ResultadoLocal) {
    match fs::set_permissions(ruta, fs::Permissions::from_mode(modo)) {
        Ok(()) => resultado.afectados += 1,
        Err(error) => resultado.error(ruta, error),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn modo_de(ruta: &Path) -> u32 {
        fs::symlink_metadata(ruta).unwrap().permissions().mode() & BITS_TODOS
    }

    fn fijar(ruta: &Path, modo: u32) {
        fs::set_permissions(ruta, fs::Permissions::from_mode(modo)).unwrap();
    }

    #[test]
    fn un_solo_modo_da_casillas_y_octal_de_cuatro_digitos() {
        let modo = ModoPermisos::desde_modos(&[Some(0o100644)]);
        assert_eq!(modo.casillas.simbolico(), "rw-r--r--");
        assert_eq!(modo.octal, "0644");
        assert_eq!(modo.casillas.casilla(0, 1), Casilla::Si);
        assert_eq!(modo.casillas.casilla(1, 1), Casilla::No);
        assert_eq!(modo.modo_y_mascara(), Ok((0o644, 0o777)));

        let setgid = ModoPermisos::desde_modos(&[Some(0o42755)]);
        assert_eq!(setgid.octal, "2755", "el cuarto dígito se enseña");
        assert_eq!(
            setgid.modo_y_mascara(),
            Ok((0o755, 0o777)),
            "sin escribirlo, los especiales se conservan"
        );
    }

    #[test]
    fn los_modos_distintos_dejan_casillas_mixtas_y_el_octal_vacio() {
        let modo = ModoPermisos::desde_modos(&[Some(0o644), Some(0o600), Some(0o755)]);
        assert_eq!(modo.casillas.simbolico(), "rw~~-~~-~");
        assert_eq!(modo.octal, "", "vacío hasta que se toque");
        assert_eq!(modo.casillas.casilla(0, 2), Casilla::Mixto);
        // Lo mixto queda fuera de la máscara: se conserva el de cada uno.
        assert_eq!(modo.modo_y_mascara(), Ok((0o600, 0o777 & !0o155)));
        assert_eq!(modo.texto(), "rw~~-~~-~");

        let desconocido = ModoPermisos::desde_modos(&[Some(0o644), None]);
        assert_eq!(desconocido.casillas.simbolico(), "~~~~~~~~~");
        assert_eq!(desconocido.modo_y_mascara(), Ok((0, 0)));
    }

    #[test]
    fn espacio_pasa_de_mixto_a_si_y_despues_alterna() {
        let mut modo = ModoPermisos::desde_modos(&[Some(0o644), Some(0o640)]);
        assert_eq!(modo.casillas.casilla(2, 0), Casilla::Mixto);
        modo.alternar(2, 0);
        assert_eq!(modo.casillas.casilla(2, 0), Casilla::Si);
        assert_eq!(modo.octal, "0644", "sin mixtas el octal vuelve");
        modo.alternar(2, 0);
        assert_eq!(modo.casillas.casilla(2, 0), Casilla::No);
        assert_eq!(modo.octal, "0640");
        modo.alternar(2, 0);
        assert_eq!(modo.casillas.casilla(2, 0), Casilla::Si);
    }

    #[test]
    fn el_octal_valido_manda_sobre_las_casillas() {
        let mut modo = ModoPermisos::desde_modos(&[Some(0o644), Some(0o600)]);
        for digito in "75".chars() {
            assert!(modo.escribir(digito));
        }
        assert_eq!(
            modo.casillas.simbolico(),
            "rw-~--~--",
            "incompleto: no toca"
        );
        assert!(modo.modo_y_mascara().is_err());
        assert!(modo.escribir('5'));
        assert_eq!(modo.casillas.simbolico(), "rwxr-xr-x");
        assert_eq!(
            modo.modo_y_mascara(),
            Ok((0o755, 0o777)),
            "con 3 dígitos los especiales se conservan"
        );
        assert!(!modo.escribir('9'), "solo del 0 al 7");
        assert!(modo.escribir('1'));
        assert_eq!(modo.octal, "7551");
        assert_eq!(modo.casillas.simbolico(), "r-xr-x--x");
        assert_eq!(
            modo.modo_y_mascara(),
            Ok((0o7551 & BITS_TODOS, 0o7777)),
            "con 4 dígitos se escriben también"
        );
        assert!(!modo.escribir('0'), "hasta 4 dígitos");
        modo.borrar();
        assert_eq!(modo.octal, "755");
        assert_eq!(modo.modo_y_mascara(), Ok((0o755, 0o777)));
        modo.borrar();
        assert_eq!(
            modo.casillas.simbolico(),
            "rw-~--~--",
            "a medias vuelven las de antes de escribir"
        );
    }

    #[test]
    fn los_especiales_escritos_sobreviven_a_las_casillas() {
        let mut modo = ModoPermisos::desde_modos(&[Some(0o644)]);
        modo.borrar();
        assert_eq!(modo.casillas.simbolico(), "---rw-r--", "«064» es válido");
        for _ in 0..3 {
            modo.borrar();
        }
        assert_eq!(modo.octal, "");
        assert_eq!(modo.casillas.simbolico(), "rw-r--r--");
        assert_eq!(
            modo.modo_y_mascara(),
            Ok((0o644, 0o777)),
            "octal vacío: mandan las casillas"
        );
        for digito in "1777".chars() {
            modo.escribir(digito);
        }
        modo.alternar(2, 1);
        assert_eq!(modo.octal, "1775");
        assert_eq!(modo.modo_y_mascara(), Ok((0o1775, 0o7777)));
    }

    #[test]
    fn el_modo_nuevo_respeta_la_mascara() {
        assert_eq!(modo_nuevo(0o2755, 0o644, 0o777), 0o2644);
        assert_eq!(modo_nuevo(0o2755, 0o644, 0o7777), 0o644);
        // [~] en x de grupo y otros: cada uno conserva el suyo.
        assert_eq!(modo_nuevo(0o755, 0o600, 0o777 & !0o011), 0o611);
        assert_eq!(modo_nuevo(0o640, 0o600, 0o777 & !0o011), 0o600);
    }

    #[test]
    fn el_aviso_sale_con_alcance_todo_y_r_sin_x() {
        let quita = ModoPermisos::desde_modos(&[Some(0o644)]);
        assert!(aviso_directorios(Some(AlcancePermisos::Todo), &quita));
        assert!(!aviso_directorios(Some(AlcancePermisos::Ficheros), &quita));
        assert!(!aviso_directorios(
            Some(AlcancePermisos::Directorios),
            &quita
        ));
        assert!(!aviso_directorios(None, &quita));
        let entra = ModoPermisos::desde_modos(&[Some(0o755)]);
        assert!(!aviso_directorios(Some(AlcancePermisos::Todo), &entra));
        let privado = ModoPermisos::desde_modos(&[Some(0o700)]);
        assert!(
            !aviso_directorios(Some(AlcancePermisos::Todo), &privado),
            "sin r tampoco se ve nada: no es un descuido"
        );
        // x mixta: cada directorio conserva la suya.
        let mixto = ModoPermisos::desde_modos(&[Some(0o755), Some(0o644)]);
        assert!(!aviso_directorios(Some(AlcancePermisos::Todo), &mixto));
    }

    #[test]
    fn cada_alcance_cambia_lo_suyo() {
        assert!(afecta(None, true) && afecta(None, false));
        let todo = Some(AlcancePermisos::Todo);
        assert!(afecta(todo, true) && afecta(todo, false));
        let dirs = Some(AlcancePermisos::Directorios);
        assert!(afecta(dirs, true) && !afecta(dirs, false));
        let ficheros = Some(AlcancePermisos::Ficheros);
        assert!(!afecta(ficheros, true) && afecta(ficheros, false));

        let mut recuento = Recuento::default();
        for tipo in [
            TipoEntrada::Directorio,
            TipoEntrada::Fichero,
            TipoEntrada::Fichero,
            TipoEntrada::Enlace,
        ] {
            recuento.contar(ficheros, tipo);
        }
        assert_eq!((recuento.ficheros, recuento.directorios), (2, 0));
        assert_eq!(recuento.enlaces, 1, "los enlaces no cuentan");
        assert_eq!(recuento.descripcion(), "2 ficheros");
    }

    #[test]
    fn los_numeros_se_agrupan_por_miles() {
        assert_eq!(con_miles(0), "0");
        assert_eq!(con_miles(999), "999");
        assert_eq!(con_miles(1_342), "1 342");
        assert_eq!(con_miles(1_234_567), "1 234 567");
        let recuento = Recuento {
            ficheros: 1_342,
            directorios: 1,
            ..Recuento::default()
        };
        assert_eq!(recuento.descripcion(), "1 342 ficheros y 1 directorio");
    }

    /// raiz/{a.txt, sub/{b.txt, dentro/}, enlace → a.txt}
    fn arbol() -> (tempfile::TempDir, PathBuf) {
        let temporal = tempfile::tempdir().unwrap();
        let raiz = temporal.path().join("raiz");
        fs::create_dir_all(raiz.join("sub/dentro")).unwrap();
        fs::write(raiz.join("a.txt"), b"a").unwrap();
        fs::write(raiz.join("sub/b.txt"), b"b").unwrap();
        std::os::unix::fs::symlink(raiz.join("a.txt"), raiz.join("enlace")).unwrap();
        for ruta in [raiz.clone(), raiz.join("sub"), raiz.join("sub/dentro")] {
            fijar(&ruta, 0o755);
        }
        fijar(&raiz.join("a.txt"), 0o600);
        fijar(&raiz.join("sub/b.txt"), 0o600);
        (temporal, raiz)
    }

    #[test]
    fn el_recuento_local_no_sigue_enlaces() {
        let (_temporal, raiz) = arbol();
        let raices = [raiz];
        let todo = contar_local(&raices, Some(AlcancePermisos::Todo));
        assert_eq!((todo.ficheros, todo.directorios, todo.enlaces), (2, 3, 1));
        let dirs = contar_local(&raices, Some(AlcancePermisos::Directorios));
        assert_eq!((dirs.ficheros, dirs.directorios), (0, 3));
        assert!(todo.incompleto.is_none());
    }

    #[test]
    fn el_chmod_local_recursivo_llega_al_fondo_y_no_toca_enlaces() {
        let (_temporal, raiz) = arbol();
        // 0644 en todo: los directorios pierden la x, pero se recorren antes.
        let resultado = aplicar_local(
            std::slice::from_ref(&raiz),
            0o644,
            0o777,
            Some(AlcancePermisos::Todo),
        );
        assert_eq!(resultado.errores, 0, "{resultado:?}");
        assert_eq!(resultado.afectados, 5);
        assert_eq!(resultado.enlaces, 1);
        assert_eq!(modo_de(&raiz), 0o644);
        // Para mirar dentro hace falta volver a entrar.
        fijar(&raiz, 0o755);
        fijar(&raiz.join("sub"), 0o755);
        assert_eq!(modo_de(&raiz.join("sub/b.txt")), 0o644);
        assert_eq!(modo_de(&raiz.join("sub/dentro")), 0o644);
        assert_eq!(modo_de(&raiz.join("a.txt")), 0o644);
        assert!(fs::symlink_metadata(raiz.join("enlace"))
            .unwrap()
            .file_type()
            .is_symlink());

        // Y vuelta a 0755 sobre directorios cerrados: primero se abren.
        fijar(&raiz.join("sub"), 0o644);
        fijar(&raiz, 0o644);
        let resultado = aplicar_local(
            std::slice::from_ref(&raiz),
            0o755,
            0o777,
            Some(AlcancePermisos::Directorios),
        );
        assert_eq!(resultado.errores, 0, "{resultado:?}");
        assert_eq!(resultado.afectados, 3);
        assert_eq!(modo_de(&raiz.join("sub/dentro")), 0o755);
        assert_eq!(modo_de(&raiz.join("sub/b.txt")), 0o644, "solo directorios");
    }

    #[test]
    fn el_chmod_local_sin_alcance_solo_toca_las_rutas() {
        let (_temporal, raiz) = arbol();
        let resultado = aplicar_local(&[raiz.join("sub"), raiz.join("enlace")], 0o700, 0o777, None);
        assert_eq!((resultado.afectados, resultado.enlaces), (1, 1));
        assert_eq!(modo_de(&raiz.join("sub")), 0o700);
        assert_eq!(modo_de(&raiz.join("sub/b.txt")), 0o600);
        assert_eq!(modo_de(&raiz.join("a.txt")), 0o600, "el enlace no se sigue");

        let fallo = aplicar_local(&[raiz.join("no-existe")], 0o700, 0o777, None);
        assert_eq!(fallo.errores, 1);
        assert!(fallo.resumen().contains("no-existe"), "{}", fallo.resumen());
        assert_eq!(resultado.resumen(), "1 cambiado · 1 enlace sin tocar");
    }
}
