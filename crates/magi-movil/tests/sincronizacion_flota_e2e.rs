//! La sincronización avisa a Flota (corrección posterior a la Fase 15).
//!
//! Flota solo recibía la lista completa con el sondeo. Si se abría con el
//! inventario vacío (el móvil aún sin sincronizar), la pasada que traía los
//! hosts no se lo decía y, sin hosts, no se sondeaba nada que la refrescara:
//! Flota se quedaba vacía hasta reiniciar la app. Ahora la pasada, dejar de
//! sincronizar y borrar datos difunden la lista completa por
//! `OyenteNucleo::flota`.
//!
//! Usa la fixture de Flota de las pruebas instrumentadas (misma clave y mismos
//! hosts) servida desde memoria, sin red.

mod comun;

use std::sync::Arc;
use std::time::Duration;

use comun::{esperar_flota, CarpetaConFicheros, PruebaNucleo};

const CLAVE: &str = "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS";
const FICHERO: &str = "magi-cccccccc-dddd-4eee-8fff-000000000000.age";
const FIXTURE: &[u8] = include_bytes!(
    "../../../android/app/src/androidTest/assets/flota/magi-cccccccc-dddd-4eee-8fff-000000000000.age"
);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn la_sincronizacion_difunde_la_flota_con_los_hosts_que_trae() {
    let mut prueba = PruebaNucleo::nueva(&[]);
    prueba.usar_carpeta(Arc::new(CarpetaConFicheros::nueva(vec![(
        FICHERO.to_string(),
        FIXTURE.to_vec(),
    )])));
    prueba.arrancar();
    let nucleo = prueba.nucleo().clone();

    // Flota se abre con el inventario vacío.
    assert!(nucleo.flota(None).expect("flota").is_empty());
    assert!(prueba.oyente().flota().is_empty());

    nucleo
        .elegir_carpeta("content://prueba/flota".to_string())
        .await
        .expect("eligiendo la carpeta");
    nucleo
        .unir_con_clave(CLAVE.to_string())
        .await
        .expect("uniendo con la clave");

    // Sin ningún sondeo de por medio, la lista llega con los tres hosts.
    let lista = esperar_flota(prueba.oyente(), Duration::from_secs(5), |lista| {
        lista.len() == 3
    })
    .await;
    let mut nombres: Vec<&str> = lista.iter().map(|host| host.nombre.as_str()).collect();
    nombres.sort_unstable();
    assert_eq!(nombres, ["caido", "db-01", "web-01"]);

    // Dejar de sincronizar vacía el inventario y Flota también se entera.
    nucleo
        .dejar_de_sincronizar()
        .await
        .expect("dejando de sincronizar");
    esperar_flota(prueba.oyente(), Duration::from_secs(5), |lista| {
        lista.is_empty()
    })
    .await;
    assert!(nucleo.flota(None).expect("flota").is_empty());
}
