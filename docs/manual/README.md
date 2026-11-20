# Manual de MAGI

[← README](../../README.md) · [English README](../../README.en.md)

El manual cuenta cómo se usa cada parte de MAGI. Está escrito en castellano,
igual que el código y la interfaz. Cada capítulo es una parte de lo que antes
era el README; para saber qué se ha probado a mano y qué no, mira
[`estado.md`](estado.md).

## Capítulos

- [Instalación, actualización y desinstalación](instalacion.md) — Requisitos, instalación por canal con su verificación, actualizar, autocompletado y man, desinstalar
- [Primeros pasos y diagnóstico](primeros-pasos.md) — Primeros pasos, primer arranque y `magi diagnostico`
- [El servidor de sesiones](servidor-de-sesiones.md) — Qué custodia el servidor, cómo se para y se actualiza
- [Pestañas y terminal](pestanas-y-terminal.md) — Pestañas, tamaño de la terminal, paneles, historial, enlaces y Claude Code
- [Sesiones persistentes](sesiones-persistentes.md) — Sesiones que sobreviven a MAGI y al equipo, con tmux, y `magi sesiones`
- [Archivos](archivos.md) — El gestor de archivos SFTP, ediciones remotas y sincronización de carpetas
- [Túneles](tuneles.md) — Túneles locales, remotos y SOCKS5
- [Snippets y deliberación MAGI](snippets-y-deliberacion.md) — Órdenes guardadas, variables y la deliberación antes de ejecutar
- [Flota, identidades y registro](flota-identidades-y-registro.md) — Sondeo de la flota, autenticación, identidades y el registro
- [Sincronización del inventario](sincronizacion.md) — Sincronizar el inventario entre equipos, cifrado, carpeta compartida y `magi sincronizar`
- [Escritorio remoto](escritorio-remoto.md) — RDP, VNC y voxl en el visor web
- [Servicios, contenedores y logs](servicios.md) — systemd, Docker y Podman, y sus logs, desde la vista Servicios
- [App Android](app-android.md) — La app compañera y el móvil de guardia
- [Termux](termux.md) — MAGI dentro de Termux en Android
- [Atajos](atajos.md) — Teclas de cada vista
- [El `Include` de `~/.ssh/config`](ssh-config.md) — Cómo se integra `~/.ssh/magi_config`
- [Seguridad](seguridad.md) — Modelo de amenazas, huellas, secretos y límites

## Qué se ha probado

[`estado.md`](estado.md) dice, por plataforma y por fase, qué se ha probado a
mano, con la fecha, y qué solo lo cubren las pruebas automáticas.

## Otras guías

- [Formato del fichero de sincronización](../formato-sincronizacion.md): la
  referencia del formato que usan el escritorio y la app.
- [Publicar una versión](../publicar.md), [publicar la app Android](../publicar-android.md)
  y [publicar el paquete de Termux](../publicar-termux.md): para quien mantiene el proyecto.
- [Contribuir](../../CONTRIBUTING.md) y [seguridad](../../SECURITY.md).
