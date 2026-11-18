//! Vigilante de las sesiones persistentes (Fase 16, §4.5): comprueba en el host
//! las sesiones de tmux de MAGI, difunde su estado a todas las ventanas y
//! atiende la vista previa y el renombrado.
//!
//! - **Vigilante.** Solo trabaja mientras haya alguna ventana conectada (un
//!   cliente puntual de la CLI cuenta) o, desde la Fase 17, en Termux con los
//!   avisos de Android activos, Termux:API sin apagar y `termux-notification`
//!   instalada: entonces sigue sin ventanas mientras viva el servidor, para
//!   poder avisar de una atención (`debe_vigilar`), salvo que lo único que
//!   mantenga vivo al servidor sean pestañas caídas que nadie ha cerrado. No
//!   mantiene vivo al servidor. Cada `[persistentes]
//!   vigilar_seg`, y
//!   enseguida con `ConsultarSesionesPersistentes`, comprueba con la suborden
//!   `estado` los hosts `ssh` que tienen fichas o la marca
//!   `sesiones_persistentes`: 8 a la vez como mucho y 10 s por host, impuestos
//!   desde fuera. Es una operación automática: nunca abre un diálogo (política
//!   `NoInteractiva` sin solicitante) y nunca toca el llavero. Un host que
//!   pediría una huella, una frase o una contraseña, o que no responde, queda
//!   con su error y sus fichas `inalcanzable`: jamás `terminada` por no poder
//!   mirar (§7.3).
//! - **Estado.** Vive solo en la memoria del servidor: por host, la última
//!   comprobación; por ficha, su estado derivado. `SesionesPersistentes{lista}`
//!   se difunde a todas las ventanas solo cuando algo cambia (las marcas de
//!   tiempo de las comprobaciones no cuentan), o siempre al acabar una consulta
//!   pedida por una ventana (es su respuesta); una ventana nueva recibe la
//!   lista actual detrás de la `Bienvenida`.
//! - **Consulta.** Las peticiones seguidas se agrupan en una sola comprobación
//!   y un host comprobado hace menos de 5 s no se repite.
//! - **Vista previa y renombrar.** Responden solo a la ventana que los pide.
//!   La vista previa no se guarda en ningún sitio. Renombrar cambia la sesión
//!   en el host; la ficha la cambia después el cliente.
//!
//! El servidor lee `SESIONES_PERSISTENTES` y `HOSTS` por su conexión de
//! lectura (validando cada fila, saltando las ilegibles con un aviso que no se
//! repite) y no escribe nunca en esas tablas ni en `REGISTRO` por esto. Ni la
//! salida de las sesiones, ni sus títulos, ni las vistas previas van al log.
//!
//! Toda conexión sale de `conexion_para_canal` y se suelta por identidad en
//! todas las ramas, también cuando salta el plazo o falla el canal.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt as _;
use russh::client::Handle;
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::{debug, warn};

use magi_nucleo::almacen::sesiones_persistentes::{leer_para_vigilante, LecturaVigilante};
use magi_nucleo::almacen::Almacen;
use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::sesiones_persistentes::estado::{estado_visto, sesion_de};
use magi_nucleo::sesiones_persistentes::lectura::{
    self, ResultadoRenombrar as LecturaRenombrar, MAXIMO_SALIDA_ESTADO,
};
use magi_nucleo::sesiones_persistentes::Ficha;
use magi_nucleo::terminal::tmux::{
    self, OrdenTmux, SALIDA_ANTIGUO, SALIDA_NO_EXISTE, SALIDA_SIN_TMUX,
};

use crate::conexion::cliente::Cliente;
use crate::config::Rutas;
use crate::modelo::Host;
use crate::protocolo::{
    ComprobacionHost, ErrorComprobacion, EstadoFicha, FichaVista, HostVisto, ListaPersistentes,
    MensajeServidor, ResultadoRenombrar, ServidorTmux,
};

use super::conexiones::{self, PoliticaConexion, UsoCanal};
use super::{difusion, EstadoServidor};

/// Estado compartido del servidor, para no repetir el tipo en cada firma.
type EstadoCompartido = Arc<Mutex<EstadoServidor>>;

// ---------------------------------------------------------------- constantes

/// Plazo total de la comprobación de un host (§4.5, T32): conectar, abrir el
/// canal y ejecutar `estado`. Se impone desde fuera: el de
/// `conexion_para_canal` (30 s) es mayor.
pub const PLAZO_HOST: Duration = Duration::from_secs(10);

/// Parte del plazo de un host que puede gastar la conexión; el resto queda para
/// abrir el canal y ejecutar la orden. Con la conexión del pool ya viva, la
/// orden dispone de todo el plazo.
pub const PLAZO_CONEXION: Duration = Duration::from_secs(7);

/// Plazo para conectar en una vista previa o un renombrado, que lo pide una
/// ventana: la orden lleva además el suyo (`OrdenTmux::plazo`).
pub const PLAZO_CONEXION_PETICION: Duration = Duration::from_secs(7);

/// Hosts que se comprueban a la vez (§8, rendimiento).
pub const SIMULTANEOS: usize = 8;

/// Un host comprobado hace menos que esto no se repite, salvo que un
/// renombrado lo obligue.
pub const REPETICION_MINIMA: Duration = Duration::from_secs(5);

/// Cuánto se espera, tras una consulta, a que lleguen las que van detrás para
/// atenderlas todas con una sola comprobación.
const AGRUPAR: Duration = Duration::from_millis(100);

/// Cuánto sigue un host pedido a mano (`host_id` de la consulta) en la lista
/// sin tener fichas ni marca.
const VIGENCIA_PUNTUAL: Duration = Duration::from_secs(10 * 60);

/// Hosts pedidos a mano que se recuerdan a la vez (la entrada es de un
/// cliente: no crece sin tope).
const MAXIMO_PUNTUALES: usize = 64;

/// Vistas previas y renombrados en vuelo a la vez entre todas las ventanas.
const MAXIMO_EN_VUELO: usize = 32;

/// Plazo de cerrar un canal exec: solo corta el flujo, pero el host puede no
/// contestar nunca.
const PLAZO_CIERRE_CANAL: Duration = Duration::from_secs(2);

/// Tope de lo que se guarda de la salida de una orden (entrada no fiable).
const TOPE_SALIDA: usize = MAXIMO_SALIDA_ESTADO + 4096;

/// Lo más que ocupa codificada la lista de `SesionesPersistentes`: el códec
/// admite 4 MiB por mensaje.
const TOPE_LISTA: usize = 3 * 1024 * 1024;

/// Sesiones sin ficha y del tmux habitual que se conservan por host si la lista
/// no cabe en el tope.
const SESIONES_POR_HOST_ACOTADAS: usize = 50;

// ------------------------------------------------------------------- estado

/// Lo que sabe el servidor de las sesiones persistentes. Todo en memoria: ni a
/// disco, ni al log, ni a `REGISTRO`.
#[derive(Default)]
pub struct Persistentes {
    /// Última comprobación de cada host vigilado o pedido.
    comprobaciones: HashMap<i64, Comprobado>,
    /// Fichas de la última lectura, para armar la lista.
    fichas: Vec<Ficha>,
    /// La lista más reciente, con sus marcas de tiempo: la que recibe una
    /// ventana nueva.
    lista: ListaPersistentes,
    /// La última lista difundida, sin marcas de tiempo, para difundir solo
    /// cuando cambia algo de verdad.
    difundida: ListaPersistentes,
    /// Alguna ventana ha pedido una comprobación que aún no se ha atendido.
    consulta: bool,
    /// Hosts pedidos a mano en una consulta, con el momento de la petición.
    puntuales: HashMap<i64, Instant>,
    /// Hosts que hay que volver a mirar aunque se comprobaran hace poco (tras
    /// renombrar una sesión), con el momento en que se pidió: una comprobación
    /// que empezó antes puede traer el nombre viejo y no cuenta como la nueva
    /// mirada.
    recomprobar: HashMap<i64, Instant>,
    /// Vistas previas y renombrados en vuelo, por (ventana, petición).
    en_vuelo: HashSet<(u32, u64)>,
    /// Filas ilegibles de las que ya se avisó en el log (una vez por motivo).
    avisadas: HashSet<String>,
    /// Despierta al vigilante cuando llega una consulta.
    despertar: Arc<Notify>,
}

impl Persistentes {
    /// La lista más reciente (la que se difunde), para quien necesita saber
    /// qué fichas están vivas y con la campana (los avisos de Android).
    pub fn lista_actual(&self) -> &ListaPersistentes {
        &self.lista
    }

    /// De los motivos de filas ilegibles de esta lectura, los que aún no se han
    /// avisado en el log. Olvida los que ya no están (la fila se arregló): si
    /// vuelven, se avisa de nuevo.
    fn motivos_sin_avisar(&mut self, motivos: &[String]) -> Vec<String> {
        self.avisadas.retain(|motivo| motivos.contains(motivo));
        motivos
            .iter()
            .filter(|motivo| self.avisadas.insert((*motivo).clone()))
            .cloned()
            .collect()
    }
}

/// La comprobación de un host y cuándo se hizo.
struct Comprobado {
    datos: ComprobacionHost,
    /// Para la regla de los 5 s.
    cuando: Instant,
    /// Para las marcas de la lista (segundos Unix).
    epoca: i64,
}

/// La lista actual para una ventana que acaba de conectarse. `None` si todavía
/// no hay nada que contar: la ventana arranca con la lista vacía.
pub fn lista_para_ventana_nueva(estado: &EstadoServidor) -> Option<ListaPersistentes> {
    let lista = &estado.persistentes.lista;
    (!lista.fichas.is_empty() || !lista.hosts.is_empty()).then(|| lista.clone())
}

/// ¿Debe trabajar el vigilante? Con alguna ventana conectada o, en Termux con
/// avisos de Android, Termux:API sin apagar y `termux-notification` instalada
/// (`TermuxServidor::vigila_sin_ventanas`), aunque no haya ninguna mientras
/// viva el servidor (§4.7), salvo que lo único que lo mantenga vivo sean
/// pestañas caídas que el usuario no ha cerrado (corrección de cierre A1: el
/// mismo criterio que el wake lock, `EstadoServidor::trabajo_vivo`; una
/// pestaña muerta no puede tener al servidor sondeando hosts por SSH sin
/// ventanas para siempre). Sin el paquete `termux-api` sigue como en el
/// escritorio. Es el único sitio que lo decide: no cambia cuándo se apaga el
/// servidor (`revisar_inactividad` no lo mira).
pub fn debe_vigilar(estado: &EstadoServidor) -> bool {
    !estado.clientes.is_empty()
        || estado.termux.as_ref().is_some_and(|termux| {
            termux.vigila_sin_ventanas(&estado.config) && !estado.solo_mantienen_pestanas_caidas()
        })
}

// --------------------------------------------------------------- lógica pura

/// ¿Toca comprobar este host? Nunca comprobado, o comprobado hace
/// `REPETICION_MINIMA` o más, o una orden de volver a mirarlo.
fn debe_comprobarse(
    ultima: Option<Instant>,
    ahora: Instant,
    repeticion_minima: Duration,
    forzado: bool,
) -> bool {
    forzado
        || ultima.is_none_or(|ultima| ahora.saturating_duration_since(ultima) >= repeticion_minima)
}

/// ¿Es la comprobación que empezó en `inicio` anterior a la orden de volver a
/// mirar el host (`marca`, la hora en que se renombró una sesión)? Su resultado
/// puede traer el nombre viejo: no se aplica ni da por hecha la nueva mirada.
fn es_anterior_a_la_marca(marca: Option<Instant>, inicio: Instant) -> bool {
    marca.is_some_and(|marca| inicio < marca)
}

/// Arma la lista a partir de las fichas y de la última comprobación de cada
/// host. El estado de cada ficha es el derivado de §7.3 (`estado_visto`); un
/// host sin comprobar deja sus fichas `sin_comprobar`.
fn construir_lista(
    fichas: &[Ficha],
    comprobaciones: &HashMap<i64, Comprobado>,
) -> ListaPersistentes {
    let fichas_vistas = fichas
        .iter()
        .map(|ficha| {
            let comprobado = comprobaciones.get(&ficha.host_id);
            let datos = comprobado.map(|comprobado| &comprobado.datos);
            let estado = estado_visto(ficha, datos);
            let sesion = match (estado, datos) {
                (EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada, Some(host)) => {
                    sesion_de(&ficha.nombre, host).cloned()
                }
                _ => None,
            };
            FichaVista {
                uuid: ficha.uuid.clone(),
                host_id: ficha.host_id,
                nombre: ficha.nombre.clone(),
                estado,
                sesion,
                comprobada_en: comprobado.map_or(0, |comprobado| comprobado.epoca),
            }
        })
        .collect();

    let mut ids: Vec<i64> = comprobaciones.keys().copied().collect();
    ids.sort_unstable();
    let hosts = ids
        .into_iter()
        .filter_map(|id| {
            let comprobado = comprobaciones.get(&id)?;
            // Una sesión del servidor propio es «sin ficha» si ninguna ficha
            // del host se llama así (nombre exacto: tmux casa prefijos, aquí
            // nunca).
            let con_ficha: HashSet<&str> = fichas
                .iter()
                .filter(|ficha| ficha.host_id == id)
                .map(|ficha| ficha.nombre.as_str())
                .collect();
            let sesiones = &comprobado.datos.sesiones;
            Some(HostVisto {
                host_id: id,
                comprobada_en: comprobado.epoca,
                ahora: comprobado.datos.ahora,
                tmux: comprobado.datos.tmux,
                error: comprobado.datos.error,
                sin_ficha: sesiones
                    .iter()
                    .filter(|sesion| {
                        sesion.servidor == ServidorTmux::Magi
                            && !con_ficha.contains(sesion.nombre.as_str())
                    })
                    .cloned()
                    .collect(),
                habitual: sesiones
                    .iter()
                    .filter(|sesion| sesion.servidor == ServidorTmux::Habitual)
                    .cloned()
                    .collect(),
            })
        })
        .collect();

    acotar_lista(ListaPersistentes {
        fichas: fichas_vistas,
        hosts,
    })
}

/// Recorta la lista si codificada pasaría de `TOPE_LISTA`: ningún mensaje del
/// protocolo puede pasar del tamaño del códec. Primero se acortan las listas
/// de sesiones de cada host y, si aun así no cabe, se quitan.
fn acotar_lista(mut lista: ListaPersistentes) -> ListaPersistentes {
    let tamano =
        |lista: &ListaPersistentes| serde_json::to_vec(lista).map_or(0, |bytes| bytes.len());
    if tamano(&lista) <= TOPE_LISTA {
        return lista;
    }
    for host in &mut lista.hosts {
        host.sin_ficha.truncate(SESIONES_POR_HOST_ACOTADAS);
        host.habitual.truncate(SESIONES_POR_HOST_ACOTADAS);
    }
    if tamano(&lista) <= TOPE_LISTA {
        return lista;
    }
    for host in &mut lista.hosts {
        host.sin_ficha.clear();
        host.habitual.clear();
    }
    while tamano(&lista) > TOPE_LISTA && !lista.fichas.is_empty() {
        let mitad = lista.fichas.len() / 2;
        lista.fichas.truncate(mitad);
    }
    lista
}

/// La lista sin las marcas de tiempo (`comprobada_en` y `ahora`): lo único que
/// cambia en cada comprobación aunque nada haya cambiado en el host.
fn sin_marcas(lista: &ListaPersistentes) -> ListaPersistentes {
    let mut copia = lista.clone();
    for ficha in &mut copia.fichas {
        ficha.comprobada_en = 0;
    }
    for host in &mut copia.hosts {
        host.comprobada_en = 0;
        host.ahora = 0;
    }
    copia
}

/// Por qué no se pudo mirar un host, según el texto del error de conexión: lo
/// que pediría un diálogo (huella, frase, contraseña, una decisión pendiente
/// del usuario), un plazo vencido, o simplemente que no responde. Los nombres
/// entre «» (los del inventario) no cuentan.
fn clasificar_error(motivo: &str) -> ErrorComprobacion {
    let mut fuera = String::with_capacity(motivo.len());
    let mut dentro = false;
    for caracter in motivo.chars() {
        match caracter {
            '«' => dentro = true,
            '»' => dentro = false,
            _ if !dentro => fuera.push(caracter),
            _ => {}
        }
    }
    let texto = fuera.to_lowercase();
    const DIALOGO: [&str; 9] = [
        "conéctate una vez",
        "no dialoga",
        "no hay diálogo",
        "huella",
        "frase",
        "contraseña",
        "llavero",
        "decisión del usuario",
        "agente ssh",
    ];
    if DIALOGO.iter().any(|clave| texto.contains(clave)) {
        return ErrorComprobacion::NecesitaDialogo;
    }
    if texto.contains("plazo") || texto.contains("timeout") || texto.contains("a tiempo") {
        return ErrorComprobacion::Plazo;
    }
    ErrorComprobacion::SinRespuesta
}

// ------------------------------------------------------- ejecución en el host

/// Por qué falló una orden remota.
#[derive(Debug)]
enum ErrorOrden {
    /// `conexion_para_canal` no dio conexión (con el motivo).
    Conexion(String),
    /// Se acabó el plazo de la conexión.
    PlazoConexion,
    /// El canal no se abrió o falló (con el motivo).
    Canal(String),
    /// Se acabó el plazo de la orden.
    PlazoOrden,
}

impl ErrorOrden {
    /// El motivo que ve el usuario (nunca lleva salida del host).
    fn motivo(&self, host: &str) -> String {
        match self {
            ErrorOrden::Conexion(motivo) | ErrorOrden::Canal(motivo) => motivo.clone(),
            ErrorOrden::PlazoConexion => format!("«{host}» no respondió a tiempo"),
            ErrorOrden::PlazoOrden => format!("«{host}» no contestó a tiempo"),
        }
    }

    /// Cómo cuenta para el vigilante.
    fn como_error_de_comprobacion(&self) -> ErrorComprobacion {
        match self {
            ErrorOrden::Conexion(motivo) => clasificar_error(motivo),
            ErrorOrden::PlazoConexion | ErrorOrden::PlazoOrden => ErrorComprobacion::Plazo,
            ErrorOrden::Canal(_) => ErrorComprobacion::SinRespuesta,
        }
    }
}

/// Ejecuta una orden del script por un canal `exec` de la conexión con un
/// límite de tiempo que cubre también la apertura del canal. Devuelve la salida
/// (stdout y stderr juntos, con tope) y el código de salida si el host lo dio.
/// El canal se cierra siempre.
async fn ejecutar_con_limite(
    handle: &Arc<Handle<Cliente>>,
    argumentos: &[String],
    limite: Instant,
) -> Result<(Vec<u8>, Option<i32>), ErrorOrden> {
    let comando = argumentos
        .iter()
        .map(|argumento| shell_escape::unix::escape(argumento.as_str().into()).to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let apertura = handle.clone();
    let restante = limite.saturating_duration_since(Instant::now());
    let mut canal =
        conexiones::abrir_con_plazo(
            restante,
            async move { apertura.channel_open_session().await },
        )
        .await
        .map_err(|motivo| {
            if Instant::now() >= limite {
                ErrorOrden::PlazoOrden
            } else {
                ErrorOrden::Canal(motivo)
            }
        })?;
    let mut salida: Vec<u8> = Vec::new();
    let mut codigo = None;
    let trabajo = async {
        canal
            .exec(true, comando.into_bytes())
            .await
            .map_err(|error| error.to_string())?;
        let _ = canal.eof().await;
        loop {
            match canal.wait().await {
                Some(russh::ChannelMsg::Data { data })
                | Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                    if salida.len() < TOPE_SALIDA {
                        let sitio = TOPE_SALIDA - salida.len();
                        salida.extend_from_slice(&data[..data.len().min(sitio)]);
                    }
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    codigo = i32::try_from(exit_status).ok();
                }
                Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok::<(), String>(())
    };
    let resultado = tokio::time::timeout_at(limite, trabajo).await;
    // El canal se cierra pase lo que pase: en una conexión compartida, uno
    // abierto de más agotaría `MaxSessions`.
    let _ = tokio::time::timeout(PLAZO_CIERRE_CANAL, canal.close()).await;
    match resultado {
        Err(_) => Err(ErrorOrden::PlazoOrden),
        Ok(Err(motivo)) => Err(ErrorOrden::Canal(motivo)),
        Ok(Ok(())) => Ok((salida, codigo)),
    }
}

/// Toma una conexión (la del pool o una nueva, según la política), ejecuta la
/// orden y la suelta. `limite_conexion` acota el tiempo de conseguirla;
/// `limite_duro`, si lo hay, el de todo. La conexión se suelta siempre, por
/// identidad, también si salta un plazo o falla el canal.
#[allow(clippy::too_many_arguments)]
async fn ejecutar_orden_en_host(
    estado: &EstadoCompartido,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &Rutas,
    politica: PoliticaConexion,
    argumentos: &[String],
    limite_conexion: Instant,
    limite_duro: Option<Instant>,
    plazo_orden: Duration,
) -> Result<(Vec<u8>, Option<i32>), ErrorOrden> {
    let conexion = conexiones::conexion_para_canal(
        estado,
        host,
        todos_los_hosts,
        rutas,
        politica,
        UsoCanal::Subsistema,
    );
    let tomada = match tokio::time::timeout_at(limite_conexion, conexion).await {
        Ok(Ok(tomada)) => tomada,
        Ok(Err(motivo)) => return Err(ErrorOrden::Conexion(motivo)),
        Err(_) => return Err(ErrorOrden::PlazoConexion),
    };
    let mut limite = Instant::now() + plazo_orden;
    if let Some(duro) = limite_duro {
        limite = limite.min(duro);
    }
    let resultado = ejecutar_con_limite(&tomada.handle, argumentos, limite).await;
    // Toma, usa y suelta: nunca se retiene una conexión del pool más de lo
    // necesario.
    tomada.soltar().await;
    resultado
}

/// Un id para los diálogos de una conexión nueva. Sale del contador de las
/// sesiones, así que nunca choca con el de una pestaña.
async fn nueva_solicitud(estado: &EstadoCompartido) -> u32 {
    let mut estado_bloqueado = estado.lock().await;
    let id = estado_bloqueado.siguiente_sesion_id;
    estado_bloqueado.siguiente_sesion_id += 1;
    id
}

/// Comprueba un host con `estado`. Sin diálogos y con el plazo de 10 s desde
/// fuera: lo que el host no pueda dar queda como un error de la comprobación,
/// nunca como «sin sesiones».
async fn comprobar_host(
    estado: &EstadoCompartido,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &Rutas,
) -> ComprobacionHost {
    let fallo = |error: ErrorComprobacion| ComprobacionHost {
        host_id: host.id,
        tmux: None,
        ahora: 0,
        sesiones: Vec::new(),
        error: Some(error),
    };
    let Some(argumentos) = OrdenTmux::Estado.argumentos() else {
        return fallo(ErrorComprobacion::SinRespuesta);
    };
    let inicio = Instant::now();
    let id_solicitud = nueva_solicitud(estado).await;
    let politica = PoliticaConexion::NoInteractiva {
        solicitante: None,
        id_solicitud,
    };
    let resultado = ejecutar_orden_en_host(
        estado,
        host,
        todos_los_hosts,
        rutas,
        politica,
        &argumentos,
        inicio + PLAZO_CONEXION,
        Some(inicio + PLAZO_HOST),
        PLAZO_HOST,
    )
    .await;
    match resultado {
        // `estado` sale con 0 cuando ha dicho todo lo que sabe. Con otro código
        // (le falta `awk` al host, se cortó a mitad) o sin decir cómo salió, lo
        // escrito puede ser la versión de tmux y ninguna sesión: sus fichas
        // saldrían «terminadas» estando vivas (y `C` las borraría). Sin una
        // salida completa no se sabe cómo están.
        Ok((_salida, codigo)) if codigo != Some(0) => {
            debug!(host = %host.nombre, "el script de tmux no terminó bien");
            fallo(ErrorComprobacion::SinRespuesta)
        }
        Ok((salida, _codigo)) => match lectura::leer_estado_bytes(&salida) {
            Some(lectura) => ComprobacionHost {
                host_id: host.id,
                tmux: lectura.tmux,
                ahora: lectura.ahora,
                sesiones: lectura.sesiones,
                error: None,
            },
            // El host contestó algo que no es la salida del script: no se sabe
            // cómo están sus sesiones.
            None => fallo(ErrorComprobacion::SinRespuesta),
        },
        Err(error) => {
            debug!(host = %host.nombre, "no se pudo comprobar tmux");
            fallo(error.como_error_de_comprobacion())
        }
    }
}

// ------------------------------------------------------------- el vigilante

/// El vigilante: espera `vigilar_seg` o una consulta y hace una pasada. Se
/// lanza una vez, junto a `tuneles::revisar`.
pub async fn revisar(estado: EstadoCompartido) {
    let despertar = estado.lock().await.persistentes.despertar.clone();
    loop {
        let periodo = estado.lock().await.config.persistentes.periodo_vigilante();
        tokio::select! {
            () = tokio::time::sleep(periodo) => {}
            () = despertar.notified() => {
                // Las consultas que llegan seguidas se atienden juntas.
                tokio::time::sleep(AGRUPAR).await;
                // Y el aviso de las que llegaron mientras tanto ya no hace
                // falta: la bandera las cubre.
                let _ = despertar.notified().now_or_never();
            }
        }
        if estado.lock().await.apagando {
            return;
        }
        pasada(&estado).await;
    }
}

/// Una pasada del vigilante: lee las fichas y los hosts, comprueba los que
/// toca (8 a la vez) y difunde lo que haya cambiado.
async fn pasada(estado: &EstadoCompartido) {
    // Sin ventanas (y sin avisos de Android que dar) no se trabaja: lo
    // pendiente se olvida.
    let (consulta, lectura_bd, rutas, repeticion) = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return;
        }
        let consulta = std::mem::take(&mut estado_bloqueado.persistentes.consulta);
        if !debe_vigilar(&estado_bloqueado) {
            return;
        }
        // Con el ritmo de pruebas (milisegundos) la regla de los 5 s no
        // rige: el propio ritmo separa las comprobaciones.
        let repeticion = if estado_bloqueado.config.persistentes.vigilar_ms_pruebas > 0 {
            Duration::ZERO
        } else {
            REPETICION_MINIMA
        };
        (
            consulta,
            estado_bloqueado.lectura.clone(),
            estado_bloqueado.rutas.clone(),
            repeticion,
        )
    };

    // El almacén se lee sin el mutex del estado en la mano y fuera del
    // runtime: es E/S de SQLite y podría tardar hasta el `busy_timeout`.
    let leida = tokio::task::spawn_blocking(move || {
        let almacen: std::sync::MutexGuard<'_, Almacen> = lectura_bd
            .lock()
            .map_err(|_| "el almacén de lectura está envenenado".to_string())?;
        leer_para_vigilante(almacen.conexion()).map_err(|error| format!("{error:#}"))
    })
    .await;
    let lectura = match leida {
        Ok(Ok(lectura)) => lectura,
        Ok(Err(motivo)) => {
            avisar_una_vez(estado, &format!("no se pudieron leer las fichas: {motivo}")).await;
            // La ventana que preguntó no se queda esperando: recibe la lista que
            // hay (la última que se pudo armar).
            contestar_consulta(estado, consulta).await;
            return;
        }
        Err(_) => {
            contestar_consulta(estado, consulta).await;
            return;
        }
    };
    avisar_ilegibles(estado, &lectura).await;

    let ahora = Instant::now();
    // Los hosts de esta pasada: los vigilados y los pedidos a mano que siguen
    // vigentes y son de terminal.
    let (objetivos, a_comprobar) = {
        let mut estado_bloqueado = estado.lock().await;
        let persistentes = &mut estado_bloqueado.persistentes;
        // Un host pedido a mano que ya no existe o no es de terminal sale al
        // momento: la entrada es de un cliente y no debe ocupar sitio 10 min.
        persistentes.puntuales.retain(|host_id, pedido| {
            ahora.saturating_duration_since(*pedido) < VIGENCIA_PUNTUAL
                && lectura
                    .hosts
                    .get(host_id)
                    .is_some_and(|host| host.tipo == TipoHost::Ssh)
        });
        let mut objetivos: Vec<i64> = lectura.vigilados.clone();
        for host_id in persistentes.puntuales.keys() {
            if !objetivos.contains(host_id) {
                objetivos.push(*host_id);
            }
        }
        objetivos.sort_unstable();
        // Lo que ya no se vigila (la ficha se borró, el host dejó la marca)
        // sale de la lista.
        persistentes
            .comprobaciones
            .retain(|host_id, _| objetivos.contains(host_id));
        persistentes
            .recomprobar
            .retain(|host_id, _| objetivos.contains(host_id));
        persistentes.fichas = lectura.fichas.clone();
        let a_comprobar: Vec<i64> = objetivos
            .iter()
            .copied()
            .filter(|host_id| {
                debe_comprobarse(
                    persistentes
                        .comprobaciones
                        .get(host_id)
                        .map(|comprobado| comprobado.cuando),
                    ahora,
                    repeticion,
                    persistentes.recomprobar.contains_key(host_id),
                )
            })
            .collect();
        // Un cambio de fichas o de hosts se difunde antes de la espera de la
        // red: no hay por qué esperar a un host lento.
        reconstruir_y_difundir(&mut estado_bloqueado, false);
        (objetivos, a_comprobar)
    };
    debug!(
        vigilados = objetivos.len(),
        comprobar = a_comprobar.len(),
        "pasada del vigilante de sesiones persistentes"
    );

    if !a_comprobar.is_empty() {
        let semaforo = Arc::new(Semaphore::new(SIMULTANEOS));
        let todos = Arc::new(lectura.hosts);
        let mut tareas = JoinSet::new();
        for host_id in a_comprobar {
            let Some(host) = todos.get(&host_id).cloned() else {
                continue;
            };
            let estado_tarea = estado.clone();
            let semaforo = semaforo.clone();
            let todos = todos.clone();
            let rutas = rutas.clone();
            tareas.spawn(async move {
                let Ok(_permiso) = semaforo.acquire_owned().await else {
                    return;
                };
                // Las ventanas pudieron irse mientras esperaba su turno.
                if !debe_vigilar(&*estado_tarea.lock().await) {
                    return;
                }
                let inicio = Instant::now();
                let datos = comprobar_host(&estado_tarea, &host, &todos, &rutas).await;
                aplicar_resultado(&estado_tarea, datos, inicio).await;
            });
        }
        while let Some(salida) = tareas.join_next().await {
            if salida.is_err() {
                warn!("una comprobación de sesiones persistentes terminó mal");
            }
        }
    }

    // La consulta de una ventana se contesta siempre difundiendo la lista,
    // también si no hubo nada que comprobar (todo era de hace menos de 5 s).
    contestar_consulta(estado, consulta).await;
}

/// La respuesta a `ConsultarSesionesPersistentes`: la lista, difundida aunque
/// no haya cambiado nada. Se da también cuando la pasada no pudo leer la base.
async fn contestar_consulta(estado: &EstadoCompartido, consulta: bool) {
    if consulta {
        let mut estado_bloqueado = estado.lock().await;
        reconstruir_y_difundir(&mut estado_bloqueado, true);
    }
}

/// Guarda el resultado de un host y difunde la lista si cambió algo. `inicio`
/// es cuándo empezó la comprobación: si es anterior a un renombrado de ese
/// host, su resultado puede traer el nombre viejo (y borraría la orden de
/// volver a mirar), así que se descarta y el host se vuelve a mirar.
async fn aplicar_resultado(estado: &EstadoCompartido, datos: ComprobacionHost, inicio: Instant) {
    let mut estado_bloqueado = estado.lock().await;
    let host_id = datos.host_id;
    let persistentes = &mut estado_bloqueado.persistentes;
    if es_anterior_a_la_marca(persistentes.recomprobar.get(&host_id).copied(), inicio) {
        return;
    }
    persistentes.recomprobar.remove(&host_id);
    persistentes.comprobaciones.insert(
        host_id,
        Comprobado {
            datos,
            cuando: Instant::now(),
            epoca: chrono::Utc::now().timestamp(),
        },
    );
    reconstruir_y_difundir(&mut estado_bloqueado, false);
}

/// Rehace la lista con lo que hay en memoria y la difunde a todas las ventanas
/// si cambió algo (sin contar las marcas de tiempo) o si se fuerza.
fn reconstruir_y_difundir(estado: &mut EstadoServidor, forzar: bool) {
    let lista = construir_lista(
        &estado.persistentes.fichas,
        &estado.persistentes.comprobaciones,
    );
    let clave = sin_marcas(&lista);
    let cambia = clave != estado.persistentes.difundida;
    estado.persistentes.lista = lista.clone();
    if cambia {
        // La atención de una ficha es lo que decide sus avisos de Android.
        if let Some(termux) = &estado.termux {
            termux.despertar_avisos();
        }
    }
    if !(cambia || forzar) {
        return;
    }
    estado.persistentes.difundida = clave;
    if !estado.clientes.is_empty() {
        difusion::difundir(
            &estado.clientes,
            MensajeServidor::SesionesPersistentes { lista },
        );
    }
}

/// Avisa en el log una sola vez de cada fila ilegible (el vigilante lee cada
/// minuto: sin esto, una fila corrupta llenaría el log) y olvida las que ya se
/// arreglaron, para avisar si vuelven. Los motivos nunca llevan comandos ni
/// notas.
async fn avisar_ilegibles(estado: &EstadoCompartido, lectura: &LecturaVigilante) {
    let nuevos = estado
        .lock()
        .await
        .persistentes
        .motivos_sin_avisar(&lectura.ilegibles);
    for motivo in nuevos {
        warn!("sesiones persistentes: se salta una fila ilegible ({motivo})");
    }
}

/// Un aviso del log que no se repite mientras sea el mismo.
async fn avisar_una_vez(estado: &EstadoCompartido, motivo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    if estado_bloqueado
        .persistentes
        .avisadas
        .insert(motivo.to_string())
    {
        warn!("sesiones persistentes: {motivo}");
    }
}

// --------------------------------------------------------------- consulta

/// `ConsultarSesionesPersistentes`: comprobar ya los hosts con fichas o
/// marcados y, si viene `host_id`, además ese host de terminal. Las consultas
/// seguidas se agrupan en una sola pasada (la bandera es única) y la respuesta
/// es la difusión de la lista al acabar.
pub async fn consultar(estado: &EstadoCompartido, cliente_id: u32, host_id: Option<i64>) {
    let mut estado_bloqueado = estado.lock().await;
    // Una ventana que ya se fue no pide nada.
    if estado_bloqueado.apagando || !estado_bloqueado.clientes.contains_key(&cliente_id) {
        return;
    }
    let persistentes = &mut estado_bloqueado.persistentes;
    if let Some(host_id) = host_id.filter(|id| *id > 0) {
        if persistentes.puntuales.len() < MAXIMO_PUNTUALES
            || persistentes.puntuales.contains_key(&host_id)
        {
            persistentes.puntuales.insert(host_id, Instant::now());
        }
    }
    persistentes.consulta = true;
    persistentes.despertar.notify_one();
}

// ------------------------------------------- lo que piden las ventanas a mano

/// Host, su cadena de saltos, las rutas y un id de solicitud, leídos sin el
/// mutex del estado. `None` si el host no existe, no se puede leer o no es de
/// terminal.
async fn datos_de_host(
    estado: &EstadoCompartido,
    host_id: i64,
) -> Result<(Host, HashMap<i64, Host>, Rutas, u32), String> {
    let (lectura, rutas) = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return Err(conexiones::APAGANDO.to_string());
        }
        (
            estado_bloqueado.lectura.clone(),
            estado_bloqueado.rutas.clone(),
        )
    };
    let leido = tokio::task::spawn_blocking(move || {
        let almacen = lectura.lock().ok()?;
        let host = almacen.obtener_host(host_id).ok()?;
        // Solo los saltos de su cadena: los demás hosts no hacen falta.
        let mut saltos = HashMap::new();
        let mut siguiente = host.salto_host_id;
        while let Some(id) = siguiente {
            if saltos.contains_key(&id) || saltos.len() >= 8 {
                break;
            }
            let salto = almacen.obtener_host(id).ok()?;
            siguiente = salto.salto_host_id;
            saltos.insert(id, salto);
        }
        Some((host, saltos))
    })
    .await
    .ok()
    .flatten();
    let Some((host, saltos)) = leido else {
        return Err("el host ya no existe".to_string());
    };
    if host.tipo != TipoHost::Ssh {
        return Err(format!("«{}» no es un host de terminal", host.nombre));
    }
    let id_solicitud = nueva_solicitud(estado).await;
    Ok((host, saltos, rutas, id_solicitud))
}

/// Reserva la petición (`(ventana, peticion_id)`): `false` si ya hay otra igual
/// en vuelo o hay demasiadas.
async fn reservar(estado: &EstadoCompartido, clave: (u32, u64)) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let persistentes = &mut estado_bloqueado.persistentes;
    persistentes.en_vuelo.len() < MAXIMO_EN_VUELO && persistentes.en_vuelo.insert(clave)
}

async fn liberar(estado: &EstadoCompartido, clave: (u32, u64)) {
    estado.lock().await.persistentes.en_vuelo.remove(&clave);
}

/// ¿Sigue conectada la ventana? Antes de abrir nada para ella se comprueba.
async fn sigue_conectada(estado: &EstadoCompartido, cliente_id: u32) -> bool {
    estado.lock().await.clientes.contains_key(&cliente_id)
}

/// Envía un mensaje solo a la ventana que lo pidió, si sigue ahí.
async fn responder(estado: &EstadoCompartido, cliente_id: u32, mensaje: MensajeServidor) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(cliente, mensaje);
    }
}

/// `PedirVistaPrevia`: las últimas líneas de la sesión en texto plano, solo
/// para la ventana que las pide. No se guardan ni se registran (ni siquiera en
/// el log); con el host inalcanzable, `VistaPrevia` lleva el error.
pub async fn pedir_vista_previa(
    estado: &EstadoCompartido,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    servidor: ServidorTmux,
    nombre: String,
) {
    let clave = (cliente_id, peticion_id);
    let resultado = if reservar(estado, clave).await {
        let resultado = leer_vista(estado, cliente_id, host_id, servidor, &nombre).await;
        liberar(estado, clave).await;
        resultado
    } else {
        Err("ya hay una petición igual en curso".to_string())
    };
    let (lineas, error) = match resultado {
        Ok(lineas) => (lineas, None),
        Err(motivo) => (Vec::new(), Some(motivo)),
    };
    responder(
        estado,
        cliente_id,
        MensajeServidor::VistaPrevia {
            peticion_id,
            host_id,
            nombre,
            lineas,
            leida_en: chrono::Utc::now().timestamp(),
            error,
        },
    )
    .await;
}

async fn leer_vista(
    estado: &EstadoCompartido,
    cliente_id: u32,
    host_id: i64,
    servidor: ServidorTmux,
    nombre: &str,
) -> Result<Vec<String>, String> {
    let lineas_pedidas = estado
        .lock()
        .await
        .config
        .persistentes
        .vista_previa_lineas_efectivo();
    let orden = OrdenTmux::Vista {
        servidor,
        nombre,
        lineas: u32::try_from(lineas_pedidas).unwrap_or(20),
    };
    let argumentos = orden
        .argumentos()
        .ok_or_else(|| "nombre de tmux no admitido".to_string())?;
    let plazo = orden.plazo().unwrap_or(tmux::PLAZO_CAPTURA);
    if !sigue_conectada(estado, cliente_id).await {
        return Err("la ventana ya no está".to_string());
    }
    let (host, saltos, rutas, id_solicitud) = datos_de_host(estado, host_id).await?;
    // Lo pide una ventana, pero un diálogo de huella no es lo que espera quien
    // pulsa `v` en una lista: sin diálogos y con el llavero de la ventana.
    let politica = PoliticaConexion::NoInteractiva {
        solicitante: Some(cliente_id),
        id_solicitud,
    };
    let (salida, codigo) = ejecutar_orden_en_host(
        estado,
        &host,
        &saltos,
        &rutas,
        politica,
        &argumentos,
        Instant::now() + PLAZO_CONEXION_PETICION,
        None,
        plazo,
    )
    .await
    .map_err(|error| error.motivo(&host.nombre))?;
    match codigo {
        Some(0) => {
            let texto = lectura::acotar_vista(&String::from_utf8_lossy(&salida));
            let mut lineas: Vec<String> = texto.split('\n').map(str::to_string).collect();
            while lineas.last().is_some_and(String::is_empty) {
                lineas.pop();
            }
            if lineas.len() > lineas_pedidas {
                lineas.drain(..lineas.len() - lineas_pedidas);
            }
            Ok(lineas)
        }
        Some(SALIDA_NO_EXISTE) => Err(format!("la sesión «{nombre}» ya no existe")),
        Some(SALIDA_SIN_TMUX) => Err("el host ya no tiene tmux".to_string()),
        Some(SALIDA_ANTIGUO) => Err("tmux es anterior a 3.0".to_string()),
        Some(otro) => Err(format!("tmux no pudo leerla (código {otro})")),
        None => Err("el host cerró el canal sin responder".to_string()),
    }
}

/// `RenombrarTmux`: renombra la sesión del tmux propio en el host (siempre con
/// `=<nombre>`: el script no casa prefijos) y contesta a la ventana. Solo si el
/// host dice `ok` se actualiza lo que el servidor sabe en memoria; la ficha la
/// cambia después el cliente.
pub async fn renombrar(
    estado: &EstadoCompartido,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    nombre: String,
    nuevo: String,
) {
    let clave = (cliente_id, peticion_id);
    let resultado = if reservar(estado, clave).await {
        let resultado = renombrar_en_host(estado, cliente_id, host_id, &nombre, &nuevo).await;
        liberar(estado, clave).await;
        resultado
    } else {
        ResultadoRenombrar::Error("ya hay una petición igual en curso".to_string())
    };
    if resultado == ResultadoRenombrar::Ok {
        aplicar_renombrado(estado, host_id, &nombre, &nuevo).await;
    }
    responder(
        estado,
        cliente_id,
        MensajeServidor::TmuxRenombrada {
            peticion_id,
            resultado,
        },
    )
    .await;
}

async fn renombrar_en_host(
    estado: &EstadoCompartido,
    cliente_id: u32,
    host_id: i64,
    nombre: &str,
    nuevo: &str,
) -> ResultadoRenombrar {
    let orden = OrdenTmux::Renombrar { nombre, nuevo };
    let Some(argumentos) = orden.argumentos() else {
        return ResultadoRenombrar::Error("nombre de tmux no admitido".to_string());
    };
    let plazo = orden.plazo().unwrap_or(tmux::PLAZO_ORDEN);
    if !sigue_conectada(estado, cliente_id).await {
        return ResultadoRenombrar::Error("la ventana ya no está".to_string());
    }
    let (host, saltos, rutas, id_solicitud) = match datos_de_host(estado, host_id).await {
        Ok(datos) => datos,
        Err(motivo) => return ResultadoRenombrar::Error(motivo),
    };
    let politica = PoliticaConexion::NoInteractiva {
        solicitante: Some(cliente_id),
        id_solicitud,
    };
    let salida = ejecutar_orden_en_host(
        estado,
        &host,
        &saltos,
        &rutas,
        politica,
        &argumentos,
        Instant::now() + PLAZO_CONEXION_PETICION,
        None,
        plazo,
    )
    .await;
    match salida {
        Ok((salida, _codigo)) => match lectura::leer_renombrar(&String::from_utf8_lossy(&salida)) {
            LecturaRenombrar::Ok => ResultadoRenombrar::Ok,
            LecturaRenombrar::NoExiste => ResultadoRenombrar::NoExiste,
            LecturaRenombrar::Ocupado => ResultadoRenombrar::Ocupado,
            LecturaRenombrar::Error => {
                ResultadoRenombrar::Error("tmux no pudo renombrar la sesión".to_string())
            }
        },
        Err(error) => ResultadoRenombrar::Error(error.motivo(&host.nombre)),
    }
}

/// Lo que cambia en el servidor cuando el host renombró una sesión: lo que
/// sabe de ese host en memoria (para no darla por terminada hasta la
/// siguiente comprobación, que se adelanta) y las pestañas enganchadas, que
/// siguen enganchadas con el nombre nuevo.
async fn aplicar_renombrado(estado: &EstadoCompartido, host_id: i64, nombre: &str, nuevo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    if let Some(comprobado) = estado_bloqueado
        .persistentes
        .comprobaciones
        .get_mut(&host_id)
    {
        for sesion in &mut comprobado.datos.sesiones {
            if sesion.servidor == ServidorTmux::Magi && sesion.nombre == nombre {
                sesion.nombre = nuevo.to_string();
            }
        }
    }
    estado_bloqueado
        .persistentes
        .recomprobar
        .insert(host_id, Instant::now());
    if renombrar_en_pestanas(&mut estado_bloqueado, host_id, nombre, nuevo) {
        super::sesiones::difundir_lista(&mut estado_bloqueado);
    }
}

/// Cambia el nombre de tmux de las pestañas del host enganchadas a la sesión
/// renombrada. Devuelve si cambió alguna (entonces hay que difundir
/// `Sesiones{lista}`).
fn renombrar_en_pestanas(
    estado: &mut EstadoServidor,
    host_id: i64,
    nombre: &str,
    nuevo: &str,
) -> bool {
    // `Sesion.tmux` es el estado vivo de la pestaña (la reconexión sale de
    // él): solo cambian las enganchadas al servidor propio con ese nombre; una
    // del tmux habitual que se llame igual es otra sesión.
    let mut cambiada = false;
    for sesion in estado.sesiones.values_mut() {
        if sesion.host_id == host_id
            && sesion.tmux_servidor == ServidorTmux::Magi
            && sesion.tmux.as_deref() == Some(nombre)
        {
            sesion.tmux = Some(nuevo.to_string());
            cambiada = true;
        }
    }
    cambiada
}

// ------------------------------------------------------------------- pruebas

#[cfg(test)]
mod pruebas {
    use super::*;
    use magi_nucleo::sesiones_persistentes::{OrigenFicha, SesionVista, VersionTmux};

    fn ficha(uuid: &str, host_id: i64, nombre: &str) -> Ficha {
        Ficha {
            id: 1,
            uuid: uuid.to_string(),
            host_id,
            nombre: nombre.to_string(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Manual,
            creada_por: "portatil".to_string(),
            creada_en: "2026-11-16 10:00:00".to_string(),
            actualizado_en: "2026-11-16 10:00:00".to_string(),
        }
    }

    fn sesion(servidor: ServidorTmux, nombre: &str, clientes: u32) -> SesionVista {
        SesionVista {
            servidor,
            nombre: nombre.to_string(),
            clientes,
            equipo: None,
            actividad: 1_700_000_000,
            creada: 1_699_990_000,
            campana: false,
            ventanas: 1,
            titulo: None,
        }
    }

    fn comprobado(host_id: i64, sesiones: Vec<SesionVista>, epoca: i64) -> Comprobado {
        Comprobado {
            datos: ComprobacionHost {
                host_id,
                tmux: Some(VersionTmux::nueva(3, 4)),
                ahora: 1_700_000_100,
                sesiones,
                error: None,
            },
            cuando: Instant::now(),
            epoca,
        }
    }

    #[test]
    fn un_host_comprobado_hace_menos_de_5_s_no_se_repite() {
        let ahora = Instant::now();
        let minima = Duration::from_secs(5);
        assert!(
            debe_comprobarse(None, ahora, minima, false),
            "sin comprobar, se comprueba"
        );
        let hace = |segundos: u64| ahora.checked_sub(Duration::from_secs(segundos));
        let hace_4 = hace(4);
        assert!(!debe_comprobarse(hace_4, ahora, minima, false));
        assert!(debe_comprobarse(hace(5), ahora, minima, false));
        assert!(debe_comprobarse(hace(60), ahora, minima, false));
        // Un renombrado lo obliga aunque sea reciente.
        assert!(debe_comprobarse(hace_4, ahora, minima, true));
    }

    #[test]
    fn una_comprobacion_anterior_al_renombrado_no_da_por_hecha_la_nueva_mirada() {
        let ahora = Instant::now();
        let antes = ahora.checked_sub(Duration::from_secs(1)).unwrap();
        let despues = ahora + Duration::from_secs(1);
        // Sin orden de volver a mirar, todo resultado vale.
        assert!(!es_anterior_a_la_marca(None, antes));
        // Empezó antes del renombrado: puede traer el nombre viejo.
        assert!(es_anterior_a_la_marca(Some(ahora), antes));
        // Empezó después (o a la vez): ya ve el nombre nuevo.
        assert!(!es_anterior_a_la_marca(Some(ahora), despues));
        assert!(!es_anterior_a_la_marca(Some(ahora), ahora));
    }

    #[test]
    fn la_lista_deriva_el_estado_de_cada_ficha_y_separa_las_que_no_tienen() {
        let fichas = vec![
            ficha("u1", 1, "api"),
            ficha("u2", 1, "web"),
            ficha("u3", 1, "vieja"),
            ficha("u4", 2, "otra"),
        ];
        let mut comprobaciones = HashMap::new();
        comprobaciones.insert(
            1,
            comprobado(
                1,
                vec![
                    sesion(ServidorTmux::Magi, "api", 0),
                    sesion(ServidorTmux::Magi, "web", 1),
                    // Sin ficha: viva en el tmux de MAGI, nadie la marcó.
                    sesion(ServidorTmux::Magi, "suelta", 0),
                    // Del tmux habitual: nunca casa con una ficha, aunque
                    // se llame igual que «vieja».
                    sesion(ServidorTmux::Habitual, "vieja", 1),
                ],
                1_700_000_050,
            ),
        );
        let lista = construir_lista(&fichas, &comprobaciones);
        let estado_de = |uuid: &str| {
            lista
                .fichas
                .iter()
                .find(|ficha| ficha.uuid == uuid)
                .unwrap()
        };
        assert_eq!(estado_de("u1").estado, EstadoFicha::VivaLibre);
        assert!(estado_de("u1").sesion.is_some());
        assert_eq!(estado_de("u2").estado, EstadoFicha::VivaEnganchada);
        assert_eq!(estado_de("u3").estado, EstadoFicha::Terminada);
        assert!(estado_de("u3").sesion.is_none());
        // El host 2 aún no se ha comprobado.
        assert_eq!(estado_de("u4").estado, EstadoFicha::SinComprobar);
        assert_eq!(estado_de("u4").comprobada_en, 0);
        assert_eq!(estado_de("u1").comprobada_en, 1_700_000_050);

        assert_eq!(lista.hosts.len(), 1);
        let host = &lista.hosts[0];
        assert_eq!(host.host_id, 1);
        assert_eq!(
            host.sin_ficha
                .iter()
                .map(|sesion| sesion.nombre.as_str())
                .collect::<Vec<_>>(),
            vec!["suelta"]
        );
        assert_eq!(host.habitual.len(), 1);
        assert_eq!(host.habitual[0].nombre, "vieja");
    }

    #[test]
    fn un_host_que_no_se_pudo_mirar_nunca_deja_sus_fichas_terminadas() {
        for error in [
            ErrorComprobacion::SinRespuesta,
            ErrorComprobacion::NecesitaDialogo,
            ErrorComprobacion::Plazo,
        ] {
            let mut comprobaciones = HashMap::new();
            comprobaciones.insert(
                1,
                Comprobado {
                    datos: ComprobacionHost {
                        host_id: 1,
                        tmux: None,
                        ahora: 0,
                        sesiones: Vec::new(),
                        error: Some(error),
                    },
                    cuando: Instant::now(),
                    epoca: 1_700_000_050,
                },
            );
            let lista = construir_lista(&[ficha("u1", 1, "api")], &comprobaciones);
            assert_eq!(lista.fichas[0].estado, EstadoFicha::Inalcanzable);
            assert_eq!(lista.hosts[0].error, Some(error));
            assert!(lista.hosts[0].sin_ficha.is_empty());
        }
    }

    #[test]
    fn las_marcas_de_tiempo_no_cuentan_como_cambio() {
        let fichas = vec![ficha("u1", 1, "api")];
        let sesiones = vec![sesion(ServidorTmux::Magi, "api", 0)];
        let mut una = HashMap::new();
        una.insert(1, comprobado(1, sesiones.clone(), 1_700_000_050));
        let mut otra = HashMap::new();
        let mut segunda = comprobado(1, sesiones.clone(), 1_700_000_110);
        segunda.datos.ahora += 60;
        otra.insert(1, segunda);
        let primera = construir_lista(&fichas, &una);
        let segunda = construir_lista(&fichas, &otra);
        assert_ne!(primera, segunda, "las marcas sí cambian la lista");
        assert_eq!(sin_marcas(&primera), sin_marcas(&segunda));

        // Un cambio de verdad (ahora hay un cliente enganchado) sí cuenta.
        let mut con_cliente = HashMap::new();
        con_cliente.insert(
            1,
            comprobado(1, vec![sesion(ServidorTmux::Magi, "api", 1)], 1_700_000_110),
        );
        assert_ne!(
            sin_marcas(&primera),
            sin_marcas(&construir_lista(&fichas, &con_cliente))
        );
    }

    #[test]
    fn una_fila_ilegible_se_avisa_una_sola_vez_y_otra_si_vuelve() {
        let mut persistentes = Persistentes::default();
        let motivo = vec!["la ficha 3 tiene un origen desconocido".to_string()];
        assert_eq!(persistentes.motivos_sin_avisar(&motivo), motivo);
        // Cada minuto la misma fila: sin ruido.
        assert!(persistentes.motivos_sin_avisar(&motivo).is_empty());
        assert!(persistentes.motivos_sin_avisar(&motivo).is_empty());
        // Se arregla...
        assert!(persistentes.motivos_sin_avisar(&[]).is_empty());
        // ...y si vuelve a romperse, se avisa de nuevo.
        assert_eq!(persistentes.motivos_sin_avisar(&motivo), motivo);
    }

    #[test]
    fn una_lista_enorme_se_acota_para_caber_en_el_codec() {
        let sesiones: Vec<SesionVista> = (0..500)
            .map(|numero| {
                let mut sesion = sesion(ServidorTmux::Habitual, &format!("s{numero}"), 0);
                sesion.titulo = Some("t".repeat(120));
                sesion
            })
            .collect();
        let mut comprobaciones = HashMap::new();
        for host_id in 1..=60 {
            comprobaciones.insert(host_id, comprobado(host_id, sesiones.clone(), 1));
        }
        let lista = construir_lista(&[], &comprobaciones);
        let bytes = serde_json::to_vec(&lista).unwrap().len();
        assert!(bytes <= TOPE_LISTA, "la lista pesa {bytes} bytes");
        assert_eq!(lista.hosts.len(), 60, "ningún host desaparece");
    }

    #[test]
    fn los_errores_de_conexion_se_clasifican_sin_confundirse_con_nombres() {
        for texto in [
            "huella desconocida de 192.0.2.10 en known_hosts; conéctate una vez con ↵",
            "«web-01» pide la contraseña cada vez y una operación automática no dialoga; guárdala",
            "la clave /home/usuario/.ssh/id requiere frase y no hay diálogo posible",
            "«web-01» usa contraseña y no hay ninguna guardada en el llavero; conéctate",
            "otra apertura de «web-01» espera una decisión del usuario; inténtalo de nuevo",
        ] {
            assert_eq!(
                clasificar_error(texto),
                ErrorComprobacion::NecesitaDialogo,
                "{texto}"
            );
        }
        for texto in [
            "se agotó el plazo conectando con «web-01» (30 s)",
            "el host no abrió el canal a tiempo",
        ] {
            assert_eq!(clasificar_error(texto), ErrorComprobacion::Plazo, "{texto}");
        }
        for texto in [
            "Connection refused (os error 111)",
            "autenticación rechazada",
            "el host de salto de «web-01» ya no existe",
        ] {
            assert_eq!(
                clasificar_error(texto),
                ErrorComprobacion::SinRespuesta,
                "{texto}"
            );
        }
        // Un host que se llame «frase» no vuelve diálogo una conexión rechazada.
        assert_eq!(
            clasificar_error("«frase-01»: Connection refused"),
            ErrorComprobacion::SinRespuesta
        );
    }
}
