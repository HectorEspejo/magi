//! Dobles y utilidades comunes de las pruebas de extremo a extremo de
//! `magi-movil` (informe §11, T95 y D283).
//!
//! Cada prueba de `tests/` declara `mod comun;` y usa lo que necesite: el
//! módulo se compila en todas, así que lleva `#![allow(dead_code)]`.
//!
//! Aquí viven las retrollamadas falsas (carpeta SAF vacía, llavero en memoria,
//! firmante P-256 y oyente que guarda las listas), el `Entorno` que monta las
//! rutas privadas, el `known_hosts` y el núcleo con una base de datos temporal,
//! y los ayudantes para crear hosts y esperar listas. El servidor SSH de
//! pruebas compartido vive aparte, en `comun/servidor_pruebas.rs`.

#![allow(dead_code)]

pub mod servidor_pruebas;

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use magi_movil::api::{
    AccionServicioMovil, EjecucionMovil, Equipo as EquipoApi, ErrorNucleo, EstadoLogs,
    EstadoSincronizacion, HostFlota, Nucleo, Pantalla, PeticionDialogo, ResultadoServicioMovil,
    Retrollamadas, Retrollamadas as RetrollamadasApi, Rutas as RutasApi, Sesion, TunelVivo,
};
use magi_movil::estado::{con_almacen, EstadoNucleo};
use magi_movil::retrollamadas::{
    CarpetaMovil, FicheroCarpeta, FirmanteMovil, LlaveroMovil, OyenteNucleo,
};
use magi_nucleo::almacen::hosts as almacen_hosts;
use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::Almacen;
use magi_nucleo::modelo::{DatosHost, Origen};
use magi_nucleo::rutas::{Equipo, Rutas};

// ---------------------------------------------------------------------------
// Retrollamadas de plataforma
// ---------------------------------------------------------------------------

/// Carpeta compartida vacía: el móvil no sincroniza en estas pruebas.
pub struct CarpetaVacia;

impl CarpetaMovil for CarpetaVacia {
    fn listar(&self) -> Result<Vec<FicheroCarpeta>, ErrorNucleo> {
        Ok(Vec::new())
    }

    fn leer(&self, _nombre: String, _maximo: u64) -> Result<Vec<u8>, ErrorNucleo> {
        Err(ErrorNucleo::Carpeta {
            detalle: "carpeta vacía".to_string(),
        })
    }
}

/// Llavero del móvil en memoria, con la misma forma que el de verdad: por
/// cuenta (p. ej. `contrasena:<uuid>`).
#[derive(Clone, Default)]
pub struct LlaveroMemoria {
    secretos: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl LlaveroMemoria {
    pub fn guardar_en_memoria(&self, cuenta: &str, valor: &[u8]) {
        self.secretos
            .lock()
            .expect("llavero")
            .insert(cuenta.to_string(), valor.to_vec());
    }

    pub fn leer_secreto(&self, cuenta: &str) -> Option<Vec<u8>> {
        self.secretos.lock().expect("llavero").get(cuenta).cloned()
    }
}

impl LlaveroMovil for LlaveroMemoria {
    fn leer(&self, clave: String) -> Result<Option<Vec<u8>>, ErrorNucleo> {
        Ok(self.secretos.lock().expect("llavero").get(&clave).cloned())
    }

    fn guardar(&self, clave: String, secreto: Vec<u8>) -> Result<(), ErrorNucleo> {
        self.secretos
            .lock()
            .expect("llavero")
            .insert(clave, secreto);
        Ok(())
    }

    fn borrar(&self, clave: String) -> Result<(), ErrorNucleo> {
        self.secretos.lock().expect("llavero").remove(&clave);
        Ok(())
    }
}

/// Clave P-256 de pruebas: la cadena del móvil autentica con la clave pública
/// y el servidor de pruebas acepta cualquier firma (no la verifica).
pub struct FirmantePrueba {
    clave: p256::ecdsa::SigningKey,
}

/// Cabecera SPKI de una clave pública EC P-256 sin comprimir.
const PREFIJO_SPKI: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

impl FirmantePrueba {
    pub fn nuevo() -> Self {
        let clave = p256::ecdsa::SigningKey::from_bytes(&p256::FieldBytes::from([7u8; 32]))
            .expect("clave P-256 de pruebas");
        Self { clave }
    }
}

impl FirmanteMovil for FirmantePrueba {
    fn clave_publica_x509(&self) -> Result<Vec<u8>, ErrorNucleo> {
        let punto = self.clave.verifying_key().to_sec1_bytes();
        assert_eq!(punto.first(), Some(&0x04), "punto SEC1 sin comprimir");
        let mut der = PREFIJO_SPKI.to_vec();
        der.extend_from_slice(&punto);
        Ok(der)
    }

    fn firmar(&self, datos: Vec<u8>) -> Result<Vec<u8>, ErrorNucleo> {
        use p256::ecdsa::signature::Signer as _;
        let firma: p256::ecdsa::Signature = self.clave.sign(datos.as_slice());
        Ok(firma.to_der().as_bytes().to_vec())
    }

    fn olvidar_clave(&self) -> Result<(), ErrorNucleo> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Oyente: guarda la última lista completa de cada retrollamada
// ---------------------------------------------------------------------------

/// Oyente del núcleo que guarda la última lista completa de cada flujo nuevo
/// de la Fase 14 y los diálogos pedidos (para comprobar que lo automático no
/// dialoga). También guarda sesiones y túneles de la Fase 11.
#[derive(Default)]
pub struct OyenteGuardado {
    sesiones: Mutex<Vec<Sesion>>,
    tuneles: Mutex<Vec<TunelVivo>>,
    flota: Mutex<Vec<HostFlota>>,
    acciones_servicio: Mutex<Vec<AccionServicioMovil>>,
    acciones_hechas: Mutex<Vec<ResultadoServicioMovil>>,
    logs: Mutex<Vec<EstadoLogs>>,
    ejecuciones: Mutex<Vec<EjecucionMovil>>,
    dialogos: Mutex<Vec<PeticionDialogo>>,
}

impl OyenteGuardado {
    pub fn sesiones(&self) -> Vec<Sesion> {
        self.sesiones.lock().expect("oyente").clone()
    }

    pub fn tuneles(&self) -> Vec<TunelVivo> {
        self.tuneles.lock().expect("oyente").clone()
    }

    pub fn flota(&self) -> Vec<HostFlota> {
        self.flota.lock().expect("oyente").clone()
    }

    pub fn acciones_servicio(&self) -> Vec<AccionServicioMovil> {
        self.acciones_servicio.lock().expect("oyente").clone()
    }

    pub fn acciones_hechas(&self) -> Vec<ResultadoServicioMovil> {
        self.acciones_hechas.lock().expect("oyente").clone()
    }

    pub fn logs(&self) -> Vec<EstadoLogs> {
        self.logs.lock().expect("oyente").clone()
    }

    pub fn ejecuciones(&self) -> Vec<EjecucionMovil> {
        self.ejecuciones.lock().expect("oyente").clone()
    }

    pub fn dialogos(&self) -> Vec<PeticionDialogo> {
        self.dialogos.lock().expect("oyente").clone()
    }
}

impl OyenteNucleo for OyenteGuardado {
    fn pantalla(&self, _pantalla: Pantalla) {}

    fn sesiones(&self, lista: Vec<Sesion>) {
        *self.sesiones.lock().expect("oyente") = lista;
    }

    fn tuneles(&self, lista: Vec<TunelVivo>) {
        *self.tuneles.lock().expect("oyente") = lista;
    }

    fn dialogo(&self, peticion: PeticionDialogo) {
        self.dialogos.lock().expect("oyente").push(peticion.clone());
        eprintln!("DIALOGO PEDIDO: {peticion:?}");
    }

    fn sincronizacion(&self, _estado: EstadoSincronizacion) {}

    fn flota(&self, lista: Vec<HostFlota>) {
        *self.flota.lock().expect("oyente") = lista;
    }

    fn acciones_servicio(&self, lista: Vec<AccionServicioMovil>) {
        *self.acciones_servicio.lock().expect("oyente") = lista;
    }

    fn accion_servicio_hecha(&self, resultado: ResultadoServicioMovil) {
        self.acciones_hechas.lock().expect("oyente").push(resultado);
    }

    fn logs(&self, estado: EstadoLogs) {
        self.logs.lock().expect("oyente").push(estado);
    }

    fn ejecuciones(&self, lista: Vec<EjecucionMovil>) {
        *self.ejecuciones.lock().expect("oyente") = lista;
    }
}

// ---------------------------------------------------------------------------
// Entorno de una prueba de extremo a extremo
// ---------------------------------------------------------------------------

/// Rutas privadas, base de datos y retrollamadas de una prueba. Al soltarse
/// deja el runtime del núcleo sin cerrar (dentro de un test async no se puede
/// soltar) y borra el directorio temporal.
pub struct Entorno {
    temporal: Option<tempfile::TempDir>,
    rutas: Rutas,
    estado: Option<Arc<EstadoNucleo>>,
    oyente: Arc<OyenteGuardado>,
    llavero: LlaveroMemoria,
}

impl Entorno {
    /// Monta el entorno con el `known_hosts` que se le pasa (puerto y clave de
    /// host por cada servidor de pruebas). Sin entradas, el `known_hosts` queda
    /// sin fichero: es el caso de la huella sin aceptar.
    pub fn nuevo(conocidos: &[(u16, ssh_key::PublicKey)]) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let base = temporal.path();
        let rutas = Rutas::nuevas(
            base.join("datos"),
            base.join("config"),
            base.join("estado"),
            base.join("hogar"),
            base.join("runtime"),
        );
        std::fs::create_dir_all(&rutas.datos).expect("datos");
        std::fs::create_dir_all(rutas.dir_ssh()).expect("ssh");
        if !conocidos.is_empty() {
            let mut lineas = String::new();
            for (puerto, clave) in conocidos {
                lineas.push_str(&format!(
                    "[127.0.0.1]:{puerto} {}
",
                    clave.to_openssh().expect("clave pública del host")
                ));
            }
            let ruta = rutas.fichero_known_hosts();
            std::fs::write(&ruta, lineas).expect("known_hosts");
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600))
                .expect("permisos del known_hosts");
        }
        let llavero = LlaveroMemoria::default();
        let oyente = Arc::new(OyenteGuardado::default());
        let estado = EstadoNucleo::nuevo(
            rutas.clone(),
            Equipo::nuevo("pruebas", "android"),
            Retrollamadas {
                carpeta: Arc::new(CarpetaVacia),
                llavero: Arc::new(llavero.clone()),
                firmante: Arc::new(FirmantePrueba::nuevo()),
                oyente: oyente.clone(),
            },
        )
        .expect("estado del núcleo");
        Self {
            temporal: Some(temporal),
            rutas,
            estado: Some(estado),
            oyente,
            llavero,
        }
    }

    pub fn estado(&self) -> &Arc<EstadoNucleo> {
        self.estado.as_ref().expect("estado del entorno")
    }

    pub fn oyente(&self) -> &Arc<OyenteGuardado> {
        &self.oyente
    }

    pub fn llavero(&self) -> &LlaveroMemoria {
        &self.llavero
    }

    pub fn rutas(&self) -> &Rutas {
        &self.rutas
    }

    /// Crea un host en el inventario y devuelve su id local y su uuid.
    pub fn crear_host(&self, datos: DatosHost) -> (i64, String) {
        let id = con_almacen(self.estado(), move |almacen| {
            almacen_hosts::crear(almacen.conexion(), &datos, Origen::Manual)
        })
        .expect("escritor")
        .expect("creando el host");
        let uuid = con_almacen(self.estado(), move |almacen| {
            movil_bd::uuid_por_id(almacen.conexion(), Tabla::Hosts)
                .map(|mapa| mapa.get(&id).cloned())
        })
        .expect("escritor")
        .expect("mapa de uuid")
        .expect("uuid del host");
        (id, uuid)
    }

    /// Guarda en el llavero la contraseña de un host (cuenta `contrasena:<uuid>`).
    pub fn guardar_contrasena(&self, host_uuid: &str, contrasena: &str) {
        self.llavero
            .guardar_en_memoria(&format!("contrasena:{host_uuid}"), contrasena.as_bytes());
    }
}

impl Drop for Entorno {
    fn drop(&mut self) {
        // El runtime del núcleo no se puede soltar dentro de un test async
        // (tokio entraría en pánico al bloquear); se deja al proceso, como
        // hacían las pruebas de la Fase 11.
        if let Some(estado) = self.estado.take() {
            std::mem::forget(estado);
        }
    }
}

// ---------------------------------------------------------------------------
// Núcleo completo (API pública) para las pruebas de extremo a extremo
// ---------------------------------------------------------------------------

/// Montaje de una prueba contra la API pública de `magi-movil`: las rutas
/// privadas y el `known_hosts`, una `magi.db` que se puede preparar antes de
/// arrancar y el `Nucleo` que se crea con `Nucleo::iniciar`, como en la app.
pub struct PruebaNucleo {
    temporal: Option<tempfile::TempDir>,
    rutas: Rutas,
    rutas_api: RutasApi,
    equipo: Equipo,
    nucleo: Option<Arc<Nucleo>>,
    oyente: Arc<OyenteGuardado>,
    llavero: LlaveroMemoria,
}

impl PruebaNucleo {
    /// Igual que `Entorno::nuevo` para el `known_hosts`, pero sin arrancar el
    /// núcleo: primero se prepara la base de datos con `preparar_bd` y después
    /// se llama a `arrancar`.
    pub fn nueva(known_hosts: &[String]) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let base = temporal.path();
        let rutas = Rutas::nuevas(
            base.join("datos"),
            base.join("config"),
            base.join("estado"),
            base.join("hogar"),
            base.join("runtime"),
        );
        std::fs::create_dir_all(&rutas.datos).expect("datos");
        std::fs::create_dir_all(rutas.dir_ssh()).expect("ssh");
        if !known_hosts.is_empty() {
            let mut lineas = String::new();
            for linea in known_hosts {
                lineas.push_str(linea);
                lineas.push('\n');
            }
            let ruta = rutas.fichero_known_hosts();
            std::fs::write(&ruta, lineas).expect("known_hosts");
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600))
                .expect("permisos del known_hosts");
        }
        let rutas_api = RutasApi {
            datos: rutas.datos.display().to_string(),
            config: rutas.config.display().to_string(),
            estado: rutas.estado.display().to_string(),
            hogar: rutas.hogar.display().to_string(),
            runtime: rutas.runtime.display().to_string(),
        };
        Self {
            temporal: Some(temporal),
            rutas,
            rutas_api,
            equipo: Equipo::nuevo("pruebas", "android"),
            nucleo: None,
            oyente: Arc::new(OyenteGuardado::default()),
            llavero: LlaveroMemoria::default(),
        }
    }

    /// Prepara la base de datos antes de arrancar el núcleo (hosts, snippets,
    /// verificaciones...). Abre `magi.db`, ejecuta la tarea y la cierra; la
    /// tarea puede devolver lo que necesite (p. ej. los uuids creados).
    pub fn preparar_bd<T>(&self, tarea: impl FnOnce(&Almacen) -> T) -> T {
        let almacen = Almacen::abrir(&self.rutas.base_datos(), &self.equipo, "pruebas")
            .expect("abriendo la magi.db de pruebas");
        let resultado = tarea(&almacen);
        almacen.cerrar().expect("cerrando la magi.db de pruebas");
        resultado
    }

    /// Arranca el núcleo como lo hace la app: `Nucleo::iniciar` con las rutas,
    /// el equipo y las retrollamadas.
    pub fn arrancar(&mut self) {
        assert!(self.nucleo.is_none(), "el núcleo ya está arrancado");
        let retrollamadas = RetrollamadasApi {
            carpeta: Arc::new(CarpetaVacia),
            llavero: Arc::new(self.llavero.clone()),
            firmante: Arc::new(FirmantePrueba::nuevo()),
            oyente: self.oyente.clone(),
        };
        let equipo = EquipoApi {
            nombre: "pruebas".to_string(),
            plataforma: "android".to_string(),
        };
        let nucleo = Nucleo::iniciar(self.rutas_api.clone(), equipo, retrollamadas)
            .expect("arrancando el núcleo");
        self.nucleo = Some(nucleo);
    }

    pub fn nucleo(&self) -> &Arc<Nucleo> {
        self.nucleo.as_ref().expect("núcleo arrancado")
    }

    pub fn oyente(&self) -> &Arc<OyenteGuardado> {
        &self.oyente
    }

    pub fn llavero(&self) -> &LlaveroMemoria {
        &self.llavero
    }

    pub fn rutas(&self) -> &Rutas {
        &self.rutas
    }

    /// Estado interno del núcleo, para llamar a los módulos (flota, ejecuciones,
    /// deliberación) con el mismo estado que creó `Nucleo::iniciar`.
    pub fn estado(&self) -> &Arc<EstadoNucleo> {
        self.nucleo().estado_de_prueba()
    }

    /// Guarda en el llavero la contraseña de un host (cuenta `contrasena:<uuid>`).
    pub fn guardar_contrasena(&self, host_uuid: &str, contrasena: &str) {
        self.llavero
            .guardar_en_memoria(&format!("contrasena:{host_uuid}"), contrasena.as_bytes());
    }
}

impl Drop for PruebaNucleo {
    fn drop(&mut self) {
        if let Some(nucleo) = self.nucleo.take() {
            std::mem::forget(nucleo);
        }
    }
}

// ---------------------------------------------------------------------------
// Ayudantes de base de datos y de espera
// ---------------------------------------------------------------------------

/// Crea un host en la base de datos de la prueba (antes de arrancar el núcleo)
/// y devuelve su id local y su uuid.
pub fn crear_host(prueba: &PruebaNucleo, datos: DatosHost) -> (i64, String) {
    prueba.preparar_bd(|almacen| {
        let id = almacen
            .crear_host(&datos, Origen::Manual)
            .expect("creando el host");
        let uuid = movil_bd::uuid_por_id(almacen.conexion(), Tabla::Hosts)
            .expect("uuid de los hosts")
            .get(&id)
            .cloned()
            .expect("uuid del host");
        (id, uuid)
    })
}

/// Ejecuta una consulta por el hilo escritor del núcleo.
pub fn consulta<T: Send + 'static>(
    nucleo: &Nucleo,
    tarea: impl FnOnce(&Almacen) -> T + Send + 'static,
) -> Option<T> {
    con_almacen(nucleo.estado_de_prueba(), move |almacen| tarea(almacen)).ok()
}

/// Cuenta las entradas de `REGISTRO` de un tipo.
pub fn contar_registro(nucleo: &Nucleo, tipo: &str) -> i64 {
    let tipo = tipo.to_string();
    consulta(nucleo, move |almacen| {
        almacen
            .conexion()
            .query_row(
                "SELECT COUNT(*) FROM REGISTRO WHERE tipo = ?1",
                [tipo],
                |fila| fila.get::<_, i64>(0),
            )
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Espera hasta que la lista de Flota del oyente cumpla la condición; si no
/// llega en el plazo, falla con la última lista recibida.
pub async fn esperar_flota(
    oyente: &Arc<OyenteGuardado>,
    plazo: Duration,
    condicion: impl Fn(&[HostFlota]) -> bool,
) -> Vec<HostFlota> {
    let inicio = Instant::now();
    loop {
        let lista = oyente.flota();
        if condicion(&lista) {
            return lista;
        }
        if inicio.elapsed() > plazo {
            panic!("la Flota no llegó al estado esperado en {plazo:?}: {lista:?}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Espera hasta que las ejecuciones del oyente cumplan la condición.
pub async fn esperar_ejecuciones(
    oyente: &Arc<OyenteGuardado>,
    plazo: Duration,
    condicion: impl Fn(&[EjecucionMovil]) -> bool,
) -> Vec<EjecucionMovil> {
    let inicio = Instant::now();
    loop {
        let lista = oyente.ejecuciones();
        if condicion(&lista) {
            return lista;
        }
        if inicio.elapsed() > plazo {
            panic!("las ejecuciones no llegaron al estado esperado en {plazo:?}: {lista:?}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Busca la fila de un host en una lista de Flota.
pub fn host_flota(lista: &[HostFlota], uuid: &str) -> Option<HostFlota> {
    lista.iter().find(|host| host.uuid == uuid).cloned()
}

/// Busca la fila de un túnel en la última lista del oyente.
pub fn tunel_por_uuid(oyente: &OyenteGuardado, uuid: &str) -> Option<TunelVivo> {
    oyente
        .tuneles()
        .into_iter()
        .find(|tunel| tunel.uuid == uuid)
}

/// Servicio de eco local: devuelve lo que recibe. Lo usan las pruebas de
/// túneles y las de sesiones con orden.
pub async fn servir_eco() -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("servicio de eco");
    let puerto = escucha.local_addr().expect("eco").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bufer = vec![0u8; 1024];
                loop {
                    match flujo.read(&mut bufer).await {
                        Ok(0) | Err(_) => return,
                        Ok(leidos) => {
                            if flujo.write_all(&bufer[..leidos]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    puerto
}
