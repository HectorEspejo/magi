# Datos de ejemplo de las releases

Ficheros `releases.json` (el formato de `DocumentoReleases`) y una página de la API
de GitHub para las pruebas de `magi-web` y para `scripts/generar-web.sh --sin-red`.
Todo es inventado: los hashes son falsos (el SHA-256 de `fixture-falso:<etiqueta>/<fichero>`)
y la huella del certificado de la app es la de abajo, que no es la de ningún keystore.

Huella usada por las pruebas: `11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00`
(`huella_distinta.json` lleva otra, `FE:DC:BA:98:…`). El `web/datos/plataformas.toml` real
tiene la huella vacía hasta que haya app publicada: para usar un fichero con app hay que
cargarlo con una huella puesta (`Plataformas::desde_texto`).

| Fichero | Qué tiene |
|---|---|
| `completo.json` | Escritorio v1.6.1 (con `.deb` de Termux) y v1.6.0, la candidata `v1.6.2-rc.1`, el borrador `v1.7.0` y la app `android-v0.2.0` con su huella |
| `sin-red.json` | Solo el escritorio v1.6.1 con Termux y sin app: pasa con el `plataformas.toml` real |
| `sin_app.json` | Como `completo.json` sin ninguna release de la app |
| `sin_deb_en_la_ultima.json` | v1.6.2 sin `.deb` y v1.6.1 con él: Termux cae a la 1.6.1 |
| `deb_sin_sha.json` | v1.6.1 con el `.deb` pero sin su `.sha256` y v1.6.0 completa: Termux cae a la 1.6.0 |
| `sha_otro_nombre.json` | El `.sha256` del tarball de Linux x86_64 nombra otro fichero: debe fallar |
| `huella_distinta.json` | La release de la app dice una huella distinta de la de las pruebas: debe fallar |
| `etiqueta_rara.json` | Etiquetas que no cumplen el patrón (`v01.6.2`, `v1.7`, `v2.0.0.0`, `v1.6.3-rc.1` sin marcar como candidata…): solo sale la v1.6.1 |
| `api_pagina_1.json` | Una página real de la API (con todos sus campos de más), recortada de cuerpo y con los `digest` cambiados |
