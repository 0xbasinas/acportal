package dev.acportal.data

import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class SessionCacheTest {
    @Test fun liveDecisionsAndCorrelationsNeverEnterDiskHistory() {
        val state=SessionState(sequence=7,items=listOf(TimelineItem.Text("m","agent","Retained history")),permissions=mapOf("p" to Permission(JsonPrimitive("p"),buildJsonObject {put("privateQuestion","not-a-disk-payload")})),modelRequests=mapOf("rpc" to "model"))
        val bytes=sessionCache(state,true)
        assertFalse(bytes.contains("privateQuestion"))
        val restored=WireJson.decodeFromString<SessionState>(bytes)
        assertEquals(state.items,restored.items);assertEquals(7L,restored.sequence)
        assertTrue(restored.permissions.isEmpty());assertTrue(restored.modelRequests.isEmpty())
    }
    @Test fun nonAsciiHistoryFallsBackBeforeCursorWindowOverflow() {
        val state=SessionState(sequence=12,items=listOf(TimelineItem.Text("large","agent","界".repeat(400_000))),sessionInfo=buildJsonObject {put("title","t".repeat(2_000_000))})
        val bytes=sessionCache(state,true)
        assertTrue(bytes.toByteArray(Charsets.UTF_8).size<MAX_SESSION_CACHE_BYTES)
        val restored=WireJson.decodeFromString<SessionState>(bytes)
        assertEquals(12L,restored.sequence);assertTrue(restored.historyGap)
        assertTrue(restored.items.isEmpty());assertEquals(256,restored.sessionInfo["title"].text().length)
    }
}
