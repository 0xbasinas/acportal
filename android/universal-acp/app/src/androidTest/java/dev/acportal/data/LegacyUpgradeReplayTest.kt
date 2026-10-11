package dev.acportal.data

import android.database.sqlite.SQLiteDatabase
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.*
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/*
 * Old build -> upgrade -> live replay, runbook §7. scripts/upgrade-replay-host.py runs a
 * real acpd with the mock agent (no model calls), creates one session and prints its host
 * id, session id and a phone pairing code. Each phase is a separate instrumentation run
 * selected with -e upgradeReplay prepare|verify|cleanup:
 *   prepare  on the installed OLDER APK: seeds the host row and a legacy oversized row
 *            for that real session (only calls that exist in older builds).
 *   verify   after installing the current APK over it: pairs with the printed code,
 *            opens the session with the network on and waits for the host's replay.
 *   cleanup  removes only the fixture host and its rows.
 */
private fun replayContext() = InstrumentationRegistry.getInstrumentation().targetContext.also {
    assumeTrue("Only an owned acceptance package may be seeded", it.packageName == "dev.acportal.acceptance")
}
private fun phase(name: String) = assumeTrue("Run with -e upgradeReplay $name", InstrumentationRegistry.getArguments().getString("upgradeReplay") == name)
private fun argument(name: String) = InstrumentationRegistry.getArguments().getString(name)?.takeIf { it.isNotBlank() } ?: error("Set -e $name (printed by scripts/upgrade-replay-host.py)")
private fun replayCheckpoint() = File(replayContext().filesDir, "upgrade-replay.txt")
private const val LEGACY_DRAFT = "replay legacy draft"

class UpgradeReplayPreparationTest {
    @Test fun seedLegacyRowsForTheLiveFixtureSession() = runBlocking {
        val context = replayContext(); phase("prepare")
        val hostId = argument("replayHostId"); val sessionId = argument("replaySessionId")
        val address = InstrumentationRegistry.getArguments().getString("replayAddress") ?: "http://127.0.0.1:8767"
        check(!replayCheckpoint().exists()) { "An earlier replay fixture is unfinished; run cleanup first" }
        val room = PortalDatabase.open(context)
        try {
            check(room.portal().hosts().first().none { it.id == hostId }) { "Host already saved; use a fresh acceptance install" }
            // Placeholder credential: the verify phase pairs again, which replaces it.
            room.portal().saveHost(HostProfile(hostId, "Replay fixture", address, "00000000-0000-4000-8000-000000000000", "fixture", Long.MAX_VALUE))
        } finally { room.close() }
        SQLiteDatabase.openDatabase(context.getDatabasePath("portal.db").path, null, SQLiteDatabase.OPEN_READWRITE).use { database ->
            val metadata = WireJson.encodeToString(SessionInfo(sessionId, "mock", "pending", "/replay", status = "running"))
            val state = WireJson.encodeToString(SessionState(items = listOf(TimelineItem.Text("legacy", "user", "Legacy replay conversation " + "r".repeat(3 * 1024 * 1024)))))
            database.execSQL("INSERT INTO sessions(hostId,id,metadata,state,draft,updatedAt,archived) VALUES(?,?,?,?,?,?,?)",
                arrayOf<Any>(hostId, sessionId, metadata, state, LEGACY_DRAFT, 1300, 0))
        }
        replayCheckpoint().writeText("$hostId\n$sessionId")
    }
}

class UpgradeReplayVerificationTest {
    @Test fun upgradedRowsReconnectAndReplayFromTheLiveHost() = runBlocking {
        replayContext(); phase("verify")
        check(replayCheckpoint().exists()) { "Run the prepare phase on the older APK first" }
        val (hostId, sessionId) = replayCheckpoint().readLines().let { it[0] to it[1] }
        val repository = (replayContext().applicationContext as PortalApplication).repository
        val before = repository.sessions.first().single { it.hostId == hostId && it.id == sessionId }
        assertEquals("the upgrade must replace the oversized legacy state", OVERSIZED_STATE_PLACEHOLDER, before.state)
        assertEquals(LEGACY_DRAFT, before.draft)
        val saved = repository.hosts.first().single { it.id == hostId }
        val paired = repository.pair(saved.address, argument("replayCode"), "Replay fixture")
        assertEquals("pairing must reach the same host", hostId, paired.id)
        val live = repository.open(hostId, sessionId)
        try {
            withTimeout(30_000) { while (live.connection.value !is ConnectionState.Connected || live.state.value.replaying) delay(50) }
            assertTrue(live.metadata.value.status in setOf("running", "ready"))
            assertFalse(live.state.value.sessionClosed)
            assertNull(live.state.value.error)
        } finally { live.close() }
        val after = repository.sessions.first().single { it.hostId == hostId && it.id == sessionId }
        assertEquals("replay must not discard the user's draft", LEGACY_DRAFT, after.draft)
        replayCheckpoint().delete()
        Unit
    }
}

class UpgradeReplayCleanupTest {
    @Test fun removeOnlyTheReplayFixture() = runBlocking {
        replayContext(); phase("cleanup")
        val hostId = argument("replayHostId")
        val repository = (replayContext().applicationContext as PortalApplication).repository
        val host = repository.hosts.first().singleOrNull { it.id == hostId }
        if (host != null) { assertEquals("Replay fixture", host.label); repository.forget(hostId) }
        replayCheckpoint().delete()
        Unit
    }
}
