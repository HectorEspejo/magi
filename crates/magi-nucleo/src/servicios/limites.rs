//! Límites de la entrada no fiable y plazos de la Fase 13 (§8 del informe).
//!
//! Todo lo que llega del host se comprueba contra estos topes: lo que se pasa
//! se corta con aviso y nunca provoca un pánico. Los plazos son los que usa el
//! servidor (§5.2).

/// Tamaño máximo de la salida de un listado (2 MiB).
pub const LISTADO_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Unidades como máximo en un listado (2 000).
pub const MAX_UNIDADES: usize = 2000;

/// Contenedores como máximo en un listado (500).
pub const MAX_CONTENEDORES: usize = 500;

/// Proyectos Compose como máximo en un listado (100).
pub const MAX_PROYECTOS: usize = 100;

/// Longitud máxima de un nombre de unidad, contenedor o proyecto (256).
pub const MAX_NOMBRE: usize = 256;

/// Texto de `systemctl cat` o de `inspect` como máximo (256 KiB).
pub const TEXTO_MAX_BYTES: usize = 256 * 1024;

/// Una línea de log como máximo (16 KiB).
pub const LINEA_LOG_MAX_BYTES: usize = 16 * 1024;

/// Líneas por lote de logs (500).
pub const LOTE_LOG_MAX: usize = 500;

/// Cola de una suscripción de log antes de descartar las más viejas (4 MiB).
pub const COLA_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Salida de una comprobación de configuración como máximo (16 KiB).
pub const SALIDA_COMPROBACION_MAX_BYTES: usize = 16 * 1024;

/// Longitud máxima de una comprobación propia (1 024 caracteres).
pub const COMPROBACION_MAX_CHARS: usize = 1024;

/// Dependencias como máximo en un detalle (500).
pub const MAX_DEPENDENCIAS: usize = 500;

/// Valores de una propiedad `systemctl show` como máximo (4 KiB).
pub const PROPIEDAD_MAX_BYTES: usize = 4 * 1024;

/// Plazo de un listado, un consumo o un detalle (10 s).
pub const PLAZO_LISTADO_SEG: u64 = 10;

/// Plazo de una comprobación de configuración (20 s).
pub const PLAZO_COMPROBACION_SEG: u64 = 20;

/// Margen que se suma a `espera_seg` para dar el plazo de toda una acción
/// (30 s).
pub const MARGEN_ACCION_SEG: u64 = 30;

/// Segunda mirada al estado después de la orden (2 s).
pub const SEGUNDA_MIRADA_SEG: u64 = 2;

/// Espera por defecto al estado final de una acción (30 s).
pub const ESPERA_POR_DEFECTO_SEG: u64 = 30;

/// Refresco por defecto de la vista abierta (10 s).
pub const REFRESCO_POR_DEFECTO_SEG: u64 = 10;

/// Tamaño por defecto del anillo del panel de logs (10 000 líneas).
pub const LINEAS_LOG_POR_DEFECTO: usize = 10_000;

/// Líneas anteriores al abrir el panel de logs (200).
pub const LINEAS_INICIALES_POR_DEFECTO: u32 = 200;

/// Minutos sin uso tras los que se olvida la contraseña de sudo (5).
pub const SUDO_MEMORIA_MIN_POR_DEFECTO: u64 = 5;

/// Unidades críticas por defecto (§7.9): las que pueden cortar la conexión
/// con el host. Admite patrones con `*`.
pub const CRITICAS_POR_DEFECTO: &[&str] = &[
    "sshd.service",
    "ssh.service",
    "networking.service",
    "NetworkManager.service",
    "systemd-networkd.service",
    "systemd-resolved.service",
    "firewalld.service",
    "ufw.service",
    "nftables.service",
    "iptables.service",
    "tailscaled.service",
    "wg-quick@*.service",
    "docker.service",
    "containerd.service",
    "podman.service",
];

/// Motores de contenedores por defecto, por orden de preferencia (§5.3).
pub const MOTORES_POR_DEFECTO: &[&str] = &["docker", "podman"];
