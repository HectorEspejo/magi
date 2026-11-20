# Snippets y deliberación MAGI

[← Índice del manual](README.md)

## Snippets

`F8` abre **Snippets**: comandos guardados que se ejecutan en varios hosts a
la vez. Cada snippet tiene nombre, comando (multilínea), descripción,
etiquetas propias, destinos, `crítico`, timeout (5-3600 s) y «parar al primer
fallo». Los **destinos** son etiquetas de host (`web` = todos los hosts con esa
etiqueta) y hosts sueltos; se resuelven al ejecutar, sin duplicados y en el
orden de Hosts.

- **Variables**: `{{servicio}}` o `{{servicio:nginx}}` (con valor por
  defecto); el nombre es `[a-z_][a-z0-9_]*` y todo lo demás (`{{.Names}}` de
  docker) es literal. El valor se sustituye **escapado para el shell** con
  `shell-escape`: `a b; rm -rf /` llega como un único argumento entrecomillado.
  Una variable vacía sin defecto no deja continuar, y una variable dentro de
  comillas o de un heredoc se rechaza al guardar (el escape no serviría ahí).
- **Ejecutar**: `↵` abre EJECUTAR (casillas por host, variables, «parar al
  primer fallo» y timeout); `a` ejecuta en todos los destinos pidiendo solo las
  variables; `p` abre una pestaña por host con el comando escrito al abrir el
  shell (confirma si son más de 5). Desde la paleta: `snippet · <nombre>` y
  `snippet · <nombre> · <host>`; en Hosts y Flota, `!` lista los snippets que
  apuntan al host seleccionado.
- **En el servidor**: la ejecución la hace el servidor de sesiones, hasta 8
  hosts en paralelo sobre la conexión del pool, con stdout y stderr separados,
  código de salida y el timeout del snippet (al vencer, el host queda en error
  «tiempo agotado» y la conexión sigue viva). Nunca dialoga: una huella sin
  aceptar, una clave con frase fuera del agente o una contraseña que no esté en
  el llavero dejan el host en error con la instrucción. «Parar al primer
  fallo» deja `omitido` lo que aún no había empezado.
- **Resultados** (`t` en Snippets): ejecuciones y hosts con su estado, código,
  duración y bytes; `↵` enseña stdout y stderr sin secuencias de escape y se
  refresca mientras corre; `s` guarda la salida en un fichero (600), `x`
  cancela, `r` repite (volviendo a deliberar si procede) y `C` limpia las
  terminadas. Cada flujo se guarda hasta 1 MiB; la salida **nunca** va al
  registro ni al log. Todas las ventanas ven el mismo progreso y las terminadas
  se conservan una hora.
- **Snippet al conectar**: en la ficha, bloque «Al conectar», un snippet no
  crítico y sin variables que se escribe en cada pestaña nueva a ese host y al
  reconectarla. Nunca delibera.
- **Atajos de Flota**: `[flota.atajos]` asigna una tecla a un snippet sobre el
  host seleccionado; sigue el mismo flujo que `snippet · <nombre> · <host>`.
  Las teclas que ya usa Flota o los snippets que no existen se ignoran con
  aviso al arrancar.

```sh
magi snippets    # destinos resueltos, crítico y último uso
```

## Deliberación MAGI

Antes de ejecutar un snippet **crítico**, en **más de un host** o en un host
con **verificaciones previas**, MAGI delibera: tres comprobaciones por host,
todas en paralelo y con un plazo duro de 2 s cada una, que nunca abren
diálogos.

| Comprobación | Aprueba si | Rechaza si |
|---|---|---|
| MELCHIOR-1 · salud | El último sondeo es NOMINAL y tiene menos de 5 min, o lo es uno nuevo | CARGA (con los culpables), ALCANZABLE, CAÍDA o sin datos a tiempo |
| BALTHASAR-2 · backup | El fichero más reciente de la ruta que casa con el patrón tiene menos de 24 h | Más viejo, directorio vacío, sin acceso SFTP o plazo vencido |
| CASPER-3 · tests | `sh -c <comando>` **en local** termina con 0 | Otro código (con la última línea de stderr) o plazo vencido (el proceso se mata) |

Qué comprueba cada host se elige en su ficha, bloque «Verificaciones previas»:
salud, backup (ruta remota y patrón, p. ej. `*.sql.gz`) y tests (comando
local, p. ej. `gh run list … | grep -qx success`). El SFTP de BALTHASAR-2 no
dialoga ni levanta los túneles automáticos del host.

El diálogo enseña una tabla host × comprobación (`✓` aprueba, `✕` rechaza,
`—` no activa, `◐` comprobando) con el dato de cada veredicto, la barra de
consenso y el estado. Hace falta **unanimidad**:

- **APROBADO**: `Ctrl+K` ejecuta. `↵` **no** ejecuta nunca.
- **BLOQUEADO**: `f` abre el campo motivo; con al menos 10 caracteres pasa a
  **FORZADO** y `Ctrl+K` ejecuta. El motivo queda registrado con el usuario y
  la hora.
- Sin comprobaciones activas (un snippet crítico en un host sin
  verificaciones) muestra «sin comprobaciones configuradas» y sigue pidiendo
  `Ctrl+K`.
- `Esc` cancela: no se ejecuta nada y el registro anota
  `deliberacion_cancelada`.

Cada deliberación resuelta queda en `DELIBERACIONES` (acción, hosts,
comprobaciones, resultado, motivo y usuario); al terminar la ejecución, el
servidor anota `deliberacion_aprobada` o `deliberacion_forzada` con el
resultado (`ok`, `parcial`, `error`, `cancelada`). En `F7`, el detalle de esas
entradas enseña las comprobaciones.

```toml
[deliberacion]
backup_horas = 24    # antigüedad máxima del backup
salud_max_min = 5    # antigüedad máxima del último sondeo
limite_seg = 2       # plazo de cada comprobación
motivo_min = 10      # caracteres mínimos del motivo de un forzado

[flota.atajos]
d = "desplegar"      # tecla → nombre del snippet
```
