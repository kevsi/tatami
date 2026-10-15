package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/**
 * Les deux validateurs d'adresse du moteur doivent dire la **même chose**.
 *
 * ## Ce qui était mesuré avant la correction
 *
 * `Server.validerUrlImage` testait l'IPv4 avec une expression régulière qui ne
 * reconnaît que la forme pointée `a.b.c.d`. Or un hôte peut s'écrire en décimal,
 * en hexadécimal, en octal ou abrégé — et OkHttp 5.1 accepte ces formes tel quel.
 *
 * Le chemin des images downloadées par un script personnel passait donc par ce
 * validateur-là, pendant que `HttpBridge` savait déjà traiter les formes
 * exotiques : les deux disaient des choses différentes, et c'était le faible qui
 * était en service.
 *
 * Chaque test a son contrôle négatif (la forme est refusée) ; le dernier est le
 * contrôle positif, sans lequel « tout est refusé » passerait.
 */
class SsrfAdresseLitteraleTest {

    /** Les formes d'un même hôte qui doivent toutes être refusées. */
    private val versBoucleLocale = listOf(
        "127.0.0.1" to "pointée",
        "127.1" to "abbreviée",
        "2130706433" to "décimale",
        "0x7f000001" to "hexadécimale",
        "017700000001" to "octale",
        "0" to "zéro",
        "10.0.0.1" to "privée 10/8",
        "172.16.0.1" to "privée 172.16/12",
        "192.168.1.1" to "privée 192.168/16",
        "169.254.169.254" to "lien local (métadonnées)",
        "100.64.0.1" to "CGNAT 100.64/10",
        "224.0.0.1" to "multicast",
    )

    @Test
    fun `chaque forme exotique de la boucle locale est reconnue non publique`() {
        for ((hote, forme) in versBoucleLocale) {
            assertEquals(
                false,
                adresseLitteraleEstPublique(hote),
                "« $hote » ($forme) doit être reconnu comme non public",
            )
        }
    }

    @Test
    fun `une ipv6 litterale locale est reconnue non publique`() {
        for (hote in listOf("[::1]", "[::ffff:127.0.0.1]", "[fe80::1]")) {
            assertEquals(
                false,
                adresseLitteraleEstPublique(hote),
                "« $hote » doit être reconnu comme non public",
            )
        }
    }

    @Test
    fun `un nom n'est pas une adresse litterale`() {
        // `null` = « ce n'est pas une adresse littérale ». La résolution d'un nom
        // se fait ailleurs : la faire ici ouvrirait la porte au DNS rebinding.
        assertNull(adresseLitteraleEstPublique("exemple.com"))
        assertNull(adresseLitteraleEstPublique("mangadex.org"))
    }

    @Test
    fun `une adresse publique est reconnue publique`() {
        // Le contrôle positif : sans lui, une fonction qui refuse tout passerait
        // tous les tests ci-dessus.
        assertEquals(true, adresseLitteraleEstPublique("1.1.1.1"))
        assertEquals(true, adresseLitteraleEstPublique("93.184.216.34"))
        assertEquals(true, adresseLitteraleEstPublique("[2606:4700:4700::1111]"))
    }

    @Test
    fun `le decimal qui designe les metadonnees est refuse`() {
        // Le cas mesuré le 2026-10-15, écrit tel quel dans la source d'un script :
        // `URI.create("https://2852039166/x").host` vaut `2852039166`, et
        // `InetAddress.getByName("2852039166")` vaut `169.254.169.254`.
        val hote = assertNotNull(
            java.net.URI.create("https://2852039166/x").host,
            "la forme décimale doit rester un hôte pour l'analyseur d'URL",
        )
        assertEquals("2852039166", hote)
        assertEquals(false, adresseLitteraleEstPublique(hote))
    }

    @Test
    fun `le chemin natif refuse desormais le http en clair`() = kotlinx.coroutines.runBlocking {
        // Le chemin natif (`fetch_octets`) n'avait aucun filtre, et son URL vient
        // du HTML du site. On verifie ici le garde lui-meme, sans passer par le
        // protocole : la fonction est interne, donc le temoin est l'exception.
        val contexte = EngineContext()
        val e = assertFailsWith<IllegalArgumentException> {
            fetch_octets(contexte, "http://example.com/1.jpg", "https://example.com/")
        }
        assertEquals(true, e.message?.contains("https"), "le refus doit nommer https : ${e.message}")
    }

    @Test
    fun `le chemin natif refuse une adresse litterale privee`() = kotlinx.coroutines.runBlocking {
        // La forme pointée **et** la forme décimale : c'est cette seconde forme
        // que le chemin natif laissait passer, parce que son validateur ne
        // reconnaît que `a.b.c.d`.
        //
        // Chaque refus est vérifié par son **message**, pas seulement par le type :
        // deux gardes différentes (https / adresse){lève} des exceptions du même
        // type, donc lever la bonne ne prouve pas que c'est la bonne.
        for (url in listOf(
            "https://169.254.169.254/latest/meta-data/",
            "https://2852039166/latest/meta-data/",
        )) {
            val e = assertFailsWith<IllegalArgumentException>(message = url) {
                fetch_octets(EngineContext(), url, "https://example.com/")
            }
            assertEquals(
                true,
                e.message?.contains("reseau prive"),
                "« $url » doit être refusé pour l'adresse, pas pour une autre règle : ${e.message}",
            )
        }
    }

    @Test
    fun `le chemin natif laisse passer une adresse https publique`() = kotlinx.coroutines.runBlocking {
        // Le contrôle **positif** du chemin natif, hors réseau. `.invalid` est
        // réservé par la RFC 2606 et ne résout jamais : l'échec vient donc du DNS,
        // ce qui prouve que le garde a laissé passer la requête. Un hôte refusé
        // par une règle d'adresse lèverait, lui, une `IllegalArgumentException`
        // **avant** toute connexion — c'est la différence que ce test mesure.
        //
        // Premier essai mesuré le 2026-10-15 : le témoin utilisait `exemple.com`,
        // qui **existe** — le téléchargement a réussi et le test a échoué en
        // annonçant « completed successfully ». Un témoin qui sort sur le réseau
        // ne prouve rien de déterministe.
        val e = assertFailsWith<java.net.UnknownHostException> {
            fetch_octets(
                EngineContext(),
                "https://temoin-hote-inexistant.invalid/1.jpg",
                "https://temoin-hote-inexistant.invalid/",
            )
        }
        assertEquals(
            true,
            e !is IllegalArgumentException,
            "un hôte public ne doit pas être refusé par une règle d'adresse : ${e.message}",
        )
    }
}
