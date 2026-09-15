# MAGI - Fase 1: Inventario y Conexión

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 14 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

---

## 1. Visión General

MAGI es un gestor SSH de terminal (TUI) con estética de cabina técnica, operado por teclado, para Linux (Omarchy / Hyprland / Alacritty) y compatible con macOS. Cubre el terreno funcional de Termius sin cuenta ni suscripción: inventario de hosts, identidades, sesiones, SFTP, túneles y sincronización cifrada entre dispositivos.

La Fase 1 entrega el núcleo que sustituye a `~/.ssh/config`: un inventario de hosts organizado en grupos y etiquetas, una ficha de edición completa, la importación del `config` existente y la exportación a un fichero que el `ssh` del sistema puede incluir. Además, y a diferencia del plan original del informe de diseño, la conexión ya es **embebida** desde esta fase: al pulsar Intro sobre un host, MAGI abre una sesión SSH mediante la librería `russh` y renderiza el terminal remoto dentro de la propia TUI, con una sola sesión a la vez (las pestañas llegan en la Fase 3).

**Objetivos principales**

1. Inventario de hosts en SQLite con grupos plegables, etiquetas libres y filtro incremental sobre todos los campos.
2. Ficha de host en cuatro bloques (identificación, acceso, al conectar, opciones extra) con salto (`ProxyJump`), multiplexado y keepalive.
3. Importación sin pérdida de `~/.ssh/config` y exportación a `~/.ssh/magi_config` incluido por `Include`, de modo que `ssh <nombre>` siga funcionando fuera de MAGI.
4. Conexión SSH embebida con `russh`: autenticación por `ssh-agent` o clave de fichero (frase pedida en la TUI), verificación estricta de huellas contra `~/.ssh/known_hosts` y terminal remoto renderizado en la TUI.
5. Paleta de comandos (`Ctrl+P`) limitada a hosts y acciones de host.
6. Sistema visual con colores del tema activo de Omarchy, paleta fija de respaldo y modo degradado ASCII.

**Contexto.** Proyecto interno de Hector en 4d3 para uso diario sobre su Omarchy. Sin cliente externo, sin plazos contractuales. La Fase 1 se implementa con Claude Code en el repositorio `magi` en GitHub.

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Lenguaje | Rust (stable, edición 2021) | Un solo crate binario `magi`; sin crate núcleo separado (decisión D1) |
| TUI | ratatui + crossterm | Bucle de eventos propio; render a ~30 fps solo cuando hay cambios |
| Async | tokio (runtime multi-hilo) | Requerido por russh; la UI corre en el hilo principal y recibe eventos por canal |
| SSH | russh + russh-keys | Cliente embebido; agente vía `SSH_AUTH_SOCK`; claves de fichero con frase |
| Terminal remoto | tui-term + vt100 | Parser de secuencias de escape y widget para pintar la pantalla del PTY remoto |
| Almacén | rusqlite (`bundled`), WAL | `~/.local/share/magi/magi.db`, permisos 600, migraciones por `PRAGMA user_version` |
| Configuración | toml + serde, `directories` | `~/.config/magi/config.toml` (XDG; en macOS `~/Library/Application Support/magi`) |
| ssh_config | Parser propio | Conserva directivas desconocidas en `opciones_extra` |
| Búsqueda difusa | nucleo-matcher | Solo en la paleta; el filtro `/` es subcadena |
| Tema | Lectura de `~/.config/omarchy/current/theme/alacritty.toml` | Paleta fija del informe de diseño como respaldo |
| Secretos en memoria | zeroize | Frases de claves y claves descifradas se borran al cerrar la sesión |
| Logs | tracing + tracing-appender | `~/.local/state/magi/magi.log`, rotación diaria |
| Tests | `cargo test` | Parser/exportador de ssh_config con ida y vuelta; almacén con SQLite en memoria |
| Forja | GitHub | Repositorio `magi` |

### Estructura de carpetas

```
magi/
├── Cargo.toml
├── CLAUDE.md
├── README.md
├── docs/
│   ├── magi-maestro.md
│   ├── magi-fase1-informe.md
│   ├── magi-fase1-prompt.md
│   ├── magi-fase1-checklist.md
│   └── magi-fase1-implementacion.md      (lo escribe el agente)
├── src/
│   ├── main.rs              CLI (clap): magi | magi importar [ruta] | magi exportar
│   ├── app.rs               estado global, bucle de eventos, enrutado de vistas
│   ├── config.rs            config.toml (prefijo de escape, rutas, tema)
│   ├── tema.rs              lectura del tema Omarchy, paleta de respaldo, modo ASCII
│   ├── modelo.rs            structs Host, Grupo, Etiqueta, IdentidadRef, EstadoSesion
│   ├── almacen/
│   │   ├── mod.rs           apertura, PRAGMAs, permisos
│   │   ├── migraciones.rs   migraciones numeradas por user_version
│   │   ├── hosts.rs         CRUD y filtro
│   │   ├── grupos.rs
│   │   └── etiquetas.rs
│   ├── sshconfig/
│   │   ├── parser.rs        tokenizador de bloques Host / Match / Include
│   │   ├── importar.rs      config → hosts (con informe de omitidos)
│   │   └── exportar.rs      hosts → magi_config, comprobación del Include
│   ├── identidades.rs       escaneo de ~/.ssh/*.pub y del agente (SSH_AUTH_SOCK)
│   ├── conexion/
│   │   ├── mod.rs           máquina de estados de la sesión
│   │   ├── cliente.rs       russh: conectar, autenticar, canal, pty, shell, resize
│   │   ├── salto.rs         ProxyJump: sesión al host de salto + canal direct-tcpip
│   │   ├── huellas.rs       known_hosts: comprobar, aprender, reemplazar
│   │   └── terminal.rs      vt100::Parser, buffer de pantalla, tamaño
│   ├── ui/
│   │   ├── mod.rs           trait Vista, dibujado común
│   │   ├── hosts.rs         inventario
│   │   ├── ficha.rs         formulario de host
│   │   ├── sesion.rs        terminal remoto (tui-term) + barra de estado
│   │   ├── paleta.rs        Ctrl+P
│   │   ├── dialogos.rs      confirmar, huella, frase, importación, mensaje
│   │   ├── barra.rs         barra inferior de atajos
│   │   ├── ayuda.rs         ? contextual
│   │   └── componentes.rs   listas, campos, desplegables, casillas
│   └── teclas.rs            mapa de atajos y prefijo de escape
└── tests/
    ├── sshconfig_ida_vuelta.rs
    └── almacen.rs
```

### Diagrama de arquitectura

```
┌────────────────────────────── proceso magi (Rust) ──────────────────────────────┐
│                                                                                  │
│  hilo principal (UI)                          runtime tokio                     │
│  ┌───────────────┐  teclas   ┌────────────┐  comandos   ┌─────────────────────┐ │
│  │ crossterm     │──────────▶│  app.rs    │────────────▶│ conexion/           │ │
│  │ (terminal)    │◀──────────│  bucle de  │◀────────────│  cliente.rs (russh) │ │
│  └───────────────┘  frames   │  eventos   │  eventos    │  salto.rs           │ │
│                              └─┬───────┬──┘  (mpsc)     │  huellas.rs         │ │
│                                │       │                └──┬────────┬─────────┘ │
│                     render     │       │ consultas          │        │ bytes    │
│  ┌───────────────┐             │       ▼                    │        ▼          │
│  │ ui/           │◀────────────┘  ┌────────────┐            │  ┌──────────────┐ │
│  │  hosts ficha  │                │ almacen/   │            │  │ terminal.rs  │ │
│  │  sesion       │◀───pantalla────│ rusqlite   │            │  │ (vt100)      │ │
│  │  paleta       │    vt100       └─────┬──────┘            │  └──────────────┘ │
│  └───────────────┘                      │                    │                   │
│  ┌───────────────┐                      │                    │ autenticación     │
│  │ tema.rs       │                      │             ┌──────▼──────────────┐    │
│  └──────┬────────┘                      │             │ identidades.rs      │    │
└─────────┼───────────────────────────────┼─────────────┼──────┬──────────────┼────┘
          ▼                               ▼             ▼      ▼              ▼
  ~/.config/omarchy/            ~/.local/share/     SSH_AUTH_SOCK   ~/.ssh/id_*     host remoto
  current/theme/alacritty.toml  magi/magi.db (WAL)  (ssh-agent)     ~/.ssh/known_hosts  (TCP :puerto,
                                                                    ~/.ssh/config       o vía salto)
                                                                    ~/.ssh/magi_config
```

La UI nunca bloquea: cada operación de red vive en una tarea tokio y comunica su progreso por un canal `mpsc` de eventos (`Conectando`, `HuellaDesconocida`, `PideFrase`, `Abierta`, `Datos(bytes)`, `Cerrada`, `Error`). La UI responde con comandos (`AceptarHuella`, `Frase(String)`, `Teclas(bytes)`, `Redimensionar(cols, filas)`, `Cerrar`).

---

## 3. Modelo de Datos

### Diagrama E-R

```
┌──────────────┐          ┌─────────────────────────┐          ┌──────────────┐
│    GRUPOS    │          │          HOSTS          │          │  ETIQUETAS   │
├──────────────┤          ├─────────────────────────┤          ├──────────────┤
│ id        PK │───┐      │ id                   PK │      ┌───│ id        PK │
│ nombre    UQ │   │      │ nombre               UQ │      │   │ nombre    UQ │
│ orden        │   └────<│ grupo_id      FK NULL   │      │   └──────────────┘
│ plegado      │          │ direccion               │      │
│ creado_en    │          │ puerto                  │      │   ┌──────────────────┐
└──────────────┘          │ usuario                 │      │   │  HOST_ETIQUETAS  │
                          │ identidad_ref           │      │   ├──────────────────┤
                    ┌────<│ salto_host_id FK NULL   │      └──<│ etiqueta_id   PK │
                    │     │ multiplexar             │          │ host_id       PK │>───┐
                    │     │ keepalive_seg           │          └──────────────────┘    │
                    │     │ opciones_extra          │                                  │
                    │     │ origen                  │◀─────────────────────────────────┘
                    │     │ ultimo_estado           │
                    │     │ ultima_conexion_en      │
                    │     │ creado_en               │
                    │     │ actualizado_en          │
                    │     └────────────┬────────────┘
                    │                  │
                    └──────────────────┘  (autorreferencia: host de salto)
```

### Tablas

**GRUPOS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| nombre | TEXT UNIQUE NOT NULL | Ej. «4d3 · producción» |
| orden | INTEGER NOT NULL | Orden de presentación en el inventario |
| plegado | INTEGER NOT NULL DEFAULT 0 | 0/1; la vista recuerda el estado |
| creado_en | TEXT NOT NULL | ISO 8601 local |

**HOSTS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| nombre | TEXT UNIQUE NOT NULL | Alias; es el `Host` de magi_config. Sin espacios ni comodines |
| grupo_id | INTEGER FK GRUPOS NULL | NULL = «sin grupo» (se lista al final) |
| direccion | TEXT NOT NULL | IP o nombre DNS (`HostName`) |
| puerto | INTEGER NOT NULL DEFAULT 22 | |
| usuario | TEXT NULL | NULL = usuario local actual |
| identidad_ref | TEXT NULL | `auto` (NULL), `agente:SHA256:…` o `fichero:/ruta/clave` |
| salto_host_id | INTEGER FK HOSTS NULL | Host intermedio (`ProxyJump`); no puede ser él mismo ni formar ciclo |
| multiplexar | INTEGER NOT NULL DEFAULT 0 | Exporta `ControlMaster auto`; dentro de MAGI aplica desde la Fase 3 |
| keepalive_seg | INTEGER NULL | Segundos entre keepalive; NULL = desactivado; por defecto 30 al crear |
| opciones_extra | TEXT NOT NULL DEFAULT '' | Directivas ssh_config literales, una por línea |
| origen | TEXT NOT NULL | `manual` o `ssh_config` |
| ultimo_estado | TEXT NULL | `ok`, `error` o NULL (nunca conectado); solo del último intento |
| ultima_conexion_en | TEXT NULL | Fecha de la última sesión abierta con éxito |
| creado_en | TEXT NOT NULL | |
| actualizado_en | TEXT NOT NULL | |

**ETIQUETAS**

| Campo | Tipo | Descripción |
|---|---|---|
| id | INTEGER PK | |
| nombre | TEXT UNIQUE NOT NULL | Minúsculas, sin espacios (se normaliza al guardar) |

**HOST_ETIQUETAS**

| Campo | Tipo | Descripción |
|---|---|---|
| host_id | INTEGER FK HOSTS ON DELETE CASCADE | |
| etiqueta_id | INTEGER FK ETIQUETAS ON DELETE CASCADE | |
| (host_id, etiqueta_id) | PK | Una etiqueta que se queda sin hosts se borra al vaciar |

Sin `CHECK` sobre enumeraciones (decisión transversal de 4d3): `origen` y `ultimo_estado` se validan en código.

### Relaciones

- GRUPOS 1 — N HOSTS (opcional; al borrar un grupo sus hosts pasan a «sin grupo», nunca se borran en cascada).
- HOSTS 1 — N HOSTS por `salto_host_id` (autorreferencia; al borrar un host que es salto de otros, se pide confirmación y se pone a NULL en los dependientes).
- HOSTS N — N ETIQUETAS vía HOST_ETIQUETAS.

### Diagrama de estados: sesión SSH (entidad en memoria, no persistida)

```
                    ↵ / paleta «conectar»
      [inactiva] ────────────────────────▶ resolviendo
                                              │ DNS falla ─────────────────▶ error
                                              ▼
                                          conectando ─── timeout / rechazo ─▶ error
                                              │ (con salto: sesión al host de salto
                                              │  y canal direct-tcpip primero)
                                              ▼
                                      verificando_huella
                                       │        │        │
                              conocida │  desconocida  cambiada
                                       │        │        │
                                       │   [diálogo]  [diálogo bloqueo]
                                       │   a acepta   r reemplaza (escribiendo el nombre)
                                       │   esc ──────▶ cancelada ◀── esc
                                       ▼
                                        autenticando
                                        │        │ clave cifrada ─▶ [diálogo frase] ─esc─▶ cancelada
                                        │ agente y ficheros agotados ──────────────────▶ error
                                        ▼
                                       ┌─────────┐   Ctrl+] q    ┌──────────────────┐
                                       │ abierta │──────────────▶│ en_segundo_plano │
                                       │         │◀──────────────│                  │
                                       └────┬────┘   F3 / ↵      └────────┬─────────┘
                                            │ exit remoto · Ctrl+] x · red caída    │
                                            ▼                                        │
                                          cerrada ◀──────────────────────────────────┘

  error / cancelada / cerrada ──▶ [inactiva]  (HOSTS.ultimo_estado = error | sin cambio | ok)
```

Transiciones inválidas: `abierta → conectando` (no se reconecta sobre una sesión viva: hay que cerrarla); `error → abierta` (tras un error siempre se vuelve a inactiva y se reinicia el flujo completo); cualquier estado → `abierta` sin pasar por `verificando_huella`; abrir una segunda sesión mientras hay una `abierta` o `en_segundo_plano` (en la Fase 1 se pide cerrar la actual).

### Estado visible del host en el inventario (derivado, no es ciclo de vida)

| Glifo | Condición |
|---|---|
| `●` | Hay sesión abierta o en segundo plano con este host |
| `○` | Sin sesión y `ultimo_estado` es NULL u `ok` |
| `✕` | Sin sesión y `ultimo_estado` es `error` |

---

## 4. Flujos de Trabajo

### 4.1 Conectar a un host

```
┌─────────────────────┐
│ ↵ sobre un host     │
└──────────┬──────────┘
           ▼
   ¿hay sesión viva?  ──sí──▶ [diálogo: cerrar la sesión con X y abrir esta? s/n] ──n──▶ fin
           │ no                                          │ s (cierra y sigue)
           ▼◀────────────────────────────────────────────┘
   ¿tiene salto?  ──sí──▶ abrir sesión al host de salto (mismo flujo, recursivo, máx. 3 niveles)
           │ no                       │ error ──▶ mensaje «salto X: <motivo>» ──▶ fin
           ▼◀─────────────────────────┘ ok: canal direct-tcpip hacia direccion:puerto
   resolver direccion ──falla──▶ mensaje «no se resuelve <direccion>» · ultimo_estado=error ──▶ fin
           ▼
   TCP + handshake SSH (timeout 10 s) ──falla──▶ mensaje con motivo · ultimo_estado=error ──▶ fin
           ▼
   huella en known_hosts?
     ├─ conocida ────────────────────────────────────────────────┐
     ├─ desconocida ─▶ [diálogo huella nueva]  a ─▶ escribe línea ─┤   esc ─▶ cancelada
     └─ cambiada ────▶ [diálogo BLOQUEO en rojo: huella anterior y nueva]
                          r + escribir el nombre del host ─▶ sustituye línea ─┤   esc ─▶ cancelada
           ▼◀───────────────────────────────────────────────────────────────┘
   autenticar según identidad_ref
     ├─ auto:      agente (todas) → ~/.ssh/id_ed25519, id_ecdsa, id_rsa
     ├─ agente:H:  solo la clave con huella H (si no está cargada ─▶ error «ejecuta ssh-add»)
     └─ fichero:R: carga R; si está cifrada ─▶ [diálogo frase] (3 intentos) ─▶ esc ─▶ cancelada
           │ nada sirve ─▶ mensaje «autenticación rechazada» · ultimo_estado=error ──▶ fin
           ▼
   canal + request-pty (tamaño actual, TERM=xterm-256color) + shell
           ▼
   ultimo_estado=ok · ultima_conexion_en=ahora · vista SESIÓN
```

### 4.2 Importar `~/.ssh/config`

```
  I en Hosts  /  magi importar [ruta]
      ▼
  leer fichero (por defecto ~/.ssh/config) ──no existe──▶ mensaje ──▶ fin
      ▼
  parsear bloques:
    Host con un solo patrón sin * ni ?  ─▶ candidato
    Host * o con comodines               ─▶ omitido (motivo: patrón)
    Match …                               ─▶ omitido (motivo: Match)
    Include ruta                          ─▶ se sigue un nivel; magi_config se ignora
      ▼
  mapear directivas: HostName→direccion (o nombre) · Port→puerto · User→usuario
    IdentityFile→identidad_ref fichero · ProxyJump→salto (resolución diferida por nombre)
    ControlMaster→multiplexar · ServerAliveInterval→keepalive_seg · resto→opciones_extra
      ▼
  por cada candidato: ¿existe host con ese nombre?
    ├─ no  ─▶ crear en grupo «~/.ssh/config», origen=ssh_config
    └─ sí  ─▶ [diálogo: sobrescribir / omitir / sobrescribir todos / omitir todos]
      ▼
  resolver ProxyJump por nombre; si no existe ─▶ la directiva va a opciones_extra
      ▼
  [diálogo resumen: N importados · M sobrescritos · K omitidos (lista con motivo)]
```

### 4.3 Exportar `~/.ssh/magi_config`

```
  E en Hosts  /  magi exportar  /  automático tras guardar una ficha (opción config)
      ▼
  generar fichero: cabecera «# Generado por MAGI — no editar a mano», un bloque por host
      ▼
  escribir ~/.ssh/magi_config (600) de forma atómica (temporal + rename)
      ▼
  ¿~/.ssh/config contiene «Include ~/.ssh/magi_config» (o equivalente) ?
    ├─ sí ─▶ mensaje «exportados N hosts»
    └─ no ─▶ [diálogo: añadir la línea Include al principio de ~/.ssh/config? s/n]
              s ─▶ copia de seguridad config.bak-<fecha> · inserta en la primera línea
              n ─▶ mensaje con la línea para añadirla a mano
```

### 4.4 Diagrama de secuencia: apertura de sesión

```
 ui/hosts   app.rs      conexion/cliente    huellas    identidades   russh/host
    │          │               │               │             │            │
    │──↵──────▶│               │               │             │            │
    │          │──Conectar(h)─▶│               │             │            │
    │◀─Conectando─────────────│──TCP+kex──────────────────────────────────▶│
    │          │               │◀─────────────────────────── clave del servidor
    │          │               │──comprobar────▶│             │            │
    │          │               │◀─desconocida───│             │            │
    │◀─HuellaDesconocida(fp)──│               │             │            │
    │──AceptarHuella──────────▶│──aprender─────▶│ (escribe known_hosts)    │
    │          │               │──identidades──────────────▶│             │
    │          │               │◀─lista(agente, ficheros)───│             │
    │          │               │──auth publickey (agente) ────────────────▶│
    │          │               │◀─────────────────────────────────── éxito │
    │          │               │──open_session · pty(cols,filas) · shell──▶│
    │◀─Abierta────────────────│               │             │            │
    │          │               │◀─────────────────────────────── datos ───│
    │◀─Datos(bytes)───────────│ (vt100 parse → pantalla)                  │
    │──Teclas(bytes)──────────▶│──channel.data────────────────────────────▶│
    │──Redimensionar(c,f)─────▶│──window_change───────────────────────────▶│
```

### 4.5 Paso a paso: crear un host nuevo

1. `n` en Hosts abre la ficha vacía con puerto 22, keepalive 30 s, identidad «auto» y el grupo del host seleccionado.
2. `Tab` recorre los campos; los desplegables (grupo, identidad, salto) se abren con `↵` y filtran al escribir.
3. `Ctrl+T` prueba la conexión sin abrir shell (resolución, handshake, huella y autenticación) y muestra el resultado en la barra.
4. `Ctrl+S` valida (nombre único y sin espacios, puerto 1-65535, salto sin ciclos) y guarda; si la opción `exportar_al_guardar` está activa, regenera `magi_config`.
5. `Esc` descarta; si hay cambios sin guardar, pide confirmación.

---

## 5. Acciones y Atajos

MAGI no expone API HTTP; el equivalente a los endpoints son los atajos por vista y los subcomandos de CLI. Esta sección es la referencia que el checklist debe reflejar uno a uno.

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi` | Abre la TUI en la vista Hosts |
| `magi importar [ruta]` | Importa un ssh_config (por defecto `~/.ssh/config`) sin TUI; imprime el resumen |
| `magi exportar` | Regenera `~/.ssh/magi_config` sin TUI |
| `magi --version` | Versión |

### 5.2 Atajos globales

| Tecla | Acción |
|---|---|
| `F2` | Ir a Hosts |
| `F3` | Ir a la sesión activa (si la hay) |
| `F1`, `F4`–`F7` | Reservadas; muestran «vista no disponible en esta fase» en la barra |
| `Ctrl+P` | Paleta de comandos |
| `/` | Filtrar en la vista actual |
| `?` | Ayuda contextual de la vista |
| `Esc` | Cerrar diálogo o paleta; limpiar filtro |
| `q` | Volver / salir (pide confirmación si hay sesión viva) |

### 5.3 Hosts

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover selección (los grupos plegados se saltan como una fila) |
| `←` `→` / `h` `l` | Plegar / desplegar el grupo de la fila actual |
| `↵` | Conectar |
| `e` | Editar ficha |
| `n` | Nuevo host |
| `g` | Menú de grupo: nuevo, renombrar, mover host a grupo, borrar grupo, reordenar |
| `x` | Borrar host (confirmación) |
| `Tab` | Alternar columna derecha: usuario·puerto ⇄ etiquetas |
| `I` | Importar `~/.ssh/config` |
| `E` | Exportar `~/.ssh/magi_config` |

### 5.4 Ficha de host

| Tecla | Acción |
|---|---|
| `Tab` / `Shift+Tab` | Campo siguiente / anterior |
| `↵` | Abrir desplegable (grupo, identidad, salto) |
| `Espacio` | Marcar / desmarcar casilla |
| `Ctrl+S` | Guardar |
| `Ctrl+T` | Probar conexión |
| `Esc` | Descartar (confirmación si hay cambios) |

### 5.5 Sesión

| Tecla | Acción |
|---|---|
| Cualquier tecla | Se envía al remoto tal cual |
| `Ctrl+]` (prefijo, configurable) | Entra en modo MAGI durante una pulsación |
| prefijo · `q` / `Esc` | Volver a Hosts dejando la sesión en segundo plano |
| prefijo · `x` | Cerrar la sesión (confirmación) |
| prefijo · prefijo | Enviar el prefijo literal al remoto |

### 5.6 Paleta de comandos

| Tecla | Acción |
|---|---|
| Escribir | Filtra (difuso) sobre hosts y acciones |
| `↑` `↓` | Mover |
| `↵` | Ejecutar la entrada seleccionada |
| `Esc` | Cerrar |

Entradas de la paleta en esta fase: `conectar · <host>`, `editar host · <host>`, `nuevo host`, `importar ~/.ssh/config`, `exportar magi_config`, `ir a sesión`.

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
                     ┌──────────────┐
                     │   ARRANQUE   │  (sin desbloqueo en esta fase)
                     └──────┬───────┘
                            ▼
                  ┌──────────────────┐   ↵ conectar   ┌──────────────────┐
     ┌──────────▶ │   F2  HOSTS      │───────────────▶│   F3  SESIÓN     │
     │            │   inventario     │◀───────────────│   terminal       │
     │            └───┬────┬────┬────┘  prefijo·q      └──────────────────┘
     │   e / n        │    │    │  I / E
     │   ┌────────────┘    │    └─────────────┐
     │   ▼                 │ g                ▼
     │ ┌──────────────┐    ▼            ┌──────────────┐
     │ │ FICHA HOST   │  ┌────────────┐ │ DIÁLOGO      │
     └─│ 4 bloques    │  │ MENÚ GRUPO │ │ importación/ │
       └──────────────┘  └────────────┘ │ exportación  │
                                        └──────────────┘
     Transversal: Ctrl+P paleta · ? ayuda · diálogos de huella y frase desde cualquier conexión
     Reservadas:  F1 Flota (F2) · F4 Archivos (F4) · F5 Claves (F2) · F6 Snippets (F4) · F7 Registro (F2)
```

### 6.1 Hosts

```
┌ MAGI · HOSTS ─────────────────────────────── 12 hosts ──────┐
│ / prod_                                                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ▼ 4d3 · producción                                    (4)  │
│      ● hetzner-01      95.217.x.x        root    22         │
│    ▸ ○ hetzner-02      95.217.x.x        root    22         │
│      ○ vps-openclaw    49.12.x.x         hector  2222       │
│      ○ backup-nas      10.0.0.12         admin   22   ⤴ h1  │
│                                                             │
│  ▼ 4d3 · desarrollo                                    (3)  │
│      ✕ rincon-dev      10.0.0.40         hector  22         │
│      ○ ci-runner       10.0.0.41         runner  22         │
│      ○ staging         10.0.0.42         hector  22         │
│                                                             │
│  ▶ ~/.ssh/config                                       (5)  │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ ↵ conectar  e editar  n nuevo  g grupo  x borrar  ⇥ etiq.   │
└─────────────────────────────────────────────────────────────┘
```

`⤴ h1` indica que el host conecta a través de un salto (nombre abreviado). Con `Tab` la columna usuario·puerto se sustituye por las etiquetas.

### 6.2 Ficha de host

```
┌ MAGI · HOST ────────────────────────── hetzner-01 · editar ─┐
│                                                             │
│  IDENTIFICACIÓN                                             │
│    Nombre      [ hetzner-01                              ]  │
│    Dirección   [ 95.217.x.x                              ]  │
│    Puerto      [ 22    ]     Grupo  [ 4d3 · producción ▾ ]  │
│    Etiquetas   [ web  postgres  crítico                  ]  │
│                                                             │
│  ACCESO                                                     │
│    Usuario     [ root                                    ]  │
│    Identidad   [ agente · 4d3-ed25519 · SHA256:hq3K…   ▾ ]  │
│    Salto vía   [ ninguno                               ▾ ]  │
│                                                             │
│  AL CONECTAR                                                │
│    Multiplexar [x] ControlMaster auto al exportar           │
│    Mantener    [x] keepalive cada [ 30 ] s                  │
│                                                             │
│  OPCIONES EXTRA (ssh_config, una por línea)                 │
│    ┌───────────────────────────────────────────────────┐    │
│    │ ForwardAgent yes                                  │    │
│    │ LocalForward 5432 127.0.0.1:5432                  │    │
│    └───────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│ ^s guardar   ^t probar conexión   esc descartar             │
└─────────────────────────────────────────────────────────────┘
```

### 6.3 Sesión

```
┌ MAGI · SESIÓN ──────────────────────────── hetzner-01 ──────┐
│ root@hetzner-01:~# systemctl status nginx                   │
│ ● nginx.service - A high performance web server             │
│      Loaded: loaded (/lib/systemd/system/nginx.service)     │
│      Active: active (running) since Mon 15:02:11            │
│                                                             │
│ root@hetzner-01:~# _                                        │
│                                                             │
│                                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ ● conectado 18 m · ed25519 (agente) · ^] q volver  ^] x cerrar│
└─────────────────────────────────────────────────────────────┘
```

Una sola línea de marco arriba y una barra abajo; todas las filas restantes son del PTY remoto, que se redimensiona al tamaño real disponible.

### 6.4 Diálogo de huella cambiada

```
        ┌─ HUELLA CAMBIADA ── hetzner-01 ──────────────────────┐
        │  ✕ La clave del servidor NO coincide con known_hosts │
        │                                                      │
        │  Anterior  ed25519  SHA256:hq3K…8fMw   (12 mar 2026) │
        │  Nueva     ed25519  SHA256:p91X…c2Qz                 │
        │                                                      │
        │  Puede ser una reinstalación… o un intermediario.    │
        │  Para sustituir la huella escribe el nombre del host │
        │  [ ________________ ]                                │
        │                                                      │
        │  r sustituir       esc cancelar (recomendado)        │
        └──────────────────────────────────────────────────────┘
```

### 6.5 Paleta de comandos

```
        ┌──────────────────────────────────────────────┐
        │  ❯ hetz_                                     │
        ├──────────────────────────────────────────────┤
        │  ▸ conectar · hetzner-01           host      │
        │    conectar · hetzner-02           host      │
        │    editar host · hetzner-01        acción    │
        │    editar host · hetzner-02        acción    │
        └──────────────────────────────────────────────┘
```

### Notas de UX y diseño

- **Tema.** Al arrancar se lee `~/.config/omarchy/current/theme/alacritty.toml`: `primary.background/foreground` → fondo y texto, `normal.yellow` → interfaz (marcos, cabeceras, selección), `normal.green` → correcto, `normal.red` → crítico, `bright.black` → inactivo. Si el fichero no existe (macOS, otra distro) se usa la paleta fija del informe de diseño (`#FFA000`, `#00FF6A`, `#FF2A1A`, `#5A5A5A`, `#EDE8DC` sobre `#0A0A0A`). Un tema explícito en `config.toml` tiene prioridad.
- **Doble codificación.** Todo estado se muestra con glifo y color. En modo degradado (locale sin UTF-8 o `MAGI_ASCII=1`) los glifos pasan a `*`, `o`, `x`, `>` y los marcos a `+ - |`.
- **Barra inferior.** Siempre visible, siempre con los atajos del contexto. Los mensajes (errores, resultados de exportación) se muestran en ella durante 4 s o hasta la siguiente tecla; los errores en rojo con `✕`.
- **Diálogos.** Centrados, un solo propósito, con las teclas en la última línea. Ninguna acción destructiva (borrar host, borrar grupo, sustituir huella, cerrar sesión, sobrescribir `~/.ssh/config`) se ejecuta con una sola pulsación.
- **Movimiento.** Solo el destello de una fila al cambiar de estado y el cursor del terminal remoto.
- **Redimensionado.** Un `Resize` de crossterm repinta la vista actual y, si hay sesión, envía `window_change` al remoto.

---

## 7. Lógica de Negocio

### 7.1 Filtro incremental del inventario

Subcadena sin distinguir mayúsculas ni acentos, sobre `nombre`, `direccion`, `usuario`, nombre del grupo y etiquetas. Los grupos que no tienen ningún host coincidente se ocultan; los grupos plegados se despliegan temporalmente mientras hay filtro. Tokens separados por espacio se combinan con AND.

```python
def coincide(host, consulta: str) -> bool:
    tokens = normalizar(consulta).split()
    pajar = normalizar(" ".join([
        host.nombre, host.direccion, host.usuario or "",
        host.grupo or "", " ".join(host.etiquetas),
    ]))
    return all(t in pajar for t in tokens)
```

### 7.2 Validación de la ficha

| Campo | Regla |
|---|---|
| nombre | Obligatorio, único, `[A-Za-z0-9._-]+` (es el `Host` de ssh_config: sin espacios ni `*`/`?`) |
| direccion | Obligatoria; IP v4/v6 o nombre DNS |
| puerto | 1–65535 |
| usuario | Opcional; sin espacios |
| identidad_ref | `auto`, `agente:<huella>` presente en el agente en el momento de guardar (aviso, no bloqueo, si no está) o `fichero:<ruta>` existente |
| salto | Distinto del propio host; recorrer la cadena de saltos y rechazar ciclos y más de 3 niveles |
| keepalive_seg | 5–600 si «Mantener» está marcado |
| etiquetas | Se normalizan a minúsculas, sin espacios internos ni duplicados |
| opciones_extra | Cada línea `Directiva valor`; se rechazan las directivas que MAGI ya gestiona (`HostName`, `Port`, `User`, `IdentityFile`, `ProxyJump`, `ControlMaster`, `ServerAliveInterval`) para evitar duplicidad |

### 7.3 Exportación a magi_config

```
# Generado por MAGI el 2026-09-14 15:42 — no editar a mano; usa `magi`
# Grupo: 4d3 · producción
Host hetzner-01
    HostName 95.217.x.x
    Port 22
    User root
    IdentityFile ~/.ssh/4d3-ed25519          # solo si identidad_ref es fichero
    ProxyJump vps-openclaw                   # nombre del host de salto
    ControlMaster auto                       # si multiplexar
    ControlPath ~/.ssh/cm-%r@%h:%p
    ControlPersist 10m
    ServerAliveInterval 30                   # si keepalive_seg
    ForwardAgent yes                         # opciones_extra, literales
```

Reglas: bloques ordenados por grupo y nombre; `agente:` no genera `IdentityFile` (el agente ya lo aporta); un host de salto se exporta antes que quien lo usa; escritura atómica y permisos 600.

### 7.4 Ida y vuelta

`importar(exportar(hosts))` debe devolver el mismo inventario (campos gestionados y `opciones_extra` línea a línea). Es el test de referencia del parser y se ejecuta en CI.

### 7.5 Resolución de identidad al conectar

```python
def candidatas(host, agente, ficheros_por_defecto):
    if host.identidad_ref is None:                       # auto
        return agente.claves() + [f for f in ficheros_por_defecto if existe(f)]
    if host.identidad_ref.startswith("agente:"):
        huella = host.identidad_ref[7:]
        clave = agente.por_huella(huella)
        if clave is None:
            raise Error("la clave %s no está en el agente; ejecuta ssh-add" % huella)
        return [clave]
    return [cargar_fichero(host.identidad_ref[8:])]      # pide frase si está cifrada
```

Se intenta cada candidata en orden y se para en la primera aceptada. Una frase introducida se mantiene en memoria (zeroize) solo mientras dure la sesión que la usó.

### 7.6 Huellas

Se usa `~/.ssh/known_hosts` del sistema (entradas hasheadas incluidas) mediante `russh-keys`. La clave se comprueba contra `direccion` y, si el puerto no es 22, contra `[direccion]:puerto`, como OpenSSH. Aceptar una huella nueva añade una línea; sustituir una cambiada elimina las líneas previas de esa dirección y añade la nueva, con copia `known_hosts.old`. Ambas operaciones quedan en el log de MAGI con fecha, host y huellas.

### 7.7 Casos especiales

- Host de salto que a su vez tiene salto: se resuelve en cadena (máx. 3 niveles); cada nivel pasa por su propia verificación de huella y autenticación.
- El host de salto y el destino pueden usar identidades distintas.
- Borrar un grupo con hosts mueve los hosts a «sin grupo» tras confirmar.
- Renombrar un host que es salto de otros no rompe nada (la relación es por id); la exportación usa el nombre nuevo.
- Si `~/.ssh` no existe o no es escribible, la exportación falla con mensaje claro y el inventario sigue operativo.
- Al salir de MAGI con una sesión viva se pide confirmación; al confirmar, se cierra el canal limpiamente antes de restaurar el terminal.

---

## 8. Requisitos No Funcionales

**Seguridad**

- MAGI no almacena claves privadas ni frases: `identidad_ref` solo guarda huellas o rutas. Las claves descifradas y las frases viven en memoria con `zeroize` y se destruyen al cerrar la sesión.
- Verificación estricta de huellas: una huella cambiada bloquea la conexión; sustituirla exige escribir el nombre del host y queda en el log.
- `magi.db`, `magi_config` y `known_hosts` se escriben con permisos 600 y de forma atómica.
- Sin telemetría ni conexiones salientes distintas de los hosts del usuario.
- La TUI restaura siempre el terminal (raw mode, pantalla alternativa) también ante un `panic`, mediante un hook.
- Sin rate limiting ni roles: aplicación local monousuario.

**Protección de datos (RGPD)**

Datos tratados: nombres de host, direcciones IP, usuarios de sistema y etiquetas, todos en el equipo del usuario, sin transferencia a terceros. Base legal: interés legítimo del propio usuario sobre su infraestructura. No hay tratamiento de datos de terceros más allá de los que el usuario introduce; los derechos ARCO se ejercen borrando registros o el fichero de datos. No aplica registro de actividades de tratamiento.

**Backups y recuperación**

- Copiar `~/.local/share/magi/magi.db` (WAL con checkpoint al salir) basta para restaurar el inventario.
- `magi_config` es un segundo respaldo legible: `magi importar ~/.ssh/magi_config` reconstruye los hosts.
- Antes de modificar `~/.ssh/config` o `known_hosts` se deja copia con sufijo de fecha.

**Rendimiento**

- Objetivo: 500 hosts, arranque < 200 ms, filtro < 16 ms por pulsación, render solo cuando cambia el estado.
- Una sesión abierta procesa la salida del remoto en la tarea tokio y solo notifica a la UI cuando el buffer vt100 cambia, coalesciendo a ~30 fps.

**Accesibilidad**

- Doble codificación glifo + color en todo estado; modo degradado ASCII; nada depende del ratón; contraste garantizado por el tema del terminal del usuario.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| `ssh-agent` | Socket en `SSH_AUTH_SOCK`; listado de claves y firma vía russh-keys |
| `~/.ssh/config` | Lectura e importación; inserción opcional de `Include ~/.ssh/magi_config` |
| `~/.ssh/magi_config` | Salida propia, formato OpenSSH ssh_config |
| `~/.ssh/known_hosts` | Lectura, aprendizaje y sustitución de huellas, formato OpenSSH |
| Tema Omarchy | `~/.config/omarchy/current/theme/alacritty.toml` (TOML de Alacritty) |
| Hosts remotos | SSH-2 sobre TCP; algoritmos por defecto de russh; `TERM=xterm-256color` |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D1 — Rust + ratatui en un solo crate binario.** Se descarta separar un `magi-core` para Android: Hector prefiere no condicionar la Fase 1; la app Android (Fase 6) reimplementará el modelo o extraerá el núcleo entonces. Descartado también Python + Textual por coherencia con mmmusic y Dicta.
- **D2 — Conexión embebida con russh desde la Fase 1.** Hector elige librería frente a delegar en el `ssh` del sistema. Ventaja: MAGI controla la sesión (estado, huellas, tamaño) y el mismo cliente sirve para SFTP, túneles y Android. Coste: el riesgo técnico del multiplexado se adelanta a esta fase; se acota con **una sola sesión** y con `tui-term` + `vt100` para no escribir un emulador propio. Contrapartida asumida: `ControlMaster` no aplica dentro de MAGI (solo se exporta) y las claves `sk` (YubiKey) requieren el agente.
- **D3 — SQLite sin cifrar en la Fase 1.** El inventario no contiene secretos; el cifrado en reposo y la pantalla de desbloqueo llegan con la sincronización (Fase 5). Descartado SQLCipher por complejidad sin beneficio aún.
- **D4 — Inventario como fuente de verdad, exportado a `magi_config` vía `Include`.** Descartado editar `~/.ssh/config` directamente (riesgo de destrozar el fichero del usuario) y leerlo en vivo (dos fuentes de verdad).
- **D5 — `known_hosts` del sistema, sin tabla propia.** Compartir huellas con el `ssh` del sistema evita divergencias. El log de MAGI registra aceptaciones y sustituciones.
- **D6 — Tema del terminal de Omarchy con paleta fija de respaldo.** Coherente con mmmusic; la paleta del informe de diseño se conserva como identidad por defecto fuera de Omarchy.
- **D7 — `opciones_extra` como texto ssh_config literal.** Garantiza importación sin pérdida sin modelar cada directiva de OpenSSH.
- **D8 — Una sesión a la vez en la Fase 1.** Las pestañas y los túneles (Fase 3) se construyen sobre esta sesión única; la máquina de estados ya contempla `en_segundo_plano` para no rehacerla.
- **D9 — Migraciones por `PRAGMA user_version`.** Un vector de funciones numeradas; sin Alembic ni refinery. Sin `CHECK` en enumeraciones (transversal 4d3).
- **D10 — Prefijo de escape `Ctrl+]` configurable.** No colisiona con tmux (`Ctrl+B`) ni con screen (`Ctrl+A`); se cambia en `config.toml`.
- **D11 — Búsqueda: subcadena en `/`, difusa en la paleta.** El filtro del inventario debe ser predecible; la paleta favorece la velocidad del usuario experto.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Esqueleto y almacén | Cargo, CLI, config, tema, almacén con migraciones, modelo, bucle de eventos, barra, ayuda, vista Hosts con grupos y filtro | 1 semana |
| S2 — Ficha, grupos y etiquetas | Formulario de 4 bloques, desplegables, validación, menú de grupo, etiquetas, borrado con confirmación, paleta | 1 semana |
| S3 — ssh_config e identidades | Parser, importación con diálogo de conflictos y resumen, exportación con Include, test de ida y vuelta, escaneo de identidades | 1 semana |
| S4 — Conexión y sesión | Cliente russh, máquina de estados, huellas, frase, salto, vista Sesión con vt100, prefijo, redimensionado, probar conexión | 2 semanas |

**Total: 4 sprints, ~5 semanas.** Supuestos: un desarrollador con Claude Code, dedicación parcial, sin bloqueos en las dependencias (`tui-term` compatible con la versión de ratatui elegida). En la práctica el agente suele cerrar una fase de este tamaño en una o dos sesiones; la estimación cubre también la validación manual sobre Omarchy y macOS.

---

## 12. Conexiones con Otras Fases

- **Fase 2 (Flota, identidades, registro):** reutiliza `ultimo_estado` y `ultima_conexion_en`; añade sondeo bajo demanda sobre el cliente russh de esta fase y la tabla REGISTRO para las acciones que aquí solo van al log.
- **Fase 3 (Sesiones con pestañas, túneles):** convierte la sesión única en una lista; la máquina de estados y `terminal.rs` se reutilizan. `multiplexar` pasa a significar «reutilizar la conexión russh para nuevos canales». `LocalForward` en `opciones_extra` puede migrarse a la tabla TUNELES.
- **Fase 4 (SFTP, snippets, deliberación):** la ficha gana los bloques «Snippet al conectar» y «Verificaciones previas»; SFTP usa `russh-sftp` sobre la misma conexión.
- **Fase 5 (sincronización):** cifra `magi.db` en reposo y añade la pantalla de arranque con desbloqueo.
- **Fase 6 (Android):** comparte el esquema de tablas de esta fase.
