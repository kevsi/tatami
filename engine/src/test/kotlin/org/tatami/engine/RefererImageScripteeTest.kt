package org.tatami.engine

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Test

/**
 * La requête de téléchargement d'image d'une **source scriptée** doit porter
 * un `Referer`.
 *
 * ## Ce qui est mesuré, et pourquoi
 *
 * Le 2026-10-13, sur `rimuscan.fr` : la même URL d'image répond **403** avec un
 * simple `User-Agent`, et **200, image/jpeg, 786 715 octets** dès qu'on ajoute
 * un `Referer`. L'**origine seule** suffit — l'URL de la page de chapitre n'est
 * pas nécessaire. Beaucoup de sites refusent le vol d'image à chaud, donc sans
 * cet en-tête toutes leurs images sont noires dans le lecteur.
 *
 * Ce n'était pas une fonctionnalité manquante : le chemin **natif**
 * (`fetch_octets`) envoie déjà un `Referer`, le chemin scripté n'en envoyait
 * aucun. Le test verrouille l'alignement.
 *
 * ## Ce que ce test ne prouve pas
 *
 * Il éprouve le **constructeur** de la requête, pas la requête vue du serveur
 * distant : `telechargerImage` est `private` et son `OkHttpClient` vient d'un
 * `EngineContext` sans couture d'injection (le constructeur ne prend aucun
 * paramètre), donc aucun test ne peut observer les en-têtes sur le fil. Un
 * intercepteur de test d'OkHttp réglerait le sujet, mais il faudrait ouvrir
 * `EngineContext`, dont le code fait l'objet d'un autre chantier.
 *
 * La conséquence est assumée et écrite ici : réécrire la requête **en dur** dans
 * `telechargerImage` laisserait ce test vert. C'est pourquoi la preuve que la
 * porte mord — la suppression du `Referer` qui fait échouer la sonde NDJSON,
 * consignée dans le journal de session — passe par cette sonde, pas ici.
 */
class RefererImageScripteeTest {

    @Test
    fun `la requete porte un Referer a l'origine de l'image`() {
        val requete = requeteImageScriptee("https://rimuscan.fr/uploads/mangas/x/chapters/1/001.jpg?v=123")
        assertEquals(
            "https://rimuscan.fr/",
            requete.header("Referer"),
            "le Referer doit etre l'origine de l'image",
        )
    }

    @Test
    fun `le Referer ne transporte ni le chemin ni la chaine de requete`() {
        val url = "https://exemple.fr/uploads/mangas/serie/chapters/3/007.jpg?jeton=secret"
        val referer = requeteImageScriptee(url).header("Referer")
        assertTrue(
            !referer!!.contains("jeton"),
            "un jeton de session ne doit pas partir dans le Referer : $referer",
        )
        assertTrue(
            !referer.contains("chapters"),
            "le Referer doit se limiter a l'origine : $referer",
        )
    }

    @Test
    fun `chaque origine donne son propre Referer`() {
        // Deux sources distinctes ne doivent pas se contaminer : le Referer
        // suit l'URL de l'image, pas une valeur mémorisée.
        assertEquals(
            "https://a.fr/",
            requeteImageScriptee("https://a.fr/i/1.jpg").header("Referer"),
        )
        assertEquals(
            "https://b.fr/",
            requeteImageScriptee("https://b.fr/i/1.jpg").header("Referer"),
        )
    }

    @Test
    fun `le port de l'image est volontairement absent du Referer`() {
        // Le Referer est bâti sur `HttpUrl.host`, qui **exclut** le port : une
        // image servie sur `exemple.fr:8443` part avec `https://exemple.fr/`.
        // Ce n'est pas un oubli, c'est l'origine « schéma + hôte » qui est
        // transmise. On l'écrit ici pour que le comportement soit connu plutôt
        // que découvert un jour sur un site qui tourne derrière un port.
        assertEquals(
            "https://exemple.fr/",
            requeteImageScriptee("https://exemple.fr:8443/i/1.jpg").header("Referer"),
        )
    }

    @Test
    fun `une URL illisible est refusee, comme le fait deja Request Builder`() {
        // Une variante « URL non analysable -> Referer = URL brute » avait été
        // écrite : elle renvoyait l'URL de l'image, chemin et `?jeton=` compris,
        // exactement ce que le reste du travail cherche à ne pas laisser
        // circuler. Elle était de surcroît **inatteignable** — `.url()` a déjà
        // rejeté l'URL avant qu'on puisse choisir un repli. Le repli est donc
        // supprimé, et le comportement réel est celui-ci : un refus franc.
        // (Le cas nominal est couvert par les autres tests : une URL légitime
        // reste acceptée et porte son `Referer`.)
        assertThrows(IllegalArgumentException::class.java) {
            requeteImageScriptee("https://hôte invalide/i/1.jpg")
        }
        assertThrows(IllegalArgumentException::class.java) {
            requeteImageScriptee("pas-une-url")
        }
    }

    @Test
    fun `la requete vise bien l'URL de l'image`() {
        // Garde-fou : si la construction evoluait pour viser autre chose, le
        // Referer resterait vert mais l'image ne serait plus la bonne.
        val requete = requeteImageScriptee("https://exemple.fr/uploads/003.jpg")
        assertEquals("https://exemple.fr/uploads/003.jpg", requete.url.toString())
        assertEquals("GET", requete.method)
    }

    @Test
    fun `User-Agent reste absent de la requete de l'image`() {
        // L'agent utilisateur est pose par l'intercepteur du contexte, pas ici.
        // Ce test ne dit rien de sa valeur : il dit que la construction ne
        // double pas un en-tete deja gere ailleurs.
        assertNull(requeteImageScriptee("https://exemple.fr/i.jpg").header("User-Agent"))
    }
}