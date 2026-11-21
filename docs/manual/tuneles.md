# Túneles

[← Índice del manual](README.md)

## Túneles

`F6` abre **Túneles**: los reenvíos definidos en la ficha de cada host y lo
que el servidor tiene levantado en ese momento. Los ejecuta el **servidor de
sesiones**, así que cerrar la ventana no los tira.

| Tipo | Qué hace |
|---|---|
| `local` | MAGI escucha en `escucha` y el host abre el destino (`ssh -L`). |
| `remoto` | El host escucha en `escucha` y MAGI abre el destino en local (`ssh -R`). |
| `dinámico` | Proxy SOCKS5 que escucha en `escucha` y sale por el host (`ssh -D`); no lleva destino. |

- **Alta**: `n` crea un túnel (host, nombre, tipo, escucha y destino), `e`
  edita la fila y `x` la borra tras confirmar. `a` marca el túnel como
  automático y `↵` abre el detalle (origen, conexiones, tráfico, último error).
- **Marcha**: `Espacio` activa un inactivo, para lo que esté en marcha y
  descarta el error de uno caído; `r` relanza un caído con los contadores a
  cero. `/` filtra y `Esc` limpia el filtro o vuelve a la vista anterior.
- **Credenciales**: levantar un túnel abre la conexión del host, así que si
  falta aceptar una huella o la clave pide frase, el diálogo sale igual que en
  una sesión. El ciclo automático nunca dialoga: deja el túnel caído con el
  motivo.
- **Automáticos**: se levantan con la primera pestaña o canal SFTP del host y
  se paran con el último. Un túnel manual nunca se para solo, y uno automático
  parado a mano no vuelve hasta el siguiente ciclo.

Desde fuera de la TUI:

```sh
magi tuneles                        # definidos y activos, con estado y tráfico
magi tunel activar <host> <nombre>  # lo levanta (autolanza el servidor si no está)
magi tunel parar <host> <nombre>    # lo para
```

Estas órdenes no dialogan: si el host necesita credenciales, avisan de que hay
que abrir una sesión desde la TUI primero. Y un túnel levantado mantiene el
servidor en pie: no se apaga por inactividad mientras haya alguno en marcha.

Los reenvíos de `~/.ssh/config` (`LocalForward`, `RemoteForward`,
`DynamicForward`) entran en la tabla de túneles al importar y vuelven a
`magi_config` al exportar; ya no se guardan en las «opciones extra» del host.

**Aviso de seguridad**: por defecto todo escucha en `127.0.0.1`. Una escucha
en `0.0.0.0` o `::` la puede usar cualquier equipo de tu red (el diálogo lo
avisa en ámbar) y el SOCKS5 no pide autenticación a quien lo use. En un túnel
remoto, el host solo expondrá esa escucha si tiene `GatewayPorts`.
