package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * La pile TLS du moteur est bien Conscrypt, et elle est bien **celle du client
 * de production**.
 *
 * Pourquoi cette porte existe, en une phrase : le 2026-10-11 on a lu « zéro
 * ligne `challenge-debug` » comme un résultat, alors que le code qui écrivait
 * ces lignes n'était pas dans le JAR. Un test qui lit l'objet qui travaille
 * (`client.sslSocketFactory`) ne peut pas raconter ce genre d'histoire : si la
 * fabrique n'est pas posée, le test le voit.
 *
 * Conscrypt n'est pas un détail d'implémentation : c'est la pile TLS de Chrome,
 * et c'est celle qu'`OkHttp` utilise sur Android, donc celle sous laquelle
 * Kotatsu réussit là où notre client était refusé. Le motif est dans
 * `Kotatsu-Redo/.../core/BaseApp.kt:88`.
 */
class PileTlsTest {

    /**
     * Le cas nominal : sans ce test, une régression qui ferait retomber le
     * moteur en JSSE passerait **silencieusement** — `construire()` renvoyant
     * `null` est un repli parfaitement légitime à la construction du client.
     */
    @Test
    fun `la fabrique construite est bien Conscrypt`() {
        val fabrique = assertNotNull(
            PileTls.construire(),
            "Conscrypt est dans le classpath : le build doit poser la pile",
        )
        assertTrue(
            PileTls.estFabriqueConscrypt(fabrique.usine),
            "fabrique inattendue : ${fabrique.usine.javaClass.name}",
        )
    }

    /**
     * Le client de production, pas une fabrique construite à côté.
     *
     * C'est la moitié qui compte : `PileTls.construire()` peut réussir pendant
     * que le builder **oublie** de s'en servir, et le repli silencieux
     * renverrait alors JSSE sans lever la moindre erreur.
     */
    @Test
    fun `le client du moteur utilise la pile Conscrypt`() {
        val contexte = EngineContext()
        assertTrue(
            PileTls.estFabriqueConscrypt(contexte.httpClient.sslSocketFactory),
            "le client de production n'a pas la pile : ${contexte.httpClient.sslSocketFactory.javaClass.name}",
        )
    }

    /**
     * Le client d'images en dérive : il est construit par `newBuilder()` à
     * partir du client principal. Une fabrique **ne se propage pas** par
     * `newBuilder()` dans tous les cas d'OkHttp — c'est un point à mesurer,
     * pas à supposer.
     */
    @Test
    fun `le client d'image herite aussi de la pile`() {
        val contexte = EngineContext()
        assertTrue(
            PileTls.estFabriqueConscrypt(contexte.clientImage.sslSocketFactory),
            "le client d'image n'a pas la pile : ${contexte.clientImage.sslSocketFactory.javaClass.name}",
        )
    }

    /**
     * `decrire` est ce que la mesure lit. Elle doit nommer la pile **en
     * service**, sinon le journal d'une session dit « JSSE » sur une machine
     * qui tourne bien sous Conscrypt — exactement le mensonge qu'une
     * instrumentation mal branchée raconte.
     */
    @Test
    fun `decrire nomme la pile en service`() {
        assertEquals("Conscrypt", PileTls.decrire(EngineContext().httpClient))
    }

    /**
     * Le repli reste possible : c'est lui qui garantit qu'un moteur démarre
     * même si le code natif BoringSSL est absent. On ne peut pas le provoquer
     * ici sans retirer le JAR du classpath, donc on vérifie au moins qu'il n'y
     * a **pas d'exception** et que `estFabriqueConscrypt` répond non sur une
     * fabrique JSSE — c'est le chemin que prendrait le repli.
     */
    @Test
    fun `une fabrique JSSE est reconnue comme telle`() {
        val jsse = assertNotNull(
            runCatching { javax.net.ssl.SSLContext.getDefault().socketFactory }.getOrNull(),
        )
        assertTrue(
            !PileTls.estFabriqueConscrypt(jsse),
            "une fabrique du JDK ne doit pas passer pour Conscrypt",
        )
    }
}