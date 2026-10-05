package org.tatami.engine

import org.koitharu.kotatsu.parsers.exception.AuthRequiredException
import org.koitharu.kotatsu.parsers.model.MangaParserSource
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/**
 * Un vrai challenge Cloudflare rapporté comme panne interne (tâches 0.14 et 0.13).
 *
 * Mesuré le 2026-10-05 : MANGANATO et KAGANE renvoyaient
 * `INTERNAL: IOException: canceled due to CloudFlareProtectionException`. Un
 * `catch (e: CloudFlareProtectionException)` ne voit rien dans une `IOException`,
 * donc la fenêtre de résolution de la tâche 0.11 ne s'ouvrait jamais.
 *
 * Chaque test vise **une forme d'emballage différente**, parce qu'OkHttp 5 (KMP)
 * les produit toutes : la cause est parfois gardée, parfois perdue au profit du
 * seul message.
 */
@OptIn(org.koitharu.kotatsu.parsers.InternalParsersApi::class)
class ErreursTest {

    @Test
    fun `le challenge direct est trouve`() {
        val e = CloudFlareProtectionException("https://manganato.com/x")
        assertEquals("https://manganato.com/x", challengeDe(e))
    }

    @Test
    fun `le challenge emballe dans une IOException est retrouve`() {
        // Forme mesuree : le parser garde la cause.
        val reel = CloudFlareProtectionException("https://kagane.org/y")
        val emballe = IOException("canceled", reel)
        assertEquals("https://kagane.org/y", challengeDe(emballe))
    }

    @Test
    fun `le challenge perdu dans le message seul est retrouve`() {
        // Forme la plus vicieuse : plus aucune cause, seulement le texte. C'est ce
        // que produit OkHttp 5 quand il remappe une exception d'intercepteur.
        val sansCause = IOException("canceled due to Cloudflare protection at https://x.fr/z")
        assertEquals("https://x.fr/z", challengeDe(sansCause))
    }

    @Test
    fun `trois niveaux d_emballage restent traversables`() {
        val reel = CloudFlareProtectionException("https://a.fr/")
        val e = IOException("niveau 1", IOException("niveau 2", IOException("niveau 3", reel)))
        assertEquals("https://a.fr/", challengeDe(e))
    }

    @Test
    fun `une cause circulaire ne fait pas tourner la boucle`() {
        val a = RuntimeException("a")
        val b = RuntimeException("b", a)
        // Une cause qui se referencia elle-meme doit etre toleree, pas boucler.
        assertNull(challengeDe(b))
    }

    @Test
    fun `une panne ordinaire ne devient pas un challenge`() {
        assertNull(challengeDe(IOException("connection reset")))
        assertNull(authRequise(IOException("connection reset")))
    }

    @Test
    fun `l_auth requise est reconnue directe et emballee`() {
        val directe = AuthRequiredException(MangaParserSource.EXHENTAI)
        assertNotNull(authRequise(directe), "l'exception directe doit etre reconnue")

        val emballee = IOException("canceled due to auth", directe)
        assertNotNull(authRequise(emballee), "l'exception emballee doit etre reconnue")
    }

    @Test
    fun `auth requise ne se confond pas avec un challenge`() {
        // Les deux sont des IOException : les confondre enverrait l'utilisateur
        // dans une fenetre de verification alors qu'il faut un compte.
        val auth = AuthRequiredException(MangaParserSource.EXHENTAI)
        assertNull(challengeDe(auth), "un 401 ne doit pas etre traite comme un challenge")
    }
}
