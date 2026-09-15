# MAGI - Fase 3: Pestañas y Servidor de Sesiones

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 15 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Especificada sobre una Fase 2 con implementación reportada pero **sin validación manual** (riesgo R15 en el maestro).

---

## 1. Visión General

La Fase 1 dejó una sesión SSH embebida; la Fase 2 la convirtió en la base del sondeo. La Fase 3 hace dos cosas que van juntas: **varias sesiones a la vez con pestañas** y **un servidor local que las custodia**, de modo que cualquier terminal en la que se abra `magi` ve las mismas sesiones, puede escribir en ellas y abrir otras nuevas, y cerrar una ventana no cierra nada. Es el mismo modelo que el servidor de tmux, con la diferencia de que lo que hay dentro son conexiones russh, no shells locales.

Esto resuelve la decisión abierta D1 del informe de diseño («¿multiplexado propio o delegado?») como **propio**: MAGI no depende de tmux ni de zellij. El coste es un proceso más y un protocolo entre cliente y servidor; el beneficio es que las sesiones sobreviven a la ventana, que dos terminales pueden compartir una pestaña y que el sondeo y las fases siguientes (SFTP, túneles, snippets) reutilizan conexiones que ya existen aunque la TUI que las abrió se haya cerrado.

**Objetivos principales**

1. Proceso servidor (`magi --servidor`) autolanzado por el primer cliente, con socket Unix en el directorio de ejecución del usuario, que custodia conexiones russh, pantallas `vt100` y el `RegistroSesiones`, y se apaga solo cuando no quedan sesiones ni clientes.
2. Protocolo JSON por líneas con saludo versionado; el cliente TUI conserva inventario, Flota, identidades y registro; el servidor solo sesiones.
3. Pestañas en la vista Sesión: barra siempre visible, prefijo + `1`-`9` / `n` / `p` / `l`, varias sesiones al mismo host, reutilización de conexión con `multiplexar`, cierre por `exit`, reconexión tras caída, indicadores `●` `◐` `✕`.
4. La misma pestaña en varias ventanas: todas ven y escriben; el tamaño remoto es el mínimo de las ventanas adjuntas.
5. Vista Sesiones (F3) como lista cuando no hay pestaña activa; `●N` en Hosts y Flota; nueva ventana con prefijo + `w`.
6. Manejo de caída del servidor: pestañas marcadas, relanzado, evento en REGISTRO.

**Contexto.** Es la fase de mayor riesgo técnico del proyecto (el informe de diseño la situaba en tercer lugar por eso). Ya no hay que escribir un emulador (Fase 1), pero sí un demonio con su ciclo de vida, su protocolo y su recuperación. Sin tabla nueva: las sesiones viven en memoria del servidor.

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin cambios de crates principales (Rust stable, ratatui 0.30, crossterm 0.29, tokio, russh 0.63, tui-term 0.3, vt100 0.16, rusqlite 0.40). Novedades:

| Capa | Tecnología | Notas |
|---|---|---|
| Socket | `tokio::net::UnixListener` / `UnixStream` | `$XDG_RUNTIME_DIR/magi/servidor.sock` (macOS: `~/Library/Caches/magi/servidor.sock`), directorio 700 |
| Protocolo | `serde_json`, un mensaje por línea (`\n`), `tokio_util::codec::LinesCodec` | Bytes del terminal en base64 (`base64` crate) |
| Demonización | `std::process::Command` + `setsid` (`nix` crate, `libc::setsid`) | El primer cliente lanza `magi --servidor` desacoplado, stdio al log |
| Bloqueo del servidor | Fichero `servidor.lock` con `flock` (`nix`) | Evita dos servidores; si el socket existe pero no responde, se borra y se relanza |
| Registro | `registro::anotar` desde el servidor | Tipos nuevos: `sesion_cerrada`, `sesion_reconectada`, `servidor_arrancado`, `servidor_detenido`, `servidor_caido`; y `sondeo_recuperado` (arreglo R12: el sondeo anota solo transiciones) |
| SQLite multi-proceso | `busy_timeout = 5000 ms`, WAL | Un escritor por proceso (cliente y servidor); T18 se reformula |

### Estructura de carpetas (novedades)

```
src/
├── main.rs                  + --servidor · servidor estado|parar · conectar <host>
├── protocolo.rs             tipos de mensaje (serde), VERSION_PROTOCOLO = 1, codificación por líneas
├── servidor/
│   ├── mod.rs               arranque, socket, lock, bucle de aceptación, apagado por inactividad
│   ├── sesiones.rs          RegistroSesiones a N: Sesion {id, host_id, nombre, estado, conexion, canal, parser, tamano, adjuntos}
│   ├── conexiones.rs        pool host_id → Arc<Handle>; reutilización si multiplexar; cierre sin canales
│   ├── cliente_remoto.rs    un cliente TUI adjunto: suscripciones, tamaño, solicitante de diálogos
│   └── difusion.rs          Datos/Estado/Sesiones a los clientes adjuntos
├── cliente/
│   ├── mod.rs               conexión al socket, autolanzado del servidor, saludo, reconexión
│   ├── pantallas.rs         un vt100::Parser por pestaña en el cliente; snapshot al adjuntar
│   └── ventana.rs           lanzar nueva ventana de terminal ([terminal] comando)
├── conexion/                (F1/F2) cliente.rs, salto.rs, huellas.rs pasan a usarse desde servidor/
├── flota/mod.rs             sondeo sobre sesión viva pide Ejecutar al servidor
└── ui/
    ├── sesion.rs            barra de pestañas, prefijo ampliado, estados por pestaña
    └── sesiones.rs          vista F3 sin pestaña activa: lista
docs/fase3/
├── magi-fase3-informe.md · magi-fase3-prompt.md · magi-fase3-checklist.md
└── magi-fase3-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 ventana 1 (alacritty)                 ventana 2 (alacritty)
 ┌──────────────────────┐              ┌──────────────────────┐
 │ magi (cliente TUI)   │              │ magi (cliente TUI)   │
 │  ui/ · inventario ·  │              │  ui/ · flota ·       │
 │  flota · identidades │              │  registro …          │
 │  cliente/pantallas   │              │  cliente/pantallas   │
 │   (vt100 por pestaña)│              │   (vt100 por pestaña)│
 └───┬──────────────┬───┘              └───┬──────────────┬───┘
     │ SQLite       │ JSON/líneas          │ JSON/líneas  │ SQLite
     │ (lectura +   │ Unix socket          │              │ (lectura +
     │  escrituras  ▼                      ▼              │  escrituras
     │  propias)  ┌──────────────────────────────┐        │  propias)
     │            │ magi --servidor              │        │
     │            │  servidor/sesiones.rs        │        │
     │            │   Sesion 1 hetzner-01 ───────┼─────┐  │
     │            │   Sesion 2 hetzner-01 (2) ───┼──┐  │  │
     │            │   Sesion 3 vps-openclaw ─────┼┐ │  │  │
     │            │  servidor/conexiones.rs      ││ │  │  │
     │            │   hetzner-01  Arc<Handle> ◀──┼┼─┴──┘  │   (multiplexar: 2 canales, 1 conexión)
     │            │   vps-openclaw Arc<Handle> ◀─┼┘       │
     │            │  vt100::Parser por sesión    │        │
     │            │  registro::anotar (sesiones) │        │
     │            └──────────────┬───────────────┘        │
     │                           │ russh                   │
     ▼                           ▼                         ▼
 ~/.local/share/magi/magi.db   hosts remotos      $XDG_RUNTIME_DIR/magi/
 (WAL, busy_timeout 5 s)                            servidor.sock · servidor.lock
```

**Reparto.** El servidor custodia sesiones y conexiones y anota en REGISTRO solo los eventos de sesión y de servidor. Todo lo demás (inventario, Flota, identidades, registro, importación/exportación, llavero) sigue en el cliente. El sondeo, cuando hay sesión viva, pide al servidor que ejecute el script (`Ejecutar`); si no la hay, abre su conexión efímera como en la Fase 2.

**Diálogos.** El servidor nunca dialoga (T15). Cuando una conexión necesita una decisión (huella, frase, contraseña), el servidor la envía al cliente **solicitante** (el que pidió `AbrirSesion` o `Reconectar`); si ese cliente se desconecta antes de responder, la apertura se cancela. Los demás clientes ven la pestaña en estado «esperando decisión en otra ventana».

---

## 3. Modelo de Datos

Sin tablas nuevas. Las sesiones viven en memoria del servidor; REGISTRO añade cinco tipos. El E-R de la Fase 2 no cambia.

### Estructuras en memoria del servidor

```
 ┌───────────────────────────────┐        ┌──────────────────────────┐
 │ Sesion                        │        │ Conexion                 │
 │ id            u32 (creciente) │   N:1  │ host_id                  │
 │ host_id                       │───────>│ handle   Arc<Handle>     │
 │ nombre        «hetzner-01 (2)»│        │ canales  u32             │
 │ estado        (ver estados)   │        │ abierta_en               │
 │ identidad     descripción     │        │ multiplexable  bool      │
 │ canal         russh Channel   │        └──────────────────────────┘
 │ parser        vt100::Parser   │
 │ tamano        (cols, filas)   │        ┌──────────────────────────┐
 │ adjuntos      [cliente_id]    │   N:M  │ Cliente                  │
 │ solicitante   Option<cliente> │<──────>│ id · pid · version       │
 │ abierta_en · ultima_actividad │        │ tamano por sesión adjunta│
 │ actividad_no_vista  bool      │        │ suscrito_lista  bool     │
 └───────────────────────────────┘        └──────────────────────────┘
```

### Diagrama de estados: sesión (servidor)

```
  AbrirSesion ─▶ abriendo ─(resolviendo · conectando · verificando_huella · autenticando,
                │           como en F1; los diálogos van al solicitante)
                │ error / cancelada ─▶ fallida ─▶ (se elimina; REGISTRO conexion_fallida)
                ▼
             abierta ◀────────────────────────────────┐
              │  │                                    │ Reconectar (nueva conexión + pty)
              │  │ exit remoto ─▶ cerrada ─▶ eliminada│ REGISTRO sesion_reconectada
              │  │   (REGISTRO sesion_cerrada)        │
              │  │ Cerrar (x) ─▶ cerrada ─▶ eliminada │
              │  └ red caída / EOF inesperado ─▶ caida ┘
              │                                    │ Cerrar (x) ─▶ eliminada
              │ (por cliente) Adjuntar / Desadjuntar: no cambia el estado de la sesión,
              │  solo la lista de adjuntos y el tamaño mínimo
              ▼
  Transiciones inválidas: caida → abierta sin Reconectar; abierta → abriendo;
  eliminada → cualquiera (el id no se reutiliza); Reconectar sobre abierta (se rechaza).
```

### Diagrama de estados: servidor

```
  (no existe) ──primer cliente ejecuta magi──▶ lanzando (magi --servidor, setsid, lock)
        ▲                                          │ socket listo (< 2 s)
        │                                          ▼
        │                                     sirviendo ◀─────────────┐
        │                                      │ clientes=0 y         │ llega un cliente
        │                                      │ sesiones=0           │ o se abre sesión
        │                                      ▼                      │
        │                             inactivo (gracia 10 s) ─────────┘
        │                                      │ sigue vacío
        │                                      ▼
        └──────── borra socket y lock ◀── apagándose (REGISTRO servidor_detenido)

  Caída (panic): los clientes reciben EOF ─▶ marcan pestañas «servidor caído», anotan servidor_caido,
  ofrecen relanzar. `magi servidor parar` ─▶ cierra sesiones (REGISTRO sesion_cerrada) y apaga.
  Inválida: dos servidores a la vez (el lock lo impide); cliente y servidor con VERSION_PROTOCOLO distinta.
```

### Diagrama de estados: pestaña (cliente)

```
  [sin pestaña] ─ Adjuntar ─▶ adjunta ─(Datos → parser → Pantalla)─▶ visible / no visible
                                │ estado de sesión = caida ─▶ ✕ (prefijo r reconecta)
                                │ sesión eliminada ─▶ pestaña desaparece (foco a la anterior)
                                │ EOF del socket ─▶ ✕ servidor caído (todas)
  Indicadores: ● abierta y visible · ◐ abierta, no visible, con actividad nueva · ✕ caída
```

---

## 4. Flujos de Trabajo

### 4.1 Arranque del cliente y autolanzado del servidor

```
  magi
    ▼
  conectar a $XDG_RUNTIME_DIR/magi/servidor.sock
    ├─ ok ─▶ Hola{v=1,pid} ─▶ Bienvenida{v, sesiones[]} ─▶ TUI (Flota) con la lista de sesiones
    │                       └─ VersionIncompatible ─▶ mensaje «servidor de otra versión; magi servidor parar» ─▶ TUI sin sesiones
    ├─ no existe ─▶ lanzar `magi --servidor` (setsid, stdio al log) ─▶ esperar socket ≤ 2 s ─▶ (ok)
    │                                                                 └─ no aparece ─▶ TUI sin sesiones + error en barra
    └─ existe pero no responde (ECONNREFUSED) ─▶ ¿lock libre? ─sí─▶ borrar socket ─▶ lanzar
                                                              └─no─▶ esperar 1 s y reintentar (3 veces) ─▶ error
```

### 4.2 Abrir una sesión (pestaña)

```
  ↵ en Hosts/Flota · prefijo c (paleta filtrada a hosts) · magi conectar <host>
    ▼
  cliente: AbrirSesion{host_id, cols, filas}
    ▼
  servidor: ¿conexión viva al host y host.multiplexar? ─sí─▶ canal nuevo sobre Arc<Handle>
                                                     └─no─▶ conexión nueva (flujo F1: salto, huella, auth)
                                                             diálogos → solicitante:
                                                               HuellaDesconocida/Cambiada → DecisionHuella
                                                               PideFrase → Frase · PideContrasena → Contrasena
    │ error ─▶ SesionFallida{motivo} ─▶ barra del solicitante · REGISTRO conexion_fallida ─▶ fin
    ▼
  pty (cols, filas del solicitante) + shell ─▶ Sesion{id, nombre «host» o «host (n)»}
    ▶ REGISTRO conexion_abierta · Sesiones{lista} a todos los clientes
    ▼
  solicitante: Adjuntar{id} automático ─▶ PantallaCompleta ─▶ vista Sesión con la pestaña activa
```

### 4.3 Adjuntar la misma pestaña desde otra ventana

```
  ventana 2: F3 (lista) ─▶ ↵ sobre la sesión ─▶ Adjuntar{id, cols, filas}
    ▼
  servidor: adjuntos += cliente2 · tamaño = (min cols, min filas) de los adjuntos
            ¿cambió? ─sí─▶ window_change al remoto · parser.set_size
    ▼
  PantallaCompleta{id, dump del parser} a ventana 2 ─▶ ambas ventanas reciben Datos a partir de ahí
  ambas envían Teclas; la última que escribe no bloquea a la otra (sin arbitraje, como tmux)
    ▼
  Desadjuntar (prefijo q, cambiar de vista, cerrar ventana) ─▶ recalcular tamaño ─▶ window_change si cambia
```

### 4.4 Caída y reconexión

```
  red caída / EOF del canal ─▶ servidor: estado=caida · Estado{id, caida, motivo} a adjuntos · REGISTRO sesion_cerrada(motivo)
    ▼
  pestaña ✕ «caída: <motivo> · prefijo r reconecta · prefijo x cierra»
    ▼
  prefijo r ─▶ Reconectar{id} ─▶ servidor: nueva conexión (misma identidad; diálogos al solicitante)
                                   · parser nuevo · mismo id y nombre ─▶ abierta · REGISTRO sesion_reconectada
```

### 4.5 Caída del servidor

```
  socket EOF sin Adios ─▶ cliente: todas las pestañas ✕ «servidor caído» · REGISTRO servidor_caido
    ▼
  [diálogo] «El servidor de sesiones ha caído; las sesiones se han perdido. ¿Relanzar? s/n»
    s ─▶ lock huérfano: borrar · lanzar `magi --servidor` · saludo · pestañas vacías
    n ─▶ TUI sigue sin sesiones; F3 muestra la lista vacía con el aviso
```

### 4.6 Diagrama de secuencia: abrir y compartir

```
 ventana1     servidor          host        ventana2
    │──AbrirSesion──▶│                          │
    │◀─HuellaDesc.───│ (solicitante = v1)       │
    │──DecisionHuella▶│──conecta·pty·shell──▶│  │
    │◀─Sesiones[1]───│──Sesiones[1]────────────▶│
    │ (Adjuntar auto)│                          │
    │◀─PantallaCompl.│                          │
    │──Teclas────────▶│──data──────────────▶│    │
    │◀─Datos─────────│◀─data──────────────│     │
    │                │◀──────────Adjuntar{1}────│
    │◀─Redimension.──│ (min tamaño) ──window_change──▶ host
    │                │───PantallaCompleta──────▶│
    │◀─Datos─────────│──Datos──────────────────▶│  (ambas reciben)
    │                │◀──────────Teclas─────────│
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi --servidor` | Ejecuta el servidor en primer plano (lo usa el autolanzado con `setsid`; sirve para depurar) |
| `magi servidor estado` | Imprime pid, versión de protocolo, sesiones abiertas y clientes conectados |
| `magi servidor parar` | Cierra todas las sesiones y apaga el servidor (confirmación si hay sesiones, `--si` para omitirla) |
| `magi conectar <host>` | Abre la TUI directamente con una sesión nueva a ese host |

### 5.2 Protocolo (mensajes)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `Hola{version, pid}` | `Bienvenida{version, sesiones}` / `VersionIncompatible{version}` |
| `AbrirSesion{host_id, cols, filas}` | `Sesiones{lista}` (difusión en cada cambio) |
| `Adjuntar{sesion_id, cols, filas}` / `Desadjuntar{sesion_id}` | `PantallaCompleta{sesion_id, bytes_b64, cols, filas}` |
| `Teclas{sesion_id, bytes_b64}` | `Datos{sesion_id, bytes_b64}` |
| `Redimensionar{sesion_id, cols, filas}` | `Redimensionada{sesion_id, cols, filas}` |
| `Cerrar{sesion_id}` / `Reconectar{sesion_id}` | `Estado{sesion_id, estado, motivo}` |
| `DecisionHuella{sesion_id, decision}` / `Frase{sesion_id, frase}` / `Contrasena{sesion_id, contrasena, recordar}` | `HuellaDesconocida{…}` / `HuellaCambiada{…}` / `PideFrase{…}` / `PideContrasena{…}` (solo al solicitante) |
| `Ejecutar{host_id, comando}` (sondeo) | `Ejecutado{host_id, salida, codigo}` / `SinSesion{host_id}` |
| `Listar` / `Adios` | `Error{mensaje}` |

Secretos (`Frase`, `Contrasena`) viajan por el socket del propio usuario (directorio 700) y no se registran en el log de ningún lado.

### 5.3 Vista Sesión (prefijo `Ctrl+]`, configurable)

| Tecla | Acción |
|---|---|
| prefijo · `1`-`9` | Ir a la pestaña N |
| prefijo · `n` / `p` | Pestaña siguiente / anterior |
| prefijo · `l` | Lista de sesiones (vista F3) |
| prefijo · `c` | Nueva sesión: paleta filtrada a «conectar · <host>» |
| prefijo · `x` | Cerrar la pestaña actual (confirmación) |
| prefijo · `r` | Reconectar la pestaña caída |
| prefijo · `w` | Abrir una ventana nueva de terminal con `magi` |
| prefijo · `q` / `Esc` | Volver a la vista anterior (las pestañas siguen en el servidor) |
| prefijo · prefijo | Enviar el prefijo literal al remoto |

### 5.4 Vista Sesiones (F3 sin pestaña activa)

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover selección |
| `↵` | Adjuntar y entrar en la pestaña |
| `x` | Cerrar la sesión (confirmación) |
| `r` | Reconectar si está caída |
| `n` | Nueva sesión (paleta filtrada a hosts) |
| `S` | Apagar el servidor (confirmación con recuento de sesiones) |

### 5.5 Otras vistas y paleta

| Dónde | Novedad |
|---|---|
| Hosts y Flota | `↵` abre siempre una sesión nueva (ya no pregunta por cerrar la anterior); glifo `●N` si hay más de una sesión al host |
| Global `q` | Salir ya no pide confirmación por sesiones vivas: muestra «N sesiones siguen abiertas en el servidor» |
| Paleta | `nueva ventana`, `cerrar sesión · <pestaña>`, `reconectar · <pestaña>`, `apagar servidor` |
| `config.toml` | `[terminal] comando = "alacritty -e magi"` (macOS por defecto `open -a Terminal magi`); `[servidor] gracia_apagado_seg = 10` |

---

## 6. Interfaz de Usuario

### Mapa de navegación (estado tras la Fase 3)

```
              F1 FLOTA ──↵──┐        F2 HOSTS ──↵──┐
                            ▼                      ▼
                    ┌────────────────── F3 SESIÓN (pestaña activa) ──────────────────┐
                    │ prefijo 1-9 n p   cambiar   · prefijo c nueva · prefijo x cerrar │
                    │ prefijo r reconectar · prefijo w ventana nueva · prefijo q volver │
                    └──────────────────────────┬────────────────────────────────────────┘
                                   prefijo l   │   ▲ ↵
                                               ▼   │
                    ┌────────── F3 SESIONES (lista, sin pestaña activa) ───────────────┐
                    │ ↵ entrar · x cerrar · r reconectar · n nueva · S apagar servidor  │
                    └──────────────────────────────────────────────────────────────────┘
              F5 IDENTIDADES · F7 REGISTRO (F2)     Reservadas: F4 Archivos · F6 Snippets
              Otra ventana: `magi` se conecta al mismo servidor y ve las mismas pestañas
```

### 6.1 Sesión con pestañas

```
┌ MAGI · SESIÓN ──────────────────────────────────────────────┐
│ ▸● hetzner-01 │ ◐ hetzner-01 (2) │ ✕ vps-openclaw │ + nueva │
├─────────────────────────────────────────────────────────────┤
│ root@hetzner-01:~# journalctl -u nginx -f                   │
│ sep 15 15:02:11 hetzner-01 nginx[812]: started              │
│                                                             │
│                                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ ● 1/3 · conectado 18 m · ed25519 (agente) · carga 2.7 · 2 ventanas · ^] ? ayuda │
└─────────────────────────────────────────────────────────────┘
```

La barra de estado añade la posición (`1/3`) y cuántas ventanas tienen adjunta la pestaña. Cuando el tamaño remoto es menor que la ventana (otra ventana más pequeña la comparte), la zona sobrante se rellena con `░` tenue y la barra indica `120×40 (mín. ventana 2)`.

### 6.2 Sesiones (lista)

```
┌ MAGI · SESIONES ───────────────────── 3 sesiones · 2 ventanas ─┐
│                                                                │
│  ESTADO  PESTAÑA          HOST          TIEMPO  IDENT.  VENT.  │
│  ────────────────────────────────────────────────────────────  │
│▸ ●       hetzner-01       hetzner-01    18 m    agente  2      │
│  ◐       hetzner-01 (2)   hetzner-01    4 m     agente  1      │
│  ✕       vps-openclaw     vps-openclaw  caída   fichero 0      │
│                                                                │
│  ────────────────────────────────────────────────────────────  │
│  vps-openclaw · caída hace 2 min: conexión cerrada por el remoto│
│  servidor pid 41022 · protocolo 1 · desde 14:31                │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ ↵ entrar   n nueva   r reconectar   x cerrar   S apagar        │
└────────────────────────────────────────────────────────────────┘
```

### 6.3 Pestaña caída

```
┌ MAGI · SESIÓN ──────────────────────────────────────────────┐
│ ● hetzner-01 │ ● hetzner-01 (2) │ ▸✕ vps-openclaw │ + nueva │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│          ✕ Sesión caída hace 2 min                          │
│            conexión cerrada por el remoto                   │
│                                                             │
│            ^] r  reconectar        ^] x  cerrar             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ ✕ 3/3 · caída · fichero · ^] r reconectar                   │
└─────────────────────────────────────────────────────────────┘
```

Se conserva la última pantalla en gris detrás del aviso, para poder leer qué pasó.

### 6.4 Diálogo de servidor caído

```
        ┌─ SERVIDOR CAÍDO ─────────────────────────────────┐
        │  ✕ El servidor de sesiones ha dejado de responder │
        │    3 sesiones se han perdido.                    │
        │    Detalle en el registro y en logs/servidor.log │
        │                                                  │
        │  s relanzar servidor        n seguir sin sesiones│
        └──────────────────────────────────────────────────┘
```

### 6.5 Hosts con varias sesiones

```
│  ▼ 4d3 · producción                                    (4)  │
│      ●2 hetzner-01     95.217.x.x        root    22         │
│    ▸ ●  hetzner-02     95.217.x.x        root    22         │
```

### Notas de UX y diseño

- La barra de pestañas ocupa una fila siempre; el nombre se trunca a 14 caracteres con `…` y, con más de 9 pestañas, las que no caben se indican con `‹ ›` y se alcanzan con `n`/`p` o la lista.
- `◐` aparece en una pestaña no visible cuando llegan datos; se limpia al entrar en ella.
- Entrar en la vista Sesión adjunta la pestaña activa; salir de la vista la desadjunta (deja de recibir `Datos`, la sesión sigue). Así una ventana en Flota no gasta ancho de banda del socket.
- La reconexión conserva nombre y posición de la pestaña; la pantalla anterior se descarta al reconectar.
- `magi conectar <host>` es la forma de abrir una sesión desde un lanzador o un atajo de Hyprland.
- Con el servidor de otra versión de protocolo, la TUI funciona sin sesiones y lo dice en la barra; `magi servidor parar` y volver a arrancar lo resuelve.

---

## 7. Lógica de Negocio

### 7.1 Tamaño de una sesión compartida

```python
def tamano(sesion):
    if not sesion.adjuntos:
        return sesion.tamano                       # se conserva el último
    return (min(c.cols for c in sesion.adjuntos), min(c.filas for c in sesion.adjuntos))

# al adjuntar, desadjuntar o Redimensionar de cualquier adjunto:
nuevo = tamano(sesion)
if nuevo != sesion.tamano:
    sesion.tamano = nuevo
    canal.window_change(nuevo)                     # al remoto
    sesion.parser.set_size(nuevo)                  # servidor
    difundir(Redimensionada(sesion.id, nuevo))     # cada cliente ajusta su parser y rellena el resto
```

### 7.2 Reutilización de conexión (`multiplexar`)

```python
def abrir(host, solicitante):
    conexion = pool.get(host.id)
    if conexion and host.multiplexar and conexion.viva():
        canal = conexion.handle.channel_open_session()   # sin nueva autenticación
    else:
        conexion = conectar_f1(host, solicitante)        # salto, huella, auth (diálogos al solicitante)
        pool[host.id] = conexion if host.multiplexar else conexion_privada
        canal = conexion.handle.channel_open_session()
    conexion.canales += 1
    return canal
```

Al eliminarse una sesión, `canales -= 1`; una conexión del pool sin canales se cierra tras 30 s de gracia (para que abrir «otra pestaña al mismo host» justo después no vuelva a autenticar). Un host con `multiplexar` desmarcado abre siempre conexión propia, que se cierra con su sesión.

### 7.3 Nombres de pestaña

`nombre = host.nombre` si no hay otra sesión viva al host; si la hay, `host.nombre (n)` con el menor `n ≥ 2` libre. Renombrar el host en la ficha no renombra pestañas ya abiertas.

### 7.4 Apagado por inactividad

```python
if servidor.clientes == 0 and servidor.sesiones == 0:
    esperar(config.servidor.gracia_apagado_seg)        # 10 s por defecto
    if servidor.clientes == 0 and servidor.sesiones == 0:
        anotar("servidor_detenido"); borrar(socket, lock); salir(0)
```

Con sesiones abiertas el servidor no se apaga nunca solo: cerrar todas las ventanas deja las sesiones vivas (es el comportamiento deseado).

### 7.5 Diálogos con solicitante

Cada apertura o reconexión guarda `solicitante = cliente_id`. Los mensajes `HuellaDesconocida`, `HuellaCambiada`, `PideFrase` y `PideContrasena` se envían solo a él; los demás clientes reciben `Estado{abriendo, "esperando decisión en otra ventana"}`. Si el solicitante envía `Adios` o cae, la apertura se cancela con motivo «ventana cerrada» y se anota `conexion_fallida`. Las decisiones tienen un timeout de 5 min por si el cliente sigue vivo pero nadie responde.

### 7.6 Escritura en SQLite desde dos procesos

Cliente y servidor abren `magi.db` con `busy_timeout = 5000` y WAL. El servidor solo escribe REGISTRO (`anotar`) y `HOSTS.ultimo_estado` / `ultima_conexion_en` al abrir o fallar; el cliente, todo lo demás. Un `SQLITE_BUSY` tras 5 s se trata como error normal con mensaje. Los cambios de la ficha que afectan a una conexión ya abierta (identidad, salto, puerto) no se aplican hasta la siguiente apertura; `multiplexar` y `keepalive_seg` se leen en cada apertura.

### 7.7 Casos especiales

- Dos clientes escriben a la vez en la misma pestaña: sin arbitraje, ambos flujos llegan al remoto en orden de llegada (comportamiento de tmux).
- `magi conectar <host>` con nombre inexistente: error y salida 1.
- Reconectar un host cuya ficha ya no existe (borrado): se rechaza y la pestaña solo puede cerrarse.
- El servidor pierde el socket (borrado a mano): sigue sirviendo a los clientes ya conectados y anota un aviso; los nuevos clientes no lo encuentran, ven el lock ocupado y esperan 3 s antes de fallar con «servidor sin socket: magi servidor parar».
- Reutilización de conexión con salto: la conexión al host de salto también se comparte por el pool.
- El sondeo con `Ejecutar` sobre una conexión del pool que se está cerrando cae a conexión efímera.
- Cambio de versión de MAGI con servidor antiguo en marcha: el saludo lo detecta y la TUI lo explica; nunca se mata un servidor con sesiones sin orden explícita.

---

## 8. Requisitos No Funcionales

**Seguridad**

- Socket y lock en un directorio 700 del usuario; el socket se crea con umask 077. Sin autenticación adicional: el modelo de amenaza es el mismo que el de `ssh-agent`.
- Frases y contraseñas atraviesan el socket una vez, en memoria `Zeroizing` a ambos lados, nunca al log ni al registro. `PantallaCompleta` y `Datos` contienen la salida del remoto: solo van a clientes adjuntos.
- El servidor nunca acepta ni sustituye huellas por sí mismo (T15); solo con `DecisionHuella` del solicitante.
- Un servidor con versión de protocolo distinta se rechaza en el saludo; nunca se interpreta un mensaje desconocido (error y desconexión del cliente que lo envía).
- El servidor restaura nada del terminal (no lo tiene); un `panic` se captura, se anota en `logs/servidor.log` y el proceso sale con código 2.

**Protección de datos (RGPD)**

Sin cambios: datos del propio usuario, locales. Las pantallas de sesión solo existen en memoria de los procesos del usuario.

**Backups y recuperación**

Las sesiones no se persisten: son conexiones vivas. `magi.db` sigue siendo lo único a copiar. Tras un reinicio del sistema las sesiones desaparecen y el servidor arranca vacío con la primera TUI.

**Rendimiento**

- 20 sesiones abiertas con 3 ventanas adjuntas: latencia de eco < 20 ms en local; `Datos` se difunden sin coalescer (el cliente coalesce a ~30 fps como en F1).
- Una ventana en otra vista no recibe `Datos` (desadjuntada).
- Arranque de la TUI con servidor ya en marcha < 250 ms; autolanzado del servidor < 2 s.

**Accesibilidad**

Pestañas con glifo + texto; posición `1/3` en la barra; nada nuevo depende del ratón.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Socket Unix | `$XDG_RUNTIME_DIR/magi/servidor.sock` (fallback `/tmp/magi-<uid>/`); macOS `~/Library/Caches/magi/` |
| Terminal | `[terminal] comando` para la ventana nueva; por defecto `alacritty -e magi` (Linux) y `open -a Terminal magi` (macOS) |
| Hyprland / lanzadores | `magi conectar <host>` como comando de atajo |
| Log del servidor | `~/.local/state/magi/logs/servidor.log.<fecha>` |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D27 — Multiplexado propio con servidor local (cierra D1 del informe de diseño).** Hector pide que otra terminal vea las mismas sesiones; eso exige un proceso que las custodie. Descartado tmux (`-L magi`): obligaría a ejecutar la sesión russh en primer plano dentro de tmux y a mantener dos modelos de ventana. Descartado «cada ventana independiente»: no cumple el requisito.
- **D28 — Mismo binario, `magi --servidor` autolanzado, apagado por inactividad con gracia.** Descartado `systemd --user`: un servicio siempre encendido complica macOS y añade instalación; el autolanzado es lo que hace tmux. Descartado morir con el último cliente: perdería sesiones.
- **D29 — El servidor solo custodia sesiones; el cliente hace el resto.** Cambio mínimo sobre F1/F2: `conexion/` se mueve al servidor; inventario, Flota, identidades y registro no se tocan. Descartado el cliente fino: reescritura de `app.rs` sin beneficio funcional.
- **D30 — Parser vt100 en el servidor y en cada cliente.** El servidor lo necesita para el snapshot al adjuntar y el tamaño; el cliente para pintar sin ida y vuelta. Descartado enviar la pantalla renderizada: duplica tráfico y ata el cliente al servidor.
- **D31 — JSON por líneas con base64 para bytes.** Legible, depurable con `socat`, sin esquema binario que mantener; el tráfico de una sesión de terminal es pequeño. Descartado bincode (Hector).
- **D32 — Pestaña compartida sin arbitraje y tamaño mínimo.** Comportamiento de tmux; cualquier alternativa (bloqueo de escritura, tamaño máximo con recorte) sorprende más de lo que ayuda.
- **D33 — Los diálogos van al solicitante.** El servidor no dialoga (T15) y un diálogo difundido a todas las ventanas produciría decisiones contradictorias. Timeout de 5 min.
- **D34 — Un escritor por proceso (reformula T18).** Servidor: REGISTRO de sesión y `ultimo_estado`; cliente: el resto; `busy_timeout` 5 s. Descartado que el cliente envíe sus escrituras al servidor: multiplica mensajes sin necesidad.
- **D35 — Pool de conexiones con gracia de 30 s.** Evita reautenticar al abrir «otra pestaña al mismo host» acto seguido; con `multiplexar` desmarcado no hay pool.
- **D36 — `q` ya no confirma por sesiones vivas.** Las sesiones sobreviven a la ventana; la confirmación de F1 pierde sentido y molesta.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Servidor y protocolo | `protocolo.rs`, socket, lock, autolanzado con `setsid`, saludo versionado, apagado por inactividad, `magi --servidor`, `servidor estado/parar`, log del servidor, mover `conexion/` al servidor con solicitante y diálogos reenviados, `RegistroSesiones` a N, pool de conexiones | 1,5 semanas |
| S2 — Cliente y pestañas | `cliente/`, parser por pestaña, adjuntar/desadjuntar, barra de pestañas, prefijo ampliado, vista Sesiones (lista), caída y reconexión, tamaño mínimo, `●N`, nueva ventana, `magi conectar` | 1,5 semanas |
| S3 — Integración y robustez | Sondeo por `Ejecutar`, caída del servidor y relanzado, versión incompatible, REGISTRO nuevos tipos, `q` sin confirmación, R12 y R13 de la Fase 2, tests de protocolo y de servidor con socket temporal, README | 1 semana |

**Total: 3 sprints, ~4 semanas.** Supuestos: los tests de servidor se hacen con un socket en directorio temporal y un `sshd` efímero como en F2; `setsid` vía `nix` funciona igual en Linux y macOS. Incluye los arreglos R12 (anotar solo transiciones de CAÍDA/NOMINAL en el sondeo) y R13 (Keychain por `security-framework`) como tareas del S3.

---

## 12. Conexiones con Otras Fases

- **Fase 1 y 2:** `conexion/` pasa al servidor sin cambiar su lógica; el sondeo cambia solo la rama «sesión viva» (`Ejecutar` en vez de canal local). Comportamientos que cambian: `↵` sobre un host ya no pregunta por cerrar la sesión anterior; `q` no confirma; la barra de Sesión añade posición y ventanas.
- **Fase 4 (SFTP):** el panel doble abre su canal `sftp` sobre la conexión del pool pidiéndola al servidor (`AbrirCanal` se añadirá al protocolo); las transferencias sobreviven a cerrar la ventana.
- **Fase 5 (Túneles):** los túneles viven en el servidor (sobreviven a la ventana) y se listan por el mismo protocolo; `multiplexar` decide si comparten conexión con las pestañas.
- **Fase 6 (Snippets y deliberación):** la ejecución en varios hosts usa `Ejecutar` y el pool; la salida se muestra en pestañas.
- **Fase 7 (Sincronización) y Fase 8 (Android):** sin sesiones en el fichero sincronizado; Android no tiene servidor (sesiones propias del dispositivo).
