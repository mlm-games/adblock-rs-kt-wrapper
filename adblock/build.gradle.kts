plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.vanniktech.mavenPublish)
}

android {
    namespace = "org.mlm.adblock"
    compileSdk = libs.versions.compileSdk.get().toInt()

    defaultConfig {
        minSdk = libs.versions.minSdk.get().toInt()
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21)
    }
}

mavenPublishing {
    publishToMavenCentral()
    signAllPublications()
}

// Rebuild the Rust .so when the prebuilt jniLibs are missing or -PrebuildRust=true is passed.
// The custom task class (ExecOperations) keeps this configuration-cache safe,
// mirroring the cargo integration in the Mages project.
val sdkDir: String? = (project.findProperty("sdk.dir") as String?)
    ?: (rootProject.findProperty("sdk.dir") as String?)
val ndkDir: String? = sdkDir?.let { d ->
    val ndkRoot = file("$d/ndk")
    if (ndkRoot.exists()) {
        ndkRoot.listFiles()?.map { it.name }?.sorted()?.lastOrNull()?.let { "$d/ndk/$it" }
    } else {
        null
    }
}

val hasPrebuiltLibs = file("src/main/jniLibs").let { it.exists() && !it.listFiles().isNullOrEmpty() }
val rebuildRequested = providers.gradleProperty("rebuildRust").orNull == "true"

val rustProjectDir = layout.projectDirectory.dir("src/main/rust")
val jniOutputDir = layout.projectDirectory.dir("src/main/jniLibs")

val buildRust = tasks.register<BuildRustTask>("buildRust") {
    abis.set(listOf("armeabi-v7a", "arm64-v8a", "x86", "x86_64"))
    rustDir.set(rustProjectDir)
    jniOut.set(jniOutputDir)
    ndkHome.set(ndkDir ?: "")
    libsPresent.set(hasPrebuiltLibs)
    rebuild.set(rebuildRequested)
}

tasks.named("preBuild").configure {
    dependsOn(buildRust)
}

@DisableCachingByDefault(because = "Builds native code")
abstract class BuildRustTask @Inject constructor(private val execOps: ExecOperations) :
    DefaultTask() {
    @get:Input abstract val abis: ListProperty<String>
    @get:InputDirectory abstract val rustDir: DirectoryProperty
    @get:OutputDirectory abstract val jniOut: DirectoryProperty
    @get:Input abstract val ndkHome: Property<String>
    @get:Input abstract val libsPresent: Property<Boolean>
    @get:Input abstract val rebuild: Property<Boolean>

    @TaskAction
    fun run() {
        if (libsPresent.get() && !rebuild.get()) {
            logger.info("Using prebuilt adblock_ffi.so (run with -PrebuildRust=true to rebuild)")
            return
        }
        execOps.exec {
            workingDir = rustDir.get().asFile
            if (ndkHome.isPresent) {
                environment("ANDROID_NDK_HOME", ndkHome.get())
            }
            val args = mutableListOf("cargo", "ndk")
            abis.get().forEach { args += listOf("-t", it) }
            args += listOf("-o", jniOut.get().asFile.absolutePath, "build", "--release")
            commandLine(args)
        }
    }
}
