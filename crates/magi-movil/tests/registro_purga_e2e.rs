//! Prueba de regresión de la purga de REGISTRO del móvil (informe §7.10 y
//! checklist de la Fase 14): al arrancar el núcleo se borran las entradas de
//! más de 90 días y se quedan las recientes. Sin red externa: solo arranca el
//! núcleo sobre una magi.db temporal preparada a mano.

mod comun;

use chrono::{Duration, Local};

use comun::{consulta, PruebaNucleo};

/// Mete una entrada en REGISTRO con la fecha y el detalle dados, antes de
/// arrancar el núcleo (el hilo escritor aún no está en marcha).
fn anotar(prueba: &PruebaNucleo, fecha: &str, detalle: &str) {
    let fecha = fecha.replace('\'', "''");
    let detalle = detalle.replace('\'', "''");
    prueba.preparar_bd(move |almacen| {
        let sql = format!(
            "INSERT INTO REGISTRO (fecha, tipo, host_id, identidad_id, detalle, resultado) \
             VALUES ('{fecha}', 'conexion_abierta', NULL, NULL, '{detalle}', 'ok')"
        );
        almacen
            .conexion()
            .execute_batch(&sql)
            .expect("anotando en el registro de prueba");
    });
}

#[test]
fn al_arrancar_se_purga_el_registro_de_mas_de_90_dias() {
    let prueba = PruebaNucleo::nueva(&[]);
    let ahora = Local::now();
    let vieja = (ahora - Duration::days(100))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let reciente = (ahora - Duration::days(89))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    anotar(&prueba, &vieja, "entrada vieja");
    anotar(&prueba, &reciente, "entrada reciente");
    let mut prueba = prueba;
    prueba.arrancar();

    let detalles = consulta(prueba.nucleo(), |almacen| -> Vec<String> {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT detalle FROM REGISTRO ORDER BY id")
            .expect("consulta del registro");
        let filas = sentencia
            .query_map([], |fila| fila.get::<_, String>(0))
            .expect("filas del registro");
        filas.map(|fila| fila.expect("fila del registro")).collect()
    })
    .expect("registro leído");
    // El arranque anota además la clave del móvil: se mira solo lo sembrado.
    assert!(
        detalles.iter().any(|detalle| detalle == "entrada reciente"),
        "la entrada de menos de 90 días se queda: {detalles:?}"
    );
    assert!(
        !detalles.iter().any(|detalle| detalle == "entrada vieja"),
        "la entrada de más de 90 días se purga: {detalles:?}"
    );
}
