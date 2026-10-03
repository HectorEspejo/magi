//! Sincronizaciones guardadas (`L`, Fase 8, §5.5 y §6.5): lista del host con
//! su último resultado, alta, edición y borrado con validación, y las entradas
//! de la paleta (`sync · <host> · <nombre>`, `sincronizar directorio`,
//! `editar fichero`). Ejecutar una guardada es pedir su plan a
//! `planificar_sincronizacion` (módulo `sincronizar`).
//!
//! La lista y el formulario viven en el hueco `App::dialogo` y llevan fijados
//! el host y, al editar, el id de la fila (T33): `↵`, `e` y `x` actúan sobre
//! la fila tal como se ve, sin releerla. Tras guardar o borrar, la lista se
//! vuelve a leer de `SINCRONIZACIONES_DIR`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::archivos::Lado;
use crate::modelo::{DatosSincronizacion, Sincronizacion};
use crate::protocolo::Direccion;
use crate::ui::componentes::CampoTexto;
use crate::ui::disposicion::Lista;
use crate::ui::Vista;

use super::{AccionDialogo, AccionPaleta, App, Dialogo, EntradaPaleta};

/// Diálogos propios (lista `L` y formulario de alta/edición).
pub enum DialogoGuardadas {
    /// Las guardadas de un host (§6.5).
    Lista(ListaGuardadas),
    /// «SINCRONIZACIÓN GUARDADA»: alta (`n`) o edición (`e`).
    Formulario(FormularioGuardada),
}

/// La lista `L`: las filas tal como se leyeron al abrirla.
pub struct ListaGuardadas {
    pub host_id: i64,
    pub host_nombre: String,
    /// Por nombre (`Almacen::sincronizaciones_de_host`).
    pub filas: Vec<Sincronizacion>,
    pub seleccion: usize,
}

impl ListaGuardadas {
    pub fn seleccionada(&self) -> Option<&Sincronizacion> {
        self.filas.get(self.seleccion)
    }

    /// Lleva la selección a `indice`, acotado a la lista.
    fn mover_a(&mut self, indice: usize) {
        self.seleccion = indice.min(self.filas.len().saturating_sub(1));
    }
}

/// Campos del formulario, en el orden en el que los recorre `Tab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoGuardada {
    Nombre,
    Local,
    Remoto,
    Direccion,
    Borrar,
    Exclusiones,
}

pub const ORDEN_GUARDADA: [CampoGuardada; 6] = [
    CampoGuardada::Nombre,
    CampoGuardada::Local,
    CampoGuardada::Remoto,
    CampoGuardada::Direccion,
    CampoGuardada::Borrar,
    CampoGuardada::Exclusiones,
];

/// Estado del formulario «SINCRONIZACIÓN GUARDADA».
pub struct FormularioGuardada {
    /// `Some` al editar: el id de la fila, fijado al abrir (T33).
    pub id: Option<i64>,
    /// Host fijado al abrir (el de la lista).
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: CampoTexto,
    pub ruta_local: CampoTexto,
    pub ruta_remota: CampoTexto,
    pub direccion: Direccion,
    /// Borrar en el destino lo que no está en el origen.
    pub borrar: bool,
    /// Patrones extra separados por espacios.
    pub exclusiones: CampoTexto,
    pub foco: CampoGuardada,
    /// Error de la última validación del almacén: se ve en el formulario.
    pub error: Option<String>,
    /// Fila seleccionada en la lista al abrirlo: `Esc` vuelve a ella.
    pub volver_a: Option<i64>,
}

/// Lo que pide una tecla del formulario.
pub enum AccionFormularioGuardada {
    Nada,
    Guardar,
    /// `Esc`: vuelve a la lista sin guardar.
    Volver,
}

impl FormularioGuardada {
    /// Alta con las rutas y la dirección por defecto (las de los paneles).
    pub fn nueva(
        host_id: i64,
        host_nombre: String,
        ruta_local: String,
        ruta_remota: String,
        direccion: Direccion,
        volver_a: Option<i64>,
    ) -> Self {
        Self {
            id: None,
            host_id,
            host_nombre,
            nombre: CampoTexto::default(),
            ruta_local: CampoTexto::nuevo(ruta_local),
            ruta_remota: CampoTexto::nuevo(ruta_remota),
            direccion,
            borrar: false,
            exclusiones: CampoTexto::default(),
            foco: CampoGuardada::Nombre,
            error: None,
            volver_a,
        }
    }

    /// Edición de una fila, con sus datos ya fijados.
    pub fn editar(guardada: &Sincronizacion) -> Self {
        Self {
            id: Some(guardada.id),
            host_id: guardada.host_id,
            host_nombre: guardada.host_nombre.clone(),
            nombre: CampoTexto::nuevo(guardada.nombre.clone()),
            ruta_local: CampoTexto::nuevo(guardada.ruta_local.clone()),
            ruta_remota: CampoTexto::nuevo(guardada.ruta_remota.clone()),
            direccion: guardada.direccion,
            borrar: guardada.borrar,
            exclusiones: CampoTexto::nuevo(guardada.exclusiones.join(" ")),
            foco: CampoGuardada::Nombre,
            error: None,
            volver_a: Some(guardada.id),
        }
    }

    /// Datos tal y como los valida y guarda el almacén.
    pub fn datos(&self) -> DatosSincronizacion {
        DatosSincronizacion {
            host_id: self.host_id,
            nombre: self.nombre.texto.trim().to_string(),
            ruta_local: self.ruta_local.texto.trim().to_string(),
            ruta_remota: self.ruta_remota.texto.trim().to_string(),
            direccion: self.direccion,
            borrar: self.borrar,
            exclusiones: self
                .exclusiones
                .texto
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        }
    }

    /// Siguiente campo en el orden del `Tab`.
    fn avanzar(&mut self, paso: i32) {
        let total = ORDEN_GUARDADA.len() as i32;
        let posicion = ORDEN_GUARDADA
            .iter()
            .position(|campo| *campo == self.foco)
            .unwrap_or(0) as i32;
        self.foco = ORDEN_GUARDADA[(posicion + paso).rem_euclid(total) as usize];
    }

    /// El campo de texto con el foco, si el foco está en uno.
    fn campo_texto(&mut self) -> Option<&mut CampoTexto> {
        match self.foco {
            CampoGuardada::Nombre => Some(&mut self.nombre),
            CampoGuardada::Local => Some(&mut self.ruta_local),
            CampoGuardada::Remoto => Some(&mut self.ruta_remota),
            CampoGuardada::Exclusiones => Some(&mut self.exclusiones),
            CampoGuardada::Direccion | CampoGuardada::Borrar => None,
        }
    }

    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> AccionFormularioGuardada {
        if tecla.modifiers.contains(KeyModifiers::CONTROL) {
            if tecla.code == KeyCode::Char('s') {
                return AccionFormularioGuardada::Guardar;
            }
            return AccionFormularioGuardada::Nada;
        }
        match tecla.code {
            KeyCode::Esc => return AccionFormularioGuardada::Volver,
            KeyCode::Enter => return AccionFormularioGuardada::Guardar,
            KeyCode::Tab => self.avanzar(1),
            KeyCode::BackTab => self.avanzar(-1),
            KeyCode::Char(' ') if self.foco == CampoGuardada::Direccion => {
                self.direccion = contraria(self.direccion);
            }
            KeyCode::Char(' ') if self.foco == CampoGuardada::Borrar => {
                self.borrar = !self.borrar;
            }
            KeyCode::Left if self.foco == CampoGuardada::Direccion => {
                self.direccion = Direccion::Subida;
            }
            KeyCode::Right if self.foco == CampoGuardada::Direccion => {
                self.direccion = Direccion::Bajada;
            }
            _ => {
                // En los campos de texto el espacio es un carácter más (en
                // las exclusiones, el que separa los patrones).
                if let Some(campo) = self.campo_texto() {
                    campo.manejar_tecla(tecla);
                }
            }
        }
        AccionFormularioGuardada::Nada
    }
}

fn contraria(direccion: Direccion) -> Direccion {
    match direccion {
        Direccion::Subida => Direccion::Bajada,
        Direccion::Bajada => Direccion::Subida,
    }
}

/// Acciones confirmadas (viajan en `AccionDialogo::Guardadas`).
pub enum AccionGuardadas {
    /// Borrar una guardada (id y nombre fijados al pedir la confirmación) y
    /// volver a la lista de su host, en la misma posición.
    Borrar {
        id: i64,
        nombre: String,
        host_id: i64,
        host_nombre: String,
        indice: usize,
    },
}

/// Entradas de la paleta: `sync · <host> · <nombre>`, `sincronizar
/// directorio` y `editar fichero`.
#[derive(Debug, Clone)]
pub enum AccionPaletaArchivos {
    /// `sync · <host> · <nombre>`: planificar esa guardada (id fijado al
    /// construir la paleta) y abrir su vista previa.
    Sincronizar(i64),
    /// `sincronizar directorio`: lo mismo que `S` en Archivos.
    SincronizarDirectorio,
    /// `editar fichero`: lo mismo que `E` en Archivos.
    EditarFichero,
}

/// La solicitud de planificación de una guardada: sus datos tal como están
/// en `SINCRONIZACIONES_DIR` al pedirla.
pub fn solicitud_de_guardada(
    guardada: &crate::modelo::Sincronizacion,
) -> super::sincronizar::SolicitudSincronizacion {
    super::sincronizar::SolicitudSincronizacion {
        host_id: guardada.host_id,
        host_nombre: guardada.host_nombre.clone(),
        guardada: Some((guardada.id, guardada.nombre.clone())),
        direccion: guardada.direccion,
        ruta_local: guardada.ruta_local.clone(),
        ruta_remota: guardada.ruta_remota.clone(),
        borrar: guardada.borrar,
        extras: guardada.exclusiones.clone(),
    }
}

/// Qué fila queda seleccionada al (re)abrir la lista.
enum Seleccionar {
    Id(i64),
    Indice(usize),
}

/// Líneas de la confirmación de borrado: solo se borra la definición.
fn lineas_borrado(nombre: &str, host: &str) -> Vec<String> {
    vec![
        format!("¿Borrar la sincronización «{nombre}» de {host}?"),
        "Solo se borra la definición: no se toca ningún fichero.".to_string(),
    ]
}

impl App {
    /// `L` en Archivos: las guardadas del host abierto.
    pub(super) fn abrir_sincronizaciones(&mut self) {
        let Some((host_id, host_nombre)) = self
            .archivos
            .as_ref()
            .map(|estado| (estado.host_id, estado.host_nombre.clone()))
        else {
            self.mensaje("abre Archivos (F4) primero", true);
            return;
        };
        self.mostrar_guardadas(host_id, host_nombre, Seleccionar::Indice(0));
    }

    /// Lee las guardadas del host y abre (o vuelve a abrir) su lista.
    fn mostrar_guardadas(&mut self, host_id: i64, host_nombre: String, seleccionar: Seleccionar) {
        let filas = match self.almacen.sincronizaciones_de_host(host_id) {
            Ok(filas) => filas,
            Err(error) => {
                self.mensaje(
                    format!("no se pudieron leer las sincronizaciones: {error}"),
                    true,
                );
                return;
            }
        };
        let seleccion = match seleccionar {
            Seleccionar::Id(id) => filas.iter().position(|fila| fila.id == id).unwrap_or(0),
            Seleccionar::Indice(indice) => indice.min(filas.len().saturating_sub(1)),
        };
        // La ventana parte de arriba (lo que hubiera era del formulario) y el
        // pintado la lleva hasta la selección.
        self.desplazamiento_modal = 0;
        self.dialogo = Some(Dialogo::Guardadas(DialogoGuardadas::Lista(
            ListaGuardadas {
                host_id,
                host_nombre,
                filas,
                seleccion,
            },
        )));
    }

    /// Tecla con la lista o el formulario abiertos. El diálogo llega sacado
    /// del hueco: si sigue abierto, hay que devolverlo a `self.dialogo`.
    pub(super) fn tecla_dialogo_guardadas(&mut self, dialogo: DialogoGuardadas, tecla: KeyEvent) {
        match dialogo {
            DialogoGuardadas::Lista(lista) => self.tecla_lista_guardadas(lista, tecla),
            DialogoGuardadas::Formulario(mut formulario) => {
                match formulario.manejar_tecla(&tecla) {
                    AccionFormularioGuardada::Nada => {
                        self.dialogo =
                            Some(Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario)));
                    }
                    AccionFormularioGuardada::Volver => {
                        let seleccionar = formulario
                            .volver_a
                            .map_or(Seleccionar::Indice(0), Seleccionar::Id);
                        self.mostrar_guardadas(
                            formulario.host_id,
                            formulario.host_nombre,
                            seleccionar,
                        );
                    }
                    AccionFormularioGuardada::Guardar => self.guardar_guardada(formulario),
                }
            }
        }
    }

    fn tecla_lista_guardadas(&mut self, mut lista: ListaGuardadas, tecla: KeyEvent) {
        // Página: las filas que se vieron en el último pintado.
        let pagina = self.disposicion.filas(Lista::Modal);
        let seleccion = lista.seleccion;
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => lista.mover_a(seleccion.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => lista.mover_a(seleccion + 1),
            KeyCode::PageUp => lista.mover_a(seleccion.saturating_sub(pagina)),
            KeyCode::PageDown => lista.mover_a(seleccion + pagina),
            KeyCode::Home => lista.mover_a(0),
            KeyCode::End => lista.mover_a(usize::MAX),
            // `↵` cierra la lista y planifica la fila tal como se ve.
            KeyCode::Enter => {
                if let Some(guardada) = lista.seleccionada() {
                    let solicitud = solicitud_de_guardada(guardada);
                    self.planificar_sincronizacion(solicitud);
                    return;
                }
            }
            KeyCode::Char('n') => {
                self.nueva_guardada(&lista);
                return;
            }
            KeyCode::Char('e') => {
                if let Some(guardada) = lista.seleccionada() {
                    let formulario = FormularioGuardada::editar(guardada);
                    self.desplazamiento_modal = 0;
                    self.dialogo =
                        Some(Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario)));
                    return;
                }
            }
            KeyCode::Char('x') => {
                if lista.seleccionada().is_some() {
                    self.confirmar_borrado_guardada(lista);
                    return;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => return,
            _ => {}
        }
        self.dialogo = Some(Dialogo::Guardadas(DialogoGuardadas::Lista(lista)));
    }

    /// `n`: alta con las rutas de los paneles y la dirección del activo (como
    /// `S`: activo local → subida, activo remoto → bajada).
    fn nueva_guardada(&mut self, lista: &ListaGuardadas) {
        let (ruta_local, ruta_remota, direccion) = match &self.archivos {
            Some(estado) if estado.host_id == lista.host_id => (
                estado.local.ruta.clone(),
                estado.remoto.ruta.clone(),
                if estado.activo == Lado::Remoto {
                    Direccion::Bajada
                } else {
                    Direccion::Subida
                },
            ),
            _ => (String::new(), String::new(), Direccion::Subida),
        };
        let formulario = FormularioGuardada::nueva(
            lista.host_id,
            lista.host_nombre.clone(),
            ruta_local,
            ruta_remota,
            direccion,
            lista.seleccionada().map(|guardada| guardada.id),
        );
        self.desplazamiento_modal = 0;
        self.dialogo = Some(Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario)));
    }

    /// `x`: confirmación con el id y el nombre fijados. La lista se aparta a
    /// la pila de diálogos: si se cancela, vuelve tal cual.
    fn confirmar_borrado_guardada(&mut self, lista: ListaGuardadas) {
        let Some(guardada) = lista.seleccionada() else {
            return;
        };
        let lineas = lineas_borrado(&guardada.nombre, &lista.host_nombre);
        let accion = AccionGuardadas::Borrar {
            id: guardada.id,
            nombre: guardada.nombre.clone(),
            host_id: lista.host_id,
            host_nombre: lista.host_nombre.clone(),
            indice: lista.seleccion,
        };
        self.pila_dialogos
            .push(Dialogo::Guardadas(DialogoGuardadas::Lista(lista)));
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "BORRAR SINCRONIZACIÓN".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Guardadas(accion),
        });
    }

    /// `Ctrl+S` o `↵` en el formulario: crea o actualiza (el almacén valida y
    /// normaliza). Si falla, el error se ve en el formulario, que sigue
    /// abierto; si no, se vuelve a la lista con la guardada seleccionada.
    fn guardar_guardada(&mut self, mut formulario: FormularioGuardada) {
        let datos = formulario.datos();
        let guardado = match formulario.id {
            Some(id) => self
                .almacen
                .actualizar_sincronizacion(id, &datos)
                .map(|()| id),
            None => self.almacen.crear_sincronizacion(&datos),
        };
        match guardado {
            Ok(id) => {
                // El mensaje va antes: si releer la lista falla, se ve ese error.
                self.mensaje(format!("sincronización «{}» guardada", datos.nombre), false);
                self.mostrar_guardadas(
                    formulario.host_id,
                    formulario.host_nombre,
                    Seleccionar::Id(id),
                );
            }
            Err(error) => {
                formulario.error = Some(error.to_string());
                self.dialogo = Some(Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario)));
            }
        }
    }

    pub(super) fn ejecutar_accion_guardadas(&mut self, accion: AccionGuardadas) {
        match accion {
            AccionGuardadas::Borrar {
                id,
                nombre,
                host_id,
                host_nombre,
                indice,
            } => {
                match self.almacen.borrar_sincronizacion(id) {
                    Ok(()) => self.mensaje(format!("sincronización «{nombre}» borrada"), false),
                    Err(error) => {
                        self.mensaje(format!("no se pudo borrar «{nombre}»: {error}"), true)
                    }
                }
                // La lista apartada al pedir la confirmación es la de antes de
                // borrar: se sustituye por una recién leída.
                self.pila_dialogos
                    .retain(|dialogo| !matches!(dialogo, Dialogo::Guardadas(_)));
                self.mostrar_guardadas(host_id, host_nombre, Seleccionar::Indice(indice));
            }
        }
    }

    /// Entradas de la paleta, con las guardadas releídas de la tabla al
    /// abrirla.
    pub(super) fn entradas_paleta_archivos(&self) -> Vec<EntradaPaleta> {
        let guardadas = self
            .almacen
            .listar_sincronizaciones()
            .unwrap_or_else(|error| {
                tracing::warn!("paleta sin sincronizaciones guardadas: {error}");
                Vec::new()
            });
        let mut entradas: Vec<EntradaPaleta> = guardadas
            .into_iter()
            .map(|guardada| EntradaPaleta {
                etiqueta: format!("sync · {} · {}", guardada.host_nombre, guardada.nombre),
                categoria: "archivos",
                accion: AccionPaleta::Archivos(AccionPaletaArchivos::Sincronizar(guardada.id)),
            })
            .collect();
        entradas.extend(
            [
                (
                    "sincronizar directorio",
                    AccionPaletaArchivos::SincronizarDirectorio,
                ),
                ("editar fichero", AccionPaletaArchivos::EditarFichero),
            ]
            .into_iter()
            .map(|(etiqueta, accion)| EntradaPaleta {
                etiqueta: etiqueta.to_string(),
                categoria: "archivos",
                accion: AccionPaleta::Archivos(accion),
            }),
        );
        entradas
    }

    pub(super) fn accion_paleta_archivos(&mut self, accion: AccionPaletaArchivos) {
        // `S` y `E` trabajan sobre los paneles: fuera de la vista Archivos no
        // hay directorio ni fila a los que referirse.
        let en_archivos = self.vista == Vista::Archivos && self.archivos.is_some();
        match accion {
            AccionPaletaArchivos::Sincronizar(id) => {
                match self.almacen.obtener_sincronizacion(id) {
                    Ok(guardada) => {
                        self.planificar_sincronizacion(solicitud_de_guardada(&guardada))
                    }
                    Err(_) => self.mensaje("esa sincronización ya no existe", true),
                }
            }
            AccionPaletaArchivos::SincronizarDirectorio | AccionPaletaArchivos::EditarFichero
                if !en_archivos =>
            {
                self.mensaje("abre Archivos (F4) primero", true);
            }
            AccionPaletaArchivos::SincronizarDirectorio => self.abrir_sincronizar(),
            AccionPaletaArchivos::EditarFichero => self.editar_entrada(),
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tecla(codigo: KeyCode) -> KeyEvent {
        KeyEvent::new(codigo, KeyModifiers::NONE)
    }

    fn formulario() -> FormularioGuardada {
        FormularioGuardada::nueva(
            7,
            "hetzner-01".to_string(),
            "/home/hector/proyectos/cooperapp".to_string(),
            "/var/www/cooperapp".to_string(),
            Direccion::Subida,
            None,
        )
    }

    fn escribir(formulario: &mut FormularioGuardada, texto: &str) {
        for caracter in texto.chars() {
            formulario.manejar_tecla(&tecla(KeyCode::Char(caracter)));
        }
    }

    #[test]
    fn tab_recorre_los_campos_en_orden_y_da_la_vuelta() {
        let mut formulario = formulario();
        let mut vistos = vec![formulario.foco];
        for _ in 0..ORDEN_GUARDADA.len() {
            formulario.manejar_tecla(&tecla(KeyCode::Tab));
            vistos.push(formulario.foco);
        }
        assert_eq!(&vistos[..6], &ORDEN_GUARDADA);
        assert_eq!(vistos[6], CampoGuardada::Nombre);
        formulario.manejar_tecla(&tecla(KeyCode::BackTab));
        assert_eq!(formulario.foco, CampoGuardada::Exclusiones);
    }

    #[test]
    fn espacio_marca_casillas_y_en_los_textos_es_un_caracter() {
        let mut formulario = formulario();
        escribir(&mut formulario, "web prod");
        assert_eq!(formulario.nombre.texto, "web prod");
        formulario.foco = CampoGuardada::Direccion;
        formulario.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(formulario.direccion, Direccion::Bajada);
        formulario.manejar_tecla(&tecla(KeyCode::Left));
        assert_eq!(formulario.direccion, Direccion::Subida);
        formulario.manejar_tecla(&tecla(KeyCode::Right));
        assert_eq!(formulario.direccion, Direccion::Bajada);
        formulario.foco = CampoGuardada::Borrar;
        formulario.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert!(formulario.borrar);
        // Ni la casilla ni el radio escriben en ningún campo.
        escribir(&mut formulario, "xyz");
        assert_eq!(formulario.nombre.texto, "web prod");
        assert_eq!(formulario.exclusiones.texto, "");
    }

    #[test]
    fn guardar_con_ctrl_s_o_intro_y_volver_con_esc() {
        let mut formulario = formulario();
        assert!(matches!(
            formulario.manejar_tecla(&KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            AccionFormularioGuardada::Guardar
        ));
        assert!(matches!(
            formulario.manejar_tecla(&tecla(KeyCode::Enter)),
            AccionFormularioGuardada::Guardar
        ));
        assert!(matches!(
            formulario.manejar_tecla(&tecla(KeyCode::Esc)),
            AccionFormularioGuardada::Volver
        ));
        // Otro Ctrl no escribe nada.
        formulario.manejar_tecla(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(formulario.nombre.texto, "");
    }

    #[test]
    fn los_datos_parten_las_exclusiones_por_espacios() {
        let mut formulario = formulario();
        escribir(&mut formulario, "  web-prod ");
        formulario.foco = CampoGuardada::Exclusiones;
        escribir(&mut formulario, " *.log   tmp/ .env ");
        let datos = formulario.datos();
        assert_eq!(datos.host_id, 7);
        assert_eq!(datos.nombre, "web-prod");
        assert_eq!(datos.ruta_local, "/home/hector/proyectos/cooperapp");
        assert_eq!(datos.ruta_remota, "/var/www/cooperapp");
        assert_eq!(datos.direccion, Direccion::Subida);
        assert!(!datos.borrar);
        assert_eq!(datos.exclusiones, vec!["*.log", "tmp/", ".env"]);
    }

    #[test]
    fn editar_fija_el_id_y_trae_los_datos_de_la_fila() {
        let guardada = Sincronizacion {
            id: 42,
            host_id: 3,
            host_nombre: "hetzner-02".to_string(),
            nombre: "logs".to_string(),
            ruta_local: "/home/hector/logs".to_string(),
            ruta_remota: "/var/log/nginx".to_string(),
            direccion: Direccion::Bajada,
            borrar: true,
            exclusiones: vec!["*.gz".to_string(), "old/".to_string()],
            ultima_ejecucion_en: None,
            ultimo_resultado: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
        };
        let formulario = FormularioGuardada::editar(&guardada);
        assert_eq!(formulario.id, Some(42));
        assert_eq!(formulario.volver_a, Some(42));
        assert_eq!(formulario.exclusiones.texto, "*.gz old/");
        let datos = formulario.datos();
        assert_eq!(datos.host_id, 3);
        assert_eq!(datos.nombre, "logs");
        assert_eq!(datos.direccion, Direccion::Bajada);
        assert!(datos.borrar);
        assert_eq!(datos.exclusiones, guardada.exclusiones);
    }

    #[test]
    fn la_seleccion_de_la_lista_no_se_sale() {
        let mut lista = ListaGuardadas {
            host_id: 1,
            host_nombre: "hetzner-01".to_string(),
            filas: Vec::new(),
            seleccion: 0,
        };
        lista.mover_a(5);
        assert_eq!(lista.seleccion, 0);
        assert!(lista.seleccionada().is_none());
    }

    #[test]
    fn la_confirmacion_dice_que_no_se_toca_ningun_fichero() {
        assert_eq!(
            lineas_borrado("web-prod", "hetzner-01"),
            vec![
                "¿Borrar la sincronización «web-prod» de hetzner-01?",
                "Solo se borra la definición: no se toca ningún fichero.",
            ]
        );
    }
}
