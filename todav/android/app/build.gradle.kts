plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose)
}

// SDK versions from gradle.properties, which sdk.sh installs.
val sdkVersion = providers.gradleProperty("todav.platform").get()
    .removePrefix("android-").substringBefore('.').toInt()
val buildTools = providers.gradleProperty("todav.buildTools").get()
val minimumSdk = providers.gradleProperty("todav.minSdk").get().toInt()

// The workspace's version (scripts/release.py sets it in the root Cargo.toml) and its code
// (1.2.3 is 10203), which release.py writes to gradle.properties for F-Droid's update check.
val todavVersion = rootDir.resolve("../../Cargo.toml").readText()
    .let { Regex("""\[workspace\.package][^\[]*?\nversion = "([^"]+)"""").find(it)!!.groupValues[1] }
val todavCode = providers.gradleProperty("todav.versionCode").get().toInt()

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
            // The release APK is built before its commit and tag exist (the Release workflow
            // tags only what passed), so F-Droid's rebuild of the tag would differ in the hash.
            vcsInfo.include = false
        }
    }
    // Unsigned release APKs, which CI signs with apksigner: F-Droid rebuilds them from the tag and
    // publishes ours when they match byte for byte. The dependency list AGP adds is encrypted for
    // Google, which F-Droid refuses; Play gets it in the bundle.
    dependenciesInfo {
        includeInApk = false
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
