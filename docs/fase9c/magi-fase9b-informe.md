# MAGI - Fase 9b: Primera Publicación

## Especificación Funcional

**Versión:** 1.2
**Fecha:** 18 de noviembre de 2026
**Cliente:** ninguno (proyecto personal de Hector Espejo; corregido el 22 nov 2026, T151)

**Changelog**
- 22 nov 2026 — v1.2: MAGI es un proyecto personal de Hector, no de 4d3 (T151): cambia la cabecera y D400 queda superada; lo que queda de 4d3 se quita en la rama `release-1.6.1`, antes de etiquetar. La app 0.2.0 se publica después de la 9c, que cambia su `applicationId` (D450 de la 9c), y el keystore se crea sin «Cuatro de Tres». Cambian la cabecera, §4.5, §10, §11 y §12.
- 21 nov 2026 — v1.1: procesado el informe de implementación (55/58, versión 1.6.1). La F17 se fusionó antes, así que la primera versión publicada es la 1.6.1 y lleva Termux «en pruebas», con una candidata `v1.6.1-rc.1` antes (D425, D426; D387 y D401 quedan superadas). La especificación sigue a lo implementado: los borrados aplazados se aplican a mano, como en la TUI (D427); la consulta de `magi sincronizar` (D428); el script de tmux con marca final, código 101 y seguimiento por identificador y hora (D429); los retoques del cierre de la F16 (D430); cuatro ficheros más con la organización anterior (D431); ocho checks obligatorios (D432); el manual con `termux.md` (D433); el resultado de `gitleaks` (D434); la demo (D435) y los scripts medidos frente a Dropbear (D436). Suma los retoques del analista para el prompt de reanudación (D437) y lo que se acepta sin cambios (D438). Cambian §1, §2, §3, §4, §5, §6, §7, §8, §9, §10, §11 y §12.

> Base: especificada en paralelo y sobre una base sin confirmar. La F16 (65/67) está fusionada en `main` (PR #17) sin su cierre, que pasa a esta fase; la F17 está especificada, sin informe, y se implementa a la vez. La F15 sigue a 95/96 y el prompt de cierre de la F14, sin informe. Parte de `main` tal como está (versión 1.5.0). **Implementada (21 nov 2026)** sobre `main` con la F17 ya fusionada (PR #18, versión 1.6.0): sale como 1.6.1 (D425).
>
> Estado (21 nov 2026): implementación reportada, 55/58 (versión 1.6.1, rama `fase9b` sin fusionar ni pasar aún por la CI de GitHub). Faltan los tres pasos de publicación, que hace el desarrollador, y, antes de etiquetar, los retoques del prompt de reanudación (D437).

---

## 1. Visión General

La 9b es **la primera publicación de MAGI**. El repositorio ya es público, en `HectorEspejo/magi`, pero no tiene ninguna release. La F9 dejó preparados los canales (GitHub Releases, AUR y Homebrew) apuntando a `4d3-org`, y la publicación quedó pendiente. Desde entonces llegaron la sincronización, la app Android, los escritorios remotos, los servicios, el terminal y las sesiones persistentes, con versiones que nunca salieron: de la 1.0 a la 1.5 en el escritorio y la 0.1 y la 0.2 en la app.

La fase hace tres cosas, en este orden. Primero **aglutina las correcciones** que deben ir antes: el cierre de la F16 y la corrección 10b de la sincronización. Después **deja el repositorio listo para compartirlo**: referencias, datos, un README corto en castellano e inglés con una animación, el manual, la guía para contribuir, las plantillas y la política de seguridad. Por último **publica** el escritorio 1.6.1, con el `.deb` de Termux de la F17 «en pruebas» (D425, D426), y la app 0.2.0 en beta. El número 9b indica que termina lo que la F9 dejó pendiente; no renumera ninguna fase.

**Objetivos principales**

1. Las correcciones previas, en una sola fase: el cierre de la F16 (D357, R157, D369, D368, D370 y el resto de su §11) y la corrección 10b (R83, R84, R85 y R97).
2. El repositorio, listo para compartir: las referencias funcionales a `4d3-org` pasan a `HectorEspejo`, los ficheros actuales quedan sin datos personales y una prueba en CI impide que vuelvan a entrar.
3. Una puerta de entrada clara: README corto en castellano e inglés con una animación, el manual en `docs/manual/` y `docs/` explicado como archivo del diseño.
4. Contribuciones abiertas: `CONTRIBUTING` en los dos idiomas, plantillas de issue y de PR, `SECURITY.md` al día, `main` protegida y avisos de seguridad de Dependabot.
5. La primera release del escritorio, 1.6.1, por los canales de la F9 y con el `.deb` de Termux, con macOS y Termux «en pruebas» y una candidata antes, y la de la app, 0.2.0 beta, con su versión candidata antes.
6. Un CHANGELOG que diga la verdad: lo no publicado, junto en la primera entrada publicada de cada uno, con un resumen en castellano e inglés.

**Contexto.** Hector quiere distribuir MAGI y compartir el repositorio mientras la F17 (Termux) se implementa en paralelo. El ensayo de la release ya está hecho y validado (Hector, 18 nov 2026). Al revisar el repositorio antes de especificar salieron cuatro cosas: es público, pero no está en `4d3-org`, que es donde apunta lo configurado; no tiene ninguna release; su historial guarda datos personales de antes (en documentos antiguos y en el autor de algunos commits); y en `main` hay tres correcciones hechas fuera del método (la copia con wl-copy, el pegado y el foco del terminal exterior, y age y Flota en Android) que el maestro no recogía.

Respuestas de Hector (18 nov 2026):

| Pregunta | Respuesta |
|---|---|
| 1 | Sale la versión que tenga `main` cuando esté listo (1.5.x, con el cierre de la F16), sin esperar a Termux, que llega en la 1.6.0; el CHANGELOG reúne lo no publicado (a) |
| 2 | Antes de publicar, el cierre de la F16 y la corrección 10b; el resto queda como pendiente conocido. Hector: «lo ponemos en una fase que aglutine todas esas correcciones»: van en esta (a, con su añadido) |
| 3 | Solo CI: el ensayo con una versión candidata ya está validado (c, con la precisión de Hector) |
| 4 | El repositorio se queda en `HectorEspejo/magi` y cambian las referencias a `4d3-org` (b) |
| 5 | Hector: «se queda en este repo». Se toma como b: ni historial reescrito ni repositorio nuevo; se limpian los ficheros actuales |
| 6 | `docs/` entero queda como archivo del diseño, revisado y explicado (a) |
| 7 | Issues y PR abiertas, sin acuerdo de cesión, con `CONTRIBUTING`, plantillas, `main` protegida y Dependabot solo para seguridad (a) |
| 8 | GitHub Releases, AUR `magi-ssh-bin` y el tap de Homebrew; Termux, con la 1.6.0 (a) |
| 9 | macOS sale «en pruebas» tras un guion corto en el mac mini, sin notarizar (a) |
| 10 | La app sale en beta como `android-v0.2.0`, con su `rc.1` antes, APK firmado para Obtainium (a) |
| 11 | README corto con una animación de VHS sobre un inventario de ejemplo, y el manual en `docs/manual/` (a) |
| 12 | README, notas de la release y `CONTRIBUTING` en castellano e inglés; el resto, en castellano (a) |

**Fuera de alcance**

- Transferir el repositorio a una organización (D390) y reescribir el historial (D391).
- Notarizar macOS, canales nuevos (`.deb`, `.rpm`, crates.io, F-Droid o IzzyOnDroid) y una web propia.
- Cerrar la F17: su prueba de concepto, el `.deb` real y el guion en el plegable siguen en su fase; aquí solo se publica en pruebas (D426).
- Traducir la interfaz, el manual o los documentos de fase.
- Los guiones de validación de fase pendientes (R59), los cierres de la F14 y la F15 y subir rusqlite a 0.41 (R134).
- Código de conducta, Discussions y wiki.
- Reescribir el manual: solo se cambia de sitio.

**Versión:** escritorio **1.6.1** (PATCH sobre la 1.6.0 que trajo la F17: correcciones; protocolo v10 con dos campos opcionales, sin migración ni `formato` nuevo; con `v1.6.1-rc.1` antes, D425 y D426) y app **0.2.0** (beta, su primera publicación, con `android-v0.2.0-rc.1` antes).

---

## 2. Arquitectura Técnica

### Stack tecnológico

| Capa | Tecnología | Notas |
|---|---|---|
| Repositorio | GitHub, `HectorEspejo/magi`, público | Regla de `main`, avisos privados de seguridad, Dependabot (solo seguridad), plantillas de issue y PR |
| Releases del escritorio | `dist` 0.33.1 y `release.yml` (F9) | 4 objetivos, `.sha256`, atestaciones y notas del CHANGELOG; tap `HectorEspejo/homebrew-tap` y AUR `magi-ssh-bin` |
| Releases de la app | `android-release.yml` (F11) | APK firmado, `.sha256` y atestación; se instala con Obtainium |
| Releases de Termux | `termux-release.yml` (F17) | El `.deb` con su `.sha256` y su atestación, en la release de la misma etiqueta; en pruebas (D426) |
| Demo | VHS (charmbracelet/vhs), con `ttyd` y `ffmpeg`, en local | `assets/demo/magi.tape` contra el servidor de pruebas del repositorio (F14), con el tema fijo (D435) |
| Documentación | Markdown en el repositorio | `README.md`, `README.en.md`, `docs/manual/`, `CONTRIBUTING.md` y `CONTRIBUTING.en.md` |
| Comprobaciones | Prueba `repositorio_publico` (nueva) y `gitleaks` una vez | Datos reales, referencias y enlaces |
| Sincronización (10b) | Núcleo (`magi-nucleo`) y la CLI | Sin dependencias nuevas |
| Terminal (cierre de la F16) | `terminal_tmux.sh` y un script de enganche mínimo | Por la entrada estándar del `exec`, con la marca final `#MAGI-FIN` (D429) |
| Dependencias | Ninguna nueva | VHS y `gitleaks` son herramientas, no dependencias del proyecto |

### Estructura de carpetas (novedades)

```
README.md · README.en.md                corto: qué es, animación, estado, instalación y enlaces
CONTRIBUTING.md · CONTRIBUTING.en.md    compilar, probar, convenciones y cómo proponer cambios
SECURITY.md                             1.6.x, aviso privado en HectorEspejo/magi, párrafo en inglés
CHANGELOG.md · android/CHANGELOG.md     primera entrada publicada, con resumen bilingüe
.github/ISSUE_TEMPLATE/                 fallo.yml · propuesta.yml · config.yml
.github/pull_request_template.md
docs/README.md                          qué hay en docs/: manual, publicación, formato y archivo de diseño
docs/magi-maestro.md                    el maestro vigente; cada entrega lo sustituye
docs/manual/                            el manual que hoy está en el README, por capítulos, con su índice y termux.md
docs/publicar.md · docs/publicar-android.md   la primera publicación paso a paso, desde HectorEspejo
assets/demo/                            magi.tape (VHS) · inventario de ejemplo · magi.gif
scripts/grabar-demo.sh                  servidor de pruebas en un entorno aislado + VHS → assets/demo/magi.gif
tests/repositorio_publico.rs            datos reales, referencias a 4d3-org y enlaces de la documentación
crates/magi-nucleo/src/sincronizacion/  10b: clave anterior, fichero a medio copiar, versión en la misma transacción
crates/magi-nucleo/scripts/terminal_tmux.sh   cierre de la F16: por la entrada estándar
crates/magi-nucleo/scripts/terminal_enganche.sh   (nuevo) el enganche mínimo con PTY, de pocos KB
```

### Diagrama de arquitectura

```
 GITHUB · HectorEspejo/magi (público)
 ┌────────────────────────────────────────────────────────────────────────────────────────────────┐
 │ main protegida (PR y 8 checks de CI) · issues con plantillas · avisos privados · Dependabot    │
 │ README · README.en · docs/manual · docs/faseN (archivo de diseño) · repositorio_publico en CI  │
 └──────────────┬────────────────────────────────────────────────────────┬────────────────────────┘
                │ tag vX.Y.Z (lo empuja el desarrollador)                │ tag android-vX.Y.Z
                ▼                                                        ▼
 ┌────────────────────────────────────────────────┐      ┌────────────────────────────────────────┐
 │ release.yml (dist 0.33.1): plan · Linux        │      │ android-release.yml: APK firmado con   │
 │ x86_64 y aarch64 · macOS arm64 y x86_64 ·      │      │ el keystore · .sha256 · atestación ·   │
 │ .sha256 · atestaciones · notas del CHANGELOG   │      │ nunca «Latest»                         │
 └──────┬────────────────┬────────────────┬───────┘      └───────────────────┬────────────────────┘
        │                │                │                                  │
        ▼                ▼                ▼                                  ▼
 ┌──────────────┐ ┌──────────────┐ ┌──────────────┐      ┌────────────────────────────────────────┐
 │ GitHub       │ │ tap de       │ │ AUR          │      │ GitHub Releases                        │
 │ Releases     │ │ Homebrew     │ │ magi-ssh-bin │      │ android-v0.2.0-rc.1 y android-v0.2.0   │
 │ v1.6.1       │ │ magi-ssh.rb  │ │ PKGBUILD     │      │ (APK)                                  │
 └──────────────┘ └──────────────┘ └──────────────┘      └────────────────────────────────────────┘
        │                │                │                                  │
        ▼                ▼                ▼                                  ▼
 tarball, en      macOS: brew      Arch y Omarchy:       Android: Obtainium, con las etiquetas
 cualquier Linux  install          yay -S                android-v y los APK magi-android-

 Secretos, uno por workflow: HOMEBREW_TAP_TOKEN (tap HectorEspejo/homebrew-tap), AUR_SSH_PRIVATE_KEY
 y los cuatro del keystore de Android (ANDROID_KEYSTORE_BASE64, ANDROID_KEYSTORE_PASSWORD,
 ANDROID_KEY_ALIAS y ANDROID_KEY_PASSWORD). Todo se verifica con sha256sum -c y gh attestation
 verify --repo HectorEspejo/magi; el APK, también con apksigner verify --print-certs.
 El mismo tag vX.Y.Z dispara termux-release.yml (F17): el .deb de Termux, con su .sha256 y su atestación,
 va a la misma release. La primera vez, como candidata: v1.6.1-rc.1 es una prerelease sin tap ni AUR (D426).


```

**Nada se publica desde un equipo personal.** El agente prepara el código, los documentos y los workflows; el desarrollador ajusta el repositorio, crea las cuentas y los secretos y empuja las etiquetas. Cada etiqueta dispara su workflow, que compila, firma o atesta y publica. El historial no se toca (D391): lo que se protege desde ahora es lo que entra, con la prueba `repositorio_publico` en cada PR.

---

## 3. Modelo de Datos

Sin tablas, columnas ni migración (`user_version` sigue en 11) y sin `formato` nuevo de sincronización. La corrección 10b cambia **cómo** se escribe y se lee, no **qué** se guarda, y el cierre de la F16 añade dos campos opcionales al protocolo v10.

### Diagrama E-R (lo que toca la fase)

```
 ┌──────────────────────────────────────────┐           ┌─────────────────────────────────────────────┐
 │ almacén (magi.db), tablas que viajan     │           │ fichero magi-<uuid>.age de otro equipo      │
 │ GRUPOS · HOSTS · IDENTIDADES · TUNELES · │  importa  │ ninguna identidad coincide: pide la clave   │
 │ SNIPPETS · SINCRONIZACIONES_DIR · …      │◀──────────│ truncado o dañado: aviso y se reintenta     │
 │ el cambio y su versión nueva: en la      │           │   en la siguiente pasada (R85)              │
 │   misma transacción (R84)                │           │ clave anterior: solo de equipos conocidos   │
 │ sin migración (user_version 11)          │           │   y no quitados (R97)                       │
 └──────────────────────────────────────────┘           └─────────────────────────────────────────────┘
                      │ N:1 (la versión es de)                                 │ N:1 (es de)
                      ▼                                                        ▼
 ┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │ EQUIPOS (F10), sin columnas nuevas: los conocidos y no quitados son los únicos cuyos               │
 │ ficheros abre la clave anterior durante sus 30 días (R97)                                          │
 └────────────────────────────────────────────────────────────────────────────────────────────────────┘

 ┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │ servidor de sesiones (memoria): TrabajoServidor por host; magi sincronizar lo consulta por el      │
 │ socket antes de borrar hosts y túneles; sin respuesta u otra versión, los aplaza todos (R83)       │
 └────────────────────────────────────────────────────────────────────────────────────────────────────┘

 Protocolo v10, campos opcionales (cierre de la F16): InfoSesion.tmux.creada · AperturaTmuxFallida.causa
 = no_existe. Sin tablas, columnas ni formato de sincronización nuevos; REGISTRO sin tipos nuevos.


```

### 3.1 Sincronización: lo que cambia con la 10b

| Riesgo | Hoy | Con la 10b |
|---|---|---|
| R83 | `magi sincronizar` aplica los borrados de hosts y túneles sin mirar si tienen trabajo vivo | Pregunta al servidor por el socket, como `magi servidor estado`. Sin servidor, aplica; con trabajo vivo en el host, aplaza ese borrado; con otra versión o sin respuesta en 5 s, aplaza todos los de hosts y túneles |
| R84 | El cambio de un elemento y su versión nueva se escriben en dos pasos | En la misma transacción, en todas las tablas que viajan |
| R85 | Un fichero truncado y uno de otra clave dan el mismo error, y los dos piden una clave nueva | Solo «ninguna identidad coincide» pide la clave; lo demás es «a medio copiar o dañado», con aviso y reintento en la siguiente pasada |
| R97 | Durante los 30 días tras una rotación, la clave anterior abre el fichero de cualquier equipo | La clave anterior solo abre ficheros de equipos conocidos y no quitados; el de un equipo desconocido se rechaza con aviso |

La corrección de age de la app (rama `correccion-age-android`, ya en `main`) explicó en castellano los errores de cifrado; R85 lleva al núcleo la separación entre «otra clave» y «fichero dañado», que vale igual en el escritorio y en la app.

### 3.2 Protocolo cliente ⇄ servidor (v10, campos opcionales)

| Mensaje | Cambio | Para qué |
|---|---|---|
| `AperturaTmuxFallida` | `causa` admite `no_existe` (D349) | La pestaña sabe que la sesión ya no estaba |
| `InfoSesion.tmux` | `creada` (opcional) (D369) | Solo la ventana cuya pestaña creó la sesión guarda la ficha `origen = host` (R163) |
| `AperturaTmuxFallida` | Aviso cuando `crear` funciona y la apertura falla después | La sesión existe en el host aunque la pestaña no se abriera |

`VERSION_PROTOCOLO` sigue en 10: los campos son opcionales y un servidor anterior (1.5.0 o 1.6.0) los ignora.

### 3.3 Fuera de la base de datos

- **El repositorio:** los ficheros de §5.1, la regla de `main` y los ajustes de seguridad (§5.5).
- **Los canales:** las releases `v1.6.1` y `android-v0.2.0`, cada una con su `rc.1` antes (la del escritorio, con el `.deb` de Termux), la fórmula del tap y el PKGBUILD de AUR.

### 3.4 Relaciones

- Cada elemento que viaja guarda la versión del equipo que lo cambió (N:1 con EQUIPOS), y desde ahora la escribe en la misma transacción que el cambio.
- Cada fichero de la carpeta es de un equipo (N:1). Con la clave anterior solo cuentan los equipos conocidos y no quitados.
- Un borrado de host o de túnel que llega por la sincronización depende del trabajo vivo de ese host en el servidor de sesiones (memoria, sin tabla).

### 3.5 Diagramas de estados

**Versión del escritorio**

```
  sin_publicar ─ CI verde en main · versión X.Y.Z-rc.N · tag vX.Y.Z-rc.N ─▶ candidata (prerelease con los
                tarballs y el .deb; sin tap ni AUR)
  candidata ─ el .deb, con el guion de la F17 en el plegable: va ─▶ lista
  candidata ─ el .deb falla de forma grave ─▶ lista sin Termux (termux-release.yml desactivado, D426)
  lista ─ versión X.Y.Z · tag vX.Y.Z del desarrollador ─▶ construyendo (release.yml y termux-release.yml)
  construyendo ─ los 4 objetivos, el .deb, .sha256 y atestaciones ─▶ publicada (GitHub Release «Latest»)
  publicada ─ fórmula en el tap y PKGBUILD en AUR ─▶ en_los_canales
  construyendo · publicada ─ un job falla ─▶ incompleta ─ se relanza ese job ─▶ publicada · en_los_canales
  en_los_canales ─ sale la siguiente PATCH o MINOR ─▶ sustituida (solo se mantiene la última MINOR)
  Inválidas: etiquetar sin CI verde · publicar desde un equipo personal · mover o reutilizar una etiqueta ·
             que una etiqueta android-v* dispare release.yml · retirar una release publicada en vez de sacar una PATCH ·
             publicar el primer .deb de Termux sin candidata
```

**Versión de la app**

```
  sin_publicar ─ tag android-v0.2.0-rc.1 ─▶ candidata (pre-release, versionCode 2001)
  candidata ─ guion corto en el plegable con Obtainium: va ─▶ lista ─ tag android-v0.2.0 ─▶ publicada (2009)
  candidata ─ algo falla ─▶ corrección ─ tag android-v0.2.0-rc.2 ─▶ candidata (2002)
  publicada ─ sale la siguiente ─▶ sustituida
  Inválidas: publicar la final sin candidata · un versionCode que baja · firmar fuera de android-release.yml ·
             marcar como «Latest» una release de la app
```

**Fichero de otro equipo, al importarlo**

```
  en_la_carpeta ─ se descifra con la clave actual ─▶ leído ─ firma y equipo correctos ─▶ importado
  en_la_carpeta ─ «ninguna identidad coincide» ─▶ ¿clave anterior vigente?
        ├─ no ─▶ pide_clave (como hasta ahora)
        └─ sí ─▶ ¿el equipo del nombre del fichero es conocido y no está quitado?
                    ├─ sí ─▶ se descifra con la anterior ─▶ leído
                    └─ no ─▶ rechazado («de un equipo que este no conocía, con la clave anterior»)
  en_la_carpeta ─ cualquier otro error de age, de tamaño o de firma ─▶ dañado («a medio copiar o dañado»)
  dañado ─ siguiente pasada ─▶ en_la_carpeta (se reintenta; nunca pide clave)
  Inválidas: pedir una clave nueva por un fichero truncado · aceptar con la clave anterior un equipo desconocido ·
             parar la pasada entera por un fichero dañado
```

**Borrado de un host o de un túnel que llega por la sincronización**

```
  recibido ─ en la TUI ─▶ (F10) aplazado si el servidor dice que tiene trabajo vivo · aplicado si no
  recibido ─ en magi sincronizar ─▶ ¿responde el servidor por el socket?
        ├─ no hay servidor ─▶ aplicado (sin servidor no hay trabajo vivo)
        ├─ misma versión ─ el host tiene trabajo ─▶ aplazado · sin trabajo ─▶ aplicado
        └─ otra versión o sin respuesta en 5 s ─▶ aplazado (todos los de hosts y túneles)
  aplazado ─ en CONFLICTOS, tecla a sin trabajo vivo en el host ─▶ aplicado (la pasada del escritorio no lo
             reintenta sola; el lector del móvil, sí: D427)
  Inválidas: aplicar el borrado de un host con pestañas, túneles o transferencias · tomar «sin respuesta» por
             «sin trabajo» · arrancar el servidor para preguntarle
```

**Reconexión de una pestaña persistente (cierre de la F16)**

```
  reconectando ─ el script comprueba y engancha en un solo paso, solo si está libre o es de este equipo ─▶ enganchada
  reconectando ─ la tiene otro equipo (sale con 101) ─▶ soltada («ahora la usa sobremesa»)
  reconectando ─ ya no existe (75) ─▶ cerrada · la ficha pasa a terminada
  soltada ─ el otro equipo la renombra ─▶ soltada (sabe qué equipo la tiene y con qué nombre)
  la sesión se sigue por su identificador y su hora de creación (con tmux reiniciado, los identificadores se
  reutilizan: D429)
  Inválidas: engancharse con -d a una sesión que otro equipo tomó entre comprobar y enganchar ·
             que una reconexión automática suelte a otro equipo
```

**Aviso de seguridad**

```
  recibido (aviso privado de GitHub) ─ se reproduce ─▶ confirmado ─ PATCH con la corrección ─▶ publicado
                                                                    (aviso de seguridad y línea en «Seguridad»)
  recibido ─ no es una vulnerabilidad o queda fuera del alcance ─▶ descartado (con respuesta y motivo)
  recibido como issue pública ─ se quitan los detalles y se pide el aviso privado ─▶ recibido
  Inválidas: discutir los detalles en una issue pública antes de la corrección · publicar el aviso sin la PATCH
```

---
## 4. Flujos de Trabajo

### 4.1 Corrección 10b: importar y borrar

```
  pasada (TUI o magi sincronizar) ─▶ cada magi-<uuid>.age de la carpeta, menos el propio
      ├─ se abre con la clave actual ─▶ firma, equipo y formato (F10) ─▶ fusión
      ├─ «ninguna identidad coincide» ─▶ ¿clave anterior vigente?
      │       ├─ sí, y el equipo del nombre es conocido y no está quitado ─▶ se abre con ella ─▶ fusión
      │       ├─ sí, pero el equipo no es conocido ─▶ rechazado · aviso · log sin contenido
      │       └─ no ─▶ pide la clave (F10)
      └─ otro error (truncado, cabecera, MAC, tamaño) ─▶ «a medio copiar o dañado» · sigue con los demás
  fusión ─▶ almacén: cada cambio con su versión nueva, en una transacción (R84)
      └─ borrados de hosts y túneles ─▶ ¿trabajo vivo? (TUI: lista del servidor · CLI: consulta por el socket)
              ├─ sí, o el servidor no responde o es de otra versión (CLI) ─▶ aplazados a CONFLICTOS · «N borrados aplazados»
              └─ no ─▶ aplicados con lápida (F10)
  escribe el magi-<uuid>.age propio (F10)
```

1. La consulta de la CLI va por el socket, con plazo de 5 s, y nunca arranca el servidor: tras la bienvenida pide `Listar` y lee hasta su respuesta, sin el segundo de espera de `leer_bienvenida` (D428). Solo se hace si hay clave (sin clave no hay borrados que decidir) y fuera del cerrojo; cada fallo aplaza. «Otra versión» incluye una `Bienvenida` del mismo protocolo con otra versión de la app, y un túnel `Parando` cuenta como trabajo vivo.
2. Un borrado aplazado queda en CONFLICTOS y se aplica a mano con `a` cuando el host ya no tiene trabajo vivo, como en la TUI (T73): la pasada del escritorio salta los ficheros cuyo hash no cambió y no lo reintenta sola; solo el lector del móvil lo hace (D427). Un plan parcial no da por «recuperado» un aplazado (NUC-1 de la revisión adversarial, corregido).
3. El aviso de un fichero dañado dice de qué equipo es (por el nombre del fichero) y cuándo se reintenta; no pide nada.
4. Un equipo `danado` vuelve a `al_dia` si su fichero recupera el hash ya importado, y el lector del móvil cuenta los ficheros a medio copiar (`incompletos`). Un error de E/S al leer el fichero de un equipo conocido sigue parando la pasada entera, como antes (D438).

### 4.2 Cierre de la F16: el script por la entrada estándar

```
  subórdenes sin PTY (comprobar, crear, estado, vista, historial, renombrar, terminar)
      ▼ exec con el arranque (OrdenTmux::ARRANQUE) en la línea de órdenes + los argumentos escapados
  el arranque lee el script entero de la entrada estándar y solo entonces lo ejecuta (entrada cerrada después)
      ├─ no llega la marca final #MAGI-FIN ─▶ sale con 70 sin ejecutar nada (D429)
      ├─ la línea de órdenes de Dropbear es corta ─▶ ya no importa: ahí solo van los argumentos
      └─ salida y códigos de salida, los de la F16
  enganchar ─▶ 1) preparación por la entrada estándar: comprobar · configuración · id y hora de creación
               2) terminal_enganche.sh con PTY (4 KB): guarda · @magi_equipo · attach-session, con -d si no se comparte
  reconexión ─▶ en el mismo paso, engancharse solo si está libre o es de este equipo
      ├─ la tiene otro equipo ─▶ sale con 101 ─▶ soltada (sabe qué equipo y con qué nombre)
      └─ libre o de este equipo ─▶ enganchada
```

El resto del cierre son retoques de la F16 (§11 de su informe v1.2): el aviso de atención con la lista vieja (D368), los de interfaz de D370, la ayuda sobre «tmux dentro de tmux» (D367), el aviso de una pestaña que alguien mira, `nombre_valido` con una lista explícita (R159), las validaciones de la importación, la prueba de `escribir_tuneles` (R160), las etiquetas de CONFLICTOS y REGISTRO y las pruebas con tmux real.

**Implementación (D429, D430).** El script se envía con `canal.data()` antes del `eof()`, dentro del plazo, y los tres ejecutores que estaban duplicados van ahora por `lanzar_exec`. tmux no tiene «comprobar y enganchar» atómico: la guarda y el `attach` van en el mismo script y la ventana queda en milisegundos; sin guarda (`prefijo r`, a mano) manda el usuario. La marca `@magi_equipo` la pone el enganche, después de la guarda. Los ganchos nuevos del tmux falso están en `tests/fijos/terminal/README.md`.

### 4.3 Preparar el repositorio

```
  referencias: 4d3-org/magi → HectorEspejo/magi · 4d3-org/homebrew-tap → HectorEspejo/homebrew-tap
      ├─ Cargo.toml, README, SECURITY, CHANGELOG, dist-workspace.toml, release.yml, publicar-aur.yml,
      │  android-release.yml, PKGBUILD, docs/publicar*.md, CLAUDE.md y AGENTS.md, y tres ficheros de la F17
      │  (la receta de TUR, paquete-termux.sh y termux_paquete.rs: D431)
      └─ docs/faseN/ (archivo): no se tocan salvo los datos personales; docs/README.md explica que dicen 4d3-org
         y que esas direcciones ya no valen (si 4d3-org está libre en GitHub, se reserva: R182)
  datos: la ruta personal del desarrollador → /home/usuario (informes de implementación de la F9 y la F11)
      ▼ gitleaks sobre todo el historial, una vez ─ un secreto real ─▶ PARAR y avisar: se revoca (D391)
  repositorio_publico en CI ─ un dato real o una referencia a 4d3-org fuera del archivo ─▶ falla con fichero y línea
  documentación: README corto + README.en.md · el manual, a docs/manual/ con el mismo texto · enlaces comprobados
  comunidad: CONTRIBUTING (ES y EN) · plantillas de issue y de PR · SECURITY.md · docs/publicar.md §4
  demo: scripts/grabar-demo.sh ─▶ servidor de pruebas en un entorno aislado ─▶ VHS ─▶ assets/demo/magi.gif
      └─ más de 2 MiB ─▶ se recorta la grabación (menos pasos o menos color), no se sube
```

### 4.4 Publicar el escritorio 1.6.1

```
  agente: 9b terminada, CI verde e informe de implementación con los pasos del desarrollador en orden
  desarrollador: ajustes del repositorio y secretos (§5.5) · comprobación de nombres en AUR
      ├─ un nombre ocupado ─▶ PARAR: se elige otro antes de publicar
      ▼ guion corto de macOS en el mac mini (§11)
      ├─ algo grave (no arranca, no conecta) ─▶ fuera los objetivos de macOS de esta release (D395)
      ▼ fusionar la 9b · PR con la versión 1.6.1-rc.1 · git tag -a v1.6.1-rc.1 · git push origin v1.6.1-rc.1
  release.yml y termux-release.yml ─▶ prerelease: tarballs, .sha256, atestaciones y el .deb (sin tap ni AUR)
      ▼ el .deb en Termux, con el guion de la F17
      ├─ falla algo grave (no instala, no arranca, no conecta) ─▶ Termux sale de esta release:
      │    termux-release.yml desactivado antes de v1.6.1 y el README y el manual sin ofrecerlo (D426)
      ▼ PR con la versión 1.6.1 · git tag -a v1.6.1 · git push origin v1.6.1
  release.yml ─▶ GitHub Release «Latest» ─▶ fórmula en el tap ─▶ PKGBUILD en AUR · termux-release.yml ─▶ .deb
      ├─ un job falla ─▶ se relanza solo ese (docs/publicar.md §7); la release no se retira
      ▼ comprobaciones de docs/publicar.md §9: yay, brew y el tarball verificado · después, la receta de TUR
```

### 4.5 Publicar la app 0.2.0

```
  desarrollador: keystore y sus cuatro secretos (docs/publicar-android.md)
      ▼ android/version.properties = 0.2.0-rc.1 (PR, CI verde) · git tag -a android-v0.2.0-rc.1
  android-release.yml ─▶ pre-release con el APK firmado (versionCode 2001), nunca «Latest»
      ▼ guion corto en el plegable: Obtainium con prereleases ─▶ instalar ─▶ desbloquear ─▶ unir ─▶ terminal y túnel
      ├─ algo falla ─▶ corrección ─▶ 0.2.0-rc.2 (2002) y otra vuelta
      ▼ version.properties = 0.2.0 (PR) · git tag -a android-v0.2.0 ─▶ release final (2009)
```

Desde la 1.5, un móvil necesita la app que lee el `formato` 4 (R153): por eso sale a la vez que el escritorio. **Cambia (22 nov 2026, D450 de la 9c):** la app sale después de la 9c, con el `applicationId` `io.github.hectorespejo.magi`; como nunca se ha publicado una app, nadie se queda con una que no lea el `formato` 4.

### 4.6 Contribuir

```
  alguien encuentra un fallo ─▶ issue con la plantilla «Fallo» (versión, sistema, pasos; sin datos privados)
  una idea ─▶ plantilla «Propuesta» ─ cambio grande ─▶ se especifica como fase con este método (T144)
  PR ─▶ su plantilla: pruebas, línea en «Sin publicar», manual al día, sin datos reales
      ▼ CI: los 8 checks obligatorios de ci.yml (y android.yml o termux si toca lo suyo) ─ falla ─▶ se corrige
      ▼ revisión del desarrollador ─▶ fusión ─▶ la rama se borra sola
  Dependabot (seguridad) ─▶ PR ─ toca una versión fijada (russh, ratatui, crossterm…) ─▶ se cierra y se hace a mano (R179)
```

### 4.7 Avisar de una vulnerabilidad

```
  SECURITY.md ─▶ Security → Report a vulnerability (aviso privado de GitHub)
      ▼ confirmar y reproducir ─▶ corrección en privado ─▶ PATCH ─▶ aviso publicado y línea en «Seguridad»
  una vulnerabilidad en una issue pública ─▶ se editan los detalles fuera y se pide el aviso privado
```

### 4.8 Diagrama de secuencia: `magi sincronizar` con trabajo vivo

```
magi sincronizar                magi --servidor                 magi.db                         carpeta
   │ ¿hay servidor? (socket)            │                               │                           │
   │─Hola · bienvenida · Listar────────▶│                               │                           │
   │◀────── trabajo: web-01 (1 pestaña) │                               │                           │
   │ plazo 5 s; sin respuesta           │                               │                           │
   │   u otra versión: aplazar          │                               │                           │
   │   todos los de hosts/túneles       │                               │                           │
   │─leer magi-*.age───────────────────────────────────────────────────────────────────────────────▶│
   │◀───────────────────────────────────────────────────────────────────────────────────── ficheros │
   │ fusión: 3 cambios y                │                               │                           │
   │   borrar web-01                    │                               │                           │
   │─cambios + versión (1 transacción)─────────────────────────────────▶│                           │
   │ borrar web-01: aplazado            │                               │                           │
   │─escribir magi-<uuid>.age──────────────────────────────────────────────────────────────────────▶│
   │ «1 borrado aplazado» · código 0    │                               │                           │
```

Solo se pregunta si hay clave; el servidor contesta a `Listar` con la lista de sesiones, que da el trabajo vivo por host (D428). El aplazado queda en CONFLICTOS (D427).

### 4.9 Diagrama de secuencia: publicar la 1.6.1

```
desarrollador           GitHub                release.yml (dist)            Releases                  tap y AUR
   │ CI verde en main       │                         │                             │                       │
   │─git push v1.6.1───────▶│                         │                             │                       │
   │                        │─dispara (tag)──────────▶│                             │                       │
   │                        │                         │ plan · 4 objetivos          │                       │
   │                        │                         │─tarballs · .sha256 · notas─▶│                       │
   │                        │                         │ atestaciones (Sigstore)     │                       │
   │                        │                         │─Formula/magi-ssh.rb────────────────────────────────▶│
   │                        │                         │ con HOMEBREW_TAP_TOKEN      │                       │
   │                        │                         │─PKGBUILD a AUR─────────────────────────────────────▶│
   │                        │                         │ con AUR_SSH_PRIVATE_KEY     │                       │
   │◀────────────────────────────────────── terminado │                             │                       │
   │ comprobaciones §9:     │                         │                             │                       │
   │   yay · brew · tarball │                         │                             │                       │
   │ si un job falla, se    │                         │                             │                       │
   │   relanza solo ese     │                         │                             │                       │
```

El mismo tag dispara `termux-release.yml`, que sube a esa release el `.deb` con su `.sha256` y su atestación (F17). Antes va la candidata `v1.6.1-rc.1`: la misma secuencia, como prerelease y sin el tap ni AUR (D426).

---

## 5. Repositorio, Workflows, CLI y Protocolo

### 5.1 Ficheros del repositorio

| Fichero | Contenido | Idioma |
|---|---|---|
| `README.md` | Qué es, la animación, estado, instalación por canal con verificación, primeros pasos, enlaces y licencia; 200 líneas como mucho | Castellano |
| `README.en.md` | Lo mismo | Inglés |
| `docs/manual/README.md` y sus capítulos | El manual que estaba en el README, sin reescribir (tabla de abajo) | Castellano |
| `docs/manual/estado.md` | Qué se ha probado a mano y qué no, por plataforma y por fase (R59) | Castellano |
| `docs/README.md` | Qué hay en `docs/` y cómo se lee el archivo de diseño (especificación, checklist e informe de implementación de cada fase) | Castellano |
| `docs/magi-maestro.md` | El maestro vigente: visión global y estado del proyecto; cada entrega lo sustituye, y la copia de cada una queda en `docs/faseN/` | Castellano |
| `CONTRIBUTING.md` · `CONTRIBUTING.en.md` | Compilar y probar, convenciones (castellano en el código y la interfaz), commits, sin acuerdo de cesión (lo que entra lo hace con MIT OR Apache-2.0), propuesta antes de cambios grandes, agentes de código (`CLAUDE.md`, `AGENTS.md`) | Castellano · inglés |
| `SECURITY.md` | Se mantiene la 1.6.x (D425); aviso privado en `HectorEspejo/magi`; alcance; un párrafo en inglés de cómo avisar | Castellano |
| `.github/ISSUE_TEMPLATE/fallo.yml` | Versión (`magi --version`), sistema y terminal, qué pasó, pasos, qué se esperaba, `magi diagnostico` y la casilla de datos privados | Castellano, con el inglés debajo |
| `.github/ISSUE_TEMPLATE/propuesta.yml` | Problema, propuesta y alternativas | Castellano, con el inglés debajo |
| `.github/ISSUE_TEMPLATE/config.yml` | Sin issues en blanco; enlace al aviso privado | — |
| `.github/pull_request_template.md` | Pruebas, «Sin publicar», manual, sin datos reales | Castellano e inglés |
| `assets/demo/` | `magi.tape`, `inventario` (ssh_config de ejemplo) y `magi.gif` | — |
| `scripts/grabar-demo.sh` | Arranca el servidor de pruebas en un entorno aislado y graba la animación con VHS | — |

Capítulos del manual (el texto pasa tal cual; solo cambian los enlaces):

| Capítulo | Secciones del README de hoy |
|---|---|
| `instalacion.md` | Requisitos, Instalación (AUR, Homebrew, tarball, verificación, desde el código), Actualizar MAGI, Desinstalación, Autocompletado y man en macOS |
| `primeros-pasos.md` | Primeros pasos, Primer arranque, Diagnóstico |
| `servidor-de-sesiones.md` | El servidor de sesiones |
| `pestanas-y-terminal.md` | Pestañas, Tamaño de la terminal, Terminal: paneles, historial y Claude Code |
| `sesiones-persistentes.md` | Sesiones persistentes, con el JSON de `magi sesiones` |
| `archivos.md` · `tuneles.md` | Archivos · Túneles |
| `snippets-y-deliberacion.md` | Snippets, Deliberación MAGI |
| `flota-identidades-y-registro.md` | Sondeo de flota, Autenticación, Identidades y registro |
| `sincronizacion.md` | Sincronización del inventario |
| `escritorio-remoto.md` · `servicios.md` | Escritorio remoto · Servicios, contenedores y logs |
| `app-android.md` | App Android |
| `termux.md` | MAGI en Termux: lo que la F17 dejó en el README (D401, D433) |
| `atajos.md` · `ssh-config.md` · `seguridad.md` | Atajos · El `Include` de `~/.ssh/config` · Seguridad |
| `CONTRIBUTING.md` | Desarrollo, Tests |

### 5.2 Referencias

| Antes | Después |
|---|---|
| `4d3-org/magi` | `HectorEspejo/magi` |
| `4d3-org/homebrew-tap` · `brew install 4d3-org/tap/magi-ssh` | `HectorEspejo/homebrew-tap` · `brew install hectorespejo/tap/magi-ssh` |
| `gh attestation verify … --repo 4d3-org/magi` | `gh attestation verify … --repo HectorEspejo/magi` |
| `4d3-org@users.noreply.github.com` (PKGBUILD y commits a AUR) | `hola@4d3.org` |
| Enlaces de los CHANGELOG | `https://github.com/HectorEspejo/magi/…` |

Cambian en los ficheros que se usan: `Cargo.toml` (los tres paquetes), `README.md`, `SECURITY.md`, `CHANGELOG.md`, `android/CHANGELOG.md`, `dist-workspace.toml`, `release.yml` (el tap), `publicar-aur.yml`, `android-release.yml`, `packaging/aur/PKGBUILD`, `docs/publicar.md`, `docs/publicar-android.md`, `CLAUDE.md` y `AGENTS.md`. Los documentos de `docs/faseN/` son el archivo de lo que se decidió entonces: no se tocan, salvo para quitar datos personales (§4.3). `release.yml` se cambia a mano, como dice `docs/publicar.md` (D148), y `dist plan --tag v1.6.1` tiene que pasar. Cambian también cuatro ficheros que la F17 añadió después: `docs/publicar-termux.md`, `packaging/termux/build.sh`, `scripts/paquete-termux.sh` y `tests/termux_paquete.rs`; el mantenedor de la receta de TUR pasa a `@HectorEspejo` (D431). `docs/publicar.md` no nombra la organización anterior: manda a `docs/README.md`.

### 5.3 CLI: `magi sincronizar`

```
$ magi sincronizar
Sincronización · carpeta ~/Sync/magi
  importados: portatil (2 cambios) · sobremesa (al día)
  ! magi-3f0a….age: a medio copiar o dañado; se reintenta en la siguiente pasada
  aplazado: borrar web-01 (tiene trabajo vivo en el servidor de sesiones)
  exportado: magi-6b1e….age
1 aviso · 1 borrado aplazado
```

Un borrado aplazado no es un error: el código de salida sigue siendo 0. `magi sincronizar estado` lista los aplazados con su motivo. Con un servidor de otra versión o que no responde, la línea dice «aplazados los borrados de hosts y túneles: no se pudo preguntar al servidor». Los aplazados quedan en CONFLICTOS (D427).

### 5.4 Protocolo

Los cambios de §3.2, dentro de v10. El servidor 1.6.1 sigue hablando con un cliente 1.6.0 o 1.5.0, y al revés: los campos nuevos son opcionales.

### 5.5 Ajustes del repositorio y secretos (los aplica el desarrollador)

| Ajuste | Valor |
|---|---|
| Avisos privados de vulnerabilidades | Activados |
| Dependabot | Alertas y actualizaciones de seguridad activadas; sin `dependabot.yml` (nada de actualizaciones de versión) |
| Regla de `main` | PR obligatoria; obligatorios ocho checks de `ci.yml`, con el nombre con que salen en la PR (`docs/publicar.md` §4 los lista): `formato`, `clippy`, los dos de la matriz `tests-linux`, `macos`, `visor`, `deny` y `aur-plantilla`; sin `push --force` ni borrado. Los de `android.yml` y los dos condicionales de la F17 (`cambios-termux` y `termux`) no se exigen: tienen filtros de rutas y bloquearían las PR que no los disparan (D432) |
| Ramas | Borrar la rama al fusionar; borrar las ya fusionadas (`fase*`, `correccion-*`), menos las abiertas |
| Issues | Activadas; Discussions y wiki, desactivadas |
| Descripción y temas | Una línea de qué es; temas `ssh`, `tui`, `terminal`, `rust`, `sftp`, `rdp`, `vnc` y `android` |
| `HOMEBREW_TAP_TOKEN` | Token de grano fino solo para `HectorEspejo/homebrew-tap`, Contents de lectura y escritura, un año |
| `AUR_SSH_PRIVATE_KEY` | Clave dedicada, registrada solo en la cuenta de AUR |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | El keystore de la app y sus claves (F11) |

El agente no cambia ajustes, no crea secretos ni cuentas y no empuja etiquetas: lo deja escrito en `docs/publicar.md` §4 y en su informe.

### 5.6 Versiones

| Pieza | Versión | Dónde |
|---|---|---|
| Escritorio | 1.6.1, con `1.6.1-rc.1` antes (D425, D426) | `Cargo.toml` de `magi-ssh` y `Cargo.lock`, `FECHA_DE_VERSION`, la DA2 de `tui_prueba` (`10601`), las instantáneas `servidor_anterior_*` y `assets/`; la candidata, como dice `docs/publicar.md` §4 |
| App | 0.2.0, con `0.2.0-rc.1` antes | `android/version.properties`; versionCode 2001 y 2009 |
| Termux | El `.deb` de la misma versión (`Version: 1.6.1~rc.1` en la candidata) | Lo arma `termux-release.yml` con la etiqueta; `TERMUX_PKG_VERSION` de `packaging/termux/build.sh`, después de la estable |
| Protocolo · base de datos · sincronización | v10 · 11 · `formato` 4 | Sin cambios |

---

## 6. Interfaz de Usuario

### Mapa de navegación

```
 GITHUB · HectorEspejo/magi
 ├─ README.md (castellano) ⇄ README.en.md (English)
 │   ├─ Instalación ─▶ docs/manual/instalacion.md (verificación, desde el código, actualizar, desinstalar)
 │   ├─ Estado ─▶ docs/manual/estado.md (qué se ha probado a mano y qué no)
 │   ├─ Manual ─▶ docs/manual/README.md ─▶ primeros pasos · servidor · pestañas y terminal · sesiones persistentes ·
 │   │                                    archivos · túneles · snippets · flota · sincronización · escritorios ·
 │   │                                    servicios · app Android · Termux · atajos · ssh_config · seguridad
 │   ├─ Contribuir ─▶ CONTRIBUTING.md ⇄ CONTRIBUTING.en.md
 │   ├─ Seguridad ─▶ SECURITY.md ─▶ Security → Report a vulnerability (aviso privado)
 │   └─ Licencia ─▶ LICENSE-MIT · LICENSE-APACHE
 ├─ Issues ─▶ [FALLO] · [PROPUESTA] (sin issue en blanco; enlace al aviso privado)
 ├─ Pull requests ─▶ plantilla · los 8 checks de ci.yml obligatorios
 ├─ Releases ─▶ v1.6.1-rc.1 (prerelease) y v1.6.1 («Latest»; escritorio y .deb de Termux) ·
 │               android-v0.2.0-rc.1 y android-v0.2.0 (nunca «Latest»)
 └─ docs/ ─▶ docs/README.md ─▶ manual · publicar.md · publicar-android.md · formato-sincronizacion.md ·
                               faseN/ (archivo de diseño)
 MAGI (TUI) ── ? ayuda: «tmux dentro de tmux» (D367) · SINCRONIZACIÓN (F10): avisos y borrados aplazados de la 10b
```

### 6.1 README en GitHub

```
┌ github.com/HectorEspejo/magi · README.md ──────────────────────────────────────────────┐
│ MAGI                                                             English · Castellano  │
│ Gestor SSH de terminal, operado por teclado: inventario, pestañas, SFTP, túneles,      │
│ snippets con deliberación, sincronización cifrada, escritorios remotos y servicios.    │
│                                                                                        │
│ ┌──────────────────────────────────────────────────────┐                               │
│ │ [animación] Flota → pestaña → Archivos → Servicios → │  texto alternativo:           │
│ │ paleta, con tres hosts de ejemplo (assets/demo)      │  «MAGI en una terminal…»      │
│ └──────────────────────────────────────────────────────┘                               │
│ Estado: Linux, uso diario · macOS, en pruebas · app Android, beta · Termux, en pruebas │
│                                                                                        │
│ Instalación                                                                            │
│   Arch y Omarchy   yay -S magi-ssh-bin                                                 │
│   macOS            brew install hectorespejo/tap/magi-ssh                              │
│   Otros Linux      tarball de la última release (sha256sum -c · gh attestation verify) │
│   Android          Obtainium con este repositorio y las etiquetas android-v            │
│   Termux           el .deb de la release, en pruebas (manual: termux.md)               │
│                                                                                        │
│ Primeros pasos · Manual · Qué se ha probado · Contribuir · Seguridad · Licencia        │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Página de la release

```
┌ Releases · MAGI 1.6.1 ──────────────────────────────────────────── Latest ─┐
│ v1.6.1 · primera versión publicada / first published release               │
│ Resumen. Gestor SSH de terminal para Linux y macOS, con app Android…       │
│ Summary. A keyboard-driven terminal SSH manager for Linux and macOS…       │
│ Qué trae · Añadido · Cambiado · Corregido · Seguridad                      │
├────────────────────────────────────────────────────────────────────────────┤
│ Assets                                                                     │
│   magi-ssh-x86_64-unknown-linux-gnu.tar.xz      .sha256                    │
│   magi-ssh-aarch64-unknown-linux-gnu.tar.xz     .sha256                    │
│   magi-ssh-aarch64-apple-darwin.tar.xz          .sha256                    │
│   magi-ssh-x86_64-apple-darwin.tar.xz           .sha256                    │
│   magi-ssh_1.6.1_aarch64.deb (Termux)           .sha256                    │
│   dist-manifest.json · Source code (zip, tar.gz)                           │
│ Atestaciones: gh attestation verify <fichero> --repo HectorEspejo/magi     │
└────────────────────────────────────────────────────────────────────────────┘
```

Las notas salen de la entrada del CHANGELOG: el resumen en los dos idiomas, una lista corta de qué trae por áreas y, debajo, el detalle.

### 6.3 Plantilla «Fallo»

```
┌ Nueva issue · Fallo / Bug report ──────────────────────────────────────┐
│ No pegues IPs, nombres de host reales, claves ni contraseñas.          │
│ Do not paste real IPs, hostnames, keys or passwords.                   │
│ Versión (magi --version) *     [ MAGI 1.6.1 · protocolo 10        ]    │
│ Sistema y terminal *           [ Omarchy · Alacritty              ]    │
│ Qué pasó / What happened *     [                                  ]    │
│ Pasos / Steps to reproduce *   [ 1. …                             ]    │
│ Qué esperabas / Expected       [                                  ]    │
│ magi diagnostico               [ salida, sin datos reales         ]    │
│ [x] He quitado los datos privados / I removed private data             │
│                                                       [ Crear issue ]  │
├────────────────────────────────────────────────────────────────────────┤
│ ¿Una vulnerabilidad? Aviso privado: Security → Report a vulnerability  │
└────────────────────────────────────────────────────────────────────────┘
```

### 6.4 Sincronización con los avisos de la 10b

```
┌ MAGI · SINCRONIZACIÓN ───────────────────────────────────────────────────────┐
│ Carpeta ~/Sync/magi · última pasada 11:42 · 3 equipos                        │
│                                                                              │
│  EQUIPO             ESTADO                                                   │
│ ▸portatil           al día                                                   │
│  sobremesa          ! a medio copiar o dañado: se reintenta (11:43)          │
│  magi-7c1e….age     ✕ rechazado: de un equipo que este no conocía,           │
│                       cifrado con la clave anterior                          │
│                                                                              │
│ Borrados aplazados: web-01 (tiene trabajo vivo en el servidor)               │
├──────────────────────────────────────────────────────────────────────────────┤
│ ↵ detalle · r sincronizar ahora · ? ayuda                                    │
└──────────────────────────────────────────────────────────────────────────────┘
```

Es la vista de la F10 con los textos nuevos: el aviso de un fichero dañado no pide nada y se reintenta solo; el rechazo dice por qué; los borrados aplazados dicen qué host y por qué.

**Implementado (21 nov 2026, D438):** la vista no pinta todavía la línea «Borrados aplazados» ni la fila «✕ rechazado»: los avisos salen en el pie y los aplazados, en CONFLICTOS, donde se aplican con `a` (D427). Queda como pendiente aceptado, para cuando se toque la vista.

### 6.5 `magi sincronizar` en la terminal

La salida de §5.3: una línea por equipo, los avisos con `!`, los aplazados con su motivo y un resumen al final.

### Notas de UX

- **El README es una puerta, no el manual:** qué es, cómo se ve, cómo se instala, qué estado tiene y adónde ir; el resto, enlazado.
- **Honestidad sobre el estado:** el README dice qué es uso diario y qué está en pruebas o en beta, y `docs/manual/estado.md` detalla lo que no se ha probado a mano (R59).
- **Sin datos reales en ningún sitio que se vea:** la animación usa un inventario inventado y el servidor de pruebas; las plantillas piden no pegar IPs, nombres de host, claves ni contraseñas.
- **La animación tiene texto alternativo** en los dos README, y nada importante se cuenta solo en ella.
- **Los avisos de la sincronización no alarman sin motivo:** un fichero a medio copiar es lo normal con Syncthing y se reintenta solo; solo una clave distinta pide algo.

---
## 7. Lógica de Negocio

### 7.1 Borrados con trabajo vivo en la CLI (R83)

```python
PLAZO_SERVIDOR = 5  # segundos

def decidir_borrados(borrados, consulta):
    """consulta: None si no hay servidor (socket ausente), Fallo si es de otra versión o no responde en el plazo,
    o el trabajo vivo por host que devuelve `Listar` tras la bienvenida (D428)."""
    de_host = [b for b in borrados if b.tipo in ("host", "tunel")]
    otros = [b for b in borrados if b.tipo not in ("host", "tunel")]
    if consulta is None:                      # sin servidor no hay pestañas, túneles ni transferencias
        return borrados, []
    if isinstance(consulta, Fallo):           # no se sabe: se aplaza todo lo que puede tener trabajo
        return otros, de_host
    vivos = consulta.hosts_con_trabajo
    aplazados = [b for b in de_host if b.host_id in vivos]
    return otros + [b for b in de_host if b.host_id not in vivos], aplazados
```

- La consulta nunca arranca el servidor; un socket que no responde cuenta como `Fallo`, no como «sin servidor».
- Solo se consulta si hay clave (sin clave no hay borrados que decidir), fuera del cerrojo; cada `Fallo` aplaza, que es la dirección segura. Un túnel `Parando` cuenta como trabajo vivo (D428).
- Los aplazados van a CONFLICTOS, como los de la TUI, y se aplican a mano con `a` cuando el host ya no tiene trabajo vivo; la pasada del escritorio no los reintenta sola, solo el lector del móvil (D427).

### 7.2 El cambio y su versión, en una transacción (R84)

```python
def guardar_elemento(bd, tabla, fila, equipo, reloj):
    """Vale para todas las tablas que viajan: si algo falla entre los dos pasos, no queda nada a medias."""
    with bd.transaccion_inmediata():          # BEGIN IMMEDIATE … COMMIT
        bd.actualizar(tabla, fila)
        bd.fijar_version(tabla, fila.uuid, f"{reloj.ms()}.{equipo.uuid}", vector=fila.vector.sumar(equipo))
```

### 7.3 Fichero de otro equipo (R85 y R97)

```python
def leer_fichero(nombre, datos, claves, equipos):
    """claves.actual y, durante los 30 días tras una rotación, claves.anterior."""
    try:
        return Leido(descifrar(datos, claves.actual))
    except NingunaIdentidadCoincide:
        pass
    except ErrorAge:                          # truncado, cabecera rota, MAC, tamaño: no es cosa de la clave
        return Danado(nombre, "a medio copiar o dañado; se reintenta en la siguiente pasada")
    if not claves.anterior_vigente():
        return PideClave()
    equipo = uuid_del_nombre(nombre)          # magi-<uuid>.age (F10: solo el uuid canónico en minúsculas)
    if equipo not in equipos.conocidos() or equipos.quitado(equipo):
        return Rechazado(nombre, "de un equipo que este no conocía, con la clave anterior")
    try:
        return Leido(descifrar(datos, claves.anterior))
    except NingunaIdentidadCoincide:
        return PideClave()
    except ErrorAge:
        return Danado(nombre, "a medio copiar o dañado; se reintenta en la siguiente pasada")
```

- Después de leer, siguen las comprobaciones de la F10: firma, que `equipo.uuid` coincida con el nombre, que no esté quitado y el `formato`.
- Un `Danado` o un `Rechazado` no paran la pasada: se importan los demás ficheros.
- El aviso y el log dicen el nombre del fichero y el motivo, nunca su contenido.

### 7.4 Números de versión

```python
def da2(version):                             # la que espera tui_prueba: 1.6.0 → 10600 · 1.6.1 → 10601
    mayor, menor, parche = map(int, version.split("."))
    return mayor * 10000 + menor * 100 + parche

def version_code(mayor, menor, parche, rc=None):   # app (informe de la F11, §7.10)
    return mayor * 100000 + menor * 1000 + parche * 10 + (rc if rc else 9)
# 0.2.0-rc.1 → 2001 · 0.2.0-rc.2 → 2002 · 0.2.0 → 2009
```

### 7.5 La primera entrada publicada del CHANGELOG

```python
TIPOS = ("Añadido", "Cambiado", "Corregido", "Seguridad")

def primera_entrada(changelog, version, fecha, resumen_es, resumen_en, areas):
    """Junta «Sin publicar» y las versiones que nunca salieron en la primera que sale de verdad."""
    no_publicadas = [changelog.sin_publicar] + [e for e in changelog.versiones if not e.etiquetada]
    entrada = Entrada(version, fecha,
                      cabecera=[f"**Resumen.** {resumen_es}", f"**Summary.** {resumen_en}",
                                "Primera versión publicada: reúne lo que se numeró durante el desarrollo "
                                "y no llegó a salir."],
                      que_trae=areas)                     # lista corta por áreas, antes del detalle
    for tipo in TIPOS:
        entrada[tipo] = [linea for e in no_publicadas for linea in e.get(tipo, [])]
    return changelog.sustituir(no_publicadas, entrada).con_sin_publicar_vacio()
```

- En el escritorio se juntan «Sin publicar» (con lo de la F17) y la 1.0.0 en `[1.6.1]` (D425); en la app, «Sin publicar», la 0.2.0 y la 0.1.0 en `[0.2.0]`. Los enlaces del final apuntan a `HectorEspejo/magi`.
- Desde ahora, cada versión empieza con su resumen en los dos idiomas (T141); el detalle sigue en castellano.
- `dist` toma de esta entrada las notas de la release: el encabezado tiene que seguir siendo `## [1.6.1] - AAAA-MM-DD`.

### 7.6 Prueba `repositorio_publico`

```python
RANGOS_DOC = ("192.0.2.0/24", "198.51.100.0/24", "203.0.113.0/24")
CORREOS = {"hola@4d3.org"}
DOMINIOS = {"ejemplo.org", "openssh.com", "users.noreply.github.com", "aur.archlinux.org"}
RUTAS = {"usuario", "usuariob", "runner", "linuxbrew", "deploy", "ana", "ana2", "prueba", "prueba-magi",
         "pruebas-magi", "pruebas-magix", "nadie", "x", "h"}   # las de las pruebas de hoy
FUERA = ("vendor/", ".git/", "target/")
ARCHIVO = ("docs/fase", "docs/magi-maestro.md", "docs/README.md")   # el diseño, su historia y lo que lo explica

def comprobar(ruta, texto):
    if ruta.startswith(FUERA) or es_binario(texto):
        return []
    errores = []
    for n, linea in enumerate(texto.splitlines(), 1):
        for ip in ipv4(linea):
            if es_publica(ip) and not en_rangos(ip, RANGOS_DOC) and not cita_de_especificacion(linea):
                errores.append((ruta, n, "IP pública fuera de los rangos de documentación"))
        for usuario in re.findall(r"/home/([a-z][a-z0-9_-]*)", linea):
            if usuario not in RUTAS:
                errores.append((ruta, n, f"ruta /home/{usuario}"))
        for correo in correos(linea):
            if correo not in CORREOS and dominio(correo) not in DOMINIOS and not plantilla_systemd(correo):
                errores.append((ruta, n, "correo fuera de la lista"))
        if "4d3-org" in linea and not ruta.startswith(ARCHIVO):
            errores.append((ruta, n, "referencia a 4d3-org"))
    return errores
```

- Las listas viven en la propia prueba, cada excepción con su motivo. `cita_de_especificacion` reconoce las referencias a secciones de especificaciones (por ejemplo, `MS-RDPBCGR 2.2.1.13`) y `plantilla_systemd`, las unidades como `getty@tty1.service`. El archivo de diseño (`docs/faseN/`), el maestro, que cuenta la historia del proyecto, y `docs/README.md`, que los explica, pueden nombrar `4d3-org`; los datos personales se comprueban en todos.
- La misma prueba comprueba los enlaces relativos y las anclas de `README.md`, `README.en.md`, `CONTRIBUTING*.md`, `SECURITY.md`, `docs/README.md`, `docs/manual/` y `docs/publicar*.md`, sin red. Los enlaces del archivo `docs/faseN/` no se comprueban.
- Al fallar dice el fichero, la línea y el motivo.
- **Implementación (D433):** sin dependencias nuevas: un escáner a mano con `std::net::Ipv4Addr` (el paquete raíz no tiene `regex`), la lista de ficheros de `git ls-files --cached --others --exclude-standard` y un cálculo propio de las anclas de GitHub. Comprueba también que los encabezados del README antiguo están en el manual, con tres excepciones: `Desarrollo` y `Tests` en `CONTRIBUTING` y `Licencia` en el README.

### 7.7 Validaciones

- **Etiquetas:** `vX.Y.Z` igual a la `version` de `Cargo.toml`; `android-vX.Y.Z[-rc.N]` igual a `android/version.properties`.
- **CHANGELOG:** el encabezado de la versión etiquetada existe y empieza con el resumen bilingüe; no queda ninguna entrada de una versión que no esté etiquetada.
- **README:** 200 líneas como mucho; todo lo que tenía el README antiguo está en el manual (la prueba compara los encabezados, con las tres excepciones de §7.6).
- **Animación:** 2 MiB como mucho y solo el inventario de ejemplo.
- **Referencias y datos:** `repositorio_publico` en verde.
- **Release:** `dist plan --tag v1.6.1`, `makepkg --printsrcinfo` y `actionlint` en verde.

### 7.8 Casos especiales

- **Una PR de Dependabot toca una versión fijada** (russh, ratatui, crossterm, tui-term, vt100, rusqlite, ironrdp): no se fusiona tal cual; se sube el conjunto con sus pruebas, como manda `CLAUDE.md` (R179).
- **Una contribución en inglés:** se acepta; el código, los comentarios y los textos de interfaz van en castellano, y la revisión ayuda con eso (R178).
- **Una vulnerabilidad en una issue pública:** se editan los detalles fuera y se pide el aviso privado (§4.7).
- **Un job de la release falla** (por ejemplo, el tap): se relanza solo ese job; la release no se retira ni se reetiqueta.
- **El nombre de AUR se ocupa antes de publicar:** se para y se elige otro (docs/publicar.md §4).
- **macOS falla en el guion:** si es grave, sale de esta release (D395); si es menor, sale «en pruebas» con el fallo anotado en `docs/manual/estado.md`.
- **La F17 se fusionó antes que la 9b** (D425): la 9b parte de ella, lleva su documentación de usuario al manual (`docs/manual/termux.md`) y la publica en pruebas (D426).
- **El `.deb` de Termux falla en la candidata:** si es grave, Termux sale de esta release (D426); si es menor, sale «en pruebas» con el fallo anotado en `docs/manual/estado.md`.
- **`gitleaks` encuentra un secreto real en el historial:** se para, se avisa y se revoca; el historial no se reescribe (D391).
- **`gitleaks` solo encuentra claves de pruebas:** se resumen en el informe sin copiar su valor y, si una coincide con una que se usa, se rota (D434).

### 7.9 Limitaciones

- El historial público conserva datos personales de antes de esta fase: no se reescribe (R176).
- Las direcciones de `4d3-org` siguen en el archivo de diseño y en el maestro: `docs/README.md` avisa de que ya no valen y, si la organización está libre, se reserva (R182).
- No hay guion de humo antes de publicar: la instalación desde AUR y Homebrew se comprueba con la release real (R177).
- macOS sin notarizar (R61) y en pruebas; la app, en beta.
- Las fases 2 a 16 siguen sin su validación manual completa (R59), y `docs/manual/estado.md` lo dice.
- El proyecto depende de una cuenta personal de GitHub (R181).
- `servicios.sh` ya rebasa el límite de la línea de órdenes de Dropbear, y `crear` mete la carpeta y el comando en ella: Servicios, y crear sesiones con textos largos, pueden no funcionar con hosts con Dropbear (D436, R189, R190).
- El README que va dentro del `.deb` y de los tarballs enlaza a `docs/`, que no viaja en el paquete (D438).

---

## 8. Requisitos No Funcionales

**Seguridad**

- Avisos privados de vulnerabilidades en GitHub, con el procedimiento de `SECURITY.md`; las correcciones salen como PATCH y su aviso, después.
- Lo publicado sale solo de CI, con `.sha256` y atestaciones de procedencia; el APK, además, firmado con el keystore de la app.
- Un secreto por workflow (F9); el token del tap, de grano fino y solo para `HectorEspejo/homebrew-tap`.
- `main` protegida: nada entra sin PR ni sin los jobs de `ci.yml` en verde, y no se puede reescribir.
- Dependabot avisa de dependencias con fallos conocidos; `cargo deny` sigue en CI.
- `gitleaks` pasa una vez por todo el historial y `repositorio_publico` vigila cada PR.

**Protección de datos (RGPD)**

- Los commits muestran el nombre y el correo de quien los hace: los nuevos van con el correo de 4d3. Los antiguos, con un correo personal, se quedan (R176); entre ellos, 18 merges hechos desde la web de GitHub, que puede ocultarlo desde ahora con «Keep my email addresses private».
- Las plantillas piden no pegar datos privados, y la de fallos lleva su casilla.
- La animación y los ejemplos usan datos inventados; los documentos de fase, sin rutas personales.
- MAGI no envía nada a ningún servicio: no hay telemetría que declarar.

**Backups y recuperación**

GitHub guarda el código, las releases y las issues; cada release se puede rehacer desde su etiqueta. El tap y el PKGBUILD se regeneran con la siguiente release, y el keystore de la app lo guarda el desarrollador fuera del repositorio (F11).

**Rendimiento**

- La animación pesa 2 MiB como mucho, para que el README cargue rápido.
- `repositorio_publico` recorre el árbol sin red en unos segundos.
- La release tarda lo que `dist` y sus cuatro objetivos (F9); `humo`, con sus 75 minutos, no es obligatoria en las PR.

**Accesibilidad**

La animación lleva texto alternativo y no cuenta nada que no esté escrito. Los dos README tienen la misma estructura de encabezados y las plantillas, sus campos con etiqueta.

---

## 9. Integraciones

| Con | Uso |
|---|---|
| GitHub | Releases, Actions, atestaciones, avisos privados de seguridad, Dependabot, reglas de ramas y plantillas |
| Homebrew | Tap `HectorEspejo/homebrew-tap` con la fórmula `magi-ssh` (la publica `dist`) |
| AUR | `magi-ssh-bin` (lo publica `publicar-aur.yml`), mantenedor `hola@4d3.org` |
| Obtainium | Instala y actualiza la app desde las releases `android-v*` |
| VHS (charmbracelet/vhs), `ttyd` y `ffmpeg` | Graban la animación en local; no se instalan en CI (D435) |
| Termux | El `.deb` en cada release (`termux-release.yml`, F17) y la receta de TUR, que el desarrollador propone después de la estable |
| `gitleaks` | Una pasada por todo el historial antes de publicar (20 nov 2026: 14 hallazgos, todos en ficheros de pruebas; D434) |

---

## 10. Decisiones Técnicas (ADR-lite)

- **D386 — La primera publicación es una fase propia, la 9b, que aglutina las correcciones previas** (Hector, 2a con su añadido): el cierre de la F16 (su §11 de la v1.2) y la corrección 10b. Sustituye al prompt de cierre de la F16, que ya no se usa. Se llama 9b porque termina lo que dejó pendiente la F9, y así no se renumera nada.
- **D387 — Escritorio 1.5.1 y app 0.2.0** (Hector, 1a y 10a). El escritorio sube a 1.5.1 aunque la 1.5.0 no saliera: los equipos de desarrollo ya tienen servidores 1.5.0, y la sustitución del servidor anterior necesita ver otra versión. La app sale con el número que tiene `main`. Sin migración, protocolo ni `formato` nuevos. Descartado volver a 1.0.0 (las versiones solo van hacia delante). **Superada (21 nov 2026, D425):** sale la 1.6.1.
- **D388 — Lo que nunca se publicó se junta en la primera entrada publicada** (Hector, 1a), y cada versión empieza desde ahora con un resumen en castellano e inglés (12a). Descartado conservar entradas fechadas de versiones que no salieron: dirían algo que no pasó.
- **D389 — La validación previa es la CI y el ensayo ya hecho** (Hector, 3c). Solo macOS (9a) y la versión candidata de la app (10a) llevan su guion corto. Los guiones de fase pendientes siguen como riesgo aceptado (R59), y `docs/manual/estado.md` lo cuenta.
- **D390 — El repositorio se queda en `HectorEspejo/magi`** (Hector, 4b). Cambian todas las referencias funcionales, también el tap (`HectorEspejo/homebrew-tap`); el archivo de fase no se toca. Cambia la decisión de la F9, que lo llevaba a `4d3-org`.
- **D391 — El historial no se reescribe** (Hector, 5): los datos personales que ya están en el historial se quedan, y se limpian los ficheros actuales. Un secreto real que apareciera se revocaría, sin reescribir. Descartados reescribir con `git filter-repo` y empezar un repositorio nuevo.
- **D392 — `docs/` entero se publica como archivo del diseño** (Hector, 6a), explicado en `docs/README.md` y enlazado desde el README.
- **D393 — Contribuciones abiertas sin acuerdo de cesión** (Hector, 7a): lo que entra lo hace con la misma licencia doble. Son obligatorios los siete jobs de `ci.yml`, no los de `android.yml`, que tienen filtros de rutas (precisión, D432: son ocho checks). Dependabot, solo de seguridad y sin `dependabot.yml`.
- **D394 — Los canales de la F9, desde `HectorEspejo`** (Hector, 8a): GitHub Releases, Homebrew y AUR; Termux llega con la 1.6.0 (F17). **Superada en parte (21 nov 2026, D426):** Termux sale ya en la 1.6.1, en pruebas.
- **D395 — macOS sale «en pruebas»** (Hector, 9a) tras un guion corto en el mac mini con un binario compilado allí desde el commit de la release. Si algo grave falla, sus dos objetivos salen de esta release y vuelven en una PATCH; si es menor, se anota en `docs/manual/estado.md`. Sin notarizar (R61).
- **D396 — La app sale en beta, con versión candidata** (Hector, 10a): `0.2.0-rc.1` instalada con Obtainium en el plegable y, si va, `0.2.0`. El keystore y sus cuatro secretos los crea el desarrollador.
- **D397 — README corto y el manual en `docs/manual/`, movido sin reescribir** (Hector, 11a): un capítulo por bloque de secciones, mismo texto y enlaces arreglados. Reescribirlo queda para después, porque mover y reescribir a la vez haría la revisión imposible.
- **D398 — La animación se graba con VHS contra el servidor de pruebas del repositorio** (`servidor_pruebas`, F14), con tres hosts de nombres inventados que apuntan a él y un entorno aislado; el GIF se versiona y se regenera a mano con `scripts/grabar-demo.sh`. Descartados grabar contra hosts reales (datos) y generarla en CI (VHS necesita navegador y ffmpeg, y el resultado cambia poco).
- **D399 — Dos idiomas solo en la puerta de entrada** (Hector, 12a): `README.en.md`, `CONTRIBUTING.en.md`, el resumen de cada versión y un párrafo de `SECURITY.md`. El código, la interfaz, el manual y los documentos siguen en castellano: es una excepción acotada a la regla de idioma de `CLAUDE.md`.
- **D400 — El correo público del proyecto es `hola@4d3.org`** (decisión del analista), en el PKGBUILD y en los commits a AUR. Nunca un correo personal. **Superada (22 nov 2026, T151):** MAGI es un proyecto personal de Hector Espejo: el mantenedor y el correo del PKGBUILD, de los commits de AUR y del `.deb` los elige él (el `noreply` de GitHub sirve) y se cambian en la rama `release-1.6.1`, antes de etiquetar.
- **D401 — La 9b va en paralelo con la F17** (decisión del analista). La 9b llega antes a `main`; la F17 integra `main` antes de su cierre y lleva su documentación de usuario al manual (`docs/manual/termux.md`), con el README solo enlazando. Se esperan conflictos en el README, el CHANGELOG, `ci.yml` y `CLAUDE.md` (R180). **Superada (21 nov 2026, D425):** la F17 llegó antes a `main` (PR #18) y la 9b partió de ella; `docs/manual/termux.md` lo hizo la 9b.
- **D402 — La revisión de datos pasa a CI** (decisión del analista): `repositorio_publico` hace en cada PR lo que la F9 hizo una vez a mano (T143). `gitleaks` no entra en CI: los secretos se vigilan en el momento de publicar.
- **D425 — Versión 1.6.1, no 1.5.1** (decisión del desarrollador; supera D387 y D401). La especificación suponía `main` en 1.5.0 con la F17 sin fusionar; el repositorio ya traía la F17 (PR #18, 1.6.0), y bajar a 1.5.1 sería volver atrás con el código de Termux dentro. Se aplica la lógica de D387 (los equipos de desarrollo tienen servidores de la versión anterior y la sustitución necesita ver otra): PATCH sobre lo que hay, 1.6.1, DA2 `10601`, etiquetas `v1.6.1-rc.1` y `v1.6.1`; la app sigue en 0.2.0 y `SECURITY.md` mantiene la 1.6.x. Donde la especificación decía 1.5.1, `10501` o `v1.5.1`, ahora va 1.6.1, `10601` y `v1.6.1`; los ítems del checklist conservan su texto, con una precisión (T149).
- **D426 — Termux sale con la primera publicación, «en pruebas»** (decisión del desarrollador; supera en parte D394). `termux-release.yml` se dispara con la misma etiqueta que `release.yml` y, como el primer `.deb` publicado es siempre una candidata (F17), `docs/publicar.md` §4 pone `v1.6.1-rc.1` antes de `v1.6.1`. El README y el manual dicen «Termux, en pruebas»; `docs/manual/termux.md` lo hizo la 9b (D401) y `termux_paquete` lee ese capítulo. Regla de la candidata (decisión del analista, como D395 para macOS): si el `.deb` falla de forma grave en el plegable (no se instala, no arranca o no conecta), Termux sale de esta release —`termux-release.yml` desactivado antes de `v1.6.1` y el README y el manual sin ofrecerlo— y vuelve en una PATCH; si el fallo es menor, sale en pruebas, con el fallo anotado en `docs/manual/estado.md`. El cierre de la F17 (la prueba de concepto, el guion y lo que falte de D418-D422) sigue en su fase.
- **D427 — Los borrados aplazados no se reintentan solos en el escritorio** (decisión del desarrollador). La v1.0 (§4.1 y §7.1) decía que se reintentaban en cada pasada «como en la TUI (T73)»; el código salta los ficheros cuyo hash no cambió, una prueba lo fija y en la TUI el aplazado se aplica a mano con `a` en CONFLICTOS. Solo el lector del móvil reintenta. La revisión adversarial encontró que ese salto hacía que un plan parcial diera por «recuperado» un aplazado (NUC-1), y está corregido. Cambian §3.5, §4.1 y §7.1.
- **D428 — La consulta de `magi sincronizar`, por un camino propio.** No reutiliza `leer_bienvenida`, que esperaría 1 s de más por la lista de escritorios: tras la bienvenida pide `Listar` y lee hasta su respuesta, con el plazo de 5 s. «Otra versión» incluye una `Bienvenida` del mismo protocolo con otra versión de la app (como `estado_cli`), y un túnel `Parando` cuenta como trabajo vivo. Solo se consulta si hay clave y fuera del cerrojo; cada fallo aplaza. Las órdenes `sincronizar` y `sincronizar estado` pasan a la biblioteca (`src/sincronizacion/cli.rs`). Fuera de la lista del checklist y necesarios para que funcione: un equipo `danado` vuelve a `al_dia` si su fichero recupera el hash ya importado, el lector del móvil cuenta `incompletos`, y `socket_en_otra_ruta`, `interpretar_terminar` y `pedir_recomprobacion` (hallazgos NUC-3, S-02 y C9B-1 de la revisión).
- **D429 — El script de tmux por la entrada estándar, con más de lo pedido** (R157, R159). La línea de órdenes lleva `OrdenTmux::ARRANQUE` (`S=$(cat); … eval "$S"`) y los argumentos escapados; el script entra por la entrada estándar, se envía con `canal.data()` antes del `eof()`, dentro del plazo, y acaba en `#MAGI-FIN`: sin esa marca, el arranque sale con 70 sin ejecutar nada (T150). Los tres ejecutores duplicados van por `lanzar_exec`. `preparar` devuelve también la hora de creación y la sesión se sigue por identificador y hora (con tmux reiniciado, los identificadores se reutilizan). tmux no tiene «comprobar y enganchar» atómico: la guarda y el `attach` van en el mismo script, la marca `@magi_equipo` la pone el enganche después de la guarda y, si otro equipo tiene la sesión, sale con 101 (`FinEnganchar::OtroEquipo`); sin guarda (`prefijo r`, a mano) manda el usuario. Los ganchos nuevos del tmux falso, en `tests/fijos/terminal/README.md`.
- **D430 — Los retoques del cierre de la F16, como quedaron.** D368 lleva la pasada inmediata a toda ventana con lista guardada, no solo a la primera (si no, una segunda ventana con el vigilante vivo no vería nunca una lista fresca), y el umbral del cliente lleva 2 s de margen. D370(e) reabre el diálogo con lo escrito solo por carpeta inexistente o nombre en uso, y solo si la apertura venía del diálogo; R164 pide confirmación en `r`, la paleta y `prefijo r` (`↵` y `c` en la propia pestaña no cambian: el recuadro «SOLTADA» ya informa). R160 se confirmó y se corrigió de forma genérica (`restaurar_nombres_apartados`), que cubre también las fichas con la clave ocupada y un host cuyo borrado se aplaza por un túnel en uso. Las instantáneas `conflictos_*` cambian por las etiquetas legibles, además de las de D370. `creada_por` se guarda recortado y `DatosSesionPersistente::validar` sigue más estricta que el CRUD con los demás textos (no los recorta), por ser entrada no fiable. Una ficha ilegible da un solo aviso por fila (con un registro acotado a 256 filas).
- **D431 — La organización anterior, también fuera de lo que nombraba el checklist.** Cuatro ficheros que la F17 añadió después la nombraban: `docs/publicar-termux.md`, `packaging/termux/build.sh`, `scripts/paquete-termux.sh` y `tests/termux_paquete.rs`; cambian igual, y el mantenedor de la receta de TUR pasa a `@HectorEspejo`. `docs/publicar.md` no nombra la organización anterior (R182): manda a `docs/README.md`, para no abrir otra excepción en la prueba. De paso se quitaron otros datos personales de los informes de la F9 y la F11.
- **D432 — La regla de `main` exige ocho checks, no siete.** `ci.yml` tiene nueve jobs: los siete de la v1.0 y dos condicionales de la F17 (`cambios-termux` y `termux`); `tests-linux` es una matriz y da dos checks. Obligatorios: `formato`, `clippy`, los dos de `tests-linux`, `macos`, `visor`, `deny` y `aur-plantilla`; los condicionales y los de `android.yml`, no, por sus filtros de rutas. `docs/publicar.md` §4 los lista con el nombre con que salen. No se renombra ni se quita ningún job.
- **D433 — El manual es el README movido sin reescribir.** El reparto, con un script por rangos de encabezados y una tabla fija de destino, comprueba que cada sección tiene sitio y que no cambia el número de líneas, y lista cada línea tocada: 19 cambios (13 enlaces y 6 frases de versión), más los textos nuevos de la fase (D367, la 10b y la F16) y `termux.md` (D401). `Desarrollo` y `Tests` van a `CONTRIBUTING.md` y `Licencia`, al README (102 líneas): son las tres excepciones del AC de los encabezados, que `repositorio_publico` comprueba con esa lista. La prueba no añade dependencias (§7.6).
- **D434 — `gitleaks`: claves de pruebas, ninguna real.** Una pasada por todo el historial el 20 nov (133 commits): 14 hallazgos, todos en ficheros de pruebas —11 `age-secret-key` de fixtures de sincronización y de las pruebas instrumentadas de Android, con UUID inventados, y 3 `private-key`: la clave de host fija del servidor de pruebas, su copia con el nombre anterior del ejemplo y el certificado de pruebas de RDP—. Ninguno parece real; lo confirma Hector y, si una clave de fixture fuera una que usa, se rota (el historial no se reescribe, D391). `docs/publicar.md` §2 cuenta el resultado en vez de esperar «no leaks found».
- **D435 — La demo, sin depender de quien graba.** `scripts/grabar-demo.sh` necesita `vhs`, `ttyd` y `ffmpeg` (no corre en CI), valida la cinta con `vhs validate` antes de grabar, para el servidor de MAGI solo si el entorno aislado ya estaba en marcha y su `XDG_RUNTIME_DIR` es el suyo, sin leer del terminal y con plazo, y fija `tema = "fijo"` en el `config.toml` de ese entorno (sin Omarchy, «auto» avisa en la barra: H-1, D437). El GIF pesa 536 370 B (1200×680, 20,6 s) y solo enseña hosts `demo-*`, `127.0.0.1:12222`, unidades inventadas y una ruta temporal; lo único que sale de la máquina que graba es la carga y los núcleos de la barra de Flota, y se acepta.
- **D436 — Los scripts embebidos frente a Dropbear, medidos y sin cambiar** (límite de unos 9 000 B, sin comprobar). `servicios.sh` 37 977 B (39 016 escapado): ya lo rebasa, desde la F13; `terminal_tmux.sh` 27 460 B, ahora por la entrada estándar; `terminal_enganche.sh` 4 029 B (4 168 escapado); `sondeo.sh` 3 110 B; `escritorio_voxl.sh` 1 569 B; `instalar_clave.sh` 719 B. Además, `crear` sigue metiendo la carpeta (hasta 1 024 B) y el comando (hasta 4 096 B) como argumentos y, con comillas escapadas, puede pasar de 9 000 B, contra «solo argumentos validados en la línea». Se acepta para la 1.6.1, con la limitación en `docs/manual/estado.md` (D437); una corrección posterior pasa `servicios.sh` y esos argumentos a la entrada estándar, como D357 (R189, R190). `src/flota/sondeo.sh` es una copia vieja y huérfana del script que se incrusta: se borra (D437).
- **D437 — Retoques antes de publicar** (decisión del analista; prompt de reanudación de la 9b). (a) H-1: con `tema = "auto"` (el valor por defecto) y sin el tema de Omarchy (macOS, un Linux sin Omarchy o un `HOME` aislado), `tema::cargar` usa la paleta de respaldo pero enseña el error del sistema en la barra de Flota en cada arranque; en «auto», un tema que no existe no es un aviso, como ya hace `Config::cargar` con `NotFound`, con su prueba y su línea en la entrada `[1.6.1]` del CHANGELOG: es lo primero que vería un usuario de macOS. (b) Se borra `src/flota/sondeo.sh`. (c) La cadena `estado_equipo_danado` de la app dice «a medio copiar o dañado», como el núcleo. (d) Si Hector confirma que los alias de la semilla de las instantáneas que señala el informe de implementación son nombres reales, se cambian por nombres inventados en la semilla, en las vistas que los usan y en las instantáneas, sin ningún otro cambio (R192). (e) `docs/manual/estado.md` dice que Servicios puede no funcionar con hosts con Dropbear (R189). (f) `SECURITY.md` dice que se mantiene la 1.6.x. (g) El informe v1.2 de la F16 pasa a `docs/fase16/`, en lugar de la v1.0. (h) El informe de implementación dice si la revisión adversarial de la 9b cubrió todo el código de la F16, servidor y cliente por separado, o solo su cierre (R161).
- **D438 — Aceptado tal cual** (decisión del analista). La vista SINCRONIZACIÓN no pinta todavía la línea «Borrados aplazados» ni la fila «✕ rechazado» de §6.4 (los avisos salen en el pie y los aplazados, en CONFLICTOS); un error de E/S al leer el fichero de un equipo conocido sigue parando la pasada entera, como antes; S-01 (una reconexión automática rechazada por la guarda deja una fila `sesion_reconectada` en REGISTRO en la carrera); el README que va dentro del `.deb` y de los tarballs enlaza a `docs/`, que no viaja; las acciones de terceros de los jobs que atestan en `termux-release.yml` van por rama o etiqueta (R80); las pruebas instrumentadas de Android no se ejecutaron (solo se reescribió el aviso esperado); R154 y R159 se comprueban de forma indirecta (sin Dropbear, `dash` ni mawk en la máquina del agente; la CI corre `tmux_real` con los de Ubuntu); y dos pruebas con carreras anteriores (una de la F16 y otra de la F17) se estabilizaron sin tocar el código de producción.

---

## 11. Plan de Desarrollo

| Sprint | Contenido | Semanas |
|---|---|---|
| S1 — Correcciones | Cierre de la F16: script por la entrada estándar y enganche mínimo (D357), medida de los demás scripts frente a Dropbear, carrera de la reconexión (R157), protocolo (D349, D369), D368, D370, D367, aviso de la pestaña mirada, `nombre_valido` (R159), validaciones de la importación, prueba de `escribir_tuneles` (R160), etiquetas y pruebas con tmux real. Corrección 10b: R83, R84, R85 y R97, con sus pruebas | 1 |
| S2 — Repositorio y publicación | Referencias, datos y `gitleaks`; `repositorio_publico`; README y `README.en.md`, manual en `docs/manual/` y `estado.md`; demo con VHS; `CONTRIBUTING`, plantillas y `SECURITY.md`; CHANGELOG y versión 1.5.1 (fue la 1.6.1, D425); `docs/publicar.md` y `docs/publicar-android.md`; revisión adversarial e informe con los pasos del desarrollador | 1 |
| Desarrollador | Ajustes, cuentas y secretos; nombres de AUR; guion de macOS; `v1.6.1-rc.1`, con el `.deb` en Termux, y `v1.6.1`; `android-v0.2.0-rc.1`, guion de la app y `android-v0.2.0`; comprobaciones posteriores y la receta de TUR | ½ día |

**Total: 2 sprints, ~2 semanas.** Supuestos: un desarrollador con Claude Code y dedicación parcial; el ensayo de la release, ya validado (Hector); VHS instalado en el equipo que graba la demo; la F17 en paralelo, fusionada después de la 9b (no fue así: se fusionó antes, D425).

**Implementación (21 nov 2026, informe procesado el mismo día).** Los dos sprints y la revisión adversarial se hicieron en la rama `fase9b`, que parte de `main` con la F17 fusionada (55/58, D425-D436). El Sprint 1 fue en cuatro pistas en paralelo sobre un contrato mínimo (`NoExiste` y `creada`), en worktrees propios y sin conflictos al fusionar. Lo que queda para el agente va en el prompt de reanudación (D437); lo demás es del desarrollador.

**Pasos del desarrollador, en orden** (los escribe `docs/publicar.md` §4 y el agente los repite en su informe; al día con el del 21 nov):

1. Confirmar el resumen de `gitleaks` (D434), decidir si se renombran los alias que parecen reales (D437, R192) y borrar la prerelease pública `artefactos-prueba-fase13` (R193).
2. Activar en GitHub «Keep my email addresses private» y «Block command line pushes that expose my email» (R176).
3. Pasar el prompt de reanudación (D437) y abrir la PR de la 9b: la CI de GitHub tiene que pasar entera, también el job `termux`.
4. Ajustes de §5.5: avisos privados, Dependabot de seguridad sin `dependabot.yml`, regla de `main` con los ocho checks, borrado de ramas, issues, descripción y temas.
5. Comprobar que `magi-ssh-bin` sigue libre en AUR y de quién es `4d3-org` en GitHub; si está libre, crearla vacía para reservarla (R182).
6. Crear `HectorEspejo/homebrew-tap` y el secreto `HOMEBREW_TAP_TOKEN`.
7. Crear la cuenta de AUR y la clave dedicada; secreto `AUR_SSH_PRIVATE_KEY`.
8. Crear el keystore de la app y sus cuatro secretos (si no existe ya). **Desde el 22 nov 2026:** sin «Cuatro de Tres» en el `-dname`; si el que hay lo lleva, se crea otro (D450 de la 9c).
9. Guion corto de macOS en el mac mini (abajo).
10. Fusionar la 9b con la CI en verde.
11. Candidata del escritorio: PR con la versión `1.6.1-rc.1` (`docs/publicar.md` §4) y etiqueta `v1.6.1-rc.1`. Sale una prerelease con los tarballs, las atestaciones y el `.deb`, sin tocar Homebrew ni AUR. Instalar el `.deb` en Termux con el guion de la F17; si falla algo grave, Termux sale de esta release (D426).
12. Estable: PR que vuelve a `1.6.1`, con la fecha del día en el CHANGELOG, y etiqueta `v1.6.1`; vigilar `release.yml` y `termux-release.yml` y hacer las comprobaciones de `docs/publicar.md` §9. Después, `TERMUX_PKG_VERSION` en `packaging/termux/build.sh` y la receta a TUR (`docs/publicar-termux.md`).
13. App: `android-v0.2.0-rc.1`, guion corto de la app (abajo) y `android-v0.2.0`. **Espera a la 9c (22 nov 2026, D450 de esa fase):** la app sale con el `applicationId` nuevo, después de esa fase.

**Guion corto de macOS** (en el mac mini, con el commit de la release):

1. `cargo build --release --locked`; `magi --version` y `magi diagnostico`.
2. Conectar a un host con una clave del agente y a otro con contraseña recordada en el Llavero.
3. Abrir dos pestañas, cerrar la ventana y volver con `magi`: siguen ahí.
4. SFTP: subir y bajar un fichero; un túnel local.
5. `magi servidor parar`. Si falla el 1 o el 2, macOS sale de esta release (D395).

**Guion corto de la app** (en el plegable, con `android-v0.2.0-rc.1`):

1. Obtainium con las prereleases activadas: instalar el APK y comprobar su certificado.
2. Desbloquear con huella o PIN.
3. Unir la sincronización (QR o pegando la clave) y ver Flota con el inventario.
4. Abrir un terminal y un túnel local.
5. Abrir Servicios de un host. Si todo va, `android-v0.2.0`; si algo falla, corrección y `0.2.0-rc.2` (versionCode 2002).

---

## 12. Conexiones con Otras Fases

**Dependencias**

- **F9:** sus canales, `docs/publicar.md` y la política de versiones; esta fase hace la primera publicación que dejó pendiente.
- **F10:** la sincronización que corrige la 10b (R83, R84, R85 y R97).
- **F16:** su cierre (§11 de su informe v1.2), que pasa aquí. El informe v1.2 no estaba en el repositorio (solo la v1.0): la 9b lo implementó con el checklist y el maestro, y la entrega del 21 nov lo trae para `docs/fase16/`.
- **F17:** se fusionó antes (PR #18, 1.6.0); la 9b parte de ella, la integra (manual, referencias y checks) y la publica en pruebas (D425, D426).
- **F11 y F14:** la app que se publica y su `android-release.yml`.
- **F15:** el aviso de las pestañas (F15) que se limpia al mirarlas.

**Datos compartidos**

- Ninguna tabla nueva; el protocolo v10 gana dos campos opcionales.
- Los dos CHANGELOG y `docs/manual/`, que desde ahora reciben la documentación de usuario de cada fase.

**Lo que viene**

- **9c (Web de MAGI):** la web en `magi.erratia.com`, con las descargas, las versiones y el manual; cambia el `applicationId` de la app, cuya primera release espera a esa fase (22 nov 2026).
- **F17 (MAGI en Termux)** se cierra después de la 1.6.1 con su prompt de reanudación: la prueba de concepto y el guion en el plegable, el `.deb` real visto en la candidata y lo que quede de D418-D422; sale en una PATCH.
- **F19 (Equipo)** necesitaba antes la corrección 10b: queda hecha aquí.
- **F18 (Accesos y certificados SSH)** no depende de esta fase.
- **Cualquier fase nueva** documenta para el usuario en `docs/manual/` y abre su versión en el CHANGELOG con el resumen bilingüe (T141, T142).
