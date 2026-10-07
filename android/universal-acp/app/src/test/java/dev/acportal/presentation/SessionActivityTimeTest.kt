package dev.acportal.presentation

import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import kotlinx.serialization.json.*
import java.time.Instant
import java.time.ZoneId
import java.util.Locale
import org.junit.Assert.*
import org.junit.Test

class SessionActivityTimeTest {
    private val now=Instant.parse("2026-10-07T12:00:00Z").toEpochMilli()
    private val utc=ZoneId.of("UTC")
    @Test fun agentTimeTakesPrecedenceWithExplicitLocalSaveFallback() {
        val stored=StoredSession("h","s","{}",updatedAt=now)
        val state=SessionState(sessionInfo=buildJsonObject {put("updatedAt","2026-10-07T10:00:00Z")})
        val reported=sessionActivityTime(stored,state)!!
        assertTrue(reported.reportedByAgent);assertEquals("2 hours ago",activityTimeLabel(reported.timestamp,now,utc,Locale.US))
        assertFalse(sessionActivityTime(stored,state.copy(sessionInfo=buildJsonObject {put("updatedAt","invalid")}))!!.reportedByAgent)
        assertNull(sessionActivityTime(stored.copy(updatedAt=0),SessionState()))
    }
    @Test fun relativeLabelsUseLocalDayBoundaries() {
        assertEquals("Just now",activityTimeLabel(now-10_000,now,utc,Locale.US))
        assertEquals("1 minute ago",activityTimeLabel(now-60_000,now,utc,Locale.US))
        assertEquals("2 hours ago",activityTimeLabel(now-2*3600_000,now,utc,Locale.US))
        val midnight=Instant.parse("2026-10-07T21:05:00Z").toEpochMilli()
        val before=Instant.parse("2026-10-07T20:55:00Z").toEpochMilli()
        assertTrue(activityTimeLabel(before,midnight,ZoneId.of("Europe/Athens"),Locale.US).startsWith("Yesterday,"))
    }
    @Test fun futureAndOldTimesRemainAbsoluteInsteadOfInventingActivity() {
        assertFalse(activityTimeLabel(now+3600_000,now,utc,Locale.US).contains("ago"))
        assertFalse(activityTimeLabel(now-7*86400_000L,now,utc,Locale.US).contains("ago"))
    }
}

