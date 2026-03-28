plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.aboutlibraries.android)
}

fun String.escapeHtml(): String =
    replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")

val monorepoRoot = layout.projectDirectory.dir("../..")
val generatedAssetsDir = layout.buildDirectory.dir("generated/assets")
val generatedJniLibsDir = layout.buildDirectory.dir("generated/jniLibs")
val licenseAssetsDir = generatedAssetsDir.map { it.dir("licenses") }
val rustThirdPartyMarkdown = monorepoRoot.file("THIRD_PARTY_LICENSES.md")
val generatedRustThirdPartyHtml = layout.buildDirectory.file("generated/licenses/vibeEmu_THIRD_PARTY_LICENSES.html")
val cargoTargetDir = layout.buildDirectory.dir("cargo")
val cargoAbis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")
val cargoPath = System.getenv("PATH")
val cargoBin = System.getenv("USERPROFILE")?.let { "$it\\.cargo\\bin" }
val defaultSdkRoot = File(System.getProperty("user.home"), "AppData/Local/Android/Sdk").invariantSeparatorsPath
val sdkRoot = System.getenv("ANDROID_SDK_ROOT") ?: System.getenv("ANDROID_HOME") ?: defaultSdkRoot
val ndkHome = "$sdkRoot/ndk/29.0.14206865"

android {
    namespace = "com.example.vibeemua"
    compileSdk = 36

    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "com.example.vibeemua"
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets["main"].jniLibs.srcDirs(generatedJniLibsDir)

    // Generate license assets into build/ to avoid dirtying the repo, but still package them.
    sourceSets["main"].assets.srcDirs("src/main/assets", generatedAssetsDir)
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11)
    }
}

tasks.register("generateRustThirdPartyLicensesHtml") {
    group = "licensing"
    description = "Render the monorepo Rust license summary into an asset-backed HTML page"

    inputs.file(rustThirdPartyMarkdown)
    outputs.file(generatedRustThirdPartyHtml)

    doLast {
        val markdown = rustThirdPartyMarkdown.asFile.readText()
        val html = """
            <!doctype html>
            <html lang="en">
            <head>
              <meta charset="utf-8">
              <meta name="viewport" content="width=device-width, initial-scale=1">
              <title>vibeEmu Third-Party Licenses</title>
              <style>
                body {
                  font-family: sans-serif;
                  margin: 0;
                  padding: 16px;
                  background: #fafafa;
                  color: #111;
                }
                pre {
                  white-space: pre-wrap;
                  word-break: break-word;
                  font-family: ui-monospace, monospace;
                }
              </style>
            </head>
            <body>
              <pre>${markdown.escapeHtml()}</pre>
            </body>
            </html>
        """.trimIndent()

        generatedRustThirdPartyHtml.get().asFile.apply {
            parentFile.mkdirs()
            writeText(html)
        }
    }
}

tasks.register<Copy>("syncLicensesToAssets") {
    dependsOn("generateRustThirdPartyLicensesHtml")
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE

    from(monorepoRoot.file("LICENSE")) { rename { "LICENSE.txt" } }
    from(monorepoRoot.file("THIRD_PARTY_LICENSES.md"))
    from(generatedRustThirdPartyHtml) { rename { "vibeEmu_THIRD_PARTY_LICENSES.html" } }
    from(monorepoRoot.file("crates/vibe-emu-mobile/vendor/libmobile-0.2.2/COPYING.LESSER")) {
        rename { "LGPL-3.0-or-later.txt" }
    }
    from(monorepoRoot.file("crates/vibe-emu-mobile/vendor/libmobile-0.2.2/COPYING")) {
        rename { "GPL-3.0.txt" }
    }

    into(licenseAssetsDir)
}

dependencies {

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation("androidx.compose.foundation:foundation")
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    implementation(libs.aboutlibraries.compose.core)
    implementation(libs.aboutlibraries.compose.m3)
    implementation(libs.aboutlibraries.core)
    implementation("androidx.compose.material:material-icons-core")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    debugImplementation(libs.androidx.ui.tooling)
}

tasks.register<Exec>("cargoBuildAndroid") {
    group = "build"
    description = "Build the monorepo Android JNI crate via cargo-ndk"
    workingDir = monorepoRoot.asFile

    inputs.file(monorepoRoot.file("Cargo.toml"))
    inputs.file(monorepoRoot.file("Cargo.lock"))
    inputs.file(monorepoRoot.file("crates/vibe-emu-android/Cargo.toml"))
    inputs.file(monorepoRoot.file("crates/vibe-emu-core/Cargo.toml"))
    inputs.file(monorepoRoot.file("crates/vibe-emu-mobile/Cargo.toml"))
    inputs.file(monorepoRoot.file("crates/vibe-emu-mobile-sys/Cargo.toml"))
    inputs.file(monorepoRoot.file("crates/vibe-emu-mobile-sys/build.rs"))
    inputs.dir(monorepoRoot.dir("crates/vibe-emu-android/src"))
    inputs.dir(monorepoRoot.dir("crates/vibe-emu-core/src"))
    inputs.dir(monorepoRoot.dir("crates/vibe-emu-mobile/src"))
    inputs.dir(monorepoRoot.dir("crates/vibe-emu-mobile-sys/src"))
    inputs.dir(monorepoRoot.dir("crates/vibe-emu-mobile/vendor/libmobile-0.2.2"))
    outputs.dir(generatedJniLibsDir)

    environment("CARGO_TARGET_DIR", cargoTargetDir.get().asFile.absolutePath)
    environment("ANDROID_NDK_HOME", ndkHome)
    environment("NDK_HOME", ndkHome)
    if (cargoBin != null && cargoPath != null) {
        environment("PATH", "$cargoBin;$cargoPath")
    }
    commandLine(
        "cargo",
        "ndk",
        "-t",
        cargoAbis.joinToString(","),
        "-o",
        generatedJniLibsDir.get().asFile.absolutePath,
        "build",
        "-p",
        "vibe-emu-android",
        "--release",
    )
}

tasks.named("preBuild") {
    dependsOn("cargoBuildAndroid")
    dependsOn("syncLicensesToAssets")
}