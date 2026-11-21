# Animación del README (VHS: https://github.com/charmbracelet/vhs).
#
# No se ejecuta a mano: la lanza scripts/grabar-demo.sh, que antes arranca el
# servidor de pruebas, prepara un entorno aislado (HOME, XDG_*, claves, huellas
# e inventario de assets/demo/inventario) y deja `magi` en el PATH. Los tres
# hosts son inventados y apuntan al servidor de pruebas; lo que enseñan las
# vistas (cifras del sondeo, ficheros) sale de la máquina que graba, así que hay
# que revisar los fotogramas antes de subir el GIF (el script lo recuerda).
#
# VHS solo envía teclas de texto y combinaciones con Ctrl: no tiene teclas de
# función. Por eso se abre Archivos con `s` y Servicios con `u` desde Flota, y se
# vuelve con la paleta («ir a flota»).
#
# El GIF tiene que pesar 2 MiB como mucho. Si pasa, recorta pasos o baja el
# tamaño, los fotogramas por segundo o los colores; no lo subas de más.

Output assets/demo/magi.gif

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 680
Set Padding 10
Set Framerate 10
Set TypingSpeed 60ms
Set Theme "Catppuccin Mocha"

# Prepara un prompt limpio, sin enseñar nada del entorno.
Hide
Type "export PS1='$ '"
Enter
Type "clear"
Enter
Show

# 1. Flota: los tres hosts con su sondeo.
Type "magi"
Enter
Sleep 5s

# 2. Archivos del host seleccionado (SFTP en panel doble).
Type "s"
Sleep 3s

# 3. De vuelta a Flota con la paleta.
Ctrl+P
Sleep 500ms
Type "ir a flota"
Sleep 1s
Enter
Sleep 2s

# 4. Servicios y contenedores del host seleccionado.
Type "u"
Sleep 4s

# 5. La paleta de órdenes.
Ctrl+P
Sleep 1s
Type "tun"
Sleep 2s
Escape
Sleep 1s
