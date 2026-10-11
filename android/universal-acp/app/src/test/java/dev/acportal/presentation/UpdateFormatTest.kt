package dev.acportal.presentation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.time.ZoneId

class UpdateFormatTest {
    @Test fun sizesUseWholeMegabytesAboveTen() {
        assertEquals("54 MB",formatMegabytes(54L*1024*1024))
        assertEquals("2.5 MB",formatMegabytes((2.5*1024*1024).toLong()))
        assertEquals("0.0 MB",formatMegabytes(0))
    }
    @Test fun publishedTimeIsLocalAndReadable() {
        assertEquals("11 Oct 2026, 18:02",formatPublished("2026-10-11T15:02:00Z",ZoneId.of("Europe/Athens")))
        assertEquals("not a date",formatPublished("not a date"))
        assertNull(formatPublished(null));assertNull(formatPublished(" "))
    }
}
