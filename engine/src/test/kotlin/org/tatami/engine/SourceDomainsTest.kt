package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SourceDomainsTest {

    @Test
    fun `les domaines natifs gardent les alternatives valides et retirent chemins et doublons`() {
        assertEquals(
            listOf("mangalib.org", "mangalib.me"),
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
}
