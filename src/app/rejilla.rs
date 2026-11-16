//! Diálogo «abrir en rejilla…» de la paleta (Fase 15, §4.6).
//!
//! Ofrece hasta cuatro hosts con casillas, agrupados por grupo: marcar un
//! grupo marca todos sus hosts, y el límite de cuatro se avisa sin pasarse.
//! Con ↵ se abren las pestañas en la disposición que toque (2 → lado a lado,
//! 3 → 1 grande + 2, 4 → rejilla). Esc cancela.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::modelo::{Grupo, Host};

/// Hosts como mucho en una rejilla.
pub const MAXIMO: usize = 4;

/// Una fila del diálogo: un grupo o un host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilaRejilla {
    Grupo {
        nombre: String,
        /// Hosts SSH del grupo, en el orden del inventario.
        hosts: Vec<i64>,
    },
    Host {
        id: i64,
        nombre: String,
        /// Sangrado: un host de un grupo va bajo su cabecera.
        bajo_grupo: bool,
    },
}

/// Lo que ha pedido el diálogo al atender una tecla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionRejilla {
    Nada,
    Cancelar,
    /// Abrir estas pestañas, en el orden en que se marcaron.
    Abrir(Vec<i64>),
}

/// Estado del diálogo de rejilla.
#[derive(Debug, Clone, Default)]
pub struct EstadoRejilla {
    pub filas: Vec<FilaRejilla>,
    pub seleccion: usize,
    /// Hosts marcados, en el orden en que se marcaron.
    marcados: Vec<i64>,
    pub aviso: Option<String>,
}

impl EstadoRejilla {
    /// Construye el diálogo con los hosts SSH del inventario, agrupados como
    /// en la vista Hosts (los grupos primero y el resto al final).
    #[must_use]
    pub fn nuevo(hosts: &[Host], grupos: &[Grupo]) -> Self {
        let ssh: Vec<&Host> = hosts.iter().filter(|host| host.tipo.es_ssh()).collect();
        let mut filas = Vec::new();
        let mut en_grupo = Vec::new();
        for grupo in grupos {
            let del_grupo: Vec<i64> = ssh
                .iter()
                .filter(|host| host.grupo_id == Some(grupo.id))
                .map(|host| host.id)
                .collect();
            if del_grupo.is_empty() {
                continue;
            }
            filas.push(FilaRejilla::Grupo {
                nombre: grupo.nombre.clone(),
                hosts: del_grupo.clone(),
            });
            for host in ssh.iter().filter(|host| host.grupo_id == Some(grupo.id)) {
                filas.push(FilaRejilla::Host {
                    id: host.id,
                    nombre: host.nombre.clone(),
                    bajo_grupo: true,
                });
                en_grupo.push(host.id);
            }
        }
        for host in ssh.iter().filter(|host| !en_grupo.contains(&host.id)) {
            filas.push(FilaRejilla::Host {
                id: host.id,
                nombre: host.nombre.clone(),
                bajo_grupo: false,
            });
        }
        Self {
            filas,
            seleccion: 0,
            marcados: Vec::new(),
            aviso: None,
        }
    }

    /// ¿Está marcado este host?
    #[must_use]
    pub fn marcado(&self, id: i64) -> bool {
        self.marcados.contains(&id)
    }

    /// Cuántos hosts hay marcados.
    #[must_use]
    pub fn marcados(&self) -> usize {
        self.marcados.len()
    }

    /// Los hosts marcados, en el orden en que se marcaron.
    #[must_use]
    pub fn hosts_marcados(&self) -> &[i64] {
        &self.marcados
    }

    /// Fila seleccionada, si hay alguna.
    #[must_use]
    pub fn fila_seleccionada(&self) -> Option<&FilaRejilla> {
        self.filas.get(self.seleccion)
    }

    /// ¿Un grupo está entero marcado?
    #[must_use]
    pub fn grupo_marcado(&self, hosts: &[i64]) -> bool {
        !hosts.is_empty() && hosts.iter().all(|id| self.marcado(*id))
    }

    fn mover(&mut self, paso: i64) {
        if self.filas.is_empty() {
            return;
        }
        let ultima = self.filas.len().saturating_sub(1) as i64;
        self.seleccion = (self.seleccion as i64 + paso).clamp(0, ultima) as usize;
    }

    /// Espacio: marca o desmarca la fila, respetando el límite de cuatro.
    fn alternar(&mut self) {
        self.aviso = None;
        let Some(fila) = self.filas.get(self.seleccion).cloned() else {
            return;
        };
        match fila {
            FilaRejilla::Host { id, .. } => {
                if let Some(posicion) = self.marcados.iter().position(|otro| *otro == id) {
                    self.marcados.remove(posicion);
                } else if self.marcados.len() < MAXIMO {
                    self.marcados.push(id);
                } else {
                    self.aviso = Some(format!("como mucho {MAXIMO} hosts: desmarca uno primero"));
                }
            }
            FilaRejilla::Grupo { hosts, .. } => {
                if self.grupo_marcado(&hosts) {
                    self.marcados.retain(|id| !hosts.contains(id));
                    return;
                }
                let mut fuera = 0;
                for id in hosts {
                    if self.marcado(id) {
                        continue;
                    }
                    if self.marcados.len() < MAXIMO {
                        self.marcados.push(id);
                    } else {
                        fuera += 1;
                    }
                }
                if fuera > 0 {
                    self.aviso = Some(format!(
                        "límite de {MAXIMO} hosts: {fuera} se quedan sin marcar"
                    ));
                }
            }
        }
    }

    /// Atiende una tecla del diálogo.
    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> AccionRejilla {
        let sin_modificadores = !tecla
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match tecla.code {
            KeyCode::Esc => AccionRejilla::Cancelar,
            KeyCode::Up | KeyCode::Char('k') if sin_modificadores => {
                self.aviso = None;
                self.mover(-1);
                AccionRejilla::Nada
            }
            KeyCode::Down | KeyCode::Char('j') if sin_modificadores => {
                self.aviso = None;
                self.mover(1);
                AccionRejilla::Nada
            }
            KeyCode::Char(' ') => {
                self.alternar();
                AccionRejilla::Nada
            }
            KeyCode::Enter => {
                let marcados = self.marcados.len();
                if (2..=MAXIMO).contains(&marcados) {
                    AccionRejilla::Abrir(self.marcados.clone())
                } else {
                    self.aviso = Some(format!("marca entre 2 y {MAXIMO} hosts: hay {marcados}"));
                    AccionRejilla::Nada
                }
            }
            _ => AccionRejilla::Nada,
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn host(id: i64, nombre: &str, grupo_id: Option<i64>) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = grupo_id;
        host
    }

    fn grupo(id: i64, nombre: &str) -> Grupo {
        Grupo {
            id,
            nombre: nombre.to_string(),
            orden: 0,
            plegado: false,
            creado_en: String::new(),
        }
    }

    fn tecla(codigo: KeyCode) -> KeyEvent {
        KeyEvent::new(codigo, KeyModifiers::NONE)
    }

    fn inventario() -> (Vec<Host>, Vec<Grupo>) {
        (
            vec![
                host(1, "web-01", Some(7)),
                host(2, "web-02", Some(7)),
                host(3, "db-01", None),
                host(4, "nas", None),
                host(5, "win-01", None),
            ],
            vec![grupo(7, "producción")],
        )
    }

    #[test]
    fn el_dialogo_agrupa_los_hosts_y_marca_con_espacio() {
        let (hosts, grupos) = inventario();
        let mut estado = EstadoRejilla::nuevo(&hosts, &grupos);
        assert_eq!(
            estado.filas,
            vec![
                FilaRejilla::Grupo {
                    nombre: "producción".to_string(),
                    hosts: vec![1, 2],
                },
                FilaRejilla::Host {
                    id: 1,
                    nombre: "web-01".to_string(),
                    bajo_grupo: true,
                },
                FilaRejilla::Host {
                    id: 2,
                    nombre: "web-02".to_string(),
                    bajo_grupo: true,
                },
                FilaRejilla::Host {
                    id: 3,
                    nombre: "db-01".to_string(),
                    bajo_grupo: false,
                },
                FilaRejilla::Host {
                    id: 4,
                    nombre: "nas".to_string(),
                    bajo_grupo: false,
                },
                FilaRejilla::Host {
                    id: 5,
                    nombre: "win-01".to_string(),
                    bajo_grupo: false,
                },
            ]
        );
        assert_eq!(
            estado.manejar_tecla(&tecla(KeyCode::Char(' '))),
            AccionRejilla::Nada
        );
        assert!(estado.grupo_marcado(&[1, 2]));
        assert_eq!(estado.marcados(), 2);
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(estado.marcados(), 0);
        estado.manejar_tecla(&tecla(KeyCode::Down));
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(estado.hosts_marcados(), &[1]);
    }

    #[test]
    fn el_limite_de_cuatro_se_avisa_sin_pasarse() {
        let (hosts, grupos) = inventario();
        let mut estado = EstadoRejilla::nuevo(&hosts, &grupos);
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        // Baja hasta db-01 (fila 3) y marca; luego nas (fila 4) = cuatro.
        for _ in 0..3 {
            estado.manejar_tecla(&tecla(KeyCode::Down));
        }
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(estado.marcados(), 3);
        estado.manejar_tecla(&tecla(KeyCode::Down));
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(estado.marcados(), 4);
        // El quinto (win-01, fila 5) no cabe.
        estado.manejar_tecla(&tecla(KeyCode::Down));
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(estado.marcados(), 4);
        assert!(estado.aviso.is_some());
    }

    #[test]
    fn enter_abre_solo_con_dos_o_mas_y_esc_cancela() {
        let (hosts, grupos) = inventario();
        let mut estado = EstadoRejilla::nuevo(&hosts, &grupos);
        assert_eq!(
            estado.manejar_tecla(&tecla(KeyCode::Enter)),
            AccionRejilla::Nada
        );
        assert!(estado.aviso.is_some());
        // Con el grupo marcado ya hay dos hosts.
        estado.manejar_tecla(&tecla(KeyCode::Char(' ')));
        assert_eq!(
            estado.manejar_tecla(&tecla(KeyCode::Enter)),
            AccionRejilla::Abrir(vec![1, 2])
        );
        assert_eq!(
            estado.manejar_tecla(&tecla(KeyCode::Esc)),
            AccionRejilla::Cancelar
        );
    }
}
