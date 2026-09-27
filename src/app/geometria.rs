//! Geometría del cliente: el único punto que decide el tamaño de la terminal.
//!
//! Los `Resize` se agrupan: cada uno sustituye al pendiente y reinicia el reloj;
//! cuando pasan 50 ms sin más eventos se aplica solo el último (un `clear` y un
//! `Redimensionar` por tamaño aplicado, aunque una animación de Hyprland mande
//! treinta). Mientras hay un tamaño pendiente, el bucle se despierta cada
//! ≤ 16 ms sin esperar a una tecla. Un tamaño igual al aplicado no hace nada,
//! salvo si la ráfaga pasó por otro tamaño (ida y vuelta): entonces el
//! terminal ya truncó y volvió a crecer, y hace falta repintarlo entero.

use std::time::{Duration, Instant};

/// Silencio tras el último `Resize` para aplicar el tamaño.
pub const AGRUPACION: Duration = Duration::from_millis(50);

/// Cada cuánto se despierta el bucle mientras hay un tamaño pendiente.
pub const TICK_PENDIENTE: Duration = Duration::from_millis(16);

/// Qué hacer con la ráfaga agrupada cuando vence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vencido {
    /// Un tamaño nuevo: terminal, disposición y remoto.
    Aplicar((u16, u16)),
    /// Se volvió al tamaño aplicado pasando por otros: solo limpiar y
    /// repintar (el remoto no cambia).
    Repintar,
}

/// Tamaño aplicado, pendiente y hora del último evento.
#[derive(Debug, Clone)]
pub struct Geometria {
    aplicado: (u16, u16),
    pendiente: Option<(u16, u16)>,
    /// La ráfaga en curso pasó por un tamaño distinto del aplicado.
    movida: bool,
    ultimo_evento: Instant,
    aplicados: u64,
}

impl Geometria {
    pub fn nueva(inicial: (u16, u16), ahora: Instant) -> Self {
        Self {
            aplicado: inicial,
            pendiente: None,
            movida: false,
            ultimo_evento: ahora,
            aplicados: 0,
        }
    }

    /// Llega un `Resize`: pasa a ser el pendiente y el reloj vuelve a cero.
    pub fn registrar(&mut self, tamano: (u16, u16), ahora: Instant) {
        self.movida |= tamano != self.aplicado;
        self.pendiente = Some(tamano);
        self.ultimo_evento = ahora;
    }

    /// Lo que toca hacer si ya pasó la agrupación. Consume el pendiente: si
    /// es igual al aplicado y la ráfaga no pasó por otro tamaño, nada.
    pub fn vencido(&mut self, ahora: Instant) -> Option<Vencido> {
        let tamano = self.pendiente?;
        if ahora.saturating_duration_since(self.ultimo_evento) < AGRUPACION {
            return None;
        }
        self.pendiente = None;
        let movida = std::mem::take(&mut self.movida);
        if tamano != self.aplicado {
            Some(Vencido::Aplicar(tamano))
        } else if movida {
            Some(Vencido::Repintar)
        } else {
            None
        }
    }

    /// El tamaño ya está aplicado (terminal, disposición y remoto).
    pub fn confirmar(&mut self, tamano: (u16, u16)) {
        self.aplicado = tamano;
        self.movida = false;
        self.aplicados += 1;
    }

    /// Olvida el pendiente (al volver de una suspensión se lee el real).
    pub fn descartar_pendiente(&mut self) {
        self.pendiente = None;
        self.movida = false;
    }

    /// Cuánto puede dormir el bucle: `None` sin pendiente (hasta el próximo
    /// evento); con pendiente, lo que falte para aplicarlo y nunca más de
    /// `TICK_PENDIENTE`.
    pub fn espera(&self, ahora: Instant) -> Option<Duration> {
        self.pendiente?;
        let transcurrido = ahora.saturating_duration_since(self.ultimo_evento);
        Some(AGRUPACION.saturating_sub(transcurrido).min(TICK_PENDIENTE))
    }

    pub fn aplicado(&self) -> (u16, u16) {
        self.aplicado
    }

    pub fn pendiente(&self) -> Option<(u16, u16)> {
        self.pendiente
    }

    /// Cuántos tamaños se han aplicado desde el arranque.
    pub fn aplicados(&self) -> u64 {
        self.aplicados
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn treinta_eventos_se_aplican_una_vez_con_el_ultimo() {
        let t0 = Instant::now();
        let mut geo = Geometria::nueva((120, 40), t0);
        let mut ultimo = t0;
        for i in 0..30u16 {
            ultimo = t0 + MS * (u32::from(i) * 13);
            geo.registrar((80 + i, 24 + i / 3), ultimo);
            assert_eq!(geo.vencido(ultimo), None);
        }
        assert_eq!(geo.vencido(ultimo + MS * 49), None);
        assert_eq!(
            geo.vencido(ultimo + MS * 50),
            Some(Vencido::Aplicar((109, 33)))
        );
        geo.confirmar((109, 33));
        assert_eq!(geo.vencido(ultimo + MS * 100), None);
        assert_eq!(geo.aplicados(), 1);
    }

    #[test]
    fn un_tamano_igual_al_aplicado_no_hace_nada() {
        let t0 = Instant::now();
        let mut geo = Geometria::nueva((80, 24), t0);
        geo.registrar((80, 24), t0);
        assert_eq!(geo.vencido(t0 + AGRUPACION), None);
        assert_eq!(geo.pendiente(), None, "se consume aunque no se aplique");
        assert_eq!(geo.aplicados(), 0);
    }

    /// Ida y vuelta dentro de la agrupación: el terminal truncó y volvió a
    /// crecer, así que se repinta entero, sin aplicar tamaño ni avisar al remoto.
    #[test]
    fn una_ida_y_vuelta_pide_repintar() {
        let t0 = Instant::now();
        let mut geo = Geometria::nueva((120, 40), t0);
        geo.registrar((90, 30), t0);
        geo.registrar((120, 40), t0 + MS * 10);
        assert_eq!(geo.vencido(t0 + MS * 60), Some(Vencido::Repintar));
        assert_eq!(geo.aplicados(), 0);
        // La siguiente ráfaga empieza limpia.
        geo.registrar((120, 40), t0 + MS * 100);
        assert_eq!(geo.vencido(t0 + MS * 200), None);
    }

    #[test]
    fn solo_hay_tick_con_un_tamano_pendiente() {
        let t0 = Instant::now();
        let mut geo = Geometria::nueva((80, 24), t0);
        assert_eq!(geo.espera(t0), None);
        geo.registrar((100, 30), t0);
        assert_eq!(geo.espera(t0), Some(TICK_PENDIENTE));
        assert_eq!(geo.espera(t0 + MS * 40), Some(MS * 10));
        assert_eq!(geo.espera(t0 + MS * 60), Some(Duration::ZERO));
        geo.descartar_pendiente();
        assert_eq!(geo.espera(t0 + MS * 60), None);
    }
}
