# Primeros pasos

Conecta con `magi demo-web-01` o con `magi <host>`: el nombre va en el inventario.
El segundo ejemplo, con <host> suelto en la prosa y también <uuid> aquí, se escapa como texto.

## Instalación

Pasos de ejemplo en `192.0.2.10` con el usuario `usuario`. La [guía de túneles](tuneles.md)
sigue y el [apartado de seguridad](tuneles.md#túnel-local-y-remoto) también.

## Instalación

El mismo título otra vez: su ancla lleva `-1`.

## Sincronización del inventario

Un título con acentos y una [referencia](tuneles.md#el-túnel-ñandú) a otro capítulo.

## Qué es `Include` y [`estado.md`](estado.md)

Título con código y un enlace.

## ÁRBOL y ÑANDÚ

Mayúsculas acentuadas: su ancla las pasa a minúsculas.

## Código

```sh
# esto no es un título
magi conectar <host> --nombre {{servicio}}
## ni esto
```

~~~text
# tampoco esto
~~~

    # ni esto, que lleva sangría

Un comentario <!-- se quita --> en medio, y otro en su propia línea:

<!-- este comentario
ocupa varias líneas -->

Texto después. El valor `{{servicio:nginx}}` es un ejemplo, no un preprocesador.

## Enlaces

- A un fichero del repositorio: [el formato](../formato-sincronizacion.md#formato).
- A una carpeta: [paquete](../../packaging/termux/).
- A la portada: [README](../../README.md) y [English](../../README.en.md#install).
- Externo: [ejemplo](https://ejemplo.org/pagina?a=1&b=2) y <https://ejemplo.org/> y
  <usuario@ejemplo.org>.
- Solo un fragmento: [arriba](#primeros-pasos) y [ancla](#instalación).
- En código `[no](../../README.md)` no se toca.

[referencia]: ../../SECURITY.md "Seguridad"
Un enlace [de referencia][referencia] y un atajo [referencia].
