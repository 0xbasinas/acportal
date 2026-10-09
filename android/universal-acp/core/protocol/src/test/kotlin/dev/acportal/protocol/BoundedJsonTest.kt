package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class BoundedJsonTest {
    @Test fun exactUtf8AndEscapesRoundTripAtTheByteBoundary() {
        val value=buildJsonObject {put("text","世界\n\u0001\"\\😀")}
        val expected=value.toString().toByteArray(Charsets.UTF_8)
        assertArrayEquals(expected,boundedJsonBytes(value,expected.size,"limit"))
        try {boundedJsonBytes(value,expected.size-1,"limit");fail("UTF-8 limit bypassed")}
        catch(failure:IllegalArgumentException) {assertEquals("limit",failure.message)}
    }
    @Test fun largeEscapedInputCannotBypassTheOutputBudget() {
        try {boundedJsonBytes(JsonPrimitive("\u0001".repeat(100_000)),1024,"limit");fail("Escaped content bypassed limit")}
        catch(failure:IllegalArgumentException) {assertEquals("limit",failure.message)}
    }
}
