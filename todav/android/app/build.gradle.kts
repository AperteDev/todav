plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose)
}

// SDK versions from gradle.properties, which sdk.sh installs.
val sdkVersion = providers.gradleProperty("todav.platform").get()
    .removePrefix("android-").substringBefore('.').toInt()
val buildTools = providers.gradleProperty("todav.buildTools").get()
val minimumSdk = providers.gradleProperty("todav.minSdk").get().toInt()

// The workspace's version (scripts/release.py sets it in the root Cargo.toml): 1.2.3 is 10203.
val todavVersion = rootDir.resolve("../../Cargo.toml").readText()
    .let { Regex("""\[workspace\.package][^\[]*?\nversion = "([^"]+)"""").find(it)!!.groupValues[1] }
val todavCode = todavVersion.split(".").map { it.toInt() }.let { (a, b, c) -> a * 10000 + b * 100 + c }

android {
    namespace = "dev.aperte.todav"
    compileSdk = sdkVersion
    buildToolsVersion = buildTools
    defaultConfig {
        applicationId = "dev.aperte.todav"
        minSdk = minimumSdk
        targetSdk = sdkVersion
        versionCode = todavCode
        versionName = todavVersion
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }
    buildFeatures {
        compose = true
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    implementation(project(":core"))
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.core.ktx)
    implementation(libs.browser)
    implementation(libs.coroutines.android)
    implementation(libs.navigation3.runtime)
    implementation(libs.navigation3.ui)
    implementation(libs.work.runtime)
    implementation(libs.glance.appwidget)
    implementation(libs.glance.material3)
}
