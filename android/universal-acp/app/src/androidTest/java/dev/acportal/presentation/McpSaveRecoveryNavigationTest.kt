package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
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
import android.content.ContextWrapper
import dev.acportal.data.McpDefinition

class McpSaveRecoveryNavigationTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    @Test fun failedSaveRetainsEditorAndExplicitRetryPersistsEncryptedDefinition() {
        checkSaveRecovery(storageFailure=false)
    }
    @Test fun failedEncryptedWritePreservesPreviousDefinitionAndEditorUntilExplicitRetry() {
        checkSaveRecovery(storageFailure=true)
    }
    private fun checkSaveRecovery(storageFailure:Boolean) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val host=runBlocking {actual.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        val beforeAliases=runBlocking {actual.settings.mcpAliases.first()}
        val beforeSessions=runBlocking {actual.api.sessions(host!!).map {it.id}.toSet()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val prefsFile=File(context.cacheDir,"mcp-save-${UUID.randomUUID()}.preferences_pb")
        val preferences=PreferenceDataStoreFactory.create(scope=scope,produceFile={prefsFile})
        val settings=SettingsStore(context,preferences)
        val vaultRoot=File(context.cacheDir,"mcp-vault-${UUID.randomUUID()}").apply {mkdirs()}
        val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir():File=vaultRoot})
        val credentials=File(vaultRoot,"credentials")
        val retained=File(vaultRoot,"retained-credentials")
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val store=ViewModelStore()
        try {
            runBlocking {database.portal().saveHost(host!!.copy(label="Configuration fixture"))}
            val repository=PortalRepository(database.portal(),actual.api,vault,settings,scope)
            val previous=McpDefinition(UUID.randomUUID().toString(),"Project docs","http","https://previous.example.invalid/mcp")
            if(storageFailure)runBlocking {repository.saveMcpServers(host!!.id,listOf(previous))}
            val previousAliases=runBlocking {settings.mcpAliases.first()}
            val previousBytes=if(storageFailure)File(credentials,previousAliases.getValue(host!!.id)).readBytes() else null
            val vm=PortalViewModel(repository);store.put("configuration",vm)
            compose.setContent {PortalTheme {PortalNavigation(vm)}}
            compose.waitUntil(20_000) {!vm.ui.value.loading && compose.onAllNodesWithContentDescription("Settings tab").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("MCP servers").performClick()
            compose.onNodeWithText("Configuration fixture").performClick()
            compose.waitUntil(10_000) {!vm.ui.value.loading && compose.onAllNodesWithText("+  Add server").fetchSemanticsNodes().isNotEmpty()}
            if(storageFailure)compose.onNodeWithText("Project docs").performClick() else {
                compose.onNodeWithText("+  Add server").performClick()
                compose.onNodeWithText("Server name").performTextInput("Project docs")
                compose.onNodeWithText("HTTP").performClick()
            }
            compose.onNodeWithText("Server URL").performTextReplacement("https://docs.example.invalid/mcp")
            scroll("Headers")
            compose.onNodeWithText("Headers").performTextInput("X-Test=retained fixture value")
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            if(storageFailure) {
                assertTrue(credentials.renameTo(retained))
                credentials.writeText("fixture storage unavailable")
            } else runBlocking {database.portal().deleteHost(host!!.id)}
            save()
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
            val message=if(storageFailure)"MCP servers could not be saved. Your changes are still in the editor. Retry when device storage is available." else "Connection is no longer saved"
            assertEquals(message,vm.ui.value.error)
            assertEquals(previousAliases,runBlocking {settings.mcpAliases.first()})
            if(storageFailure)assertArrayEquals(previousBytes,File(retained,previousAliases.getValue(host!!.id)).readBytes())
            scroll("Server name")
            compose.onNodeWithText("Server name").assertTextContains("Project docs")
            scroll("Server URL")
            compose.onNodeWithText("Server URL").assertTextContains("https://docs.example.invalid/mcp")
            scroll("Headers")
            compose.onNodeWithText("Headers").assertTextContains("X-Test=retained fixture value")
            if(storageFailure) {
                assertTrue(credentials.delete())
                assertTrue(retained.renameTo(credentials))
                assertEquals(listOf(previous),runBlocking {repository.mcpServers(host!!.id)})
            } else runBlocking {database.portal().saveHost(host!!.copy(label="Configuration fixture"))}
            assertEquals(previousAliases,runBlocking {settings.mcpAliases.first()})
            compose.onNodeWithContentDescription("Dismiss error").performClick()
            save()
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.mcpServers.size==1 && compose.onAllNodesWithText("+  Add server").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Project docs").assertIsDisplayed()
            val restored=runBlocking {repository.mcpServers(host!!.id).single()}
            assertEquals("http",restored.transport)
            assertEquals("https://docs.example.invalid/mcp",restored.endpoint)
            assertEquals("retained fixture value",restored.values.single().value)
            if(storageFailure) {
                assertNotEquals(previousAliases,runBlocking {settings.mcpAliases.first()})
                assertFalse(File(credentials,previousAliases.getValue(host!!.id)).exists())
            }
            assertEquals(beforeSessions,runBlocking {actual.api.sessions(host!!).map {it.id}.toSet()})
            assertTrue(runBlocking {actual.settings.mcpAliases.first()}==beforeAliases)
        } finally {
            store.clear()
            runBlocking {settings.mcpAliases.first().values.forEach(vault::remove);scope.coroutineContext[Job]!!.cancelAndJoin()}
            database.close();prefsFile.delete();File(prefsFile.path+".tmp").delete();vaultRoot.deleteRecursively()
        }
    }
    private fun scroll(label:String) {
        compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(label))
    }
    private fun save() {
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        if(compose.onAllNodes(hasText("Save server") and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().isNotEmpty())scroll("Save server")
        compose.onNodeWithText("Save server").performClick()
    }
}
