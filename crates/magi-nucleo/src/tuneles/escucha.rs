//! Escuchas de los túneles: partir la dirección, comprobar el resultado del
//! bind y, en el móvil, forzar el loopback y ofrecer un puerto libre.

use tokio::net::TcpListener;

use crate::modelo::TipoTunel;

pub fn partir_escucha(escucha: &str) -> Result<(String, u16), String> {
    crate::modelo::partir_direccion_puerto(escucha)
        .ok_or_else(|| format!("la escucha «{escucha}» no es válida"))
}

pub fn escucha_de(listener: &TcpListener, direccion: &str) -> Result<String, String> {
    let puerto = listener
        .local_addr()
        .map(|direccion| direccion.port())
        .map_err(|error| format!("no se pudo leer el puerto: {error}"))?;
    Ok(direccion_y_puerto(direccion, u32::from(puerto)))
}

pub fn direccion_y_puerto(direccion: &str, puerto: u32) -> String {
    crate::modelo::juntar_direccion_puerto(direccion, puerto.min(u32::from(u16::MAX)) as u16)
}

/// Motivo legible para un bind que falla, con lo que el usuario puede hacer.
pub fn bind_fallido(escucha: &str, error: &std::io::Error) -> String {
    let puerto = crate::modelo::partir_direccion_puerto(escucha)
        .map(|(_, puerto)| puerto)
        .unwrap_or_default();
    match error.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("el puerto {puerto} ya está en uso en esta máquina")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("no hay permiso para escuchar en el puerto {puerto}")
        }
        _ => format!("no se pudo escuchar en {escucha}: {error}"),
    }
}

// ---------------------------------------------------------------- móvil

/// Escucha que de verdad usa el móvil: los túneles locales y dinámicos
/// escuchan siempre en 127.0.0.1, diga lo que diga el inventario, con aviso
/// si la dirección era otra (T35). En remoto escucha el host y no se toca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscuchaMovil {
    pub direccion: String,
    pub puerto: u16,
    /// El inventario pedía escuchar en otra dirección y se ha forzado el
    /// loopback.
    pub aviso: bool,
}

impl EscuchaMovil {
    /// Texto de la escucha efectiva (127.0.0.1:puerto o [::]:puerto).
    pub fn texto(&self) -> String {
        direccion_y_puerto(&self.direccion, u32::from(self.puerto))
    }
}

/// Normaliza la escucha de un túnel para el móvil.
pub fn escucha_movil(tipo: TipoTunel, escucha: &str) -> Result<EscuchaMovil, String> {
    let (direccion, puerto) = partir_escucha(escucha)?;
    if tipo == TipoTunel::Remoto {
        return Ok(EscuchaMovil {
            direccion,
            puerto,
            aviso: false,
        });
    }
    let local = direccion == "127.0.0.1" || direccion == "localhost";
    Ok(EscuchaMovil {
        direccion: "127.0.0.1".to_string(),
        puerto,
        aviso: !local,
    })
}

/// En el móvil no se intenta escuchar por debajo del puerto 1024: Android no
/// lo permite a las apps. El puerto 0 sigue siendo válido (el sistema elige).
pub fn validar_puerto_movil(puerto: u16) -> Result<(), String> {
    if puerto != 0 && puerto < 1024 {
        return Err(format!(
            "el puerto {puerto} está reservado en Android; abre el túnel en un puerto libre"
        ));
    }
    Ok(())
}

/// Abre una escucha en el loopback con el puerto pedido (0 = el que libre el
/// sistema) y devuelve el listener con su texto efectivo.
pub async fn abrir_escucha_movil(
    direccion: &str,
    puerto: u16,
) -> Result<(TcpListener, String), String> {
    validar_puerto_movil(puerto)?;
    let listener = TcpListener::bind((direccion, puerto))
        .await
        .map_err(|error| bind_fallido(&direccion_y_puerto(direccion, u32::from(puerto)), &error))?;
    let efectiva = escucha_de(&listener, direccion)?;
    Ok((listener, efectiva))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_movil_fuerza_loopback_en_local_y_dinamico() {
        let local = escucha_movil(TipoTunel::Local, "0.0.0.0:8080").expect("escucha válida");
        assert_eq!(local.direccion, "127.0.0.1");
        assert_eq!(local.puerto, 8080);
        assert!(local.aviso);
        assert_eq!(local.texto(), "127.0.0.1:8080");

        let dinamico =
            escucha_movil(TipoTunel::Dinamico, "localhost:1080").expect("escucha válida");
        assert_eq!(dinamico.direccion, "127.0.0.1");
        assert!(!dinamico.aviso);
    }

    #[test]
    fn un_tunel_remoto_conserva_la_escucha_del_host() {
        let remoto = escucha_movil(TipoTunel::Remoto, "0.0.0.0:9000").expect("escucha válida");
        assert_eq!(remoto.direccion, "0.0.0.0");
        assert_eq!(remoto.puerto, 9000);
        assert!(!remoto.aviso);
    }

    #[test]
    fn android_no_deja_escuchar_por_debajo_de_1024_pero_acepta_el_puerto_libre() {
        assert!(validar_puerto_movil(0).is_ok());
        assert!(validar_puerto_movil(1024).is_ok());
        let error = validar_puerto_movil(80).expect_err("80 está reservado");
        assert!(error.contains("reservado en Android"), "{error}");
    }

    #[tokio::test]
    async fn abrir_en_puerto_libre_devuelve_el_puerto_asignado() {
        let (listener, efectiva) = abrir_escucha_movil("127.0.0.1", 0)
            .await
            .expect("escucha en puerto libre");
        let (_, puerto) = partir_escucha(&efectiva).expect("escucha válida");
        assert!(puerto > 0, "el sistema debe asignar un puerto: {efectiva}");
        assert_eq!(
            listener.local_addr().expect("dirección local").port(),
            puerto
        );
    }
}
