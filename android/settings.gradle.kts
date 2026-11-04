// MAGI · app Android (Fase 11)
// Gradle 9.8.2 · AGP 9.5.0 · Kotlin 2.4.21 · Jetpack Compose + Material 3
//
// Repositorios: solo Google Maven y Maven Central (sin Google Play Services ni
// servicios de terceros en las dependencias de la app).

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "magi-android"

include(":app")
