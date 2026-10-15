import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.app)
}

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
        versionName = "0.1.0"
        versionCode = 1
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
}

base.archivesName.set("cratarr-android")

dependencies {
    implementation(libs.coroutines.android)
    implementation(libs.androidx.core)
    implementation(libs.androidx.appcompat)
    implementation(libs.androidx.activity)
    implementation(libs.androidx.lifecycle.runtime)
    implementation(libs.androidx.webkit)
    implementation(libs.androidx.preference)
}
