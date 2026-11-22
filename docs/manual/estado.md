# Qué se ha probado a mano

[← Índice del manual](README.md)

Este documento dice con honestidad qué se ha probado a mano, por quién y
cuándo, y qué solo cubren las pruebas automáticas. MAGI se desarrolla con un
agente de código y lo prueba una sola persona con sus equipos y hosts: lo que
no figura aquí como probado a mano **no se ha probado a mano**, aunque tenga
pruebas automáticas. Última actualización: 20 de noviembre de 2026, antes de la
primera publicación.

## Cómo se lee

- **A mano:** el desarrollador lo ha usado con hosts o dispositivos reales. Se
  anota la fecha cuando consta.
- **Automático:** lo cubren las pruebas del repositorio (más de 2 400, en la CI
  de GitHub sobre Ubuntu 22.04 y 24.04; en macOS solo compila y pasa las
  pruebas unitarias de la biblioteca). Las pruebas no usan red externa: usan un
  servidor SSH de pruebas en proceso, `sftp-server` real y un `tmux` real donde
  hace falta.
- **Pendiente:** hay un guion de validación escrito, en el informe de
  implementación de la fase, y nadie lo ha ejecutado todavía.

Cuando alguien valide una de las partes pendientes, este documento se actualiza
con la fecha y el resultado.

## Por plataforma

| Plataforma | Estado | A mano | Pendiente |
|---|---|---|---|
| Linux (Omarchy) | Uso diario | Inventario y conexión (fase 1, 15 sep 2026) y redimensionado (fase 7, 3 oct 2026), validados; además, el uso diario del desarrollador | Las fases 2 a 6 y 8 a 16, abajo |
| macOS | En pruebas | Nada: nunca se ha probado a mano durante el desarrollo | La CI compila para macOS y pasa las pruebas de la biblioteca. Un guion corto (compilar, conectar con clave del agente y con contraseña del Llavero, pestañas que sobreviven a la ventana, SFTP, un túnel y parar el servidor) lo ejecuta el desarrollador antes de la primera release; sale «en pruebas» y sin notarizar |
| App Android | Beta | En el plegable (un Honor con MagicOS): instalación, bloqueo con el Keystore y un túnel local (corrección de la fase 11, 4 nov 2026), y los arreglos de `age` y de Flota con la APK de prueba (hacia el 15 nov 2026) | El resto de lo que cubre el guion de la fase 11 y los de las fases 14 y 15 en ese mismo móvil. Antes de la versión final se instala una candidata con Obtainium y se recorre un guion corto: instalar y comprobar el certificado, desbloquear, unir la sincronización, abrir un terminal y un túnel, y abrir Servicios |
| Termux | En pruebas | Nada | Prueba de concepto y guion de validación en el plegable (foco, portapapeles, plazo de arranque, avisos y wake lock); el `.deb` real |

## Por fase

| Fase | Qué es | A mano | Pendiente |
|---|---|---|---|
| 1 | Inventario y conexión | Validada en Omarchy, 15 sep 2026 | macOS |
| 2 | Flota, identidades y registro | — | Contraseña contra un `sshd` real, RSA 4096, portapapeles en Wayland, revocar una clave |
| 3 | Pestañas y servidor de sesiones | Parcial, 15 sep 2026: salieron seis fallos (dos críticos), corregidos | Validación completa (la corrección 3b cambió el pool que usan todas las vistas) |
| 4 | Archivos (SFTP) | — | Guion con hosts reales. Automático: 17 pruebas de extremo a extremo con `sftp-server` real |
| 5 | Túneles | Túnel remoto y fallo de bind, contra un host real, 16 sep 2026 | El resto de los casos |
| 6 | Snippets y deliberación | — | Validación manual. La revisión adversarial de sus tres primeros sprints no se hizo |
| 7 | Redimensionado adaptable | Validada, 3 oct 2026 | — |
| 8 | Archivos, segunda vuelta | — | Guion de 12 pasos de su informe. La revisión adversarial no se hizo |
| 9 | Empaquetado y releases | Ensayo de la release con `dist`, validado el 18 nov 2026 | AUR y Homebrew no se publican todavía: se comprobarán cuando se activen |
| 10 | Sincronización cifrada | — | Llavero real, Syncthing entre dos equipos, QR y `age` real |
| 11 | App Android | Ver la plataforma Android | El plegable (guion de su informe) |
| 12 | Escritorio remoto | RDP contra un Windows 11 real, 5 nov 2026: conexión, NLA, huella, rectángulos y visor con la imagen completa (la pasada la hizo el agente por el túnel del desarrollador) | El resto del guion: una VM con QEMU, xrdp, TigerVNC y voxl. El AVC420 está implementado pero no se anuncia en esta versión |
| 13 | Servicios, contenedores y logs | — | Nginx, sudo, Docker Compose y Podman reales |
| 14 | Android, segunda vuelta (móvil de guardia) | — | El plegable; la primera ejecución del job `humo` de GitHub |
| 15 | Terminal, segunda vuelta | — | Claude Code real en Alacritty y Ghostty (guion de 12 pasos) |
| 16 | Sesiones persistentes | — | Claude Code real con tmux 3.5 (Mayús+Intro, `modifyOtherKeys`); Dropbear, que no se ha podido probar (el script ya no viaja en la línea de órdenes, pero no se ha probado con un Dropbear real, ni con `dash` ni `mawk`); el diálogo «Volver a la sesión», el diálogo que vuelve con la carpeta que no existe y el aviso de atención sin la lista vieja, que solo se han probado con `TestBackend` y tmux real en la CI local |
| 17 | MAGI en Termux | — | Todo lo que solo se ve en el móvil: prueba de concepto y guion en el plegable |

La fase 9b (primera publicación) no añade funciones: recoge el cierre de la
fase 16 y la corrección de la sincronización (10b), prepara el repositorio y se
valida con la propia release. De la corrección 10b, `magi sincronizar` con un
servidor real y una pestaña abierta, y la app con un fichero cortado en la
carpeta, quedan por probar a mano.

## Qué esperar

- Las fases con «—» en la columna «A mano» tienen sus pruebas automáticas y,
  salvo las que lo dicen, la revisión adversarial que consta en su informe, pero
  no se han usado a mano con hosts reales (más allá de lo que cada fila anota):
  pueden tener defectos que las pruebas no cazan. Las revisiones adversariales
  de cierre que no se hicieron (fases 6 y 8) y la de la fase 16, que hizo la 9b,
  constan en sus informes. Si usas
  MAGI con datos o equipos importantes, haz copia de `~/.ssh` y de
  `~/.local/share/magi/` antes de empezar.
- El uso diario del desarrollador es en Linux (Omarchy) con hosts reales; esa es
  la razón de que Linux figure «uso diario» y las demás plataformas, no.
- Si encuentras un fallo, abre una issue con la plantilla «Fallo» y la salida
  de `magi diagnostico`; las correcciones salen como versiones PATCH.
