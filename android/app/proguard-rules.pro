# MAGI · app Android (Fase 11)
# Reglas de R8 para la release. De momento la minificación está desactivada
# (esqueleto); estas reglas quedan preparadas para cuando se active.

# JNA carga su biblioteca nativa y los mapeos por reflexión.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-dontwarn java.awt.**

# Bindings generados por UniFFI (paquete fijo del contrato).
-keep class org.cuatrodetres.magi.nucleo.** { *; }
