//! Pruebas de caja de los enlaces del terminal (Fase 15): reconocimiento
//! sobre líneas lógicas, puntuación final, letras a-z, destinos locales y URL
//! reescrita para el túnel temporal. Sin red.

use magi_nucleo::terminal::enlaces::{
    con_letras, destino_tunel, enlaces, es_web, url_por_tunel, Enlace,
};

/// Construye las líneas lógicas (índice, texto) que recibe el cliente.
fn lineas_logicas(textos: &[&str]) -> Vec<(usize, String)> {
    textos
        .iter()
        .enumerate()
        .map(|(indice, texto)| (indice, (*texto).to_string()))
        .collect()
}

/// Los enlaces de un solo texto.
fn urls(texto: &str) -> Vec<String> {
    enlaces(&lineas_logicas(&[texto]))
        .into_iter()
        .map(|enlace| enlace.url)
        .collect()
}

#[test]
fn reconoce_solo_http_y_https() {
    assert_eq!(
        urls("https://ejemplo.org/guia"),
        ["https://ejemplo.org/guia"]
    );
    assert_eq!(urls("http://a.b"), ["http://a.b"]);
    assert!(urls("ftp://ejemplo.org/fichero").is_empty());
    assert!(urls("ssh://usuario@ejemplo.org").is_empty());
    assert!(urls("www.ejemplo.org").is_empty());
    assert!(urls("HTTP://ejemplo.org").is_empty());
    assert!(es_web("http://ejemplo.org"));
    assert!(es_web("https://ejemplo.org"));
    assert!(!es_web("ftp://ejemplo.org"));
    assert!(!es_web("ejemplo.org"));
}

#[test]
fn quita_la_puntuacion_final_de_la_frase() {
    assert_eq!(
        urls("Mira https://ejemplo.org/x."),
        ["https://ejemplo.org/x"]
    );
    assert_eq!(
        urls("(ver https://ejemplo.org/y)."),
        ["https://ejemplo.org/y"]
    );
    assert_eq!(
        urls("¿https://ejemplo.org/z? no, https://ejemplo.org/w!"),
        ["https://ejemplo.org/z", "https://ejemplo.org/w"]
    );
    assert_eq!(
        urls("https://ejemplo.org/a; https://ejemplo.org/b:"),
        ["https://ejemplo.org/a", "https://ejemplo.org/b"]
    );
    // Las comillas y los ángulos no forman parte del enlace.
    assert_eq!(
        urls("Superscript: <https://ejemplo.org/c>."),
        ["https://ejemplo.org/c"]
    );
    assert_eq!(
        urls("Copiado: \"https://ejemplo.org/d\", y luego nada."),
        ["https://ejemplo.org/d"]
    );
}

#[test]
fn conserva_el_parentesis_que_cierra_uno_de_dentro() {
    assert_eq!(
        urls("https://ejemplo.org/wiki/Foo_(bar)"),
        ["https://ejemplo.org/wiki/Foo_(bar)"]
    );
    assert_eq!(
        urls("(https://ejemplo.org/wiki/Foo_(bar))"),
        ["https://ejemplo.org/wiki/Foo_(bar)"]
    );
    assert_eq!(
        urls("https://ejemplo.org/wiki/Foo_(bar))."),
        ["https://ejemplo.org/wiki/Foo_(bar)"]
    );
}

#[test]
fn respeta_los_limites_de_largo() {
    // Nueve caracteres no llegan al mínimo.
    assert!(urls("http://ab").is_empty());
    // Diez sí.
    assert_eq!(urls("http://abc"), ["http://abc"]);

    let prefijo = "https://ejemplo.org/";
    let relleno = "a".repeat(2048 - prefijo.len());
    let justo = format!("{prefijo}{relleno}");
    assert_eq!(justo.chars().count(), 2048);
    assert_eq!(urls(&justo), std::slice::from_ref(&justo));

    let largo = format!("{prefijo}{}", "a".repeat(2048 - prefijo.len() + 1));
    assert_eq!(largo.chars().count(), 2049);
    assert!(urls(&largo).is_empty());
}

#[test]
fn encuentra_varios_enlaces_con_su_linea_y_su_columna() {
    let lineas = vec![
        (
            0,
            "Guía https://ejemplo.org/a y http://localhost:8000/x".to_string(),
        ),
        (2, "  → https://ejemplo.org/b".to_string()),
    ];
    let encontrados = enlaces(&lineas);
    assert_eq!(encontrados.len(), 3);
    assert_eq!(encontrados[0].linea, 0);
    assert_eq!(encontrados[0].columna, "Guía ".chars().count());
    assert_eq!(encontrados[0].url, "https://ejemplo.org/a");
    assert_eq!(encontrados[1].linea, 0);
    assert_eq!(
        encontrados[1].columna,
        "Guía https://ejemplo.org/a y ".chars().count()
    );
    assert_eq!(encontrados[1].url, "http://localhost:8000/x");
    assert_eq!(encontrados[2].linea, 2);
    assert_eq!(encontrados[2].columna, "  → ".chars().count());
    assert_eq!(encontrados[2].url, "https://ejemplo.org/b");
}

#[test]
fn una_linea_sin_enlaces_no_devuelve_nada() {
    assert!(enlaces(&lineas_logicas(&["usuario@web-01:~/app$ ls"])).is_empty());
    assert!(enlaces(&[]).is_empty());
}

#[test]
fn con_letras_marca_los_26_mas_cercanos_al_final() {
    let todos: Vec<Enlace> = (0..30)
        .map(|numero| Enlace {
            linea: numero,
            columna: 0,
            url: format!("https://ejemplo.org/{numero}"),
        })
        .collect();
    let marcados = con_letras(&todos);
    assert_eq!(marcados.len(), 26);
    assert_eq!(marcados[0].0, 'a');
    // Los elegidos son los últimos: el primero de ellos es el número 4.
    assert_eq!(marcados[0].1, todos[4]);
    assert_eq!(marcados[25].0, 'z');
    assert_eq!(marcados[25].1, todos[29]);

    let pocos = con_letras(&todos[..2]);
    assert_eq!(
        pocos,
        vec![('a', todos[0].clone()), ('b', todos[1].clone())]
    );
    assert!(con_letras(&[]).is_empty());
}

#[test]
fn destino_del_tunel_para_los_hosts_locales() {
    assert_eq!(
        destino_tunel("http://localhost:5173/")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("localhost", 5173))
    );
    assert_eq!(
        destino_tunel("https://localhost/guia")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("localhost", 443))
    );
    assert_eq!(
        destino_tunel("http://127.0.0.1/informe")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("127.0.0.1", 80))
    );
    assert_eq!(
        destino_tunel("http://0.0.0.0:8080/estado")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("127.0.0.1", 8080))
    );
    assert_eq!(
        destino_tunel("http://[::1]:3000/x")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("::1", 3000))
    );
    assert_eq!(
        destino_tunel("https://[::1]/x")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("::1", 443))
    );
    // La información de usuario no cambia el host.
    assert_eq!(
        destino_tunel("http://usuario:clave@localhost:8000/x")
            .as_ref()
            .map(|(host, puerto)| (host.as_str(), *puerto)),
        Some(("localhost", 8000))
    );
}

#[test]
fn no_hay_tunel_para_hosts_remotos_ni_urls_invalidas() {
    assert!(destino_tunel("http://192.0.2.10:5173/").is_none());
    assert!(destino_tunel("https://ejemplo.org/guia#arranque").is_none());
    assert!(destino_tunel("ftp://localhost/fichero").is_none());
    assert!(destino_tunel("http://localhost:0/x").is_none());
    assert!(destino_tunel("http://localhost:70000/x").is_none());
    assert!(destino_tunel("http://localhost:puerto/x").is_none());
    assert!(destino_tunel("http:///x").is_none());
    assert!(destino_tunel("").is_none());
}

#[test]
fn url_del_tunel_conserva_esquema_ruta_consulta_y_fragmento() {
    assert_eq!(
        url_por_tunel("http://localhost:8000/informe?x=1", 3210).as_deref(),
        Some("http://127.0.0.1:3210/informe?x=1")
    );
    assert_eq!(
        url_por_tunel("https://localhost/guia#arranque", 4443).as_deref(),
        Some("https://127.0.0.1:4443/guia#arranque")
    );
    assert_eq!(
        url_por_tunel("http://0.0.0.0:9000", 1234).as_deref(),
        Some("http://127.0.0.1:1234")
    );
    assert_eq!(
        url_por_tunel("http://[::1]:9000/a?b=1#c", 1234).as_deref(),
        Some("http://127.0.0.1:1234/a?b=1#c")
    );
}

#[test]
fn la_url_del_tunel_no_lleva_credenciales_ni_acepta_lo_que_no_es_tunel() {
    assert_eq!(
        url_por_tunel("http://usuario:clave@localhost:8000/x", 5555).as_deref(),
        Some("http://127.0.0.1:5555/x")
    );
    assert!(url_por_tunel("https://ejemplo.org/x", 5555).is_none());
    assert!(url_por_tunel("http://localhost:8000/x", 0).is_none());
    assert!(url_por_tunel("ftp://localhost/x", 5555).is_none());
    assert!(url_por_tunel("http://localhost:8000/x", 65535).is_some());
}

#[test]
fn conserva_el_corchete_de_una_ipv6_y_no_marca_mas_de_26() {
    // `http://[::1]` acaba en `]`, pero no es puntuación de la frase.
    let encontrados = enlaces(&[(0, "mira http://[::1]:8000/x".to_string())]);
    assert_eq!(encontrados.len(), 1);
    assert_eq!(encontrados[0].url, "http://[::1]:8000/x");
    assert_eq!(
        destino_tunel(&encontrados[0].url),
        Some(("::1".to_string(), 8000))
    );

    // Con muchos enlaces se devuelven solo los 26 más cercanos al final.
    let texto = (0..30)
        .map(|indice| format!("http://ejemplo.org/{indice}"))
        .collect::<Vec<_>>()
        .join(" ");
    let muchos = enlaces(&[(0, texto)]);
    assert_eq!(muchos.len(), 26);
    assert!(muchos[0].url.ends_with("/4"));
    assert!(muchos[25].url.ends_with("/29"));
}
