# Túneles

[← Índice del manual](index.md)

Un túnel local abre un puerto en tu equipo y lo lleva, por SSH, a un servicio del
host. MAGI los guarda en el inventario y los activa desde la vista Túneles.

## Túneles locales {#tuneles-locales}

Un túnel local escucha siempre en `127.0.0.1`. Esta orden de `ssh` hace lo mismo
que un túnel de MAGI:

```bash
ssh -N -L 127.0.0.1:8080:127.0.0.1:80 usuario@192.0.2.10   # tunel-demo
```

Con MAGI no hace falta escribirla: eliges el host y los puertos, y MAGI abre la
conexión por ti. Si el puerto está ocupado, el túnel pasa a `caído` con el
motivo.

## Un título con ancla propia {#ancla-propia}

Este título lleva su propio `id`. Los enlaces a `tuneles.md#ancla-propia` tienen
que seguir funcionando en la web. Vuelve a los [requisitos](instalacion.md#requisitos).

## Tipos de túnel {#tipos}

| Tipo | Escucha en | Destino | Cuándo se usa | Ejemplo de orden equivalente |
|---|---|---|---|---|
| Local | `127.0.0.1:puerto` | Un servicio del host | Llegar a una base de datos que solo escucha en el host | `ssh -N -L 127.0.0.1:5432:127.0.0.1:5432 usuario@web-01.ejemplo.org` |
| Remoto | El host | Un servicio de tu equipo | Enseñar un servidor de desarrollo a otro equipo | `ssh -N -R 127.0.0.1:9000:127.0.0.1:3000 usuario@web-01.ejemplo.org` |
| Dinámico | `127.0.0.1:puerto` | Cualquiera, por SOCKS5 | Navegar como si estuvieras en la red del host | `ssh -N -D 127.0.0.1:1080 usuario@web-01.ejemplo.org` |

La dirección `/home/usuario/.ssh/config` no cambia: MAGI usa su propio fichero,
`magi_config`.
