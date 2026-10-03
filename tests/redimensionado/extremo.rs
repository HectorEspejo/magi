//! De extremo a extremo: la App real (con su bucle y su `Geometria`) contra el
//! servidor de sesiones en proceso y el host SSH de pruebas, que hace de
//! `vim`/`stty size`: registra cada `window-change` y escribe `TAM c×f` en la
//! pestaña.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

use magi::almacen::Almacen;
use magi::app::{App, Evento};
use magi::protocolo::{EstadoSesionRemota, MensajeCliente};
use magi::tema::Tema;
use magi::ui::Vista;

use crate::arnes::{texto_de, BackendContador};
use crate::comun::{escenario, Escenario, Observado, OpcionesEscenario};

struct Extremo {
    app: App,
    terminal: Terminal<BackendContador>,
    esc: Escenario,
    observado: Arc<Observado>,
    rt: tokio::runtime::Runtime,
}

impl Extremo {
    /// Escenario con un host, App conectada al servidor y pestaña abierta.
    fn con_pestana(cols: u16, filas: u16) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let esc = rt.block_on(escenario(OpcionesEscenario::default()));
        let observado = esc.observado.clone();
        let almacen = Almacen::abrir(&esc.rutas().base_datos()).unwrap();
        let (mut app, _) = App::de_prueba(
            esc.rutas().clone(),
            esc.entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .unwrap();
        let mut terminal = Terminal::new(BackendContador::new(cols, filas)).unwrap();
        app.iniciar_pintado(&mut terminal).unwrap();
        app.conectar_con_servidor();
        let mut extremo = Self {
            app,
            terminal,
            esc,
            observado,
            rt,
        };
        assert!(
            extremo.hasta(|app, _| app.cliente_id.is_some()),
            "sin bienvenida"
        );
        extremo.tecla(KeyCode::F(2));
        extremo.tecla(KeyCode::Down);
        extremo.tecla(KeyCode::Enter);
        assert!(
            extremo.hasta(|app, observado| {
                app.vista == Vista::Sesion
                    && app
                        .pestanas
                        .first()
                        .is_some_and(|p| p.estado == EstadoSesionRemota::Abierta)
                    && !observado.ptys().is_empty()
            }),
            "la pestaña no abrió"
        );
        extremo
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    }

    /// Bombea eventos hasta que se cumpla la condición (plazo de 10 s).
    fn hasta(&mut self, condicion: impl Fn(&App, &Observado) -> bool) -> bool {
        let fin = Instant::now() + Duration::from_secs(10);
        while Instant::now() < fin {
            if condicion(&self.app, &self.observado) {
                return true;
            }
            self.app
                .bombear(&mut self.terminal, Duration::from_millis(20))
                .unwrap();
        }
        condicion(&self.app, &self.observado)
    }

    /// Deja correr el bucle un rato (pintados y difusiones pendientes).
    fn bombear(&mut self, milis: u64) {
        self.app
            .bombear(&mut self.terminal, Duration::from_millis(milis))
            .unwrap();
    }

    /// La ventana de Alacritty cambia de tamaño (zoom de fuente, mosaico…).
    fn redimensionar(&mut self, cols: u16, filas: u16) {
        self.terminal.backend_mut().interior.resize(cols, filas);
        self.app.procesar(Evento::Redimension(cols, filas));
    }

    fn texto(&self) -> String {
        texto_de(&self.terminal.backend().interior)
    }

    fn sesion_id(&self) -> u32 {
        self.app.pestanas[0].sesion_id
    }
}

/// El AC de `stty size`: tras cada tamaño aplicado, el remoto recibe su
/// `window-change` y lo que imprime aparece en la pestaña sin tocar ninguna
/// tecla más.
#[test]
fn una_ventana_cada_tamano_llega_al_remoto() {
    let mut e = Extremo::con_pestana(100, 30);
    assert_eq!(e.observado.ptys(), vec![(100, 26)]);

    e.redimensionar(80, 24);
    assert!(
        e.hasta(|_, o| o.tamanos().last() == Some(&(80, 20))),
        "{:?}",
        e.observado.tamanos()
    );

    // Crecer también llega (antes el remoto se quedaba en el tamaño menor).
    e.redimensionar(120, 40);
    assert!(
        e.hasta(|_, o| o.tamanos().last() == Some(&(120, 36))),
        "{:?}",
        e.observado.tamanos()
    );
    assert!(
        e.hasta(|app, _| {
            app.pantallas
                .contenido(app.pestanas[0].sesion_id)
                .is_some_and(|contenido| contenido.contains("TAM 120x36"))
        }),
        "la pestaña no muestra el tamaño nuevo"
    );
    e.bombear(100);
    assert!(e.texto().contains("TAM 120x36"), "{}", e.texto());
    assert_eq!(e.observado.tamanos(), vec![(80, 20), (120, 36)]);
}

/// Dos ventanas en la misma pestaña: manda la menor, la mayor rellena con `░`
/// y dice quién impone el mínimo; al cerrarse la pequeña, el remoto vuelve al
/// tamaño de la grande.
#[test]
fn dos_ventanas_manda_la_menor_y_al_cerrarse_vuelve_la_mayor() {
    let mut e = Extremo::con_pestana(200, 50);
    assert_eq!(e.observado.ptys(), vec![(200, 46)]);
    let sesion_id = e.sesion_id();

    let mut segunda = e.rt.block_on(e.esc.otra_ventana());
    let id_segunda = segunda.cliente_id;
    e.rt.block_on(segunda.enviar(&MensajeCliente::Adjuntar {
        sesion_id,
        cols: 120,
        filas: 36,
    }));
    assert!(e.hasta(|_, o| o.tamanos().last() == Some(&(120, 36))));
    e.rt.block_on(segunda.enviar(&MensajeCliente::Redimensionar {
        sesion_id,
        cols: 100,
        filas: 26,
    }));
    assert!(e.hasta(|_, o| o.tamanos().last() == Some(&(100, 26))));
    assert!(e.hasta(|app, _| app.pestanas[0].cols_remoto == 100));
    let marca = format!("(mín. ventana {id_segunda})");
    e.bombear(100);
    let texto = e.texto();
    assert!(texto.contains('░'), "sin relleno:\n{texto}");
    assert!(texto.contains(&marca), "sin «{marca}»:\n{texto}");

    // La ventana grande cambia de tamaño: su mínimo no manda, pero sigue
    // contando (200×50 → 180×44 no cambia nada en el remoto).
    e.redimensionar(180, 44);
    e.bombear(100);
    assert_eq!(e.observado.tamanos().last(), Some(&(100, 26)));

    drop(segunda);
    assert!(
        e.hasta(|_, o| o.tamanos().last() == Some(&(180, 40))),
        "{:?}",
        e.observado.tamanos()
    );
    assert!(e.hasta(|app, _| app.pestanas[0].cols_remoto == 180));
    e.bombear(100);
    assert!(!e.texto().contains('░'), "{}", e.texto());
}
