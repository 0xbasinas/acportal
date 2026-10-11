package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import dev.acportal.storage.PortalSettings
import dev.acportal.storage.StoredSession
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/**
 * Recovery of route state across saved-instance-state restoration (rotation and process
 * recreation) for folder browsing, message-cache settings, offline history and prompt
 * history. Uses production composables and explicit callbacks; nothing is sent.
 * Full-process and real-network variants are device checks (runbook §3).
 */
class RouteRecoveryUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    private val restoration get()=StateRestorationTester(compose)

    @Test fun folderBrowsingErrorKeepsRetryAndFolderChoices() {
        var retries=0;var browsed=""
        var error by mutableStateOf<String?>("Could not open this folder")
        compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
            WorkspacePicker(listOf(Workspace("/workspace/app","App")),null,{},{browsed=it},error=error,onRetry={retries++})
        }}}
        compose.onNodeWithText("Could not open this folder").assertExists()
        compose.onNodeWithText("Retry").performScrollTo().performClick()
        compose.runOnIdle {assertEquals(1,retries);error=null}
        compose.onNodeWithText("Could not open this folder").assertDoesNotExist()
        compose.onNodeWithContentDescription("Browse App").performScrollTo().performClick()
        compose.runOnIdle {assertEquals("/workspace/app",browsed)}
    }

    @Test fun messageCacheDialogSurvivesRecreationAndReportsTheChoice() {
        val tester=restoration
        var settings by mutableStateOf(PortalSettings(cacheMessages=true))
        val choices=mutableListOf<Boolean>()
        tester.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
            SettingsScreen(settings,{},{choices+=it;settings=settings.copy(cacheMessages=it)})
        }}}
        compose.onNodeWithText("History on this device").performClick()
        compose.onNodeWithText("Save messages").assertIsDisplayed()
        tester.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Save messages").assertIsDisplayed()
        compose.onNode(isToggleable()).assertIsOn().performClick()
        compose.runOnIdle {assertEquals(listOf(false),choices)}
        compose.onNode(isToggleable()).assertIsOff()
        compose.onNodeWithText("Done").performClick()
        compose.onNodeWithText("Save messages").assertDoesNotExist()
    }

    @Test fun offlineHistoryStaysVisibleAfterRestoration() {
        val tester=restoration
        val info=SessionInfo("saved","goose","acp-saved","/workspace/saved",status="ready")
        val state=SessionState(sequence=4,items=listOf(TimelineItem.Text("u","user","Summarise the logs"),TimelineItem.Text("a","agent","The logs show two retries.")))
        val stored=StoredSession("h","saved",WireJson.encodeToString(info),WireJson.encodeToString(state),draft="unsent follow-up")
        var retries=0
        tester.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {SavedSessionScreen(info,stored,null,false,{},{},{retries++})}}}
        compose.onNodeWithText("Showing messages saved on this device. Connect to check the latest activity.").assertIsDisplayed()
        compose.onNodeWithText("Summarise the logs").assertExists()
        tester.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Summarise the logs").assertExists()
        compose.onNodeWithText("unsent follow-up").assertExists()
        compose.onNodeWithText("Connect to host").performClick()
        compose.runOnIdle {assertEquals(1,retries)}
    }

    @Test fun promptHistorySelectionPlacesTextWithoutSending() {
        val tester=restoration
        var open by mutableStateOf(true)
        val selected=mutableListOf<String>()
        tester.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
            if(open)PromptHistorySheet(listOf(PromptHistoryEntry("1","Run the tests"),PromptHistoryEntry("2","Fix lint")),{open=false}) {selected+=it;open=false}
        }}}
        compose.onNodeWithText("Prompt history").assertIsDisplayed()
        tester.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Fix lint").performClick()
        compose.runOnIdle {assertEquals(listOf("Fix lint"),selected);assertFalse(open)}
    }
}
