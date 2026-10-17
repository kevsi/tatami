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
    implementation("com.github.Kotatsu-Redo:kotatsu-parsers-redo:b4df40e9")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core-jvm:1.10.2")
    // Versions alignées sur gradle/libs.versions.toml du fork (okhttp 5.x, pas 4.x comme kotatsu-dl).
    implementation("com.squareup.okhttp3:okhttp:5.1.0")
    implementation("com.squareup.okio:okio:3.16.0")
    implementation("org.json:json:20240303")
    // Moteur JS : le JDK 21 n'en fournit plus (voir EngineContext.evaluateJs).
    implementation("org.openjdk.nashorn:nashorn-core:15.6")

    // **Pile TLS de Chrome.** Kotatsu tourne sur Android, dont le provider TLS
    // par defaut EST Conscrypt ; notre moteur tourne sur un JVM de bureau, ou
    // le defaut est JSSE. Mesure du 2026-10-11 (sonde `ConscryptProbe.java`) :
    // JSSE = JA3 `0a80f686…` sans GREASE, Conscrypt = `4944639f…` avec
    // `156-157-47-53`, comme Edge.
    //
    // Version 2.5.2 : celle de `Kotatsu-Redo/gradle/libs.versions.toml:13`.
    // `openjdk-uber` et non `android` — c'est la variante qui embarque le code
    // natif BoringSSL pour une **JVM de bureau** ; `conscrypt-android` suppose
    // l'environnement Android.
    //
    // Attention : la porte `gradle/verification-metadata.xml` refuse une
    // dependance non declaree. Apres cette ligne, regenerer le fichier avec
    // `--write-verification-metadata sha256 --refresh-dependencies`, sinon la
    // CI echoue sur un cache froid alors que le poste passe (lecon AGENTS.md
    // §14, 2026-10-05).
    implementation("org.conscrypt:conscrypt-openjdk-uber:2.5.2")

    // Test uniquement : `kotlin.test` sansJUnit. Le moteur n'avait aucun test
    // (0 fichier dans build/test-results), donc la porte `gradle test` passait
    // sans rien vérifier du tout.
    testImplementation(kotlin("test"))
    testImplementation("com.squareup.okhttp3:mockwebserver:5.1.0")
}

kotlin {
    jvmToolchain(21)
}

tasks.test {
    useJUnitPlatform()
    // **Chaque test dans son propre process.**
    //
    // Mesuré le 2026-10-05 : `FermetureStdinTest` lance un vrai moteur JVM et
    // compte 0 réponse sur 5 dès qu'un autre test réseau (`ZENMANGA`,
    // `ParserPoolPaginationTest`) tourne dans le même JVM. Le moteur enfant
    // hérite du classpath complet du test, donc charge les 25 classes de test,
    // démarre plus lentement, et la fermeture immédiate de stdin le coupe avant
    // qu'il réponde.
    //
    // Le symptôme est trompeur : le test qui échoue n'a rien à voir avec le
    // changement qui l'a déclenché. D'où `forkEvery = 1` — un JVM par test, ce
    // que ferait de toute façon une CI.
    forkEvery = 1
    // `FermetureStdinTest` lance le moteur réel : il lui faut le classpath de
    // `installDist`. On le construit ici plutôt que d'exiger un
    // `gradle installDist` en amont, sinon un simple `gradle test` échoue sur une
    // porte qui n'a rien à voir avec le code modifié.
    dependsOn(tasks.named("installDist"))
    testLogging {
        events("passed", "failed", "skipped")
        showStandardStreams = false
    }
}

application {
    mainClass.set("org.tatami.engine.MainKt")
}
