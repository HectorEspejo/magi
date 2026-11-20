# Escritorio remoto

[← Índice del manual](README.md)

## Escritorio remoto

Además de hosts SSH, el inventario admite **hosts de escritorio**: `RDP`
(Windows, xrdp, GNOME Remote Desktop y KDE), `VNC` (QEMU/libvirt/Proxmox,
TigerVNC y x11vnc) y `voxl` (un host Linux con X11 que tenga `voxl-server`
instalado). Se crean como cualquier host, con grupo y etiquetas; el **tipo se
elige al crearlo y no se cambia** después. Los escritorios no salen en
`magi_config`, ni en el terminal, ni en SFTP, túneles, snippets o saltos: solo
los hosts SSH hacen esas cosas.

El escritorio se ve en una **ventana del navegador** (Chromium, Brave, Chrome o
Edge en modo aplicación), porque una TUI no pinta imágenes. La página la sirve
el propio `magi --servidor` **solo en `127.0.0.1`**, en un puerto libre, y se
entra con un enlace de un solo uso que caduca a los 60 s y se gasta al usarlo.
La clave del visor vive en el `sessionStorage` de la pestaña y la URL se
limpia al canjearla. El servidor comprueba `Host` siempre y `Origin` en
`/api/*`, y sirve una CSP estricta sin recursos externos.

En RDP, la imagen llega al visor como **rectángulos** (mapas de bits y RemoteFX
decodificados por IronRDP) y el cursor se pinta aparte. El H.264 de Windows
(AVC420) todavía no se anuncia: exige decodificar también los demás códecs del
canal EGFX, que es lo que manda Windows para la interfaz; está anotado como
pendiente en el informe de la Fase 12.

### Abrir un escritorio

`↵` sobre un escritorio lo abre; si ya tenía sesión, va a su pestaña y abre el
visor con un enlace nuevo. La sesión la custodia `magi --servidor`: sobrevive a
cerrar la TUI y se ve desde todas las ventanas. Mientras no haya ningún visor,
la sesión se cierra sola al cabo de `[escritorio] gracia_seg` (60 s por
defecto); se puede volver a abrir el visor desde la pestaña antes de que expire.
Un segundo visor sustituye al primero.

Siempre que se puede, la conexión va **por SSH**: se toma la conexión del host
«a través de» y se abre un canal `direct-tcpip` hasta el servidor de
escritorio, sin abrir puertos en tu equipo. Un VNC que vaya directo a una
dirección que no es de loopback avisa de que la imagen y el teclado viajarán sin
cifrar. RDP directo va cifrado con TLS.

### Certificado y credenciales

En RDP, antes de mandar ninguna credencial se comprueba el certificado del
servidor con la misma política que las huellas SSH: si es la primera vez, se
pide aceptarlo (y se guarda en `HOSTS.huella_escritorio`, propia de ese
equipo); si cambia, el aviso es rojo y hay que escribir el nombre del host para
sustituirlo. Las contraseñas de RDP y VNC viven solo en el llavero del sistema;
el servidor de sesiones nunca lo toca: se las pide al cliente que abrió el
escritorio.

### voxl

`voxl` se arranca **en el host SSH** con un script embebido y una contraseña
aleatoria por sesión que viaja por la entrada estándar, nunca en la línea de
órdenes. Escucha solo en `127.0.0.1` del host y se llega por un túnel de la
sesión. Al cerrar la sesión (o si se cae la conexión SSH) el script para voxl y
lo mata si no termina. MAGI no instala voxl: si falta, lo dice.

### Configuración

```toml
[escritorio]
# auto o una orden con {url}, por ejemplo "brave --app={url}"
navegador = "auto"
# segundos sin visor antes de cerrar la sesión (10-3600)
gracia_seg = 60
# puerto del servidor del visor; 0 = uno libre
puerto_visor = 0
# auto (sale de LANG) o una distribución como es-ES
distribucion_teclado = "auto"
# reservado para el H.264 de Windows (AVC420); hoy RDP va por rectángulos
h264 = true
```

`magi diagnostico` dice qué navegador abrirá el visor y avisa si no encuentra
ninguno. El portapapeles es solo de texto y no se guarda: ni la imagen, ni las
teclas, ni el portapapeles pasan por el log ni por el registro.
