# Prompt MAGI - Fase 10: Sincronización Cifrada

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3, para que el mismo inventario esté en todos los equipos del usuario mediante ficheros cifrados en una carpeta que él elige (Syncthing, Nextcloud, iCloud…). `magi.db` no se cifra. Stack: **el de `Cargo.toml` más `age`, `hmac`, `hkdf`, `uuid`, `notify` y `qrcode`**. Tablas (migración 7): `uuid`, `version` (`<ms>.<uuid equipo>`) y `vector` (JSON por equipo) en **GRUPOS, HOSTS, IDENTIDADES** (uuid v5 de la huella), **TUNELES, SNIPPETS, SINCRONIZACIONES_DIR**; nuevas (1) **EQUIPOS** con uuid, nombre, plataforma, es_este, app_version, ultimo_fichero_en, ultima_importacion_en, hash_ultimo_fichero, estado, quitado_en, reloj_ms; (2) **LAPIDAS** con uuid, tipo, nombre, version, vector, borrado_en; (3) **CONFLICTOS** con tipo, elemento_tipo, uuid, nombre, version_perdedora_json, version_ganadora, equipo_ganador, detectado_en, resuelto_en, resolucion. Funcionalidades: reloj híbrido; solo los campos que viajan suben la versión; un fichero `magi-<uuid>.age` por equipo (instantánea completa, cabecera, firma HMAC derivada con HKDF y JSON `formato` 1, cifrado con age X25519); fusión por vectores (dominante gana; concurrentes, gana la más reciente y la versión local perdedora va a CONFLICTOS), lápidas permanentes, huérfanos, renombrado `(2)` en colisiones y borrados aplazados si hay trabajo vivo; huellas de `known_hosts` que solo se añaden si no hay ninguna; crear o unir (unión por nombre), clave en el llavero mostrada como texto y QR, quitar equipo con cambio de clave y parada si un fichero conocido deja de abrirse; pasada al abrir, al cambiar la carpeta y tras cada cambio, con cerrojo; copia de `magi.db` y confirmación de borrados masivos; `magi sincronizar [estado]`. Interfaz: vistas SINCRONIZACIÓN y CONFLICTOS desde la paleta, diálogos CLAVE, UNIR e IMPORTACIÓN CON BORRADOS, indicador `⇅` en la barra. Documenta el formato en `docs/formato-sincronizacion.md` para la app Android.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase10-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase10-implementacion.md`**
   (informe de implementación) con exactamente esta estructura:
   - Resumen de lo implementado
   - Desviaciones respecto a la especificación (qué y por qué)
   - Estructura de archivos creada/modificada
   - Decisiones técnicas tomadas durante el desarrollo
   - Funcionalidades del checklist completadas (copiando su texto exacto)
   - Pendientes y bloqueos
   - Ejecución y pruebas (cómo arrancar, migrar y testear)
4. Actualiza el informe de implementación en cada sesión, no lo
   regeneres desde cero.
5. Cuando el informe esté listo, avisa al desarrollador de que debe
   entregárselo al analista funcional: hasta entonces la documentación de
   proyecto (checklist maestro, informe maestro) no refleja la realidad.
