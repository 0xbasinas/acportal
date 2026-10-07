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
import java.security.MessageDigest
import java.util.Properties
import java.util.UUID

/** Run prepare, verify/stop the app process externally, then run restore alone. */
class McpColdProcessNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<McpLifecycleFixtureActivity>()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    private val definition=McpDefinition("cold-server","Saved documentation ".repeat(7).take(128),"http","https://saved.example.invalid/"+"nested/".repeat(150),values=listOf(McpValue("X-Fixture","saved-value-"+"v".repeat(4000))))
    private val hostId="cold-host"
    private val hostLabel="Cold fixture"
    private val draftName="Unsaved cold draft"

    @Test fun prepareSavedDefinitionAndLeaveDraftUnsaved()=checkColdProcess(restore=false)
    @Test fun restoreSavedDefinitionAfterVerifiedProcessStop()=checkColdProcess(restore=true)
    /** Intentionally terminated by adb after checkpoint assertions, before fixture teardown. */
    @Test fun prepareVisibleDraftUntilExternalKill()=checkColdProcess(restore=false,awaitKill=true)

    private fun checkColdProcess(restore:Boolean,awaitKill:Boolean=false) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val actualAliases=runBlocking {actual.settings.mcpAliases.first()}
        val root=File(context.cacheDir,"mcp-cold-process-fixture")
        val marker=File(root,"checkpoint.properties")
        if(restore)assertTrue("Run prepare and stop the process first",marker.exists())
        else {assertFalse("Existing cold fixture must be restored/cleaned first",root.exists());assertTrue(root.mkdirs())}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val prefsFile=File(root,"settings.preferences_pb")
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={prefsFile}))
        val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir()=root})
        val db=Room.databaseBuilder<PortalDatabase>(context,File(root,"portal.db").absolutePath).setDriver(AndroidSQLiteDriver()).build()
        var prepared=false
        try {
            val repository=PortalRepository(db.portal(),actual.api,vault,settings,scope)
            if(!restore)runBlocking {
                db.portal().saveHost(HostProfile(hostId,hostLabel,"http://127.0.0.1:1",UUID.randomUUID().toString(),"fixture",Long.MAX_VALUE))
                repository.saveMcpServers(hostId,listOf(definition))
            }
            val checkpoint=Properties()
            if(restore) {
                marker.inputStream().use(checkpoint::load)
                assertNotEquals("Requires a fresh app process",checkpoint.getProperty("pid").toInt(),android.os.Process.myPid())
                assertNull(McpLifecycleFixtureActivity.fixtureRepository)
                assertEquals(listOf(definition),runBlocking {repository.mcpServers(hostId)})
            }
            val alias=runBlocking {settings.mcpAliases.first().getValue(hostId)}
            val encrypted=File(root,"credentials/$alias")
            val digest=MessageDigest.getInstance("SHA-256").digest(encrypted.readBytes()).joinToString("") {"%02x".format(it)}
            if(restore) {assertEquals(checkpoint.getProperty("alias"),alias);assertEquals(checkpoint.getProperty("digest"),digest)}
            compose.runOnUiThread {McpLifecycleFixtureActivity.fixtureRepository=repository}
            compose.waitUntil(10_000) {compose.onAllNodesWithContentDescription("Settings tab").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("MCP servers").performClick()
            compose.onNodeWithText(hostLabel).performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithText(definition.name).fetchSemanticsNodes().isNotEmpty()}
            val vm=ViewModelProvider(compose.activity)[PortalViewModel::class.java]
            assertNull(vm.mcpEdit(hostId).draft)
            compose.onNodeWithText(draftName).assertDoesNotExist()
            reveal(definition.name);compose.onNodeWithText(definition.name).performClick()
            reveal("Server name");compose.onNodeWithText("Server name").assertTextContains(definition.name)
            reveal("Server URL");compose.onNodeWithText("Server URL").assertTextContains(definition.endpoint)
            reveal("Headers");compose.onNodeWithText("Headers").assertTextContains("X-Fixture="+definition.values.single().value)
            if(!restore) {
                compose.onNodeWithText("Headers").performTextReplacement("X-Fixture=unsaved-fixture-secret")
                androidx.test.espresso.Espresso.closeSoftKeyboard()
                reveal("Server name");compose.onNodeWithText("Server name").performTextReplacement(draftName)
                androidx.test.espresso.Espresso.closeSoftKeyboard()
                compose.runOnIdle {assertEquals(draftName,vm.mcpEdit(hostId).draft!!.name)}
                checkpoint.setProperty("pid",android.os.Process.myPid().toString())
                checkpoint.setProperty("alias",alias);checkpoint.setProperty("digest",digest)
                marker.outputStream().use {checkpoint.store(it,"Fixture identity and encrypted-byte checkpoint only")}
                prepared=true
            }
            assertEquals(listOf(definition),runBlocking {repository.mcpServers(hostId)})
            assertEquals(setOf(alias),File(root,"credentials").listFiles()!!.map {it.name}.toSet())
            assertEquals(actualAliases,runBlocking {actual.settings.mcpAliases.first()})
            if(awaitKill) {
                compose.onNodeWithText("Server name").assertTextContains(draftName).assertIsDisplayed()
                checkpoint.setProperty("waitingForKill","true")
                marker.outputStream().use {checkpoint.store(it,"Validated foreground fixture awaiting external process termination")}
                runBlocking {awaitCancellation()}
            }
        } finally {
            compose.activityRule.scenario.close()
            McpLifecycleFixtureActivity.fixtureRepository=null;McpLifecycleFixtureActivity.savedBytes=null
            runBlocking {scope.coroutineContext[Job]!!.cancelAndJoin()}
            db.close()
            if(restore || !prepared)root.deleteRecursively()
        }
    }
    private fun reveal(text:String) {
        if(compose.onAllNodes(hasText(text) and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().isNotEmpty())compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(text))
    }
}
