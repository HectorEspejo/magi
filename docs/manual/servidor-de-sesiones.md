# El servidor de sesiones

[← Índice del manual](README.md)

## El servidor de sesiones

Desde la Fase 3 hay dos procesos con el mismo binario: la TUI (`magi`) es el
cliente y `magi --servidor` es un pequeño servidor local que **custodia las
sesiones SSH**. El primer cliente lo lanza desacoplado (`setsid`) y hablan por
un socket Unix en `$XDG_RUNTIME_DIR/magi/servidor.sock` (macOS:
`~/Library/Caches/magi/`), con directorio 700 y bloqueo por `flock`.

- Cerrar la ventana **no cierra las sesiones**: siguen en el servidor y
  reaparecen al abrir `magi` de nuevo.
- La misma pestaña puede abrirse desde varias terminales: todas ven y
  escriben (sin arbitraje, como tmux) y el tamaño remoto es el mínimo de las
  ventanas adjuntas.
- Varios terminales comparten el servidor; el diálogo de huella, frase o
  contraseña lo recibe solo la ventana que pidió la conexión.
- Cada host tiene como mucho **una conexión compartida** en el servidor. El
  SFTP, los túneles, el sondeo por la conexión viva y las ejecuciones de
  snippets la usan siempre; las pestañas, solo si el host tiene
  `multiplexar`: entonces una segunda pestaña abre un canal nuevo sin
  reautenticar ni volver a preguntar la huella. Sin `multiplexar`, cada
  pestaña abre su propia conexión, que se cierra con ella. Abrir una pestaña
  nunca tumba un túnel ni el SFTP del host, y dos aperturas simultáneas del
  mismo host esperan una a la otra en vez de abrir dos conexiones. La
  conexión compartida se cierra tras 30 s sin canales.
- Sin clientes, sesiones, transferencias ni túneles levantados durante 10 s el
  servidor se apaga solo (`[servidor] gracia_apagado_seg`). Si muere, las
  ventanas lo detectan y ofrecen relanzarlo.
- `SIGTERM` para el servidor igual que `magi servidor parar`: cierra sesiones
  y túneles anotándolo y borra socket y lock. Si el servidor en marcha es de
  otra versión (tras actualizar MAGI), `magi servidor parar` muestra su pid y
  le envía `SIGTERM` tras confirmar (`--si` no pregunta); al abrir `magi`, el
  servidor anterior se sustituye solo si no custodia nada y, si no, se pregunta
  (ver [Actualizar MAGI](instalacion.md#actualizar-magi)).
- El sondeo de Flota, con sesión viva, ejecuta el script sobre esa conexión;
  sin ella cae a su conexión efímera.
- El log del servidor va en `~/.local/state/magi/logs/servidor.log.<fecha>`;
  para depurarlo, `cargo run -- --servidor` en primer plano.
