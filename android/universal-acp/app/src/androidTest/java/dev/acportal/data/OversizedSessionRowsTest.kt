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
