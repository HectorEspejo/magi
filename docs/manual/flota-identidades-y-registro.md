# Flota, identidades y registro

[← Índice del manual](README.md)

## Sondeo de flota

`F1` abre **Flota**, la vista de arranque. El sondeo ejecuta un único script
`sh` embebido (`src/flota/sondeo.sh`) por canal `exec` de SSH, en paralelo con
un semáforo de 8 y timeout de 5 s por host. Devuelve núcleos, carga, memoria,
disco, red y el estado de las unidades systemd listadas en la ficha
(campo **Servicios**, una por línea). Los estados se derivan del último sondeo
con los umbrales de `config.toml`: `FRÍA`, `CAÍDA`, `ALCANZ.`, `CARGA` y
`NOMINAL`. Se conservan los 20 últimos sondeos por host.

El sondeo nunca abre diálogos: si falta aceptar una huella o la clave pide
frase, el host queda CAÍDA con la instrucción «conéctate una vez con ↵». Si hay
una sesión viva con el host, el sondeo pide al servidor que ejecute el script
sobre esa conexión (`Ejecutar`); el registro solo anota las transiciones de
caída y de vuelta, no cada refresco.

## Autenticación

MAGI autentica con clave pública —agente o fichero, pidiendo la frase si está
cifrada— o con **contraseña**. En la ficha hay dos modos:

- «contraseña · se pide al conectar»: diálogo en cada conexión (3 intentos,
  `zeroize`); la contraseña no se guarda.
- «contraseña · llavero del sistema»: MAGI la recupera del llavero
  (`secret-tool`/libsecret en Linux, Keychain por API `security-framework` en
  macOS) y, si no existe, la pide con la casilla «recordar». El secreto nunca
  está en `magi.db`, solo la referencia `contrasena:llavero` en el host; con
  «recordar» marcada, el guardado ocurre al abrirse la sesión. En la paleta,
  «olvidar contraseña · <host>» la borra del llavero tras confirmación.

Las operaciones automáticas nunca dialogan: con el llavero, el sondeo usa la
contraseña guardada sin preguntar; sin ella, el host queda CAÍDA con
instrucción, salvo que ya exista una sesión viva, en cuyo caso reutiliza su
canal.

## Identidades y registro

`F5` es la pantalla de identidades: sincroniza las claves de `~/.ssh` y del
agente por huella, permite generar ed25519/RSA 4096 (con frase opcional y alta
en el agente), importar, copiar la pública (`wl-copy`/`pbcopy`, con fallback a
un diálogo), editar alias y revocar/reactivar. Revocar es una baja lógica: los
hosts pasan a identidad `auto` y la clave sigue en `~/.ssh` y en el agente.

`F7` muestra el **Registro**: conexiones, huellas aceptadas o sustituidas,
importación/exportación, claves y sondeos fallidos, con filtro, detalle,
purga de más de 90 días y exportación CSV/JSON. Todo pasa por
`registro::anotar`; las entradas nunca se editan. En la columna TIPO, los
cuatro tipos de las sesiones persistentes salen abreviados (`persistente_creada`,
`persistente_borrada`, `persistente_reabierta` y `persistente_enganchada`) para
que quepan; el detalle de la entrada y el filtro `/` usan el tipo real
(`sesion_persistente_creada`…).
