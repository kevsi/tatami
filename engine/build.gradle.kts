plugins {
    kotlin("jvm") version "2.2.10"
    application
}

group = "org.tatami"
version = "0.1.0-SNAPSHOT"

repositories {
    mavenCentral()
    google() // androidx.collection (transitif des parsers) n'est que là
    maven { url = uri("https://jitpack.io") }
}

dependencies {
    // Fork redo, épinglé sur un commit (pas de tags/releases). Voir docs/notes/loader-context.md.
    implementation("com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core-jvm:1.10.2")
    // Versions alignées sur gradle/libs.versions.toml du fork (okhttp 5.x, pas 4.x comme kotatsu-dl).
    implementation("com.squareup.okhttp3:okhttp:5.1.0")
    implementation("com.squareup.okio:okio:3.16.0")
    implementation("org.json:json:20240303")
    // Moteur JS : le JDK 21 n'en fournit plus (voir EngineContext.evaluateJs).
    implementation("org.openjdk.nashorn:nashorn-core:15.6")
}

kotlin {
    jvmToolchain(21)
}

application {
    mainClass.set("org.tatami.engine.MainKt")
}
