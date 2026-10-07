package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class AgentInfoTest {
    @Test fun discoveryRetainsOptionalHostDiagnosticsAndAcceptsOlderResponses() {
        val detailed=WireJson.decodeFromString<AgentInfo>("""{"id":"custom","name":"Custom agent","installed":true,"status":"misconfigured","executable":"/usr/bin/custom","version":"2.0","reason":"Working directory is unavailable"}""")
        assertEquals("/usr/bin/custom",detailed.executable)
        assertEquals("2.0",detailed.version)
        assertEquals("Working directory is unavailable",detailed.reason)
        assertFalse(detailed.canStartSession())
        val older=WireJson.decodeFromString<AgentInfo>("""{"id":"old","name":"Older host","installed":true,"status":"available"}""")
        assertNull(older.executable);assertNull(older.version);assertNull(older.reason)
        assertTrue(older.canStartSession())
        assertFalse(older.copy(enabled=false).canStartSession())
        assertFalse(older.copy(installed=false).canStartSession())
    }
}
