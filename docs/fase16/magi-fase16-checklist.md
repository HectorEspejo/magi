# MAGI - Checklist Fase 16: Sesiones Persistentes

## Núcleo: modelo, validación y almacén
- [x] Migración 11: `HOSTS.sesiones_persistentes` (0, viaja) y tabla SESIONES_PERSISTENTES (`uuid`, `version`, `vector`, `host_id` con CASCADE, `nombre` único por host, `carpeta`, `comando`, `comando_reanudar`, `nota`, `origen`, `creada_por`, `creada_en`, `actualizado_en`)
  - AC: Dado una `magi.db` en `user_version` 10, cuando la abre la 1.5.0, entonces queda en 11 con la tabla y la columna nuevas; una base en 12 no se abre
- [x] Validadores del núcleo: nombre `^[A-Za-z0-9_-]{1,64}$`, carpeta absoluta o `~/…` (hasta 1024 bytes, sin saltos de línea), comando y comando de reanudación (hasta 4096 bytes), nota (hasta 500 caracteres) y solo hosts `ssh`
- [x] Al abrir tras migrar, cada host con `tmux_persistente = 1` pasa a `sesiones_persistentes = 1` por el camino que sube la versión, `tmux_persistente` vuelve a 0 y se avisa una vez
  - AC: Dado un host con `tmux_persistente = 1`, cuando se abre la 1.5.0, entonces tiene `sesiones_persistentes = 1` con versión nueva y `tmux_persistente = 0`; al abrir otra vez no cambia nada
- [x] Nombre automático en un host marcado: `<etiqueta del equipo>-<n>` con el primer `n` libre entre las sesiones del host y sus fichas; el diálogo propone el nombre de la carpeta o `sesion-<n>`
- [x] Estado derivado de cada ficha (`viva_libre`, `viva_enganchada`, `terminada`, `sin_comprobar`, `inalcanzable`, `sin_tmux`), atención, título, actividad comparada con la hora del host y «antigua» (más de `antigua_dias` sin actividad)
  - AC: Dado una ficha cuya sesión no sale en `estado` de un host que responde, cuando se deriva su estado, entonces es `terminada`; si el host no responde, `inalcanzable`, nunca `terminada`
- [x] Lectura acotada de lo que devuelve el script: campos separados por 0x1F, títulos saneados de hasta 120 caracteres, 500 sesiones por host como mucho, vista previa de hasta 64 KiB e historial de hasta 8 MiB y 64 tramos por fila; nunca pánico

## Sincronización
- [x] Elemento `sesion_persistente` en `sincronizacion/elementos.rs` y campo `sesiones_persistentes` del host; `formato` 4 solo si hay alguna ficha o algún host marcado
  - AC: Dado un inventario sin fichas ni hosts marcados, cuando se exporta, entonces el fichero no pasa al formato 4; con una ficha, sí
- [x] Dos fichas del mismo host con el mismo nombre se unen: queda la de `uuid` menor, con lo que le falte de la otra; la otra se borra con lápida y queda un conflicto `nombre`
  - AC: Dado el mismo nombre en el mismo host creado en dos equipos sin conexión, cuando los dos sincronizan, entonces ambos acaban con una sola ficha, la misma
- [x] Borrar una ficha deja lápida y nunca termina la sesión de tmux del host
- [x] `docs/formato-sincronizacion.md` al día: `formato` 4, elemento `sesion_persistente`, campo del host y regla de unión
- [x] La app Android lee el `formato` 4 (guarda las fichas sin enseñarlas y no usa la marca del host), con su fixture y una línea en `android/CHANGELOG.md`
  - AC: Dado un fichero de sincronización en formato 4, cuando el móvil pasa en modo lector, entonces importa el resto del inventario y no se queda en «formato nuevo»

## tmux en el host
- [x] `terminal_tmux.sh` usa el servidor propio `tmux -L magi -f /dev/null` con la configuración de MAGI aplicada con `set` al crear y al enganchar: sin barra, sin prefijo, ratón, teclas extendidas (3.2+, `csi-u` en 3.5+), portapapeles, foco, color completo, título hacia fuera, campana y `history-limit` igual a `[terminal] historial_lineas`
  - AC: Dado un host con su propio `~/.tmux.conf`, cuando MAGI crea una sesión, entonces esa configuración no se carga y las sesiones del tmux habitual siguen igual
- [x] Subórdenes nuevas `crear`, `enganchar`, `estado`, `vista`, `historial` y `renombrar`; `comprobar` y `terminar` ganan el servidor; `abrir` y `listar` desaparecen
- [x] `crear` comprueba la carpeta y crea la sesión sin engancharse; con comando, lo ejecuta con el shell de acceso del usuario y, al acabar, escribe «[MAGI] <comando> terminó con código N» y deja un shell en la carpeta
  - AC: Dado `comando = "false"`, cuando la sesión lo ejecuta, entonces se ve «terminó con código 1» y queda un shell en la carpeta
- [x] `enganchar` suelta a los demás salvo al compartir, pone `@magi_equipo` y sale con 100 si a este cliente lo suelta otro, con 0 si la sesión terminó y con 75 si no existía
- [x] Toda orden con `-t` apunta a `=<nombre>` y la existencia se comprueba con la lista exacta, también en `terminar` y `renombrar` (T126, R146)
  - AC: Dado `magi-a-10` viva y ninguna `magi-a-1`, cuando se pide terminar `magi-a-1`, entonces da `no_existe` y `magi-a-10` sigue viva
- [x] tmux 3.0 o posterior para el servidor propio: con uno anterior, «tmux demasiado antiguo (3.0 o posterior)» y una pestaña normal; sin tmux, «instala tmux en el host»
- [x] `estado` lista las sesiones de los dos servidores, el propio y el habitual, con servidor, nombre, clientes, equipo, actividad, creada, campana, ventanas y título, más la hora del host

## Teclado
- [ ] MAGI entiende `modifyOtherKeys` (`CSI > 4 ; n m`), que es como lo pide tmux, y codifica con él Mayús+Intro, Ctrl+Intro y el resto de combinaciones con modificadores; se apaga con `CSI > 4 m` o al cerrar la pestaña
  - AC: Dado Claude Code en una sesión persistente con tmux 3.5, cuando se pulsa Mayús+Intro, entonces inserta un salto de línea sin enviar

## Servidor
- [x] Pestaña persistente: `AbrirSesion` con `tmux` crea la sesión si hace falta (`comprobar` y `crear`) y la abre con `enganchar` en el canal con PTY; el snippet al conectar y el comando inicial (F6) solo se escriben en una sesión nueva sin comando propio
- [x] Pestaña soltada: si el script sale con 100, la pestaña pasa a `soltada` con el equipo que la tiene ahora (leído con `estado`) y conserva la última pantalla en vez de cerrarse
- [x] La reconexión automática (F15) de una pestaña persistente vuelve soltando a los demás solo si los clientes enganchados son de este equipo; si la tiene otro equipo, la pestaña queda soltada
  - AC: Dado una pestaña de sobremesa caída mientras el portátil usa la sesión, cuando sobremesa reconecta, entonces no se la quita al portátil y queda «soltada»
- [x] Vigilante: mientras haya alguna ventana conectada, cada `vigilar_seg` comprueba con `estado` los hosts con fichas o marcados, por el pool o con una conexión efímera sin diálogos, con 8 hosts a la vez como mucho y 10 s de plazo por host
  - AC: Dado un host que pide frase, cuando el vigilante lo comprueba, entonces no abre ningún diálogo y sus fichas quedan «sin comprobar: conéctate a mano»
- [x] `SesionesPersistentes{lista}` a todas las ventanas en cada cambio, con el estado visto por ficha y, por host comprobado, sus sesiones sin ficha y las del tmux habitual
- [ ] `ConsultarSesionesPersistentes` comprueba enseguida (al abrir la lista, con `r` y cada `refresco_seg` con la lista a la vista), agrupa las peticiones seguidas y no repite un host comprobado hace menos de 5 s
- [x] `CapturarHistorial` en una pestaña persistente lee el historial de tmux con `historial` y lo manda en tramos como en la F15
- [x] `PedirVistaPrevia` devuelve las últimas `vista_previa_lineas` en texto plano solo a la ventana que la pide, sin guardarlas
- [ ] `RenombrarTmux` renombra en el host y solo entonces el cliente cambia la ficha; las pestañas enganchadas siguen enganchadas
- [x] El servidor lee y valida las filas de SESIONES_PERSISTENTES y HOSTS antes de usarlas y nunca escribe en ellas
- [x] Ni la salida de las sesiones, ni sus títulos, ni las vistas previas, ni el historial de tmux van al log ni a REGISTRO

## Cliente: Persistentes en la vista Sesiones
- [ ] Bloque «Persistentes» arriba en Sesiones (`prefijo l`): fichas de todos los hosts con marca, estado, nombre, host, quién la tiene, actividad, `!` de atención, título y «antigua»; `Tab` pasa a «Pestañas» y vuelve
- [ ] Bajo cada host comprobado, sus sesiones sin ficha del tmux de MAGI («sin ficha», `a` las añade con `origen = descubierta`) y las del tmux habitual («de tu tmux»: se enganchan y se terminan, sin ficha)
- [ ] Teclas del bloque: `↵` engancharse (en una terminada, reabrirla), `c` compartir, `v` vista previa, `n` nueva, `e` editar ficha, `,` renombrar, `T` terminar, `x` borrar ficha, `C` borrar las terminadas, `r` refrescar y `/` filtrar
- [ ] Diálogo «Nueva sesión persistente» (host `ssh`, nombre, carpeta, comando, comando de reanudación y nota), también desde Hosts, Flota, la paleta y `prefijo N`
  - AC: Dado un nombre que ya existe en ese host, cuando se acepta, entonces avisa y ofrece engancharse a la existente o cambiar el nombre
- [ ] El cliente guarda la ficha y anota `sesion_persistente_creada` solo cuando la sesión ya existe en el host; si algo falla antes, no queda ficha
- [ ] Reabrir una terminada la crea con su `comando_reanudar` (o su `comando`) en su carpeta y anota `sesion_persistente_reabierta`
- [ ] Terminar (`T`) con una confirmación que nombra la sesión y el host y dice que se cerrará lo que corre dentro; anota `tmux_terminada` y la ficha queda «terminada»
- [ ] Borrar una ficha (`x`) con confirmación; si la sesión vive, avisa de que seguirá en el host sin ficha; anota `sesion_persistente_borrada`
- [ ] Ficha de host: «Todas las sesiones persistentes» al crear y al editar, en el bloque SESIONES, que viaja, con el aviso de tmux dentro de tmux; sale del bloque TERMINAL de la F15
- [ ] En un host marcado, toda pestaña nueva es persistente, con nombre automático y ficha (`origen = host`); sin tmux, pestaña normal con aviso; cerrarla (`prefijo x`) pregunta como en la F15
- [ ] Marca `◆` (en ASCII, `P`) en las pestañas y fichas persistentes y barra de la pestaña con «persistente · <nombre> · tmux <versión>»; soltada: «Ahora la usa <equipo> · ↵ volver · c compartir · x cerrar»
- [ ] Aviso en la barra global desde cualquier vista cuando una sesión pasa a pedir atención («◆ agente-api pide atención · web-01», o «◆ N sesiones piden atención»); se quita cuando deja de pedirla
  - AC: Dado Claude Code esperando permiso con la campana en una sesión libre, cuando el vigilante la comprueba, entonces el aviso sale en la barra de todas las ventanas antes de `vigilar_seg`
- [ ] Vista previa (`v`): las últimas líneas con la hora de lectura y «↵ engancharse · r refrescar · Esc volver»
- [ ] `prefijo [` en una pestaña persistente abre el modo historial con el historial de tmux: búsqueda, selección, copia y guardado como en la F15
- [ ] Errores con instrucción: sin tmux, tmux antiguo, carpeta que no existe, host sin conexión o que pide un diálogo, y nombre ocupado
- [ ] REGISTRO: `sesion_persistente_creada`, `sesion_persistente_reabierta`, `sesion_persistente_enganchada` (solo a petición del usuario, nunca en las reconexiones) y `sesion_persistente_borrada`, con host y nombre y sin comandos, títulos ni contenido
- [ ] Ayuda (`?`) y paleta con las teclas y órdenes nuevas («nueva sesión persistente», «engancharse a <nombre>»), y la recomendación de usar la campana como canal de avisos de Claude Code

## CLI y configuración
- [ ] `magi sesion <nombre> [--host <host>]` abre MAGI enganchado a esa sesión; si el nombre está en varios hosts, pide `--host` y dice cuáles
- [ ] `magi sesiones [--json]` lista las fichas con su estado, que pregunta al servidor (lo arranca si hace falta)
- [x] `[persistentes]` en `config.toml`: `vigilar_seg` (60; 30-600), `refresco_seg` (30; 10-300), `antigua_dias` (7; 1-365) y `vista_previa_lineas` (20; 5-100), acotadas con aviso al arrancar
- [ ] `assets/` regenerado (página man y autocompletado) con las órdenes nuevas

## Interfaz: instantáneas
- [ ] Instantáneas a 40×12, 80×24 y 200×60 (T45) de Sesiones con el bloque Persistentes (atención, enganchada, terminada, sin ficha y tu tmux), el diálogo de nueva sesión, la vista previa, una pestaña soltada, el aviso de la barra y la ficha con el bloque SESIONES

## API: protocolo cliente ⇄ servidor (v10)
- [ ] `VERSION_PROTOCOLO = 10` y los `peticion_id` de las sesiones persistentes en el rango del terminal (2^49)
- [x] `AbrirSesion` con `tmux{servidor, nombre, crear, compartir}`
- [x] `ListarTmux` → `SesionesTmux` con `servidor`, `clientes`, `equipo`, `actividad`, `campana` y `titulo`, y `TerminarTmux` → `TmuxTerminada` con `servidor`
- [x] `ConsultarSesionesPersistentes` y `SesionesPersistentes{lista}`
- [x] `PedirVistaPrevia` → `VistaPrevia`
- [x] `RenombrarTmux` → `TmuxRenombrada`
- [x] `CapturarHistorial` → `HistorialLote` en pestañas persistentes y `Sesiones{lista}` con `InfoSesion.tmux` (`persistente` y `soltada`)

## Calidad, pruebas y publicación
- [x] Pruebas del núcleo: `sesiones_persistentes` (validadores, nombres automáticos, estado y lectura acotada) y `tmux_script` (subórdenes nuevas con el tmux falso)
- [ ] `tmux_real`: con tmux de verdad (en CI, instalado con apt; se omite si no lo hay), crear, enganchar, soltar desde otro cliente (sale con 100), historial, renombrar y terminar con `magi-a-1` y `magi-a-10`
- [x] `sincronizacion_sesiones`: `formato` 4 solo cuando toca, unión de fichas con el mismo nombre, lápidas y la marca del host
- [ ] `sesiones_persistentes_servidor` (vigilante, soltada, reconexión sin quitarla a otro equipo, captura y vista previa) y `sesiones_persistentes_ui` con `App::de_prueba`
- [ ] `sesiones_persistentes_e2e` con el servidor SSH de prueba y tmux real: crear, cerrar la ventana, volver desde otra, soltar, atención por la campana y terminar
- [ ] `tui_prueba` ampliado: Mayús+Intro con `modifyOtherKeys` dentro del tmux de MAGI
- [ ] Revisión adversarial de cierre (T31, T54), servidor y cliente por separado, con su informe en `docs/fase16/magi-fase16-revision-adversarial.md`
- [ ] Versión 1.5.0: `CHANGELOG.md` («Sin publicar»), `README.md` (sección «Sesiones persistentes»), `FECHA_DE_VERSION` y guion de validación manual con Claude Code real en el informe de implementación

---

**Progreso Fase 16:** 36 / 67 funcionalidades

**Total MAGI (Fases 1-16):** 1282 / 1318 funcionalidades
