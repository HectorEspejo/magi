import java.util.Properties

// MAGI · app Android (Fase 11)
// Kotlin 2.4.21 (KGP declarado en el build raíz) · Compose + Material 3 · AGP 9.5.0.
// Este módulo aplica el Kotlin integrado de AGP 9: no se aplica kotlin-android.

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// ===========================================================================
// 1. Versión de la app (android/version.properties · informe §7.10)
// ===========================================================================

val propiedadesVersion = Properties().apply {
    file("../version.properties").reader(Charsets.UTF_8).use { load(it) }
}

val versionApp: String = propiedadesVersion.getProperty("version")
    ?: throw GradleException("android/version.properties no define 'version'")

/** versionCode = MAYOR*100000 + MENOR*1000 + PARCHE*10 + R (R = rc 1-8, o 9 en la final). */
fun calcularCodigoVersion(version: String): Int {
    val patron = Regex("""^(\d+)\.(\d{1,2})\.(\d{1,2})(?:-rc\.([1-8]))?$""")
    val partes = patron.matchEntire(version)
        ?: throw GradleException("Versión no válida en android/version.properties: '$version'")
    val mayor = partes.groupValues[1].toInt()
    val menor = partes.groupValues[2].toInt()
    val parche = partes.groupValues[3].toInt()
    val candidata = partes.groupValues[4].toIntOrNull() ?: 9
    return mayor * 100_000 + menor * 1_000 + parche * 10 + candidata
}

val codigoVersion = calcularCodigoVersion(versionApp)

// ===========================================================================
// 2. Contrato con el núcleo Rust (crates/magi-movil)
//
//   - crate magi-movil → cdylib libmagi_movil.so
//   - .so por ABI en app/src/main/jniLibs/<abi>/libmagi_movil.so
//   - bindings Kotlin en el paquete org.cuatrodetres.magi.nucleo
//   - generador: cargo run -p magi-movil --features bindgen --bin uniffi-bindgen
// ===========================================================================

val directorioRaizRepositorio: File = rootProject.projectDir.parentFile
val directorioCrateMovil = File(directorioRaizRepositorio, "crates/magi-movil")
val directorioJniLibs = file("src/main/jniLibs")
val directorioBindings: File = layout.buildDirectory.dir("generated/uniffi/kotlin").get().asFile
val ndkFijado = "30.0.16248370"

/** SDK de Android: -PmagiSdkDir, ANDROID_HOME, ANDROID_SDK_ROOT, local.properties o ~/Android/Sdk. */
fun rutaSdkAndroid(): String {
    providers.gradleProperty("magiSdkDir").orNull?.let { return it }
    providers.environmentVariable("ANDROID_HOME").orNull?.let { return it }
    providers.environmentVariable("ANDROID_SDK_ROOT").orNull?.let { return it }
    val propiedadesLocales = file("../local.properties")
    if (propiedadesLocales.isFile) {
        val locales = Properties().apply {
            propiedadesLocales.reader(Charsets.UTF_8).use { load(it) }
        }
        locales.getProperty("sdk.dir")?.let { return it }
    }
    val porDefecto = File(System.getProperty("user.home"), "Android/Sdk")
    if (porDefecto.isDirectory) return porDefecto.absolutePath
    throw GradleException(
        "No encuentro el SDK de Android: define ANDROID_HOME, sdk.dir en local.properties " +
            "o pasa -PmagiSdkDir=<ruta>."
    )
}

/** NDK r30: ANDROID_NDK_HOME/ANDROID_NDK_ROOT si vienen del entorno, si no <sdk>/ndk/<versión>. */
fun rutaNdkAndroid(): String {
    listOf("ANDROID_NDK_HOME", "ANDROID_NDK_ROOT").forEach { nombre ->
        providers.environmentVariable(nombre).orNull?.takeIf { it.isNotBlank() }?.let { return it }
    }
    val ndk = File(rutaSdkAndroid(), "ndk/$ndkFijado")
    if (!ndk.isDirectory) {
        throw GradleException(
            "Falta el NDK r30 ($ndkFijado) en $ndk. Instálalo con: sdkmanager 'ndk;$ndkFijado'"
        )
    }
    return ndk.absolutePath
}

/** Falla con un mensaje claro mientras el crate del núcleo móvil no exista. */
fun comprobarCrateNucleo() {
    if (!File(directorioCrateMovil, "Cargo.toml").isFile) {
        throw GradleException(
            "No existe el crate del núcleo móvil (${directorioCrateMovil.path}/Cargo.toml).\n" +
                "Es el contrato de la Fase 11: crates/magi-movil debe ser una cdylib llamada " +
                "libmagi_movil.so (crate-type = [\"cdylib\", \"lib\"]) con el bin uniffi-bindgen.\n" +
                "Vuelve a ejecutar la tarea cuando el crate exista."
        )
    }
}

/** Prepara el entorno del NDK para una tarea cargo-ndk. */
fun org.gradle.api.tasks.Exec.prepararEntornoNdk() {
    doFirst {
        comprobarCrateNucleo()
        val sdk = rutaSdkAndroid()
        val ndk = rutaNdkAndroid()
        environment("ANDROID_HOME", sdk)
        environment("ANDROID_NDK_HOME", ndk)
        environment("ANDROID_NDK_ROOT", ndk)
    }
}

// ---------------------------------------------------------------------------
// 2.1. Tareas del núcleo (cargo y cargo-ndk)
// ---------------------------------------------------------------------------

val compilarNucleoAnfitrion = tasks.register<Exec>("compilarNucleoAnfitrion") {
    group = "núcleo"
    description = "Compila magi-movil para el anfitrión (pruebas JVM y bindings UniFFI)."
    workingDir = directorioRaizRepositorio
    commandLine("cargo", "build", "-p", "magi-movil")
    onlyIf {
        val hayCrate = File(directorioCrateMovil, "Cargo.toml").isFile
        if (!hayCrate) {
            logger.warn(
                "MAGI: todavía no existe crates/magi-movil; se omite la compilación " +
                    "del núcleo para el anfitrión."
            )
        }
        hayCrate
    }
}

val compilarNucleoDebug = tasks.register<Exec>("compilarNucleoDebug") {
    group = "núcleo"
    description = "Compila magi-movil con cargo-ndk para arm64-v8a y x86_64 (perfil debug)."
    workingDir = directorioRaizRepositorio
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-P", "30",
        "-o", directorioJniLibs.absolutePath,
        "build", "-p", "magi-movil",
    )
    prepararEntornoNdk()
}

val compilarNucleoRelease = tasks.register<Exec>("compilarNucleoRelease") {
    group = "núcleo"
    description = "Compila magi-movil con cargo-ndk para arm64-v8a (perfil release)."
    workingDir = directorioRaizRepositorio
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-P", "30",
        "-o", directorioJniLibs.absolutePath,
        "build", "--release", "-p", "magi-movil",
    )
    prepararEntornoNdk()
}

val cdylibAnfitrion = File(directorioRaizRepositorio, "target/debug/libmagi_movil.so")

val generarBindingsNucleo = tasks.register<Exec>("generarBindingsNucleo") {
    group = "núcleo"
    description = "Genera los bindings Kotlin de UniFFI (paquete org.cuatrodetres.magi.nucleo)."
    dependsOn(compilarNucleoAnfitrion)
    workingDir = directorioRaizRepositorio
    commandLine(
        "cargo", "run", "--quiet",
        "--manifest-path", File(directorioCrateMovil, "Cargo.toml").path,
        "--features", "bindgen",
        "--bin", "uniffi-bindgen",
        "--", "generate",
        "--library", cdylibAnfitrion.path,
        "--language", "kotlin",
        "--out-dir", directorioBindings.absolutePath,
        "--no-format",
    )
    onlyIf {
        // Sin crate, el esqueleto se configura y pasa lint; cuando exista, esta
        // tarea genera los bindings de UniFFI en cada compilación.
        val hayCrate = File(directorioCrateMovil, "Cargo.toml").isFile
        when {
            !hayCrate -> {
                logger.warn(
                    "MAGI: todavía no existe crates/magi-movil; no hay bindings que generar."
                )
                false
            }
            !cdylibAnfitrion.isFile -> {
                logger.warn(
                    "MAGI: falta la cdylib del anfitrión (${cdylibAnfitrion.path}); " +
                        "no hay bindings que generar."
                )
                false
            }
            else -> true
        }
    }
    doFirst {
        directorioBindings.deleteRecursively()
        directorioBindings.mkdirs()
    }
    doLast {
        // UniFFI genera una comprobación explícita de java.lang.ref.Cleaner
        // (API 33) con Class.forName y vuelve al cleaner de JNA en Android 30-32.
        // Lint no ve ese guard, así que el fichero generado se marca con
        // @file:Suppress("NewApi"); el resto de la app conserva el chequeo.
        val patronSuppress = Regex("@file:Suppress\\(([^)]*)\\)")
        val patronCleaner = Regex(
            "(?m)^(private class (?:JavaLangRefCleaner|JavaLangRefCleanable)\\b)"
        )
        directorioBindings.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .forEach { archivo ->
                var contenido = archivo.readText()
                if (!contenido.contains("\"NewApi\"")) {
                    contenido = if (patronSuppress.containsMatchIn(contenido)) {
                        // UniFFI ya emite @file:Suppress("NAME_SHADOWING"); se
                        // amplía en vez de duplicar (la anotación no es repetible).
                        patronSuppress.replaceFirst(
                            contenido,
                            "@file:Suppress(\$1, \"NewApi\") // UniFFI comprueba Cleaner con Class.forName",
                        )
                    } else {
                        "@file:Suppress(\"NewApi\")\n" + contenido
                    }
                }
                contenido = patronCleaner.replace(contenido) { coincidencia ->
                    "@Suppress(\"NewApi\")\n" + coincidencia.groupValues[1]
                }
                archivo.writeText(contenido)
            }
    }
}


// ===========================================================================
// 2.2. Licencias de terceros generadas en la compilación
//
// Checklist Fase 11: la pantalla «Acerca de · Licencias» lee R.raw.licencias,
// generado en cada compilación con las dependencias Kotlin/AndroidX, los crates
// de Rust del workspace y la OFL de JetBrains Mono. El fichero se escribe en
// build/ (no en src/) para no versionar contenido derivado.
// ===========================================================================

val directorioLicencias: File = layout.buildDirectory.dir("generated/licencias/res").get().asFile

val generarLicencias = tasks.register("generarLicencias") {
    group = "recursos"
    description = "Genera res/raw/licencias.txt con las licencias de terceros."
    doLast {
        val destino = File(directorioLicencias, "raw")
        destino.mkdirs()
        val separador = "=".repeat(72)
        val texto = StringBuilder()

        texto.appendLine("MAGI · app Android ${versionApp}")
        texto.appendLine("Licencias de terceros")
        texto.appendLine()
        texto.appendLine("MAGI se distribuye con licencia dual MIT OR Apache-2.0.")
        texto.appendLine()

        texto.appendLine(separador)
        texto.appendLine("JetBrains Mono (SIL Open Font License 1.1)")
        texto.appendLine(separador)
        val ofl = file("src/main/res/raw/licencia_jetbrains_mono.txt")
        texto.appendLine(if (ofl.isFile) ofl.readText().trim() else "(no se encontró el texto OFL)")
        texto.appendLine()

        texto.appendLine(separador)
        texto.appendLine("Dependencias Kotlin/AndroidX (classpath de ejecución de debug)")
        texto.appendLine(separador)
        val dependencias = configurations.getByName("debugRuntimeClasspath")
            .incoming.resolutionResult.allComponents
            .mapNotNull { componente ->
                if (componente.id is org.gradle.api.artifacts.component.ProjectComponentIdentifier) {
                    null
                } else {
                    componente.moduleVersion?.let { "${it.group}:${it.name}:${it.version}" }
                }
            }
            .distinct()
            .sorted()
        dependencias.forEach { texto.appendLine(it) }
        texto.appendLine()

        texto.appendLine(separador)
        texto.appendLine("Crates de Rust (Cargo.lock del workspace)")
        texto.appendLine(separador)
        val bloqueo = File(directorioRaizRepositorio, "Cargo.lock")
        if (bloqueo.isFile) {
            Regex("""\[\[package\]\]\s*\nname = "([^"]+)"\s*\nversion = "([^"]+)"""")
                .findAll(bloqueo.readText())
                .map { "${it.groupValues[1]} ${it.groupValues[2]}" }
                .distinct()
                .sorted()
                .forEach { texto.appendLine(it) }
        } else {
            texto.appendLine("(no se encontró Cargo.lock)")
        }

        File(destino, "licencias.txt").writeText(texto.toString())
    }
}

// ===========================================================================
// 3. Configuración Android
// ===========================================================================

// Firma de release por variables de entorno. Solo android-release.yml las recibe
// (secretos del desarrollador); en local, sin ellas, el APK sale sin firmar.
val rutaAlmacen = providers.environmentVariable("MAGI_KEYSTORE_PATH")
val contrasenaAlmacen = providers.environmentVariable("MAGI_KEYSTORE_PASSWORD")
val aliasClave = providers.environmentVariable("MAGI_KEY_ALIAS")
val contrasenaClave = providers.environmentVariable("MAGI_KEY_PASSWORD")
val hayFirmaCi = listOf(rutaAlmacen, contrasenaAlmacen, aliasClave, contrasenaClave)
    .all { it.isPresent }

android {
    namespace = "org.cuatrodetres.magi"
    compileSdk = 37
    ndkVersion = ndkFijado

    defaultConfig {
        applicationId = "org.cuatrodetres.magi"
        minSdk = 30
        targetSdk = 37
        versionCode = codigoVersion
        versionName = versionApp
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables { useSupportLibrary = true }
    }

    signingConfigs {
        if (hayFirmaCi) {
            create("release") {
                storeFile = file(rutaAlmacen.get())
                storePassword = contrasenaAlmacen.get()
                keyAlias = aliasClave.get()
                keyPassword = contrasenaClave.get()
                // Solo esquemas modernos: v2 y v3 (minSdk 30 no necesita v1/JAR).
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        debug {
            // El emulador de la Fase 11 es x86_64 (API 30); arm64 permite depurar
            // en un móvil real.
            ndk { abiFilters.addAll(listOf("arm64-v8a", "x86_64")) }
        }
        release {
            isMinifyEnabled = false
            isShrinkResources = false
            if (hayFirmaCi) {
                signingConfig = signingConfigs.findByName("release")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // La release publicada solo lleva arm64-v8a.
            ndk { abiFilters.addAll(listOf("arm64-v8a")) }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    lint {
        abortOnError = true
        checkDependencies = true
    }

    testOptions {
        // AGP 9.5 registra tareas de previsualización de Compose que exigen los
        // recursos de Android; además, las pruebas de la JVM podrán leer recursos.
        unitTests.isIncludeAndroidResources = true
        unitTests.all {
            // Los bindings de UniFFI cargan libmagi_movil.so del anfitrión con JNA.
            it.systemProperty(
                "jna.library.path",
                File(directorioRaizRepositorio, "target/debug").absolutePath,
            )
        }
    }

    sourceSets {
        getByName("main") {
            // Bindings Kotlin generados por UniFFI en cada compilación.
            kotlin.directories.add(directorioBindings.absolutePath)
            // Licencias de terceros generadas en la compilación (R.raw.licencias).
            res.directories.add(directorioLicencias.absolutePath)
        }
    }
}

// ---------------------------------------------------------------------------
// 3.1. Enganches de las tareas del núcleo al ciclo de vida de Gradle
// ---------------------------------------------------------------------------

tasks.matching { it.name.startsWith("compile") && it.name.endsWith("Kotlin") }.configureEach {
    dependsOn(generarBindingsNucleo)
}

// La generación de licencias no declara salidas propias para no chocar con la
// validación de dependencias implícitas de Gradle 9 (varias tareas de AGP leen el
// directorio de recursos generado). Se ejecuta siempre en preBuild: es barata y
// deja R.raw.licencias antes de cualquier tarea de recursos.
tasks.named("preBuild") {
    dependsOn(generarLicencias)
}

tasks.matching { it.name == "mergeDebugJniLibFolders" }.configureEach {
    dependsOn(compilarNucleoDebug)
}

tasks.matching { it.name == "mergeReleaseJniLibFolders" }.configureEach {
    dependsOn(compilarNucleoRelease)
}

// Las pruebas de la JVM usan el núcleo compilado para el anfitrión.
tasks.matching { it.name == "testDebugUnitTest" }.configureEach {
    dependsOn(compilarNucleoAnfitrion)
}

// ===========================================================================
// 4. Dependencias
// ===========================================================================

dependencies {
    val bomCompose = platform(libs.androidx.compose.bom)
    implementation(bomCompose)
    androidTestImplementation(bomCompose)

    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material3)
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    // ProcessLifecycleOwner: sondeo, refrescos y logs solo a la vista (F14 §8).
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.datastore.preferences)
    implementation(libs.androidx.biometric)
    // androidx.biometric 1.1.0 arrastra fragment 1.2.5; ActivityResult necesita
    // fragment >= 1.3.0 (lint InvalidFragmentVersionForActivityResult).
    implementation(libs.androidx.fragment)

    // QR: CameraX + ZXing core (sin ML Kit ni Google Play Services).
    implementation(libs.androidx.camera.core)
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
    implementation(libs.zxing.core)

    // JNA en formato aar: el código Kotlin generado por UniFFI carga la cdylib.
    val jna = libs.jna.get()
    implementation("${jna.module}:${jna.versionConstraint.requiredVersion}@aar")

    implementation(libs.kotlinx.coroutines.android)

    testImplementation(libs.junit)

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
}