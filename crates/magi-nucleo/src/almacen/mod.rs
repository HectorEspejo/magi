use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

pub mod comprobaciones_servicio;
pub mod conflictos;
pub mod deliberaciones;
pub mod equipos;
pub mod etiquetas;
pub mod grupos;
pub mod hosts;
pub mod identidades;
pub mod lapidas;
pub mod migraciones;
pub mod movil;
pub mod registro;
pub mod sesiones_persistentes;
pub mod sincronizaciones;
pub mod snippets;
pub mod sondeos;
pub mod tuneles;
pub mod verificaciones;
pub mod versiones;

/// Abre una transacción de escritura con `BEGIN IMMEDIATE`: toma el bloqueo
/// desde el principio para que `busy_timeout` espere de verdad. Con dos
/// procesos compartiendo `magi.db` (la TUI y el servidor de sesiones), una
/// transacción deferida que empieza leyendo y luego escribe recibe
/// `SQLITE_BUSY` en la promoción sin pasar por el plazo de espera.
pub fn transaccion(conexion: &Connection) -> Result<rusqlite::Transaction<'_>> {
    Ok(rusqlite::Transaction::new_unchecked(
        conexion,
        rusqlite::TransactionBehavior::Immediate,
    )?)
}

/// Fichero SQLite del inventario.
pub struct Almacen {
    conexion: Connection,
    ruta: Option<PathBuf>,
    /// Hosts cuya marca `tmux_persistente` (Fase 15) pasó a
    /// `sesiones_persistentes` al abrir (migración 11). Distinto de cero solo
    /// en la apertura que migra: es para avisar una sola vez.
    hosts_con_marca_migrada: usize,
}

impl Almacen {
    /// Abre (o crea) la base de datos. La identidad del equipo y la versión de
    /// la app vienen inyectadas: el núcleo no lee el entorno.
    pub fn abrir(ruta: &Path, equipo: &crate::rutas::Equipo, app_version: &str) -> Result<Self> {
        if let Some(directorio) = ruta.parent() {
            crear_directorio_privado(directorio)?;
        }
        // Antes incluso de abrir la conexión: una base de datos de una versión
        // más nueva de MAGI no se abre ni se toca (Fase 9). Abrirla en lectura/
        // escritura aplicaría un WAL pendiente y la modificaría.
        migraciones::comprobar_version_en_disco(ruta)?;
        let conexion =
            Connection::open(ruta).with_context(|| format!("abriendo {}", ruta.display()))?;
        migraciones::comprobar_version(&conexion)?;
        fs::set_permissions(ruta, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("ajustando permisos de {}", ruta.display()))?;
        let mut almacen = Self {
            conexion,
            ruta: Some(ruta.to_path_buf()),
            hosts_con_marca_migrada: 0,
        };
        almacen.ajustes(true)?;
        let migracion = migraciones::aplicar(&mut almacen.conexion, equipo, app_version)?;
        almacen.hosts_con_marca_migrada = migracion.hosts_con_marca_migrada;
        Ok(almacen)
    }

    /// Almacén en memoria para tests, con una identidad de equipo de pruebas.
    pub fn abrir_en_memoria() -> Result<Self> {
        Self::abrir_en_memoria_con(&crate::rutas::Equipo::nuevo("pruebas", "linux"), "0.0.0")
    }

    /// Almacén en memoria con la identidad de equipo que se le pase.
    pub fn abrir_en_memoria_con(equipo: &crate::rutas::Equipo, app_version: &str) -> Result<Self> {
        let conexion = Connection::open_in_memory().context("abriendo SQLite en memoria")?;
        let mut almacen = Self {
            conexion,
            ruta: None,
            hosts_con_marca_migrada: 0,
        };
        almacen.ajustes(false)?;
        let migracion = migraciones::aplicar(&mut almacen.conexion, equipo, app_version)?;
        almacen.hosts_con_marca_migrada = migracion.hosts_con_marca_migrada;
        Ok(almacen)
    }

    fn ajustes(&mut self, en_fichero: bool) -> Result<()> {
        if en_fichero {
            self.conexion
                .pragma_update(None, "journal_mode", "WAL")
                .context("activando WAL")?;
        }
        self.conexion
            .pragma_update(None, "foreign_keys", "ON")
            .context("activando foreign_keys")?;
        self.conexion
            .pragma_update(None, "busy_timeout", 5000)
            .context("configurando busy_timeout")?;
        Ok(())
    }

    pub fn conexion(&self) -> &Connection {
        &self.conexion
    }

    /// Checkpoint de WAL al salir.
    pub fn cerrar(self) -> Result<()> {
        self.conexion
            .pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .context("checkpoint de WAL")?;
        Ok(())
    }

    pub fn ruta(&self) -> Option<&Path> {
        self.ruta.as_deref()
    }

    /// Cuántos hosts migraron su marca `tmux_persistente` a
    /// `sesiones_persistentes` al abrir (migración 11). Cero salvo en la
    /// apertura que aplica la migración.
    pub fn hosts_con_marca_migrada(&self) -> usize {
        self.hosts_con_marca_migrada
    }

    /// Aviso de la migración 11 para enseñar una sola vez: la marca de
    /// sesiones persistentes de la Fase 15 ahora viaja con la sincronización.
    /// `None` si esta apertura no migró ningún host (también en las
    /// siguientes, que encuentran la migración ya aplicada).
    pub fn aviso_de_migracion(&self) -> Option<String> {
        aviso_marca_migrada(self.hosts_con_marca_migrada)
    }

    // Hosts -----------------------------------------------------------------

    pub fn listar_hosts(&self) -> Result<Vec<crate::modelo::Host>> {
        hosts::listar(&self.conexion)
    }

    pub fn obtener_host(&self, id: i64) -> Result<crate::modelo::Host> {
        hosts::obtener(&self.conexion, id)
    }

    pub fn crear_host(
        &self,
        datos: &crate::modelo::DatosHost,
        origen: crate::modelo::Origen,
    ) -> Result<i64> {
        hosts::crear(&self.conexion, datos, origen)
    }

    pub fn actualizar_host(&self, id: i64, datos: &crate::modelo::DatosHost) -> Result<()> {
        hosts::actualizar(&self.conexion, id, datos)
    }

    pub fn borrar_host(&self, id: i64) -> Result<()> {
        hosts::borrar(&self.conexion, id)
    }

    pub fn existe_nombre_host(&self, nombre: &str, excluyendo: Option<i64>) -> Result<bool> {
        hosts::existe_nombre(&self.conexion, nombre, excluyendo)
    }

    pub fn dependientes_de_salto(&self, id: i64) -> Result<i64> {
        hosts::dependientes_de_salto(&self.conexion, id)
    }

    pub fn mover_host_a_grupo(&self, id: i64, grupo_id: Option<i64>) -> Result<()> {
        hosts::mover_a_grupo(&self.conexion, id, grupo_id)
    }

    pub fn marcar_origen(&self, id: i64, origen: crate::modelo::Origen) -> Result<()> {
        hosts::marcar_origen(&self.conexion, id, origen)
    }

    pub fn fijar_salto(&self, id: i64, salto_host_id: Option<i64>) -> Result<()> {
        hosts::fijar_salto(&self.conexion, id, salto_host_id)
    }

    pub fn anadir_opciones_extra(&self, id: i64, linea: &str) -> Result<()> {
        hosts::anadir_opciones_extra(&self.conexion, id, linea)
    }

    pub fn marcar_conexion(&self, id: i64) -> Result<()> {
        hosts::marcar_conexion(&self.conexion, id)
    }

    pub fn marcar_identidad_ref(
        &self,
        id: i64,
        identidad: &crate::modelo::IdentidadRef,
    ) -> Result<()> {
        hosts::marcar_identidad_ref(&self.conexion, id, identidad)
    }

    pub fn marcar_estado(
        &self,
        id: i64,
        estado: Option<crate::modelo::UltimoEstado>,
    ) -> Result<()> {
        hosts::marcar_estado(&self.conexion, id, estado)
    }

    /// Fija (o borra) la huella del certificado RDP aceptado (Fase 12). Es
    /// propia del equipo: no sube la versión del host ni viaja.
    pub fn fijar_huella_escritorio(&self, id: i64, huella: Option<&str>) -> Result<()> {
        hosts::fijar_huella_escritorio(&self.conexion, id, huella)
    }

    /// Últimos directorios de la vista Archivos de un host (F4).
    pub fn fijar_dirs_sftp(
        &self,
        id: i64,
        dir_local: Option<&str>,
        dir_remoto: Option<&str>,
    ) -> Result<()> {
        hosts::fijar_dirs_sftp(&self.conexion, id, dir_local, dir_remoto)
    }

    pub fn fijar_snippet_al_conectar(&self, id: i64, snippet_id: Option<i64>) -> Result<()> {
        hosts::fijar_snippet_al_conectar(&self.conexion, id, snippet_id)
    }

    /// Reconexión automática de las pestañas de este host (Fase 15). Propia
    /// del equipo: no sube la versión del host ni viaja.
    pub fn fijar_reconectar(&self, id: i64, reconectar: bool) -> Result<()> {
        hosts::fijar_reconectar(&self.conexion, id, reconectar)
    }

    /// Sesión persistente con tmux de este host (Fase 15). Propia del equipo.
    pub fn fijar_tmux_persistente(&self, id: i64, persistente: bool) -> Result<()> {
        hosts::fijar_tmux_persistente(&self.conexion, id, persistente)
    }

    /// Permiso del portapapeles del remoto (OSC 52) de este host (Fase 15).
    /// Propia del equipo; leer el portapapeles, nunca.
    pub fn fijar_portapapeles_remoto(&self, id: i64, permitido: bool) -> Result<()> {
        hosts::fijar_portapapeles_remoto(&self.conexion, id, permitido)
    }

    // Snippets --------------------------------------------------------------

    pub fn listar_snippets(&self) -> Result<Vec<crate::snippets::Snippet>> {
        snippets::listar(&self.conexion)
    }

    pub fn obtener_snippet(&self, id: i64) -> Result<crate::snippets::Snippet> {
        snippets::obtener(&self.conexion, id)
    }

    pub fn snippet_por_nombre(&self, nombre: &str) -> Result<Option<crate::snippets::Snippet>> {
        snippets::por_nombre(&self.conexion, nombre)
    }

    pub fn crear_snippet(&self, datos: &crate::snippets::DatosSnippet) -> Result<i64> {
        snippets::crear(&self.conexion, datos)
    }

    pub fn actualizar_snippet(&self, id: i64, datos: &crate::snippets::DatosSnippet) -> Result<()> {
        snippets::actualizar(&self.conexion, id, datos)
    }

    pub fn borrar_snippet(&self, id: i64) -> Result<()> {
        snippets::borrar(&self.conexion, id)
    }

    pub fn marcar_uso_snippet(&self, id: i64) -> Result<()> {
        snippets::marcar_uso(&self.conexion, id)
    }

    pub fn hosts_con_snippet_al_conectar(&self, id: i64) -> Result<Vec<(i64, String)>> {
        snippets::hosts_con_snippet_al_conectar(&self.conexion, id)
    }

    // Comprobaciones de servicio (Fase 13) ---------------------------------

    pub fn comprobaciones_de_host(
        &self,
        host_id: i64,
    ) -> Result<Vec<crate::modelo::ComprobacionServicio>> {
        comprobaciones_servicio::de_host(&self.conexion, host_id)
    }

    pub fn comprobaciones_servicio(&self) -> Result<Vec<crate::modelo::ComprobacionServicio>> {
        comprobaciones_servicio::listar(&self.conexion)
    }

    pub fn obtener_comprobacion(
        &self,
        host_id: i64,
        tipo: crate::servicios::TipoObjetivo,
        objetivo: &str,
    ) -> Result<Option<crate::modelo::ComprobacionServicio>> {
        comprobaciones_servicio::obtener(&self.conexion, host_id, tipo, objetivo)
    }

    /// Crea o actualiza (upsert por host, tipo y objetivo) la comprobación
    /// propia de un objetivo y devuelve su id.
    pub fn guardar_comprobacion(
        &self,
        host_id: i64,
        tipo: crate::servicios::TipoObjetivo,
        objetivo: &str,
        comando: &str,
    ) -> Result<i64> {
        comprobaciones_servicio::guardar(&self.conexion, host_id, tipo, objetivo, comando)
    }

    /// Borra una comprobación dejando su lápida (Fase 10).
    pub fn borrar_comprobacion(&self, id: i64) -> Result<()> {
        comprobaciones_servicio::borrar(&self.conexion, id)
    }

    // Sesiones persistentes (Fase 16) ----------------------------------------

    /// Todas las fichas, por host y nombre. Las filas ilegibles se saltan con
    /// un aviso en el log.
    pub fn listar_fichas(&self) -> Result<Vec<crate::sesiones_persistentes::Ficha>> {
        sesiones_persistentes::listar(&self.conexion)
    }

    /// Fichas de un host, por nombre.
    pub fn fichas_de_host(&self, host_id: i64) -> Result<Vec<crate::sesiones_persistentes::Ficha>> {
        sesiones_persistentes::de_host(&self.conexion, host_id)
    }

    pub fn obtener_ficha(&self, id: i64) -> Result<Option<crate::sesiones_persistentes::Ficha>> {
        sesiones_persistentes::obtener(&self.conexion, id)
    }

    /// La ficha de un host con ese nombre exacto, si existe.
    pub fn ficha_por_nombre(
        &self,
        host_id: i64,
        nombre: &str,
    ) -> Result<Option<crate::sesiones_persistentes::Ficha>> {
        sesiones_persistentes::por_nombre(&self.conexion, host_id, nombre)
    }

    /// Nombres de todas las fichas de un host (también las que no se pueden
    /// leer): están reservados para los nombres automáticos y propuestos.
    pub fn nombres_de_fichas_de_host(&self, host_id: i64) -> Result<Vec<String>> {
        sesiones_persistentes::nombres_de_host(&self.conexion, host_id)
    }

    /// Crea la ficha de una sesión que ya existe en el host. Solo hosts `ssh`.
    pub fn crear_ficha(
        &self,
        host_id: i64,
        datos: &crate::sesiones_persistentes::DatosFicha,
    ) -> Result<i64> {
        sesiones_persistentes::crear(&self.conexion, host_id, datos)
    }

    /// Edita la ficha (nombre, carpeta, comandos y nota): sube su versión si
    /// algo cambió.
    pub fn actualizar_ficha(
        &self,
        id: i64,
        datos: &crate::sesiones_persistentes::DatosFicha,
    ) -> Result<()> {
        sesiones_persistentes::actualizar(&self.conexion, id, datos)
    }

    /// Cambia solo el nombre de la ficha. El de la sesión en el host lo
    /// cambia el servidor, antes.
    pub fn renombrar_ficha(&self, id: i64, nombre: &str) -> Result<()> {
        sesiones_persistentes::renombrar(&self.conexion, id, nombre)
    }

    /// Borra la ficha dejando su lápida. Nunca termina la sesión del host.
    pub fn borrar_ficha(&self, id: i64) -> Result<()> {
        sesiones_persistentes::borrar(&self.conexion, id)
    }

    /// Marca o desmarca un host para que toda pestaña nueva sea persistente.
    /// Sube la versión del host: la marca viaja.
    pub fn fijar_sesiones_persistentes(&self, id: i64, marcado: bool) -> Result<()> {
        hosts::fijar_sesiones_persistentes(&self.conexion, id, marcado)
    }

    // Verificaciones previas y deliberaciones -----------------------------

    pub fn verificaciones_de_host(
        &self,
        host_id: i64,
    ) -> Result<Option<crate::deliberacion::Verificaciones>> {
        verificaciones::de_host(&self.conexion, host_id)
    }

    pub fn verificaciones_por_host(
        &self,
    ) -> Result<std::collections::HashMap<i64, crate::deliberacion::Verificaciones>> {
        verificaciones::por_host(&self.conexion)
    }

    pub fn guardar_verificaciones(
        &self,
        host_id: i64,
        datos: &crate::deliberacion::DatosVerificaciones,
    ) -> Result<()> {
        verificaciones::guardar(&self.conexion, host_id, datos)
    }

    pub fn crear_deliberacion(
        &self,
        nueva: &crate::deliberacion::NuevaDeliberacion,
    ) -> Result<i64> {
        deliberaciones::crear(&self.conexion, nueva)
    }

    pub fn obtener_deliberacion(
        &self,
        id: i64,
    ) -> Result<crate::deliberacion::RegistroDeliberacion> {
        deliberaciones::obtener(&self.conexion, id)
    }

    pub fn fijar_resultado_deliberacion(
        &self,
        id: i64,
        resultado: crate::deliberacion::EjecucionResultado,
    ) -> Result<bool> {
        deliberaciones::fijar_resultado_ejecucion(&self.conexion, id, resultado)
    }

    // Túneles ---------------------------------------------------------------

    pub fn listar_tuneles(&self) -> Result<Vec<crate::modelo::Tunel>> {
        tuneles::listar(&self.conexion)
    }

    pub fn tuneles_de_host(&self, host_id: i64) -> Result<Vec<crate::modelo::Tunel>> {
        tuneles::de_host(&self.conexion, host_id)
    }

    pub fn tuneles_por_host(
        &self,
    ) -> Result<std::collections::HashMap<i64, Vec<crate::modelo::Tunel>>> {
        tuneles::por_host(&self.conexion)
    }

    pub fn obtener_tunel(&self, id: i64) -> Result<crate::modelo::Tunel> {
        tuneles::obtener(&self.conexion, id)
    }

    pub fn tunel_por_nombre(
        &self,
        host_id: i64,
        nombre: &str,
    ) -> Result<Option<crate::modelo::Tunel>> {
        tuneles::por_nombre(&self.conexion, host_id, nombre)
    }

    pub fn crear_tunel(&self, datos: &crate::modelo::DatosTunel) -> Result<i64> {
        tuneles::crear(&self.conexion, datos)
    }

    pub fn actualizar_tunel(&self, id: i64, datos: &crate::modelo::DatosTunel) -> Result<()> {
        tuneles::actualizar(&self.conexion, id, datos)
    }

    pub fn borrar_tunel(&self, id: i64) -> Result<()> {
        tuneles::borrar(&self.conexion, id)
    }

    pub fn alternar_tunel_automatico(&self, id: i64, automatico: bool) -> Result<()> {
        tuneles::alternar_automatico(&self.conexion, id, automatico)
    }

    // Sincronizaciones guardadas (Fase 8) ------------------------------------

    pub fn listar_sincronizaciones(&self) -> Result<Vec<crate::modelo::Sincronizacion>> {
        sincronizaciones::listar(&self.conexion)
    }

    pub fn sincronizaciones_de_host(
        &self,
        host_id: i64,
    ) -> Result<Vec<crate::modelo::Sincronizacion>> {
        sincronizaciones::de_host(&self.conexion, host_id)
    }

    pub fn obtener_sincronizacion(&self, id: i64) -> Result<crate::modelo::Sincronizacion> {
        sincronizaciones::obtener(&self.conexion, id)
    }

    pub fn crear_sincronizacion(&self, datos: &crate::modelo::DatosSincronizacion) -> Result<i64> {
        sincronizaciones::crear(&self.conexion, datos)
    }

    pub fn actualizar_sincronizacion(
        &self,
        id: i64,
        datos: &crate::modelo::DatosSincronizacion,
    ) -> Result<()> {
        sincronizaciones::actualizar(&self.conexion, id, datos)
    }

    pub fn borrar_sincronizacion(&self, id: i64) -> Result<()> {
        sincronizaciones::borrar(&self.conexion, id)
    }

    pub fn marcar_ejecucion_sincronizacion(&self, id: i64) -> Result<()> {
        sincronizaciones::marcar_ejecucion(&self.conexion, id)
    }

    // Grupos ----------------------------------------------------------------

    pub fn listar_grupos(&self) -> Result<Vec<crate::modelo::Grupo>> {
        grupos::listar(&self.conexion)
    }

    pub fn crear_grupo(&self, nombre: &str) -> Result<i64> {
        grupos::crear(&self.conexion, nombre)
    }

    pub fn renombrar_grupo(&self, id: i64, nombre: &str) -> Result<()> {
        grupos::renombrar(&self.conexion, id, nombre)
    }

    pub fn borrar_grupo(&self, id: i64) -> Result<()> {
        grupos::borrar(&self.conexion, id)
    }

    pub fn alternar_plegado(&self, id: i64, plegado: bool) -> Result<()> {
        grupos::alternar_plegado(&self.conexion, id, plegado)
    }

    pub fn mover_grupo(&self, id: i64, direccion: i8) -> Result<()> {
        grupos::mover(&self.conexion, id, direccion)
    }

    pub fn hosts_en_grupo(&self, id: i64) -> Result<i64> {
        grupos::hosts_en_grupo(&self.conexion, id)
    }

    // Etiquetas -------------------------------------------------------------

    pub fn listar_etiquetas(&self) -> Result<Vec<crate::modelo::Etiqueta>> {
        etiquetas::listar(&self.conexion)
    }

    // Sondeos ---------------------------------------------------------------

    pub fn guardar_sondeo(&self, sondeo: &crate::modelo::Sondeo) -> Result<i64> {
        sondeos::guardar(&self.conexion, sondeo)
    }

    pub fn ultimo_sondeo(&self, host_id: i64) -> Result<Option<crate::modelo::Sondeo>> {
        sondeos::ultimo(&self.conexion, host_id)
    }

    pub fn ultimos_sondeos(&self, host_id: i64, limite: i64) -> Result<Vec<crate::modelo::Sondeo>> {
        sondeos::ultimos(&self.conexion, host_id, limite)
    }

    pub fn sondeos_por_host(
        &self,
    ) -> Result<std::collections::HashMap<i64, crate::modelo::Sondeo>> {
        sondeos::ultimos_por_host(&self.conexion)
    }

    // Identidades -----------------------------------------------------------

    pub fn sincronizar_identidades(&self, claves: &[identidades::ClaveSincronizada]) -> Result<()> {
        identidades::sincronizar(&self.conexion, claves)
    }

    pub fn listar_identidades(
        &self,
        incluir_revocadas: bool,
    ) -> Result<Vec<crate::modelo::Identidad>> {
        identidades::listar(&self.conexion, incluir_revocadas)
    }

    pub fn obtener_identidad(&self, id: i64) -> Result<crate::modelo::Identidad> {
        identidades::obtener(&self.conexion, id)
    }

    pub fn identidad_por_huella(&self, huella: &str) -> Result<Option<crate::modelo::Identidad>> {
        identidades::por_huella(&self.conexion, huella)
    }

    pub fn crear_identidad(
        &self,
        tipo: &str,
        huella: &str,
        origen: crate::modelo::OrigenIdentidad,
        ruta: Option<&str>,
        comentario: Option<&str>,
    ) -> Result<i64> {
        identidades::crear(&self.conexion, tipo, huella, origen, ruta, comentario)
    }

    pub fn renombrar_identidad(&self, id: i64, alias: &str) -> Result<()> {
        identidades::renombrar(&self.conexion, id, alias)
    }

    pub fn marcar_uso_identidad(&self, huella: &str) -> Result<()> {
        identidades::marcar_uso(&self.conexion, huella, &crate::modelo::fecha_ahora())
    }

    pub fn hosts_que_usan_identidad(
        &self,
        identidad: &crate::modelo::Identidad,
        hogar: &std::path::Path,
    ) -> Result<Vec<String>> {
        identidades::hosts_que_usan(&self.conexion, identidad, hogar)
    }

    pub fn revocar_identidad(&self, id: i64, hogar: &std::path::Path) -> Result<usize> {
        identidades::revocar(&self.conexion, id, hogar)
    }

    pub fn reactivar_identidad(&self, id: i64) -> Result<()> {
        identidades::reactivar(&self.conexion, id)
    }

    // Registro --------------------------------------------------------------

    pub fn listar_registro(
        &self,
        filtro: &crate::registro::FiltroRegistro,
        limite: i64,
        desplazamiento: i64,
    ) -> Result<Vec<crate::modelo::EntradaRegistro>> {
        registro::listar(&self.conexion, filtro, limite, desplazamiento)
    }

    pub fn contar_registro(&self, filtro: &crate::registro::FiltroRegistro) -> Result<i64> {
        registro::contar(&self.conexion, filtro)
    }

    pub fn registro_para_exportar(
        &self,
        desde: Option<&str>,
    ) -> Result<Vec<crate::modelo::EntradaRegistro>> {
        registro::listar_todas(&self.conexion, desde)
    }

    pub fn purgar_registro(&self, fecha_corte: &str) -> Result<usize> {
        registro::purgar_anteriores(&self.conexion, fecha_corte)
    }

    pub fn contar_registro_anterior(&self, fecha_corte: &str) -> Result<i64> {
        registro::contar_anteriores(&self.conexion, fecha_corte)
    }
}

/// Aviso de la migración 11 (§4.10 del informe de la Fase 16): la marca de
/// sesiones persistentes de la Fase 15, que era propia del equipo, ahora viaja
/// a los demás equipos. `None` si no se migró ningún host.
pub fn aviso_marca_migrada(hosts: usize) -> Option<String> {
    match hosts {
        0 => None,
        1 => Some(
            "La marca de sesiones persistentes de 1 host viaja ahora a tus otros equipos"
                .to_string(),
        ),
        n => Some(format!(
            "La marca de sesiones persistentes de {n} hosts viaja ahora a tus otros equipos"
        )),
    }
}

/// Datos mínimos de una fila que va a borrarse, para dejar su lápida
/// (Fase 10).
pub(crate) struct FilaParaLapida {
    pub uuid: String,
    pub nombre: String,
    pub vector: String,
}

/// Lee uuid, nombre y vector de una fila por su id. None si no existe. Las
/// tablas que viajan y se borran desde el almacén tienen columna `nombre`;
/// las identidades no se borran nunca (se revocan).
pub(crate) fn fila_para_lapida(
    conexion: &Connection,
    tabla: versiones::Tabla,
    id: i64,
) -> Result<Option<FilaParaLapida>> {
    use rusqlite::OptionalExtension as _;
    Ok(conexion
        .query_row(
            &format!(
                "SELECT uuid, nombre, vector FROM {} WHERE id = ?1",
                tabla.nombre()
            ),
            rusqlite::params![id],
            |fila| {
                Ok(FilaParaLapida {
                    uuid: fila.get(0)?,
                    nombre: fila.get(1)?,
                    vector: fila.get::<_, Option<String>>(2)?.unwrap_or_default(),
                })
            },
        )
        .optional()?)
}

/// Deja la lápida de una fila que se va a borrar: emite una versión nueva
/// sobre el vector anterior del elemento, para que el borrado domine las
/// ediciones que ya había visto y pierda ante una edición posterior (Fase 10).
/// Se llama antes del DELETE, en la misma transacción.
pub(crate) fn dejar_lapida(
    conexion: &Connection,
    tabla: versiones::Tabla,
    uuid: &str,
    nombre: &str,
    vector_anterior: &str,
) -> Result<()> {
    let (version, vector) = versiones::version_de_lapida(conexion, vector_anterior)?;
    lapidas::guardar(
        conexion,
        &lapidas::Lapida {
            uuid: uuid.to_string(),
            tipo: tabla.tipo_elemento().to_string(),
            nombre: nombre.to_string(),
            version,
            vector,
            borrado_en: crate::modelo::fecha_ahora(),
        },
    )
}

/// Crea un directorio con permisos 0700 si no existe (para copias y
/// temporales de la sincronización).
pub fn crear_directorio_privado_publico(ruta: &Path) -> Result<()> {
    crear_directorio_privado(ruta)
}

fn crear_directorio_privado(ruta: &Path) -> Result<()> {
    if ruta.exists() {
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(ruta)
        .with_context(|| format!("creando {}", ruta.display()))?;
    Ok(())
}

/// Detecta una violación de restricción UNIQUE (o de clave primaria, que
/// también es un choque de unicidad). Se mira el código extendido: la familia
/// `ConstraintViolation` incluye también las claves ajenas y los nulos, y
/// llamar «ya existe» a un `host_id` que no existe sería mentir al usuario.
pub fn es_conflicto_unico(error: &rusqlite::Error) -> bool {
    const UNIQUE: i32 = rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE;
    const PRIMARY_KEY: i32 = rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY;
    matches!(
        error,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code,
                ..
            },
            _
        ) if *extended_code == UNIQUE || *extended_code == PRIMARY_KEY
    )
}
