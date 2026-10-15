plugins {
    alias(libs.plugins.android.app) apply false
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
