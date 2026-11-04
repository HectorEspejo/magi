// MAGI · app Android (Fase 11)
//
// AGP 9.5.0 trae Kotlin integrado: NO se aplica el plugin org.jetbrains.kotlin.android.
// La versión de Kotlin (KGP) se fija aquí y debe coincidir con la del catálogo
// (gradle/libs.versions.toml) y con el plugin del compilador de Compose.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
}

buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        // AGP 9 depende en tiempo de ejecución de KGP 2.2.10. Se declara aquí la
        // versión fijada (2.4.21) para compilar Kotlin con ella.
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.21")
    }
}
