import javax.inject.Inject

// The Rust core for Android: todav-ffi built with cargo-ndk for the ABIs in `todav.abis`, its
// Kotlin bindings (package dev.aperte.todav.core) and the translation catalogs (assets
// locale/<language>/todav.mo, compiled from po/ by msgfmt).

plugins {
    alias(libs.plugins.android.library)
}

// SDK versions from gradle.properties, which sdk.sh installs.
val sdkVersion = providers.gradleProperty("todav.platform").get()
    .removePrefix("android-").substringBefore('.').toInt()
val buildTools = providers.gradleProperty("todav.buildTools").get()
val minimumSdk = providers.gradleProperty("todav.minSdk").get().toInt()
val ndkRelease = providers.gradleProperty("todav.ndk").get()
val repo = rootDir.resolve("../..").canonicalFile
val ffi = repo.resolve("todav/ffi")
val targetAbis = providers.gradleProperty("todav.abis").get().split(",").map { it.trim() }

android {
    namespace = "dev.aperte.todav.core"
    compileSdk = sdkVersion
    buildToolsVersion = buildTools
    // The NDK cargo-ndk builds with.
    ndkVersion = ndkRelease
    defaultConfig {
        minSdk = minimumSdk
        consumerProguardFiles("consumer-rules.pro")
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    // UniFFI's bindings call the library through JNA (its Android build ships libjnidispatch).
    api(libs.jna) { artifact { type = "aar" } }
}

/** todav-ffi built for the Android ABIs, as `<abi>/libtodav_ffi.so` in `out`. */
abstract class CargoNdk @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:InputFiles
    abstract val sources: ConfigurableFileCollection

    @get:Input
    abstract val abis: ListProperty<String>

    @get:Input
    abstract val platform: Property<Int>

    @get:Internal
    abstract val crate: DirectoryProperty

    @get:Internal
    abstract val ndk: DirectoryProperty

    @get:OutputDirectory
    abstract val out: DirectoryProperty

    @TaskAction
    fun build() {
        exec.exec {
            workingDir(crate)
            environment("ANDROID_NDK_HOME", ndk.get().asFile.path)
            commandLine(
                listOf("cargo", "ndk", "--platform", "${platform.get()}") +
                    abis.get().flatMap { listOf("-t", it) } +
                    listOf("-o", out.get().asFile.path, "build", "--release", "--locked"),
            )
        }
    }
}

/** The Kotlin bindings of `library`, in `out`. */
abstract class Bindings @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:InputFile
    abstract val library: RegularFileProperty

    @get:Internal
    abstract val crate: DirectoryProperty

    @get:OutputDirectory
    abstract val out: DirectoryProperty

    @TaskAction
    fun generate() {
        val out = out.get().asFile
        out.deleteRecursively()
        exec.exec {
            workingDir(crate)
            commandLine(
                "cargo", "run", "--locked", "-q", "--features", "bindgen", "--bin",
                "uniffi-bindgen", "--", "generate", "--library", library.get().asFile.path,
                "--language", "kotlin", "--out-dir", out.path, "--no-format",
            )
        }
    }
}

/** msgfmt over each language's catalog: `locale/<language>/todav.mo` in `out`. */
abstract class Catalogs @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:InputFiles
    abstract val sources: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val out: DirectoryProperty

    @TaskAction
    fun compile() {
        val out = out.get().asFile
        out.deleteRecursively()
        for (po in sources) {
            val mo = out.resolve("locale/${po.nameWithoutExtension}/todav.mo")
            mo.parentFile.mkdirs()
            exec.exec { commandLine("msgfmt", "--check", "-o", mo.path, po.path) }
        }
    }
}

val cargoNdk = tasks.register<CargoNdk>("cargoNdk") {
    description = "Builds todav-ffi for the Android ABIs."
    sources.from(
        fileTree(repo.resolve("todav")) {
            include("core/src/**", "core/Cargo.toml", "app/src/**", "app/Cargo.toml")
            include("ffi/src/**", "ffi/Cargo.toml", "ffi/Cargo.lock", "ffi/uniffi.toml")
        },
    )
    abis = targetAbis
    platform = minimumSdk
    crate = ffi
    ndk = androidComponents.sdkComponents.ndkDirectory
    out = layout.buildDirectory.dir("rust/jniLibs")
}

val bindings = tasks.register<Bindings>("uniffiBindings") {
    description = "Generates the Kotlin bindings of todav-ffi."
    library = cargoNdk.flatMap { it.out.file("${targetAbis.first()}/libtodav_ffi.so") }
    crate = ffi
    out = layout.buildDirectory.dir("generated/uniffi")
}

val catalogs = tasks.register<Catalogs>("catalogs") {
    description = "Compiles the translation catalogs."
    val languages = repo.resolve("po/LINGUAS").readLines().map { it.trim() }
        .filter { it.isNotEmpty() && !it.startsWith("#") }
    sources.from(languages.map { repo.resolve("po/$it.po") })
    out = layout.buildDirectory.dir("generated/catalogs")
}

androidComponents.onVariants { variant ->
    variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdk::out)
    variant.sources.kotlin?.addGeneratedSourceDirectory(bindings, Bindings::out)
    variant.sources.assets?.addGeneratedSourceDirectory(catalogs, Catalogs::out)
}
