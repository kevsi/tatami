package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals

class SourceLocaleIndexTest {

    @Test
    fun `les sources connues a suffixe trompeur restent en francais`() {
        assertEquals("fr", SourceLocaleIndex.langOf("MANGASORIGINES"))
        assertEquals("fr", SourceLocaleIndex.langOf("HENTAIORIGINES"))
    }
}
