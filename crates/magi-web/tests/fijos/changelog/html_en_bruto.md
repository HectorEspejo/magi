# Registro de cambios de DEMO (con HTML y enlaces hostiles)

## [1.0.0] - 2026-11-22

**Resumen.** Versión con <b>HTML</b> en el resumen y un `código`.

**Summary.** Release with <i>HTML</i> in the summary.

### HTML en bruto

<script>alert(1)</script>

Una línea con <img src=x onerror=alert(1)> dentro y <a href="javascript:alert(2)">un enlace en bruto</a>.

<div onclick="alert(3)">un bloque en bruto</div>

Entidades que no se deshacen: &lt;script&gt;alert(4)&lt;/script&gt; y &amp;lt;.

### Enlaces

- [click](javascript:alert(5))
- [mayúsculas](JaVaScRiPt:alert(6))
- [entidad](javascript&colon;alert(7))
- [numérica](&#106;avascript:alert(8))
- [tabulador](java&#x09;script:alert(9))
- [datos](data:text/html;base64,AAAA)
- [vb](vbscript:msgbox(1))
- [inseguro](http://ejemplo.org/x)
- [correo](mailto:usuario@ejemplo.org)
- [fichero](file:///etc/hosts)
- [protocolo](//ejemplo.org/x)
- [solo ancla](#una-ancla)
- [con usuario](https://usuario:clave@ejemplo.org/)
- [puerto](https://ejemplo.org:8443/)
- [bueno](https://ejemplo.org/ruta)
- [relativo](docs/guia.md)
- [relativo con ancla](docs/guia.md#seccion-uno)
- [carpeta](docs/manual/)
- [sube demasiado](../../../etc/passwd)
- <https://ejemplo.org/autoenlace>
- <javascript:alert(10)>
- ![texto alternativo](https://ejemplo.org/imagen.png)
- ![sin peligro](javascript:alert(11))

### Referencias

- [una referencia mala][mala] y [otra buena][buena] y [la abreviada].
- [otra][BUENA] con otra capitalización, y [una][  buena  ] con espacios.
- [sin definir][no-existe] y [tampoco].

[mala]: javascript:alert(12)
[buena]: docs/guia.md
[la abreviada]: https://ejemplo.org/abreviada
