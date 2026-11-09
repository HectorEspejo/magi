# MAGI - Fase 14: Android, Segunda Vuelta — Informe de implementación

> Documento vivo: se actualiza de forma incremental en cada sesión. Refleja lo
> que de verdad se ha implementado, las desviaciones y lo que queda pendiente.
> El análisis funcional parte de este informe para actualizar la especificación.

**Fecha:** 9 de noviembre de 2026
**Rama:** `fase14`

## Resumen de lo implementado

### Núcleo compartido (completado)

- **Sondeo en el núcleo (D274, R125).** `crates/magi-nucleo/src/flota/` con el
  script embebido `crates/magi-nucleo/scripts/sondeo.sh` (que recibe
  `<motores>` como primer argumento), la lectura de las líneas `MAGI_*`
  (`lectura.rs`), los estados con sus culpables, la tasa de red y los formatos
  legibles (`estado.rs`), más `aplicar_metricas` para rellenar un `Sondeo`.
  El escritorio conserva en `src/flota/` solo su orquestación y reexporta los
  tipos del núcleo; sus pruebas de pintado pasan sin cambios (303 instantáneas
  de `redimensionado` en verde).
- **Deliberación en el núcleo (D275).** `crates/magi-nucleo/src/deliberacion/`
  como carpeta: `plan.rs` (`PlanDeliberado`, `PlanEjecucion`,
  `PlanServicio`), `consenso.rs`, `salud.rs` (MELCHIOR-1), `backup.rs`
  (BALTHASAR-2), `tests.rs` (CASPER-3), y `comprobaciones.rs` con el trait
  `Proveedores`, `Limites`, `Trabajo`, `lanzar` (plazo duro y paralelismo)
  y `NoDisponible` (una comprobación que la plataforma no puede hacer rechaza).
  El escritorio delibera con `ProveedoresEscritorio` (servidor de sesiones,
  SFTP no interactivo y `sh -c` local); sus pruebas de integración y sus
  instantáneas siguen en verde.
- **`sftp_lectura.rs`**: listar un directorio por SFTP sin recursión ni
  enlaces, con plazo para abrir el subsistema y 10 000 entradas como máximo.
- **`snippets/`**: `destinos.rs` (resolución de destinos, motivos de
  deliberación) y `ejecucion.rs` (estados por host, resultado de la ejecución,
  `recortar_flujo` a 1 MiB con saneado).
- **Fijos compartidos de la F13 (D283).** Los guiones falsos
  (`systemctl`, `journalctl`, `docker/podman`, `sudo`, `nginx`) viven una
  sola vez en `crates/magi-nucleo/tests/fijos/servicios/falsos/` y el helper
  `tests/fijos/servicios/binarios.rs` los instala con `#[path]` desde las
  pruebas del escritorio, del núcleo y del móvil.
- **Autenticación automática del móvil:** `autenticar_movil` prueba también la
  contraseña guardada cuando la conexión es automática (sin diálogo), como pide
  §7.2.

### Móvil (`magi-movil`)

- **Contrato UniFFI completo (§5.1, §5.2, §5.4):** 29 funciones nuevas, 5
  métodos nuevos en `OyenteNucleo`, DTO propios para flota, servicios, logs,
  sesiones con orden, snippets, ejecuciones, deliberación, registro y ajustes;
  variantes de `ErrorNucleo` con instrucción (`HuellaSinAceptar`,
  `SinCredenciales`, `NoResponde`, `SudoRechazada`, `SinPermiso`).
- **Conexiones (D277):** `conexiones.rs` con una conexión por host para todo,
  contador de usos con RAII (`UsoConexion`), gracia de 60 s con temporizador,
  cerrojo por host, vigilante de 500 ms que avisa a los oyentes
  (`OyenteConexion`), políticas interactiva/automática (la automática nunca
  abre diálogos y traduce los fallos con instrucción) y el ejecutor remoto
  compartido `ejecutar_remoto` (plazo total, tope por flujo, EOF y cierre en
  todas las ramas). Sesiones y túneles conservan su uso mientras duran.
- **Flota (D266, D285):** `flota.rs` sondea con semáforo de 8 y 5 s por host,
  sin solapar sondeos del mismo host; guarda en SONDEOS (20 por host, con
  `fallos_json`); anota `sondeo_fallido` al pasar a CAÍDA y
  `sondeo_recuperado` al volver; nunca toca `HOSTS.ultimo_estado`; difunde la
  lista completa por `OyenteNucleo::flota`; `hosts_flota` compone estado,
  culpables, métricas, fallos, sesiones y hora; la ficha gana el bloque FLOTA,
  las verificaciones y el snippet al conectar.
- **Ajustes del móvil (D280):** `ajustes.rs` normaliza y guarda los ajustes de
  Flota, Servicios y Deliberación en el estado del núcleo con los rangos
  aprobados y devuelve avisos; accesores `actuales`, `umbrales` y `motores`.
- **Snippets y registro:** `snippets.rs` (lista, detalle y preparación con
  variables escapadas, validación de contexto, errores por campo y motivos de
  deliberación) y `registro.rs` (páginas de 50 con cursor por id, filtros en
  SQL y detalle con la deliberación).
- **Ejecuciones:** `ejecuciones.rs` con motor en memoria (1 h), semáforo de 8,
  conexión automática, tope de 1 MiB por flujo, parar al primer fallo,
  cancelar/repetir/limpiar, difusión por `OyenteNucleo::ejecuciones`,
  `snippet_ejecutado` por host y `usado_veces`/`ultimo_uso_en`.
- **Deliberación del móvil (D275, D282):** `deliberacion.rs` con proveedores
  del móvil (MELCHIOR-1 por sondeo o último sondeo, BALTHASAR-2 por
  `sftp_lectura`, CASPER-3 «solo en el escritorio»), tabla, consenso por
  unanimidad, resolución (aprobar/forzar con motivo ≥ 10/cancelar), fila de
  DELIBERACIONES con usuario «móvil (<equipo>)», `consumir` de un solo uso y
  `registrar_resultado` (fija `ejecucion_resultado` y anota
  `deliberacion_aprobada`/`_forzada`).
- **Servidor de pruebas:** `examples/servidor_pruebas.rs` (antes
  `servidor_pruebas_tunel`) es ahora un envoltorio fino de
  `tests/comun/servidor_pruebas.rs`: SSH en 127.0.0.1:12222 con exec, PTY y
  SFTP, eco en 19091, control en 12223, binarios falsos en el PATH, backups en
  `/tmp/magi-pruebas/backups` y línea «listo:». El job `humo` de
  `android.yml` lo arranca antes del emulador, espera «listo:» y lo para
  siempre al terminar (R106/R107).
- **Pruebas de extremo a extremo en Rust:** `flota_e2e` (4) y
  `snippets_movil_e2e` (2) en verde contra el servidor de pruebas; la de
  deliberación y la de servicios/logs están en marcha.

### App Android

- **Contrato Kotlin:** `FuenteNucleo` gana los flujos de Flota, acciones,
  logs y ejecuciones y las funciones de §5.1; modelos de vista nuevos para
  Flota, Servicios, Logs, Snippets, Ejecuciones, Deliberación, Registro y
  Ajustes; adaptador real y dobles actualizados.
- **Navegación (D265):** barra/riel FLOTA · SESIONES · TÚNELES · SERVICIOS ·
  MÁS; MÁS agrupa Snippets, Resultados, Registro y Ajustes; estado de navegación
  con `SeccionMas`; dos paneles por pantalla en plegable/pantalla ancha.
- **Flota (pantalla completa):** lista con grupos plegables, búsqueda, fila de
  dos líneas con glifo y palabra del estado, ⚠N, culpables en ámbar y motivo;
  sondeo al abrir y al volver, al deslizar (con sincronización) y refresco
  automático solo a la vista y solo de los hosts visibles; ficha con bloque
  FLOTA, verificaciones, snippet al conectar y botones SERVICIOS/SNIPPETS.
- **Ajustes de la Fase 14 en DataStore:** claves y acotación de Flota,
  Servicios y Deliberación, pantallas de ajuste y aplicación al núcleo.
- **Estructura:** servicio en primer plano con recuentos, ciclo de vida
  (`Visibilidad`) para sondeo/refrescos/logs, aviso al volver si Android cerró
  MAGI con acciones o ejecuciones en curso y olvido del sudo al bloquear.

## Desviaciones respecto a la especificación

1. **`PlanDeliberado` del núcleo con dos variantes.** El núcleo define
   `Snippet` y `Servicio` (las que usa el móvil); el escritorio conserva su
   enum de tres variantes, que añade `Sincronizacion`, y sus proveedores. Se
   evita arrastrar al núcleo los tipos de la sincronización.
2. **`Limites::desde_config` en el escritorio mediante el trait local
   `LimitesDeConfig`**, porque no se puede añadir un impl inherente a un tipo
   del núcleo desde otro crate.
3. **Rangos de los ajustes del móvil.** La especificación remite a «los rangos
   del escritorio», pero `config.toml` no valida los umbrales de Flota ni las
   horas de backup/salud. Se fijan en el móvil: carga por núcleo 0,1-16,0;
   memoria y disco 0-100; backup 1-720 h; salud 1-1440 min. Los que sí fija la
   especificación (refrescos, espera, motores, sudo) van tal cual.
4. **`sondeo_recuperado`** además de `sondeo_fallido` al volver de CAÍDA, por
   paridad con el escritorio (R12). La especificación solo menciona el fallido.
5. **Deliberación como `suspend` que devuelve la tabla final**, sin
   retrollamada nueva en `OyenteNucleo` (el contrato §5.2 no la tiene). La app
   puede enseñar progreso mientras espera; no se añadió nada fuera del contrato.
6. **Memoria de deliberaciones acotada (64) y consumibles una sola vez por
   sesión**: al reiniciar MAGI, una deliberación antigua ya no vale y hay que
   volver a deliberar (coherente con «una deliberación autoriza una sola
   acción»).
7. **Registro por cursor de id**, no por offset: las entradas nuevas no
   desplazan las páginas.
8. **Propiedades de unidades y contenedores como lista clave/valor** en la
   FFI, en vez de mapa, para no perder el orden del `BTreeMap` y no exponer
   mapas en UniFFI.
9. **Suscripciones de log paradas**: se sustituyen al volver a pedir el mismo
   objetivo y se descartan a los 10 min o si hay más de 8, para no acumular
   anillos de 8 MiB en memoria.
10. **`servidor_pruebas`**: la implementación pasa a
    `crates/magi-movil/tests/comun/servidor_pruebas.rs` para poder usarse
    in-process en las pruebas de Rust; el ejemplo queda como envoltorio con los
    puertos fijos. Además se instala `openssh-server` en el job humo para el
    `sftp-server` de BALTHASAR-2.
11. **`tests/servicios_servidor.rs`** conserva su copia incrustada de los
    binarios falsos: migrarla a los fijos compartidos es un refactor grande
    fuera del alcance de la fase (anotado como pendiente).

## Estructura de archivos creada/modificada

### Núcleo

- Creados: `crates/magi-nucleo/scripts/sondeo.sh`;
  `src/flota/{mod,lectura,estado}.rs`;
  `src/deliberacion/{plan,consenso,salud,backup,tests,comprobaciones}.rs`;
  `src/sftp_lectura.rs`; `src/snippets/{destinos,ejecucion}.rs`;
  `tests/fijos/servicios/binarios.rs`;
  `tests/generar_fixture_flota_android.rs`.
- Modificados: `src/lib.rs`, `src/deliberacion/mod.rs`,
  `src/snippets/mod.rs`, `src/conexion/cliente.rs`, `Cargo.toml`,
  `tests/servicios_script.rs`, `tests/generar_fixture_tunel_android.rs`.

### Escritorio

- Modificados: `src/flota/mod.rs` (orquestación y `PeticionSondeo.motores`),
  `src/deliberacion/mod.rs` (proveedores y límites),
  `src/app/dialogo_magi.rs`, `src/main.rs`, `src/app.rs`,
  `tests/deliberacion.rs`, `tests/servicios_e2e.rs`, `tests/servicios_ui.rs`.
- Borrados: `src/flota/{parser,estado}.rs`,
  `src/deliberacion/{salud,backup,tests}.rs`,
  `tests/servicios_e2e/binarios.rs` (duplicado de los fijos compartidos).

### Móvil

- Creados: `crates/magi-movil/src/{conexiones,flota,ajustes,servicios,logs,
  ejecuciones,deliberacion,registro,snippets}.rs`;
  `tests/comun/{mod,servidor_pruebas}.rs`; `tests/{flota,snippets_movil,
  deliberacion_movil,servicios_movil}_e2e.rs`;
  `examples/servidor_pruebas.rs` (renombrado).
- Modificados: `src/api.rs`, `retrollamadas.rs`, `lib.rs`, `estado.rs`,
  `gestor.rs`, `inventario.rs`, `Cargo.toml`, `tests/tunel_local_e2e.rs`.

### Android

- Creados: `ui/modelos/Modelos{Flota,Servicios,Logs,Snippets,Ejecuciones,
  Deliberacion,Registro}.kt`; `ui/flota/`; `ui/mas/`; `ui/servicios/`;
  `ui/ajustes/PantallaAjustes{Flota,Servicios,Deliberacion}.kt`;
  `plataforma/Visibilidad.kt`; `androidTest/assets/flota/`.
- Modificados: `ui/AppMagi.kt`, `navegacion/{Destino,Rutas}.kt`,
  `ui/fuente/FuenteNucleo.kt` y sus dobles, `ui/modelos/`,
  `preferencias/PreferenciasMagi.kt`, `plataforma/`, `ui/vistaModelo/`,
  `res/values/strings*.xml`, `gradle/`.

### Documentación y CI

- `.github/workflows/android.yml` (job humo con servidor de pruebas),
  `AGENTS.md`/`CLAUDE.md`, `docs/fase14/magi-maestro.md`,
  `docs/publicar-android.md`.

## Decisiones técnicas tomadas durante el desarrollo

- **Contrato antes que pistas (T63).** Antes de paralelizar se dejó en el
  núcleo un contrato compilable (tipos, módulos y firmas); el escritorio, el
  móvil y la app implementan contra él.
- **Proveedores por plataforma (D275).** `Proveedores` es un trait con los
  tres accesos que no son locales: sondeo nuevo (MELCHIOR-1), listado de un
  directorio (BALTHASAR-2) y comando local (CASPER-3). El móvil devuelve
  `NoDisponible` para CASPER-3, que rechaza con «solo en el escritorio».
- **`EntradaListado` propio del núcleo** en `sftp_lectura`: no reutiliza los
  tipos de la vista Archivos del escritorio (D276).
- **Conexión única con usos RAII.** `UsoConexion` se suelta con `Drop`; no hay
  método que lo consuma antes, así que nadie descuenta dos veces y una conexión
  antigua no puede tocar la nueva del mismo host.
- **Ejecutor remoto único** (`conexiones::ejecutar_remoto`) para sondeo,
  servicios, sudo y ejecuciones: plazo total, tope por flujo, EOF y cierre del
  canal en todas las ramas.
- **El motor de ejecuciones usa `snippets::preparar_interno`** (no duplica la
  preparación) y valida la deliberación con `deliberacion::consumir` antes de
  lanzar.
- **La app no decide nada de negocio:** los estados de Flota, las acciones
  permitidas, la deliberación, las variables de un snippet y el registro los
  resuelve el núcleo; Kotlin solo agrupa, formatea y pinta.
- **Pantallas autocontenidas** para Flota, Servicios y Snippets/Resultados/
  Registro: cada una lleva su ViewModel y su disposición de dos paneles, y
  AppMagi solo cablea destinos y callbacks. Así se pudieron construir en
  paralelo sin tocar el mismo fichero.

## Funcionalidades del checklist completadas

(Pendiente de consolidar al cerrar la sesión, copiando el texto exacto del
checklist una vez verificado todo.)

## Pendientes y bloqueos

- **Android:** pantallas de Servicios/Logs (en curso), Snippets/Resultados/
  Registro y diálogo MAGI (en curso), integración final en AppMagi y pruebas
  JVM de los ViewModels nuevos.
- **Rust:** `servicios.rs` y `logs.rs` del móvil (en curso);
  `servicios_movil_e2e` y `deliberacion_movil_e2e` (en curso).
- **Emulador:** pruebas instrumentadas de Flota, Servicios y Snippets y
  comprobación de `connectedDebugAndroidTest` en local.
- **CI:** comprobar que el job `humo` arranca el servidor de pruebas y que las
  pruebas instrumentadas dejan de omitirse (R106); actionlint ya en verde.
- **Publicación:** `android/CHANGELOG.md` y la sección «App Android» del
  README al día; la versión ya es 0.2.0 (`versionCode` 2009).
- **Cierre:** revisión adversarial (T31, T54) con su informe en
  `docs/fase14/magi-fase14-revision-adversarial.md`, checklist marcado y este
  informe consolidado.

## Ejecución y pruebas

```sh
# Núcleo
cargo test -p magi-nucleo --offline

# Escritorio (las instantáneas de Flota no cambian)
TZ=UTC cargo test -p magi-ssh --test deliberacion --test redimensionado --offline
TZ=UTC cargo test -p magi-ssh --test servicios_e2e --offline

# Móvil (Rust)
cargo test -p magi-movil --lib --offline
cargo test -p magi-movil --test tunel_local_e2e --test flota_e2e \
  --test snippets_movil_e2e --test deliberacion_movil_e2e \
  --test servicios_movil_e2e --offline

# Servidor de pruebas (para las pruebas instrumentadas)
cargo run -p magi-movil --example servidor_pruebas

# Android (necesita SDK y NDK r30; el wrapper usa local.properties)
cd android && ./gradlew assembleDebug
cd android && ./gradlew lint testDebugUnitTest
cd android && ./gradlew connectedDebugAndroidTest   # con el servidor de pruebas
```

Regenerar fixtures de androidTest:

```sh
cargo test -p magi-nucleo --test generar_fixture_android -- --ignored
cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored
cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored
```
