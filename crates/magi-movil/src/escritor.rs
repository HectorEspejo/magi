//! Hilo escritor de SQLite del núcleo móvil: el único que escribe en
//! magi.db (como OrdenBd en el servidor del escritorio).
//!
//! Todas las operaciones de base de datos del móvil pasan por aquí: abrir el
//! almacén una vez, ejecutar las tareas en un solo hilo y responder por un
//! canal síncrono. Así no hay dos hilos tocando la base a la vez y las
//! lecturas nunca ven una escritura a medias.

use std::path::Path;
use std::sync::mpsc;

use magi_nucleo::almacen::Almacen;
use magi_nucleo::rutas::Equipo;

type Tarea = Box<dyn FnOnce(&mut Almacen) + Send + 'static>;

/// Manejador del hilo escritor: clonable y utilizable desde cualquier hilo.
#[derive(Clone)]
pub struct Escritor {
    tx: mpsc::Sender<Tarea>,
}

impl Escritor {
    /// Abre la base de datos y arranca el hilo escritor.
    pub fn arrancar(ruta: &Path, equipo: &Equipo, app_version: &str) -> Result<Self, String> {
        let almacen = Almacen::abrir(ruta, equipo, app_version)
            .map_err(|error| format!("no se pudo abrir magi.db: {error:#}"))?;
        let (tx, rx) = mpsc::channel::<Tarea>();
        std::thread::Builder::new()
            .name("magi-bd".to_string())
            .spawn(move || {
                let mut almacen = Some(almacen);
                while let Ok(tarea) = rx.recv() {
                    if let Some(almacen) = almacen.as_mut() {
                        tarea(almacen);
                    }
                }
                if let Some(almacen) = almacen {
                    let _ = almacen.cerrar();
                }
            })
            .map_err(|error| format!("no se pudo arrancar el hilo escritor: {error}"))?;
        Ok(Self { tx })
    }

    /// Ejecuta una tarea en el hilo escritor y espera su resultado.
    pub fn con_almacen<R: Send + 'static>(
        &self,
        tarea: impl FnOnce(&mut Almacen) -> R + Send + 'static,
    ) -> Result<R, String> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Box::new(move |almacen| {
                let _ = tx.send(tarea(almacen));
            }))
            .map_err(|_| "el hilo escritor no está disponible".to_string())?;
        rx.recv()
            .map_err(|_| "el hilo escritor no respondió".to_string())
    }

    /// Igual que con_almacen, pero sin bloquear un hilo de tokio.
    pub async fn con_almacen_async<R: Send + 'static>(
        &self,
        tarea: impl FnOnce(&mut Almacen) -> R + Send + 'static,
    ) -> Result<R, String> {
        let escritor = self.clone();
        tokio::task::spawn_blocking(move || escritor.con_almacen(tarea))
            .await
            .map_err(|error| format!("la tarea de la base de datos se interrumpió: {error}"))?
    }
}
