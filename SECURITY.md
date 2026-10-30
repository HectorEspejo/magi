# Política de seguridad

MAGI es un gestor SSH de terminal que se ejecuta en la máquina del usuario y
custodia sus conexiones. Esta política explica cómo reportar vulnerabilidades,
qué versiones se mantienen y qué queda dentro del alcance.

## Versiones soportadas

Se mantiene la última versión MINOR de la serie 1.x (hoy, la 1.0.x). Las
versiones MINOR anteriores dejan de recibir correcciones de seguridad cuando se
publica una nueva: las actualizaciones llegan por el gestor de paquetes (AUR o
Homebrew) o por el tarball de la release.

| Versión | Soportada |
|---|---|
| 1.0.x (última MINOR de 1.x) | Sí |
| 0.x y MINOR anteriores de 1.x | No |

La política de versiones completa está en [`docs/publicar.md`](docs/publicar.md).

## Cómo reportar una vulnerabilidad

**No abras un issue público con detalles del fallo.** Usa el
[aviso privado de seguridad de GitHub](https://github.com/4d3-org/magi/security/advisories/new):
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
- Antes de publicar se pasa `gitleaks` sobre todo el historial y se revisan
  datos reales en documentación, tests y ejemplos.
- No se aceptan cambios que añadan telemetría ni conexiones salientes fuera de
  los hosts del usuario.
