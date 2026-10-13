package org.tatami.engine

import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertIs

class CloudFlareExceptionTest {

    @Test
    fun `les exceptions de challenge sont des erreurs IO prises en charge par OkHttp`() {
        assertIs<IOException>(CloudFlareProtectionException("https://exemple.test/"))
        assertIs<IOException>(CloudFlareUnreplayableException("https://exemple.test/"))
    }
}
