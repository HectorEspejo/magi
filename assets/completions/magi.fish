# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_magi_global_optspecs
    string join \n servidor h/help V/version
end

function __fish_magi_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_magi_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_magi_using_subcommand
    set -l cmd (__fish_magi_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c magi -n "__fish_magi_needs_command" -l servidor -d 'Ejecuta el servidor de sesiones en primer plano'
complete -c magi -n "__fish_magi_needs_command" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_needs_command" -s V -l version -d 'Print version'
complete -c magi -n "__fish_magi_needs_command" -f -a "importar" -d 'Importa un ssh_config (por defecto ~/.ssh/config) sin abrir la TUI'
complete -c magi -n "__fish_magi_needs_command" -f -a "exportar" -d 'Regenera ~/.ssh/magi_config sin abrir la TUI'
complete -c magi -n "__fish_magi_needs_command" -f -a "sondear" -d 'Sondea la flota sin abrir la TUI'
complete -c magi -n "__fish_magi_needs_command" -f -a "registro" -d 'Exporta el historial de MAGI'
complete -c magi -n "__fish_magi_needs_command" -f -a "servidor" -d 'El servidor de sesiones: estado o parada'
complete -c magi -n "__fish_magi_needs_command" -f -a "conectar" -d 'Abre la TUI con una sesión nueva al host indicado'
complete -c magi -n "__fish_magi_needs_command" -f -a "tunel" -d 'Activa o para un túnel del host indicado'
complete -c magi -n "__fish_magi_needs_command" -f -a "tuneles" -d 'Lista los túneles definidos y activos'
complete -c magi -n "__fish_magi_needs_command" -f -a "snippets" -d 'Lista los snippets con sus destinos resueltos, si son críticos y su último uso'
complete -c magi -n "__fish_magi_needs_command" -f -a "sincronizaciones" -d 'Lista las sincronizaciones de directorio guardadas con su host, dirección, rutas, borrado y último resultado'
complete -c magi -n "__fish_magi_needs_command" -f -a "sincronizar" -d 'Sincroniza el inventario entre equipos por la carpeta cifrada, sin abrir la TUI'
complete -c magi -n "__fish_magi_needs_command" -f -a "diagnostico" -d 'Comprueba lo que MAGI necesita en este sistema sin modificar nada'
complete -c magi -n "__fish_magi_needs_command" -f -a "autocompletado" -d 'Escribe el script de autocompletado de la shell en la salida estándar'
complete -c magi -n "__fish_magi_needs_command" -f -a "man" -d 'Escribe la página man de MAGI en la salida estándar'
complete -c magi -n "__fish_magi_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand importar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand exportar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand sondear" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand registro; and not __fish_seen_subcommand_from exportar help" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand registro; and not __fish_seen_subcommand_from exportar help" -f -a "exportar" -d 'Exporta el registro a CSV (por defecto) o JSON'
complete -c magi -n "__fish_magi_using_subcommand registro; and not __fish_seen_subcommand_from exportar help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand registro; and __fish_seen_subcommand_from exportar" -l desde -d 'Solo entradas desde esta fecha (AAAA-MM-DD)' -r
complete -c magi -n "__fish_magi_using_subcommand registro; and __fish_seen_subcommand_from exportar" -l json -d 'Exporta en JSON en lugar de CSV'
complete -c magi -n "__fish_magi_using_subcommand registro; and __fish_seen_subcommand_from exportar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand registro; and __fish_seen_subcommand_from help" -f -a "exportar" -d 'Exporta el registro a CSV (por defecto) o JSON'
complete -c magi -n "__fish_magi_using_subcommand registro; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand servidor; and not __fish_seen_subcommand_from estado parar help" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand servidor; and not __fish_seen_subcommand_from estado parar help" -f -a "estado" -d 'Imprime pid, versión de protocolo, sesiones y clientes del servidor'
complete -c magi -n "__fish_magi_using_subcommand servidor; and not __fish_seen_subcommand_from estado parar help" -f -a "parar" -d 'Cierra todas las sesiones y apaga el servidor'
complete -c magi -n "__fish_magi_using_subcommand servidor; and not __fish_seen_subcommand_from estado parar help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from estado" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from parar" -l si -d 'No pide confirmación aunque haya sesiones abiertas'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from parar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from help" -f -a "estado" -d 'Imprime pid, versión de protocolo, sesiones y clientes del servidor'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from help" -f -a "parar" -d 'Cierra todas las sesiones y apaga el servidor'
complete -c magi -n "__fish_magi_using_subcommand servidor; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand conectar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand tunel; and not __fish_seen_subcommand_from activar parar help" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand tunel; and not __fish_seen_subcommand_from activar parar help" -f -a "activar" -d 'Levanta el túnel sobre el servidor de sesiones'
complete -c magi -n "__fish_magi_using_subcommand tunel; and not __fish_seen_subcommand_from activar parar help" -f -a "parar" -d 'Para el túnel y cierra lo que estaba escuchando'
complete -c magi -n "__fish_magi_using_subcommand tunel; and not __fish_seen_subcommand_from activar parar help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand tunel; and __fish_seen_subcommand_from activar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand tunel; and __fish_seen_subcommand_from parar" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand tunel; and __fish_seen_subcommand_from help" -f -a "activar" -d 'Levanta el túnel sobre el servidor de sesiones'
complete -c magi -n "__fish_magi_using_subcommand tunel; and __fish_seen_subcommand_from help" -f -a "parar" -d 'Para el túnel y cierra lo que estaba escuchando'
complete -c magi -n "__fish_magi_using_subcommand tunel; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand tuneles" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand snippets" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand sincronizaciones" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and not __fish_seen_subcommand_from estado help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and not __fish_seen_subcommand_from estado help" -f -a "estado" -d 'Muestra la carpeta, los equipos con su estado y los conflictos pendientes, sin descifrar ningún fichero ni leer la clave del llavero'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and not __fish_seen_subcommand_from estado help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and __fish_seen_subcommand_from estado" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and __fish_seen_subcommand_from help" -f -a "estado" -d 'Muestra la carpeta, los equipos con su estado y los conflictos pendientes, sin descifrar ningún fichero ni leer la clave del llavero'
complete -c magi -n "__fish_magi_using_subcommand sincronizar; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand diagnostico" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand autocompletado" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand man" -s h -l help -d 'Print help'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "importar" -d 'Importa un ssh_config (por defecto ~/.ssh/config) sin abrir la TUI'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "exportar" -d 'Regenera ~/.ssh/magi_config sin abrir la TUI'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "sondear" -d 'Sondea la flota sin abrir la TUI'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "registro" -d 'Exporta el historial de MAGI'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "servidor" -d 'El servidor de sesiones: estado o parada'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "conectar" -d 'Abre la TUI con una sesión nueva al host indicado'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "tunel" -d 'Activa o para un túnel del host indicado'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "tuneles" -d 'Lista los túneles definidos y activos'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "snippets" -d 'Lista los snippets con sus destinos resueltos, si son críticos y su último uso'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "sincronizaciones" -d 'Lista las sincronizaciones de directorio guardadas con su host, dirección, rutas, borrado y último resultado'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "sincronizar" -d 'Sincroniza el inventario entre equipos por la carpeta cifrada, sin abrir la TUI'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "diagnostico" -d 'Comprueba lo que MAGI necesita en este sistema sin modificar nada'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "autocompletado" -d 'Escribe el script de autocompletado de la shell en la salida estándar'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "man" -d 'Escribe la página man de MAGI en la salida estándar'
complete -c magi -n "__fish_magi_using_subcommand help; and not __fish_seen_subcommand_from importar exportar sondear registro servidor conectar tunel tuneles snippets sincronizaciones sincronizar diagnostico autocompletado man help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from registro" -f -a "exportar" -d 'Exporta el registro a CSV (por defecto) o JSON'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from servidor" -f -a "estado" -d 'Imprime pid, versión de protocolo, sesiones y clientes del servidor'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from servidor" -f -a "parar" -d 'Cierra todas las sesiones y apaga el servidor'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from tunel" -f -a "activar" -d 'Levanta el túnel sobre el servidor de sesiones'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from tunel" -f -a "parar" -d 'Para el túnel y cierra lo que estaba escuchando'
complete -c magi -n "__fish_magi_using_subcommand help; and __fish_seen_subcommand_from sincronizar" -f -a "estado" -d 'Muestra la carpeta, los equipos con su estado y los conflictos pendientes, sin descifrar ningún fichero ni leer la clave del llavero'
