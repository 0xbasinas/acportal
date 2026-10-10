package dev.acportal.data

import android.database.sqlite.SQLiteDatabase
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runners.model.Statement
import java.io.File
import java.security.MessageDigest

private fun upgradeContext() = InstrumentationRegistry.getInstrumentation().targetContext.also {
    assumeTrue("Only an owned acceptance package may be seeded", it.packageName == "dev.acportal.acceptance")
}
private fun checkpoint() = File(upgradeContext().filesDir, "legacy-upgrade.sha256")

/** Hash large columns in small SQLite slices, avoiding the CursorWindow row limit. */
private fun fingerprint(database: SQLiteDatabase): String {
    val digest = MessageDigest.getInstance("SHA-256")
    database.rawQuery("SELECT id, metadata, updatedAt, archived FROM sessions ORDER BY id", null).use { rows ->
        while (rows.moveToNext()) {
            val id = rows.getString(0)
            repeat(4) { digest.update(rows.getString(it).toByteArray()) }
            for (column in listOf("state", "draft")) {
                var position = 1
                while (true) {
                    val slice = database.rawQuery("SELECT substr($column, ?, 4096) FROM sessions WHERE id=?", arrayOf(position.toString(), id)).use {
                        check(it.moveToFirst()); it.getString(0)
                    }
                    if (slice.isEmpty()) break
                    digest.update(slice.toByteArray()); position += slice.length
                }
            }
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) }
}

/** Run only this method against the installed legacy APK, then replace that APK without clearing data. */
class LegacyUpgradePreparationTest {
    @Test fun seedOwnedLegacyRows() = runBlocking {
        val context = upgradeContext()
        assumeTrue("Run preparation separately on the legacy APK", InstrumentationRegistry.getArguments().getString("legacyUpgrade") == "prepare")
        check(!checkpoint().exists()) { "Do not overwrite an unfinished upgrade fixture" }
        val room = PortalDatabase.open(context)
        var alreadySeeded = false
        try {
            val hosts = room.portal().hosts().first()
            alreadySeeded = hosts.isNotEmpty()
            if (alreadySeeded) {
                assertEquals(listOf("upgrade"), hosts.map { it.id })
                assertEquals("Upgrade fixture", hosts.single().label)
            } else room.portal().saveHost(HostProfile("upgrade", "Upgrade fixture", "http://127.0.0.1:1", "unused-fixture", "fixture", Long.MAX_VALUE))
            room.portal().saveMainPage("sessions")
        } finally { room.close() }
        SQLiteDatabase.openDatabase(context.getDatabasePath("portal.db").path, null, SQLiteDatabase.OPEN_READWRITE).use { database ->
            if (alreadySeeded) {
                // Recover only the exact fixture after a checkpoint-write failure, without replacing rows.
                database.rawQuery("SELECT count(*), min(updatedAt), max(updatedAt) FROM sessions WHERE hostId='upgrade'", null).use {
                    assertTrue(it.moveToFirst()); assertEquals(2, it.getInt(0)); assertEquals(1233L, it.getLong(1)); assertEquals(1234L, it.getLong(2))
                }
                checkpoint().writeText(fingerprint(database))
                return@use
            }
            val metadata = WireJson.encodeToString(SessionInfo("large", "goose", "fixture-acp", "/Upgrade large fixture", status="interrupted"))
            val state = WireJson.encodeToString(SessionState(items=listOf(TimelineItem.Text("legacy", "user", "Legacy large conversation " + "x".repeat(3 * 1024 * 1024)))))
            database.execSQL("INSERT INTO sessions(hostId,id,metadata,state,draft,updatedAt,archived) VALUES(?,?,?,?,?,?,?)",
                arrayOf<Any>("upgrade", "large", metadata, state, "legacy draft " + "d".repeat(600 * 1024), 1234, 0))
            val small = WireJson.encodeToString(SessionInfo("small", "goose", "fixture-small", "/Upgrade small fixture", status="interrupted"))
            database.execSQL("INSERT INTO sessions(hostId,id,metadata,state,draft,updatedAt,archived) VALUES(?,?,?,?,?,?,?)",
                arrayOf<Any>("upgrade", "small", small, "", "keep small draft", 1233, 0))
            checkpoint().writeText(fingerprint(database))
        }
    }
}

/** Run separately after installing the current acceptance APK over the older APK. */
class LegacyUpgradeVerificationTest {
    val compose = createAndroidComposeRule<MainActivity>()
    private val ownedPackage = TestRule { base, _ -> object : Statement() {
        override fun evaluate() {
            upgradeContext()
            assumeTrue("Run verification separately after upgrade", InstrumentationRegistry.getArguments().getString("legacyUpgrade") == "verify")
            base.evaluate()
        }
    } }
    @get:Rule val rules: RuleChain = RuleChain.outerRule(ownedPackage).around(compose)
    @Test fun actualSessionsAndSavedHistoryPreserveLegacyColumns() = runBlocking {
        val context = upgradeContext()
        check(checkpoint().exists()) { "Preparation must run on the actual older APK first" }
        val room = PortalDatabase.open(context)
        try {
            val rows = room.portal().sessions().first()
            assertEquals(2, rows.size)
            assertEquals(OVERSIZED_STATE_PLACEHOLDER, rows.single { it.id == "large" }.state)
            assertEquals("", rows.single { it.id == "large" }.draft)
            assertEquals("keep small draft", rows.single { it.id == "small" }.draft)
        } finally { room.close() }
        compose.waitUntil(10000) { compose.onAllNodesWithText("Upgrade large fixture").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("session-large").performClick()
        compose.waitUntil(10000) { compose.onAllNodesWithText("View saved conversation").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("View saved conversation").performClick()
        compose.waitUntil(10000) { compose.onAllNodesWithText("Some earlier messages are unavailable in this conversation.").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Some earlier messages are unavailable in this conversation.").assertIsDisplayed()
        // The fixture has no credential and an unavailable loopback host, so opening cannot launch an agent.
        SQLiteDatabase.openDatabase(context.getDatabasePath("portal.db").path, null, SQLiteDatabase.OPEN_READONLY).use {
            assertEquals(checkpoint().readText(), fingerprint(it))
        }
        checkpoint().delete()
        Unit
    }
}
