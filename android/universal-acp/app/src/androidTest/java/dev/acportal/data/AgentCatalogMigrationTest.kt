package dev.acportal.data

import androidx.room3.Room
import androidx.room3.testing.MigrationTestHelper
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.sqlite.execSQL
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.storage.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.UUID

class AgentCatalogMigrationTest {
    private val instrumentation=InstrumentationRegistry.getInstrumentation()
    private val context=instrumentation.targetContext
    private val name="agent-catalog-test-${UUID.randomUUID()}.db"
    @get:Rule val helper=MigrationTestHelper(instrumentation=instrumentation,file=context.getDatabasePath(name),driver=AndroidSQLiteDriver(),databaseClass=PortalDatabase::class)
    @Test fun migrationPreservesHostSessionDraftArchiveAndRecentWorkspace()=runBlocking {
        helper.createDatabase(1).use {connection->
            connection.execSQL("INSERT INTO hosts VALUES ('h','Host','https://host.example','encrypted-alias','device',123,45,2,1)")
            connection.execSQL("""INSERT INTO sessions VALUES ('h','s','{}','{"sequence":17}','Keep this draft',99,1)""")
            connection.execSQL("INSERT INTO recent_workspaces VALUES ('h','/workspace','mock',88)")
        }
        helper.runMigrationsAndValidate(2,emptyList()).use {connection->
            connection.prepare("SELECT credentialAlias, agentCount FROM hosts WHERE id='h'").use {row->assertTrue(row.step());assertEquals("encrypted-alias",row.getText(0));assertEquals(2L,row.getLong(1))}
            connection.prepare("SELECT state, draft, archived FROM sessions WHERE id='s'").use {row->assertTrue(row.step());assertEquals("""{"sequence":17}""",row.getText(0));assertEquals("Keep this draft",row.getText(1));assertEquals(1L,row.getLong(2))}
            connection.prepare("SELECT path, agentId FROM recent_workspaces").use {row->assertTrue(row.step());assertEquals("/workspace",row.getText(0));assertEquals("mock",row.getText(1))}
            connection.prepare("SELECT COUNT(*) FROM agent_catalogs").use {row->assertTrue(row.step());assertEquals(0L,row.getLong(0))}
        }
        context.deleteDatabase(name)
        Unit
    }
    @Test fun agentCatalogReopensReplacesAtomicallyAndCascadesOnlyItsHost()=runBlocking {
        helper.createDatabase(2).close()
        fun open()=Room.databaseBuilder<PortalDatabase>(context,name).setDriver(AndroidSQLiteDriver()).build()
        val catalog=StoredAgentCatalog("h","""[{"id":"custom","name":"Custom","installed":true,"executable":"/usr/bin/custom","reason":null}]""",1234)
        val first=open()
        try {val database=first
            database.portal().saveHost(HostProfile("h","Host","https://host.example","encrypted-alias","device",0))
            database.portal().saveHost(HostProfile("other","Other","https://other.example","other-alias","device",0))
            database.portal().saveAgentCatalog(catalog)
            database.portal().saveAgentCatalog(catalog.copy(hostId="other"))
        } finally {first.close()}
        val second=open()
        try {val database=second
            val dao=database.portal()
            assertEquals(catalog,dao.agentCatalogs().first().single {it.hostId=="h"})
            dao.saveAgentCatalog(catalog.copy(metadata="[]",discoveredAt=5678))
            assertEquals("[]",dao.agentCatalogs().first().single {it.hostId=="h"}.metadata)
            assertEquals(2,dao.agentCatalogs().first().size)
            dao.deleteHost("h")
            assertEquals(listOf("other"),dao.agentCatalogs().first().map {it.hostId})
        } finally {second.close()}
        context.deleteDatabase(name)
        Unit
    }
}
