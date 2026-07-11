import java.io.File

plugins {
    java
    id("me.champeau.jmh") version "0.7.3"
}

group = "com.example"
version = "1.0-SNAPSHOT"

val uniffiDir = "${projectDir}/../../adapters/uniffi/target/release"
val boltffiJvmDir = "${projectDir}/../../generated/boltffi/dist/java"
val boltffiJavaGenerator = providers.gradleProperty("boltffiJavaGenerator").orElse("legacy")
val boltffiJavaComparisonSuite = providers.gradleProperty("boltffiJavaComparisonSuite").orNull
val boltffiJavaPreparedDir = providers.gradleProperty("boltffiJavaPreparedDir")
    .orNull
    ?.let(::file)
val boltffiJavaComparisonBuildDir = providers.gradleProperty("boltffiJavaComparisonBuildDir")
    .orNull
    ?.let(::file)
if (boltffiJavaComparisonBuildDir != null) {
    layout.buildDirectory.set(boltffiJavaComparisonBuildDir)
}
val boltffiJavaSourceDir = boltffiJavaPreparedDir ?: file(boltffiJvmDir)
val nativePath = if (boltffiJavaComparisonSuite != null && boltffiJavaPreparedDir != null) {
    boltffiJavaSourceDir.absolutePath
} else {
    listOf(uniffiDir, boltffiJavaSourceDir.absolutePath).joinToString(File.pathSeparator)
}

if (boltffiJavaGenerator.get() !in setOf("legacy", "ir")) {
    throw GradleException("boltffiJavaGenerator must be 'legacy' or 'ir'")
}
if (boltffiJavaComparisonSuite != null && boltffiJavaComparisonSuite !in setOf("primitive", "record", "enum")) {
    throw GradleException("boltffiJavaComparisonSuite must be 'primitive', 'record', or 'enum'")
}
if (boltffiJavaPreparedDir != null) {
    val actualGenerator = boltffiJavaPreparedDir
        .resolve(".boltffi-java-generator")
        .takeIf(File::isFile)
        ?.readText()
        ?.trim()
    if (actualGenerator != boltffiJavaGenerator.get()) {
        throw GradleException(
            "expected prepared ${boltffiJavaGenerator.get()} Java sources, found '$actualGenerator'",
        )
    }
}

repositories {
    mavenCentral()
}

val buildUniffiJava by tasks.registering(Exec::class) {
    workingDir = projectDir
    commandLine("../../adapters/uniffi/build-java.sh")
}

val buildBoltffiJava by tasks.registering(Exec::class) {
    workingDir = projectDir
    commandLine(
        "../../generated/boltffi/build-java.sh",
        "--generator",
        boltffiJavaGenerator.get(),
    )
    outputs.upToDateWhen { false }
    doLast {
        val marker = file("$boltffiJvmDir/.boltffi-java-generator")
        val actualGenerator = marker.takeIf(File::isFile)?.readText()?.trim()
        if (actualGenerator != boltffiJavaGenerator.get()) {
            throw GradleException(
                "expected freshly generated ${boltffiJavaGenerator.get()} Java sources, found '$actualGenerator'",
            )
        }
    }
}

tasks.named("compileJava") {
    if (boltffiJavaPreparedDir == null && boltffiJavaComparisonSuite == null) {
        dependsOn(buildUniffiJava)
    }
    if (boltffiJavaPreparedDir == null) {
        dependsOn(buildBoltffiJava)
    }
}

tasks.matching { it.name.startsWith("jmh") }.configureEach {
    if (boltffiJavaPreparedDir == null && boltffiJavaComparisonSuite == null) {
        dependsOn(buildUniffiJava)
    }
    if (boltffiJavaPreparedDir == null) {
        dependsOn(buildBoltffiJava)
    }
}

val benchmarkJavaLauncher = javaToolchains.launcherFor {
    languageVersion = JavaLanguageVersion.of(25)
}
tasks.register("writeBenchmarkJavaLauncher") {
    val destination = layout.buildDirectory.file("java-launcher.txt")
    outputs.file(destination)
    doLast {
        destination.get().asFile.writeText(
            benchmarkJavaLauncher.get().executablePath.asFile.absolutePath + "\n",
        )
    }
}

tasks.named("jmh") {
    doFirst {
        file("${layout.buildDirectory.get()}/tmp/jmh/jmh.lock").delete()
    }
}

tasks.withType<JavaExec> {
    jvmArgs(
        "-Djava.library.path=$nativePath",
        "--enable-native-access=ALL-UNNAMED",
    )
}

jmh {
    jmhVersion = "1.37"
    fork = 1
    warmupIterations = 3
    iterations = 3
    warmup = "1s"
    timeOnIteration = "1s"
    resultFormat = "JSON"
    val include = providers.gradleProperty("jmhInclude").orNull
    if (include != null) {
        includes = listOf(include)
    }
    jvmArgsAppend = listOf(
        "-Djava.library.path=$nativePath",
        "--enable-native-access=ALL-UNNAMED",
    )
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
    sourceSets {
        named("main") {
            if (boltffiJavaComparisonSuite == null) {
                java.srcDir("${projectDir}/../../adapters/uniffi/dist/java")
            }
            java.srcDir(boltffiJavaSourceDir)
        }
        named("jmh") {
            when (boltffiJavaComparisonSuite) {
                "primitive" -> {
                    java.exclude("com/example/bench_compare/UniffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaRecordBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaEnumBench.java")
                }
                "record" -> {
                    java.exclude("com/example/bench_compare/UniffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaPrimitiveBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaEnumBench.java")
                }
                "enum" -> {
                    java.exclude("com/example/bench_compare/UniffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaPrimitiveBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaRecordBench.java")
                }
                else -> {
                    java.exclude("com/example/bench_compare/BoltffiJavaPrimitiveBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaRecordBench.java")
                    java.exclude("com/example/bench_compare/BoltffiJavaEnumBench.java")
                }
            }
        }
    }
}
