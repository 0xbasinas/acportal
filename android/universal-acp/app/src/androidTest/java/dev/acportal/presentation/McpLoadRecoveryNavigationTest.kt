package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.data.McpDefinition
import dev.acportal.data.PortalRepository
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

class McpLoadRecoveryNavigationTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)

    @Test fun unreadableEncryptedDefinitionsRemainIntactUntilExplicitRetry() {
        checkLoadRecovery(missing=false)
    }

    @Test fun missingEncryptedDefinitionsRequireExplicitRetryAfterRestoringStorage() {
        checkLoadRecovery(missing=true)
    }

    private fun checkLoadRecovery(missing:Boolean) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val host=runBlocking {actual.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        val originalAliases=runBlocking {actual.settings.mcpAliases.first()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val prefsFile=File(context.cacheDir,"mcp-load-${UUID.randomUUID()}.preferences_pb")
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={prefsFile}))
        val vault=CredentialVault(context)
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val store=ViewModelStore()
        try {
            val repository=PortalRepository(database.portal(),actual.api,vault,settings,scope)
            val definition=McpDefinition(UUID.randomUUID().toString(),"Retained docs","http","https://docs.example.invalid/mcp")
            runBlocking {database.portal().saveHost(host!!.copy(label="Load fixture"));repository.saveMcpServers(host.id,listOf(definition))}
            val alias=runBlocking {settings.mcpAliases.first().getValue(host!!.id)}
            val encrypted=File(context.noBackupFilesDir,"credentials/$alias")
            val originalBytes=encrypted.readBytes()
            if(missing)assertTrue(encrypted.delete()) else encrypted.writeBytes(byteArrayOf(0))
            val vm=PortalViewModel(repository);store.put("load",vm)
            compose.setContent {PortalTheme {PortalNavigation(vm)}}
            compose.waitUntil(20_000) {!vm.ui.value.loading && compose.onAllNodesWithContentDescription("Settings tab").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("MCP servers").performClick()
            compose.onNodeWithText("Load fixture").performClick()
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
            compose.onNodeWithText("Servers unavailable").assertIsDisplayed()
            compose.onNodeWithText("Loading servers").assertDoesNotExist()
            assertEquals("Saved MCP servers could not be read. Retry after restoring this device's protected storage.",vm.ui.value.error)
            assertTrue(vm.ui.value.mcpServers.isEmpty())
            assertEquals(alias,runBlocking {settings.mcpAliases.first().getValue(host!!.id)})
            if(missing)assertFalse(encrypted.exists()) else assertArrayEquals(byteArrayOf(0),encrypted.readBytes())
            compose.onNodeWithContentDescription("Back").assertIsDisplayed()
            compose.onNodeWithContentDescription("Dismiss error").performClick()
            compose.onNodeWithText("Reload servers").assertIsDisplayed()
            encrypted.writeBytes(originalBytes)
            compose.waitForIdle()
            compose.onNodeWithText("Servers unavailable").assertIsDisplayed()
            assertTrue(vm.ui.value.mcpServers.isEmpty())
            compose.onNodeWithText("Reload servers").performClick()
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.mcpServers.size==1}
            compose.onNodeWithText("Retained docs").assertIsDisplayed()
            assertNull(vm.ui.value.error)
            assertEquals(listOf(definition),runBlocking {repository.mcpServers(host!!.id)})
            assertArrayEquals(originalBytes,encrypted.readBytes())
            assertEquals(originalAliases,runBlocking {actual.settings.mcpAliases.first()})
        } finally {
            store.clear()
            runBlocking {settings.mcpAliases.first().values.forEach(vault::remove);scope.coroutineContext[Job]!!.cancelAndJoin()}
            database.close();prefsFile.delete();File(prefsFile.path+".tmp").delete()
        }
    }
}
