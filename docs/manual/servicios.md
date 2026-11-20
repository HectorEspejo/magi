# Servicios, contenedores y logs

[← Índice del manual](README.md)

## Servicios, contenedores y logs

Desde la Fase 13 MAGI maneja lo que corre en cada host desde la vista
**Servicios** (`F9`; también `u` desde Flota, Hosts y la ficha, con ese host
elegido). A la izquierda, los hosts SSH con el estado de su último sondeo y
`⚠N` de fallos; a la derecha, tres secciones que se cambian con `1`
**Servicios**, `2` **Timers** y `3` **Contenedores**, con su recuento. Arriba
salen los servicios vigilados (`★`) y los fallidos, separados del resto por una
línea; `/` filtra por nombre y `Tab` alterna hosts y lista en modo estrecho.
En «Toda la flota» se juntan los fallos del último sondeo de todos los hosts,
sin abrir ninguna conexión: `↵` va a ese host con el elemento elegido y `r`
sondea todos. Flota marca `⚠N` en la fila del host y su detalle enseña el
bloque FALLOS.

`↵` abre el detalle: estado y subestado desde cuándo, arranque, PID principal,
memoria, fichero de la unidad, si admite recargar, qué comprobación de
configuración se aplicará y sus dependencias («necesita» y «la necesitan»), o
el texto de `systemctl cat`; en un contenedor, id, imagen, creado, estado y
código de salida, reinicios, salud, puertos, montajes, redes, proyecto Compose
y el texto de `inspect`; en un timer, su próxima y última ejecución y el
servicio que activa. `l` abre los logs en vivo, con filtro por texto (`/`) y
por nivel (`n`), pausa (`Espacio`) sin cortar la llegada de líneas, un anillo
de 10 000 líneas y `t` para el seguimiento completo en una pestaña. Al cerrar
el panel se corta el `journalctl` de MAGI en el host.

Por ejemplo, en `web-01` (`192.0.2.10`, que sirve `web.ejemplo.org` con
Nginx delante de una aplicación systemd): `F9`, el host arriba con su `⚠N`,
`R` sobre `app.service`, deliberación, contraseña de sudo si hace falta y el
resultado con las últimas líneas del diario si no vuelve; `l` para ver los
logs y `3` para los contenedores de su proyecto Compose.

**Acciones.** `A` arrancar (en un timer, lanzar ahora su servicio, con
confirmación), `P` parar, `R` reiniciar, `G` recargar y `H` activar o
desactivar el arranque, ofrecidas solo cuando valen en el estado de la unidad:
`G` en un servicio sin `CanReload` avisa «nginx.service no admite recargar».
En un contenedor, `A` `P` `R` según su estado y `S` abre una shell en una
pestaña nueva; sobre la cabecera de un proyecto Compose, `A` `P` y `R`
actúan sobre el proyecto entero con `-p` y los ficheros de sus etiquetas, y
sin ficheros conocidos no se ofrecen y se dice por qué. Tras una acción, MAGI
espera al estado final hasta `espera_seg` (30 s por defecto) y vuelve a mirar a
los 2 s; si la unidad no vuelve a `active`, el resultado es «fallido» con sus
últimas 20 líneas del diario, y si se agota el plazo, «sin confirmar» con la
orden todavía en el host. Una sola acción a la vez por objetivo: otra sobre el
mismo objetivo se rechaza con «hay otra acción en curso».

**Deliberación y confirmación.** Lo que deja algo parado —parar, reiniciar o
desactivar un servicio, y parar o reiniciar un contenedor o un proyecto— pasa
por la deliberación MAGI (F6) con las comprobaciones del host y
`PlanDeliberado::Servicio`; MELCHIOR-1 no cuenta el estado del propio objetivo,
porque reiniciar lo que falla es justo lo que se quiere hacer. Arrancar,
recargar, activar el arranque y lanzar un timer piden una confirmación simple.
Ninguna acción se hace con una pulsación: las teclas que cambian algo en el host
van en mayúsculas. Las unidades críticas (`[servicios] criticas`: sshd, la red,
el cortafuegos, Tailscale, Docker…) avisan en rojo de que la acción «puede
cortar tu conexión» con el host y con los hosts que van a través de él.

**sudo.** Las órdenes privilegiadas van una a una con `sudo`, nunca con
`sudo sh`, y con `timeout` por fuera de sudo: así basta un sudoers limitado y
el registro del host (`journalctl _COMM=sudo` o `auth.log`) enseña la orden
real. Si el usuario es root o `sudo -n` vale, MAGI no pregunta nada; si hace
falta contraseña, la pide en un diálogo, la comprueba con `ComprobarSudo` antes
de confirmar o deliberar y la recuerda solo en memoria (`Zeroizing`) hasta
`sudo_memoria_min` minutos sin uso. Viaja al host por la primera línea de la
entrada estándar, nunca en la línea de órdenes, y no va a disco, al llavero, al
log ni a REGISTRO. `!` pide la contraseña de sudo para el host elegido, para
ver lo que exige permisos; un usuario sin sudo ve «usuario no puede usar sudo
en <host>» y no se le ofrecen las acciones que lo necesitan. Con Podman nunca se
usa sudo: se ven los contenedores del usuario SSH.

**Comprobación de configuración.** Antes de recargar o reiniciar Nginx
(`nginx -t`), Caddy (`caddy validate`), Apache (`apachectl configtest`),
HAProxy (`haproxy -c`) o un proyecto Compose (`compose config -q`), y antes
de aplicar la comprobación propia de un servicio o de un proyecto
(COMPROBACIONES_SERVICIO), MAGI valida la configuración dentro de la propia
acción, en el mismo canal y justo antes: si falla, devuelve 127 (no encontrada)
o pasa de 20 s, no se ejecuta nada y el resultado dice «rechazada por la
configuración» con la salida del validador. Las comprobaciones propias se crean,
editan y borran en la ficha del host y con `c` sobre el servicio o el proyecto;
borrar pide confirmación.

**Permisos que hacen falta en el host.** MAGI usa lo que el host ya tiene y no
instala nada:

- systemd 230 o posterior, con `systemctl` y `journalctl`. Para leer el diario
  del sistema, el usuario SSH debe estar en `systemd-journal`, `adm` o
  `wheel`, o usar sudo. Un host sin systemd se ve como «sin systemd», con solo
  sus contenedores, y uno con systemd anterior a 230, como «systemd demasiado
  antiguo».
- Docker 20.10 o posterior con Compose v2 (`docker compose`): el socket exige
  el grupo `docker` o sudo. Sin permiso, la sección lo dice («añade el usuario
  al grupo docker o pulsa `!`»).
- Podman 4 o posterior, sin root: se ven los contenedores del usuario SSH y con
  Podman nunca se usa sudo. Compose v2 también con `podman compose`.
- `sudo` 1.8 o posterior, `timeout` (coreutils), `awk` y, para los logs de
  contenedor, `mktemp` y `mkfifo` (coreutils/util-linux). Un sudoers limitado
  a las órdenes que MAGI ejecuta basta, por ejemplo:

```sudoers
# /etc/sudoers.d/magi — sudo orden a orden, nunca «sudo sh»
Cmnd_Alias MAGI_SERVICIOS = /usr/bin/systemctl, /usr/bin/journalctl, \
    /usr/bin/docker, /usr/bin/nginx, /usr/bin/apachectl
usuario ALL=(root) NOPASSWD: MAGI_SERVICIOS
```

Con `NOPASSWD`, MAGI detecta `sudo_sin_clave` y no pide contraseña; sin él, la
pide y la comprueba con `ComprobarSudo`. Los hosts sin Docker ni Podman lo
dicen en la sección Contenedores; con los dos, se usa el primero de
`[servicios] contenedores` y la cabecera dice cuál.

```toml
[servicios]
# refresco de la vista abierta en segundos (5-300; 0 = solo al entrar y con r)
refresco_seg = 10
# espera máxima al estado final de una acción en segundos (5-300)
espera_seg = 30
# líneas del anillo del panel de logs (1 000-100 000)
lineas_log = 10000
# líneas anteriores al abrir los logs (0-5 000)
lineas_iniciales = 200
# motores de contenedores por orden de preferencia ([] desactiva)
contenedores = ["docker", "podman"]
# unidades que pueden cortar la conexión con el host (admite * como patrón)
criticas = ["sshd.service", "ssh.service", "networking.service",
  "NetworkManager.service", "systemd-networkd.service",
  "systemd-resolved.service", "firewalld.service", "ufw.service",
  "nftables.service", "iptables.service", "tailscaled.service",
  "wg-quick@*.service", "docker.service", "containerd.service",
  "podman.service"]
# minutos sin uso tras los que se olvida la contraseña de sudo (0-15; 0 = pedirla siempre)
sudo_memoria_min = 5
```

El fichero de sincronización sube a `formato` 3 en cuanto hay alguna
comprobación de servicio propia: los equipos con MAGI 1.2 lo verán como
«formato nuevo» hasta que se actualicen. La app Android ya lee el formato 3 y
guarda las comprobaciones en su copia sin usarlas; la vista de servicios del
móvil llega en la Fase 14. Desde la 1.5.0 sube a `formato` 4 en cuanto hay
alguna ficha de [sesión persistente](sesiones-persistentes.md#sesiones-persistentes) o algún host con
«Todas las sesiones persistentes»; el 4 incluye lo del 3, y los equipos con
una versión anterior lo verán como «formato nuevo» hasta que se actualicen. La
app Android lee el formato 4 y guarda las fichas sin enseñarlas.
