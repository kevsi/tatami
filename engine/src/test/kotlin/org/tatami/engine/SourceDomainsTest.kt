package org.tatami.engine

import java.net.IDN
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SourceDomainsTest {

    @Test
    fun `les domaines natifs gardent alternatives valides et dedoublonnent`() {
        assertEquals(
            listOf("mangalib.org", "mangalib.me", "toomics.com"),
            domainesNatifs(
                "Mangalib.org:443",
                listOf("mangalib.org", "mangalib.me", "toomics.com/fr"),
            ),
        )
    }

    @Test
    fun `baseUrl script devient un hote canonique sans chemin ni port`() {
        assertEquals("exemple.com", domaineScript("https://Exemple.com:443/fr/"))
        assertEquals("exemple.com", domaineScript("HTTPS://Exemple.com:443/fr/"))
        assertEquals("www.exemple.com", domaineScript("http://www.exemple.com:8080/fr"))
    }

    @Test
    fun `baseUrl sans hote http valide ne fournit pas de domaine`() {
        assertNull(domaineScript("ftp://exemple.com/"))
        assertNull(domaineScript("https://utilisateur@exemple.com/"))
    }

    @Test
    fun `domaines natifs extraient uniquement un hote valide`() {
        assertEquals(listOf("toomics.com"), domainesNatifs("toomics.com/fr", emptyList()))
        assertEquals(listOf("toomics.com"), domainesNatifs("https://toomics.com/fr/", emptyList()))
        assertEquals(listOf("toomics.com"), domainesNatifs("toomics.com:8443/x", emptyList()))
        assertEquals(
            listOf("toomics.com"),
            domainesNatifs(
                "toomics.com/fr",
                listOf("https://toomics.com/fr/?lang=fr#top"),
            ),
        )

        listOf(
            "user@toomics.com/fr",
            "toomics.com@attaquant.com/fr",
            "/fr",
            "",
            "bad_host.example/fr",
        ).forEach { valeur ->
            assertEquals(emptyList(), domainesNatifs(valeur, emptyList()), valeur)
        }
    }

    @Test
    fun `domaine natif internationalise est normalise en ascii`() {
        assertEquals(
            listOf(IDN.toASCII("пример.рф")),
            domainesNatifs("пример.рф/страница", emptyList()),
        )
    }
}
