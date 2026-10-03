# MAGI - Fase 7: Redimensionado Adaptable

## Especificación Funcional

**Versión:** 1.3
**Fecha:** 27 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

**Changelog**
- 3 oct 2026 — v1.3: renumeración de fases futuras (la Fase 8 es Archivos, segunda vuelta; Sincronización cifrada pasa a F9 y Android a F10). Sin cambios de alcance.
- 28 sep 2026 — v1.2: ajustada al informe de implementación (46/46; rama `fase7-fix_redimensionado_ventana`, un commit por sprint). Causas de R36 localizadas (§1.1). `Geometria` en `src/app/geometria.rs`. El mínimo de cada vista sustituye al global (Sesión 40×8). Detalle plegado de Snippets con `i` (`↵` ejecuta). Con el aviso visible, diálogos, paleta y ayuda se pintan encima y conservan sus teclas; una deliberación tapada solo admite `Esc`; en la ficha con cambios, `q` se ignora; el aviso dice siempre qué exige el mínimo. Un redimensionado de ida y vuelta limpia y repinta sin `Redimensionar`. `Redimensionada` por ventana con la ventana que explica su relleno; la tarea de lectura del cliente aplica el tamaño en orden con los datos; `PantallaCompleta` declara el tamaño de su parser. Archivos resume la cola también en estrecho; paneles hasta 240 columnas. Superficie de pruebas `#[doc(hidden)] pub` (`App::de_prueba`, `ciclo`, `paso`, `Cliente::de_prueba`).
- 27 sep 2026 — v1.1: procesado el informe de implementación de la Fase 6 (sin cambios de alcance). La ficha de host ya tiene desplazamiento vertical desde la F6 (la F7 lo revisa); `app` es ahora un módulo con submódulos (`src/app/`); la F6 ya usa `TestBackend` en algunas pruebas de pintado; el diálogo EJECUTAR (F6) entra en la revisión de tamaños.

> La Fase 6 ya tiene informe de implementación procesado (79/80, sin la revisión adversarial de S1-S3, R38). Las Fases 2 a 5 siguen sin validación manual completa (R15). La fase revisa también las vistas de la Fase 6 (Snippets, EJECUTAR, Resultados, diálogo MAGI).

---

## 1. Visión General

Hector ha detectado dos fallos al cambiar el tamaño de la terminal (27 sep 2026):

1. **La interfaz no se repinta**, o se queda con el tamaño viejo hasta que se pulsa una tecla.
2. **En una pestaña, el remoto no se entera**: `vim` o `htop` siguen con el tamaño anterior.

Las dos cosas estaban pedidas desde la Fase 1 («Redimensionado de la terminal repinta la vista actual», «Redimensionar la terminal envía `window_change` al remoto») y se validaron entonces, así que se tratan como una **regresión** (R36), probablemente introducida con los dos procesos de la Fase 3 o con la suspensión de la TUI de la Fase 4.

La fase no se limita a parchear: convierte el redimensionado en un mecanismo único y probado. Un solo punto recibe los cambios de tamaño, los agrupa, recalcula todo lo que depende del tamaño y avisa al servidor. Cada vista se adapta a ventanas estrechas o bajas en lugar de cortarse, y por debajo de un mínimo muestra un aviso en vez de una pantalla rota. Pruebas de pintado a tres tamaños impiden que vuelva a romperse.

**Objetivos principales**

1. Diagnosticar y documentar la causa de los dos fallos, con un test que los reproduzca antes de arreglarlos.
2. Tubería única de redimensionado: evento → agrupación de 50 ms → aplicar (repintar, recalcular, avisar al remoto). Cubre mosaico de Hyprland, mover ventanas, zoom de fuente de Alacritty, pantalla completa y la vuelta del visor.
3. Pestañas: cada cambio efectivo de tamaño llega al remoto; la regla del tamaño mínimo entre ventanas compartidas (F3) se recalcula siempre.
4. Disposición adaptable en todas las vistas: modo estrecho (paneles dobles a uno solo con `Tab`, columnas secundarias ocultas), vistas bajas (detalles plegados), barras con prioridad.
5. Tamaño mínimo por vista con aviso centrado; diálogos, paleta y ayuda que se recolocan y encogen con desplazamiento.
6. Pruebas de pintado con `TestBackend` a 40×12, 80×24 y 200×60 en todas las vistas, y pruebas de extremo a extremo del `window_change`.

**Contexto.** Fase pequeña-media sin cambios en el modelo de datos ni en el protocolo (salvo que el diagnóstico lo exija). Toca muchas vistas, por eso lo importante es la regla común, no el parche de cada una.

### 1.1 Causas encontradas (implementación)

- **Fallo 2 (remoto sin tamaño nuevo), en el servidor, desde la Fase 3:** `sesiones::tamano_minimo` calculaba el mínimo de los adjuntos partiendo del tamaño ya aplicado (`fold(actual, min)`), así que una sesión podía encoger pero **nunca crecer**: fallaban crecer la ventana, `Ctrl+-`, salir de un mosaico, cerrar la ventana pequeña de una pestaña compartida y adjuntar una ventana mayor. Encoger sí funcionaba, por eso parecía intermitente.
- **Fallo 1 (sin repintado hasta pulsar una tecla), en el cliente, desde la Fase 4:** durante la pausa del visor, el hilo de teclas hacía `poll(ZERO); read()`; ese `read()` bloquea, y al cerrar el paginador el hilo tiraba el primer evento que llegara (si era un `Resize`, no había repintado ni `Redimensionar`). Además robaba pulsaciones a `less`. Arreglo: la pausa no toca el tty y se confirma con acuse (`Condvar`, ≤ 200 ms).
- **Fuera del visor el bucle viejo sí repintaba.** Si el fallo 1 aparece sin haber abierto el visor en esa sesión, la causa no está localizada (queda para la validación manual).

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin crates nuevos en ejecución. En desarrollo, **`insta`** para comparar el pintado de cada vista con una instantánea de texto.

| Capa | Tecnología | Notas |
|---|---|---|
| Eventos de tamaño | `crossterm::event::Event::Resize` (SIGWINCH) | Llegan por el hilo de eventos de la Fase 3; nunca se descartan ni se filtran |
| Terminal | `ratatui::Terminal::autoresize` / `resize` + `clear` al aplicar | El buffer anterior se invalida para no dejar restos |
| Remoto | `Redimensionar{sesion_id, cols, filas}` (F3) → `window_change` | Sin mensajes nuevos |
| Pruebas | `ratatui::backend::TestBackend` + `insta` (dev) | Instantáneas por vista y tamaño |

### Estructura de carpetas (novedades)

```
src/
├── app.rs                 aplicar_tamano(), tamano_pty(), bucle ciclo/paso genérico sobre Backend
├── app/geometria.rs       Geometria: tamaño aplicado, pendiente y agrupación; Vencido {Aplicar, Repintar}
├── app/teclado.rs         hilo de teclas con FuenteEventos inyectable y pausa con acuse (no toca el tty)
├── ui/
│   ├── disposicion.rs     NUEVO  puntos de corte, mínimos por vista, prioridades de columnas y barras,
│   │                             centrar/encoger diálogos, ModoAncho {Normal, Estrecho, Minimo}
│   ├── aviso_tamano.rs    NUEVO  aviso «ventana demasiado pequeña»
│   └── *.rs               cada vista calcula su disposición desde el área en cada pintado
└── cliente/mod.rs         Redimensionar de la pestaña adjunta al aplicar un tamaño
tests/
├── redimensionado.rs      NUEVO  pintado a 3 tamaños por vista, secuencias de cambio, estado coherente
├── snapshots/             NUEVO  instantáneas insta
└── comun/mod.rs           el servidor SSH de pruebas registra los window_change recibidos
docs/fase7/
├── magi-fase7-informe.md · magi-fase7-prompt.md · magi-fase7-checklist.md
└── magi-fase7-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura (tubería de redimensionado)

```
  Hyprland / Alacritty (mosaico, mover, zoom, pantalla completa)
        │ SIGWINCH
        ▼
  hilo de eventos (crossterm) ── Event::Resize(c, f) ──▶ canal de eventos (F3)
        │  (nunca se pausa para Resize: el visor de F4 pausa solo el teclado)
        ▼
  app.rs · Geometria
  ┌──────────────────────────────────────────────────────────────┐
  │ pendiente = (c, f), ultimo_evento = ahora                     │
  │ tick: si ahora - ultimo_evento ≥ 50 ms y pendiente ≠ aplicado │
  │    └─▶ aplicar_tamano(c, f)                                   │
  │          1. terminal.resize + clear                           │
  │          2. ModoAncho / mínimo de la vista actual             │
  │          3. recortar desplazamientos y selección visibles     │
  │          4. marcar sucio → pintar                             │
  │          5. si vista Sesión: Redimensionar(pestaña, c, alto_pty(f)) ─┐
  └──────────────────────────────────────────────────────────────┘      │
        ▼ pintado                                                       ▼ socket (F3)
  ui/*.rs: cada vista pide a disposicion.rs su reparto            magi --servidor
  a partir del área del frame; nada guarda tamaños                 min(adjuntos) → window_change
  entre pintados                                                   → parser.set_size → Redimensionada
                                                                            │
                                                                  host remoto (SIGWINCH a vim/htop)
```

---

## 3. Modelo de Datos

Sin cambios en SQLite ni en el protocolo. Estructuras en memoria del cliente:

```
 ┌─────────────────────────────────────┐        ┌──────────────────────────────────┐
 │ Geometria (app.rs)                  │        │ Disposicion (ui/disposicion.rs)  │
 │ aplicado      (cols, filas)         │───────▶│ modo_ancho   Normal|Estrecho|Min │
 │ pendiente     Option<(cols, filas)> │ deriva │ minimo_vista (cols, filas)       │
 │ ultimo_evento Instant               │        │ areas        por vista (Rect)    │
 │ agrupacion    50 ms                 │        │ filas_visibles por lista         │
 └─────────────────────────────────────┘        │ columnas_visibles por tabla      │
                                                 └──────────────────────────────────┘
  La Disposicion se recalcula en cada pintado; las teclas que necesitan «página» o
  «filas visibles» usan la del último pintado (única copia, sobrescrita en cada frame).
```

### Diagrama de estados: geometría del cliente

```
   estable ──Event::Resize──▶ agrupando (guarda el último tamaño; reinicia el reloj de 50 ms)
      ▲                          │  más Resize: sigue agrupando
      │                          │  50 ms sin eventos
      │                          ▼
      └──────────────────── aplicando ── resize + clear + recalcular + pintar
                                 │        + Redimensionar si hay pestaña adjunta
                                 │  tamaño igual al aplicado → no hace nada
  Otras entradas a «aplicando»: arranque, vuelta del visor/paginador (F4), relanzado del servidor (F3),
  adjuntar una pestaña (se envía el tamaño actual en Adjuntar).
  Inválido: pintar con un tamaño distinto del aplicado; enviar Redimensionar sin pestaña adjunta.
```

### Diagrama de estados: modo de disposición (por vista)

```
                  ancho ≥ 100                ancho < 100 (o alto bajo)        ancho o alto < mínimo de la vista
   Normal ◀──────────────────────▶ Estrecho ◀──────────────────────────▶ Mínimo (aviso centrado)
   paneles dobles, todas las       un panel con Tab, columnas              sin pintar la vista; teclas globales
   columnas, detalles abiertos     secundarias ocultas, detalles           activas; en Sesión las teclas siguen
                                   plegados                                 yendo al remoto
  El cambio de modo conserva la selección y el panel activo; volver a Normal restaura la disposición.
```

---

## 4. Flujos de Trabajo

### 4.1 Diagnóstico (primer paso de la fase)

```
  reproducir el fallo 1 con TestBackend: pintar a 120×40 → evento Resize(80×24) sin más eventos
      → ¿se repinta a 80×24 sin pulsar tecla?  no ─▶ localizar la causa (evento descartado, pintado
        condicionado a «sucio», tamaño guardado, hilo de eventos pausado…)
  reproducir el fallo 2 con el servidor SSH de pruebas: pestaña adjunta → Resize → ¿llega window_change?
      no ─▶ localizar dónde se pierde (cliente no envía, envía el tamaño viejo, servidor no aplica)
      ▼
  documentar la causa en el informe de implementación ─▶ convertir ambas reproducciones en tests de regresión
```

### 4.2 Cambio de tamaño en cualquier vista

```
  Event::Resize(c, f)
      ▼
  Geometria.pendiente = (c, f) · ultimo_evento = ahora
      ▼ (tick del bucle, cada ≤ 16 ms mientras hay pendiente)
  ¿50 ms sin Resize? ─no─▶ esperar
      ▼ sí
  ¿(c, f) == aplicado? ─sí─▶ descartar
      ▼ no
  terminal.resize(c, f) · clear
      ▼
  ¿vista por debajo de su mínimo? ─sí─▶ pintar aviso de tamaño (§6.3) ─▶ fin
      ▼ no
  Disposicion para la vista (Normal / Estrecho) ─▶ recortar desplazamiento y selección para que la fila
  seleccionada siga visible ─▶ recolocar diálogo o paleta abiertos ─▶ pintar
      ▼
  ¿vista Sesión con pestaña adjunta? ─sí─▶ Redimensionar{sesion_id, c, alto_pty(f)}
```

### 4.3 Pestaña compartida entre dos ventanas

```
  ventana 1 (200×50) y ventana 2 (120×40) adjuntas a la pestaña P
      ▼ ventana 2 pasa a 100×30 (tras 50 ms de agrupación)
  ventana 2: Redimensionar{P, 100, alto_pty(30)}
      ▼
  servidor: tamaño de P = mínimo de adjuntos = (100, 26) ─cambia─▶ window_change remoto · parser.set_size
            ─▶ Redimensionada{P, 100, 26, ventana_minima = 2} a ambas
      ▼
  ventana 1: rellena con ░ lo que sobra y muestra «100×26 (mín. ventana 2)»
  ventana 2: ocupa todo · ventana 2 vuelve a 120×40 ─▶ el mínimo vuelve a ser (120, 36) ─▶ window_change
  ventana 2 se cierra o sale de Sesión (desadjunta) ─▶ el mínimo pasa a ser el de ventana 1 ─▶ window_change
```

### 4.4 Diagrama de secuencia: `vim` remoto tras el zoom de Alacritty

```
 Alacritty   hilo eventos   app (Geometria)     servidor        host (vim)
    │─SIGWINCH─▶│              │                    │                │
    │           │─Resize(160,45)▶│ agrupa            │                │
    │─SIGWINCH─▶│─Resize(150,42)▶│ agrupa (reinicia)  │                │
    │           │              │ … 50 ms …          │                │
    │           │              │ resize+clear+pinta │                │
    │           │              │─Redimensionar(P,150,38)─▶│          │
    │           │              │                    │─window_change─▶│ SIGWINCH → vim repinta
    │           │              │◀─Redimensionada────│                │
    │           │              │ parser.set_size    │◀──── Datos ────│
    │           │              │◀──── Datos ────────│                │
```

---

## 5. Acciones y Atajos

No hay endpoints ni mensajes nuevos. Cambios de comportamiento:

| Dónde | Novedad |
|---|---|
| Flota, Archivos (y cualquier vista de dos paneles) | En modo estrecho se ve un panel; `Tab` alterna entre ellos (en Archivos ya alternaba el activo) |
| Listas y tablas | Columnas secundarias ocultas en modo estrecho; `i` / `↵` abre el detalle con todo |
| Detalles inferiores (Identidades, Túneles, Registro, Snippets) | Se pliegan si la vista es baja; `↵` los abre en diálogo |
| Barra inferior | Muestra los atajos por prioridad; si no caben todos, termina en `? más` |
| Barra de pestañas (Sesión) | Truncado más agresivo en estrecho (solo glifo y número si no cabe el nombre) |
| Barra de estado de Sesión | Se compacta por prioridad: glifo · posición · host · tiempo · identidad · carga · ventanas |
| Diálogos, paleta, ayuda | Se recolocan en cada cambio y encogen al área disponible con desplazamiento (`↑` `↓`, `PgUp` `PgDn`) |
| Aviso de tamaño mínimo | Sustituye a la vista; `q`, `F1`-`F8` y `Ctrl+P` siguen activos; en Sesión las teclas siguen yendo al remoto |

---

## 6. Interfaz de Usuario

### Mapa de navegación

Sin pantallas nuevas. Adaptación por vista:

```
   F1 Flota ········ lista │ detalle ──estrecho──▶ lista ⇄ detalle (Tab); barras de ancho variable
   F2 Hosts ········ columnas: nombre · dirección · usuario · puerto ──▶ oculta dirección (<80), usuario·puerto (<60)
      └ Ficha ······ formulario con desplazamiento; el campo con foco siempre visible
   F3 Sesión ······· PTY = área real; pestañas y barra compactas; ░ si el remoto es menor
      └ Sesiones ··· columnas secundarias ocultas
   F4 Archivos ····· local │ remoto ──estrecho──▶ un panel (Tab); cola al pie oculta si alto < 20
      └ Transferencias columnas por prioridad
   F5 Identidades ·· tabla + detalle ──bajo──▶ detalle plegado (↵)
   F6 Túneles ······ tabla + detalle ──bajo──▶ detalle plegado; columna host abreviada
   F7 Registro ····· tabla + detalle ──bajo──▶ detalle plegado
   F8 Snippets ····· lista + detalle ──bajo──▶ detalle plegado; diálogo EJECUTAR con rejilla de hosts adaptable
      └ Resultados · ejecuciones / hosts / salida ──bajo──▶ salida solo con ↵
   Transversal: diálogos · paleta · ayuda · diálogo MAGI · aviso de tamaño mínimo
```

### 6.1 Flota en modo estrecho (70×20)

```
┌ MAGI · FLOTA ─────────────────── 7 hosts ─┐
│  HOSTS                        ⇥ detalle   │
│  ───────────────────────────────────────  │
│▸ ● hetzner-01    NOMINAL                  │
│  ● hetzner-02    NOMINAL                  │
│  ◐ mac-mini-m1   CARGA                    │
│  ○ dgx-spark     FRÍA                     │
│  ✕ rincon-dev    CAÍDA                    │
│                                           │
├───────────────────────────────────────────┤
│ ↵ ssh  r sondear  ⇥ detalle  ? más        │
└───────────────────────────────────────────┘
```

Con `Tab`, el mismo espacio muestra el detalle de hetzner-01 (barras de ancho ajustado) y `⇥ lista`.

### 6.2 Sesión estrecha (60×16)

```
┌ MAGI · SESIÓN ──────────────────── hetz-01 ┐
│▸● 1 hetzner-01 │ ◐ 2 │ ✕ 3 │ +             │
├────────────────────────────────────────────┤
│ root@hetzner-01:~# htop                    │
│  (htop repintado a 60×12)                  │
│                                            │
├────────────────────────────────────────────┤
│ ● 1/3 · hetzner-01 · 18 m · ^] ?           │
└────────────────────────────────────────────┘
```

### 6.3 Aviso de tamaño mínimo

```
┌──────────────────────────────┐
│                              │
│   ✕ ventana demasiado        │
│     pequeña                  │
│     36×10 · mínimo 40×12     │
│   (Archivos necesita 50×14)  │
│                              │
└──────────────────────────────┘
```

Si ni el aviso cabe (menos de 20×3), se pinta solo `MAGI 18×2`.

### 6.4 Archivos estrecho (80×22)

```
┌ MAGI · ARCHIVOS ─── local ⇄ hetzner-01 ── [remoto] ⇥ ┐
│  /var/www/cooperapp                                   │
│  ───────────────────────────────────────────────────  │
│    ..                                                 │
│    app/                 —   12 sep                    │
│  ▸ main.py           14 kB  12 sep  ≠                 │
│    config.yaml        1 kB  12 sep  ≠                 │
│  ───────────────────────────────────────────────────  │
│  5 elementos · 1.1 MB  ·  ⇄ 78 % main.py              │
├───────────────────────────────────────────────────────┤
│ ⇥ local  ↵ abrir  c copiar  m mover  x borrar  ? más  │
└───────────────────────────────────────────────────────┘
```

La cabecera indica qué panel se ve (`[remoto]`); la cola se resume en el pie cuando no hay filas para sus 3 líneas.

### 6.5 Diálogo MAGI compacto (70×18)

```
   ┌─ DELIBERACIÓN MAGI ─ reiniciar nginx → 3 ───────┐
   │               M-1 salud  B-2 backup  C-3 tests  │
   │ hetzner-01    ✓          ✓ 3 h       ✓          │
   │ hetzner-02    ✓          ✕ 31 h      ✓          │
   │ vps-openclaw  ✓          —           —          │
   │ CONSENSO 6/7 ████████░░  ▸ BLOQUEADO            │
   │ f forzar   esc cancelar                ↑↓ 1/3   │
   └─────────────────────────────────────────────────┘
```

Los nombres MAGI se abrevian (`M-1`, `B-2`, `C-3`) y los datos se recortan; los hosts se desplazan con `↑` `↓` si no caben.

### Notas de UX y diseño

- Puntos de corte: **estrecho** si ancho < 100; **columnas mínimas** si ancho < 80; **bajo** si alto < 20. Una sola tabla en `ui/disposicion.rs`.
- Mínimo global 40×12; cada vista declara el suyo (tabla §7.2). Nunca se pinta una vista por debajo de su mínimo.
- Tras aplicar un tamaño, la fila seleccionada sigue visible y el desplazamiento no deja huecos al final.
- El cambio de modo no pierde estado: selección, marcados de Archivos, filtro, panel activo, texto a medio escribir en diálogos.
- Sin parpadeos: se agrupan los eventos y se limpia la pantalla una sola vez por tamaño aplicado.

---

## 7. Lógica de Negocio

### 7.1 Agrupación de eventos

```python
AGRUPACION = 0.050   # s

def al_evento_resize(geo, cols, filas, ahora):
    geo.pendiente = (cols, filas)
    geo.ultimo_evento = ahora

def en_tick(geo, ahora):
    if geo.pendiente and ahora - geo.ultimo_evento >= AGRUPACION:
        tam, geo.pendiente = geo.pendiente, None
        if tam != geo.aplicado:
            aplicar_tamano(tam)          # resize + clear + disposición + pintar + Redimensionar
            geo.aplicado = tam
```

El bucle debe despertarse por el tick aunque no haya teclas (con pendiente, cada ≤ 16 ms). Es probablemente parte del fallo 1: un bucle que solo pinta al recibir una tecla.

### 7.2 Mínimos por vista

| Vista | Mínimo (cols × filas) | Modo estrecho (< 100 cols) |
|---|---|---|
| Global (cualquier vista) | 40×12 | — |
| Flota | 40×12 | Un panel, `Tab` alterna lista / detalle |
| Hosts y Sesiones | 40×12 | Oculta dirección (< 80) y usuario·puerto (< 60) |
| Ficha de host | 50×14 | Etiquetas encima de los campos; desplazamiento vertical |
| Sesión | 40×8 (sustituye al global) | Pestañas y barra compactas; el PTY recibe el área real |
| Archivos | 50×14 | Un panel, `Tab` alterna local / remoto; cola resumida en el pie |
| Transferencias, Túneles, Registro, Identidades, Snippets | 50×12 | Columnas por prioridad; detalle plegado si alto < 20 (se abre con `↵`; en Snippets con `i`, porque `↵` ejecuta) |
| Resultados | 60×14 | Salida solo con `↵` si alto < 24 |
| Diálogo MAGI | 50×12 | Nombres abreviados, hosts con desplazamiento |

### 7.3 Columnas por prioridad

```python
def columnas_visibles(ancho, columnas):      # columnas = [(nombre, ancho_min, prioridad)], 1 = imprescindible
    visibles = sorted(columnas, key=lambda c: c.prioridad)
    while sum(c.ancho_min for c in visibles) + separadores(visibles) > ancho and len(visibles) > 1:
        visibles.pop()                       # quita la de menor prioridad
    return [c for c in columnas if c in visibles]   # conserva el orden original
```

Cada tabla declara sus columnas con prioridad; el nombre (o identificador principal) y el glifo de estado son siempre prioridad 1.

### 7.4 Tamaño del PTY

`alto_pty` (F3) sigue siendo el único cálculo del alto remoto y debe usarse con el **tamaño aplicado**, nunca con uno guardado al abrir la pestaña. El ancho es el del área de la pestaña. Todo tamaño se sanea a 2×1 como mínimo (T28). En modo por debajo del mínimo de Sesión, el remoto recibe igualmente su tamaño real saneado.

### 7.5 Casos especiales

- **Vuelta del visor (F4) o de cualquier suspensión:** se lee el tamaño real y se aplica como un Resize (el terminal pudo cambiar mientras tanto).
- **Relanzado del servidor (F3):** al readjuntar pestañas se envía el tamaño aplicado en `Adjuntar`.
- **Cambio de pestaña:** la pestaña que se adjunta recibe el tamaño aplicado; la que se desadjunta deja de contar para el mínimo.
- **Diálogo abierto durante el cambio:** se recoloca y conserva el texto y el foco; si deja de caber, pasa a desplazamiento.
- **Aviso de tamaño y diálogos (implementación):** diálogos, paleta y ayuda se pintan encima del aviso y conservan sus teclas; la paleta no ejecuta una entrada que no se ve; una deliberación tapada por el aviso solo admite `Esc` (también en Sesión); en la ficha con cambios sin guardar, `q` se ignora (salir con `F1`-`F8`, que confirman). El aviso dice siempre qué vista o diálogo exige el mínimo.
- **Ida y vuelta dentro de la agrupación:** si la ráfaga termina en el tamaño ya aplicado pero pasó por otro, se limpia y se repinta una vez, sin `Redimensionar`.
- **Pestaña compartida (implementación):** cada ventana recibe su propio `Redimensionada` con la ventana que explica **su** relleno (`ventana_minima_para`); la que se adjunta lo recibe aunque el tamaño no cambie; la tarea de lectura del cliente aplica el tamaño al parser en orden con los `Datos`; `PantallaCompleta` declara el tamaño del parser del que sale el volcado.
- **Ficha con foco en un campo que queda fuera:** el desplazamiento lleva el campo con foco a la vista.
- **Ventanas muy altas o anchas (≥ 200×60):** los paneles no crecen sin límite: el detalle de Flota y los diálogos tienen ancho máximo y se centran.

---

## 8. Requisitos No Funcionales

**Seguridad** — sin cambios.

**Protección de datos (RGPD)** — sin cambios.

**Backups y recuperación** — sin cambios.

**Rendimiento**

- Del último evento de tamaño al repintado: ≤ 70 ms (50 de agrupación + pintado).
- Un único `clear` y un único `Redimensionar` por tamaño aplicado, aunque lleguen 30 eventos en una animación de Hyprland.
- El tick de 16 ms solo corre mientras hay un tamaño pendiente.

**Accesibilidad** — el modo estrecho mantiene la doble codificación glifo + color y todas las acciones por teclado; el aviso de tamaño mínimo dice qué tamaño hace falta; útil también para zoom de fuente grande.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Hyprland | Cambios por mosaico, arrastre, `togglefloating`, pantalla completa; animaciones que generan ráfagas |
| Alacritty | Zoom de fuente (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`) cambia columnas y filas |
| Host remoto | `window_change` (RFC 4254) → SIGWINCH en el proceso remoto |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D76 — Diagnóstico antes que arreglo.** Las dos funcionalidades estaban hechas y validadas en la Fase 1; se reproduce el fallo con un test y se documenta la causa antes de tocar nada, para no parchear el síntoma.
- **D77 — Tubería única con agrupación de 50 ms.** Un solo punto aplica el tamaño; descartado aplicar cada evento (parpadeo y ráfagas de `window_change` durante las animaciones de Hyprland).
- **D78 — Ninguna vista guarda tamaños entre pintados.** Todo se deriva del área en cada pintado; la única copia es la disposición del último frame, que usan las teclas de página. Evita la clase entera de fallos «tamaño viejo».
- **D79 — Disposición adaptable por puntos de corte comunes** (100 / 80 columnas, 20 filas) en `ui/disposicion.rs`, en lugar de reglas por vista dispersas.
- **D80 — Aviso por debajo del mínimo en lugar de pintar recortado.** Una pantalla rota confunde más que un aviso que dice qué tamaño hace falta; el remoto sigue recibiendo su tamaño real.
- **D81 — Se mantiene la regla del mínimo entre ventanas de la Fase 3** para pestañas compartidas; descartado seguir a la ventana con foco (dos ventanas se pelearían por el tamaño).
- **D82 — Pruebas de pintado con `TestBackend` e instantáneas `insta`.** Es la única manera de que una regresión de interfaz la detecte `cargo test` y no Hector.
- **D88 — (implementación) No se pinta con un tamaño pendiente;** `aplicar_tamano` usa `terminal.resize` (que ya limpia) y no añade otro `clear`. El `timeout` del bucle se crea dentro del runtime de tokio.
- **D89 — (implementación) App separada en `construir` (estado) y efectos;** `App::de_prueba` no lanza hilos ni conecta, y el bucle real (`ciclo`, `paso`) es genérico sobre `Backend` para ejercerlo en pruebas.
- **D90 — (implementación) Instantáneas deterministas sin reloj inyectable:** fechas fijas con `TZ=UTC`, relativas lejos de redondeos, filtro de insta para «hace N s» y ruta temporal de longitud fija.
- **D91 — (implementación) Un solo degradador ASCII (`disposicion::texto_ascii`)** que conserva letras con tilde y quita `¿`/`¡`.
- **D92 — (implementación) Barrido de tamaños más allá de los tres del checklist:** anchos {1…200} × altos {1…60} en Unicode y ASCII, sin pánico, con el tamaño nuevo y la selección a la vista.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Diagnóstico y tubería | Reproducir los dos fallos con tests, documentar la causa, `Geometria` con agrupación, `aplicar_tamano`, tick, visor y relanzado, `Redimensionar` en Sesión, servidor de pruebas que registra `window_change` | 0,5 semanas |
| S2 — Disposición adaptable | `ui/disposicion.rs`, mínimos y aviso, modo estrecho en todas las vistas, columnas por prioridad, barras compactas, diálogos/paleta/ayuda/MAGI que encogen, recorte de selección y desplazamiento | 1 semana |
| S3 — Pruebas y cierre | Instantáneas a 40×12, 80×24 y 200×60 por vista, secuencias de cambio, revisión adversarial (T31), README | 0,5 semanas |

**Total: 3 sprints, ~2 semanas.** (Real: una sesión de Claude Code el 27 sep 2026; 178 pruebas nuevas y 128 instantáneas.) Supuesto: la Fase 6 está implementada antes (sus vistas entran en el alcance); si no lo está, sus vistas quedan pendientes y se anota.

---

## 12. Conexiones con Otras Fases

- **F1:** restaura «Redimensionado de la terminal repinta la vista actual» y «Redimensionar la terminal envía `window_change` al remoto y reajusta el parser» (regresión R36).
- **F3:** usa `Redimensionar`, `Redimensionada`, `alto_pty` y la regla del mínimo entre ventanas sin cambiar el protocolo.
- **F4:** la vuelta del visor aplica el tamaño real.
- **F6:** Snippets, Resultados y el diálogo MAGI entran en la revisión.
- **F8 (Archivos, segunda vuelta), F9 (Sincronización cifrada) y F10 (Android):** toda vista nueva debe declarar su mínimo, usar `disposicion.rs` y tener instantáneas a los tres tamaños (T45).
