//! Pantallas del cliente: un emulador por pestaña, compartido con la tarea de
//! lectura (que lo alimenta) y el bucle de UI (que lo pinta).
//!
//! El cliente usa el emulador del núcleo **sin responder**: las consultas del
//! remoto las contesta solo el servidor (D310). Del emulador salen los modos
//! que necesita el cliente para codificar teclas, pegado y ratón (2004,
//! cursor de aplicación, banderas de kitty y modo de ratón) y la salida
//! sincronizada (2026): mientras está abierta no se pinta la pestaña hasta su
//! cierre o `SALIDA_SINCRONIZADA_MAX`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use magi_nucleo::terminal::emulador::{PantallaEmulada, SALIDA_SINCRONIZADA_MAX};
use magi_nucleo::terminal::raton::ModoRaton;
use magi_nucleo::terminal::teclas::ModoTerminal;

/// El parser de una pestaña y el estado de su salida sincronizada.
pub struct PantallaCliente {
    emulada: PantallaEmulada,
    /// Momento en que se abrió la salida sincronizada (2026), si está abierta.
    sincronizada_desde: Option<Instant>,
}

impl std::fmt::Debug for PantallaCliente {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PantallaCliente")
            .field("filas", &self.emulada.filas())
            .field("columnas", &self.emulada.columnas())
            .field("sincronizada", &self.sincronizada_desde.is_some())
            .finish()
    }
}

impl PantallaCliente {
    #[must_use]
    pub fn nueva(filas: u16, cols: u16) -> Self {
        Self {
            emulada: PantallaEmulada::nueva(filas, cols, 0, false),
            sincronizada_desde: None,
        }
    }

    /// La pantalla de vt100, para pintarla con tui-term.
    #[must_use]
    pub fn screen(&self) -> &vt100::Screen {
        self.emulada.screen()
    }

    /// Alimenta el parser con datos del remoto.
    pub fn procesar(&mut self, bytes: &[u8]) {
        let _ = self.emulada.procesar(bytes);
        let sincronizada = self.emulada.emulador().modos().salida_sincronizada;
        match (sincronizada, self.sincronizada_desde) {
            (true, None) => self.sincronizada_desde = Some(Instant::now()),
            (false, Some(_)) => self.sincronizada_desde = None,
            _ => {}
        }
    }

    /// Cambia el tamaño del parser.
    pub fn redimensionar(&mut self, filas: u16, cols: u16) {
        self.emulada.redimensionar(filas, cols, 0);
    }

    /// ¿Hay que retener el pintado? (salida sincronizada abierta y aún dentro
    /// del plazo de 200 ms).
    #[must_use]
    pub fn retener_pintado(&self, ahora: Instant) -> bool {
        self.sincronizada_desde
            .is_some_and(|desde| ahora.duration_since(desde) < SALIDA_SINCRONIZADA_MAX)
    }

    /// Los modos con los que se codifican las teclas y el pegado.
    #[must_use]
    pub fn modo_terminal(&self) -> ModoTerminal {
        ModoTerminal::desde_emulador(&self.emulada)
    }

    /// El modo de ratón que pidió el remoto de la pantalla activa.
    #[must_use]
    pub fn modo_raton(&self) -> ModoRaton {
        ModoRaton::desde_pantalla(self.emulada.screen())
    }

    /// ¿El remoto está en la pantalla alternativa? (sin historial en MAGI).
    #[must_use]
    pub fn pantalla_alternativa(&self) -> bool {
        self.emulada.screen().alternate_screen()
    }

    /// La captura de lo visible (historial 0 en el cliente: solo la pantalla).
    #[must_use]
    pub fn captura(&mut self) -> Vec<magi_nucleo::terminal::historial::LineaHistorial> {
        magi_nucleo::terminal::historial::capturar(&mut self.emulada).lineas
    }
}

/// Puntero compartido al parser de una pestaña.
pub type Pantalla = Arc<Mutex<PantallaCliente>>;

/// Crea el parser de una pestaña.
#[must_use]
pub fn nuevo(filas: u16, cols: u16) -> Pantalla {
    Arc::new(Mutex::new(PantallaCliente::nueva(filas, cols)))
}

/// Redimensiona el parser de una pestaña.
pub fn redimensionar(pantalla: &Pantalla, filas: u16, cols: u16) {
    if let Ok(mut parser) = pantalla.lock() {
        parser.redimensionar(filas, cols);
    }
}

/// Registro de pantallas por sesión. La tarea de lectura alimenta el parser y
/// decide si hay que repintar; el bucle de la UI la repinta a ~30 fps.
#[derive(Default, Clone)]
pub struct Pantallas {
    interno: Arc<Mutex<HashMap<u32, Pantalla>>>,
    /// Pestañas con datos nuevos retenidos por la salida sincronizada.
    retenidas: Arc<Mutex<HashSet<u32>>>,
}

impl Pantallas {
    /// Devuelve el parser de una pestaña, creándolo si aún no existe. Si la
    /// tarea de lectura ya volcó una pantalla, se reutiliza el mismo parser
    /// (nunca se reemplaza: UI y lectura comparten el puntero).
    pub fn crear(&self, sesion_id: u32, filas: u16, cols: u16) -> Pantalla {
        let mut guardia = self.interno.lock().expect("pantallas del cliente");
        guardia
            .entry(sesion_id)
            .or_insert_with(|| nuevo(filas, cols))
            .clone()
    }

    /// Contenido actual de la pantalla de una sesión (para diagnósticos y
    /// pruebas).
    pub fn contenido(&self, sesion_id: u32) -> Option<String> {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        }?;
        pantalla
            .lock()
            .ok()
            .map(|parser| parser.screen().contents())
    }

    /// Reemplaza el contenido del parser de una pestaña con el volcado del
    /// servidor (`PantallaCompleta`), manteniendo el mismo puntero que la UI.
    /// Devuelve el puntero vigente (creándolo si aún no existe).
    pub fn volcar(&self, sesion_id: u32, filas: u16, cols: u16, bytes: &[u8]) -> Option<Pantalla> {
        let pantalla = {
            let mut guardia = self.interno.lock().expect("pantallas del cliente");
            match guardia.get(&sesion_id) {
                Some(pantalla) => pantalla.clone(),
                None => {
                    let pantalla = nuevo(filas, cols);
                    guardia.insert(sesion_id, pantalla.clone());
                    pantalla
                }
            }
        };
        if let Ok(mut parser) = pantalla.lock() {
            parser.redimensionar(filas.max(1), cols.max(1));
            parser.procesar(bytes);
        }
        self.retenidas
            .lock()
            .expect("retenidas del cliente")
            .remove(&sesion_id);
        Some(pantalla)
    }

    pub fn quitar(&self, sesion_id: u32) {
        self.interno
            .lock()
            .expect("pantallas del cliente")
            .remove(&sesion_id);
        self.retenidas
            .lock()
            .expect("retenidas del cliente")
            .remove(&sesion_id);
    }

    /// Cambia el tamaño del parser de una pestaña (lo hace la tarea de lectura
    /// al llegar `Redimensionada`, antes de los datos que el remoto pinte ya
    /// con el tamaño nuevo).
    pub fn redimensionar(&self, sesion_id: u32, filas: u16, cols: u16) {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        };
        if let Some(pantalla) = pantalla {
            redimensionar(&pantalla, filas, cols);
        }
    }

    /// Alimenta el parser de una pestaña con datos del remoto.
    ///
    /// Devuelve `true` si hay que anunciar el repintado ya; `false` si la
    /// salida sincronizada lo retiene (se anunciará con
    /// `anuncios_vencidos` al cerrarse o a los 200 ms).
    pub fn procesar(&self, sesion_id: u32, bytes: &[u8]) -> bool {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        };
        let Some(pantalla) = pantalla else {
            return false;
        };
        let mut guardia = match pantalla.lock() {
            Ok(pantalla) => pantalla,
            Err(_) => return false,
        };
        guardia.procesar(bytes);
        let retenida = guardia.retener_pintado(Instant::now());
        drop(guardia);
        let mut retenidas = self.retenidas.lock().expect("retenidas del cliente");
        if retenida {
            retenidas.insert(sesion_id);
            false
        } else {
            retenidas.remove(&sesion_id);
            true
        }
    }

    /// Pestañas retenidas por la salida sincronizada cuyo plazo ya venció (o
    /// cuya salida ya se cerró): hay que anunciarlas.
    pub fn anuncios_vencidos(&self, ahora: Instant) -> Vec<u32> {
        let mut retenidas = self.retenidas.lock().expect("retenidas del cliente");
        let mut vencidas = Vec::new();
        retenidas.retain(|sesion_id| {
            let parser = self
                .interno
                .lock()
                .expect("pantallas del cliente")
                .get(sesion_id)
                .cloned();
            let Some(parser) = parser else {
                return false;
            };
            let Ok(parser) = parser.lock() else {
                return false;
            };
            if parser.retener_pintado(ahora) {
                true
            } else {
                vencidas.push(*sesion_id);
                false
            }
        });
        vencidas
    }

    /// ¿Hay que retener el pintado de esta pestaña? (salida sincronizada).
    #[must_use]
    pub fn retener_pintado(&self, sesion_id: u32) -> bool {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        };
        let Some(pantalla) = pantalla else {
            return false;
        };
        let retenida = match pantalla.lock() {
            Ok(parser) => parser.retener_pintado(Instant::now()),
            Err(_) => false,
        };
        retenida
    }

    /// Los modos de una pestaña para codificar teclas (None si no existe).
    #[must_use]
    pub fn modo_terminal(&self, sesion_id: u32) -> Option<ModoTerminal> {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        }?;
        pantalla.lock().ok().map(|parser| parser.modo_terminal())
    }

    /// El modo de ratón de una pestaña (None si no existe).
    #[must_use]
    pub fn modo_raton(&self, sesion_id: u32) -> Option<ModoRaton> {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        }?;
        pantalla.lock().ok().map(|parser| parser.modo_raton())
    }

    /// ¿La pestaña está en la pantalla alternativa? (puede no existir aún).
    #[must_use]
    pub fn pantalla_alternativa(&self, sesion_id: u32) -> bool {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        };
        pantalla
            .and_then(|pantalla| {
                pantalla
                    .lock()
                    .ok()
                    .map(|parser| parser.pantalla_alternativa())
            })
            .unwrap_or(false)
    }

    /// La captura de la pantalla de una pestaña (para enlaces en vivo).
    pub fn captura(
        &self,
        sesion_id: u32,
    ) -> Option<Vec<magi_nucleo::terminal::historial::LineaHistorial>> {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        }?;
        let mut parser = pantalla.lock().ok()?;
        Some(parser.captura())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_pantallas_se_crean_se_vuelcan_y_se_quitan() {
        let pantallas = Pantallas::default();
        let pantalla = pantallas.crear(1, 24, 80);
        assert!(Arc::ptr_eq(
            &pantalla,
            &pantallas.volcar(1, 24, 80, b"hola").unwrap()
        ));
        assert_eq!(
            pantalla.lock().unwrap().screen().contents(),
            "hola".to_string()
        );
        assert!(pantallas.procesar(1, b" mundo"));
        assert_eq!(
            pantalla.lock().unwrap().screen().contents(),
            "hola mundo".to_string()
        );
        pantallas.quitar(1);
        assert!(!pantallas.procesar(1, b"sin parser")); // sin pestaña: no hace nada
    }

    #[test]
    fn la_salida_sincronizada_retiene_el_pintado_hasta_cerrarse_o_200_ms() {
        let pantallas = Pantallas::default();
        let _ = pantallas.crear(1, 24, 80);
        assert!(pantallas.procesar(1, b"antes"));
        // Abre la salida sincronizada: no se anuncia.
        assert!(!pantallas.procesar(1, b"\x1b[?2026h"));
        assert!(pantallas.retener_pintado(1));
        assert!(pantallas.anuncios_vencidos(Instant::now()).is_empty());
        // Al cerrarla sí.
        assert!(pantallas.procesar(1, b"\x1b[?2026l"));
        assert!(!pantallas.retener_pintado(1));
        assert!(pantallas.anuncios_vencidos(Instant::now()).is_empty());
    }

    #[test]
    fn una_salida_sincronizada_que_no_se_cierra_se_pinta_a_los_200_ms() {
        let pantallas = Pantallas::default();
        let _ = pantallas.crear(1, 24, 80);
        assert!(!pantallas.procesar(1, b"\x1b[?2026h"));
        let vencidas = pantallas.anuncios_vencidos(
            Instant::now() + SALIDA_SINCRONIZADA_MAX + std::time::Duration::from_millis(1),
        );
        assert_eq!(vencidas, vec![1]);
    }

    #[test]
    fn los_modos_salen_del_emulador_de_cada_pestana() {
        let pantallas = Pantallas::default();
        let _ = pantallas.crear(1, 24, 80);
        pantallas.procesar(1, b"\x1b[?2004h\x1b[>1u\x1b[?1002;1006h");
        let modo = pantallas.modo_terminal(1).unwrap();
        assert!(modo.pegado_corchetes);
        assert_eq!(modo.banderas_kitty, 1);
        let raton = pantallas.modo_raton(1).unwrap();
        assert!(raton.pedido());
        assert_eq!(
            raton.codificacion,
            magi_nucleo::terminal::raton::Codificacion::Sgr
        );
    }
}
