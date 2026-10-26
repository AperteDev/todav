// The Kotlin bindings against the library built for this machine (`cargo build` in todav/ffi,
// then the bindings generated into target/bindings/kotlin): `gradle run`. A check that they
// compile and work, not the Android build.
plugins {
    kotlin("jvm") version "2.2.21"
    application
}

repositories { mavenCentral() }

dependencies { implementation("net.java.dev.jna:jna:5.17.0") }

val ffi = projectDir.resolve("../..").normalize()

kotlin {
    jvmToolchain(17)
    sourceSets["main"].kotlin.srcDirs("src", ffi.resolve("target/bindings/kotlin"))
}

application { mainClass.set("SmokeKt") }

tasks.named<JavaExec>("run") {
    systemProperty("jna.library.path", ffi.resolve("target/debug").path)
    args(layout.buildDirectory.dir("data").get().asFile.path)
}
