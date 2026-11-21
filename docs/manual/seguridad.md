# Seguridad

[← Índice del manual](README.md)

## Seguridad

- MAGI no guarda claves privadas ni frases: solo referencias
  (`agente:SHA256:…` o `fichero:<ruta>`). Las frases y los ficheros de
  clave pasan por memoria con `zeroize` al cerrar la sesión.
- El servidor de sesiones no dialoga: reenvía huellas, frases y contraseñas
  al cliente solicitante por el socket del propio usuario (directorio 700) y
  los secretos viajan en memoria `zeroize`, jamás al log ni al registro. Las
  pantallas (`Datos`/`PantallaCompleta`) solo llegan a las ventanas adjuntas.
- El servidor nunca acepta ni sustituye huellas por su cuenta: solo con la
  decisión del solicitante.
- Verificación estricta de huellas contra `~/.ssh/known_hosts` (entradas
  hasheadas incluidas). Una huella cambiada bloquea la conexión y sustituirla
  exige escribir el nombre del host; antes se deja copia `known_hosts.old`.
- El sondeo nunca acepta ni sustituye huellas, no dialoga y no toca
  `HOSTS.ultimo_estado`: solo los sondeos van a `SONDEOS`, acotado a 20 por
  host.
- Los túneles escuchan en `127.0.0.1` salvo que la escucha diga otra cosa: una
  escucha en `0.0.0.0` o `::` queda al alcance de toda la red local y el SOCKS5
  no pide autenticación. Un canal `forwarded-tcpip` que no corresponda a un
  túnel remoto registrado se rechaza.
- MAGI genera claves en `~/.ssh` (nunca sobrescribe) pero jamás borra ficheros
  de `~/.ssh` ni quita claves del agente: revocar es una baja lógica.
- Los snippets se ejecutan con sus variables escapadas para el shell
  (`shell-escape`) y la salida de los comandos remotos nunca se escribe en el
  registro ni en el log; al pintarla se quitan las secuencias de escape.
- Las ejecuciones y la deliberación nunca dialogan: la contraseña de una
  ejecución solo sale del llavero de la ventana que la lanzó. CASPER-3 ejecuta
  en **local** el comando que pongas en la ficha, con tu usuario.
- Un snippet crítico, en varios hosts o en un host con verificaciones no se
  ejecuta sin deliberación, y forzarla exige un motivo que queda registrado.
- Un temporal de edición nunca se borra mientras tenga trabajo sin poner a
  salvo; el editor se lanza como programa y argumentos, sin shell. Sobrescribir
  un fichero remoto es atómico (`posix-rename@openssh.com`, o en tres pasos sin
  perder nunca el original) y conserva sus permisos; si el propietario va a
  cambiar, se avisa antes.
- Una sincronización solo borra en el destino rutas fijadas en el plan, nunca
  excluidas, solo tras copiar todo sin errores y siempre con deliberación MAGI.
  `chmod` nunca sigue enlaces y el recursivo pide confirmación con el recuento.
- Los nombres de unidad systemd se validan (`[A-Za-z0-9@._-]+`) antes de
  pasarlos al script de sondeo, que los recibe como argumentos.
- `magi.db`, `magi_config`, `known_hosts`, las claves generadas y las
  exportaciones se escriben con permisos 600 (644 el `.pub`) y de forma
  atómica.
- Un mensaje desconocido en el protocolo produce error y desconexión de ese
  cliente, nunca un fallo del servidor; un servidor de otra versión de
  protocolo se rechaza en el saludo.
- Las sesiones persistentes usan un servidor de tmux propio (`tmux -L magi`:
  su socket se llama `magi` y vive en el directorio de sockets de tmux del
  usuario del host, que tmux crea con modo 700) y
  nunca cambian opciones del tmux habitual. Todo nombre se valida (`[A-Za-z0-9_-]`
  hasta 64) en MAGI y en el script, y se apunta a la sesión por nombre exacto
  (`=<nombre>`): tmux casa prefijos y `agente` no puede tocar `agente-api`. El
  comando de arranque es texto del usuario, llega al script como argumento y no
  va a REGISTRO. Ni la salida de las sesiones, ni sus títulos, ni la vista
  previa ni el historial de tmux van al log ni al registro; REGISTRO guarda solo
  el host y el nombre. El vigilante no abre diálogos.
- Sin telemetría: las únicas conexiones salientes son a los hosts del
  usuario.
