import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.net.URI
import java.security.MessageDigest
import java.util.zip.ZipFile

plugins {
    alias(libs.plugins.android.app)
}

/** VERSION at the repository's root (the audit, 2026-10-29: one version for everything). */
val cratarrVersion: String = rootProject.projectDir.resolve("../VERSION").readText().trim()

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_11
    }
}

android {
    namespace = "app.cratarr"
    compileSdk = libs.versions.android.compileSdk.get().toInt()

    defaultConfig {
        applicationId = "app.cratarr"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.targetSdk.get().toInt()
        // The repository's one version (VERSION, scripts/version.sh); its code worked out from it, so each
        // release is newer than the one before without a second number to keep.
        versionName = cratarrVersion
        versionCode = cratarrVersion.split('.').map(String::toInt).let { (major, minor, patch) -> major * 10000 + minor * 100 + patch }
    }

    signingConfigs {
        // A release build is signed only when all four are given (Gradle properties or environment variables).
        fun setting(name: String) = (findProperty(name) ?: System.getenv(name.uppercase().replace('.', '_')))?.toString()
        val keystoreFile = setting("keystore.file")
        val keystorePassword = setting("keystore.password")
        val signingKeyAlias = setting("signing.key.alias")
        val signingKeyPassword = setting("signing.key.password")

        if (keystoreFile != null && keystorePassword != null && signingKeyAlias != null && signingKeyPassword != null) {
            create("release") {
                storeFile = file(keystoreFile)
                storePassword = keystorePassword
                keyAlias = signingKeyAlias
                keyPassword = signingKeyPassword
            }
        }
    }

    dependenciesInfo {
        includeInBundle = false
        includeInApk = false
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }

        getByName("debug") {
            applicationIdSuffix = ".debug"
            isDebuggable = true
        }
    }

    buildFeatures {
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    // Snapcast's player is an executable that Android runs from the app's library folder: kept unpacked there.
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

/**
 * Snapcast's player (snapclient, GPL-3.0) for sync play (decision 0037), as Snapdroid ships it for Android: taken from
 * Snapdroid's release at build time, checked against its pinned checksum, so no binary sits in this repository. Their
 * source: github.com/snapcast/snapcast (v0.29.0) and github.com/snapcast/snapdroid (v0.29.0.2).
 */
abstract class FetchSnapclient : DefaultTask() {
    @get:Input abstract val url: Property<String>
    @get:Input abstract val sha256: Property<String>
    @get:Internal abstract val apk: RegularFileProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun fetch() {
        val file = apk.get().asFile
        fun sum() = MessageDigest.getInstance("SHA-256").digest(file.readBytes()).joinToString("") { "%02x".format(it) }
        if (!file.exists() || sum() != sha256.get()) {
            file.parentFile.mkdirs()
            URI(url.get()).toURL().openStream().use { input -> file.outputStream().use { input.copyTo(it) } }
            check(sum() == sha256.get()) { "${url.get()} is not the release pinned here" }
        }
        val out = outputDir.get().asFile
        ZipFile(file).use { zip ->
            for (abi in listOf("arm64-v8a", "armeabi-v7a", "x86_64")) {
                val entry = zip.getEntry("lib/$abi/libsnapclient.so") ?: error("no snapclient for $abi")
                val to = out.resolve("$abi/libsnapclient.so")
                to.parentFile.mkdirs()
                zip.getInputStream(entry).use { input -> to.outputStream().use { input.copyTo(it) } }
            }
        }
    }
}

val fetchSnapclient = tasks.register<FetchSnapclient>("fetchSnapclient") {
    url.set("https://github.com/snapcast/snapdroid/releases/download/v0.29.0.2/Snapcast-signed-v0.29.0.2.apk")
    sha256.set("cd835c12ae3bf084ef06064333599040559a871fee48e83f3539e89224b40010")
    apk.set(layout.buildDirectory.file("snapdroid/Snapcast-signed-v0.29.0.2.apk"))
    outputDir.set(layout.buildDirectory.dir("snapclient"))
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(fetchSnapclient, FetchSnapclient::outputDir)
    }
}

base.archivesName.set("cratarr-android")

dependencies {
    implementation(libs.coroutines.android)
    implementation(libs.androidx.core)
    implementation(libs.androidx.appcompat)
    implementation(libs.androidx.activity)
    implementation(libs.androidx.lifecycle.runtime)
    implementation(libs.androidx.webkit)
    implementation(libs.androidx.media3.exoplayer)
    implementation(libs.androidx.media3.session)
    implementation(libs.androidx.work.runtime)
    testImplementation(libs.junit)
    testImplementation(libs.org.json)
}
