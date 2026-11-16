# MAGI - Checklist Fase 15: Terminal, Segunda Vuelta

## Núcleo: emulador y compatibilidad
- [x] `crates/magi-nucleo/src/terminal/emulador.rs` con los `Callbacks` de vt100 0.16: modos 1004 y 2026, banderas de kitty con una pila por pantalla (16 como mucho), ED 3, título, avisos (BEL, OSC 9 y OSC 777) y OSC 52; el mismo código en el servidor (responde) y en el cliente (no responde)
  - AC: Dado `CSI ? 2026 h` partido entre dos lotes, cuando se procesan, entonces el modo 2026 queda activo
- [x] Respuestas a DA1, DA2, DSR 5 y 6, DECRQM (§5.5 del informe), `CSI ? u`, XTVERSION, XTWINOPS 18 y OSC 10, 11 y 12, en orden con la salida; OSC 52 de lectura nunca se responde
  - AC: Dado un remoto que escribe «hola» al principio de la fila 3 y pide `CSI 6 n`, cuando el servidor lo procesa, entonces responde `CSI 3;5R` en el mismo lote
- [x] Codificación de teclas con la bandera 1 de kitty, como Ghostty: Intro, Tab y Retroceso sin modificadores no cambian; Mayús+Intro es `CSI 13;2u`, Esc es `CSI 27u` y Ctrl o Alt + tecla van en `CSI u`; sin banderas, la codificación clásica de la F11
  - AC: Dado un remoto que empujó `CSI > 1 u`, cuando se pulsa Mayús+Intro, entonces se envía `CSI 13;2u`; tras `CSI < 1 u`, se envía `CR`
- [x] `terminal/raton.rs`: codificación de pulsar, soltar, arrastrar y rueda con modificadores para los modos 1000, 1002 y 1003 y las codificaciones por defecto, UTF-8 (1005) y SGR (1006)
- [x] `terminal/pegado.rs`: saneado (sin `ESC[200~`, `ESC[201~` ni controles salvo tabulador y saltos de línea; finales de línea a CR) y marcas si el remoto activó 2004
  - AC: Dado un texto con `ESC[201~rm -rf ~` dentro, cuando se pega en un remoto con 2004, entonces llega un solo bloque `ESC[200~…ESC[201~` sin la marca interior
- [x] `terminal/historial.rs`: captura del historial y la pantalla principal en tramos con `envuelta`, búsqueda (texto, con mayúsculas solo si el patrón las tiene, o expresión regular con `regex`) sobre líneas lógicas, texto de una selección y texto plano para guardar
  - AC: Dada una línea de 300 caracteres partida en dos filas, cuando se busca una palabra que cruza el corte, entonces se encuentra, y al copiarla sale en una sola línea
- [x] `terminal/enlaces.rs`: enlaces `http` y `https` en líneas lógicas (sin la puntuación final, 10-2048 caracteres), destinos locales (`localhost`, `127.0.0.1`, `::1`, `0.0.0.0`) y URL reescrita para el túnel temporal
- [x] `conexion/reconexion.rs`: causas recuperables y no recuperables, esperas (2, 5, 10 y 30 s, luego 60 s) y detección de la vuelta de una suspensión
- [x] `terminal/tmux.rs` y `scripts/terminal_tmux.sh` (`comprobar`, `abrir`, `listar`, `terminar`) con nombres validados en Rust y en el script, y `magi-<equipo>-<n>` para las sesiones nuevas
  - AC: Dado un host sin tmux, cuando se ejecuta `abrir`, entonces se abre el shell de acceso, y `listar` devuelve `MAGI_TMUX=no`
- [x] Migración 10: HOSTS `reconectar` (1), `tmux_persistente` (0) y `portapapeles_remoto` (0), propios del equipo
  - AC: Dado un equipo con la 1.4 que cambia esos campos, cuando sincroniza, entonces el fichero no los lleva y `formato` sigue en 3

## Servidor
- [x] Parser de cada pestaña con historial de `historial_lineas` y tope `historial_mib` según las columnas, recalculado al redimensionar
- [x] `CSI 3 J` vacía el historial de la pestaña (lote partido y parser rehecho con su estado)
  - AC: Dado un historial de 500 líneas, cuando llega `CSI 2 J CSI 3 J`, entonces la captura siguiente no tiene historial
- [x] Las respuestas las escribe solo el servidor en el canal, una vez por pestaña, con o sin ventanas adjuntas
  - AC: Dada una pestaña adjunta en dos ventanas, cuando el remoto pide DA1, entonces recibe una sola respuesta
- [x] `CapturarHistorial` → `HistorialLote` en lotes cortados por tamaño codificado, con plazo de 5 s
- [x] Título, aviso (uno cada 5 s) y estado de reconexión en `Sesiones{lista}`, saneados y de hasta 120 caracteres
- [x] Foco agregado de las ventanas (`Foco`); con 1004 activo, `CSI I` y `CSI O` al remoto cuando cambia
- [x] OSC 52 del remoto: con permiso, `Portapapeles` a la ventana con la pestaña enfocada; sin permiso, `PortapapelesPedido` y la última petición 60 s en memoria
- [x] `COLORTERM=truecolor` pedido al abrir cada pestaña (el host puede ignorarlo) y colores del tema recibidos en `Adjuntar` para OSC 10, 11 y 12
- [x] `RenombrarSesion` y `MoverSesion` para todas las ventanas, con nombres únicos («(n)» si se repite)

## Terminal exterior y pegado
- [x] MAGI pide al terminal exterior el pegado entre corchetes, los eventos de foco y, con `teclado_mejorado` y si lo admite, la desambiguación de kitty
- [x] Al salir, al ceder la terminal (visor, paginador) y en un pánico, MAGI deshace lo que pidió al terminal exterior (pegado, foco, banderas de kitty y ratón)
  - AC: Dado MAGI con el ratón capturado y las banderas de kitty, cuando se fuerza un pánico en una prueba, entonces el terminal queda sin captura ni banderas
- [x] El prefijo se reconoce en la codificación clásica (`0x1d`) y en la de kitty (`CSI 93;5u`)
- [x] Pegar en una pestaña usa `pegado.rs`: un solo pegado entre corchetes, en escrituras propias y sin teclas detrás
  - AC: Dado Claude Code en el remoto, cuando se pegan 5 líneas, entonces llegan como un solo pegado y no se envía ningún mensaje
- [x] Aviso PEGAR VARIAS LÍNEAS (`[terminal] avisar_pegado`) cuando el remoto no activó 2004, con las primeras líneas; `↵` pega y `Esc` cancela
- [x] Salida sincronizada en el cliente: con 2026 abierto, la pestaña no se pinta hasta el cierre o 200 ms
  - AC: Dado un remoto que abre 2026, pinta 40 filas y lo cierra, cuando el cliente pinta, entonces no se ve ningún estado intermedio

## Ratón
- [x] Si el remoto del panel enfocado pide ratón (1000, 1002 o 1003), MAGI lo captura y se lo pasa con su modo y codificación, también con `raton = false`; lo suelta cuando el remoto deja de pedirlo, al cambiar de panel o de modo y al salir de la vista
  - AC: Dado Claude Code a pantalla completa, cuando se gira la rueda sobre su panel, entonces su conversación se desplaza
- [x] Con `raton = true`: la rueda en la pantalla principal abre el modo historial, en la alternativa sin ratón envía 3 flechas por paso, el clic enfoca otro panel y el arrastre selecciona en el modo historial
- [x] La ayuda dice que, con el ratón capturado, la selección del terminal exterior sigue con Mayús+arrastre

## Historial, búsqueda y copia
- [x] `prefijo [` y `Shift+RePág` (este, solo con la pantalla principal) abren el modo historial del panel enfocado
  - AC: Dado Vim abierto en la pestaña, cuando se pulsa `Shift+RePág`, entonces la tecla llega al remoto y no se abre el modo historial
- [x] Navegación con flechas, `j`/`k`, RePág/AvPág, `Ctrl+U`/`Ctrl+D` y `g`/`G`; la salida nueva no mueve la vista y se avisa con «↓ salida nueva»
- [x] `/` y `prefijo /`: búsqueda hacia arriba con `Tab` para texto o expresión regular, `n`/`N`, coincidencias resaltadas y contador; una expresión no válida se dice en la barra
- [x] Con la pantalla alternativa, `prefijo [` avisa «este programa tiene su propio historial»
- [x] `q` o `Esc` vuelven al vivo y liberan la captura
- [x] `v` y `V` seleccionan; `y` o `↵` copian uniendo las líneas envueltas y sin espacios finales
- [x] Copia al portapapeles con `wl-copy` o `pbcopy` (como en la F2) y, si no hay o MAGI corre por SSH, con OSC 52; más de 1 MiB no se copia y se propone guardar
- [x] `w` guarda la captura con una ruta propuesta (`ficheros.rs`, `create_new`, 600), nunca sobrescribe y anota `historial_guardado` sin el contenido
- [x] Al reconectar se conserva el historial con la línea «── reconectada HH:MM ──», que no llega al remoto

## Portapapeles del remoto
- [x] Ficha: «Permitir que el host copie al portapapeles» (propio del equipo); con el permiso, cada copia del remoto se avisa con el host y el tamaño
- [x] Sin permiso, aviso «<host> quiere copiar al portapapeles · ^] P permitir»; `prefijo P` activa el permiso y entrega la petición si no han pasado 60 s
  - AC: Dado un host sin permiso que copia por OSC 52, cuando se pulsa `prefijo P` a los 20 s, entonces el texto llega al portapapeles y la ficha queda con el permiso

## Enlaces y túneles temporales
- [x] `prefijo o` (y `o` en el modo historial) marca con letras los enlaces visibles (hasta 26, los más cercanos al final)
- [x] Diálogo del enlace con la URL completa y el host resaltado: `↵` abre (solo `http` y `https`), `c` copia y `Esc` sale
- [x] Abrir con `[terminal] abrir_enlaces` (`xdg-open` u `open`), con la URL como único argumento, sin shell y desde el cliente
- [x] Enlace a localhost del host: `t` abre un túnel temporal en `127.0.0.1:<libre>` y la misma URL con ese puerto
  - AC: Dado `http://localhost:8000/informe?x=1` en web-01, cuando se elige `t`, entonces se abre `http://127.0.0.1:<libre>/informe?x=1` y la página llega del host
- [x] Túneles temporales como trabajo del servidor (`TrabajoServidor.tuneles_temporales`, `TunelesTemporales{lista}`): se ven en Túneles como «temporal», se cierran con su pestaña y anotan `tunel_*` con «temporal»

## Paneles
- [x] Cinco disposiciones (completa, lado a lado, apilados, 1 grande + 2 y rejilla) y `prefijo Espacio` para pasar a la siguiente
- [x] `prefijo 1`-`9`, `n` y `p` ponen la pestaña en el panel enfocado; si ya está en otro panel, se intercambian
- [x] `prefijo` + flechas cambia de panel y `prefijo z` amplía el panel enfocado y vuelve
- [x] Cada panel pasa su tamaño por `Geometria` (T46), con una fila de cabecera; con la misma pestaña en otra ventana, manda el mínimo
  - AC: Dada una ventana de 80×24 en rejilla, cuando se mira el tamaño de cada pestaña, entonces es el de su panel sin la cabecera
- [x] Por debajo del mínimo de la disposición, solo el panel enfocado con «disposición en espera: ventana pequeña», y vuelve al agrandar
- [x] Cabecera de cada panel con glifo y nombre; el enfocado, con `▸` y resaltado
- [x] Paleta «abrir en rejilla…»: hasta 4 hosts con casillas o un grupo; con 2, lado a lado; con 3, «1 grande + 2»; con 4, rejilla
- [x] La disposición es de la ventana: no se guarda ni se comparte

## Escritura en varios
- [x] `prefijo E` con dos o más paneles: confirmación si es un solo host sin comprobaciones; si no, deliberación MAGI (`PlanDeliberado::EscrituraVarios`, `Ctrl+K`, forzado con motivo)
- [x] `IniciarEscrituraVarios`: el servidor recomprueba pestañas abiertas, hosts y deliberación válida y sin usar, y solo escribe en lo concedido
  - AC: Dado un cliente que manda `EscribirVarios` con una pestaña no concedida, cuando llega al servidor, entonces se rechaza entero
- [x] Cada pestaña recibe sus bytes codificados con sus modos (teclado de kitty y pegado)
- [x] Aviso rojo fijo «ESCRIBIENDO EN N PANELES» y `»` en cada panel mientras dura
- [x] Se apaga con `prefijo E`, al cambiar de disposición o de pestaña en un panel, si cae una pestaña, al salir de la vista, a los 10 min sin escribir y al cerrar la ventana
- [x] REGISTRO `escritura_varios_activada` y `escritura_varios_terminada`, y `ejecucion_resultado` con la duración y los envíos; nunca lo tecleado
- [x] El prefijo y el ratón no se difunden; el pegado sí, con el aviso de varias líneas por pestaña

## Reconexión automática
- [x] Con `reconectar`, una caída de red pasa a `reconectando` y el servidor reintenta sin diálogos (llavero, huellas aceptadas, plazo de 20 s)
  - AC: Dado un host que corta la conexión y vuelve a los 20 s, cuando llega el siguiente intento, entonces la pestaña se reabre sola y REGISTRO anota `sesion_reconectada` «automática»
- [x] No se reintenta tras `exit`, credenciales rechazadas, huella nueva o cambiada, frase pendiente, cierre del usuario ni host borrado; se dice por qué y queda `prefijo r`
- [x] Pasado `reconexion_max_min`: `caida` con «reconexión agotada» y REGISTRO `reconexion_agotada`
- [x] La vuelta de una suspensión adelanta el siguiente intento
- [x] Cuenta atrás en la pestaña y en la barra; `prefijo r` reintenta ya (con diálogos) y `prefijo x` cierra
- [x] Varias pestañas del mismo host comparten la conexión del pool al reconectar (T42)

## tmux
- [x] Ficha: «Sesión persistente con tmux» (propio del equipo), con el aviso de tmux dentro de tmux
- [x] Pestaña nueva con la opción: `listar`, nombre libre `magi-<equipo>-<n>` y `abrir`; sin tmux, shell normal con aviso
- [x] La reconexión vuelve a la misma sesión de tmux con `comprobar` y `abrir`
  - AC: Dado `sleep 600` en una pestaña con tmux, cuando la red cae y vuelve, entonces la pestaña reconectada enseña el `sleep` en marcha
- [x] Cerrar una pestaña con tmux pregunta: `↵` la deja viva en el host y `T` la termina también
- [x] Sesiones: `t` lista las sesiones de tmux del host (nombre, enganchada, ventanas, creada); `↵` abre una pestaña enganchada y `T` la termina con confirmación y REGISTRO `tmux_terminada`
- [x] El snippet al conectar y el comando inicial solo se escriben en una sesión de tmux nueva

## Pestañas, avisos y título
- [x] Campana, OSC 9 y OSC 777 en una pestaña que no se ve: `!` y el texto en la barra; se limpia al verla; nunca avisos del sistema
- [x] Título del remoto (OSC 0 y 2) en la lista de Sesiones y en la barra de la pestaña
- [x] `prefijo ,` renombra la pestaña (1-32 caracteres) para todas las ventanas
- [x] `prefijo <` y `>` mueven la pestaña; la numeración de `prefijo 1`-`9` sigue el orden nuevo
- [x] Marcas `↻` (reconectando), `!` (aviso), `▸` (enfocado) y `»` (escritura en varios) con su texto y su equivalente ASCII

## Configuración, ficha y ayuda
- [x] `[terminal]`: `historial_lineas`, `historial_mib`, `raton`, `teclado_mejorado`, `avisar_pegado`, `abrir_enlaces` y `reconexion_max_min`, con sus rangos y aviso al arrancar
- [x] Ficha de host: bloque TERMINAL «solo en este equipo» con los tres campos, solo en hosts `ssh`
- [x] Ayuda `?`, barra de la vista Sesión, Sesiones y paleta con las teclas nuevas

## Interfaz: instantáneas
- [x] Instantáneas a 40×12, 80×24 y 200×60 (T45) de las cinco disposiciones, el modo historial con búsqueda y selección, los enlaces y su diálogo, la escritura en varios, una pestaña reconectando, Sesiones con tmux, el aviso de pegado y la ficha con el bloque TERMINAL

## API: protocolo cliente ⇄ servidor (v9)
- [x] `CapturarHistorial` → `HistorialLote`
- [x] `Foco`
- [x] `IniciarEscrituraVarios` → `EscrituraVariosIniciada`, `EscribirVarios` y `TerminarEscrituraVarios` / `EscrituraVariosTerminada`
- [x] `AbrirTunelTemporal` → `TunelTemporal`, `CerrarTunelTemporal` y `TunelesTemporales{lista}`
- [x] `ListarTmux` → `SesionesTmux` y `TerminarTmux` → `TmuxTerminada`
- [x] `Portapapeles`, `PortapapelesPedido` y `PortapapelesPermitido`
- [x] `RenombrarSesion` y `MoverSesion`
- [x] `Adjuntar` con `colores`, `AbrirSesion` con `tmux` y `Sesiones{lista}` con `reconexion`, `titulo`, `aviso` y `tmux`
- [x] `VERSION_PROTOCOLO = 9`, `peticion_id` del terminal desde 2^49 y `TrabajoServidor.tuneles_temporales`

## Calidad, pruebas y publicación
- [x] Pruebas del núcleo: `terminal_emulador`, `terminal_teclas`, `terminal_raton`, `terminal_pegado`, `terminal_historial` (también los bytes por celda de §7.1), `terminal_enlaces`, `tmux_script` y `reconexion`
- [x] Programa de prueba `tui_prueba`, que hace lo que Claude Code (consultas, 2004, teclado de kitty, 2026, ratón, OSC 52, campana y título), y prueba de extremo a extremo `terminal_e2e` con el servidor SSH de prueba, sin red externa
  - AC: Dado `tui_prueba` en el servidor SSH de prueba, cuando arranca, entonces recibe las respuestas a sus consultas, y un pegado de 5 líneas y un Mayús+Intro le llegan con los bytes esperados
- [x] `terminal_servidor` (reconexión con un servidor que corta la conexión, tmux falso, escritura en varios, capturas, portapapeles y túnel temporal) y `terminal_ui` (paneles, modos y avisos) con `App::de_prueba`
- [ ] El móvil no cambia: sus pruebas JVM, de extremo a extremo e instrumentadas siguen en verde sin tocarlas
- [x] Dependencia `regex` con `cargo deny` en verde; clippy, fmt y `shellcheck` del script nuevo
- [x] Versión 1.4.0, CHANGELOG y sección «Terminal» del README con Claude Code (pegado, Mayús+Intro, rueda y permiso del portapapeles)
- [x] Revisión adversarial de cierre (T31, T54), servidor y cliente por separado, con su informe en `docs/fase15/magi-fase15-revision-adversarial.md`

---

**Progreso Fase 15:** 95 / 96 funcionalidades

**Total MAGI (Fases 1-15):** 1246 / 1251 funcionalidades
