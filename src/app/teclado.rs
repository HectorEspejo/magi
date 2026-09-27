//! Hilo de teclas: lee los eventos del terminal (crossterm) y los pasa al bucle
//! de la UI por el canal de eventos. Las teclas van como `Evento::Tecla` y los
//! cambios de tamaño como `Evento::Redimension`, siempre, sin filtrar.
//!
//! Mientras el paginador (visor F4) tiene el terminal, el hilo se para sin
//! tocar el tty: si leyera, le robaría las pulsaciones a `less`. Antes el hilo
//! se quedaba bloqueado en un `read()` durante la pausa y, al volver, tiraba el
//! primer evento que llegase, fuese una tecla o un cambio de tamaño (R36).

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::Event;
use tokio::sync::mpsc;

use super::Evento;

/// Cada cuánto mira el hilo si debe pararse o salir mientras espera eventos.
const ESPERA_EVENTO: Duration = Duration::from_millis(50);

/// De dónde salen los eventos del terminal (crossterm, o uno falso en pruebas).
pub trait FuenteEventos: Send + 'static {
    /// ¿Hay un evento listo antes de `plazo`?
    fn esperar(&mut self, plazo: Duration) -> io::Result<bool>;
    /// El siguiente evento (solo tras un `esperar` que dijo que sí).
    fn leer(&mut self) -> io::Result<Event>;
}

/// Los eventos reales del terminal.
pub struct FuenteCrossterm;

impl FuenteEventos for FuenteCrossterm {
    fn esperar(&mut self, plazo: Duration) -> io::Result<bool> {
        crossterm::event::poll(plazo)
    }

    fn leer(&mut self) -> io::Result<Event> {
        crossterm::event::read()
    }
}

#[derive(Default)]
struct EstadoPausa {
    /// La UI ha pedido que el hilo deje el terminal.
    pedida: bool,
    /// El hilo está parado y no toca el tty.
    parado: bool,
}

/// Mando del hilo de teclas: pausa (con acuse) y salida.
#[derive(Default)]
pub struct ControlTeclas {
    salir: AtomicBool,
    pausa: Mutex<EstadoPausa>,
    cambio: Condvar,
}

impl ControlTeclas {
    /// Pide la pausa y espera, como mucho `plazo`, a que el hilo deje de leer
    /// el terminal. Devuelve si llegó a pararse.
    pub fn pausar_y_esperar(&self, plazo: Duration) -> bool {
        let fin = Instant::now() + plazo;
        let Ok(mut estado) = self.pausa.lock() else {
            return false;
        };
        estado.pedida = true;
        self.cambio.notify_all();
        while !estado.parado {
            let restante = fin.saturating_duration_since(Instant::now());
            if restante.is_zero() {
                return false;
            }
            match self.cambio.wait_timeout(estado, restante) {
                Ok((siguiente, _)) => estado = siguiente,
                Err(_) => return false,
            }
        }
        true
    }

    /// Devuelve el terminal al hilo.
    pub fn reanudar(&self) {
        if let Ok(mut estado) = self.pausa.lock() {
            estado.pedida = false;
        }
        self.cambio.notify_all();
    }

    /// El hilo termina en cuanto lo vea (≤ 50 ms).
    pub fn salir(&self) {
        self.salir.store(true, Ordering::Relaxed);
        self.reanudar();
    }

    fn saliendo(&self) -> bool {
        self.salir.load(Ordering::Relaxed)
    }

    /// Si hay pausa pedida, se para sin tocar el tty hasta que se reanude.
    /// Devuelve si ha estado parado.
    fn parar_si_toca(&self) -> bool {
        let Ok(mut estado) = self.pausa.lock() else {
            return false;
        };
        if !estado.pedida {
            return false;
        }
        estado.parado = true;
        self.cambio.notify_all();
        while estado.pedida && !self.saliendo() {
            match self.cambio.wait_timeout(estado, ESPERA_EVENTO) {
                Ok((siguiente, _)) => estado = siguiente,
                Err(_) => return true,
            }
        }
        estado.parado = false;
        true
    }
}

/// Bucle del hilo de teclas.
pub fn bucle_teclas<F: FuenteEventos>(
    mut fuente: F,
    tx: mpsc::UnboundedSender<Evento>,
    control: Arc<ControlTeclas>,
) {
    while !control.saliendo() {
        if control.parar_si_toca() {
            continue;
        }
        match fuente.esperar(ESPERA_EVENTO) {
            Ok(true) => {
                let evento = match fuente.leer() {
                    Ok(Event::Key(tecla)) => Evento::Tecla(tecla),
                    Ok(Event::Resize(cols, filas)) => Evento::Redimension(cols, filas),
                    Ok(_) => continue,
                    Err(_) => break,
                };
                if tx.send(evento).is_err() {
                    break;
                }
            }
            Ok(false) => {}
            Err(_) => break,
        }
    }
}

/// Lanza el hilo de teclas sobre el terminal real.
pub fn lanzar(tx: mpsc::UnboundedSender<Evento>, control: Arc<ControlTeclas>) {
    std::thread::spawn(move || bucle_teclas(FuenteCrossterm, tx, control));
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::collections::VecDeque;

    /// Fuente guionizada: devuelve sus eventos en orden y anota cada lectura.
    #[derive(Clone, Default)]
    struct FuenteFalsa {
        cola: Arc<Mutex<VecDeque<Event>>>,
        lecturas: Arc<Mutex<usize>>,
    }

    impl FuenteFalsa {
        fn meter(&self, evento: Event) {
            self.cola.lock().unwrap().push_back(evento);
        }

        fn lecturas(&self) -> usize {
            *self.lecturas.lock().unwrap()
        }
    }

    impl FuenteEventos for FuenteFalsa {
        fn esperar(&mut self, plazo: Duration) -> io::Result<bool> {
            if self.cola.lock().unwrap().is_empty() {
                std::thread::sleep(plazo.min(Duration::from_millis(5)));
                return Ok(false);
            }
            Ok(true)
        }

        fn leer(&mut self) -> io::Result<Event> {
            *self.lecturas.lock().unwrap() += 1;
            self.cola
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| io::Error::other("sin eventos"))
        }
    }

    fn arrancar(
        fuente: &FuenteFalsa,
    ) -> (
        Arc<ControlTeclas>,
        mpsc::UnboundedReceiver<Evento>,
        std::thread::JoinHandle<()>,
    ) {
        let control = Arc::new(ControlTeclas::default());
        let (tx, rx) = mpsc::unbounded_channel();
        let hilo = {
            let fuente = fuente.clone();
            let control = control.clone();
            std::thread::spawn(move || bucle_teclas(fuente, tx, control))
        };
        (control, rx, hilo)
    }

    fn recibir(rx: &mut mpsc::UnboundedReceiver<Evento>) -> Option<Evento> {
        let fin = Instant::now() + Duration::from_secs(2);
        while Instant::now() < fin {
            if let Ok(evento) = rx.try_recv() {
                return Some(evento);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        None
    }

    #[test]
    fn los_cambios_de_tamano_pasan_siempre() {
        let fuente = FuenteFalsa::default();
        let (control, mut rx, hilo) = arrancar(&fuente);
        fuente.meter(Event::Resize(80, 24));
        assert!(matches!(
            recibir(&mut rx),
            Some(Evento::Redimension(80, 24))
        ));
        control.salir();
        hilo.join().unwrap();
    }

    /// Regresión R36: con la pausa del visor el hilo no lee el terminal (antes
    /// hacía un `read()` bloqueante que robaba pulsaciones al paginador).
    #[test]
    fn la_pausa_no_lee_el_tty() {
        let fuente = FuenteFalsa::default();
        let (control, _rx, hilo) = arrancar(&fuente);
        assert!(control.pausar_y_esperar(Duration::from_secs(2)));
        let antes = fuente.lecturas();
        fuente.meter(Event::Resize(100, 30));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(fuente.lecturas(), antes, "leyó el tty durante la pausa");
        control.salir();
        hilo.join().unwrap();
    }

    /// Regresión R36: el cambio de tamaño que llega durante la pausa sale al
    /// reanudar; antes se tiraba el primer evento tras volver del paginador.
    #[test]
    fn el_resize_tras_la_pausa_no_se_pierde() {
        let fuente = FuenteFalsa::default();
        let (control, mut rx, hilo) = arrancar(&fuente);
        assert!(control.pausar_y_esperar(Duration::from_secs(2)));
        fuente.meter(Event::Resize(100, 30));
        std::thread::sleep(Duration::from_millis(100));
        control.reanudar();
        assert!(matches!(
            recibir(&mut rx),
            Some(Evento::Redimension(100, 30))
        ));
        control.salir();
        hilo.join().unwrap();
    }
}
