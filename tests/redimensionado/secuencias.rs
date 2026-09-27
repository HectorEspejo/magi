//! Secuencias de cambio 200×60 → 80×24 → 40×12 → 200×60 (S3): el estado de
//! cada vista es coherente en todos los pasos (selección a la vista, filtro y
//! marcados conservados, diálogo recolocado con su texto) y el último pintado
//! coincide con la instantánea de referencia.

use crossterm::event::KeyCode;

use magi::almacen::{hosts, Almacen};
use magi::app::{Dialogo, PanelFlota};
use magi::modelo::{DatosHost, Origen};
use magi::ui::disposicion::Lista;
use magi::ui::Vista;

use crate::arnes::AppPrueba;

const SECUENCIA: [(u16, u16); 4] = [(200, 60), (80, 24), (40, 12), (200, 60)];

/// La fila `seleccion` de la lista está dentro de la ventana pintada.
fn a_la_vista(prueba: &AppPrueba, lista: Lista, seleccion: usize) {
    if prueba.app.disposicion().aviso {
        return;
    }
    let ventana = prueba
        .app
        .disposicion()
        .lista(lista)
        .unwrap_or_else(|| panic!("{lista:?} sin pintar"));
    assert!(
        (ventana.inicio..ventana.inicio + ventana.filas).contains(&seleccion),
        "{lista:?}: selección {seleccion} fuera de {ventana:?}"
    );
}

/// Hosts con 50 hosts más, la selección en la fila 30 y un filtro activo.
#[test]
fn hosts_con_50_hosts_seleccion_y_filtro() {
    let (prueba, _) = AppPrueba::con_semilla(200, 60);
    {
        let almacen = Almacen::abrir(&prueba.app.rutas.base_datos()).unwrap();
        for numero in 0..50 {
            hosts::crear(
                almacen.conexion(),
                &DatosHost {
                    nombre: format!("nodo-{numero:02}"),
                    direccion: format!("10.9.0.{numero}"),
                    usuario: Some("ops".to_string()),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .unwrap();
        }
        almacen.cerrar().unwrap();
    }
    // La App lee el inventario al arrancar: se crea otra sobre la misma BD.
    let entorno = prueba._entorno;
    let mut prueba = AppPrueba::con_entorno(entorno, 200, 60);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "nodo".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    for _ in 0..30 {
        prueba.tecla(KeyCode::Down);
    }
    let seleccion = prueba.app.seleccion;
    let filtro = prueba.app.filtro.clone();
    assert_eq!(filtro, "nodo");
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert_eq!(prueba.app.seleccion, seleccion);
        assert_eq!(prueba.app.filtro, filtro);
        a_la_vista(&prueba, Lista::Hosts, seleccion);
    }
    prueba.instantanea("secuencia_hosts_200x60");
}

/// Flota en estrecho con el detalle a la vista: el panel se conserva al pasar
/// por el modo normal y vuelve al estrechar.
#[test]
fn flota_conserva_el_panel_y_la_seleccion() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    let seleccion = prueba.app.seleccion_flota;
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
        assert_eq!(prueba.app.seleccion_flota, seleccion);
    }
    prueba.pasar_por(80, 24);
    assert!(prueba.texto().contains("DETALLE"), "{}", prueba.texto());
    prueba.pasar_por(200, 60);
    prueba.instantanea("secuencia_flota_200x60");
}

/// Archivos con marcados y el panel remoto activo.
#[test]
fn archivos_conserva_marcados_y_panel_activo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
    crate::vistas_d::abrir_archivos(&mut prueba, &sembrado);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Char(' '));
    let estado = |prueba: &AppPrueba| {
        let archivos = prueba.app.archivos.as_ref().unwrap();
        let mut marcados: Vec<String> = archivos.remoto.marcados.iter().cloned().collect();
        marcados.sort();
        (archivos.activo, archivos.remoto.seleccion, marcados)
    };
    let antes = estado(&prueba);
    assert_eq!(antes.2.len(), 2);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert_eq!(estado(&prueba), antes);
        a_la_vista(&prueba, Lista::ArchivosRemoto, antes.1);
    }
    prueba.instantanea("secuencia_archivos_200x60");
}

/// Un diálogo con texto a medio escribir se recoloca en cada paso y conserva
/// el texto y el cursor.
#[test]
fn entrada_de_texto_conserva_el_texto_y_se_recoloca() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Char('g'));
    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::EntradaTexto { .. })),
        "{}",
        prueba.texto()
    );
    for caracter in "servidores-oficina".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    let campo = |prueba: &AppPrueba| match &prueba.app.dialogo {
        Some(Dialogo::EntradaTexto { campo, .. }) => (campo.texto.clone(), campo.cursor),
        otro => panic!("se esperaba la entrada de texto: {}", otro.is_some()),
    };
    let antes = campo(&prueba);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert_eq!(campo(&prueba), antes);
        let texto = prueba.texto();
        assert!(
            texto.contains("servidores-of"),
            "a {cols}×{filas}:\n{texto}"
        );
    }
    assert_eq!(prueba.app.vista, Vista::Hosts);
    prueba.instantanea("secuencia_entrada_texto_200x60");
}
