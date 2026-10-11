package dev.acportal.presentation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentSuggestionsTest {
    @Test fun everySuggestionFillsAValidForm() {
        agentSuggestions.forEach {
            assertTrue(it.name,agentIdFromName(it.name).isNotEmpty())
            assertTrue(it.name,it.command.isNotBlank() && it.command.none {c->c.isWhitespace() || c=='/' || c.code==92})
            assertTrue(it.name,it.args.none {arg->arg.isBlank() || arg.contains('\n')})
            assertNull(it.name,parseEnvironment(it.env.entries.joinToString("\n") {e->"${e.key}=${e.value}"}).second)
        }
    }
    @Test fun idsAreUniqueAndNothingIsRegisteredByDefault() {
        assertEquals(agentSuggestions.size,agentSuggestions.map {agentIdFromName(it.name)}.toSet().size)
        assertFalse(agentSuggestions.isEmpty())
    }
}
