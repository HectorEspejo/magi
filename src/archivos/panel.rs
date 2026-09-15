//! Modelo de la vista Archivos (F4): los dos paneles, las peticiones en vuelo
//! y la operación pendiente de decidir.
//!
//! El panel local y el remoto son el mismo tipo: solo cambia de dónde salen
//! sus entradas y quién ejecuta las operaciones.

use std::collections::{HashMap, HashSet};

use crate::archivos::marcas::{Entrada, Marca};
use crate::protocolo::{Direccion, InfoTransferencia, Politica};

/// Ficheros grandes que se confirman antes de traerlos para verlos.
pub const TAMANO_AVISO_VISOR: u64 = 10 * 1024 * 1024;

/// Cuál de los dos paneles tiene el foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lado {
    Local,
    Remoto,
}

impl Lado {
    pub fn contrario(self) -> Lado {
        match self {
            Lado::Local => Lado::Remoto,
            Lado::Remoto => Lado::Local,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            Lado::Local => "local",
            Lado::Remoto => "remoto",
        }
    }
}

/// Un panel: su ruta, lo que lista, dónde está el cursor y qué hay marcado.
#[derive(Debug, Clone, Default)]
pub struct Panel {
    pub ruta: String,
    pub entradas: Vec<Entrada>,
    pub seleccion: usize,
    pub desplazamiento: usize,
    /// Nombres marcados con `Espacio` (sobreviven al cambio de directorio no:
    /// se limpian al listar otro sitio).
    pub marcados: HashSet<String>,
    pub filtro: String,
    pub filtro_activo: bool,
    pub ocultos: bool,
    /// Fallo del último listado, para la barra.
    pub error: Option<String>,
}

impl Panel {
    pub fn nuevo(ruta: String, ocultos: bool) -> Self {
        Self {
            ruta,
            ocultos,
            ..Panel::default()
        }
    }

    /// Entradas que se ven con el filtro y los ocultos de ahora. La fila `..`
    /// se ve siempre: sin ella no habría forma de subir con el filtro puesto.
    pub fn visibles(&self) -> Vec<usize> {
        self.entradas
            .iter()
            .enumerate()
            .filter(|(_, entrada)| {
                entrada.nombre == ".." || self.ocultos || !es_oculto(&entrada.nombre)
            })
            .filter(|(_, entrada)| {
                entrada.nombre == ".."
                    || self.filtro.is_empty()
                    || entrada
                        .nombre
                        .to_lowercase()
                        .contains(&self.filtro.to_lowercase())
            })
            .map(|(indice, _)| indice)
            .collect()
    }

    /// Cuántas entradas se ven ahora mismo.
    pub fn total_visibles(&self) -> usize {
        self.visibles().len()
    }

    /// Entrada bajo el cursor, si la hay.
    pub fn entrada_actual(&self) -> Option<&Entrada> {
        let visibles = self.visibles();
        visibles
            .get(self.seleccion)
            .map(|indice| &self.entradas[*indice])
    }

    /// Nombres de lo que hay que operar: lo marcado o, si no hay marcas, la
    /// fila actual.
    pub fn seleccionados(&self) -> Vec<&Entrada> {
        if self.marcados.is_empty() {
            return self.entrada_actual().into_iter().collect();
        }
        self.entradas
            .iter()
            .filter(|entrada| self.marcados.contains(&entrada.nombre))
            .collect()
    }

    pub fn total_marcados(&self) -> usize {
        self.marcados.len()
    }

    /// Suma de los tamaños de los ficheros visibles (los directorios no
    /// cuentan: enseña lo que ocupa lo que se ve).
    pub fn tamano_visible(&self) -> u64 {
        self.entradas
            .iter()
            .filter(|entrada| !entrada.es_dir())
            .map(|entrada| entrada.tamano)
            .sum()
    }

    /// Mueve el cursor dentro de lo visible, ajustando el desplazamiento.
    pub fn mover(&mut self, delta: i32, altura: usize) {
        let total = self.total_visibles();
        if total == 0 {
            self.seleccion = 0;
            self.desplazamiento = 0;
            return;
        }
        let destino = self.seleccion as i32 + delta;
        self.seleccion = destino.clamp(0, total as i32 - 1) as usize;
        self.ajustar_ventana(altura);
    }

    pub fn ir_a(&mut self, indice: usize, altura: usize) {
        let total = self.total_visibles();
        self.seleccion = indice.min(total.saturating_sub(1));
        self.ajustar_ventana(altura);
    }

    /// Deja el cursor dentro de la ventana visible.
    pub fn ajustar_ventana(&mut self, altura: usize) {
        let altura = altura.max(1);
        if self.seleccion < self.desplazamiento {
            self.desplazamiento = self.seleccion;
        }
        if self.seleccion >= self.desplazamiento + altura {
            self.desplazamiento = self.seleccion + 1 - altura;
        }
        let total = self.total_visibles();
        if self.desplazamiento + altura > total {
            self.desplazamiento = total.saturating_sub(altura);
        }
    }

    /// Marca o desmarca la fila actual y baja una.
    pub fn alternar_marca(&mut self, altura: usize) {
        if let Some(nombre) = self.entrada_actual().map(|entrada| entrada.nombre.clone()) {
            if !self.marcados.remove(&nombre) {
                self.marcados.insert(nombre);
            }
        }
        self.mover(1, altura);
    }

    /// Marca todo lo visible (respetando filtro y ocultos).
    pub fn marcar_todo(&mut self) {
        for indice in self.visibles() {
            let nombre = self.entradas[indice].nombre.clone();
            self.marcados.insert(nombre);
        }
    }

    pub fn desmarcar_todo(&mut self) {
        self.marcados.clear();
    }

    /// Cambia el directorio: se va la selección, las marcas y el filtro.
    pub fn cambiar_ruta(&mut self, ruta: String) {
        self.ruta = ruta;
        self.entradas.clear();
        self.seleccion = 0;
        self.desplazamiento = 0;
        self.marcados.clear();
        self.filtro.clear();
        self.error = None;
    }

    /// Recalcula las marcas comparando con el otro lado.
    /// Prepara las entradas de un listado nuevo con la fila `..` delante.
    /// `mismo_directorio` distingue un refresco de un cambio de directorio: al
    /// refrescar se conservan las marcas y el cursor, que si no el usuario
    /// pierde la selección cada vez que termina una transferencia.
    pub fn fijar_entradas(&mut self, entradas: Vec<Entrada>, mismo_directorio: bool) {
        let mut entradas = entradas;
        crate::archivos::marcas::ordenar(&mut entradas);
        let mut con_padre = Vec::with_capacity(entradas.len() + 1);
        con_padre.push(crate::archivos::marcas::entrada_padre());
        con_padre.extend(entradas);
        self.entradas = con_padre;
        self.error = None;
        if !mismo_directorio {
            self.seleccion = 0;
            self.desplazamiento = 0;
            self.marcados.clear();
            return;
        }
        // Las marcas de lo que ya no está se van solas.
        let nombres: std::collections::HashSet<&str> = self
            .entradas
            .iter()
            .map(|entrada| entrada.nombre.as_str())
            .collect();
        self.marcados
            .retain(|nombre| nombres.contains(nombre.as_str()));
        self.seleccion = self.seleccion.min(self.total_visibles().saturating_sub(1));
    }

    /// ¿El listado que se va a fijar es del mismo directorio?
    pub fn es_mismo_directorio(&self, ruta: &str) -> bool {
        self.ruta == ruta && !self.entradas.is_empty()
    }

    pub fn marcar_diferencias(&mut self, otro: &mut Panel) {
        crate::archivos::marcas::calcular(&mut self.entradas, &mut otro.entradas);
    }
}

pub fn es_oculto(nombre: &str) -> bool {
    nombre.starts_with('.') && nombre != "." && nombre != ".."
}

/// Qué se pidió al servidor y sigue sin contestar.
#[derive(Debug, Clone, PartialEq)]
pub enum Peticion {
    /// Abrir el canal SFTP del host.
    AbrirSftp,
    /// Listar un directorio remoto (con el motivo, para saber qué refrescar).
    ListarRemoto(MotivoListado),
    /// Traer un fichero remoto a un temporal para verlo.
    VerRemoto(String),
}

/// Por qué se pidió un listado remoto: de eso depende qué se hace al llegar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoListado {
    /// Primer listado al entrar en la vista.
    Inicial,
    /// Refresco manual (`R`).
    Refresco,
    /// Tras una operación que cambia el directorio.
    Operacion,
}

/// Operación esperando respuesta del usuario (conflicto o aviso).
#[derive(Debug, Clone)]
pub struct OperacionPendiente {
    pub direccion: Direccion,
    pub lado_origen: Lado,
    /// Elementos de primer nivel, con su política ya decidida.
    pub elementos: Vec<ElementoOperacion>,
    pub borrar_origen: bool,
    /// Índice del elemento que se está preguntando.
    pub indice: usize,
    /// Política para los que quedan, si el usuario eligió «todos».
    pub politica_global: Option<Politica>,
}

#[derive(Debug, Clone)]
pub struct ElementoOperacion {
    pub origen: String,
    pub destino: String,
    pub bytes: u64,
    pub es_dir: bool,
    pub politica: Option<Politica>,
}

impl OperacionPendiente {
    /// Política decidida para el elemento que toca preguntar.
    pub fn politica_del_actual(&self) -> Option<Politica> {
        self.elementos
            .get(self.indice)
            .and_then(|elemento| elemento.politica)
    }

    pub fn quedan(&self) -> usize {
        self.elementos.len().saturating_sub(self.indice)
    }
}

/// Estado completo de la vista Archivos.
#[derive(Debug, Clone)]
pub struct EstadoArchivos {
    pub host_id: i64,
    pub host_nombre: String,
    pub activo: Lado,
    pub local: Panel,
    pub remoto: Panel,
    /// El host no ofrece SFTP: la vista se queda con el panel local.
    pub solo_local: bool,
    pub motivo_solo_local: Option<String>,
    /// Directorio de inicio del usuario remoto.
    pub dir_inicio: String,
    /// Aviso de sensibles ya compilado, para no rehacerlo en cada pulsación.
    pub sensibles: crate::archivos::sensibles::Sensibles,
    pub peticiones: HashMap<u64, Peticion>,
    pub siguiente_peticion: u64,
    /// Última cola difundida por el servidor.
    pub cola: Vec<InfoTransferencia>,
    /// Operación esperando decisión del usuario.
    pub operacion: Option<OperacionPendiente>,
    /// Aviso de sensibles esperando confirmación.
    pub aviso: Option<AvisoPendiente>,
    /// Medición para la velocidad y el restante de la vista Transferencias:
    /// id → (instante, bytes).
    pub muestras: HashMap<u32, (std::time::Instant, u64)>,
    /// Fichero remoto que se está trayendo para verlo.
    pub viendo: Option<String>,
}

/// Aviso de subida de ficheros sensibles, esperando un sí o un no.
#[derive(Debug, Clone)]
pub struct AvisoPendiente {
    pub coincidencias: Vec<(String, u64)>,
    pub restantes: usize,
    pub destino: String,
    pub operacion: OperacionPendiente,
}

impl EstadoArchivos {
    pub fn nueva_peticion(&mut self, peticion: Peticion) -> u64 {
        self.siguiente_peticion += 1;
        let id = self.siguiente_peticion;
        self.peticiones.insert(id, peticion);
        id
    }

    pub fn resolver_peticion(&mut self, id: u64) -> Option<Peticion> {
        self.peticiones.remove(&id)
    }

    /// Recalcula las marcas de los dos paneles, siempre juntos: si se
    /// recalculara solo uno, las marcas dirían cosas distintas en cada lado.
    ///
    /// Sin listado remoto no hay nada con lo que comparar: marcar todo como
    /// «no está al otro lado» sería mentira mientras el canal abre (o cuando
    /// el host no ofrece SFTP), así que no se marca nada.
    pub fn recalcular_marcas(&mut self) {
        if self.solo_local || self.remoto.entradas.is_empty() {
            for entrada in &mut self.local.entradas {
                entrada.marca = crate::archivos::Marca::Ninguna;
            }
            for entrada in &mut self.remoto.entradas {
                entrada.marca = crate::archivos::Marca::Ninguna;
            }
            return;
        }
        self.local.marcar_diferencias(&mut self.remoto);
    }

    /// Transferencias vivas.
    pub fn vivas(&self) -> Vec<&InfoTransferencia> {
        self.cola
            .iter()
            .filter(|fila| !fila.estado.terminada())
            .collect()
    }

    /// Bytes de lo que está en cola (sin contar lo que ya corre).
    pub fn bytes_en_cola(&self) -> u64 {
        self.cola
            .iter()
            .filter(|fila| fila.estado == crate::protocolo::EstadoTransferencia::EnCola)
            .map(|fila| fila.bytes_total.saturating_sub(fila.bytes_hechos))
            .sum()
    }

    pub fn en_curso(&self) -> Option<&InfoTransferencia> {
        self.cola
            .iter()
            .find(|fila| fila.estado == crate::protocolo::EstadoTransferencia::EnCurso)
    }

    /// Guarda la muestra de progreso para calcular velocidad y restante con
    /// las dos últimas difusiones.
    pub fn muestrear(&mut self, ahora: std::time::Instant) {
        for fila in &self.cola {
            if fila.estado == crate::protocolo::EstadoTransferencia::EnCurso {
                self.muestras.insert(fila.id, (ahora, fila.bytes_hechos));
            }
        }
        self.muestras
            .retain(|id, _| self.cola.iter().any(|fila| fila.id == *id));
    }

    /// Velocidad en bytes por segundo de una transferencia, si hay muestra
    /// anterior con la que comparar.
    pub fn velocidad(&self, fila: &InfoTransferencia) -> Option<f64> {
        let (antes, bytes_antes) = self.muestras.get(&fila.id)?;
        let transcurrido = antes.elapsed().as_secs_f64();
        if transcurrido <= 0.0 || fila.bytes_hechos <= *bytes_antes {
            return None;
        }
        Some((fila.bytes_hechos - bytes_antes) as f64 / transcurrido)
    }
}

/// Marca de una entrada del panel activo, para el detalle.
pub fn marca_de(panel: &Panel, nombre: &str) -> Marca {
    panel
        .entradas
        .iter()
        .find(|entrada| entrada.nombre == nombre)
        .map(|entrada| entrada.marca)
        .unwrap_or(Marca::Ninguna)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::archivos::marcas::TipoEntrada;

    fn entrada(nombre: &str, tamano: u64) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            tipo: TipoEntrada::Fichero,
            tamano,
            mtime: 0,
            permisos: None,
            propietario: None,
            enlace: None,
            marca: Marca::Ninguna,
        }
    }

    fn panel_con(nombres: &[&str]) -> Panel {
        let mut panel = Panel::nuevo("/".to_string(), false);
        panel.entradas = nombres.iter().map(|nombre| entrada(nombre, 10)).collect();
        panel
    }

    #[test]
    fn la_fila_padre_encabeza_el_listado_y_no_la_esconde_el_filtro() {
        let mut panel = Panel::nuevo("/".to_string(), false);
        panel.fijar_entradas(vec![entrada("a.txt", 10), entrada("b.md", 10)], false);
        assert_eq!(panel.entradas[0].nombre, "..");
        assert_eq!(panel.total_visibles(), 3);

        panel.filtro = "a.txt".to_string();
        assert_eq!(panel.total_visibles(), 2, "«..» sigue a la vista");
        assert_eq!(panel.entrada_actual().unwrap().nombre, "..");
    }

    #[test]
    fn el_filtro_y_los_ocultos_deciden_lo_visible() {
        let mut panel = panel_con(&[".env", "main.py", "otro.txt"]);
        assert_eq!(
            panel.total_visibles(),
            2,
            "los ocultos no se ven por defecto"
        );

        panel.filtro = "MAIN".to_string();
        assert_eq!(panel.total_visibles(), 1, "el filtro ignora mayúsculas");

        panel.filtro.clear();
        panel.ocultos = true;
        assert_eq!(panel.total_visibles(), 3);
    }

    #[test]
    fn el_cursor_no_se_sale_de_lo_visible() {
        let mut panel = panel_con(&["a", "b", "c"]);
        panel.mover(10, 2);
        assert_eq!(panel.seleccion, 2);
        panel.mover(-10, 2);
        assert_eq!(panel.seleccion, 0);
        panel.mover(-1, 2);
        assert_eq!(panel.seleccion, 0);

        panel.filtro = "zzz".to_string();
        panel.mover(1, 2);
        assert_eq!(panel.seleccion, 0, "sin visibles el cursor no se va");
    }

    #[test]
    fn marcar_y_desmarcar_cambia_la_seleccion_de_la_operacion() {
        let mut panel = panel_con(&["a", "b"]);
        assert_eq!(panel.seleccionados().len(), 1, "sin marcas, la fila actual");

        panel.alternar_marca(10);
        assert_eq!(panel.marcados.len(), 1, "marcar baja una fila");
        assert_eq!(panel.seleccion, 1);

        // Volver a la misma fila y marcar de nuevo la desmarca.
        panel.mover(-1, 10);
        panel.alternar_marca(10);
        assert!(panel.marcados.is_empty(), "la segunda vez desmarca");
        assert_eq!(panel.seleccion, 1);
    }

    #[test]
    fn marcar_todo_respeta_el_filtro() {
        let mut panel = panel_con(&["a.txt", "b.txt", "c.md"]);
        panel.filtro = ".txt".to_string();
        panel.marcar_todo();
        let mut marcados: Vec<&String> = panel.marcados.iter().collect();
        marcados.sort();
        assert_eq!(marcados, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn refrescar_el_mismo_directorio_conserva_marcas_y_cursor() {
        let mut panel = Panel::nuevo("/".to_string(), false);
        panel.fijar_entradas(vec![entrada("a.txt", 10), entrada("b.txt", 10)], false);
        panel.filtro.clear();
        panel.marcados.insert("a.txt".to_string());
        panel.seleccion = 2;
        panel.fijar_entradas(vec![entrada("a.txt", 10), entrada("b.txt", 10)], true);
        assert_eq!(panel.marcados.len(), 1, "el refresco no pierde las marcas");
        assert_eq!(panel.seleccion, 2);

        // Al cambiar de directorio sí se limpia todo.
        panel.fijar_entradas(vec![entrada("otra.txt", 10)], false);
        assert!(panel.marcados.is_empty());
        assert_eq!(panel.seleccion, 0);
    }

    #[test]
    fn cambiar_de_directorio_limpia_marcas_y_filtro() {
        let mut panel = panel_con(&["a"]);
        panel.marcados.insert("a".to_string());
        panel.filtro = "a".to_string();
        panel.cambiar_ruta("/otra".to_string());
        assert!(panel.marcados.is_empty());
        assert!(panel.filtro.is_empty());
        assert!(panel.entradas.is_empty());
    }

    #[test]
    fn las_marcas_se_calculan_sobre_los_dos_paneles_a_la_vez() {
        let mut local = panel_con(&["igual", "cambia"]);
        local.entradas[1].tamano = 20;
        let mut remoto = panel_con(&["igual", "cambia"]);
        local.marcar_diferencias(&mut remoto);
        assert_eq!(local.entradas[1].marca, Marca::Distinta);
        assert_eq!(remoto.entradas[1].marca, Marca::Distinta);
        assert_eq!(local.entradas[0].marca, Marca::Ninguna);
    }

    #[test]
    fn el_panel_remoto_que_falta_en_el_local_se_marca_con_aspa() {
        let mut local = panel_con(&["uno"]);
        let mut remoto = panel_con(&["uno", "solo_remoto"]);
        local.marcar_diferencias(&mut remoto);
        assert_eq!(remoto.entradas[1].marca, Marca::Ausente);
    }
}

#[cfg(test)]
mod pruebas_marcas_sin_remoto {
    use super::*;
    use crate::archivos::marcas::TipoEntrada;
    use crate::archivos::Marca;

    fn entrada(nombre: &str) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            tipo: TipoEntrada::Fichero,
            tamano: 10,
            mtime: 0,
            permisos: None,
            propietario: None,
            enlace: None,
            marca: Marca::Ninguna,
        }
    }

    fn estado() -> EstadoArchivos {
        EstadoArchivos {
            host_id: 1,
            host_nombre: "prueba".to_string(),
            activo: Lado::Local,
            local: Panel::nuevo("/tmp".to_string(), false),
            remoto: Panel::nuevo(String::new(), false),
            solo_local: false,
            motivo_solo_local: None,
            dir_inicio: String::new(),
            sensibles: crate::archivos::sensibles::Sensibles::default(),
            peticiones: std::collections::HashMap::new(),
            siguiente_peticion: 0,
            cola: Vec::new(),
            operacion: None,
            aviso: None,
            muestras: std::collections::HashMap::new(),
            viendo: None,
        }
    }

    #[test]
    fn sin_listado_remoto_no_se_marca_nada() {
        let mut estado = estado();
        estado.local.fijar_entradas(vec![entrada("main.py")], false);
        estado.recalcular_marcas();
        assert_eq!(
            estado.local.entradas[1].marca,
            Marca::Ninguna,
            "todavía no hay con qué comparar"
        );
    }

    #[test]
    fn con_los_dos_listados_las_marcas_vuelven() {
        let mut estado = estado();
        estado
            .local
            .fijar_entradas(vec![entrada("main.py"), entrada("solo_local")], false);
        estado
            .remoto
            .fijar_entradas(vec![entrada("main.py")], false);
        estado.recalcular_marcas();
        assert_eq!(estado.local.entradas[1].marca, Marca::Ninguna);
        assert_eq!(estado.local.entradas[2].marca, Marca::Ausente);
    }

    #[test]
    fn con_el_host_sin_sftp_tampoco_se_marca() {
        let mut estado = estado();
        estado.solo_local = true;
        estado.local.fijar_entradas(vec![entrada("main.py")], false);
        estado.recalcular_marcas();
        assert_eq!(estado.local.entradas[1].marca, Marca::Ninguna);
    }
}
