//! Conversión entre las directivas de reenvío de `ssh_config` y los túneles
//! del inventario (Fase 5). Es una conversión pura: no toca la base de datos,
//! de modo que la importación y la exportación la compartan y se pueda probar
//! sola.

use crate::modelo::{
    juntar_direccion_puerto, partir_direccion_puerto, DatosTunel, TipoTunel, Tunel,
};
use crate::sshconfig::parser;

/// Dirección en la que escucha `ssh` cuando el reenvío no lleva un `bind`
/// explícito. Al exportar se omite, para que la ida y vuelta sea estable.
const DIRECCION_POR_DEFECTO: &str = "127.0.0.1";

/// Tipo de túnel de una directiva, si es una de las que MAGI gestiona.
fn tipo_de(nombre: &str) -> Option<TipoTunel> {
    match nombre.to_lowercase().as_str() {
        "localforward" => Some(TipoTunel::Local),
        "remoteforward" => Some(TipoTunel::Remoto),
        "dynamicforward" => Some(TipoTunel::Dinamico),
        _ => None,
    }
}

/// Convierte una directiva de `ssh_config` en los datos de un túnel.
///
/// Devuelve `None` si no es una directiva de reenvío o si el valor no se
/// entiende; en ese caso el llamador la conserva como opción extra.
///
/// El `host_id` sale a `0`: lo fija quien crea la fila en `TUNELES`.
pub fn desde_directiva(directiva: &parser::Directiva) -> Option<DatosTunel> {
    let tipo = tipo_de(&directiva.nombre)?;
    // Los valores pueden venir entre comillas (por partes) y con espacios de
    // sobra; las comillas se quitan token a token.
    let partes: Vec<&str> = directiva
        .valor
        .split_whitespace()
        .map(parser::sin_comillas)
        .filter(|parte| !parte.is_empty())
        .collect();
    let (escucha, destino) = match (tipo, partes.as_slice()) {
        (TipoTunel::Dinamico, [escucha]) => (escucha, None),
        (TipoTunel::Dinamico, _) => return None,
        (_, [escucha, destino]) => (escucha, Some(destino)),
        // Formas que ssh admite y MAGI no representa (varios destinos en una
        // misma directiva, reenvíos a un socket local…) se quedan fuera.
        _ => return None,
    };
    let escucha = partir_escucha(escucha)?;
    let destino = match destino {
        Some(texto) => Some(partir_destino(texto)?),
        None => None,
    };
    Some(DatosTunel {
        host_id: 0,
        nombre: nombre_por_defecto(tipo, &escucha),
        tipo,
        escucha,
        destino,
        // `ssh` levanta los reenvíos del fichero al conectar: el túnel nace
        // automático.
        automatico: true,
    })
}

/// Línea de `ssh_config` de un túnel, sin sangría (la pone `exportar::bloque`).
///
/// Devuelve `None` cuando `ssh` no sabría leer la línea, y entonces no se
/// emite: un `LocalForward 0 destino` o un local sin destino son opciones
/// inválidas, y como el `magi_config` se incluye desde `~/.ssh/config`, una
/// sola línea mala dejaría al usuario sin `ssh` para ningún host. El puerto 0
/// solo vale en remoto, donde significa «el que elija el host».
pub fn a_directiva(tunel: &Tunel) -> Option<String> {
    let (_, puerto) = partir_direccion_puerto(&tunel.escucha)?;
    if puerto == 0 && tunel.tipo != TipoTunel::Remoto {
        return None;
    }
    let escucha = como_escucha(&tunel.escucha);
    let directiva = tunel.tipo.directiva();
    match tunel.destino.as_deref() {
        Some(destino) => Some(format!("{directiva} {escucha} {destino}")),
        // El dinámico no lleva destino.
        None if tunel.tipo == TipoTunel::Dinamico => Some(format!("{directiva} {escucha}")),
        None => None,
    }
}

/// ¿Esta línea de `opciones_extra` es un reenvío que MAGI sabe representar como
/// túnel? Solo esos están prohibidos a mano: las formas que `ssh` admite y MAGI
/// no representa (varios destinos en una directiva, reenvíos a un socket local)
/// siguen siendo opciones extra legítimas.
pub fn reenvio_gestionado(linea: &str) -> bool {
    let linea = linea.trim();
    if linea.is_empty() || linea.starts_with('#') {
        return false;
    }
    let Some((nombre, valor)) = linea.split_once(char::is_whitespace) else {
        return false;
    };
    if tipo_de(nombre).is_none() {
        return false;
    }
    desde_directiva(&parser::Directiva {
        nombre: nombre.to_string(),
        valor: valor.to_string(),
        linea: 0,
    })
    .is_some()
}

/// `[bind:]puerto` → `dirección:puerto`. Sin `bind` escucha en `127.0.0.1`,
/// como `ssh`: `LocalForward 5432 host:5432` solo abre el puerto en local.
fn partir_escucha(texto: &str) -> Option<String> {
    if let Ok(puerto) = texto.parse::<u16>() {
        return Some(juntar_direccion_puerto(DIRECCION_POR_DEFECTO, puerto));
    }
    let (direccion, puerto) = partir_direccion_puerto(texto)?;
    Some(juntar_direccion_puerto(&direccion, puerto))
}

/// `host:hostport` → `host:hostport` en la forma canónica (IPv6 entre
/// corchetes). El puerto es obligatorio: sin él no hay destino que abrir.
fn partir_destino(texto: &str) -> Option<String> {
    let (direccion, puerto) = partir_direccion_puerto(texto)?;
    Some(juntar_direccion_puerto(&direccion, puerto))
}

/// Nombre del túnel recién importado: el tipo y el puerto de la escucha.
fn nombre_por_defecto(tipo: TipoTunel, escucha: &str) -> String {
    let prefijo = match tipo {
        TipoTunel::Local => "local",
        TipoTunel::Remoto => "remoto",
        TipoTunel::Dinamico => "socks",
    };
    let puerto = partir_direccion_puerto(escucha)
        .map(|(_, puerto)| puerto)
        .unwrap_or(0);
    format!("{prefijo}-{puerto}")
}

/// El `bind` por defecto se omite, como hace `ssh`: `LocalForward 5432 …`.
/// Cualquier otra dirección va explícita, con corchetes si es IPv6.
fn como_escucha(escucha: &str) -> String {
    match partir_direccion_puerto(escucha) {
        Some((direccion, puerto)) if direccion == DIRECCION_POR_DEFECTO => puerto.to_string(),
        Some((direccion, puerto)) => juntar_direccion_puerto(&direccion, puerto),
        None => escucha.trim().to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn directiva(nombre: &str, valor: &str) -> parser::Directiva {
        parser::Directiva {
            nombre: nombre.to_string(),
            valor: valor.to_string(),
            linea: 1,
        }
    }

    fn tunel(tipo: TipoTunel, escucha: &str, destino: Option<&str>) -> Tunel {
        Tunel {
            id: 1,
            host_id: 1,
            host_nombre: "host".to_string(),
            nombre: "tunel".to_string(),
            tipo,
            escucha: escucha.to_string(),
            destino: destino.map(str::to_string),
            automatico: true,
            creado_en: "2026-01-01 00:00:00".to_string(),
            actualizado_en: "2026-01-01 00:00:00".to_string(),
        }
    }

    #[test]
    fn reconoce_las_tres_directivas_con_bind_por_defecto() {
        let local = desde_directiva(&directiva("LocalForward", "5432 10.0.0.5:5432")).unwrap();
        assert_eq!(local.tipo, TipoTunel::Local);
        assert_eq!(local.escucha, "127.0.0.1:5432");
        assert_eq!(local.destino.as_deref(), Some("10.0.0.5:5432"));
        assert_eq!(local.nombre, "local-5432");
        assert!(local.automatico);
        assert_eq!(local.host_id, 0);

        let remoto = desde_directiva(&directiva("RemoteForward", "9000 127.0.0.1:9000")).unwrap();
        assert_eq!(remoto.tipo, TipoTunel::Remoto);
        assert_eq!(remoto.escucha, "127.0.0.1:9000");
        assert_eq!(remoto.destino.as_deref(), Some("127.0.0.1:9000"));
        assert_eq!(remoto.nombre, "remoto-9000");

        let dinamico = desde_directiva(&directiva("DynamicForward", "1080")).unwrap();
        assert_eq!(dinamico.tipo, TipoTunel::Dinamico);
        assert_eq!(dinamico.escucha, "127.0.0.1:1080");
        assert_eq!(dinamico.destino, None);
        assert_eq!(dinamico.nombre, "socks-1080");
    }

    #[test]
    fn acepta_minusculas_espacios_comillas_e_ipv6() {
        let datos = desde_directiva(&directiva(
            "localforward",
            "   0.0.0.0:8080   10.0.0.5:80   ",
        ))
        .unwrap();
        assert_eq!(datos.escucha, "0.0.0.0:8080");
        assert_eq!(datos.destino.as_deref(), Some("10.0.0.5:80"));
        assert_eq!(datos.nombre, "local-8080");

        let citado =
            desde_directiva(&directiva("LocalForward", "\"5432\"   \"10.0.0.5:5432\"")).unwrap();
        assert_eq!(citado.escucha, "127.0.0.1:5432");
        assert_eq!(citado.destino.as_deref(), Some("10.0.0.5:5432"));

        let ipv6 = desde_directiva(&directiva("DynamicForward", "[::1]:1080")).unwrap();
        assert_eq!(ipv6.escucha, "[::1]:1080");
        assert_eq!(ipv6.nombre, "socks-1080");
    }

    #[test]
    fn rechaza_lo_que_no_entiende() {
        assert!(desde_directiva(&directiva("ForwardAgent", "yes")).is_none());
        // Falta el destino.
        assert!(desde_directiva(&directiva("LocalForward", "5432")).is_none());
        // Varios destinos en una misma directiva.
        assert!(desde_directiva(&directiva("LocalForward", "5432 a:80 b:80")).is_none());
        // Sin puerto en el destino.
        assert!(desde_directiva(&directiva("RemoteForward", "9000 10.0.0.5")).is_none());
        // Un dinámico no admite destino.
        assert!(desde_directiva(&directiva("DynamicForward", "1080 10.0.0.5:1080")).is_none());
        // Puerto fuera de rango.
        assert!(desde_directiva(&directiva("LocalForward", "70000 10.0.0.5:80")).is_none());
        // Valor vacío.
        assert!(desde_directiva(&directiva("LocalForward", "")).is_none());
    }

    #[test]
    fn exporta_la_linea_de_cada_tipo() {
        assert_eq!(
            a_directiva(&tunel(
                TipoTunel::Local,
                "127.0.0.1:5432",
                Some("10.0.0.5:5432")
            ))
            .as_deref(),
            Some("LocalForward 5432 10.0.0.5:5432")
        );
        assert_eq!(
            a_directiva(&tunel(
                TipoTunel::Local,
                "0.0.0.0:8080",
                Some("10.0.0.5:80")
            ))
            .as_deref(),
            Some("LocalForward 0.0.0.0:8080 10.0.0.5:80")
        );
        assert_eq!(
            a_directiva(&tunel(
                TipoTunel::Remoto,
                "127.0.0.1:9000",
                Some("127.0.0.1:9000")
            ))
            .as_deref(),
            Some("RemoteForward 9000 127.0.0.1:9000")
        );
        assert_eq!(
            a_directiva(&tunel(TipoTunel::Dinamico, "127.0.0.1:1080", None)).as_deref(),
            Some("DynamicForward 1080")
        );
        assert_eq!(
            a_directiva(&tunel(TipoTunel::Dinamico, "[::1]:1080", None)).as_deref(),
            Some("DynamicForward [::1]:1080")
        );
    }

    /// `ssh` no admite el puerto 0 en un reenvío local ni dinámico (en remoto sí:
    /// lo elige el host), y una línea inválida en `magi_config` dejaría sin
    /// `ssh` al usuario entero. Esas no se emiten.
    #[test]
    fn no_exporta_lo_que_ssh_rechazaria() {
        assert!(a_directiva(&tunel(
            TipoTunel::Local,
            "127.0.0.1:0",
            Some("10.0.0.5:5432")
        ))
        .is_none());
        assert!(a_directiva(&tunel(TipoTunel::Dinamico, "127.0.0.1:0", None)).is_none());
        assert!(a_directiva(&tunel(TipoTunel::Dinamico, "0.0.0.0:0", None)).is_none());
        // El remoto con 0 sí vale: el host elige el puerto.
        assert_eq!(
            a_directiva(&tunel(
                TipoTunel::Remoto,
                "127.0.0.1:0",
                Some("127.0.0.1:9000")
            ))
            .as_deref(),
            Some("RemoteForward 0 127.0.0.1:9000")
        );
        // Un local o remoto sin destino tampoco es una directiva válida.
        assert!(a_directiva(&tunel(TipoTunel::Local, "127.0.0.1:5432", None)).is_none());
    }

    #[test]
    fn solo_son_gestionados_los_reenvios_que_sabemos_representar() {
        assert!(reenvio_gestionado("LocalForward 5432 10.0.0.5:5432"));
        assert!(reenvio_gestionado("dynamicforward 1080"));
        // Formas que `ssh` admite y MAGI no representa: siguen valiendo como
        // opciones extra.
        assert!(!reenvio_gestionado("LocalForward 5432"));
        assert!(!reenvio_gestionado("LocalForward 8080 a:80 b:80"));
        assert!(!reenvio_gestionado("LocalForward 7000 /var/run/ssh.sock"));
        assert!(!reenvio_gestionado("ForwardAgent yes"));
        assert!(!reenvio_gestionado("# LocalForward 5432 10.0.0.5:5432"));
    }

    #[test]
    fn ida_y_vuelta_en_los_tres_tipos() {
        let casos = [
            (TipoTunel::Local, "127.0.0.1:5432", Some("10.0.0.5:5432")),
            (TipoTunel::Local, "0.0.0.0:8080", Some("10.0.0.5:80")),
            (TipoTunel::Local, "[::1]:6000", Some("[2001:db8::1]:6000")),
            (TipoTunel::Remoto, "127.0.0.1:9000", Some("127.0.0.1:9000")),
            (TipoTunel::Remoto, "0.0.0.0:9100", Some("10.0.0.5:9100")),
            (TipoTunel::Dinamico, "127.0.0.1:1080", None),
            (TipoTunel::Dinamico, "0.0.0.0:1080", None),
        ];
        for (tipo, escucha, destino) in casos {
            let original = tunel(tipo, escucha, destino);
            let linea = a_directiva(&original)
                .unwrap_or_else(|| panic!("«{escucha}» debería poder escribirse en ssh_config"));
            let partes: Vec<&str> = linea.split_whitespace().collect();
            let ida = desde_directiva(&directiva(partes[0], &partes[1..].join(" ")))
                .unwrap_or_else(|| panic!("no se entendió «{linea}»"));
            assert_eq!(ida.tipo, tipo, "tipo de «{linea}»");
            assert_eq!(ida.escucha, escucha, "escucha de «{linea}»");
            assert_eq!(ida.destino.as_deref(), destino, "destino de «{linea}»");
            assert!(ida.automatico);
            // Y el nombre vuelve a salir del tipo y el puerto de la escucha.
            assert_eq!(ida.nombre, nombre_por_defecto(tipo, escucha));
        }
    }
}
