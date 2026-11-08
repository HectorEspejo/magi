# MAGI - Checklist Fase 13: Servicios, Contenedores y Logs

## Núcleo y script
- [ ] `crates/magi-nucleo/src/servicios/` con los tipos (objetivo, acción, listado, privilegio, resultado, orden de pestaña), la lectura de las salidas, las acciones permitidas, los validadores y los límites, sin depender de la TUI ni del servidor
- [ ] Script embebido `crates/magi-nucleo/scripts/servicios.sh` (`include_str!`) con las subórdenes `privilegio`, `sudo`, `listar`, `consumo`, `detalle`, `accion` y `logs`, que vuelve a validar sus argumentos
  - AC: Dado un objetivo `nginx.service; rm -rf /`, cuando se pide una acción, entonces Rust lo rechaza antes de ejecutar y el script, si le llegara, sale con `ERROR objetivo` y código 64
- [ ] Nombres validados en Rust: unidades `[A-Za-z0-9@._-]+` acabadas en `.service` o `.timer`, contenedores por id o nombre, proyectos `[a-z0-9][a-z0-9_-]*` y motor `docker` o `podman`; las unidades con otros caracteres se ven, pero sin acciones
- [ ] Lectura con `systemctl show` (clave=valor) y con los JSON de Docker, Podman y `journalctl -o json`, probada con salidas grabadas de Debian 12, Ubuntu 24.04, Rocky 9 y Arch, Docker 27 y Podman 5
- [ ] Límites de la entrada no fiable (2 MiB por listado, 2 000 unidades, 500 contenedores, 100 proyectos, texto de unidad o de `inspect` hasta 256 KiB, líneas de log hasta 16 KiB): lo que se pasa se corta con aviso y nunca hay pánico
  - AC: Dado un host que devuelve 5 000 unidades, cuando se lista, entonces se ven 2 000 y la vista dice «lista incompleta»
- [ ] Privilegio detectado por host con `LC_ALL=C`: `root`, `sudo_sin_clave`, `sudo_con_clave` o `sin_sudo`
- [ ] Las órdenes privilegiadas usan sudo orden a orden (nunca `sudo sh`), con `timeout` por fuera de sudo
  - AC: Dado un sudoers que solo permite `systemctl` y `journalctl`, cuando se reinicia un servicio, entonces funciona y el registro del host enseña `systemctl restart <unidad>`

## Vista Servicios
- [ ] Vista nueva en `F9`, y `u` desde Flota, Hosts y la ficha (fuera de un campo) la abre con ese host elegido
- [ ] Columna de hosts solo de tipo `ssh`, con el estado de su último sondeo y `⚠N` de fallos, y «Toda la flota» arriba
- [ ] Secciones Servicios, Timers y Contenedores con su recuento, que se eligen con `1`, `2` y `3`; `Tab` cambia entre hosts y lista
- [ ] Servicios con unidad, estado (glifo y texto), arranque y desde; la descripción aparece a partir de 120 columnas
- [ ] Los vigilados (`★`) y los fallidos van arriba, los fallidos primero, y el resto debajo por orden alfabético, separados por una línea
  - AC: Dado `copia.service` fallido y `nginx.service` vigilado y activo, cuando se abre la sección, entonces arriba están `copia.service` y después `nginx.service`, y debajo el resto
- [ ] Se listan los servicios cargados y los instalados sin cargar; las plantillas (`@.service`) no, sus instancias sí
- [ ] Timers con próxima y última ejecución, el servicio que activan y su estado
- [ ] Contenedores agrupados por proyecto Compose (y «sin proyecto») con estado, código de salida, salud, imagen y puertos; la cabecera dice el motor y su versión
- [ ] CPU y memoria de los contenedores en marcha, pedidas aparte y solo con la sección Contenedores a la vista, sin frenar el listado
- [ ] `/` filtra por nombre en la sección visible
- [ ] Detalle (`↵`) de un servicio: estado y subestado desde cuándo, arranque, PID principal, memoria, fichero de la unidad, si admite recargar, dependencias («necesita» y «la necesitan») y el texto de `systemctl cat`
- [ ] Detalle de un contenedor (id, imagen, creado, estado y código, reinicios, salud, puertos, montajes, redes, proyecto y orden, con el texto de `inspect`) y de un timer (próxima y última ejecución, servicio que activa y unidad)
- [ ] Host sin systemd: «sin systemd», con solo los contenedores; systemd anterior a 230: «systemd demasiado antiguo»
- [ ] Sin Docker ni Podman, la sección Contenedores lo dice; con los dos, se usa el primero de `[servicios] contenedores` y la cabecera dice cuál
- [ ] Sin permiso para el diario del sistema o para Docker: aviso con la instrucción y `!` para dar la contraseña de sudo
- [ ] Refresco al entrar, con `r` y cada `refresco_seg` (10) mientras la vista está abierta, sin solapar refrescos y sin diálogos; al salir no queda nada en marcha
  - AC: Dado la vista abierta en web-01, cuando se cambia a Sesión, entonces no se piden más listados hasta volver a `F9`
- [ ] Si la conexión se cae, el último listado se queda con su hora y «desconectado · r reconectar»
- [ ] Tamaño mínimo y modo estrecho (un panel con `Tab`) declarados en `ui/disposicion.rs`; instantáneas de la vista, el detalle, el panel de logs y los diálogos a 40×12, 80×24 y 200×60 (T45)

## Toda la flota y Flota
- [ ] El sondeo (F2) cuenta los servicios fallidos y los contenedores caídos (salieron con código distinto de 0, se reinician una y otra vez o están `unhealthy`), con hasta 20 nombres y sin sudo
- [ ] Migración 9: SONDEOS gana `fallos_json`, nulo en los sondeos anteriores; `user_version` 9
- [ ] Estos fallos no cambian el estado del host: solo lo cambian los servicios vigilados, como en la F2
- [ ] Flota marca `⚠N` en la fila del host y enseña el bloque FALLOS en su detalle
- [ ] «Toda la flota» junta los fallos de todos los hosts, del último sondeo de cada uno y con su hora, sin abrir conexiones; `↵` va al host con el elemento elegido y `r` sondea todos
  - AC: Dado un fallo en web-01 y dos en db-01, cuando se abre «Toda la flota», entonces salen los tres con su host, y `↵` en uno de db-01 abre db-01 con ese elemento seleccionado

## Acciones sobre servicios y timers
- [ ] Arrancar (`A`), parar (`P`), reiniciar (`R`), recargar (`G`) y activar o desactivar el arranque (`H`), ofrecidas solo cuando valen en el estado de la unidad
  - AC: Dado un servicio sin `CanReload`, cuando se pulsa `G`, entonces no recarga y avisa «nginx.service no admite recargar»
- [ ] Tras una acción, MAGI espera al estado final hasta `espera_seg` (30) y vuelve a mirar a los 2 s; un `oneshot` que termina bien cuenta como hecho
- [ ] Si la unidad no vuelve a `active`, el resultado es «fallido» con las últimas 20 líneas del diario; si se acaba el plazo, «sin confirmar», y la orden sigue en el host
  - AC: Dado un servicio que muere al arrancar, cuando se reinicia, entonces el diálogo de resultado dice «fallido» y enseña sus últimas líneas
- [ ] En un timer, `A` lanza ahora el servicio que activa, con confirmación
- [ ] Una sola acción a la vez por objetivo: otra sobre el mismo se rechaza con «hay otra acción en curso»
- [ ] Unidades críticas (`[servicios] criticas`: sshd, red, cortafuegos, Tailscale, Docker…) con el aviso en rojo «puede cortar tu conexión con <host>» y los hosts que van a través de él
- [ ] Diálogo de resultado (hecho, fallido, sin confirmar, rechazada por la configuración o error) y refresco inmediato después

## Contenedores y Compose
- [ ] Arrancar, parar y reiniciar un contenedor según su estado; tras arrancar o reiniciar, espera a `running` y, si tiene healthcheck, a que deje de estar `starting`
- [ ] Proyecto Compose: arrancar, parar y reiniciar el proyecto entero con `-p` y los ficheros de sus etiquetas; sin ficheros conocidos, no se ofrecen y se dice por qué
- [ ] `S` abre en Sesión una pestaña con una shell en el contenedor (`exec -it`, con bash si lo hay y si no sh), que se cierra al salir
- [ ] Con Podman sin root se ven los contenedores del usuario SSH, y con Podman nunca se usa sudo

## Deliberación y confirmación
- [ ] Parar, reiniciar y desactivar un servicio, y parar o reiniciar un contenedor o un proyecto, pasan por la deliberación MAGI con las comprobaciones activas del host (`PlanDeliberado::Servicio`)
- [ ] MELCHIOR-1 no cuenta el estado del propio objetivo al mirar la salud del host
  - AC: Dado `app.service` vigilado y caído (el host está en CARGA solo por eso), cuando se reinicia, entonces MELCHIOR-1 aprueba
- [ ] Arrancar, recargar, activar el arranque y lanzar un timer piden una confirmación simple (T6)
- [ ] El servidor recomprueba qué exige deliberación y valida la deliberación (que exista, sea de esa acción y no esté usada, T62); DELIBERACIONES guarda la acción y el servidor, su resultado
- [ ] Sin comprobaciones activas, la deliberación dice «sin comprobaciones configuradas» y sigue pidiendo `Ctrl+K`, como en la F6

## sudo
- [ ] Si una acción necesita sudo con contraseña y no está en memoria, el cliente la pide en un diálogo y la comprueba con `ComprobarSudo` antes de confirmar o deliberar, con 3 intentos
- [ ] La contraseña viaja en `Zeroizing`, llega al host por la entrada estándar (nunca en la línea de órdenes) y nunca va a disco, al llavero, al log ni a REGISTRO
- [ ] Se recuerda en memoria del cliente, por host, hasta `sudo_memoria_min` (5) minutos sin uso o hasta salir de la vista
  - AC: Dado una contraseña dada hace 6 min y sin usar desde entonces, cuando se reinicia otro servicio, entonces MAGI la pide otra vez
- [ ] `!` pide la contraseña de sudo para el host elegido, para ver lo que exige permisos
- [ ] Un usuario sin sudo ve «usuario no puede usar sudo en <host>» y no se le ofrecen las acciones que lo necesitan

## Comprobación de configuración
- [ ] Antes de recargar o reiniciar Nginx, Caddy, Apache o HAProxy, y de reiniciar un proyecto Compose, se valida la configuración en el mismo canal, con los mismos privilegios y justo antes de la acción
  - AC: Dado un `nginx.conf` roto, cuando se recarga Nginx, entonces no se recarga, el resultado es «rechazada por la configuración» y se ve la salida de `nginx -t`
- [ ] Caddy y HAProxy sacan la ruta de su configuración del `ExecStart` de la unidad o, si no está, usan la de siempre
- [ ] Comprobación propia por servicio o proyecto (COMPROBACIONES_SERVICIO, migración 9), que sustituye a la conocida y se ejecuta con `sh -c` y los privilegios de la acción
- [ ] Las comprobaciones propias se crean, se editan y se borran en la ficha del host y con `c` sobre el servicio o el proyecto en `F9`; borrar pide confirmación
- [ ] Un código distinto de 0 (también 127, no encontrada) o 20 s sin terminar paran la acción

## Logs en vivo
- [ ] `l` abre el panel de logs de un servicio, del servicio de un timer o de un contenedor, con las últimas `lineas_iniciales` (200) y en vivo
- [ ] El servidor manda lotes cada 100 ms (hasta 500 líneas) y, si el cliente no da abasto (cola de más de 4 MiB), descarta las más viejas y las cuenta
- [ ] Anillo de `lineas_log` (10 000) en el cliente; el panel dice cuántas líneas hay y marca dónde se omitieron
- [ ] Filtro por texto (`/`) y por nivel (`n`: todo, info, aviso, error) en el diario; en contenedores, todo o solo stderr
  - AC: Dado el panel con «502» en el filtro y el nivel en aviso, cuando llega una línea de info con 502, entonces no se ve
- [ ] `Espacio` pausa y sigue sin cortar la llegada de líneas; desplazarse hacia arriba pausa, y `Fin` vuelve al final
- [ ] `t` abre en una pestaña el seguimiento completo (`journalctl -fu` o `docker logs -f`)
- [ ] Al cerrar el panel, cambiar de host o salir de la vista, `PararLog` cierra el canal y el script corta el seguimiento en el host
  - AC: Dado un panel de logs abierto, cuando se cierra, entonces en el host ya no corre el `journalctl -f` de MAGI
- [ ] Si el proceso remoto termina, se cae la conexión o falta permiso, el panel lo dice y conserva las líneas

## Servidor y protocolo
- [ ] Todo lo remoto lo hace el servidor sobre el pool (`conexion_para_canal`, `UsoCanal::Servicios`) y suelta la conexión por identidad (T34)
- [ ] Plazos: 10 s por listado, consumo o detalle; 20 s por comprobación; `espera_seg` + 30 s por acción; los logs siguen mientras dura la suscripción
- [ ] Las acciones en curso se custodian: siguen si se cierra la ventana, salen en `AccionesServicio{lista}` y cuentan en `TrabajoServidor.acciones_servicio` (T67)
- [ ] Pestañas con orden: `AbrirSesion` gana `orden` (`ShellContenedor`, `SeguirDiario`, `SeguirContenedor`) y el servidor arma la orden, sin recibir nunca texto libre
- [ ] `peticion_id` de servicios en su propio rango, desde 2^45
- [ ] `VERSION_PROTOCOLO = 8`; `Hola` y `VersionIncompatible` sin cambios (T66)
- [ ] REGISTRO `servicio_accion` con objetivo, acción, resultado, duración y deliberación, tras la `Barrera` (T47); nunca la salida de las órdenes
- [ ] `config.toml` `[servicios]`: `refresco_seg`, `espera_seg`, `lineas_log`, `lineas_iniciales`, `contenedores`, `criticas` y `sudo_memoria_min`, validados con error claro

## Sincronización y app Android
- [ ] COMPROBACIONES_SERVICIO viaja como elemento propio (`comprobacion_servicio`) con `uuid`, `version` y `vector`, y borrar una deja lápida
- [ ] El fichero sube a `formato` 3 solo si hay alguna comprobación propia; si no, sigue en 2 o en 1, como en la F12
  - AC: Dado un inventario sin comprobaciones propias, cuando se exporta, entonces el formato es el de antes y un equipo con MAGI 1.2 lo importa
- [ ] Al guardar la primera comprobación propia, MAGI avisa de que los equipos con MAGI 1.2 dejarán de leer este equipo
- [ ] MAGI 1.3 lee los formatos 1, 2 y 3, y un equipo con 1.2 ve el 3 como `formato_nuevo`
- [ ] `docs/formato-sincronizacion.md` documenta el `formato` 3 y el elemento nuevo
- [ ] La app Android lee el `formato` 3 y guarda las comprobaciones en su copia sin usarlas; `android/CHANGELOG.md` lo recoge en «Sin publicar»

## API: protocolo
- [ ] `ListarServicios` → `Servicios`
- [ ] `PedirConsumo` → `Consumo`
- [ ] `DetalleServicio` → `Detalle`
- [ ] `ComprobarSudo` → `SudoComprobado`
- [ ] `AccionServicio` → `AccionServicioHecha`
- [ ] `AccionesServicio{lista}`
- [ ] `SeguirLog`, `LineasLog`, `PararLog` y `FinLog`
- [ ] `AbrirSesion` con `orden`

## Calidad, pruebas y publicación
- [ ] Pruebas del núcleo con salidas grabadas: unidades, timers, contenedores, proyectos, detalle y diario; límites y nombres inválidos
- [ ] Pruebas del script con el servidor SSH de prueba y `systemctl`, `journalctl`, `docker`, `podman`, `sudo` y `nginx` falsos: argumentos, privilegios, contraseña por la entrada estándar, comprobación y espera
- [ ] Pruebas del servidor: refrescos sin solapes, acciones custodiadas, deliberación exigida, sudo, plazos, y logs con contrapresión y parada
- [ ] Prueba de extremo a extremo sin red externa: vista → servidor → host de prueba, con un reinicio que falla y sus últimas líneas, y un panel de logs que al cerrarse para el seguimiento
- [ ] Pruebas de sincronización: formatos 1, 2 y 3, `formato_nuevo` y la comprobación de un host que ya no existe
- [ ] `shellcheck` en verde para `servicios.sh` y para el `sondeo.sh` ampliado
- [ ] Versión 1.3.0: `Cargo.toml`, fecha de la página man, `assets/` regenerados y `CHANGELOG.md`
- [ ] README con la sección «Servicios, contenedores y logs»: vista, acciones, deliberación, sudo, comprobaciones y permisos en el host
- [ ] Revisión adversarial de cierre (T31, T54) del servidor de servicios, el script, los logs y el manejo de sudo, anotada en el informe de implementación

---

**Progreso Fase 13:** 0 / 95 funcionalidades

**Total MAGI (Fases 1-13):** 958 / 1057 funcionalidades
