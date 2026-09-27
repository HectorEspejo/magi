# MAGI - Checklist Fase 7: Redimensionado Adaptable

## Diagnóstico
- [ ] Test que reproduce el fallo 1: tras pintar a 120×40 y recibir `Resize(80, 24)` sin más eventos, la vista debe quedar pintada a 80×24 sin pulsar ninguna tecla
- [ ] Test que reproduce el fallo 2: con una pestaña adjunta, un `Resize` debe llegar al remoto como `window_change` con el tamaño nuevo (el servidor SSH de pruebas registra los `window_change` recibidos)
- [ ] Causa de los dos fallos documentada en el informe de implementación (dónde se perdía el evento o el tamaño) y ambos tests convertidos en regresión permanente

## Tubería de redimensionado
- [ ] `Event::Resize` nunca se descarta ni se filtra en el hilo de eventos; la pausa del teclado durante el visor (F4) no afecta a los eventos de tamaño
- [ ] `Geometria` en `app.rs` con tamaño aplicado, pendiente y hora del último evento; agrupación de 50 ms que aplica solo el último tamaño
  - AC: Dado 30 eventos `Resize` en 400 ms, cuando pasan 50 ms sin eventos, entonces se aplica un único tamaño (el último), con un solo `clear` y un solo `Redimensionar`
- [ ] El bucle se despierta por tick (≤ 16 ms) mientras hay un tamaño pendiente, sin esperar a una tecla
- [ ] `aplicar_tamano`: `terminal.resize` + `clear`, recálculo de la disposición, recorte de desplazamiento y selección, recolocación del diálogo o paleta abiertos y repintado
- [ ] Un tamaño igual al ya aplicado no hace nada
- [ ] Al volver del visor o de cualquier suspensión de la TUI se lee el tamaño real y se aplica
- [ ] Al relanzar el servidor y readjuntar pestañas, `Adjuntar` lleva el tamaño aplicado

## Sesión y remoto
- [ ] En la vista Sesión, cada tamaño aplicado envía `Redimensionar{sesion_id, cols, alto_pty(filas)}` de la pestaña adjunta
  - AC: Dado `stty size` en una pestaña, cuando se hace zoom de fuente en Alacritty y se vuelve a ejecutar, entonces muestra el tamaño nuevo sin tocar ninguna tecla más
- [ ] `alto_pty` se calcula siempre con el tamaño aplicado, nunca con uno guardado al abrir la pestaña
- [ ] Cambiar de pestaña adjunta la nueva con el tamaño aplicado y la desadjuntada deja de contar para el mínimo
- [ ] Pestaña compartida: el servidor recalcula el mínimo de los adjuntos en cada `Redimensionar`, `Adjuntar` y `Desadjuntar`, envía `window_change` si cambia y difunde `Redimensionada`
  - AC: Dado una pestaña en ventanas de 200×50 y 120×40, cuando la segunda pasa a 100×30 y luego se cierra, entonces el remoto recibe primero el tamaño de 100 columnas y después el de 200
- [ ] La ventana mayor rellena con `░` la zona sobrante e indica `cols×filas (mín. ventana N)` en cada cambio
- [ ] Por debajo del mínimo de Sesión el remoto sigue recibiendo su tamaño real saneado (mín. 2×1) y las teclas siguen yendo al remoto
- [ ] Test de extremo a extremo: `vim` o `htop` simulados reciben el `window_change` tras cada tamaño aplicado, con una y con dos ventanas

## Disposición adaptable
- [ ] `ui/disposicion.rs` con los puntos de corte (estrecho < 100 columnas, columnas mínimas < 80, bajo < 20 filas), `ModoAncho {Normal, Estrecho, Minimo}` y los mínimos por vista
- [ ] Ninguna vista guarda tamaños entre pintados; las teclas de página y filas visibles usan la disposición del último pintado
- [ ] Tras aplicar un tamaño, la fila seleccionada sigue visible y el desplazamiento no deja huecos al final de la lista
- [ ] El cambio de modo conserva selección, marcados, filtro, panel activo y el texto de los diálogos
- [ ] Tablas con columnas por prioridad: el identificador principal y el glifo de estado nunca se ocultan
- [ ] Barra inferior con atajos por prioridad y `? más` cuando no caben todos
- [ ] Paneles, detalles y diálogos con ancho máximo y centrados en ventanas muy grandes (≥ 200×60)

## Tamaño mínimo
- [ ] Mínimo global 40×12 y mínimos por vista: Ficha 50×14, Sesión 40×8, Archivos 50×14, Transferencias/Túneles/Registro/Identidades/Snippets 50×12, Resultados 60×14, diálogo MAGI 50×12
- [ ] Por debajo del mínimo se pinta el aviso centrado «ventana demasiado pequeña · actual · mínimo» con la vista que lo exige, en lugar de la vista
  - AC: Dado la vista Archivos a 45×20, cuando se pinta, entonces aparece el aviso con «mínimo 50×14» y ningún panel recortado
- [ ] Si ni el aviso cabe (menos de 20×3), se pinta solo `MAGI cols×filas`
- [ ] Con el aviso visible siguen activas `q`, `F1`-`F8` y `Ctrl+P`

## Vistas
- [ ] Flota: en estrecho un panel y `Tab` alterna lista y detalle; barras de carga, memoria y disco con ancho variable
- [ ] Hosts y Sesiones: ocultan dirección por debajo de 80 columnas y usuario·puerto por debajo de 60
- [ ] Ficha de host: desplazamiento vertical con el campo con foco siempre visible; en estrecho, etiquetas encima de los campos
- [ ] Sesión: barra de pestañas compacta (solo glifo y número si no cabe el nombre) y barra de estado compactada por prioridad
- [ ] Archivos: en estrecho un panel con `Tab` e indicador `[local]` / `[remoto]` en la cabecera; con alto < 20 la cola se resume en el pie
- [ ] Transferencias, Túneles, Registro, Identidades y Snippets: columnas por prioridad y detalle inferior plegado con alto < 20 (se abre con `↵`)
- [ ] Resultados: con alto < 24 la salida solo se ve con `↵`; paneles de ejecuciones y hosts apilados
- [ ] Diálogos, paleta y ayuda `?`: se recolocan en cada cambio y encogen al área con desplazamiento (`↑` `↓`, `PgUp` `PgDn`)
- [ ] Diálogo MAGI compacto: nombres abreviados `M-1`, `B-2`, `C-3`, datos recortados y hosts con desplazamiento
- [ ] Modo ASCII degradado correcto en todos los modos (sin glifos Unicode en estrecho ni en el aviso)

## Pruebas de pintado
- [ ] `tests/redimensionado.rs` con `TestBackend`: todas las vistas y diálogos pintan sin panic a 40×12, 80×24 y 200×60
- [ ] Instantáneas `insta` de cada vista a los tres tamaños, revisadas y guardadas en `tests/snapshots/`
- [ ] Secuencias de cambio (200×60 → 80×24 → 40×12 → 200×60) con estado coherente: selección visible, filtro y marcados conservados, diálogo abierto recolocado
- [ ] Toda vista nueva que se añada en el futuro debe incluirse en estas pruebas (anotado en `CLAUDE.md`)

## Calidad
- [ ] `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check` limpios
- [ ] `cargo test` verde, incluidas las instantáneas
- [ ] Revisión adversarial del código tocado antes de cerrar, con resultado en el informe de implementación
- [ ] README: comportamiento al redimensionar, modo estrecho, tamaños mínimos

---

**Progreso Fase 7:** 0 / 46 funcionalidades

**Total MAGI (Fases 1-7):** 487 / 536 funcionalidades
