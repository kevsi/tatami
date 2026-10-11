package org.tatami.engine

import java.security.KeyStore
import java.security.Provider
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager

/**
 * Pile TLS du moteur : Conscrypt quand elle est disponible, JSSE sinon.
 *
 * **Pourquoi.** Sur Android, `OkHttp` ne voit que le provider TLS **par défaut de
 * la plateforme**, qui est Conscrypt — la bibliothèque de Google, celle de
 * Chrome. C'est vérifié dans le code de référence :
 * `Kotatsu-Redo/.../core/BaseApp.kt:88` fait
 * `Security.insertProviderAt(Conscrypt.newProvider(), 1)`, mais **uniquement**
 * sous `if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q)` : ce n'est pas un
 * choix, c'est un rattrapage pour les Android trop vieux. Sur un Android
 * récent, Conscrypt est déjà là sans rien écrire.
 *
 * Notre moteur, lui, tourne sur un **JVM de bureau** (`jlink`, Temurin 21), où le
 * provider par défaut est **JSSE**, la pile du JDK. Mesuré le 2026-10-11 :
 * JSSE donne un JA3 `0a80f686…` **sans valeur GREASE**, Conscrypt
 * `4944639f…` **avec** `156-157-47-53` — exactement ce que fait Edge, notre
 * référence navigateur.
 *
 * Donc : pour reproduire le comportement de Kotatsu sur un JVM de bureau, il
 * faut poser Conscrypt explicitement.
 *
 * **Comment.** `Security.insertProviderAt` ne suffirait pas et ne serait pas
 * vérifiable ici : JSSE choisit son propre provider, insérer le nôtre en
 * position 1 ne change pas l'`SSLContext` déjà construit. On construit donc
 * l'`SSLContext` **avec** le provider Conscrypt et on le pose sur le client
 * OkHttp. C'est aussi ce que fait la sonde du 2026-10-11 qui a produit les deux
 * JA3 ci-dessus, et qui est la seule source de ce fichier.
 *
 * **Le repli.** Toute erreur de construction (provider absent du classpath,
 * initialisation refusée) rend `null` : le client garde alors **JSSE**, donc le
 * comportement d'aujourd'hui. Un moteur qui démarre est mieux qu'un moteur qui
 * ne démarre pas.
 */
internal object PileTls {

    /**
     * Les deux morceaux qu'`OkHttpClient.Builder.sslSocketFactory` exige
     * **ensemble** : une fabrique et le gestionnaire de confiance qui va avec.
     *
     * Les garder dans une seule valeur évite le piège de reconstruire le
     * gestionnaire ailleurs et de retomber sur celui du JDK — qui, avec une
     * fabrique Conscrypt, vérifierait la chaine par le mauvais chemin.
     */
    data class Fabrique(
        val usine: SSLSocketFactory,
        val gestionnaire: X509TrustManager,
    )

    /** Construit la pile Conscrypt, ou `null` si elle est absente ou refusée. */
    fun construire(): Fabrique? = try {
        val provider: Provider = org.conscrypt.Conscrypt.newProvider()

        // Le TrustManagerFactory est lui aussi demandé **avec** le provider :
        // l'algorithme par défaut du JDK s'appuierait sinon sur JSSE et
        // demanderait des certificats à une chaîne qui ne les connaît pas.
        val fabrique = TrustManagerFactory.getInstance(
            TrustManagerFactory.getDefaultAlgorithm(),
            provider,
        )
        fabrique.init(null as KeyStore?)
        val delegue = fabrique.trustManagers[0] as X509TrustManager

        // **Conscrypt appelle le gestionnaire avec `authType = "GENERIC"`,** que
        // sa propre implémentation refuse. Sans ce passage en `"RSA"` la
        // poignée de main échoue sur une chaîne de certificats parfaitement
        // valide. Mesuré le 2026-10-11 (`ConscryptProbe.java`) : ce n'est
        // jamais le site qui refuse, c'est la vérification de chaîne.
        val gestionnaire = object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                if (chain != null && authType != null) delegue.checkClientTrusted(chain, authType)
            }

            override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                if (chain != null) delegue.checkServerTrusted(chain, "RSA")
            }

            override fun getAcceptedIssuers(): Array<X509Certificate> = delegue.acceptedIssuers
        }

        val contexte = SSLContext.getInstance("TLS", provider).apply {
            // Le gestionnaire doit être passé **ici** : c'est lui que la socket
            // Conscrypt appellera pendant la poignée de main.
            init(null, arrayOf(gestionnaire), null)
        }
        Fabrique(contexte.socketFactory, gestionnaire)
    } catch (e: Throwable) {
        // Inclusivement `NoClassDefFoundError` : le JAR peut être absent du
        // classpath au lieu d'échouer proprement.
        System.err.println("[tls] Conscrypt indisponible, repli JSSE : ${e.javaClass.simpleName}")
        null
    }

    /**
     * Décrit la pile **réellement** en service, sans la supposer.
     *
     * C'est la réponse à la faute de méthode la plus coûteuse de la séance du
     * 2026-10-11 : une instrumentation absente du binaire a été lue comme un
     * résultat (« zéro ligne `challenge-debug` », alors que le code n'était pas
     * dans le JAR). Ici on lit l'objet qui va vraiment travailler.
     */
    fun decrire(client: okhttp3.OkHttpClient): String =
        if (estFabriqueConscrypt(client.sslSocketFactory)) "Conscrypt" else "JSSE"

    /** Vrai si la fabrique du client est bien une fabrique Conscrypt. */
    fun estFabriqueConscrypt(usine: SSLSocketFactory): Boolean =
        usine.javaClass.name.contains("Conscrypt", ignoreCase = true)
}