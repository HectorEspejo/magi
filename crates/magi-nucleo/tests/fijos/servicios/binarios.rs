//! Binarios falsos de la Fase 13 para las pruebas de servicios, compartidos
//! por el escritorio y el móvil (D283): `systemctl`, `journalctl`,
//! `docker`/`podman`, `sudo` y `nginx`.
//!
//! Los guiones viven, una sola copia, en `tests/fijos/servicios/falsos/`;
//! este módulo los embebe con `include_str!` y los instala en un directorio
//! temporal sustituyendo el marcador `@DIR@` por su directorio de estado. Se
//! incluye con `#[path]` desde las pruebas de integración:
//!
//! ```ignore
//! #[path = "../crates/magi-nucleo/tests/fijos/servicios/binarios.rs"]
//! mod binarios;                      // desde tests/ del crate raíz
//! #[path = "fijos/servicios/binarios.rs"]
//! mod binarios;                      // desde crates/magi-nucleo/tests/
//! #[path = "../../magi-nucleo/tests/fijos/servicios/binarios.rs"]
//! mod binarios;                      // desde crates/magi-movil/{tests,examples}/
//! ```
//!
//! El estado que leen y escriben los guiones es el de la F13: `show/`,
//! `cat/`, `list_units_*`, `journal*`, `llamadas`, etc. Cuelga de
//! [`BinariosFalsos::estado`] y lo preparan `preparar_estado` y los métodos
//! `unidad`, `texto_unidad`, `contenedor`, `consumo` y `escribir`.
//!
//! `tests/servicios_script.rs` (núcleo) y `tests/servicios_e2e.rs`
//! (escritorio) usan este mismo helper, de modo que los dos juegos de falsos
//! son los mismos guiones y el mismo formato de estado. `tests/
//! servicios_servidor.rs` conserva por ahora su copia incrustada (no se toca
//! en la Fase 14); no se añade ningún juego nuevo.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Directorio temporal con los binarios falsos en `bin/` (lo que se pone en
/// el `PATH` del host de prueba) y su estado en `estado/`.
pub struct BinariosFalsos {
    _temporal: tempfile::TempDir,
    estado: PathBuf,
    bin: PathBuf,
}

impl BinariosFalsos {
    /// Instala los guiones de `falsos/` con `@DIR@` sustituido por el
    /// directorio de estado. El estado empieza vacío: ver [`Self::preparar_estado`]
    /// para los valores por defecto de las pruebas.
    pub fn nuevos() -> Self {
        let temporal = tempfile::tempdir().expect("directorio de binarios falsos");
        let estado = temporal.path().join("estado");
        let bin = temporal.path().join("bin");
        std::fs::create_dir_all(&estado).expect("directorio de estado");
        std::fs::create_dir_all(&bin).expect("directorio de binarios");
        let destino = estado.to_string_lossy().to_string();
        let guiones: [(&str, &str); 6] = [
            ("systemctl", include_str!("falsos/systemctl")),
            ("journalctl", include_str!("falsos/journalctl")),
            ("docker", include_str!("falsos/motor")),
            ("podman", include_str!("falsos/motor")),
            ("sudo", include_str!("falsos/sudo")),
            ("nginx", include_str!("falsos/nginx")),
        ];
        for (nombre, guion) in guiones {
            let guion = guion.replace("@DIR@", &destino);
            Self::instalar(&bin.join(nombre), guion.as_bytes());
        }
        Self {
            _temporal: temporal,
            estado,
            bin,
        }
    }

    fn instalar(destino: &Path, contenido: &[u8]) {
        std::fs::write(destino, contenido).expect("escribir el binario falso");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(destino, std::fs::Permissions::from_mode(0o755))
                .expect("permisos del binario falso");
        }
    }

    /// Directorio de los binarios ejecutables (lo que se antepone al `PATH`).
    pub fn ruta(&self) -> &Path {
        &self.bin
    }

    /// Directorio donde los guiones leen y escriben su estado.
    pub fn estado(&self) -> &Path {
        &self.estado
    }

    /// Escribe un fichero de estado (creando sus directorios), como un
    /// marcador o una unidad falsa.
    pub fn escribir(&self, nombre: &str, contenido: &str) {
        let ruta = self.estado.join(nombre);
        if let Some(padre) = ruta.parent() {
            std::fs::create_dir_all(padre).expect("directorio de estado");
        }
        std::fs::write(ruta, contenido).expect("escribir estado");
    }

    /// Las llamadas que dejaron los binarios falsos, una por línea.
    pub fn llamadas(&self) -> String {
        std::fs::read_to_string(self.estado.join("llamadas")).unwrap_or_default()
    }

    pub fn borrar_llamadas(&self) {
        let _ = std::fs::remove_file(self.estado.join("llamadas"));
    }

    /// Deja un marcador que cambia lo que hace un binario falso (misma cosa
    /// que [`Self::escribir`], con el nombre de siempre en las pruebas).
    pub fn marcar(&self, nombre: &str, contenido: &str) {
        self.escribir(nombre, contenido);
    }

    fn anadir(&self, fichero: &str, linea: &str) {
        let ruta = self.estado.join(fichero);
        let mut contenido = std::fs::read_to_string(&ruta).unwrap_or_default();
        if !contenido.is_empty() && !contenido.ends_with('\n') {
            contenido.push('\n');
        }
        contenido.push_str(linea);
        contenido.push('\n');
        self.escribir(fichero, &contenido);
    }

    fn anadir_si_falta(&self, fichero: &str, clave: &str, linea: &str) {
        let ruta = self.estado.join(fichero);
        let contenido = std::fs::read_to_string(&ruta).unwrap_or_default();
        let ya_esta = contenido
            .lines()
            .any(|vista| vista.split_whitespace().next() == Some(clave));
        if !ya_esta {
            self.anadir(fichero, linea);
        }
    }

    /// Los valores por defecto del estado que usan las pruebas de servicios:
    /// versiones de systemd y de los motores, sudo sin contraseña, un juego de
    /// unidades (app, copia y nginx) y un contenedor, y los diarios vacíos.
    pub fn preparar_estado(&self) {
        self.escribir("sudo_modo", "sin_clave");
        self.escribir("sudo_clave", "secreta");
        self.escribir("systemctl_version", "systemd 252 (252.31-1~deb12u1)\n");
        self.escribir(
            "list_units_service",
            "app.service loaded active running Aplicación\n\
             copia.service loaded failed failed Copia nocturna\n\
             nginx.service loaded active running Servidor web\n",
        );
        self.escribir(
            "list_unit_files_service",
            "app.service enabled enabled\n\
             copia.service enabled enabled\n\
             nginx.service enabled enabled\n",
        );
        self.escribir("list_units_timer", "");
        self.escribir("list_unit_files_timer", "");
        self.escribir(
            "show/app.service",
            "Id=app.service\nNames=app.service\nDescription=Aplicación\n\
             LoadState=loaded\nActiveState=active\nSubState=running\n\
             Type=simple\nResult=success\nUnitFileState=enabled\nCanReload=no\n\
             FragmentPath=/etc/systemd/system/app.service\n",
        );
        self.escribir(
            "show/copia.service",
            "Id=copia.service\nNames=copia.service\nDescription=Copia\n\
             LoadState=loaded\nActiveState=failed\nSubState=failed\n\
             Type=oneshot\nResult=exit-code\nUnitFileState=enabled\nCanReload=no\n",
        );
        self.escribir(
            "show/nginx.service",
            "Id=nginx.service\nNames=nginx.service\nDescription=Servidor web\n\
             LoadState=loaded\nActiveState=active\nSubState=running\n\
             Type=forking\nResult=success\nUnitFileState=enabled\nCanReload=yes\n\
             FragmentPath=/lib/systemd/system/nginx.service\n",
        );
        self.escribir("docker_version", "Docker version 27.3.1, build a7dcaa6\n");
        self.escribir("podman_version", "podman version 5.2.3\n");
        self.escribir("docker_contenedores", "web-app-1\n");
        self.escribir("docker_estado", "running|healthy\n");
        self.escribir(
            "docker_ps",
            r#"{"ID":"a1b2c3d4e5f6","Names":"web-app-1","Image":"nginx:1.27.3","State":"running","Status":"Up 7 days (healthy)","Labels":"com.docker.compose.project=web","Ports":"0.0.0.0:8080->80/tcp"}"#,
        );
        self.escribir("docker_etiquetas", "web|/srv/web/docker-compose.yml\n");
        self.escribir("docker_etiquetas_proyecto", "/srv/web/docker-compose.yml\n");
        self.escribir("docker_estado_proyecto", "running\nrunning\n");
        self.escribir(
            "docker_inspect",
            r#"[{"Id":"a1b2c3d4e5f6","Name":"/web-app-1","Config":{"Image":"nginx:1.27.3","Labels":{}}}]"#,
        );
        self.escribir("docker_stats", "");
        self.escribir("podman_stats", "");
        self.escribir("nginx_codigo", "0");
        self.escribir("nginx_salida", "");
    }

    /// Una unidad de systemd con sus propiedades (las que da `systemctl
    /// show`) y apuntada en `list-units` y `list-unit-files` del tipo que
    /// le toque (servicio o timer).
    pub fn unidad(&self, nombre: &str, props: &[(&str, &str)]) {
        let mut contenido = String::new();
        let mut vistas: Vec<String> = Vec::new();
        let base: [(String, String); 13] = [
            ("Id".to_string(), nombre.to_string()),
            ("LoadState".to_string(), "loaded".to_string()),
            ("ActiveState".to_string(), "active".to_string()),
            ("SubState".to_string(), "running".to_string()),
            ("UnitFileState".to_string(), "enabled".to_string()),
            ("Description".to_string(), format!("prueba {nombre}")),
            ("Type".to_string(), "simple".to_string()),
            ("Result".to_string(), "success".to_string()),
            ("CanReload".to_string(), "yes".to_string()),
            ("ExecMainPID".to_string(), "4242".to_string()),
            ("MemoryCurrent".to_string(), "1048576".to_string()),
            (
                "ActiveEnterTimestampMonotonic".to_string(),
                "1000000".to_string(),
            ),
            (
                "FragmentPath".to_string(),
                format!("/etc/systemd/system/{nombre}"),
            ),
        ];
        for (clave, valor) in &base {
            let valor = props
                .iter()
                .find(|(buscada, _)| buscada == clave)
                .map(|(_, valor)| valor.to_string())
                .unwrap_or_else(|| valor.clone());
            contenido.push_str(&format!("{clave}={valor}\n"));
            vistas.push(clave.clone());
        }
        for (clave, valor) in props {
            if !vistas.iter().any(|vista| vista == clave) {
                contenido.push_str(&format!("{clave}={valor}\n"));
            }
        }
        self.escribir(&format!("show/{nombre}"), &contenido);
        let propiedad = |clave: &str, defecto: &str| {
            props
                .iter()
                .find(|(buscada, _)| *buscada == clave)
                .map(|(_, valor)| valor.to_string())
                .unwrap_or_else(|| defecto.to_string())
        };
        let tipo = if nombre.ends_with(".timer") {
            "timer"
        } else {
            "service"
        };
        self.anadir_si_falta(
            &format!("list_units_{tipo}"),
            nombre,
            &format!(
                "{nombre} loaded {} {} descripcion",
                propiedad("ActiveState", "active"),
                propiedad("SubState", "running")
            ),
        );
        self.anadir_si_falta(
            &format!("list_unit_files_{tipo}"),
            nombre,
            &format!("{nombre} {}", propiedad("UnitFileState", "enabled")),
        );
    }

    /// El texto de `systemctl cat` de una unidad.
    pub fn texto_unidad(&self, nombre: &str, texto: &str) {
        self.escribir(&format!("cat/{nombre}"), texto);
    }

    /// Un contenedor tal como lo enseña `ps -a --format '{{json .}}'`, con su
    /// estado para `inspect`. Se apunta en docker y en podman: la prueba
    /// elige el motor en el escenario.
    pub fn contenedor(&self, id: &str, nombre: &str, estado: &str, etiquetas: &str) {
        let json = format!(
            "{{\"ID\":\"{id}\",\"Names\":\"{nombre}\",\"Image\":\"nginx:1.27\",\"State\":\"{estado}\",\"Status\":\"Up 2 hours\",\"Ports\":\"0.0.0.0:8080->80/tcp\",\"Labels\":\"{etiquetas}\"}}"
        );
        for motor in ["docker", "podman"] {
            self.anadir(&format!("{motor}_ps"), &json);
            self.anadir(&format!("{motor}_contenedores"), nombre);
            self.escribir(&format!("{motor}_estado"), &format!("{estado}|-"));
        }
    }

    /// Las estadísticas de consumo de los contenedores (`stats --no-stream`).
    /// El `id` se conserva por compatibilidad con el helper anterior; los
    /// guiones falsos enseñan todas las líneas guardadas.
    pub fn consumo(&self, _id: &str, linea: &str) {
        for motor in ["docker", "podman"] {
            self.anadir(&format!("{motor}_stats"), linea);
        }
    }
}
