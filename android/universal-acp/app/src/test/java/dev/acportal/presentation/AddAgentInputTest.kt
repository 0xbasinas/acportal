package dev.acportal.presentation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AddAgentInputTest {
    @Test fun idComesFromTheNameAsLowerCaseHyphens() {
        assertEquals("goose",agentIdFromName("Goose"))
        assertEquals("my-agent-2",agentIdFromName("  My Agent #2 "))
        assertEquals("",agentIdFromName("***"))
        assertEquals(64,agentIdFromName("a".repeat(100)).length)
    }
    @Test fun environmentLinesParseOrNameTheBadLine() {
        val (ok,bad)=parseEnvironment("GOOSE_MODE=approve\n\n  EMPTY=  ")
        assertNull(bad)
        assertEquals(mapOf("GOOSE_MODE" to "approve","EMPTY" to ""),ok)
        assertEquals("KEY = x",parseEnvironment("GOOSE_MODE=approve\nKEY = x").second)
        assertEquals("=oops",parseEnvironment("=oops").second)
        assertEquals("novalue",parseEnvironment("novalue").second)
        assertEquals(mapOf("A" to "b=c"),parseEnvironment("A=b=c").first)
    }
}
