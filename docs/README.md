# Qué hay en `docs/`

[← README](../README.md) · [English README](../README.en.md)

Esta carpeta junta la documentación de MAGI. Está en castellano, como el código
y la interfaz. Si buscas cómo se usa, empieza por el [manual](manual/README.md);
el resto es para quien mantiene el proyecto, o para quien quiera saber por qué
MAGI es como es.

## Para usar MAGI

- [`manual/`](manual/README.md): el manual de usuario, por capítulos. En
  [`manual/estado.md`](manual/estado.md) está lo que se ha probado a mano y lo
  que no.
- [`formato-sincronizacion.md`](formato-sincronizacion.md): el formato del
  fichero cifrado con el que se sincronizan el inventario entre equipos.
  Es la referencia que usan el escritorio y la app Android, que comparten el
  mismo código.
- La web de MAGI, <https://magi.erratia.com>: la portada, las descargas con su
  suma SHA-256 y cómo verificarlas, las versiones y el manual de la última
  versión publicada. Se genera desde este repositorio; cómo, en
  [`../web/README.md`](../web/README.md).

## Para publicar

- [`publicar.md`](publicar.md): cómo se publica una versión del escritorio
  (GitHub Releases, AUR y Homebrew), la política de versiones y qué hacer si un
  canal falla.
- [`publicar-android.md`](publicar-android.md): cómo se publica la app Android
  (el keystore, las versiones candidatas y Obtainium).
- [`publicar-termux.md`](publicar-termux.md): cómo se publica el `.deb` de
  Termux y cómo se propone la receta a TUR.
- [`publicar-web.md`](publicar-web.md): cómo se publica la web (DNS, dominio
  verificado, ajustes de Pages, primera publicación, cómo repetirla y cómo
  retirarla). Cómo se genera y de dónde sale cada cosa está en
  [`../web/README.md`](../web/README.md).

## El archivo de diseño: `faseN/` y el maestro

MAGI se ha construido por fases. Cada fase se especifica antes de programarla
y se documenta al terminar; ese rastro está en las carpetas `faseN/`, y se
conserva como **archivo de diseño**: cuenta lo que se decidió en cada momento y
por qué, no cómo funciona hoy.

- [`magi-maestro.md`](magi-maestro.md): la visión global, el mapa de fases, las
  decisiones técnicas y los riesgos de todo el proyecto.
- `faseN/magi-faseN-informe.md`: la especificación funcional de la fase.
- `faseN/magi-faseN-checklist.md`: el alcance de la fase, verificable.
- `faseN/magi-faseN-implementacion.md`: lo que realmente se hizo, con sus
  desviaciones y sus pendientes. Es el documento que dice la verdad sobre cada
  fase.
- `faseN/magi-faseN-revision-adversarial.md`: la revisión de cierre, en las
  fases que la tienen.
- `faseN/magi-faseN-prompt.md` y `CLAUDE.md`: el encargo que recibió el agente
  de código y las reglas con las que trabajaba en ese momento.

Cómo leerlo: para entender una parte, empieza por el informe de su fase y sigue
con el informe de implementación; el maestro sirve de índice. **Los documentos
de las fases no se reescriben**, así que pueden contener datos que ya no son
ciertos (versiones, rutas, decisiones que luego se cambiaron). Lo que vale hoy
está en el manual y en el código.

### Por qué algunos documentos dicen `4d3-org`

La Fase 9 preparó la publicación pensando en alojar el repositorio en una
organización de GitHub llamada `4d3-org`, y los documentos de las fases y el
maestro conservan esa dirección (`4d3-org/magi`, `4d3-org/homebrew-tap`). Al
final el repositorio se quedó en [`HectorEspejo/magi`](https://github.com/HectorEspejo/magi)
y el tap es `HectorEspejo/homebrew-tap`. **Esas direcciones de `4d3-org` ya no
valen**: no las uses. Fuera de esta carpeta, todas las referencias apuntan a
`HectorEspejo`.
