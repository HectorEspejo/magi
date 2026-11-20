# Primeros pasos y diagnóstico

[← Índice del manual](README.md)

## Primeros pasos

1. Ejecuta `magi diagnostico` para comprobar rutas, permisos, socket, llavero,
   portapapeles y `~/.ssh/config`; cada aviso trae la instrucción de arreglo.
2. Arranca `magi` (abre la vista **Flota**). En **Hosts** (`F2`), pulsa `I`
   para importar `~/.ssh/config` o `magi importar [ruta]` desde fuera de la TUI.
3. Revisa la ficha con `e` (dirección, usuario, identidad, salto, keepalive,
   multiplexado, servicios y verificaciones previas) y guarda con `Ctrl+S`.
4. Conecta con `↵` (siempre abre una sesión nueva) y vuelve a la TUI con el
   prefijo `Ctrl+]` y `q` (o `Esc`). Cerrar la ventana no cierra las sesiones.
5. Explora con `Ctrl+P` (paleta) y `?` (ayuda): `F4` Archivos, `F5`
   Identidades, `F6` Túneles, `F7` Registro, `F8` Snippets y `F9` Servicios.
6. Si quieres que `ssh <nombre>` vea los hosts de MAGI, exporta con `E` en
   Hosts (o `magi exportar`) y acepta el `Include` en `~/.ssh/config`.

## Diagnóstico

`magi diagnostico` comprueba, sin escribir nada, sin arrancar el servidor y sin
leer secretos del llavero:

- versión, protocolo y commit;
- rutas de datos, configuración, estado y runtime con sus permisos, y la
  longitud de la ruta del socket;
- `config.toml`, `magi.db` (en solo lectura) y el servidor de sesiones (si está
  en marcha, su versión y lo que custodia);
- agente SSH, llavero, portapapeles, editor, paginador, terminal e `Include` de
  `magi_config` en `~/.ssh/config`.

Cada línea sale como `✓` correcto, `!` aviso o `✕` error (en ASCII `ok`, `!` y
`x`), con la instrucción de arreglo; no hay color si `NO_COLOR` está definido o
la salida no es una terminal. El código de salida es 0 si no hay errores (los
avisos no cuentan) y 1 si hay alguno. Es la primera comprobación que conviene
ejecutar tras instalar o actualizar MAGI.

## Primer arranque

MAGI crea su configuración, su base de datos y su log en las rutas XDG
(equivalentes en macOS vía `directories`):

| Qué | Ruta |
|---|---|
| Inventario | `~/.local/share/magi/magi.db` (SQLite en WAL, 600) |
| Configuración | `~/.config/magi/config.toml` |
| Log | `~/.local/state/magi/logs/magi.log.<fecha>` (rotación diaria) |
| Log del servidor | `~/.local/state/magi/logs/servidor.log.<fecha>` |
| Socket y lock | `$XDG_RUNTIME_DIR/magi/servidor.sock` + `servidor.lock` (700) |

`config.toml`:

```toml
prefijo_escape = "Ctrl+]"   # prefijo de la vista Sesión
exportar_al_guardar = true  # regenera magi_config al guardar una ficha
tema = "auto"               # auto | fijo | /ruta/a/alacritty.toml
terminal_ascii = false      # fuerza el modo degradado ASCII

[flota]
auto_refresco_seg = 0       # 0 = desactivado; mínimo 15 s (se conmuta con «a»)

[flota.umbrales]
carga_por_nucleo = 1.0      # CARGA si carga_1m > umbral × núcleos
memoria_pct = 90.0          # CARGA si el uso de memoria supera el %
disco_pct = 90.0            # CARGA si el uso de disco supera el %

[terminal]
comando = "alacritty -e magi"   # ventana nueva (prefijo w); macOS: open -a Terminal magi
historial_lineas = 10000        # líneas de historial por pestaña (0-100000)
historial_mib = 64              # tope de memoria del historial (8-512)
raton = false                   # rueda en el historial y clic para elegir panel
teclado_mejorado = true         # desambiguación de kitty si el terminal la admite
avisar_pegado = true            # aviso al pegar varias líneas sin modo 2004
abrir_enlaces = "xdg-open"      # macOS: open · Termux: termux-open-url
reconexion_max_min = 15         # minutos de reconexión automática (1-120)

[servidor]
gracia_apagado_seg = 10     # sin clientes ni sesiones, el servidor se apaga
```

Con `tema = "auto"` se leen los colores de
`~/.config/omarchy/current/theme/alacritty.toml`; si no existe se usa la
paleta fija de respaldo.
