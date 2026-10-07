package dev.acportal.presentation

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.PortalDatabase
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class SavedHistoryNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun interruptedHistoryCanBeReadAndExportedWithoutOpeningOrResumingAnAgent() {
        val repository=(compose.activity.application as PortalApplication).repository
        val host=runBlocking {repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        val previousUi=runBlocking {repository.uiState.first()}
        lateinit var vm:PortalViewModel
        compose.runOnIdle {vm=ViewModelProvider(compose.activity)[PortalViewModel::class.java]}
        compose.waitUntil(20_000) {!vm.ui.value.loading}
        val stored=runBlocking {repository.create(host!!.id,"mock",repository.api.workspaces(host).first().path)}
        val originalInfo=WireJson.decodeFromString<SessionInfo>(stored.metadata)
        val database=PortalDatabase.open(compose.activity)
        val file=java.io.File.createTempFile("saved-history-",".md",compose.activity.cacheDir)
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val filter=IntentFilter(Intent.ACTION_CREATE_DOCUMENT).apply {addCategory(Intent.CATEGORY_OPENABLE);addDataType("text/markdown")}
        val monitor=instrumentation.addMonitor(filter,Instrumentation.ActivityResult(Activity.RESULT_OK,Intent().setData(Uri.fromFile(file))),true)
        try {
            val cached=SessionState(items=listOf(TimelineItem.Text("message","agent","Saved history fixture"),TimelineItem.Text("thought","thought","Agent-provided saved progress")),permissions=mapOf("pending" to Permission(JsonPrimitive("pending"),buildJsonObject {put("private","PERMISSION-EXCLUDED")})))
            runBlocking {
                database.portal().saveSession(stored.copy(metadata=WireJson.encodeToString(SessionInfo.serializer(),originalInfo.copy(status="interrupted")),state=WireJson.encodeToString(SessionState.serializer(),cached)))
                repository.draft(stored.hostId,stored.id,"DRAFT-EXCLUDED")
                repository.saveSessionFilters(false,false,"")
            }
            compose.onNodeWithContentDescription("Sessions tab").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithTag("session-${stored.id}").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithTag("session-${stored.id}").performClick()
            compose.onNodeWithText("Session interrupted").assertIsDisplayed()
            compose.onNodeWithText("View saved conversation").performClick()
            compose.onNodeWithText("Saved history fixture").assertIsDisplayed()
            compose.onNodeWithText("Session actions").assertIsDisplayed()
            compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
            compose.runOnIdle {assertNull(vm.ui.value.live)}
            compose.onNodeWithContentDescription("Session menu").performClick()
            compose.onNodeWithText("Export transcript").performClick()
            try {compose.waitUntil(10_000) {file.readText().contains("Saved history fixture")}}
            catch(failure:Throwable) {throw AssertionError("Document contract hits=${monitor.hits}, output bytes=${file.length()}",failure)}
            val exported=file.readText()
            assertTrue(exported.contains("Agent thoughts"));assertTrue(exported.contains("Agent-provided saved progress"))
            assertFalse(exported.contains("DRAFT-EXCLUDED"));assertFalse(exported.contains("PERMISSION-EXCLUDED"))
            assertEquals(1,monitor.hits)
            compose.runOnIdle {assertNull(vm.ui.value.live)}
            assertEquals(originalInfo.status,runBlocking {repository.api.session(host!!,stored.id).status})
            compose.onNodeWithText("Session actions").performClick()
            compose.onNodeWithText("Session interrupted").assertIsDisplayed()
        } finally {
            instrumentation.removeMonitor(monitor)
            runBlocking {repository.delete(stored);repository.saveMainPage(previousUi.mainPage);repository.saveSessionFilters(previousUi.sessionsArchived,previousUi.sessionsActive,previousUi.sessionsQuery)}
            database.close();file.delete()
        }
    }
}
