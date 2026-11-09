//! Servidor de pruebas del móvil de guardia (Fase 14): envoltorio fino del
//! módulo `tests/comun/servidor_pruebas.rs` con los puertos fijos que esperan
//! las fixtures del emulador.
//!
//!   cargo run -p magi-movil --example servidor_pruebas
//!
//! Escucha SOLO en 127.0.0.1 del anfitrión (R107): el emulador Android llega a
//! él por 10.0.2.2, que es el loopback del anfitrión. Su SSH solo reenvía los
//! canales `direct-tcpip` al servicio de eco de la prueba.
//!
//! Puertos fijos (las fixtures y las pruebas instrumentadas los llevan
//! escritos):
//!
//! - SSH en 127.0.0.1:12222 (el emulador lo ve en 10.0.2.2:12222);
//! - eco en 127.0.0.1:19091, destino de los túneles de la prueba;
//! - control en 127.0.0.1:12223: al escribir «tirar» corta todas las
//!   conexiones SSH, como un cambio de red (lo usan las pruebas del vigilante).
//!
//! El host SSH de pruebas:
//!
//! - acepta cualquier clave pública sin verificar la firma y la contraseña
//!   «secreta»;
//! - usa la clave de host fija de siempre (la que llevan las fixtures en
//!   `known_hosts`);
//! - sirve `exec` con `sh -c` y los binarios falsos compartidos de la F13
//!   delante en el `PATH` (`MAGI_USUARIO` lleva el usuario autenticado), con
//!   el directorio de trabajo fijo `/tmp/magi-pruebas/trabajo`;
//! - sirve PTY y shell (`sh` conectado a la entrada y las salidas del canal)
//!   para las sesiones con orden;
//! - sirve el subsistema `sftp` con el `sftp-server` real de OpenSSH
//!   (`MAGI_SFTP_SERVER` manda si apunta a un fichero existente);
//! - solo reenvía `direct-tcpip` a 127.0.0.1:19091.
//!
//! Contrato que esperan las fixtures de Flota, Servicios, Snippets y
//! deliberación:
//!
//! - host `10.0.2.2:12222`, usuario `usuario` y clave de host
//!   `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMDUg4hwJY3WuFnQXmPzlo2c28c7JgaJ2zkxyf8OGHJs`;
//! - systemd 252 con `app.service` (activa, `CanReload=no`), `copia.service`
//!   (fallida), `nginx.service` (activa, `CanReload=yes`) y
//!   `postgresql.service` (activa), más el timer `app.timer` (activo);
//! - Docker 27 con `web-app-1` (en marcha, sano) y `panel` (caído con salida
//!   1) en el proyecto Compose `web` de `/srv/web/docker-compose.yml`;
//! - sudo sin contraseña (la contraseña de prueba es «secreta») y `nginx -t`
//!   correcto;
//! - backups en `/tmp/magi-pruebas/backups` para BALTHASAR-2, legibles por
//!   SFTP: `db-1.sql.gz` (hace 3 h), `db-2.sql.gz` (hace 31 h) y `notas.txt`
//!   (hace 1 h);
//! - directorio de trabajo de los `exec` y las shells:
//!   `/tmp/magi-pruebas/trabajo`.
//!
//! Imprime una línea «listo: …» cuando ya escuchan los tres puertos, el
//! directorio de backups está preparado y los binarios falsos instalados: la
//! CI la espera antes de arrancar el emulador (R106).

#[path = "../tests/comun/servidor_pruebas.rs"]
mod servidor_pruebas;

use servidor_pruebas::{arrancar, PuertosPrueba};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let servidor = arrancar(PuertosPrueba::fijos()).await;
    println!(
        "listo: SSH 127.0.0.1:{} · ECO 127.0.0.1:{} · CONTROL 127.0.0.1:{} · BACKUPS {}",
        servidor.puertos.ssh,
        servidor.puertos.eco,
        servidor.puertos.control,
        servidor_pruebas::DIRECTORIO_BACKUPS
    );
    // El proceso vive hasta que lo paren la CI o el desarrollador (SIGTERM o
    // Ctrl+C); el servidor se para con él.
    std::future::pending::<()>().await;
}
