# MAGI - Fase 9b: revisión adversarial de cierre

**Fecha:** 20 de noviembre de 2026 · **Rama:** `fase9b` · **Alcance:** las correcciones de la sincronización (10b), el cierre de la F16 y la publicación (workflows, referencias y datos), sobre el código nuevo (`git diff 490afc6`).

## Método

Seis revisores independientes de solo lectura, uno por zona, con la orden de encontrar fallos reales y no de estilo: pérdida de datos, fugas de recursos, esperas sin plazo, estado leído a destiempo, entradas no fiables, secretos o datos reales, compatibilidad y reglas de `CLAUDE.md`. Después, **un verificador independiente por cada hallazgo medio o alto**, con el encargo de refutarlo (por defecto, «no es real» si no se podía demostrar) y de confirmarlo con una prueba cuando fuera posible. Los hallazgos bajos no pasaron por verificador: los he valorado yo.

| Zona | Qué miró |
|---|---|
| Servidor | `consultar_trabajo` y su plazo; la bienvenida y el vigilante (D368); el script por la entrada estándar y su `#MAGI-FIN`; la guarda de la reconexión (R157, código 101); `creada` y `NoExiste`; los avisos de pestaña |
| Cliente e interfaz | La confirmación de «reconectar» (R164), `creada` en el cliente, el umbral de D368, el diálogo que vuelve con lo escrito, las etiquetas de CONFLICTOS y REGISTRO |
| Núcleo de sincronización | R84 (transacciones), R85 y R97 en el escritor y el lector del móvil, R160, `magi sincronizar` y su estado |
| Scripts remotos | Inyección, listas de caracteres (R159), la guarda y el seguimiento por identidad, portabilidad (dash, mawk), tamaños frente a Dropbear |
| Publicación | Referencias y datos reales, la prueba `repositorio_publico`, workflows y etiquetas, `docs/publicar*.md`, CHANGELOG, `gitleaks`, la demo |
| Fidelidad del manual | Que el manual sea el README anterior sin reescribir y que `estado.md` solo diga lo que consta |

## Resultado

25 hallazgos en bruto: 6 medios (todos reales; el verificador rebajó cuatro a bajos) y 19 bajos. **Ninguno crítico ni alto. Ninguno de pérdida de datos confirmada**: el más cercano (NUC-1) hacía que un borrado aplazado se diera por «recuperado» —es decir, que el host se conservara— y no que se perdiera nada. Los medios, todos con prueba de regresión que falla sin el arreglo:

| Id | Gravedad (verificada) | Dónde | Qué pasaba | Estado |
|---|---|---|---|---|
| NUC-1 | media | `aplicacion.rs`, paso 9 | Con un plan parcial (la pasada salta los ficheros cuyo hash no cambió, entre ellos el que traía el borrado), un borrado aplazado se marcaba «recuperado» solo por seguir el host en el plan | **Corregido**: solo se resuelve si la versión o el vector del plan difieren de los locales. Prueba: `un_borrado_aplazado_no_se_da_por_recuperado_con_un_plan_parcial` |
| NUC-2 | baja | `lector.rs` | Lo mismo en el lector del móvil: tras R85, un fichero dañado ya no frenaba `resolver_borrados_obsoletos` | **Corregido**: contador `incompletos` (dañado, formato nuevo, equipo inválido, demasiado grande). Prueba: `un_fichero_danado_no_resuelve_un_borrado_aplazado` |
| NUC-3 | baja | `servidor::consultar_trabajo` | «Sin servidor» se decidía por la ausencia del socket en una ruta que depende del entorno: sin `XDG_RUNTIME_DIR` (cron, `su`) se miraba `/tmp/magi-<uid>` y el servidor de la sesión gráfica escucha en `/run/user/<uid>/magi`, de modo que se aplicaban borrados de hosts con trabajo vivo | **Corregido**: con la ruta de reserva y sin `XDG_RUNTIME_DIR`, si existe el socket de la otra ruta es `Fallo` y se aplaza todo. Pruebas unitarias en `src/sincronizacion/cli.rs` |
| R1 | baja | `terminal_enganche.sh`, `terminal_tmux.sh` | `awk` compara como número lo que parece un número: una sesión «7» y otra «007» se confundían (la guarda R157 miraba la equivocada y `preparar` daba su identidad) | **Corregido**: comparación como texto en los cuatro `awk`. Prueba con tmux real: `sesiones_con_nombres_que_parecen_el_mismo_numero_no_se_confunden_de_verdad` |
| P1 | media | `assets/demo/magi.tape` | La cinta usaba `F4` y `F9`, teclas que VHS no tiene: `vhs` se habría negado a grabar | **Corregido**: `s` y `u` desde Flota y la paleta para volver; sin la pestaña (necesitaba `Ctrl+]`, que no se puede probar sin VHS). Pruebas: la cinta solo usa órdenes de VHS; el script valida con `vhs validate` |
| P2 | baja | `scripts/grabar-demo.sh` | La limpieza llamaba a `magi servidor parar` sin `--si`, con la salida a `/dev/null` y sin plazo: se quedaba esperando una confirmación invisible | **Corregido**: `parar --si </dev/null` con `timeout`. Prueba estática |

Hallazgos bajos que he corregido porque eran baratos y ciertos:

| Id | Qué | Cómo |
|---|---|---|
| R2 | `buscar_sesion` daba por la misma una sesión con igual nombre e igual identificador pero otra hora de creación (tmux reiniciado reutiliza los números) | `es_otra` compara también la hora de creación. Caso nuevo en la prueba |
| R3 / S-02 | `TerminarTmux` ignoraba el código de salida: una salida cortada o un script que no llegó a ejecutarse se leía como «la sesión ya no existía» (contra la regla «un fallo de lectura nunca se parece a una ausencia») | `interpretar_terminar`: «ya no existía» solo con la marca y código 0; lo demás es un error. Prueba unitaria |
| C9B-1 | Cerrar terminando una pestaña no garantizaba que la lista dejara de enseñar la sesión viva (la comprobación de menos de 5 s no se repetía) | `pedir_recomprobacion` tras terminar. Prueba del servidor: `tras_terminar_una_sesion_la_siguiente_consulta_vuelve_a_mirar_el_host` |
| P4 | El escáner de correos daba falsos positivos (`usuario@127.0.0.1`, `git@github.com`, las etiquetas de Kotlin con arroba (`this@…`)) y rompería la CI de una PR legítima | TLD en minúsculas, IP que no es pública y `github.com` en la lista |
| P8 | La guía de la candidata no avisaba de que cambiar `version` arrastra la DA2 (`10600` en una candidata), las instantáneas y la página man | Aviso en `docs/publicar.md` §4 y `docs/publicar-termux.md` §4 |
| P10 | La cinta decía que nada de lo que se ve es real, pero el sondeo mide la máquina que graba | Texto corregido; el script recuerda revisar los fotogramas |
| DOC-1 a DOC-4 | El README decía que «en pruebas» funciona cuando `estado.md` dice que macOS y Termux no se han probado; `estado.md` contradecía los informes en F3, F5, F12 y Android y callaba que la revisión de la F16 la hace la 9b; el ejemplo de `magi sincronizar` del manual no era lo que imprime el código | Corregidos con lo que consta en los informes de cada fase |

## Aceptado o pendiente (sin cambio de código)

| Id | Qué | Motivo |
|---|---|---|
| NUC-4 | La consulta al servidor va antes del cerrojo y se usa como foto al aplicar: queda una ventana de segundos | Así lo manda la especificación (§4.1); es la misma ventana que tiene la TUI entre su lista y la importación |
| S-01 | Una reconexión automática rechazada por la guarda (101) tras `marcar_abierta` deja anotada una `sesion_reconectada` en REGISTRO (y pinta «reconectada» un instante) | Solo en la carrera; las filas de REGISTRO no se editan. Pendiente: no anotar hasta ver que el enganche llegó a ocurrir |
| P5 | El escáner de `repositorio_publico` no mira IPv6, `/Users/` ni nombres con mayúsculas o punto | Los datos que cubre el checklist son IP públicas v4, `/home/<nombre>` y correos |
| P3 | La prerelease pública `artefactos-prueba-fase13` lleva un binario y un APK de depuración compilados en local, sin atestación | Fuera del árbol; decide el desarrollador si la borra antes de publicar |
| P6 | Alias que pueden ser nombres reales en la semilla de las instantáneas (`hetzner-01`, `vps-openclaw`…) | Ya anotado: no son IP, ruta ni correo; decide el desarrollador |
| P7 | `termux-release.yml` fija por rama o etiqueta mutable las acciones de terceros de los jobs que atestan | De la F17; se anota para revisarlo |
| P9 | El historial público lleva el correo personal del desarrollador en 18 merges de GitHub | Ya anotado (D391): el historial no se reescribe |
| DOC-5 | Los tarballs de `dist` y el `.deb` de Termux incluyen `README.md`, cuyos enlaces relativos a `docs/` no existen dentro del paquete | Solo afecta a quien lee el README del paquete; se anota |

## Revisado sin hallazgos

- **Servidor:** el plazo de 5 s de `consultar_trabajo` cubre conectar, saludar y leer, y el socket se suelta en todas las ramas; un servidor que miente o contesta basura solo puede dar `Fallo`, nunca `Trabajo`; el trabajo vivo cuenta lo mismo que la TUI y más; un cliente que conecta y desconecta en bucle no dispara SSH sin tope (5 s por host, 8 a la vez); el script por la entrada estándar va con plazo, cierra el canal en todas las ramas y no se ejecuta si no termina en `#MAGI-FIN`; los datos del usuario viajan como argumentos escapados y se vuelven a validar en el script; `creada` no queda pegada en reconexiones.
- **Cliente:** el diálogo «Volver a la sesión» fija la sesión y no manda nada si cambió; dos ventanas del mismo equipo no duplican fichas (`UNIQUE` y solo anota quien inserta); una ventana que se conecta tarde no resucita fichas borradas.
- **Núcleo:** todas las escrituras de tablas que viajan usan `cambiar_con_version`; el rechazo de un equipo desconocido no abre el fichero con la clave anterior ni deja contenido en el log; `restaurar_nombres_apartados` solo toca filas con el nombre temporal exacto.
- **Scripts:** `shellcheck` y `shellcheck -s dash` limpios; no hay bashismos ni usos de `awk` que `mawk` no soporte; el enganche escapado ocupa unos 4,2 KB (por debajo de los ~9000 B de Dropbear).
- **Publicación:** ninguna referencia funcional a `4d3-org` ni datos reales fuera del archivo de diseño; `gitleaks` (14 hallazgos, todos en ficheros de pruebas); workflows coherentes con `docs/publicar.md` §4.

## Lo que no se ha podido comprobar aquí

- Dropbear, `dash` y `mawk` (la máquina no los tiene): R154 y R159 se verifican de forma indirecta.
- `vhs` (no instalado): la cinta y el script de la demo se han revisado por lectura y con pruebas estáticas, no ejecutado.
- Las pruebas instrumentadas de Android (necesitan emulador).
