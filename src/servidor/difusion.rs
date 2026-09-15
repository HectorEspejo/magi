//! Envío de mensajes a los clientes conectados. Las pantallas y datos solo
//! van a clientes adjuntos; los estados y la lista de sesiones se difunden.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::protocolo::MensajeServidor;

use super::cliente_remoto::ClienteRemoto;

/// Ritmo máximo de la difusión de progreso de la cola (4 veces por segundo).
pub const RITMO_COLA: Duration = Duration::from_millis(250);

/// Lo que se espera cuando hay un cambio de estado pendiente: la revisora mira
/// casi enseguida en vez de agotar el ritmo del progreso.
pub const ESPERA_INMEDIATA: Duration = Duration::from_millis(20);

/// Envío de la lista de transferencias, coalescido: los cambios de estado
/// salen de inmediato y el progreso, como mucho cada `RITMO_COLA`.
#[derive(Default)]
pub struct DifusionCola {
    /// Hay algo nuevo que enviar.
    pub sucio: bool,
    /// El cambio es de estado y no puede esperar al ritmo.
    pub inmediato: bool,
    ultimo_envio: Option<Instant>,
}

impl DifusionCola {
    pub fn marcar(&mut self, inmediato: bool) {
        self.sucio = true;
        self.inmediato |= inmediato;
    }

    /// ¿Toca difundir ahora? Consume la marca cuando dice que sí.
    pub fn toca(&mut self, ahora: Instant) -> bool {
        if !self.sucio {
            return false;
        }
        if !toca_difundir(
            self.sucio,
            self.inmediato,
            self.ultimo_envio.map(|ultimo| ahora.duration_since(ultimo)),
        ) {
            return false;
        }
        self.sucio = false;
        self.inmediato = false;
        self.ultimo_envio = Some(ahora);
        true
    }
}

/// Espera el ritmo salvo que el cambio sea de estado o sea el primero. Función
/// pura para poder probarla sin relojes.
pub fn toca_difundir(sucio: bool, inmediato: bool, desde: Option<Duration>) -> bool {
    if !sucio {
        return false;
    }
    if inmediato {
        return true;
    }
    match desde {
        None => true,
        Some(transcurrido) => transcurrido >= RITMO_COLA,
    }
}

/// Envía un mensaje a un cliente concreto; si su cola ya no existe, se ignora.
pub fn enviar(cliente: &ClienteRemoto, mensaje: MensajeServidor) {
    let _ = cliente.tx.send(mensaje);
}

/// Envía el mismo mensaje a todos los clientes conectados.
pub fn difundir(clientes: &HashMap<u32, ClienteRemoto>, mensaje: MensajeServidor) {
    for cliente in clientes.values() {
        let _ = cliente.tx.send(mensaje.clone());
    }
}

/// Envía un mensaje solo a los clientes adjuntos a una sesión.
pub fn difundir_a_adjuntos(
    clientes: &HashMap<u32, ClienteRemoto>,
    adjuntos: &[u32],
    mensaje: MensajeServidor,
) {
    for id in adjuntos {
        if let Some(cliente) = clientes.get(id) {
            let _ = cliente.tx.send(mensaje.clone());
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_difusion_coalesce_el_progreso_pero_no_los_cambios_de_estado() {
        // Sin nada sucio no se envía.
        assert!(!toca_difundir(false, false, None));
        // El primero sale siempre.
        assert!(toca_difundir(true, false, None));
        // Un cambio de estado no espera al ritmo.
        assert!(toca_difundir(true, true, Some(Duration::from_millis(1))));
        // Un progreso con el ritmo sin cumplir, espera.
        assert!(!toca_difundir(
            true,
            false,
            Some(Duration::from_millis(249))
        ));
        assert!(toca_difundir(true, false, Some(RITMO_COLA)));
    }

    #[test]
    fn la_marca_sucia_se_consume_al_difundir() {
        let mut difusion = DifusionCola::default();
        let ahora = Instant::now();
        difusion.marcar(false);
        assert!(difusion.toca(ahora));
        assert!(!difusion.toca(ahora), "ya no hay nada nuevo");
        difusion.marcar(true);
        assert!(difusion.toca(ahora), "el cambio de estado sale igual");
    }
}
