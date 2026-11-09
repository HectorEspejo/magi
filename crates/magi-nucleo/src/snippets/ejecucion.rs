//! Estado de una ejecución de snippet y de cada uno de sus hosts (F6 §7.5):
//! los estados que enseña Resultados y el resultado final (ok, parcial, error
//! o cancelada). La ejecución en sí (semáforo, conexión por host, salida) la
//! lleva cada plataforma; aquí están las reglas comunes.

use crate::deliberacion::EjecucionResultado;

/// Hosts de una ejecución a la vez (§7.6: semáforo de 8).
pub const CONCURRENTES: usize = 8;

/// Lo más que se guarda de stdout y de stderr de un host (1 MiB por flujo).
pub const MAXIMO_SALIDA: usize = 1024 * 1024;

/// Lo que dura una ejecución terminada en memoria antes de olvidarse (§7.6).
pub const VIDA_SEG: i64 = 3600;

/// Estado de un host dentro de una ejecución (diagrama de §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoHostEjecucion {
    /// En espera de turno del semáforo.
    EnCola,
    /// Abriendo la conexión automática con el host.
    Conectando,
    /// Comando en marcha.
    Ejecutando,
    /// Terminó con código 0.
    Ok,
    /// Terminó con código distinto de 0.
    Fallo,
    /// No se pudo ejecutar (huella, credenciales, red, plazo).
    Error,
    /// Cancelada por el usuario.
    Cancelado,
    /// No llegó a ejecutarse por «parar al primer fallo» o por cancelar.
    Omitido,
}

impl EstadoHostEjecucion {
    pub fn es_final(self) -> bool {
        matches!(
            self,
            EstadoHostEjecucion::Ok
                | EstadoHostEjecucion::Fallo
                | EstadoHostEjecucion::Error
                | EstadoHostEjecucion::Cancelado
                | EstadoHostEjecucion::Omitido
        )
    }

    /// Glifo de Resultados (§6.9): ● ok, ✕ fallo o error, ◐ conectando o
    /// ejecutando, ○ en cola, – omitido o cancelado.
    pub fn glifo(self) -> &'static str {
        match self {
            EstadoHostEjecucion::EnCola => "○",
            EstadoHostEjecucion::Conectando | EstadoHostEjecucion::Ejecutando => "◐",
            EstadoHostEjecucion::Ok => "●",
            EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error => "✕",
            EstadoHostEjecucion::Cancelado | EstadoHostEjecucion::Omitido => "–",
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            EstadoHostEjecucion::EnCola => "en cola",
            EstadoHostEjecucion::Conectando => "conectando",
            EstadoHostEjecucion::Ejecutando => "ejecutando",
            EstadoHostEjecucion::Ok => "ok",
            EstadoHostEjecucion::Fallo => "fallo",
            EstadoHostEjecucion::Error => "error",
            EstadoHostEjecucion::Cancelado => "cancelado",
            EstadoHostEjecucion::Omitido => "omitido",
        }
    }

    /// ¿Cuenta como éxito para el resultado de la ejecución?
    pub fn es_ok(self) -> bool {
        matches!(self, EstadoHostEjecucion::Ok)
    }
}

/// Un host de la ejecución, con su estado y lo que duró y salió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEjecucion {
    pub host_id: i64,
    pub host_nombre: String,
    pub estado: EstadoHostEjecucion,
    /// Código de salida, si llegó a ejecutarse.
    pub codigo: Option<i32>,
    pub duracion_ms: i64,
    pub bytes_stdout: u64,
    pub bytes_stderr: u64,
    /// Motivo del error, si lo hubo (con instrucción cuando toca).
    pub error: Option<String>,
}

impl HostEjecucion {
    pub fn nuevo(host_id: i64, host_nombre: String) -> Self {
        Self {
            host_id,
            host_nombre,
            estado: EstadoHostEjecucion::EnCola,
            codigo: None,
            duracion_ms: 0,
            bytes_stdout: 0,
            bytes_stderr: 0,
            error: None,
        }
    }
}

/// Una ejecución de un snippet en uno o varios hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ejecucion {
    pub id: u64,
    pub snippet_id: Option<i64>,
    pub snippet_nombre: String,
    /// El comando con las variables ya sustituidas (escapadas).
    pub comando: String,
    pub hosts: Vec<HostEjecucion>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub deliberacion_id: Option<i64>,
    /// Época en segundos.
    pub creada_en: i64,
    /// Época en segundos; None mientras siga en curso.
    pub terminada_en: Option<i64>,
    pub cancelada: bool,
}

impl Ejecucion {
    /// ¿Todos los hosts están en un estado final?
    pub fn terminada(&self) -> bool {
        self.hosts.iter().all(|host| host.estado.es_final())
    }

    /// El resultado de la ejecución cuando ya terminó: ok si todos ok; error
    /// si ninguno; parcial en otro caso; cancelada si se canceló.
    pub fn resultado(&self) -> Option<EjecucionResultado> {
        if !self.terminada() {
            return None;
        }
        let ok = self.hosts.iter().filter(|host| host.estado.es_ok()).count();
        Some(EjecucionResultado::de_hosts(
            ok,
            self.hosts.len(),
            self.cancelada,
        ))
    }

    /// (ok, fallos, en curso u omitidos) para la fila de Resultados.
    pub fn recuentos(&self) -> (usize, usize) {
        let ok = self.hosts.iter().filter(|host| host.estado.es_ok()).count();
        (ok, self.hosts.len())
    }

    /// Los hosts que todavía no han empezado, para omitirlos.
    pub fn omitir_los_que_esperan(&mut self) {
        for host in &mut self.hosts {
            if matches!(
                host.estado,
                EstadoHostEjecucion::EnCola | EstadoHostEjecucion::Conectando
            ) {
                host.estado = EstadoHostEjecucion::Omitido;
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ejecucion(estados: &[EstadoHostEjecucion]) -> Ejecucion {
        Ejecucion {
            id: 1,
            snippet_id: Some(3),
            snippet_nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: estados
                .iter()
                .enumerate()
                .map(|(indice, estado)| HostEjecucion {
                    host_id: indice as i64,
                    host_nombre: format!("web-0{}", indice + 1),
                    estado: *estado,
                    ..HostEjecucion::nuevo(indice as i64, String::new())
                })
                .collect(),
            timeout_seg: 60,
            parar_al_fallo: false,
            deliberacion_id: None,
            creada_en: 0,
            terminada_en: None,
            cancelada: false,
        }
    }

    #[test]
    fn el_resultado_sale_de_los_estados_de_los_hosts() {
        assert_eq!(
            ejecucion(&[EstadoHostEjecucion::Ok, EstadoHostEjecucion::Ok]).resultado(),
            Some(EjecucionResultado::Ok)
        );
        assert_eq!(
            ejecucion(&[EstadoHostEjecucion::Fallo, EstadoHostEjecucion::Error]).resultado(),
            Some(EjecucionResultado::Error)
        );
        assert_eq!(
            ejecucion(&[EstadoHostEjecucion::Ok, EstadoHostEjecucion::Fallo]).resultado(),
            Some(EjecucionResultado::Parcial)
        );
        assert_eq!(
            ejecucion(&[EstadoHostEjecucion::EnCola, EstadoHostEjecucion::Ok]).resultado(),
            None
        );
        let mut cancelada = ejecucion(&[EstadoHostEjecucion::Cancelado, EstadoHostEjecucion::Ok]);
        cancelada.cancelada = true;
        assert_eq!(cancelada.resultado(), Some(EjecucionResultado::Cancelada));
    }

    #[test]
    fn parar_al_fallo_omite_los_que_esperan() {
        let mut en_curso = ejecucion(&[
            EstadoHostEjecucion::Ok,
            EstadoHostEjecucion::Fallo,
            EstadoHostEjecucion::EnCola,
            EstadoHostEjecucion::Ejecutando,
        ]);
        en_curso.omitir_los_que_esperan();
        assert_eq!(en_curso.hosts[2].estado, EstadoHostEjecucion::Omitido);
        assert_eq!(en_curso.hosts[3].estado, EstadoHostEjecucion::Ejecutando);
        assert!(en_curso.terminada());
        assert_eq!(en_curso.recuentos(), (1, 4));
    }

    #[test]
    fn los_glifos_son_los_de_resultados() {
        assert_eq!(EstadoHostEjecucion::Ok.glifo(), "●");
        assert_eq!(EstadoHostEjecucion::Fallo.glifo(), "✕");
        assert_eq!(EstadoHostEjecucion::Error.glifo(), "✕");
        assert_eq!(EstadoHostEjecucion::Ejecutando.glifo(), "◐");
        assert_eq!(EstadoHostEjecucion::EnCola.glifo(), "○");
        assert_eq!(EstadoHostEjecucion::Omitido.glifo(), "–");
    }
}
