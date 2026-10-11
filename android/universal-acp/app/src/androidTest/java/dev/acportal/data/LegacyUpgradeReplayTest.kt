package dev.acportal.data

import android.database.sqlite.SQLiteDatabase
import androidx.test.platform.app.InstrumentationRegistry
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.PortalApplication
import dev.acportal.presentation.PortalTheme
import dev.acportal.presentation.SessionScreen
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
import org.junit.Rule
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
        val permissionId = InstrumentationRegistry.getArguments().getString("replayPermissionId")
        check(!replayCheckpoint().exists()) { "An earlier replay fixture is unfinished; run cleanup first" }
        val room = PortalDatabase.open(context)
        try {
            check(room.portal().hosts().first().none { it.id == hostId }) { "Host already saved; use a fresh acceptance install" }
            // Placeholder credential: the verify phase pairs again, which replaces it.
            room.portal().saveHost(HostProfile(hostId, "Replay fixture", address, "00000000-0000-4000-8000-000000000000", "fixture", Long.MAX_VALUE))
        } finally { room.close() }
        SQLiteDatabase.openDatabase(context.getDatabasePath("portal.db").path, null, SQLiteDatabase.OPEN_READWRITE).use { database ->
            val metadata = WireJson.encodeToString(SessionInfo(sessionId, "mock", "pending", "/replay", status = "running"))
            val state = if(permissionId == null) WireJson.encodeToString(SessionState(items = listOf(TimelineItem.Text("legacy", "user", "Legacy replay conversation " + "r".repeat(3 * 1024 * 1024)))))
                else WireJson.encodeToString(SessionState(items=listOf(
                    TimelineItem.Text("legacy-user","user","Upgrade fixture original prompt"),
                    TimelineItem.Text("legacy-agent","agent","I'll inspect the workspace.\n")
                ),permissions=mapOf("obsolete-cache" to Permission(kotlinx.serialization.json.JsonPrimitive("obsolete-cache"),WireJson.parseToJsonElement("""{"toolCall":{"title":"Obsolete cached approval"},"options":[{"optionId":"obsolete","name":"Never submit cached approval","kind":"allow_once"}]}""").objectValue()))))
            database.execSQL("INSERT INTO sessions(hostId,id,metadata,state,draft,updatedAt,archived) VALUES(?,?,?,?,?,?,?)",
                arrayOf<Any>(hostId, sessionId, metadata, state, LEGACY_DRAFT, 1300, 0))
            if(permissionId != null) {
                val oversized = WireJson.encodeToString(SessionState(items=listOf(TimelineItem.Text("oversized","user","r".repeat(3*1024*1024)))))
                database.execSQL("INSERT INTO sessions(hostId,id,metadata,state,draft,updatedAt,archived) VALUES(?,?,?,?,?,?,?)",
                    arrayOf<Any>(hostId,"upgrade-oversized",metadata,oversized,"oversized retained draft",1400,1))
            }
        }
        replayCheckpoint().writeText("$hostId\n$sessionId\n${permissionId.orEmpty()}")
    }
}

class UpgradeReplayVerificationTest {
    @get:Rule val compose=createComposeRule()
    @Test fun upgradedRowsReconnectAndReplayFromTheLiveHost() = runBlocking {
        replayContext(); phase("verify")
        check(replayCheckpoint().exists()) { "Run the prepare phase on the older APK first" }
        val (hostId, sessionId) = replayCheckpoint().readLines().let { it[0] to it[1] }
        val expectedPermission = replayCheckpoint().readLines().getOrNull(2)?.takeIf {it.isNotEmpty()}
        val repository = (replayContext().applicationContext as PortalApplication).repository
        val before = repository.sessions.first().single { it.hostId == hostId && it.id == sessionId }
        if(expectedPermission == null) assertEquals("the upgrade must replace the oversized legacy state", OVERSIZED_STATE_PLACEHOLDER, before.state)
        else {
            assertTrue(storedSessionState(before).items.any {it is TimelineItem.Text && it.text=="Upgrade fixture original prompt"})
            val oversized = repository.sessions.first().single {it.hostId==hostId && it.id=="upgrade-oversized"}
            assertEquals(OVERSIZED_STATE_PLACEHOLDER,oversized.state)
            assertEquals("oversized retained draft",oversized.draft)
            assertTrue(oversized.archived)
        }
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
            if(expectedPermission != null) {
                assertEquals(setOf(kotlinx.serialization.json.JsonPrimitive(expectedPermission).toString()),live.state.value.permissions.keys)
                assertFalse(live.state.value.permissions.containsKey("obsolete-cache"))
                assertTrue(live.state.value.items.any {it is TimelineItem.Text && it.role=="user" && it.text=="Upgrade fixture original prompt"})
                assertTrue(live.state.value.items.any {it is TimelineItem.Text && it.role=="agent" && it.text.contains("I'll inspect")})
                compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
                    SessionScreen(live,before,onBack={},onPrompt={_,_->fail("Upgrade cannot send a prompt")},onCancel={},
                        onPermission={_,_->fail("Upgrade cannot answer an approval")},onConfigure={_,_->},onDraft={},
                        onElicitation={_,_,_->fail("Upgrade cannot submit a form")})
                }}}
                compose.onNodeWithText("Propose a mock file change").assertIsDisplayed()
                compose.onNodeWithText("Allow once").assertIsDisplayed().assertIsEnabled()
                compose.onNodeWithText("Never submit cached approval").assertDoesNotExist()
            }
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
