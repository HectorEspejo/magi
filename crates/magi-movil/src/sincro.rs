//! Sincronización del móvil en modo lector: adapta la carpeta SAF al trait del
//! núcleo, guarda la clave en LlaveroMovil y ejecuta la pasada en el hilo
//! escritor. Nunca escribe en la carpeta, nunca exporta y no hace copias.

use std::sync::Arc;

use anyhow::Context;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::{conflictos, equipos as equipos_bd, movil as movil_bd};
use magi_nucleo::conexion::huellas::entradas_de_host;
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::rutas::Rutas;
use magi_nucleo::sincronizacion::carpeta::{
    uuid_de_nombre, CarpetaSincronizacion, EntradaCarpeta, TAMANO_MAXIMO,
};
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::formato;
use magi_nucleo::sincronizacion::huellas;
use magi_nucleo::sincronizacion::lector::{pasada_lector, EstadoLectura};
use magi_nucleo::trabajo_vivo::TrabajoVivo;

use crate::api::{
    AlcanceBorrado, ConflictoHuella, EquipoSincronizacion, ErrorNucleo, EstadoEquipo,
    EstadoSincronizacion, FaseSincronizacion,
};
use crate::clave::{CUENTA_CLAVE, CUENTA_CLAVE_ANTERIOR};
use crate::estado::{con_almacen, EstadoNucleo, FaseSincro};
use crate::retrollamadas::CarpetaMovil;

/// Adaptador del callback SAF de Kotlin al trait de carpeta del núcleo.
pub struct CarpetaRetrollamada {
    movil: Arc<dyn CarpetaMovil>,
}

impl CarpetaRetrollamada {
    pub fn nueva(movil: Arc<dyn CarpetaMovil>) -> Self {
        Self { movil }
    }
}

impl CarpetaSincronizacion for CarpetaRetrollamada {
    fn listar(&self) -> Result<Vec<EntradaCarpeta>, String> {
        let ficheros = self.movil.listar().map_err(|error| error.to_string())?;
        Ok(ficheros
            .into_iter()
            .map(|fichero| EntradaCarpeta {
                nombre: fichero.nombre,
                tamano: fichero.tamano,
                modificado_ms: fichero.modificado_en.saturating_mul(1000),
            })
            .collect())
    }

    fn leer(&self, nombre: &str, maximo: u64) -> Result<Vec<u8>, String> {
        self.movil
            .leer(nombre.to_string(), maximo)
            .map_err(|error| error.to_string())
    }
}

/// Días que se conserva la clave anterior tras una rotación (F10 §7.4).
pub const DIAS_CLAVE_ANTERIOR: i64 = 30;

/// La clave anterior se guarda con su fecha límite y su texto separados
/// por un salto de línea; así solo lee mientras dura el plazo, aunque
/// la app se reinicie.
fn partir_anterior(texto: &str) -> Option<(i64, String)> {
    let (hasta, clave) = texto.split_once('\n')?;
    let hasta = hasta.trim().parse::<i64>().ok()?;
    Some((hasta, clave.to_string()))
}

fn juntar_anterior(hasta: i64, clave: &str) -> String {
    format!(
        "{hasta}
{clave}"
    )
}

/// Claves de sincronización guardadas en el llavero (la actual y, si existe y
/// no ha caducado, la anterior de una rotación).
pub fn claves_guardadas(estado: &EstadoNucleo) -> Vec<ClaveSincronizacion> {
    let mut claves = Vec::new();
    if let Ok(Some(texto)) = estado.llavero.leer(CUENTA_CLAVE) {
        if let Ok(clave) = ClaveSincronizacion::desde_texto(texto.as_str()) {
            claves.push(clave);
        }
    }
    if let Ok(Some(guardada)) = estado.llavero.leer(CUENTA_CLAVE_ANTERIOR) {
        match partir_anterior(guardada.as_str()) {
            Some((hasta, texto)) => {
                if chrono::Utc::now().timestamp() <= hasta {
                    if let Ok(clave) = ClaveSincronizacion::desde_texto(&texto) {
                        claves.push(clave);
                    }
                } else {
                    // Caducada: se olvida (la anterior solo lee 30 días).
                    let _ = estado.llavero.borrar(CUENTA_CLAVE_ANTERIOR);
                }
            }
            None => {
                // Formato antiguo sin fecha: se trata como caducada.
                let _ = estado.llavero.borrar(CUENTA_CLAVE_ANTERIOR);
            }
        }
    }
    claves
}

/// Normaliza el texto de una clave pegada o escaneada: sin espacios ni saltos
/// y en mayúsculas (texto canónico de la identidad).
pub fn normalizar_clave(texto: &str) -> String {
    texto
        .chars()
        .filter(|caracter| !caracter.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn nombre_de_uri(uri: &str) -> String {
    // La URI SAF viene codificada (primary%3ASync%2Fmagi): se decodifican los
    // separadores para quedarse con el nombre legible de la carpeta.
    let texto = uri
        .replace("%3A", ":")
        .replace("%3a", ":")
        .replace("%2F", "/")
        .replace("%2f", "/")
        .replace("%20", " ");
    let limpio = texto.trim_end_matches('/');
    let ultimo = limpio.rsplit('/').next().unwrap_or(limpio);
    if ultimo.is_empty() {
        "carpeta".to_string()
    } else {
        ultimo.to_string()
    }
}

/// Comprueba que la carpeta tiene al menos un fichero de MAGI.
fn carpeta_con_ficheros(carpeta: &dyn CarpetaSincronizacion) -> Result<(), ErrorNucleo> {
    let entradas = carpeta.listar().map_err(|detalle| ErrorNucleo::Carpeta {
        detalle: format!("no se pudo leer la carpeta: {detalle}"),
    })?;
    if entradas
        .iter()
        .any(|entrada| uuid_de_nombre(&entrada.nombre).is_some())
    {
        return Ok(());
    }
    Err(ErrorNucleo::Carpeta {
        detalle: "esta carpeta no tiene ficheros de MAGI: crea la sincronización en el escritorio primero"
            .to_string(),
    })
}

/// Guarda el árbol SAF elegido y comprueba que tiene ficheros de MAGI.
pub async fn elegir_carpeta(estado: &Arc<EstadoNucleo>, uri: String) -> Result<(), ErrorNucleo> {
    let carpeta = CarpetaRetrollamada::nueva(estado.retrollamadas.carpeta.clone());
    carpeta_con_ficheros(&carpeta)?;
    {
        let mut sincro = estado.sincro.lock().await;
        sincro.carpeta_uri = Some(uri.clone());
        sincro.carpeta_nombre = Some(nombre_de_uri(&uri));
        sincro.error = None;
        if claves_guardadas(estado).is_empty() {
            sincro.fase = FaseSincro::SinClave;
        }
    }
    estado.emitir_sincronizacion();
    if !claves_guardadas(estado).is_empty() {
        let _ = sincronizar_ahora(estado).await;
    }
    Ok(())
}

/// Une el móvil con la clave de sincronización (pegada o escaneada).
pub async fn unir_con_clave(estado: &Arc<EstadoNucleo>, texto: String) -> Result<(), ErrorNucleo> {
    let normalizado = normalizar_clave(&texto);
    let clave =
        ClaveSincronizacion::desde_texto(&normalizado).map_err(|error| ErrorNucleo::General {
            detalle: format!("la clave de sincronización no es válida: {error}"),
        })?;
    let carpeta = CarpetaRetrollamada::nueva(estado.retrollamadas.carpeta.clone());
    let entradas = carpeta.listar().map_err(|detalle| ErrorNucleo::Carpeta {
        detalle: format!("no se pudo leer la carpeta: {detalle}"),
    })?;
    let mut abre = false;
    let mut leidos = 0usize;
    for entrada in &entradas {
        if uuid_de_nombre(&entrada.nombre).is_none() || leidos >= 8 {
            continue;
        }
        leidos += 1;
        let Ok(bytes) = carpeta.leer(&entrada.nombre, TAMANO_MAXIMO) else {
            continue;
        };
        let Ok(plano) = clave.descifrar(&bytes) else {
            continue;
        };
        let Ok(en_claro) = formato::separar(&plano) else {
            continue;
        };
        if clave.verificar(en_claro.json.as_bytes(), &en_claro.firma) {
            abre = true;
            break;
        }
    }
    if !abre {
        return Err(ErrorNucleo::Carpeta {
            detalle: "la clave no abre los ficheros de esta carpeta".to_string(),
        });
    }
    // Si ya había una clave distinta, pasa a anterior: solo lee 30 días.
    let anterior = estado
        .llavero
        .leer(CUENTA_CLAVE)
        .ok()
        .flatten()
        .map(|texto| texto.to_string());
    let nueva = clave.texto().to_string();
    if let Some(anterior) = anterior {
        if anterior != nueva {
            let hasta = chrono::Utc::now().timestamp() + DIAS_CLAVE_ANTERIOR * 24 * 60 * 60;
            estado
                .llavero
                .guardar(CUENTA_CLAVE_ANTERIOR, &juntar_anterior(hasta, &anterior))
                .map_err(|detalle| ErrorNucleo::Llavero { detalle })?;
        }
    }
    estado
        .llavero
        .guardar(CUENTA_CLAVE, &nueva)
        .map_err(|detalle| ErrorNucleo::Llavero { detalle })?;
    {
        let mut sincro = estado.sincro.lock().await;
        sincro.huella_clave = Some(clave.huella_corta());
        sincro.error = None;
    }
    sincronizar_ahora(estado).await?;
    Ok(())
}

/// Pasada en modo lector. Si ya hay una en curso, no hace nada.
pub async fn sincronizar_ahora(estado: &Arc<EstadoNucleo>) -> Result<(), ErrorNucleo> {
    {
        let mut sincro = estado.sincro.lock().await;
        if sincro.fase == FaseSincro::Sincronizando {
            return Ok(());
        }
        if sincro.carpeta_uri.is_none() {
            sincro.fase = FaseSincro::SinConfigurar;
            return Ok(());
        }
        if claves_guardadas(estado).is_empty() {
            sincro.fase = FaseSincro::SinClave;
            return Ok(());
        }
        sincro.fase = FaseSincro::Sincronizando;
        sincro.error = None;
    }
    estado.emitir_sincronizacion();
    let claves = claves_guardadas(estado);
    let carpeta = Arc::new(CarpetaRetrollamada::nueva(
        estado.retrollamadas.carpeta.clone(),
    ));
    let trabajo = estado.trabajo_vivo();
    let rutas: Rutas = estado.rutas.clone();
    let carpeta_pasada = carpeta.clone();
    let resultado = estado
        .escritor
        .con_almacen_async(move |almacen| {
            pasada_lector(
                almacen.conexion(),
                &rutas,
                carpeta_pasada.as_ref(),
                &claves,
                &trabajo as &dyn TrabajoVivo,
            )
        })
        .await
        .map_err(|detalle| ErrorNucleo::General { detalle })?;
    let resultado = match resultado {
        Ok(resultado) => resultado,
        Err(error) => {
            let mut sincro = estado.sincro.lock().await;
            sincro.fase = FaseSincro::ErrorCarpeta;
            sincro.error = Some(format!("la pasada de sincronización falló: {error:#}"));
            drop(sincro);
            estado.emitir_sincronizacion();
            return Err(ErrorNucleo::General {
                detalle: format!("la pasada de sincronización falló: {error:#}"),
            });
        }
    };
    let ahora = chrono::Utc::now().timestamp();
    {
        let mut sincro = estado.sincro.lock().await;
        sincro.ultima_pasada = Some(ahora);
        sincro.avisos = resultado.avisos.clone();
        sincro.error = None;
        sincro.fase = match &resultado.estado {
            EstadoLectura::SinClave => FaseSincro::SinClave,
            EstadoLectura::AlDia => FaseSincro::Activa,
            EstadoLectura::EsperandoClave(_) => FaseSincro::EsperandoClave,
            EstadoLectura::Quitado => {
                sincro.avisos.push(
                    "este móvil ha sido quitado de la sincronización; únete de nuevo".to_string(),
                );
                FaseSincro::SinConfigurar
            }
            EstadoLectura::ErrorCarpeta(motivo) => {
                sincro.error = Some(motivo.clone());
                FaseSincro::ErrorCarpeta
            }
        };
    }
    estado.emitir_sincronizacion();
    Ok(())
}

/// Deja de sincronizar: borra la clave y el inventario (conserva la clave del
/// móvil, las contraseñas y las huellas).
pub async fn dejar_de_sincronizar(estado: &Arc<EstadoNucleo>) -> Result<(), ErrorNucleo> {
    comprobar_sin_trabajo(estado)?;
    estado
        .llavero
        .borrar(CUENTA_CLAVE)
        .map_err(|detalle| ErrorNucleo::Llavero { detalle })?;
    estado
        .llavero
        .borrar(CUENTA_CLAVE_ANTERIOR)
        .map_err(|detalle| ErrorNucleo::Llavero { detalle })?;
    borrar_inventario(estado)?;
    {
        let mut sincro = estado.sincro.lock().await;
        *sincro = Default::default();
    }
    estado.emitir_sincronizacion();
    Ok(())
}

/// Borra los datos del móvil según el alcance.
pub async fn borrar_datos(
    estado: &Arc<EstadoNucleo>,
    alcance: AlcanceBorrado,
) -> Result<(), ErrorNucleo> {
    comprobar_sin_trabajo(estado)?;
    let todo = matches!(alcance, AlcanceBorrado::Todo);
    let uuids = con_almacen(estado, |almacen| {
        movil_bd::uuid_por_id(almacen.conexion(), Tabla::Hosts)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los hosts: {error:#}"),
    })?;
    for uuid in uuids.values() {
        let _ = estado
            .llavero
            .borrar(&crate::clave::cuenta_contrasena(uuid));
    }
    let _ = estado.llavero.borrar(CUENTA_CLAVE);
    let _ = estado.llavero.borrar(CUENTA_CLAVE_ANTERIOR);
    if todo {
        let _ = estado.retrollamadas.firmante.olvidar_clave();
        let ruta_known_hosts = estado.rutas.fichero_known_hosts();
        let _ = std::fs::remove_file(&ruta_known_hosts);
    }
    borrar_inventario(estado)?;
    if todo {
        let _ = con_almacen(estado, |almacen| {
            almacen.conexion().execute_batch("DELETE FROM REGISTRO;")
        });
    }
    {
        let mut sincro = estado.sincro.lock().await;
        *sincro = Default::default();
    }
    estado.emitir_sincronizacion();
    Ok(())
}

fn comprobar_sin_trabajo(estado: &EstadoNucleo) -> Result<(), ErrorNucleo> {
    let hay_sesiones = estado
        .sesiones
        .lock()
        .map(|sesiones| !sesiones.is_empty())
        .unwrap_or(false);
    let hay_tuneles = estado
        .tuneles
        .lock()
        .map(|tuneles| !tuneles.is_empty())
        .unwrap_or(false);
    if hay_sesiones || hay_tuneles {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "cierra antes las sesiones y los túneles".to_string(),
        });
    }
    Ok(())
}

/// Vacía el inventario del móvil dejando viva la fila de este equipo.
fn borrar_inventario(estado: &EstadoNucleo) -> Result<(), ErrorNucleo> {
    con_almacen(estado, |almacen| {
        let conexion = almacen.conexion();
        conexion
            .execute_batch(
                "DELETE FROM HOST_ETIQUETAS;
                 DELETE FROM SNIPPET_DESTINOS;
                 DELETE FROM VERIFICACIONES_HOST;
                 DELETE FROM TUNELES;
                 DELETE FROM HOSTS;
                 DELETE FROM GRUPOS;
                 DELETE FROM ETIQUETAS;
                 DELETE FROM SNIPPETS;
                 DELETE FROM IDENTIDADES;
                 DELETE FROM SINCRONIZACIONES_DIR;
                 DELETE FROM LAPIDAS;
                 DELETE FROM CONFLICTOS;
                 DELETE FROM EQUIPOS WHERE es_este = 0;",
            )
            .context("vaciando el inventario")?;
        conexion.execute(
            "UPDATE EQUIPOS SET estado = 'este', ultimo_fichero_en = NULL,
             ultima_importacion_en = NULL, hash_ultimo_fichero = NULL,
             quitado_en = NULL WHERE es_este = 1",
            [],
        )?;
        Ok::<_, anyhow::Error>(())
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo vaciar el inventario: {error:#}"),
    })?;
    Ok(())
}

/// Estado público de la sincronización para la app.
pub fn estado_publico(estado: &EstadoNucleo) -> Result<EstadoSincronizacion, ErrorNucleo> {
    let (equipos, conflictos_pendientes) = con_almacen(estado, |almacen| {
        let conexion = almacen.conexion();
        let equipos = equipos_bd::listar(conexion)?;
        let conflictos = conflictos::listar_pendientes(conexion)?
            .into_iter()
            .filter(|conflicto| conflicto.tipo == conflictos::TIPO_HUELLA)
            .count();
        Ok::<_, anyhow::Error>((equipos, conflictos))
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo la sincronización: {error:#}"),
    })?;
    let (fase, carpeta_uri, carpeta_nombre, huella_clave, ultima_pasada, avisos, error) = {
        match estado.sincro.try_lock() {
            Ok(sincro) => (
                sincro.fase,
                sincro.carpeta_uri.clone(),
                sincro.carpeta_nombre.clone(),
                sincro.huella_clave.clone(),
                sincro.ultima_pasada,
                sincro.avisos.clone(),
                sincro.error.clone(),
            ),
            Err(_) => (
                FaseSincro::Sincronizando,
                None,
                None,
                None,
                None,
                Vec::new(),
                None,
            ),
        }
    };
    let mut equipos_publicos: Vec<EquipoSincronizacion> = equipos
        .into_iter()
        .map(|equipo| EquipoSincronizacion {
            nombre: equipo.nombre,
            plataforma: equipo.plataforma,
            version: equipo.app_version.unwrap_or_default(),
            ultimo_fichero: equipo.ultimo_fichero_en.clone(),
            estado: match equipo.estado.as_str() {
                equipos_bd::ESTADO_ESTE => EstadoEquipo::EsteEquipo,
                equipos_bd::ESTADO_AL_DIA => EstadoEquipo::AlDia,
                equipos_bd::ESTADO_ILEGIBLE => EstadoEquipo::Ilegible,
                equipos_bd::ESTADO_DANADO => EstadoEquipo::Danado,
                equipos_bd::ESTADO_FORMATO_NUEVO => EstadoEquipo::FormatoNuevo,
                _ => EstadoEquipo::Quitado,
            },
            ultima_vez: equipo
                .ultimo_fichero_en
                .as_deref()
                .and_then(|fecha| chrono::DateTime::parse_from_rfc3339(fecha).ok())
                .map(|fecha| fecha.timestamp()),
        })
        .collect();
    equipos_publicos.sort_by(|a, b| {
        let peso = |estado: EstadoEquipo| match estado {
            EstadoEquipo::EsteEquipo => 0,
            _ => 1,
        };
        peso(a.estado)
            .cmp(&peso(b.estado))
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    Ok(EstadoSincronizacion {
        fase: match fase {
            FaseSincro::SinConfigurar => FaseSincronizacion::SinConfigurar,
            FaseSincro::SinClave => FaseSincronizacion::SinClave,
            FaseSincro::Activa => FaseSincronizacion::Activa,
            FaseSincro::Sincronizando => FaseSincronizacion::Sincronizando,
            FaseSincro::EsperandoClave => FaseSincronizacion::EsperandoClave,
            FaseSincro::ErrorCarpeta => FaseSincronizacion::ErrorCarpeta,
        },
        carpeta_uri,
        carpeta_nombre,
        huella_corta_clave: huella_clave,
        ultima_pasada,
        equipos: equipos_publicos,
        conflictos_huella: conflictos_pendientes.min(u32::MAX as usize) as u32,
        error: error.or_else(|| avisos.first().cloned()),
    })
}

/// Conflictos de huella pendientes en el móvil.
pub fn conflictos_huella(estado: &EstadoNucleo) -> Result<Vec<ConflictoHuella>, ErrorNucleo> {
    let ruta_known_hosts = estado.rutas.fichero_known_hosts();
    let conflictos = con_almacen(estado, move |almacen| {
        let conexion = almacen.conexion();
        let mut lista = Vec::new();
        for conflicto in conflictos::listar_pendientes(conexion)? {
            if conflicto.tipo != conflictos::TIPO_HUELLA {
                continue;
            }
            let Some(datos) = huellas::datos_huella_ganadora(&conflicto) else {
                continue;
            };
            let host = movil_bd::host_por_uuid(conexion, &conflicto.uuid)?;
            let huella_mia = match &host {
                Some(host) => entradas_de_host(&ruta_known_hosts, &host.direccion, host.puerto)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(tipo, huella)| format!("{tipo} {huella}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                None => String::new(),
            };
            lista.push((conflicto, datos, huella_mia));
        }
        Ok::<_, anyhow::Error>(lista)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los conflictos: {error:#}"),
    })?;
    Ok(conflictos
        .into_iter()
        .map(|(conflicto, datos, huella_mia)| ConflictoHuella {
            id: conflicto.id.max(0) as u64,
            host_uuid: conflicto.uuid,
            host_nombre: conflicto.nombre,
            direccion: datos.nombre,
            huella_mia,
            huella_otra: datos.huella,
            equipo_origen: Some(datos.equipo).filter(|equipo| !equipo.is_empty()),
        })
        .collect())
}

/// Acepta la huella del otro equipo: sustituye la del known_hosts del móvil.
pub async fn aceptar_huella_de_otro_equipo(
    estado: &Arc<EstadoNucleo>,
    id: u64,
) -> Result<(), ErrorNucleo> {
    let ruta = estado.rutas.fichero_known_hosts();
    con_almacen(estado, move |almacen| {
        let conexion = almacen.conexion();
        let conflicto = conflictos::obtener(conexion, id as i64)?
            .with_context(|| format!("no existe el conflicto {id}"))?;
        huellas::aceptar_conflicto(conexion, &ruta, &conflicto)?;
        conflictos::resolver(conexion, conflicto.id, conflictos::RESOLUCION_ACEPTADO)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("aceptando la huella: {error:#}"),
    })?;
    estado.emitir_sincronizacion();
    Ok(())
}

/// Mantiene la huella del móvil y descarta la del otro equipo.
pub async fn mantener_mi_huella(estado: &Arc<EstadoNucleo>, id: u64) -> Result<(), ErrorNucleo> {
    con_almacen(estado, move |almacen| {
        conflictos::resolver(
            almacen.conexion(),
            id as i64,
            conflictos::RESOLUCION_DESCARTADO,
        )
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("manteniendo la huella: {error:#}"),
    })?;
    estado.emitir_sincronizacion();
    Ok(())
}

/// Anota en REGISTRO que la clave del móvil se creó (primera vez).
pub fn anotar_clave_generada(estado: &EstadoNucleo) {
    let _ = estado.escritor.con_almacen(|almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::CLAVE_GENERADA,
            None,
            None,
            "clave del móvil creada en el Keystore",
            ResultadoRegistro::Ok,
        )
    });
}

/// Los ficheros de la carpeta tal como los ve la app (para pruebas y avisos).
pub fn listar_carpeta(estado: &EstadoNucleo) -> Result<Vec<EntradaCarpeta>, ErrorNucleo> {
    let carpeta = CarpetaRetrollamada::nueva(estado.retrollamadas.carpeta.clone());
    carpeta
        .listar()
        .map_err(|detalle| ErrorNucleo::Carpeta { detalle })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_clave_pegada_se_normaliza_sin_espacios_y_en_mayusculas() {
        let texto = "  age-secret-key-1 abc \n  def\t";
        assert_eq!(normalizar_clave(texto), "AGE-SECRET-KEY-1ABCDEF");
    }

    #[test]
    fn la_clave_anterior_se_guarda_con_su_plazo() {
        let hasta = 1_900_000_000;
        let guardada = juntar_anterior(hasta, "AGE-SECRET-KEY-1PRUEBA");
        let (leida, clave) = partir_anterior(&guardada).expect("formato válido");
        assert_eq!(leida, hasta);
        assert_eq!(clave, "AGE-SECRET-KEY-1PRUEBA");
    }

    #[test]
    fn un_formato_antiguo_sin_fecha_no_se_acepta() {
        assert!(partir_anterior("AGE-SECRET-KEY-1PRUEBA").is_none());
        assert!(partir_anterior("no-numero\nclave").is_none());
    }

    #[test]
    fn el_nombre_de_la_carpeta_sale_de_la_uri_saf() {
        let uri = "content://com.android.externalstorage.documents/tree/primary%3ASync%2Fmagi";
        assert_eq!(nombre_de_uri(uri), "magi");
        assert_eq!(
            nombre_de_uri("content://x/tree/primary%3ASync%2Fmagi/documents"),
            "documents"
        );
    }
}
