package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import org.json.JSONObject

class BrowserIdentityTest {

    @Test
    fun `le profil conserve les client hints et les langues captures`() {
        val identity = BrowserIdentity.fromJson(
            JSONObject(
                """
                {
                  "brands":[{"brand":"Microsoft Edge","version":"154"}],
                  "mobile":false,
                  "platform":"Windows",
                  "fullVersionList":[{"brand":"Microsoft Edge","version":"154.0.1"}],
                  "platformVersion":"19.0.0",
                  "architecture":"x86",
                  "bitness":"64",
                  "model":"",
                  "language":"fr-FR",
                  "languages":["fr-FR","fr"]
                }
                """.trimIndent(),
            ),
        )

        assertEquals(listOf(BrandVersion("Microsoft Edge", "154")), identity.brands)
        assertEquals(false, identity.mobile)
        assertEquals("Windows", identity.platform)
        assertEquals(
            listOf(BrandVersion("Microsoft Edge", "154.0.1")),
            identity.fullVersionList,
        )
        assertEquals("19.0.0", identity.platformVersion)
        assertEquals("x86", identity.architecture)
        assertEquals("64", identity.bitness)
        assertEquals("", identity.model)
        assertEquals("fr-FR", identity.language)
        assertEquals(listOf("fr-FR", "fr"), identity.languages)
    }

    @Test
    fun `le profil est associe au domaine de la clearance et a ses sous domaines`() {
        val context = EngineContext()
        val identity = BrowserIdentity.fromJson(
            JSONObject("""{"brands":[],"mobile":false,"platform":"Windows"}"""),
        )
        context.definirIdentiteNavigateurChallenge("toonfr.com", identity)

        assertEquals(identity, context.identiteNavigateurChallengePour("toonfr.com"))
        assertEquals(identity, context.identiteNavigateurChallengePour("img.toonfr.com"))
        assertNull(context.identiteNavigateurChallengePour("autre.example"))
    }
}
