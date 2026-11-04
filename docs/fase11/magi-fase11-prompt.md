# Prompt MAGI - Fase 11: App Android

Continúa desarrollando **MAGI**, el gestor SSH de terminal de 4d3, con su primera **app Android**: compañera de guardia con inventario de solo lectura, terminal SSH y túneles. Stack: **workspace de Cargo** (`magi-ssh` en la raíz; `crates/magi-nucleo` con lo compartido, movido sin reescribir; `crates/magi-movil`, cdylib con **UniFFI 0.32.2**) y **Kotlin + Jetpack Compose + Material 3** en `android/` (minSdk 30, targetSdk 37, NDK r30, cargo-ndk). Tablas: ninguna nueva; el móvil usa el mismo esquema (`user_version` 7): (1) **GRUPOS, HOSTS, ETIQUETAS, HOST_ETIQUETAS, TUNELES, IDENTIDADES, SNIPPETS, SNIPPET_DESTINOS, VERIFICACIONES_HOST, SINCRONIZACIONES_DIR** como espejo que solo escribe la pasada, salvo `plegado`, `ultimo_estado` y `ultima_conexion_en`; (2) **EQUIPOS, LAPIDAS, CONFLICTOS** (solo `huella` y `borrado`); (3) **REGISTRO** local con `clave_instalada`. Funcionalidades: núcleo sin rutas ni entorno, con carpeta, llavero, firmante y trabajo vivo inyectados; pasada en **modo lector** (nunca escribe en la carpeta, sin copia ni confirmación de borrados, aplazados que se aplican solos); carpeta por SAF solo lectura; unir por QR (CameraX + ZXing) o pegando la clave; clave del móvil **ECDSA P-256 en el Keystore**, no exportable; secretos cifrados con el Keystore; bloqueo con huella o PIN (5 min); autenticación clave del móvil → contraseña guardada → diálogo; huellas estrictas en un `known_hosts` privado; saltos; una conexión por host; «instalar mi clave»; sesiones con los estados de la F3; túneles local, remoto y SOCKS5 en `127.0.0.1` con puerto libre; servicio en primer plano `specialUse` y guía de batería. Interfaz: cabina de MAGI (paleta fija, JetBrains Mono), Hosts · Sesiones · Túneles · Ajustes, ficha, terminal en Compose con pantalla por tramos a 30 fps como máximo, entrada en bruto, fila de teclas extra, selección, zoom, teclado físico y dos paneles en plegable. Publicación: `android.yml`, `android-release.yml` con etiquetas `android-vX.Y.Z`, APK `arm64-v8a` firmado, atestación, nunca «Latest», Obtainium.

---

## Instrucciones para el agente

1. Lee primero `CLAUDE.md` en la raíz del repositorio: contiene las reglas
   permanentes de trabajo (idioma, commits, effort, cierre de fase).
2. Sigue el checklist `magi-fase11-checklist.md` como definición
   del alcance. No añadas funcionalidades fuera de él sin indicarlo.
3. Al finalizar el desarrollo o al pausar la sesión, marca el checklist y
   genera/actualiza el archivo **`magi-fase11-implementacion.md`**
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
