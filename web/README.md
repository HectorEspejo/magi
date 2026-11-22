# Web de MAGI

Esta carpeta guarda lo propio de la web de MAGI (`https://magi.erratia.com`): las
plantillas, los estilos, las fuentes, los textos en castellano e inglés, los datos
de las plataformas y las capturas de la interfaz. Todo sale de `main`. El
generador es el crate `crates/magi-web` (biblioteca y binario `magi-web`), que
también trae de GitHub las versiones y los artefactos de cada release, prepara el
manual para mdBook y comprueba el sitio generado antes de desplegarlo.

Este fichero explica cómo se genera la web y de dónde sale cada cosa. Hoy está
completa la sección de fuentes; el resto del índice lo completa el cierre de la fase.

## Índice

- [Cómo se genera](#cómo-se-genera) <!-- S3: completar -->
- [De dónde sale cada cosa](#de-dónde-sale-cada-cosa) <!-- S3: completar -->
- [Manual](#manual) <!-- S3: completar -->
- [Dominios permitidos](#dominios-permitidos) <!-- S3: completar -->
- [Fuentes](#fuentes)
  - [Qué fuentes hay y de dónde salen](#qué-fuentes-hay-y-de-dónde-salen)
  - [Cobertura de glifos](#cobertura-de-glifos)
  - [Rehacer el subconjunto de símbolos](#rehacer-el-subconjunto-de-símbolos)
  - [Reconvertir JetBrains Mono a woff2](#reconvertir-jetbrains-mono-a-woff2)

## Cómo se genera

<!-- S3: completar -->

Pendiente de completar en el cierre de la fase.

## De dónde sale cada cosa

<!-- S3: completar -->

Pendiente de completar en el cierre de la fase.

## Manual

<!-- S3: completar -->

Pendiente de completar en el cierre de la fase.

## Dominios permitidos

<!-- S3: completar -->

Pendiente de completar en el cierre de la fase.

## Fuentes

La web sirve sus fuentes desde sí misma (en `web/fuentes/`): ni Google Fonts ni
ningún otro dominio. Son tres ficheros woff2 y las licencias de lo que contienen.

### Qué fuentes hay y de dónde salen

| Fichero | Qué es | Origen | Licencia |
|---|---|---|---|
| `JetBrainsMono-Regular.woff2` | JetBrains Mono 2.305, peso normal | El TTF de la app Android (`android/app/src/main/res/font/jetbrains_mono_regular.ttf`), convertido a woff2 una vez | SIL OFL 1.1: [`fuentes/OFL.txt`](fuentes/OFL.txt) |
| `JetBrainsMono-Bold.woff2` | JetBrains Mono 2.305, negrita | El TTF de la app (`jetbrains_mono_bold.ttf`), convertido a woff2 una vez | La misma |
| `magi-simbolos.woff2` | Subconjunto de DejaVu Sans Mono 2.37 con los nueve glifos de la TUI que JetBrains Mono no trae | `DejaVuSansMono.ttf` del tarball oficial de DejaVu 2.37, recortado con `pyftsubset` | Bitstream Vera y dominio público (Arev): [`fuentes/LICENSE-DejaVu.txt`](fuentes/LICENSE-DejaVu.txt) |

`OFL.txt` es una copia literal de `android/app/src/main/res/raw/licencia_jetbrains_mono.txt`
(la misma que enseña la app en «Licencias») y `LICENSE-DejaVu.txt`, la del fichero
`LICENSE` del tarball de DejaVu, sin tocar. La lista de glifos del subconjunto, con
un comentario por símbolo, es `fuentes/magi-simbolos.unicodes`.

Lo que se descargó y las sumas SHA-256 de todo lo que interviene:

| Qué | SHA-256 |
|---|---|
| `dejavu-fonts-ttf-2.37.tar.bz2` | `fa9ca4d13871dd122f61258a80d01751d603b4d3ee14095d65453b4e846e17d7` |
| `ttf/DejaVuSansMono.ttf` (dentro del tarball) | `b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582` |
| `LICENSE` (dentro del tarball) = `LICENSE-DejaVu.txt` | `7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93` |
| `jetbrains_mono_regular.ttf` (de la app) | `e6fd0d7e91550b3ed2b735d4312474362c4716edc4fc0577a0f61ed782d5aed1` |
| `jetbrains_mono_bold.ttf` (de la app) | `d22c4f3821d725eb01210d278d95dfcfcaadc34699a06658d47c8a5cc5830ada` |
| `licencia_jetbrains_mono.txt` (de la app) = `OFL.txt` | `a76abf002c49097d146e86740a3105a5d00450b1592e820a1109a8c5680cd697` |
| `JetBrainsMono-Regular.woff2` (resultado) | `139a4008ab7480821fe5f7e2a42a1e39782a882ee7deb09e73eff4f9b7a00773` |
| `JetBrainsMono-Bold.woff2` (resultado) | `131a42ad32d2a136a9ec561497d6408732dbdafa888797a2a7601627179a7efd` |
| `magi-simbolos.woff2` (resultado) | `17981eac73141b2eff3fc55a592c08cbd3a284c3e3415ebe0d04d6c4afa4794d` |

El tarball se baja de la release oficial de DejaVu en GitHub:

```text
https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2
```

Herramientas con las que salieron los tres woff2 (versiones que dejan los mismos
bytes; con otras, lo que cuenta es que pasen las verificaciones de más abajo):

| Herramienta | Versión | Para qué |
|---|---|---|
| `fonttools` (`pyftsubset`) | 4.68.0 | Recortar DejaVu Sans Mono y comprobar glifos |
| `brotli` (módulo de Python) | 1.2.1 | Comprimir el woff2 que escribe `pyftsubset` |
| Python | 3.14 | Ejecutar fontTools |
| `woff2_compress` y `woff2_decompress` de Google (paquete `woff2`) | 1.0.2 | Convertir JetBrains Mono a woff2 y deshacerlo para verificar |

La licencia de DejaVu admite recortar y modificar la fuente siempre que el
resultado no se llame «Bitstream» ni «Vera». El subconjunto conserva el nombre
«DejaVu Sans Mono» y el aviso de copyright en su tabla `name`, así que cumple.

**Avance de los glifos.** JetBrains Mono avanza 0,600 em (600 de 1000 unidades) y
DejaVu Sans Mono 0,602 em (1233 de 2048, que son 0,6021): un 0,34 % más. Cada
símbolo del subconjunto ocupa 0,0034 columnas más que un carácter de JetBrains
Mono, y solo lo suma él: una línea con unos pocos símbolos se corre una centésima
de columna, y las cajas de las capturas no se descuadran. El `@font-face` de
`magi-simbolos` debe declarar con `unicode-range` los mismos puntos de código que
`fuentes/magi-simbolos.unicodes`.

### Cobertura de glifos

JetBrains Mono 2.305 no tiene diez de los glifos que usa la TUI en sus
instantáneas: `↵ ↻ ⇄ ⇅ ▣ ◐ ◑ ★ ⚑ ⤴`. Los nueve primeros están en el subconjunto. El
décimo, `⤴` (U+2934, el «salto» de la vista Hosts), tampoco está en DejaVu Sans
Mono: en las capturas se cambia por su equivalente ASCII de
`disposicion::texto_ascii`, que es el único degradador.

`crates/magi-web/src/fuentes.rs` lo vigila sin salir de Rust:

- `codigos_de_ttf` lee la tabla `cmap` de un TTF (formatos 4 y 12) y devuelve los
  puntos de código con glifo; se usa sobre los TTF de la app, que son los mismos
  que se convirtieron a woff2.
- `leer_unicodes` lee `fuentes/magi-simbolos.unicodes`, que es a la vez la entrada
  de `pyftsubset` y la fuente de verdad de la prueba. Los woff2 no se leen:
  el subconjunto se comprueba contra esa lista y, al rehacerlo, con fontTools.
- La prueba de cobertura de las capturas falla si una captura trae un carácter
  que no está en JetBrains Mono, ni en el subconjunto, ni en la tabla de degradación.

Si cambias la lista, cambia en el mismo PR el subconjunto (con el procedimiento de
abajo), el `unicode-range` del CSS y la prueba `el_subconjunto_declara_nueve_de_los_diez`.

### Rehacer el subconjunto de símbolos

Se hace a mano, una vez por cambio de la lista. Todo se descarga y se prepara fuera
del repositorio, en `$HOME/.cache/magi/fuentes/`, con cada descarga en su propio
directorio nuevo y vacío. Los scripts de Python viven fuera de lo descargado y se
ejecutan siempre con `python3 -I`. Lanza los bloques en orden, en la misma terminal
(comparten variables) y empezando desde dentro del repositorio.

**Paso 1.** Descarga DejaVu 2.37 y comprueba su suma:

```sh
BASE="$HOME/.cache/magi/fuentes"
mkdir -p "$BASE/scripts"

mkdir "$BASE/dejavu-2.37"
curl -fL -o "$BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" \
  https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2
echo "fa9ca4d13871dd122f61258a80d01751d603b4d3ee14095d65453b4e846e17d7  $BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" | sha256sum -c -

mkdir "$BASE/dejavu-2.37-extraido"
tar -xjf "$BASE/dejavu-2.37/dejavu-fonts-ttf-2.37.tar.bz2" -C "$BASE/dejavu-2.37-extraido"
DEJAVU="$BASE/dejavu-2.37-extraido/dejavu-fonts-ttf-2.37"
echo "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582  $DEJAVU/ttf/DejaVuSansMono.ttf" | sha256sum -c -
echo "7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93  $DEJAVU/LICENSE" | sha256sum -c -
```

En macOS, `sha256sum` es `shasum -a 256`.

**Paso 2.** Crea el entorno de Python con las versiones de arriba:

```sh
python3 -I -m venv "$BASE/venv"
"$BASE/venv/bin/python" -I -m pip install fonttools==4.68.0 brotli==1.2.1
```

**Paso 3.** Comprueba que DejaVu Sans Mono tiene los nueve glifos y no tiene `⤴`, y mira su
avance:

```sh
"$BASE/venv/bin/python" -I - "$DEJAVU/ttf/DejaVuSansMono.ttf" <<'EOF'
import sys
from fontTools.ttLib import TTFont

fuente = TTFont(sys.argv[1])
cmap = fuente.getBestCmap()
for codigo in (0x21B5, 0x21BB, 0x21C4, 0x21C5, 0x25A3, 0x25D0, 0x25D1, 0x2605, 0x2691, 0x2934):
    glifo = cmap.get(codigo)
    print(f"U+{codigo:04X} {chr(codigo)}", glifo or "SIN GLIFO")
print("unidades por em:", fuente["head"].unitsPerEm)
EOF
```

Salen los nueve primeros con glifo y `U+2934` con «SIN GLIFO».

**Paso 4.** Edita `web/fuentes/magi-simbolos.unicodes` si hace falta y genera el subconjunto
en un directorio nuevo (el aviso `FFTM NOT subset` es inocuo):

```sh
REPO="$(git rev-parse --show-toplevel)"
mkdir "$BASE/salida-simbolos"
cd "$BASE"
"$BASE/venv/bin/pyftsubset" "$DEJAVU/ttf/DejaVuSansMono.ttf" \
  --unicodes-file="$REPO/web/fuentes/magi-simbolos.unicodes" \
  --flavor=woff2 --layout-features= --no-hinting --desubroutinize \
  --output-file="$BASE/salida-simbolos/magi-simbolos.woff2"
ls -l "$BASE/salida-simbolos/magi-simbolos.woff2"
```

- `--unicodes-file` es la lista de este repositorio, tal cual.
- `--layout-features=` (vacío) quita las características OpenType: los símbolos
  no las necesitan.
- `--no-hinting` quita las instrucciones de ajuste y `--desubroutinize` aplana
  las subrutinas (solo importa en fuentes CFF; en TrueType es inocuo).
- `--flavor=woff2` pide el módulo `brotli` del paso 2.

Debe pesar unos pocos KB (hoy, 1168 bytes).

**Paso 5.** Verifica con fontTools que contiene exactamente los glifos de la lista (nueve) y
ninguno más, y apunta su avance:

```sh
cat > "$BASE/scripts/ver_simbolos.py" <<'EOF'
import re, sys
from fontTools.ttLib import TTFont

woff2, unicodes = sys.argv[1], sys.argv[2]

# Los puntos de código de la lista (U+XXXX o U+XXXX-YYYY; lo que sigue a «#» es un comentario).
esperados = set()
for linea in open(unicodes, encoding="utf-8"):
    for ini, fin in re.findall(r"U\+([0-9A-Fa-f]+)(?:-([0-9A-Fa-f]+))?", linea.split("#")[0]):
        esperados.update(range(int(ini, 16), int(fin or ini, 16) + 1))

fuente = TTFont(woff2)
cmap = fuente.getBestCmap()
assert set(cmap) == esperados, "sobran o faltan: " + ", ".join(f"U+{c:04X}" for c in sorted(set(cmap) ^ esperados))
assert len(cmap) == 9 and 0x2934 not in cmap
em = fuente["head"].unitsPerEm
for codigo in sorted(cmap):
    avance = fuente["hmtx"][cmap[codigo]][0]
    print(f"U+{codigo:04X} {chr(codigo)}  avance {avance}/{em} = {avance / em:.4f} em")
print("OK: exactamente los 9 glifos de la lista")
EOF
"$BASE/venv/bin/python" -I "$BASE/scripts/ver_simbolos.py" \
  "$BASE/salida-simbolos/magi-simbolos.woff2" "$REPO/web/fuentes/magi-simbolos.unicodes"
```

Hoy los nueve avanzan 1233/2048 = 0,6021 em, frente a 0,600 de JetBrains Mono.

**Paso 6.** Copia el resultado y repite las pruebas:

```sh
cp "$BASE/salida-simbolos/magi-simbolos.woff2" "$REPO/web/fuentes/magi-simbolos.woff2"
cd "$REPO" && cargo test -p magi-web --test fuentes
```

Si `LICENSE-DejaVu.txt` cambia con otra versión de DejaVu, cópiala tal cual del
tarball nuevo y actualiza las sumas de este fichero.

### Reconvertir JetBrains Mono a woff2

Los woff2 de JetBrains Mono salen de los TTF que ya lleva la app Android (la 2.305),
convertidos con `woff2_compress`, que escribe el resultado junto a su entrada: por
eso se trabaja sobre copias, en un directorio nuevo.

```sh
BASE="$HOME/.cache/magi/fuentes"
REPO="$(git rev-parse --show-toplevel)"
FUENTES_APP="$REPO/android/app/src/main/res/font"

mkdir -p "$BASE/scripts"
mkdir "$BASE/jetbrains-mono"
cp "$FUENTES_APP/jetbrains_mono_regular.ttf" "$BASE/jetbrains-mono/JetBrainsMono-Regular.ttf"
cp "$FUENTES_APP/jetbrains_mono_bold.ttf"    "$BASE/jetbrains-mono/JetBrainsMono-Bold.ttf"
(cd "$BASE/jetbrains-mono" && woff2_compress JetBrainsMono-Regular.ttf && woff2_compress JetBrainsMono-Bold.ttf)

cp "$BASE/jetbrains-mono/JetBrainsMono-Regular.woff2" "$BASE/jetbrains-mono/JetBrainsMono-Bold.woff2" "$REPO/web/fuentes/"
cp "$REPO/android/app/src/main/res/raw/licencia_jetbrains_mono.txt" "$REPO/web/fuentes/OFL.txt"
```

`woff2_compress` viene en el paquete `woff2` (Arch: `pacman -S woff2`; Debian y
Ubuntu: `apt install woff2`; macOS: `brew install woff2`). El resultado es
determinista: con la misma versión, las sumas de la tabla de arriba.

Para verificar, compara el woff2 con el TTF de la app (mismo orden de glifos, mismos
puntos de código, mismos avances y mismos contornos). Hace falta el entorno de
Python del apartado anterior:

```sh
cat > "$BASE/scripts/ver_jetbrains.py" <<'EOF'
import sys
from fontTools.ttLib import TTFont

original, convertida = TTFont(sys.argv[1]), TTFont(sys.argv[2])
assert original.getGlyphOrder() == convertida.getGlyphOrder(), "el orden de glifos cambió"
assert {(t.platformID, t.platEncID): t.cmap for t in original["cmap"].tables} == \
       {(t.platformID, t.platEncID): t.cmap for t in convertida["cmap"].tables}, "la tabla cmap cambió"
assert original["hmtx"].metrics == convertida["hmtx"].metrics, "los avances cambiaron"
for nombre in original.getGlyphOrder():
    a, b = original["glyf"][nombre], convertida["glyf"][nombre]
    assert a.numberOfContours == b.numberOfContours, nombre
    if a.numberOfContours:
        assert a.getCoordinates(original["glyf"])[:2] == b.getCoordinates(convertida["glyf"])[:2], nombre
print("OK:", len(original.getBestCmap()), "puntos de código y", len(original.getGlyphOrder()), "glifos idénticos")
EOF
for par in regular:Regular bold:Bold; do
  peso="${par%%:*}"
  nombre="${par##*:}"
  "$BASE/venv/bin/python" -I "$BASE/scripts/ver_jetbrains.py" \
    "$FUENTES_APP/jetbrains_mono_$peso.ttf" "$REPO/web/fuentes/JetBrainsMono-$nombre.woff2"
done
```

Cada uno debe terminar con `OK: 1372 puntos de código y 1754 glifos idénticos`.
Después, `cargo test -p magi-web --test fuentes` comprueba, además, que los TTF de la
app siguen sin los diez glifos de la TUI y que los tres ficheros son woff2.
