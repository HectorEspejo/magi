# Instalación, actualización y desinstalación

[← Índice del manual](index.md)

MAGI es un gestor SSH de terminal. Se instala con el paquete de tu sistema o con
el archivo de la [página de releases](https://github.com/HectorEspejo/magi/releases).

## Requisitos {#requisitos}

- Linux o macOS con una terminal que admita 256 colores.
- Un agente SSH o claves en `~/.ssh`.

## Instalación por canal {#instalacion-por-canal}

| Canal | Plataforma | Orden | Verificación |
|---|---|---|---|
| AUR | Arch Linux y Omarchy | `yay -S magi-ssh-bin` | `pacman -Qi magi-ssh-bin` |
| Homebrew | macOS y Linux | `brew install HectorEspejo/tap/magi-ssh` | `brew info magi-ssh` |
| Archivo `.tar.xz` | Cualquier Linux | `tar -xJf magi-ssh-x86_64-unknown-linux-gnu.tar.xz` | `sha256sum --check magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256` |

Después de instalar, comprueba que todo va bien con `magi diagnostico`. Para
seguir, mira los [túneles](tuneles.md), [el ancla propia](tuneles.md#ancla-propia)
o lo que [se ha probado](estado.md#por-plataforma).

## Desinstalar {#desinstalar}

Borra el paquete con el gestor de tu sistema. Los datos de MAGI quedan en
`~/.local/share/magi`; bórralos a mano si no los quieres.

## Configuración mínima {#configuracion-minima}

El fichero `config.toml` vive en `~/.config/magi/`. Un ejemplo:

```toml
[terminal]
teclado_mejorado = true
historial_lineas = 10000
```

Y la salida de `magi diagnostico`, sin lenguaje:

```
servidor: en marcha
llavero: disponible
```
