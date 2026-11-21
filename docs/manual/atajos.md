# Atajos

[← Índice del manual](README.md)

## Atajos

Globales: `F1` Flota · `F2` Hosts · `F3` pestaña activa o lista de sesiones ·
`F4` Archivos · `F5` Identidades · `F6` Túneles · `F7` Registro · `F8`
Snippets · `F9` Servicios · `Ctrl+P` paleta · `?` ayuda · `Esc` cierra
diálogos y filtros · `q` vuelve o sale.

Flota: `↑` `↓` / `j` `k` mover · `↵` conectar · `r` sondear el host · `R`
sondear todos los visibles · `e` editar la ficha · `/` filtro · `a`
activar/pausar el auto-refresco · `!` snippets del host · `Tab` alterna
lista y detalle en modo estrecho · las teclas de `[flota.atajos]`, que
aparecen en la barra. `↵` abre siempre una sesión nueva.

Hosts: `↑` `↓` / `j` `k` mover · `←` `→` / `h` `l` plegar grupo · `↵`
conectar (siempre sesión nueva) · `e` editar · `n` nuevo · `g` menú de
grupos · `x` borrar · `Tab` alternar usuario·puerto / etiquetas · `/` filtro ·
`I` importar · `E` exportar · `!` snippets del host. `●N` junto al nombre
indica N sesiones vivas.

Identidades: `n` generar · `i` importar · `c` copiar la pública · `e` alias ·
`x` revocar/reactivar · `s` reescanear · `v` ver revocadas · `↵` detalle
(con la vista baja).

Registro: `↵` detalle · `/` filtrar · `t` ciclar el tipo · `p` purgar >90
días · `x` exportar CSV/JSON.

Ficha: `Tab` / `Shift+Tab` campos · `↵` abrir desplegable · `Espacio`
marcar casilla · `Ctrl+S` guardar · `Ctrl+T` probar la conexión · `Esc`
descartar.

Sesión: todas las teclas van al host remoto salvo el prefijo configurado
(`Ctrl+]` por defecto). Con el prefijo: navegación de pestañas (`1-9`, `n`,
`p`), lista (`l`), nueva sesión (`c`), archivos (`f`), cerrar pestaña (`x`),
reconectar (`r`), ventana nueva (`w`), volver (`q`/`Esc`) y el prefijo
literal (`prefijo prefijo`). Las sesiones viven en el servidor: cerrar la
ventana no las cierra.

Sesiones (`prefijo l`), bloque Persistentes: `↵` engancharse soltando a los
demás · `c` compartir · `v` vista previa · `n` nueva · `e` editar la ficha · `,`
renombrar · `T` terminar · `x` borrar la ficha · `a` añadir una sin ficha · `C`
borrar las terminadas · `r` refrescar · `/` filtrar · `Tab` pasa a Pestañas y
vuelve. Con pestañas, `prefijo N` abre una sesión persistente nueva. Una
pestaña soltada: `↵` volver · `c` compartir · `x` cerrar.

Archivos: `Tab` panel · `↑` `↓` / `j` `k` mover · `PgUp` `PgDn` página ·
`Home` `End` extremos · `↵` entrar o ver · `⌫` / `-` subir · `Espacio` marcar y
bajar · `a` / `A` marcar todo / desmarcar · `c` copiar · `m` mover · `x`
borrar · `E` editar · `p` permisos · `S` sincronizar · `L` sincronizaciones
guardadas · `r` renombrar · `d` crear directorio · `.` ocultos · `/` filtro ·
`R` refrescar · `g` ir a ruta · `i` detalle · `h` cambiar de host · `t` cola ·
`Esc` limpiar filtro o marcas · `q` volver.

PERMISOS: `↑` `↓` `←` `→` rejilla · `Espacio` marcar · `Tab` / `Shift+Tab`
octal, recursivo y alcance · `Ctrl+S` aplicar · `Esc` cancelar.

SINCRONIZAR: `Tab` / `Shift+Tab` campos · `Espacio` casillas · `↵` planificar ·
`Esc` cancelar. Vista previa: `↑` `↓` `PgUp` `PgDn` recorrer · `f` filtrar por
tipo de cambio · `↵` continuar · `Esc` cancelar. Guardadas (`L`): `↵`
ejecutar · `n` nueva · `e` editar · `x` borrar · `q` volver.

Transferencias: `↑` `↓` mover · `x` cancelar · `C` limpiar terminadas · `↵`
detalle · `q` volver.

Túneles: `↑` `↓` / `j` `k` mover · `Espacio` activar, parar o descartar el
error · `n` nuevo · `e` editar · `x` borrar · `a` automático · `r` relanzar ·
`↵` detalle · `/` filtro · `Esc` limpiar el filtro o volver · `q` volver.

Snippets: `↑` `↓` / `j` `k` mover · `↵` ejecutar (diálogo) · `a` en todos los
destinos · `p` en pestañas · `n` nuevo · `e` editar · `x` borrar · `/`
filtro · `t` Resultados · `i` detalle (con la vista baja) · `q` volver. En el
formulario, `Ctrl+S` guarda.

Servicios: `↑` `↓` / `j` `k` mover · `Tab` hosts ⇄ lista (en el detalle,
Estado · Dependencias · Unidad) · `1` `2` `3` sección · `/` filtro · `↵`
detalle · `l` logs · `A` `P` `R` `G` `H` acciones · `S` shell del
contenedor · `c` comprobación propia · `!` contraseña de sudo · `r` refrescar
· `Esc` volver. En el panel de logs: `/` texto · `n` nivel · `Espacio` pausa ·
`Fin` al final · `t` pestaña. En «Toda la flota», `↵` va al host y `r`
sondea todos.

Resultados: `↑` `↓` / `j` `k` mover · `Tab` panel · `↵` ver la salida (`Tab`
alterna stdout/stderr; con menos de 24 filas es la única forma de verla) ·
`s` guardar la salida · `x` cancelar · `r` repetir · `C` limpiar terminadas ·
`q` volver.

Deliberación: `Ctrl+K` ejecutar (solo con consenso o forzada) · `f` forzar con
motivo · `↑` `↓` recorrer hosts · `Esc` cancelar o salir del motivo.

En `config.toml`:

```toml
[archivos]
avisar = [".env*", "*.pem", "*.key", "id_*"]
mostrar_ocultos = false
pager = ""        # vacío usa $PAGER y, si no hay, less
editor = ""       # vacío usa $VISUAL, $EDITOR, nvim y vi (programa y argumentos, sin shell)
excluir = [".git/", "target/", "node_modules/", "__pycache__/", ".DS_Store", ".magiignore"]
editar_max_mb = 10   # por encima, E pide confirmación antes de bajar el fichero
```
