package dev.acportal.data

import org.junit.Assert.*
import org.junit.Test

class PreviewAddressTest {
    @Test fun aPortAloneOrWithTheLocalNameIsAPage() {
        assertEquals(5173 to "/",parsePreviewAddress("5173"))
        assertEquals(5173 to "/",parsePreviewAddress("localhost:5173"))
        assertEquals(3000 to "/a/b?x=1",parsePreviewAddress("http://127.0.0.1:3000/a/b?x=1"))
        assertEquals(8080 to "/",parsePreviewAddress("  localhost:8080/ "))
    }
    @Test fun aPathAloneKeepsTheCurrentPort() {
        assertEquals(null to "/docs",parsePreviewAddress("/docs"))
    }
    @Test fun otherSitesAndNonsenseAreRefused() {
        assertNull(parsePreviewAddress("example.com"))
        assertNull(parsePreviewAddress("https://example.com:8443/"))
        assertNull(parsePreviewAddress(""))
        assertNull(parsePreviewAddress("99999"))
        assertNull(parsePreviewAddress("javascript:alert(1)"))
    }
}
