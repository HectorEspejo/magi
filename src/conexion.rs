//! Puente del escritorio con la conexión SSH del núcleo: traduce los eventos
//! de la conexión al evento de la TUI y aporta el usuario local del proceso.

use std::sync::OnceLock;

use tokio::sync::mpsc;

pub use magi_nucleo::conexion::*;

use crate::app::Evento;

/// Nombre del usuario local, para las conexiones sin usuario en la ficha.
///
/// En el escritorio de siempre es `USER`, o `USERNAME` si no está, o `root`: no
/// se consulta nada más (una variable vacía vale tal cual). En Termux, que no
/// pone `USER`, una variable vacía cuenta como no puesta y, si el entorno no
/// dice nada, sale el nombre del usuario del uid (del estilo `u0_a123`); `root`
/// como último recurso.
pub fn usuario_local() -> String {
    usuario_local_con(
        crate::termux::es_termux(),
        std::env::var("USER").ok(),
        std::env::var("USERNAME").ok(),
        usuario_del_uid,
    )
}

/// La lógica de [`usuario_local`] sobre valores dados, para probar los dos
/// caminos sin tocar el entorno del proceso. `uid` solo se llama en Termux y
/// solo si ninguna variable sirve.
fn usuario_local_con(
    termux: bool,
    user: Option<String>,
    username: Option<String>,
    uid: impl FnOnce() -> Option<String>,
) -> String {
    let ultimo_recurso = || "root".to_string();
    if !termux {
        // Como siempre: la primera variable puesta, aunque esté vacía, y sin
        // preguntar al sistema (`getpwuid_r` puede ir a NSS de red).
        return user.or(username).unwrap_or_else(ultimo_recurso);
    }
    [user, username]
        .into_iter()
        .flatten()
        .find(|valor| !valor.is_empty())
        .or_else(uid)
        .unwrap_or_else(ultimo_recurso)
}

/// El nombre del usuario del uid del proceso, calculado una sola vez: la
/// consulta (`getpwuid_r`) puede bloquear y `usuario_local` se llama desde el
/// bucle de la interfaz. Solo lo usa la rama de Termux.
fn usuario_del_uid() -> Option<String> {
    static USUARIO: OnceLock<Option<String>> = OnceLock::new();
    USUARIO
        .get_or_init(|| {
            nix::unistd::User::from_uid(nix::unistd::Uid::current())
                .ok()
                .flatten()
                .map(|usuario| usuario.name)
                .filter(|nombre| !nombre.is_empty())
        })
        .clone()
}

/// Lanza la tarea tokio de la sesión y devuelve el canal de comandos.
pub fn lanzar(
    runtime: &tokio::runtime::Runtime,
    plan: PlanConexion,
    tx: mpsc::UnboundedSender<Evento>,
) -> mpsc::UnboundedSender<ComandoConexion> {
    let (tx_eventos, mut rx_eventos) = mpsc::unbounded_channel();
    let comandos = magi_nucleo::conexion::lanzar(runtime, plan, tx_eventos);
    runtime.spawn(async move {
        while let Some(evento) = rx_eventos.recv().await {
            if tx.send(Evento::Conexion(evento)).is_err() {
                break;
            }
        }
    });
    comandos
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::cell::Cell;

    fn texto(valor: &str) -> Option<String> {
        Some(valor.to_string())
    }

    /// Un proveedor del uid que cuenta cuántas veces se le pregunta.
    fn uid_contado<'a>(
        llamadas: &'a Cell<u32>,
        nombre: Option<&str>,
    ) -> impl FnOnce() -> Option<String> + 'a {
        let nombre = nombre.map(str::to_string);
        move || {
            llamadas.set(llamadas.get() + 1);
            nombre
        }
    }

    #[test]
    fn fuera_de_termux_es_el_comportamiento_de_siempre() {
        let llamadas = Cell::new(0);
        // USER manda; si no está, USERNAME; si no, root.
        let con = |user, username| {
            usuario_local_con(
                false,
                user,
                username,
                uid_contado(&llamadas, Some("del_uid")),
            )
        };
        assert_eq!(con(texto("usuario"), texto("otro")), "usuario");
        assert_eq!(con(None, texto("otro")), "otro");
        assert_eq!(con(None, None), "root", "sin variables, root (no el uid)");
        // Una variable vacía se devuelve tal cual, como antes.
        assert_eq!(con(texto(""), texto("otro")), "");
        assert_eq!(con(None, texto("")), "");
        // Nunca se consulta el uid: getpwuid_r puede bloquear en NSS de red.
        assert_eq!(llamadas.get(), 0);
    }

    #[test]
    fn en_termux_una_variable_vacia_cuenta_como_no_puesta_y_cae_al_uid() {
        let llamadas = Cell::new(0);
        let con = |user, username, uid: Option<&str>| {
            usuario_local_con(true, user, username, uid_contado(&llamadas, uid))
        };
        assert_eq!(con(texto("usuario"), texto("otro"), None), "usuario");
        assert_eq!(con(None, texto("otro"), None), "otro");
        assert_eq!(con(texto(""), texto("otro"), None), "otro");
        assert_eq!(llamadas.get(), 0, "con una variable válida no se pregunta");

        assert_eq!(con(None, None, Some("u0_a123")), "u0_a123");
        assert_eq!(con(texto(""), texto(""), Some("u0_a123")), "u0_a123");
        assert_eq!(llamadas.get(), 2);
        // Sin variables ni uid conocido, root.
        assert_eq!(con(None, None, None), "root");
    }

    #[test]
    fn el_uid_es_el_mismo_en_cada_llamada() {
        // El valor sale del `OnceLock`: se calcula una vez y se reparte.
        let primero = usuario_del_uid();
        assert_eq!(usuario_del_uid(), primero);
        assert!(primero.is_none_or(|nombre| !nombre.is_empty()));
    }
}
