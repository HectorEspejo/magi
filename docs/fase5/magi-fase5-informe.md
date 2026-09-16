# MAGI - Fase 5: Túneles

## Especificación Funcional

**Versión:** 1.0
**Fecha:** 15 de septiembre de 2026
**Cliente:** 4d3 (producto propio, sin cliente externo)

> Especificada sobre las Fases 2, 3 y 4 con implementación reportada pero **sin validación manual completa** (riesgo R15 en el maestro).

---

## 1. Visión General

La Fase 5 añade los túneles SSH: locales (`-L`), remotos (`-R`) y dinámicos (`-D`, proxy SOCKS5), gestionados desde una tabla con interruptor en lugar de recordar la sintaxis. Viven en el servidor de sesiones sobre la conexión del pool, sobreviven a la ventana, se ven desde todas y pueden marcarse como automáticos para levantarse con la primera pestaña o SFTP al host y pararse con la última. Es la última pieza del terreno de Termius y no introduce riesgo técnico: son canales `direct-tcpip` (los mismos de los saltos de la Fase 1) y `tcpip-forward` / `forwarded-tcpip` de russh sobre la infraestructura de las Fases 3 y 4.

**Objetivos principales**

1. Tabla TUNELES (migración 4): host, nombre, tipo, escucha, destino, automático.
2. Túneles en el servidor: escucha local con `TcpListener`, `direct-tcpip` por conexión aceptada, SOCKS5 para el dinámico, `tcpip-forward` + `forwarded-tcpip` para el remoto; contadores de tráfico y conexiones; `Tuneles{lista}`; protocolo v3.
3. Vista Túneles (`F6`) con interruptor `Espacio`, alta/edición/borrado, relanzado, detalle y filtro; bloque «Túneles» en la ficha de host.
4. Automáticos ligados al ciclo de vida de las conexiones del host; manuales que no se paran solos.
5. Importación de `LocalForward` / `RemoteForward` / `DynamicForward` desde `opciones_extra` y ssh_config a la tabla, y exportación a `magi_config`.
6. REGISTRO con `tunel_abierto`, `tunel_cerrado`, `tunel_fallido`; `magi servidor estado` con los túneles; CLI `magi tunel`.

**Contexto.** Fase media (≈ 60 funcionalidades) que cierra el roadmap funcional antes de snippets y deliberación. Fuera de alcance: puertos privilegiados (< 1024) en remoto, reenvío de agente y de X11, `GatewayPorts` en el host (se puede pedir `0.0.0.0` pero el host manda).

---

## 2. Arquitectura Técnica

### Stack tecnológico

Sin crates nuevos: `tokio::net::TcpListener`/`TcpStream`, `tokio::io::copy_bidirectional`, canales `direct-tcpip` y `tcpip_forward` de russh 0.63; SOCKS5 implementado a mano (`CONNECT`, sin autenticación, direcciones IPv4, IPv6 y dominio; unas 80 líneas). `VERSION_PROTOCOLO = 3`.

### Estructura de carpetas (novedades)

```
src/
├── almacen/migraciones.rs   migración 4: TUNELES
├── almacen/tuneles.rs       CRUD (cliente)
├── modelo.rs                Tunel {id, host_id, nombre, tipo, escucha, destino, automatico}
├── protocolo.rs             VERSION_PROTOCOLO = 3 + mensajes de túneles
├── sshconfig/tuneles.rs     LocalForward/RemoteForward/DynamicForward ⇄ Tunel
├── servidor/
│   ├── tuneles.rs           TunelActivo, arranque/parada, listeners, contadores, automáticos, Tuneles{lista}
│   ├── socks5.rs            saludo y CONNECT
│   └── conexiones.rs        + reenvíos remotos registrados por (dirección, puerto) → túnel
├── conexion/cliente.rs      Handler: server_channel_open_forwarded_tcpip → tuneles
└── ui/
    ├── tuneles.rs           F6
    ├── ficha.rs             bloque «Túneles»
    └── dialogos.rs          alta/edición de túnel, aviso 0.0.0.0, importar a túneles
docs/fase5/
├── magi-fase5-informe.md · magi-fase5-prompt.md · magi-fase5-checklist.md
└── magi-fase5-implementacion.md   (lo escribe el agente)
```

### Diagrama de arquitectura

```
 cliente magi                                 magi --servidor
 ┌──────────────────────┐   ActivarTunel      ┌─────────────────────────────────────────────┐
 │ ui/tuneles.rs (F6)   │ ──────────────────▶ │ servidor/tuneles.rs                         │
 │ ui/ficha.rs (bloque) │ ◀── Tuneles{lista}  │  TunelActivo por tunel_id                   │
 │ almacen/tuneles.rs   │   RecargarTuneles   │   local:    TcpListener 127.0.0.1:5432 ─┐   │
 │  (CRUD en SQLite)    │ ──────────────────▶ │   dinámico: TcpListener + socks5.rs ────┼─▶ direct-tcpip ─▶ pool ─▶ host ─▶ destino
 └──────────────────────┘                     │   remoto:   tcpip_forward en el host ◀──┘   │
          │ SQLite (TUNELES)                  │             forwarded-tcpip ─▶ TcpStream local (destino)
          ▼                                   │  contadores · automáticos por host          │
   ~/.local/share/magi/magi.db  ◀── lectura ──│  registro::anotar (tunel_*) vía hilo BD     │
                                              └─────────────────────────────────────────────┘
```

**Reparto.** El cliente hace el CRUD de TUNELES en SQLite (T18) y avisa con `RecargarTuneles{host_id}`; el servidor lee la tabla con su conexión de solo lectura, mantiene los túneles activos, cuenta tráfico y difunde `Tuneles{lista}` (T27). Activar un túnel sin conexión al host abre una con el flujo de la Fase 3 (diálogos al solicitante) y la mete en el pool; el túnel cuenta como canal del pool mientras está activo (T34).

---

## 3. Modelo de Datos

### TUNELES (migración 4)

```
 ┌──────────────────────┐        ┌─────────────────────────────┐
 │ HOSTS (F1)           │───────<│ TUNELES                     │
 └──────────────────────┘        │ id            PK            │
                                 │ host_id       FK CASCADE    │
                                 │ nombre        TEXT NOT NULL │  único por host
                                 │ tipo          TEXT NOT NULL │  local | remoto | dinamico
                                 │ escucha       TEXT NOT NULL │  «127.0.0.1:5432»
                                 │ destino       TEXT NULL     │  «10.0.0.5:5432»; NULL en dinámico
                                 │ automatico    INTEGER 0/1   │
                                 │ creado_en · actualizado_en  │
                                 └─────────────────────────────┘
 UNIQUE(host_id, nombre). Sin CHECK sobre tipo (validación en código).
```

Semántica por tipo: **local** escucha en la máquina de MAGI y el host abre `destino`; **remoto** el host escucha en `escucha` y MAGI abre `destino` en local; **dinámico** escucha en local como proxy SOCKS5 y el host abre lo que pida cada cliente. El estado en vivo no se persiste. REGISTRO añade `tunel_abierto`, `tunel_cerrado`, `tunel_fallido`.

### Estructuras en memoria del servidor

```
 TunelActivo
 │ tunel_id · host_id · tipo · escucha · destino
 │ estado          activando | activo | parando | caido
 │ origen          manual | automatico
 │ listener        JoinHandle (local/dinámico) | registro de tcpip_forward (remoto)
 │ conexiones      abiertas ahora · aceptadas total
 │ bytes_subidos · bytes_bajados
 │ desde · ultimo_error · solicitante
```

### Diagrama de estados: túnel

```
   [inactivo] (fila en TUNELES sin TunelActivo)
        │ Espacio / ActivarTunel / automático (primera pestaña o SFTP al host)
        ▼
    activando ── conexión del pool o nueva (diálogos al solicitante) ──
        │  bind falla (EADDRINUSE, permiso) · host rechaza tcpip_forward · conexión falla
        │        ─▶ caido (ultimo_error) · REGISTRO tunel_fallido
        ▼
     activo ── acepta conexiones · cuenta bytes ── REGISTRO tunel_abierto
        │  Espacio / PararTunel / automático: se cerró la última pestaña o SFTP
        │        ─▶ parando (cierra listener o cancel_tcpip_forward, corta las conexiones abiertas)
        │            ─▶ [inactivo] · REGISTRO tunel_cerrado(motivo «parado»)
        │  conexión del pool caída ─▶ caido («conexión caída») · REGISTRO tunel_cerrado(motivo)
        ▼
     caido ── r / RelanzarTunel ─▶ activando          Espacio ─▶ [inactivo] (descarta el error)
  Inválidas: activo → activando; parando → activo; caido → activo sin relanzar; activar una fila
  que ya está activa (se ignora con mensaje).
```

---

## 4. Flujos de Trabajo

### 4.1 Activar un túnel local

```
  Espacio sobre una fila inactiva (o `magi tunel activar <host> <nombre>`)
      ▼
  cliente: ActivarTunel{tunel_id} ─▶ servidor: lee TUNELES (solo lectura)
      ▼
  conexión del pool al host ─no hay─▶ abrir con el flujo F3 (huella/frase/contraseña → solicitante)
      │ error ─▶ caido · tunel_fallido ─▶ fin
      ▼
  TcpListener::bind(escucha) ─EADDRINUSE / permiso─▶ caido «puerto 5432 en uso» · tunel_fallido ─▶ fin
      ▼
  activo · tunel_abierto · Tuneles{lista}
      ▼ por cada conexión aceptada:
  channel_open_direct_tcpip(destino, origen) ─rechazo del host─▶ cierra esa conexión, ultimo_error, sigue activo
      ▼
  copy_bidirectional(TcpStream ⇄ canal) · contadores · fin de la conexión ─▶ conexiones -1
```

### 4.2 Túnel remoto

```
  ActivarTunel ─▶ conexión del pool ─▶ handle.tcpip_forward(escucha.direccion, escucha.puerto)
      │ el host responde «no» (puerto ocupado allí, GatewayPorts, < 1024) ─▶ caido «el host rechazó el reenvío»
      ▼
  registrar (dirección, puerto) → tunel_id en la conexión
      ▼ cada vez que el host recibe una conexión:
  Handler::server_channel_open_forwarded_tcpip(canal, dirección, puerto, origen) ─▶ ¿túnel activo con esa clave?
      ├─ sí ─▶ TcpStream::connect(destino) ─falla─▶ cierra el canal, ultimo_error
      │         copy_bidirectional · contadores
      └─ no ─▶ se rechaza el canal
  Parar ─▶ handle.cancel_tcpip_forward · corta las conexiones abiertas
```

### 4.3 Túnel dinámico (SOCKS5)

```
  TcpListener::bind(escucha) ─▶ por conexión: socks5: saludo (métodos; se acepta 0x00 «sin auth»)
      ─▶ petición CONNECT con destino IPv4 / IPv6 / dominio:puerto (BIND y UDP ASSOCIATE → 0x07 «no soportado»)
      ─▶ channel_open_direct_tcpip(destino) ─rechazo─▶ respuesta SOCKS 0x05 «conexión rechazada»
      ─▶ respuesta 0x00 · copy_bidirectional · contadores
```

### 4.4 Automáticos

```
  servidor: se abre el primer canal (pestaña o SFTP) de un host
      ▼
  leer TUNELES WHERE host_id = h AND automatico = 1 ─▶ ActivarTunel(origen = automatico) por cada uno
      (los que ya estén activos, manuales o no, se dejan como están)
  servidor: se cierra el último canal de pestaña/SFTP del host
      ▼
  parar los túneles con origen = automatico; los manuales siguen (y mantienen viva la conexión del pool)
```

### 4.5 Importar reenvíos de ssh_config

```
  Ficha de host con `LocalForward` / `RemoteForward` / `DynamicForward` en opciones_extra
      ▼ (aviso «hay 2 reenvíos en opciones extra · i importar a túneles») · también al importar ~/.ssh/config
  parsear: LocalForward [bind:]puerto destino:puerto → local · RemoteForward → remoto · DynamicForward [bind:]puerto → dinamico
      ▼
  crear filas en TUNELES (nombre «local-5432», «remoto-9000», «socks-1080»; automatico = 1, como haría ssh) · quitar las líneas de opciones_extra
      ▼
  exportar magi_config: por cada túnel del host, la directiva correspondiente (así `ssh <host>` fuera de MAGI los sigue teniendo)
```

### 4.6 Diagrama de secuencia: túnel local con conexión nueva

```
 ui/tuneles   servidor        host          app local (psql)
    │──ActivarTunel──▶│
    │◀─PideContrasena─│  (sin conexión: flujo F3 al solicitante)
    │──Contrasena────▶│──conecta──────▶│
    │◀─Tuneles[activo]│  bind 127.0.0.1:5432
    │                 │◀────────────────────── connect 127.0.0.1:5432 ──│
    │                 │──direct-tcpip 10.0.0.5:5432──▶│
    │                 │◀═══════════ copy_bidirectional ═══════════════▶│
    │◀─Tuneles[1 conexión · 14 kB]│ (coalescido ≤ 2/s)
```

---

## 5. Acciones y Atajos

### 5.1 Subcomandos de CLI

| Comando | Descripción |
|---|---|
| `magi tunel activar <host> <nombre>` | Activa un túnel (abre la conexión si hace falta; los diálogos van a… ninguna ventana: sin conexión viva y sin llavero, falla con instrucción) |
| `magi tunel parar <host> <nombre>` | Para un túnel |
| `magi tuneles` | Lista túneles activos y definidos con estado y tráfico |
| `magi servidor estado` | Añade los túneles activos |

### 5.2 Protocolo (mensajes nuevos, `VERSION_PROTOCOLO = 3`)

| Cliente → servidor | Servidor → cliente |
|---|---|
| `ActivarTunel{tunel_id}` / `PararTunel{tunel_id}` / `RelanzarTunel{tunel_id}` | `Tuneles{lista}` (difusión en cada cambio de estado; contadores ≤ 2/s) |
| `RecargarTuneles{host_id}` (tras un CRUD en el cliente) | `Hecho{peticion_id}` / `Error{peticion_id, mensaje}` |

`Bienvenida` añade `tuneles` (activos).

### 5.3 Vista Túneles (`F6`)

| Tecla | Acción |
|---|---|
| `↑` `↓` / `j` `k` | Mover |
| `Espacio` | Activar / parar (sobre `caido`, vuelve a inactivo) |
| `r` | Relanzar un túnel caído |
| `n` | Nuevo túnel (diálogo: host, nombre, tipo, escucha, destino, automático) |
| `e` | Editar (si está activo, se para, se edita y se ofrece relanzar) |
| `x` | Borrar (confirmación; si está activo, primero se para) |
| `a` | Alternar automático |
| `↵` | Detalle: desde cuándo, conexiones abiertas y aceptadas, bytes ↑↓, último error, origen |
| `/` | Filtrar por nombre, host, puerto o tipo |
| `q` / `Esc` | Volver (los túneles siguen) |

### 5.4 Ficha de host

| Novedad | Descripción |
|---|---|
| Bloque «Túneles» | Lista de túneles del host con tipo, escucha → destino y `[a]` automático; `n` nuevo, `e` editar, `x` borrar (misma lógica que la vista) |
| Aviso de reenvíos en opciones extra | «hay N reenvíos en opciones extra · i importar a túneles» |
| Validación de opciones extra | `LocalForward`, `RemoteForward` y `DynamicForward` pasan a ser directivas gestionadas: se rechazan al editar a mano |

### 5.5 Otras vistas y paleta

| Dónde | Novedad |
|---|---|
| Sesión (barra) | «túneles 2» si el host tiene túneles activos |
| Flota y Hosts | glifo `⇅` tras el nombre si el host tiene túneles activos |
| Paleta | `túnel · <host> · <nombre>` (activa/para), `ir a túneles`, `nuevo túnel` |
| Importación de ssh_config | Los reenvíos van a TUNELES en lugar de a `opciones_extra` |
| Exportación a magi_config | Emite `LocalForward`/`RemoteForward`/`DynamicForward` por túnel |

---

## 6. Interfaz de Usuario

### Mapa de navegación (novedades)

```
   F6 TÚNELES ◀── paleta «ir a túneles» ◀── Hosts/Flota ⇅ ◀── Sesión «túneles 2»
   │ Espacio activar/parar · n e x · a · r · ↵ detalle · /
   └── diálogo túnel (host · nombre · tipo · escucha · destino · [x] automático)
   Ficha de host ── bloque Túneles (n e x) ── aviso «i importar a túneles»
   Reservada: ninguna (F1-F7 todas ocupadas; Snippets (F6 del roadmap) irá a Ctrl+P / subvista)
```

### 6.1 Túneles

```
┌ MAGI · TÚNELES ─────────────────────────── 4 · 2 activos ───┐
│                                                             │
│  ESTADO  TIPO     ESCUCHA          →  DESTINO         HOST  │
│  ─────────────────────────────────────────────────────────  │
│▸ ●       local    127.0.0.1:5432  →  10.0.0.5:5432  hetz-01 a│
│  ●       local    127.0.0.1:8080  →  127.0.0.1:80   hetz-02  │
│  ○       dinámico 127.0.0.1:1080     (socks5)       vps-ocl  │
│  ✕       remoto   127.0.0.1:9000  →  127.0.0.1:9000 hetz-01  │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  pg-prod · hetzner-01 · automático                           │
│    Activo desde  15:24:03 · 18 min · 1 conexión (3 en total) │
│    Tráfico       ↓ 14.2 MB   ↑ 890 kB                        │
│    Último error  —                                           │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ espacio activar/parar  n nuevo  e editar  x borrar  a auto  │
└─────────────────────────────────────────────────────────────┘
```

`a` en la columna final marca automático. Un túnel `✕` muestra el error en la línea del detalle en rojo y `r` en la barra.

### 6.2 Diálogo de túnel

```
        ┌─ TÚNEL ─────────────────────────────────────────────┐
        │  Host      [ hetzner-01                          ▾ ] │
        │  Nombre    [ pg-prod                               ] │
        │  Tipo      (•) local   ( ) remoto   ( ) dinámico     │
        │  Escucha   [ 127.0.0.1 ] : [ 5432  ]                 │
        │  Destino   [ 10.0.0.5  ] : [ 5432  ]   (no en dinámico)│
        │  [x] automático: se levanta con la primera sesión     │
        │                                                      │
        │  ^s guardar          esc cancelar                    │
        └──────────────────────────────────────────────────────┘
```

Con escucha `0.0.0.0` (o `::`) aparece un aviso en ámbar: «cualquier equipo de tu red podrá usar este túnel» (local/dinámico) o «el host solo lo expondrá si tiene GatewayPorts» (remoto).

### 6.3 Bloque «Túneles» en la ficha

```
│  TÚNELES                                              n nuevo│
│    [a] local     127.0.0.1:5432 → 10.0.0.5:5432   pg-prod   │
│    [ ] remoto    127.0.0.1:9000 → 127.0.0.1:9000  webhook   │
│    ✕ hay 1 reenvío en opciones extra · i importar a túneles │
```

### 6.4 Detalle de túnel caído

```
        ┌─ webhook · hetzner-01 · remoto ──────────────────────┐
        │  ✕ Caído desde 15:40:12                              │
        │    el host rechazó el reenvío de 127.0.0.1:9000      │
        │    (¿puerto ocupado en el host?)                     │
        │  Conexiones 0 (12 en total) · ↓ 2.1 MB ↑ 40 kB       │
        │  r relanzar        espacio descartar                 │
        └──────────────────────────────────────────────────────┘
```

### Notas de UX y diseño

- Estados: `●` activo, `◐` activando/parando, `○` inactivo, `✕` caído, doble codificación; ASCII `* o x`.
- El detalle muestra «automático» u «manual (ventana N)» como origen; un automático parado a mano pasa a manual hasta el siguiente ciclo.
- Los contadores se difunden como mucho 2 veces por segundo y solo si cambian.
- `F6` con la tabla vacía muestra «Sin túneles: n para crear uno, o importa los reenvíos de tus hosts».
- Al borrar un host (F1) sus túneles se borran en cascada; si alguno está activo, se para antes.

---

## 7. Lógica de Negocio

### 7.1 Validación del túnel

| Campo | Regla |
|---|---|
| nombre | Obligatorio, único por host, `[A-Za-z0-9._-]+` |
| tipo | `local`, `remoto`, `dinamico` |
| escucha | `dirección:puerto`; dirección IPv4/IPv6/`localhost`; puerto 1-65535; aviso con `0.0.0.0`/`::`; en remoto, aviso si < 1024 (el host lo rechazará sin root) |
| destino | Obligatorio en local y remoto (`host:puerto`, nombre DNS o IP); vacío en dinámico |
| duplicado | Dos túneles del mismo tipo con la misma escucha en la misma máquina (local/dinámico: cualquier host; remoto: mismo host) se rechazan |

### 7.2 Ciclo automático

```python
def canal_abierto(host_id):            # primera pestaña o SFTP del host
    if canales_de_pestana_o_sftp(host_id) == 1:
        for t in tuneles_automaticos(host_id):
            if not activo(t.id): activar(t.id, origen="automatico")

def canal_cerrado(host_id):            # última pestaña o SFTP del host
    if canales_de_pestana_o_sftp(host_id) == 0:
        for t in activos(host_id):
            if t.origen == "automatico": parar(t.id, motivo="última sesión cerrada")
```

Los túneles cuentan como canales del pool (mantienen la conexión) pero **no** como «pestaña o SFTP» para este cómputo; así un túnel manual no impide que los automáticos se paren, ni un automático mantiene vivo a otro.

### 7.3 Contadores

Bytes por dirección sumados en `copy_bidirectional` (envolviendo los flujos), conexiones abiertas (inc/dec) y aceptadas (inc). Se reinician al relanzar. En `tunel_cerrado` el detalle lleva el total.

### 7.4 Conversión ssh_config ⇄ TUNELES

| Directiva | Túnel |
|---|---|
| `LocalForward [bind:]puerto host:hostport` | local, escucha `bind:puerto` (bind por defecto `127.0.0.1`), destino `host:hostport` |
| `RemoteForward [bind:]puerto host:hostport` | remoto |
| `DynamicForward [bind:]puerto` | dinámico |

Los reenvíos importados nacen con `automatico = 1` (es lo que hace `ssh` al conectar). La exportación emite las tres directivas por túnel en el bloque del host; en `opciones_extra` pasan a estar prohibidas.

### 7.5 Casos especiales

- Activar un túnel en un host con salto: la conexión del pool ya resuelve el salto; `direct-tcpip` sale desde el host final.
- Túnel dinámico con petición de dominio: se envía el nombre al host (`direct-tcpip` acepta nombres); el DNS se resuelve en el remoto, como `ssh -D`.
- Un túnel remoto con la conexión compartida por pestañas: `cancel_tcpip_forward` al parar no afecta a las pestañas.
- Reenvío remoto con `escucha` `0.0.0.0` y host sin `GatewayPorts`: el host acepta pero escucha solo en loopback; MAGI muestra la dirección que el host confirma (`tcpip_forward` devuelve el puerto real, útil con puerto 0).
- Puerto 0 en escucha (local/dinámico): se asigna uno libre y se muestra; no se persiste.
- Servidor caído: los túneles se pierden con las sesiones; al relanzar, los automáticos vuelven con la siguiente pestaña.
- `magi tunel activar` sin ventana y sin credenciales disponibles (huella desconocida, clave con frase no en el agente, contraseña sin llavero): falla con «abre una sesión desde la TUI primero».

---

## 8. Requisitos No Funcionales

**Seguridad**

- Escucha por defecto en `127.0.0.1`; `0.0.0.0` exige aviso explícito. El SOCKS5 no tiene autenticación: por eso solo debe escuchar en loopback salvo que el usuario decida otra cosa con el aviso.
- Ningún puerto < 1024 se abre en local (bind fallará sin root; mensaje claro).
- Los reenvíos remotos solo aceptan canales `forwarded-tcpip` cuya (dirección, puerto) esté registrada; el resto se rechaza.
- Los contadores y REGISTRO no guardan contenido ni destinos de las conexiones SOCKS (solo totales).

**Protección de datos (RGPD)** — sin cambios.

**Backups y recuperación** — TUNELES viaja con `magi.db`; se exporta también a `magi_config`.

**Rendimiento**

- `copy_bidirectional` con búfer de 64 KiB; objetivo ≥ 50 MB/s en LAN por túnel; 100 conexiones simultáneas por túnel sin degradar la UI.
- `Tuneles{lista}` coalescido a 2/s.

**Accesibilidad** — glifo + palabra + color; todo por teclado.

---

## 9. Integraciones

| Integración | Detalle |
|---|---|
| Host remoto | `direct-tcpip`, `tcpip-forward`, `forwarded-tcpip` (RFC 4254); `AllowTcpForwarding` del host decide |
| Aplicaciones locales | Cualquier cliente TCP (psql, navegador con SOCKS5, curl) |
| ssh_config | `LocalForward`, `RemoteForward`, `DynamicForward` |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D51 — Túneles en el servidor, sobre el pool, contando como canal (T23, T34).** Sobreviven a la ventana y comparten conexión con pestañas y SFTP. Descartado el cliente: morirían con la ventana.
- **D52 — CRUD en el cliente, ejecución en el servidor (T18).** El servidor lee TUNELES con su conexión de solo lectura al activar o al ciclar automáticos; `RecargarTuneles` avisa de cambios. Descartado duplicar el CRUD por protocolo.
- **D53 — SOCKS5 propio, solo CONNECT, sin autenticación.** 80 líneas frente a una dependencia; escucha en loopback por defecto. BIND y UDP no tienen sentido sobre `direct-tcpip`.
- **D54 — Automáticos ligados a pestañas y SFTP, no al servidor.** Es lo que hace `ssh` con `LocalForward`; levantarlos al arrancar el servidor abriría conexiones sin que nadie las pida.
- **D55 — Sin reintento automático en caídas.** Un túnel que reconecta solo puede reabrir un puerto que otra cosa ya ocupa o enmascarar una caída; `r` y el estado `✕` son explícitos. Reconsiderar si molesta.
- **D56 — Reenvíos de ssh_config pasan a la tabla y quedan prohibidos en `opciones_extra`.** Una sola fuente de verdad; la exportación los devuelve al fichero.
- **D57 — `F6` para Túneles.** Snippets (Fase 6) irá a la paleta y a una subvista, porque las siete teclas de función están ocupadas; alternativa: `Shift+F1`-`F7`, a decidir en la F6.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Estimación |
|---|---|---|
| S1 — Modelo y protocolo | Migración 4, `almacen/tuneles.rs`, modelo y validación, `sshconfig/tuneles.rs` (importar/exportar), protocolo v3, tipos de REGISTRO | 0,5 semanas |
| S2 — Servidor | `servidor/tuneles.rs` (local, remoto con handler `forwarded-tcpip`, dinámico con `socks5.rs`), contadores, automáticos ligados a canales, caídas, `Tuneles{lista}`, `servidor estado`, tests contra el `sshd` efímero | 1,5 semanas |
| S3 — Interfaz y CLI | Vista F6, diálogo, bloque en la ficha, aviso e importación a túneles, paleta, `⇅` y barra de Sesión, `magi tunel`/`magi tuneles`, README, revisión adversarial (T31) | 1 semana |

**Total: 3 sprints, ~3 semanas.** Supuestos: el `sshd` efímero de los tests permite `AllowTcpForwarding` (por defecto sí); russh 0.63 expone `tcpip_forward`/`cancel_tcpip_forward` y el handler `server_channel_open_forwarded_tcpip` (sí en 0.4x+).

---

## 12. Conexiones con Otras Fases

- **F1:** `LocalForward`/`RemoteForward`/`DynamicForward` dejan de vivir en `opciones_extra`; la importación y exportación de ssh_config cambian.
- **F3/F4:** pool, solicitante, hilo escritor, difusión de listas y `servidor estado`; los túneles cuentan como canal del pool pero no como pestaña/SFTP para el ciclo automático.
- **F6 (Snippets y deliberación):** un snippet podrá exigir un túnel activo («backup postgres» por 5432); la deliberación verá los túneles activos como contexto. Snippets necesita decidir su tecla (D57).
- **F7 (Sincronización):** TUNELES viaja con el inventario; el estado en vivo no.
- **F8 (Android):** túneles locales en el dispositivo reimplementados; remotos y dinámicos probablemente fuera.
