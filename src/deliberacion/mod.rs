//! Deliberación MAGI (Fase 6): la pausa obligatoria antes de una ejecución
//! arriesgada. Tres comprobaciones por host —MELCHIOR-1 salud, BALTHASAR-2
//! backup reciente, CASPER-3 tests en verde—, unanimidad para ejecutar y
//! forzado solo con motivo escrito, que queda registrado.
//!
//! Desde la Fase 14 el modelo, las comprobaciones y su orquestación con plazo
//! viven en `magi_nucleo::deliberacion` (D275) y los comparten el escritorio y
//! el móvil. Aquí queda lo que es solo del escritorio: el usuario del sistema,
//! los límites de `config.toml`, la máquina de estados del diálogo y los
//! proveedores con los que se hacen las comprobaciones (el `Ejecutar` del
//! servidor de sesiones, su `AbrirSftp` con `ListarDir` y `sh -c` en local).

pub mod estado;

pub use magi_nucleo::deliberacion::*;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use magi_nucleo::deliberacion as nucleo;
use magi_nucleo::sftp_lectura::{EntradaListado, TipoEntradaListado};

use crate::archivos::{Entrada, TipoEntrada};
use crate::modelo::{ResultadoSondeo, Sondeo};

/// El futuro que devuelve una comprobación: el del núcleo.
type Futuro<T> = nucleo::comprobaciones::Futuro<T>;

/// Usuario local para `DELIBERACIONES.usuario`: el del uid del proceso (una
/// variable de entorno se puede falsear y esto es un registro de auditoría);
/// `$USER` solo si el sistema no lo sabe.
pub fn usuario_del_sistema() -> String {
    nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
        .map(|usuario| usuario.name)
        .unwrap_or_else(crate::conexion::usuario_local)
}

/// Los límites de `[deliberacion]` de `config.toml`, con los suelos del
/// núcleo. `Limites` es del núcleo y no admite métodos inherentes desde aquí,
/// así que este trait conserva la llamada `Limites::desde_config` que usaba el
/// escritorio.
pub trait LimitesDeConfig {
    /// Los límites de la sección, con un suelo para que un valor absurdo no
    /// desactive nada.
    fn desde_config(seccion: &crate::config::SeccionDeliberacion) -> Self;
}

impl LimitesDeConfig for Limites {
    fn desde_config(seccion: &crate::config::SeccionDeliberacion) -> Self {
        Limites::nuevos(
            seccion.backup_horas,
            seccion.salud_max_min,
            seccion.limite_seg,
            seccion.motivo_min,
        )
    }
}

/// Los proveedores del escritorio para las comprobaciones (D275): el sondeo
/// por el servidor de sesiones (o una conexión efímera), el listado del
/// directorio de backups por un SFTP de comprobación y el comando local de
/// CASPER-3. Se construye al abrir la deliberación con lo que ya está fijado
/// (cliente, una petición de sondeo por host y hogar) y no relee el estado al
/// confirmar (T33).
pub struct ProveedoresEscritorio {
    cliente: crate::cliente::Cliente,
    /// La petición de sondeo de cada host, calculada en el hilo de la UI.
    peticiones: Mutex<HashMap<i64, crate::flota::PeticionSondeo>>,
    /// Directorio en el que corre el comando local de CASPER-3.
    hogar: PathBuf,
    /// Plazo de cada comprobación, medido desde el inicio de la deliberación:
    /// el mismo instante que recibe `lanzar`.
    limite: tokio::time::Instant,
}

impl ProveedoresEscritorio {
    pub fn nuevo(
        cliente: crate::cliente::Cliente,
        peticiones: HashMap<i64, crate::flota::PeticionSondeo>,
        hogar: PathBuf,
        limite: tokio::time::Instant,
    ) -> Self {
        Self {
            cliente,
            peticiones: Mutex::new(peticiones),
            hogar,
            limite,
        }
    }
}

impl nucleo::Proveedores for ProveedoresEscritorio {
    fn sondear(&self, host_id: i64, excluir: Vec<String>) -> Futuro<Sondeo> {
        // Cada host se sondea una sola vez por deliberación: la petición se
        // consume tal cual se fijó al abrirla. Si el host ya no existe, el
        // veredicto lo dice como una caída (el diálogo no llega a abrirse así).
        let peticion = self
            .peticiones
            .lock()
            .unwrap_or_else(|veneno| veneno.into_inner())
            .remove(&host_id);
        let cliente = self.cliente.clone();
        Box::pin(async move {
            match peticion {
                Some(mut peticion) => {
                    // El estado del propio objetivo no cuenta (§7.2): el
                    // sondeo nuevo no lo mira.
                    peticion
                        .servicios
                        .retain(|unidad| !excluir.contains(unidad));
                    crate::flota::sondear_via_servidor(&peticion, &cliente).await
                }
                None => {
                    let mut sondeo = Sondeo::vacio(host_id);
                    sondeo.resultado = ResultadoSondeo::Error;
                    sondeo.error = Some("el host ya no existe".to_string());
                    sondeo
                }
            }
        })
    }

    fn listar(&self, host_id: i64, ruta: String) -> Futuro<Result<Vec<EntradaListado>, String>> {
        let cliente = self.cliente.clone();
        Box::pin(async move { listar_por_sftp(&cliente, host_id, ruta).await })
    }

    fn tests(
        &self,
        _host_id: i64,
        comando: String,
    ) -> Futuro<Result<(bool, String), nucleo::NoDisponible>> {
        let limite = self.limite;
        let hogar = self.hogar.clone();
        Box::pin(async move {
            Ok(magi_nucleo::deliberacion::tests::ejecutar(&comando, limite, &hogar).await)
        })
    }
}

/// `AbrirSftp` no interactivo y `ListarDir` de la ruta de los backups,
/// convertido al listado mínimo que entiende BALTHASAR-2 (D276).
async fn listar_por_sftp(
    cliente: &crate::cliente::Cliente,
    host_id: i64,
    ruta: String,
) -> Result<Vec<EntradaListado>, String> {
    use crate::protocolo::{MensajeCliente, MensajeServidor};
    let caido = |_| "sin servidor de sesiones".to_string();
    let abierto = cliente
        .peticion(|peticion_id| MensajeCliente::AbrirSftp {
            host_id,
            peticion_id: Some(peticion_id),
            no_interactivo: true,
        })
        .await
        .map_err(caido)?;
    match abierto {
        MensajeServidor::SftpAbierto { .. } => {}
        MensajeServidor::Error { mensaje, .. } => return Err(mensaje),
        _ => return Err("respuesta inesperada del servidor".to_string()),
    }
    let listado = cliente
        .peticion(|peticion_id| MensajeCliente::ListarDir {
            host_id,
            ruta,
            peticion_id,
        })
        .await
        .map_err(caido)?;
    match listado {
        MensajeServidor::DirListado { entradas, .. } => {
            Ok(entradas.into_iter().map(a_listado).collect())
        }
        MensajeServidor::Error { mensaje, .. } => Err(mensaje),
        _ => Err("respuesta inesperada del servidor".to_string()),
    }
}

/// La entrada del panel de Archivos, al listado mínimo de la deliberación.
fn a_listado(entrada: Entrada) -> EntradaListado {
    EntradaListado {
        nombre: entrada.nombre,
        tipo: match entrada.tipo {
            TipoEntrada::Fichero => TipoEntradaListado::Fichero,
            TipoEntrada::Directorio => TipoEntradaListado::Directorio,
            TipoEntrada::Enlace => TipoEntradaListado::Enlace,
        },
        tamano: entrada.tamano,
        mtime: entrada.mtime,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_veredicto_viaja_como_json_legible() {
        let json = serde_json::to_string(&Veredicto::Rechaza {
            detalle: "hace 31 h".to_string(),
            ms: 120,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"estado":"rechaza","detalle":"hace 31 h","ms":120}"#
        );
        let no_activa = serde_json::to_string(&Veredicto::NoActiva).unwrap();
        assert_eq!(no_activa, r#"{"estado":"n/a"}"#);
        let vuelta: Veredicto = serde_json::from_str(&json).unwrap();
        assert!(vuelta.rechaza());
    }

    #[test]
    fn validar_las_verificaciones() {
        assert!(validar_verificaciones(&DatosVerificaciones::default()).is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            backup_ruta: Some("/var/backups".to_string()),
            backup_patron: Some("*.sql.gz".to_string()),
            ..Default::default()
        })
        .is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup_patron: Some("[".to_string()),
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            tests: true,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn el_resultado_de_la_ejecucion_segun_los_hosts() {
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, false),
            EjecucionResultado::Ok
        );
        assert_eq!(
            EjecucionResultado::de_hosts(0, 3, false),
            EjecucionResultado::Error
        );
        assert_eq!(
            EjecucionResultado::de_hosts(2, 3, false),
            EjecucionResultado::Parcial
        );
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, true),
            EjecucionResultado::Cancelada
        );
    }

    #[test]
    fn el_detalle_lleva_el_id_y_se_recupera() {
        let deliberacion = RegistroDeliberacion {
            id: 12,
            fecha: String::new(),
            snippet_id: Some(1),
            accion: "reiniciar nginx → hetzner-01".to_string(),
            hosts: vec![(1, "hetzner-01".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 1,
                },
                backup: Veredicto::Rechaza {
                    detalle: "hace 31 h".to_string(),
                    ms: 1,
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Forzada,
            bloqueada: true,
            motivo: Some("revisado a mano".to_string()),
            usuario: "usuario".to_string(),
            ejecucion_resultado: None,
        };
        let detalle = detalle_registro(&deliberacion, Some(EjecucionResultado::Ok));
        assert!(detalle.starts_with("deliberación #12 · "));
        assert!(detalle.contains("consenso 1/2"));
        assert!(detalle.contains("motivo «revisado a mano»"));
        assert!(detalle.contains("resultado ok"));
        assert_eq!(id_en_detalle(&detalle), Some(12));
        assert_eq!(id_en_detalle("otra cosa"), None);
    }

    #[test]
    fn el_detalle_para_el_registro_lista_cada_comprobacion() {
        let deliberacion = RegistroDeliberacion {
            id: 4,
            fecha: String::new(),
            snippet_id: None,
            accion: "reiniciar nginx → hetzner-01".to_string(),
            hosts: vec![(1, "hetzner-01".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 1,
                },
                backup: Veredicto::Rechaza {
                    detalle: "hace 31 h\u{1b}[2J".to_string(),
                    ms: 1,
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Forzada,
            bloqueada: true,
            motivo: Some("revisado a mano".to_string()),
            usuario: "usuario".to_string(),
            ejecucion_resultado: Some(EjecucionResultado::Parcial),
        };
        let lineas = lineas_detalle(&deliberacion, 70);
        assert_eq!(lineas[0], "DELIBERACIÓN #4 · forzada · hubo rechazos");
        assert!(lineas.contains(&"Motivo:    revisado a mano".to_string()));
        assert!(lineas.contains(&"Ejecución: parcial".to_string()));
        assert!(lineas.contains(&"Consenso:  1 / 2".to_string()));
        assert!(lineas.contains(&"    MELCHIOR-1 salud   ✓ NOMINAL".to_string()));
        assert!(lineas.contains(&"    BALTHASAR-2 backup ✕ hace 31 h".to_string()));
        assert!(lineas.contains(&"    CASPER-3 tests     — no activa".to_string()));
        assert!(lineas.iter().all(|linea| !linea.contains('\u{1b}')));
        assert!(lineas.iter().all(|linea| linea.chars().count() <= 70));
    }

    #[test]
    fn el_usuario_del_sistema_no_esta_vacio() {
        assert!(!usuario_del_sistema().is_empty());
    }
}
