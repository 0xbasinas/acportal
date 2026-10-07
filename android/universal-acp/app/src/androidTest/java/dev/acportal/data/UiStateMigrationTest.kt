package dev.acportal.data

import androidx.room3.Room
import androidx.room3.testing.MigrationTestHelper
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.sqlite.execSQL
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.UUID

class UiStateMigrationTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val context=instrumentation.targetContext
    private val name="ui-state-test-${UUID.randomUUID()}.db"
    @get:Rule val helper=MigrationTestHelper(instrumentation=instrumentation,file=context.getDatabasePath(name),driver=AndroidSQLiteDriver(),databaseClass=PortalDatabase::class)
    @Test fun migrationFromVersionTwoPreservesCatalogAndAddsEmptyUiTable()=runBlocking {
        helper.createDatabase(2).use {connection->
            connection.execSQL("INSERT INTO hosts VALUES ('h','Host','https://host.example','alias','device',123,45,2,1)")
            connection.execSQL("INSERT INTO agent_catalogs VALUES ('h','[]',88)")
        }
        helper.runMigrationsAndValidate(3,emptyList()).use {connection->
            connection.prepare("SELECT metadata,discoveredAt FROM agent_catalogs").use {row->assertTrue(row.step());assertEquals("[]",row.getText(0));assertEquals(88L,row.getLong(1))}
            connection.prepare("SELECT COUNT(*) FROM ui_state").use {row->assertTrue(row.step());assertEquals(0L,row.getLong(0))}
        }
        context.deleteDatabase(name);Unit
    }
    @Test fun independentConcurrentPreferencesReopenWithoutOverwritingEachOther()=runBlocking {
        helper.createDatabase(1).close()
        fun open()=Room.databaseBuilder<PortalDatabase>(context,name).setDriver(AndroidSQLiteDriver()).build()
        val first=open()
        try {
            val dao=first.portal()
            assertNull(dao.uiState().first())
            coroutineScope {launch {dao.saveMainPage("sessions")};launch {dao.saveSessionFilters(true,true,"configuration")}}
            assertEquals(StoredUiState(mainPage="sessions",sessionsArchived=true,sessionsActive=true,sessionsQuery="configuration"),dao.uiState().first())
            try {dao.saveMainPage("pair");fail("Action routes must be rejected")} catch(_:IllegalArgumentException) {}
            try {dao.saveSessionFilters(false,false,"x".repeat(513));fail("Unbounded search must be rejected")} catch(_:IllegalArgumentException) {}
        } finally {first.close()}
        val second=open()
        try {
            val dao=second.portal();val saved=dao.uiState().first()!!
            assertEquals("sessions",saved.mainPage);assertTrue(saved.sessionsArchived);assertTrue(saved.sessionsActive);assertEquals("configuration",saved.sessionsQuery)
            dao.saveMainPage("settings")
            assertEquals(saved.copy(mainPage="settings"),dao.uiState().first())
        } finally {second.close()}
        context.deleteDatabase(name);Unit
    }
}
