package dev.acportal.data

import dev.acportal.protocol.*
import dev.acportal.transport.ConnectionState
import kotlinx.serialization.json.*
import org.junit.Assert.assertEquals
import org.junit.Test

class LiveSessionActivityTest {
    @Test fun cachedProcessingDoesNotClaimLiveWorkWithoutAConnectionAndReplay() {
        val busy=SessionState(processing=true)
        assertEquals(LiveSessionActivity.DISCONNECTED,liveSessionActivity(busy,ConnectionState.Disconnected))
        assertEquals(LiveSessionActivity.RECONNECTING,liveSessionActivity(busy,ConnectionState.Reconnecting(1)))
        assertEquals(LiveSessionActivity.SYNCING,liveSessionActivity(busy.copy(replaying=true),ConnectionState.Connected))
        assertEquals(LiveSessionActivity.WORKING,liveSessionActivity(busy,ConnectionState.Connected))
    }
    @Test fun approvalStopAndConnectionFailuresHaveDistinctStates() {
        val pending=SessionState(processing=true,permissions=mapOf("p" to Permission(JsonPrimitive("p"),buildJsonObject {})))
        assertEquals(LiveSessionActivity.WAITING_APPROVAL,liveSessionActivity(pending,ConnectionState.Connected))
        assertEquals(LiveSessionActivity.STOPPED,liveSessionActivity(pending.copy(sessionClosed=true),ConnectionState.Disconnected))
        assertEquals(LiveSessionActivity.FAILED,liveSessionActivity(pending,ConnectionState.Failed("Unavailable")))
        assertEquals(LiveSessionActivity.READY,liveSessionActivity(SessionState(),ConnectionState.Connected))
        assertEquals(LiveSessionActivity.CONNECTING,liveSessionActivity(SessionState(),ConnectionState.Connecting))
    }
}
