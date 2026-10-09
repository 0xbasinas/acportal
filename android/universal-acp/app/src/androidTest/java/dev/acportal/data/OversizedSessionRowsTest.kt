package dev.acportal.data

import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

/**
 * Rows written by older versions may exceed Android's 2 MiB CursorWindow, which fails the
 * whole query. The guarded DAO queries must replace such columns instead (device only).
 */
class OversizedSessionRowsTest {
    @Test fun metadataRefreshPreservesOriginalLegacyColumns() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = java.io.File(context.cacheDir, "pr10-rows-${java.util.UUID.randomUUID()}.db")
        val database = Room.databaseBuilder<PortalDatabase>(context, file.absolutePath).setDriver(AndroidSQLiteDriver()).build()
        try {
            val dao = database.portal()
            dao.saveHost(HostProfile("h", "Host", "https://host.invalid", "alias", "device", Long.MAX_VALUE))
            val huge = "x".repeat(STORED_STATE_MAX_BYTES + 1)
            dao.saveSession(StoredSession("h", "big", "old", state = huge, draft = huge, updatedAt = 7, archived = true))
            val guarded = dao.session("h", "big")!!
            dao.saveSessionMetadata(guarded.copy(metadata = "fresh"))
            // Read only scalar lengths directly, so this assertion cannot hit CursorWindow's row budget.
            AndroidSQLiteDriver().open(file.absolutePath).use { connection ->
                connection.prepare("SELECT metadata, length(state), length(draft), updatedAt, archived FROM sessions WHERE id='big'").use { row ->
                    assertTrue(row.step())
                    assertEquals("fresh", row.getText(0))
                    assertEquals(huge.length.toLong(), row.getLong(1))
                    assertEquals(huge.length.toLong(), row.getLong(2))
                    assertEquals(7L, row.getLong(3))
                    assertEquals(1L, row.getLong(4))
                }
            }
            dao.saveSessionMetadata(StoredSession("h", "new", "new metadata"))
            assertEquals("new metadata", dao.session("h", "new")!!.metadata)
        } finally {
            database.close()
            listOf(file, java.io.File(file.path + "-wal"), java.io.File(file.path + "-shm"), java.io.File(file.path + "-journal")).forEach { it.delete() }
        }
    }
    @Test fun oversizedLegacyColumnsAreReplacedInsteadOfFailingTheList() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val database = Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        try {
            val dao = database.portal()
            dao.saveHost(HostProfile("h", "Host", "https://host.invalid", "alias", "device", Long.MAX_VALUE))
            val info = SessionInfo("big", "mock", "acp", "/w")
            val huge = "界".repeat(1_000_000)
            dao.saveSession(StoredSession("h", "big", WireJson.encodeToString(info) + " ".repeat(STORED_METADATA_MAX_BYTES), state = "{\"x\":\"$huge\"}", draft = huge))
            dao.saveSession(StoredSession("h", "small", WireJson.encodeToString(info.copy(id = "small")), state = "", draft = "keep"))
            val rows = dao.sessions().first().associateBy { it.id }
            assertEquals(2, rows.size)
            val big = rows.getValue("big")
            assertEquals("", big.metadata)
            assertEquals(OVERSIZED_STATE_PLACEHOLDER, big.state)
            assertEquals("", big.draft)
            assertTrue(storedSessionState(big).historyGap)
            assertEquals(STORED_DETAILS_UNAVAILABLE, storedSessionInfo(big).status)
            assertEquals("keep", rows.getValue("small").draft)
            assertEquals("", dao.session("h", "big")!!.draft)
        } finally {
            database.close()
        }
    }
}
