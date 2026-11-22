# Política de seguridad

MAGI es un gestor SSH de terminal que se ejecuta en la máquina del usuario y
custodia sus conexiones. Esta política explica cómo reportar vulnerabilidades,
qué versiones se mantienen y qué queda dentro del alcance.

## Versiones soportadas

Se mantiene la última versión MINOR de la serie 1.x del escritorio (hoy, la
1.6.x) y la última de la app Android, que está en beta (hoy, la 0.2.x). Las
versiones anteriores dejan de recibir correcciones de seguridad cuando se
publica una nueva: las actualizaciones llegan por el tarball de la release (y,
cuando se publiquen, por el gestor de paquetes: AUR o Homebrew) o, en Android,
por Obtainium.

| Versión | Soportada |
|---|---|
| Escritorio 1.6.x (última MINOR de 1.x) | Sí |
| App Android 0.2.x (beta, la última) | Sí |
| MINOR anteriores de 1.x y de la app | No |

El paquete de Termux sigue la versión del escritorio.

La política de versiones completa está en [`docs/publicar.md`](docs/publicar.md).

## Cómo reportar una vulnerabilidad

**No abras un issue público con detalles del fallo.** Usa el
[aviso privado de seguridad de GitHub](https://github.com/HectorEspejo/magi/security/advisories/new):
es privado entre quien reporta y los mantenedores y permite coordinar la
corrección y la divulgación.

Incluye, en lo posible:

- versión de MAGI (`magi --version`, que muestra versión, protocolo y commit) y
  sistema operativo;
- pasos para reproducirlo y el impacto que crees que tiene (pérdida de datos,
  ejecución de código, fuga de credenciales, escalada de privilegios…);
- una prueba de concepto mínima o los ficheros de configuración implicados,
  **sin claves privadas ni contraseñas reales**;
- si quieres que te acreditemos en la corrección, cómo prefieres aparecer.

No publiques los detalles hasta que haya una versión con la corrección o hasta
que acordemos la fecha de divulgación.

## Tiempos orientativos de respuesta

Son orientativos y dependen de la gravedad y de la disponibilidad de los
mantenedores:

| Paso | Plazo orientativo |
|---|---|
| Acuse de recibo | 72 horas |
| Evaluación inicial y gravedad | 7 días |
| Corrección de una vulnerabilidad crítica | 30 días |
| Corrección del resto | 90 días |
| Divulgación coordinada | 90 días desde el reporte, o antes de mutuo acuerdo |

Si la corrección necesita un cambio incompatible, se publicará en la siguiente
versión según la política de versiones; los parches de seguridad salen como
PATCH de la MINOR soportada.

## Alcance

Dentro del alcance:

- el cliente (`magi`) y el servidor de sesiones (`magi --servidor`), incluidos
  el protocolo y el socket Unix;
- la verificación de huellas y la gestión de claves, frases y contraseñas
  (agente, fichero de clave, llavero del sistema, `known_hosts`);
- las escrituras en `~/.ssh` y en los ficheros de MAGI (`magi.db`,
  `magi_config`, `config.toml`);
- SFTP, transferencias, sincronización de directorios, túneles, snippets y
  deliberación;
- la app Android: su APK firmado, el Keystore, la sincronización y los túneles;
- el paquete de Termux (`.deb`) y su receta;
- la cadena de suministro: workflows de integración continua, `dist`, las
  atestaciones de GitHub, el PKGBUILD de AUR y la fórmula de Homebrew.

Fuera del alcance:

- vulnerabilidades en dependencias de terceros: repórtalas también a su
  proyecto, aunque nos avises para valorar el impacto y actualizar;
- fallos que solo se producen con un atacante que ya controla la cuenta del
  usuario (el modelo de amenaza es el de `ssh-agent`);
- hosts remotos comprometidos o configuraciones inseguras elegidas por el
  usuario (por ejemplo, un túnel escuchando en `0.0.0.0`);
- la falta de notarización de los binarios de macOS: está documentada y la
  fórmula de Homebrew no aplica cuarentena;
- ingeniería social y ataques físicos.

## Prácticas de seguridad del proyecto

- Ninguna clave privada ni frase se guarda en disco ni en el log; los secretos
  en memoria usan `zeroize` y las contraseñas viven solo en el llavero.
- `magi.db` guarda referencias, nunca secretos, tampoco cifrados.
- Los binarios publicados salen solo de CI a partir de un tag, con SHA-256 y
  atestaciones de procedencia de GitHub, y `cargo deny` revisa licencias y
  avisos de RustSec en cada PR.
- Antes de la primera publicación se pasó `gitleaks` sobre todo el historial, y
  desde entonces la prueba `repositorio_publico` impide en cada PR que entren
  datos reales (IP, rutas personales, correos) en documentación, tests y
  ejemplos. El historial anterior no se ha reescrito.
- No se aceptan cambios que añadan telemetría ni conexiones salientes fuera de
  los hosts del usuario.

## Reporting a vulnerability (English)

Please **do not open a public issue** for a vulnerability. Use GitHub's private
advisory: go to the repository's *Security* tab and choose *Report a
vulnerability* ([direct link](https://github.com/HectorEspejo/magi/security/advisories/new)).
Include the MAGI version (`magi --version`), your operating system, the steps to
reproduce it and the impact you think it has, without real keys or passwords.
You will get an acknowledgement within 72 hours. We maintain the latest 1.x
minor release of the desktop client and the latest release of the Android app
(beta); fixes ship as patch releases. Issues in third-party dependencies, an
attacker who already controls the user's account, and compromised remote hosts
are out of scope.
