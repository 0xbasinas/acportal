package dev.acportal.transport

import org.junit.Assert.*
import org.junit.Test

class ConnectionDiagnosticsTest {
    @Test fun failureTextCannotLeakIntoCopiedDiagnostics() {
        val event=connectionDiagnostic(ConnectionState.Failed("Bearer secret-token user-prompt"),42)
        assertEquals("Error",event.level)
        assertFalse(event.message.contains("secret-token"))
        assertFalse(event.message.contains("user-prompt"))
        assertEquals(42L,event.time)
    }
    @Test fun retentionKeepsOnlyTheLatestTwoHundredEvents() {
        var events=emptyList<ConnectionDiagnostic>()
        repeat(1000) {events=appendDiagnostic(events,connectionDiagnostic(ConnectionState.Reconnecting(it),it.toLong()))}
        assertEquals(200,events.size)
        assertEquals(800L,events.first().time)
        assertEquals(999L,events.last().time)
    }
}
