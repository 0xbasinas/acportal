package dev.acportal.presentation

import android.content.ContextWrapper
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelProvider
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.data.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

class McpLifecycleNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<McpLifecycleFixtureActivity>()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)

    @Test fun activityRecreationRetainsDraftWithoutSerializingSecretOrSavingAutomatically() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val originalAliases=runBlocking {actual.settings.mcpAliases.first()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val root=File(context.cacheDir,"mcp-lifecycle-${UUID.randomUUID()}").apply {mkdirs()}
        val prefs=File(root,"settings.preferences_pb")
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={prefs}))
        val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir()=root})
        val db=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val host=HostProfile("fixture-lifecycle","Lifecycle fixture","http://127.0.0.1:1",UUID.randomUUID().toString(),"fixture",Long.MAX_VALUE)
        val previous=McpDefinition("fixture-server","Saved docs","http","https://saved.example.invalid/mcp")
        val secret="fixture-secret-${UUID.randomUUID()}"
        try {
            val repository=PortalRepository(db.portal(),actual.api,vault,settings,scope)
            runBlocking {db.portal().saveHost(host);repository.saveMcpServers(host.id,listOf(previous))}
            val oldAliases=runBlocking {settings.mcpAliases.first()}
            compose.runOnUiThread {McpLifecycleFixtureActivity.fixtureRepository=repository}
            compose.waitUntil(10_000) {compose.onAllNodesWithContentDescription("Settings tab").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("MCP servers").performClick()
            compose.onNodeWithText(host.label).performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithText(previous.name).fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText(previous.name).performClick()
            compose.onNodeWithText("Server name").performTextReplacement("Unsaved docs")
            compose.onNodeWithText("Server URL").performTextReplacement("https://draft.example.invalid/mcp")
            reveal("Headers")
            compose.onNodeWithText("Headers").performTextReplacement("Authorization=$secret")
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            val before=ViewModelProvider(compose.activity)[PortalViewModel::class.java]
            compose.activityRule.scenario.recreate()
            compose.waitUntil(10_000) {compose.onAllNodesWithText("Edit server").fetchSemanticsNodes().isNotEmpty()}
            val after=ViewModelProvider(compose.activity)[PortalViewModel::class.java]
            assertSame(before,after)
            reveal("Server name");compose.onNodeWithText("Server name").assertTextContains("Unsaved docs")
            reveal("Server URL");compose.onNodeWithText("Server URL").assertTextContains("https://draft.example.invalid/mcp")
            reveal("Headers");compose.onNodeWithText("Headers").assertTextContains("Authorization=$secret")
            val bytes=requireNotNull(McpLifecycleFixtureActivity.savedBytes)
            assertFalse(bytes.toString(Charsets.UTF_8).contains(secret))
            assertFalse(bytes.toString(Charsets.UTF_16LE).contains(secret))
            assertEquals(oldAliases,runBlocking {settings.mcpAliases.first()})
            assertEquals(listOf(previous),runBlocking {repository.mcpServers(host.id)})
            reveal("Save server");compose.onNodeWithText("Save server").performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithText("Unsaved docs").fetchSemanticsNodes().isNotEmpty() && !after.ui.value.loading}
            val saved=runBlocking {repository.mcpServers(host.id).single()}
            assertEquals("Unsaved docs",saved.name)
            assertEquals("https://draft.example.invalid/mcp",saved.endpoint)
            assertEquals(secret,saved.values.single().value)
            assertNull(after.mcpEdit(host.id).draft)
            assertEquals(originalAliases,runBlocking {actual.settings.mcpAliases.first()})
        } finally {
            compose.activityRule.scenario.close()
            McpLifecycleFixtureActivity.fixtureRepository=null;McpLifecycleFixtureActivity.savedBytes=null
            runBlocking {settings.mcpAliases.first().values.forEach(vault::remove);scope.coroutineContext[Job]!!.cancelAndJoin()}
            db.close();root.deleteRecursively()
        }
    }
    private fun reveal(text:String) {
        if(compose.onAllNodes(hasText(text) and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().isNotEmpty())compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(text))
    }
}
